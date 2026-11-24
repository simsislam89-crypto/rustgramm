# Скиллы Rustgramm

Этот файл — реестр скиллов и протокол проекта **Rustgramm** (нативный порт AyuGram Desktop 7.2.9 и Telegram Android с функциями exteraGram на Rust / `egui` / `grammers`).

Протокол: `/allskill <задача>` (`.agents/skills/allskill/SKILL.md` и `.claude/skills/allskill/SKILL.md`). Действует в каждом ответе и без вызова.
История этапов и грабли: `RRR.md`. Карта соответствия функций AyuGram/Telegram: `docs/PARITY.md`.

## Проект в двух строках
- Workspace из 7 крейтов (`rg-net`, `rg-settings`, `rg-core`, `rg-ayu`, `rg-ui`, `rg-desktop`, `rg-android`), `v0.19.1`, >2130 тестов. Единственный релизный бинарник ПК — `dist/Rustgramm.exe` (`bash scripts/make-dist.sh`), Android APK — `scripts/make-apk.sh` (Full с плагинами и Lite без плагинов).
- Паритет 1 в 1 с живым `D:\download WIN\AyuGram\AyuGram.exe`: Ghost Mode, журнал удалённых/правок, пер-диалоговые фильтры, Message Shot, режим стримера, настройки AyuGram (~110) и exteraGram (~85) + `.plugin`, изолированные мультиаккаунты, щит приватности и тотальная очистка данных (`WipeScope`), звонки E2E (MTProto + UDP/STUN/Reflector AES-256-IGE), секретные чаты E2E, истории 9:16, форумы, боты/Mini Apps (`wry` WebView2 / Android JNI), `RichMessage` (`eroot`), Stars/Gifts/Business, темы `.tdesktop-theme`/`.attheme`, 70% матовое стекло (Acrylic/iOS) с регулятором.

## Протокол (кратко, полный текст в `.agents/skills/allskill/SKILL.md`)
| Шаг | Что делать |
|---|---|
| 0 | Прочитать `RRR.md` (последние этапы) и `docs/PARITY.md`; ответы на русском, `ultracave-auto` (первая строка — суть, без воды) |
| 1 | Проверить наличие скиллов в `.agents/skills/` и `.claude/skills/`; недостающие поставить sparse-клоном из `sad-launcher` |
| 2 | Записать одной строкой: «Задача: <тип>. Скиллы: <список>» и вызвать нужные скиллы |
| 3 | Агенты — только когда выигрывает отдельный контекст (`research`, `search_web`, широкая разведка по крейтам) |
| 4 | Сверка с живым `AyuGram.exe`, проверка в `dist\Rustgramm.exe` (тестовые сообщения — только в `sudoyp` с удалением за собой) |
| 5 | Перед «готово»: `cargo clippy --workspace --all-targets -- -D warnings`, `cargo test --workspace`, `bash scripts/make-dist.sh`, запись в `RRR.md`, `verify-and-stop` |

## Задача → скиллы
| Задача | Скиллы |
|---|---|
| Интерфейс (`egui`): значки, меню, панели, анимации | `impeccable` (`clarify`, `animate`, `polish`, `harden`, `layout`) + `ui-ux-pro-max` или `web-interface-guidelines`; тексты — `/tov`; проверка — `/apple-hig` |
| Паритет с AyuGram | сначала живой `AyuGram.exe` (скриншоты, наведение, меню), затем `investigate-first`: эталон → Rustgramm → сравнение |
| Скорость (старт, прокрутка, память, CPU в простое) | `investigate-first`: замер и причина до правки, цифры «до/после» |
| Баг | `investigate-first` → `surgical-patch` |
| Новая функция | `lean-build`; больше одного файла — `/ecc-plan` |
| Рефакторинг / чистка | `safe-refactor`, `/ecc-refactor-clean` |
| Миграция настроек/кэша | `migration` (`serde(default)`, обратимо) |
| Сеть, файлы, ввод, медиа | `ecc-security-review` (пути, размеры, `.part`, отмена) |
| Тесты | `ecc-tdd-workflow`, `/ecc-test-coverage` (`#[cfg(test)]` в модуле) |
| Перенос кода и ускорение на Rust (`EL4CTEO/fast-anything`, `rust-anything`) | `fast-anything` (`scan` → `port` → `verify` → `enable` → `bench` → `report`; откат `FAST_ANYTHING_DISABLE=1`, https://github.com/EL4CTEO/fast-anything) и `rust-anything` (`profile` → `analyze` → `bridge` → `record` → порт → `diff` N/N → `bench` → `gate`) |
| Поиск по коду | `caveman-explore`, `cavecrew` |
| Ревью | `caveman-review`, `/ecc-code-review` |
| Сборка упала | `/ecc-build-fix` |
| Документация | `/ecc-update-docs`, `/ecc-update-codemaps` |
| Короткие ответы / коммит | `caveman`, `ultracave`, `ultracave-auto`, `caveman-commit` |
| Проверка готовности | `verify-and-stop` (всегда в конце), `/ecc-verify`, `ecc-verification-loop` |

Команды (`/tov`, `/apple-hig`, `/ui-audit`, `/live`, `/ecc-*`) лежат в `.agents/commands/` и `.claude/commands/`.
Скиллы лежат в `.agents/skills/` и `.claude/skills/` (включая `fast-anything` и `rust-anything`, вне git).

## Запрещено
- `caveman-setup`, `-discover`, `-evidence-review`, `-manage`, `-optimize` (сторонний шлюз), `megacave`, `roblox-*`, `screenpipe-*`, `/tg-sync`.
- Код не на Rust (кроме `assets/player.html`); Python — только разовые скрипты в `scripts/dev` или scratchpad.
- Секреты (`api_id`/`api_hash`, номер, токены, `.env`, `.rg/api.keys`, `*.session`, `*.jks`) в коммитах, логах и `RRR.md`.
