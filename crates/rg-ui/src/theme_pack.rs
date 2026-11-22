//! Импорт и экспорт тем Telegram (`.tdesktop-theme` — текст или ZIP с обоями, и `.attheme` Android/AyuGram),
//! а также встроенные пресеты и живая конвертация цветов для конструктора палитры.

use std::collections::HashMap;
use std::io::Read;
use std::path::Path;

use eframe::egui::Color32;
use flate2::read::DeflateDecoder;
use rg_settings::CustomThemeSettings;

/// Разобранная тема: настройки палитры + необязательные байты встроенных обоев (`background.jpg`/`WPS`).
#[derive(Debug, Clone)]
pub struct ImportedTheme {
    pub settings: CustomThemeSettings,
    pub wallpaper_bytes: Option<(String, Vec<u8>)>,
}

/// Встроенные пресеты оформления Rustgramm / AyuGram.
pub fn builtin_presets() -> Vec<(&'static str, CustomThemeSettings)> {
    vec![
        (
            "Rustgramm Classic (Оранжевый)",
            CustomThemeSettings::default(),
        ),
        (
            "AyuGram AMOLED (Чёрный)",
            CustomThemeSettings {
                active_theme_name: "AyuGram AMOLED".to_owned(),
                accent_hex: "#F58220".to_owned(),
                window_bg_hex: "#08080A".to_owned(),
                sidebar_bg_hex: "#000000".to_owned(),
                chat_bg_hex: "#000000".to_owned(),
                msg_in_bg_hex: "#16161A".to_owned(),
                msg_out_bg_hex: "#2C221A".to_owned(),
                text_primary_hex: "#F2F2F5".to_owned(),
                text_secondary_hex: "#8E8E98".to_owned(),
                button_bg_hex: "#202026".to_owned(),
                ..CustomThemeSettings::default()
            },
        ),
        (
            "Graphite Studio",
            CustomThemeSettings {
                active_theme_name: "Graphite Studio".to_owned(),
                accent_hex: "#FF9838".to_owned(),
                window_bg_hex: "#181A1F".to_owned(),
                sidebar_bg_hex: "#111317".to_owned(),
                chat_bg_hex: "#101216".to_owned(),
                msg_in_bg_hex: "#23262E".to_owned(),
                msg_out_bg_hex: "#3E352D".to_owned(),
                text_primary_hex: "#ECEEF2".to_owned(),
                text_secondary_hex: "#98A0AE".to_owned(),
                button_bg_hex: "#2E323C".to_owned(),
                ..CustomThemeSettings::default()
            },
        ),
        (
            "Monochrome Paper",
            CustomThemeSettings {
                active_theme_name: "Monochrome Paper".to_owned(),
                accent_hex: "#E8E8EC".to_owned(),
                window_bg_hex: "#121214".to_owned(),
                sidebar_bg_hex: "#0B0B0D".to_owned(),
                chat_bg_hex: "#09090B".to_owned(),
                msg_in_bg_hex: "#1E1E22".to_owned(),
                msg_out_bg_hex: "#3A3A40".to_owned(),
                text_primary_hex: "#F5F5F7".to_owned(),
                text_secondary_hex: "#8A8A94".to_owned(),
                button_bg_hex: "#2C2C32".to_owned(),
                ..CustomThemeSettings::default()
            },
        ),
    ]
}

/// Преобразует `#RRGGBB` или `#RRGGBBAA` в `Color32`.
pub fn parse_hex_color(s: &str) -> Option<Color32> {
    let hex = s.trim().trim_start_matches('#');
    match hex.len() {
        6 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            Some(Color32::from_rgb(r, g, b))
        }
        8 => {
            let r = u8::from_str_radix(&hex[0..2], 16).ok()?;
            let g = u8::from_str_radix(&hex[2..4], 16).ok()?;
            let b = u8::from_str_radix(&hex[4..6], 16).ok()?;
            let a = u8::from_str_radix(&hex[6..8], 16).ok()?;
            Some(Color32::from_rgba_unmultiplied(r, g, b, a))
        }
        _ => None,
    }
}

/// Форматирует `Color32` в `#RRGGBB`.
pub fn color_to_hex(c: Color32) -> String {
    format!("#{:02X}{:02X}{:02X}", c.r(), c.g(), c.b())
}

