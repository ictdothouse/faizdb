# 🚀 FaizDB — Universal Multi-Platform Installation Guide

This guide covers installing and deploying **FaizDB** across Linux servers, macOS workstations, Windows PCs, and containerized Docker environments.

---

## ⚡ Quick 1-Line Installers

### 1. Linux & macOS (Apple Silicon & Intel)
Open your terminal and run:

```bash
curl -fsSL https://raw.githubusercontent.com/ictdothouse/faizdb/main/scripts/install.sh | bash
```

* **What it does:**
  - Auto-detects OS (`Linux` / `macOS`) and CPU architecture (`x86_64` / `aarch64` / `arm64`).
  - Installs the optimized `faizdb` binary to `/usr/local/bin` (or `~/.local/bin`).
  - Configures shell `PATH` in `.bashrc` / `.zshrc`.
  - On Linux servers (with root/sudo), automatically sets up and starts the **systemd 24/7 background service**.

---

### 2. Windows 10 / 11 & Windows Server
Open **PowerShell** (Run as Administrator for system-wide access) and execute:

```powershell
iwr -useb https://raw.githubusercontent.com/ictdothouse/faizdb/main/scripts/install.ps1 | iex
```

* **What it does:**
  - Installs `faizdb.exe` to `$HOME\.faizdb\bin`.
  - Adds FaizDB permanently to your User `PATH` environment variable.

---

### 3. Docker & Containerized Environments

Deploy FaizDB Server + Web Management Studio in one command:

```bash
# Clone repository
git clone https://github.com/ictdothouse/faizdb.git
cd faizdb

# Launch complete stack via Docker Compose
docker compose up -d
```

* **Exposed Endpoints:**
  - **MongoDB Wire Protocol**: `mongodb://localhost:27017`
  - **PostgreSQL Wire Protocol**: `psql -h localhost -p 5432 -U postgres -d faizdb`
  - **gRPC / Protocol Buffers**: `localhost:50051`
  - **HTTP REST & WebSocket API**: `http://localhost:27018`
  - **FaizDB Web Studio**: `http://localhost:27020`

---

## 🛠️ Linux Production Daemon Setup (`systemd`)

For production Linux servers (Ubuntu, Debian, RHEL, CentOS), FaizDB can be run as a managed system daemon:

### Create Systemd Service File
```bash
sudo tee /etc/systemd/system/faizdb.service <<EOF
[Unit]
Description=FaizDB AI-Native NoSQL Database Server
After=network.target

[Service]
Type=simple
User=root
ExecStart=/usr/local/bin/faizdb serve --wire-port 27017 --http-port 27018 --host 0.0.0.0
Restart=always
RestartSec=3
LimitNOFILE=65535

[Install]
WantedBy=multi-user.target
EOF
```

### Manage FaizDB Service
```bash
# Reload daemon definitions
sudo systemctl daemon-reload

# Start FaizDB
sudo systemctl start faizdb

# Enable auto-start on server reboot
sudo systemctl enable faizdb

# Check live server status
sudo systemctl status faizdb

# View real-time logs
sudo journalctl -u faizdb -f
```

---

## 🏗️ Build from Source (Any OS with Rust)

If you prefer building from source:

```bash
# 1. Install Rust toolchain (if not already installed)
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh

# 2. Clone repository
git clone https://github.com/ictdothouse/faizdb.git
cd faizdb

# 3. Build optimized release binary
cargo build --release

# 4. Install globally
sudo cp target/release/faizdb /usr/local/bin/
```

---

## 🌐 Verify Installation

After installation, verify that FaizDB is ready:

```bash
# Check version and engine information
faizdb --version
faizdb info

# Start interactive multi-dialect shell
faizdb shell

# Test HTTP Health endpoint
curl http://localhost:27018/v1/health
```

Output:
```json
{
  "status": "healthy",
  "engine": "FaizDB",
  "version": "0.1.0"
}
```

---

---

## 🪶 Embedded & IoT / Edge Mode (Zero-Dependency SQLite-Style In-Process DB)

For lightweight, embedded, or edge environments (Raspberry Pi, IoT gateways, smart TVs, desktop tools, Android/iOS apps) where you do **not** want a separate background server daemon or open network ports:

### 1. Embedded in Rust Applications (`Cargo.toml`)
Add `faizdb-core` directly to your Rust project:

```toml
[dependencies]
faizdb-core = { git = "https://github.com/ictdothouse/faizdb.git" }
```

```rust
use faizdb_core::storage::engine::{StorageConfig, StorageEngine};
use std::path::PathBuf;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = StorageEngine::open(StorageConfig {
        data_dir: PathBuf::from("./iot_data"),
        memtable_size: 4 * 1024 * 1024, // 4MB RAM footprint for constrained devices
        sync_writes: false,
        enable_wal: true,
    })?;

    // Direct in-process reads/writes (Sub-microsecond latency)
    db.put(b"sensor:01", b"{\"temp\": 28.5}")?;
    let val = db.get(b"sensor:01")?;
    println!("Value: {:?}", val);
    Ok(())
}
```

### 2. Standalone Static Binary for IoT (Zero Glibc Dependencies)
Download or cross-compile standalone static MUSL binaries for ARM / Raspberry Pi:

```bash
# Cross-compile static MUSL binary for ARM64 (Raspberry Pi 4/5, Orange Pi):
cross build --target aarch64-unknown-linux-musl --release -p faizdb-cli

# Cross-compile for ARMv7 (Raspberry Pi 2/3, 32-bit IoT boards):
cross build --target armv7-unknown-linux-musleabihf --release -p faizdb-cli
```

### 3. Mobile Library Compilation (Android NDK & iOS XCFramework)
```bash
# Android ARM64 (.so shared object):
cargo build --target aarch64-linux-android --release -p faizdb-core

# iOS Static Framework (.xcframework):
cargo build --target aarch64-apple-ios --release -p faizdb-core
```

---

## 🌐 Official Download Channels

| Artifact | Source / Method | Target Use Case |
|:---|:---|:---|
| **Server Binaries (.tar.gz / .zip)** | [**GitHub Releases**](https://github.com/ictdothouse/faizdb/releases) | Linux, macOS, Windows Production Servers |
| **Rust In-Process Crate** | `cargo add faizdb-core` / GitHub | Embedded Desktop, CLI, & In-Process Applications |
| **Python SDK** | `pip install .` / `pyproject.toml` | Python AI / Microservice Apps |
| **Node.js / TypeScript SDK** | `npm install ./bindings/node` | TypeScript backend microservices |
| **Docker Image** | `docker pull ictdothouse/faizdb:latest` | Kubernetes, Docker Compose, Cloud Clusters |
| **1-Line Shell Script** | `curl -fsSL ... | bash` | Fast automated terminal setup |

---

## 🔒 4-Way Multi-Protocol Port Configuration Summary

| Port | Protocol | Usage |
|:---:|:---:|:---|
| **27017** | **🍃 MongoDB Wire Protocol** | Drop-in connection for PyMongo, Mongoose, Prisma, PHP |
| **5432** | **🐘 PostgreSQL Wire Protocol** | Drop-in connection for `psql`, DBeaver, TablePlus, Grafana, SQL ORMs |
| **50051** | **⚡ gRPC & Protocol Buffers** | Ultra low-latency microservices and high-throughput Vector ANN streaming |
| **27018** | **🌐 HTTP REST & WebSockets** | FaizQL queries, Change Streams, Health check, Backups, Geo-Replication |
| **27020** | **🎛️ HTTP (FaizDB Studio)** | Visual Management Dashboard & Query Explorer |
