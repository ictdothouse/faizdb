# 📦 FaizDB — Global Distribution & Packaging Architecture

This document outlines the binary distribution strategy, package managers, cross-compilation matrix, and embedded native bindings for **FaizDB**.

---

## 🎯 Target Platform Matrix

FaizDB is compiled into ultra-compact, zero-dependency native binaries:

| Platform | Target Triple | Distribution Artifact | Target Devices |
|:---|:---|:---|:---|
| **Linux (x86_64)** | `x86_64-unknown-linux-gnu` / `musl` | `faizdb-linux-x86_64.tar.gz` | Cloud VMs (AWS EC2, GCP Compute, Azure, DigitalOcean), On-Premises Servers |
| **Linux (ARM64)** | `aarch64-unknown-linux-gnu` / `musl` | `faizdb-linux-arm64.tar.gz` | AWS Graviton, Raspberry Pi 4/5, Oracle Ampere, Edge IoT |
| **macOS (Apple Silicon)** | `aarch64-apple-darwin` | `faizdb-macos-arm64.tar.gz` | MacBook Pro / Air (M1, M2, M3, M4), Mac Studio, Mac Mini |
| **macOS (Intel)** | `x86_64-apple-darwin` | `faizdb-macos-x86_64.tar.gz` | Intel-based Mac hardware |
| **Windows (x64)** | `x86_64-pc-windows-msvc` | `faizdb-windows-x64.zip` | Windows 10, Windows 11, Windows Server 2019/2022 |
| **Container / OCI** | `linux/amd64`, `linux/arm64` | `ictdothouse/faizdb:latest` | Kubernetes (K8s), Docker Swarm, Nomad, AWS ECS |

---

## 🛠️ Automated Cross-Compilation Commands

To produce standalone release binaries for all operating systems from a single build machine:

```bash
# Install cross-compilation engine
cargo install cross --git https://github.com/cross-rs/cross

# 1. Linux x86_64 (Static MUSL - runs on any Linux distribution)
cross build --target x86_64-unknown-linux-musl --release

# 2. Linux ARM64 (AWS Graviton & IoT)
cross build --target aarch64-unknown-linux-musl --release

# 3. Windows 64-bit Executable (.exe)
cross build --target x86_64-pc-windows-gnu --release

# 4. macOS Universal Binary
cargo build --target aarch64-apple-darwin --release
cargo build --target x86_64-apple-darwin --release
lipo -create -output faizdb-macos-universal \
    target/aarch64-apple-darwin/release/faizdb \
    target/x86_64-apple-darwin/release/faizdb
```

---

## 📱 Mobile & Embedded Native Library Distribution (iOS & Android)

`faizdb-core` can be compiled into embedded shared libraries (`.so`, `.dylib`, `.dll`, `.framework`) to run locally inside mobile apps without requiring an external server daemon:

```bash
# Android NDK (ARM64 .so)
cargo build --target aarch64-linux-android --release -p faizdb-core

# iOS Static Framework (.a / .xcframework)
cargo build --target aarch64-apple-ios --release -p faizdb-core
```

---

## 🌐 Package Manager Integrations

### 1. Homebrew (macOS & Linux)
```bash
# Formula: faizdb.rb
brew tap ictdothouse/faizdb
brew install faizdb
```

### 2. Windows Package Manager (`winget`)
```powershell
winget install FaizDB.FaizDB
```

### 3. Arch Linux (`AUR`)
```bash
yay -S faizdb-bin
```

---

## 🚢 Continuous Delivery via GitHub Actions

Every tagged release (`git tag v0.1.0 && git push origin v0.1.0`) triggers `.github/workflows/release.yml`, building native artifacts with SHA-256 checksums and publishing them directly to **GitHub Releases**.
