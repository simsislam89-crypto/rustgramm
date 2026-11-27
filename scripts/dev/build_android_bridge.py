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
import android.app.AlertDialog;
import android.app.Dialog;
import android.app.Fragment;
import android.app.FragmentManager;
import android.content.Context;
import android.content.DialogInterface;
import android.content.Intent;
import android.database.Cursor;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.text.InputType;
import android.view.Gravity;
import android.view.KeyEvent;
import android.view.View;
import android.view.ViewGroup;
import android.view.Window;
import android.view.WindowInsets;
import android.view.WindowManager;
import android.view.inputmethod.EditorInfo;
import android.view.inputmethod.InputMethodManager;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.EditText;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

public final class RgBridge {
    public static final AtomicReference<String> PICKED_PATH = new AtomicReference<>(null);
    public static final AtomicReference<String> PENDING_LINK = new AtomicReference<>(null);
    public static final AtomicReference<String> INPUT_TAG = new AtomicReference<>(null);
    public static final AtomicReference<String> INPUT_RESULT = new AtomicReference<>(null);
    // Атомарный канал "OK:tag\0value" / "CAN:tag", исключающий гонку pollInputTag/pollInputResult.
    public static final AtomicReference<String> INPUT_EVENT = new AtomicReference<>(null);
    public static final AtomicBoolean WEBVIEW_OPEN = new AtomicBoolean(false);
    private static volatile Dialog activeWebDialog = null;
    private static volatile WebView activeWebView = null;
    private static volatile AlertDialog activeInputDialog = null;
    private static volatile String pendingDestDir = null;

