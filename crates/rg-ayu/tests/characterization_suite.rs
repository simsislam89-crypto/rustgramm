//! Полный набор характеристических и модульных тестов `rg-ayu` (политика режима призрака,
//! движок плагинов exteraGram/AyuGram, калькулятор выражений, фильтры сообщений и очистка Zalgo).

use grammers_tl_types::{Identifiable, functions};
use rg_ayu::filters::{Filter, FilterSet, strip_zalgo};
use rg_ayu::ghost::{
    Verdict, should_mark_read_after_action, verdict, verdict_status, verdict_typing,
};
use rg_ayu::plugins::{HookOutcome, PluginRegistry, eval_expr, parse_plugin};
use rg_settings::GhostMode;

macro_rules! calc_tests {
    ($($name:ident: $expr:expr => $expected:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let got = eval_expr($expr).expect(concat!("failed to eval: ", $expr));
                assert!(
                    (got - ($expected as f64)).abs() < 1e-6,
                    "expr {} => got {}, expected {}",
                    $expr,
                    got,
                    $expected
                );
            }
        )+
    };
}

calc_tests! {
    calc_add_001: "0 + 0" => 0.0,
    calc_add_002: "1 + 1" => 2.0,
    calc_add_003: "2 + 2" => 4.0,
    calc_add_004: "10 + 20" => 30.0,
    calc_add_005: "100 + 250" => 350.0,
    calc_add_006: "999 + 1" => 1000.0,
    calc_add_007: "1234 + 5678" => 6912.0,
    calc_add_008: "42 + 58" => 100.0,
    calc_add_009: "7 + 8 + 9" => 24.0,
    calc_add_010: "1 + 2 + 3 + 4 + 5" => 15.0,
    calc_add_011: "10 + 20 + 30 + 40" => 100.0,
    calc_add_012: "0.5 + 0.5" => 1.0,
    calc_add_013: "0.25 + 0.75" => 1.0,
    calc_add_014: "1.5 + 2.25" => 3.75,
    calc_add_015: "3.125 + 4.875" => 8.0,
    calc_add_016: "100.1 + 200.2" => 300.3,
    calc_add_017: "+5 + +7" => 12.0,
    calc_add_018: "-5 + 12" => 7.0,
    calc_add_019: "-10 + -20" => -30.0,
    calc_add_020: "1000000 + 2000000" => 3000000.0,

    calc_sub_021: "10 - 3" => 7.0,
    calc_sub_022: "100 - 99" => 1.0,
    calc_sub_023: "50 - 50" => 0.0,
    calc_sub_024: "5 - 12" => -7.0,
    calc_sub_025: "1000 - 333" => 667.0,
    calc_sub_026: "100 - 20 - 30" => 50.0,
    calc_sub_027: "500 - 100 - 50 - 25" => 325.0,
    calc_sub_028: "10.5 - 0.5" => 10.0,
    calc_sub_029: "7.75 - 2.25" => 5.5,
    calc_sub_030: "-10 - 5" => -15.0,
    calc_sub_031: "-10 - -5" => -5.0,
    calc_sub_032: "0 - 42" => -42.0,
    calc_sub_033: "2048 - 1024" => 1024.0,
    calc_sub_034: "65536 - 1" => 65535.0,
    calc_sub_035: "99.9 - 9.9" => 90.0,
    calc_sub_036: "1 - 0.125" => 0.875,
    calc_sub_037: "50 - (20 - 5)" => 35.0,
    calc_sub_038: "(50 - 20) - 5" => 25.0,
    calc_sub_039: "100 - (10 + 20 + 30)" => 40.0,
    calc_sub_040: "144 - 89" => 55.0,

    calc_mul_041: "2 * 3" => 6.0,
    calc_mul_042: "7 * 8" => 56.0,
    calc_mul_043: "12 * 12" => 144.0,
    calc_mul_044: "15 * 15" => 225.0,
    calc_mul_045: "25 * 4" => 100.0,
    calc_mul_046: "125 * 8" => 1000.0,
    calc_mul_047: "1024 * 1024" => 1048576.0,
    calc_mul_048: "0 * 9999" => 0.0,
    calc_mul_049: "1 * 42" => 42.0,
    calc_mul_050: "-6 * 7" => -42.0,
    calc_mul_051: "-6 * -7" => 42.0,
    calc_mul_052: "2 * 3 * 4" => 24.0,
    calc_mul_053: "2 * 3 * 4 * 5" => 120.0,
    calc_mul_054: "1.5 * 4" => 6.0,
    calc_mul_055: "2.5 * 2.5" => 6.25,
    calc_mul_056: "0.5 * 0.5" => 0.25,
    calc_mul_057: "0.1 * 100" => 10.0,
    calc_mul_058: "360 * 2" => 720.0,
    calc_mul_059: "1920 * 1080" => 2073600.0,
    calc_mul_060: "60 * 60 * 24" => 86400.0,

    calc_div_061: "10 / 2" => 5.0,
    calc_div_062: "100 / 4" => 25.0,
    calc_div_063: "144 / 12" => 12.0,
    calc_div_064: "1000 / 8" => 125.0,
    calc_div_065: "1 / 2" => 0.5,
    calc_div_066: "1 / 4" => 0.25,
    calc_div_067: "1 / 8" => 0.125,
    calc_div_068: "3 / 4" => 0.75,
    calc_div_069: "7 / 2" => 3.5,
    calc_div_070: "9 / 4" => 2.25,
    calc_div_071: "100 / 10 / 2" => 5.0,
    calc_div_072: "100 / (10 / 2)" => 20.0,
    calc_div_073: "-42 / 6" => -7.0,
    calc_div_074: "-42 / -6" => 7.0,
    calc_div_075: "0 / 42" => 0.0,
    calc_div_076: "256 / 16" => 16.0,
    calc_div_077: "1024 / 32" => 32.0,
    calc_div_078: "360 / 4" => 90.0,
    calc_div_079: "45 / 9" => 5.0,
    calc_div_080: "81 / 9" => 9.0,

    calc_mix_081: "2 + 2 * 2" => 6.0,
    calc_mix_082: "(2 + 2) * 2" => 8.0,
    calc_mix_083: "10 + 20 * 3" => 70.0,
    calc_mix_084: "(10 + 20) * 3" => 90.0,
    calc_mix_085: "100 - 25 * 3" => 25.0,
    calc_mix_086: "(100 - 25) * 3" => 225.0,
    calc_mix_087: "20 / 4 + 3 * 2" => 11.0,
    calc_mix_088: "20 / (4 + 1) * 2" => 8.0,
    calc_mix_089: "((1 + 2) * (3 + 4))" => 21.0,
    calc_mix_090: "(((10)))" => 10.0,
    calc_mix_091: "-(2 + 3)" => -5.0,
    calc_mix_092: "-(2 * 3) + 10" => 4.0,
    calc_mix_093: "+ (4 + 6)" => 10.0,
    calc_mix_094: "2 * (3 + 4 * (5 - 2))" => 30.0,
    calc_mix_095: "(8 + 2) / (3 + 2)" => 2.0,
    calc_mix_096: "(100 - 40) / (2 * 3)" => 10.0,
    calc_mix_097: "5 * 5 + 12 * 12" => 169.0,
    calc_mix_098: "3 * 3 + 4 * 4" => 25.0,
    calc_mix_099: "8 * 8 + 15 * 15" => 289.0,
    calc_mix_100: "(1 + 2 + 3 + 4) * (10 - 5)" => 50.0,
    calc_mix_101: "1024 * 1024 / 512" => 2048.0,
    calc_mix_102: "17 * 19 - 23" => 300.0,
    calc_mix_103: "13 * 17 + 19" => 240.0,
    calc_mix_104: "29 * 31 - 99" => 800.0,
    calc_mix_105: "11 * 11 - 21" => 100.0,
    calc_mix_106: "16 * 16 - 56" => 200.0,
    calc_mix_107: "18 * 18 - 24" => 300.0,
    calc_mix_108: "21 * 21 - 41" => 400.0,
    calc_mix_109: "25 * 25 - 125" => 500.0,
    calc_mix_110: "30 * 30 - 300" => 600.0,
    calc_mix_111: "0.125 * 8 + 9" => 10.0,
    calc_mix_112: "0.0625 * 16 + 15" => 16.0,
    calc_mix_113: "0.03125 * 32 + 31" => 32.0,
    calc_mix_114: "(1.5 + 2.5) * (3.5 + 4.5)" => 32.0,
    calc_mix_115: "(10.5 - 2.5) / (1.5 + 2.5)" => 2.0,
    calc_mix_116: "7 * (8 + 9) - 19" => 100.0,
    calc_mix_117: "9 * (11 + 13) - 16" => 200.0,
    calc_mix_118: "12 * (14 + 16) - 60" => 300.0,
    calc_mix_119: "15 * (18 + 22) - 100" => 500.0,
    calc_mix_120: "20 * (25 + 35) - 200" => 1000.0,
}

