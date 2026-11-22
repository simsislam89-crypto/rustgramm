//! Проверка и установка обновлений Rustgramm с GitHub Releases (`simsislam89-crypto/rustgramm`).
//! Работает анонимно (без ID/сессии/телеметрии), только по официальному репозиторию,
//! на Windows использует системный WinHTTP (0 КБ лишних крейтов, нативный TLS 1.3).

use std::path::PathBuf;

const RELEASES_API_URL: &str = "https://api.github.com/repos/simsislam89-crypto/rustgramm/releases?per_page=8"; // allow-host
const ALLOWED_DOWNLOAD_PREFIX: &str = "https://github.com/simsislam89-crypto/rustgramm/releases/download/"; // allow-host

/// Сведения о доступном обновлении Rustgramm.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpdateInfo {
    pub version: String,
    pub tag: String,
    pub notes: String,
    pub download_url: String,
    pub page_url: String,
    pub asset_name: String,
    pub size: u64,
}

/// Разбирает строку версии вида `"v0.17.0"`, `"0.17.0-android-full"` в `(major, minor, patch)`.
pub fn parse_semver(raw: &str) -> Option<(u32, u32, u32)> {
    let s = raw.trim().trim_start_matches(['v', 'V']);
    let core = s.split(['-', '+', ' ']).next()?;
    let mut parts = core.split('.');
    let maj = parts.next()?.parse::<u32>().ok()?;
    let min = parts.next()?.parse::<u32>().ok()?;
    let pat = parts.next().and_then(|x| x.parse::<u32>().ok()).unwrap_or(0);
    Some((maj, min, pat))
}

/// Возвращает `true`, если `candidate` строго новее `current`.
pub fn is_newer_version(current: &str, candidate: &str) -> bool {
    match (parse_semver(current), parse_semver(candidate)) {
        (Some(c), Some(n)) => n > c,
        _ => false,
    }
}

/// Ожидаемое имя файла релиза для текущей платформы и редакции (Full/Lite).
pub fn expected_asset_name() -> &'static str {
    if cfg!(target_os = "android") {
        if cfg!(feature = "plugins") {
            "Rustgramm-Android-Full.apk"
        } else {
            "Rustgramm-Android-Lite.apk"
        }
    } else {
        "Rustgramm-Windows-x64.exe"
    }
}

/// Строгая проверка, что ссылка скачивания принадлежит исключительно нашему репозиторию.
pub fn is_allowed_release_asset_url(url: &str) -> bool {
    url.starts_with(ALLOWED_DOWNLOAD_PREFIX)
}

