/allskill Следующий этап: Rustgramm v0.33.0.

Текущее состояние (после v0.32.0):
- Рабочее пространство на Rust/egui + grammers (MTProto): полностью переписано ядро `glass.rs` под эталон `21st.dev/s/liquid-glass` и `reactbits.dev/components/tilted-card`, подключены 3D-карточки подарков и тем (`TiltedCard`), переработаны `NotificationPopover` (#4), `LiquidGlassExpandableSidebar` (#7), `AppleSpotlight` (#8), единый `AIComposer` (#9), `macOS Dock` (#10), `OtpInputEx` и 5 реальных функций Telegram (избранные стикеры/GIF, точная дата прочтения 1-на-1, правка профиля/запретов канала и группы, галерея/загрузка/удаление фото профиля, глобальные настройки приватности и 18+).
- Дизайн-система: строгий чёрно-белый матовый обсидиан (`#08080A` / `#F2F2F6` + `on_accent()`), живые цвета Telegram только на аватарках, никах, полосах ответа, ссылках и значках verified/premium.

Задачи на v0.33.0:
1. Прочитай `RRR.md`, `docs/PARITY.md` и `docs/LIQUID_GLASS_CATALOG.md` (осталось 61 позиций: 15 компонентов `21st.dev` и 46 из `Aceternity UI`, `Magic UI`, `Motion Primitives`, `GlassCN`, `Uiverse`).
2. Выбери и качественно перенеси следующий пакет из 10–15 компонентов каталога `docs/LIQUID_GLASS_CATALOG.md` (например, `Liquid Glass Audio Waveform Player Pill`, `Liquid Glass Media / Music Player Card`, `Liquid Glass Calendar`, `Animated Modal`, `Morphing Popover` / `Toolbar Dynamic` / `Bento Grid` / `Focus Cards`) с реальной визуальной полировкой без двойных рамок и tofu-глифов.
3. Реализуй следующие 5+ реальных функций Telegram / AyuGram на базе `grammers-tl-types` (без заглушек, с проверкой против живого AyuGram или TL-схемы).
4. Обнови `dist/rustgramm-preview.html` под `v0.33.0`, проверь через локальный HTTP-сервер и `browser`-субагента все ключевые экраны, обнови `docs/LIQUID_GLASS_CATALOG.md`, `RRR.md`, `docs/PARITY.md`, `docs/NEXT_PROMPT.md`.
5. Подними версию до `0.33.0`, прогони Full + Lite `clippy -D warnings`, `cargo test --workspace`, `scripts/test-apk.sh`, собери `scripts/make-dist.sh`, сделай live smoke, выпусти релиз и подготовь следующую чистую сессию.