    public static void showSoftInput(final Activity act) {
        if (act == null) return;
        act.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    View decor = act.getWindow() != null ? act.getWindow().getDecorView() : null;
                    if (decor == null) return;
                    decor.requestFocus();
                    if (Build.VERSION.SDK_INT >= 30 && decor.getWindowInsetsController() != null) {
                        decor.getWindowInsetsController().show(WindowInsets.Type.ime());
                    }
                    InputMethodManager imm = (InputMethodManager) act.getSystemService(Context.INPUT_METHOD_SERVICE);
                    if (imm != null) {
                        imm.showSoftInput(decor, InputMethodManager.SHOW_FORCED);
                    }
                } catch (Throwable ignored) {}
            }
        });
    }

    public static void promptTextInput(
        final Activity act,
        final String tag,
        final String title,
        final String hint,
        final String initial,
        final int inputTypeKind
    ) {
        if (act == null) return;
        act.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    if (activeInputDialog != null && activeInputDialog.isShowing()) {
                        activeInputDialog.dismiss();
                    }
                    INPUT_TAG.set(null);
                    INPUT_RESULT.set(null);
                    INPUT_EVENT.set(null);
                    final EditText et = new EditText(act);
                    et.setHint(hint != null ? hint : "");
                    et.setText(initial != null ? initial : "");
                    et.setTextColor(Color.WHITE);
                    et.setHintTextColor(Color.parseColor("#8E8E98"));
                    et.setBackgroundColor(Color.parseColor("#22232A"));
                    et.setPadding(28, 24, 28, 24);
                    et.setSingleLine(true);
                    int itype = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_FLAG_NO_SUGGESTIONS;
                    if (inputTypeKind == 1) {
                        itype = InputType.TYPE_CLASS_PHONE;
                    } else if (inputTypeKind == 2) {
                        itype = InputType.TYPE_CLASS_NUMBER;
                    } else if (inputTypeKind == 3) {
                        itype = InputType.TYPE_CLASS_TEXT | InputType.TYPE_TEXT_VARIATION_PASSWORD;
                    }
                    et.setInputType(itype);
                    if (et.getText() != null) {
                        et.setSelection(et.getText().length());
                    }
                    et.setImeOptions(EditorInfo.IME_ACTION_DONE);

                    LinearLayout box = new LinearLayout(act);
                    box.setOrientation(LinearLayout.VERTICAL);
                    box.setPadding(40, 20, 40, 8);
                    box.setBackgroundColor(Color.parseColor("#18191E"));
                    box.addView(et, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT));

                    final AlertDialog dlg = new AlertDialog.Builder(act, android.R.style.Theme_DeviceDefault_Dialog_Alert)
                        .setTitle(title != null ? title : "")
                        .setView(box)
                        .setPositiveButton("Готово", new DialogInterface.OnClickListener() {
                            @Override
                            public void onClick(DialogInterface d, int which) {
                                String t = tag != null ? tag : "";
                                String val = et.getText() != null ? et.getText().toString() : "";
                                INPUT_EVENT.set("OK:" + t + "\0" + val);
                                INPUT_RESULT.set(val);
                                INPUT_TAG.set(t);
                            }
                        })
                        .setNegativeButton("Отмена", new DialogInterface.OnClickListener() {
                            @Override
                            public void onClick(DialogInterface d, int which) {
                                String t = tag != null ? tag : "";
                                INPUT_EVENT.set("CAN:" + t);
                                INPUT_RESULT.set(null);
                                INPUT_TAG.set("");
                            }
                        })
                        .setOnCancelListener(new DialogInterface.OnCancelListener() {
                            @Override
                            public void onCancel(DialogInterface d) {
                                String t = tag != null ? tag : "";
                                INPUT_EVENT.set("CAN:" + t);
                                INPUT_RESULT.set(null);
                                INPUT_TAG.set("");
                            }
                        })
                        .create();

                    et.setOnEditorActionListener(new TextView.OnEditorActionListener() {
                        @Override
                        public boolean onEditorAction(TextView v, int actionId, KeyEvent event) {
                            if (actionId == EditorInfo.IME_ACTION_DONE || actionId == EditorInfo.IME_ACTION_GO || actionId == EditorInfo.IME_ACTION_SEND) {
                                String t = tag != null ? tag : "";
                                String val = et.getText() != null ? et.getText().toString() : "";
                                INPUT_EVENT.set("OK:" + t + "\0" + val);
                                INPUT_RESULT.set(val);
                                INPUT_TAG.set(t);
                                dlg.dismiss();
                                return true;
                            }
                            return false;
                        }
                    });
                    Window w = dlg.getWindow();
                    if (w != null) {
                        w.setSoftInputMode(WindowManager.LayoutParams.SOFT_INPUT_STATE_ALWAYS_VISIBLE);
                        if (Build.VERSION.SDK_INT >= 31) {
                            w.addFlags(WindowManager.LayoutParams.FLAG_BLUR_BEHIND);
                            WindowManager.LayoutParams lp = w.getAttributes();
                            lp.setBlurBehindRadius(36);
                            w.setAttributes(lp);
                        }
                    }
                    activeInputDialog = dlg;
                    dlg.show();
                    et.requestFocus();
                    et.postDelayed(new Runnable() {
                        @Override
                        public void run() {
                            try {
                                InputMethodManager imm = (InputMethodManager) act.getSystemService(Context.INPUT_METHOD_SERVICE);
                                if (imm != null) {
                                    imm.showSoftInput(et, InputMethodManager.SHOW_IMPLICIT);
                                }
                            } catch (Throwable ignored) {}
                        }
                    }, 80);
                } catch (Throwable ignored) {
                    INPUT_EVENT.set("CAN:" + (tag != null ? tag : ""));
                    INPUT_RESULT.set(null);
                    INPUT_TAG.set("");
                }
            }
        });
    }

    public static String pollInputEvent() {
        return INPUT_EVENT.getAndSet(null);
    }

    public static String pollInputTag() {
        return INPUT_TAG.getAndSet(null);
    }

    public static String pollInputResult() {
        return INPUT_RESULT.getAndSet(null);
    }

    public static void setupGlassWindow(final Activity act) {
        if (act == null) return;
        act.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    Window w = act.getWindow();
                    if (w == null) return;
                    w.addFlags(WindowManager.LayoutParams.FLAG_DRAWS_SYSTEM_BAR_BACKGROUNDS);
                    w.clearFlags(
                        WindowManager.LayoutParams.FLAG_TRANSLUCENT_STATUS
                            | WindowManager.LayoutParams.FLAG_TRANSLUCENT_NAVIGATION
                    );
                    w.setStatusBarColor(Color.TRANSPARENT);
                    w.setNavigationBarColor(Color.parseColor("#660C0E14"));
                    if (Build.VERSION.SDK_INT >= 30) {
                        w.setDecorFitsSystemWindows(false);
                    }
                } catch (Throwable ignored) {}
            }
        });
    }

    public static String getSafeInsetsPx(final Activity act) {
        if (act == null || act.getWindow() == null) return "0,0,0,0";
        try {
            View decor = act.getWindow().getDecorView();
            if (decor == null || Build.VERSION.SDK_INT < 23) return "0,0,0,0";
            WindowInsets wi = decor.getRootWindowInsets();
            if (wi == null) return "0,0,0,0";
            if (Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets ins = wi.getInsets(
                    WindowInsets.Type.systemBars()
                        | WindowInsets.Type.displayCutout()
                        | WindowInsets.Type.ime()
                );
                return ins.top + "," + ins.right + "," + ins.bottom + "," + ins.left;
            }
            return wi.getSystemWindowInsetTop()
                + ","
                + wi.getSystemWindowInsetRight()
                + ","
                + wi.getSystemWindowInsetBottom()
                + ","
                + wi.getSystemWindowInsetLeft();
        } catch (Throwable ignored) {
            return "0,0,0,0";
        }
    }

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

    public static boolean isWebViewOpen() {
        return WEBVIEW_OPEN.get();
    }

    public static void openExternalUrl(final Activity act, final String url) {
        if (act == null || url == null) return;
        if (!(url.startsWith("http://") || url.startsWith("https://"))) return;
        act.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    Intent intent = new Intent(Intent.ACTION_VIEW, Uri.parse(url));
                    intent.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK);
                    act.startActivity(intent);
                } catch (Throwable ignored) {}
            }
        });
    }

    public static void openWebView(final Activity act, final String url, final String title, final boolean spoofAndroid) {
        if (act == null || url == null) return;
        if (!(url.startsWith("http://") || url.startsWith("https://"))) return;
        act.runOnUiThread(new Runnable() {
            @Override
            public void run() {
                try {
                    closeWebView(act);
                    final Dialog dlg = new Dialog(act, android.R.style.Theme_Black_NoTitleBar_Fullscreen);
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

                    final WebView wv = new WebView(act);
                    WebSettings ws = wv.getSettings();
                    ws.setJavaScriptEnabled(true);
                    ws.setDomStorageEnabled(true);
                    ws.setAllowFileAccess(false);
                    ws.setAllowContentAccess(false);
                    ws.setMediaPlaybackRequiresUserGesture(false);
                    if (spoofAndroid) {
                        ws.setUserAgentString("Mozilla/5.0 (Linux; Android 14; Pixel 8) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0.0.0 Mobile Safari/537.36");
                    }
                    wv.addJavascriptInterface(new JsProxy(act, dlg), "TelegramWebviewProxy");
                    wv.setWebViewClient(new WebViewClient() {
                        @Override
                        public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                            String u = request.getUrl() != null ? request.getUrl().toString() : "";
                            if (u.startsWith("tg://") || u.startsWith("https://t.me/") || u.startsWith("https://telegram.me/")) {
                                PENDING_LINK.set(u);
                                dlg.dismiss();
                                return true;
                            }
                            return !(u.startsWith("http://") || u.startsWith("https://"));
                        }
                    });
                    dlg.setOnDismissListener(new DialogInterface.OnDismissListener() {
                        @Override
                        public void onDismiss(DialogInterface d) {
                            WEBVIEW_OPEN.set(false);
                            if (activeWebDialog == dlg) activeWebDialog = null;
                            if (activeWebView == wv) activeWebView = null;
                            try {
                                wv.stopLoading();
                                wv.destroy();
                            } catch (Throwable ignored) {}
                        }
                    });
                    wv.loadUrl(url);
                    root.addView(wv, new LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, 0, 1f));
                    dlg.setContentView(root);
                    activeWebDialog = dlg;
                    activeWebView = wv;
                    WEBVIEW_OPEN.set(true);
                    dlg.show();
                } catch (Throwable ignored) {
                    WEBVIEW_OPEN.set(false);
                }
            }
        });
    }

    public static void closeWebView(final Activity act) {
        final Dialog d = activeWebDialog;
        final WebView wv = activeWebView;
        activeWebDialog = null;
        activeWebView = null;
        WEBVIEW_OPEN.set(false);
        if ((d != null || wv != null) && act != null) {
            act.runOnUiThread(new Runnable() {
                @Override
                public void run() {
                    try {
                        if (d != null && d.isShowing()) d.dismiss();
                    } catch (Throwable ignored) {}
                    try {
                        if (wv != null) {
                            wv.stopLoading();
                            wv.destroy();
                        }
                    } catch (Throwable ignored) {}
                }
            });
        }
    }

    public static final class JsProxy {
        private final Activity act;
        private final Dialog dlg;
        JsProxy(Activity a, Dialog d) { this.act = a; this.dlg = d; }
        @JavascriptInterface
        public void postEvent(final String eventType, final String eventData) {
            if ("web_app_close".equals(eventType)) {
                act.runOnUiThread(new Runnable() {
                    @Override
                    public void run() {
                        try { dlg.dismiss(); } catch (Throwable ignored) {}
                    }
                });
            } else if ("web_app_open_tg_link".equals(eventType) && eventData != null) {
                PENDING_LINK.set(eventData);
                act.runOnUiThread(new Runnable() {
                    @Override
                    public void run() {
                        try { dlg.dismiss(); } catch (Throwable ignored) {}
                    }
                });
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