/// Выбирает наиболее свежий подходящий релиз из JSON-массива GitHub Releases.
pub fn pick_latest_update(json: &str, current_version: &str, wanted_asset: &str) -> Option<UpdateInfo> {
    let list: serde_json::Value = serde_json::from_str(json).ok()?;
    let arr = list.as_array()?;
    let mut best: Option<((u32, u32, u32), UpdateInfo)> = None;
    for rel in arr {
        if rel.get("draft").and_then(serde_json::Value::as_bool).unwrap_or(false) {
            continue;
        }
        let tag = rel.get("tag_name").and_then(serde_json::Value::as_str).unwrap_or("");
        let Some(ver) = parse_semver(tag) else { continue };
        if !is_newer_version(current_version, tag) {
            continue;
        }
        let Some(assets) = rel.get("assets").and_then(serde_json::Value::as_array) else {
            continue;
        };
        let matched = assets.iter().find(|a| {
            a.get("name")
                .and_then(serde_json::Value::as_str)
                .is_some_and(|n| n.eq_ignore_ascii_case(wanted_asset))
        });
        let Some(asset) = matched else { continue };
        let dl = asset
            .get("browser_download_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("");
        if !is_allowed_release_asset_url(dl) {
            continue;
        }
        let size = asset.get("size").and_then(serde_json::Value::as_u64).unwrap_or(0);
        let page = rel
            .get("html_url")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .to_owned();
        let body = rel
            .get("body")
            .and_then(serde_json::Value::as_str)
            .unwrap_or("")
            .lines()
            .take(3)
            .collect::<Vec<_>>()
            .join(" · ");
        let info = UpdateInfo {
            version: format!("{}.{}.{}", ver.0, ver.1, ver.2),
            tag: tag.to_owned(),
            notes: body.chars().take(180).collect(),
            download_url: dl.to_owned(),
            page_url: page,
            asset_name: wanted_asset.to_owned(),
            size,
        };
        if best.as_ref().is_none_or(|(b, _)| ver > *b) {
            best = Some((ver, info));
        }
    }
    best.map(|(_, info)| info)
}

#[cfg(windows)]
mod platform {
    use super::*;
    use std::ffi::c_void;
    use std::io::Write;

    const WINHTTP_ACCESS_TYPE_DEFAULT_PROXY: u32 = 0;
    const WINHTTP_FLAG_SECURE: u32 = 0x0080_0000;
    const WINHTTP_ADDREQ_FLAG_ADD: u32 = 0x2000_0000;
    const WINHTTP_OPTION_REDIRECT_POLICY: u32 = 88;
    const WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS: u32 = 2;
    const WINHTTP_QUERY_STATUS_CODE: u32 = 19;
    const WINHTTP_QUERY_FLAG_NUMBER: u32 = 0x2000_0000;

    #[link(name = "winhttp")]
    unsafe extern "system" {
        fn WinHttpOpen(
            psz_agent_w: *const u16,
            dw_access_type: u32,
            psz_proxy_w: *const u16,
            psz_proxy_bypass_w: *const u16,
            dw_flags: u32,
        ) -> *mut c_void;
        fn WinHttpSetTimeouts(
            h_internet: *mut c_void,
            resolve_timeout: i32,
            connect_timeout: i32,
            send_timeout: i32,
            receive_timeout: i32,
        ) -> i32;
        fn WinHttpConnect(
            h_session: *mut c_void,
            pswz_server_name: *const u16,
            n_server_port: u16,
            dw_reserved: u32,
        ) -> *mut c_void;
        fn WinHttpOpenRequest(
            h_connect: *mut c_void,
            pwsz_verb: *const u16,
            pwsz_object_name: *const u16,
            pwsz_version: *const u16,
            pwsz_referrer: *const u16,
            ppwsz_accept_types: *const *const u16,
            dw_flags: u32,
        ) -> *mut c_void;
        fn WinHttpAddRequestHeaders(
            h_request: *mut c_void,
            lpsz_headers: *const u16,
            dw_headers_length: u32,
            dw_modifiers: u32,
        ) -> i32;
        fn WinHttpSetOption(
            h_internet: *mut c_void,
            dw_option: u32,
            lp_buffer: *const c_void,
            dw_buffer_length: u32,
        ) -> i32;
        fn WinHttpSendRequest(
            h_request: *mut c_void,
            lpsz_headers: *const u16,
            dw_headers_length: u32,
            lp_optional: *const c_void,
            dw_optional_length: u32,
            dw_total_length: u32,
            dw_context: usize,
        ) -> i32;
        fn WinHttpReceiveResponse(h_request: *mut c_void, lp_reserved: *mut c_void) -> i32;
        fn WinHttpQueryHeaders(
            h_request: *mut c_void,
            dw_info_level: u32,
            pwsz_name: *const u16,
            lp_buffer: *mut c_void,
            lpdw_buffer_length: *mut u32,
            lpdw_index: *mut u32,
        ) -> i32;
        fn WinHttpQueryDataAvailable(h_request: *mut c_void, lpdw_number_of_bytes_available: *mut u32) -> i32;
        fn WinHttpReadData(
            h_request: *mut c_void,
            lp_buffer: *mut c_void,
            dw_number_of_bytes_to_read: u32,
            lpdw_number_of_bytes_read: *mut u32,
        ) -> i32;
        fn WinHttpCloseHandle(h_internet: *mut c_void) -> i32;
    }

    fn wide(s: &str) -> Vec<u16> {
        s.encode_utf16().chain(std::iter::once(0)).collect()
    }

    fn parse_https_url(url: &str) -> Result<(String, String), String> {
        let rest = url
            .strip_prefix("https://") // allow-host
            .ok_or_else(|| "Поддерживается только HTTPS".to_owned())?;
        let (host, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        Ok((host.to_owned(), path.to_owned()))
    }

    fn https_get_stream(
        url: &str,
        extra_headers: &str,
        mut on_chunk: impl FnMut(&[u8]) -> Result<(), String>,
    ) -> Result<(), String> {
        let (host, path) = parse_https_url(url)?;
        let agent = wide("Rustgramm");
        let session = unsafe {
            WinHttpOpen(
                agent.as_ptr(),
                WINHTTP_ACCESS_TYPE_DEFAULT_PROXY,
                std::ptr::null(),
                std::ptr::null(),
                0,
            )
        };
        if session.is_null() {
            return Err("WinHttpOpen не удался".to_owned());
        }
        unsafe {
            WinHttpSetTimeouts(session, 8000, 10000, 15000, 30000);
        }
        let whost = wide(&host);
        let conn = unsafe { WinHttpConnect(session, whost.as_ptr(), 443, 0) };
        if conn.is_null() {
            unsafe { WinHttpCloseHandle(session) };
            return Err("Не удалось подключиться к серверу обновлений".to_owned());
        }
        let verb = wide("GET");
        let wpath = wide(&path);
        let req = unsafe {
            WinHttpOpenRequest(
                conn,
                verb.as_ptr(),
                wpath.as_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                std::ptr::null(),
                WINHTTP_FLAG_SECURE,
            )
        };
        if req.is_null() {
            unsafe {
                WinHttpCloseHandle(conn);
                WinHttpCloseHandle(session);
            }
            return Err("WinHttpOpenRequest не удался".to_owned());
        }
        let policy = WINHTTP_OPTION_REDIRECT_POLICY_ALWAYS;
        unsafe {
            WinHttpSetOption(
                req,
                WINHTTP_OPTION_REDIRECT_POLICY,
                std::ptr::from_ref(&policy).cast(),
                std::mem::size_of::<u32>() as u32,
            );
        }
        if !extra_headers.is_empty() {
            let hdr = wide(extra_headers);
            unsafe {
                WinHttpAddRequestHeaders(req, hdr.as_ptr(), u32::MAX, WINHTTP_ADDREQ_FLAG_ADD);
            }
        }
        let ok = unsafe { WinHttpSendRequest(req, std::ptr::null(), 0, std::ptr::null(), 0, 0, 0) != 0 }
            && unsafe { WinHttpReceiveResponse(req, std::ptr::null_mut()) != 0 };
        if !ok {
            unsafe {
                WinHttpCloseHandle(req);
                WinHttpCloseHandle(conn);
                WinHttpCloseHandle(session);
            }
            return Err("Не удалось получить ответ от GitHub Releases".to_owned());
        }
        let mut status: u32 = 0;
        let mut status_len = std::mem::size_of::<u32>() as u32;
        unsafe {
            WinHttpQueryHeaders(
                req,
                WINHTTP_QUERY_STATUS_CODE | WINHTTP_QUERY_FLAG_NUMBER,
                std::ptr::null(),
                std::ptr::from_mut(&mut status).cast(),
                &mut status_len,
                std::ptr::null_mut(),
            );
        }
        if !(200..300).contains(&status) {
            unsafe {
                WinHttpCloseHandle(req);
                WinHttpCloseHandle(conn);
                WinHttpCloseHandle(session);
            }
            return Err(format!("Сервер обновлений вернул HTTP {status}"));
        }

        let mut buf = vec![0u8; 64 * 1024];
        let mut read_err: Option<String> = None;
        loop {
            let mut avail: u32 = 0;
            if unsafe { WinHttpQueryDataAvailable(req, &mut avail) } == 0 || avail == 0 {
                break;
            }
            let want = (avail as usize).min(buf.len()) as u32;
            let mut got: u32 = 0;
            if unsafe { WinHttpReadData(req, buf.as_mut_ptr().cast(), want, &mut got) } == 0 || got == 0 {
                break;
            }
            if let Err(e) = on_chunk(&buf[..got as usize]) {
                read_err = Some(e);
                break;
            }
        }
        unsafe {
            WinHttpCloseHandle(req);
            WinHttpCloseHandle(conn);
            WinHttpCloseHandle(session);
        }
        if let Some(e) = read_err {
            return Err(e);
        }
        Ok(())
    }

    pub fn fetch_releases_json() -> Result<String, String> {
        let mut body = Vec::with_capacity(32 * 1024);
        https_get_stream(
            RELEASES_API_URL,
            "Accept: application/vnd.github+json\r\n",
            |chunk| {
                if body.len() + chunk.len() > 2 * 1024 * 1024 {
                    return Err("Ответ API слишком большой".to_owned());
                }
                body.extend_from_slice(chunk);
                Ok(())
            },
        )?;
        String::from_utf8(body).map_err(|_| "Некорректная кодировка JSON".to_owned())
    }

    pub fn download_and_launch_update(
        info: &UpdateInfo,
        on_progress: impl Fn(u64, u64),
    ) -> Result<PathBuf, String> {
        if !is_allowed_release_asset_url(&info.download_url) {
            return Err("Недопустимый адрес обновления".to_owned());
        }
        let dir = std::env::temp_dir().join("rustgramm-updater");
        let _ = std::fs::create_dir_all(&dir);
        let new_exe = dir.join(format!("Rustgramm-{}.exe", info.version));
        let mut f = std::fs::File::create(&new_exe).map_err(|e| format!("Файл обновления: {e}"))?;
        let mut done = 0u64;
        https_get_stream(&info.download_url, "", |chunk| {
            f.write_all(chunk).map_err(|e| e.to_string())?;
            done += chunk.len() as u64;
            on_progress(done, info.size.max(done));
            Ok(())
        })?;
        drop(f);
        // Проверяем заголовок PE (`MZ`) и минимальный размер, чтобы не подменить битым файлом.
        let head = std::fs::read(&new_exe).map_err(|e| e.to_string())?;
        if head.len() < 512 * 1024 || &head[..2] != b"MZ" {
            let _ = std::fs::remove_file(&new_exe);
            return Err("Скачанный файл повреждён".to_owned());
        }
        if let Ok(cur_exe) = std::env::current_exe() {
            let bat = dir.join("apply_update.cmd");
            let cur_s = cur_exe.to_string_lossy();
            let new_s = new_exe.to_string_lossy();
            let script = format!(
                "@echo off\r\n\
                 chcp 65001 >nul\r\n\
                 timeout /t 1 /nobreak >nul\r\n\
                 :loop\r\n\
                 copy /y \"{new_s}\" \"{cur_s}\" >nul 2>&1\r\n\
                 if errorlevel 1 (\r\n\
                   timeout /t 1 /nobreak >nul\r\n\
                   goto loop\r\n\
                 )\r\n\
                 start \"\" \"{cur_s}\"\r\n\
                 del /f /q \"{new_s}\" >nul 2>&1\r\n\
                 del /f /q \"%~f0\" >nul 2>&1\r\n"
            );
            std::fs::write(&bat, script).map_err(|e| e.to_string())?;
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x0800_0000;
            let _ = std::process::Command::new("cmd")
                .args(["/C", &bat.to_string_lossy()])
                .creation_flags(CREATE_NO_WINDOW)
                .spawn();
        }
        Ok(new_exe)
    }
}

#[cfg(not(windows))]
mod platform {
    use super::*;

    pub fn fetch_releases_json() -> Result<String, String> {
        let out = std::process::Command::new("curl")
            .args([
                "-fsSL",
                "--max-time",
                "12",
                "-H",
                "User-Agent: Rustgramm",
                "-H",
                "Accept: application/vnd.github+json",
                RELEASES_API_URL,
            ])
            .output()
            .map_err(|e| format!("Проверка обновлений недоступна: {e}"))?;
        if !out.status.success() {
            return Err("Не удалось проверить обновления".to_owned());
        }
        String::from_utf8(out.stdout).map_err(|_| "Некорректный JSON".to_owned())
    }

    pub fn download_and_launch_update(
        info: &UpdateInfo,
        _on_progress: impl Fn(u64, u64),
    ) -> Result<PathBuf, String> {
        if !is_allowed_release_asset_url(&info.download_url) {
            return Err("Недопустимый адрес обновления".to_owned());
        }
        crate::rich::open_url(&info.download_url);
        Ok(PathBuf::from(&info.asset_name))
    }
}

/// Проверяет, вышел ли более новый релиз Rustgramm для текущей платформы.
pub fn check_latest_release(current_version: &str) -> Result<Option<UpdateInfo>, String> {
    let json = platform::fetch_releases_json()?;
    Ok(pick_latest_update(&json, current_version, expected_asset_name()))
}

/// Скачивает и применяет обновление (`Windows`: самозамена `.exe` и перезапуск; `Android`: открывает загрузку APK).
pub fn download_and_apply_update(
    info: &UpdateInfo,
    on_progress: impl Fn(u64, u64),
) -> Result<PathBuf, String> {
    platform::download_and_launch_update(info, on_progress)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn semver_parsing_and_comparison_work() {
        assert_eq!(parse_semver("v0.17.0"), Some((0, 17, 0)));
        assert_eq!(parse_semver("0.17.1-android-full"), Some((0, 17, 1)));
        assert!(is_newer_version("0.16.0", "v0.17.0"));
        assert!(!is_newer_version("0.17.0", "v0.17.0"));
        assert!(!is_newer_version("0.18.0", "v0.17.9"));
    }

    #[test]
    fn picks_only_matching_asset_from_trusted_repo() {
        let sample = r#"[
            {
                "tag_name": "v0.18.0",
                "draft": false,
                "html_url": "https://github.com/simsislam89-crypto/rustgramm/releases/tag/v0.18.0",
                "body": "Новые функции\nИсправлены баги",
                "assets": [
                    {
                        "name": "Rustgramm-Windows-x64.exe",
                        "size": 12500000,
                        "browser_download_url": "https://github.com/simsislam89-crypto/rustgramm/releases/download/v0.18.0/Rustgramm-Windows-x64.exe"
                    }
                ]
            },
            {
                "tag_name": "v0.99.0",
                "draft": false,
                "html_url": "https://evil.example/rel",
                "body": "spoof",
                "assets": [
                    {
                        "name": "Rustgramm-Windows-x64.exe",
                        "size": 999,
                        "browser_download_url": "https://evil.example/Rustgramm-Windows-x64.exe"
                    }
                ]
            }
        ]"#;
        let picked = pick_latest_update(sample, "0.17.0", "Rustgramm-Windows-x64.exe").unwrap();
        assert_eq!(picked.version, "0.18.0");
        assert_eq!(picked.tag, "v0.18.0");
        assert!(is_allowed_release_asset_url(&picked.download_url));
    }
}