/// Импортирует файл темы (`.tdesktop-theme` — ZIP или текст, либо `.attheme`).
pub fn import_theme_file(path: &Path) -> Result<ImportedTheme, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("Чтение темы: {e}"))?;
    let name = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("Импортированная тема")
        .to_owned();
    let ext = path
        .extension()
        .and_then(|e| e.to_str())
        .map(str::to_ascii_lowercase)
        .unwrap_or_default();
    if ext == "attheme" {
        return parse_attheme(&bytes, &name);
    }
    if bytes.starts_with(b"PK\x03\x04") {
        return parse_tdesktop_zip(&bytes, &name);
    }
    let text = String::from_utf8_lossy(&bytes);
    Ok(ImportedTheme {
        settings: parse_tdesktop_colors(&text, &name),
        wallpaper_bytes: None,
    })
}

/// Разбирает текстовый файл `colors.tdesktop-theme`.
pub fn parse_tdesktop_colors(text: &str, name: &str) -> CustomThemeSettings {
    let mut raw_map: HashMap<String, String> = HashMap::new();
    for line in text.lines() {
        let line = line.split("//").next().unwrap_or("").trim();
        if line.is_empty() || line.starts_with("/*") {
            continue;
        }
        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_owned();
            let val = v.trim().trim_end_matches(';').trim().to_owned();
            if !key.is_empty() && !val.is_empty() {
                raw_map.insert(key, val);
            }
        }
    }
    let resolve = |target: &str| -> Option<String> {
        let mut cur = target;
        for _ in 0..8 {
            let v = raw_map.get(cur)?;
            if v.starts_with('#') {
                return parse_hex_color(v).map(color_to_hex);
            }
            cur = v;
        }
        None
    };

    let mut out = CustomThemeSettings {
        active_theme_name: name.to_owned(),
        ..CustomThemeSettings::default()
    };
    if let Some(c) = resolve("windowBgActive").or_else(|| resolve("activeButtonBg")).or_else(|| resolve("dialogsUnreadBg")) {
        out.accent_hex = c;
    }
    if let Some(c) = resolve("windowBg") {
        out.window_bg_hex = c;
    }
    if let Some(c) = resolve("dialogsBg").or_else(|| resolve("topBarBg")) {
        out.sidebar_bg_hex = c;
    }
    if let Some(c) = resolve("historyComposeAreaBg").or_else(|| resolve("windowBg")) {
        out.chat_bg_hex = c;
    }
    if let Some(c) = resolve("msgInBg") {
        out.msg_in_bg_hex = c;
    }
    if let Some(c) = resolve("msgOutBg") {
        out.msg_out_bg_hex = c;
    }
    if let Some(c) = resolve("windowFg").or_else(|| resolve("historyTextInFg")) {
        out.text_primary_hex = c;
    }
    if let Some(c) = resolve("windowSubTextFg").or_else(|| resolve("dialogsTextFg")) {
        out.text_secondary_hex = c;
    }
    if let Some(c) = resolve("lightButtonBg").or_else(|| resolve("windowBgOver")) {
        out.button_bg_hex = c;
    }
    out
}

/// Разбирает ZIP-архив `.tdesktop-theme` (`colors.tdesktop-theme` + `background.*` / `tiled.*`).
pub fn parse_tdesktop_zip(bytes: &[u8], name: &str) -> Result<ImportedTheme, String> {
    let entries = extract_zip_entries(bytes)?;
    let colors_text = entries
        .iter()
        .find(|(n, _)| n.eq_ignore_ascii_case("colors.tdesktop-theme") || n.ends_with(".tdesktop-theme"))
        .map(|(_, d)| String::from_utf8_lossy(d).into_owned())
        .ok_or_else(|| "В архиве .tdesktop-theme не найден colors.tdesktop-theme".to_owned())?;

    let wallpaper_bytes = entries
        .into_iter()
        .find(|(n, d)| {
            let low = n.to_ascii_lowercase();
            !d.is_empty()
                && (low.starts_with("background.") || low.starts_with("tiled."))
                && (low.ends_with(".jpg") || low.ends_with(".jpeg") || low.ends_with(".png") || low.ends_with(".webp"))
        });

    Ok(ImportedTheme {
        settings: parse_tdesktop_colors(&colors_text, name),
        wallpaper_bytes,
    })
}

