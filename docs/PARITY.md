# Карта соответствия: AyuGram Desktop 7.2.9 → Rustgramm

Цель: ни одна функция AyuGram не пропадает, скорость выше. Эталон — установленный
`D:\download WIN\AyuGram\AyuGram.exe` и исходники v7.2.9.
Статусы: ✅ сделано · 🟡 частично · ⬜ не начато · ⛔ намеренно не переносим (чужие серверы, телеметрия).

## Функции AyuGram (слой `ayu/`)

| Функция | Статус | Где |
|---|---|---|
| Режим призрака (прочитано, в сети, прогресс, истории, «не в сети», закрепление значений) | ✅ политика и настройки | `rg-ayu::ghost`, `rg-settings` |
| Сохранять удалённые, история правок | ✅ | `rg-ayu::store`, `Engine::apply` |
| Метка «удалено»/«изменено», полупрозрачные удалённые | ✅ | `chat.rs` |
| Отправка без звука / через отложенные | ✅ без звука и отправка отложкой (+12 с, `ghost::schedule_date` и `offline = true`) | `rg-ayu::ghost`, `Engine::send_reply` |
| Фильтры сообщений (текст, regex, исключения чатов), чистка «зальго» | ✅ глобальные и для конкретного чата (`dialog_id`), импорт/экспорт JSON (Rustgramm и AyuGram), чистка «зальго» | `rg-ayu::filters`, окно «Настройки» |
| Контекстное меню (реакции, повторить, скрыть, сведения, фильтр, ID) | ✅ + просмотры (`menu_views_panel`), сообщения участника (`menu_user_messages`), журнал удалённых и правок (`audit_peer`) | `chat.rs::menu`, `rg-ayu::store` |
| Все ~110 настроек AyuGram (модель данных + окно) | ✅ модель, окно и привязка переключателей (MD3-переключатели, хвостики пузырей, компактные реплаи, иконки инфо, счётчики, фоны, шрифты, метки, подтверждения, быстрая пересылка, нижняя кнопка канала) | `rg-settings::ayu`, `chat.rs`, `kit.rs`, `settings_ui.rs` |
| Радиус пузыря, ширина, секунды, ID чата, метки | ✅ влияют на ленту | `chat.rs` |
| Настройки exteraGram (основа Android-версии, ~85 параметров) + система плагинов `.plugin` (Full с плагинами / Lite без плагинов) | ✅ модель данных, движок `rg-ayu::plugins`, разделы «Плагины и телефон» и «Настройки exteraGram» (центрирование шапки, скрытие статуса/номера, квадратный FAB, разделители, секунды, двойной клик), скрипты Android Full/Lite | `rg-settings::extera`, `rg-ayu::plugins`, `tg_settings.rs`, `chat.rs` |
| Скрытие рекламы | ⛔ реклама Telegram не запрашивается | — |
| Снимок сообщения (Message shot) | ✅ интерактивное окно предпросмотра (`shot.rs`: GDI ClearType + 5 тем `ShotTheme`, мультивыбор, копирование в буфер и сохранение PNG) | `ayu/features/message_shot` |
| Режим стримера | ✅ защита захвата экрана `WDA_EXCLUDEFROMCAPTURE` + маскировка телефона, `@имени`, ID (`theme::mask_streamer`) | `ayu/features/streamer_mode`, `win.rs` |
| Переводчик (встроенный Telegram) | ✅ этап 18 (`Engine::translate_text`, `messages.translateText` → в пузыре) | `messages.translateText` |
| Пересылка без автора / расширенная | ✅ пакетная пересылка нескольких сообщений (`selected_msgs` + `Engine::forward_batch`) и скрытие автора (`forward_no_author`) | `ayu/features/forward` |
| Локальный Premium, скрытие статусов Premium | 🟡 `hide_premium_statuses` скрывает значки; `local_premium` как переключатель в настройках | `lib.rs`, `chat.rs`, `tg_settings.rs` |
| Иконки приложения, шрифты, темы | ✅ `render_app_icon_rgba` (12 палитр + бейдж трея), выбор системных/моноширинных шрифтов (`sys_fonts`), стили фона чата | `theme.rs`, `win.rs`, `settings_ui.rs` |
| Sentry, update.ayugram.one, api.exteragram.app, jsdelivr, dpaste, iTunes, Google/Yandex-перевод | ⛔ | `docs/AUDIT.md` |

## Новые функции этапа 26 (v0.15.0)

