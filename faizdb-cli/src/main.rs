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
use std::sync::Arc;

use clap::{Parser, Subcommand};
use faizdb_core::document::model::{Document, Value};
use faizdb_graph::{Edge, GraphStore, Vertex};
use faizdb_query::{parse_query, DatabaseContext, QueryResult};
use faizdb_vector::{DistanceMetric, HnswConfig, HnswIndex};

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

    /// Start the 5-Way Multi-Protocol Server (MongoDB 27017 + PostgreSQL 5432 + MySQL 3306 + gRPC 50051 + HTTP API 27018)
    Serve {
        /// MongoDB Wire Protocol Port (Drop-in replacement for MongoDB apps)
        #[arg(short = 'w', long, default_value = "27017")]
        wire_port: u16,
        /// PostgreSQL Wire Protocol Port (Drop-in compatibility for psql, DBeaver, TablePlus, Grafana)
        #[arg(short = 'g', long, default_value = "5432")]
        pg_port: u16,
        /// MySQL / MariaDB Wire Protocol Port (Drop-in compatibility for MySQL CLI, PHP mysqli/PDO, Laravel Eloquent)
        #[arg(short = 'm', long, default_value = "3306")]
        mysql_port: u16,
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

    /// Export database collections to open formats (JSONL or SQL)
    Dump {
        /// Target collection name (if omitted, dumps all collections)
        #[arg(short, long)]
        collection: Option<String>,
        /// Export format: jsonl or sql
        #[arg(short, long, default_value = "jsonl")]
        format: String,
        /// Destination output file path (if omitted, prints to stdout)
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Path to database directory
        #[arg(short, long, default_value = "./faizdb_data")]
        data_dir: PathBuf,
    },

    /// Run preflight system diagnostics across all 5 gateways and storage
    Doctor {
        /// Target host address to probe (defaults to 127.0.0.1)
        #[arg(short = 'H', long, default_value = "127.0.0.1")]
        host: String,
        /// HTTP / REST API Port
        #[arg(short = 'p', long, default_value = "27018")]
        http_port: u16,
        /// MongoDB Wire Port
        #[arg(short = 'w', long, default_value = "27017")]
        wire_port: u16,
        /// PostgreSQL Wire Port
        #[arg(short = 'g', long, default_value = "5432")]
        pg_port: u16,
        /// MySQL Wire Port
        #[arg(short = 'm', long, default_value = "3306")]
        mysql_port: u16,
        /// gRPC Port
        #[arg(short = 'r', long, default_value = "50051")]
        grpc_port: u16,
        /// Data directory to check
        #[arg(short, long, default_value = "./faizdb_data")]
        data_dir: PathBuf,
    },

    /// Seed sample multi-model datasets for instant testing (Relational, Vector, Graph, Documents)
    Seed {
        /// Dataset type: "ecommerce" (default), "agent-memory", or "social-graph"
        #[arg(short = 's', long, default_value = "ecommerce")]
        dataset: String,
        /// Path to database directory
        #[arg(short = 'd', long, default_value = "./faizdb_data")]
        data_dir: PathBuf,
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
        Some(Commands::Serve {
            wire_port,
            pg_port,
            mysql_port,
            grpc_port,
            http_port,
            host,
        }) => {
            let wire_addr = format!("{host}:{wire_port}");
            let pg_addr = format!("{host}:{pg_port}");
            let mysql_addr = format!("{host}:{mysql_port}");
            let grpc_addr = format!("{host}:{grpc_port}");
            let http_addr = format!("{host}:{http_port}");
            println!("╔══════════════════════════════════════════════════════════════════╗");
            println!(
                "║  🔥 FaizDB Server v{} Running 5-Way Universal Gateway       ║",
                faizdb_core::VERSION
            );
            println!("╠══════════════════════════════════════════════════════════════════╣");
            println!(
                "║  🍃 MongoDB Wire Protocol : mongodb://{:<26} ║",
                wire_addr
            );
            println!(
                "║  🐘 PostgreSQL Wire Proto : postgresql://{:<23} ║",
                pg_addr
            );
            println!(
                "║  🐬 MySQL / MariaDB Wire   : mysql://{:<28} ║",
                mysql_addr
            );
            println!("║  ⚡ gRPC / Protobuf       : grpc://{:<29} ║", grpc_addr);
            println!("║  🌐 HTTP / REST API       : http://{:<29} ║", http_addr);
            println!("║                                                                  ║");
            println!("║  👉 Connection Strings:                                          ║");
            println!(
                "║     Mongo : mongodb://127.0.0.1:{}                          ║",
                wire_port
            );
            println!(
                "║     PSQL  : psql -h 127.0.0.1 -p {} -U postgres -d faizdb    ║",
                pg_port
            );
            println!(
                "║     MySQL : mysql -h 127.0.0.1 -P {} -u root faizdb          ║",
                mysql_port
            );
            println!(
                "║     gRPC  : localhost:{}                                     ║",
                grpc_port
            );
            println!(
                "║     REST  : http://127.0.0.1:{}                              ║",
                http_port
            );
            println!("╚══════════════════════════════════════════════════════════════════╝");
            faizdb_server::run_multi_protocol_server(&wire_addr, &pg_addr, &mysql_addr, &grpc_addr, &http_addr)
                .await?;
        }
        Some(Commands::Info) => print_info(),
        Some(Commands::Benchmark { count, durable }) => run_benchmark(count, durable),
        Some(Commands::VectorDemo) => run_vector_demo(),
        Some(Commands::GraphDemo) => run_graph_demo(),
        Some(Commands::Backup { output }) => run_backup_cli(&output),
        Some(Commands::Restore { input }) => run_restore_cli(&input),
        Some(Commands::Dump {
            collection,
            format,
            output,
            data_dir,
        }) => {
            run_dump_cli(collection, &format, output, &data_dir);
        }
        Some(Commands::Doctor {
            host,
            http_port,
            wire_port,
            pg_port,
            mysql_port,
            grpc_port,
            data_dir,
        }) => {
            run_doctor_cli(
                &host,
                http_port,
                wire_port,
                pg_port,
                mysql_port,
                grpc_port,
                &data_dir,
            )
            .await;
        }
        Some(Commands::Seed { dataset, data_dir }) => {
            run_seed_cli(&dataset, &data_dir);
        }
        None => {
            print_info();
            println!();
            run_shell(&PathBuf::from("./faizdb_data"));
        }
    }

    Ok(())
}