/// Разбирает формат Telegram Android / AyuGram `.attheme` (`key=ARGB` + опциональный `WPS\n...WPE`).
pub fn parse_attheme(bytes: &[u8], name: &str) -> Result<ImportedTheme, String> {
    let wps_marker = b"\nWPS\n";
    let (cfg_slice, wallpaper_bytes) = if let Some(pos) = bytes.windows(wps_marker.len()).position(|w| w == wps_marker) {
        let after = &bytes[pos + wps_marker.len()..];
        let wp_end = after
            .windows(4)
            .position(|w| w == b"\nWPE")
            .unwrap_or(after.len());
        let wp = &after[..wp_end];
        let ext = if wp.starts_with(b"\x89PNG") { "png" } else { "jpg" };
        let wp_opt = (!wp.is_empty()).then(|| (format!("wallpaper.{ext}"), wp.to_vec()));
        (&bytes[..pos], wp_opt)
    } else {
        (bytes, None)
    };

    let text = String::from_utf8_lossy(cfg_slice);
    let mut map: HashMap<String, Color32> = HashMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') || line.starts_with("//") {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let key = k.trim();
            let val = v.trim();
            if let Ok(argb) = val.parse::<i32>() {
                let u = argb as u32;
                let r = ((u >> 16) & 0xff) as u8;
                let g = ((u >> 8) & 0xff) as u8;
                let b = (u & 0xff) as u8;
                map.insert(key.to_owned(), Color32::from_rgb(r, g, b));
            } else if let Some(h) = val.strip_prefix('#')
                && h.len() == 8
                && let Ok(u) = u32::from_str_radix(h, 16)
            {
                let r = ((u >> 16) & 0xff) as u8;
                let g = ((u >> 8) & 0xff) as u8;
                let b = (u & 0xff) as u8;
                map.insert(key.to_owned(), Color32::from_rgb(r, g, b));
            } else if let Some(c) = parse_hex_color(val) {
                map.insert(key.to_owned(), c);
            }
        }
    }

    let mut out = CustomThemeSettings {
        active_theme_name: name.to_owned(),
        ..CustomThemeSettings::default()
    };
    if let Some(c) = map.get("chats_actionBackground").or_else(|| map.get("actionBarDefault")).or_else(|| map.get("windowBackgroundWhiteBlueText4")) {
        out.accent_hex = color_to_hex(*c);
    }
    if let Some(c) = map.get("windowBackgroundWhite").or_else(|| map.get("windowBackgroundGray")) {
        out.window_bg_hex = color_to_hex(*c);
    }
    if let Some(c) = map.get("chats_menuBackground").or_else(|| map.get("actionBarDefault")) {
        out.sidebar_bg_hex = color_to_hex(*c);
    }
    if let Some(c) = map.get("chat_wallpaper").or_else(|| map.get("windowBackgroundGray")) {
        out.chat_bg_hex = color_to_hex(*c);
    }
    if let Some(c) = map.get("chat_inBubble") {
        out.msg_in_bg_hex = color_to_hex(*c);
    }
    if let Some(c) = map.get("chat_outBubble") {
        out.msg_out_bg_hex = color_to_hex(*c);
    }
    if let Some(c) = map.get("windowBackgroundWhiteBlackText").or_else(|| map.get("chat_messageTextIn")) {
        out.text_primary_hex = color_to_hex(*c);
    }
    if let Some(c) = map.get("windowBackgroundWhiteGrayText").or_else(|| map.get("chats_message")) {
        out.text_secondary_hex = color_to_hex(*c);
    }
    Ok(ImportedTheme {
        settings: out,
        wallpaper_bytes,
    })
}

/// Экспортирует текущие цвета в формат `.tdesktop-theme`.
pub fn export_tdesktop_colors(s: &CustomThemeSettings) -> String {
    fn pick<'a>(hex: &'a str, fallback: &'a str) -> &'a str {
        let t = hex.trim();
        if t.is_empty() { fallback } else { t }
    }
    format!(
        "// Экспортировано из Rustgramm\n\
         windowBg: {};\n\
         dialogsBg: {};\n\
         historyComposeAreaBg: {};\n\
         msgInBg: {};\n\
         msgOutBg: {};\n\
         windowBgActive: {};\n\
         activeButtonBg: {};\n\
         windowFg: {};\n\
         windowSubTextFg: {};\n\
         lightButtonBg: {};\n",
        pick(&s.window_bg_hex, "#0E0E10"),
        pick(&s.sidebar_bg_hex, "#08080A"),
        pick(&s.chat_bg_hex, "#08080A"),
        pick(&s.msg_in_bg_hex, "#1A1A1E"),
        pick(&s.msg_out_bg_hex, "#38383F"),
        pick(&s.accent_hex, "#F58220"),
        pick(&s.accent_hex, "#F58220"),
        pick(&s.text_primary_hex, "#EEEEF1"),
        pick(&s.text_secondary_hex, "#86868E"),
        pick(&s.button_bg_hex, "#2E2E34"),
    )
}

