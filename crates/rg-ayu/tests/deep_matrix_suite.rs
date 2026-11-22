//! Расширенная матрица модульных и характеристических тестов `rg-ayu`:
//! сложные арифметические деревья, цепочки правил плагинов, комбинации фильтров
//! и локальное SQLite-хранилище удалённых/отредактированных сообщений `MessageStore`.

use rg_ayu::filters::{Filter, FilterSet, strip_zalgo};
use rg_ayu::plugins::{HookOutcome, PluginRegistry, eval_expr, parse_plugin};
use rg_ayu::store::MessageStore;

macro_rules! deep_math_tests {
    ($($name:ident: $n:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let x: i64 = $n;
                // Проверяем тождество: (x + 7) * (x - 3) - (x * x + 4 * x - 21) == 0
                let expr1 = format!("({x} + 7) * ({x} - 3)");
                let expr2 = format!("{x} * {x} + 4 * {x} - 21");
                let v1 = eval_expr(&expr1).unwrap();
                let v2 = eval_expr(&expr2).unwrap();
                let expected = ((x + 7) * (x - 3)) as f64;
                assert!((v1 - expected).abs() < 1e-6);
                assert!((v2 - expected).abs() < 1e-6);
            }
        )+
    };
}

deep_math_tests! {
    dmath_001: 1, dmath_002: 2, dmath_003: 3, dmath_004: 4, dmath_005: 5,
    dmath_006: 6, dmath_007: 7, dmath_008: 8, dmath_009: 9, dmath_010: 10,
    dmath_011: 11, dmath_012: 12, dmath_013: 13, dmath_014: 14, dmath_015: 15,
    dmath_016: 16, dmath_017: 17, dmath_018: 18, dmath_019: 19, dmath_020: 20,
    dmath_021: 21, dmath_022: 22, dmath_023: 23, dmath_024: 24, dmath_025: 25,
    dmath_026: 26, dmath_027: 27, dmath_028: 28, dmath_029: 29, dmath_030: 30,
    dmath_031: 31, dmath_032: 32, dmath_033: 33, dmath_034: 34, dmath_035: 35,
    dmath_036: 36, dmath_037: 37, dmath_038: 38, dmath_039: 39, dmath_040: 40,
    dmath_041: 41, dmath_042: 42, dmath_043: 43, dmath_044: 44, dmath_045: 45,
    dmath_046: 46, dmath_047: 47, dmath_048: 48, dmath_049: 49, dmath_050: 50,
    dmath_051: 51, dmath_052: 52, dmath_053: 53, dmath_054: 54, dmath_055: 55,
    dmath_056: 56, dmath_057: 57, dmath_058: 58, dmath_059: 59, dmath_060: 60,
    dmath_061: 61, dmath_062: 62, dmath_063: 63, dmath_064: 64, dmath_065: 65,
    dmath_066: 66, dmath_067: 67, dmath_068: 68, dmath_069: 69, dmath_070: 70,
    dmath_071: 71, dmath_072: 72, dmath_073: 73, dmath_074: 74, dmath_075: 75,
    dmath_076: 76, dmath_077: 77, dmath_078: 78, dmath_079: 79, dmath_080: 80,
    dmath_081: 81, dmath_082: 82, dmath_083: 83, dmath_084: 84, dmath_085: 85,
    dmath_086: 86, dmath_087: 87, dmath_088: 88, dmath_089: 89, dmath_090: 90,
    dmath_091: 91, dmath_092: 92, dmath_093: 93, dmath_094: 94, dmath_095: 95,
    dmath_096: 96, dmath_097: 97, dmath_098: 98, dmath_099: 99, dmath_100: 100,
    dmath_101: 101, dmath_102: 102, dmath_103: 103, dmath_104: 104, dmath_105: 105,
    dmath_106: 106, dmath_107: 107, dmath_108: 108, dmath_109: 109, dmath_110: 110,
    dmath_111: 111, dmath_112: 112, dmath_113: 113, dmath_114: 114, dmath_115: 115,
    dmath_116: 116, dmath_117: 117, dmath_118: 118, dmath_119: 119, dmath_120: 120,
    dmath_121: 121, dmath_122: 122, dmath_123: 123, dmath_124: 124, dmath_125: 125,
    dmath_126: 126, dmath_127: 127, dmath_128: 128, dmath_129: 129, dmath_130: 130,
    dmath_131: 131, dmath_132: 132, dmath_133: 133, dmath_134: 134, dmath_135: 135,
    dmath_136: 136, dmath_137: 137, dmath_138: 138, dmath_139: 139, dmath_140: 140,
    dmath_141: 141, dmath_142: 142, dmath_143: 143, dmath_144: 144, dmath_145: 145,
    dmath_146: 146, dmath_147: 147, dmath_148: 148, dmath_149: 149, dmath_150: 150,
}

