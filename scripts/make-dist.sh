#!/usr/bin/env bash
# Сборка релизного Rustgramm.exe в dist/ (без консольного окна).
# Ключи приложения (api_id, api_hash) вшиваются при сборке из .rg/api.keys или переменных
# RG_BUILTIN_API_ID / RG_BUILTIN_API_HASH. В git они не попадают; пользователю ключи вводить не нужно.
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${RG_BUILTIN_API_ID:-}" ] && [ -f .rg/api.keys ]; then
  RG_BUILTIN_API_ID="$(sed -n 1p .rg/api.keys | tr -d '\r')"
  RG_BUILTIN_API_HASH="$(sed -n 2p .rg/api.keys | tr -d '\r')"
fi
export RG_BUILTIN_API_ID RG_BUILTIN_API_HASH
cargo build --release -p rg-desktop
mkdir -p dist
cp target/release/rg-desktop.exe dist/Rustgramm.exe 2>/dev/null || { cp target/release/rg-desktop.exe dist/Rustgramm-new.exe; echo "Rustgramm.exe занят, сохранено как Rustgramm-new.exe"; }
ls -la dist/Rustgramm.exe