/// Минимальный читатель ZIP-архивов для `.tdesktop-theme` (store и deflate через `flate2`).
fn extract_zip_entries(bytes: &[u8]) -> Result<Vec<(String, Vec<u8>)>, String> {
    let mut pos = 0usize;
    let mut out = Vec::new();
    while pos + 30 <= bytes.len() {
        if &bytes[pos..pos + 4] != b"PK\x03\x04" {
            break;
        }
        let flags = u16::from_le_bytes([bytes[pos + 6], bytes[pos + 7]]);
        let method = u16::from_le_bytes([bytes[pos + 8], bytes[pos + 9]]);
        let comp_size = u32::from_le_bytes([bytes[pos + 18], bytes[pos + 19], bytes[pos + 20], bytes[pos + 21]]) as usize;
        let name_len = u16::from_le_bytes([bytes[pos + 26], bytes[pos + 27]]) as usize;
        let extra_len = u16::from_le_bytes([bytes[pos + 28], bytes[pos + 29]]) as usize;
        let hdr_end = pos + 30 + name_len + extra_len;
        if hdr_end > bytes.len() || (flags & 0x08) != 0 && comp_size == 0 {
            break;
        }
        let name = String::from_utf8_lossy(&bytes[pos + 30..pos + 30 + name_len]).into_owned();
        if hdr_end + comp_size > bytes.len() || comp_size > 32 * 1024 * 1024 {
            break;
        }
        let raw = &bytes[hdr_end..hdr_end + comp_size];
        let data = match method {
            0 => raw.to_vec(),
            8 => {
                let mut dec = DeflateDecoder::new(raw);
                let mut buf = Vec::new();
                dec.read_to_end(&mut buf).map_err(|e| format!("Deflate ZIP: {e}"))?;
                buf
            }
            _ => Vec::new(),
        };
        out.push((name, data));
        pos = hdr_end + comp_size;
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_tdesktop_theme_references_and_comments() {
        let src = r#"
            // comment
            brandOrange: #f58220;
            windowBgActive: brandOrange;
            windowBg: #101214ff;
            msgInBg: #1f2228;
            msgOutBg: #34281f;
        "#;
        let s = parse_tdesktop_colors(src, "MyTheme");
        assert_eq!(s.active_theme_name, "MyTheme");
        assert_eq!(s.accent_hex, "#F58220");
        assert_eq!(s.window_bg_hex, "#101214");
        assert_eq!(s.msg_in_bg_hex, "#1F2228");
        assert_eq!(s.msg_out_bg_hex, "#34281F");
    }

    #[test]
    fn parses_attheme_signed_ints_and_wps() {
        let mut raw = b"chats_actionBackground=-687584\nchat_inBubble=-14606046\nchat_outBubble=#FF332211\nWPS\n\x89PNGfakepng\nWPE".to_vec();
        let imp = parse_attheme(&raw, "AyuAndroid").unwrap();
        assert_eq!(imp.settings.active_theme_name, "AyuAndroid");
        assert_eq!(imp.settings.accent_hex, "#F58220");
        assert_eq!(imp.settings.msg_out_bg_hex, "#332211");
        assert!(imp.wallpaper_bytes.is_some());
        raw.clear();
    }

    #[test]
    fn export_and_reimport_roundtrips() {
        let s = CustomThemeSettings {
            accent_hex: "#FF6600".to_owned(),
            window_bg_hex: "#111215".to_owned(),
            ..Default::default()
        };
        let txt = export_tdesktop_colors(&s);
        let re = parse_tdesktop_colors(&txt, "Exported");
        assert_eq!(re.accent_hex, "#FF6600");
        assert_eq!(re.window_bg_hex, "#111215");
    }
}