macro_rules! plugin_replace_tests {
    ($($name:ident: ($from:expr, $to:expr, $input:expr) => $expected:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let src = format!(
                    "__id__ = \"rep_{}\"\n__name__ = \"Rep\"\nreplace {} => {}\n",
                    stringify!($name), $from, $to
                );
                let p = parse_plugin(&src).unwrap();
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

plugin_replace_tests! {
    prep_001: (":fire:", "🔥", "Это :fire: релиз") => "Это 🔥 релиз",
    prep_002: (":heart:", "❤️", "Люблю Rust :heart:") => "Люблю Rust ❤️",
    prep_003: (":check:", "✅", "Статус :check:") => "Статус ✅",
    prep_004: (":cross:", "❌", "Ошибка :cross:") => "Ошибка ❌",
    prep_005: (":star:", "⭐", "Пять :star:") => "Пять ⭐",
    prep_006: (":rocket:", "🚀", "Полетели :rocket:") => "Полетели 🚀",
    prep_007: (":crab:", "🦀", "Rust :crab:") => "Rust 🦀",
    prep_008: (":crown:", "👑", "Король :crown:") => "Король 👑",
    prep_009: (":gem:", "💎", "Алмаз :gem:") => "Алмаз 💎",
    prep_010: (":bolt:", "⚡", "Молния :bolt:") => "Молния ⚡",
    prep_011: (":lock:", "🔒", "Закрыто :lock:") => "Закрыто 🔒",
    prep_012: (":key:", "🔑", "Ключ :key:") => "Ключ 🔑",
    prep_013: (":bell:", "🔔", "Звонок :bell:") => "Звонок 🔔",
    prep_014: (":mute:", "🔕", "Тихо :mute:") => "Тихо 🔕",
    prep_015: (":pin:", "📌", "Важно :pin:") => "Важно 📌",
    prep_016: (":link:", "🔗", "Ссылка :link:") => "Ссылка 🔗",
    prep_017: (":gear:", "⚙️", "Настройки :gear:") => "Настройки ⚙️",
    prep_018: (":shield:", "🛡️", "Защита :shield:") => "Защита 🛡️",
    prep_019: (":ghost:", "👻", "Призрак :ghost:") => "Призрак 👻",
    prep_020: (":eye:", "👁️", "Взгляд :eye:") => "Взгляд 👁️",
    prep_021: (":bulb:", "💡", "Идея :bulb:") => "Идея 💡",
    prep_022: (":book:", "📖", "Книга :book:") => "Книга 📖",
    prep_023: (":memo:", "📝", "Заметка :memo:") => "Заметка 📝",
    prep_024: (":folder:", "📁", "Папка :folder:") => "Папка 📁",
    prep_025: (":trash:", "🗑️", "Корзина :trash:") => "Корзина 🗑️",
    prep_026: (":clock:", "🕒", "Время :clock:") => "Время 🕒",
    prep_027: (":hourglass:", "⏳", "Ждём :hourglass:") => "Ждём ⏳",
    prep_028: (":watch:", "⌚", "Часы :watch:") => "Часы ⌚",
    prep_029: (":calendar:", "📅", "Дата :calendar:") => "Дата 📅",
    prep_030: (":chart:", "📈", "Рост :chart:") => "Рост 📈",
    prep_031: (":globe:", "🌐", "Сеть :globe:") => "Сеть 🌐",
    prep_032: (":cloud:", "☁️", "Облако :cloud:") => "Облако ☁️",
    prep_033: (":sun:", "☀️", "Солнце :sun:") => "Солнце ☀️",
    prep_034: (":moon:", "🌙", "Луна :moon:") => "Луна 🌙",
    prep_035: (":snow:", "❄️", "Снег :snow:") => "Снег ❄️",
    prep_036: (":wave:", "👋", "Привет :wave:") => "Привет 👋",
    prep_037: (":clap:", "👏", "Браво :clap:") => "Браво 👏",
    prep_038: (":thumbsup:", "👍", "Супер :thumbsup:") => "Супер 👍",
    prep_039: (":thumbsdown:", "👎", "Плохо :thumbsdown:") => "Плохо 👎",
    prep_040: (":ok:", "👌", "Ладно :ok:") => "Ладно 👌",
    prep_041: (":muscle:", "💪", "Сила :muscle:") => "Сила 💪",
    prep_042: (":brain:", "🧠", "Ум :brain:") => "Ум 🧠",
    prep_043: (":eyes:", "👀", "Смотрю :eyes:") => "Смотрю 👀",
    prep_044: (":party:", "🎉", "Ура :party:") => "Ура 🎉",
    prep_045: (":gift:", "🎁", "Подарок :gift:") => "Подарок 🎁",
    prep_046: (":trophy:", "🏆", "Победа :trophy:") => "Победа 🏆",
    prep_047: (":medal:", "🥇", "Первое :medal:") => "Первое 🥇",
    prep_048: (":target:", "🎯", "В яблочко :target:") => "В яблочко 🎯",
    prep_049: (":dice:", "🎲", "Кубик :dice:") => "Кубик 🎲",
    prep_050: (":music:", "🎵", "Музыка :music:") => "Музыка 🎵",
    prep_051: (":notes:", "🎶", "Ноты :notes:") => "Ноты 🎶",
    prep_052: (":mic:", "🎤", "Микрофон :mic:") => "Микрофон 🎤",
    prep_053: (":headphones:", "🎧", "Наушники :headphones:") => "Наушники 🎧",
    prep_054: (":speaker:", "🔊", "Громко :speaker:") => "Громко 🔊",
    prep_055: (":camera:", "📷", "Фото :camera:") => "Фото 📷",
    prep_056: (":video:", "📹", "Видео :video:") => "Видео 📹",
    prep_057: (":tv:", "📺", "Экран :tv:") => "Экран 📺",
    prep_058: (":phone:", "📱", "Телефон :phone:") => "Телефон 📱",
    prep_059: (":laptop:", "💻", "Ноутбук :laptop:") => "Ноутбук 💻",
    prep_060: (":desktop:", "🖥️", "ПК :desktop:") => "ПК 🖥️",
    prep_061: (":keyboard:", "⌨️", "Клавиатура :keyboard:") => "Клавиатура ⌨️",
    prep_062: (":mouse:", "🖱️", "Мышь :mouse:") => "Мышь 🖱️",
    prep_063: (":battery:", "🔋", "Батарея :battery:") => "Батарея 🔋",
    prep_064: (":plug:", "🔌", "Сеть :plug:") => "Сеть 🔌",
    prep_065: (":magnet:", "🧲", "Магнит :magnet:") => "Магнит 🧲",
    prep_066: (":wrench:", "🔧", "Ключ :wrench:") => "Ключ 🔧",
    prep_067: (":hammer:", "🔨", "Молоток :hammer:") => "Молоток 🔨",
    prep_068: (":package:", "📦", "Пакет :package:") => "Пакет 📦",
    prep_069: (":coffee:", "☕", "Кофе :coffee:") => "Кофе ☕",
    prep_070: (":tea:", "🍵", "Чай :tea:") => "Чай 🍵",
    prep_071: (":pizza:", "🍕", "Пицца :pizza:") => "Пицца 🍕",
    prep_072: (":apple:", "🍎", "Яблоко :apple:") => "Яблоко 🍎",
    prep_073: (":lemon:", "🍋", "Лимон :lemon:") => "Лимон 🍋",
    prep_074: (":cherry:", "🍒", "Вишня :cherry:") => "Вишня 🍒",
    prep_075: (":grape:", "🍇", "Виноград :grape:") => "Виноград 🍇",
    prep_076: (":cake:", "🎂", "Торт :cake:") => "Торт 🎂",
    prep_077: (":cookie:", "🍪", "Печенье :cookie:") => "Печенье 🍪",
    prep_078: (":honey:", "🍯", "Мёд :honey:") => "Мёд 🍯",
    prep_079: (":bread:", "🍞", "Хлеб :bread:") => "Хлеб 🍞",
    prep_080: (":cheese:", "🧀", "Сыр :cheese:") => "Сыр 🧀",
    prep_081: (":cat:", "🐱", "Кот :cat:") => "Кот 🐱",
    prep_082: (":dog:", "🐶", "Пёс :dog:") => "Пёс 🐶",
    prep_083: (":fox:", "🦊", "Лиса :fox:") => "Лиса 🦊",
    prep_084: (":wolf:", "🐺", "Волк :wolf:") => "Волк 🐺",
    prep_085: (":bear:", "🐻", "Медведь :bear:") => "Медведь 🐻",
    prep_086: (":panda:", "🐼", "Панда :panda:") => "Панда 🐼",
    prep_087: (":lion:", "🦁", "Лев :lion:") => "Лев 🦁",
    prep_088: (":tiger:", "🐯", "Тигр :tiger:") => "Тигр 🐯",
    prep_089: (":eagle:", "🦅", "Орёл :eagle:") => "Орёл 🦅",
    prep_090: (":owl:", "🦉", "Сова :owl:") => "Сова 🦉",
    prep_091: (":shark:", "🦈", "Акула :shark:") => "Акула 🦈",
    prep_092: (":whale:", "🐋", "Кит :whale:") => "Кит 🐋",
    prep_093: (":dolphin:", "🐬", "Дельфин :dolphin:") => "Дельфин 🐬",
    prep_094: (":octopus:", "🐙", "Осьминог :octopus:") => "Осьминог 🐙",
    prep_095: (":bee:", "🐝", "Пчела :bee:") => "Пчела 🐝",
    prep_096: (":butterfly:", "🦋", "Бабочка :butterfly:") => "Бабочка 🦋",
    prep_097: (":tree:", "🌲", "Ель :tree:") => "Ель 🌲",
    prep_098: (":rose:", "🌹", "Роза :rose:") => "Роза 🌹",
    prep_099: (":cactus:", "🌵", "Кактус :cactus:") => "Кактус 🌵",
    prep_100: (":clover:", "🍀", "Клевер :clover:") => "Клевер 🍀",
}

macro_rules! store_tests {
    ($($name:ident: ($peer:expr, $msg_id:expr, $v1:expr, $v2:expr, $v3:expr)),+ $(,)?) => {
        $(
            #[tokio::test]
            async fn $name() {
                let st = MessageStore::in_memory().await.unwrap();
                st.remember($peer, $msg_id, $v1, 1000).await.unwrap();
                st.record_edit($peer, $msg_id, $v2, 1010).await.unwrap();
                st.record_edit($peer, $msg_id, $v3, 1020).await.unwrap();
                let cur = st.get($peer, $msg_id).await.unwrap().unwrap();
                assert_eq!(cur.text, $v3);
                assert!(!cur.deleted);
                let counts = st.edit_counts($peer).await.unwrap();
                assert_eq!(counts.get(&$msg_id).copied(), Some(2));
                st.mark_deleted($peer, &[$msg_id]).await.unwrap();
                let del = st.deleted($peer).await.unwrap();
                assert_eq!(del.len(), 1);
                assert_eq!(del[0].text, $v3);
                assert!(del[0].deleted);
            }
        )+
    };
}

store_tests! {
    str_001: (101, 1, "первая версия 1", "вторая версия 1", "третья версия 1"),
    str_002: (102, 2, "первая версия 2", "вторая версия 2", "третья версия 2"),
    str_003: (103, 3, "первая версия 3", "вторая версия 3", "третья версия 3"),
    str_004: (104, 4, "первая версия 4", "вторая версия 4", "третья версия 4"),
    str_005: (105, 5, "первая версия 5", "вторая версия 5", "третья версия 5"),
    str_006: (106, 6, "первая версия 6", "вторая версия 6", "третья версия 6"),
    str_007: (107, 7, "первая версия 7", "вторая версия 7", "третья версия 7"),
    str_008: (108, 8, "первая версия 8", "вторая версия 8", "третья версия 8"),
    str_009: (109, 9, "первая версия 9", "вторая версия 9", "третья версия 9"),
    str_010: (110, 10, "первая версия 10", "вторая версия 10", "третья версия 10"),
    str_011: (111, 11, "первая версия 11", "вторая версия 11", "третья версия 11"),
    str_012: (112, 12, "первая версия 12", "вторая версия 12", "третья версия 12"),
    str_013: (113, 13, "первая версия 13", "вторая версия 13", "третья версия 13"),
    str_014: (114, 14, "первая версия 14", "вторая версия 14", "третья версия 14"),
    str_015: (115, 15, "первая версия 15", "вторая версия 15", "третья версия 15"),
    str_016: (116, 16, "первая версия 16", "вторая версия 16", "третья версия 16"),
    str_017: (117, 17, "первая версия 17", "вторая версия 17", "третья версия 17"),
    str_018: (118, 18, "первая версия 18", "вторая версия 18", "третья версия 18"),
    str_019: (119, 19, "первая версия 19", "вторая версия 19", "третья версия 19"),
    str_020: (120, 20, "первая версия 20", "вторая версия 20", "третья версия 20"),
    str_021: (121, 21, "первая версия 21", "вторая версия 21", "третья версия 21"),
    str_022: (122, 22, "первая версия 22", "вторая версия 22", "третья версия 22"),
    str_023: (123, 23, "первая версия 23", "вторая версия 23", "третья версия 23"),
    str_024: (124, 24, "первая версия 24", "вторая версия 24", "третья версия 24"),
    str_025: (125, 25, "первая версия 25", "вторая версия 25", "третья версия 25"),
    str_026: (126, 26, "первая версия 26", "вторая версия 26", "третья версия 26"),
    str_027: (127, 27, "первая версия 27", "вторая версия 27", "третья версия 27"),
    str_028: (128, 28, "первая версия 28", "вторая версия 28", "третья версия 28"),
    str_029: (129, 29, "первая версия 29", "вторая версия 29", "третья версия 29"),
    str_030: (130, 30, "первая версия 30", "вторая версия 30", "третья версия 30"),
    str_031: (-1001, 31, "канал 1 v1", "канал 1 v2", "канал 1 v3"),
    str_032: (-1002, 32, "канал 2 v1", "канал 2 v2", "канал 2 v3"),
    str_033: (-1003, 33, "канал 3 v1", "канал 3 v2", "канал 3 v3"),
    str_034: (-1004, 34, "канал 4 v1", "канал 4 v2", "канал 4 v3"),
    str_035: (-1005, 35, "канал 5 v1", "канал 5 v2", "канал 5 v3"),
    str_036: (-1006, 36, "канал 6 v1", "канал 6 v2", "канал 6 v3"),
    str_037: (-1007, 37, "канал 7 v1", "канал 7 v2", "канал 7 v3"),
    str_038: (-1008, 38, "канал 8 v1", "канал 8 v2", "канал 8 v3"),
    str_039: (-1009, 39, "канал 9 v1", "канал 9 v2", "канал 9 v3"),
    str_040: (-1010, 40, "канал 10 v1", "канал 10 v2", "канал 10 v3"),
    str_041: (-1011, 41, "канал 11 v1", "канал 11 v2", "канал 11 v3"),
    str_042: (-1012, 42, "канал 12 v1", "канал 12 v2", "канал 12 v3"),
    str_043: (-1013, 43, "канал 13 v1", "канал 13 v2", "канал 13 v3"),
    str_044: (-1014, 44, "канал 14 v1", "канал 14 v2", "канал 14 v3"),
    str_045: (-1015, 45, "канал 15 v1", "канал 15 v2", "канал 15 v3"),
    str_046: (-1016, 46, "канал 16 v1", "канал 16 v2", "канал 16 v3"),
    str_047: (-1017, 47, "канал 17 v1", "канал 17 v2", "канал 17 v3"),
    str_048: (-1018, 48, "канал 18 v1", "канал 18 v2", "канал 18 v3"),
    str_049: (-1019, 49, "канал 19 v1", "канал 19 v2", "канал 19 v3"),
    str_050: (-1020, 50, "канал 20 v1", "канал 20 v2", "канал 20 v3"),
}

macro_rules! multi_filter_tests {
    ($($name:ident: ($chat:expr, $text:expr) => $hide:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let filters = [
                    Filter {
                        text: "спам".to_owned(),
                        is_regex: false,
                        case_sensitive: false,
                        enabled: true,
                        excluded_chats: vec![777],
                    },
                    Filter {
                        text: r"\bVIP\b".to_owned(),
                        is_regex: true,
                        case_sensitive: true,
                        enabled: true,
                        excluded_chats: vec![888],
                    },
                    Filter {
                        text: "казино".to_owned(),
                        is_regex: false,
                        case_sensitive: false,
                        enabled: true,
                        excluded_chats: vec![],
                    },
                ];
                let (set, bad) = FilterSet::build(&filters);
                assert!(bad.is_empty());
                let cleaned = strip_zalgo($text);
                assert_eq!(set.hides($chat, &cleaned), $hide);
            }
        )+
    };
}

multi_filter_tests! {
    mflt_001: (1, "Это СПАМ сообщение") => true,
    mflt_002: (777, "Это СПАМ сообщение") => false,
    mflt_003: (1, "Доступ в VIP чат") => true,
    mflt_004: (1, "Доступ в vip чат") => false,
    mflt_005: (888, "Доступ в VIP чат") => false,
    mflt_006: (777, "Доступ в VIP чат") => true,
    mflt_007: (888, "Это СПАМ сообщение") => true,
    mflt_008: (777, "Бонус КАЗИНО") => true,
    mflt_009: (888, "Бонус КАЗИНО") => true,
    mflt_010: (1, "Обычное сообщение") => false,
    mflt_011: (2, "Привет, как проект?") => false,
    mflt_012: (3, "Сборка Rustgramm готова") => false,
    mflt_013: (4, "VIPER — быстрая машина") => false,
    mflt_014: (5, "Наш VIP-канал открыт") => true,
    mflt_015: (6, "Лучшее казино онлайн") => true,
    mflt_016: (7, "Тут нет спама... хотя есть слово спам") => true,
    mflt_017: (8, "Чистый диалог без рекламы") => false,
    mflt_018: (9, "Проверка фильтров 123") => false,
    mflt_019: (10, "Скрытый спам внутри строки") => true,
    mflt_020: (11, "VIP подписка") => true,
}
