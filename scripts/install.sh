#!/usr/bin/env bash
# ==============================================================================
# 🔥 FaizDB Universal Installer for Linux & macOS (Darwin)
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/ictdothouse/faizdb/main/scripts/install.sh | bash
#
# Architected by Ahmad Faiz <faiz@faizdb.io>
# ==============================================================================

set -e

RESET="\033[0m"
BOLD="\033[1m"
GREEN="\033[32m"
BLUE="\033[34m"
CYAN="\033[36m"
YELLOW="\033[33m"
RED="\033[31m"

echo -e "${CYAN}${BOLD}"
echo "╔══════════════════════════════════════════════════════════════════╗"
echo "║  🔥 FaizDB — The AI-Native NoSQL Database Engine Installer   ║"
echo "╚══════════════════════════════════════════════════════════════════╝"
echo -e "${RESET}"

# 1. Detect OS & Architecture
OS="$(uname -s | tr '[:upper:]' '[:lower:]')"
ARCH="$(uname -m)"

case "$ARCH" in
    x86_64|amd64)
        TARGET_ARCH="x86_64"
        ;;
    aarch64|arm64)
        TARGET_ARCH="aarch64"
        ;;
    *)
        echo -e "${RED}❌ Unsupported architecture: $ARCH${RESET}"
        exit 1
        ;;
esac

case "$OS" in
    linux)
        TARGET_OS="linux"
        ;;
    darwin)
        TARGET_OS="apple-darwin"
        ;;
    *)
        echo -e "${RED}❌ Unsupported operating system: $OS${RESET}"
        echo "For Windows, please run: iwr -useb https://raw.githubusercontent.com/ictdothouse/faizdb/main/scripts/install.ps1 | iex"
        exit 1
        ;;
esac

echo -e "${BLUE}ℹ️  Detected System:${RESET} ${BOLD}${TARGET_OS} (${TARGET_ARCH})${RESET}"

# 2. Determine Installation Directory
if [ "$EUID" -eq 0 ]; then
    INSTALL_DIR="/usr/local/bin"
else
    INSTALL_DIR="$HOME/.local/bin"
    mkdir -p "$INSTALL_DIR"
fi

echo -e "${BLUE}ℹ️  Target Directory:${RESET} ${BOLD}${INSTALL_DIR}${RESET}"

# 3. Build or Download FaizDB Binary
echo -e "${YELLOW}⚡ Preparing FaizDB binary...${RESET}"

if command -v cargo >/dev/null 2>&1; then
    echo -e "${GREEN}✓ Found Rust toolchain. Compiling optimized release binary...${RESET}"
    TMP_DIR=$(mktemp -d)
    git clone --depth 1 https://github.com/ictdothouse/faizdb.git "$TMP_DIR/faizdb"
    cd "$TMP_DIR/faizdb"
    cargo build --release
    cp target/release/faizdb "$INSTALL_DIR/faizdb"
    chmod +x "$INSTALL_DIR/faizdb"
    rm -rf "$TMP_DIR"
else
    echo -e "${YELLOW}Rust not detected. Installing via source bootstrap...${RESET}"
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source "$HOME/.cargo/env"
    TMP_DIR=$(mktemp -d)
    git clone --depth 1 https://github.com/ictdothouse/faizdb.git "$TMP_DIR/faizdb"
    cd "$TMP_DIR/faizdb"
    cargo build --release
    cp target/release/faizdb "$INSTALL_DIR/faizdb"
    chmod +x "$INSTALL_DIR/faizdb"
    rm -rf "$TMP_DIR"
fi

# 4. PATH Configuration check
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    SHELL_PROFILE=""
    if [ -n "$ZSH_VERSION" ] || [ "$SHELL" = "/bin/zsh" ]; then
        SHELL_PROFILE="$HOME/.zshrc"
    elif [ -n "$BASH_VERSION" ] || [ "$SHELL" = "/bin/bash" ]; then
        SHELL_PROFILE="$HOME/.bashrc"
    else
        SHELL_PROFILE="$HOME/.profile"
    fi

    if [ -f "$SHELL_PROFILE" ]; then
        echo "export PATH=\"\$PATH:$INSTALL_DIR\"" >> "$SHELL_PROFILE"
        echo -e "${GREEN}✓ Added $INSTALL_DIR to PATH in $SHELL_PROFILE${RESET}"
    fi
fi

# 5. Linux systemd Service Setup (Optional for root/sudo)
if [ "$TARGET_OS" = "linux" ] && [ "$EUID" -eq 0 ] && command -v systemctl >/dev/null 2>&1; then
    echo -e "${YELLOW}⚙️  Configuring systemd background daemon service...${RESET}"
    cat <<EOF > /etc/systemd/system/faizdb.service
[Unit]
Description=FaizDB AI-Native NoSQL Database Daemon
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
    systemctl daemon-reload
    systemctl enable faizdb
    systemctl restart faizdb
    echo -e "${GREEN}✓ Systemd service 'faizdb.service' enabled and running on ports 27017 & 27018!${RESET}"
fi

echo ""
echo -e "${GREEN}${BOLD}🎉 FaizDB was installed successfully!${RESET}"
echo -e "${BOLD}Version:${RESET} $( "$INSTALL_DIR/faizdb" --version 2>/dev/null || echo "v0.1.0" )"
echo ""
echo -e "${CYAN}To get started immediately:${RESET}"
echo -e "  ${BOLD}faizdb shell${RESET}                         # Interactive Multi-Dialect REPL"
echo -e "  ${BOLD}faizdb serve --wire-port 27017${RESET}       # Launch Dual-Protocol Daemon"
echo -e "  ${BOLD}faizdb backup --output backup.json${RESET}   # Point-In-Time Backup"
echo ""