macro_rules! calc_err_tests {
    ($($name:ident: $expr:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert!(eval_expr($expr).is_err(), "expected Err for invalid expr: {}", $expr);
            }
        )+
    };
}

calc_err_tests! {
    calc_err_001: "",
    calc_err_002: "   ",
    calc_err_003: "1 / 0",
    calc_err_004: "100 / (5 - 5)",
    calc_err_005: "42 / (2 * 3 - 6)",
    calc_err_006: "(1 + 2",
    calc_err_007: "1 + 2)",
    calc_err_008: "((1 + 2)",
    calc_err_009: "1 + * 2",
    calc_err_010: "1 + / 2",
    calc_err_011: "*",
    calc_err_012: "/",
    calc_err_013: "+",
    calc_err_014: "-",
    calc_err_015: "()",
    calc_err_016: "( )",
    calc_err_017: "abc",
    calc_err_018: "1 + abc",
    calc_err_019: "2 2",
    calc_err_020: "1..2 + 3",
    calc_err_021: "10 / 0.0",
    calc_err_022: "(5 + 3) / (4 - 2 - 2)",
    calc_err_023: "1 + 2 trailing",
    calc_err_024: "1 @ 2",
    calc_err_025: "1 # 2",
    calc_err_026: "1 % 0",
    calc_err_027: "1 ^ 2",
    calc_err_028: "1 & 2",
    calc_err_029: "1 | 2",
    calc_err_030: "1 = 2",
}

