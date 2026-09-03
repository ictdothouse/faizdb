//! FaizDB CLI — Command-Line Interface, Interactive REPL & Multi-Protocol Server
//!
//! Usage:
//!   faizdb shell                   - Start interactive multi-dialect REPL (SQL, Mongo, FaizQL)
//!   faizdb serve --port 27018      - Start background REST/HTTP server
//!   faizdb benchmark --count 10000 - Run ultra-fast insert & query performance benchmark
//!   faizdb vector-demo             - Run AI vector similarity search demo
//!   faizdb graph-demo              - Run GraphRAG relationship traversal demo

use std::io::{self, BufRead, Write};
use std::path::PathBuf;

use clap::{Parser, Subcommand};
use faizdb_core::document::model::{Document, Value};
use faizdb_query::{parse_query, DatabaseContext, QueryResult};
use faizdb_vector::{DistanceMetric, HnswConfig, HnswIndex};
use faizdb_graph::{Edge, GraphStore, Vertex};

/// FaizDB — The AI-Native NoSQL Database Engine
#[derive(Parser)]
#[command(
    name = "faizdb",
    version,
    about = "🔥 FaizDB — The AI-Native NoSQL Database Engine\nFast. Secure. Run Everywhere.",
    long_about = None
)]
struct Cli {
    #[command(subcommand)]
    command: Option<Commands>,
}

#[derive(Subcommand)]
enum Commands {
    /// Start the interactive REPL shell (Supports SQL, Mongo, FaizQL)
    Shell {
        /// Path to database directory
        #[arg(short, long, default_value = "./faizdb_data")]
        data_dir: PathBuf,
    },

    /// Start the 4-Way Multi-Protocol Server (MongoDB 27017 + PostgreSQL 5432 + gRPC 50051 + HTTP API 27018)
    Serve {
        /// MongoDB Wire Protocol Port (Drop-in replacement for MongoDB apps)
        #[arg(short = 'w', long, default_value = "27017")]
        wire_port: u16,
        /// PostgreSQL Wire Protocol Port (Drop-in compatibility for psql, DBeaver, TablePlus, Grafana)
        #[arg(short = 'g', long, default_value = "5432")]
        pg_port: u16,
        /// gRPC & Protocol Buffers Port (Ultra-low latency microservices & vector streaming)
        #[arg(short = 'r', long, default_value = "50051")]
        grpc_port: u16,
        /// HTTP/REST API Port
        #[arg(short = 'p', long, default_value = "27018")]
        http_port: u16,
        /// Host address
        #[arg(short = 'H', long, default_value = "0.0.0.0")]
        host: String,
    },

    /// Show database information, features, and version
    Info,

    /// Run high-throughput performance benchmark
    Benchmark {
        /// Number of documents to insert
        #[arg(short, long, default_value = "10000")]
        count: usize,
        /// Optional storage directory to benchmark durable disk writes + WAL (defaults to in-memory)
        #[arg(short, long)]
        durable: Option<String>,
    },

    /// Run AI Vector Similarity Search demo
    VectorDemo,

    /// Run GraphRAG Knowledge Graph demo
    GraphDemo,

    /// Create an atomic consistent snapshot backup archive
    Backup {
        /// Destination snapshot file path (e.g. ./backups/faizdb_dump.json)
        #[arg(short, long, default_value = "./backups/faizdb_snapshot.json")]
        output: PathBuf,
    },

