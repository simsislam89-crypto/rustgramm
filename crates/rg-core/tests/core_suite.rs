//! Полный набор модульных и характеристических тестов `rg-core`
//! (пересчёт UTF-16 сущностей `convert_entities`, фильтрация папок `folder_matches`,
//! метаданные музыки `AudioTrack`, безопасные имена файлов `safe_file_name`,
//! размеры `human_size`, MIME-типы, пятибитная волна голосовых `decode_waveform`).

use std::path::Path;

use grammers_client::tl;
use grammers_session::types::{PeerAuth, PeerId, PeerRef};
use rg_core::engine::{
    AudioTrack, DialogKind, DialogRow, EntityKind, FolderRow, NotifyScope, PrivacyKeyKind,
    PrivacyLevel, convert_entities, folder_matches,
};
use rg_core::media::{
    MediaInfo, MediaKind, decode_waveform, ext_for_mime, human_size, is_safe_to_open,
    safe_file_name,
};

fn dummy_dialog(
    id: i64,
    kind: DialogKind,
    contact: bool,
    muted: bool,
    unread: i32,
    archived: bool,
) -> DialogRow {
    DialogRow {
        peer: PeerRef {
            id: PeerId::user_unchecked(id.abs().max(1)),
            auth: PeerAuth::default(),
        },
        id,
        name: format!("Chat {id}"),
        last: String::new(),
        date: 1_763_700_000,
        unread,
        pinned: false,
        muted,
        notify_explicit: None,
        saved: false,
        archived,
        kind,
        contact,
        read_out: 0,
        emoji_status: None,
        verified: false,
        premium: false,
    }
}

macro_rules! folder_tests {
    ($($name:ident: ($c:expr, $nc:expr, $g:expr, $b:expr, $bot:expr, $em:expr, $er:expr, $ea:expr, $inc:expr, $exc:expr, $id:expr, $kind:expr, $is_c:expr, $mut:expr, $unr:expr, $arch:expr) => $exp:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let f = FolderRow {
                    id: 1,
                    title: "Тест".to_owned(),
                    contacts: $c,
                    non_contacts: $nc,
                    groups: $g,
                    broadcasts: $b,
                    bots: $bot,
                    exclude_muted: $em,
                    exclude_read: $er,
                    exclude_archived: $ea,
                    include: $inc.to_vec(),
                    exclude: $exc.to_vec(),
                };
                let d = dummy_dialog($id, $kind, $is_c, $mut, $unr, $arch);
                assert_eq!(folder_matches(&f, &d), $exp);
            }
        )+
    };
}