macro_rules! plugin_cmd_tests {
    ($($name:ident: $input:expr => $expected:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let reg = PluginRegistry::with_builtins();
                match reg.apply_outgoing($input) {
                    HookOutcome::Modified(out) => assert_eq!(out, $expected),
                    HookOutcome::Unchanged => assert_eq!($input, $expected),
                }
            }
        )+
    };
}

plugin_cmd_tests! {
    plug_cmd_001: "/shrug" => "¯\\_(ツ)_/¯",
    plug_cmd_002: "/shrug привет" => "привет ¯\\_(ツ)_/¯",
    plug_cmd_003: "/shrug всё готово" => "всё готово ¯\\_(ツ)_/¯",
    plug_cmd_004: "/flip" => "(╯°□°）╯︵ ┻━┻",
    plug_cmd_005: "/flip баг" => "баг (╯°□°）╯︵ ┻━┻",
    plug_cmd_006: "/unflip" => "┬─┬ノ( º _ ºノ)",
    plug_cmd_007: "/unflip починил" => "починил ┬─┬ノ( º _ ºノ)",
    plug_cmd_008: "/lenny" => "( ͡° ͜ʖ ͡°)",
    plug_cmd_009: "/lenny хм" => "хм ( ͡° ͜ʖ ͡°)",
    plug_cmd_010: "/calc 2 + 2" => "2 + 2 = 4",
    plug_cmd_011: "/calc 10 * 15" => "10 * 15 = 150",
    plug_cmd_012: "/calc (100 - 25) / 3" => "(100 - 25) / 3 = 25",
    plug_cmd_013: "/calc 1 / 2" => "1 / 2 = 0.5",
    plug_cmd_014: "/calc 1024 * 8" => "1024 * 8 = 8192",
    plug_cmd_015: "/upper hello world" => "HELLO WORLD",
    plug_cmd_016: "/upper привет мир" => "ПРИВЕТ МИР",
    plug_cmd_017: "/upper rustgramm 0.13" => "RUSTGRAMM 0.13",
    plug_cmd_018: "/lower HELLO WORLD" => "hello world",
    plug_cmd_019: "/lower ПРИВЕТ МИР" => "привет мир",
    plug_cmd_020: "/lower AYUGRAM DESKTOP" => "ayugram desktop",
    plug_cmd_021: "/reverse abcdef" => "fedcba",
    plug_cmd_022: "/reverse топот" => "топот",
    plug_cmd_023: "/reverse привет" => "тевирп",
    plug_cmd_024: "/reverse 12345" => "54321",
    plug_cmd_025: "/spaced rust" => "r u s t",
    plug_cmd_026: "/spaced ayu" => "a y u",
    plug_cmd_027: "/spaced тест" => "т е с т",
    plug_cmd_028: "/code cargo test" => "`cargo test`",
    plug_cmd_029: "/code fn main()" => "`fn main()`",
    plug_cmd_030: "/quote важное сообщение" => "> важное сообщение",
    plug_cmd_031: "/quote Rustgramm" => "> Rustgramm",
    plug_cmd_032: "/box OK" => "[ OK ]",
    plug_cmd_033: "/box RELEASE" => "[ RELEASE ]",
    plug_cmd_034: ":arrow: стрелка" => "→ стрелка",
    plug_cmd_035: ":check: готово" => "✓ готово",
    plug_cmd_036: ":star: звезда" => "★ звезда",
    plug_cmd_037: ":arrow: :check: :star:" => "→ ✓ ★",
    plug_cmd_038: "/calc 10 % 3" => "10 % 3 = 1",
    plug_cmd_039: "/calc 25 % 7" => "25 % 7 = 4",
    plug_cmd_040: "/calc 100 % 10" => "100 % 10 = 0",
    plug_cmd_041: "обычный текст без команд" => "обычный текст без команд",
    plug_cmd_042: "привет, как дела?" => "привет, как дела?",
    plug_cmd_043: "/unknown_cmd test" => "/unknown_cmd test",
    plug_cmd_044: "/calc " => "/calc ",
    plug_cmd_045: "/upper " => "/upper ",
    plug_cmd_046: "/lower " => "/lower ",
    plug_cmd_047: "/reverse " => "/reverse ",
    plug_cmd_048: "/spaced " => "/spaced ",
    plug_cmd_049: "/code " => "/code ",
    plug_cmd_050: "/quote " => "/quote ",
}

