//! Полный набор модульных и характеристических тестов `rg-settings`
//! (`Settings`, `AyuSettings`, `ExteraSettings`, `GhostMode`, `NotifySettings`,
//! `LanguageSettings`, `SecuritySettings`, `hash_passcode`, JSON-миграции).

use rg_settings::{
    AyuSettings, ChannelBottomButton, Filter, LanguageSettings, MenuVisibility, NotifySettings,
    PeerIdDisplay, SecuritySettings, SendWithoutSound, Settings, TranslationProvider, WorkMode,
    hash_passcode,
};

macro_rules! passcode_tests {
    ($($name:ident: ($pin_a:expr, $pin_b:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let ha1 = hash_passcode($pin_a);
                let ha2 = hash_passcode($pin_a);
                let hb = hash_passcode($pin_b);
                assert_eq!(ha1.len(), 32);
                assert_eq!(ha1, ha2, "hash must be deterministic");
                assert_ne!(ha1, hb, "distinct passcodes {} and {} must not collide", $pin_a, $pin_b);
            }
        )+
    };
}

passcode_tests! {
    pin_001: ("0000", "0001"),
    pin_002: ("0001", "0002"),
    pin_003: ("0002", "0003"),
    pin_004: ("0003", "0004"),
    pin_005: ("0004", "0005"),
    pin_006: ("0005", "0006"),
    pin_007: ("0006", "0007"),
    pin_008: ("0007", "0008"),
    pin_009: ("0008", "0009"),
    pin_010: ("0009", "0010"),
    pin_011: ("1111", "1112"),
    pin_012: ("1234", "4321"),
    pin_013: ("2580", "0852"),
    pin_014: ("9999", "9998"),
    pin_015: ("1337", "7331"),
    pin_016: ("2026", "2027"),
    pin_017: ("7777", "7776"),
    pin_018: ("8888", "8889"),
    pin_019: ("4242", "2424"),
    pin_020: ("1010", "0101"),
    pin_021: ("alpha", "Alpha"),
    pin_022: ("rustgramm", "Rustgramm"),
    pin_023: ("ayugram", "AyuGram"),
    pin_024: ("exteragram", "exteraGram"),
    pin_025: ("пароль1", "пароль2"),
    pin_026: ("секрет", "Секрет"),
    pin_027: ("123456", "1234567"),
    pin_028: ("abcdef", "abcdeg"),
    pin_029: ("!@#$%^", "!@#$%&"),
    pin_030: ("   ", "    "),
    pin_031: ("01", "001"),
    pin_032: ("001", "0001"),
    pin_033: ("0001", "00001"),
    pin_034: ("a", "b"),
    pin_035: ("b", "c"),
    pin_036: ("c", "d"),
    pin_037: ("d", "e"),
    pin_038: ("e", "f"),
    pin_039: ("f", "g"),
    pin_040: ("g", "h"),
    pin_041: ("h", "i"),
    pin_042: ("i", "j"),
    pin_043: ("j", "k"),
    pin_044: ("k", "l"),
    pin_045: ("l", "m"),
    pin_046: ("m", "n"),
    pin_047: ("n", "o"),
    pin_048: ("o", "p"),
    pin_049: ("p", "q"),
    pin_050: ("q", "r"),
    pin_051: ("r", "s"),
    pin_052: ("s", "t"),
    pin_053: ("t", "u"),
    pin_054: ("u", "v"),
    pin_055: ("v", "w"),
    pin_056: ("w", "x"),
    pin_057: ("x", "y"),
    pin_058: ("y", "z"),
    pin_059: ("z", "0"),
    pin_060: ("100", "101"),
    pin_061: ("102", "103"),
    pin_062: ("104", "105"),
    pin_063: ("106", "107"),
    pin_064: ("108", "109"),
    pin_065: ("110", "111"),
    pin_066: ("112", "113"),
    pin_067: ("114", "115"),
    pin_068: ("116", "117"),
    pin_069: ("118", "119"),
    pin_070: ("120", "121"),
    pin_071: ("122", "123"),
    pin_072: ("124", "125"),
    pin_073: ("126", "127"),
    pin_074: ("128", "129"),
    pin_075: ("130", "131"),
    pin_076: ("132", "133"),
    pin_077: ("134", "135"),
    pin_078: ("136", "137"),
    pin_079: ("138", "139"),
    pin_080: ("140", "141"),
    pin_081: ("142", "143"),
    pin_082: ("144", "145"),
    pin_083: ("146", "147"),
    pin_084: ("148", "149"),
    pin_085: ("150", "151"),
    pin_086: ("152", "153"),
    pin_087: ("154", "155"),
    pin_088: ("156", "157"),
    pin_089: ("158", "159"),
    pin_090: ("160", "161"),
    pin_091: ("162", "163"),
    pin_092: ("164", "165"),
    pin_093: ("166", "167"),
    pin_094: ("168", "169"),
    pin_095: ("170", "171"),
    pin_096: ("172", "173"),
    pin_097: ("174", "175"),
    pin_098: ("176", "177"),
    pin_099: ("178", "179"),
    pin_100: ("180", "181"),
}

