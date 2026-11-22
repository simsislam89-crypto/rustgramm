use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    println!("cargo:rerun-if-changed=assets/icon.ico");
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("OUT_DIR"));
    let src_ico = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"))
        .join("assets")
        .join("icon.ico");
    if !src_ico.exists() {
        return;
    }

    // windres не переносит пробелы в путях: копируем во временную папку без пробелов.
    let tmp_dir = env::temp_dir().join("rustgramm-icon");
    let _ = fs::create_dir_all(&tmp_dir);
    let safe_ico = tmp_dir.join("app.ico");
    let safe_rc = tmp_dir.join("app.rc");
    if fs::copy(&src_ico, &safe_ico).is_err() {
        return;
    }
    let ico_escaped = safe_ico.to_string_lossy().replace('\\', "/");
    let rc_text = format!("1 ICON \"{ico_escaped}\"\n");
    if fs::write(&safe_rc, rc_text).is_err() {
        return;
    }

    let target_env = env::var("CARGO_CFG_TARGET_ENV").unwrap_or_default();
    if target_env == "gnu" {
        let obj = out_dir.join("icon.o");
        if Command::new("windres")
            .arg(&safe_rc)
            .arg("-O")
            .arg("coff")
            .arg("-o")
            .arg(&obj)
            .status()
            .is_ok_and(|s| s.success())
        {
            println!("cargo:rustc-link-arg={}", obj.display());
        }
    } else if target_env == "msvc" {
        let res = out_dir.join("icon.res");
        if Command::new("rc")
            .arg("/fo")
            .arg(&res)
            .arg(&safe_rc)
            .status()
            .is_ok_and(|s| s.success())
        {
            println!("cargo:rustc-link-arg={}", res.display());
        }
    }
}