macro_rules! ghost_matrix_tests {
    ($($name:ident: ($active:expr, $rm:expr, $rs:expr, $on:expr, $up:expr, $ma:expr, $l_rm:expr, $l_rs:expr, $l_on:expr, $l_up:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let mut g = GhostMode {
                    send_read_messages: $rm,
                    send_read_stories: $rs,
                    send_online_packets: $on,
                    send_upload_progress: $up,
                    mark_read_after_action: $ma,
                    locked_read_messages: $l_rm,
                    locked_read_stories: $l_rs,
                    locked_online_packets: $l_on,
                    locked_upload_progress: $l_up,
                    ..GhostMode::default()
                };
                g.set_active($active);
                let exp_rm = if g.send_read_messages { Verdict::Send } else { Verdict::Drop };
                let exp_rs = if g.send_read_stories { Verdict::Send } else { Verdict::Drop };
                assert_eq!(verdict(&g, functions::messages::ReadHistory::CONSTRUCTOR_ID), exp_rm);
                assert_eq!(verdict(&g, functions::channels::ReadHistory::CONSTRUCTOR_ID), exp_rm);
                assert_eq!(verdict(&g, functions::messages::ReadMessageContents::CONSTRUCTOR_ID), exp_rm);
                assert_eq!(verdict(&g, functions::channels::ReadMessageContents::CONSTRUCTOR_ID), exp_rm);
                assert_eq!(verdict(&g, functions::stories::ReadStories::CONSTRUCTOR_ID), exp_rs);
                assert_eq!(verdict(&g, functions::stories::IncrementStoryViews::CONSTRUCTOR_ID), exp_rs);
                assert_eq!(verdict(&g, functions::messages::SendMessage::CONSTRUCTOR_ID), Verdict::Send);
                assert_eq!(
                    verdict_status(&g, false),
                    if g.send_online_packets { Verdict::Send } else { Verdict::Drop }
                );
                assert_eq!(verdict_status(&g, true), Verdict::Send);
                assert_eq!(
                    verdict_typing(&g, true),
                    if g.send_upload_progress { Verdict::Send } else { Verdict::Drop }
                );
                assert_eq!(verdict_typing(&g, false), Verdict::Send);
                assert_eq!(should_mark_read_after_action(&g), g.active && g.mark_read_after_action);
            }
        )+
    };
}

