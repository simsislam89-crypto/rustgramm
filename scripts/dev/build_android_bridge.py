#!/usr/bin/env python3
"""Builds the minimal Android DEX helper (`crates/rg-ui/assets/rg_bridge.dex`)
for NativeActivity: Intent file/gallery picker Fragment + embedded WebView Dialog.
Requires JDK javac + Android SDK d8 + android.jar.
"""
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
OUT_DEX = ROOT / "crates" / "rg-ui" / "assets" / "rg_bridge.dex"

JAVA_SRC = r"""package app.rustgramm;

import android.app.Activity;
import android.app.Dialog;
import android.app.Fragment;
import android.app.FragmentManager;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.view.Gravity;
import android.view.View;
import android.view.ViewGroup;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.util.concurrent.atomic.AtomicReference;

public final class RgBridge {
    public static final AtomicReference<String> PICKED_PATH = new AtomicReference<>(null);
    public static final AtomicReference<String> PENDING_LINK = new AtomicReference<>(null);
    private static volatile Dialog activeWebDialog = null;
    private static volatile String pendingDestDir = null;

    public static void pickFile(final Activity act, final String mime, final String destDir) {
        if (act == null) return;
        pendingDestDir = destDir;
        PICKED_PATH.set(null);
        act.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    FragmentManager fm = act.getFragmentManager();
                    Fragment old = fm.findFragmentByTag("rg_picker");
                    if (old != null) {
                        fm.beginTransaction().remove(old).commitNowAllowingStateLoss();
                    }
                    PickerFragment frag = new PickerFragment();
                    Bundle b = new Bundle();
                    b.putString("mime", mime == null || mime.isEmpty() ? "*/*" : mime);
                    frag.setArguments(b);
                    fm.beginTransaction().add(frag, "rg_picker").commitAllowingStateLoss();
                } catch (Throwable ignored) {}
            }
        });
    }

    public static String pollPickedFile() {
        return PICKED_PATH.getAndSet(null);
    }

    public static String pollPendingLink() {
        return PENDING_LINK.getAndSet(null);
    }

    public static void openWebView(final Activity act, final String url, final String title, final boolean spoofAndroid) {
        if (act == null || url == null) return;
        act.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    closeWebView(act);
                    Dialog dlg = new Dialog(act, android.R.style.Theme_Black_NoTitleBar_Fullscreen);
                    LinearLayout root = new LinearLayout(act);
                    root.setOrientation(LinearLayout.VERTICAL);
                    root.setBackgroundColor(Color.parseColor("#0E0E10"));

                    LinearLayout bar = new LinearLayout(act);
                    bar.setOrientation(LinearLayout.HORIZONTAL);
                    bar.setGravity(Gravity.CENTER_VERTICAL);
                    bar.setBackgroundColor(Color.parseColor("#141418"));
                    bar.setPadding(24, 16, 16, 16);

                    TextView tv = new TextView(act);
                    tv.setText(title == null || title.isEmpty() ? "Mini App" : title);
                    tv.setTextColor(Color.WHITE);
                    tv.setTextSize(16);
                    LinearLayout.LayoutParams tvLp = new LinearLayout.LayoutParams(0, ViewGroup.LayoutParams.WRAP_CONTENT, 1f);
                    bar.addView(tv, tvLp);

                    Button closeBtn = new Button(act);
                    closeBtn.setText("Закрыть");
                    closeBtn.setTextColor(Color.WHITE);
                    closeBtn.setBackgroundColor(Color.parseColor("#F58220"));
                    closeBtn.setOnClickListener(new View.OnClickListener() {
                        @Override
                        public void onClick(View v) {
                            dlg.dismiss();
                        }
                    });
                    bar.addView(closeBtn);
                    root.addView(bar, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));

                    WebView wv = new WebView(act);
                    WebSettings ws = wv.getSettings();
                    ws.setJavaScriptEnabled(true);
                    ws.setDomStorageEnabled(true);
                    ws.setMediaPlaybackRequiresUserGesture(false);
                    if (spoofAndroid) {
                        ws.setUserAgentString("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36");
                    }
                    wv.addJavascriptInterface(new JsProxy(dlg), "TelegramWebviewProxy");
                    wv.setWebViewClient(new WebViewClient() {
                        @Override
                        public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                            String u = request.getUrl() != null ? request.getUrl().toString() : "";
                            if (u.startsWith("tg://") || u.startsWith("https://t.me/") || u.startsWith("https://telegram.me/")) {
                                PENDING_LINK.set(u);
                                dlg.dismiss();
                                return true;
                            }
                            return false;
                        }
                    });
                    wv.loadUrl(url);
                    root.addView(wv, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
                    dlg.setContentView(root);
                    activeWebDialog = dlg;
                    dlg.show();
                } catch (Throwable ignored) {}
            }
        });
    }

    public static void closeWebView(final Activity act) {
        final Dialog d = activeWebDialog;
        activeWebDialog = null;
        if (d != null && act != null) {
            act.runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    try {
                        if (d.isShowing()) d.dismiss();
                    } catch (Throwable ignored) {}
                }
            });
        }
    }

    public static final class JsProxy {
        private final Dialog dlg;
        JsProxy(Dialog d) { this.dlg = d; }
        @JavascriptInterface
        public void postEvent(String eventType, String eventData) {
            if ("web_app_close".equals(eventType)) {
                dlg.dismiss();
            } else if ("web_app_open_tg_link".equals(eventType) && eventData != null) {
                PENDING_LINK.set(eventData);
                dlg.dismiss();
            }
        }
    }

    public static final class PickerFragment extends Fragment {
        private static final int REQ_CODE = 44182;

        @Override
        public void onCreate(Bundle savedInstanceState) {
            super.onCreate(savedInstanceState);
            String mime = getArguments() != null ? getArguments().getString("mime", "*/*") : "*/*";
            Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
            intent.addCategory(Intent.CATEGORY_OPENABLE);
            intent.setType(mime);
            try {
                startActivityForResult(intent, REQ_CODE);
            } catch (Throwable t) {
                Intent fallback = new Intent(Intent.ACTION_GET_CONTENT);
                fallback.setType(mime);
                try { startActivityForResult(fallback, REQ_CODE); } catch (Throwable ignored) {}
            }
        }

        @Override
        public void onActivityResult(int requestCode, int resultCode, Intent data) {
            super.onActivityResult(requestCode, resultCode, data);
            if (requestCode == REQ_CODE && resultCode == Activity.RESULT_OK && data != null && data.getData() != null) {
                copyUriToCache(getActivity(), data.getData());
            }
            try {
                getFragmentManager().beginTransaction().remove(this).commitAllowingStateLoss();
            } catch (Throwable ignored) {}
        }

        private static void copyUriToCache(Activity act, Uri uri) {
            if (act == null || uri == null) return;
            String name = "picked_" + System.currentTimeMillis();
            try (Cursor c = act.getContentResolver().query(uri, null, null, null, null)) {
                if (c != null && c.moveToFirst()) {
                    int idx = c.getColumnIndex(OpenableColumns.DISPLAY_NAME);
                    if (idx >= 0) {
                        String n = c.getString(idx);
                        if (n != null && !n.trim().isEmpty()) {
                            name = n.replace('/', '_').replace('\\', '_');
                        }
                    }
                }
            } catch (Throwable ignored) {}
            File dir = pendingDestDir != null ? new File(pendingDestDir) : act.getCacheDir();
            dir.mkdirs();
            File dst = new File(dir, System.currentTimeMillis() + "_" + name);
            try (InputStream in = act.getContentResolver().openInputStream(uri);
                 FileOutputStream out = new FileOutputStream(dst)) {
                if (in == null) return;
                byte[] buf = new byte[16384];
                int r;
                while ((r = in.read(buf)) > 0) {
                    out.write(buf, 0, r);
                }
                PICKED_PATH.set(dst.getAbsolutePath());
            } catch (Throwable ignored) {}
        }
    }
}
"""


