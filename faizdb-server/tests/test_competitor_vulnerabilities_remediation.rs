//! Comprehensive Verification Suite for Advanced Multi-Model Engine Capabilities & Compliance.
//!
//! Verifies mission-critical enterprise features:
//! 1. PostgreSQL Extended Query Protocol ('P' Parse, 'B' Bind, 'D' Describe, 'E' Execute, 'S' Sync, 'C' Close)
//! 2. MongoDB Wire Protocol O(1) _id lookup and Stateful Cursor pagination (getMore and killCursors)
//! 3. HNSW Vector Engine Tombstone Deletion and In-Place Mutation
//! 4. Relational SQL Multi-Table INNER JOIN and LEFT JOIN
//! 5. Knowledge Graph Edge Deduplication and Incident Reference Management
//! 6. Distributed Raft Cluster Election Quorum Verification

use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;

use bson::{doc, Document as BsonDocument};
use faizdb_core::document::model::{Document as FaizDocument, Value};
use faizdb_graph::{Direction, Edge, GraphStore};
use faizdb_query::{parse_query, DatabaseContext, QueryResult};
use faizdb_security::{Role, UserStore};
use faizdb_server::wire::header::{MsgHeader, HEADER_LEN};
use faizdb_server::wire::op_msg::OpMsg;
use faizdb_server::wire::run_wire_server;
use faizdb_vector::distance::DistanceMetric;
use faizdb_vector::hnsw::{HnswConfig, HnswIndex};

/// Helper: Send an OP_MSG to a MongoDB wire stream and decode the response document
async fn send_op_msg(stream: &mut TcpStream, body: BsonDocument) -> BsonDocument {
    let msg = OpMsg::response(100, 0, body);
    let bytes = msg.encode().expect("Failed to encode OP_MSG");
    stream
        .write_all(&bytes)
        .await
        .expect("Failed to write to stream");
    stream.flush().await.expect("Failed to flush stream");

    let mut head_buf = [0u8; HEADER_LEN];
    stream
        .read_exact(&mut head_buf)
        .await
        .expect("Failed to read header");
    let header = MsgHeader::decode(&head_buf).expect("Failed to decode header");

    let body_len = (header.message_length as usize).saturating_sub(HEADER_LEN);
    let mut full = Vec::with_capacity(header.message_length as usize);
    full.extend_from_slice(&head_buf);
    if body_len > 0 {
        let mut body_buf = vec![0u8; body_len];
        stream
            .read_exact(&mut body_buf)
            .await
            .expect("Failed to read body");
        full.extend_from_slice(&body_buf);
    }

    let resp = OpMsg::decode(&full).expect("Failed to decode response OP_MSG");
    resp.primary_document().cloned().unwrap_or_default()
}

// ── 1. PostgreSQL Extended Query Protocol Verification ───────────────────────