| Функция | Статус | Где |
|---|---|---|
| Чистые модалки без затемнения внутри карточек | ✅ убран темнящий оверлей `paint_modal_subblur` (заменён на внешнюю тень + стеклянную кромку) и осветлена вуаль `paint_frosted_scrim` | `theme.rs` |
| Виртуализация больших чатов и 0% CPU в простое | ✅ `show_viewport` по накопленным высотам `msg_heights` + умный троттлинг `tick_tgs` и кэш кадров без трэшинга | `chat.rs`, `lib.rs` |
| Полный функционал ботов и Mini Apps | ✅ инлайн-кнопки (`Callback`/`Url`/`WebView`/`Copy`/`SwitchInline`/`UserProfile`), reply-клавиатуры (`⌨`), стартовый экран `/start` + `BotInfo.description`, кнопка меню WebApp (`🌐`), встроенный `MiniAppView` (`wry` WebView2 / Android) | `engine.rs`, `video.rs`, `chat.rs`, `worker.rs`, `lib.rs` |
| Внутренняя маршрутизация ссылок Telegram | ✅ `parse_tg_link` + `resolve_username` + `open_or_import_invite` (`tg://`, `t.me/`, `t.me/+HASH`, `t.me/c/...`, `@username`) внутри клиента без браузера | `engine.rs`, `worker.rs`, `lib.rs`, `chat.rs` |
| Щит приватности трафика и метаданных (`PrivacyShieldSettings`) | ✅ блокировка телеметрии активности/онлайна/печати/чтения/просмотра историй, случайный джиттер таймингов пакетов (`0..300 мс`), режим только в ОЗУ (`ephemeral_local_cache`), обезличивание имён файлов при отправке (`file.<ext>`) | `rg-settings`, `engine.rs`, `worker.rs`, `settings_ui.rs`, `tg_settings.rs` |
| Изолированные аккаунты (анти-корреляция) | ✅ `AccountProfile::isolated` (`accounts/slot_<N>/`): своя БД/сессия/MiniApp-профиль, уникальный отпечаток `device_model`/`system_version`/`app_version` в MTProto `initConnection`, отдельный SOCKS5-прокси, разрыв старого пула `pool_handle.quit()` и случайная пауза перед переключением | `rg-settings`, `rg-core::lib`, `worker.rs`, `settings_ui.rs`, `tg_settings.rs` |
| Тотальная очистка данных у других людей (`WipeScope`) | ✅ удаление всех личных переписок и чатов с ботами у обеих сторон (`revoke = true`), поиск и удаление своих сообщений в группах (`revoke = true`), удаление контактов и рейтинга частых собеседников, очистка черновиков, недавних стикеров, платёжных данных и локального кэша БЕЗ потери аккаунта | `engine.rs`, `worker.rs`, `settings_ui.rs`, `tg_settings.rs` |
| Фирменная оранжевая иконка Rustgramm (Android + Windows) | ✅ PNG mipmap (mdpi..xxxhdpi), adaptive icon (`mipmap-anydpi-v26`), `icon.ico` через `build.rs` + `render_app_icon_rgba`, подпись APK через `apksigner` v1/v2/v3 | `crates/rg-android/res`, `crates/rg-desktop`, `scripts/make-apk.sh`, `win.rs` |
| Вход по телефону без запроса API, бесшовные аккаунты и локальное скрытие удалённых | ✅ `builtin_keys` с публичным fallback, прямой старт с телефона на Android, мгновенное `+ Добавить аккаунт` без перезапуска с автосохранением никнейма/`@username`, клиппинг всех строк от вылезания, замена квадратов на иконку `menu/delete`, пункт «Удалить у себя» для уже удалённых (`purge_deleted`) и «Копировать ссылку» (`t.me/...`) | `worker.rs`, `store.rs`, `kit.rs`, `settings_ui.rs`, `chat.rs`, `lib.rs` |
| Идеальный вход, исправленный звук, запись голосовых, истории, звонки E2E, секретные чаты E2E, админка, Mini App bridge, цветные кнопки ботов, автообновление и автоскролл за курсором (`v0.17.0`) | ✅ 70+ стран с поиском и автоопределением кода (`countries.rs`), отступ статус-бара Android, таймауты и мгновенное применение прокси; нативный WinMM звук уведомлений/гудков + исправленный WebView2 `AudioView`/`VideoView`/`stream.rs` (CORS + `HEAD`); запись голосовых WinMM `waveIn*` + `encode_waveform` (`audio_io.rs`, `send_voice`); публикация и просмотр историй с фото/реакциями (`send_story`, `react_story`, `story_media_path`); голосовые/видеозвонки MTProto `phone.*` с 4 эмодзи E2E (`calls.rs`); секретные чаты MTProto 2.0 AES-256-IGE + DH 2048 + 8×8 ключ + TTL (`secret.rs`, `store.rs`); админка групп/каналов (`edit_admin`, `edit_banned`, `edit_default_banned_rights`, `admin_log`, `chat_invites`); Mini App с `TelegramWebviewProxy` (BackButton/MainButton/expand/links); цветные анимированные кнопки ботов (`Primary`/`Danger`/`Success` + shimmer/ripple); анонимная плашка автообновления GitHub Releases (`updater.rs`); автоскролл зажатием и кликом колёсика с иконкой на курсоре | `countries.rs`, `audio_io.rs`, `updater.rs`, `secret.rs`, `calls.rs`, `engine_ext.rs`, `features_ui.rs`, `video.rs`, `stream.rs`, `chat.rs`, `lib.rs` |
| Потоковая загрузка до 5k чатов и 1k+ участников, UDP/STUN медиа-поток звонков, нативный Android JNI (аудио/Intent/WebView/уведомления) + постоянный keystore, редактор медиа, темы `.tdesktop-theme`/`.attheme` + паки иконок, Telegram Stars/Gifts/Business, защита от 2-го окна и чистое закрытие аудио (`v0.18.0`) | ✅ `dialogs_stream(60, 5000)` + кэширование фильтрации чатов (`filtered_cache`) + виртуализация участников (`ScrollArea::show_rows` и `channel_participants_paged`) + масштабирование Tokio до `4..=12` потоков (`worker.rs`); шифрованный UDP/STUN/Reflector медиа-поток звонков (`call_media.rs`, AES-256-IGE, IMA-ADPCM 4:1, `CallAudioPump`, `relay_only_privacy`); нативный Android JNI (`android_jni.rs`, `rg_bridge.dex`: `AudioRecord`/`MediaPlayer`, `ACTION_OPEN_DOCUMENT`, встроенный `WebView`, `NotificationManager`) + постоянный keystore в CI (`ANDROID_KEYSTORE_*`); встроенный редактор медиа перед отправкой (`media_editor.rs`: кроп/поворот/отражение/рисование/блюр, сжатие HD, спойлер, `ttl_seconds`); темы `.tdesktop-theme`/`.attheme` + живой конструктор палитры + обои с затемнением + паки иконок (`theme_pack.rs`, `theme.rs`, `icons.rs`); баланс Telegram Stars, витрина/профиль Star Gifts, платные звёздные реакции, бизнес-часы и быстрые ответы (`/shortcut`); Win32 single-instance mutex (`acquire_single_instance`) + остановка звука/микрофона/WebView при выходе (`shutdown_resources`) | `call_media.rs`, `calls.rs`, `engine.rs`, `engine_ext.rs`, `media.rs`, `rg-settings`, `media_editor.rs`, `theme_pack.rs`, `theme.rs`, `icons.rs`, `android_jni.rs`, `rg_bridge.dex`, `audio_io.rs`, `win.rs`, `worker.rs`, `features_ui.rs`, `chat.rs`, `lib.rs`, `make-apk.sh`, `release.yml` |
| Стабильность окна/ленты, RichMessage (таблицы/цитаты ботов `eroot`), мгновенные live-сообщения, ретрай RPC 500, 96-поточная конкурентность, 70% матовое стекло, 9:16 истории, Telegram-вход + маскоты, жидкие QR, безлимитная расшифровка голоса, подробные логи и 10 новых фич Telegram (`v0.19.0`) | ✅ устранено двоение окна (`LayerId::background`, клиппинг `emoji-side`/`info`, `SetWindowPos(SWP_FRAMECHANGED)`), квадраты `□` (`seguiemj.ttf`/`seguisym.ttf`/`arial.ttf`/Noto во всех 3 семействах), дёргание ленты при входе, бесконечная загрузка (`Ev::History(dialog_id)` + `HistoryError`), RPC 500 `RPC_MCGET_FAIL` (`Engine::invoke_retry`), пропадание live-сообщений (ранний `run_updates` + `updates.getState` + локальный подъём диалога); старт с 5 основных чатов без загрузки архива (ленивый `Cmd::LoadArchive`) и до 96 параллельных потоков; 70% прозрачность по умолчанию + многослойное матовое стекло левой шторки со скроллом; перенос Stars/Gifts/Business в Настройки; единое выравнивание иконок поля ввода/поиска/историй; прозрачные стикеры + 30 FPS TGS + прозрачные WebM + мини-окно эмодзи по наведению на ПК + избранные/недавние стикеры и GIF; проверка `ChannelFull.paid_reactions_available` для звёздных реакций; ускоренная смена неограниченных аккаунтов (<1 с) + диагностический лог `<RG_HOME>/logs/rustgramm.log`; 9:16 истории с 0-мс предзагрузкой и кросс-DC `RawLoc`; `RichMessage` Layer 233 (`rich_msg.rs`); безлимитная расшифровка голоса (`transcribe_voice` + оффлайн-фоллбэк); пружинные микроанимации (`⋮` и др.) и жидкие QR (`draw_styled_qr`); 10 новых фич: чек-листы (To-Do), эффекты сообщений, отложенные/напоминания, теги «Избранного», «Отправить от имени», конструктор опросов/викторин, акцентный цвет профиля, глобальный поиск `#хэштегов`, Slow Mode, палитра `Ctrl+K` | `diag.rs`, `rich_msg.rs`, `engine.rs`, `engine_ext.rs`, `rg-settings`, `theme.rs`, `win.rs`, `frame.rs`, `worker.rs`, `lib.rs`, `chat.rs`, `panels.rs`, `rich.rs`, `video.rs`, `settings_ui.rs`, `tg_settings.rs`, `features_ui.rs`, `rg-android` |
| Патч v0.19.1: раскладка левой панели, двойные кнопки окна Win11, мут/архив/`silent`/категории уведомлений, антиспам >3/сек, мягкий звук (`v0.19.1`) | ✅ `allocate_ui_with_layout` вместо `horizontal_centered` + регресс-тест; снятие `WS_SYSMENU` после показа окна; `DialogRow.notify_explicit`, `MessageRow.silent`, `NotifyGate`, `is_dialog_muted_for_notify`; новый `generate_chime_wav` | `lib.rs`, `chat.rs`, `win.rs`, `engine.rs`, `worker.rs`, `audio_io.rs` |