macro_rules! opacity_roundtrip_tests {
    ($($name:ident: $op:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let s = Settings {
                    window_opacity: $op,
                    blur_enabled: $op < 100,
                    ..Settings::default()
                };
                let json = s.to_json();
                let loaded = Settings::from_json(&json).expect("valid settings json");
                assert_eq!(loaded.window_opacity, $op);
                assert_eq!(loaded.blur_enabled, $op < 100);
            }
        )+
    };
}

opacity_roundtrip_tests! {
    op_015: 15, op_016: 16, op_017: 17, op_018: 18, op_019: 19,
    op_020: 20, op_021: 21, op_022: 22, op_023: 23, op_024: 24,
    op_025: 25, op_026: 26, op_027: 27, op_028: 28, op_029: 29,
    op_030: 30, op_031: 31, op_032: 32, op_033: 33, op_034: 34,
    op_035: 35, op_036: 36, op_037: 37, op_038: 38, op_039: 39,
    op_040: 40, op_041: 41, op_042: 42, op_043: 43, op_044: 44,
    op_045: 45, op_046: 46, op_047: 47, op_048: 48, op_049: 49,
    op_050: 50, op_051: 51, op_052: 52, op_053: 53, op_054: 54,
    op_055: 55, op_056: 56, op_057: 57, op_058: 58, op_059: 59,
    op_060: 60, op_061: 61, op_062: 62, op_063: 63, op_064: 64,
    op_065: 65, op_066: 66, op_067: 67, op_068: 68, op_069: 69,
    op_070: 70, op_071: 71, op_072: 72, op_073: 73, op_074: 74,
    op_075: 75, op_076: 76, op_077: 77, op_078: 78, op_079: 79,
    op_080: 80, op_081: 81, op_082: 82, op_083: 83, op_084: 84,
    op_085: 85, op_086: 86, op_087: 87, op_088: 88, op_089: 89,
    op_090: 90, op_091: 91, op_092: 92, op_093: 93, op_094: 94,
    op_095: 95, op_096: 96, op_097: 97, op_098: 98, op_099: 99,
    op_100: 100,
}

macro_rules! ayu_bool_field_tests {
    ($($name:ident: $field:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let mut s = Settings::default();
                let initial = s.ayu.$field;
                s.ayu.$field = !initial;
                let json = s.to_json();
                let loaded = Settings::from_json(&json).unwrap();
                assert_eq!(loaded.ayu.$field, !initial);
            }
        )+
    };
}