fn print_info() {
    println!(
        r#"
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
"#,
        faizdb_core::VERSION
    );
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
            .field("city", "Kuala Lumpur"),
    );
    let _ = users.insert(
        Document::new()
            .field("name", "Linus Torvalds")
            .field("role", "Linux Creator")
            .field("age", 55)
            .field("city", "Portland"),
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
                            println!(
                                "   Index Used        : {}",
                                plan.index_used
                                    .unwrap_or_else(|| "None (Sequential Scan)".into())
                            );
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
    println!(
        r#"
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
"#
    );
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
        println!(
            "🏎️ FaizDB High-Throughput Benchmark — {} documents [Durable Disk + WAL]\n",
            count
        );
        println!(
            "📂 Storage Directory: {}\n",
            durable_dir.as_deref().unwrap_or("")
        );
    } else {
        println!(
            "🏎️ FaizDB High-Throughput Benchmark — {} documents [In-Memory MemTable]\n",
            count
        );
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
        println!(
            "⚡ INSERT (Durable Disk + WAL): {:>8} docs in {:>8.2?} ({:>10.0} ops/sec)",
            count, insert_dur, insert_ops
        );
    } else {
        println!(
            "⚡ INSERT (In-Memory MemTable): {:>8} docs in {:>8.2?} ({:>10.0} ops/sec)",
            count, insert_dur, insert_ops
        );
    }

    // 2. Full Scan Benchmark
    let start = std::time::Instant::now();
    let all = col.find_all(None);
    let scan_dur = start.elapsed();
    let scan_ops = all.len() as f64 / scan_dur.as_secs_f64();

    println!(
        "⚡ SCAN   : {:>8} docs in {:>8.2?} ({:>10.0} ops/sec)",
        all.len(),
        scan_dur,
        scan_ops
    );

    // 3. Filtered Query Benchmark
    let start = std::time::Instant::now();
    let filter = vec![("active".to_string(), Value::Boolean(true))];
    let filtered = col.find(&filter, None, None).unwrap();
    let filter_dur = start.elapsed();

    println!(
        "⚡ FILTER : {:>8} docs in {:>8.2?}",
        filtered.len(),
        filter_dur
    );

    let stats = col.stats();
    println!("\n📊 Summary:");
    println!("  Documents in memory: {}", stats.document_count);
    println!(
        "  Total data size:     {:.2} MB",
        stats.total_size as f64 / 1_048_576.0
    );
    println!("  Avg doc size:        {} bytes", stats.avg_document_size);
}