ghost_matrix_tests! {
    ghost_mat_001: (false, true, true, true, true, true, false, false, false, false),
    ghost_mat_002: (true, true, true, true, true, true, false, false, false, false),
    ghost_mat_003: (true, false, false, false, false, false, false, false, false, false),
    ghost_mat_004: (false, false, false, false, false, false, false, false, false, false),
    ghost_mat_005: (true, true, true, true, true, true, true, false, false, false),
    ghost_mat_006: (true, true, true, true, true, true, false, true, false, false),
    ghost_mat_007: (true, true, true, true, true, true, false, false, true, false),
    ghost_mat_008: (true, true, true, true, true, true, false, false, false, true),
    ghost_mat_009: (true, true, true, true, true, true, true, true, false, false),
    ghost_mat_010: (true, true, true, true, true, true, true, false, true, false),
    ghost_mat_011: (true, true, true, true, true, true, true, false, false, true),
    ghost_mat_012: (true, true, true, true, true, true, false, true, true, false),
    ghost_mat_013: (true, true, true, true, true, true, false, true, false, true),
    ghost_mat_014: (true, true, true, true, true, true, false, false, true, true),
    ghost_mat_015: (true, true, true, true, true, true, true, true, true, false),
    ghost_mat_016: (true, true, true, true, true, true, true, true, false, true),
    ghost_mat_017: (true, true, true, true, true, true, true, false, true, true),
    ghost_mat_018: (true, true, true, true, true, true, false, true, true, true),
    ghost_mat_019: (true, true, true, true, true, true, true, true, true, true),
    ghost_mat_020: (false, false, false, false, false, true, true, false, false, false),
    ghost_mat_021: (false, false, false, false, false, true, false, true, false, false),
    ghost_mat_022: (false, false, false, false, false, true, false, false, true, false),
    ghost_mat_023: (false, false, false, false, false, true, false, false, false, true),
    ghost_mat_024: (false, false, false, false, false, true, true, true, false, false),
    ghost_mat_025: (false, false, false, false, false, true, true, false, true, false),
    ghost_mat_026: (false, false, false, false, false, true, true, false, false, true),
    ghost_mat_027: (false, false, false, false, false, true, false, true, true, false),
    ghost_mat_028: (false, false, false, false, false, true, false, true, false, true),
    ghost_mat_029: (false, false, false, false, false, true, false, false, true, true),
    ghost_mat_030: (false, false, false, false, false, true, true, true, true, true),
    ghost_mat_031: (true, false, true, false, true, false, true, false, true, false),
    ghost_mat_032: (true, true, false, true, false, true, false, true, false, true),
    ghost_mat_033: (false, true, false, true, false, false, true, false, true, false),
    ghost_mat_034: (false, false, true, false, true, true, false, true, false, true),
    ghost_mat_035: (true, false, false, true, true, true, true, true, false, false),
    ghost_mat_036: (true, true, true, false, false, false, false, false, true, true),
    ghost_mat_037: (false, true, true, false, false, true, true, true, false, false),
    ghost_mat_038: (false, false, false, true, true, false, false, false, true, true),
    ghost_mat_039: (true, true, false, false, true, true, true, false, false, true),
    ghost_mat_040: (true, false, true, true, false, false, false, true, true, false),
    ghost_mat_041: (false, true, false, false, true, false, true, false, false, true),
    ghost_mat_042: (false, false, true, true, false, true, false, true, true, false),
    ghost_mat_043: (true, true, true, true, false, true, true, true, true, false),
    ghost_mat_044: (true, true, true, false, true, true, true, true, false, true),
    ghost_mat_045: (true, true, false, true, true, true, true, false, true, true),
    ghost_mat_046: (true, false, true, true, true, true, false, true, true, true),
    ghost_mat_047: (false, true, true, true, false, false, true, true, true, false),
    ghost_mat_048: (false, true, true, false, true, false, true, true, false, true),
    ghost_mat_049: (false, true, false, true, true, false, true, false, true, true),
    ghost_mat_050: (false, false, true, true, true, false, false, true, true, true),
}

macro_rules! filter_tests {
    ($($name:ident: ($pat:expr, $regex:expr, $cs:expr, $excl:expr, $chat:expr, $msg:expr) => $hide:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let f = Filter {
                    text: $pat.to_owned(),
                    enabled: true,
                    is_regex: $regex,
                    case_sensitive: $cs,
                    dialog_id: None,
                    excluded_chats: $excl.to_vec(),
                };
                let (set, bad) = FilterSet::build(&[f]);
                assert!(bad.is_empty());
                assert_eq!(set.hides($chat, $msg), $hide);
            }
        )+
    };
}