ayu_bool_field_tests! {
    ayu_f_001: show_background,
    ayu_f_002: show_date,
    ayu_f_003: show_reactions,
    ayu_f_004: show_header_decorations,
    ayu_f_005: show_colorful_replies,
    ayu_f_006: reveal_spoilers,
    ayu_f_007: semi_transparent_deleted_messages,
    ayu_f_008: disable_custom_backgrounds,
    ayu_f_009: material_switches,
    ayu_f_010: remove_message_tail,
    ayu_f_011: single_corner_radius,
    ayu_f_012: simple_quotes_and_replies,
    ayu_f_013: replace_bottom_info_with_icons,
    ayu_f_014: adaptive_cover_color,
    ayu_f_015: improve_link_previews,
    ayu_f_016: show_message_seconds,
    ayu_f_017: save_deleted_messages,
    ayu_f_018: save_messages_history,
    ayu_f_019: save_for_bots,
    ayu_f_020: filters_enabled,
    ayu_f_021: filters_enabled_in_chats,
    ayu_f_022: hide_from_blocked,
    ayu_f_023: filter_zalgo,
    ayu_f_024: disable_ads,
    ayu_f_025: disable_stories,
    ayu_f_026: hide_premium_statuses,
    ayu_f_027: show_only_added_emojis_and_stickers,
    ayu_f_028: collapse_similar_channels,
    ayu_f_029: hide_similar_channels,
    ayu_f_030: hide_notification_counters,
    ayu_f_031: hide_notification_badge,
    ayu_f_032: hide_all_chats_folder,
    ayu_f_033: disable_greeting_sticker,
    ayu_f_034: hide_fast_share,
    ayu_f_035: unlimited_recent_stickers,
    ayu_f_036: disable_notifications_delay,
    ayu_f_037: disable_open_link_warning,
    ayu_f_038: spoof_webview_as_android,
    ayu_f_039: increase_webview_height,
    ayu_f_040: increase_webview_width,
    ayu_f_041: local_premium,
    ayu_f_042: show_channel_reactions,
    ayu_f_043: show_group_reactions,
    ayu_f_044: show_private_chat_reactions,
    ayu_f_045: field_attach_button,
    ayu_f_046: field_commands_button,
    ayu_f_047: field_emoji_button,
    ayu_f_048: field_microphone_button,
    ayu_f_049: field_auto_delete_button,
    ayu_f_050: field_gift_button,
    ayu_f_051: field_ai_editor_button,
    ayu_f_052: show_attach_popup,
    ayu_f_053: show_emoji_popup,
    ayu_f_054: sticker_confirmation,
    ayu_f_055: gif_confirmation,
    ayu_f_056: voice_confirmation,
    ayu_f_057: round_confirmation,
    ayu_f_058: drawer_my_profile,
    ayu_f_059: drawer_bots,
    ayu_f_060: drawer_new_group,
    ayu_f_061: drawer_new_channel,
    ayu_f_062: drawer_contacts,
    ayu_f_063: drawer_calls,
    ayu_f_064: drawer_saved_messages,
    ayu_f_065: drawer_l_read_toggle,
    ayu_f_066: drawer_s_read_toggle,
    ayu_f_067: drawer_night_mode_toggle,
    ayu_f_068: drawer_ghost_toggle,
    ayu_f_069: drawer_streamer_toggle,
    ayu_f_070: tray_ghost_toggle,
    ayu_f_071: tray_streamer_toggle,
    ayu_f_072: quick_admin_shortcuts,
    ayu_f_073: show_message_shot,
    ayu_f_074: streamer_mode,
    ayu_f_075: use_global_ghost_mode,
    ayu_f_076: suggest_ghost_mode_before_viewing_story,
}

macro_rules! extera_bool_field_tests {
    ($($name:ident: $field:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let mut s = Settings::default();
                let initial = s.extera.$field;
                s.extera.$field = !initial;
                let json = s.to_json();
                let loaded = Settings::from_json(&json).unwrap();
                assert_eq!(loaded.extera.$field, !initial);
            }
        )+
    };
}