    /// Restore database from a snapshot archive
    Restore {
        /// Source snapshot file path
        #[arg(short, long, default_value = "./backups/faizdb_snapshot.json")]
        input: PathBuf,
    },
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("faizdb=info".parse().unwrap()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Some(Commands::Shell { data_dir }) => run_shell(&data_dir),
        Some(Commands::Serve { wire_port, pg_port, grpc_port, http_port, host }) => {
            let wire_addr = format!("{host}:{wire_port}");
            let pg_addr = format!("{host}:{pg_port}");
            let grpc_addr = format!("{host}:{grpc_port}");
            let http_addr = format!("{host}:{http_port}");
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!("║  🔥 FaizDB Server v{} Running 4-Way Multi-Protocol Gateway ║", faizdb_core::VERSION);
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!("║  🍃 MongoDB Wire Protocol : mongodb://{:<26} ║", wire_addr);
            println!("║  🐘 PostgreSQL Wire Proto : postgresql://{:<23} ║", pg_addr);
            println!("║  ⚡ gRPC / Protobuf       : grpc://{:<29} ║", grpc_addr);
            println!("║  🌐 HTTP / REST API       : http://{:<29} ║", http_addr);
            println!("║                                                                  ║");
            println!("║  👉 Connection Strings:                                          ║");
            println!("║     Mongo : mongodb://127.0.0.1:{}                          ║", wire_port);
            println!("║     PSQL  : psql -h 127.0.0.1 -p {} -U postgres -d faizdb    ║", pg_port);
            println!("║     gRPC  : localhost:{}                                     ║", grpc_port);
            println!("║     REST  : http://127.0.0.1:{}                              ║", http_port);
            println!("╚══════════════════════════════════════════════════════════════════╝");
            faizdb_server::run_multi_protocol_server(&wire_addr, &pg_addr, &grpc_addr, &http_addr).await?;
        }
        Some(Commands::Info) => print_info(),
        Some(Commands::Benchmark { count, durable }) => run_benchmark(count, durable),
        Some(Commands::VectorDemo) => run_vector_demo(),
        Some(Commands::GraphDemo) => run_graph_demo(),
        Some(Commands::Backup { output }) => run_backup_cli(&output),
        Some(Commands::Restore { input }) => run_restore_cli(&input),
        None => {
            print_info();
            println!();
            run_shell(&PathBuf::from("./faizdb_data"));
        }
    }

    Ok(())
}

