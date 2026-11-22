#!/usr/bin/env bash
# Сборка двух версий Rustgramm для Android (arm64) в dist/:
#   1) dist/Rustgramm-Android-Full.apk — полная версия (с системой плагинов exteraGram/AyuGram)
#   2) dist/Rustgramm-Android-Lite.apk — облегчённая версия (без плагинов)
#   + копия dist/Rustgramm.apk (указывает на полную версию).
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

mkdir -p dist

# 1. Полная версия для телефона (с плагинами)
cargo apk build -p rg-android --release --features plugins
APK_FULL="$(ls target/release/apk/*.apk | head -n 1)"
cp -f "$APK_FULL" dist/Rustgramm-Android-Full.apk
cp -f "$APK_FULL" dist/Rustgramm.apk

# 2. Облегчённая версия для телефона (без плагинов)
cargo apk build -p rg-android --release --no-default-features
APK_LITE="$(ls target/release/apk/*.apk | head -n 1)"
cp -f "$APK_LITE" dist/Rustgramm-Android-Lite.apk

ls -la dist/Rustgramm-Android-Full.apk dist/Rustgramm-Android-Lite.apk dist/Rustgramm.apk