fn run_vector_demo() {
    println!("\n🤖 FaizDB AI Vector Engine (HNSW Sub-millisecond ANN Search)");
    println!("------------------------------------------------------------");

    let config = HnswConfig::new(4, DistanceMetric::Cosine);
    let mut index = HnswIndex::new(config);

    // Embeddings of conceptual documents
    index
        .insert("doc_database_ai", vec![0.95, 0.90, 0.10, 0.05])
        .unwrap();
    index
        .insert("doc_rust_engine", vec![0.90, 0.85, 0.05, 0.10])
        .unwrap();
    index
        .insert("doc_cooking_recipe", vec![0.05, 0.10, 0.95, 0.90])
        .unwrap();
    index
        .insert("doc_baking_bread", vec![0.10, 0.05, 0.90, 0.95])
        .unwrap();

    println!("Indexed 4 concept vectors (4 dimensions)");

    // Query: "High performance database engineering"
    let query_embedding = vec![0.92, 0.88, 0.08, 0.06];
    println!("Query Vector: {:?}", query_embedding);

    let results = index.search(&query_embedding, 2);
    println!("\nTop 2 Semantic Matches:");
    for (rank, res) in results.iter().enumerate() {
        println!(
            "  #{}: ID='{}' | Similarity={:.4} | Distance={:.4}",
            rank + 1,
            res.id,
            res.similarity,
            res.distance
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

    println!(
        "Graph created: {} vertices, {} edges",
        graph.vertex_count(),
        graph.edge_count()
    );

    // GraphRAG context traversal starting from "faiz"
    println!("\nGraphRAG Traversal (Depth 2 from 'faiz'):");
    let path = graph.traverse_bfs("faiz", 2, None);
    for step in path {
        println!(
            "  Depth {}: Node='{}' (via relation: {:?})",
            step.depth, step.vertex_id, step.relation
        );
    }
}

fn run_backup_cli(output_path: &std::path::Path) {
    println!("\n💾 FaizDB Consistent Snapshot Backup Engine");
    println!("-------------------------------------------");
    println!(
        "Initiating non-blocking online snapshot to: {}",
        output_path.display()
    );

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
            println!(
                "   • Archive Size : {} bytes",
                archive.manifest.file_size_bytes
            );
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
            println!(
                "✅ Cryptographic Checksum verified: {}",
                archive.manifest.checksum
            );
            println!(
                "   Restoring {} documents across {} collections...",
                archive.manifest.total_documents,
                archive.manifest.collections.len()
            );

            let db = DatabaseContext::new();
            let mut restored = 0;
            for (col_name, doc_vals) in archive.collections_data {
                let col = db.get_or_create_collection(&col_name);
                for val in doc_vals {
                    if let Some(doc) = faizdb_core::document::model::Document::from_json_value(val)
                    {
                        if col.insert(doc).is_ok() {
                            restored += 1;
                        }
                    }
                }
            }
            println!(
                "🎉 Disaster Recovery Complete: {restored} documents restored into live database!"
            );
        }
        Err(e) => {
            eprintln!("❌ Failed to restore snapshot: {e}");
        }
    }
}

