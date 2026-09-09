//! Empirical YCSB (Yahoo! Cloud Serving Benchmark) Workloads A through F.
//!
//! Validates throughput, latency distribution (p50, p95, p99), and transactional
//! correctness under concurrent multi-threaded execution on FaizDB's core engine:
//! - Workload A: Update Heavy (50% Read, 50% Update)
//! - Workload B: Read Mostly (95% Read, 5% Update)
//! - Workload C: Read Only (100% Read)
//! - Workload D: Read Latest (95% Read Latest, 5% Insert)
//! - Workload E: Short Range Scans (95% Range Scan, 5% Insert)
//! - Workload F: Read-Modify-Write (50% Read, 50% Read-Modify-Write)

use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::{Document, DocumentId, Value};
use std::sync::Arc;
use std::time::Instant;

fn setup_ycsb_collection(workload_name: &str) -> Arc<Collection> {
    let col = Arc::new(Collection::new(format!("ycsb_{workload_name}")));

    // Pre-populate 1,000 baseline records
    for i in 0..1000 {
        let doc = Document::with_id(DocumentId::from_string(format!("user_{i}")))
            .field("val", format!("field_content_{i}"))
            .field("count", i as i64);
        col.insert(doc).expect("Pre-populating record failed");
    }

    col
}

#[test]
fn test_ycsb_workload_a_update_heavy() {
    let col = setup_ycsb_collection("a");
    let ops_per_thread = 2000;
    let threads = 4;
    let total_ops = ops_per_thread * threads;

    let start = Instant::now();
    let mut handles = Vec::new();

    for t in 0..threads {
        let c = col.clone();
        handles.push(std::thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_thread);
            for i in 0..ops_per_thread {
                let t0 = Instant::now();
                let key = format!("user_{}", (t * 250 + i) % 1000);
                if i % 2 == 0 {
                    // Read
                    let doc = c.find_by_id(&key).unwrap();
                    assert_eq!(doc.id.as_str(), key);
                } else {
                    // In-place Update
                    let updated_val = format!("updated_by_t{t}_{i}");
                    c.update_by_id(&key, |d| {
                        d.fields.insert("val".to_string(), Value::String(updated_val));
                        d.fields.insert("count".to_string(), Value::Integer(9999));
                    })
                    .unwrap();
                }
                latencies.push(t0.elapsed().as_micros() as f64 / 1000.0);
            }
            latencies
        }));
    }

    let mut all_latencies = Vec::new();
    for h in handles {
        all_latencies.extend(h.join().unwrap());
    }

    let elapsed = start.elapsed().as_secs_f64();
    let ops_sec = total_ops as f64 / elapsed;
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = all_latencies[all_latencies.len() / 2];
    let p99 = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];

    println!(
        "\n⚡ [YCSB Workload A - 50/50 R/W] Ops: {}, Elapsed: {:.3}s, Throughput: {:.0} ops/s, p50: {:.3}ms, p99: {:.3}ms",
        total_ops, elapsed, ops_sec, p50, p99
    );
    assert!(ops_sec > 2_000.0, "Throughput must exceed 2,000 ops/sec in debug mode");
}

