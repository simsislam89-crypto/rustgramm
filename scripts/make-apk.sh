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
KS_PASS="$(cat "$PASS_FILE")"
export CARGO_APK_RELEASE_KEYSTORE_PASSWORD="$KS_PASS"

BUILD_TOOLS="$(ls -d "${ANDROID_HOME}"/build-tools/* | sort -V | tail -n 1)"
ZIPALIGN="${BUILD_TOOLS}/zipalign"
APKSIGNER="${BUILD_TOOLS}/apksigner"

align_and_sign() {
  local src="$1"
  local dst="$2"
  local tmp="${dst}.aligned.apk"
  "$ZIPALIGN" -f -p 4 "$src" "$tmp"
  "$APKSIGNER" sign \
    --ks "$KS" \
    --ks-pass "pass:${KS_PASS}" \
    --ks-key-alias rustgramm \
    --v1-signing-enabled true \
    --v2-signing-enabled true \
    --v3-signing-enabled true \
    --out "$dst" \
    "$tmp"
  rm -f "$tmp"
  "$APKSIGNER" verify --verbose "$dst"
}

mkdir -p dist

# 1. Полная версия для телефона (с плагинами)
rm -f target/release/apk/*.apk
cargo apk build -p rg-android --release --features plugins
APK_FULL="$(ls target/release/apk/*.apk | head -n 1)"
align_and_sign "$APK_FULL" dist/Rustgramm-Android-Full.apk
cp -f dist/Rustgramm-Android-Full.apk dist/Rustgramm.apk

# 2. Облегчённая версия для телефона (без плагинов)
rm -f target/release/apk/*.apk
cargo apk build -p rg-android --release --no-default-features
APK_LITE="$(ls target/release/apk/*.apk | head -n 1)"
align_and_sign "$APK_LITE" dist/Rustgramm-Android-Lite.apk

ls -la dist/Rustgramm-Android-Full.apk dist/Rustgramm-Android-Lite.apk dist/Rustgramm.apk