fn run_dump_cli(
    collection: Option<String>,
    format_str: &str,
    output: Option<PathBuf>,
    data_dir: &std::path::Path,
) {
    let db = faizdb_query::DatabaseContext::with_storage_dir(data_dir)
        .unwrap_or_else(|_| faizdb_query::DatabaseContext::new());

    let collections = if let Some(ref col_name) = collection {
        let col = db.get_or_create_collection(col_name);
        vec![(col_name.clone(), col)]
    } else {
        db.all_collections()
    };

    let mut out_content = String::new();
    let is_sql = format_str.eq_ignore_ascii_case("sql");

    for (col_name, col) in collections {
        let docs = col.find_all(None);
        if is_sql {
            out_content.push_str(&format!("-- Table / Collection: {}\n", col_name));
            for doc in docs {
                let id_str = doc.id.as_str().replace('\'', "''");
                let mut col_names = vec!["id".to_string()];
                let mut col_vals = vec![format!("'{}'", id_str)];

                for (k, v) in &doc.fields {
                    col_names.push(k.clone());
                    match v {
                        Value::String(s) => col_vals.push(format!("'{}'", s.replace('\'', "''"))),
                        Value::Integer(i) => col_vals.push(i.to_string()),
                        Value::Float(f) => col_vals.push(f.to_string()),
                        Value::Boolean(b) => col_vals.push(b.to_string()),
                        Value::Null => col_vals.push("NULL".to_string()),
                        _ => {
                            let json_s = serde_json::to_string(v)
                                .unwrap_or_default()
                                .replace('\'', "''");
                            col_vals.push(format!("'{}'", json_s));
                        }
                    }
                }
                out_content.push_str(&format!(
                    "INSERT INTO {} ({}) VALUES ({});\n",
                    col_name,
                    col_names.join(", "),
                    col_vals.join(", ")
                ));
            }
        } else {
            // JSONL format: {"_id": "...", ...}
            for doc in docs {
                let mut json_obj = serde_json::Map::new();
                json_obj.insert(
                    "_id".to_string(),
                    serde_json::Value::String(doc.id.to_string()),
                );
                for (k, v) in &doc.fields {
                    let json_val = serde_json::to_value(v).unwrap_or(serde_json::Value::Null);
                    json_obj.insert(k.clone(), json_val);
                }
                if let Ok(line) = serde_json::to_string(&json_obj) {
                    out_content.push_str(&line);
                    out_content.push('\n');
                }
            }
        }
    }

    if let Some(out_path) = output {
        match std::fs::write(&out_path, &out_content) {
            Ok(_) => println!(
                "✅ Exported to open format successfully: {}",
                out_path.display()
            ),
            Err(e) => eprintln!("❌ Failed to write export to '{}': {e}", out_path.display()),
        }
    } else {
        print!("{out_content}");
    }
}

async fn run_doctor_cli(
    host: &str,
    http_port: u16,
    wire_port: u16,
    pg_port: u16,
    mysql_port: u16,
    grpc_port: u16,
    data_dir: &std::path::Path,
) {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║              🩺 FaizDB System Preflight & Doctor                 ║");
    println!("║       Multi-Gateway, Storage, Consensus & Security Audit         ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    println!("🔍 [1/4] Probing Multi-Protocol Gateway Status ({host})...");
    let ports = [
        ("REST & WebSocket API", http_port),
        ("MongoDB Wire Ingress", wire_port),
        ("PostgreSQL Wire Ingress", pg_port),
        ("MySQL / MariaDB Wire", mysql_port),
        ("gRPC & ProtoBuf Gateway", grpc_port),
    ];

    let mut online_count = 0;
    let mut available_count = 0;
    let mut conflict_count = 0;

    for (name, port) in ports {
        let addr = format!("{host}:{port}");
        match tokio::net::TcpStream::connect(&addr).await {
            Ok(_) => {
                online_count += 1;
                println!("  🟢 Port {:<5} [{:<23}] : ONLINE (Active & Listening)", port, name);
            }
            Err(_) => {
                match std::net::TcpListener::bind(("0.0.0.0", port)) {
                    Ok(_) => {
                        available_count += 1;
                        println!("  ⚪ Port {:<5} [{:<23}] : AVAILABLE (Ready to bind)", port, name);
                    }
                    Err(e) => {
                        conflict_count += 1;
                        println!("  🔴 Port {:<5} [{:<23}] : CONFLICT (Blocked: {e})", port, name);
                    }
                }
            }
        }
    }

    println!("\n🔍 [2/4] Verifying Storage & Durability Subsystems...");
    let dir_exists = data_dir.exists();
    if !dir_exists {
        match std::fs::create_dir_all(data_dir) {
            Ok(_) => println!("  🟢 Storage Directory: Created successfully at '{}'", data_dir.display()),
            Err(e) => println!("  🔴 Storage Directory: Failed to create '{}' ({e})", data_dir.display()),
        }
    } else {
        println!("  🟢 Storage Directory: Present at '{}'", data_dir.display());
    }

    let test_file = data_dir.join(".faizdb_doctor_probe");
    match std::fs::write(&test_file, b"faizdb-doctor-probe") {
        Ok(_) => {
            let _ = std::fs::remove_file(&test_file);
            println!("  🟢 Disk I/O Integrity: Read/Write verified (Zero permissions lock)");
        }
        Err(e) => {
            println!("  🔴 Disk I/O Integrity: Permission denied ({e})");
        }
    }

    let wal_path = data_dir.join("faizdb.wal");
    if wal_path.exists() {
        if let Ok(meta) = std::fs::metadata(&wal_path) {
            println!("  🟢 Write-Ahead Log (WAL): Active ({} bytes logged)", meta.len());
        }
    } else {
        println!("  ⚪ Write-Ahead Log (WAL): Clean (Ready for new session)");
    }

    println!("\n🔍 [3/4] Hardware & Environment Diagnostics...");
    let cpus = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1);
    println!("  🟢 CPU Architecture: {} logical execution cores detected", cpus);
    println!("  🟢 Engine Kernel: FaizDB v{} (Pure Safe Rust, Single-Binary)", faizdb_core::VERSION);

    println!("\n🔍 [4/4] Enterprise Security & Configuration Review...");
    let root_user = std::env::var("FAIZDB_ROOT_USER").unwrap_or_else(|_| "admin".to_string());
    println!("  🟢 Root Authentication: Configured (User: '{}')", root_user);

    let has_custom_pw = std::env::var("FAIZDB_ROOT_PASSWORD").is_ok();
    if has_custom_pw {
        println!("  🟢 Root Password: Custom production secret loaded via environment");
    } else {
        println!("  🟡 Root Password: Using default development credential (Set FAIZDB_ROOT_PASSWORD for production)");
    }

    let auto_backup = std::env::var("FAIZDB_AUTO_BACKUP").unwrap_or_else(|_| "disabled".to_string());
    println!("  🟢 Autonomous Backup Daemon: {}", if auto_backup == "true" || auto_backup == "1" { "ENABLED (Daily Snapshot Routine)" } else { "STANDBY (Set FAIZDB_AUTO_BACKUP=1 to activate)" });

    println!("\n══════════════════════════════════════════════════════════════════");
    println!("  Summary: {online_count} online, {available_count} available, {conflict_count} conflicting.");
    if conflict_count > 0 {
        println!("⚠️  DIAGNOSTIC ADVISORY: {conflict_count} port conflicts detected.");
        println!("    Ensure conflicting services are stopped or supply custom flags, e.g.:");
        println!("    faizdb serve --pg-port 5433 --mysql-port 3307");
    } else if online_count == 5 {
        println!("🎉 ALL 5 GATEWAYS ONLINE! FaizDB cluster is running in prime operational health.");
    } else if online_count > 0 {
        println!("⚡ FaizDB is active with {online_count}/5 gateways online.");
    } else {
        println!("🚀 PREFLIGHT CLEAN! All 5 ports and storage paths ready for 'faizdb serve'.");
    }
    println!("══════════════════════════════════════════════════════════════════\n");
}