extera_bool_field_tests! {
    ext_f_001: hide_action_bar_status,
    ext_f_002: hide_all_chats,
    ext_f_003: center_title,
    ext_f_004: tab_counter,
    ext_f_005: use_solar_icons,
    ext_f_006: square_fab,
    ext_f_007: force_blur,
    ext_f_008: force_snow,
    ext_f_009: use_system_fonts,
    ext_f_010: new_switch_style,
    ext_f_011: disable_dividers,
    ext_f_012: use_l_navigation,
    ext_f_013: alternative_open_animation,
    ext_f_014: change_status,
    ext_f_015: new_group,
    ext_f_016: new_secret_chat,
    ext_f_017: new_channel,
    ext_f_018: contacts,
    ext_f_019: calls,
    ext_f_020: people_nearby,
    ext_f_021: archived_chats,
    ext_f_022: saved_messages,
    ext_f_023: scan_qr,
    ext_f_024: disable_number_rounding,
    ext_f_025: format_time_with_seconds,
    ext_f_026: upload_speed_boost,
    ext_f_027: hide_phone_number,
    ext_f_028: archive_on_pull,
    ext_f_029: disable_unarchive_swipe,
    ext_f_030: hide_sticker_time,
    ext_f_031: unlimited_recent_stickers,
    ext_f_032: hide_categories,
    ext_f_033: hide_keyboard_on_scroll,
    ext_f_034: permissions_shortcut,
    ext_f_035: administrators_shortcut,
    ext_f_036: members_shortcut,
    ext_f_037: recent_actions_shortcut,
    ext_f_038: disable_jump_to_next_channel,
    ext_f_039: show_action_timestamps,
    ext_f_040: hide_share_button,
    ext_f_041: show_copy_photo_button,
    ext_f_042: show_clear_button,
    ext_f_043: show_save_message_button,
    ext_f_044: show_details_button,
    ext_f_045: show_report_button,
    ext_f_046: show_history_button,
    ext_f_047: add_comma_after_mention,
    ext_f_048: hide_camera_tile,
    ext_f_049: hide_photo_counter,
    ext_f_050: static_zoom,
    ext_f_051: remember_last_used_camera,
    ext_f_052: use_camera_x_optimized_mode,
    ext_f_053: pause_on_minimize,
    ext_f_054: disable_playback,
}

macro_rules! notify_bool_field_tests {
    ($($name:ident: $field:ident),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let mut s = Settings::default();
                let initial = s.notify.$field;
                s.notify.$field = !initial;
                let json = s.to_json();
                let loaded = Settings::from_json(&json).unwrap();
                assert_eq!(loaded.notify.$field, !initial);
            }
        )+
    };
}

notify_bool_field_tests! {
    not_f_001: desktop_notifications,
    not_f_002: flash_taskbar,
    not_f_003: allow_sound,
    not_f_004: show_name,
    not_f_005: show_text,
    not_f_006: pinned_messages,
    not_f_007: accept_calls,
    not_f_008: badge_include_muted,
    not_f_009: badge_count_messages,
    not_f_010: use_windows_notifications,
    not_f_011: respect_focus_mode,
}

macro_rules! scale_tests {
    ($($name:ident: $sc:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let s = Settings {
                    default_scale: false,
                    ui_scale: $sc,
                    ..Settings::default()
                };
                let loaded = Settings::from_json(&s.to_json()).unwrap();
                assert!(!loaded.default_scale);
                assert_eq!(loaded.ui_scale, $sc);
            }
        )+
    };
}

scale_tests! {
    sc_075: 75, sc_080: 80, sc_085: 85, sc_090: 90, sc_095: 95,
    sc_100: 100, sc_105: 105, sc_110: 110, sc_115: 115, sc_120: 120,
    sc_125: 125, sc_130: 130, sc_135: 135, sc_140: 140, sc_145: 145,
    sc_150: 150, sc_155: 155, sc_160: 160, sc_165: 165, sc_170: 170,
    sc_175: 175, sc_180: 180, sc_185: 185, sc_190: 190, sc_195: 195,
    sc_200: 200,
}

macro_rules! bubble_radius_tests {
    ($($name:ident: ($rad:expr, $exp:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let a = AyuSettings {
                    message_bubble_radius: $rad,
                    ..AyuSettings::default()
                };
                assert_eq!(a.bubble_radius(), $exp);
            }
        )+
    };
}

bubble_radius_tests! {
    brad_00: (0, 0), brad_01: (1, 1), brad_02: (2, 2), brad_03: (3, 3),
    brad_04: (4, 4), brad_05: (5, 5), brad_06: (6, 6), brad_07: (7, 7),
    brad_08: (8, 8), brad_09: (9, 9), brad_10: (10, 10), brad_11: (11, 11),
    brad_12: (12, 12), brad_13: (13, 13), brad_14: (14, 14), brad_15: (15, 15),
    brad_16: (16, 16), brad_17: (17, 17), brad_18: (18, 18), brad_19: (19, 19),
    brad_20: (20, 20), brad_21: (21, 20), brad_22: (22, 20), brad_23: (23, 20),
    brad_24: (24, 20), brad_25: (25, 20), brad_26: (26, 20), brad_27: (27, 20),
    brad_28: (28, 20), brad_29: (29, 20), brad_30: (50, 20), brad_neg: (-5, 0),
}