## Функции Telegram Desktop (клиентское ядро)

| Функция | Статус |
|---|---|
| Вход: QR, номер, код, облачный пароль | ✅ |
| Список чатов, непрочитанные, закреп, без звука | ✅ |
| Лента, дата-разделители, подгрузка старой истории | ✅ |
| Отправка текста, ответ, правка, удаление, пересылка, закрепление, реакции | ✅ |
| Поиск по чату | ✅ |
| Отправка файлов и фото | ✅ |
| Предпросмотр фото и статичных стикеров, скачивание файлов | ✅ |
| Группы: имя отправителя в пузыре | ✅ |
| Левая панель меню (профиль, переключатели) | ✅ вид; Профиль/Группа/Канал/Контакты/Настройки Telegram — ✅; Звонки — ⬜ |
| Окно «Настройки Rustgramm» (6 категорий + фильтры + теневой бан) | ✅ |
| Избранное, «Мой профиль» (отдельное модальное окно без переброса в Избранное), «Прочитать на сервере» (все чаты из меню + текущий чат) | ✅ этап 24 (`my_profile_window`, `force_read_all`, `Engine::force_read_many`) |
| Горячие клавиши и кнопки мыши (быстрый автоскролл колёсиком Middle-Click, ускорение колеса Shift/Alt ×3.5, боковые кнопки мыши Back/Forward по истории чатов и страниц настроек, `Ctrl+Z`/`Ctrl+Y`, стрелка `↑` для правки последнего сообщения) | ✅ этап 24 (`lib.rs::handle_shortcuts`, `chat.rs`) |
| Архив чатов (просмотр, в архив/из архива), закреп/звук/прочитать из списка | ✅ |
| Папки чатов (вкладки, правила Telegram, создание/правка/удаление) | ✅ этап 16 (`messages.updateDialogFilter`) |
| Контакты (окно, поиск, открытие чата) | ✅ |
| Стикеры: панель, наборы, отправка | ✅ статичные, WebM и настоящие векторные `.tgs` (Lottie) на чистом Rust (`lottie::TgsAnim`) |
| Альбомы (сетка превью в одном пузыре) | ✅ (вживую не проверено) |
| Глобальный поиск, контакты, избранное, профили | ✅ этап 16 (`contacts.search` в левой колонке) |
| Цветные эмодзи (Twemoji/Apple) в ленте, списке и панели + поиск эмодзи | ✅ этап 16 (встроенный словарь + `messages.getEmojiKeywords`, закрепление панели) |
| Прозрачное окно 70% матовое стекло на старте (миграция `opacity_v19_migrated`) + ползунок 15–100% + блюр (Acrylic/iOS), оранжевые основные кнопки | ✅ этап 16 / 30 |
| Форматирование текста (жирный, курсив, код, блок кода, спойлер, цитата, ссылки, упоминания) | ✅ отображение + парсер отправки (`rich::parse_formatted`), панель `[Aa]` и клавиши `Ctrl+B`/`I`/`Shift+X`/`M`/`P` |
| Превью ссылок (веб-страницы) | ✅ этап 18 (`MediaKind::WebPage`, карточка сайта с кликом) |
| Фото профилей | ✅ |
| Просмотр фото, карточки видео/GIF/голосовых/файлов (с кольцом загрузки), открытие безопасных типов | ✅ + навигация галереи фото (`←`/`→`), Drag & Drop отправка файлов, закреплённое сообщение в шапке |
| Встроенный плеер (WebView2) + потоковое воспроизведение (`127.0.0.1` Range) + автозапуск GIF в ленте | ✅ этап 16–17; миниатюры и живое воспроизведение TGS/WebM ✅ |
| Анимированные стикеры (Lottie/WebM), GIF, видео, голосовые, кружки | ✅ чистый Rust TGS (Lottie JSON) 24 FPS + WebM/видео/GIF/кружки |
| Опросы, истории, форумы/темы, комментарии каналов | ✅ опросы/викторины (`messages.sendVote`), публикация и просмотр историй (`stories.sendStory`, `stories.getAllStories`, `stories.getStoriesByID`, `stories.sendReaction`, `stories.readStories`), темы форумов (`messages.getForumTopics`), комментарии каналов (`messages.getDiscussionMessage`, `messages.getReplies`) |
| Секретные чаты | ✅ этап 28 (`rg-core::secret`: DH 2048, MTProto 2.0 AES-256-IGE, 8×8 ключ, таймер самоуничтожения TTL, SQLite слота) |
| Изолированные несколько аккаунтов | ✅ этап 26 (`AccountProfile::isolated`, отдельные слоты/отпечатки/прокси без корреляции) |
| Боты (инлайн-кнопки, реплай-клавиатуры, `/start`, Mini Apps) и внутренние `t.me`/`tg://` ссылки | ✅ этап 26 + 28 (`extract_reply_markup` с `KeyboardButtonStyle`, цветные анимированные кнопки + shimmer/ripple, `MiniAppView` + JS-бридж `TelegramWebviewProxy`, `parse_tg_link`) |
| Звонки (аудио и видео) | ✅ этап 28 (`rg-core::calls`: MTProto `phone.*` сигналинг, DH 2048, 4 E2E-эмодзи, окно вызова/история звонков, гудок WinMM) |
| Администрирование групп и каналов | ✅ этап 28 (`channels.editAdmin`, `channels.editBanned`, `messages.editChatDefaultBannedRights`, `channels.getAdminLog`, пригласительные ссылки) |
| Запись голосовых сообщений | ✅ этап 28 (`audio_io.rs` WinMM `waveIn*` + живая волна + `encode_waveform` + `Engine::send_voice`) |
| Уведомления Windows, трей, автозапуск | ✅ системные toast + встроенные карточки, звук/мигание; трей Windows (`Shell_NotifyIconW`, `WorkMode`, бейдж на иконке), автозапуск HKCU Run и URL-схема `tg://` (`win.rs`, `tray.rs`) |
| Настройки Telegram (аккаунт, уведомления, приватность, данные, папки, внешний вид, язык, устройства, энергосбережение) | ✅ полностью интерактивные этап 23 (поиск, QR, `@username`, все 12 правил `account.setPrivacy`, локальный код-пароль, автоудаление, разблокировка, завершение сеансов `account.resetAuthorization`/`auth.resetAuthorizations`, переводчик чатов, масштаб 75–200%) |
| Темы, фон чата, масштаб, блюр | ✅ этап 16 / 23 (сплошной / градиент / узор, масштаб 75–200%, блюр стекла, радиус/ширина пузыря) |
| Значки: набор Telegram (376 шт., атлас), меню со значками, меню «три точки» | ✅ этап 13 |
| Анимации по значениям Telegram (120–240 мс, без пружин) + плавное открытие всех модальных окон с 5-слойным гауссовым подблюром (`paint_modal_subblur`) и скольжением страниц | ✅ этап 13 / 16 / 24 |
| Эмодзи Apple из кэша AyuGram (локально), крупные 1–3 эмодзи, наборы эмодзи, показ и отправка Premium-эмодзи (`MessageEntityCustomEmoji`), статусы EmojiStatus и значки Verified/Premium в списке/шапке/профиле | ✅ этап 13б / 22 / 23 |
| Музыка в профиле (`saved_music` / `users.getSavedMusic`) и встроенный аудиоплеер (M4A/MP3/OGG/Opus/FLAC) с потоковым стартом после первых 128 КБ и фоновой предзагрузкой | ✅ этап 23 / 24 (`download_track_streaming`, `StreamAudio`, `AudioView::new_url`) |
| Создание каналов и групп с добавлением любых людей (`@username` / контакты) и публичной ссылкой, полноценные профили (действия, музыка, медиа, файлы, ссылки, участники) | ✅ этап 21 / 23 |