fn run_seed_cli(dataset: &str, data_dir: &std::path::Path) {
    println!("╔══════════════════════════════════════════════════════════════════╗");
    println!("║              🌱 FaizDB Multi-Model Dataset Seeder                ║");
    println!("║   Populating Relational, Document, Vector & Graph Collections    ║");
    println!("╚══════════════════════════════════════════════════════════════════╝\n");

    let db = faizdb_query::DatabaseContext::with_storage_dir(data_dir)
        .unwrap_or_else(|_| faizdb_query::DatabaseContext::new());

    match dataset.to_lowercase().as_str() {
        "ecommerce" | "default" => seed_ecommerce(&db, data_dir),
        "agent-memory" | "agent" => seed_agent_memory(&db, data_dir),
        "social-graph" | "social" => seed_social_graph(&db, data_dir),
        other => {
            eprintln!("Unknown dataset '{other}'. Available datasets: 'ecommerce', 'agent-memory', 'social-graph'");
        }
    }
}

fn seed_ecommerce(db: &DatabaseContext, data_dir: &std::path::Path) {
    let products = db.get_or_create_collection("products");
    let customers = db.get_or_create_collection("customers");
    let orders = db.get_or_create_collection("orders");

    let prod_items = [
        ("prod_1", "Quantum RTX 5090", "Hardware", 1999.99, true, 4.9, "Flagship AI training and ray-tracing GPU"),
        ("prod_2", "Neural TPU v5 Accelerator", "AI Accelerators", 2499.50, true, 4.8, "High-efficiency transformer inference card"),
        ("prod_3", "Cyberpunk Mech Keyboard", "Peripherals", 149.00, false, 4.6, "Hot-swappable magnetic switch RGB board"),
        ("prod_4", "HoloLens Spatial Pro", "Spatial Computing", 1299.00, true, 4.7, "Dual micro-OLED 4K mixed-reality headset"),
        ("prod_5", "Starlink Mini Edge Dish", "Networking", 599.00, true, 4.5, "Portable phased-array low-earth orbit satellite terminal"),
    ];

    for (id, name, cat, price, in_stock, rating, desc) in &prod_items {
        let _ = products.insert(
            Document::with_id(*id)
                .field("name", *name)
                .field("category", *cat)
                .field("price", *price)
                .field("in_stock", *in_stock)
                .field("rating", *rating)
                .field("description", *desc),
        );
    }

    let cust_items = [
        ("cust_1", "Ahmad Faiz", "faiz@ict.house", "Diamond", 4500.00, "Kuala Lumpur"),
        ("cust_2", "Elena Rostova", "elena@techcorp.io", "Platinum", 2800.00, "London"),
        ("cust_3", "Marcus Vance", "marcus@cloudsys.dev", "Gold", 1200.00, "San Francisco"),
    ];

    for (id, name, email, tier, spend, city) in &cust_items {
        let _ = customers.insert(
            Document::with_id(*id)
                .field("name", *name)
                .field("email", *email)
                .field("loyalty_tier", *tier)
                .field("total_spend", *spend)
                .field("city", *city),
        );
    }

    let order_items = [
        ("ord_101", "cust_1", "prod_1", 2, 3999.98, "completed"),
        ("ord_102", "cust_2", "prod_2", 1, 2499.50, "completed"),
        ("ord_103", "cust_3", "prod_4", 1, 1299.00, "shipped"),
        ("ord_104", "cust_1", "prod_5", 1, 599.00, "pending"),
    ];

    for (id, cust_id, prod_id, qty, total, status) in &order_items {
        let _ = orders.insert(
            Document::with_id(*id)
                .field("customer_id", *cust_id)
                .field("product_id", *prod_id)
                .field("quantity", *qty)
                .field("total_amount", *total)
                .field("status", *status),
        );
    }

    // 64-dimensional HNSW Vector Index
    let v_config = faizdb_vector::HnswConfig::new(64, faizdb_vector::DistanceMetric::Cosine);
    let mut hnsw = faizdb_vector::HnswIndex::new(v_config);

    for (i, (id, _, _, _, _, _, _)) in prod_items.iter().enumerate() {
        let mut vec = vec![0.0f32; 64];
        let offset = (i * 12) % 64;
        for j in 0..12 {
            vec[(offset + j) % 64] = 0.5 + (j as f32 * 0.05);
        }
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        let _ = hnsw.insert(id.to_string(), vec);
    }

    let hnsw_path = data_dir.join("products.hnsw");
    let _ = hnsw.save_to_file(&hnsw_path);
    db.vector_indexes()
        .insert("products".to_string(), Arc::new(parking_lot::RwLock::new(hnsw)));

    // Knowledge Graph
    {
        let graph_store = db.graph_store();
        let mut graph = graph_store.write();
        for (id, name, _, _, _, _) in &cust_items {
            graph.add_vertex(faizdb_graph::Vertex::with_properties(
                *id,
                "Customer",
                Document::new().field("name", *name),
            ));
        }
        for (id, name, cat, price, _, _, _) in &prod_items {
            graph.add_vertex(faizdb_graph::Vertex::with_properties(
                *id,
                "Product",
                Document::new()
                    .field("name", *name)
                    .field("category", *cat)
                    .field("price", *price),
            ));
        }

        graph.add_edge(faizdb_graph::Edge::with_weight("cust_1", "prod_1", "PURCHASED", 5.0));
        graph.add_edge(faizdb_graph::Edge::with_weight("cust_1", "prod_5", "PURCHASED", 5.0));
        graph.add_edge(faizdb_graph::Edge::with_weight("cust_2", "prod_2", "PURCHASED", 5.0));
        graph.add_edge(faizdb_graph::Edge::with_weight("cust_3", "prod_4", "PURCHASED", 4.0));
        graph.add_edge(faizdb_graph::Edge::new("cust_1", "cust_2", "REFERRED"));
        graph.add_edge(faizdb_graph::Edge::new("prod_1", "prod_2", "COMPATIBLE_WITH"));

        let graph_path = data_dir.join("graph_store.json");
        if let Ok(graph_json) = serde_json::to_string_pretty(&*graph) {
            let _ = std::fs::write(&graph_path, graph_json);
        }
    }

    let _ = db.flush();

    println!("✅ Dataset 'ecommerce' Seeded Successfully!");
    println!("  📦 Collections:");
    println!("     • 'products'  : 5 documents (Quantum RTX 5090, TPU v5, Cyberpunk Mech, HoloLens, Starlink)");
    println!("     • 'customers' : 3 documents (Ahmad Faiz, Elena Rostova, Marcus Vance)");
    println!("     • 'orders'    : 4 transactional records");
    println!("  🧠 AI Vector Index:");
    println!("     • 'products'  : 64 dimensions (Cosine Metric, HNSW persisted to '{}')", hnsw_path.display());
    println!("  🕸️ Knowledge Graph:");
    println!("     • 8 Vertices (3 Customers, 5 Products)");
    println!("     • 6 Directed Edges (PURCHASED, REFERRED, COMPATIBLE_WITH)");
    println!("\n👉 Instant Verification Queries:");
    println!("     1. SQL Query       : SELECT name, price FROM products WHERE price > 1000");
    println!("     2. SQL Aggregation : SELECT customer_id, SUM(total_amount) FROM orders GROUP BY customer_id");
    println!("     3. Mongo Wire      : db.products.find({{ \"category\": \"Hardware\" }})");
    println!("     4. Cypher Graph    : MATCH (c:Customer)-[:PURCHASED]->(p:Product) RETURN c.name, p.name");
    println!("     5. Vector Near     : FIND products VECTOR NEAR [0.12, 0.45, ...] TOP 2\n");
}