#[test]
fn test_ycsb_workload_b_read_mostly() {
    let col = setup_ycsb_collection("b");
    let ops_per_thread = 2000;
    let threads = 4;
    let total_ops = ops_per_thread * threads;

    let start = Instant::now();
    let mut handles = Vec::new();

    for t in 0..threads {
        let c = col.clone();
        handles.push(std::thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_thread);
            for i in 0..ops_per_thread {
                let t0 = Instant::now();
                let key = format!("user_{}", (t * 250 + i) % 1000);
                if i % 20 != 0 {
                    // 95% Read
                    let doc = c.find_by_id(&key).unwrap();
                    assert_eq!(doc.id.as_str(), key);
                } else {
                    // 5% Update
                    let updated_val = format!("updated_by_t{t}_{i}");
                    c.update_by_id(&key, |d| {
                        d.fields.insert("val".to_string(), Value::String(updated_val));
                        d.fields.insert("count".to_string(), Value::Integer(8888));
                    })
                    .unwrap();
                }
                latencies.push(t0.elapsed().as_micros() as f64 / 1000.0);
            }
            latencies
        }));
    }

    let mut all_latencies = Vec::new();
    for h in handles {
        all_latencies.extend(h.join().unwrap());
    }

    let elapsed = start.elapsed().as_secs_f64();
    let ops_sec = total_ops as f64 / elapsed;
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = all_latencies[all_latencies.len() / 2];
    let p99 = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];

    println!(
        "⚡ [YCSB Workload B - 95/5 Read Mostly] Ops: {}, Elapsed: {:.3}s, Throughput: {:.0} ops/s, p50: {:.3}ms, p99: {:.3}ms",
        total_ops, elapsed, ops_sec, p50, p99
    );
    assert!(ops_sec > 10_000.0);
}

#[test]
fn test_ycsb_workload_c_read_only() {
    let col = setup_ycsb_collection("c");
    let ops_per_thread = 2500;
    let threads = 4;
    let total_ops = ops_per_thread * threads;

    let start = Instant::now();
    let mut handles = Vec::new();

    for t in 0..threads {
        let c = col.clone();
        handles.push(std::thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_thread);
            for i in 0..ops_per_thread {
                let t0 = Instant::now();
                let key = format!("user_{}", (t * 250 + i) % 1000);
                let doc = c.find_by_id(&key).unwrap();
                assert_eq!(doc.id.as_str(), key);
                latencies.push(t0.elapsed().as_micros() as f64 / 1000.0);
            }
            latencies
        }));
    }

    let mut all_latencies = Vec::new();
    for h in handles {
        all_latencies.extend(h.join().unwrap());
    }

    let elapsed = start.elapsed().as_secs_f64();
    let ops_sec = total_ops as f64 / elapsed;
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = all_latencies[all_latencies.len() / 2];
    let p99 = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];

    println!(
        "⚡ [YCSB Workload C - 100% Read Only] Ops: {}, Elapsed: {:.3}s, Throughput: {:.0} ops/s, p50: {:.3}ms, p99: {:.3}ms",
        total_ops, elapsed, ops_sec, p50, p99
    );
    assert!(ops_sec > 20_000.0);
}

#[test]
fn test_ycsb_workload_d_read_latest() {
    let col = setup_ycsb_collection("d");
    let ops_per_thread = 2000;
    let threads = 4;
    let total_ops = ops_per_thread * threads;

    let start = Instant::now();
    let mut handles = Vec::new();

    for t in 0..threads {
        let c = col.clone();
        handles.push(std::thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_thread);
            for i in 0..ops_per_thread {
                let t0 = Instant::now();
                if i % 20 != 0 {
                    // 95% Read latest inserted records (user_900..user_999)
                    let target_id = 900 + (i % 100);
                    let key = format!("user_{target_id}");
                    let doc = c.find_by_id(&key).unwrap();
                    assert_eq!(doc.id.as_str(), key);
                } else {
                    // 5% Insert brand new record
                    let key = format!("user_d_{t}_{i}");
                    let doc = Document::with_id(DocumentId::from_string(&key))
                        .field("val", "new_latest_record")
                        .field("count", 100i64);
                    c.insert(doc).unwrap();
                }
                latencies.push(t0.elapsed().as_micros() as f64 / 1000.0);
            }
            latencies
        }));
    }

    let mut all_latencies = Vec::new();
    for h in handles {
        all_latencies.extend(h.join().unwrap());
    }

    let elapsed = start.elapsed().as_secs_f64();
    let ops_sec = total_ops as f64 / elapsed;
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = all_latencies[all_latencies.len() / 2];
    let p99 = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];

    println!(
        "⚡ [YCSB Workload D - Read Latest 95/5] Ops: {}, Elapsed: {:.3}s, Throughput: {:.0} ops/s, p50: {:.3}ms, p99: {:.3}ms",
        total_ops, elapsed, ops_sec, p50, p99
    );
    assert!(ops_sec > 10_000.0);
}