folder_tests! {
    fld_001: (true, false, false, false, false, false, false, false, &[], &[], 10, DialogKind::User, true, false, 1, false) => true,
    fld_002: (true, false, false, false, false, false, false, false, &[], &[], 10, DialogKind::User, false, false, 1, false) => false,
    fld_003: (false, true, false, false, false, false, false, false, &[], &[], 10, DialogKind::User, false, false, 1, false) => true,
    fld_004: (false, true, false, false, false, false, false, false, &[], &[], 10, DialogKind::User, true, false, 1, false) => false,
    fld_005: (false, false, true, false, false, false, false, false, &[], &[], -10, DialogKind::Group, false, false, 1, false) => true,
    fld_006: (false, false, true, false, false, false, false, false, &[], &[], -10, DialogKind::Channel, false, false, 1, false) => false,
    fld_007: (false, false, false, true, false, false, false, false, &[], &[], -10, DialogKind::Channel, false, false, 1, false) => true,
    fld_008: (false, false, false, true, false, false, false, false, &[], &[], -10, DialogKind::Group, false, false, 1, false) => false,
    fld_009: (false, false, false, false, true, false, false, false, &[], &[], 20, DialogKind::Bot, false, false, 1, false) => true,
    fld_010: (false, false, false, false, true, false, false, false, &[], &[], 20, DialogKind::User, true, false, 1, false) => false,
    fld_011: (true, false, false, false, false, true, false, false, &[], &[], 10, DialogKind::User, true, true, 1, false) => false,
    fld_012: (true, false, false, false, false, true, false, false, &[], &[], 10, DialogKind::User, true, false, 1, false) => true,
    fld_013: (true, false, false, false, false, false, true, false, &[], &[], 10, DialogKind::User, true, false, 0, false) => false,
    fld_014: (true, false, false, false, false, false, true, false, &[], &[], 10, DialogKind::User, true, false, 3, false) => true,
    fld_015: (true, false, false, false, false, false, false, true, &[], &[], 10, DialogKind::User, true, false, 1, true) => false,
    fld_016: (true, false, false, false, false, false, false, true, &[], &[], 10, DialogKind::User, true, false, 1, false) => true,
    fld_017: (false, false, false, false, false, true, true, true, &[77], &[], 77, DialogKind::Channel, false, true, 0, true) => true,
    fld_018: (true, true, true, true, true, false, false, false, &[], &[77], 77, DialogKind::User, true, false, 5, false) => false,
    fld_019: (true, true, true, true, true, false, false, false, &[77], &[77], 77, DialogKind::User, true, false, 5, false) => false,
    fld_020: (true, true, false, false, false, false, false, false, &[], &[], 10, DialogKind::User, true, false, 0, false) => true,
    fld_021: (true, true, false, false, false, false, false, false, &[], &[], 10, DialogKind::User, false, false, 0, false) => true,
    fld_022: (true, true, false, false, false, false, false, false, &[], &[], 10, DialogKind::Bot, false, false, 0, false) => false,
    fld_023: (false, false, true, true, false, false, false, false, &[], &[], -5, DialogKind::Group, false, false, 0, false) => true,
    fld_024: (false, false, true, true, false, false, false, false, &[], &[], -5, DialogKind::Channel, false, false, 0, false) => true,
    fld_025: (false, false, true, true, false, false, false, false, &[], &[], 5, DialogKind::User, true, false, 0, false) => false,
    fld_026: (true, false, true, false, false, true, true, true, &[], &[], 10, DialogKind::User, true, false, 2, false) => true,
    fld_027: (true, false, true, false, false, true, true, true, &[], &[], 10, DialogKind::User, true, true, 2, false) => false,
    fld_028: (true, false, true, false, false, true, true, true, &[], &[], 10, DialogKind::User, true, false, 0, false) => false,
    fld_029: (true, false, true, false, false, true, true, true, &[], &[], 10, DialogKind::User, true, false, 2, true) => false,
    fld_030: (true, false, true, false, false, true, true, true, &[], &[], -10, DialogKind::Group, false, false, 4, false) => true,
    fld_031: (true, false, true, false, false, true, true, true, &[], &[], -10, DialogKind::Group, false, true, 4, false) => false,
    fld_032: (true, false, true, false, false, true, true, true, &[], &[], -10, DialogKind::Group, false, false, 0, false) => false,
    fld_033: (true, false, true, false, false, true, true, true, &[], &[], -10, DialogKind::Group, false, false, 4, true) => false,
    fld_034: (false, false, false, false, false, false, false, false, &[1, 2, 3], &[], 2, DialogKind::Bot, false, false, 0, false) => true,
    fld_035: (false, false, false, false, false, false, false, false, &[1, 2, 3], &[], 4, DialogKind::Bot, false, false, 0, false) => false,
    fld_036: (false, true, false, false, true, false, true, false, &[], &[], 50, DialogKind::Bot, false, false, 1, false) => true,
    fld_037: (false, true, false, false, true, false, true, false, &[], &[], 50, DialogKind::Bot, false, false, 0, false) => false,
    fld_038: (false, true, false, false, true, false, true, false, &[], &[], 50, DialogKind::User, false, false, 2, false) => true,
    fld_039: (false, true, false, false, true, false, true, false, &[], &[], 50, DialogKind::User, true, false, 2, false) => false,
    fld_040: (false, false, false, true, false, true, false, true, &[], &[], -100, DialogKind::Channel, false, false, 0, false) => true,
    fld_041: (false, false, false, true, false, true, false, true, &[], &[], -100, DialogKind::Channel, false, true, 0, false) => false,
    fld_042: (false, false, false, true, false, true, false, true, &[], &[], -100, DialogKind::Channel, false, false, 0, true) => false,
    fld_043: (true, true, true, true, true, false, false, false, &[], &[], 1, DialogKind::User, true, false, 0, false) => true,
    fld_044: (true, true, true, true, true, false, false, false, &[], &[], 2, DialogKind::User, false, false, 0, false) => true,
    fld_045: (true, true, true, true, true, false, false, false, &[], &[], 3, DialogKind::Bot, false, false, 0, false) => true,
    fld_046: (true, true, true, true, true, false, false, false, &[], &[], -4, DialogKind::Group, false, false, 0, false) => true,
    fld_047: (true, true, true, true, true, false, false, false, &[], &[], -5, DialogKind::Channel, false, false, 0, false) => true,
    fld_048: (false, false, false, false, false, false, false, false, &[], &[], 1, DialogKind::User, true, false, 1, false) => false,
    fld_049: (false, false, false, false, false, false, false, false, &[], &[], -1, DialogKind::Group, false, false, 1, false) => false,
    fld_050: (false, false, false, false, false, false, false, false, &[], &[], -2, DialogKind::Channel, false, false, 1, false) => false,
}