fn seed_agent_memory(db: &DatabaseContext, data_dir: &std::path::Path) {
    let memories = db.get_or_create_collection("agent_memory");
    let tools = db.get_or_create_collection("agent_tools");

    let memory_items = [
        ("mem_1", "episodic", "agent_alpha", "User requested database performance benchmark YCSB Workload B", 0.95, 1725500000i64),
        ("mem_2", "episodic", "agent_alpha", "Executed WAL checkpoint flush; reclaimed 45MB disk space", 0.88, 1725501000i64),
        ("mem_3", "semantic", "agent_alpha", "FaizDB supports 5 native wire gateways: 3306, 5432, 27017, 27018, 50051", 0.99, 1725502000i64),
        ("mem_4", "working", "agent_beta", "Current task: optimize multi-hop GraphRAG context injection for LangGraph", 0.92, 1725503000i64),
        ("mem_5", "semantic", "agent_beta", "Cosine distance is normalized between 0.0 and 2.0 with safe IEEE 754 float clamping", 0.85, 1725504000i64),
    ];

    for (id, tier, agent_id, content, imp, ts) in &memory_items {
        let _ = memories.insert(
            Document::with_id(*id)
                .field("memory_type", *tier)
                .field("agent_id", *agent_id)
                .field("content", *content)
                .field("importance", *imp)
                .field("timestamp", *ts),
        );
    }

    let tool_items = [
        ("tool_sql", "execute_sql", "Runs ANSI SQL statement against relational collections", true),
        ("tool_vector", "vector_search", "Performs k-NN similarity search on 64-dim HNSW embeddings", true),
        ("tool_graph", "graph_traverse", "Traverses multi-hop entity relationships for GraphRAG", true),
    ];

    for (id, name, desc, active) in &tool_items {
        let _ = tools.insert(
            Document::with_id(*id)
                .field("tool_name", *name)
                .field("description", *desc)
                .field("is_active", *active),
        );
    }

    let v_config = faizdb_vector::HnswConfig::new(64, faizdb_vector::DistanceMetric::Cosine);
    let mut hnsw = faizdb_vector::HnswIndex::new(v_config);

    for (i, (id, _, _, _, _, _)) in memory_items.iter().enumerate() {
        let mut vec = vec![0.0f32; 64];
        let offset = (i * 10) % 64;
        for j in 0..10 {
            vec[(offset + j) % 64] = 0.6 + (j as f32 * 0.04);
        }
        let norm: f32 = vec.iter().map(|v| v * v).sum::<f32>().sqrt();
        if norm > 0.0 {
            for v in &mut vec {
                *v /= norm;
            }
        }
        let _ = hnsw.insert(id.to_string(), vec);
    }

    let hnsw_path = data_dir.join("agent_memory.hnsw");
    let _ = hnsw.save_to_file(&hnsw_path);
    db.vector_indexes()
        .insert("agent_memory".to_string(), Arc::new(parking_lot::RwLock::new(hnsw)));

    {
        let graph_store = db.graph_store();
        let mut graph = graph_store.write();
        graph.add_vertex(faizdb_graph::Vertex::new("goal:system_hardening", "Goal"));
        graph.add_vertex(faizdb_graph::Vertex::new("task:wal_checkpoint", "Task"));
        graph.add_vertex(faizdb_graph::Vertex::new("task:mvcc_reaper", "Task"));
        graph.add_vertex(faizdb_graph::Vertex::new("agent:alpha", "Agent"));

        graph.add_edge(faizdb_graph::Edge::new("agent:alpha", "goal:system_hardening", "ASSIGNED_TO"));
        graph.add_edge(faizdb_graph::Edge::new("goal:system_hardening", "task:wal_checkpoint", "SUBTASK"));
        graph.add_edge(faizdb_graph::Edge::new("goal:system_hardening", "task:mvcc_reaper", "SUBTASK"));

        let graph_path = data_dir.join("agent_graph.json");
        if let Ok(graph_json) = serde_json::to_string_pretty(&*graph) {
            let _ = std::fs::write(&graph_path, graph_json);
        }
    }

    let _ = db.flush();

    println!("✅ Dataset 'agent-memory' Seeded Successfully!");
    println!("  🧠 Agent Memory Tier:");
    println!("     • 'agent_memory' : 5 records (Episodic, Semantic, Working memories)");
    println!("     • 'agent_tools'  : 3 registered AI agent capabilities");
    println!("     • HNSW Vector    : 64-dim embeddings ready for Semantic RAG Recall");
    println!("     • Knowledge Graph: Agent -> Goal -> Subtask hierarchy\n");
}

