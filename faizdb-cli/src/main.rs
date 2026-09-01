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

    /// Start the HTTP/REST server
    Serve {
        /// Port to bind
        #[arg(short, long, default_value = "27018")]
        port: u16,
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
    },

    /// Run AI Vector Similarity Search demo
    VectorDemo,

    /// Run GraphRAG Knowledge Graph demo
    GraphDemo,
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
        Some(Commands::Serve { port, host }) => {
            let addr = format!("{host}:{port}");
            println!("🔥 Starting FaizDB Server on http://{addr} ...");
            faizdb_server::run_server(&addr).await?;
        }
        Some(Commands::Info) => print_info(),
        Some(Commands::Benchmark { count }) => run_benchmark(count),
        Some(Commands::VectorDemo) => run_vector_demo(),
        Some(Commands::GraphDemo) => run_graph_demo(),
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

fn run_benchmark(count: usize) {
    println!("🏎️ FaizDB High-Throughput Benchmark — {} documents\n", count);

    let db = DatabaseContext::new();
    let col = db.get_or_create_collection("bench");

    // 1. Bulk Insertion Benchmark
    let start = std::time::Instant::now();
    for i in 0..count {
        let doc = Document::new()
            .field("seq", Value::Integer(i as i64))
            .field("title", Value::String(format!("FaizDB Record #{i}")))
            .field("score", Value::Float((i % 100) as f64 * 1.5))
            .field("active", Value::Boolean(i % 2 == 0));
        let _ = col.insert(doc);
    }
    let insert_dur = start.elapsed();
    let insert_ops = count as f64 / insert_dur.as_secs_f64();

    println!("⚡ INSERT : {:>8} docs in {:>8.2?} ({:>10.0} ops/sec)", count, insert_dur, insert_ops);

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
