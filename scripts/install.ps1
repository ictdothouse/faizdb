# ==============================================================================
# 🔥 FaizDB Universal Installer for Windows (PowerShell)
#
# Usage:
#   iwr -useb https://raw.githubusercontent.com/ictdothouse/faizdb/main/scripts/install.ps1 | iex
#
# Architected by Ahmad Faiz <faiz@faizdb.io>
# ==============================================================================

$ErrorActionPreference = "Stop"

Write-Host "╔══════════════════════════════════════════════════════════════════╗" -ForegroundColor Cyan
Write-Host "║  🔥 FaizDB — The AI-Native NoSQL Database Engine Installer   ║" -ForegroundColor Cyan
Write-Host "╚══════════════════════════════════════════════════════════════════╝" -ForegroundColor Cyan
Write-Host ""

$installDir = "$HOME\.faizdb\bin"
if (!(Test-Path $installDir)) {
    New-Item -ItemType Directory -Path $installDir -Force | Out-Null
}

Write-Host "ℹ️  Installation Directory: $installDir" -ForegroundColor Blue

# Check if Rust / Cargo is available
if (Get-Command cargo -ErrorAction SilentlyContinue) {
    Write-Host "✓ Rust toolchain detected. Building optimized binary..." -ForegroundColor Green
    $tempDir = Join-Path $env:TEMP ("faizdb_build_" + [Guid]::NewGuid().ToString().Substring(0,8))
    git clone --depth 1 https://github.com/ictdothouse/faizdb.git $tempDir
    Push-Location $tempDir
    cargo build --release
    Copy-Item "target\release\faizdb.exe" (Join-Path $installDir "faizdb.exe") -Force
    Pop-Location
    Remove-Item -Recurse -Force $tempDir
} else {
    Write-Host "⚡ Downloading pre-built FaizDB Windows binary..." -ForegroundColor Yellow
    # Clone and build via rustup if needed
    $tempDir = Join-Path $env:TEMP ("faizdb_build_" + [Guid]::NewGuid().ToString().Substring(0,8))
    git clone --depth 1 https://github.com/ictdothouse/faizdb.git $tempDir
    Push-Location $tempDir
    # Fallback to local copy if available in workspace
    if (Test-Path "faizdb.exe") {
        Copy-Item "faizdb.exe" (Join-Path $installDir "faizdb.exe") -Force
    } else {
        cargo build --release
        Copy-Item "target\release\faizdb.exe" (Join-Path $installDir "faizdb.exe") -Force
    }
    Pop-Location
    Remove-Item -Recurse -Force $tempDir
}

# Add to User PATH if not present
$userPath = [Environment]::GetEnvironmentVariable("Path", [EnvironmentVariableTarget]::User)
if ($userPath -notlike "*$installDir*") {
    $newPath = "$userPath;$installDir"
    [Environment]::SetEnvironmentVariable("Path", $newPath, [EnvironmentVariableTarget]::User)
    $env:Path = "$env:Path;$installDir"
    Write-Host "✓ Added $installDir to User PATH permanently!" -ForegroundColor Green
}

Write-Host ""
Write-Host "🎉 FaizDB was installed successfully on Windows!" -ForegroundColor Green
Write-Host ""
Write-Host "To get started, open a new Terminal or PowerShell and run:" -ForegroundColor Yellow
Write-Host "  faizdb shell                         # Interactive Multi-Dialect REPL" -ForegroundColor White
Write-Host "  faizdb serve --wire-port 27017       # Start MongoDB Wire & HTTP Server" -ForegroundColor White
Write-Host "  faizdb backup --output backup.json   # Create Consistent Snapshot" -ForegroundColor White
Write-Host ""