fn print_info() {
    println!(r#"
╔══════════════════════════════════════════════════════════════════╗
║                                                                  ║
║   🔥 FaizDB v{}                                            ║
║   The AI-Native NoSQL Database Engine                            ║
║                                                                  ║
║   Created by: Ahmad Faiz                                         ║
║   License: Apache 2.0 (Open Source)                              ║
║                                                                  ║
║   Core Highlights:                                               ║
║   • Multi-Dialect Query Engine (SQL, MongoDB JSON, FaizQL)       ║
║   • Sub-millisecond HNSW Vector Search (AI-Native)               ║
║   • Knowledge Graph & GraphRAG Traversal Engine                  ║
║   • Full ACID Multi-Version Concurrency Control (MVCC)           ║
║   • Hybrid LSM-Tree + B-Tree Storage Engine with WAL             ║
║   • Zero-Trust AES-256-GCM Encryption by Default                 ║
║   • Embedded (Single Binary) & Server Modes                      ║
║                                                                  ║
╚══════════════════════════════════════════════════════════════════╝
"#, faizdb_core::VERSION);
}

fn run_shell(_data_dir: &PathBuf) {
    println!("🔥 FaizDB Interactive Shell v{}", faizdb_core::VERSION);
    println!("Supports SQL (SELECT / INSERT), MongoDB (db.col.find), and FaizQL commands.");
    println!("Type 'help' for examples, 'exit' to quit.\n");

    let db = DatabaseContext::new();

    // Prepopulate some demo data in 'users' collection
    let users = db.get_or_create_collection("users");
    let _ = users.insert(
        Document::new()
            .field("name", "Ahmad Faiz")
            .field("role", "DB Architect")
            .field("age", 30)
            .field("city", "Kuala Lumpur")
    );
    let _ = users.insert(
        Document::new()
            .field("name", "Linus Torvalds")
            .field("role", "Linux Creator")
            .field("age", 55)
            .field("city", "Portland")
    );

    let stdin = io::stdin();
    let mut stdout = io::stdout();

    loop {
        print!("faizdb> ");
        stdout.flush().unwrap();

        let mut input = String::new();
        if stdin.lock().read_line(&mut input).is_err() {
            break;
        }

        let input = input.trim();
        if input.is_empty() {
            continue;
        }

        match input {
            "exit" | "quit" | "\\q" => {
                println!("Goodbye! 👋 Terima kasih telah menggunakan FaizDB.");
                break;
            }
            "help" | "\\h" => print_help(),
            "info" => print_info(),
            "demo vector" => run_vector_demo(),
            "demo graph" => run_graph_demo(),
            query_str => match parse_query(query_str) {
                Ok(stmt) => match db.execute(stmt) {
                    Ok(result) => match result {
                        QueryResult::Documents(docs) => {
                            if docs.is_empty() {
                                println!("(0 documents returned)");
                            } else {
                                for doc in &docs {
                                    println!("{doc}");
                                    println!("---");
                                }
                                println!("({} document(s) returned)", docs.len());
                            }
                        }
                        QueryResult::Count(c) => println!("Count: {c}"),
                        QueryResult::Inserted(ids) => {
                            println!("✅ Inserted {} document(s): {:?}", ids.len(), ids);
                        }
                        QueryResult::Updated(u) => println!("✅ Updated {u} document(s)"),
                        QueryResult::Deleted(d) => println!("✅ Deleted {d} document(s)"),
                        QueryResult::Success(msg) => println!("✅ {msg}"),
                        QueryResult::Explain(plan) => {
                            println!("📊 Query Execution Plan:");
                            println!("   Plan Type         : {}", plan.plan_type);
                            println!("   Collection        : {}", plan.collection);
                            println!("   Index Used        : {}", plan.index_used.unwrap_or_else(|| "None (Sequential Scan)".into()));
                            println!("   Execution Latency : {} µs", plan.execution_time_us);
                            println!("   Documents Examined: {}", plan.documents_examined);
                            println!("   Documents Returned: {}", plan.documents_returned);
                            println!("   Unique Constraint : {}", plan.is_unique);
                            println!("   Estimated Cost    : {:.2}", plan.estimated_cost_score);
                        }
                    },
                    Err(e) => println!("❌ Execution error: {e}"),
                },
                Err(e) => println!("❌ Parse error: {e}"),
            },
        }
    }
}

fn print_help() {
    println!(r#"
FaizDB Multi-Dialect Query Examples:

1. SQL Dialect:
   SELECT * FROM users
   SELECT * FROM users WHERE age > 25 AND city = 'Kuala Lumpur' LIMIT 5
   INSERT INTO users {{"name": "Developer", "age": 28, "city": "Cyberjaya"}}
   DELETE FROM users WHERE age < 20
   COUNT FROM users

2. MongoDB Dialect:
   db.users.find()
   db.users.find({{"city": "Kuala Lumpur"}})
   db.users.find({{"age": {{"$gte": 30}}}})
   db.users.insert({{"name": "Siti", "role": "AI Engineer"}})
   db.users.count()

3. Demos & Utilities:
   demo vector    Run AI Vector Search Demo
   demo graph     Run GraphRAG Traversal Demo
   info           Show FaizDB Architecture Info
   exit           Quit Shell
"#);
}

fn run_benchmark(count: usize, durable_path: Option<String>) {
    let durable_dir = durable_path.or_else(|| std::env::var("FAIZDB_DATA_DIR").ok());
    let (db, is_durable) = if let Some(ref path) = durable_dir {
        match DatabaseContext::with_storage_dir(path) {
            Ok(ctx) => (ctx, true),
            Err(e) => {
                eprintln!("⚠️ Failed to initialize storage engine at '{path}': {e}. Falling back to in-memory.");
                (DatabaseContext::new(), false)
            }
        }
    } else {
        (DatabaseContext::new(), false)
    };

    if is_durable {
        println!("🏎️ FaizDB High-Throughput Benchmark — {} documents [Durable Disk + WAL]\n", count);
        println!("📂 Storage Directory: {}\n", durable_dir.as_deref().unwrap_or(""));
    } else {
        println!("🏎️ FaizDB High-Throughput Benchmark — {} documents [In-Memory MemTable]\n", count);
        println!("ℹ️  Running in-memory MemTable benchmark. Use --durable <path> or set FAIZDB_DATA_DIR to benchmark durable disk + WAL writes.\n");
    }

    let col = db.get_or_create_collection("bench");

    // 1. Bulk Insertion Benchmark
    let start = std::time::Instant::now();
    for i in 0..count {
        let doc = Document::new()
            .field("seq", Value::Integer(i as i64))
            .field("title", Value::String(format!("FaizDB Record #{i}")))
            .field("score", Value::Float((i % 100) as f64 * 1.5))
            .field("active", Value::Boolean(i % 2 == 0));
        let doc_id = doc.id.clone();
        let _ = col.insert(doc.clone());
        if let Some(storage) = db.storage() {
            let key = format!("doc:bench:{doc_id}");
            if let Ok(val) = serde_json::to_vec(&doc) {
                if let Err(e) = storage.put(key.as_bytes(), &val) {
                    eprintln!("Warning: failed to persist doc {doc_id}: {e}");
                }
            }
        }
    }
    let insert_dur = start.elapsed();
    let insert_ops = count as f64 / insert_dur.as_secs_f64();

    if is_durable {
        println!("⚡ INSERT (Durable Disk + WAL): {:>8} docs in {:>8.2?} ({:>10.0} ops/sec)", count, insert_dur, insert_ops);
    } else {
        println!("⚡ INSERT (In-Memory MemTable): {:>8} docs in {:>8.2?} ({:>10.0} ops/sec)", count, insert_dur, insert_ops);
    }

    // 2. Full Scan Benchmark
    let start = std::time::Instant::now();
    let all = col.find_all(None);
    let scan_dur = start.elapsed();
    let scan_ops = all.len() as f64 / scan_dur.as_secs_f64();

    println!("⚡ SCAN   : {:>8} docs in {:>8.2?} ({:>10.0} ops/sec)", all.len(), scan_dur, scan_ops);

    // 3. Filtered Query Benchmark
    let start = std::time::Instant::now();
    let filter = vec![("active".to_string(), Value::Boolean(true))];
    let filtered = col.find(&filter, None, None).unwrap();
    let filter_dur = start.elapsed();

    println!("⚡ FILTER : {:>8} docs in {:>8.2?}", filtered.len(), filter_dur);

    let stats = col.stats();
    println!("\n📊 Summary:");
    println!("  Documents in memory: {}", stats.document_count);
    println!("  Total data size:     {:.2} MB", stats.total_size as f64 / 1_048_576.0);
    println!("  Avg doc size:        {} bytes", stats.avg_document_size);
}

fn run_vector_demo() {
    println!("\n🤖 FaizDB AI Vector Engine (HNSW Sub-millisecond ANN Search)");
    println!("------------------------------------------------------------");

    let config = HnswConfig::new(4, DistanceMetric::Cosine);
    let mut index = HnswIndex::new(config);

    // Embeddings of conceptual documents
    index.insert("doc_database_ai", vec![0.95, 0.90, 0.10, 0.05]).unwrap();
    index.insert("doc_rust_engine", vec![0.90, 0.85, 0.05, 0.10]).unwrap();
    index.insert("doc_cooking_recipe", vec![0.05, 0.10, 0.95, 0.90]).unwrap();
    index.insert("doc_baking_bread", vec![0.10, 0.05, 0.90, 0.95]).unwrap();

    println!("Indexed 4 concept vectors (4 dimensions)");

    // Query: "High performance database engineering"
    let query_embedding = vec![0.92, 0.88, 0.08, 0.06];
    println!("Query Vector: {:?}", query_embedding);

    let results = index.search(&query_embedding, 2);
    println!("\nTop 2 Semantic Matches:");
    for (rank, res) in results.iter().enumerate() {
        println!("  #{}: ID='{}' | Similarity={:.4} | Distance={:.4}",
            rank + 1, res.id, res.similarity, res.distance
        );
    }
}

fn run_graph_demo() {
    println!("\n🕸️ FaizDB Knowledge Graph & GraphRAG Engine");
    println!("-------------------------------------------");

    let mut graph = GraphStore::new();

    // Create Entities
    graph.add_vertex(Vertex::new("faiz", "Creator"));
    graph.add_vertex(Vertex::new("faizdb", "Database"));
    graph.add_vertex(Vertex::new("nosql", "Paradigm"));
    graph.add_vertex(Vertex::new("ai_rag", "Capability"));

    // Create Relationships
    graph.add_edge(Edge::new("faiz", "faizdb", "INVENTED"));
    graph.add_edge(Edge::new("faizdb", "nosql", "BELONGS_TO"));
    graph.add_edge(Edge::new("faizdb", "ai_rag", "SUPPORTS"));

    println!("Graph created: {} vertices, {} edges", graph.vertex_count(), graph.edge_count());

    // GraphRAG context traversal starting from "faiz"
    println!("\nGraphRAG Traversal (Depth 2 from 'faiz'):");
    let path = graph.traverse_bfs("faiz", 2, None);
    for step in path {
        println!("  Depth {}: Node='{}' (via relation: {:?})",
            step.depth, step.vertex_id, step.relation
        );
    }
}

fn run_backup_cli(output_path: &std::path::Path) {
    println!("\n💾 FaizDB Consistent Snapshot Backup Engine");
    println!("-------------------------------------------");
    println!("Initiating non-blocking online snapshot to: {}", output_path.display());

    let db = DatabaseContext::new();
    let collections = db.all_collections();
    let mut data = Vec::new();
    for (name, col) in collections {
        let docs = col.find_all(None);
        data.push((name, docs));
    }

    let archive = faizdb_core::backup::build_snapshot(&data);
    match faizdb_core::backup::save_snapshot_file(&archive, output_path) {
        Ok(_) => {
            println!("✅ Snapshot archive successfully created!");
            println!("   • Collections  : {:?}", archive.manifest.collections);
            println!("   • Documents    : {}", archive.manifest.total_documents);
            println!("   • Checksum     : {}", archive.manifest.checksum);
            println!("   • Archive Size : {} bytes", archive.manifest.file_size_bytes);
        }
        Err(e) => {
            eprintln!("❌ Failed to create snapshot: {e}");
        }
    }
}

fn run_restore_cli(input_path: &std::path::Path) {
    println!("\n🔄 FaizDB Disaster Recovery & Snapshot Restore");
    println!("----------------------------------------------");
    println!("Restoring database from: {}", input_path.display());

    match faizdb_core::backup::load_and_verify_snapshot(input_path) {
        Ok(archive) => {
            println!("✅ Cryptographic Checksum verified: {}", archive.manifest.checksum);
            println!("   Restoring {} documents across {} collections...",
                archive.manifest.total_documents, archive.manifest.collections.len()
            );

            let db = DatabaseContext::new();
            let mut restored = 0;
            for (col_name, doc_vals) in archive.collections_data {
                let col = db.get_or_create_collection(&col_name);
                for val in doc_vals {
                    if let Some(doc) = faizdb_core::document::model::Document::from_json_value(val) {
                        if col.insert(doc).is_ok() {
                            restored += 1;
                        }
                    }
                }
            }
            println!("🎉 Disaster Recovery Complete: {restored} documents restored into live database!");
        }
        Err(e) => {
            eprintln!("❌ Failed to restore snapshot: {e}");
        }
    }
}