## Новые функции этапа 32 (v0.20.0)

| Функция | Статус | Где |
|---|---|---|
| Вход по номеру телефона и Android-клавиатура + редизайн экрана входа и crisp QR 512 px | ✅ прямая обработка `auth.sendCode` (303 migrate, `AUTH_RESTART`, `SentCode::PaymentRequired`), таймауты сети, нативный Android `promptTextInput` в `rg_bridge.dex`, экран входа с анимацией логотипа и SVG-самолётиком, `render_qr_image` (`EcLevel::H`) с GPU-кэшем, `RG_DEMO=login` | `auth.rs`, `qr.rs`, `android_jni.rs`, `countries.rs`, `theme.rs`, `lib.rs`, `worker.rs`, `build_android_bridge.py` |
| Ускорение рендера, БД, сети и CPU/RAM (методология профилирования горячих путей) | ✅ `opt-level = 3`, `traffic_jitter_ms = 0`, 4 индекса SQLite + `synchronous=NORMAL` + O(1) `audit_peer`, прекомпилированный `FilterSet`, `BufWriter` логов/медиа, bbox-маска `lottie::stroke_contours`, бюджет 6 GPU-текстур и 2 TGS/кадр, одинарный `Galley` layout, `show_rows` виртуализация | `Cargo.toml`, `rg-settings`, `rg-ayu`, `rg-core`, `rg-ui` |
| Перевод черновика перед отправкой (`translateBeforeSend`) | ✅ кнопка языка в поле ввода, выбор языка (`target_language`), перевод в поле или сразу с отправкой через `messages.translateText` | `chat.rs`, `worker.rs`, `lib.rs` |
| Инлайн-боты (`@bot запрос`) | ✅ `parse_inline_bot_input` с 320мс дебаунсом, кэш `ResolveUsername`, `messages.getInlineBotResults` + пагинация «Ещё», `messages.sendInlineBotResult`, `SwitchInline` | `engine_ext.rs`, `worker.rs`, `chat.rs` |
| Раздельный двойной тап (входящие / свои, 8 действий) + быстрая реакция по умолчанию | ✅ `double_tap_action` и `double_tap_action_out_owner`, настраиваемый чип реакции (`messages.getAvailableReactions` + `messages.setDefaultReaction`) | `extera.rs`, `engine_ext.rs`, `chat.rs`, `tg_settings.rs` |
| Эмодзи-статус аккаунта и таймер | ✅ модальное окно из меню и «Мой профиль», загрузка стандартных/пользовательских эмодзи-статусов (`account.getDefaultEmojiStatuses`), установка с таймером (`account.updateEmojiStatus`) | `engine_ext.rs`, `worker.rs`, `features_ui.rs`, `settings_ui.rs`, `tg_settings.rs` |
| Полные сведения о сообщении (3 вкладки + DC) и копирование фото в буфер | ✅ кликабельные метаданные + `DC` (`dc_location`), списки «Прочитали» (`messages.getMessageReadParticipants`) и «Реакции» (`messages.getMessageReactionsList`), ID·DC в профиле, `Act::CopyPhoto` (`arboard`) на ПК | `media.rs`, `engine.rs`, `engine_ext.rs`, `worker.rs`, `chat.rs` |

