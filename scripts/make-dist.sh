#!/usr/bin/env bash
# Сборка релизного Rustgramm.exe в dist/ (без консольного окна).
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release -p rg-desktop
mkdir -p dist
cp target/release/rg-desktop.exe dist/Rustgramm.exe
ls -la dist/Rustgramm.exe
