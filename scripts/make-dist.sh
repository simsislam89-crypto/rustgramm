#!/usr/bin/env bash
# Сборка релизного Rustgramm.exe в dist/ (один файл dist/Rustgramm.exe, без создания копий -new).
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
taskkill //F //IM Rustgramm.exe >/dev/null 2>&1 || true
rm -f dist/Rustgramm-new.exe
for _ in 1 2 3 4 5; do
  cp -f target/release/rg-desktop.exe dist/Rustgramm.exe 2>/dev/null && break
  sleep 0.2
done
ls -la dist/Rustgramm.exe