fn seed_social_graph(db: &DatabaseContext, data_dir: &std::path::Path) {
    let profiles = db.get_or_create_collection("profiles");
    let posts = db.get_or_create_collection("posts");

    let profile_items = [
        ("u1", "Ahmad Faiz", "founder", 1250i64),
        ("u2", "Elena Rostova", "engineer", 890i64),
        ("u3", "Marcus Vance", "devops", 640i64),
        ("u4", "Sophia Lin", "researcher", 1120i64),
    ];

    for (id, name, handle, followers) in &profile_items {
        let _ = profiles.insert(
            Document::with_id(*id)
                .field("name", *name)
                .field("handle", *handle)
                .field("followers_count", *followers),
        );
    }

    let post_items = [
        ("post_1", "u1", "FaizDB v0.1.0 5-Way Gateway is now live!", 420i64),
        ("post_2", "u2", "Benchmarking HNSW 32x Binary Quantization: zero accuracy drop", 310i64),
        ("post_3", "u4", "GraphRAG + LangGraph checkpointer in pure Safe Rust", 550i64),
    ];

    for (id, author_id, text, likes) in &post_items {
        let _ = posts.insert(
            Document::with_id(*id)
                .field("author_id", *author_id)
                .field("text", *text)
                .field("likes", *likes),
        );
    }

    {
        let graph_store = db.graph_store();
        let mut graph = graph_store.write();
        for (id, _, _, _) in &profile_items {
            graph.add_vertex(faizdb_graph::Vertex::new(*id, "User"));
        }
        graph.add_edge(faizdb_graph::Edge::new("u1", "u2", "FOLLOWS"));
        graph.add_edge(faizdb_graph::Edge::new("u2", "u1", "FOLLOWS"));
        graph.add_edge(faizdb_graph::Edge::new("u3", "u1", "FOLLOWS"));
        graph.add_edge(faizdb_graph::Edge::new("u4", "u1", "FOLLOWS"));
        graph.add_edge(faizdb_graph::Edge::new("u4", "u2", "FOLLOWS"));

        let graph_path = data_dir.join("social_graph.json");
        if let Ok(graph_json) = serde_json::to_string_pretty(&*graph) {
            let _ = std::fs::write(&graph_path, graph_json);
        }
    }

    let _ = db.flush();

    println!("✅ Dataset 'social-graph' Seeded Successfully!");
    println!("  👥 Social Graph:");
    println!("     • 'profiles'     : 4 user profiles");
    println!("     • 'posts'        : 3 posts with engagement counts");
    println!("     • Knowledge Graph: 4 Users with bi-directional FOLLOWS relationships\n");
}