filter_tests! {
    flt_001: ("реклама", false, false, &[], 100, "Тут РЕКЛАМА ставок") => true,
    flt_002: ("реклама", false, false, &[], 100, "обычный разговор") => false,
    flt_003: ("Реклама", false, true, &[], 100, "реклама строчными") => false,
    flt_004: ("Реклама", false, true, &[], 100, "Это Реклама с большой") => true,
    flt_005: ("реклама", false, false, &[100], 100, "Тут РЕКЛАМА") => false,
    flt_006: ("реклама", false, false, &[100], 200, "Тут РЕКЛАМА") => true,
    flt_007: (r"\bкрипт[аыуеой]\b", true, false, &[], 1, "купи крипту сейчас") => true,
    flt_008: (r"\bкрипт[аыуеой]\b", true, false, &[], 1, "криптография в Rust") => false,
    flt_009: (r"^\d{6}$", true, true, &[], 1, "618140") => true,
    flt_010: (r"^\d{6}$", true, true, &[], 1, "61814") => false,
    flt_011: (r"@[a-zA-Z0-9_]{5,}", true, false, &[], 1, "подпишись на @some_channel") => true,
    flt_012: (r"@[a-zA-Z0-9_]{5,}", true, false, &[42], 42, "подпишись на @some_channel") => false,
    flt_013: ("казино", false, false, &[], -100123, "Бонус КАЗИНО 100%") => true,
    flt_014: ("казино", false, false, &[-100123], -100123, "Бонус КАЗИНО 100%") => false,
    flt_015: ("airdrop", false, false, &[], 555, "Free AIRDROP tokens") => true,
    flt_016: ("airdrop", false, true, &[], 555, "Free AIRDROP tokens") => false,
    flt_017: ("AIRDROP", false, true, &[], 555, "Free AIRDROP tokens") => true,
    flt_018: (r"(?i)giveaway|розыгрыш", true, false, &[], 10, "Большой РОЗЫГРЫШ призов") => true,
    flt_019: (r"(?i)giveaway|розыгрыш", true, false, &[], 10, "Huge giveaway today") => true,
    flt_020: (r"(?i)giveaway|розыгрыш", true, false, &[], 10, "рабочий созвон в 15:00") => false,
    flt_021: ("спам", false, false, &[1, 2, 3], 2, "спам") => false,
    flt_022: ("спам", false, false, &[1, 2, 3], 4, "спам") => true,
    flt_023: (r"^\[AD\]", true, true, &[], 9, "[AD] Купи слона") => true,
    flt_024: (r"^\[AD\]", true, true, &[], 9, "[ad] Купи слона") => false,
    flt_025: (r"^\[AD\]", true, false, &[], 9, "[ad] Купи слона") => true,
    flt_026: ("#реклама", false, false, &[], 9, "Пост #Реклама") => true,
    flt_027: ("#промо", false, false, &[], 9, "Скидка по #ПРОМО") => true,
    flt_028: ("ставки", false, false, &[], 9, "Прогнозы и СТАВКИ") => true,
    flt_029: ("беттинг", false, false, &[], 9, "Лучший беттинг") => true,
    flt_030: ("сигналы", false, false, &[], 9, "VIP сигналы") => true,
    flt_031: (r"\+\d{11,15}", true, false, &[], 1, "Звони +79991234567") => true,
    flt_032: (r"\+\d{11,15}", true, false, &[], 1, "Код +7") => false,
    flt_033: ("🔥", false, false, &[], 1, "Срочно 🔥🔥🔥") => true,
    flt_034: ("🔥", false, false, &[], 1, "Спокойный текст") => false,
    flt_035: ("💎", false, false, &[], 1, "Алмаз 💎") => true,
    flt_036: ("🎰", false, false, &[], 1, "Слоты 🎰") => true,
    flt_037: ("💰", false, false, &[], 1, "Деньги 💰") => true,
    flt_038: (r"^\s*$", true, false, &[], 1, "   ") => true,
    flt_039: (r"^\s*$", true, false, &[], 1, "не пусто") => false,
    flt_040: ("scam", false, false, &[10, 20, 30, 40], 25, "this is SCAM") => true,
    flt_041: ("scam", false, false, &[10, 20, 30, 40], 30, "this is SCAM") => false,
    flt_042: ("phishing", false, false, &[], 1, "phishing link") => true,
    flt_043: ("токен", false, false, &[], 1, "Новый ТОКЕН вышел") => true,
    flt_044: ("памп", false, false, &[], 1, "Сегодня ПАМП монеты") => true,
    flt_045: ("дамп", false, false, &[], 1, "Осторожно ДАМП") => true,
    flt_046: ("инсайд", false, false, &[], 1, "Секретный ИНСАЙД") => true,
    flt_047: ("раскрутка", false, false, &[], 1, "Раскрутка счёта") => true,
    flt_048: ("договорняк", false, false, &[], 1, "Точный договорняк") => true,
    flt_049: ("халява", false, false, &[], 1, "Полная ХАЛЯВА") => true,
    flt_050: ("заработок", false, false, &[], 1, "Лёгкий заработок") => true,
}

