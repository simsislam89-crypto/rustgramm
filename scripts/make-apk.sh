#!/usr/bin/env bash
# Сборка Rustgramm.apk (Android, arm64) в dist/.
# Нужно заранее: Android SDK (platform 34, build-tools 34) и NDK, переменные ANDROID_HOME и
# ANDROID_NDK_ROOT, Rust-цель aarch64-linux-android, `cargo install cargo-apk`, Java (keytool).
# Ключи приложения (api_id, api_hash) вшиваются при сборке из .rg/api.keys, в git они не попадают.
set -euo pipefail
cd "$(dirname "$0")/.."

: "${ANDROID_HOME:?задайте ANDROID_HOME (каталог Android SDK)}"
: "${ANDROID_NDK_ROOT:?задайте ANDROID_NDK_ROOT (каталог NDK)}"

if [ -z "${RG_BUILTIN_API_ID:-}" ] && [ -f .rg/api.keys ]; then
  RG_BUILTIN_API_ID="$(sed -n 1p .rg/api.keys | tr -d '\r')"
  RG_BUILTIN_API_HASH="$(sed -n 2p .rg/api.keys | tr -d '\r')"
fi
export RG_BUILTIN_API_ID RG_BUILTIN_API_HASH

# Ключ подписи создаётся один раз и хранится только у вас (.rg/ в .gitignore).
KS=".rg/rustgramm.jks"
PASS_FILE=".rg/keystore.pass"
if [ ! -f "$KS" ]; then
  mkdir -p .rg
  head -c 24 /dev/urandom | base64 | tr -d '/+=\n' > "$PASS_FILE"
  keytool -genkeypair -keystore "$KS" -storepass "$(cat "$PASS_FILE")" -keypass "$(cat "$PASS_FILE")" \
    -alias rustgramm -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Rustgramm"
fi
export CARGO_APK_RELEASE_KEYSTORE="$PWD/$KS"
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD="$(cat "$PASS_FILE")"

cargo apk build -p rg-android --release
mkdir -p dist
APK="$(ls target/release/apk/*.apk | head -n 1)"
cp "$APK" dist/Rustgramm.apk
ls -la dist/Rustgramm.apk