def find_tool(candidates):
    for p in candidates:
        if p and Path(p).exists():
            return str(p)
    return None


def main():
    sdk = Path(os.environ.get("ANDROID_HOME", r"D:\android\sdk"))
    javac = find_tool([
        shutil.which("javac"),
        r"D:\android\jdk17\jdk-17.0.21+9\bin\javac.exe",
    ])
    android_jars = sorted((sdk / "platforms").glob("android-*/android.jar"))
    d8_list = sorted((sdk / "build-tools").glob("*/d8*"))
    if not javac or not android_jars or not d8_list:
        print("Missing javac / android.jar / d8", file=sys.stderr)
        sys.exit(1)
    android_jar = str(android_jars[-1])
    d8 = str(d8_list[-1])

    with tempfile.TemporaryDirectory() as td:
        td_path = Path(td)
        java_file = td_path / "app" / "rustgramm" / "RgBridge.java"
        java_file.parent.mkdir(parents=True, exist_ok=True)
        java_file.write_text(JAVA_SRC, encoding="utf-8")
        classes_dir = td_path / "classes"
        classes_dir.mkdir()
        subprocess.check_call([
            javac, "--release", "8", "-classpath", android_jar, "-d", str(classes_dir), str(java_file)
        ])
        class_files = [str(p) for p in classes_dir.rglob("*.class")]
        dex_out = td_path / "dex"
        dex_out.mkdir()
        env = os.environ.copy()
        jdk_bin = str(Path(javac).parent)
        env["JAVA_HOME"] = str(Path(javac).parent.parent)
        env["PATH"] = jdk_bin + os.pathsep + env.get("PATH", "")
        subprocess.check_call([d8, "--min-api", "24", "--output", str(dex_out)] + class_files, env=env)
        OUT_DEX.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(dex_out / "classes.dex", OUT_DEX)
        print(f"Built {OUT_DEX} ({OUT_DEX.stat().st_size} bytes)")


if __name__ == "__main__":
    main()
