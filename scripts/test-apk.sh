#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "[1/5] RgBridge Java unit test..."
python scripts/dev/test_rg_bridge.py

echo "[2/5] Verify RgBridge DEX asset..."
python -c "
from pathlib import Path
dex = Path('crates/rg-ui/assets/rg_bridge.dex')
assert dex.exists(), 'Missing rg_bridge.dex'
data = dex.read_bytes()
assert len(data) > 4096, f'rg_bridge.dex unexpectedly small: {len(data)}'
assert data[:4] == b'dex\n', f'Bad DEX magic: {data[:8]!r}'
print(f'DEX OK: {dex} ({len(data)} bytes, version {data[4:7].decode(\"ascii\", \"replace\")})')
"

echo "[3/5] Check Android cross-compilation (Full + Lite)..."
if [ -z "${ANDROID_HOME:-}" ] && [ -d "/d/Android/sdk" ]; then
    export ANDROID_HOME="D:/Android/sdk"
fi
if [ -z "${ANDROID_NDK_HOME:-}" ] && [ -d "/d/Android/sdk/ndk/27.0.12077973" ]; then
    export ANDROID_NDK_HOME="D:/Android/sdk/ndk/27.0.12077973"
fi
if [ -n "${ANDROID_NDK_HOME:-}" ]; then
    NDK_WIN="$(cygpath -m "$ANDROID_NDK_HOME" 2>/dev/null || echo "$ANDROID_NDK_HOME")"
    if [ -d "${NDK_WIN}/toolchains/llvm/prebuilt/windows-x86_64/bin" ]; then
        NDK_BIN="${NDK_WIN}/toolchains/llvm/prebuilt/windows-x86_64/bin"
        export CC_aarch64_linux_android="${CC_aarch64_linux_android:-${NDK_BIN}/aarch64-linux-android24-clang.cmd}"
        export CXX_aarch64_linux_android="${CXX_aarch64_linux_android:-${NDK_BIN}/aarch64-linux-android24-clang++.cmd}"
        export AR_aarch64_linux_android="${AR_aarch64_linux_android:-${NDK_BIN}/llvm-ar.exe}"
        export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER:-${NDK_BIN}/aarch64-linux-android24-clang.cmd}"
    elif [ -d "${NDK_WIN}/toolchains/llvm/prebuilt/linux-x86_64/bin" ]; then
        NDK_BIN="${NDK_WIN}/toolchains/llvm/prebuilt/linux-x86_64/bin"
        export CC_aarch64_linux_android="${CC_aarch64_linux_android:-${NDK_BIN}/aarch64-linux-android24-clang}"
        export CXX_aarch64_linux_android="${CXX_aarch64_linux_android:-${NDK_BIN}/aarch64-linux-android24-clang++}"
        export AR_aarch64_linux_android="${AR_aarch64_linux_android:-${NDK_BIN}/llvm-ar}"
        export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="${CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER:-${NDK_BIN}/aarch64-linux-android24-clang}"
    fi
fi
cargo check -p rg-android --target aarch64-linux-android
cargo check -p rg-android --target aarch64-linux-android --no-default-features

echo "[4/5] Static APK audit (if built APK is present)..."
python -c "
import zipfile
from pathlib import Path

apks = list(Path('dist').glob('*.apk')) + list(Path('target').glob('dx/*/*/android/app/**/*.apk'))
if not apks:
    print('No prebuilt APK found in dist/ or target/ — skipping zip/signature check')
else:
    for apk in apks:
        with zipfile.ZipFile(apk) as zf:
            names = set(zf.namelist())
            assert 'AndroidManifest.xml' in names, f'{apk}: missing AndroidManifest.xml'
            assert any(n.endswith('.dex') for n in names), f'{apk}: missing classes.dex'
            assert any('lib/arm64-v8a/' in n and n.endswith('.so') for n in names), f'{apk}: missing arm64-v8a .so'
            print(f'APK OK: {apk} ({apk.stat().st_size // 1024} KiB, {len(names)} entries)')
"

echo "[5/5] Workspace test suite..."
cargo test --workspace

echo "ALL APK/BRIDGE & WORKSPACE GATES PASSED!"