## Новые функции этапа 33 (v0.21.0)

| Функция | Статус | Где |
|---|---|---|
| Прозрачные 60 FPS маскоты входа (без фона и «мыльных» эмодзи) + оранжевый брендинг | ✅ векторные HiDPI маскоты (`login_anim.rs`: пульсирующий самолётик, следящая за длиной кода обезьянка, обезьянка 2FA с лапами/подглядыванием, левитирующий ключ), 100% оранжевый акцент на входе и QR (`c_orange()`, центр-бейдж `draw_styled_qr` v3) и TGS стикеры 256 px | `login_anim.rs`, `lib.rs`, `theme.rs` |
| Надёжная и мгновенная доставка входящих и исходящих сообщений | ✅ буфер обновлений `updates_channel_capacity = 2048`, кэш `self_user` ДО старта `run_updates` и до `ImportLoginToken` (`cache_tl_peers`/`cache_tl_auth_user`), персист пиров/channel pts из `GetDialogs`/`GetDiscussionMessage`, диспатч `on_event` до фоновой БД; оптимистичная 0-мс отправка в ленту и обсуждение (`temp_id < 0`, `🕒`, `SentConfirm`/`SendFailed`) и автодозагрузка `SyncOpenChat` после простоя | `rg-core/{lib,auth,qr,engine}.rs`, `worker.rs`, `lib.rs`, `chat.rs` |
| Ускорение горячих путей по методологии `fast-anything` | ✅ O(N) `HashMap`-индексация `parse_raw_dialogs` и `tl_messages_to_rows`, быстрый pre-check ASCII/Cyrillic в `Emoji::contains` с тестом эквивалентности, однопроходный `rich::build_runs` без `Vec<char>`, per-chat `rich_runs_cache` в `messages_ui` | `engine.rs`, `emoji.rs`, `rich.rs`, `chat.rs`, `lib.rs` |
| Интерфейс каналов и полноценная ветка комментариев Telegram | ✅ плоская 42px нижняя полоса канала `channel_bottom_bar`, интегрированная строка комментариев поста (`history_comments` + счётчик с плюрализацией + `›`), встроенная на всю высоту панели ветка обсуждения (`discussion_panel`: `←`, карточка поста, пузыри с ответами и реакциями, оптимистичная отправка комментария) | `chat.rs`, `engine.rs`, `worker.rs`, `lib.rs` |
| 5 новых функций Telegram (облачные черновики, отметка непрочитанным + `@`/`❤`, мини-игры Dice, просмотр и установка стикерпаков, редактор ДР/`@username`) | ✅ `saveDraft`/`getAllDrafts` + автосохранение + превью `Черновик:`; `markDialogUnread`, фильтр «Непрочитанные», FAB `@` и `❤`; `InputMediaDice` (`🎲 🎯 🏀 ⚽ 🎰 🎳`, анимация и расшифровка); `getStickerSet`/`installStickerSet`/`uninstallStickerSet` + модальное окно; `updateBirthday`/`updateUsername` с валидацией | `engine.rs`, `engine_ext.rs`, `worker.rs`, `lib.rs`, `chat.rs`, `media_ui.rs`, `panels.rs`, `tg_settings.rs` |

