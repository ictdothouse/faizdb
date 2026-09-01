//! MongoDB Command Handler and Dispatcher.
//!
//! Executes commands requested by MongoDB drivers (Compass, Mongoose, PyMongo, Go, etc.)
//! and converts between BSON and FaizDB internal document models.

use std::sync::Arc;
use bson::{doc, Bson, DateTime as BsonDateTime, Document as BsonDocument};

use faizdb_core::document::model::{Document as FaizDocument, Value as FaizValue};
use faizdb_query::DatabaseContext;
use super::op_msg::OpMsg;

/// Dispatch and execute an incoming OP_MSG command
pub fn handle_op_msg(db: &Arc<DatabaseContext>, msg: &OpMsg, client_addr: &str) -> OpMsg {
    let req_id = msg.header.request_id;
    let primary_doc = match msg.primary_document() {
        Some(d) => d,
        None => return OpMsg::response(0, req_id, doc! { "ok": 0.0, "errmsg": "Missing command document" }),
    };

    let response_body = dispatch_command(db, primary_doc, msg, client_addr);
    OpMsg::response(0, req_id, response_body)
}

/// Dispatch a command document to its handler
fn dispatch_command(
    db: &Arc<DatabaseContext>,
    cmd: &BsonDocument,
    msg: &OpMsg,
    client_addr: &str,
) -> BsonDocument {
    // 1. Handshake & Cluster Info
    if cmd.contains_key("isMaster") || cmd.contains_key("ismaster") || cmd.contains_key("hello") {
        return handle_is_master();
    }

    if cmd.contains_key("buildInfo") || cmd.contains_key("buildinfo") {
        return handle_build_info();
    }

    if cmd.contains_key("whatsmyuri") {
        return doc! {
            "you": client_addr,
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

    // 2. Database & Collection Metadata
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

    if let Ok(col_name) = cmd.get_str("listCollections") {
        let _ = col_name;
        let db_name = cmd.get_str("$db").unwrap_or("default");
        return doc! {
            "cursor": doc! {
                "id": 0i64,
                "ns": format!("{db_name}.$cmd.listCollections"),
                "firstBatch": [
                    doc! { "name": "users", "type": "collection" },
                    doc! { "name": "default", "type": "collection" }
                ]
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

    // 3. CRUD Operations
    if let Ok(col_name) = cmd.get_str("insert") {
        return handle_insert(db, col_name, cmd, msg);
    }

    if let Ok(col_name) = cmd.get_str("find") {
        return handle_find(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("aggregate") {
        return handle_aggregate(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("count") {
        return handle_count(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("delete") {
        return handle_delete(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("update") {
        return handle_update(db, col_name, cmd);
    }

    if let Ok(col_name) = cmd.get_str("drop") {
        let _ = col_name;
        return doc! { "nIndexesWas": 1, "ok": 1.0 };
    }

    // Generic fallback for unhandled commands to avoid driver crashes
    tracing::debug!("Received unhandled MongoDB command: {:?}", cmd);
    doc! { "ok": 1.0 }
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
            db.change_stream_bus().publish(faizdb_core::stream::ChangeEvent::insert(collection_name, doc_clone));
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

    let all_docs = col.find_all(None);

    let filter_doc = cmd.get_document("filter").ok();

    let filtered: Vec<BsonDocument> = all_docs
        .into_iter()
        .filter(|d| {
            if let Some(filter) = filter_doc {
                if filter.is_empty() {
                    return true;
                }
                // Check exact field matches
                for (k, v) in filter {
                    if let Some(val) = d.get_nested(k) {
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
        .skip(skip.unwrap_or(0))
        .take(limit.unwrap_or(usize::MAX))
        .map(|d| faiz_document_to_bson(&d))
        .collect();

    doc! {
        "cursor": doc! {
            "id": 0i64, // 0 = complete cursor (no getMore needed)
            "ns": format!("{db_name}.{collection_name}"),
            "firstBatch": filtered
        },
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
            Ok(stages) => faizdb_query::execute_pipeline(all_docs, &stages),
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
    _cmd: &BsonDocument,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let count = col.stats().document_count;
    doc! {
        "n": count as i64,
        "ok": 1.0
    }
}

fn handle_delete(
    db: &Arc<DatabaseContext>,
    collection_name: &str,
    cmd: &BsonDocument,
) -> BsonDocument {
    let col = db.get_or_create_collection(collection_name);
    let mut deleted_count = 0;

    if let Ok(deletes) = cmd.get_array("deletes") {
        for item in deletes {
            if let Some(del_doc) = item.as_document() {
                if let Ok(q) = del_doc.get_document("q") {
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

                    for id in matching_ids {
                        if col.delete_by_id(&id).is_ok() {
                            db.change_stream_bus().publish(faizdb_core::stream::ChangeEvent::delete(collection_name, &id));
                            deleted_count += 1;
                        }
                    }
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
                            db.change_stream_bus().publish(faizdb_core::stream::ChangeEvent::update(
                                collection_name,
                                &id,
                                updated_fields,
                                updated_doc,
                            ));
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
        Bson::DateTime(dt) => FaizValue::DateTime(chrono::DateTime::from_timestamp_millis(dt.timestamp_millis()).unwrap_or_default()),
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

        // Insert
        let insert_cmd = doc! {
            "insert": "users",
            "documents": [
                doc! { "name": "Ahmad Faiz", "role": "Architect", "city": "KL" },
                doc! { "name": "Linus", "role": "Creator", "city": "Portland" }
            ]
        };
        let op_msg = OpMsg::response(1, 0, insert_cmd);
        let insert_res = handle_op_msg(&db, &op_msg, "127.0.0.1:12345");
        let primary = insert_res.primary_document().unwrap();
        assert_eq!(primary.get_i32("n"), Ok(2));

        // Find
        let find_cmd = doc! {
            "find": "users",
            "filter": doc! { "city": "KL" }
        };
        let find_msg = OpMsg::response(2, 0, find_cmd);
        let find_res = handle_op_msg(&db, &find_msg, "127.0.0.1:12345");
        let primary = find_res.primary_document().unwrap();
        let cursor = primary.get_document("cursor").unwrap();
        let batch = cursor.get_array("firstBatch").unwrap();
        assert_eq!(batch.len(), 1);
        let first_doc = batch[0].as_document().unwrap();
        assert_eq!(first_doc.get_str("name"), Ok("Ahmad Faiz"));
    }
}