#[test]
fn menu_visibility_logic() {
    assert!(!MenuVisibility::Hidden.shown(false));
    assert!(!MenuVisibility::Hidden.shown(true));
    assert!(MenuVisibility::Visible.shown(false));
    assert!(MenuVisibility::Visible.shown(true));
    assert!(!MenuVisibility::WithModifier.shown(false));
    assert!(MenuVisibility::WithModifier.shown(true));
}

#[test]
fn peer_id_and_channel_button_roundtrip() {
    for pid in [PeerIdDisplay::Hidden, PeerIdDisplay::BotApi, PeerIdDisplay::Telegram] {
        for cb in [
            ChannelBottomButton::Hidden,
            ChannelBottomButton::Mute,
            ChannelBottomButton::Discuss,
            ChannelBottomButton::DiscussWithFallback,
        ] {
            let mut s = Settings::default();
            s.ayu.show_peer_id = pid;
            s.ayu.channel_bottom_button = cb;
            let loaded = Settings::from_json(&s.to_json()).unwrap();
            assert_eq!(loaded.ayu.show_peer_id, pid);
            assert_eq!(loaded.ayu.channel_bottom_button, cb);
        }
    }
}

#[test]
fn send_without_sound_and_translation_provider_roundtrip() {
    for sws in [SendWithoutSound::Never, SendWithoutSound::InGhostMode, SendWithoutSound::Always] {
        for tp in [TranslationProvider::Telegram, TranslationProvider::External] {
            let mut s = Settings::default();
            s.ghost.send_without_sound = sws;
            s.translation = tp;
            let loaded = Settings::from_json(&s.to_json()).unwrap();
            assert_eq!(loaded.ghost.send_without_sound, sws);
            assert_eq!(loaded.translation, tp);
        }
    }
}

#[test]
fn language_and_security_and_filter_roundtrip() {
    let s = Settings {
        language: LanguageSettings {
            interface_lang: "en".to_owned(),
            show_translate_button: true,
            translate_entire_chats: true,
            do_not_translate: vec!["ru".to_owned(), "en".to_owned(), "uk".to_owned()],
            translate_to: "ru".to_owned(),
        },
        security: SecuritySettings {
            passcode_hash: hash_passcode("2026"),
            auto_lock_mins: 15,
            suggest_frequent_contacts: false,
        },
        notify: NotifySettings {
            volume: 65,
            corner: 2,
            max_count: 5,
            ..NotifySettings::default()
        },
        filters: vec![Filter {
            text: "спам".to_owned(),
            is_regex: false,
            case_sensitive: false,
            enabled: true,
            dialog_id: None,
            excluded_chats: vec![-1001, 42],
        }],
        ..Settings::default()
    };
    let loaded = Settings::from_json(&s.to_json()).unwrap();
    assert_eq!(loaded, s);
}

#[test]
fn work_mode_and_autostart_roundtrip() {
    for mode in [WorkMode::WindowAndTray, WorkMode::TrayOnly, WorkMode::WindowOnly] {
        let s = Settings {
            work_mode: mode,
            autostart: true,
            start_minimized: true,
            ..Settings::default()
        };
        let loaded = Settings::from_json(&s.to_json()).unwrap();
        assert_eq!(loaded.work_mode, mode);
        assert!(loaded.autostart);
        assert!(loaded.start_minimized);
    }
}

#[test]
fn locked_ghost_flags_and_per_dialog_filter_roundtrip() {
    let mut s = Settings::default();
    s.ghost.locked_read_messages = true;
    s.ghost.locked_read_stories = true;
    s.ghost.locked_online_packets = true;
    s.ghost.locked_upload_progress = true;
    s.ghost.locked_offline_after_online = true;
    s.ghost.set_active(true);
    s.filters.push(Filter {
        text: "реклама".to_owned(),
        dialog_id: Some(-100500),
        ..Filter::default()
    });
    let loaded = Settings::from_json(&s.to_json()).unwrap();
    assert!(loaded.ghost.send_read_messages);
    assert!(loaded.ghost.send_read_stories);
    assert!(loaded.ghost.send_online_packets);
    assert_eq!(loaded.filters[0].dialog_id, Some(-100500));
}