## Новые функции этапа 34 (v0.22.0)

| Функция | Статус | Где |
|---|---|---|
| Похожие каналы (`channels.getChannelRecommendations`) + сворачивание/скрытие | ✅ `Engine::similar_channels`, секция в правой панели канала, привязка `collapse_similar_channels` и `hide_similar_channels` | `engine_ext.rs`, `worker.rs`, `lib.rs`, `chat.rs` |
| Глобальный журнал удалённых + экспорт истории/правок чата (TXT / JSON) | ✅ `MessageStore::search_deleted_global` / `stats`, `global_deleted_window`, `export_audit_txt` / `export_audit_json` | `store.rs`, `engine.rs`, `worker.rs`, `features_ui.rs`, `chat.rs`, `settings_ui.rs` |
| Прочитать локально (без отправки в Telegram) + настройка меню | ✅ `local_read_dialog` / `local_read_all`, `local_read_dates`, `drawer_l_read_toggle` («Прочитать локально») и пункты в контекстном меню/`⋮` | `lib.rs`, `chat.rs`, `settings_ui.rs` |
| Переход к дате (календарь + `messages.getHistory { offset_date }`) | ✅ `Engine::history_at_date`, `parse_date_dmy`, `date_jump_window` (быстрые кнопки + сетка календаря + ввод ДД.ММ.ГГГГ), кликабельные пилюли даты | `engine_ext.rs`, `worker.rs`, `features_ui.rs`, `chat.rs` |
| Общие группы + добавление/правка/удаление контакта в профиле | ✅ `Engine::common_chats` (`messages.getCommonChats`), `add_contact` / `delete_contact`, вкладка «Общие» и блок контакта в `info_ui` | `engine_ext.rs`, `worker.rs`, `lib.rs`, `chat.rs` |

