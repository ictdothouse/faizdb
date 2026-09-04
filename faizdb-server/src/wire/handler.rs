//! MongoDB Command Handler and Dispatcher.
//!
//! Executes commands requested by MongoDB drivers (Compass, Mongoose, PyMongo, Go, etc.)
//! and converts between BSON and FaizDB internal document models.

use bson::{doc, Bson, DateTime as BsonDateTime, Document as BsonDocument};
use dashmap::DashMap;
use std::sync::{Arc, LazyLock};

use super::op_msg::OpMsg;
use faizdb_core::document::model::{Document as FaizDocument, Value as FaizValue};
use faizdb_query::DatabaseContext;

struct CachedCursor {
    ns: String,
    docs: Vec<BsonDocument>,
    #[allow(dead_code)]
    created_at: std::time::Instant,
}

static CURSOR_CACHE: LazyLock<DashMap<i64, CachedCursor>> = LazyLock::new(DashMap::new);

/// Session state for a MongoDB Wire Protocol connection
#[derive(Debug, Clone)]
pub struct MongoSession {
    pub client_addr: String,
    pub authenticated_user: Option<String>,
    pub role: Option<faizdb_security::Role>,
}

impl MongoSession {
    pub fn new(client_addr: String) -> Self {
        let no_auth = std::env::var("FAIZDB_NO_AUTH")
            .map(|v| v == "1" || v == "true")
            .unwrap_or(false);
        Self {
            client_addr,
            authenticated_user: if no_auth {
                Some("admin".to_string())
            } else {
                None
            },
            role: if no_auth {
                Some(faizdb_security::Role::Admin)
            } else {
                None
            },
        }
    }

    pub fn is_authenticated(&self) -> bool {
        self.authenticated_user.is_some()
    }
}

/// Dispatch and execute an incoming OP_MSG command
pub fn handle_op_msg(
    db: &Arc<DatabaseContext>,
    msg: &OpMsg,
    session: &mut MongoSession,
    user_store: &Arc<faizdb_security::UserStore>,
) -> OpMsg {
    let req_id = msg.header.request_id;
    let primary_doc = match msg.primary_document() {
        Some(d) => d,
        None => {
            return OpMsg::response(
                0,
                req_id,
                doc! { "ok": 0.0, "errmsg": "Missing command document" },
            )
        }
    };

    let response_body = dispatch_command(db, primary_doc, msg, session, user_store);
    OpMsg::response(0, req_id, response_body)
}