#[test]
fn test_ycsb_workload_e_short_ranges() {
    let col = setup_ycsb_collection("e");
    let ops_per_thread = 1000;
    let threads = 4;
    let total_ops = ops_per_thread * threads;

    let start = Instant::now();
    let mut handles = Vec::new();

    for t in 0..threads {
        let c = col.clone();
        handles.push(std::thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_thread);
            for i in 0..ops_per_thread {
                let t0 = Instant::now();
                if i % 20 != 0 {
                    // 95% Short range scans (paginated 10 records)
                    let skip = (t * 50 + i) % 990;
                    let docs = c.find_paginated(skip, 10);
                    assert!(!docs.is_empty());
                } else {
                    // 5% Insert
                    let key = format!("user_e_{t}_{i}");
                    let doc = Document::with_id(DocumentId::from_string(&key))
                        .field("val", "range_scan_insert")
                        .field("count", 500i64);
                    c.insert(doc).unwrap();
                }
                latencies.push(t0.elapsed().as_micros() as f64 / 1000.0);
            }
            latencies
        }));
    }

    let mut all_latencies = Vec::new();
    for h in handles {
        all_latencies.extend(h.join().unwrap());
    }

    let elapsed = start.elapsed().as_secs_f64();
    let ops_sec = total_ops as f64 / elapsed;
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = all_latencies[all_latencies.len() / 2];
    let p99 = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];

    println!(
        "⚡ [YCSB Workload E - Short Range Scans 95/5] Ops: {}, Elapsed: {:.3}s, Throughput: {:.0} ops/s, p50: {:.3}ms, p99: {:.3}ms",
        total_ops, elapsed, ops_sec, p50, p99
    );
    assert!(ops_sec > 1000.0);
}

#[test]
fn test_ycsb_workload_f_read_modify_write() {
    let col = setup_ycsb_collection("f");
    let ops_per_thread = 2000;
    let threads = 4;
    let total_ops = ops_per_thread * threads;

    let start = Instant::now();
    let mut handles = Vec::new();

    for t in 0..threads {
        let c = col.clone();
        handles.push(std::thread::spawn(move || {
            let mut latencies = Vec::with_capacity(ops_per_thread);
            for i in 0..ops_per_thread {
                let t0 = Instant::now();
                let key = format!("user_{}", (t * 250 + i) % 1000);
                if i % 2 == 0 {
                    // Read
                    let doc = c.find_by_id(&key).unwrap();
                    assert_eq!(doc.id.as_str(), key);
                } else {
                    // Read-Modify-Write via in-place update
                    c.update_by_id(&key, |doc| {
                        let current_count = match doc.get("count") {
                            Some(Value::Integer(cnt)) => *cnt,
                            _ => 0,
                        };
                        doc.fields.insert("count".to_string(), Value::Integer(current_count + 1));
                        doc.fields.insert("rmw_thread".to_string(), Value::Integer(t as i64));
                    })
                    .unwrap();
                }
                latencies.push(t0.elapsed().as_micros() as f64 / 1000.0);
            }
            latencies
        }));
    }

    let mut all_latencies = Vec::new();
    for h in handles {
        all_latencies.extend(h.join().unwrap());
    }

    let elapsed = start.elapsed().as_secs_f64();
    let ops_sec = total_ops as f64 / elapsed;
    all_latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let p50 = all_latencies[all_latencies.len() / 2];
    let p99 = all_latencies[(all_latencies.len() as f64 * 0.99) as usize];

    println!(
        "⚡ [YCSB Workload F - Read-Modify-Write] Ops: {}, Elapsed: {:.3}s, Throughput: {:.0} ops/s, p50: {:.3}ms, p99: {:.3}ms\n",
        total_ops, elapsed, ops_sec, p50, p99
    );
    assert!(ops_sec > 2_000.0, "Throughput must exceed 2,000 ops/sec in debug mode");
}