macro_rules! zalgo_tests {
    ($($name:ident: $in:expr => $out:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(strip_zalgo($in), $out);
            }
        )+
    };
}

zalgo_tests! {
    zalgo_001: "привет" => "привет",
    zalgo_002: "a\u{0300}b" => "a\u{0300}b",
    zalgo_003: "a\u{0300}\u{0301}b" => "a\u{0300}\u{0301}b",
    zalgo_004: "a\u{0300}\u{0301}\u{0302}b" => "a\u{0300}\u{0301}b",
    zalgo_005: "a\u{0300}\u{0301}\u{0302}\u{0303}\u{0304}\u{0305}b" => "a\u{0300}\u{0301}b",
    zalgo_006: "x\u{036F}\u{036F}\u{036F}\u{036F}y" => "x\u{036F}\u{036F}y",
    zalgo_007: "x\u{1AB0}\u{1AB1}\u{1AB2}\u{1AB3}y" => "x\u{1AB0}\u{1AB1}y",
    zalgo_008: "x\u{1DC0}\u{1DC1}\u{1DC2}\u{1DC3}y" => "x\u{1DC0}\u{1DC1}y",
    zalgo_009: "x\u{20D0}\u{20D1}\u{20D2}\u{20D3}y" => "x\u{20D0}\u{20D1}y",
    zalgo_010: "x\u{FE20}\u{FE21}\u{FE22}\u{FE23}y" => "x\u{FE20}\u{FE21}y",
    zalgo_011: "й\u{0306}\u{0306}\u{0306}ц" => "й\u{0306}\u{0306}ц",
    zalgo_012: "" => "",
    zalgo_013: "Rustgramm 0.13.0" => "Rustgramm 0.13.0",
    zalgo_014: "A\u{0300}\u{0301}\u{0302}B\u{0303}\u{0304}\u{0305}C" => "A\u{0300}\u{0301}B\u{0303}\u{0304}C",
    zalgo_015: "1\u{0336}\u{0336}\u{0336}2\u{0336}\u{0336}\u{0336}3" => "1\u{0336}\u{0336}2\u{0336}\u{0336}3",
    zalgo_016: "z\u{0310}\u{0311}\u{0312}\u{0313}\u{0314}\u{0315}\u{0316}z" => "z\u{0310}\u{0311}z",
    zalgo_017: "т\u{0320}\u{0321}\u{0322}\u{0323}е\u{0324}\u{0325}\u{0326}ст" => "т\u{0320}\u{0321}е\u{0324}\u{0325}ст",
    zalgo_018: "а\u{0340}\u{0341}\u{0342}\u{0343}\u{0344}б" => "а\u{0340}\u{0341}б",
    zalgo_019: "м\u{0350}\u{0351}\u{0352}\u{0353}\u{0354}ир" => "м\u{0350}\u{0351}ир",
    zalgo_020: "к\u{0360}\u{0361}\u{0362}\u{0363}\u{0364}од" => "к\u{0360}\u{0361}од",
}

macro_rules! custom_plugin_tests {
    ($($name:ident: ($id:expr, $cmd:expr, $tpl:expr, $input:expr) => $expected:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let src = format!(
                    "__id__ = \"{}\"\n__name__ = \"Test {}\"\n__version__ = \"1.0\"\n__author__ = \"Rustgramm\"\n__description__ = \"Desc\"\ncommand {} => {}\n",
                    $id, $id, $cmd, $tpl
                );
                let p = parse_plugin(&src).expect("valid plugin");
                assert_eq!(p.id, $id);
                let mut reg = PluginRegistry::default();
                reg.upsert(p);
                match reg.apply_outgoing($input) {
                    HookOutcome::Modified(s) => assert_eq!(s, $expected),
                    HookOutcome::Unchanged => assert_eq!($input, $expected),
                }
            }
        )+
    };
}