## Новые функции этапа 35 (v0.23.0)

| Функция | Статус | Где |
|---|---|---|
| Исправление мобильного интерфейса входа и нативного Android-ввода во всех полях | ✅ безопасные отступы `android-safe-top = 54.0` / `android-safe-bottom = 18.0`, выравнивание клавиатуры `paint_android_keypad` и ширины колонки `col_w`, центрирование текста `.align(Align2::LEFT_CENTER)`, модальное окно выбора страны `country_picker_window`, глобальный `poll_android_bridge` в начале `App::ui` и хуки `open_android_input`/`hook_android_text` в чате/поиске/настройках/модалках | `lib.rs`, `chat.rs`, `tg_settings.rs` |
| Исправление входа по QR-коду (`ExportLoginToken`, cross-DC 2FA и чёткость QR) | ✅ устранено 2-сек перезаписывание токена (ожидание `has_login_token_update` + `sleep_until` по expiry), `password_step_in_dc` для 2FA после `MigrateTo`, кнопка «Обновить QR», сплошные модули `render_qr_image` и уменьшенный центр-бейдж `rg-qr-tex-cache-v4` с сохранением шаблона выравнивания Version 7 QR | `qr.rs`, `theme.rs`, `worker.rs`, `lib.rs` |
| Надёжное сохранение удалённых сообщений (свои + офлайн + перезапуски) | ✅ столбцы `outgoing` и `sender` в `Saved`, `remember_full`/`remember_many_full`/`upsert_deleted`/`mark_missing_deleted`/`deleted_in_range` без перезаписи `deleted=1`, `UpdatesConfiguration { catch_up: true }`, `delete_with_backup`, слияние удалённых в `history_before`, сохранение `messages.db*` в `wipe_if_logged_out`, фоновая синхронизация 10 верхних чатов | `rg-ayu/src/store.rs`, `engine.rs`, `engine_ext.rs`, `worker.rs`, `chat.rs`, `lib.rs` |
| Глобальный поиск сообщений (`messages.searchGlobal`) | ✅ `Engine::search_global_messages` (`GlobalMessageHit`), переключатель `Чаты и люди` / `Сообщения` и фильтры `Все`/`Личные`/`Группы`/`Каналы` в левой колонке, переход в чат с подсветкой (`pending_jump_to_msg`) | `engine_ext.rs`, `worker.rs`, `lib.rs` |
| Очистка истории чата и выход из группы/канала | ✅ `Engine::clear_history` (`messages.deleteHistory`/`channels.deleteHistory`) и `Engine::leave_chat` (`channels.leaveChannel`/`messages.deleteChatUser`), пункты в контекстном меню и `⋮`, диалог подтверждения `chat_action_confirm_window` | `engine_ext.rs`, `worker.rs`, `lib.rs`, `chat.rs`, `features_ui.rs` |
| Список всех закреплённых сообщений + «Открепить все» | ✅ `Engine::pinned_messages` (`InputMessagesFilterPinned`) и `Engine::unpin_all_messages` (`messages.unpinAllMessages`), кнопка «Все» в плашке закрепа и меню `⋮`, модалка `pinned_list_window` | `engine_ext.rs`, `worker.rs`, `chat.rs`, `features_ui.rs` |
| Расширенная пересылка AyuGram (`ForwardOptions`) | ✅ `Engine::forward_advanced` (`drop_author`, `drop_media_captions`, `silent`) и 3 чекбокса в окнах одиночной и пакетной пересылки | `engine_ext.rs`, `worker.rs`, `chat.rs` |
| Ссылки на папки Telegram (`chatlists.*` + `t.me/addlist/...`) | ✅ `TgLink::ChatlistInvite`, `export_folder_invite`/`check_folder_invite`/`join_folder_invite`, «Поделиться ссылкой» и «Добавить папку по ссылке» в настройках папок, предпросмотр и подключение `folder_invite_window` | `engine.rs`, `engine_ext.rs`, `worker.rs`, `tg_settings.rs`, `features_ui.rs`, `lib.rs` |