macro_rules! human_size_tests {
    ($($name:ident: $bytes:expr => $expected:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(human_size($bytes), $expected);
            }
        )+
    };
}

human_size_tests! {
    hsz_001: 0 => "0 Б",
    hsz_002: 1 => "1 Б",
    hsz_003: 10 => "10 Б",
    hsz_004: 100 => "100 Б",
    hsz_005: 512 => "512 Б",
    hsz_006: 1023 => "1023 Б",
    hsz_007: 1024 => "1,0 КБ",
    hsz_008: 1536 => "1,5 КБ",
    hsz_009: 2048 => "2,0 КБ",
    hsz_010: 4096 => "4,0 КБ",
    hsz_011: 8192 => "8,0 КБ",
    hsz_012: 10240 => "10,0 КБ",
    hsz_013: 16384 => "16,0 КБ",
    hsz_014: 32768 => "32,0 КБ",
    hsz_015: 65536 => "64,0 КБ",
    hsz_016: 131072 => "128,0 КБ",
    hsz_017: 262144 => "256,0 КБ",
    hsz_018: 524288 => "512,0 КБ",
    hsz_019: 1048576 => "1,0 МБ",
    hsz_020: 1572864 => "1,5 МБ",
    hsz_021: 2097152 => "2,0 МБ",
    hsz_022: 3145728 => "3,0 МБ",
    hsz_023: 4194304 => "4,0 МБ",
    hsz_024: 5242880 => "5,0 МБ",
    hsz_025: 10485760 => "10,0 МБ",
    hsz_026: 12111052 => "11,5 МБ",
    hsz_027: 16777216 => "16,0 МБ",
    hsz_028: 33554432 => "32,0 МБ",
    hsz_029: 67108864 => "64,0 МБ",
    hsz_030: 134217728 => "128,0 МБ",
    hsz_031: 268435456 => "256,0 МБ",
    hsz_032: 536870912 => "512,0 МБ",
    hsz_033: 1073741824 => "1,0 ГБ",
    hsz_034: 1610612736 => "1,5 ГБ",
    hsz_035: 2147483648 => "2,0 ГБ",
    hsz_036: 3221225472 => "3,0 ГБ",
    hsz_037: 4294967296 => "4,0 ГБ",
    hsz_038: 5368709120 => "5,0 ГБ",
    hsz_039: 10737418240 => "10,0 ГБ",
    hsz_040: 21474836480 => "20,0 ГБ",
}

macro_rules! safe_name_tests {
    ($($name:ident: $in:expr => $out:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(safe_file_name($in), $out);
            }
        )+
    };
}

safe_name_tests! {
    sfn_001: "photo.jpg" => "photo.jpg",
    sfn_002: "track.mp3" => "track.mp3",
    sfn_003: "video.mp4" => "video.mp4",
    sfn_004: "отчёт 2026.pdf" => "отчёт 2026.pdf",
    sfn_005: "..\\..\\Windows\\System32\\cmd.exe" => "cmd.exe",
    sfn_006: "../../../etc/shadow" => "shadow",
    sfn_007: "C:\\Users\\admin\\secret.txt" => "secret.txt",
    sfn_008: "/var/log/syslog" => "syslog",
    sfn_009: "CON" => "file_CON",
    sfn_010: "con.txt" => "file_con.txt",
    sfn_011: "PRN" => "file_PRN",
    sfn_012: "prn.log" => "file_prn.log",
    sfn_013: "AUX" => "file_AUX",
    sfn_014: "aux.bin" => "file_aux.bin",
    sfn_015: "NUL" => "file_NUL",
    sfn_016: "nul.dat" => "file_nul.dat",
    sfn_017: "COM1" => "file_COM1",
    sfn_018: "COM2.txt" => "file_COM2.txt",
    sfn_019: "COM3" => "file_COM3",
    sfn_020: "COM4" => "file_COM4",
    sfn_021: "COM5" => "file_COM5",
    sfn_022: "COM6" => "file_COM6",
    sfn_023: "COM7" => "file_COM7",
    sfn_024: "COM8" => "file_COM8",
    sfn_025: "COM9" => "file_COM9",
    sfn_026: "LPT1" => "file_LPT1",
    sfn_027: "LPT2.doc" => "file_LPT2.doc",
    sfn_028: "LPT3" => "file_LPT3",
    sfn_029: "LPT4" => "file_LPT4",
    sfn_030: "LPT5" => "file_LPT5",
    sfn_031: "LPT6" => "file_LPT6",
    sfn_032: "LPT7" => "file_LPT7",
    sfn_033: "LPT8" => "file_LPT8",
    sfn_034: "LPT9" => "file_LPT9",
    sfn_035: "" => "file_",
    sfn_036: "..." => "file_",
    sfn_037: "   " => "file_",
    sfn_038: ". . ." => "file_",
    sfn_039: "a<b>c:d\"e|f?g*.txt" => "a_b_c_d_e_f_g_.txt",
    sfn_040: "line1\nline2\rline3\0.txt" => "line1_line2_line3_.txt",
}

