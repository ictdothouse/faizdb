use criterion::{criterion_group, criterion_main, Criterion};
use faizdb_core::document::model::{Document, Value};
use faizdb_core::document::collection::Collection;

fn bench_collection_insert(c: &mut Criterion) {
    c.bench_function("collection_insert_document", |b| {
        let col = Collection::new("bench_col");
        let mut idx = 0i64;
        b.iter(|| {
            idx += 1;
            let doc = Document::new()
                .field("seq", Value::Integer(idx))
                .field("name", Value::String("Benchmark User".into()))
                .field("score", Value::Float(99.9));
            let _ = col.insert(doc);
        });
    });
}

criterion_group!(benches, bench_collection_insert);
criterion_main!(benches);
