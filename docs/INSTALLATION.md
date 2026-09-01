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

## 🔒 Port Configuration Summary

| Port | Protocol | Usage |
|:---:|:---:|:---|
| **27017** | **MongoDB Wire Protocol** | Drop-in connection for PyMongo, Mongoose, Prisma, PHP |
| **27018** | **HTTP REST & WebSockets** | FaizQL queries, Change Streams, Health check, Backups |
| **27020** | **HTTP (FaizDB Studio)** | Visual Management Dashboard & Query Explorer |