macro_rules! safe_open_tests {
    ($($name:ident: $path:expr => $safe:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(is_safe_to_open(Path::new($path)), $safe);
            }
        )+
    };
}

safe_open_tests! {
    sop_001: "video.mp4" => true,
    sop_002: "video.MP4" => true,
    sop_003: "movie.mkv" => true,
    sop_004: "clip.webm" => true,
    sop_005: "apple.mov" => true,
    sop_006: "old.avi" => true,
    sop_007: "itunes.m4v" => true,
    sop_008: "song.mp3" => true,
    sop_009: "voice.ogg" => true,
    sop_010: "audio.oga" => true,
    sop_011: "note.opus" => true,
    sop_012: "track.m4a" => true,
    sop_013: "pcm.wav" => true,
    sop_014: "lossless.flac" => true,
    sop_015: "photo.jpg" => true,
    sop_016: "photo.jpeg" => true,
    sop_017: "shot.png" => true,
    sop_018: "anim.gif" => true,
    sop_019: "sticker.webp" => true,
    sop_020: "bitmap.bmp" => true,
    sop_021: "doc.pdf" => true,
    sop_022: "readme.txt" => true,
    sop_023: "trojan.exe" => false,
    sop_024: "script.bat" => false,
    sop_025: "script.cmd" => false,
    sop_026: "power.ps1" => false,
    sop_027: "macro.vbs" => false,
    sop_028: "code.js" => false,
    sop_029: "installer.msi" => false,
    sop_030: "screensaver.scr" => false,
    sop_031: "library.dll" => false,
    sop_032: "shortcut.lnk" => false,
    sop_033: "archive.zip" => false,
    sop_034: "archive.rar" => false,
    sop_035: "archive.7z" => false,
    sop_036: "page.html" => false,
    sop_037: "page.htm" => false,
    sop_038: "app.apk" => false,
    sop_039: "shell.sh" => false,
    sop_040: "no_extension" => false,
}

macro_rules! audio_track_tests {
    ($($name:ident: ($perf:expr, $title:expr, $fname:expr, $mime:expr) => ($disp:expr, $ext:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let t = AudioTrack {
                    id: 1,
                    access_hash: 2,
                    file_ref: vec![],
                    performer: $perf.to_owned(),
                    title: $title.to_owned(),
                    file_name: $fname.to_owned(),
                    mime: $mime.to_owned(),
                    duration: 180,
                    size: 4096,
                };
                assert_eq!(t.display_title(), $disp);
                assert_eq!(t.ext(), $ext);
            }
        )+
    };
}