#[tokio::test]
async fn test_postgres_extended_query_protocol_lifecycle() {
    let user_store = Arc::new(UserStore::new());
    user_store
        .create_user("test_admin", "admin123", Role::Admin)
        .unwrap();

    let db = Arc::new(DatabaseContext::new());
    let users_col = db.get_or_create_collection("users");

    let mut u1 = FaizDocument::new();
    u1.id = "u100".into();
    u1.set("name", "Alice");
    u1.set("age", 28);
    users_col.insert(u1).unwrap();

    // Pick an available random ephemeral port
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let pg_addr = addr.to_string();
    let db_clone = db.clone();
    let user_store_clone = user_store.clone();

    tokio::spawn(async move {
        let _ = faizdb_server::wire::postgres::run_postgres_server(
            &pg_addr,
            db_clone,
            user_store_clone,
        )
        .await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();

    // 1. Send Startup Message
    let mut startup_body = Vec::new();
    startup_body.extend_from_slice(&196608i32.to_be_bytes()); // Protocol v3.0
    startup_body.extend_from_slice(b"user\0test_admin\0database\0default\0\0");
    let total_startup_len = (4 + startup_body.len()) as i32;

    stream
        .write_all(&total_startup_len.to_be_bytes())
        .await
        .unwrap();
    stream.write_all(&startup_body).await.unwrap();
    stream.flush().await.unwrap();

    // Read password challenge ('R' with code 3)
    let mut auth_req = [0u8; 9];
    stream.read_exact(&mut auth_req).await.unwrap();
    assert_eq!(auth_req[0], b'R');
    assert_eq!(
        i32::from_be_bytes([auth_req[5], auth_req[6], auth_req[7], auth_req[8]]),
        3
    );

    // Send PasswordMessage ('p')
    let pass = b"admin123\0";
    let p_len = (4 + pass.len()) as i32;
    let mut p_msg = vec![b'p'];
    p_msg.extend_from_slice(&p_len.to_be_bytes());
    p_msg.extend_from_slice(pass);
    stream.write_all(&p_msg).await.unwrap();
    stream.flush().await.unwrap();

    // Read until ReadyForQuery ('Z')
    let mut init_buf = vec![0u8; 2048];
    let mut total_init = 0;
    while !init_buf[..total_init].contains(&b'Z') {
        let n = stream.read(&mut init_buf[total_init..]).await.unwrap();
        if n == 0 {
            break;
        }
        total_init += n;
    }
    assert!(
        init_buf[..total_init].contains(&b'Z'),
        "Server must complete startup handshake with ReadyForQuery ('Z')"
    );

    // 2. Extended Query Protocol: 'P' (Parse) statement with parameter $1
    let query_sql = "SELECT * FROM users WHERE id = $1\0";
    let stmt_name = "stmt_get_user\0";
    let mut parse_body = Vec::new();
    parse_body.extend_from_slice(stmt_name.as_bytes());
    parse_body.extend_from_slice(query_sql.as_bytes());
    parse_body.extend_from_slice(&1i16.to_be_bytes()); // 1 parameter
    parse_body.extend_from_slice(&25i32.to_be_bytes()); // PG_TYPE_TEXT (25)

    let parse_len = (4 + parse_body.len()) as i32;
    stream.write_all(b"P").await.unwrap();
    stream.write_all(&parse_len.to_be_bytes()).await.unwrap();
    stream.write_all(&parse_body).await.unwrap();
    stream.flush().await.unwrap();

    // Assert ParseComplete ('1')
    let mut resp_type = [0u8; 1];
    stream.read_exact(&mut resp_type).await.unwrap();
    assert_eq!(resp_type[0], b'1', "Server must return ParseComplete ('1')");
    let mut resp_len = [0u8; 4];
    stream.read_exact(&mut resp_len).await.unwrap();
    assert_eq!(i32::from_be_bytes(resp_len), 4);

    // 3. Extended Query Protocol: 'B' (Bind) portal with $1 = "u100"
    let portal_name = "portal_get_user\0";
    let mut bind_body = Vec::new();
    bind_body.extend_from_slice(portal_name.as_bytes());
    bind_body.extend_from_slice(stmt_name.as_bytes());
    bind_body.extend_from_slice(&0i16.to_be_bytes()); // 0 format codes (all text)
    bind_body.extend_from_slice(&1i16.to_be_bytes()); // 1 parameter value
    let param_val = b"u100";
    bind_body.extend_from_slice(&(param_val.len() as i32).to_be_bytes());
    bind_body.extend_from_slice(param_val);
    bind_body.extend_from_slice(&0i16.to_be_bytes()); // 0 result format codes

    let bind_len = (4 + bind_body.len()) as i32;
    stream.write_all(b"B").await.unwrap();
    stream.write_all(&bind_len.to_be_bytes()).await.unwrap();
    stream.write_all(&bind_body).await.unwrap();
    stream.flush().await.unwrap();

    // Assert BindComplete ('2')
    stream.read_exact(&mut resp_type).await.unwrap();
    assert_eq!(resp_type[0], b'2', "Server must return BindComplete ('2')");
    stream.read_exact(&mut resp_len).await.unwrap();
    assert_eq!(i32::from_be_bytes(resp_len), 4);

    // 4. Extended Query Protocol: 'D' (Describe) Portal
    let mut desc_body = Vec::new();
    desc_body.push(b'P'); // Describe portal
    desc_body.extend_from_slice(portal_name.as_bytes());
    let desc_len = (4 + desc_body.len()) as i32;
    stream.write_all(b"D").await.unwrap();
    stream.write_all(&desc_len.to_be_bytes()).await.unwrap();
    stream.write_all(&desc_body).await.unwrap();
    stream.flush().await.unwrap();

    // Read response ('n' NoData or 'T' RowDescription)
    stream.read_exact(&mut resp_type).await.unwrap();
    assert!(resp_type[0] == b'n' || resp_type[0] == b'T');
    stream.read_exact(&mut resp_len).await.unwrap();
    let body_len = (i32::from_be_bytes(resp_len) - 4) as usize;
    let mut skip_body = vec![0u8; body_len];
    stream.read_exact(&mut skip_body).await.unwrap();

    // 5. Extended Query Protocol: 'E' (Execute)
    let mut exec_body = Vec::new();
    exec_body.extend_from_slice(portal_name.as_bytes());
    exec_body.extend_from_slice(&0i32.to_be_bytes()); // 0 = unlimited rows
    let exec_len = (4 + exec_body.len()) as i32;

    stream.write_all(b"E").await.unwrap();
    stream.write_all(&exec_len.to_be_bytes()).await.unwrap();
    stream.write_all(&exec_body).await.unwrap();

    // 6. Extended Query Protocol: 'S' (Sync)
    stream.write_all(&[b'S', 0, 0, 0, 4]).await.unwrap();
    stream.flush().await.unwrap();

    // Read execution output until ReadyForQuery ('Z')
    let mut exec_buf = vec![0u8; 4096];
    let mut total_read = 0;
    while !exec_buf[..total_read].contains(&b'Z') {
        let n = stream.read(&mut exec_buf[total_read..]).await.unwrap();
        if n == 0 {
            break;
        }
        total_read += n;
    }

    let exec_str = String::from_utf8_lossy(&exec_buf[..total_read]);
    assert!(
        exec_str.contains("Alice"),
        "Executed portal must return data for Alice"
    );
    assert!(
        exec_str.contains("SELECT 1"),
        "Must return CommandComplete for SELECT 1"
    );
    assert!(
        exec_buf[..total_read].contains(&b'Z'),
        "Sync must conclude with ReadyForQuery ('Z')"
    );

    // 7. Extended Query Protocol: 'C' (Close)
    let mut close_body = Vec::new();
    close_body.push(b'P');
    close_body.extend_from_slice(portal_name.as_bytes());
    let close_len = (4 + close_body.len()) as i32;
    stream.write_all(b"C").await.unwrap();
    stream.write_all(&close_len.to_be_bytes()).await.unwrap();
    stream.write_all(&close_body).await.unwrap();
    stream.flush().await.unwrap();

    stream.read_exact(&mut resp_type).await.unwrap();
    assert_eq!(resp_type[0], b'3', "Server must return CloseComplete ('3')");
}

// ── 2. MongoDB Wire O(1) Lookup & Stateful Cursor Pagination ─────────────────

#[tokio::test]
async fn test_mongo_wire_o1_lookup_and_cursor_pagination() {
    let user_store = Arc::new(UserStore::new());
    user_store
        .create_user("test_mongo_admin", "admin123", Role::Admin)
        .unwrap();

    let db = Arc::new(DatabaseContext::new());
    let items_col = db.get_or_create_collection("inventory");

    // Insert 5 items
    for i in 1..=5 {
        let mut d = FaizDocument::new();
        d.id = format!("item_{i}").into();
        d.set("sku", format!("SKU-{i}"));
        d.set("qty", i * 10);
        items_col.insert(d).unwrap();
    }

    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    drop(listener);

    let mongo_addr = addr.to_string();
    let db_clone = db.clone();
    let user_store_clone = user_store.clone();

    tokio::spawn(async move {
        let _ = run_wire_server(&mongo_addr, db_clone, user_store_clone).await;
    });

    tokio::time::sleep(std::time::Duration::from_millis(100)).await;

    let mut stream = TcpStream::connect(addr).await.unwrap();

    // Authenticate
    let auth_resp = send_op_msg(
        &mut stream,
        doc! {
            "authenticate": 1,
            "user": "test_mongo_admin",
            "pwd": "admin123"
        },
    )
    .await;
    assert_eq!(
        auth_resp.get_f64("ok").unwrap(),
        1.0,
        "Authentication must succeed"
    );

    // 1. Test O(1) find by _id
    let find_id_resp = send_op_msg(
        &mut stream,
        doc! {
            "find": "inventory",
            "filter": doc! { "_id": "item_3" },
            "$db": "default"
        },
    )
    .await;
    let cursor_doc = find_id_resp.get_document("cursor").unwrap();
    let batch = cursor_doc.get_array("firstBatch").unwrap();
    assert_eq!(batch.len(), 1, "O(1) search must return exactly 1 document");
    let item_doc = batch[0].as_document().unwrap();
    assert_eq!(item_doc.get_str("_id").unwrap(), "item_3");

    // 2. Test Stateful Cursor Pagination: find with batchSize = 2
    let find_paginated_resp = send_op_msg(
        &mut stream,
        doc! {
            "find": "inventory",
            "batchSize": 2,
            "$db": "default"
        },
    )
    .await;
    let cursor_doc = find_paginated_resp.get_document("cursor").unwrap();
    let cursor_id = cursor_doc.get_i64("id").unwrap();
    let first_batch = cursor_doc.get_array("firstBatch").unwrap();

    assert_eq!(
        first_batch.len(),
        2,
        "First batch must yield exactly 2 items"
    );
    assert_ne!(
        cursor_id, 0,
        "Cursor ID must be non-zero for stateful pagination"
    );

    // 3. getMore next 2 items using the stateful cursor
    let get_more_resp = send_op_msg(
        &mut stream,
        doc! {
            "getMore": cursor_id,
            "collection": "inventory",
            "batchSize": 2,
            "$db": "default"
        },
    )
    .await;
    let cursor_doc = get_more_resp.get_document("cursor").unwrap();
    let next_batch = cursor_doc.get_array("nextBatch").unwrap();
    assert_eq!(next_batch.len(), 2, "Second batch must yield 2 items");
    let next_cursor_id = cursor_doc.get_i64("id").unwrap();
    assert_eq!(
        next_cursor_id, cursor_id,
        "Cursor must stay open when items remain"
    );

    // 4. getMore final item: cursor should exhaust and return id: 0
    let final_resp = send_op_msg(
        &mut stream,
        doc! {
            "getMore": cursor_id,
            "collection": "inventory",
            "batchSize": 2,
            "$db": "default"
        },
    )
    .await;
    let cursor_doc = final_resp.get_document("cursor").unwrap();
    let final_batch = cursor_doc.get_array("nextBatch").unwrap();
    assert_eq!(
        final_batch.len(),
        1,
        "Final batch must yield remaining 1 item"
    );
    assert_eq!(
        cursor_doc.get_i64("id").unwrap(),
        0,
        "Exhausted cursor must return id: 0"
    );

    // 5. Test killCursors command
    let kill_resp = send_op_msg(
        &mut stream,
        doc! {
            "killCursors": "inventory",
            "cursors": [cursor_id],
            "$db": "default"
        },
    )
    .await;
    assert_eq!(kill_resp.get_f64("ok").unwrap(), 1.0);
}

// ── 3. HNSW Vector Tombstone Deletion & In-Place Mutation ────────────────────

#[test]
fn test_hnsw_vector_tombstone_and_update_remediation() {
    let dim = 4;
    let config = HnswConfig::new(dim, DistanceMetric::Euclidean);
    let mut index = HnswIndex::new(config);

    index.insert("doc_1", vec![1.0, 0.0, 0.0, 0.0]).unwrap();
    index.insert("doc_2", vec![0.0, 1.0, 0.0, 0.0]).unwrap();
    index.insert("doc_3", vec![0.0, 0.0, 1.0, 0.0]).unwrap();

    assert_eq!(index.len(), 3);
    assert_eq!(index.deleted_count(), 0);

    // Initial search should find doc_1
    let results = index.search(&[0.9, 0.0, 0.0, 0.0], 1);
    assert_eq!(results[0].id, "doc_1");

    // GDPR Deletion: tombstone doc_1
    assert!(index.delete("doc_1"));
    assert_eq!(index.len(), 2);
    assert_eq!(index.deleted_count(), 1);
    assert!(!index.contains_id("doc_1"));

    // doc_1 must never be returned again
    let post_delete_res = index.search(&[0.9, 0.0, 0.0, 0.0], 2);
    assert!(!post_delete_res.iter().any(|r| r.id == "doc_1"));

    // Mutation: update doc_2 with new embedding
    index.update("doc_2", vec![1.0, 0.0, 0.0, 0.0]).unwrap();
    assert_eq!(index.len(), 2);
    let updated_res = index.search(&[0.9, 0.0, 0.0, 0.0], 1);
    assert_eq!(
        updated_res[0].id, "doc_2",
        "Updated vector doc_2 must now match the new coordinates"
    );
}

// ── 4. Relational SQL Multi-Table JOIN Verification ──────────────────────────

#[test]
fn test_sql_inner_and_left_join_remediation() {
    let ctx = DatabaseContext::new();
    let customers = ctx.get_or_create_collection("customers");
    let orders = ctx.get_or_create_collection("orders");

    let mut c1 = FaizDocument::new();
    c1.id = "c10".into();
    c1.set("customer_name", "TechCorp");
    customers.insert(c1).unwrap();

    let mut o1 = FaizDocument::new();
    o1.id = "o100".into();
    o1.set("cust_id", "c10");
    o1.set("total", 999);
    orders.insert(o1).unwrap();

    let mut o2 = FaizDocument::new();
    o2.id = "o200".into();
    o2.set("cust_id", "c99"); // Unmatched customer
    o2.set("total", 450);
    orders.insert(o2).unwrap();

    // 1. INNER JOIN
    let inner_sql =
        parse_query("SELECT * FROM orders JOIN customers ON orders.cust_id = customers.id")
            .unwrap();
    let inner_res = ctx.execute(inner_sql).unwrap();
    match inner_res {
        QueryResult::Documents(docs) => {
            assert_eq!(docs.len(), 1, "INNER JOIN must only yield matching records");
            assert_eq!(docs[0].id.as_str(), "o100");
            assert_eq!(
                docs[0].get("customer_name"),
                Some(&Value::String("TechCorp".to_string()))
            );
            assert_eq!(
                docs[0].get("customers_customer_name"),
                Some(&Value::String("TechCorp".to_string()))
            );
        }
        _ => panic!("Expected QueryResult::Documents"),
    }

    // 2. LEFT JOIN
    let left_sql =
        parse_query("SELECT * FROM orders LEFT JOIN customers ON orders.cust_id = customers.id")
            .unwrap();
    let left_res = ctx.execute(left_sql).unwrap();
    match left_res {
        QueryResult::Documents(docs) => {
            assert_eq!(docs.len(), 2, "LEFT JOIN must yield all left table records");
        }
        _ => panic!("Expected QueryResult::Documents"),
    }
}

// ── 5. Knowledge Graph Deduplication & Deletion Verification ─────────────────

#[test]
fn test_graph_deduplication_and_dangling_pruning_remediation() {
    let mut graph = GraphStore::new();

    // 1. Deduplication check
    graph.add_edge(Edge::new("Org1", "Server1", "OWNS"));
    graph.add_edge(Edge::new("Org1", "Server1", "OWNS"));
    graph.add_edge(Edge::new("Org1", "Server1", "OWNS"));

    assert_eq!(
        graph.edge_count(),
        1,
        "Identical edges must be deduplicated in-place"
    );
    assert_eq!(graph.vertex_count(), 2);

    // 2. Dangling edge pruning on node removal
    graph.add_edge(Edge::new("Server1", "Database1", "HOSTS"));
    assert_eq!(graph.edge_count(), 2);
    assert_eq!(graph.vertex_count(), 3);

    // Delete Server1
    assert!(graph.remove_vertex("Server1"));
    assert_eq!(graph.vertex_count(), 2);
    assert_eq!(
        graph.edge_count(),
        0,
        "Removing Server1 must cleanly purge incident edges without dangling pointers"
    );
    assert!(graph.edges("Org1", Direction::Outgoing, None).is_empty());
    assert!(graph
        .edges("Database1", Direction::Incoming, None)
        .is_empty());
}

// ── 6. Distributed Raft Quorum Election Verification ─────────────────────────

#[tokio::test]
async fn test_raft_cluster_election_quorum_verification() {
    let ctx = Arc::new(DatabaseContext::new());
    let raft = ctx.raft();

    // 1. Initially Leader in standalone 1-node mode
    assert_eq!(raft.get_info().role, faizdb_core::cluster::NodeRole::Leader);
    assert!(raft.get_info().is_leader);

    // 2. Add 2 peers to cluster (now 3 nodes, quorum requires 2 votes)
    raft.add_peer("node_2".to_string(), "127.0.0.1:27022".to_string());
    raft.add_peer("node_3".to_string(), "127.0.0.1:27023".to_string());
    assert_eq!(raft.list_peers().len(), 2);
    assert_eq!(raft.quorum_size(), 2);

    // 3. Start election: self votes (1/3), which is less than quorum (2) -> transitions to Candidate!
    let (term2, _) = raft.start_election();
    assert_eq!(
        raft.get_info().role,
        faizdb_core::cluster::NodeRole::Candidate
    );
    assert!(!raft.get_info().is_leader);

    // 4. Record vote from node_2 (now 2/3 votes -> reaches quorum)
    let became_leader = raft.record_vote("node_2", term2, true);
    assert!(became_leader);
    assert_eq!(raft.get_info().role, faizdb_core::cluster::NodeRole::Leader);
    assert!(raft.get_info().is_leader);
}
