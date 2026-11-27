#!/usr/bin/env python3
"""
Автотест Java-моста RgBridge (`scripts/dev/build_android_bridge.py`).
Извлекает `JAVA_SRC`, компилирует с `RgBridgeTest.java` на JDK 17 против android.jar
и проверяет атомарный INPUT_EVENT, null-безопасность lifecycle-методов, getSafeInsetsPx
и многопоточную гонку INPUT_EVENT без подключённого устройства.
"""
import os
import subprocess
import sys
import tempfile
from pathlib import Path

from build_android_bridge import find_tool, JAVA_SRC

TEST_JAVA_SRC = r"""package app.rustgramm;

public final class RgBridgeTest {
    private static void check(boolean cond, String msg) {
        if (!cond) {
            throw new AssertionError("FAIL: " + msg);
        }
    }

    public static void main(String[] args) throws Exception {
        // 1. Чистое начальное состояние
        check(RgBridge.pollInputEvent() == null, "initial pollInputEvent must be null");
        check(RgBridge.pollInputTag() == null, "initial pollInputTag must be null");
        check(RgBridge.pollInputResult() == null, "initial pollInputResult must be null");
        check(RgBridge.pollPickedFile() == null, "initial pollPickedFile must be null");
        check(RgBridge.pollPendingLink() == null, "initial pollPendingLink must be null");

        // 2. Атомарная доставка OK:tag\0val (даже при пустом val)
        RgBridge.INPUT_EVENT.set("OK:login_code\012345");
        check("OK:login_code\012345".equals(RgBridge.pollInputEvent()), "first pollInputEvent OK mismatch");
        check(RgBridge.pollInputEvent() == null, "second pollInputEvent must drain to null");

        RgBridge.INPUT_EVENT.set("OK:login_phone\0");
        String emptyOk = RgBridge.pollInputEvent();
        check(emptyOk != null && emptyOk.startsWith("OK:login_phone\0"), "empty val OK must preserve prefix");

        // 3. Сентинел отмены CAN:tag не путается с OK:
        RgBridge.INPUT_EVENT.set("CAN:login_code");
        String can = RgBridge.pollInputEvent();
        check("CAN:login_code".equals(can), "cancel sentinel mismatch");
        check(RgBridge.pollInputEvent() == null, "cancel sentinel must drain");

        // 4. Null-безопасность и проверка схем URL для Activity-методов (headless CI)
        check(!RgBridge.isWebViewOpen(), "isWebViewOpen must default to false");
        RgBridge.showSoftInput(null);
        RgBridge.promptTextInput(null, "login_code", "Title", "Hint", "123", 2);
        RgBridge.pickFile(null, "*/*", "/tmp");
        RgBridge.openExternalUrl(null, "https://telegram.org");
        RgBridge.openExternalUrl(null, "file:///etc/passwd");
        RgBridge.openWebView(null, "https://telegram.org", "Preview", true);
        RgBridge.openWebView(null, "file:///etc/passwd", "BadScheme", true);
        RgBridge.closeWebView(null);
        RgBridge.setupGlassWindow(null);
        String ins = RgBridge.getSafeInsetsPx(null);
        check("0,0,0,0".equals(ins), "getSafeInsetsPx(null) must be 0,0,0,0, got: " + ins);

        // 4b. Проверка наличия ключевых JNI-сигнатур RgBridge через reflection
        check(RgBridge.class.getMethod("openExternalUrl", android.app.Activity.class, String.class) != null, "openExternalUrl missing");
        check(RgBridge.class.getMethod("isWebViewOpen") != null, "isWebViewOpen missing");
        check(RgBridge.class.getMethod("setupGlassWindow", android.app.Activity.class) != null, "setupGlassWindow missing");
        check(RgBridge.class.getMethod("getSafeInsetsPx", android.app.Activity.class) != null, "getSafeInsetsPx missing");
        check(RgBridge.class.getMethod("pollInputEvent") != null, "pollInputEvent missing");

        // 5. Конкурентная запись/чтение INPUT_EVENT без разрыва tag и value
        final int threads = 4;
        final int perThread = 500;
        Thread[] workers = new Thread[threads];
        final Throwable[] errs = new Throwable[1];
        for (int t = 0; t < threads; t++) {
            final int tid = t;
            workers[t] = new Thread(new Runnable() {
                @Override
                public void run() {
                    try {
                        for (int i = 0; i < perThread; i++) {
                            String tag = "t" + tid;
                            String val = "v" + i;
                            RgBridge.INPUT_EVENT.set("OK:" + tag + "\0" + val);
                            String got = RgBridge.pollInputEvent();
                            if (got != null) {
                                int nul = got.indexOf('\0');
                                check(got.startsWith("OK:") && nul > 3, "torn atomic event: " + got);
                            }
                        }
                    } catch (Throwable ex) {
                        errs[0] = ex;
                    }
                }
            });
            workers[t].start();
        }
        for (Thread w : workers) {
            w.join();
        }
        if (errs[0] != null) {
            throw new AssertionError("Concurrent test error", errs[0]);
        }
        System.out.println("RgBridgeTest: ALL CHECKS PASSED");
    }
}
"""


def find_java(javac_path: str) -> str:
    p = Path(javac_path)
    cand = p.with_name("java.exe" if os.name == "nt" else "java")
    if cand.exists():
        return str(cand)
    return find_tool(["java", "java.exe"]) or "java"


def main():
    sdk = Path(os.environ.get("ANDROID_HOME", r"D:\Android\sdk"))
    if not sdk.exists():
        print("Android SDK not found — skipping RgBridge Java test", file=sys.stderr)
        sys.exit(1)
    javac = find_tool([
        r"D:\Android\jdk17\jdk-17.0.21+9\bin\javac.exe",
        r"C:\Program Files\Java\jdk-17\bin\javac.exe",
        "javac",
    ])
    android_jars = sorted((sdk / "platforms").glob("android-*/android.jar"))
    if not javac or not android_jars:
        print("Missing javac / android.jar", file=sys.stderr)
        sys.exit(1)
    android_jar = str(android_jars[-1])
    java = find_java(javac)

    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        pkg_dir = td_path / "app" / "rustgramm"
        pkg_dir.mkdir(parents=True, exist_ok=True)
        rg_bridge_java = pkg_dir / "RgBridge.java"
        rg_test_java = pkg_dir / "RgBridgeTest.java"
        rg_bridge_java.write_text(JAVA_SRC, encoding="utf-8")
        rg_test_java.write_text(TEST_JAVA_SRC, encoding="utf-8")

        classes_dir = td_path / "classes"
        classes_dir.mkdir()
        subprocess.check_call([
            javac,
            "--release", "8",
            "-classpath", android_jar,
            "-d", str(classes_dir),
            str(rg_bridge_java),
            str(rg_test_java),
        ])
        cp_sep = ";" if os.name == "nt" else ":"
        subprocess.check_call([
            java,
            "-cp", f"{classes_dir}{cp_sep}{android_jar}",
            "app.rustgramm.RgBridgeTest",
        ])


if __name__ == "__main__":
    main()