audio_track_tests! {
    atr_001: ("Kinneret", "No Wind Resistance!", "Kinneret.m4a", "audio/mp4") => ("Kinneret — No Wind Resistance!", "m4a"),
    atr_002: ("", "No Wind Resistance!", "Kinneret.m4a", "audio/mp4") => ("No Wind Resistance!", "m4a"),
    atr_003: ("Kinneret", "", "Kinneret.m4a", "audio/mp4") => ("Kinneret", "m4a"),
    atr_004: ("", "", "Kinneret - No Wind Resistance!.m4a", "audio/mp4") => ("Kinneret - No Wind Resistance!.m4a", "m4a"),
    atr_005: ("", "", "", "audio/mpeg") => ("Аудиозапись", "mp3"),
    atr_006: ("Artist", "Song", "song.mp3", "audio/mpeg") => ("Artist — Song", "mp3"),
    atr_007: ("Artist", "Song", "song.flac", "audio/flac") => ("Artist — Song", "flac"),
    atr_008: ("Artist", "Song", "song.ogg", "audio/ogg") => ("Artist — Song", "ogg"),
    atr_009: ("Artist", "Song", "song.opus", "audio/opus") => ("Artist — Song", "opus"),
    atr_010: ("Artist", "Song", "song.wav", "audio/wav") => ("Artist — Song", "wav"),
    atr_011: ("Artist", "Song", "", "audio/mp4") => ("Artist — Song", "m4a"),
    atr_012: ("Artist", "Song", "", "audio/x-m4a") => ("Artist — Song", "m4a"),
    atr_013: ("Artist", "Song", "", "audio/m4a") => ("Artist — Song", "m4a"),
    atr_014: ("Artist", "Song", "", "audio/ogg") => ("Artist — Song", "ogg"),
    atr_015: ("Artist", "Song", "", "audio/opus") => ("Artist — Song", "ogg"),
    atr_016: ("Artist", "Song", "", "audio/flac") => ("Artist — Song", "flac"),
    atr_017: ("Artist", "Song", "", "audio/wav") => ("Artist — Song", "wav"),
    atr_018: ("Artist", "Song", "", "audio/x-wav") => ("Artist — Song", "wav"),
    atr_019: ("  Киннерет  ", "  Трек  ", "a.mp3", "audio/mpeg") => ("Киннерет — Трек", "mp3"),
    atr_020: ("   ", "   ", "  demo.aac  ", "audio/aac") => ("demo.aac", "mp3"),
}

macro_rules! entity_utf16_tests {
    ($($name:ident: ($text:expr, $off16:expr, $len16:expr) => ($char_start:expr, $char_end:expr, $slice:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let raw = [tl::enums::MessageEntity::Bold(tl::types::MessageEntityBold {
                    offset: $off16,
                    length: $len16,
                })];
                let ents = convert_entities($text, &raw);
                assert_eq!(ents.len(), 1);
                assert_eq!(ents[0].kind, EntityKind::Bold);
                assert_eq!((ents[0].start, ents[0].end), ($char_start, $char_end));
                let extracted: String = $text.chars().skip(ents[0].start).take(ents[0].end - ents[0].start).collect();
                assert_eq!(extracted, $slice);
            }
        )+
    };
}

entity_utf16_tests! {
    ent_001: ("Привет мир", 0, 6) => (0, 6, "Привет"),
    ent_002: ("Привет мир", 7, 3) => (7, 10, "мир"),
    ent_003: ("🔥 Привет", 3, 6) => (2, 8, "Привет"),
    ent_004: ("🔥🔥 Тест", 5, 4) => (3, 7, "Тест"),
    ent_005: ("🔥🔥🔥 Rust", 7, 4) => (4, 8, "Rust"),
    ent_006: ("A😀B😀C", 0, 1) => (0, 1, "A"),
    ent_007: ("A😀B😀C", 1, 2) => (1, 2, "😀"),
    ent_008: ("A😀B😀C", 3, 1) => (2, 3, "B"),
    ent_009: ("A😀B😀C", 4, 2) => (3, 4, "😀"),
    ent_010: ("A😀B😀C", 6, 1) => (4, 5, "C"),
    ent_011: ("🎉🎊🎈 Праздник", 7, 8) => (4, 12, "Праздник"),
    ent_012: ("Код: 123456", 5, 6) => (5, 11, "123456"),
    ent_013: ("Кириллица и ASCII", 0, 9) => (0, 9, "Кириллица"),
    ent_014: ("Кириллица и ASCII", 12, 5) => (12, 17, "ASCII"),
    ent_015: ("日本語テスト", 0, 3) => (0, 3, "日本語"),
    ent_016: ("日本語テスト", 3, 3) => (3, 6, "テスト"),
    ent_017: ("🚀日本語🔥", 2, 3) => (1, 4, "日本語"),
    ent_018: ("🚀日本語🔥", 5, 2) => (4, 5, "🔥"),
    ent_019: ("💎 Premium статус", 3, 7) => (2, 9, "Premium"),
    ent_020: ("💎 Premium статус", 11, 6) => (10, 16, "статус"),
    ent_021: ("⭐ Звёзды Telegram", 0, 1) => (0, 1, "⭐"),
    ent_022: ("⭐ Звёзды Telegram", 2, 6) => (2, 8, "Звёзды"),
    ent_023: ("⭐ Звёзды Telegram", 9, 8) => (9, 17, "Telegram"),
    ent_024: ("❤️ Любовь", 3, 6) => (3, 9, "Любовь"),
    ent_025: ("👍🏻 Одобрено", 5, 8) => (3, 11, "Одобрено"),
    ent_026: ("🇷🇺 Россия", 5, 6) => (3, 9, "Россия"),
    ent_027: ("1️⃣ Первый пункт", 4, 6) => (4, 10, "Первый"),
    ent_028: ("🎶 Музыка в профиле", 3, 6) => (2, 8, "Музыка"),
    ent_029: ("🎶 Музыка в профиле", 12, 7) => (11, 18, "профиле"),
    ent_030: ("🦀 Rustgramm v0.13.0", 3, 9) => (2, 11, "Rustgramm"),
}