/// Dispatch a command document to its handler
fn dispatch_command(
    db: &Arc<DatabaseContext>,
    cmd: &BsonDocument,
    msg: &OpMsg,
    session: &mut MongoSession,
    user_store: &Arc<faizdb_security::UserStore>,
) -> BsonDocument {
    // 1. Handshake & Cluster Info (Allowed without authentication)
    if cmd.contains_key("isMaster") || cmd.contains_key("ismaster") || cmd.contains_key("hello") {
        return handle_is_master();
    }

    if cmd.contains_key("buildInfo") || cmd.contains_key("buildinfo") {
        return handle_build_info();
    }

    if cmd.contains_key("whatsmyuri") {
        return doc! {
            "you": session.client_addr.clone(),
            "ok": 1.0
        };
    }

    if cmd.contains_key("ping") {
        return doc! { "ok": 1.0 };
    }

    if cmd.contains_key("getLog") {
        return doc! {
            "totalLinesWritten": 0,
            "log": Vec::<Bson>::new(),
            "ok": 1.0
        };
    }

    // 2. Authentication Handshake Commands
    if cmd.contains_key("authenticate") {
        return handle_authenticate(cmd, session, user_store);
    }

    if cmd.contains_key("saslStart") {
        return handle_sasl_start(cmd, session, user_store);
    }

    if cmd.contains_key("saslContinue") {
        return handle_sasl_continue(session);
    }

    if cmd.contains_key("logout") {
        session.authenticated_user = None;
        session.role = None;
        return doc! { "ok": 1.0 };
    }

    // 3. Security Guard: All operational and metadata commands require authentication
    if !session.is_authenticated() {
        let cmd_name = cmd.keys().next().map(|s| s.as_str()).unwrap_or("unknown");
        tracing::warn!(
            "Unauthorized MongoDB command '{cmd_name}' rejected from {}",
            session.client_addr
        );
        return doc! {
            "ok": 0.0,
            "errmsg": format!("command '{cmd_name}' requires authentication"),
            "code": 13,
            "codeName": "Unauthorized"
        };
    }

    // 4. RBAC Guard: ReadOnly users cannot perform write operations
    if session.role == Some(faizdb_security::Role::ReadOnly) {
        if let Some(cmd_name) = ["insert", "update", "delete", "drop"]
            .iter()
            .find(|k| cmd.contains_key(**k))
        {
            tracing::warn!(
                "ReadOnly user '{}' attempted write command '{cmd_name}' from {}",
                session.authenticated_user.as_deref().unwrap_or("unknown"),
                session.client_addr
            );
            return doc! {
                "ok": 0.0,
                "errmsg": format!("not authorized on collection to execute write command '{cmd_name}'"),
                "code": 13,
                "codeName": "Unauthorized"
            };
        }
    }

    // 5. Database & Collection Metadata
    if cmd.contains_key("listDatabases") {
        return doc! {
            "databases": [
                doc! { "name": "default", "sizeOnDisk": 1048576, "empty": false },
                doc! { "name": "admin", "sizeOnDisk": 1048576, "empty": false }
            ],
            "totalSize": 2097152,
            "ok": 1.0
        };
    }

    if cmd.contains_key("listCollections") {
        let db_name = cmd.get_str("$db").unwrap_or("default");
        let collections = db.list_collections();
        let first_batch: Vec<bson::Bson> = collections
            .into_iter()
            .map(|name| {
                bson::to_bson(&doc! { "name": name, "type": "collection" })
                    .unwrap_or(bson::Bson::Null)
            })
            .collect();
        return doc! {
            "cursor": doc! {
                "id": 0i64,
                "ns": format!("{db_name}.$cmd.listCollections"),
                "firstBatch": first_batch
            },
            "ok": 1.0
        };
    }

    if let Ok(col_name) = cmd.get_str("createIndexes") {
        let _ = col_name;
        return doc! {
            "numIndexesBefore": 1,
            "numIndexesAfter": 2,
            "note": "FaizDB Auto-indexing active",
            "ok": 1.0
        };
    }

    // 6. CRUD Operations
    if let Ok(col_name) = cmd.get_str("insert") {
        return handle_insert(db, col_name, cmd, msg);
    }

    if let Ok(col_name) = cmd.get_str("find") {
        return handle_find(db, col_name, cmd);
    }

    if cmd.contains_key("getMore") {
        return handle_get_more(cmd);
    }

    if cmd.contains_key("killCursors") {
        return handle_kill_cursors(cmd);
    }

    if let Ok(col_name) = cmd.get_str("aggregate") {
        return handle_aggregate(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("count") {
        return handle_count(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("delete") {
        return handle_delete(db, col_name, cmd, msg);
    }

    if let Ok(col_name) = cmd.get_str("update") {
        return handle_update(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("drop") {
        let db_name = cmd.get_str("$db").unwrap_or("default");
        let dropped = db.drop_collection(col_name);
        return doc! {
            "nIndexesWas": 1,
            "ns": format!("{db_name}.{col_name}"),
            "ok": if dropped { 1.0 } else { 0.0 }
        };
    }

    // Generic fallback for unhandled commands to avoid driver crashes
    tracing::debug!("Received unhandled MongoDB command: {:?}", cmd);
    doc! { "ok": 1.0 }
}

fn handle_authenticate(
    cmd: &BsonDocument,
    session: &mut MongoSession,
    user_store: &Arc<faizdb_security::UserStore>,
) -> BsonDocument {
    let user = cmd.get_str("user").unwrap_or("");
    let pwd = cmd.get_str("pwd").unwrap_or("");

    if let Some(role) = user_store.authenticate(user, pwd) {
        session.authenticated_user = Some(user.to_string());
        session.role = Some(role);
        tracing::info!(
            "🔐 User '{user}' authenticated successfully via MongoDB wire from {}",
            session.client_addr
        );
        doc! { "ok": 1.0 }
    } else {
        tracing::warn!(
            "MongoDB wire authentication failed for user '{user}' from {}",
            session.client_addr
        );
        doc! {
            "ok": 0.0,
            "errmsg": "Authentication failed.",
            "code": 18,
            "codeName": "AuthenticationFailed"
        }
    }
}

fn handle_sasl_start(
    cmd: &BsonDocument,
    session: &mut MongoSession,
    user_store: &Arc<faizdb_security::UserStore>,
) -> BsonDocument {
    let mechanism = cmd.get_str("mechanism").unwrap_or("PLAIN");
    if mechanism != "PLAIN" && mechanism != "SCRAM-SHA-256" {
        return doc! {
            "ok": 0.0,
            "errmsg": format!("Unsupported authentication mechanism '{mechanism}'"),
            "code": 18,
            "codeName": "AuthenticationFailed"
        };
    }

    let payload_bytes = match cmd.get("payload") {
        Some(Bson::Binary(b)) => b.bytes.clone(),
        Some(Bson::String(s)) => s.as_bytes().to_vec(),
        _ => Vec::new(),
    };

    let parts: Vec<&[u8]> = payload_bytes.split(|&b| b == 0).collect();
    let (user_opt, pass_opt) = match parts.len() {
        3 => {
            let u = if parts[1].is_empty() {
                parts[0]
            } else {
                parts[1]
            };
            (
                String::from_utf8(u.to_vec()).ok(),
                String::from_utf8(parts[2].to_vec()).ok(),
            )
        }
        2 => (
            String::from_utf8(parts[0].to_vec()).ok(),
            String::from_utf8(parts[1].to_vec()).ok(),
        ),
        _ => (None, None),
    };

    if let (Some(user), Some(pass)) = (user_opt, pass_opt) {
        if !user.is_empty() && !pass.is_empty() {
            if let Some(role) = user_store.authenticate(&user, &pass) {
                session.authenticated_user = Some(user.clone());
                session.role = Some(role);
                tracing::info!(
                    "🔐 User '{user}' authenticated successfully via MongoDB SASL PLAIN from {}",
                    session.client_addr
                );
                return doc! {
                    "conversationId": 1,
                    "done": true,
                    "payload": bson::Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: vec![] },
                    "ok": 1.0
                };
            }
        }
    }

    tracing::warn!(
        "MongoDB SASL PLAIN authentication failed from {}",
        session.client_addr
    );
    doc! {
        "ok": 0.0,
        "errmsg": "Authentication failed.",
        "code": 18,
        "codeName": "AuthenticationFailed"
    }
}

fn handle_sasl_continue(session: &MongoSession) -> BsonDocument {
    if session.is_authenticated() {
        doc! {
            "conversationId": 1,
            "done": true,
            "payload": bson::Binary { subtype: bson::spec::BinarySubtype::Generic, bytes: vec![] },
            "ok": 1.0
        }
    } else {
        doc! {
            "ok": 0.0,
            "errmsg": "Authentication failed.",
            "code": 18,
            "codeName": "AuthenticationFailed"
        }
    }
}

fn handle_is_master() -> BsonDocument {
    doc! {
        "ismaster": true,
        "isWritablePrimary": true,
        "maxBsonObjectSize": 268435456, // 256MB max document size (FaizDB advantage)
        "maxMessageSizeBytes": 268435456,
        "maxWriteBatchSize": 100000,
        "localTime": BsonDateTime::now(),
        "logicalSessionTimeoutMinutes": 30,
        "minWireVersion": 0,
        "maxWireVersion": 21, // Supports modern MongoDB 7.0/8.0 wire protocols
        "readOnly": false,
        "saslSupportedMechs": ["PLAIN"],
        "faizdb": "The AI-Native NoSQL Database Engine",
        "ok": 1.0
    }
}

fn handle_build_info() -> BsonDocument {
    doc! {
        "version": "7.0.0",
        "gitVersion": "faizdb-v0.1.0-engine",
        "modules": Vec::<Bson>::new(),
        "allocator": "rust-native",
        "versionArray": [7, 0, 0, 0],
        "bits": 64,
        "maxBsonObjectSize": 268435456,
        "engine": "FaizDB",
        "faizdbVersion": env!("CARGO_PKG_VERSION"),
        "creator": "Ahmad Faiz",
        "ok": 1.0
    }
}

fn handle_insert(
    db: &Arc<DatabaseContext>,
    collection_name: &str,
    cmd: &BsonDocument,
    msg: &OpMsg,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let mut inserted_count = 0;

    // Documents can be in body `documents: [...]` or in a Section 1 sequence
    let mut docs_to_insert = Vec::new();

    if let Ok(docs_array) = cmd.get_array("documents") {
        for item in docs_array {
            if let Some(doc) = item.as_document() {
                docs_to_insert.push(doc.clone());
            }
        }
    }

    // Also check document sequence sections
    let seq_docs = msg.document_sequence("documents");
    docs_to_insert.extend(seq_docs);

    for bdoc in docs_to_insert {
        let faiz_doc = bson_to_faiz_document(&bdoc);
        let doc_clone = faiz_doc.clone();
        if col.insert(faiz_doc).is_ok() {
            db.change_stream_bus()
                .publish(faizdb_core::stream::ChangeEvent::insert(
                    collection_name,
                    doc_clone,
                ));
            inserted_count += 1;
        }
    }

    doc! {
        "n": inserted_count,
        "ok": 1.0
    }
}

fn handle_find(
    db: &Arc<DatabaseContext>,
    collection_name: &str,
    cmd: &BsonDocument,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let db_name = cmd.get_str("$db").unwrap_or("default");

    let limit = cmd.get_i32("limit").ok().map(|l| l.max(0) as usize);
    let skip = cmd.get_i32("skip").ok().map(|s| s.max(0) as usize);
    let batch_size = cmd.get_i32("batchSize").ok().map(|b| b.max(0) as usize);

    let filter_doc = cmd.get_document("filter").ok();

    // 1. Direct O(1) Primary Key Lookup if filter targets _id
    let id_query = filter_doc.and_then(|f| {
        if f.len() == 1 && f.contains_key("_id") {
            match f.get("_id") {
                Some(bson::Bson::String(s)) => Some(s.clone()),
                Some(bson::Bson::ObjectId(oid)) => Some(oid.to_hex()),
                _ => None,
            }
        } else {
            None
        }
    });

    let mut matched_docs: Vec<faizdb_core::document::model::Document> =
        if let Some(ref id_str) = id_query {
            if let Ok(doc) = col.find_by_id(id_str) {
                vec![doc]
            } else {
                Vec::new()
            }
        } else {
            let all_docs = col.find_all(None);
            all_docs
                .into_iter()
                .filter(|d| {
                    if let Some(filter) = filter_doc {
                        if filter.is_empty() {
                            return true;
                        }
                        // Check exact field matches
                        for (k, v) in filter {
                            if k == "_id" {
                                let id_str = d.id.as_str();
                                let matches_id = match v {
                                    bson::Bson::String(s) => s.as_str() == id_str,
                                    bson::Bson::ObjectId(oid) => oid.to_hex() == id_str,
                                    _ => false,
                                };
                                if !matches_id {
                                    return false;
                                }
                            } else if let Some(val) = d.get_nested(k) {
                                let b_val = faiz_val_to_bson(val);
                                if &b_val != v {
                                    return false;
                                }
                            } else {
                                return false;
                            }
                        }
                    }
                    true
                })
                .collect()
        };

    // Sort documents if "sort" document was supplied
    if let Ok(sort_doc) = cmd.get_document("sort") {
        if let Some((k, dir_val)) = sort_doc.iter().next() {
            let dir = match dir_val {
                bson::Bson::Int32(i) => *i as i8,
                bson::Bson::Int64(i) => *i as i8,
                bson::Bson::Double(f) => *f as i8,
                _ => 1,
            };
            matched_docs.sort_by(|a, b| {
                let va = a.get_nested(k);
                let vb = b.get_nested(k);
                let cmp = match (va, vb) {
                    (Some(x), Some(y)) => match (x, y) {
                        (
                            faizdb_core::document::model::Value::Integer(ix),
                            faizdb_core::document::model::Value::Integer(iy),
                        ) => ix.cmp(iy),
                        (
                            faizdb_core::document::model::Value::Float(fx),
                            faizdb_core::document::model::Value::Float(fy),
                        ) => fx.partial_cmp(fy).unwrap_or(std::cmp::Ordering::Equal),
                        (
                            faizdb_core::document::model::Value::Integer(ix),
                            faizdb_core::document::model::Value::Float(fy),
                        ) => (*ix as f64)
                            .partial_cmp(fy)
                            .unwrap_or(std::cmp::Ordering::Equal),
                        (
                            faizdb_core::document::model::Value::Float(fx),
                            faizdb_core::document::model::Value::Integer(iy),
                        ) => fx
                            .partial_cmp(&(*iy as f64))
                            .unwrap_or(std::cmp::Ordering::Equal),
                        (
                            faizdb_core::document::model::Value::String(sx),
                            faizdb_core::document::model::Value::String(sy),
                        ) => sx.cmp(sy),
                        (
                            faizdb_core::document::model::Value::Boolean(bx),
                            faizdb_core::document::model::Value::Boolean(by),
                        ) => bx.cmp(by),
                        _ => std::cmp::Ordering::Equal,
                    },
                    (Some(_), None) => std::cmp::Ordering::Greater,
                    (None, Some(_)) => std::cmp::Ordering::Less,
                    (None, None) => std::cmp::Ordering::Equal,
                };
                if dir < 0 {
                    cmp.reverse()
                } else {
                    cmp
                }
            });
        }
    }

    let effective_skip = skip.unwrap_or(0);
    let effective_limit = limit.unwrap_or(usize::MAX);
    let requested_batch_size = batch_size.unwrap_or(effective_limit);

    let skipped_docs: Vec<faizdb_core::document::model::Document> = matched_docs
        .into_iter()
        .skip(effective_skip)
        .take(effective_limit)
        .collect();

    let initial_batch_count = requested_batch_size.min(skipped_docs.len());
    let mut all_bson: Vec<BsonDocument> = skipped_docs
        .into_iter()
        .map(|d| faiz_document_to_bson(&d))
        .collect();
    let first_batch: Vec<BsonDocument> = all_bson.drain(0..initial_batch_count).collect();

    let cursor_id = if !all_bson.is_empty() {
        let new_id = (std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos() as i64)
            .abs()
            % 1_000_000_000
            + 1;
        CURSOR_CACHE.insert(
            new_id,
            CachedCursor {
                ns: format!("{db_name}.{collection_name}"),
                docs: all_bson,
                created_at: std::time::Instant::now(),
            },
        );
        new_id
    } else {
        0i64
    };

    doc! {
        "cursor": doc! {
            "id": cursor_id,
            "ns": format!("{db_name}.{collection_name}"),
            "firstBatch": first_batch
        },
        "ok": 1.0
    }
}

fn handle_get_more(cmd: &BsonDocument) -> BsonDocument {
    let cursor_id = match cmd.get("getMore") {
        Some(Bson::Int64(i)) => *i,
        Some(Bson::Int32(i)) => *i as i64,
        _ => return doc! { "ok": 0.0, "errmsg": "Invalid cursor id in getMore" },
    };

    let batch_size = cmd.get_i32("batchSize").unwrap_or(101).max(1) as usize;

    if let Some(mut entry) = CURSOR_CACHE.get_mut(&cursor_id) {
        let ns = entry.ns.clone();
        let remaining = &mut entry.docs;
        let drain_count = batch_size.min(remaining.len());
        let batch: Vec<BsonDocument> = remaining.drain(0..drain_count).collect();
        let is_empty = remaining.is_empty();

        let return_id = if is_empty {
            drop(entry);
            CURSOR_CACHE.remove(&cursor_id);
            0i64
        } else {
            cursor_id
        };

        doc! {
            "cursor": doc! {
                "id": return_id,
                "ns": ns,
                "nextBatch": batch,
            },
            "ok": 1.0
        }
    } else {
        doc! {
            "cursor": doc! {
                "id": 0i64,
                "ns": cmd.get_str("collection").unwrap_or(""),
                "nextBatch": Vec::<BsonDocument>::new(),
            },
            "ok": 0.0,
            "errmsg": format!("Cursor {cursor_id} not found"),
            "code": 43,
            "codeName": "CursorNotFound"
        }
    }
}

fn handle_kill_cursors(cmd: &BsonDocument) -> BsonDocument {
    let mut killed = Vec::new();
    if let Ok(cursors_arr) = cmd.get_array("cursors") {
        for c in cursors_arr {
            let id = match c {
                Bson::Int64(i) => *i,
                Bson::Int32(i) => *i as i64,
                _ => continue,
            };
            if CURSOR_CACHE.remove(&id).is_some() {
                killed.push(Bson::Int64(id));
            }
        }
    }
    doc! {
        "cursorsKilled": killed,
        "cursorsNotFound": Vec::<Bson>::new(),
        "cursorsAlive": Vec::<Bson>::new(),
        "cursorsUnknown": Vec::<Bson>::new(),
        "ok": 1.0
    }
}

fn handle_aggregate(
    db: &Arc<DatabaseContext>,
    collection_name: &str,
    cmd: &BsonDocument,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let db_name = cmd.get_str("$db").unwrap_or("default");

    let all_docs = col.find_all(None);

    let result_docs = if let Ok(pipeline_array) = cmd.get_array("pipeline") {
        // Convert BSON pipeline array to JSON value
        let json_arr: Vec<serde_json::Value> = pipeline_array
            .iter()
            .filter_map(|item| {
                if let Some(d) = item.as_document() {
                    let faiz_doc = bson_to_faiz_document(d);
                    serde_json::to_value(&faiz_doc.fields).ok()
                } else {
                    None
                }
            })
            .collect();

        match faizdb_query::parse_pipeline(&serde_json::Value::Array(json_arr)) {
            Ok(stages) => {
                faizdb_query::execute_pipeline_with_collections(all_docs, &stages, |from_col| {
                    db.get_or_create_collection(from_col).find_all(None)
                })
            }
            Err(e) => {
                tracing::warn!("Aggregation pipeline parse error: {e}");
                all_docs
            }
        }
    } else {
        all_docs
    };

    let bson_docs: Vec<BsonDocument> = result_docs
        .into_iter()
        .map(|d| faiz_document_to_bson(&d))
        .collect();

    doc! {
        "cursor": doc! {
            "id": 0i64,
            "ns": format!("{db_name}.{collection_name}"),
            "firstBatch": bson_docs
        },
        "ok": 1.0
    }
}

fn handle_count(
    db: &Arc<DatabaseContext>,
    collection_name: &str,
    cmd: &BsonDocument,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let filter_doc = cmd
        .get_document("query")
        .ok()
        .or_else(|| cmd.get_document("filter").ok());

    let count = if let Some(filter) = filter_doc {
        if filter.is_empty() {
            col.stats().document_count
        } else {
            col.find_all(None)
                .into_iter()
                .filter(|d| {
                    for (k, v) in filter {
                        if k == "_id" {
                            let id_str = d.id.as_str();
                            let matches_id = match v {
                                bson::Bson::String(s) => s.as_str() == id_str,
                                bson::Bson::ObjectId(oid) => oid.to_hex() == id_str,
                                _ => false,
                            };
                            if !matches_id {
                                return false;
                            }
                        } else if let Some(val) = d.get_nested(k) {
                            let b_val = faiz_val_to_bson(val);
                            if &b_val != v {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    true
                })
                .count() as u64
        }
    } else {
        col.stats().document_count
    };

    doc! {
        "n": count as i64,
        "ok": 1.0
    }
}

fn handle_delete(
    db: &Arc<DatabaseContext>,
    collection_name: &str,
    cmd: &BsonDocument,
    msg: &OpMsg,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let mut deleted_count = 0;

    let mut all_deletes = Vec::new();
    if let Ok(deletes) = cmd.get_array("deletes") {
        for item in deletes {
            if let Some(del_doc) = item.as_document() {
                all_deletes.push(del_doc.clone());
            }
        }
    }
    let seq_deletes = msg.document_sequence("deletes");
    all_deletes.extend(seq_deletes);

    for del_doc in all_deletes {
        if let Ok(q) = del_doc.get_document("q") {
            let matching_ids: Vec<String> = col
                .find_all(None)
                .into_iter()
                .filter(|d| {
                    if q.is_empty() {
                        return true;
                    }
                    for (k, v) in q {
                        if k == "_id" {
                            let id_str = d.id.as_str();
                            let matches_id = match v {
                                bson::Bson::String(s) => s.as_str() == id_str,
                                bson::Bson::ObjectId(oid) => oid.to_hex() == id_str,
                                _ => false,
                            };
                            if !matches_id {
                                return false;
                            }
                        } else if let Some(val) = d.get_nested(k) {
                            let b_val = faiz_val_to_bson(val);
                            if &b_val != v {
                                return false;
                            }
                        } else {
                            return false;
                        }
                    }
                    true
                })
                .map(|d| d.id.as_str().to_string())
                .collect();

            for id in matching_ids {
                if col.delete_by_id(&id).is_ok() {
                    db.change_stream_bus()
                        .publish(faizdb_core::stream::ChangeEvent::delete(
                            collection_name,
                            &id,
                        ));
                    deleted_count += 1;
                }
            }
        }
    }

    doc! {
        "n": deleted_count,
        "ok": 1.0
    }
}

fn handle_update(
    db: &Arc<DatabaseContext>,
    collection_name: &str,
    cmd: &BsonDocument,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let mut modified_count = 0;

    if let Ok(updates) = cmd.get_array("updates") {
        for item in updates {
            if let Some(up_doc) = item.as_document() {
                if let (Ok(q), Ok(u)) = (up_doc.get_document("q"), up_doc.get_document("u")) {
                    let matching_ids: Vec<String> = col
                        .find_all(None)
                        .into_iter()
                        .filter(|d| {
                            for (k, v) in q {
                                if let Some(val) = d.get_nested(k) {
                                    let b_val = faiz_val_to_bson(val);
                                    if &b_val != v {
                                        return false;
                                    }
                                } else {
                                    return false;
                                }
                            }
                            true
                        })
                        .map(|d| d.id.as_str().to_string())
                        .collect();

                    // If update has "$set"
                    let set_map = u.get_document("$set").unwrap_or(u);

                    for id in matching_ids {
                        let res = col.update_by_id(&id, |d| {
                            for (k, v) in set_map {
                                d.set(k.clone(), bson_val_to_faiz(v));
                            }
                        });
                        if res.is_ok() {
                            let mut updated_fields = std::collections::BTreeMap::new();
                            for (k, v) in set_map {
                                updated_fields.insert(k.clone(), bson_val_to_faiz(v));
                            }
                            let updated_doc = col.find_by_id(&id).ok();
                            db.change_stream_bus().publish(
                                faizdb_core::stream::ChangeEvent::update(
                                    collection_name,
                                    &id,
                                    updated_fields,
                                    updated_doc,
                                ),
                            );
                            modified_count += 1;
                        }
                    }
                }
            }
        }
    }

    doc! {
        "n": modified_count,
        "nModified": modified_count,
        "ok": 1.0
    }
}

// ── BSON <-> FaizDB Conversion Helpers ───────────────────────────

pub fn bson_to_faiz_document(bdoc: &BsonDocument) -> FaizDocument {
    let mut doc = FaizDocument::new();
    for (k, v) in bdoc {
        if k == "_id" {
            if let Bson::String(s) = v {
                doc.id = s.clone().into();
            } else if let Bson::ObjectId(oid) = v {
                doc.id = oid.to_hex().into();
            }
        } else {
            doc.set(k.clone(), bson_val_to_faiz(v));
        }
    }
    doc
}

pub fn faiz_document_to_bson(doc: &FaizDocument) -> BsonDocument {
    let mut bdoc = BsonDocument::new();
    bdoc.insert("_id", Bson::String(doc.id.as_str().to_string()));
    for (k, v) in &doc.fields {
        bdoc.insert(k.clone(), faiz_val_to_bson(v));
    }
    bdoc
}

fn bson_val_to_faiz(b: &Bson) -> FaizValue {
    match b {
        Bson::Double(f) => FaizValue::Float(*f),
        Bson::String(s) => FaizValue::String(s.clone()),
        Bson::Array(arr) => FaizValue::Array(arr.iter().map(bson_val_to_faiz).collect()),
        Bson::Document(d) => {
            let mut map = std::collections::BTreeMap::new();
            for (k, v) in d {
                map.insert(k.clone(), bson_val_to_faiz(v));
            }
            FaizValue::Object(map)
        }
        Bson::Boolean(b) => FaizValue::Boolean(*b),
        Bson::Null => FaizValue::Null,
        Bson::Int32(i) => FaizValue::Integer(*i as i64),
        Bson::Int64(i) => FaizValue::Integer(*i),
        Bson::Binary(bin) => FaizValue::Binary(bin.bytes.clone()),
        Bson::ObjectId(oid) => FaizValue::String(oid.to_hex()),
        Bson::DateTime(dt) => FaizValue::DateTime(
            chrono::DateTime::from_timestamp_millis(dt.timestamp_millis()).unwrap_or_default(),
        ),
        _ => FaizValue::String(b.to_string()),
    }
}

fn faiz_val_to_bson(v: &FaizValue) -> Bson {
    match v {
        FaizValue::Null => Bson::Null,
        FaizValue::Boolean(b) => Bson::Boolean(*b),
        FaizValue::Integer(i) => Bson::Int64(*i),
        FaizValue::Float(f) => Bson::Double(*f),
        FaizValue::String(s) => Bson::String(s.clone()),
        FaizValue::Array(arr) => Bson::Array(arr.iter().map(faiz_val_to_bson).collect()),
        FaizValue::Object(obj) => {
            let mut bdoc = BsonDocument::new();
            for (k, val) in obj {
                bdoc.insert(k.clone(), faiz_val_to_bson(val));
            }
            Bson::Document(bdoc)
        }
        FaizValue::Binary(b) => Bson::Binary(bson::Binary {
            subtype: bson::spec::BinarySubtype::Generic,
            bytes: b.clone(),
        }),
        FaizValue::DateTime(dt) => Bson::DateTime(BsonDateTime::from_millis(dt.timestamp_millis())),
        FaizValue::Uuid(u) => Bson::String(u.to_string()),
        FaizValue::Vector(v) => {
            let b_arr = v.iter().map(|f| Bson::Double(*f as f64)).collect();
            Bson::Array(b_arr)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_master_handshake() {
        let resp = handle_is_master();
        assert_eq!(resp.get_bool("ismaster"), Ok(true));
        assert_eq!(resp.get_bool("isWritablePrimary"), Ok(true));
        assert_eq!(resp.get_i32("maxBsonObjectSize"), Ok(268435456));
        assert_eq!(resp.get_f64("ok"), Ok(1.0));
    }

    #[test]
    fn test_build_info() {
        let resp = handle_build_info();
        assert_eq!(resp.get_str("version"), Ok("7.0.0"));
        assert_eq!(resp.get_str("engine"), Ok("FaizDB"));
        assert_eq!(resp.get_f64("ok"), Ok(1.0));
    }

    #[test]
    fn test_insert_and_find_flow() {
        let db = Arc::new(DatabaseContext::new());
        let user_store = Arc::new(faizdb_security::UserStore::new());
        let mut session = MongoSession::new("127.0.0.1:12345".to_string());
        session.authenticated_user = Some("admin".to_string());
        session.role = Some(faizdb_security::Role::Admin);

        // Insert
        let insert_cmd = doc! {
            "insert": "users",
            "documents": [
                doc! { "name": "Ahmad Faiz", "role": "Architect", "city": "KL" },
                doc! { "name": "Linus", "role": "Creator", "city": "Portland" }
            ]
        };
        let op_msg = OpMsg::response(1, 0, insert_cmd);
        let insert_res = handle_op_msg(&db, &op_msg, &mut session, &user_store);
        let primary = insert_res.primary_document().unwrap();
        assert_eq!(primary.get_i32("n"), Ok(2));

        // Find
        let find_cmd = doc! {
            "find": "users",
            "filter": doc! { "city": "KL" }
        };
        let find_msg = OpMsg::response(2, 0, find_cmd);
        let find_res = handle_op_msg(&db, &find_msg, &mut session, &user_store);
        let primary = find_res.primary_document().unwrap();
        let cursor = primary.get_document("cursor").unwrap();
        let batch = cursor.get_array("firstBatch").unwrap();
        assert_eq!(batch.len(), 1);
        let first_doc = batch[0].as_document().unwrap();
        assert_eq!(first_doc.get_str("name"), Ok("Ahmad Faiz"));
    }
}