## Новые функции этапа 36 (v0.24.0)

| Функция | Статус | Где |
|---|---|---|
| Исправление входа Android, оффлайн-устойчивость и перенаправление DC | ✅ `isolated_connection_params`, `invoke_with_dc_migrate`, `resend_login_code`, `SentCodeInfo::code_length`, `check_password_safe` с таймаутами на `SRP_ID_INVALID`, `normalize_phone`/`format_phone_input`, атомарный `INPUT_EVENT` + `CAN:` fallback, `updates.getState`, сохранение `Stage::Ready` с кэшем при оффлайне | `rg-core/{lib,auth}.rs`, `countries.rs`, `worker.rs`, `lib.rs`, `android_jni.rs`, `build_android_bridge.py` |
| Стеклянный дизайн Android (`window_opacity` 50..=100 + safe insets) | ✅ GPU-фон `paint_android_glass_backdrop`, `paint_glass_bar`, `setupGlassWindow` + `getSafeInsetsPx` (250 мс кэш) в `RgBridge.java`, стеклянные карточки/клавиатура/шапка/композер | `theme.rs`, `lib.rs`, `android_jni.rs`, `build_android_bridge.py` |
| Автотестер APK и Java/JNI-моста без эмулятора | ✅ `test_rg_bridge.py` (`RgBridgeTest`), проверка `rg_bridge.dex`, Full+Lite `aarch64-linux-android` cross-check, аудит APK и `cargo test --workspace` в `scripts/test-apk.sh` | `scripts/dev/test_rg_bridge.py`, `scripts/test-apk.sh` |
| Мгновенная загрузка и переключение чатов на ПК | ✅ `write_lock`, частичный индекс, `scan_range_for_merge`, `prune_old_live(1500)`, последовательный `run_updates`, `history_before` `.limit`, неблокирующие `tokio::spawn` + `spawn_blocking`, 64-чатовый LRU `history_cache` | `rg-ayu/src/store.rs`, `rg-core/src/engine.rs`, `rg-ui/src/{worker,chat,lib}.rs` |
| Анимации стикеров и Custom Emoji без лишней нагрузки на CPU/RAM | ✅ `TgsScratch` (`render_frame_into`), трекинг видимых `visible_tgs_docs`/`visible_tgs_msgs`, защита от вытеснения видимых, 0% CPU в простое, сброс `msg_heights` на `Ev::Translated` | `lottie.rs`, `panels.rs`, `chat.rs`, `lib.rs` |
| Кликабельные пилюли реакций (`ReactionPill`) | ✅ Обычные, кастомные эмодзи и Paid ★ пилюли под сообщениями (`extract_reaction_pills`, `toggle_reaction`, `Cmd::ReactToggle`) | `engine.rs`, `worker.rs`, `chat.rs`, `lib.rs` |
| Предпросмотр чата без прочтения (`Chat Peek`) | ✅ `Cmd::PeekHistory` / `Ev::PeekHistory` / `DialogAct::Peek` и окно `peek_window` без отправки `messages.readHistory` | `worker.rs`, `chat.rs`, `lib.rs` |
| Поиск участников и фильтры `Все`/`Админы`/`Боты` | ✅ `ParticipantFilter` + `channel_participants_filtered` (`Cmd::ParticipantsFiltered`) с поиском в правой панели | `engine.rs`, `worker.rs`, `chat.rs`, `lib.rs` |
| 3-точечная волна набора текста + Ghost-безопасный `setTyping` | ✅ `paint_typing_wave` в шапке/списке, `Engine::typing_allowed` (Ghost Mode + `PrivacyShieldSettings`), `Cmd::SetTyping` | `engine.rs`, `worker.rs`, `chat.rs`, `lib.rs` |
| Контакты и геопозиция: отправка и карточки | ✅ `MediaKind::Contact` / `MediaKind::Geo`, `send_contact` / `send_geo`, богатые карточки с копированием и модальные окна отправки | `media.rs`, `engine.rs`, `worker.rs`, `media_ui.rs`, `lib.rs` |

## Порядок дальнейших этапов
1. **Этап 9 — лента как у Telegram:** форматирование текста и ссылки, GIF/видео/голосовые, анимированные стикеры, альбомы.
2. **Этап 10 — навигация:** папки, архив, контакты, избранное, профили, глобальный поиск.
3. **Этап 11 — системные:** уведомления, трей, автозапуск, настройки Telegram, несколько аккаунтов.
4. **Этап 12 — Android:** SDK/NDK, сборка APK (ждёт загрузки инструментов).
5. **Этап 13 — звонки.**
6. **Этап 7 (скорость):** замеры «до/после» против `AyuGram.exe` — запуск, ОЗУ, прокрутка. Эталон: `AyuGram.exe` 240 МБ, у Rustgramm release ≈ 11 МБ.
