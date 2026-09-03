//! FaizDB Official Criterion Micro-Benchmark Suite
//!
//! Measures raw engine throughput and microsecond latencies:
//! - Concurrent Document Ingestion (MemTable + Skiplist)
//! - Sequential Table Scan Throughput
//! - Secondary Index Point Lookup Latency
//! - Persistent WAL & Storage Engine Append Throughput

use std::sync::Arc;
use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use faizdb_core::document::collection::Collection;
use faizdb_core::document::model::{Document, Value};
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};

fn bench_collection_ingestion(c: &mut Criterion) {
    let mut group = c.benchmark_group("ingestion");
    for size in [1_000, 10_000, 50_000].iter() {
        group.throughput(Throughput::Elements(*size as u64));
        group.bench_with_input(BenchmarkId::new("concurrent_insert", size), size, |b, &s| {
            b.iter_batched(
                || {
                    let col = Arc::new(Collection::new("bench_col"));
                    let docs: Vec<Document> = (0..s)
                        .map(|i| {
                            let mut d = Document::new();
                            d.set("seq", i as i64);
                            d.set("name", format!("User_{i}"));
                            d.set("score", 99.5f64);
                            d.set("active", i % 2 == 0);
                            d
                        })
                        .collect();
                    (col, docs)
                },
                |(col, docs)| {
                    for doc in docs {
                        let _ = col.insert(black_box(doc));
                    }
                },
                criterion::BatchSize::SmallInput,
            );
        });
    }
    group.finish();
}

fn bench_sequential_scan(c: &mut Criterion) {
    let col = Arc::new(Collection::new("bench_scan_col"));
    for i in 0..50_000 {
        let mut d = Document::new();
        d.set("seq", i as i64);
        d.set("name", format!("User_{i}"));
        d.set("score", (i % 100) as f64);
        let _ = col.insert(d);
    }

    let mut group = c.benchmark_group("scan");
    group.throughput(Throughput::Elements(50_000));
    group.bench_function("scan_50k_documents", |b| {
        b.iter(|| {
            let docs = col.find_all(None);
            black_box(docs.len());
        });
    });
    group.finish();
}

fn bench_secondary_index_lookup(c: &mut Criterion) {
    let col = Arc::new(Collection::new("bench_index_col"));
    col.create_secondary_index("email", false).unwrap();

    for i in 0..10_000 {
        let mut d = Document::new();
        d.set("email", format!("user_{i}@example.com"));
        d.set("seq", i as i64);
        let _ = col.insert(d);
    }

    let mut group = c.benchmark_group("index");
    group.throughput(Throughput::Elements(1));
    group.bench_function("point_lookup_indexed", |b| {
        let mut i = 0;
        b.iter(|| {
            i = (i + 1) % 10_000;
            let val = Value::String(format!("user_{i}@example.com"));
            let res = col.find_by_secondary_index("email", &val);
            black_box(res);
        });
    });
    group.finish();
}

fn bench_persistent_storage_wal(c: &mut Criterion) {
    let mut group = c.benchmark_group("storage_wal");
    group.throughput(Throughput::Elements(1_000));

    group.bench_function("persistent_put_1k_records", |b| {
        b.iter_batched(
            || {
                let temp_dir = tempfile::tempdir().unwrap();
                let config = StorageConfig {
                    data_dir: temp_dir.path().to_path_buf(),
                    sync_writes: false, // Group commit
                    ..Default::default()
                };
                let storage = StorageEngine::open(config).unwrap();
                (temp_dir, storage)
            },
            |(_temp_dir, storage)| {
                for i in 0..1_000 {
                    let key = format!("user:{i:08}");
                    let val = b"{\"name\":\"Faiz\",\"role\":\"Architect\",\"active\":true}";
                    storage.put(key.as_bytes(), val).unwrap();
                }
            },
            criterion::BatchSize::SmallInput,
        );
    });
    group.finish();
}

criterion_group!(
    benches,
    bench_collection_ingestion,
    bench_sequential_scan,
    bench_secondary_index_lookup,
    bench_persistent_storage_wal
);
criterion_main!(benches);