macro_rules! mime_ext_tests {
    ($($name:ident: $mime:expr => $ext:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(ext_for_mime($mime), $ext);
            }
        )+
    };
}

mime_ext_tests! {
    mext_001: "audio/ogg" => "ogg",
    mext_002: "audio/opus" => "ogg",
    mext_003: "audio/mpeg" => "mp3",
    mext_004: "audio/mp4" => "m4a",
    mext_005: "audio/m4a" => "m4a",
    mext_006: "audio/wav" => "wav",
    mext_007: "audio/x-wav" => "wav",
    mext_008: "video/mp4" => "mp4",
    mext_009: "video/webm" => "webm",
    mext_010: "video/quicktime" => "mov",
    mext_011: "image/gif" => "gif",
    mext_012: "image/jpeg" => "jpg",
    mext_013: "image/png" => "png",
    mext_014: "image/webp" => "webp",
    mext_015: "application/pdf" => "pdf",
    mext_016: "application/octet-stream" => "bin",
    mext_017: "text/plain" => "bin",
    mext_018: "" => "bin",
    mext_019: "unknown/type" => "bin",
    mext_020: "application/x-tgsticker" => "bin",
}

macro_rules! waveform_tests {
    ($($name:ident: $val:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let v: u8 = $val & 0x1f;
                let bytes = [v, 0, 0, 0, 0];
                let decoded = decode_waveform(&bytes);
                assert_eq!(decoded.len(), 8);
                assert_eq!(decoded[0], v);
            }
        )+
    };
}

waveform_tests! {
    wf_00: 0, wf_01: 1, wf_02: 2, wf_03: 3, wf_04: 4, wf_05: 5, wf_06: 6, wf_07: 7,
    wf_08: 8, wf_09: 9, wf_10: 10, wf_11: 11, wf_12: 12, wf_13: 13, wf_14: 14, wf_15: 15,
    wf_16: 16, wf_17: 17, wf_18: 18, wf_19: 19, wf_20: 20, wf_21: 21, wf_22: 22, wf_23: 23,
    wf_24: 24, wf_25: 25, wf_26: 26, wf_27: 27, wf_28: 28, wf_29: 29, wf_30: 30, wf_31: 31,
}

#[test]
fn privacy_and_notify_enums_coverage() {
    for k in PrivacyKeyKind::ALL {
        assert!(!k.title().is_empty());
    }
    for l in [PrivacyLevel::Everybody, PrivacyLevel::Contacts, PrivacyLevel::Nobody] {
        assert!(!l.label().is_empty());
    }
    let scopes = [NotifyScope::Users, NotifyScope::Chats, NotifyScope::Broadcasts];
    assert_eq!(scopes.len(), 3);
}

#[test]
fn media_kind_preview_and_downloadable_matrix() {
    let kinds = [
        (MediaKind::Photo, false, true, true),
        (MediaKind::Sticker, false, true, true),
        (MediaKind::AnimatedSticker, false, false, true),
        (MediaKind::AnimatedSticker, true, true, true),
        (MediaKind::Video, false, false, true),
        (MediaKind::Video, true, true, true),
        (MediaKind::Gif, false, false, true),
        (MediaKind::Gif, true, true, true),
        (MediaKind::Round, false, false, true),
        (MediaKind::Round, true, true, true),
        (MediaKind::Voice, true, false, true),
        (MediaKind::Audio, true, false, true),
        (MediaKind::File, true, false, true),
        (MediaKind::Other("x".into()), true, false, false),
    ];
    for (kind, has_thumb, exp_preview, exp_dl) in kinds {
        let mut m = MediaInfo::basic(kind, "item", 100);
        m.has_thumb = has_thumb;
        assert_eq!(m.has_preview(), exp_preview);
        assert_eq!(m.is_downloadable(), exp_dl);
    }
}