custom_plugin_tests! {
    cplug_001: ("p01", "/greet", "Привет, {args}!", "/greet мир") => "Привет, мир!",
    cplug_002: ("p02", "/ping", "PONG {args}", "/ping 42") => "PONG 42",
    cplug_003: ("p03", "/star", "★ {args} ★", "/star Rustgramm") => "★ Rustgramm ★",
    cplug_004: ("p04", "/fire", "🔥 {args} 🔥", "/fire релиз") => "🔥 релиз 🔥",
    cplug_005: ("p05", "/bold", "**{args}**", "/bold текст") => "**текст**",
    cplug_006: ("p06", "/ital", "__{args}__", "/ital курсив") => "__курсив__",
    cplug_007: ("p07", "/strike", "~~{args}~~", "/strike зачёркнуто") => "~~зачёркнуто~~",
    cplug_008: ("p08", "/spoil", "||{args}||", "/spoil секрет") => "||секрет||",
    cplug_009: ("p09", "/brack", "【{args}】", "/brack заголовок") => "【заголовок】",
    cplug_010: ("p10", "/arrow", "➤ {args}", "/arrow пункт") => "➤ пункт",
    cplug_011: ("p11", "/check", "✅ {args}", "/check готово") => "✅ готово",
    cplug_012: ("p12", "/warn", "⚠️ {args}", "/warn внимание") => "⚠️ внимание",
    cplug_013: ("p13", "/info", "ℹ️ {args}", "/info справка") => "ℹ️ справка",
    cplug_014: ("p14", "/heart", "❤️ {args} ❤️", "/heart Rust") => "❤️ Rust ❤️",
    cplug_015: ("p15", "/music", "🎵 {args} 🎵", "/music трек") => "🎵 трек 🎵",
    cplug_016: ("p16", "/crown", "👑 {args}", "/crown sudoyp") => "👑 sudoyp",
    cplug_017: ("p17", "/bolt", "⚡ {args} ⚡", "/bolt скорость") => "⚡ скорость ⚡",
    cplug_018: ("p18", "/gem", "💎 {args}", "/gem премиум") => "💎 премиум",
    cplug_019: ("p19", "/rocket", "🚀 {args}", "/rocket старт") => "🚀 старт",
    cplug_020: ("p20", "/lock", "🔒 {args}", "/lock защита") => "🔒 защита",
    cplug_021: ("p21", "/tag", "#{args}", "/tag rustgramm") => "#rustgramm",
    cplug_022: ("p22", "/at", "@{args}", "/at channel") => "@channel",
    cplug_023: ("p23", "/paren", "({args})", "/paren прим") => "(прим)",
    cplug_024: ("p24", "/angle", "<{args}>", "/angle тег") => "<тег>",
    cplug_025: ("p25", "/curly", "[{args}]", "/curly блок") => "[блок]",
    cplug_026: ("p26", "/wave", "~ {args} ~", "/wave волна") => "~ волна ~",
    cplug_027: ("p27", "/dash", "— {args} —", "/dash тире") => "— тире —",
    cplug_028: ("p28", "/dots", "{args}…", "/dots пауза") => "пауза…",
    cplug_029: ("p29", "/bang", "{args}!!!", "/bang ура") => "ура!!!",
    cplug_030: ("p30", "/ask", "❓ {args}?", "/ask почему") => "❓ почему?",
}

macro_rules! zw_tests {
    ($($name:ident: $in:expr => $out:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let reg = PluginRegistry::with_builtins();
                assert_eq!(reg.apply_incoming($in), $out);
            }
        )+
    };
}

zw_tests! {
    zw_001: "при\u{200B}вет" => "привет",
    zw_002: "те\u{200C}ст" => "тест",
    zw_003: "сло\u{200D}во" => "слово",
    zw_004: "ле\u{200E}во" => "лево",
    zw_005: "пра\u{200F}во" => "право",
    zw_006: "би\u{202A}ди1" => "биди1",
    zw_007: "би\u{202B}ди2" => "биди2",
    zw_008: "би\u{202C}ди3" => "биди3",
    zw_009: "би\u{202D}ди4" => "биди4",
    zw_010: "би\u{202E}ди5" => "биди5",
    zw_011: "со\u{2060}ед1" => "соед1",
    zw_012: "со\u{2061}ед2" => "соед2",
    zw_013: "со\u{2062}ед3" => "соед3",
    zw_014: "со\u{2063}ед4" => "соед4",
    zw_015: "со\u{2064}ед5" => "соед5",
    zw_016: "\u{FEFF}бом" => "бом",
    zw_017: "чистый текст" => "чистый текст",
    zw_018: "\u{200B}\u{200C}\u{200D}\u{FEFF}" => "",
    zw_019: "R\u{200B}u\u{200C}s\u{200D}t" => "Rust",
    zw_020: "A\u{202E}y\u{202C}u" => "Ayu",
}
