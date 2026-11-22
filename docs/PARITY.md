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
| Окно непрозрачное на старте (100%) + прозрачность окна и кнопок + блюр (Acrylic/iOS), оранжевые основные кнопки | ✅ этап 16 |
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

## Порядок дальнейших этапов
1. **Этап 9 — лента как у Telegram:** форматирование текста и ссылки, GIF/видео/голосовые, анимированные стикеры, альбомы.
2. **Этап 10 — навигация:** папки, архив, контакты, избранное, профили, глобальный поиск.
3. **Этап 11 — системные:** уведомления, трей, автозапуск, настройки Telegram, несколько аккаунтов.
4. **Этап 12 — Android:** SDK/NDK, сборка APK (ждёт загрузки инструментов).
5. **Этап 13 — звонки.**
6. **Этап 7 (скорость):** замеры «до/после» против `AyuGram.exe` — запуск, ОЗУ, прокрутка. Эталон: `AyuGram.exe` 240 МБ, у Rustgramm release ≈ 11 МБ.
