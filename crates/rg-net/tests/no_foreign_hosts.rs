//! CI-проверка: в исходниках workspace нет URL вне белого списка Telegram.
//! Строка с комментарием `// allow-host` пропускается (ссылки в документации).

use rg_net::{Guard, host_of};
use std::fs;
use std::path::Path;

fn walk(dir: &Path, out: &mut Vec<std::path::PathBuf>) {
    let Ok(rd) = fs::read_dir(dir) else { return };
    for e in rd.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

#[test]
fn sources_contain_only_telegram_urls() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("..");
    let mut files = Vec::new();
    walk(&root, &mut files);
    assert!(!files.is_empty());
    let guard = Guard::new();
    let mut bad = Vec::new();
    for f in files {
        if f.ends_with("no_foreign_hosts.rs") {
            continue;
        }
        let text = fs::read_to_string(&f).unwrap_or_default();
        for (n, line) in text.lines().enumerate() {
            if line.trim() == "#[cfg(test)]" {
                break; // тестовые данные в конце файла проверяем не здесь
            }
            if line.contains("allow-host") {
                continue;
            }
            let mut rest = line;
            while let Some(i) = rest.find("://") {
                let after = &rest[i + 3..];
                let scheme_ok = rest[..i].ends_with("https") || rest[..i].ends_with("http");
                if scheme_ok
                    && let Some(h) = host_of(after)
                    && !guard.host_allowed(&h)
                {
                    bad.push(format!("{}:{}: {h}", f.display(), n + 1));
                }
                rest = after;
            }
        }
    }
    assert!(bad.is_empty(), "внешние хосты в коде:\n{}", bad.join("\n"));
}
