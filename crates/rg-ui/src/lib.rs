//! Общий интерфейс Rustgramm для ПК и Android в стиле Telegram Desktop (AyuGram v7.2.9),
//! синий везде заменён на оранжевый. Данные уходят только на серверы Telegram (см. `rg-net`).

pub mod android_jni;
mod audio_io;
mod chat;
mod countries;
pub mod emoji;
mod features_ui;
mod frame;
mod glass;
pub mod icons;
mod kit;
mod login_anim;
mod lottie;
mod media_editor;
mod media_ui;
mod panels;
mod rich;
mod settings_ui;
mod shot;
pub mod stream;
mod tg_settings;
pub mod theme;
pub mod theme_pack;
mod tray;
pub mod updater;
mod win;
#[cfg(windows)]
mod video;
pub mod worker;

use std::collections::{HashMap, HashSet, VecDeque};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, FontId, RichText, Sense, Vec2};
use rg_core::engine::{
    ContactRow, DialogRow, DiscussionThread, FolderRow, ForumTopicRow, MessageRow, StickerRow,
    StickerSetRow, StoryRow, folder_matches,
};
use rg_settings::Settings;
use tokio::sync::mpsc::UnboundedSender;

pub use worker::{Cmd, Ev, Stage};

use theme::*;

#[allow(dead_code)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AndroidPickTarget {
    ChatAttach,
    Story,
    Theme,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
enum LoginButtonRight {
    None,
    Chevron(bool),
    Keyboard,
}

/// Копирует текст в системный буфер обмена на Windows/Desktop (`arboard` + `egui`) и Android (`ClipboardManager`).
pub(crate) fn copy_text_to_clipboard(ctx: &egui::Context, text: &str) -> bool {
    ctx.copy_text(text.to_owned());
    #[cfg(not(target_os = "android"))]
    {
        if let Ok(mut cb) = arboard::Clipboard::new()
            && cb.set_text(text.to_owned()).is_ok()
        {
            return true;
        }
        false
    }
    #[cfg(target_os = "android")]
    {
        android_jni::copy_text(text)
    }
}

struct App {
    cmd: UnboundedSender<Cmd>,
    ev: Receiver<Ev>,
    stage: Stage,
    me: String,
    code: String,
    password: String,
    dialogs: Vec<DialogRow>,
    selected: Option<usize>,
    messages: Vec<MessageRow>,
    input: String,
    search: String,
    emoji_open: bool,
    emoji_pinned: bool,
    emoji_jump: Option<usize>,
    emoji_btn: egui::Rect,
    emoji_search: String,
    emoji_kw_requested: bool,
    emoji: Option<emoji::Emoji>,
    emoji_tried: bool,
    settings: Settings,
    status: String,
    error: String,
    textures: HashMap<i32, egui::TextureHandle>,
    /// Фото профилей по идентификатору чата и уже запрошенные.
    avatars: HashMap<i64, egui::TextureHandle>,
    avatar_req: HashSet<i64>,
    requested: HashSet<i32>,
    key_id: String,
    key_hash: String,
    proxy: String,
    proxy_open: bool,
    now: i64,
    /// Когда показана текущая ошибка (unix); через 6 секунд она скрывается.
    error_at: i64,
    /// Ответ на сообщение (идентификатор и начало текста).
    reply: Option<(i32, String)>,
    /// Идентификатор редактируемого сообщения.
    editing: Option<i32>,
    search_open: bool,
    chat_search: String,
    /// Результаты поиска по чату (`None` — обычная лента).
    found: Option<Vec<MessageRow>>,
    forward: Option<i32>,
    forward_filter: String,
    forward_no_author: bool,
    translated: HashMap<i32, String>,
    confirm_delete: Option<i32>,
    /// Адрес, ожидающий подтверждения «Открыть ссылку?».
    link_confirm: Option<String>,
    /// Раскрытые спойлеры (сообщение, номер куска).
    spoilers: HashSet<(i32, usize)>,
    /// Высота содержимого ленты без отступа снизу (для прижатия коротких переписок к низу).
    msgs_h: f32,
    /// Открытая в просмотрщике картинка (идентификатор сообщения) и масштаб.
    viewer: Option<i32>,
    viewer_zoom: f32,
    details: Option<i32>,
    edit_history: Option<(i32, Vec<String>)>,
    hidden: HashSet<i32>,
    settings_open: bool,
    settings_tab: usize,
    filter_new: String,
    drawer_open: bool,
    /// Показан список архива вместо основного.
    in_archive: bool,
    /// Папки чатов и выбранная (`None` — «Все чаты»).
    folders: Vec<FolderRow>,
    folder: Option<i32>,
    /// Окно контактов.
    contacts_open: bool,
    contacts: Vec<ContactRow>,
    contact_search: String,
    contact_av_req: HashSet<i64>,
    /// Вкладка панели: 0 — эмодзи, 1 — стикеры.
    emoji_tab: usize,
    sticker_sets: Vec<StickerSetRow>,
    sticker_cur: usize,
    sticker_lists: HashMap<i64, Vec<StickerRow>>,
    sticker_tex: HashMap<i64, egui::TextureHandle>,
    sticker_req: HashSet<i64>,
    stickers_requested: bool,
    /// Установленные и рекомендуемые наборы эмодзи, выбранный набор, окно «Добавить».
    emoji_sets: Vec<StickerSetRow>,
    /// Открытый ролик: путь или потоковый локальный адрес (url, название).
    player_path: Option<std::path::PathBuf>,
    player_stream: Option<(String, String)>,
    /// Идущие загрузки вложений: сообщение → (скачано, всего).
    fetching: HashMap<i32, (u64, u64)>,
    /// Окно «Настройки» Telegram.
    tgs: tg_settings::State,
    /// Сообщения, вложения которых уже в кэше (открываются мгновенно).
    cached_media: HashSet<i32>,
    /// Скачанные GIF для автозапуска в ленте и отслеживание видимой карточки.
    gif_paths: HashMap<i32, std::path::PathBuf>,
    gif_auto_req: HashSet<i32>,
    inline_gif_rect: Option<(i32, egui::Rect)>,
    #[cfg(windows)]
    inline_gif: Option<video::InlineGifView>,
    /// Кэш истории уже открытых чатов (мгновенное переключение за 0 мс).
    history_cache: HashMap<i64, Vec<MessageRow>>,
    history_cache_order: std::collections::VecDeque<i64>,
    history_fresh_at: HashMap<i64, std::time::Instant>,
    history_loading: bool,
    /// Время загрузки миниатюр (для плавного проявления).
    img_arrived: HashMap<i32, f64>,
    /// Глобальный поиск пользователей и каналов в левой колонке.
    global_results: Vec<ContactRow>,
    global_last_q: String,
    global_query_at: Option<std::time::Instant>,
    /// Сохранённые GIF и окно создания группы/канала.
    gif_list: Vec<StickerRow>,
    gifs_requested: bool,
    /// Создание: 0 — нет, 1 — группа, 2 — канал, 3 — пригласить в открытый чат.
    create_kind: u8,
    create_title: String,
    create_about: String,
    create_username: String,
    create_public: bool,
    create_members: HashSet<i64>,
    create_member_query: String,
    create_member_results: Vec<ContactRow>,
    create_member_map: HashMap<i64, ContactRow>,
    create_member_last_q: String,
    create_member_at: Option<std::time::Instant>,
    /// Правая панель сведений о чате.
    info_open: bool,
    info_tab: usize,
    peer_infos: HashMap<i64, rg_core::engine::PeerInfo>,
    info_req: HashSet<i64>,
    participants: HashMap<i64, Vec<ContactRow>>,
    participants_req: HashSet<i64>,
    /// Пользовательские Premium-эмодзи в поле ввода `(char_start, char_len, doc_id)` и запрошенные текстуры.
    input_custom_emojis: Vec<(usize, usize, i64)>,
    custom_emoji_req: HashSet<i64>,
    last_toast: Option<std::time::Instant>,
    /// Антиспам-фильтр уведомлений (>3 сообщений в секунду) и 350-мс буфер перед звуком.
    notify_gate: NotifyGate,
    /// Встроенные карточки уведомлений Rustgramm (когда выключены системные уведомления Windows).
    toasts: Vec<InAppToast>,
    /// Локальная блокировка по код-паролю.
    app_locked: bool,
    lock_input: String,
    lock_err: String,
    passcode_checked: bool,
    /// Встроенный аудиоплеер (музыка профиля, голосовые, аудиофайлы).
    audio_path: Option<std::path::PathBuf>,
    audio_stream: Option<String>,
    audio_title: String,
    audio_dur: u32,
    audio_vol: f32,
    #[cfg(windows)]
    audio_view: Option<video::AudioView>,
    #[cfg(windows)]
    player: Option<video::VideoView>,
    /// Реестр плагинов Rustgramm (встроенные + пользовательские `.plugin`).
    plugins: rg_ayu::plugins::PluginRegistry,
    /// Время открытия окна настроек Rustgramm (для плавной пружинной анимации).
    settings_opened_at: Option<std::time::Instant>,
    /// История переходов по чатам для боковых кнопок мыши (Back/Forward) и Alt+←/→.
    chat_nav_back: Vec<i64>,
    chat_nav_fwd: Vec<i64>,
    nav_jumping: bool,
    /// Стек отката/возврата текста в поле ввода (`Ctrl+Z` / `Ctrl+Y` и кнопки мыши).
    input_undo: Vec<String>,
    input_redo: Vec<String>,
    /// Якорь быстрого скролла при нажатии/зажатии колёсика мыши (Middle Click Auto-Scroll).
    autoscroll_anchor: Option<egui::Pos2>,
    refresh_at: Option<std::time::Instant>,
    featured_sets: Vec<StickerSetRow>,
    emoji_sets_requested: bool,
    emoji_pack: Option<usize>,
    emoji_add_open: bool,
    set_icon_req: HashSet<i64>,
    /// Идентификаторы уже показанных сообщений и время появления новых (для плавного входа).
    known: HashSet<i32>,
    arrived: HashMap<i32, f64>,
    chat_opened: std::time::Instant,
    logout_confirm: bool,
    shadow_new: String,
    /// Раскрытые строки со счётчиком «n/m» (битовая маска).
    expanded: u32,
    older_pending: bool,
    history_end: bool,
    scroll_to: Option<i32>,
    /// Системный трей Windows и состояние окна.
    tray: Option<tray::TrayManager>,
    is_demo: bool,
    quitting: bool,
    hidden_to_tray: bool,
    autostart_minimized: bool,
    pending_tg_uri: Option<String>,
    /// Декодированные `.tgs` (Lottie) анимации стикеров/эмодзи (`doc_id` -> `TgsAnim`) и сообщений (`msg_id`).
    tgs_docs: HashMap<i64, std::sync::Arc<lottie::TgsAnim>>,
    tgs_msgs: HashMap<i32, std::sync::Arc<lottie::TgsAnim>>,
    tgs_msg_req: HashSet<i32>,
    tgs_frame_cache: HashMap<(i64, usize), egui::TextureHandle>,
    /// Интерактивное окно «Снимок сообщения» (`message_shot_box.cpp`).
    shot_modal: Option<ShotModal>,
    /// Множественное выделение сообщений в ленте и пакетная пересылка (`ayu_forward.cpp`).
    selected_msgs: HashSet<i32>,
    forward_batch: Option<Vec<i32>>,
    /// Журнал удалённых и изменённых сообщений чата (`ayu/ui/message_history`).
    audit_history: Option<(i64, Vec<rg_ayu::store::SavedAudit>)>,
    /// Кэшированные идентификаторы заблокированных пользователей (`hide_from_blocked`).
    blocked_ids: HashSet<i64>,
    /// Выбор моноширинного шрифта в настройках.
    font_picker_open: bool,
    font_search: String,
    sys_fonts: Vec<String>,
    /// Активные индикаторы «печатает...» по ID чата (`(текст, время последнего события)`).
    typing: HashMap<i64, (String, std::time::Instant)>,
    /// Кэш закреплённых сообщений для баннера в шапке (`chat_id -> MessageRow`).
    pinned_msgs: HashMap<i64, MessageRow>,
    pinned_req: HashSet<(i64, i32)>,
    /// Все закреплённые сообщения по чату для сегментированного индикатора и циклического перехода.
    pinned_lists: HashMap<i64, Vec<MessageRow>>,
    pinned_cur_idx: HashMap<i64, usize>,
    pinned_list_req: HashSet<i64>,
    /// Подтверждение открепления сообщения из верхней плашки `(dialog_id, msg_id)`.
    confirm_unpin: Option<(i64, i32)>,
    /// Граница `read_inbox_max_id` при открытии чата (плашка «Непрочитанные сообщения»).
    open_unread_anchor: Option<i32>,
    /// Выбранный индекс в автодополнении `@упоминаний` и фильтр/индекс поиска по чату.
    mention_sel_idx: usize,
    chat_search_filter: rg_core::engine::ChatSearchFilter,
    chat_search_idx: usize,
    /// Записанное голосовое сообщение, ожидающее подтверждения перед отправкой `(path, dur_secs, waveform, reply_to)`.
    pending_voice_confirm: Option<(std::path::PathBuf, u32, Vec<u8>, Option<i32>)>,
    /// Форумы и темы супергрупп (`chat_id -> topics`).
    forum_topics: HashMap<i64, Vec<ForumTopicRow>>,
    forum_req: HashSet<i64>,
    active_topic: Option<i32>,
    topic_msgs: Option<(i32, Vec<MessageRow>)>,
    /// Комментарии к посту канала (`messages.getDiscussionMessage` + `messages.getReplies`).
    discussion: Option<DiscussionThread>,
    comment_input: String,
    /// Активные истории контактов и окно просмотра.
    stories: Vec<StoryRow>,
    story_open: Option<usize>,
    story_ghost_ask: Option<usize>,
    /// Перетащенные в окно файлы (`Drag & Drop`) и подпись к отправке.
    drop_paths: Vec<std::path::PathBuf>,
    drop_caption: String,
    /// Подтверждение перед отправкой стикера или GIF (`sticker_confirmation` / `gif_confirmation`).
    /// `(is_gif, doc_or_msg_id, reply_to)`.
    pending_confirm_media: Option<(bool, i64, Option<i32>)>,
    /// Фильтр ленты по конкретному отправителю (`menu_user_messages`).
    user_filter: Option<(i64, String)>,
    /// Попапы поля ввода: команды бота (`/`), автоудаление, панель форматирования.
    cmd_popup_open: bool,
    cmd_sel_idx: usize,
    ttl_popup_open: bool,
    format_bar_open: bool,
    composer_sel: Option<(usize, usize)>,
    /// Показать плавающую кнопку «Вниз» (FAB), если пользователь пролистал ленту вверх.
    scrolled_up: bool,
    /// Кэш измеренных высот пузырей сообщений (`msg_id -> px`) для виртуализации ленты в больших чатах.
    msg_heights: HashMap<i32, f32>,
    msg_last_avail: f32,
    /// Показать реплай-клавиатуру бота под строкой ввода.
    bot_keyboard_open: bool,
    /// Всплывающее уведомление (`alert = true`) от инлайн-кнопки бота.
    bot_alert: Option<String>,
    /// Открытое окно Mini App / WebApp бота (`(название, url)`).
    miniapp_modal: Option<(String, String)>,
    #[cfg(windows)]
    miniapp_view: Option<video::MiniAppView>,
    /// Модальное окно добавления изолированного второго аккаунта (анти-корреляция).
    add_account_open: bool,
    new_acc_label: String,
    new_acc_proxy: String,
    /// Модальное окно тотальной очистки данных у других людей и на сервере.
    wipe_modal_open: bool,
    wipe_confirm_step: bool,
    wipe_scope: rg_core::engine::WipeScope,
    /// Подсветка целевого сообщения после перехода к реплаю / поиску / закрепу `(msg_id, started_at)`.
    highlight_msg: Option<(i32, f64)>,
    /// Улучшенный вход по номеру телефона: выбранная страна, код, локальный номер, поиск страны, статус отправки.
    login_country_idx: usize,
    login_dial: String,
    login_phone_full: String,
    login_keep_signed_in: bool,
    login_code_hint: String,
    login_country_open: bool,
    login_country_query: String,
    login_busy: bool,
    login_last_phone: String,
    login_code_sent_at: Option<std::time::Instant>,
    login_code_len: u8,
    login_resend_secs: u64,
    /// Активные ожидания нативных диалогов Android (чтобы не дёргать JNI каждый кадр впустую).
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    android_webview_open: bool,
    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    android_input_pending: bool,
    /// Состояние автоскролла колёсиком мыши: удержание кнопки и факт перетаскивания.
    autoscroll_holding: bool,
    autoscroll_dragged: bool,
    /// Активная запись голосового сообщения с микрофона.
    voice_rec: Option<audio_io::VoiceRecorder>,
    /// Публикация своей истории и загруженные картинки просмотра историй.
    story_composer_open: bool,
    story_file_path: String,
    story_caption: String,
    story_privacy: rg_core::engine::StoryPrivacy,
    story_period: i32,
    story_pinned: bool,
    story_noforwards: bool,
    story_textures: HashMap<i32, egui::TextureHandle>,
    story_media_req: HashSet<i32>,
    story_reply_text: String,
    story_opened_at: Option<std::time::Instant>,
    /// Активный голосовой/видеозвонок и история звонков.
    active_call: Option<features_ui::ActiveCallUi>,
    calls_open: bool,
    call_history: Vec<rg_core::calls::CallLogEntry>,
    /// Секретные чаты E2E.
    secret_open: bool,
    secret_chats: Vec<rg_ayu::store::SecretChatRecord>,
    secret_selected: Option<i32>,
    secret_messages: Vec<rg_ayu::store::SecretMessageRecord>,
    secret_input: String,
    secret_show_key: bool,
    /// Администрирование групп и каналов.
    admin_modal: Option<features_ui::AdminModalState>,
    admin_log: Vec<rg_core::engine::AdminLogRow>,
    admin_invites: Vec<rg_core::engine::InviteLinkRow>,
    /// Плашка автообновления при выходе нового релиза на GitHub.
    update_available: Option<updater::UpdateInfo>,
    update_dismissed: bool,
    update_progress: Option<(u64, u64)>,
    /// Анимация ожидания ответа callback-кнопки бота `(msg_id, btn_text, clicked_at)`.
    bot_btn_pending: Option<(i32, String, f64)>,
    /// Время открытия Mini App (для плавной анимации) и режим разворачивания.
    #[cfg(windows)]
    miniapp_opened_at: Option<std::time::Instant>,
    #[cfg(windows)]
    miniapp_expanded: bool,
    /// Кэш фильтрации списка чатов для O(видимых_строк) при 5000+ чатах.
    dialogs_gen: u64,
    filtered_cache_key: (u64, String, bool, Option<i32>, bool),
    filtered_cache: Vec<usize>,
    archived_total_cache: usize,
    folder_tabs_cache_key: (u64, usize, bool, bool, bool),
    folder_tabs_cache: Vec<(Option<i32>, String, i32)>,
    archive_summary_cache_key: (u64, bool),
    archive_summary_cache: (String, i32),
    tray_unread_cache: ((u64, bool, bool), i32, bool),
    last_win_icon: Option<(usize, i32, bool, bool)>,
    last_capture_excluded: Option<bool>,
    wallpaper_failed_path: String,
    visible_tgs_msgs: HashSet<i32>,
    visible_tgs_docs: HashSet<i64>,
    tgs_scratch: lottie::TgsScratch,
    /// Модальный предпросмотр чата без отметки прочтения `(dialog_id, name, messages)`.
    peek_modal: Option<(i64, String, Vec<MessageRow>)>,
    participants_filter: rg_core::engine::ParticipantFilter,
    participants_query: String,
    last_typing_sent: Option<(usize, std::time::Instant)>,
    contact_modal_open: bool,
    send_contact_phone: String,
    send_contact_first: String,
    send_contact_last: String,
    geo_modal_open: bool,
    geo_lat: String,
    geo_lon: String,
    pending_tex_uploads: VecDeque<Ev>,
    /// Общее число участников в супергруппах/каналах (для догрузки по страницам).
    participants_total: HashMap<i64, i32>,
    /// Встроенный редактор фото/видео перед отправкой.
    media_editor: Option<media_editor::MediaEditorState>,
    /// Цель системного выбора файла через Android Intent.
    #[allow(dead_code)]
    android_pick_target: Option<AndroidPickTarget>,
    /// Живая статистика UDP/WebRTC медиа-потока звонка.
    call_media_stats: Option<rg_core::call_media::CallMediaStats>,
    call_stats_tick: Option<std::time::Instant>,
    /// Telegram Stars, Подарки (Star Gifts) и платные реакции.
    stars_modal_open: bool,
    stars_tab: u8,
    stars_summary: Option<rg_core::engine::StarsSummary>,
    star_gifts_catalog: Vec<rg_core::engine::StarGiftItem>,
    my_saved_gifts: Vec<rg_core::engine::SavedGiftItem>,
    peer_saved_gifts: HashMap<i64, Vec<rg_core::engine::SavedGiftItem>>,
    saved_gifts_req: HashSet<i64>,
    paid_react_msg: Option<i32>,
    paid_react_count: i32,
    paid_react_anon: bool,
    /// Telegram Бизнес: часы работы и быстрые ответы.
    business_open: bool,
    biz_hours_enabled: bool,
    biz_timezone: String,
    biz_start_h: i32,
    biz_end_h: i32,
    biz_weekdays_only: bool,
    quick_replies: Vec<rg_core::engine::QuickReplyItem>,
    /// Темы `.tdesktop-theme` / `.attheme`, живой конструктор палитры и обои чата.
    themes_open: bool,
    wallpaper_tex: Option<(String, egui::TextureHandle)>,
    /// Флаги загрузки стикеров/GIF и сохранённые избранные/недавние стикеры.
    stickers_loaded: bool,
    gifs_loaded: bool,
    faved_stickers: Vec<StickerRow>,
    recent_stickers: Vec<StickerRow>,
    /// Всплывающее мини-окно эмодзи/стикеров/GIF при наведении на смайл (на ПК) и таймеры антидребезга.
    emoji_hover_open: bool,
    emoji_hover_since: Option<f64>,
    emoji_leave_since: Option<f64>,
    /// Архив загружен лениво (по клику на «Архив», не на старте).
    archive_loaded: bool,
    /// Состояние просмотрщика историй (пауза, звук, ошибки загрузки, лайки).
    story_errors: HashMap<i32, String>,
    story_paused: bool,
    story_muted: bool,
    story_liked: HashSet<i32>,
    /// Показать/скрыть пароль 2FA (интерактивная обезьянка 🙈/🐵 как в Telegram).
    login_show_password: bool,
    /// Расшифровки голосовых / видеосообщений (Telegram API + локальный оффлайн-движок).
    transcriptions: HashMap<i32, String>,
    transcribing: HashSet<i32>,
    /// 1. Интерактивные чек-листы (To-Do).
    todo_modal_open: bool,
    todo_title: String,
    todo_items: Vec<String>,
    todo_others_complete: bool,
    todo_others_append: bool,
    /// 2. Анимированные эффекты сообщений и меню кнопки отправки.
    available_effects: Vec<rg_core::engine::MessageEffectItem>,
    effects_requested: bool,
    selected_effect: Option<i64>,
    send_menu_open: bool,
    effect_bursts: HashMap<i32, f64>,
    /// 3. Отложенные сообщения и личные напоминания.
    scheduled_open: bool,
    scheduled_msgs: Vec<MessageRow>,
    schedule_picker_open: bool,
    schedule_delay_mins: i32,
    /// 4. Теги «Избранного» (Saved Messages Tags).
    saved_tags: Vec<rg_core::engine::SavedTagItem>,
    saved_tag_filter: Option<String>,
    saved_tags_req: bool,
    saved_tag_edit: Option<(String, String)>,
    /// 5. Отправка от имени канала / профиля (Send As).
    send_as_map: HashMap<i64, Vec<rg_core::engine::SendAsItem>>,
    send_as_req: HashSet<i64>,
    selected_send_as: HashMap<i64, grammers_session::types::PeerRef>,
    send_as_popup_open: bool,
    /// 6. Встроенный конструктор опросов и викторин.
    poll_modal_open: bool,
    poll_question: String,
    poll_options: Vec<String>,
    poll_anonymous: bool,
    poll_multiple: bool,
    poll_quiz: bool,
    poll_correct: usize,
    poll_explanation: String,
    /// 7. Акцентный цвет профиля и ответов (`account.updateColor`).
    profile_accent_color: Option<i32>,
    replies_accent_color: Option<i32>,
    /// 8. Глобальный поиск по хэштегам в публичных каналах.
    hashtag_modal: Option<(String, Vec<MessageRow>)>,
    /// 9. Медленный режим (выбранная задержка в окне администрирования).
    admin_slowmode_secs: i32,
    /// 10. Палитра быстрого перехода `Ctrl+K` (Quick Switcher).
    palette_open: bool,
    palette_query: String,
    palette_sel: usize,
    /// Перевод черновика перед отправкой (`translateBeforeSend`).
    draft_translating: bool,
    translate_draft_popup: bool,
    /// Живые результаты инлайн-бота (`@bot запрос`).
    inline_bot_key: Option<(String, String)>,
    inline_bot_at: Option<std::time::Instant>,
    inline_bot_page: Option<rg_core::engine::InlineBotResultsPage>,
    inline_bot_loading: bool,
    inline_bot_error: String,
    /// Доступные реакции сервера (для выбора быстрой реакции).
    available_reactions: Vec<rg_core::engine::AvailableReactionRow>,
    available_reactions_req: bool,
    /// Окно выбора эмодзи-статуса аккаунта и его таймера.
    emoji_status_open: bool,
    default_emoji_statuses: Vec<i64>,
    default_emoji_statuses_req: bool,
    emoji_status_ttl: i32,
    /// Детали сообщения: активная вкладка (0=Мета, 1=Прочитали, 2=Реакции) и загруженные списки.
    details_tab: u8,
    msg_activity: Option<(i32, Vec<rg_core::engine::ReadParticipantRow>, Vec<rg_core::engine::MessageReactionPeerRow>)>,
    /// Генератор отрицательных временных ID для мгновенной оптимистичной отправки.
    next_temp_msg_id: i32,
    /// Облачные черновики по `dialog_id`: `(текст, reply_to)`.
    drafts: HashMap<i64, (String, Option<i32>)>,
    /// Отложенная отправка черновика на сервер `(dialog_idx, время_последней_правки)`.
    draft_dirty_at: Option<(usize, f64)>,
    /// Фильтр списка диалогов «Только непрочитанные».
    unread_filter_only: bool,
    /// Открытое модальное окно просмотра и установки набора стикеров.
    sticker_pack_modal: Option<rg_core::engine::StickerPackDetails>,
    /// Кэш разобранных текстовых прогонов `(msg_id, len)` для быстрого скролла ленты.
    rich_runs_cache: HashMap<(i32, usize), Vec<rich::Run>>,
    /// Отслеживание потери фокуса для мягкой дозагрузки пропущенного.
    last_focus_lost_at: Option<f64>,
    was_focused: bool,
    /// Выбранный комментарий-цель ответа `(msg_id, имя_автора)` в ветке обсуждения.
    disc_reply_to: Option<(i32, String)>,
    /// Поля редактора `@username` и даты рождения в «Моём профиле».
    profile_username_edit: String,
    profile_bday_day: String,
    profile_bday_month: String,
    profile_bday_year: String,
    profile_bday_synced: bool,
    /// Время появления анимации броска/игры (`🎲 🎯 🏀 ⚽ 🎰 🎳`).
    dice_spawn_times: HashMap<i32, f64>,
    /// Похожие каналы (`channel_id -> список`), уже запрошенные каналы и раскрытые карточки.
    similar_channels: HashMap<i64, Vec<rg_core::engine::SimilarChannelRow>>,
    similar_req: HashSet<i64>,
    similar_expanded: HashSet<i64>,
    /// Глобальный журнал удалённых сообщений по всем чатам.
    global_deleted_open: bool,
    global_deleted_query: String,
    global_deleted_items: Vec<rg_ayu::store::Saved>,
    global_deleted_stats: (usize, usize, usize),
    /// Локально прочитанные чаты в режиме призрака (`dialog_id -> date`).
    local_read_dates: HashMap<i64, i64>,
    /// Переход к дате (календарь + `messages.getHistory { offset_date }`).
    date_jump_open: bool,
    date_jump_input: String,
    date_jump_year: i32,
    date_jump_month: u32,
    date_jump_active: bool,
    /// Общие группы с собеседником и редактор контакта в правой панели.
    common_chats: HashMap<i64, Vec<ContactRow>>,
    common_chats_req: HashSet<i64>,
    contact_edit_open: Option<i64>,
    contact_first: String,
    contact_last: String,
    /// 1. Глобальный поиск по сообщениям во всех чатах.
    global_msg_mode: bool,
    global_msg_scope: u8,
    global_msg_hits: Vec<rg_core::engine::GlobalMessageHit>,
    global_msg_last_q: String,
    global_msg_at: Option<std::time::Instant>,
    global_msg_loading: bool,
    pending_jump_to_msg: Option<(i64, i32, i64)>,
    /// 2. Подтверждение очистки истории или выхода из группы/канала `(dialog_id, name, is_leave, revoke)`.
    confirm_chat_action: Option<(i64, String, bool, bool)>,
    /// 3. Модальное окно всех закреплённых сообщений чата.
    pinned_list_open: bool,
    pinned_list_did: Option<i64>,
    pinned_list: Vec<MessageRow>,
    /// 4. Расширенная пересылка (скрытие подписи к медиа и без звука).
    forward_no_captions: bool,
    forward_silent: bool,
    /// 5. Ссылки-приглашения на папки чатов (`t.me/addlist/...`).
    folder_invite_preview: Option<rg_core::engine::FolderInvitePreview>,
    folder_invite_url: Option<String>,
    /// Время клика по реакции `(msg_id, emoji)` для анимации отклика.
    rx_pop: HashMap<(i32, String), f64>,
    /// Время открытия текущего чата (для плавного проявления ленты).
    chat_opened_at: f64,
    /// Отслеживание последнего toast-статуса для автоскрытия и плавного появления.
    status_last: String,
    status_at: i64,
    /// Попап центра уведомлений (#4), лента входящих уведомлений и поля новой UI-темы/шторки.
    notify_popover_open: bool,
    notify_feed: VecDeque<(i64, String, String, i64, bool)>,
    drawer_filter: String,
    drawer_accounts_open: bool,
    new_topic_open: bool,
    new_topic_title: String,
}

pub(crate) struct ShotModal {
    pub messages: Vec<MessageRow>,
    pub theme: shot::ShotTheme,
    pub cached_key: Option<(shot::ShotTheme, bool, bool, bool, bool, bool, bool)>,
    pub rgba: Vec<u8>,
    pub dims: (u32, u32),
    pub preview_tex: Option<egui::TextureHandle>,
}

/// Карточка встроенного уведомления Rustgramm (позиционируется в выбранном углу экрана).
struct InAppToast {
    title: String,
    body: String,
    at: std::time::Instant,
}

/// Защита уведомлений от спам-флуда (> 3 сообщений/сек) и проверка глобальной/локальной тишины.
#[derive(Default)]
struct NotifyGate {
    global_hits: VecDeque<std::time::Instant>,
    peer_hits: HashMap<i64, VecDeque<std::time::Instant>>,
    spam_muted_until: Option<std::time::Instant>,
    pending: Option<(std::time::Instant, String, String)>,
}

impl NotifyGate {
    /// Фиксирует входящее сообщение и возвращает `true`, если включён антиспам (>3 сообщений/сек).
    fn record_incoming(&mut self, now: std::time::Instant, peer_id: i64) -> bool {
        let win = std::time::Duration::from_secs(1);
        while self
            .global_hits
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= win)
        {
            self.global_hits.pop_front();
        }
        self.global_hits.push_back(now);

        let peer_q = self.peer_hits.entry(peer_id).or_default();
        while peer_q
            .front()
            .is_some_and(|t| now.saturating_duration_since(*t) >= win)
        {
            peer_q.pop_front();
        }
        peer_q.push_back(now);
        let peer_len = peer_q.len();

        if self.peer_hits.len() > 64 {
            self.peer_hits.retain(|k, q| {
                *k == peer_id
                    || q.back()
                        .is_some_and(|t| now.saturating_duration_since(*t) < win)
            });
        }

        if self.global_hits.len() > 3 || peer_len > 3 {
            self.spam_muted_until = Some(now + std::time::Duration::from_secs(6));
            self.pending = None;
            return true;
        }
        if self.is_spam_muted(now) {
            self.pending = None;
            return true;
        }
        false
    }

    fn is_spam_muted(&self, now: std::time::Instant) -> bool {
        self.spam_muted_until.is_some_and(|until| now < until)
    }

    fn enqueue(&mut self, now: std::time::Instant, title: String, body: String) {
        if !self.is_spam_muted(now) {
            self.pending = Some((now, title, body));
        }
    }

    fn take_ready(&mut self, now: std::time::Instant) -> Option<(String, String)> {
        if self.is_spam_muted(now) {
            self.pending = None;
            return None;
        }
        let ready = self
            .pending
            .as_ref()
            .is_some_and(|(at, _, _)| now.saturating_duration_since(*at) >= std::time::Duration::from_millis(350));
        if ready {
            self.pending.take().map(|(_, title, body)| (title, body))
        } else {
            None
        }
    }

    fn next_wakeup_ms(&self, now: std::time::Instant) -> Option<u64> {
        if self.is_spam_muted(now) {
            return None;
        }
        self.pending.as_ref().map(|(at, _, _)| {
            350u64
                .saturating_sub(now.saturating_duration_since(*at).as_millis() as u64)
                .max(20)
        })
    }
}

/// Возвращает `true`, если для этого чата звук/уведомления отключены (чат в муте, архиве,
/// не загружен или отключена вся его категория в глобальных настройках Telegram).
fn is_dialog_muted_for_notify(
    dialog: Option<&DialogRow>,
    account: Option<&rg_core::engine::AccountOverview>,
) -> bool {
    let Some(d) = dialog else {
        return true;
    };
    if d.archived || d.saved || d.muted || d.notify_explicit == Some(true) {
        return true;
    }
    if d.notify_explicit == Some(false) {
        return false;
    }
    let Some(acc) = account else {
        return true;
    };
    let scope = match d.kind {
        rg_core::engine::DialogKind::User | rg_core::engine::DialogKind::Bot => {
            rg_core::engine::NotifyScope::Users
        }
        rg_core::engine::DialogKind::Group => rg_core::engine::NotifyScope::Chats,
        rg_core::engine::DialogKind::Channel => rg_core::engine::NotifyScope::Broadcasts,
    };
    acc.notify
        .iter()
        .find(|n| n.scope == scope)
        .is_none_or(|n| !n.enabled)
}

fn link(ui: &mut egui::Ui, text: &str) -> egui::Response {
    ui.add(egui::Label::new(RichText::new(text).color(ACCENT)).sense(Sense::click()))
        .on_hover_cursor(egui::CursorIcon::PointingHand)
}

impl App {
    fn new(cmd: UnboundedSender<Cmd>, ev: Receiver<Ev>) -> Self {
        Self {
            cmd,
            ev,
            stage: Stage::Starting,
            me: String::new(),
            code: String::new(),
            password: String::new(),
            dialogs: Vec::new(),
            selected: None,
            messages: Vec::new(),
            input: String::new(),
            search: String::new(),
            emoji_open: false,
            emoji_pinned: false,
            emoji_jump: None,
            emoji_btn: egui::Rect::NOTHING,
            emoji_search: String::new(),
            emoji_kw_requested: false,
            emoji: None,
            emoji_tried: false,
            settings: Settings::default(),
            status: String::new(),
            error: String::new(),
            textures: HashMap::new(),
            avatars: HashMap::new(),
            avatar_req: HashSet::new(),
            requested: HashSet::new(),
            key_id: String::new(),
            key_hash: String::new(),
            proxy: String::new(),
            proxy_open: false,
            now: 0,
            error_at: 0,
            reply: None,
            editing: None,
            search_open: false,
            chat_search: String::new(),
            found: None,
            forward: None,
            forward_filter: String::new(),
            forward_no_author: false,
            translated: HashMap::new(),
            confirm_delete: None,
            link_confirm: None,
            spoilers: HashSet::new(),
            msgs_h: 0.0,
            viewer: None,
            viewer_zoom: 1.0,
            details: None,
            edit_history: None,
            hidden: HashSet::new(),
            settings_open: false,
            settings_tab: 0,
            filter_new: String::new(),
            drawer_open: false,
            in_archive: false,
            folders: Vec::new(),
            folder: None,
            contacts_open: false,
            contacts: Vec::new(),
            contact_search: String::new(),
            contact_av_req: HashSet::new(),
            emoji_tab: 0,
            sticker_sets: Vec::new(),
            sticker_cur: 0,
            sticker_lists: HashMap::new(),
            sticker_tex: HashMap::new(),
            sticker_req: HashSet::new(),
            stickers_requested: false,
            emoji_sets: Vec::new(),
            player_path: None,
            player_stream: None,
            fetching: HashMap::new(),
            tgs: tg_settings::State::default(),
            cached_media: HashSet::new(),
            gif_paths: HashMap::new(),
            gif_auto_req: HashSet::new(),
            inline_gif_rect: None,
            #[cfg(windows)]
            inline_gif: None,
            history_cache: HashMap::new(),
            history_cache_order: std::collections::VecDeque::new(),
            history_fresh_at: HashMap::new(),
            history_loading: false,
            img_arrived: HashMap::new(),
            global_results: Vec::new(),
            global_last_q: String::new(),
            global_query_at: None,
            gif_list: Vec::new(),
            gifs_requested: false,
            create_kind: 0,
            create_title: String::new(),
            create_about: String::new(),
            create_username: String::new(),
            create_public: false,
            create_members: HashSet::new(),
            create_member_query: String::new(),
            create_member_results: Vec::new(),
            create_member_map: HashMap::new(),
            create_member_last_q: String::new(),
            create_member_at: None,
            info_open: false,
            info_tab: 0,
            peer_infos: HashMap::new(),
            info_req: HashSet::new(),
            participants: HashMap::new(),
            participants_req: HashSet::new(),
            input_custom_emojis: Vec::new(),
            custom_emoji_req: HashSet::new(),
            last_toast: None,
            notify_gate: NotifyGate::default(),
            toasts: Vec::new(),
            app_locked: false,
            lock_input: String::new(),
            lock_err: String::new(),
            passcode_checked: false,
            audio_path: None,
            audio_stream: None,
            audio_title: String::new(),
            audio_dur: 0,
            audio_vol: 1.0,
            #[cfg(windows)]
            audio_view: None,
            #[cfg(windows)]
            player: None,
            plugins: {
                if cfg!(feature = "plugins") {
                    let mut reg = rg_ayu::plugins::PluginRegistry::with_builtins();
                    let _ = reg.load_dir(&worker::home().join("plugins"));
                    reg
                } else {
                    rg_ayu::plugins::PluginRegistry::default()
                }
            },
            settings_opened_at: None,
            chat_nav_back: Vec::new(),
            chat_nav_fwd: Vec::new(),
            nav_jumping: false,
            input_undo: Vec::new(),
            input_redo: Vec::new(),
            autoscroll_anchor: None,
            refresh_at: None,
            featured_sets: Vec::new(),
            emoji_sets_requested: false,
            emoji_pack: None,
            emoji_add_open: false,
            set_icon_req: HashSet::new(),
            known: HashSet::new(),
            arrived: HashMap::new(),
            chat_opened: std::time::Instant::now(),
            logout_confirm: false,
            shadow_new: String::new(),
            expanded: 0,
            older_pending: false,
            history_end: false,
            scroll_to: None,
            tray: None,
            is_demo: false,
            quitting: false,
            hidden_to_tray: false,
            autostart_minimized: std::env::args().skip(1).any(|a| a == "--autostart"),
            pending_tg_uri: std::env::args().skip(1).find(|a| a.starts_with("tg://")),
            tgs_docs: HashMap::new(),
            tgs_msgs: HashMap::new(),
            tgs_msg_req: HashSet::new(),
            tgs_frame_cache: HashMap::new(),
            shot_modal: None,
            selected_msgs: HashSet::new(),
            forward_batch: None,
            audit_history: None,
            blocked_ids: HashSet::new(),
            font_picker_open: false,
            font_search: String::new(),
            sys_fonts: Vec::new(),
            typing: HashMap::new(),
            pinned_msgs: HashMap::new(),
            pinned_req: HashSet::new(),
            pinned_lists: HashMap::new(),
            pinned_cur_idx: HashMap::new(),
            pinned_list_req: HashSet::new(),
            confirm_unpin: None,
            open_unread_anchor: None,
            mention_sel_idx: 0,
            chat_search_filter: rg_core::engine::ChatSearchFilter::All,
            chat_search_idx: 0,
            pending_voice_confirm: None,
            forum_topics: HashMap::new(),
            forum_req: HashSet::new(),
            active_topic: None,
            topic_msgs: None,
            discussion: None,
            comment_input: String::new(),
            stories: Vec::new(),
            story_open: None,
            story_ghost_ask: None,
            drop_paths: Vec::new(),
            drop_caption: String::new(),
            pending_confirm_media: None,
            user_filter: None,
            cmd_popup_open: false,
            cmd_sel_idx: 0,
            ttl_popup_open: false,
            format_bar_open: false,
            composer_sel: None,
            scrolled_up: false,
            msg_heights: HashMap::new(),
            msg_last_avail: 0.0,
            bot_keyboard_open: true,
            bot_alert: None,
            miniapp_modal: None,
            #[cfg(windows)]
            miniapp_view: None,
            add_account_open: false,
            new_acc_label: String::new(),
            new_acc_proxy: String::new(),
            wipe_modal_open: false,
            wipe_confirm_step: false,
            wipe_scope: rg_core::engine::WipeScope::default(),
            highlight_msg: None,
            login_country_idx: 0,
            login_dial: "+7".to_owned(),
            login_phone_full: "+7 ".to_owned(),
            login_keep_signed_in: true,
            login_code_hint: String::new(),
            login_country_open: false,
            login_country_query: String::new(),
            login_busy: false,
            login_last_phone: String::new(),
            login_code_sent_at: None,
            login_code_len: 5,
            login_resend_secs: 60,
            android_webview_open: false,
            android_input_pending: false,
            autoscroll_holding: false,
            autoscroll_dragged: false,
            voice_rec: None,
            story_composer_open: false,
            story_file_path: String::new(),
            story_caption: String::new(),
            story_privacy: rg_core::engine::StoryPrivacy::Everyone,
            story_period: 86_400,
            story_pinned: true,
            story_noforwards: false,
            story_textures: HashMap::new(),
            story_media_req: HashSet::new(),
            story_reply_text: String::new(),
            story_opened_at: None,
            active_call: None,
            calls_open: false,
            call_history: Vec::new(),
            secret_open: false,
            secret_chats: Vec::new(),
            secret_selected: None,
            secret_messages: Vec::new(),
            secret_input: String::new(),
            secret_show_key: false,
            admin_modal: None,
            admin_log: Vec::new(),
            admin_invites: Vec::new(),
            update_available: None,
            update_dismissed: false,
            update_progress: None,
            bot_btn_pending: None,
            #[cfg(windows)]
            miniapp_opened_at: None,
            #[cfg(windows)]
            miniapp_expanded: false,
            dialogs_gen: 0,
            filtered_cache_key: (u64::MAX, String::new(), false, None, false),
            filtered_cache: Vec::new(),
            archived_total_cache: 0,
            folder_tabs_cache_key: (u64::MAX, 0, false, false, false),
            folder_tabs_cache: Vec::new(),
            archive_summary_cache_key: (u64::MAX, false),
            archive_summary_cache: (String::new(), 0),
            tray_unread_cache: ((u64::MAX, false, false), 0, false),
            last_win_icon: None,
            last_capture_excluded: None,
            wallpaper_failed_path: String::new(),
            visible_tgs_msgs: HashSet::new(),
            visible_tgs_docs: HashSet::new(),
            tgs_scratch: lottie::TgsScratch::default(),
            peek_modal: None,
            participants_filter: rg_core::engine::ParticipantFilter::All,
            participants_query: String::new(),
            last_typing_sent: None,
            contact_modal_open: false,
            send_contact_phone: String::new(),
            send_contact_first: String::new(),
            send_contact_last: String::new(),
            geo_modal_open: false,
            geo_lat: "55.7558".to_owned(),
            geo_lon: "37.6173".to_owned(),
            pending_tex_uploads: VecDeque::new(),
            participants_total: HashMap::new(),
            media_editor: None,
            android_pick_target: None,
            call_media_stats: None,
            call_stats_tick: None,
            stars_modal_open: false,
            stars_tab: 0,
            stars_summary: None,
            star_gifts_catalog: Vec::new(),
            my_saved_gifts: Vec::new(),
            peer_saved_gifts: HashMap::new(),
            saved_gifts_req: HashSet::new(),
            paid_react_msg: None,
            paid_react_count: 10,
            paid_react_anon: false,
            business_open: false,
            biz_hours_enabled: true,
            biz_timezone: "Europe/Moscow".to_owned(),
            biz_start_h: 9,
            biz_end_h: 18,
            biz_weekdays_only: true,
            quick_replies: Vec::new(),
            themes_open: false,
            wallpaper_tex: None,
            stickers_loaded: false,
            gifs_loaded: false,
            faved_stickers: Vec::new(),
            recent_stickers: Vec::new(),
            emoji_hover_open: false,
            emoji_hover_since: None,
            emoji_leave_since: None,
            archive_loaded: false,
            story_errors: HashMap::new(),
            story_paused: false,
            story_muted: false,
            story_liked: HashSet::new(),
            login_show_password: false,
            transcriptions: HashMap::new(),
            transcribing: HashSet::new(),
            todo_modal_open: false,
            todo_title: String::new(),
            todo_items: vec![String::new(), String::new()],
            todo_others_complete: true,
            todo_others_append: false,
            available_effects: Vec::new(),
            effects_requested: false,
            selected_effect: None,
            send_menu_open: false,
            effect_bursts: HashMap::new(),
            scheduled_open: false,
            scheduled_msgs: Vec::new(),
            schedule_picker_open: false,
            schedule_delay_mins: 15,
            saved_tags: Vec::new(),
            saved_tag_filter: None,
            saved_tags_req: false,
            saved_tag_edit: None,
            send_as_map: HashMap::new(),
            send_as_req: HashSet::new(),
            selected_send_as: HashMap::new(),
            send_as_popup_open: false,
            poll_modal_open: false,
            poll_question: String::new(),
            poll_options: vec![String::new(), String::new()],
            poll_anonymous: true,
            poll_multiple: false,
            poll_quiz: false,
            poll_correct: 0,
            poll_explanation: String::new(),
            profile_accent_color: None,
            replies_accent_color: None,
            hashtag_modal: None,
            admin_slowmode_secs: 0,
            palette_open: false,
            palette_query: String::new(),
            palette_sel: 0,
            draft_translating: false,
            translate_draft_popup: false,
            inline_bot_key: None,
            inline_bot_at: None,
            inline_bot_page: None,
            inline_bot_loading: false,
            inline_bot_error: String::new(),
            available_reactions: Vec::new(),
            available_reactions_req: false,
            emoji_status_open: false,
            default_emoji_statuses: Vec::new(),
            default_emoji_statuses_req: false,
            emoji_status_ttl: 0,
            details_tab: 0,
            msg_activity: None,
            next_temp_msg_id: -1,
            drafts: HashMap::new(),
            draft_dirty_at: None,
            unread_filter_only: false,
            sticker_pack_modal: None,
            rich_runs_cache: HashMap::new(),
            last_focus_lost_at: None,
            was_focused: true,
            disc_reply_to: None,
            profile_username_edit: String::new(),
            profile_bday_day: String::new(),
            profile_bday_month: String::new(),
            profile_bday_year: String::new(),
            profile_bday_synced: false,
            dice_spawn_times: HashMap::new(),
            similar_channels: HashMap::new(),
            similar_req: HashSet::new(),
            similar_expanded: HashSet::new(),
            global_deleted_open: false,
            global_deleted_query: String::new(),
            global_deleted_items: Vec::new(),
            global_deleted_stats: (0, 0, 0),
            local_read_dates: HashMap::new(),
            date_jump_open: false,
            date_jump_input: String::new(),
            date_jump_year: 2026,
            date_jump_month: 11,
            date_jump_active: false,
            common_chats: HashMap::new(),
            common_chats_req: HashSet::new(),
            contact_edit_open: None,
            contact_first: String::new(),
            contact_last: String::new(),
            global_msg_mode: false,
            global_msg_scope: 0,
            global_msg_hits: Vec::new(),
            global_msg_last_q: String::new(),
            global_msg_at: None,
            global_msg_loading: false,
            pending_jump_to_msg: None,
            confirm_chat_action: None,
            pinned_list_open: false,
            pinned_list_did: None,
            pinned_list: Vec::new(),
            forward_no_captions: false,
            forward_silent: false,
            folder_invite_preview: None,
            folder_invite_url: None,
            rx_pop: HashMap::new(),
            chat_opened_at: 0.0,
            status_last: String::new(),
            status_at: 0,
            notify_popover_open: false,
            notify_feed: VecDeque::with_capacity(32),
            drawer_filter: String::new(),
            drawer_accounts_open: true,
            new_topic_open: false,
            new_topic_title: String::new(),
        }
    }

    pub(crate) fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }

    pub(crate) fn dialog_id_at(&self, idx: usize) -> Option<i64> {
        self.dialogs.get(idx).map(|d| d.id)
    }

    pub(crate) fn selected_dialog_id(&self) -> Option<i64> {
        self.selected.and_then(|i| self.dialog_id_at(i))
    }

    pub(crate) fn toggle_mute_dialog(&mut self, idx: usize, muted: bool) {
        let did = self.dialog_id_at(idx);
        if let Some(d) = self.dialogs.get_mut(idx) {
            d.muted = muted;
            d.notify_explicit = Some(muted);
            self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
        }
        if let Some(did) = did {
            self.send(Cmd::MuteDialog(did, muted));
        }
    }

    fn flush_pending_notification(&mut self, ctx: &egui::Context) {
        let now = std::time::Instant::now();
        if ctx.input(|i| i.focused) {
            self.notify_gate.pending = None;
            return;
        }
        if let Some((title, body)) = self.notify_gate.take_ready(now) {
            let cooldown_ok = self.last_toast.is_none_or(|t| t.elapsed().as_secs() >= 2);
            if cooldown_ok {
                self.last_toast = Some(now);
                self.push_notification(ctx, &title, &body);
            }
        } else if let Some(ms) = self.notify_gate.next_wakeup_ms(now) {
            ctx.request_repaint_after(std::time::Duration::from_millis(ms));
        }
    }

    /// Предпросмотры только для свежих вложений и по одному разу.
    /// Событие из потока обновлений: сообщения доходят сразу, без перезахода в чат.
    /// Обновляет строку чата в списке по новому сообщению и поднимает чат наверх локально (0 мс,
    /// без тяжёлого полного перезапроса списка!). Возвращает `true` только если чата вообще нет в списке.
    fn apply_live_to_list(&mut self, key: i64, row: &(i64, bool, String), is_open: bool) -> bool {
        let Some(pos) = self.dialogs.iter().position(|d| d.id == key) else {
            return true;
        };
        let open_id = self.selected.and_then(|i| self.dialogs.get(i)).map(|d| d.id);
        let (date, outgoing, text) = row;
        let Some((old_date, old_pinned, old_archived)) = self.dialogs.get(pos).map(|d| (d.date, d.pinned, d.archived)) else {
            return true;
        };
        let newer = *date >= old_date;
        let Some(d) = self.dialogs.get_mut(pos) else {
            return true;
        };
        if newer {
            if !text.is_empty() {
                d.last.clone_from(text);
            }
            if *date > old_date {
                d.date = *date;
                if !outgoing && !is_open {
                    d.unread += 1;
                }
            }
        }
        // Мгновенно поднимаем чат наверх своей папки (после закреплённых), без лишнего RPC!
        if *date > old_date && !old_pinned {
            let target = self
                .dialogs
                .iter()
                .position(|o| !o.pinned && o.archived == old_archived)
                .unwrap_or(0);
            if target < pos {
                let item = self.dialogs.remove(pos);
                self.dialogs.insert(target, item);
                if let Some(oid) = open_id {
                    self.selected = self.dialogs.iter().position(|x| x.id == oid);
                }
            }
        }
        self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
        false
    }

    fn on_live(&mut self, ctx: &egui::Context, l: rg_core::engine::Live) {
        use rg_core::engine::Live;
        let open_id = self.selected.and_then(|i| self.dialogs.get(i)).map(|d| d.id);
        match l {
            Live::Message(key, row) => {
                self.typing.remove(&key);
                let mut row = *row;
                if cfg!(feature = "plugins") && self.settings.plugins_enabled && !row.outgoing {
                    row.text = self.plugins.apply_incoming(&row.text);
                }
                let row_silent = row.silent;
                let row_snapshot = (row.date, row.outgoing, row.text.chars().take(80).collect::<String>());
                if open_id == Some(key) {
                    let idx = self.selected.unwrap_or(0);
                    let now_t = ctx.input(|i| i.time);
                    if self.known.insert(row.id) {
                        self.arrived.insert(row.id, now_t);
                    }
                    if row
                        .media
                        .as_ref()
                        .is_some_and(|m| matches!(m.kind, rg_core::media::MediaKind::Dice { .. }))
                    {
                        self.dice_spawn_times.insert(row.id, now_t);
                    }
                    self.request_previews(idx, std::slice::from_ref(&row));
                    let incoming = !row.outgoing;
                    let already_present = self.messages.iter().any(|m| m.id == row.id);
                    if !already_present
                        && row.outgoing
                        && let Some(temp_pos) = self
                            .messages
                            .iter()
                            .position(|m| m.id < 0 && m.outgoing && m.text == row.text)
                    {
                        let temp_id = self.messages[temp_pos].id;
                        self.rich_runs_cache.retain(|k, _| k.0 != temp_id);
                        self.messages[temp_pos] = row;
                    } else {
                        match self.messages.iter_mut().find(|m| m.id == row.id) {
                            Some(old) => {
                                let (deleted, edits) = (old.deleted, old.edits);
                                *old = row;
                                old.deleted = deleted;
                                old.edits = edits + usize::from(old.edit_date.is_some());
                                self.rich_runs_cache.retain(|k, _| k.0 != old.id);
                            }
                            None => {
                                self.messages.push(row);
                                self.messages.sort_by_key(|m| (m.date, m.id));
                            }
                        }
                    }
                    // Окно в фокусе и чат открыт: отмечаем прочитанным (режим призрака учитывается).
                    if incoming && ctx.input(|i| i.focused) {
                        self.send(Cmd::MarkRead(key));
                    }
                }
                // Строка списка обновляется на месте; полный перезапрос (300 чатов + архив)
                // нужен, только если чат должен подняться вверх или его нет в списке.
                if self.apply_live_to_list(key, &row_snapshot, open_id == Some(key)) {
                    self.refresh_at.get_or_insert(std::time::Instant::now());
                }
                self.history_fresh_at.remove(&key);
                // Уведомление по настройкам с учётом мута (чат, архив, категория Telegram), `silent` и антиспама >3/сек.
                if !row_snapshot.1 {
                    let dialog = self.dialogs.iter().find(|d| d.id == key);
                    let muted_dlg = is_dialog_muted_for_notify(dialog, self.tgs.account.as_ref());
                    let title = dialog.map_or_else(|| "Rustgramm".to_owned(), |d| d.name.clone());
                    let body = if row_snapshot.2.is_empty() {
                        "Вложение".to_owned()
                    } else {
                        row_snapshot.2.clone()
                    };
                    if open_id != Some(key) && !row_silent && !muted_dlg {
                        self.notify_feed.retain(|x| !(x.0 == key && x.2 == body));
                        self.notify_feed.push_front((key, title.clone(), body.clone(), self.now.max(1), false));
                        while self.notify_feed.len() > 25 {
                            self.notify_feed.pop_back();
                        }
                    }
                    let now = std::time::Instant::now();
                    let is_spam = self.notify_gate.record_incoming(now, key);
                    if !row_silent && !is_spam && !ctx.input(|i| i.focused) && !muted_dlg {
                        self.notify_gate.enqueue(now, title, body);
                        ctx.request_repaint_after(std::time::Duration::from_millis(360));
                    }
                }
            }
            Live::Deleted(ch, ids) => {
                let keep = self.settings.ayu.save_deleted_messages;
                let hit = ch.is_none() || ch == open_id;
                if hit {
                    if keep {
                        for m in self.messages.iter_mut().filter(|m| ids.contains(&m.id)) {
                            m.deleted = true;
                        }
                    } else {
                        self.messages.retain(|m| !ids.contains(&m.id));
                    }
                }
                for (cid, cached) in &mut self.history_cache {
                    let cache_hit = match ch {
                        Some(target) => *cid == target,
                        None => *cid > -1_000_000_000_000,
                    };
                    if cache_hit {
                        if keep {
                            for m in cached.iter_mut().filter(|m| ids.contains(&m.id)) {
                                m.deleted = true;
                            }
                        } else {
                            cached.retain(|m| !ids.contains(&m.id));
                        }
                    }
                }
                if let Some(cid) = ch {
                    self.history_fresh_at.remove(&cid);
                } else {
                    self.history_fresh_at.retain(|cid, _| *cid <= -1_000_000_000_000);
                }
            }
            Live::ReadOutbox(key, max) => {
                if let Some(d) = self.dialogs.iter_mut().find(|d| d.id == key) {
                    d.read_out = d.read_out.max(max);
                }
            }
            Live::ReadInbox(key, max_id) => {
                if max_id > 0
                    && let Some(d) = self.dialogs.iter_mut().find(|d| d.id == key)
                {
                    d.unread = 0;
                    self.dialogs_gen += 1;
                }
            }
            Live::Draft(key, text, reply_to) => {
                if text.trim().is_empty() && reply_to.is_none() {
                    self.drafts.remove(&key);
                } else {
                    self.drafts.insert(key, (text.clone(), reply_to));
                }
                if open_id == Some(key) && self.draft_dirty_at.is_none() {
                    self.input = text;
                }
                self.dialogs_gen += 1;
            }
            Live::MessageReactions(key, _msg_id, delta) => {
                if delta > 0
                    && let Some(d) = self.dialogs.iter_mut().find(|d| d.id == key)
                {
                    d.unread_reactions = d.unread_reactions.saturating_add(delta);
                    self.dialogs_gen += 1;
                }
            }
            Live::Typing(key, lbl) => {
                if lbl.is_empty() {
                    self.typing.remove(&key);
                } else {
                    self.typing.insert(key, (lbl, std::time::Instant::now()));
                }
                ctx.request_repaint();
            }
            Live::PhoneCall(ev) => {
                use rg_core::calls::{CallEvent, CallState};
                match ev {
                    CallEvent::Incoming {
                        call_id,
                        access_hash,
                        admin_id,
                        video,
                        ..
                    } => {
                        let name = self
                            .dialogs
                            .iter()
                            .find(|d| d.id == admin_id)
                            .map(|d| d.name.clone())
                            .unwrap_or_else(|| format!("Пользователь {admin_id}"));
                        if self.settings.notify.allow_sound && self.settings.notify.volume > 0 {
                            audio_io::play_ringtone(self.settings.notify.volume);
                        }
                        self.push_notification(
                            ctx,
                            &name,
                            if video { "Входящий видеозвонок" } else { "Входящий голосовой звонок" },
                        );
                        self.active_call = Some(features_ui::ActiveCallUi {
                            call_id,
                            access_hash,
                            peer_name: name,
                            peer_id: admin_id,
                            video,
                            outgoing: false,
                            state: CallState::RingingIn,
                            connected_at: None,
                            muted: false,
                            speaker: false,
                            camera_on: video,
                            emojis: None,
                            relays: 0,
                        });
                    }
                    CallEvent::Waiting { call_id, access_hash, .. } => {
                        if let Some(c) = self.active_call.as_mut()
                            && (c.call_id == 0 || c.call_id == call_id)
                        {
                            c.call_id = call_id;
                            c.access_hash = access_hash;
                        }
                    }
                    CallEvent::Accepted { call_id, access_hash, g_b } => {
                        audio_io::stop_ringtone();
                        if let Some(c) = self.active_call.as_mut()
                            && (c.call_id == 0 || c.call_id == call_id)
                        {
                            c.call_id = call_id;
                            c.access_hash = access_hash;
                            c.state = CallState::Connecting;
                        }
                        self.send(Cmd::ConfirmCall { call_id, access_hash, g_b });
                    }
                    CallEvent::Active {
                        call_id,
                        access_hash,
                        g_a_or_b,
                        video,
                        p2p_allowed,
                        relays,
                        endpoints,
                        ..
                    } => {
                        audio_io::stop_ringtone();
                        let derive_incoming = if let Some(c) = self.active_call.as_mut()
                            && (c.call_id == 0 || c.call_id == call_id)
                        {
                            c.call_id = call_id;
                            c.access_hash = access_hash;
                            c.state = CallState::Active;
                            if c.connected_at.is_none() {
                                c.connected_at = Some(std::time::Instant::now());
                            }
                            c.relays = relays;
                            !c.outgoing && c.emojis.is_none()
                        } else {
                            false
                        };
                        if derive_incoming {
                            self.send(Cmd::StartIncomingCallMedia {
                                call_id,
                                g_a: g_a_or_b,
                                endpoints,
                                p2p_allowed,
                                video,
                            });
                        }
                    }
                    CallEvent::SignalingData { .. } => {}
                    CallEvent::Discarded { call_id, reason, duration } => {
                        audio_io::stop_ringtone();
                        self.call_media_stats = None;
                        if let Some(c) = self.active_call.as_mut()
                            && (c.call_id == 0 || c.call_id == call_id)
                        {
                            c.state = CallState::Ended(reason.clone());
                            self.call_history.insert(
                                0,
                                rg_core::calls::CallLogEntry {
                                    peer_name: c.peer_name.clone(),
                                    peer_id: c.peer_id,
                                    video: c.video,
                                    outgoing: c.outgoing,
                                    duration: duration.max(0) as u32,
                                    status: reason,
                                    timestamp: self.now,
                                },
                            );
                        }
                    }
                }
                ctx.request_repaint();
            }
            Live::SecretChatUpdate(rec) => {
                if rec.state == "discarded" {
                    self.secret_chats.retain(|c| c.chat_id != rec.chat_id);
                    if self.secret_selected == Some(rec.chat_id) {
                        self.secret_selected = None;
                        self.secret_messages.clear();
                    }
                } else if let Some(ex) = self.secret_chats.iter_mut().find(|c| c.chat_id == rec.chat_id) {
                    *ex = rec;
                } else {
                    self.push_notification(ctx, &rec.peer_name, "Новый секретный чат (E2E)");
                    self.secret_chats.insert(0, rec);
                }
                ctx.request_repaint();
            }
            Live::SecretMsg(msg) => {
                if self.secret_selected == Some(msg.chat_id) {
                    self.secret_messages.push(msg.clone());
                }
                let now = std::time::Instant::now();
                if !self.notify_gate.record_incoming(now, i64::from(msg.chat_id)) && !ctx.input(|i| i.focused) {
                    self.notify_gate.enqueue(
                        now,
                        "Секретный чат (E2E)".to_owned(),
                        "Новое зашифрованное сообщение".to_owned(),
                    );
                    ctx.request_repaint_after(std::time::Duration::from_millis(360));
                }
                ctx.request_repaint();
            }
        }
    }

    /// Отправляет уведомление с учётом всех настроек (Windows Toast или встроенные карточки, звук, мигание).
    pub(crate) fn push_notification(&mut self, ctx: &egui::Context, raw_title: &str, raw_body: &str) {
        let cfg = &self.settings.notify;
        if !cfg.desktop_notifications {
            return;
        }
        let streamer = self.settings.ayu.streamer_mode;
        let title = if cfg.show_name && !streamer { raw_title } else { "Rustgramm" };
        let body = if cfg.show_text && !streamer { raw_body } else { "Новое сообщение" };
        if cfg.flash_taskbar {
            win::flash_taskbar();
        }
        if cfg.allow_sound && cfg.volume > 0 {
            win::play_notify_sound(cfg.volume);
        }
        if cfg.use_windows_notifications {
            notify_toast(title, body);
        } else {
            let max_n = usize::from(cfg.max_count.clamp(1, 5));
            self.toasts.push(InAppToast {
                title: title.to_owned(),
                body: body.to_owned(),
                at: std::time::Instant::now(),
            });
            while self.toasts.len() > max_n {
                self.toasts.remove(0);
            }
            ctx.request_repaint();
        }
    }

    /// Встроенные всплывающие карточки уведомлений в выбранном углу окна (0..=3).
    #[allow(deprecated)]
    fn toasts_overlay(&mut self, ctx: &egui::Context) {
        self.toasts.retain(|t| t.at.elapsed().as_secs_f32() < 4.5);
        if self.toasts.is_empty() {
            return;
        }
        ctx.request_repaint_after(std::time::Duration::from_millis(60));
        let screen = ctx.content_rect();
        let corner = self.settings.notify.corner;
        let card_w = 290.0_f32.min(screen.width() - 28.0);
        let card_h = 62.0;
        let gap = 8.0;
        let mut dismiss: Option<usize> = None;
        for (idx, toast) in self.toasts.iter().enumerate() {
            let age = toast.at.elapsed().as_secs_f32();
            let alpha = if age < 0.2 {
                ios_spring(age / 0.2)
            } else if age > 4.0 {
                ((4.5 - age) / 0.5).clamp(0.0, 1.0)
            } else {
                1.0
            };
            let offset_y = idx as f32 * (card_h + gap);
            let x = match corner {
                0 | 2 => screen.left() + 14.0,
                _ => screen.right() - card_w - 14.0,
            };
            let y = match corner {
                0 | 1 => screen.top() + 44.0 + offset_y,
                _ => screen.bottom() - card_h - 14.0 - offset_y,
            };
            egui::Area::new(egui::Id::new(("rg-toast", idx)))
                .order(egui::Order::Tooltip)
                .fixed_pos(egui::pos2(x, y))
                .show(ctx, |ui| {
                    ui.set_opacity(alpha);
                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::click());
                    let p = ui.painter();
                    p.rect_filled(rect, egui::CornerRadius::same(12), t2(WINDOW_BG));
                    p.rect_stroke(
                        rect,
                        egui::CornerRadius::same(12),
                        egui::Stroke::new(1.0, Color32::from_white_alpha(38)),
                        egui::StrokeKind::Inside,
                    );
                    p.circle_filled(egui::pos2(rect.left() + 26.0, rect.center().y), 16.0, t_btn(ORANGE));
                    p.text(
                        egui::pos2(rect.left() + 26.0, rect.center().y),
                        egui::Align2::CENTER_CENTER,
                        "R",
                        FontId::proportional(14.0),
                        theme::on_accent(),
                    );
                    let clip = egui::Rect::from_min_max(
                        egui::pos2(rect.left() + 50.0, rect.top()),
                        egui::pos2(rect.right() - 10.0, rect.bottom()),
                    );
                    p.with_clip_rect(clip).text(
                        egui::pos2(rect.left() + 50.0, rect.center().y - 9.0),
                        egui::Align2::LEFT_CENTER,
                        &toast.title,
                        FontId::proportional(13.5),
                        WHITE,
                    );
                    p.with_clip_rect(clip).text(
                        egui::pos2(rect.left() + 50.0, rect.center().y + 10.0),
                        egui::Align2::LEFT_CENTER,
                        &toast.body,
                        FontId::proportional(12.5),
                        SUBTEXT,
                    );
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        dismiss = Some(idx);
                    }
                });
        }
        if let Some(i) = dismiss {
            self.toasts.remove(i);
        }
    }

    pub(crate) fn audio_is_playing(&self) -> bool {
        #[cfg(windows)]
        {
            self.audio_view.as_ref().is_some_and(video::AudioView::is_playing)
        }
        #[cfg(not(windows))]
        {
            false
        }
    }

    pub(crate) fn toggle_audio_playback(&mut self) {
        #[cfg(windows)]
        if let Some(v) = self.audio_view.as_ref() {
            v.set_playing(!v.is_playing());
        }
    }

    fn stop_audio(&mut self) {
        #[cfg(windows)]
        if let Some(v) = self.audio_view.take() {
            v.stop();
        }
        self.audio_path = None;
        self.audio_stream = None;
        self.audio_title.clear();
        self.audio_dur = 0;
    }

    /// Отмечает все чаты прочитанными на сервере и мгновенно обнуляет счётчики непрочитанных в UI.
    pub(crate) fn force_read_all(&mut self) {
        for d in &mut self.dialogs {
            d.unread = 0;
        }
        self.status = "Отмечаем чаты прочитанными на сервере…".to_owned();
        self.send(Cmd::ForceReadAll);
    }

    /// Отмечает один чат прочитанным локально (без отправки `ReadHistory` на сервер Telegram).
    pub(crate) fn local_read_dialog(&mut self, idx: usize) {
        if let Some(d) = self.dialogs.get_mut(idx) {
            d.unread = 0;
            d.unread_mentions = 0;
            d.unread_reactions = 0;
            self.local_read_dates.insert(d.id, d.date);
            self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
            self.status = format!("«{}» прочитан локально (без отправки на сервер)", d.name);
        }
    }

    /// Отмечает все чаты прочитанными локально без отправки статусов чтения на сервер Telegram.
    pub(crate) fn local_read_all(&mut self) {
        for d in &mut self.dialogs {
            d.unread = 0;
            d.unread_mentions = 0;
            d.unread_reactions = 0;
            self.local_read_dates.insert(d.id, d.date);
        }
        self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
        self.status = "Все чаты прочитаны локально (без отправки на сервер)".to_owned();
    }

    /// Мини-плеер музыки/аудио сверху чата (как в Telegram/AyuGram).
    fn audio_bar(&mut self, ui: &mut egui::Ui) {
        let playing = self.audio_is_playing();
        #[cfg(windows)]
        let (pos_s, dur_s) = self
            .audio_view
            .as_ref()
            .map(|v| (v.pos_secs(), v.dur_secs().max(self.audio_dur as f32)))
            .unwrap_or((0.0, self.audio_dur as f32));
        #[cfg(not(windows))]
        let (pos_s, dur_s) = (0.0f32, self.audio_dur as f32);
        let mut toggle = false;
        let mut seek_to: Option<f32> = None;
        let mut vol_changed = false;
        let mut close = false;
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            let (pr, presp) = ui.allocate_exact_size(Vec2::splat(30.0), Sense::click());
            ui.painter().circle_filled(pr.center(), 14.0, t_btn(ORANGE));
            icons::tg_sized(
                ui.painter(),
                if playing { "media_pause" } else { "media_play" },
                pr.center(),
                Vec2::splat(13.0),
                theme::on_accent(),
            );
            if presp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                toggle = true;
            }
            ui.add_space(6.0);
            // Название трека + полоса перемотки.
            let bar_w = (ui.available_width() - 180.0).max(90.0);
            let (tr, tresp) = ui.allocate_exact_size(Vec2::new(bar_w, 34.0), Sense::click_and_drag());
            let p = ui.painter();
            let clip = egui::Rect::from_min_max(tr.min, egui::pos2(tr.right(), tr.top() + 20.0));
            p.with_clip_rect(clip).text(
                egui::pos2(tr.left(), tr.top() + 9.0),
                egui::Align2::LEFT_CENTER,
                &self.audio_title,
                FontId::proportional(13.0),
                WHITE,
            );
            let track = egui::Rect::from_min_max(egui::pos2(tr.left(), tr.bottom() - 7.0), egui::pos2(tr.right(), tr.bottom() - 3.0));
            let frac = if dur_s > 0.1 { (pos_s / dur_s).clamp(0.0, 1.0) } else { 0.0 };
            p.rect_filled(track, egui::CornerRadius::same(2), Color32::from_white_alpha(32));
            p.rect_filled(
                egui::Rect::from_min_max(track.min, egui::pos2(track.left() + track.width() * frac, track.max.y)),
                egui::CornerRadius::same(2),
                t_btn(ORANGE),
            );
            if (tresp.clicked() || tresp.dragged())
                && dur_s > 0.1
                && let Some(pt) = tresp.interact_pointer_pos()
            {
                let f = ((pt.x - track.left()) / track.width().max(1.0)).clamp(0.0, 1.0);
                seek_to = Some(f * dur_s);
            }
            ui.add_space(8.0);
            let time_s = format!(
                "{} / {}",
                media_ui::mmss(pos_s as u32),
                media_ui::mmss(dur_s as u32)
            );
            ui.label(RichText::new(time_s).size(11.5).color(SUBTEXT));
            // Ползунок громкости.
            let (vr, vresp) = ui.allocate_exact_size(Vec2::new(52.0, 20.0), Sense::click_and_drag());
            let vtrack = egui::Rect::from_min_max(egui::pos2(vr.left(), vr.center().y - 1.5), egui::pos2(vr.right(), vr.center().y + 1.5));
            ui.painter().rect_filled(vtrack, egui::CornerRadius::same(2), Color32::from_white_alpha(32));
            ui.painter().rect_filled(
                egui::Rect::from_min_max(vtrack.min, egui::pos2(vtrack.left() + vtrack.width() * self.audio_vol, vtrack.max.y)),
                egui::CornerRadius::same(2),
                t_btn(ORANGE),
            );
            if (vresp.clicked() || vresp.dragged())
                && let Some(pt) = vresp.interact_pointer_pos()
            {
                self.audio_vol = ((pt.x - vtrack.left()) / vtrack.width().max(1.0)).clamp(0.0, 1.0);
                vol_changed = true;
            }
            let (xr, xresp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
            icons::tg(ui.painter(), "info/info_close", xr.center(), if xresp.hovered() { WHITE } else { SUBTEXT });
            if xresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                close = true;
            }
        });
        if toggle {
            self.toggle_audio_playback();
        }
        #[cfg(windows)]
        if let Some(s) = seek_to
            && let Some(v) = self.audio_view.as_ref()
        {
            v.seek_secs(s);
        }
        #[cfg(windows)]
        if vol_changed
            && let Some(v) = self.audio_view.as_ref()
        {
            v.set_volume(self.audio_vol);
        }
        #[cfg(not(windows))]
        let _ = (seek_to, vol_changed);
        if close {
            self.stop_audio();
        }
    }

    /// Синхронизирует скрытый WebView2 аудиоплеер с текущим треком (файл или потоковый HTTP Range URL).
    #[cfg(windows)]
    fn audio_overlay(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let has_source = self.audio_path.is_some() || self.audio_stream.is_some();
        if !has_source {
            if let Some(v) = self.audio_view.take() {
                v.stop();
            }
            return;
        }
        if self.audio_view.is_none() {
            let dir = worker::home().join("audio_player");
            let built = if let Some(url) = &self.audio_stream {
                video::AudioView::new_url(frame, url, &dir, self.audio_vol)
            } else if let Some(path) = &self.audio_path {
                video::AudioView::new(frame, path, &dir, self.audio_vol)
            } else {
                return;
            };
            match built {
                Ok(v) => self.audio_view = Some(v),
                Err(e) => {
                    self.error = e;
                    self.error_at = self.now;
                    self.audio_path = None;
                    self.audio_stream = None;
                    return;
                }
            }
        }
        if let Some(v) = self.audio_view.as_ref() {
            if let Some(err) = v.take_error() {
                self.error = err;
                self.error_at = self.now;
            }
            if v.is_playing() {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
            let _ = v.take_ended();
        }
    }

    /// Экран локальной блокировки по код-паролю (из «Конфиденциальность → Локальный код-пароль»).
    fn lock_screen_ui(&mut self, ui: &mut egui::Ui) {
        ui.vertical_centered(|ui| {
            ui.add_space((ui.available_height() * 0.24).max(40.0));
            let (ir, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
            ui.painter().circle_filled(ir.center(), 30.0, t_btn(ORANGE));
            icons::tg(ui.painter(), "settings/lock", ir.center(), theme::on_accent());
            ui.add_space(14.0);
            ui.label(RichText::new("Rustgramm заблокирован").size(20.0).strong().color(WHITE));
            ui.label(RichText::new("Введите локальный код-пароль").size(13.5).color(SUBTEXT));
            ui.add_space(16.0);
            let r = ui.add(
                egui::TextEdit::singleline(&mut self.lock_input)
                    .password(true)
                    .hint_text("Код-пароль")
                    .desired_width(240.0)
                    .margin(Vec2::new(10.0, 9.0)),
            );
            self.hook_android_text(&r, "lock_input", "Код-пароль", "Введите локальный код-пароль", "", 3);
            if !self.lock_err.is_empty() {
                ui.add_space(6.0);
                ui.colored_label(DANGER, &self.lock_err);
            }
            ui.add_space(12.0);
            let submit = orange_button(ui, "Разблокировать", Vec2::new(240.0, 38.0)).clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if submit {
                if rg_settings::hash_passcode(&self.lock_input) == self.settings.security.passcode_hash {
                    self.app_locked = false;
                    self.lock_input.clear();
                    self.lock_err.clear();
                } else {
                    self.lock_err = "Неверный код-пароль".to_owned();
                    self.lock_input.clear();
                }
            }
        });
    }

    /// Пакетно запрашивает текстуры пользовательских Premium-эмодзи статусов из видимой верхушки списка чатов.
    fn request_emoji_statuses(&mut self) {
        let mut ids: Vec<i64> = Vec::new();
        for d in self.dialogs.iter().take(120) {
            if let Some(doc_id) = d.emoji_status
                && !self.sticker_tex.contains_key(&doc_id)
                && self.custom_emoji_req.insert(doc_id)
            {
                ids.push(doc_id);
                if ids.len() >= 90 {
                    break;
                }
            }
        }
        if !ids.is_empty() {
            self.send(Cmd::CustomEmojiDocs(ids));
        }
    }

    /// Импортирует тему `.tdesktop-theme`/`.attheme` и применяет её палитру и обои.
    pub(crate) fn apply_imported_theme(&mut self, ctx: &egui::Context, path: &std::path::Path) {
        match theme_pack::import_theme_file(path) {
            Ok(imp) => {
                let icon_pack = self.settings.theme.icon_pack.clone();
                self.settings.theme = imp.settings;
                self.settings.theme.icon_pack = icon_pack;
                if let Some((wp_name, wp_bytes)) = imp.wallpaper_bytes {
                    let themes_dir = worker::home().join("themes");
                    let _ = std::fs::create_dir_all(&themes_dir);
                    let dst = themes_dir.join(wp_name);
                    if std::fs::write(&dst, wp_bytes).is_ok() {
                        self.settings.theme.wallpaper_path = dst.to_string_lossy().into_owned();
                    }
                }
                self.wallpaper_tex = None;
                sync_custom_theme(&self.settings.theme);
                self.status = format!("Применена тема «{}»", self.settings.theme.active_theme_name);
                ctx.request_repaint();
            }
            Err(e) => {
                self.error = e;
                self.error_at = self.now;
            }
        }
    }

    /// Полная остановка аудио, микрофона, WebView, звонков и трея при закрытии приложения.
    pub(crate) fn shutdown_resources(&mut self) {
        audio_io::stop_all_audio();
        if let Some(rec) = self.voice_rec.take() {
            rec.cancel();
        }
        #[cfg(windows)]
        {
            if let Some(v) = self.audio_view.take() {
                v.stop();
            }
            if let Some(v) = self.player.take() {
                v.stop();
            }
            if let Some(v) = self.inline_gif.take() {
                v.stop();
            }
            if let Some(v) = self.miniapp_view.take() {
                v.stop();
            }
        }
        android_jni::shutdown_all();
        if let Some(mut t) = self.tray.take() {
            t.shutdown();
        }
        self.send(Cmd::Shutdown);
    }

    /// Просмотрщик ролика: локальный файл или потоковый адрес `127.0.0.1`, закрытие по Esc/крестику/клику.
    #[cfg(windows)]
    #[allow(deprecated)]
    fn player_overlay(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let source = if let Some(path) = self.player_path.clone() {
            let name = path.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
            Some((Some(path), None, name))
        } else if let Some((url, name)) = self.player_stream.clone() {
            Some((None, Some(url), name))
        } else {
            None
        };
        let Some((path_opt, url_opt, name)) = source else {
            if let Some(p) = self.player.take() {
                p.stop();
            }
            return;
        };
        let screen = ctx.content_rect();
        let ppp = ctx.pixels_per_point();
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape))
            || self.player.as_ref().is_some_and(video::VideoView::wants_close);
        ctx.request_repaint_after(std::time::Duration::from_millis(120));
        // Окно плеера по пропорциям самого видео; полный экран — вся область окна.
        let full = self.player.as_ref().is_some_and(video::VideoView::is_full);
        let aspect = self.player.as_ref().and_then(video::VideoView::aspect).unwrap_or(16.0 / 9.0);
        let vr = if full {
            screen
        } else {
            let avail = egui::Rect::from_min_max(screen.min + Vec2::new(60.0, 64.0), screen.max - Vec2::new(60.0, 56.0));
            let w = avail.width().min(avail.height() * aspect).max(160.0);
            egui::Rect::from_center_size(avail.center(), Vec2::new(w, w / aspect))
        };
        if self.player.is_none() {
            let dir = worker::home().join("player");
            let built = match (&path_opt, &url_opt) {
                (Some(p), _) => video::VideoView::new(frame, p, &dir, false),
                (None, Some(u)) => video::VideoView::new_url(frame, u, &dir, false),
                _ => Err("Источник видео не задан".to_owned()),
            };
            match built {
                Ok(v) => self.player = Some(v),
                Err(e) => {
                    self.error = e;
                    self.error_at = self.now;
                    if let Some(p) = path_opt {
                        rg_core::media::open_downloaded(&p);
                    }
                    self.player_path = None;
                    self.player_stream = None;
                    return;
                }
            }
        }
        if let Some(v) = self.player.as_mut() {
            v.place((
                (vr.min.x * ppp).round() as i32,
                (vr.min.y * ppp).round() as i32,
                (vr.width() * ppp).round() as u32,
                (vr.height() * ppp).round() as u32,
            ));
        }
        egui::Area::new(egui::Id::new("player-overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                paint_frosted_scrim(ui.painter(), rect, 1.0);
                ui.painter().text(
                    rect.left_top() + Vec2::new(24.0, 32.0),
                    egui::Align2::LEFT_CENTER,
                    name,
                    FontId::proportional(14.0),
                    WHITE,
                );
                let x = egui::Rect::from_center_size(rect.right_top() + Vec2::new(-36.0, 72.0), Vec2::splat(36.0));
                let xr = ui.interact(x, ui.make_persistent_id("player-close"), Sense::click());
                icons::tg(ui.painter(), "info/info_close", x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
                let over_x = xr.hovered();
                if xr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    close = true;
                }
                if resp.clicked() && !over_x {
                    close = true;
                }
            });
        if close {
            if let Some(p) = self.player.take() {
                p.stop();
            }
            self.player_path = None;
            self.player_stream = None;
        }
    }

    /// Автозапуск видимого кэшированного GIF без звука прямо в ленте (только когда поверх нет окон).
    #[cfg(windows)]
    fn inline_gif_overlay(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let blocked = !self.settings.auto_play_gifs
            || self.player_path.is_some()
            || self.player_stream.is_some()
            || self.miniapp_modal.is_some()
            || self.viewer.is_some()
            || self.drawer_open
            || self.settings_open
            || self.tgs.open
            || self.tgs.profile_open
            || self.contacts_open
            || self.create_kind != 0
            || self.forward.is_some()
            || self.confirm_delete.is_some()
            || self.details.is_some()
            || self.link_confirm.is_some()
            || self.logout_confirm
            || ctx.any_popup_open();
        let target = if blocked { None } else { self.inline_gif_rect };
        let Some((msg_id, rect)) = target else {
            if let Some(g) = self.inline_gif.take() {
                g.stop();
            }
            return;
        };
        let Some(path) = self.gif_paths.get(&msg_id).cloned() else {
            if let Some(g) = self.inline_gif.take() {
                g.stop();
            }
            return;
        };
        let need_new = self.inline_gif.as_ref().is_none_or(|g| g.msg_id != msg_id);
        if need_new {
            if let Some(old) = self.inline_gif.take() {
                old.stop();
            }
            let dir = worker::home().join("inline_gif");
            if let Ok(v) = video::InlineGifView::new(frame, msg_id, &path, &dir) {
                self.inline_gif = Some(v);
            }
        }
        let ppp = ctx.pixels_per_point();
        if let Some(v) = self.inline_gif.as_mut() {
            v.place((
                (rect.min.x * ppp).round() as i32,
                (rect.min.y * ppp).round() as i32,
                (rect.width() * ppp).round() as u32,
                (rect.height() * ppp).round() as u32,
            ));
            if v.take_clicked() {
                self.player_path = Some(path);
            }
        }
    }

    /// Раз в секунду после событий обновляем список чатов (счётчики, последние сообщения).
    fn flush_refresh(&mut self, ctx: &egui::Context) {
        if let Some(t) = self.refresh_at {
            if t.elapsed().as_millis() >= 900 {
                self.refresh_at = None;
                self.send(Cmd::RefreshDialogs);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(300));
            }
        }
    }

    fn request_previews(&mut self, idx: usize, rows: &[MessageRow]) {
        let Some(did) = self.dialog_id_at(idx) else { return };
        // В больших группах (1000+ участников) подгружаем аватарки порциями (≤ 24 уникальных),
        // чтобы открытие чата не создавало лавину сетевых запросов.
        if did < 0 {
            let mut sent_av = 0usize;
            for r in rows.iter().rev().filter(|r| !r.outgoing && r.sender_id != 0) {
                if self.contact_av_req.insert(r.sender_id) && !self.avatars.contains_key(&r.sender_id) {
                    let _ = self.cmd.send(Cmd::SenderAvatar(r.id, r.sender_id));
                    sent_av += 1;
                    if sent_av >= 24 {
                        break;
                    }
                }
            }
        }
        let want: Vec<i32> = rows
            .iter()
            .rev()
            .filter(|r| r.media.as_ref().is_some_and(|x| x.has_preview()))
            .take(30)
            .map(|r| r.id)
            .collect();
        for id in want {
            if self.requested.insert(id) {
                let _ = self.cmd.send(Cmd::Preview(did, id));
            }
        }
        // Для анимированных .tgs стикеров в ленте дополнительно качаем векторную Lottie-анимацию.
        let tgs_want: Vec<i32> = rows
            .iter()
            .rev()
            .filter(|r| {
                r.media.as_ref().is_some_and(|x| {
                    x.kind == rg_core::media::MediaKind::AnimatedSticker && x.mime == "application/x-tgsticker"
                })
            })
            .take(20)
            .map(|r| r.id)
            .collect();
        for id in tgs_want {
            if !self.tgs_msgs.contains_key(&id) && self.tgs_msg_req.insert(id) {
                let _ = self.cmd.send(Cmd::TgsMessage(did, id));
            }
        }
        // Какие ролики уже скачаны: на них рисуется play, остальные — стрелка загрузки.
        let videos: Vec<i32> = rows
            .iter()
            .filter(|r| {
                r.media.as_ref().is_some_and(|x| {
                    matches!(x.kind, rg_core::media::MediaKind::Video | rg_core::media::MediaKind::Gif | rg_core::media::MediaKind::Round)
                }) && !self.cached_media.contains(&r.id)
            })
            .map(|r| r.id)
            .collect();
        if !videos.is_empty() {
            let _ = self.cmd.send(Cmd::CachedMedia(did, videos));
        }
        // Короткие GIF и видео-стикеры WebM до 4 МБ подгружаются в фоне для плавного автозапуска в ленте.
        if self.settings.auto_play_gifs {
            let gifs: Vec<i32> = rows
                .iter()
                .rev()
                .filter(|r| {
                    r.media.as_ref().is_some_and(|x| {
                        (x.kind == rg_core::media::MediaKind::Gif
                            || (x.kind == rg_core::media::MediaKind::AnimatedSticker && x.mime == "video/webm"))
                            && x.size > 0
                            && x.size <= 4 * 1024 * 1024
                    })
                })
                .take(8)
                .map(|r| r.id)
                .collect();
            for id in gifs {
                if !self.gif_paths.contains_key(&id) && self.gif_auto_req.insert(id) {
                    let _ = self.cmd.send(Cmd::AutoFetchGif(did, id));
                }
            }
        }
        self.request_reaction_docs(rows);
    }

    /// Запрашивает кастомные Premium-эмодзи из текста, RichMessage-блоков и пилюль реакций канала.
    pub(crate) fn request_reaction_docs(&mut self, rows: &[MessageRow]) {
        let mut custom_ids: Vec<i64> = Vec::new();
        for r in rows {
            for e in &r.entities {
                if let rg_core::engine::EntityKind::CustomEmoji(doc_id) = e.kind
                    && !self.sticker_tex.contains_key(&doc_id)
                    && self.custom_emoji_req.insert(doc_id)
                {
                    custom_ids.push(doc_id);
                }
            }
            for pill in &r.reaction_items {
                if let Some(doc_id) = pill.custom_doc_id
                    && !self.sticker_tex.contains_key(&doc_id)
                    && self.custom_emoji_req.insert(doc_id)
                {
                    custom_ids.push(doc_id);
                }
            }
            let mut rich_ids = Vec::new();
            rg_core::rich_msg::collect_rich_custom_emojis(&r.rich_blocks, &mut rich_ids);
            for doc_id in rich_ids {
                if !self.sticker_tex.contains_key(&doc_id) && self.custom_emoji_req.insert(doc_id) {
                    custom_ids.push(doc_id);
                }
            }
        }
        if !custom_ids.is_empty() {
            let _ = self.cmd.send(Cmd::CustomEmojiDocs(custom_ids));
        }
    }

    /// Следующий временный отрицательный ID для оптимистичной отправки (< 0).
    pub(crate) fn next_temp_id(&mut self) -> i32 {
        let id = self.next_temp_msg_id;
        self.next_temp_msg_id = if self.next_temp_msg_id <= -1_000_000 {
            -1
        } else {
            self.next_temp_msg_id - 1
        };
        id
    }

    /// Мгновенно добавляет исходящее сообщение (id < 0) в ленту и превью списка чатов.
    pub(crate) fn push_optimistic_message(
        &mut self,
        text: String,
        entities: Vec<rg_core::engine::Entity>,
        reply_to: Option<i32>,
        media: Option<rg_core::media::MediaInfo>,
    ) -> i32 {
        let temp_id = self.next_temp_id();
        let reply_preview = reply_to.map(|rid| {
            let txt = self
                .messages
                .iter()
                .find(|m| m.id == rid)
                .map_or_else(|| format!("#{rid}"), |r| {
                    let who = if r.outgoing { self.me.as_str() } else { r.sender.as_str() };
                    let snippet: String = r.text.chars().take(80).collect();
                    if who.is_empty() { snippet } else { format!("{who}: {snippet}") }
                });
            (rid, txt)
        });
        let row = MessageRow {
            entities,
            reply_to: reply_preview,
            media,
            ..MessageRow::plain(temp_id, text.clone(), true, self.now.max(1))
        };
        self.messages.push(row.clone());
        self.scrolled_up = false;
        self.scroll_to = Some(temp_id);
        if let Some(sel) = self.selected
            && let Some(d) = self.dialogs.get_mut(sel)
        {
            let did = d.id;
            let preview: String = if !text.is_empty() {
                text.chars().take(80).collect()
            } else {
                row.media.as_ref().map(|m| m.name.clone()).unwrap_or_default()
            };
            if !preview.is_empty() {
                d.last = preview;
            }
            d.date = d.date.max(row.date);
            self.drafts.remove(&did);
            self.draft_dirty_at = None;
            self.dialogs_gen += 1;
        }
        temp_id
    }

    /// Сохраняет изменённый черновик на сервер при паузе ввода или смене чата/фокуса.
    pub(crate) fn flush_draft_if_dirty(&mut self, force: bool, ui_time: f64) {
        let Some((idx, at)) = self.draft_dirty_at else {
            return;
        };
        if !force && ui_time - at < 1.2 {
            return;
        }
        self.draft_dirty_at = None;
        let reply_to = if self.selected == Some(idx) {
            self.reply.as_ref().map(|(id, _)| *id)
        } else {
            None
        };
        if let Some(d) = self.dialogs.get(idx) {
            let did = d.id;
            let text = if self.selected == Some(idx) {
                self.input.clone()
            } else {
                self.drafts.get(&did).map(|(t, _)| t.clone()).unwrap_or_default()
            };
            self.send(Cmd::SaveDraft(did, text, reply_to));
        }
    }

    /// Открыть чат из списка (мгновенно из кэша в памяти, если уже открывался).
    fn open_dialog(&mut self, i: usize) {
        self.flush_draft_if_dirty(true, 0.0);
        if let Some(prev_idx) = self.selected
            && let Some(prev_d) = self.dialogs.get(prev_idx)
            && self.dialogs.get(i).is_none_or(|d| d.id != prev_d.id)
        {
            if self.input.trim().is_empty() && self.reply.is_none() {
                self.drafts.remove(&prev_d.id);
            } else {
                self.drafts
                    .insert(prev_d.id, (self.input.clone(), self.reply.as_ref().map(|(id, _)| *id)));
            }
            self.dialogs_gen += 1;
        }
        if !self.nav_jumping
            && let Some(prev_id) = self.selected.and_then(|cur| self.dialogs.get(cur)).map(|d| d.id)
            && self.dialogs.get(i).is_none_or(|d| d.id != prev_id)
        {
            if self.chat_nav_back.last() != Some(&prev_id) {
                self.chat_nav_back.push(prev_id);
                if self.chat_nav_back.len() > 32 {
                    self.chat_nav_back.remove(0);
                }
            }
            self.chat_nav_fwd.clear();
        }
        self.reset_chat_state();
        self.selected = Some(i);
        self.chat_opened_at = -1.0;
        if let Some((id, unread, rim)) = self.dialogs.get(i).map(|d| (d.id, d.unread, d.read_inbox_max_id)) {
            for item in &mut self.notify_feed {
                if item.0 == id {
                    item.4 = true;
                }
            }
            self.open_unread_anchor = (unread > 0 && rim > 0).then_some(rim);
            if let Some((draft_txt, draft_reply)) = self.drafts.get(&id).cloned() {
                self.input = draft_txt;
                self.reply = draft_reply.map(|rid| (rid, format!("#{rid}")));
            } else {
                self.input.clear();
            }
            if self.info_req.insert(id) {
                self.send(Cmd::PeerInfo(id));
            }
            if self.peer_infos.get(&id).and_then(|p| p.pinned_msg_id).is_some()
                && self.pinned_list_req.insert(id)
            {
                self.send(Cmd::LoadPinnedMessages(id));
            }
            if let Some(cached) = self.history_cache.get(&id).cloned()
                && !cached.is_empty()
            {
                self.history_cache_order.retain(|&x| x != id);
                self.history_cache_order.push_back(id);
                self.known.extend(cached.iter().map(|r| r.id));
                self.request_previews(i, &cached);
                self.messages = cached;
                self.history_loading = false;
                let fresh = self
                    .history_fresh_at
                    .get(&id)
                    .is_some_and(|t| t.elapsed().as_millis() < 1500);
                self.send(Cmd::Open(id, !fresh));
                return;
            } else {
                self.history_loading = true;
            }
            self.send(Cmd::Open(id, true));
        } else {
            self.input.clear();
            self.history_loading = true;
        }
    }

    /// Закрыть открытый чат и сбросить состояние ленты.
    fn close_chat(&mut self) {
        self.selected = None;
        self.reset_chat_state();
    }

    fn reset_chat_state(&mut self) {
        self.cached_media.clear();
        self.visible_tgs_msgs.clear();
        self.participants_filter = rg_core::engine::ParticipantFilter::All;
        self.participants_query.clear();
        self.mention_sel_idx = 0;
        self.chat_search_filter = rg_core::engine::ChatSearchFilter::All;
        self.chat_search_idx = 0;
        self.pending_voice_confirm = None;
        self.open_unread_anchor = None;
        self.last_typing_sent = None;
        self.contact_modal_open = false;
        self.geo_modal_open = false;
        self.pending_tex_uploads.retain(|ev| !matches!(ev, Ev::Image(..)));
        self.gif_paths.clear();
        self.gif_auto_req.clear();
        self.inline_gif_rect = None;
        #[cfg(windows)]
        if let Some(g) = self.inline_gif.take() {
            g.stop();
        }
        self.known.clear();
        self.arrived.clear();
        self.img_arrived.clear();
        self.translated.clear();
        self.history_loading = false;
        self.chat_opened = std::time::Instant::now();
        self.messages.clear();
        self.textures.clear();
        self.requested.clear();
        self.tgs_msgs.clear();
        self.tgs_msg_req.clear();
        self.selected_msgs.clear();
        self.forward_batch = None;
        self.found = None;
        self.reply = None;
        self.editing = None;
        self.rx_pop.clear();
        self.search_open = false;
        self.chat_search.clear();
        self.hidden.clear();
        self.older_pending = false;
        self.history_end = false;
        self.date_jump_active = false;
        self.scroll_to = None;
        self.highlight_msg = None;
        self.autoscroll_anchor = None;
        self.active_topic = None;
        self.topic_msgs = None;
        self.discussion = None;
        self.disc_reply_to = None;
        self.comment_input.clear();
        self.rich_runs_cache.clear();
        self.dice_spawn_times.clear();
        self.draft_translating = false;
        self.translate_draft_popup = false;
        self.inline_bot_key = None;
        self.inline_bot_at = None;
        self.inline_bot_page = None;
        self.inline_bot_loading = false;
        self.inline_bot_error.clear();
        self.msg_activity = None;
        self.user_filter = None;
        self.cmd_popup_open = false;
        self.cmd_sel_idx = 0;
        self.emoji_hover_open = false;
        self.emoji_hover_since = None;
        self.emoji_leave_since = None;
        self.ttl_popup_open = false;
        self.format_bar_open = false;
        self.composer_sel = None;
        self.pending_confirm_media = None;
        self.scrolled_up = false;
        self.msg_heights.clear();
        self.bot_keyboard_open = true;
        self.miniapp_modal = None;
        #[cfg(windows)]
        if let Some(m) = self.miniapp_view.take() {
            m.stop();
        }
    }

    /// Горячие клавиши и боковые кнопки мыши (Mouse Back / Forward, быстрые переходы, чтение на сервере).
    fn handle_shortcuts(&mut self, ctx: &egui::Context) {
        if self.app_locked || self.stage != Stage::Ready {
            return;
        }
        let modal_open = self.tgs.open
            || self.tgs.profile_open
            || self.settings_open
            || self.contacts_open
            || self.create_kind != 0
            || self.drawer_open
            || self.viewer.is_some()
            || self.forward.is_some();

        // Боковые кнопки мыши (Extra1 = Назад, Extra2 = Вперёд) и Alt+← / Alt+→ для истории чатов.
        if !modal_open {
            let nav_back = ctx.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Extra1)
                    || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowLeft))
            });
            let nav_fwd = ctx.input(|i| {
                i.pointer.button_clicked(egui::PointerButton::Extra2)
                    || (i.modifiers.alt && i.key_pressed(egui::Key::ArrowRight))
            });
            if nav_back {
                while let Some(prev_id) = self.chat_nav_back.pop() {
                    if let Some(pos) = self.dialogs.iter().position(|d| d.id == prev_id) {
                        if let Some(cur_id) = self.selected.and_then(|c| self.dialogs.get(c)).map(|d| d.id) {
                            self.chat_nav_fwd.push(cur_id);
                        }
                        self.nav_jumping = true;
                        self.open_dialog(pos);
                        self.nav_jumping = false;
                        break;
                    }
                }
            } else if nav_fwd {
                while let Some(next_id) = self.chat_nav_fwd.pop() {
                    if let Some(pos) = self.dialogs.iter().position(|d| d.id == next_id) {
                        if let Some(cur_id) = self.selected.and_then(|c| self.dialogs.get(c)).map(|d| d.id) {
                            self.chat_nav_back.push(cur_id);
                        }
                        self.nav_jumping = true;
                        self.open_dialog(pos);
                        self.nav_jumping = false;
                        break;
                    }
                }
            }
        }

        // Ctrl+K — Палитра быстрого перехода (Quick Switcher) по всем чатам и действиям.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::K)) {
            self.palette_open = !self.palette_open;
            if self.palette_open {
                self.palette_query.clear();
                self.palette_sel = 0;
            }
        }
        // Ctrl+, — открыть Настройки Telegram.
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Comma)) {
            self.open_tg_settings();
        }
        // Ctrl+P — открыть карточку «Мой профиль».
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::P)) {
            self.open_my_profile();
        }
        // Ctrl+F — поиск сообщений в открытом чате.
        if self.selected.is_some()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::F))
        {
            self.search_open = !self.search_open;
            if !self.search_open {
                self.chat_search.clear();
                self.found = None;
            }
        }
        // Ctrl+Shift+R — отметить все чаты прочитанными на сервере.
        if ctx.input_mut(|i| {
            i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::R)
        }) {
            self.force_read_all();
        }
        // Ctrl+L — заблокировать приложение локальным код-паролем.
        if !self.settings.security.passcode_hash.is_empty()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::L))
        {
            self.app_locked = true;
        }
        // Ctrl+Up / Ctrl+Down — быстрое переключение на предыдущий / следующий чат.
        if !modal_open && !self.dialogs.is_empty() {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowUp)) {
                let next = match self.selected {
                    Some(i) if i > 0 => i - 1,
                    _ => 0,
                };
                self.open_dialog(next);
            } else if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::ArrowDown)) {
                let next = match self.selected {
                    Some(i) if i + 1 < self.dialogs.len() => i + 1,
                    Some(i) => i,
                    None => 0,
                };
                self.open_dialog(next);
            }
        }
        // Ctrl+Tab — переключение вкладок папок.
        if !modal_open
            && !self.folders.is_empty()
            && ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Tab))
        {
            self.folder = match self.folder {
                None => self.folders.first().map(|f| f.id),
                Some(cur) => {
                    let pos = self.folders.iter().position(|f| f.id == cur);
                    pos.and_then(|idx| self.folders.get(idx + 1)).map(|f| f.id)
                }
            };
        }
        // Ctrl+Q — полное завершение приложения (с остановкой аудио, звонков и системного трея).
        if ctx.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Q)) {
            self.quitting = true;
            self.shutdown_resources();
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    pub(crate) fn hide_window_to_tray(&mut self) {
        self.hidden_to_tray = true;
        win::hide_to_tray();
    }

    pub(crate) fn show_window_from_tray(&mut self) {
        self.hidden_to_tray = false;
        win::show_from_tray();
    }

    fn current_tray_state(&mut self) -> tray::TrayState {
        let count_msgs = self.settings.notify.badge_count_messages;
        let inc_muted = self.settings.notify.badge_include_muted;
        let key = (self.dialogs_gen, count_msgs, inc_muted);
        let (unread, unread_muted) = if self.tray_unread_cache.0 == key {
            (self.tray_unread_cache.1, self.tray_unread_cache.2)
        } else {
            let mut total = 0i32;
            let mut unmuted_unread = 0i32;
            for d in &self.dialogs {
                let add = if count_msgs { d.unread } else { i32::from(d.unread > 0) };
                if !d.muted {
                    unmuted_unread += add;
                    total += add;
                } else if inc_muted {
                    total += add;
                }
            }
            let um = total > 0 && unmuted_unread == 0;
            self.tray_unread_cache = (key, total, um);
            (total, um)
        };
        tray::TrayState {
            enabled: self.settings.work_mode != rg_settings::WorkMode::WindowOnly,
            window_visible: !self.hidden_to_tray && win::is_window_visible(),
            icon_idx: self.settings.ayu.app_icon.parse().unwrap_or(0),
            unread,
            unread_muted,
            hide_badge: self.settings.ayu.hide_notification_badge,
            desktop_notify: self.settings.notify.desktop_notifications,
            ghost_active: self.settings.ghost.active,
            streamer_mode: self.settings.ayu.streamer_mode,
            show_ghost_toggle: self.settings.ayu.tray_ghost_toggle,
            show_streamer_toggle: self.settings.ayu.tray_streamer_toggle,
        }
    }

    fn sync_tray_and_window(&mut self, ctx: &egui::Context) {
        let streamer = self.settings.ayu.streamer_mode;
        if self.last_capture_excluded != Some(streamer) {
            self.last_capture_excluded = Some(streamer);
            win::set_capture_excluded(streamer);
        }
        let st = self.current_tray_state();
        let icon_key = (st.icon_idx, st.unread, st.unread_muted, st.hide_badge);
        if !self.is_demo && self.last_win_icon != Some(icon_key) {
            self.last_win_icon = Some(icon_key);
            win::set_window_icon(st.icon_idx, st.unread, st.unread_muted, st.hide_badge);
        }
        if self.is_demo || !st.enabled {
            if let Some(mut t) = self.tray.take() {
                t.shutdown();
            }
            return;
        }
        if self.tray.is_none() {
            self.tray = Some(tray::TrayManager::start(ctx.clone(), st.clone()));
        }
        if let Some(t) = self.tray.as_mut() {
            t.sync(st);
            let events = t.poll();
            for ev in events {
                match ev {
                    tray::TrayEvent::ToggleWindow => {
                        if win::is_window_visible() && !self.hidden_to_tray {
                            self.hide_window_to_tray();
                        } else {
                            self.show_window_from_tray();
                        }
                    }
                    tray::TrayEvent::ToggleNotifications => {
                        self.settings.notify.desktop_notifications = !self.settings.notify.desktop_notifications;
                        self.send(Cmd::Settings(Box::new(self.settings.clone())));
                    }
                    tray::TrayEvent::ToggleGhost => {
                        let on = !self.settings.ghost.active;
                        self.settings.ghost.set_active(on);
                        self.send(Cmd::Settings(Box::new(self.settings.clone())));
                    }
                    tray::TrayEvent::ToggleStreamer => {
                        self.settings.ayu.streamer_mode = !self.settings.ayu.streamer_mode;
                        self.send(Cmd::Settings(Box::new(self.settings.clone())));
                    }
                    tray::TrayEvent::Quit => {
                        self.quitting = true;
                        self.shutdown_resources();
                        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                    }
                }
            }
        }
        if let Some(uri) = self.pending_tg_uri.clone()
            && self.stage == Stage::Ready
            && !self.dialogs.is_empty()
        {
            self.pending_tg_uri = None;
            self.handle_tg_uri(&uri);
        }
    }

    fn tick_tgs(&mut self, ctx: &egui::Context) {
        if self.tgs_docs.is_empty() && self.tgs_msgs.is_empty() {
            return;
        }
        // Не нагружаем CPU в фоне, когда окно свёрнуто в трей или не в фокусе.
        let focused = !self.hidden_to_tray && ctx.input(|i| i.focused);
        if self.tgs_frame_cache.len() > 600 {
            let keep_docs = &self.visible_tgs_docs;
            let keep_msgs = &self.visible_tgs_msgs;
            let drop_keys: Vec<(i64, usize)> = self
                .tgs_frame_cache
                .keys()
                .enumerate()
                .filter(|(idx, (k, _))| {
                    let is_vis_msg = *k < -900_000
                        && i32::try_from(-*k - 1_000_000).is_ok_and(|mid| keep_msgs.contains(&mid));
                    let is_vis_doc = keep_docs.contains(k);
                    !is_vis_msg && !is_vis_doc && idx % 2 == 0
                })
                .map(|(_, k)| *k)
                .collect();
            for k in drop_keys {
                self.tgs_frame_cache.remove(&k);
            }
            // Жёсткий потолок на случай десятков одновременно видимых анимированных стикеров.
            if self.tgs_frame_cache.len() > 750 {
                let hard_drop: Vec<(i64, usize)> = self
                    .tgs_frame_cache
                    .keys()
                    .enumerate()
                    .filter(|(idx, _)| idx % 2 == 0)
                    .map(|(_, k)| *k)
                    .collect();
                for k in hard_drop {
                    self.tgs_frame_cache.remove(&k);
                }
            }
        }
        let speed = f64::from(self.settings.anim_speed) / 100.0;
        let t = if speed <= 0.01 || !focused {
            0.0
        } else {
            ctx.input(|i| i.time) * speed
        };
        let mut active = false;
        let mut rendered_this_frame = 0usize;
        let max_tgs_rasters: usize = if cfg!(target_os = "android") { 4 } else { 8 };
        let bucket_step: usize = if cfg!(target_os = "android") { 3 } else { 2 };
        let doc_px: usize = if cfg!(target_os = "android") { 72 } else { 96 };

        // Обновляем только видимые анимированные Custom Emoji / стикеры панели и сообщений (`visible_tgs_docs`):
        // невидимые рендерятся ровно 1 раз (0-й кадр) — 0% CPU в простое!
        for (&doc_id, anim) in &self.tgs_docs {
            let is_visible = self.visible_tgs_docs.contains(&doc_id);
            let bucket = if is_visible && focused && speed > 0.01 {
                let f = anim.frame_at_time(t);
                active = true;
                (f.round() as usize / bucket_step) * bucket_step
            } else {
                0
            };
            if bucket == 0 && self.sticker_tex.contains_key(&doc_id) && !is_visible {
                continue;
            }
            let key = (doc_id, bucket);
            if let Some(tex) = self.tgs_frame_cache.get(&key) {
                self.sticker_tex.insert(doc_id, tex.clone());
            } else if rendered_this_frame < max_tgs_rasters || !self.sticker_tex.contains_key(&doc_id) {
                rendered_this_frame += 1;
                let img = anim.render_frame_into(bucket as f32, doc_px, &mut self.tgs_scratch);
                let tex = ctx.load_texture(format!("tgsd_{doc_id}_{bucket}"), img, egui::TextureOptions::LINEAR);
                self.tgs_frame_cache.insert(key, tex.clone());
                self.sticker_tex.insert(doc_id, tex);
            }
        }

        // Обновляем анимированные TGS-стикеры в видимой области открытого чата (`tgs_msgs`).
        if focused && speed > 0.01 {
            let has_vis = !self.visible_tgs_msgs.is_empty();
            for (&msg_id, anim) in &self.tgs_msgs {
                if has_vis && !self.visible_tgs_msgs.contains(&msg_id) {
                    continue;
                }
                let f = anim.frame_at_time(t);
                let bucket = (f.round() as usize / bucket_step) * bucket_step;
                let key = (-i64::from(msg_id) - 1_000_000, bucket);
                if let Some(tex) = self.tgs_frame_cache.get(&key) {
                    self.textures.insert(msg_id, tex.clone());
                } else if rendered_this_frame < max_tgs_rasters || !self.textures.contains_key(&msg_id) {
                    rendered_this_frame += 1;
                    let img = anim.render_frame_into(bucket as f32, 256, &mut self.tgs_scratch);
                    let tex = ctx.load_texture(format!("tgsm_{msg_id}_{bucket}"), img, egui::TextureOptions::LINEAR);
                    self.tgs_frame_cache.insert(key, tex.clone());
                    self.textures.insert(msg_id, tex);
                }
                active = true;
            }
        }

        if active && focused && speed > 0.01 {
            let delay_ms = if cfg!(target_os = "android") { 50 } else { 33 };
            ctx.request_repaint_after(std::time::Duration::from_millis(delay_ms));
        }
    }

    pub(crate) fn handle_tg_uri(&mut self, uri: &str) -> bool {
        let Some(link) = rg_core::engine::parse_tg_link(uri) else {
            return false;
        };
        self.show_window_from_tray();
        if link == rg_core::engine::TgLink::Settings {
            self.open_tg_settings();
        } else {
            self.send(Cmd::OpenTgLink(link));
        }
        true
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        let mut tex_left: usize = if cfg!(target_os = "android") { 8 } else { 14 };
        for _ in 0..self.pending_tex_uploads.len().min(tex_left) {
            if let Some(ev) = self.pending_tex_uploads.pop_front() {
                tex_left = tex_left.saturating_sub(1);
                self.handle_event(ctx, ev);
            }
        }
        while let Ok(e) = self.ev.try_recv() {
            if matches!(
                e,
                Ev::StickerImg(..) | Ev::Image(..) | Ev::Avatar(..) | Ev::StoryImage(..)
            ) {
                if tex_left == 0 {
                    self.pending_tex_uploads.push_back(e);
                    continue;
                }
                tex_left -= 1;
            }
            self.handle_event(ctx, e);
        }
        if !self.pending_tex_uploads.is_empty() {
            ctx.request_repaint();
        }
    }

    fn handle_event(&mut self, ctx: &egui::Context, e: Ev) {
        match e {
                Ev::Stage(s) => {
                    let changed = std::mem::discriminant(&self.stage) != std::mem::discriminant(&s);
                    let is_ready = s == Stage::Ready;
                    if changed && !matches!(s, Stage::NeedCode) {
                        self.code.clear();
                    }
                    self.stage = s;
                    if changed || is_ready {
                        self.error.clear();
                    }
                }
                Ev::Me(n) => self.me = n,
                Ev::Dialogs(mut d) => {
                    // Открытый чат ищем по идентификатору: порядок строк мог измениться.
                    let keep = self.selected.and_then(|i| self.dialogs.get(i)).map(|x| x.id);
                    for row in &mut d {
                        if let Some(&read_at) = self.local_read_dates.get(&row.id)
                            && row.date <= read_at
                        {
                            row.unread = 0;
                            row.unread_mentions = 0;
                            row.unread_reactions = 0;
                        }
                    }
                    self.dialogs = d;
                    self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                    if let Some(id) = keep {
                        self.selected = self.dialogs.iter().position(|x| x.id == id);
                    }
                    self.request_emoji_statuses();
                }
                Ev::DialogsBatch(batch, _done) => {
                    if !batch.is_empty() {
                        let mut known: HashSet<i64> = self.dialogs.iter().map(|d| d.id).collect();
                        let mut added = false;
                        for mut row in batch {
                            if let Some(&read_at) = self.local_read_dates.get(&row.id)
                                && row.date <= read_at
                            {
                                row.unread = 0;
                                row.unread_mentions = 0;
                                row.unread_reactions = 0;
                            }
                            if known.insert(row.id) {
                                self.dialogs.push(row);
                                added = true;
                            }
                        }
                        if added {
                            self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                        }
                    }
                }
                Ev::Folders(f) => {
                    if self.folder.is_some_and(|id| !f.iter().any(|x| x.id == id)) {
                        self.folder = None;
                    }
                    self.folders = f;
                    self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                }
                Ev::Contacts(c) => self.contacts = c,
                Ev::Select(id) => {
                    if let Some(i) = self.dialogs.iter().position(|d| d.id == id) {
                        if let Some(d) = self.dialogs.get(i) {
                            self.in_archive = d.archived;
                        }
                        self.open_dialog(i);
                    }
                }
                Ev::StickerSets(s) => {
                    self.sticker_sets = s;
                    self.sticker_cur = 0;
                    self.stickers_loaded = true;
                }
                Ev::FavedStickers(list) => {
                    self.faved_stickers = list;
                }
                Ev::RecentStickers(list) => {
                    self.recent_stickers = list;
                }
                Ev::StickersLoaded => {
                    self.stickers_loaded = true;
                }
                Ev::Live(l) => self.on_live(ctx, l),
                Ev::PlayVideo(p) => {
                    self.player_stream = None;
                    self.player_path = Some(p);
                }
                Ev::StreamVideo(url, name) => {
                    self.player_path = None;
                    self.player_stream = Some((url, name));
                }
                Ev::PlayAudio(path, title, dur) => {
                    #[cfg(windows)]
                    if let Some(v) = self.audio_view.take() {
                        v.stop();
                    }
                    #[cfg(target_os = "android")]
                    android_jni::play_file(&path, false, self.audio_vol);
                    self.audio_stream = None;
                    self.audio_path = Some(path);
                    self.audio_title = title;
                    self.audio_dur = dur;
                }
                Ev::StreamAudio(url, title, dur) => {
                    #[cfg(windows)]
                    if let Some(v) = self.audio_view.take() {
                        v.stop();
                    }
                    self.audio_path = None;
                    self.audio_stream = Some(url);
                    self.audio_title = title;
                    self.audio_dur = dur;
                }
                Ev::GifReady(id, p) => {
                    self.gif_paths.insert(id, p);
                }
                Ev::EmojiKeywords(pairs) => {
                    if let Some(em) = self.emoji.as_mut() {
                        em.merge_keywords(pairs);
                    }
                }
                Ev::GlobalFound(list) => self.global_results = list,
                Ev::Translated(id, tr) => {
                    self.translated.insert(id, tr);
                    self.msg_heights.remove(&id);
                    self.status.clear();
                }
                Ev::Participants(id, list) => {
                    self.participants.insert(id, list);
                }
                Ev::ParticipantsPage(id, rows, offset, total) => {
                    self.participants_total.insert(id, total);
                    if offset == 0 {
                        self.participants.insert(id, rows);
                    } else {
                        let entry = self.participants.entry(id).or_default();
                        for r in rows {
                            if !entry.iter().any(|x| x.id == r.id) {
                                entry.push(r);
                            }
                        }
                    }
                }
                Ev::InviteLink(id, link) => {
                    copy_text_to_clipboard(ctx, &link);
                    if let Some(info) = self.peer_infos.get_mut(&id) {
                        info.invite_link = link.clone();
                    }
                    self.status = format!("Ссылка скопирована: {link}");
                }
                Ev::MemberSearch(list) => {
                    self.create_member_results = list;
                }
                Ev::Progress(id, done, total) => {
                    self.fetching.insert(id, (done, total));
                }
                Ev::FetchEnd(id) => {
                    self.fetching.remove(&id);
                }
                Ev::Cached(ids) => self.cached_media.extend(ids),
                Ev::Account(a) => {
                    self.blocked_ids = a.blocked.iter().map(|u| u.id).collect();
                    self.tgs.account = Some(*a);
                    self.tgs.filled = false;
                }
                Ev::SavedHistory(id, rows) => {
                    self.audit_history = Some((id, rows));
                }
                Ev::TgsDoc(id, bytes) => {
                    if self.tgs_docs.len() > 160 {
                        self.tgs_docs.retain(|k, _| self.visible_tgs_docs.contains(k));
                    }
                    if let Some(anim) = lottie::TgsAnim::from_bytes(&bytes) {
                        self.tgs_docs.insert(id, anim);
                    }
                }
                Ev::TgsMsg(msg_id, bytes) => {
                    if self.tgs_msgs.len() > 60 {
                        self.tgs_msgs.retain(|k, _| self.visible_tgs_msgs.contains(k));
                    }
                    if let Some(anim) = lottie::TgsAnim::from_bytes(&bytes) {
                        self.tgs_msgs.insert(msg_id, anim);
                    }
                }
                Ev::Gifs(g) => {
                    self.gif_list = g;
                    self.gifs_loaded = true;
                }
                Ev::GifsLoaded => {
                    self.gifs_loaded = true;
                }
                Ev::PeerInfo(id, info) => {
                    if let Some(doc_id) = info.emoji_status
                        && !self.sticker_tex.contains_key(&doc_id)
                        && self.custom_emoji_req.insert(doc_id)
                    {
                        self.send(Cmd::CustomEmojiDocs(vec![doc_id]));
                    }
                    if let Some(did) = self.selected_dialog_id().filter(|&did| did == id) {
                        if let Some(pid) = info.pinned_msg_id {
                            if self.pinned_msgs.get(&id).is_none_or(|m| m.id != pid)
                                && self.pinned_req.insert((id, pid))
                            {
                                self.send(Cmd::LoadPinned(did, pid));
                            }
                            if self.pinned_list_req.insert(id) {
                                self.send(Cmd::LoadPinnedMessages(id));
                            }
                        }
                        if info.forum
                            && !self.forum_topics.contains_key(&id)
                            && self.forum_req.insert(id)
                        {
                            self.send(Cmd::ForumTopics(did));
                        }
                    }
                    self.peer_infos.insert(id, *info);
                }
                Ev::ForumTopics(id, topics) => {
                    self.forum_topics.insert(id, topics);
                }
                Ev::TopicMessages(id, topic_id, rows) => {
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == id)
                        && self.active_topic == Some(topic_id)
                    {
                        if let Some(idx) = self.selected {
                            self.request_previews(idx, &rows);
                        }
                        self.topic_msgs = Some((topic_id, rows));
                    }
                }
                Ev::Discussion(thread) => {
                    self.disc_reply_to = None;
                    self.discussion = Some(thread);
                }
                Ev::Stories(list) => {
                    self.stories = list;
                }
                Ev::PinnedMsg(id, msg) => {
                    let msg_v = *msg;
                    let list = self.pinned_lists.entry(id).or_default();
                    if !list.iter().any(|x| x.id == msg_v.id) {
                        list.insert(0, msg_v.clone());
                    }
                    self.pinned_cur_idx.entry(id).or_insert(0);
                    self.pinned_msgs.insert(id, msg_v);
                }
                Ev::MiniAppReady { title, url } => {
                    #[cfg(windows)]
                    {
                        if let Some(v) = self.miniapp_view.take() {
                            v.stop();
                        }
                        self.miniapp_opened_at = Some(std::time::Instant::now());
                        self.miniapp_expanded = false;
                    }
                    self.miniapp_modal = Some((title, url));
                    self.bot_btn_pending = None;
                }
                Ev::BotAlert(msg) => {
                    self.bot_alert = Some(msg);
                    self.bot_btn_pending = None;
                }
                Ev::NavigateTo { id, post } => {
                    if let Some(pos) = self.dialogs.iter().position(|d| d.id == id) {
                        if let Some(d) = self.dialogs.get(pos) {
                            self.in_archive = d.archived;
                        }
                        self.open_dialog(pos);
                        if let Some(pid) = post {
                            self.scroll_to = Some(pid);
                            self.highlight_msg = Some((pid, ctx.input(|i| i.time)));
                        }
                    }
                }
                Ev::LocalPurged(did, msg_id) => {
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        self.messages.retain(|m| m.id != msg_id);
                        self.msg_heights.remove(&msg_id);
                    }
                    if let Some(cached) = self.history_cache.get_mut(&did) {
                        cached.retain(|m| m.id != msg_id);
                    }
                }
                Ev::LocalPurgedAll(did) => {
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        self.messages.retain(|m| !m.deleted);
                        self.msg_heights.clear();
                    }
                    if let Some(cached) = self.history_cache.get_mut(&did) {
                        cached.retain(|m| !m.deleted);
                    }
                }
                Ev::ResetSession => {
                    self.reset_chat_state();
                    self.pending_tex_uploads.clear();
                    self.visible_tgs_msgs.clear();
                    self.selected = None;
                    self.dialogs.clear();
                    self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                    self.folders.clear();
                    self.avatars.clear();
                    self.avatar_req.clear();
                    self.peer_infos.clear();
                    self.participants.clear();
                    self.participants_total.clear();
                    self.peer_saved_gifts.clear();
                    self.saved_gifts_req.clear();
                    self.history_cache.clear();
                    self.history_cache_order.clear();
                    self.history_fresh_at.clear();
                    self.archive_loaded = false;
                    self.in_archive = false;
                    self.stickers_requested = false;
                    self.stickers_loaded = false;
                    self.gifs_requested = false;
                    self.gifs_loaded = false;
                    self.faved_stickers.clear();
                    self.recent_stickers.clear();
                    self.sticker_sets.clear();
                    self.gif_list.clear();
                    self.story_textures.clear();
                    self.story_media_req.clear();
                    self.story_errors.clear();
                    self.transcriptions.clear();
                    self.transcribing.clear();
                    self.me.clear();
                    self.stage = Stage::Starting;
                    self.code.clear();
                    self.password.clear();
                    self.error.clear();
                    self.tgs.account = None;
                    self.tgs.open = false;
                    self.settings_open = false;
                    self.drawer_open = false;
                    self.miniapp_modal = None;
                }
                Ev::EmojiSets(s) => {
                    if self.emoji_pack.is_some_and(|i| i >= s.len()) {
                        self.emoji_pack = None;
                    }
                    self.emoji_sets = s;
                }
                Ev::FeaturedEmoji(s) => self.featured_sets = s,
                Ev::StickerList(id, l) => {
                    self.sticker_lists.insert(id, l);
                }
                Ev::StickerImg(id, img) => {
                    // Ограничение кэша текстур стикеров/эмодзи в ОЗУ (до 360 шт.), не вытесняя видимые.
                    if self.sticker_tex.len() > 360 {
                        let keep: HashSet<i64> = self
                            .dialogs
                            .iter()
                            .filter_map(|d| d.emoji_status)
                            .chain(self.visible_tgs_docs.iter().copied())
                            .chain(self.tgs_docs.keys().copied())
                            .collect();
                        self.sticker_tex.retain(|k, _| keep.contains(k));
                        self.sticker_req.retain(|k| self.sticker_tex.contains_key(k));
                        self.custom_emoji_req.retain(|k| self.sticker_tex.contains_key(k));
                    }
                    let tex = ctx.load_texture(format!("s{id}"), *img, egui::TextureOptions::LINEAR);
                    self.sticker_tex.insert(id, tex);
                }
                Ev::History(dialog_id, m) => {
                    // Сохраняем в 64-чатовый LRU-кэш ОЗУ даже при быстром переключении чатов.
                    if !self.history_cache.contains_key(&dialog_id) && self.history_cache.len() >= 64 {
                        while let Some(drop_key) = self.history_cache_order.pop_front() {
                            if drop_key != dialog_id && self.history_cache.remove(&drop_key).is_some() {
                                self.history_fresh_at.remove(&drop_key);
                                break;
                            }
                        }
                    }
                    self.history_cache_order.retain(|&x| x != dialog_id);
                    self.history_cache_order.push_back(dialog_id);
                    self.history_cache.insert(dialog_id, m.clone());
                    self.history_fresh_at.insert(dialog_id, std::time::Instant::now());
                    // Сопоставляем по стабильному `dialog_id` — перестановки списка чатов больше не ломают загрузку!
                    if let Some(idx) = self
                        .selected
                        .filter(|&i| self.dialogs.get(i).is_some_and(|d| d.id == dialog_id))
                    {
                        let now_t = ctx.input(|i| i.time);
                        if self.known.is_empty() {
                            for r in &m {
                                self.arrived.insert(r.id, now_t);
                            }
                        } else {
                            for r in &m {
                                if !self.known.contains(&r.id) {
                                    self.arrived.insert(r.id, now_t);
                                    if r.effect.is_some() {
                                        self.effect_bursts.insert(r.id, now_t);
                                    }
                                }
                            }
                        }
                        self.known.extend(m.iter().map(|r| r.id));
                        self.request_previews(idx, &m);
                        if let Some((jdid, jmid, jdate)) = self.pending_jump_to_msg
                            && jdid == dialog_id
                        {
                            if m.iter().any(|r| r.id == jmid) {
                                self.scroll_to = Some(jmid);
                                self.highlight_msg = Some((jmid, now_t));
                                self.pending_jump_to_msg = None;
                            } else if jdate > 0 {
                                self.pending_jump_to_msg = Some((jdid, jmid, 0));
                                self.send(Cmd::JumpToDate(jdid, (jdate + 2).min(i64::from(i32::MAX)) as i32));
                            }
                        }
                        self.messages = m;
                        self.history_loading = false;
                        self.older_pending = false;
                    }
                }
                Ev::HistoryError(dialog_id, err) => {
                    if self
                        .selected
                        .and_then(|i| self.dialogs.get(i))
                        .is_some_and(|d| d.id == dialog_id)
                    {
                        self.history_loading = false;
                        self.older_pending = false;
                    }
                    self.error = err;
                    self.error_at = self.now;
                }
                Ev::Older(did, rows) if self.selected_dialog_id() == Some(did) => {
                    self.older_pending = false;
                    if rows.is_empty() {
                        self.history_end = true;
                    } else if let Some(idx) = self.selected {
                        let now_t = ctx.input(|i| i.time);
                        self.scroll_to = self.messages.first().map(|m| m.id);
                        self.request_previews(idx, &rows);
                        let known: HashSet<i32> = self.messages.iter().map(|m| m.id).collect();
                        let mut all: Vec<MessageRow> =
                            rows.into_iter().filter(|r| !known.contains(&r.id)).collect();
                        for r in &all {
                            self.arrived.insert(r.id, now_t);
                        }
                        all.append(&mut self.messages);
                        self.messages = all;
                    }
                }
                Ev::Older(..) => {}
                Ev::Found(did, rows) if self.selected_dialog_id() == Some(did) => {
                    if let Some(idx) = self.selected {
                        self.request_previews(idx, &rows);
                    }
                    self.chat_search_idx = rows.len().saturating_sub(1);
                    if let Some(last) = rows.last() {
                        if self.messages.iter().any(|m| m.id == last.id) {
                            self.scroll_to = Some(last.id);
                            self.highlight_msg = Some((last.id, ctx.input(|i| i.time)));
                        } else {
                            self.send(Cmd::JumpToMessage(did, last.id));
                        }
                    }
                    self.found = Some(rows);
                }
                Ev::Found(..) => {}
                Ev::EditHistory(id, v) => self.edit_history = Some((id, v)),
                Ev::Image(did, id, img) if self.selected_dialog_id() == Some(did) => {
                    let tex = ctx.load_texture(format!("m{id}"), *img, egui::TextureOptions::LINEAR);
                    self.textures.insert(id, tex);
                    self.img_arrived.insert(id, ctx.input(|i| i.time));
                }
                Ev::Image(..) => {}
                Ev::Avatar(id, img) => {
                    if self.avatars.len() > 160 && !self.avatars.contains_key(&id) {
                        let keep: HashSet<i64> = self.dialogs.iter().take(40).map(|d| d.id).collect();
                        self.avatars.retain(|k, _| keep.contains(k));
                        self.avatar_req.retain(|k| self.avatars.contains_key(k));
                    }
                    let tex = ctx.load_texture(format!("a{id}"), *img, egui::TextureOptions::LINEAR);
                    self.avatars.insert(id, tex);
                }
                Ev::Settings(s) => {
                    self.settings = *s;
                    self.plugins.sync_disabled(&self.settings.disabled_plugins);
                    theme::sync_custom_theme(&self.settings.theme);
                    icons::set_icon_pack(
                        &self.settings.theme.icon_pack,
                        Some(&worker::home().join("icon_packs")),
                    );
                    if !self.passcode_checked {
                        self.passcode_checked = true;
                        if !self.settings.security.passcode_hash.is_empty() {
                            self.app_locked = true;
                        }
                        if self.autostart_minimized
                            && self.settings.start_minimized
                            && self.settings.work_mode != rg_settings::WorkMode::WindowOnly
                        {
                            self.hide_window_to_tray();
                        }
                        self.autostart_minimized = false;
                    }
                }
                Ev::LoginBusy(b) => {
                    self.login_busy = b;
                    ctx.request_repaint();
                }
                Ev::CodeDeliveryHint(h) => {
                    self.login_code_hint = h;
                }
                Ev::CodeMeta { length, resend_secs } => {
                    self.login_code_len = length.clamp(4, 12);
                    if let Some(secs) = resend_secs {
                        self.login_resend_secs = (secs as u64).clamp(15, 300);
                    }
                }
                Ev::StoryImage(sid, img) => {
                    let tex = ctx.load_texture(format!("story-{sid}"), *img, egui::TextureOptions::LINEAR);
                    self.story_textures.insert(sid, tex);
                    self.story_errors.remove(&sid);
                }
                Ev::StoryError(sid, err) => {
                    self.story_errors.insert(sid, err);
                }
                Ev::Transcribed(msg_id, text) => {
                    self.transcribing.remove(&msg_id);
                    self.transcriptions.insert(msg_id, text);
                }
                Ev::ScheduledHistory(did, rows) => {
                    if self
                        .selected
                        .and_then(|i| self.dialogs.get(i))
                        .is_some_and(|d| d.id == did)
                    {
                        self.scheduled_msgs = rows;
                    }
                }
                Ev::SavedTags(tags) => {
                    self.saved_tags = tags;
                }
                Ev::SendAsList(did, list) => {
                    self.send_as_map.insert(did, list);
                }
                Ev::HashtagResults(tag, rows) => {
                    self.hashtag_modal = Some((tag, rows));
                }
                Ev::AvailableEffects(eff) => {
                    self.available_effects = eff;
                }
                Ev::PeekHistory(did, name, rows) => {
                    self.peek_modal = Some((did, name, rows));
                }
                Ev::AdminLog(rows) => self.admin_log = rows,
                Ev::AdminInvites(rows) => self.admin_invites = rows,
                Ev::SecretChats(list) => self.secret_chats = list,
                Ev::SecretMessages(cid, msgs) => {
                    if self.secret_selected == Some(cid) {
                        self.secret_messages = msgs;
                    }
                }
                Ev::CallStarted {
                    call_id,
                    access_hash,
                    peer_name,
                    video,
                } => {
                    if let Some(c) = self.active_call.as_mut() {
                        c.call_id = call_id;
                        c.access_hash = access_hash;
                        c.peer_name = peer_name;
                        c.video = video;
                    }
                }
                Ev::CallEmojis(em) => {
                    if let Some(c) = self.active_call.as_mut() {
                        c.emojis = Some(em);
                        c.state = rg_core::calls::CallState::Active;
                        if c.connected_at.is_none() {
                            c.connected_at = Some(std::time::Instant::now());
                        }
                    }
                }
                Ev::CallMediaStats(st) => {
                    self.call_media_stats = Some(st);
                }
                Ev::StarsStatus(st) => {
                    self.stars_summary = Some(st);
                }
                Ev::StarGiftsCatalog(list) => {
                    self.star_gifts_catalog = list;
                }
                Ev::SavedGifts(pid, list) => {
                    let is_mine = self.dialogs.iter().find(|d| d.saved).is_some_and(|d| d.id == pid)
                        || self.tgs.account.as_ref().is_some_and(|a| a.me.id == pid);
                    if is_mine {
                        self.my_saved_gifts = list.clone();
                    }
                    self.peer_saved_gifts.insert(pid, list);
                }
                Ev::QuickReplies(list) => {
                    self.quick_replies = list;
                }
                Ev::UpdateAvailable(info) => {
                    self.update_available = Some(*info);
                    self.update_dismissed = false;
                }
                Ev::UpdateProgress(done, total) => {
                    self.update_progress = Some((done, total));
                }
                Ev::UpdateReady(msg) => {
                    self.update_progress = None;
                    self.status = msg;
                }
                Ev::DraftTranslated(tr) => {
                    self.draft_translating = false;
                    self.translate_draft_popup = false;
                    if !tr.is_empty() || !self.input.is_empty() {
                        self.input = tr;
                    }
                }
                Ev::InlineBotResults {
                    bot,
                    query,
                    append,
                    page,
                } => {
                    self.inline_bot_loading = false;
                    self.inline_bot_error.clear();
                    if self.inline_bot_key.as_ref() == Some(&(bot, query)) {
                        if append && let Some(prev) = self.inline_bot_page.as_mut() {
                            prev.query_id = page.query_id;
                            prev.next_offset = page.next_offset;
                            for r in page.results {
                                if !prev.results.iter().any(|x| x.id == r.id) {
                                    prev.results.push(r);
                                }
                            }
                        } else {
                            self.inline_bot_page = Some(page);
                        }
                    }
                }
                Ev::InlineBotError(e) => {
                    self.inline_bot_loading = false;
                    self.inline_bot_error = e;
                }
                Ev::AvailableReactions(list) => {
                    self.available_reactions = list;
                }
                Ev::DefaultEmojiStatuses(ids) => {
                    self.default_emoji_statuses = ids;
                }
                Ev::MessageActivity {
                    msg_id,
                    readers,
                    reactions,
                } => {
                    self.msg_activity = Some((msg_id, readers, reactions));
                }
                Ev::SentConfirm(did, temp_id, row) => {
                    let row = *row;
                    let now_t = ctx.input(|i| i.time);
                    if row
                        .media
                        .as_ref()
                        .is_some_and(|m| matches!(m.kind, rg_core::media::MediaKind::Dice { .. }))
                    {
                        self.dice_spawn_times.insert(row.id, now_t);
                    }
                    let preview: String = if !row.text.is_empty() {
                        row.text.chars().take(80).collect()
                    } else {
                        row.media.as_ref().map(|m| m.name.clone()).unwrap_or_default()
                    };
                    if let Some(d) = self.dialogs.iter_mut().find(|d| d.id == did) {
                        if !preview.is_empty() {
                            d.last = preview;
                        }
                        d.date = d.date.max(row.date);
                        self.dialogs_gen += 1;
                    }
                    let is_open = self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did);
                    if is_open {
                        if let Some(idx) = self.selected {
                            self.request_previews(idx, std::slice::from_ref(&row));
                        }
                        self.known.insert(row.id);
                        self.arrived.entry(row.id).or_insert(now_t);
                        self.rich_runs_cache.retain(|k, _| k.0 != temp_id && k.0 != row.id);
                        if temp_id < 0 {
                            if self.messages.iter().any(|m| m.id == row.id) {
                                self.messages.retain(|m| m.id != temp_id);
                            } else if let Some(slot) = self.messages.iter_mut().find(|m| m.id == temp_id) {
                                *slot = row.clone();
                            } else {
                                self.messages.push(row.clone());
                                self.messages.sort_by_key(|m| (m.date, m.id));
                            }
                        } else if let Some(slot) = self.messages.iter_mut().find(|m| m.id == row.id) {
                            *slot = row.clone();
                        } else {
                            self.messages.push(row.clone());
                            self.messages.sort_by_key(|m| (m.date, m.id));
                        }
                    }
                    if let Some(cached) = self.history_cache.get_mut(&did) {
                        if temp_id < 0 {
                            cached.retain(|m| m.id != temp_id);
                        }
                        if let Some(slot) = cached.iter_mut().find(|m| m.id == row.id) {
                            *slot = row;
                        } else {
                            cached.push(row);
                            cached.sort_by_key(|m| (m.date, m.id));
                        }
                    }
                }
                Ev::SendFailed(did, temp_id, _err) => {
                    self.rich_runs_cache.retain(|k, _| k.0 != temp_id);
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        self.messages.retain(|m| m.id != temp_id);
                    }
                    if let Some(cached) = self.history_cache.get_mut(&did) {
                        cached.retain(|m| m.id != temp_id);
                    }
                }
                Ev::CommentConfirmed(post_id, temp_id, row) => {
                    let row = *row;
                    if let Some(disc) = self.discussion.as_mut()
                        && disc.post_id == post_id
                    {
                        if let Some(slot) = disc.comments.iter_mut().find(|c| c.id == temp_id) {
                            *slot = row;
                        } else if !disc.comments.iter().any(|c| c.id == row.id) {
                            disc.comments.push(row);
                        }
                    }
                    if let Some(post) = self.messages.iter_mut().find(|m| m.id == post_id) {
                        post.replies_count = post.replies_count.saturating_add(1);
                    }
                }
                Ev::CommentFailed(post_id, temp_id, _err) => {
                    if let Some(disc) = self.discussion.as_mut()
                        && disc.post_id == post_id
                    {
                        disc.comments.retain(|c| c.id != temp_id);
                    }
                }
                Ev::Drafts(rows) => {
                    for d in rows {
                        if d.text.trim().is_empty() && d.reply_to.is_none() {
                            self.drafts.remove(&d.peer_id);
                        } else {
                            self.drafts.insert(d.peer_id, (d.text.clone(), d.reply_to));
                        }
                    }
                    if let Some(cur_id) = self.selected.and_then(|i| self.dialogs.get(i)).map(|d| d.id)
                        && self.input.is_empty()
                        && self.draft_dirty_at.is_none()
                        && let Some((txt, r)) = self.drafts.get(&cur_id).cloned()
                    {
                        self.input = txt;
                        if self.reply.is_none()
                            && let Some(rid) = r
                        {
                            self.reply = Some((rid, format!("#{rid}")));
                        }
                    }
                    self.dialogs_gen += 1;
                }
                Ev::JumpToMessage(did, msg_id) => {
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        if msg_id == 0 {
                            if let Some(sel) = self.selected
                                && let Some(d) = self.dialogs.get_mut(sel)
                            {
                                d.unread_mentions = 0;
                                self.dialogs_gen += 1;
                            }
                        } else {
                            self.scroll_to = Some(msg_id);
                            self.highlight_msg = Some((msg_id, ctx.input(|i| i.time)));
                        }
                    }
                }
                Ev::StickerPack(pack) => {
                    self.sticker_pack_modal = Some(*pack);
                }
                Ev::StickerPackInstalled(set_id, installed) => {
                    if let Some(m) = self.sticker_pack_modal.as_mut()
                        && m.id == set_id
                    {
                        m.installed = installed;
                    }
                }
                Ev::UsernameUpdated(u) => {
                    self.profile_username_edit = u;
                }
                Ev::BirthdayUpdated(_) => {
                    self.profile_bday_synced = false;
                }
                Ev::SimilarChannels(did, list) => {
                    self.similar_channels.insert(did, list);
                }
                Ev::GlobalDeletedLog { query, items, stats } => {
                    if query == self.global_deleted_query || self.global_deleted_items.is_empty() {
                        self.global_deleted_items = items;
                        self.global_deleted_stats = stats;
                    }
                }
                Ev::DateJumpHistory(did, rows, _target_unix) => {
                    if let Some(idx) = self
                        .selected
                        .filter(|&i| self.dialogs.get(i).is_some_and(|d| d.id == did))
                    {
                        self.known.clear();
                        self.arrived.clear();
                        let now_t = ctx.input(|i| i.time);
                        for r in &rows {
                            self.known.insert(r.id);
                            self.arrived.insert(r.id, now_t);
                        }
                        self.request_previews(idx, &rows);
                        if let Some((_, jmid, _)) = self.pending_jump_to_msg.take().filter(|p| p.0 == did) {
                            let target = if rows.iter().any(|r| r.id == jmid) {
                                Some(jmid)
                            } else {
                                rows.iter().min_by_key(|r| (r.id - jmid).abs()).map(|r| r.id)
                            };
                            if let Some(tid) = target {
                                self.scroll_to = Some(tid);
                                self.highlight_msg = Some((tid, now_t));
                            } else {
                                self.scroll_to = rows.first().map(|r| r.id);
                            }
                        } else {
                            self.scroll_to = rows.first().map(|r| r.id);
                        }
                        self.messages = rows;
                        self.history_loading = false;
                        self.older_pending = false;
                        self.date_jump_active = true;
                    }
                }
                Ev::CommonChats(uid, list) => {
                    self.common_chats.insert(uid, list);
                }
                Ev::ContactUpdated(uid, is_contact, name) => {
                    if let Some(d) = self.dialogs.iter_mut().find(|d| d.id == uid) {
                        d.contact = is_contact;
                        if is_contact && !name.is_empty() {
                            d.name = name.clone();
                        }
                        self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                    }
                    if let Some(info) = self.peer_infos.get_mut(&uid)
                        && is_contact
                        && !name.is_empty()
                    {
                        info.name = name;
                    }
                    self.contact_edit_open = None;
                }
                Ev::GlobalMessages(q, hits) => {
                    self.global_msg_loading = false;
                    let cur_q = format!("{}:{}", self.global_msg_scope, self.search.trim().to_lowercase());
                    if cur_q == q || self.search.trim().to_lowercase() == q {
                        self.global_msg_hits = hits;
                    }
                }
                Ev::HistoryCleared(did) => {
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        if self.settings.ayu.save_deleted_messages {
                            for m in &mut self.messages {
                                m.deleted = true;
                            }
                        } else {
                            self.messages.clear();
                        }
                    }
                    if let Some(cached) = self.history_cache.get_mut(&did) {
                        if self.settings.ayu.save_deleted_messages {
                            for m in cached {
                                m.deleted = true;
                            }
                        } else {
                            cached.clear();
                        }
                    }
                }
                Ev::LeftChat(did) => {
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        self.selected = None;
                        self.messages.clear();
                    }
                    self.dialogs.retain(|d| d.id != did);
                    self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                    self.history_cache.remove(&did);
                }
                Ev::PinnedMessagesList(did, rows) => {
                    self.request_reaction_docs(&rows);
                    if let Some(first) = rows.first() {
                        self.pinned_msgs.insert(did, first.clone());
                    } else {
                        self.pinned_msgs.remove(&did);
                    }
                    self.pinned_lists.insert(did, rows.clone());
                    let cur = self.pinned_cur_idx.entry(did).or_insert(0);
                    if !rows.is_empty() && *cur >= rows.len() {
                        *cur = 0;
                    }
                    self.pinned_list_did = Some(did);
                    self.pinned_list = rows;
                }
                Ev::AllUnpinned(did) => {
                    self.pinned_msgs.remove(&did);
                    self.pinned_lists.remove(&did);
                    self.pinned_cur_idx.remove(&did);
                    if self.pinned_list_did == Some(did) {
                        self.pinned_list.clear();
                    }
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        for m in &mut self.messages {
                            m.pinned = false;
                        }
                    }
                }
                Ev::Unpinned(did, msg_id) => {
                    if let Some(list) = self.pinned_lists.get_mut(&did) {
                        list.retain(|m| m.id != msg_id);
                        if let Some(next) = list.first().cloned() {
                            self.pinned_msgs.insert(did, next);
                        } else {
                            self.pinned_msgs.remove(&did);
                        }
                        if let Some(cur) = self.pinned_cur_idx.get_mut(&did) {
                            *cur = if list.is_empty() { 0 } else { (*cur).min(list.len() - 1) };
                        }
                    } else if self.pinned_msgs.get(&did).is_some_and(|m| m.id == msg_id) {
                        self.pinned_msgs.remove(&did);
                    }
                    if self.pinned_list_did == Some(did) {
                        self.pinned_list.retain(|m| m.id != msg_id);
                    }
                    if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id == did) {
                        for m in &mut self.messages {
                            if m.id == msg_id {
                                m.pinned = false;
                            }
                        }
                    }
                }
                Ev::HistoryAround(did, rows, target_id) => {
                    if let Some(idx) = self.selected.filter(|&i| self.dialogs.get(i).is_some_and(|d| d.id == did)) {
                        self.request_previews(idx, &rows);
                        self.known.extend(rows.iter().map(|r| r.id));
                        self.messages = rows;
                        self.history_loading = false;
                        self.msg_heights.clear();
                        self.scroll_to = Some(target_id);
                        self.highlight_msg = Some((target_id, ctx.input(|i| i.time)));
                    }
                }
                Ev::SelectDialogById(did) => {
                    if let Some(pos) = self.dialogs.iter().position(|d| d.id == did) {
                        if let Some(d) = self.dialogs.get(pos) {
                            self.in_archive = d.archived;
                        }
                        self.open_dialog(pos);
                    }
                }
                Ev::OpenResolvedDialog(dlg) => {
                    let did = dlg.id;
                    if let Some(pos) = self.dialogs.iter().position(|d| d.id == did) {
                        if let Some(d) = self.dialogs.get(pos) {
                            self.in_archive = d.archived;
                        }
                        self.open_dialog(pos);
                    } else {
                        self.dialogs.insert(0, *dlg);
                        self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                        self.in_archive = false;
                        self.open_dialog(0);
                    }
                }
                Ev::FolderInviteUrl(url) => {
                    copy_text_to_clipboard(ctx, &url);
                    self.folder_invite_url = Some(url.clone());
                    self.status = format!("Ссылка на папку скопирована: {url}");
                }
                Ev::FolderInvitePreview(prev) => {
                    self.folder_invite_preview = Some(prev);
                }
                Ev::Status(s) => {
                    self.status = s;
                    self.bot_btn_pending = None;
                }
                Ev::Error(e) => {
                    self.error = e;
                    self.error_at = self.now;
                    self.login_busy = false;
                    self.bot_btn_pending = None;
                }
        }
    }

    // ---------- вход (как в официальном Telegram + анимации + поддержка нативной Android-клавиатуры) ----------

    #[cfg_attr(not(target_os = "android"), allow(dead_code))]
    fn open_android_input(&mut self, tag: &str, title: &str, hint: &str, initial: &str, kind: i32) {
        #[cfg(target_os = "android")]
        {
            self.android_input_pending = true;
            android_jni::prompt_text_input(tag, title, hint, initial, kind);
        }
        #[cfg(not(target_os = "android"))]
        let _ = (tag, title, hint, initial, kind);
    }

    pub(crate) fn hook_android_text(
        &mut self,
        resp: &egui::Response,
        tag: &str,
        title: &str,
        hint: &str,
        initial: &str,
        kind: i32,
    ) {
        if cfg!(target_os = "android") && !self.android_input_pending && (resp.clicked() || resp.gained_focus()) {
            self.open_android_input(tag, title, hint, initial, kind);
        }
    }

    fn poll_android_bridge(&mut self, ctx: &egui::Context) {
        #[cfg(target_os = "android")]
        {
            if self.android_pick_target.is_some()
                && let Some(picked) = android_jni::poll_picked_file()
            {
                match self.android_pick_target.take() {
                    Some(AndroidPickTarget::Story) => {
                        self.story_file_path = picked.to_string_lossy().into_owned();
                    }
                    Some(AndroidPickTarget::Theme) => {
                        self.apply_imported_theme(ctx, &picked);
                        self.send(Cmd::Settings(Box::new(self.settings.clone())));
                    }
                    _ => {
                        let reply = self.reply.take().map(|(id, _)| id);
                        let caption = std::mem::take(&mut self.input);
                        self.media_editor = Some(media_editor::MediaEditorState::open(
                            picked,
                            caption.trim().to_owned(),
                            reply,
                        ));
                    }
                }
            }
            if self.android_webview_open {
                if let Some(tgu) = android_jni::poll_pending_link() {
                    if let Some(parsed) = rg_core::engine::parse_tg_link(&tgu) {
                        self.send(Cmd::OpenTgLink(parsed));
                    }
                    self.android_webview_open = false;
                    self.miniapp_modal = None;
                } else if !android_jni::is_webview_open() {
                    self.android_webview_open = false;
                    self.miniapp_modal = None;
                }
            }
            if self.android_input_pending {
                ctx.request_repaint_after(std::time::Duration::from_millis(120));
                if let Some((tag, val)) = android_jni::poll_text_input() {
                    self.android_input_pending = false;
                    match tag.as_str() {
                        "" => {}
                        "login_phone" => {
                            self.login_phone_full = countries::format_phone_input(&val);
                            if let Some(idx) = countries::detect_country_from_full(&self.login_phone_full) {
                                self.login_country_idx = idx;
                                self.login_dial = countries::COUNTRIES[idx].dial.to_owned();
                            }
                        }
                        "country_search" => {
                            self.login_country_query = val;
                        }
                        "login_code" => {
                            self.code = val.chars().filter(char::is_ascii_digit).collect();
                        }
                        "login_password" => {
                            self.password = val;
                        }
                        "proxy" => {
                            self.proxy = val;
                        }
                        "key_id" => {
                            self.key_id = val.chars().filter(char::is_ascii_digit).collect();
                        }
                        "key_hash" => {
                            self.key_hash = val.trim().to_owned();
                        }
                        "lock_input" => {
                            self.lock_input = val;
                        }
                        "dialogs_search" => {
                            self.search = val;
                        }
                        "chat_input" => {
                            self.input = val;
                            if let Some(idx) = self.selected {
                                self.draft_dirty_at = Some((idx, ctx.input(|i| i.time)));
                            }
                        }
                        "chat_search" => {
                            self.chat_search = val;
                            if !self.chat_search.trim().is_empty()
                                && let Some(did) = self.selected_dialog_id()
                            {
                                self.send(Cmd::Search(did, self.chat_search.clone()));
                            }
                        }
                        "comment_input" => {
                            self.comment_input = val;
                        }
                        "forward_filter" => {
                            self.forward_filter = val;
                        }
                        "story_reply" => {
                            self.story_reply_text = val;
                        }
                        "tgs_folder_link" => {
                            self.tgs.folder_invite_link = val;
                        }
                        _ => {}
                    }
                }
            }
        }
        #[cfg(not(target_os = "android"))]
        let _ = ctx;
    }

    #[allow(clippy::too_many_arguments)]
    fn outlined_login_field<R>(
        ui: &mut egui::Ui,
        id_tag: &str,
        width: f32,
        height: f32,
        label: &str,
        active: bool,
        accent: Color32,
        bg_cut: Color32,
        inner: impl FnOnce(&mut egui::Ui) -> R,
    ) -> (egui::Rect, R) {
        let focus_k = theme::tween(ui.ctx(), ("lg-fld", id_tag), f32::from(u8::from(active)), 0.14);
        let stroke_c = lerp_color(Color32::from_white_alpha(52), accent, focus_k);
        let stroke_w = 1.0 + 0.85 * focus_k;
        let label_c = lerp_color(Color32::from_rgb(154, 154, 164), accent, focus_k);

        let (rect, _) = ui.allocate_exact_size(Vec2::new(width, height + 6.0), Sense::hover());
        let box_rect = egui::Rect::from_min_max(rect.min + Vec2::new(0.0, 6.0), rect.max);
        let p = ui.painter();
        p.rect_filled(box_rect, egui::CornerRadius::same(11), Color32::from_white_alpha(7));
        p.rect_stroke(
            box_rect,
            egui::CornerRadius::same(11),
            egui::Stroke::new(stroke_w, stroke_c),
            egui::StrokeKind::Inside,
        );
        // Вырез и плавающая подпись на верхней кромке рамки (как в Telegram Web/Desktop).
        let galley = p.layout_no_wrap(label.to_owned(), FontId::proportional(11.5), label_c);
        let cut_rect = egui::Rect::from_min_size(
            egui::pos2(box_rect.left() + 12.0, box_rect.top() - 7.0),
            Vec2::new(galley.size().x + 10.0, 14.0),
        );
        p.rect_filled(cut_rect, egui::CornerRadius::same(3), bg_cut);
        p.galley(
            egui::pos2(cut_rect.left() + 5.0, cut_rect.top()),
            galley,
            label_c,
        );

        let content_rect = box_rect.shrink2(Vec2::new(14.0, 4.0));
        let res = ui.scope_builder(
            egui::UiBuilder::new().max_rect(content_rect),
            |ui| {
                ui.set_min_size(content_rect.size());
                ui.with_layout(egui::Layout::left_to_right(egui::Align::Center), |ui| {
                    inner(ui)
                })
                .inner
            },
        );
        (box_rect, res.inner)
    }

    #[allow(clippy::too_many_arguments)]
    fn outlined_login_button(
        ui: &mut egui::Ui,
        id_tag: &str,
        width: f32,
        height: f32,
        label: &str,
        active: bool,
        accent: Color32,
        bg_cut: Color32,
        value: &str,
        hint: &str,
        right: LoginButtonRight,
    ) -> egui::Response {
        let focus_k = theme::tween(ui.ctx(), ("lg-fld", id_tag), f32::from(u8::from(active)), 0.14);
        let stroke_c = lerp_color(Color32::from_white_alpha(52), accent, focus_k);
        let stroke_w = 1.0 + 0.85 * focus_k;
        let label_c = lerp_color(Color32::from_rgb(154, 154, 164), accent, focus_k);

        let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, height + 6.0), Sense::click());
        let box_rect = egui::Rect::from_min_max(rect.min + Vec2::new(0.0, 6.0), rect.max);
        let p = ui.painter();
        let fill = if resp.hovered() {
            Color32::from_white_alpha(12)
        } else {
            Color32::from_white_alpha(7)
        };
        p.rect_filled(box_rect, egui::CornerRadius::same(11), fill);
        p.rect_stroke(
            box_rect,
            egui::CornerRadius::same(11),
            egui::Stroke::new(stroke_w, stroke_c),
            egui::StrokeKind::Inside,
        );
        let galley = p.layout_no_wrap(label.to_owned(), FontId::proportional(11.5), label_c);
        let cut_rect = egui::Rect::from_min_size(
            egui::pos2(box_rect.left() + 12.0, box_rect.top() - 7.0),
            Vec2::new(galley.size().x + 10.0, 14.0),
        );
        p.rect_filled(cut_rect, egui::CornerRadius::same(3), bg_cut);
        p.galley(egui::pos2(cut_rect.left() + 5.0, cut_rect.top()), galley, label_c);

        let shown = if value.is_empty() { hint } else { value };
        let text_col = if value.is_empty() {
            Color32::from_rgb(122, 124, 136)
        } else {
            WHITE
        };
        let right_pad = match right {
            LoginButtonRight::None => 14.0,
            _ => 38.0,
        };
        let text_clip = egui::Rect::from_min_max(
            egui::pos2(box_rect.left() + 14.0, box_rect.top()),
            egui::pos2(box_rect.right() - right_pad, box_rect.bottom()),
        );
        p.with_clip_rect(text_clip).text(
            egui::pos2(text_clip.left(), box_rect.center().y),
            egui::Align2::LEFT_CENTER,
            shown,
            FontId::proportional(15.5),
            text_col,
        );
        let icon_center = egui::pos2(box_rect.right() - 20.0, box_rect.center().y);
        match right {
            LoginButtonRight::None => {}
            LoginButtonRight::Chevron(open) => icons::chevron(p, icon_center, open, SUBTEXT),
            LoginButtonRight::Keyboard => {
                icons::tg_sized(
                    p,
                    "chat/input_bot_keyboard",
                    icon_center,
                    Vec2::splat(18.0),
                    SUBTEXT,
                );
            }
        }
        resp.on_hover_cursor(egui::CursorIcon::PointingHand)
    }

    fn paint_android_keypad(ui: &mut egui::Ui, width: f32, target: &mut String, allow_plus: bool, max_digits: usize) -> bool {
        let mut changed = false;
        let rows: [&[&str]; 4] = [
            &["1", "2", "3"],
            &["4", "5", "6"],
            &["7", "8", "9"],
            &[if allow_plus { "+" } else { "Стереть" }, "0", "BACK"],
        ];
        let kw = ((width - 12.0) / 3.0).floor().max(52.0);
        let gap = ((width - kw * 3.0) * 0.5).max(4.0);
        let row_h = if width < 320.0 { 38.0 } else { 42.0 };
        let total_h = 4.0 * row_h + 3.0 * gap;
        ui.allocate_ui_with_layout(
            Vec2::new(width, total_h),
            egui::Layout::top_down(egui::Align::LEFT),
            |ui| {
                ui.spacing_mut().item_spacing.y = gap;
                for row in rows {
                    ui.allocate_ui_with_layout(
                        Vec2::new(width, row_h),
                        egui::Layout::left_to_right(egui::Align::Center),
                        |ui| {
                            ui.spacing_mut().item_spacing.x = gap;
                            for &key in row {
                                let is_glass = theme::mobile_glass_active();
                                let (rect, resp) = ui.allocate_exact_size(Vec2::new(kw, row_h), Sense::click());
                                let hk = theme::tween(ui.ctx(), ("akp", key, allow_plus), f32::from(u8::from(resp.hovered())), 0.12);
                                if is_glass {
                                    glass::paint_surface(ui.painter(), rect, 12.0, glass::GlassVariant::Clear, hk);
                                } else {
                                    ui.painter().rect_filled(
                                        rect,
                                        egui::CornerRadius::same(12),
                                        Color32::from_white_alpha(if resp.hovered() { 26 } else { 18 }),
                                    );
                                    ui.painter().rect_stroke(
                                        rect,
                                        egui::CornerRadius::same(12),
                                        egui::Stroke::new(1.0, Color32::from_white_alpha(26)),
                                        egui::StrokeKind::Inside,
                                    );
                                }
                                if key == "BACK" {
                                    // Векторная иконка Backspace (без системного Unicode-глифа ⌫).
                                    let p = ui.painter();
                                    let c = rect.center();
                                    let stroke = egui::Stroke::new(1.7, WHITE);
                                    let outline = vec![
                                        egui::Pos2::new(c.x - 10.0, c.y),
                                        egui::Pos2::new(c.x - 4.0, c.y - 6.5),
                                        egui::Pos2::new(c.x + 9.0, c.y - 6.5),
                                        egui::Pos2::new(c.x + 9.0, c.y + 6.5),
                                        egui::Pos2::new(c.x - 4.0, c.y + 6.5),
                                    ];
                                    p.add(egui::Shape::closed_line(outline, stroke));
                                    p.line_segment(
                                        [egui::Pos2::new(c.x - 0.5, c.y - 3.0), egui::Pos2::new(c.x + 5.5, c.y + 3.0)],
                                        stroke,
                                    );
                                    p.line_segment(
                                        [egui::Pos2::new(c.x - 0.5, c.y + 3.0), egui::Pos2::new(c.x + 5.5, c.y - 3.0)],
                                        stroke,
                                    );
                                } else {
                                    ui.painter().text(
                                        rect.center(),
                                        egui::Align2::CENTER_CENTER,
                                        key,
                                        FontId::proportional(if key.len() > 2 { 13.5 } else { 16.5 }),
                                        WHITE,
                                    );
                                }
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    match key {
                                        "BACK" => {
                                            target.pop();
                                            changed = true;
                                        }
                                        "Стереть" => {
                                            target.clear();
                                            changed = true;
                                        }
                                        "+" => {
                                            if !target.contains('+') {
                                                target.insert(0, '+');
                                                changed = true;
                                            }
                                        }
                                        d => {
                                            let cur_digits = target.chars().filter(char::is_ascii_digit).count();
                                            if cur_digits < max_digits {
                                                target.push_str(d);
                                                changed = true;
                                            }
                                        }
                                    }
                                }
                            }
                        },
                    );
                }
            },
        );
        changed
    }

    fn country_picker_window(&mut self, ctx: &egui::Context, accent: Color32) {
        if !self.login_country_open || self.stage != Stage::NeedPhone {
            return;
        }
        let mut open = true;
        let win_w = (ctx.content_rect().width() - 28.0).clamp(260.0, 360.0);
        let cur_country = countries::COUNTRIES
            .get(self.login_country_idx)
            .copied()
            .unwrap_or(countries::COUNTRIES[0]);
        egui::Window::new("Выбор страны")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .fixed_size(Vec2::new(win_w, 420.0))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let qw = ui.available_width();
                let qresp = ui.add_sized(
                    [qw, 36.0],
                    egui::TextEdit::singleline(&mut self.login_country_query)
                        .align(egui::Align2::LEFT_CENTER)
                        .hint_text("Поиск страны или кода (+7, +1)…")
                        .margin(Vec2::new(10.0, 6.0)),
                );
                if cfg!(target_os = "android") && !self.android_input_pending && (qresp.clicked() || qresp.gained_focus()) {
                    self.open_android_input(
                        "country_search",
                        "Поиск страны",
                        "Например: Россия или +7",
                        &self.login_country_query.clone(),
                        0,
                    );
                }
                ui.add_space(6.0);
                let matches = countries::filter_countries(&self.login_country_query);
                egui::ScrollArea::vertical()
                    .max_height(352.0)
                    .id_salt("country-picker-modal")
                    .show_rows(ui, 42.0, matches.len(), |ui, range| {
                        for row_i in range {
                            let idx = matches[row_i];
                            let c = countries::COUNTRIES[idx];
                            let sel = idx == self.login_country_idx;
                            let row_w = ui.available_width();
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(row_w, 40.0), Sense::click());
                            let bg = if sel {
                                accent.gamma_multiply(0.20)
                            } else if resp.hovered() {
                                Color32::from_white_alpha(14)
                            } else {
                                Color32::TRANSPARENT
                            };
                            ui.painter().rect_filled(r, egui::CornerRadius::same(8), bg);
                            ui.painter().text(
                                egui::pos2(r.left() + 10.0, r.center().y),
                                egui::Align2::LEFT_CENTER,
                                format!("{}  {}", c.iso, c.name),
                                FontId::proportional(14.5),
                                if sel { accent } else { WHITE },
                            );
                            ui.painter().text(
                                egui::pos2(r.right() - 10.0, r.center().y),
                                egui::Align2::RIGHT_CENTER,
                                c.dial,
                                FontId::proportional(14.0),
                                if sel { accent } else { SUBTEXT },
                            );
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                let old_dial = cur_country.dial;
                                self.login_country_idx = idx;
                                self.login_dial = c.dial.to_owned();
                                self.login_phone_full = countries::replace_dial_in_phone(
                                    &self.login_phone_full,
                                    old_dial,
                                    c.dial,
                                );
                                self.login_country_open = false;
                            }
                        }
                    });
            });
        if !open {
            self.login_country_open = false;
        }
    }

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        let accent = c_orange();
        let bg_cut = Color32::from_rgb(24, 25, 30);
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            if self.login_busy {
                ui.add(egui::Spinner::new().size(15.0).color(accent));
                ui.label(RichText::new("Связь с Telegram…").size(12.5).color(SUBTEXT));
            } else if !self.status.is_empty() {
                ui.label(RichText::new(&self.status).size(12.0).color(SUBTEXT));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                if ui
                    .add(
                        egui::Button::new(RichText::new("Прокси / Сеть").size(13.0).strong().color(accent))
                            .frame(false),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.proxy_open = !self.proxy_open;
                }
            });
        });
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                let compact = ui.available_width() < 520.0;
                let outer_pad = if compact { 12.0 } else { 24.0 };
                let margin_x = if compact { 14 } else { 20 };
                let margin_y = if compact { 14 } else { 20 };
                let top_gap = (ui.available_height() * if compact { 0.015 } else { 0.06 }).clamp(4.0, 52.0);
                ui.add_space(top_gap);
                let col_w = (ui.available_width() - outer_pad * 2.0 - f32::from(margin_x) * 2.0).clamp(260.0, 336.0);
                let glass_mobile = theme::mobile_glass_active();
                egui::Frame::new()
                    .fill(if glass_mobile {
                        Color32::from_rgba_unmultiplied(18, 20, 28, (226.0 * theme::opacity_f32().max(0.66)) as u8)
                    } else if compact {
                        Color32::from_rgba_unmultiplied(24, 25, 30, 250)
                    } else {
                        Color32::from_rgba_unmultiplied(24, 25, 30, (220.0 * opacity_f32().max(0.65)) as u8)
                    })
                    .stroke(egui::Stroke::new(
                        1.0,
                        Color32::from_white_alpha(if glass_mobile { 46 } else { 22 }),
                    ))
                    .corner_radius(egui::CornerRadius::same(if glass_mobile { 24 } else { 22 }))
                    .inner_margin(egui::Margin::symmetric(margin_x, margin_y))
                    .show(ui, |ui| {
                        ui.set_width(col_w);
                        if glass_mobile {
                            // Внутренний спекулярный верхний блик iOS 26 Liquid Glass без фоновых пятен.
                            let outer = ui.max_rect().expand2(Vec2::new(f32::from(margin_x), f32::from(margin_y)));
                            let sheen = egui::Rect::from_min_max(
                                egui::Pos2::new(outer.left() + 1.0, outer.top() + 1.0),
                                egui::Pos2::new(outer.right() - 1.0, (outer.top() + 20.0).min(outer.bottom())),
                            );
                            ui.painter().rect_filled(
                                sheen,
                                egui::CornerRadius { nw: 23, ne: 23, sw: 2, se: 2 },
                                Color32::from_white_alpha(12),
                            );
                            ui.painter().line_segment(
                                [egui::Pos2::new(outer.left() + 24.0, outer.top() + 0.8), egui::Pos2::new(outer.right() - 24.0, outer.top() + 0.8)],
                                egui::Stroke::new(1.0, Color32::from_white_alpha(84)),
                            );
                        }
                        ui.vertical_centered(|ui| match self.stage.clone() {
                            Stage::Qr(url) => self.qr_screen(ui, &url, accent),
                            _ => self.form_screen(ui, accent, bg_cut),
                        });
                    });
                if !self.error.is_empty() {
                    ui.add_space(12.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(220, 68, 68, 42))
                        .stroke(egui::Stroke::new(1.0, DANGER.gamma_multiply(0.65)))
                        .corner_radius(egui::CornerRadius::same(12))
                        .inner_margin(egui::Margin::symmetric(14, 10))
                        .show(ui, |ui| {
                            ui.set_max_width(col_w + 20.0);
                            ui.label(RichText::new(&self.error).size(12.5).color(Color32::from_rgb(255, 175, 175)));
                            ui.add_space(6.0);
                            ui.horizontal(|ui| {
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Повторить подключение").size(12.0).color(WHITE))
                                            .fill(t_btn(BUTTON))
                                            .corner_radius(egui::CornerRadius::same(8)),
                                    )
                                    .clicked()
                                {
                                    self.error.clear();
                                    self.send(Cmd::RetryConnect);
                                }
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Прокси").size(12.0).color(accent))
                                            .frame(false),
                                    )
                                    .clicked()
                                {
                                    self.proxy_open = true;
                                }
                                if ui
                                    .add(
                                        egui::Button::new(RichText::new("Копировать").size(12.0).color(SUBTEXT))
                                            .frame(false),
                                    )
                                    .clicked()
                                {
                                    copy_text_to_clipboard(ui.ctx(), &self.error);
                                    self.status = "Текст ошибки скопирован".to_owned();
                                }
                            });
                        });
                }
                if let Some(prev) = self
                    .settings
                    .accounts
                    .iter()
                    .find(|a| a.slot != self.settings.active_account && a.is_authorized_hint())
                    .cloned()
                {
                    ui.add_space(12.0);
                    if link(ui, &format!("← Вернуться к {}", prev.title())).clicked() {
                        let cur = self.settings.active_account;
                        if cur > 0 {
                            self.send(Cmd::RemoveAccountSlot(cur));
                        } else {
                            self.send(Cmd::SwitchAccount(prev.slot));
                        }
                    }
                }
                ui.add_space(14.0);
                ui.label(
                    RichText::new(format!("Rustgramm v{} · Zero Telemetry", env!("CARGO_PKG_VERSION")))
                        .color(SUBTEXT)
                        .size(11.5),
                );
                ui.add_space(16.0);
            });
        });
        self.country_picker_window(ui.ctx(), accent);
        self.proxy_window(ui.ctx());
    }

    fn qr_screen(&mut self, ui: &mut egui::Ui, url: &str, accent: Color32) {
        let qr_side = (ui.available_width() - 24.0).clamp(184.0, 248.0);
        let pop_k = theme::tween(ui.ctx(), "login-qr-pop", if url.is_empty() { 0.0 } else { 1.0 }, 0.28);
        if url.is_empty() {
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(qr_side), Sense::hover());
            ui.painter()
                .rect_filled(rect, egui::CornerRadius::same(20), Color32::from_white_alpha(16));
            ui.put(rect, egui::Spinner::new().size(28.0).color(accent));
        } else {
            let _ = theme::draw_styled_qr(ui, url, qr_side, accent, pop_k);
        }
        ui.add_space(16.0);
        ui.label(
            RichText::new("Вход по QR-коду")
                .size(21.0)
                .strong()
                .color(WHITE),
        );
        ui.add_space(12.0);
        for (num, text) in [
            ("1", "Откройте Telegram на телефоне"),
            ("2", "Настройки → Устройства → Подключить устройство"),
            ("3", "Наведите камеру на этот экран"),
        ] {
            ui.horizontal(|ui| {
                let (br, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
                ui.painter().circle_filled(br.center(), 9.5, accent);
                ui.painter().text(
                    br.center(),
                    egui::Align2::CENTER_CENTER,
                    num,
                    FontId::proportional(11.5),
                    Color32::WHITE,
                );
                ui.add_space(4.0);
                ui.label(RichText::new(text).size(13.0).color(WHITE));
            });
            ui.add_space(4.0);
        }
        if !url.is_empty() {
            ui.add_space(8.0);
            ui.horizontal(|ui| {
                if ui
                    .add(
                        egui::Button::new(RichText::new("Скопировать ссылку").size(12.5).color(SUBTEXT))
                            .frame(false),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    copy_text_to_clipboard(ui.ctx(), url);
                    self.status = "Ссылка QR-входа скопирована".to_owned();
                }
                ui.label(RichText::new("·").size(12.5).color(SUBTEXT));
                if ui
                    .add(
                        egui::Button::new(RichText::new("Обновить QR").size(12.5).color(accent))
                            .frame(false),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.error.clear();
                    self.send(Cmd::RefreshQr);
                }
            });
        }
        ui.add_space(14.0);
        if ui
            .add_sized(
                [ui.available_width().min(316.0), 42.0],
                egui::Button::new(RichText::new("ВОЙТИ ПО НОМЕРУ ТЕЛЕФОНА").size(13.0).strong().color(accent))
                    .fill(Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 22))
                    .stroke(egui::Stroke::new(1.0, accent.gamma_multiply(0.45)))
                    .corner_radius(egui::CornerRadius::same(11)),
            )
            .on_hover_cursor(egui::CursorIcon::PointingHand)
            .clicked()
        {
            self.error.clear();
            self.send(Cmd::UsePhone);
        }
    }

    fn form_screen(&mut self, ui: &mut egui::Ui, accent: Color32, bg_cut: Color32) {
        let form_w = ui.available_width().min(336.0);
        match self.stage.clone() {
            Stage::Starting => {
                let enter_k = theme::tween(ui.ctx(), "lg-logo-start", 1.0, 0.36);
                login_anim::paint_plane_mascot(ui, true, enter_k);
                ui.add_space(10.0);
                ui.label(RichText::new("Telegram").size(26.0).strong().color(WHITE));
                ui.add_space(6.0);
                ui.label(RichText::new("Подключение к серверам Telegram…").size(13.5).color(SUBTEXT));
                ui.add_space(12.0);
                if ui
                    .add(
                        egui::Button::new(RichText::new("Войти по номеру вручную").size(13.0).color(accent))
                            .frame(false),
                    )
                    .clicked()
                {
                    self.stage = Stage::NeedPhone;
                }
            }
            Stage::NeedKeys => {
                login_anim::paint_keys_mascot(ui);
                ui.add_space(10.0);
                ui.label(RichText::new("Ключи API Telegram").size(20.0).strong().color(WHITE));
                ui.label(
                    RichText::new("my.telegram.org → API development tools (хранятся только на устройстве)")
                        .size(12.5)
                        .color(SUBTEXT),
                );
                ui.add_space(8.0);
                let r_id = ui.add(
                    egui::TextEdit::singleline(&mut self.key_id)
                        .align(egui::Align2::LEFT_CENTER)
                        .hint_text("api_id")
                        .desired_width(form_w),
                );
                self.hook_android_text(&r_id, "key_id", "API ID", "Например: 2040", &self.key_id.clone(), 2);
                ui.add_space(6.0);
                let r_hash = ui.add(
                    egui::TextEdit::singleline(&mut self.key_hash)
                        .align(egui::Align2::LEFT_CENTER)
                        .password(true)
                        .hint_text("api_hash")
                        .desired_width(form_w),
                );
                self.hook_android_text(&r_hash, "key_hash", "API Hash", "32 символа", &self.key_hash.clone(), 0);
                ui.add_space(10.0);
                if orange_button(ui, "Сохранить и продолжить", Vec2::new(form_w, 44.0)).clicked() {
                    let hash = std::mem::take(&mut self.key_hash);
                    self.send(Cmd::Keys(self.key_id.clone(), hash));
                }
            }
            Stage::NeedPhone => {
                let enter_k = theme::tween(ui.ctx(), "lg-logo-phone", 1.0, 0.38);
                let tight = ui.available_width() < 360.0;
                login_anim::paint_plane_mascot(ui, self.login_busy, enter_k);
                ui.add_space(if tight { 4.0 } else { 6.0 });
                ui.label(RichText::new("Telegram").size(if tight { 24.0 } else { 28.0 }).strong().color(WHITE));
                ui.add_space(if tight { 4.0 } else { 6.0 });
                ui.label(
                    RichText::new("Проверьте код страны и введите свой номер\nтелефона.")
                        .size(if tight { 12.5 } else { 13.5 })
                        .color(Color32::from_rgb(162, 162, 172)),
                );
                ui.add_space(if tight { 12.0 } else { 18.0 });

                let country = countries::COUNTRIES
                    .get(self.login_country_idx)
                    .copied()
                    .unwrap_or(countries::COUNTRIES[0]);

                // Поле 1: «Страна» (единая кнопка по всей ширине с центрированным текстом и шевроном справа).
                let country_display = format!("{}  ({})", country.name, country.dial);
                if Self::outlined_login_button(
                    ui,
                    "country",
                    form_w,
                    48.0,
                    "Страна",
                    self.login_country_open,
                    accent,
                    bg_cut,
                    &country_display,
                    "Выберите страну",
                    LoginButtonRight::Chevron(self.login_country_open),
                )
                .clicked()
                {
                    self.login_country_open = !self.login_country_open;
                    self.login_country_query.clear();
                }

                ui.add_space(12.0);

                // Поле 2: «Номер телефона» (единый ввод `+7 ...` с вертикальным центрированием текста).
                let mut enter_pressed = false;
                if cfg!(target_os = "android") {
                    let phone_hint = format!("{} {}", country.dial, country.hint);
                    let active_phone = !self.login_phone_full.trim().is_empty() || self.android_input_pending;
                    if Self::outlined_login_button(
                        ui,
                        "phone",
                        form_w,
                        48.0,
                        "Номер телефона",
                        active_phone,
                        accent,
                        bg_cut,
                        &self.login_phone_full,
                        &phone_hint,
                        LoginButtonRight::Keyboard,
                    )
                    .clicked()
                    {
                        self.open_android_input(
                            "login_phone",
                            "Номер телефона",
                            "+7 999 123-45-67",
                            &self.login_phone_full.clone(),
                            1,
                        );
                    }
                    // Поддержка внешней аппаратной/IME-клавиатуры даже поверх кнопки поля на Android.
                    if !self.login_country_open && !self.proxy_open {
                        let (typed, bs, enter) = ui.input(|i| {
                            let mut s = String::new();
                            let mut bs = false;
                            for ev in &i.events {
                                match ev {
                                    egui::Event::Text(t) | egui::Event::Paste(t) => s.push_str(t),
                                    egui::Event::Key { key: egui::Key::Backspace, pressed: true, .. } => bs = true,
                                    _ => {}
                                }
                            }
                            let ent = i.key_pressed(egui::Key::Enter);
                            (s, bs, ent)
                        });
                        if bs && !self.login_phone_full.is_empty() {
                            self.login_phone_full.pop();
                        }
                        let mut changed_hw = bs;
                        for ch in typed.chars() {
                            if ch.is_ascii_digit() || (ch == '+' && self.login_phone_full.is_empty()) {
                                self.login_phone_full.push(ch);
                                changed_hw = true;
                            }
                        }
                        if changed_hw {
                            self.login_phone_full = countries::format_phone_input(&self.login_phone_full);
                            if let Some(idx) = countries::detect_country_from_full(&self.login_phone_full) {
                                self.login_country_idx = idx;
                                self.login_dial = countries::COUNTRIES[idx].dial.to_owned();
                            }
                        }
                        if enter {
                            enter_pressed = true;
                        }
                    }
                } else {
                    let phone_focus_id = ui.make_persistent_id("lg-phone-te");
                    let phone_focused = ui.memory(|m| m.has_focus(phone_focus_id)) || !self.login_phone_full.trim().is_empty();
                    let (_, (pr, kb_btn)) = Self::outlined_login_field(
                        ui,
                        "phone",
                        form_w,
                        48.0,
                        "Номер телефона",
                        phone_focused,
                        accent,
                        bg_cut,
                        |ui| {
                            let edit_w = (form_w - 58.0).max(160.0);
                            let pr = ui.add_sized(
                                [edit_w, 28.0],
                                egui::TextEdit::singleline(&mut self.login_phone_full)
                                    .id(phone_focus_id)
                                    .align(egui::Align2::LEFT_CENTER)
                                    .frame(egui::Frame::NONE)
                                    .font(FontId::proportional(15.5))
                                    .hint_text(format!("{} {}", country.dial, country.hint)),
                            );
                            let (kb_r, kb_resp) = ui.allocate_exact_size(Vec2::splat(24.0), Sense::click());
                            icons::tg_sized(
                                ui.painter(),
                                "chat/input_bot_keyboard",
                                kb_r.center(),
                                Vec2::splat(17.0),
                                if kb_resp.hovered() { WHITE } else { SUBTEXT },
                            );
                            let kb_btn = kb_resp
                                .on_hover_text("Ввести номер с клавиатуры")
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked();
                            (pr, kb_btn)
                        },
                    );
                    if kb_btn {
                        pr.request_focus();
                    }
                    if pr.changed() {
                        self.login_phone_full = countries::format_phone_input(&self.login_phone_full);
                        if let Some(idx) = countries::detect_country_from_full(&self.login_phone_full) {
                            self.login_country_idx = idx;
                            self.login_dial = countries::COUNTRIES[idx].dial.to_owned();
                        }
                    }
                    if (pr.lost_focus() || pr.has_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                        enter_pressed = true;
                    }
                }

                // Встроенная цифровая клавиатура на Android / узких экранах — идеально выровнена по ширине полей.
                if cfg!(target_os = "android") {
                    ui.add_space(10.0);
                    if Self::paint_android_keypad(ui, form_w, &mut self.login_phone_full, true, 15) {
                        self.login_phone_full = countries::format_phone_input(&self.login_phone_full);
                        if let Some(idx) = countries::detect_country_from_full(&self.login_phone_full) {
                            self.login_country_idx = idx;
                            self.login_dial = countries::COUNTRIES[idx].dial.to_owned();
                        }
                    }
                }

                ui.add_space(12.0);

                // Чекбокс «Оставаться в системе» (Keep me signed in — как на скрине).
                ui.allocate_ui_with_layout(
                    Vec2::new(form_w, 28.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        let (chk_r, chk_resp) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::click());
                        let chk_k = theme::tween(ui.ctx(), "lg-keep-chk", f32::from(u8::from(self.login_keep_signed_in)), 0.14);
                        let box_r = egui::Rect::from_center_size(chk_r.center(), Vec2::splat(18.0));
                        let box_bg = lerp_color(Color32::TRANSPARENT, accent, chk_k);
                        ui.painter().rect_filled(box_r, egui::CornerRadius::same(5), box_bg);
                        ui.painter().rect_stroke(
                            box_r,
                            egui::CornerRadius::same(5),
                            egui::Stroke::new(1.5, lerp_color(SUBTEXT, accent, chk_k)),
                            egui::StrokeKind::Inside,
                        );
                        if chk_k > 0.25 {
                            let tick_col = theme::on_accent().gamma_multiply(chk_k);
                            let c = box_r.center();
                            ui.painter().add(egui::Shape::line(
                                vec![
                                    c + Vec2::new(-4.5, 0.2),
                                    c + Vec2::new(-1.2, 3.6),
                                    c + Vec2::new(4.8, -3.5),
                                ],
                                egui::Stroke::new(2.0, tick_col),
                            ));
                        }
                        ui.add_space(8.0);
                        let lbl_resp = ui.add(
                            egui::Label::new(RichText::new("Оставаться в системе").size(14.0).color(WHITE))
                                .sense(Sense::click()),
                        );
                        if chk_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() || lbl_resp.clicked() {
                            self.login_keep_signed_in = !self.login_keep_signed_in;
                            self.send(Cmd::SetKeepSignedIn(self.login_keep_signed_in));
                        }
                    },
                );

                ui.add_space(14.0);

                let btn_lbl = if self.login_busy { "ОТПРАВКА КОДА…" } else { "ДАЛЕЕ" };
                let btn_fill = if self.login_busy {
                    accent.gamma_multiply(0.55)
                } else {
                    accent
                };
                let go_btn = ui.add_enabled(
                    !self.login_busy,
                    egui::Button::new(
                        RichText::new(btn_lbl)
                            .size(14.0)
                            .strong()
                            .color(if self.login_busy { SUBTEXT } else { theme::on_accent() }),
                    )
                    .fill(btn_fill)
                    .corner_radius(egui::CornerRadius::same(12))
                    .min_size(Vec2::new(form_w, 44.0)),
                );
                if (go_btn.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() || enter_pressed) && !self.login_busy {
                    match countries::normalize_phone(&self.login_dial, &self.login_phone_full) {
                        Ok(full_phone) => {
                            self.error.clear();
                            self.login_last_phone = full_phone.clone();
                            self.login_code_sent_at = Some(std::time::Instant::now());
                            self.login_busy = true;
                            self.send(Cmd::SetKeepSignedIn(self.login_keep_signed_in));
                            self.send(Cmd::RequestCode(full_phone));
                        }
                        Err(msg) => {
                            self.error = msg.to_owned();
                            self.error_at = self.now;
                        }
                    }
                }

                ui.add_space(16.0);
                if ui
                    .add(
                        egui::Button::new(RichText::new("ВХОД ПО QR-КОДУ").size(13.5).strong().color(accent))
                            .frame(false),
                    )
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.error.clear();
                    self.send(Cmd::UseQr);
                }
            }
            Stage::NeedCode => {
                login_anim::paint_code_monkey(ui, self.code.chars().count(), !self.error.is_empty());
                ui.add_space(12.0);
                ui.label(
                    RichText::new(if self.login_last_phone.is_empty() {
                        "Код подтверждения"
                    } else {
                        &self.login_last_phone
                    })
                    .size(20.0)
                    .strong()
                    .color(WHITE),
                );
                ui.add_space(4.0);
                let hint_txt = if self.login_code_hint.is_empty() {
                    "Мы отправили код в приложение Telegram на другом устройстве или по SMS"
                } else {
                    self.login_code_hint.as_str()
                };
                ui.label(RichText::new(hint_txt).size(12.5).color(SUBTEXT));
                ui.add_space(14.0);

                let mut enter_code = false;
                let code_target_len = usize::from(self.login_code_len.clamp(4, 8));
                ui.allocate_ui_with_layout(
                    Vec2::new(form_w, 60.0),
                    egui::Layout::top_down(egui::Align::Center),
                    |ui| {
                        if glass::otp_input(ui, "login-otp", &mut self.code, code_target_len, !self.error.is_empty()) {
                            enter_code = true;
                        }
                    },
                );
                if cfg!(target_os = "android") {
                    let code_ph = format!("{code_target_len} цифр кода");
                    let code_prompt_hint = format!("Введите {code_target_len} цифр кода");
                    ui.add_space(6.0);
                    if Self::outlined_login_button(
                        ui,
                        "code",
                        form_w,
                        44.0,
                        "Код из Telegram",
                        true,
                        accent,
                        bg_cut,
                        &self.code,
                        &code_ph,
                        LoginButtonRight::Keyboard,
                    )
                    .clicked()
                    {
                        self.open_android_input(
                            "login_code",
                            "Код подтверждения",
                            &code_prompt_hint,
                            &self.code.clone(),
                            2,
                        );
                    }
                }
                let mut auto_submit = false;
                if cfg!(target_os = "android") {
                    ui.add_space(8.0);
                    let max_code_chars = usize::from(self.login_code_len.clamp(4, 12));
                    if Self::paint_android_keypad(ui, form_w, &mut self.code, false, max_code_chars) {
                        let digits: String = self.code.chars().filter(char::is_ascii_digit).collect();
                        if digits.len() == max_code_chars {
                            auto_submit = true;
                        }
                    }
                }
                ui.add_space(14.0);
                let btn_lbl = if self.login_busy { "ПРОВЕРКА…" } else { "ВОЙТИ" };
                let go = ui
                    .add_enabled(
                        !self.login_busy,
                        egui::Button::new(
                            RichText::new(btn_lbl)
                                .size(14.0)
                                .strong()
                                .color(if self.login_busy { SUBTEXT } else { theme::on_accent() }),
                        )
                        .fill(if self.login_busy { accent.gamma_multiply(0.55) } else { accent })
                        .corner_radius(egui::CornerRadius::same(11))
                        .min_size(Vec2::new(form_w, 46.0)),
                    )
                    .clicked()
                    || auto_submit
                    || enter_code;
                if go && !self.code.trim().is_empty() && !self.login_busy {
                    self.error.clear();
                    self.login_busy = true;
                    let clean: String = self.code.chars().filter(char::is_ascii_digit).collect();
                    self.send(Cmd::Code(if clean.is_empty() { self.code.trim().to_owned() } else { clean }));
                }
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if link(ui, "← Изменить номер").clicked() {
                        self.error.clear();
                        self.code.clear();
                        self.send(Cmd::BackToPhone);
                    }
                    ui.add_space(16.0);
                    let wait_left = self
                        .login_code_sent_at
                        .map_or(0, |t| self.login_resend_secs.saturating_sub(t.elapsed().as_secs()));
                    if wait_left > 0 {
                        ui.label(RichText::new(format!("Повторно через {wait_left}с")).size(12.0).color(SUBTEXT));
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                    } else if !self.login_last_phone.is_empty() && link(ui, "Отправить ещё раз").clicked() {
                        self.error.clear();
                        self.login_code_sent_at = Some(std::time::Instant::now());
                        self.login_busy = true;
                        self.send(Cmd::ResendCode);
                    }
                });
            }
            Stage::NeedPassword(hint) => {
                login_anim::paint_password_monkey(
                    ui,
                    self.login_show_password,
                    self.password.chars().count(),
                );
                ui.add_space(12.0);
                ui.label(RichText::new("Введите облачный пароль").size(20.0).strong().color(WHITE));
                if !hint.is_empty() && hint != "нет" {
                    ui.label(RichText::new(format!("Подсказка: {hint}")).size(13.0).color(SUBTEXT));
                }
                ui.add_space(14.0);
                let mut enter_pw = false;
                if cfg!(target_os = "android") {
                    let masked: String = if self.login_show_password {
                        self.password.clone()
                    } else {
                        "•".repeat(self.password.chars().count())
                    };
                    if Self::outlined_login_button(
                        ui,
                        "password",
                        form_w,
                        48.0,
                        "Облачный пароль",
                        true,
                        accent,
                        bg_cut,
                        &masked,
                        "Пароль 2FA",
                        LoginButtonRight::Keyboard,
                    )
                    .clicked()
                    {
                        self.open_android_input(
                            "login_password",
                            "Облачный пароль",
                            "Введите пароль 2FA",
                            &self.password.clone(),
                            3,
                        );
                    }
                    ui.add_space(6.0);
                    if ui
                        .add(
                            egui::Button::new(
                                RichText::new(if self.login_show_password { "Скрыть пароль" } else { "Показать пароль" })
                                    .size(12.0)
                                    .color(accent),
                            )
                            .frame(false),
                        )
                        .clicked()
                    {
                        self.login_show_password = !self.login_show_password;
                    }
                } else {
                    Self::outlined_login_field(
                        ui,
                        "password",
                        form_w,
                        48.0,
                        "Облачный пароль",
                        true,
                        accent,
                        bg_cut,
                        |ui| {
                            let r = ui.add_sized(
                                [(form_w - 76.0).max(140.0), 28.0],
                                egui::TextEdit::singleline(&mut self.password)
                                    .align(egui::Align2::LEFT_CENTER)
                                    .password(!self.login_show_password)
                                    .frame(egui::Frame::NONE)
                                    .hint_text("Пароль 2FA"),
                            );
                            if r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                                enter_pw = true;
                            }
                            if ui
                                .add(
                                    egui::Button::new(
                                        RichText::new(if self.login_show_password { "Скр." } else { "Пок." })
                                            .size(11.5)
                                            .color(accent),
                                    )
                                    .frame(false),
                                )
                                .clicked()
                            {
                                self.login_show_password = !self.login_show_password;
                            }
                            r
                        },
                    );
                }
                ui.add_space(14.0);
                let btn_lbl = if self.login_busy { "ПРОВЕРКА…" } else { "ВОЙТИ" };
                let go = ui
                    .add_enabled(
                        !self.login_busy,
                        egui::Button::new(
                            RichText::new(btn_lbl)
                                .size(14.0)
                                .strong()
                                .color(if self.login_busy { SUBTEXT } else { theme::on_accent() }),
                        )
                        .fill(if self.login_busy { accent.gamma_multiply(0.55) } else { accent })
                        .corner_radius(egui::CornerRadius::same(11))
                        .min_size(Vec2::new(form_w, 46.0)),
                    )
                    .clicked()
                    || enter_pw;
                if go && !self.password.is_empty() && !self.login_busy {
                    self.error.clear();
                    self.login_busy = true;
                    let pw = std::mem::take(&mut self.password);
                    self.send(Cmd::Password(pw));
                }
                ui.add_space(14.0);
                if link(ui, "← Назад к номеру").clicked() {
                    self.error.clear();
                    self.password.clear();
                    self.send(Cmd::BackToPhone);
                }
            }
            Stage::Qr(_) | Stage::Ready => {}
        }
    }

    fn proxy_window(&mut self, ctx: &egui::Context) {
        if !self.proxy_open {
            return;
        }
        let mut open = true;
        egui::Window::new("Настройки соединения")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Прокси SOCKS5 (необязательно)");
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.proxy)
                        .hint_text("socks5://host:port")
                        .desired_width(280.0),
                );
                self.hook_android_text(&r, "proxy", "Прокси SOCKS5", "socks5://host:port", &self.proxy.clone(), 0);
                ui.weak("Без прокси соединение идёт напрямую с серверами Telegram.");
                if orange_button(ui, "Сохранить", Vec2::new(110.0, 32.0)).clicked() {
                    self.send(Cmd::Proxy(self.proxy.trim().to_owned()));
                    self.status = "Сохранение прокси и переподключение…".to_owned();
                }
            });
        self.proxy_open = open;
    }

    // ---------- главное окно ----------

    fn dialogs_ui(&mut self, ui: &mut egui::Ui) {
        // Верхняя строка: меню и поле поиска — фиксированная высота 40 px, иначе horizontal_centered
        // внутри вертикальной панели растягивает её на всю высоту окна!
        ui.add_space(6.0);
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), 40.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(38.0, 36.0), Sense::click());
            let m_hov = theme::tween(ui.ctx(), "burger-h", f32::from(u8::from(resp.hovered())), 0.12);
            if m_hov > 0.01 {
                ui.painter().circle_filled(
                    rect.center(),
                    17.0 * (0.9 + 0.1 * ios_spring_bounce(m_hov)),
                    t(WINDOW_OVER),
                );
            }
            icons::menu(ui.painter(), rect.center(), lerp_color(DIALOG_DATE, WHITE, m_hov));
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.drawer_open = true;
            }
            let has_lock = !self.settings.security.passcode_hash.is_empty();
            let w = (ui.available_width() - 10.0 - 38.0 - if has_lock { 38.0 } else { 0.0 }).max(80.0);
            let (srect, _) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::hover());
            ui.painter()
                .rect_filled(srect, egui::CornerRadius::same(18), t(FILTER_INPUT));
            // Точное вертикальное центрирование текста «Поиск» в пилюле 36 px.
            let text_h = 22.0;
            let inner = egui::Rect::from_min_max(
                egui::pos2(srect.left() + 14.0, srect.center().y - text_h * 0.5),
                egui::pos2(
                    srect.right() - if self.search.is_empty() { 14.0 } else { 32.0 },
                    srect.center().y + text_h * 0.5,
                ),
            );
            let d_sr = ui.put(
                inner,
                egui::TextEdit::singleline(&mut self.search)
                    .align(egui::Align2::LEFT_CENTER)
                    .hint_text("Поиск  (Ctrl+K)")
                    .frame(egui::Frame::NONE)
                    .desired_width(inner.width()),
            );
            self.hook_android_text(&d_sr, "dialogs_search", "Поиск", "Чаты, люди, сообщения", &self.search.clone(), 0);
            if !self.search.is_empty() {
                let xr = egui::Rect::from_center_size(egui::pos2(srect.right() - 16.0, srect.center().y), Vec2::splat(24.0));
                let xresp = ui.interact(xr, ui.make_persistent_id("dlg-search-clear"), Sense::click());
                icons::tg(ui.painter(), "info/info_close", xr.center(), if xresp.hovered() { WHITE } else { SUBTEXT });
                if xresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.search.clear();
                    self.global_results.clear();
                }
            }
            // #4 Notification Center Popover: кнопка колокольчика + бейдж.
            let feed_unread = self.notify_feed.iter().filter(|x| !x.4).count();
            let bell_cnt = if !self.notify_feed.is_empty() {
                feed_unread
            } else {
                self.dialogs.iter().filter(|d| d.unread > 0 && !d.muted).count()
            };
            let (br, bresp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
            let bh = theme::tween(ui.ctx(), "notify-bell-h", f32::from(u8::from(bresp.hovered() || self.notify_popover_open)), 0.12);
            if bh > 0.02 {
                glass::paint_surface(
                    ui.painter(),
                    br,
                    17.0,
                    if self.notify_popover_open { glass::GlassVariant::Tinted(c_orange()) } else { glass::GlassVariant::Clear },
                    bh,
                );
            }
            let bell_c = if self.notify_popover_open { Color32::WHITE } else { lerp_color(DIALOG_DATE, WHITE, bh) };
            icons::tg_sized(ui.painter(), "settings/notifications", br.center(), Vec2::splat(17.0), bell_c);
            if bell_cnt > 0 && !self.settings.ayu.hide_notification_counters {
                let btxt = if bell_cnt > 99 { "99+".to_owned() } else { bell_cnt.to_string() };
                let bw = (btxt.chars().count() as f32 * 5.6 + 7.0).max(15.0);
                let badge_r = egui::Rect::from_center_size(egui::pos2(br.right() - 6.0, br.top() + 7.0), Vec2::new(bw, 14.0));
                ui.painter().rect_filled(badge_r, egui::CornerRadius::same(7), c_orange());
                ui.painter().text(badge_r.center(), egui::Align2::CENTER_CENTER, btxt, FontId::proportional(9.5), theme::on_accent());
            }
            if bresp.on_hover_text("Центр уведомлений").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.notify_popover_open = !self.notify_popover_open;
            }
            if has_lock {
                let (lr, lresp) = ui.allocate_exact_size(Vec2::splat(34.0), Sense::click());
                if lresp.hovered() {
                    ui.painter().circle_filled(lr.center(), 16.0, t(WINDOW_OVER));
                }
                icons::tg(ui.painter(), "dialogs/dialogs_lock_off", lr.center(), if lresp.hovered() { ORANGE } else { DIALOG_DATE });
                if lresp.on_hover_text("Заблокировать Rustgramm").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.app_locked = true;
                }
            }
        });
        if self.notify_popover_open {
            if ui.input(|i| i.key_pressed(egui::Key::Escape)) {
                self.notify_popover_open = false;
            }
            let pop_k = glass::spring_anim(ui.ctx(), "nc-pop-spring", 1.0, glass::SpringSpec::SNAPPY);
            let mut open_did: Option<i64> = None;
            let mut dismiss_idx: Option<usize> = None;
            let mut read_all = false;
            let pop_w = 332.0_f32.min(ui.ctx().content_rect().width() - 24.0);
            let slide_y = (1.0 - pop_k) * -8.0;
            let anchor = egui::pos2(ui.max_rect().left() + 12.0, ui.min_rect().bottom() + 4.0 + slide_y);
            #[allow(deprecated)]
            egui::Area::new(egui::Id::new("notify-center-popover"))
                .order(egui::Order::Foreground)
                .fixed_pos(anchor)
                .show(ui.ctx(), |ui| {
                    egui::Frame::new()
                        .inner_margin(egui::Margin::same(10))
                        .show(ui, |ui| {
                            ui.set_width(pop_w);
                            glass::paint_modal_card(ui.painter(), ui.max_rect().expand(10.0), 16.0, pop_k.clamp(0.1, 1.0));
                            ui.horizontal(|ui| {
                                ui.label(RichText::new("Уведомления").strong().size(14.0).color(WHITE));
                                let unread_total = self.dialogs.iter().filter(|d| !d.muted).map(|d| i64::from(d.unread)).sum::<i64>();
                                if unread_total > 0 {
                                    ui.add_space(4.0);
                                    glass::animated_counter(ui, "nc-unread-cnt", unread_total, FontId::proportional(12.0), c_orange());
                                }
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    if glass::pill_button(ui, "nc-read-all", Vec2::new(112.0, 26.0), None, "Прочитать все", true).clicked() {
                                        read_all = true;
                                    }
                                });
                            });
                            ui.add_space(6.0);
                            let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
                            egui::ScrollArea::vertical().max_height(260.0).id_salt("nc-list").show(ui, |ui| {
                                if self.notify_feed.is_empty() {
                                    let unread_dlgs: Vec<(i64, String, String, i64)> = self
                                        .dialogs
                                        .iter()
                                        .filter(|d| d.unread > 0 && !d.muted)
                                        .take(12)
                                        .map(|d| (d.id, d.name.clone(), format!("{} непроч. — {}", d.unread, d.last), d.date))
                                        .collect();
                                    if unread_dlgs.is_empty() {
                                        ui.label(RichText::new("Нет новых уведомлений").size(12.5).color(SUBTEXT));
                                    } else {
                                        for (did, title, sub, ts) in unread_dlgs {
                                            let (rr, rresp) = ui.allocate_exact_size(Vec2::new(pop_w - 8.0, 44.0), Sense::click());
                                            let nh = theme::tween(ui.ctx(), ("nc-u", did), f32::from(u8::from(rresp.hovered())), 0.12);
                                            if nh > 0.02 {
                                                glass::paint_spotlight_card(ui.painter(), rr, 10.0, ptr, nh * 0.5);
                                            }
                                            draw_avatar(
                                                ui,
                                                egui::pos2(rr.left() + 20.0, rr.center().y),
                                                &title,
                                                did,
                                                28.0,
                                                self.avatars.get(&did),
                                                self.settings.extera.avatar_corners,
                                            );
                                            let tx = rr.left() + 40.0;
                                            if ts > 0 {
                                                ui.painter().text(
                                                    egui::pos2(rr.right() - 8.0, rr.center().y - 8.0),
                                                    egui::Align2::RIGHT_CENTER,
                                                    theme::hhmm(ts),
                                                    FontId::proportional(10.5),
                                                    SUBTEXT,
                                                );
                                            }
                                            let clip = egui::Rect::from_min_max(
                                                egui::pos2(tx, rr.top()),
                                                egui::pos2((rr.right() - 48.0).max(tx + 40.0), rr.bottom()),
                                            );
                                            let cp = ui.painter().with_clip_rect(clip);
                                            cp.text(egui::pos2(tx, rr.center().y - 8.0), egui::Align2::LEFT_CENTER, title, FontId::proportional(13.0), WHITE);
                                            cp.text(egui::pos2(tx, rr.center().y + 8.0), egui::Align2::LEFT_CENTER, sub, FontId::proportional(11.0), SUBTEXT);
                                            if rresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                                open_did = Some(did);
                                            }
                                        }
                                    }
                                } else {
                                    for (idx, (did, title, body, ts, is_read)) in self.notify_feed.iter().enumerate() {
                                        let (rr, rresp) = ui.allocate_exact_size(Vec2::new(pop_w - 8.0, 44.0), Sense::click());
                                        let nh = theme::tween(ui.ctx(), ("nc-f", idx), f32::from(u8::from(rresp.hovered())), 0.12);
                                        if nh > 0.02 {
                                            glass::paint_spotlight_card(ui.painter(), rr, 10.0, ptr, nh * 0.5);
                                        }
                                        draw_avatar(
                                            ui,
                                            egui::pos2(rr.left() + 20.0, rr.center().y),
                                            title,
                                            *did,
                                            28.0,
                                            self.avatars.get(did),
                                            self.settings.extera.avatar_corners,
                                        );
                                        if !*is_read {
                                            ui.painter().circle_filled(egui::pos2(rr.left() + 6.0, rr.center().y), 3.0, c_orange());
                                        }
                                        let close_r = egui::Rect::from_center_size(egui::pos2(rr.right() - 14.0, rr.center().y), Vec2::splat(18.0));
                                        let close_resp = ui.interact(close_r, ui.make_persistent_id(("nc-x", idx)), Sense::click());
                                        let close_hov = close_resp.hovered();
                                        ui.painter().text(
                                            close_r.center(),
                                            egui::Align2::CENTER_CENTER,
                                            "×",
                                            FontId::proportional(13.0),
                                            if close_hov { WHITE } else { SUBTEXT },
                                        );
                                        let close_clicked = close_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked();
                                        if close_clicked {
                                            dismiss_idx = Some(idx);
                                        }
                                        let ts_s = if *ts > 0 { theme::hhmm(*ts) } else { String::new() };
                                        if !ts_s.is_empty() {
                                            ui.painter().text(
                                                egui::pos2(rr.right() - 28.0, rr.center().y - 8.0),
                                                egui::Align2::RIGHT_CENTER,
                                                ts_s,
                                                FontId::proportional(10.5),
                                                SUBTEXT,
                                            );
                                        }
                                        let tx = rr.left() + 40.0;
                                        let clip = egui::Rect::from_min_max(
                                            egui::pos2(tx, rr.top()),
                                            egui::pos2((rr.right() - 56.0).max(tx + 40.0), rr.bottom()),
                                        );
                                        let cp = ui.painter().with_clip_rect(clip);
                                        cp.text(egui::pos2(tx, rr.center().y - 8.0), egui::Align2::LEFT_CENTER, title, FontId::proportional(13.0), WHITE);
                                        cp.text(egui::pos2(tx, rr.center().y + 8.0), egui::Align2::LEFT_CENTER, body, FontId::proportional(11.0), SUBTEXT);
                                        if rresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !close_clicked {
                                            open_did = Some(*did);
                                        }
                                    }
                                }
                            });
                        });
                });
            if let Some(rm_idx) = dismiss_idx
                && rm_idx < self.notify_feed.len()
            {
                self.notify_feed.remove(rm_idx);
            }
            if read_all {
                for item in &mut self.notify_feed {
                    item.4 = true;
                }
                for d in &mut self.dialogs {
                    d.unread = 0;
                }
                self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                self.send(Cmd::ForceReadAll);
                self.status = "Все уведомления прочитаны".to_owned();
                self.notify_popover_open = false;
            }
            if let Some(did) = open_did {
                for item in &mut self.notify_feed {
                    if item.0 == did {
                        item.4 = true;
                    }
                }
                self.notify_popover_open = false;
                if let Some(pos) = self.dialogs.iter().position(|d| d.id == did) {
                    self.open_dialog(pos);
                }
            }
        }
        ui.add_space(4.0);
        let needle = self.search.trim().to_lowercase();
        if !needle.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                if ui.selectable_label(!self.global_msg_mode, "Чаты и люди").clicked() {
                    self.global_msg_mode = false;
                }
                if ui.selectable_label(self.global_msg_mode, "Сообщения").clicked() {
                    self.global_msg_mode = true;
                    self.global_msg_last_q.clear();
                }
            });
            if self.global_msg_mode {
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    for (sc, lbl) in [(0u8, "Все"), (1, "Личные"), (2, "Группы"), (3, "Каналы")] {
                        if ui.selectable_label(self.global_msg_scope == sc, lbl).clicked() {
                            self.global_msg_scope = sc;
                            self.global_msg_last_q.clear();
                        }
                    }
                });
            }
        } else if self.global_msg_mode {
            self.global_msg_mode = false;
            self.global_msg_hits.clear();
            self.global_msg_last_q.clear();
        }
        // Дебаунс глобального поиска в Telegram (чаты/люди при >= 3 симв., сообщения при >= 2 симв.).
        if self.global_msg_mode && needle.chars().count() >= 2 {
            let key = format!("{}:{}", self.global_msg_scope, needle);
            if key != self.global_msg_last_q {
                self.global_msg_last_q = key;
                self.global_msg_at = Some(std::time::Instant::now());
            }
            if let Some(at) = self.global_msg_at {
                if at.elapsed().as_millis() >= 320 {
                    self.global_msg_at = None;
                    self.global_msg_loading = true;
                    self.send(Cmd::SearchGlobalMessages(needle.clone(), self.global_msg_scope));
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
                }
            }
            let mut jump_hit: Option<rg_core::engine::GlobalMessageHit> = None;
            if self.global_msg_loading && self.global_msg_hits.is_empty() {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.add(egui::Spinner::new().size(20.0).color(ORANGE));
                    ui.label(RichText::new("Поиск по всем чатам…").size(12.5).color(SUBTEXT));
                });
            } else if self.global_msg_hits.is_empty() {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| {
                    ui.label(RichText::new("Сообщений по запросу не найдено").size(12.5).color(SUBTEXT));
                });
            } else {
                egui::ScrollArea::vertical().id_salt("global-msg-hits").auto_shrink([false; 2]).show_rows(
                    ui,
                    DIALOG_ROW_HEIGHT,
                    self.global_msg_hits.len(),
                    |ui, range| {
                        ui.spacing_mut().item_spacing.y = 0.0;
                        for idx in range {
                            let h = &self.global_msg_hits[idx];
                            let (rect, resp) = ui.allocate_exact_size(
                                Vec2::new(ui.available_width(), DIALOG_ROW_HEIGHT),
                                Sense::click(),
                            );
                            if resp.hovered() {
                                ui.painter().rect_filled(rect, 0.0, t(DIALOG_OVER));
                            }
                            draw_avatar(
                                ui,
                                rect.left_center() + Vec2::new(32.0, 0.0),
                                &h.peer_name,
                                h.dialog_id,
                                DIALOG_PHOTO,
                                self.avatars.get(&h.dialog_id),
                                self.settings.ayu.avatar_corners,
                            );
                            let d_txt = dialog_date(h.date, self.now);
                            ui.painter().text(
                                egui::pos2(rect.right() - 12.0, rect.top() + 12.0),
                                egui::Align2::RIGHT_TOP,
                                d_txt,
                                FontId::proportional(12.0),
                                DIALOG_DATE,
                            );
                            ui.painter().text(
                                rect.left_top() + Vec2::new(68.0, 12.0),
                                egui::Align2::LEFT_TOP,
                                &h.peer_name,
                                FontId::proportional(14.5),
                                WHITE,
                            );
                            let prefix = if h.outgoing { "Вы: " } else { "" };
                            let line: String = format!("{prefix}{}", h.text.replace('\n', " "))
                                .chars()
                                .take(58)
                                .collect();
                            ui.painter().text(
                                rect.left_top() + Vec2::new(68.0, 35.0),
                                egui::Align2::LEFT_TOP,
                                line,
                                FontId::proportional(13.0),
                                DIALOG_TEXT,
                            );
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                jump_hit = Some(h.clone());
                            }
                        }
                    },
                );
            }
            if let Some(hit) = jump_hit {
                self.pending_jump_to_msg = Some((hit.dialog_id, hit.msg_id, hit.date));
                if let Some(pos) = self.dialogs.iter().position(|d| d.id == hit.dialog_id) {
                    if let Some(d) = self.dialogs.get(pos) {
                        self.in_archive = d.archived;
                    }
                    if self.selected == Some(pos) {
                        let now_t = ui.ctx().input(|i| i.time);
                        if self.messages.iter().any(|m| m.id == hit.msg_id) {
                            self.scroll_to = Some(hit.msg_id);
                            self.highlight_msg = Some((hit.msg_id, now_t));
                            self.pending_jump_to_msg = None;
                        } else if hit.date > 0 {
                            self.pending_jump_to_msg = Some((hit.dialog_id, hit.msg_id, 0));
                            self.send(Cmd::JumpToDate(hit.dialog_id, (hit.date + 2).min(i64::from(i32::MAX)) as i32));
                        }
                    } else {
                        self.open_dialog(pos);
                    }
                } else {
                    self.send(Cmd::OpenContact(hit.peer, hit.dialog_id, hit.peer_name));
                }
            }
            return;
        }
        if needle.chars().count() >= 3 {
            if needle != self.global_last_q {
                self.global_last_q.clone_from(&needle);
                self.global_query_at = Some(std::time::Instant::now());
            }
            if let Some(at) = self.global_query_at {
                if at.elapsed().as_millis() >= 350 {
                    self.global_query_at = None;
                    self.send(Cmd::GlobalSearch(needle.clone()));
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(120));
                }
            }
        } else if !self.global_last_q.is_empty() || !self.global_results.is_empty() {
            self.global_last_q.clear();
            self.global_results.clear();
            self.global_query_at = None;
        }
        let now = self.now;
        if needle.is_empty() {
            self.stories_strip(ui);
        }
        self.folder_tabs(ui);
        let mut open = None;
        let mut open_global: Option<(grammers_session::types::PeerRef, i64, String)> = None;
        let mut dact: Option<(usize, DialogAct)> = None;
        let mut toggle_archive = false;
        let in_archive = self.in_archive;
        let active_folder = self.folders.iter().find(|f| Some(f.id) == self.folder).cloned();
        let cache_key = (
            self.dialogs_gen,
            needle.clone(),
            in_archive,
            self.folder,
            self.unread_filter_only,
        );
        if self.filtered_cache_key != cache_key {
            self.filtered_cache_key = cache_key;
            self.archived_total_cache = self.dialogs.iter().filter(|d| d.archived).count();
            self.filtered_cache = self
                .dialogs
                .iter()
                .enumerate()
                .filter(|(_, d)| {
                    let in_scope = match &active_folder {
                        Some(f) if !in_archive => folder_matches(f, d),
                        _ => d.archived == in_archive,
                    };
                    in_scope
                        && (!self.unread_filter_only || d.unread > 0)
                        && (needle.is_empty() || d.name.to_lowercase().contains(&needle))
                })
                .map(|(i, _)| i)
                .collect();
        }
        let archived_total = self.archived_total_cache;
        let filtered = &self.filtered_cache;
        if in_archive {
            if archive_header(ui) {
                toggle_archive = true;
            }
        } else if (archived_total > 0 || !self.archive_loaded) && needle.is_empty() && active_folder.is_none() {
            let arch_key = (self.dialogs_gen, self.archive_loaded);
            if self.archive_summary_cache_key != arch_key {
                self.archive_summary_cache_key = arch_key;
                let names: Vec<&str> = self
                    .dialogs
                    .iter()
                    .filter(|d| d.archived)
                    .take(3)
                    .map(|d| d.name.as_str())
                    .collect();
                let sub = if !names.is_empty() {
                    names.join(", ")
                } else if self.archive_loaded {
                    "Архив пуст".to_owned()
                } else {
                    "Нажмите, чтобы загрузить архивные чаты".to_owned()
                };
                let unread: i32 = self
                    .dialogs
                    .iter()
                    .filter(|d| d.archived && !d.muted)
                    .map(|d| d.unread)
                    .sum();
                self.archive_summary_cache = (sub, unread);
            }
            if archive_row(ui, &self.archive_summary_cache.0, self.archive_summary_cache.1) {
                toggle_archive = true;
            }
        }
        let show_global = !needle.is_empty() && !self.global_results.is_empty();
        let sel_row_pos = self.selected.and_then(|sel| filtered.iter().position(|&idx| idx == sel));
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show_rows(
            ui,
            DIALOG_ROW_HEIGHT,
            filtered.len() + if show_global { self.global_results.len() + 1 } else { 0 },
            |ui, row_range| {
                ui.spacing_mut().item_spacing.y = 0.0;
                if let Some(target_row) = sel_row_pos {
                    let list_top_y = ui.cursor().min.y - (row_range.start as f32) * DIALOG_ROW_HEIGHT;
                    let list_left = ui.cursor().min.x;
                    let list_w = ui.available_width();
                    let slide_row = ui.ctx().animate_value_with_time(
                        egui::Id::NULL.with("dialogs-slide-row"),
                        target_row as f32,
                        0.22,
                    );
                    let ind_y = list_top_y + slide_row * DIALOG_ROW_HEIGHT;
                    if ind_y + DIALOG_ROW_HEIGHT >= ui.clip_rect().top() && ind_y <= ui.clip_rect().bottom() {
                        let pill_rect = egui::Rect::from_min_max(
                            egui::pos2(list_left + 4.0, ind_y + 2.0),
                            egui::pos2((list_left + list_w - 4.0).max(list_left + 20.0), ind_y + DIALOG_ROW_HEIGHT - 2.0),
                        );
                        let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
                        let rad = if theme::mobile_glass_active() { 12.0 } else { 9.0 };
                        glass::paint_surface_interactive(
                            ui.painter(),
                            pill_rect,
                            rad,
                            glass::GlassVariant::Tinted(t(DIALOG_ACTIVE)),
                            0.16,
                            ptr,
                        );
                        let accent_bar = egui::Rect::from_min_max(
                            egui::pos2(list_left + 1.0, ind_y + 9.0),
                            egui::pos2(list_left + 4.5, ind_y + DIALOG_ROW_HEIGHT - 9.0),
                        );
                        ui.painter().rect_filled(accent_bar, egui::CornerRadius::same(2), c_orange());
                    }
                }
                for row_pos in row_range {
                    if row_pos >= filtered.len() {
                        let g_idx = row_pos - filtered.len();
                        if g_idx == 0 {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), DIALOG_ROW_HEIGHT), Sense::hover());
                            let y_line = r.bottom() - 28.0;
                            ui.painter().line_segment(
                                [egui::pos2(r.left() + 14.0, y_line), egui::pos2(r.right() - 14.0, y_line)],
                                egui::Stroke::new(1.0, Color32::from_white_alpha((18.0 * opacity_f32()) as u8)),
                            );
                            ui.painter().text(
                                egui::pos2(r.left() + 16.0, r.bottom() - 14.0),
                                egui::Align2::LEFT_CENTER,
                                format!("ГЛОБАЛЬНЫЙ ПОИСК · {}", self.global_results.len()),
                                display(12.0),
                                ORANGE,
                            );
                        } else if let Some(c) = self.global_results.get(g_idx - 1) {
                            let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), DIALOG_ROW_HEIGHT), Sense::click());
                            let hover = theme::tween(ui.ctx(), ("glbl", c.id), f32::from(u8::from(resp.hovered())), 0.12);
                            ui.painter().rect_filled(rect, 0.0, theme::lerp_color(Color32::TRANSPARENT, t(DIALOG_OVER), hover));
                            draw_avatar(ui, rect.left_center() + Vec2::new(32.0, 0.0), &c.name, c.id, DIALOG_PHOTO, self.avatars.get(&c.id), self.settings.ayu.avatar_corners);
                            ui.painter().text(rect.left_top() + Vec2::new(68.0, 12.0), egui::Align2::LEFT_TOP, &c.name, FontId::proportional(15.0), WHITE);
                            ui.painter().text(rect.left_top() + Vec2::new(68.0, 35.0), egui::Align2::LEFT_TOP, &c.status, FontId::proportional(13.5), DIALOG_TEXT);
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open_global = Some((c.peer, c.id, c.name.clone()));
                            }
                        }
                        continue;
                    }
                    let i = filtered[row_pos];
                    let Some(d) = self.dialogs.get(i) else { continue };
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), DIALOG_ROW_HEIGHT),
                        Sense::click(),
                    );
                    let selected = self.selected == Some(i);
                    let hover = theme::tween(ui.ctx(), ("dlg", i), f32::from(u8::from(resp.hovered())), 0.12);
                    let p = ui.painter();
                    if hover > 0.01 && !selected {
                        let ptr = ui.ctx().input(|inp| inp.pointer.hover_pos());
                        // #4 Flowing Menu: стеклянный блеск по позиции курсора при наведении на чат.
                        glass::paint_surface_interactive(
                            p,
                            rect.shrink2(Vec2::new(4.0, 2.0)),
                            10.0,
                            glass::GlassVariant::Clear,
                            hover * 0.55,
                            ptr,
                        );
                    }
                    if self.avatar_req.insert(d.id) {
                        let _ = self.cmd.send(Cmd::Avatar(d.id));
                    }
                    draw_avatar(
                        ui,
                        rect.left_center() + Vec2::new(32.0, 0.0),
                        if d.saved { "И" } else { &d.name },
                        d.id,
                        DIALOG_PHOTO,
                        self.avatars.get(&d.id),
                        self.settings.ayu.avatar_corners,
                    );
                    let (name_c, text_c, date_c) = if selected {
                        (Color32::WHITE, Color32::WHITE, Color32::WHITE)
                    } else {
                        (WHITE, DIALOG_TEXT, DIALOG_DATE)
                    };
                    // Бейдж непрочитанных справа внизу, время справа вверху.
                    let mut right = rect.right() - 12.0;
                    if d.unread > 0 && !self.settings.ayu.hide_notification_counters {
                        let label = if d.unread > 999 {
                            "999+".to_owned()
                        } else {
                            d.unread.to_string()
                        };
                        let w = (label.chars().count() as f32 * 7.5 + 12.0).max(19.0);
                        let b = egui::Rect::from_min_size(
                            egui::pos2(right - w, rect.bottom() - 14.0 - 19.0),
                            Vec2::new(w, 19.0),
                        );
                        let bg = if selected {
                            Color32::WHITE
                        } else if d.muted {
                            t(UNREAD_MUTED)
                        } else {
                            t_btn(ORANGE)
                        };
                        p.rect_filled(b, egui::CornerRadius::same(10), bg);
                        let badge_fg = if selected {
                            t(DIALOG_ACTIVE)
                        } else if d.muted {
                            Color32::WHITE
                        } else {
                            theme::on_accent()
                        };
                        p.text(
                            b.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            FontId::proportional(12.0),
                            badge_fg,
                        );
                        right = b.left() - 6.0;
                    } else if d.pinned {
                        // Закреплённый чат без непрочитанных: значок кнопки вместо бейджа.
                        let c = egui::pos2(rect.right() - 22.0, rect.bottom() - 24.0);
                        icons::tg(p, "dialogs/dialogs_pinned", c, date_c);
                        right = c.x - 12.0;
                    }
                    if d.unread_mentions > 0 && !self.settings.ayu.hide_notification_counters {
                        let b = egui::Rect::from_center_size(
                            egui::pos2(right - 9.0, rect.bottom() - 23.5),
                            Vec2::splat(18.0),
                        );
                        p.circle_filled(b.center(), 9.0, if selected { Color32::WHITE } else { ORANGE });
                        p.text(
                            b.center(),
                            egui::Align2::CENTER_CENTER,
                            "@",
                            FontId::proportional(11.5),
                            if selected { t(DIALOG_ACTIVE) } else { theme::on_accent() },
                        );
                        right = b.left() - 5.0;
                    }
                    if d.unread_reactions > 0 && !self.settings.ayu.hide_notification_counters {
                        let b = egui::Rect::from_center_size(
                            egui::pos2(right - 9.0, rect.bottom() - 23.5),
                            Vec2::splat(18.0),
                        );
                        p.circle_filled(b.center(), 9.0, if selected { Color32::WHITE } else { Color32::from_rgb(232, 74, 95) });
                        p.text(
                            b.center(),
                            egui::Align2::CENTER_CENTER,
                            "❤",
                            FontId::proportional(10.5),
                            if selected { t(DIALOG_ACTIVE) } else { Color32::WHITE },
                        );
                        right = b.left() - 5.0;
                    }
                    let date = if d.date > 0 {
                        dialog_date(d.date, now)
                    } else {
                        String::new()
                    };
                    let date_galley = p.layout_no_wrap(date, FontId::proportional(13.0), date_c);
                    let date_w = date_galley.size().x;
                    p.galley(
                        egui::pos2(rect.right() - 12.0 - date_w, rect.top() + 12.0),
                        date_galley,
                        date_c,
                    );
                    let clip_right = (rect.right() - 12.0 - date_w - 8.0).max(rect.left() + 70.0);
                    let name_clip = egui::Rect::from_min_max(rect.min, egui::pos2(clip_right, rect.max.y));
                    let text_clip = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(right.max(rect.left() + 70.0), rect.max.y),
                    );
                    let title_str = if d.saved { "Избранное" } else { d.name.as_str() };
                    let title_galley = p.layout_no_wrap(title_str.to_owned(), FontId::proportional(15.0), name_c);
                    let title_w = title_galley.size().x;
                    p.with_clip_rect(name_clip).galley(
                        rect.left_top() + Vec2::new(68.0, 12.0),
                        title_galley,
                        name_c,
                    );
                    // Значки после имени: Premium-эмодзи статус / галочка / Premium / без звука (как в AyuGram!).
                    let mut badge_x = rect.left() + 68.0 + title_w + 11.0;
                    let badge_y = rect.top() + 21.0;
                    let show_prem = !self.settings.ayu.hide_premium_statuses;
                    if show_prem
                        && let Some(doc_id) = d.emoji_status
                        && badge_x < clip_right - 8.0
                    {
                        if let Some(tex) = self.sticker_tex.get(&doc_id) {
                            self.visible_tgs_docs.insert(doc_id);
                            let area = egui::Rect::from_center_size(egui::pos2(badge_x, badge_y), Vec2::splat(16.0));
                            egui::Image::new(tex).fit_to_exact_size(area.size()).paint_at(ui, area);
                        } else {
                            icons::tg(ui.painter(), "dialogs/dialogs_premium", egui::pos2(badge_x, badge_y), ORANGE);
                        }
                        badge_x += 18.0;
                    } else if d.verified && badge_x < clip_right - 8.0 {
                        icons::tg(ui.painter(), "dialogs/dialogs_verified_star", egui::pos2(badge_x, badge_y), theme::TG_BLUE);
                        badge_x += 18.0;
                    } else if show_prem && d.premium && badge_x < clip_right - 8.0 {
                        icons::tg(ui.painter(), "dialogs/dialogs_premium", egui::pos2(badge_x, badge_y), theme::TG_BLUE);
                        badge_x += 18.0;
                    }
                    if d.muted && badge_x < clip_right - 8.0 {
                        icons::tg(ui.painter(), "dialogs/dialogs_mute", egui::pos2(badge_x - 2.0, badge_y), date_c);
                    }
                    let live_typing = self
                        .typing
                        .get(&d.id)
                        .filter(|(_, at)| at.elapsed().as_secs() < 6)
                        .map(|(lbl, _)| lbl.as_str());
                    if let Some(lbl) = live_typing {
                        chat::paint_typing_wave(
                            &p.with_clip_rect(text_clip),
                            ui.ctx(),
                            rect.left_top() + Vec2::new(68.0, 42.0),
                            ORANGE,
                            lbl,
                        );
                    } else if let Some((draft_txt, _)) = self.drafts.get(&d.id).filter(|(t, _)| !t.trim().is_empty()) {
                        let prefix = "Черновик: ";
                        let pr_g = p.layout_no_wrap(
                            prefix.to_owned(),
                            FontId::proportional(14.0),
                            if selected { Color32::WHITE } else { Color32::from_rgb(235, 92, 86) },
                        );
                        let pr_w = pr_g.size().x;
                        let pos = rect.left_top() + Vec2::new(68.0, 35.0);
                        p.with_clip_rect(text_clip).galley(
                            pos,
                            pr_g,
                            if selected { Color32::WHITE } else { Color32::from_rgb(235, 92, 86) },
                        );
                        let one_line: String = draft_txt.chars().take(72).map(|c| if c == '\n' { ' ' } else { c }).collect();
                        p.with_clip_rect(text_clip).text(
                            pos + Vec2::new(pr_w, 0.0),
                            egui::Align2::LEFT_TOP,
                            one_line,
                            FontId::proportional(14.0),
                            text_c,
                        );
                    } else {
                        match self.emoji.as_ref().filter(|e| e.contains(&d.last)) {
                            Some(e) => e.paint_line(
                                ui,
                                text_clip,
                                rect.left_top() + Vec2::new(68.0, 35.0),
                                &d.last,
                                14.0,
                                text_c,
                            ),
                            None => {
                                p.with_clip_rect(text_clip).text(
                                    rect.left_top() + Vec2::new(68.0, 35.0),
                                    egui::Align2::LEFT_TOP,
                                    &d.last,
                                    FontId::proportional(14.0),
                                    text_c,
                                );
                            }
                        }
                    }
                    if resp.clicked() {
                        open = Some(i);
                    }
                    egui::Popup::context_menu(&resp)
                        .frame(kit::compact_popup_frame())
                        .show(|ui| {
                            ui.set_min_width(184.0);
                            ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                            let mut item = |ui: &mut egui::Ui, icon: &str, label: &str, a: DialogAct| {
                                if kit::menu_item(ui, icon, label, false) {
                                    dact = Some((i, a));
                                    ui.close();
                                }
                            };
                            if !d.saved {
                                item(
                                    ui,
                                    if d.archived { "menu/unarchive" } else { "menu/archive" },
                                    if d.archived { "Вернуть из архива" } else { "В архив" },
                                    DialogAct::Archive(!d.archived),
                                );
                            }
                            item(
                                ui,
                                if d.pinned { "menu/unpin" } else { "menu/pin" },
                                if d.pinned { "Открепить" } else { "Закрепить" },
                                DialogAct::Pin(!d.pinned),
                            );
                            if d.muted {
                                item(ui, "menu/unmute", "Включить звук", DialogAct::MuteFor(0));
                            } else {
                                item(ui, "menu/mute", "Отключить на 1 ч", DialogAct::MuteFor(3600));
                                item(ui, "menu/mute", "Отключить на 8 ч", DialogAct::MuteFor(8 * 3600));
                                item(ui, "menu/mute", "Отключить на 2 д", DialogAct::MuteFor(2 * 86_400));
                                item(ui, "menu/mute", "Отключить навсегда", DialogAct::MuteFor(i32::MAX));
                            }
                            if d.unread > 0 {
                                item(ui, "menu/read", "Прочитать локально", DialogAct::LocalRead);
                                item(ui, "menu/read", "Прочитать на сервере", DialogAct::Read);
                            } else {
                                item(ui, "menu/unread", "Отметить непрочитанным", DialogAct::Unread(true));
                            }
                            item(ui, "menu/info", "Подсмотреть (без прочтения)", DialogAct::Peek);
                            kit::menu_separator(ui);
                            item(ui, "menu/delete", "Очистить историю", DialogAct::ConfirmClear);
                            if !d.saved && matches!(d.kind, rg_core::engine::DialogKind::Group | rg_core::engine::DialogKind::Channel) {
                                item(ui, "menu/leave", "Покинуть чат / канал", DialogAct::ConfirmLeave);
                            }
                        });
                }
            },
        );
        if toggle_archive {
            self.in_archive = !self.in_archive;
            if self.in_archive && !self.archive_loaded {
                self.archive_loaded = true;
                self.send(Cmd::LoadArchive);
            }
        }
        if let Some((peer, id, name)) = open_global {
            self.search.clear();
            self.global_results.clear();
            self.send(Cmd::OpenContact(peer, id, name));
        }
        if let Some((i, a)) = dact {
            let did = self.dialog_id_at(i);
            match a {
                DialogAct::Pin(on) => {
                    if let Some(did) = did {
                        self.send(Cmd::PinDialog(did, on));
                    }
                }
                DialogAct::Mute(on) => self.toggle_mute_dialog(i, on),
                DialogAct::MuteFor(secs) => {
                    let muted = secs > 0;
                    if let Some(d) = self.dialogs.get_mut(i) {
                        d.muted = muted;
                        d.notify_explicit = Some(muted);
                        self.dialogs_gen = self.dialogs_gen.wrapping_add(1);
                    }
                    if let Some(did) = did {
                        self.send(Cmd::MuteDialogFor(did, secs));
                    }
                }
                DialogAct::LocalRead => self.local_read_dialog(i),
                DialogAct::Read => {
                    if let Some(d) = self.dialogs.get_mut(i) {
                        d.unread = 0;
                        self.dialogs_gen += 1;
                    }
                    if let Some(did) = did {
                        self.send(Cmd::ReadDialog(did));
                    }
                }
                DialogAct::Unread(on) => {
                    if let Some(d) = self.dialogs.get_mut(i) {
                        d.unread = if on { d.unread.max(1) } else { 0 };
                        self.dialogs_gen += 1;
                    }
                    if let Some(did) = did {
                        self.send(Cmd::MarkDialogUnread(did, on));
                    }
                }
                DialogAct::Archive(on) => {
                    if let Some(did) = did {
                        self.send(Cmd::Archive(did, on));
                    }
                }
                DialogAct::Peek => {
                    if let Some(did) = did {
                        self.send(Cmd::PeekHistory(did));
                    }
                }
                DialogAct::ConfirmClear => {
                    if let Some(d) = self.dialogs.get(i) {
                        self.confirm_chat_action = Some((d.id, d.name.clone(), false, false));
                    }
                }
                DialogAct::ConfirmLeave => {
                    if let Some(d) = self.dialogs.get(i) {
                        self.confirm_chat_action = Some((d.id, d.name.clone(), true, false));
                    }
                }
            }
        }
        if let Some(i) = open {
            self.open_dialog(i);
        }
    }
}

/// Неблокирующее системное уведомление Windows (без консольного окна).
fn notify_toast(title: &str, body: &str) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        let esc = |s: &str| {
            s.chars()
                .take(120)
                .map(|c| match c {
                    '\'' | '"' | '<' | '>' | '&' | '\r' | '\n' => ' ',
                    other => other,
                })
                .collect::<String>()
        };
        let t = esc(title);
        let b = esc(body);
        let script = format!(
            "[Windows.UI.Notifications.ToastNotificationManager, Windows.UI.Notifications, ContentType = WindowsRuntime] | Out-Null; \
            $xml = [Windows.UI.Notifications.ToastNotificationManager]::GetTemplateContent([Windows.UI.Notifications.ToastTemplateType]::ToastText02); \
            $nodes = $xml.GetElementsByTagName('text'); $nodes[0].InnerText = '{t}'; $nodes[1].InnerText = '{b}'; \
            $toast = [Windows.UI.Notifications.ToastNotification]::new($xml); \
            [Windows.UI.Notifications.ToastNotificationManager]::CreateToastNotifier('Rustgramm').Show($toast)"
        );
        let _ = std::process::Command::new("powershell")
            .args(["-NoProfile", "-WindowStyle", "Hidden", "-Command", &script])
            .creation_flags(0x0800_0000)
            .spawn();
    }
    #[cfg(target_os = "android")]
    {
        let mut h = 2_166_136_261u32;
        for &b in title.as_bytes() {
            h = (h ^ u32::from(b)).wrapping_mul(16_777_619);
        }
        let nid = 1000 + (h % 512) as i32;
        crate::android_jni::show_notification(nid, title, body);
    }
    #[cfg(all(not(windows), not(target_os = "android")))]
    let _ = (title, body);
}

/// Действие из контекстного меню строки списка чатов.
enum DialogAct {
    Pin(bool),
    #[allow(dead_code)]
    Mute(bool),
    MuteFor(i32),
    LocalRead,
    Read,
    Unread(bool),
    Archive(bool),
    Peek,
    ConfirmClear,
    ConfirmLeave,
}

/// Значок папки «Архив»: серый круг и значок «ящик со стрелкой» из набора Telegram.
fn paint_archive_icon(p: &egui::Painter, c: egui::Pos2, size: f32) {
    p.circle_filled(c, size / 2.0, Color32::from_rgb(70, 70, 78));
    icons::tg(p, "archive_userpic", c, Color32::WHITE);
}

/// Строка «Архив» в начале списка; `true` — нажата. Архив грузится лениво по клику.
fn archive_row(ui: &mut egui::Ui, sub: &str, unread: i32) -> bool {
    let (rect, resp) = ui.allocate_exact_size(
        Vec2::new(ui.available_width(), DIALOG_ROW_HEIGHT),
        Sense::click(),
    );
    let hover = theme::tween(ui.ctx(), "archive-row", f32::from(u8::from(resp.hovered())), 0.12);
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::lerp_color(Color32::TRANSPARENT, t(DIALOG_OVER), hover));
    paint_archive_icon(p, rect.left_center() + Vec2::new(32.0, 0.0), DIALOG_PHOTO);
    p.text(
        rect.left_top() + Vec2::new(68.0, 12.0),
        egui::Align2::LEFT_TOP,
        "Архив",
        FontId::proportional(15.0),
        WHITE,
    );
    p.with_clip_rect(egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.right() - 60.0, rect.max.y),
    ))
    .text(
        rect.left_top() + Vec2::new(68.0, 35.0),
        egui::Align2::LEFT_TOP,
        sub,
        FontId::proportional(13.5),
        DIALOG_TEXT,
    );
    if unread > 0 {
        let label = if unread > 999 { "999+".to_owned() } else { unread.to_string() };
        let w = (label.chars().count() as f32 * 7.5 + 12.0).max(19.0);
        let b = egui::Rect::from_min_size(
            egui::pos2(rect.right() - 12.0 - w, rect.bottom() - 33.0),
            Vec2::new(w, 19.0),
        );
        p.rect_filled(b, egui::CornerRadius::same(10), t(UNREAD_MUTED));
        p.text(b.center(), egui::Align2::CENTER_CENTER, label, FontId::proportional(12.0), WHITE);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

/// Шапка просмотра архива: стрелка «назад» и заголовок; `true` — нажата.
fn archive_header(ui: &mut egui::Ui) -> bool {
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 44.0), Sense::click());
    let hover = theme::tween(ui.ctx(), "archive-head", f32::from(u8::from(resp.hovered())), 0.12);
    let p = ui.painter();
    p.rect_filled(rect, 0.0, theme::lerp_color(Color32::TRANSPARENT, t(DIALOG_OVER), hover));
    icons::back(p, rect.left_center() + Vec2::new(26.0, 0.0), WHITE);
    p.text(
        rect.left_center() + Vec2::new(54.0, 0.0),
        egui::Align2::LEFT_CENTER,
        "Архив",
        FontId::proportional(16.0),
        WHITE,
    );
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

impl eframe::App for App {
    fn clear_color(&self, _visuals: &egui::Visuals) -> [f32; 4] {
        #[cfg(target_os = "android")]
        {
            [0.055, 0.055, 0.063, 1.0]
        }
        #[cfg(not(target_os = "android"))]
        {
            [0.0, 0.0, 0.0, 0.0]
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, frame: &mut eframe::Frame) {
        self.flush_refresh(ui.ctx());
        let ctx = ui.ctx().clone();
        if ctx.input(|i| i.viewport().close_requested())
            && !self.quitting
            && !self.is_demo
            && self.settings.work_mode != rg_settings::WorkMode::WindowOnly
        {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
            self.hide_window_to_tray();
        }
        theme::set_opacity(self.settings.window_opacity);
        theme::set_blur(self.settings.blur_enabled || self.settings.extera.force_blur);
        theme::apply_mono_font(&ctx, &self.settings.ayu.mono_font);
        theme::sync_visuals(&ctx);
        kit::sync_prefs(
            self.settings.ayu.material_switches,
            self.settings.extera.disable_dividers,
        );
        win::set_window_blur((self.settings.blur_enabled || self.settings.extera.force_blur) && self.settings.window_opacity < 100);
        if self.settings.extera.pause_on_minimize && ctx.input(|i| i.viewport().minimized == Some(true)) {
            #[cfg(windows)]
            {
                if let Some(v) = self.audio_view.as_ref()
                    && v.is_playing()
                {
                    v.set_playing(false);
                }
                if let Some(v) = self.player.take() {
                    v.stop();
                    self.player_path = None;
                    self.player_stream = None;
                }
            }
        }
        self.sync_tray_and_window(&ctx);
        theme::set_anim_speed(self.settings.anim_speed);
        let dropped: Vec<std::path::PathBuf> = ctx.input(|i| {
            i.raw
                .dropped_files
                .iter()
                .map(|f| f.path().to_path_buf())
                .filter(|p| !p.as_os_str().is_empty())
                .collect()
        });
        if !dropped.is_empty() && self.selected.is_some() {
            self.drop_paths = dropped;
        }
        let scale_pct = if self.settings.default_scale {
            100
        } else {
            self.settings.ui_scale.clamp(75, 200)
        };
        let zoom = f32::from(scale_pct) / 100.0;
        if (ctx.zoom_factor() - zoom).abs() > 0.001 {
            ctx.set_zoom_factor(zoom);
        }
        // Единая фоновая подложка на фоновом слое (без двоения краёв и швов при прозрачности).
        let bg_rect = ctx.content_rect().expand(2.0);
        if theme::mobile_glass_active() {
            theme::paint_android_glass_backdrop(&ctx, bg_rect, theme::c_orange());
        } else {
            ctx.layer_painter(egui::LayerId::background()).rect_filled(
                bg_rect,
                egui::CornerRadius::ZERO,
                t(BACKDROP),
            );
        }
        if !self.emoji_tried {
            self.emoji_tried = true;
            self.emoji = emoji::Emoji::load(&ctx);
        }
        self.drain_events(&ctx);
        self.poll_android_bridge(&ctx);
        self.flush_pending_notification(&ctx);
        self.tick_tgs(&ctx);
        self.visible_tgs_docs.clear();
        self.handle_shortcuts(&ctx);
        let ui_t = ctx.input(|i| i.time);
        let focused = ctx.input(|i| i.focused);
        if self.was_focused && !focused {
            self.last_focus_lost_at = Some(ui_t);
            self.flush_draft_if_dirty(true, ui_t);
        } else if !self.was_focused
            && focused
            && let Some(lost) = self.last_focus_lost_at.take()
            && ui_t - lost > 45.0
            && self.stage == Stage::Ready
            && !self.is_demo
        {
            self.send(Cmd::SyncOpenChat(self.selected_dialog_id()));
        }
        self.was_focused = focused;
        self.flush_draft_if_dirty(false, ui_t);
        self.now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(self.now, |d| d.as_secs() as i64);
        if !self.error.is_empty() && self.stage == Stage::Ready {
            if self.now - self.error_at > 6 {
                self.error.clear();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        if self.status != self.status_last {
            self.status_last = self.status.clone();
            self.status_at = self.now;
        }
        if !self.status.is_empty() && self.stage == Stage::Ready {
            if self.now - self.status_at > 4 {
                self.status.clear();
                self.status_last.clear();
            } else {
                ctx.request_repaint_after(std::time::Duration::from_secs(1));
            }
        }
        if self.selected.is_some() && self.chat_opened_at < 0.0 {
            self.chat_opened_at = ui_t;
        }
        let toast_is_err = !self.error.is_empty();
        let toast_txt = if toast_is_err { self.error.clone() } else { self.status.clone() };
        let toast_k = theme::tween(
            &ctx,
            "rg-status-toast",
            f32::from(u8::from(self.stage == Stage::Ready && !toast_txt.is_empty())),
            0.16,
        );
        #[cfg(not(target_os = "android"))]
        {
            egui::Panel::top("titlebar")
                .exact_size(34.0)
                .frame(egui::Frame::new().fill(tint_title()))
                .show(ui, |ui| self.titlebar(ui));
            self.resize_edges(ui);
        }
        if theme::mobile_glass_active() {
            // Безопасные отступы под статус-бар / вырез камеры / IME в стеклянном стиле.
            let ins = android_jni::safe_insets_dp(ctx.pixels_per_point());
            let top_h = if ins[0] > 1.0 {
                ins[0].clamp(20.0, 72.0)
            } else if cfg!(target_os = "android") {
                40.0
            } else {
                26.0
            };
            let bot_h = if ins[2] > 1.0 {
                ins[2].clamp(12.0, 420.0)
            } else if cfg!(target_os = "android") {
                16.0
            } else {
                14.0
            };
            let top_resp = egui::Panel::top("android-safe-top")
                .exact_size(top_h)
                .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                .show(ui, |_ui| {});
            theme::paint_glass_bar(ui.painter(), top_resp.response.rect, 0.0, 152);
            let bot_resp = egui::Panel::bottom("android-safe-bottom")
                .exact_size(bot_h)
                .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                .show(ui, |_ui| {});
            theme::paint_glass_bar(ui.painter(), bot_resp.response.rect, 0.0, 148);
        }
        if self.update_available.is_some() && !self.update_dismissed {
            egui::Panel::top("update-banner")
                .exact_size(42.0)
                .frame(egui::Frame::new().fill(Color32::TRANSPARENT))
                .show(ui, |ui| self.update_banner_ui(ui));
        }
        if self.app_locked && !self.settings.security.passcode_hash.is_empty() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.lock_screen_ui(ui));
            return;
        }
        if self.stage != Stage::Ready {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.login_ui(ui));
            return;
        }
        // Узкое окно (телефон): одна колонка — либо список чатов с плавающей Liquid Glass панелью вкладок, либо открытый чат.
        let narrow = ui.available_width() < NARROW_WIDTH;
        if narrow && self.selected.is_none() {
            let main_unread: i32 = self.dialogs.iter().filter(|d| !d.archived).map(|d| d.unread).sum();
            let arch_unread: i32 = self.dialogs.iter().filter(|d| d.archived).map(|d| d.unread).sum();
            let active_tab = if self.tgs.open {
                3
            } else if self.contacts_open {
                2
            } else if self.in_archive {
                1
            } else {
                0
            };
            if let Some(tab) = egui::Panel::bottom("mobile-glass-tabs")
                .frame(egui::Frame::NONE.inner_margin(egui::Margin::ZERO))
                .show(ui, |ui| {
                    glass::floating_tab_bar(
                        ui,
                        "mob-nav",
                        &[
                            ("menu/chats", "Чаты", main_unread),
                            ("menu/archive", "Архив", arch_unread),
                            ("folders/folders_type_contacts", "Контакты", 0),
                            ("menu/settings", "Настройки", 0),
                        ],
                        active_tab,
                    )
                })
                .inner
            {
                match tab {
                    0 => {
                        self.in_archive = false;
                        self.contacts_open = false;
                        self.tgs.open = false;
                    }
                    1 => {
                        self.in_archive = true;
                        self.contacts_open = false;
                        self.tgs.open = false;
                        if !self.archive_loaded {
                            self.archive_loaded = true;
                            self.send(Cmd::LoadArchive);
                        }
                    }
                    2 => {
                        self.contacts_open = true;
                        self.tgs.open = false;
                        self.contact_search.clear();
                        self.send(Cmd::Contacts);
                    }
                    _ => {
                        self.tgs.open = true;
                        self.contacts_open = false;
                    }
                }
            }
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.dialogs_ui(ui));
            glass::paint_toast(&ctx, &toast_txt, toast_is_err, toast_k);
            self.render_modals(&ctx, frame);
            return;
        }
        if !narrow {
            egui::Panel::left("dialogs")
                .default_size(COLUMN_LEFT_NORMAL)
                .size_range(COLUMN_LEFT_MIN..=COLUMN_LEFT_MAX)
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.dialogs_ui(ui));
        }
        // Панель эмодзи/стикеров/GIF в широком окне стоит справа, как в AyuGram (жёсткий clip,
        // без min_width(300) при сворачивании — устраняет «двоение» и вылезание текста за правый край!).
        let emoji_k = theme::tween(&ctx, "emoji-side", f32::from(u8::from(self.emoji_open && self.selected.is_some() && !narrow)), 0.18);
        if emoji_k > 0.02 && self.selected.is_some() {
            let panel_w = (320.0 * ios_spring(emoji_k)).max(2.0);
            egui::Panel::right("emoji-side")
                .exact_size(panel_w)
                .frame(egui::Frame::new().fill(tint_panel()).inner_margin(egui::Margin::symmetric(4, 0)))
                .show(ui, |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    if panel_w >= 120.0 {
                        self.emoji_panel_body(ui);
                    }
                });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !self.emoji_pinned {
                self.emoji_open = false;
            }
        }
        let info_k = theme::tween(&ctx, "info-panel", f32::from(u8::from(self.info_open && !self.emoji_open && self.selected.is_some() && !narrow)), 0.18);
        if info_k > 0.02 && self.selected.is_some() && emoji_k <= 0.02 {
            let panel_w = (320.0 * ios_spring(info_k)).max(2.0);
            egui::Panel::right("info")
                .exact_size(panel_w)
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    if panel_w >= 120.0 {
                        self.info_ui(ui);
                    }
                });
        }
        let glass_mobile = theme::mobile_glass_active();
        let header_frame = if glass_mobile {
            egui::Frame::NONE
        } else {
            egui::Frame::new().fill(tint_panel())
        };
        egui::Panel::top("header")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(header_frame)
            .show(ui, |ui| {
                if glass_mobile {
                    glass::paint_surface(ui.painter(), ui.max_rect(), 0.0, glass::GlassVariant::Regular, 0.0);
                }
                self.header_ui(ui, narrow);
            });
        let has_pinned_bar = self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| {
            self.pinned_lists.get(&d.id).is_some_and(|l| !l.is_empty())
                || self.pinned_msgs.contains_key(&d.id)
        });
        let pin_k = theme::tween(&ctx, "pinned-bar-anim", f32::from(u8::from(has_pinned_bar)), 0.18);
        if pin_k > 0.02 {
            let pin_h = (46.0 * ios_spring(pin_k)).max(2.0);
            let pin_frame = if glass_mobile {
                egui::Frame::NONE.inner_margin(egui::Margin::ZERO)
            } else {
                egui::Frame::new().fill(tint_panel()).inner_margin(egui::Margin::ZERO)
            };
            egui::Panel::top("pinned-bar")
                .exact_size(pin_h)
                .frame(pin_frame)
                .show(ui, |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.set_opacity(pin_k);
                    if glass_mobile {
                        glass::paint_surface(ui.painter(), ui.max_rect(), 0.0, glass::GlassVariant::Regular, 0.0);
                    }
                    if pin_h >= 24.0 {
                        self.pinned_bar_ui(ui);
                    }
                });
        }
        if self.audio_path.is_some() || self.audio_stream.is_some() {
            egui::Panel::top("audio-bar")
                .exact_size(40.0)
                .frame(egui::Frame::new().fill(t(WINDOW_BG)))
                .show(ui, |ui| self.audio_bar(ui));
        }
        if self.settings.language.translate_entire_chats && self.selected.is_some() {
            egui::Panel::top("translate-bar")
                .exact_size(34.0)
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.translate_chat_bar(ui));
        }
        let search_k = theme::tween(&ctx, "chat-search-anim", f32::from(u8::from(self.search_open && self.selected.is_some())), 0.18);
        if search_k > 0.02 && self.selected.is_some() {
            let sh = (68.0 * ios_spring(search_k)).max(2.0);
            egui::Panel::top("chat-search")
                .exact_size(sh)
                .frame(if glass_mobile { egui::Frame::NONE } else { egui::Frame::new().fill(tint_panel()) })
                .show(ui, |ui| {
                    ui.set_clip_rect(ui.max_rect());
                    ui.set_opacity(search_k);
                    if glass_mobile {
                        glass::paint_surface(ui.painter(), ui.max_rect(), 0.0, glass::GlassVariant::Regular, 0.0);
                    }
                    if sh >= 32.0 {
                        self.search_bar(ui);
                    }
                });
        }
        if self.selected.is_some() {
            let comp_frame = if glass_mobile {
                egui::Frame::NONE.inner_margin(egui::Margin::symmetric(8, 4))
            } else {
                egui::Frame::new()
                    .fill(tint_panel())
                    .inner_margin(egui::Margin::symmetric(8, 4))
            };
            egui::Panel::bottom("composer")
                .frame(comp_frame)
                .show(ui, |ui| {
                    if glass_mobile {
                        let outer = ui.max_rect().expand2(Vec2::new(8.0, 4.0));
                        glass::paint_surface(ui.painter(), outer, 16.0, glass::GlassVariant::Regular, 0.0);
                    }
                    self.composer_ui(ui);
                });
        }
        // Стабильные отступы ленты при входе в чат (без покадрового смещения inner_margin.left,
        // которое переносило строки и вызывало дёргание сообщений при открытии чата!).
        let bg_style = if self.settings.ayu.disable_custom_backgrounds {
            0
        } else {
            self.settings.chat_bg_style
        };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .inner_margin(egui::Margin {
                        left: 8,
                        right: 8,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| {
                let wp_path = self.settings.theme.wallpaper_path.clone();
                if wp_path.is_empty() {
                    self.wallpaper_tex = None;
                    self.wallpaper_failed_path.clear();
                } else if self.wallpaper_tex.as_ref().is_none_or(|(p, _)| p != &wp_path)
                    && self.wallpaper_failed_path != wp_path
                {
                    if let Ok(bytes) = std::fs::read(&wp_path)
                        && let Ok(img) = image::load_from_memory(&bytes)
                    {
                        let rgba = img.thumbnail(1920, 1920).to_rgba8();
                        let ci = egui::ColorImage::from_rgba_unmultiplied(
                            [rgba.width() as usize, rgba.height() as usize],
                            rgba.as_raw(),
                        );
                        let tex = ctx.load_texture("rg-wallpaper", ci, egui::TextureOptions::LINEAR);
                        self.wallpaper_tex = Some((wp_path, tex));
                        self.wallpaper_failed_path.clear();
                    } else {
                        self.wallpaper_tex = None;
                        self.wallpaper_failed_path = wp_path;
                    }
                }
                let wp_tex = self.wallpaper_tex.as_ref().map(|(_, t)| t);
                paint_chat_backdrop_ex(
                    ui.painter(),
                    ui.max_rect(),
                    bg_style,
                    wp_tex,
                    self.settings.theme.wallpaper_dim,
                );
                let chat_fade = if self.chat_opened_at > 0.0 {
                    ((ui_t - self.chat_opened_at) / 0.14).clamp(0.0, 1.0) as f32
                } else {
                    1.0
                };
                if chat_fade < 0.99 {
                    ctx.request_repaint();
                    ui.set_opacity(chat_fade);
                }
                self.messages_ui(ui);
            });
        // Плавающее мини-окно эмодзи/стикеров/GIF работает и на узком экране, и при наведении на ПК!
        if narrow || (self.emoji_hover_open && !self.emoji_open) {
            self.emoji_window(&ctx);
        }
        glass::paint_toast(&ctx, &toast_txt, toast_is_err, toast_k);
        self.render_modals(&ctx, frame);
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.shutdown_resources();
    }
}

impl Drop for App {
    fn drop(&mut self) {
        self.shutdown_resources();
    }
}

impl App {
    /// Все поверх-оконные шторки, модалки и плееры (работают и в обычном, и в узком Android-режиме).
    fn render_modals(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        self.chat_windows(ctx);
        self.viewer_window(ctx);
        self.contacts_window(ctx);
        self.drawer(ctx);
        self.create_window(ctx);
        self.settings_window(ctx);
        self.tg_settings_window(ctx);
        self.proxy_window(ctx);
        self.story_composer_window(ctx);
        self.call_window(ctx);
        self.calls_history_window(ctx);
        self.secret_chats_window(ctx);
        self.admin_window(ctx);
        self.stars_gifts_window(ctx);
        self.paid_reaction_modal(ctx);
        self.business_window(ctx);
        self.themes_window(ctx);
        self.emoji_status_window(ctx);
        self.scheduled_window(ctx);
        self.todo_creator_window(ctx);
        self.poll_creator_window(ctx);
        self.hashtag_search_window(ctx);
        self.quick_switcher_window(ctx);
        self.global_deleted_window(ctx);
        self.date_jump_window(ctx);
        self.peek_window(ctx);
        self.contact_send_window(ctx);
        self.geo_send_window(ctx);
        self.chat_action_confirm_window(ctx);
        self.pinned_list_window(ctx);
        self.folder_invite_window(ctx);
        self.confirm_unpin_modal(ctx);
        if let Some((path, caption, reply_to, opts)) =
            media_editor::media_editor_window(ctx, &mut self.media_editor, &worker::home().join("cache"))
            && let Some(did) = self.selected_dialog_id()
        {
            let cleanup_temp = path
                .parent()
                .is_some_and(|p| p.ends_with("edited"));
            self.send(Cmd::SendMediaEdited {
                dialog_id: did,
                path,
                caption,
                reply_to,
                opts,
                cleanup_temp,
            });
        }
        self.toasts_overlay(ctx);
        #[cfg(windows)]
        {
            self.inline_gif_overlay(ctx, frame);
            self.player_overlay(ctx, frame);
            self.audio_overlay(ctx, frame);
            self.miniapp_overlay(ctx, frame);
        }
        #[cfg(target_os = "android")]
        {
            let _ = frame;
            if let Some((title, url)) = self.miniapp_modal.take() {
                self.android_webview_open = true;
                android_jni::open_webview(&url, &title, self.settings.ayu.spoof_webview_as_android);
            }
        }
        #[cfg(all(not(windows), not(target_os = "android")))]
        {
            let _ = frame;
            self.miniapp_fallback_window(ctx);
        }
    }

    /// Модальное окно подтверждения открепления сообщения.
    pub(crate) fn confirm_unpin_modal(&mut self, ctx: &egui::Context) {
        let Some((_did, mid)) = self.confirm_unpin else {
            return;
        };
        let mut close = false;
        egui::Window::new("Открепить сообщение?")
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(300.0);
                ui.label("Сообщение останется в истории чата, но пропадёт из закрепа.");
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if orange_button(ui, "Открепить", Vec2::new(112.0, 30.0)).clicked()
                        && let Some(did) = self.selected_dialog_id()
                    {
                        self.send(Cmd::Pin(did, mid, false));
                        close = true;
                    }
                    if ui.button("Отмена").clicked() {
                        close = true;
                    }
                });
            });
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.confirm_unpin = None;
        }
    }

    /// Предпросмотр последних 20 сообщений чата без пометки прочитанным.
    fn peek_window(&mut self, ctx: &egui::Context) {
        let Some((did, title, rows)) = self.peek_modal.clone() else {
            return;
        };
        let mut open = true;
        let mut open_chat = false;
        let w = (ctx.content_rect().width() - 32.0).clamp(280.0, 520.0);
        egui::Window::new(format!("👁 Предпросмотр: {title}"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(w)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(18, 20, 28, 232))
                    .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(36)))
                    .corner_radius(egui::CornerRadius::same(16))
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(w - 24.0);
                let appear = theme::tween(ctx, "mdl-peek", 1.0, 0.16);
                glass::paint_modal_card(ui.painter(), ui.max_rect().expand(14.0), 16.0, appear);
                ui.label(
                    RichText::new("Сообщения загружены локально — статус прочтения не отправляется")
                        .size(11.5)
                        .color(SUBTEXT),
                );
                ui.add_space(6.0);
                egui::ScrollArea::vertical()
                    .max_height(380.0)
                    .stick_to_bottom(true)
                    .show(ui, |ui| {
                        if rows.is_empty() {
                            ui.label(RichText::new("Нет сообщений").color(SUBTEXT));
                        }
                        for m in &rows {
                            let bg = if m.outgoing {
                                Color32::from_rgba_unmultiplied(c_orange().r(), c_orange().g(), c_orange().b(), 30)
                            } else {
                                Color32::from_white_alpha(14)
                            };
                            egui::Frame::new()
                                .fill(bg)
                                .corner_radius(egui::CornerRadius::same(10))
                                .inner_margin(egui::Margin::symmetric(10, 6))
                                .show(ui, |ui| {
                                    ui.set_width((w - 56.0).max(180.0));
                                    ui.horizontal(|ui| {
                                        let who = if m.outgoing {
                                            "Вы"
                                        } else if m.sender.is_empty() {
                                            title.as_str()
                                        } else {
                                            m.sender.as_str()
                                        };
                                        ui.label(RichText::new(who).size(12.0).strong().color(c_orange()));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            ui.label(RichText::new(theme::hhmm(m.date)).size(11.0).color(SUBTEXT));
                                        });
                                    });
                                    if let Some(mi) = &m.media {
                                        ui.label(RichText::new(format!("[{}]", mi.name)).size(12.0).color(SUBTEXT));
                                    }
                                    if !m.text.is_empty() {
                                        ui.label(RichText::new(&m.text).size(13.5).color(WHITE));
                                    }
                                });
                            ui.add_space(4.0);
                        }
                    });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    if orange_button(ui, "Открыть чат", Vec2::new(136.0, 32.0)).clicked() {
                        open_chat = true;
                    }
                    if ui.button("Закрыть").clicked() || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                        self.peek_modal = None;
                    }
                });
            });
        if open_chat {
            self.peek_modal = None;
            if let Some(pos) = self.dialogs.iter().position(|d| d.id == did) {
                self.open_dialog(pos);
            }
        } else if !open {
            self.peek_modal = None;
        }
    }

    /// Модальная форма отправки контакта в открытый чат.
    fn contact_send_window(&mut self, ctx: &egui::Context) {
        if !self.contact_modal_open {
            return;
        }
        let mut open = true;
        let mut submit = false;
        egui::Window::new("Отправить контакт")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(18, 20, 28, 232))
                    .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(36)))
                    .corner_radius(egui::CornerRadius::same(16))
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(280.0);
                let appear = theme::tween(ctx, "mdl-geo-send", 1.0, 0.16);
                glass::paint_modal_card(ui.painter(), ui.max_rect().expand(14.0), 16.0, appear);
                let appear = theme::tween(ctx, "mdl-contact-send", 1.0, 0.16);
                glass::paint_modal_card(ui.painter(), ui.max_rect().expand(14.0), 16.0, appear);
                ui.label(RichText::new("Телефон (E.164):").size(12.0).color(SUBTEXT));
                ui.add(egui::TextEdit::singleline(&mut self.send_contact_phone).desired_width(270.0).hint_text("+79991234567"));
                ui.add_space(4.0);
                ui.label(RichText::new("Имя:").size(12.0).color(SUBTEXT));
                ui.add(egui::TextEdit::singleline(&mut self.send_contact_first).desired_width(270.0).hint_text("Иван"));
                ui.add_space(4.0);
                ui.label(RichText::new("Фамилия (необязательно):").size(12.0).color(SUBTEXT));
                ui.add(egui::TextEdit::singleline(&mut self.send_contact_last).desired_width(270.0).hint_text("Иванов"));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let valid = !self.send_contact_phone.trim().is_empty() && !self.send_contact_first.trim().is_empty();
                    let send_lbl = RichText::new("Отправить").color(if valid { theme::on_accent() } else { SUBTEXT });
                    if ui.add_enabled(valid, egui::Button::new(send_lbl).fill(c_orange())).clicked() {
                        submit = true;
                    }
                    if ui.button("Отмена").clicked() {
                        self.contact_modal_open = false;
                    }
                });
            });
        if submit && let Some(did) = self.selected_dialog_id() {
            let reply_to = self.reply.take().map(|(id, _)| id);
            self.send(Cmd::SendContact {
                dialog_id: did,
                phone: self.send_contact_phone.trim().to_owned(),
                first_name: self.send_contact_first.trim().to_owned(),
                last_name: self.send_contact_last.trim().to_owned(),
                reply_to,
            });
            self.send_contact_phone.clear();
            self.send_contact_first.clear();
            self.send_contact_last.clear();
            self.contact_modal_open = false;
        } else if submit || !open {
            self.contact_modal_open = false;
        }
    }

    /// Модальная форма отправки геопозиции (`lat`, `lon`).
    fn geo_send_window(&mut self, ctx: &egui::Context) {
        if !self.geo_modal_open {
            return;
        }
        let mut open = true;
        let mut submit = false;
        egui::Window::new("Отправить геопозицию")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(18, 20, 28, 232))
                    .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(36)))
                    .corner_radius(egui::CornerRadius::same(16))
                    .inner_margin(egui::Margin::same(14)),
            )
            .show(ctx, |ui| {
                ui.set_min_width(280.0);
                let appear = theme::tween(ctx, "mdl-geo-send", 1.0, 0.16);
                glass::paint_modal_card(ui.painter(), ui.max_rect().expand(14.0), 16.0, appear);
                let appear = theme::tween(ctx, "mdl-contact-send", 1.0, 0.16);
                glass::paint_modal_card(ui.painter(), ui.max_rect().expand(14.0), 16.0, appear);
                ui.label(RichText::new("Широта (-90..90):").size(12.0).color(SUBTEXT));
                ui.add(egui::TextEdit::singleline(&mut self.geo_lat).desired_width(270.0));
                ui.add_space(4.0);
                ui.label(RichText::new("Долгота (-180..180):").size(12.0).color(SUBTEXT));
                ui.add(egui::TextEdit::singleline(&mut self.geo_lon).desired_width(270.0));
                let parsed = self.geo_lat.trim().replace(',', ".").parse::<f64>().ok().zip(
                    self.geo_lon.trim().replace(',', ".").parse::<f64>().ok(),
                );
                let ok = parsed.is_some_and(|(lat, lon)| (-90.0..=90.0).contains(&lat) && (-180.0..=180.0).contains(&lon));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    let send_lbl = RichText::new("Отправить точку").color(if ok { theme::on_accent() } else { SUBTEXT });
                    if ui.add_enabled(ok, egui::Button::new(send_lbl).fill(c_orange())).clicked() {
                        submit = true;
                    }
                    if ui.button("Отмена").clicked() {
                        self.geo_modal_open = false;
                    }
                });
            });
        if submit
            && let Some(did) = self.selected_dialog_id()
            && let (Ok(lat), Ok(lon)) = (
                self.geo_lat.trim().replace(',', ".").parse::<f64>(),
                self.geo_lon.trim().replace(',', ".").parse::<f64>(),
            )
        {
            let reply_to = self.reply.take().map(|(id, _)| id);
            self.send(Cmd::SendGeo { dialog_id: did, lat, lon, reply_to });
            self.geo_modal_open = false;
        } else if submit || !open {
            self.geo_modal_open = false;
        }
    }

    #[cfg(windows)]
    #[allow(deprecated)]
    fn miniapp_overlay(&mut self, ctx: &egui::Context, frame: &eframe::Frame) {
        let Some((title, url)) = self.miniapp_modal.clone() else {
            if let Some(v) = self.miniapp_view.take() {
                v.stop();
            }
            return;
        };
        let screen = ctx.content_rect();
        let ppp = ctx.pixels_per_point();
        let open_k = self
            .miniapp_opened_at
            .map_or(1.0, |t| (t.elapsed().as_secs_f32() / 0.22).clamp(0.0, 1.0));
        if open_k < 1.0 {
            ctx.request_repaint();
        }
        let ease = ios_spring(open_k);
        let bridge = self
            .miniapp_view
            .as_ref()
            .map(video::MiniAppView::bridge_snapshot)
            .unwrap_or_default();
        if bridge.expanded {
            self.miniapp_expanded = true;
        }
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape)) || bridge.wants_close;
        let mut reload = false;
        let mut open_ext = false;
        let mut back_pressed = false;
        let mut main_pressed = false;
        let base_w: f32 = if self.miniapp_expanded {
            (screen.width() - 48.0).min(860.0)
        } else if self.settings.ayu.increase_webview_width {
            560.0
        } else {
            440.0
        };
        let base_h: f32 = if self.miniapp_expanded {
            (screen.height() - 48.0).min(840.0)
        } else if self.settings.ayu.increase_webview_height {
            720.0
        } else {
            610.0
        };
        let card_w = base_w.min(screen.width() - 24.0).max(300.0);
        let card_h = base_h.min(screen.height() - 36.0).max(340.0);
        let slide_y = (1.0 - ease) * 24.0;
        let card_rect = egui::Rect::from_center_size(
            screen.center() + Vec2::new(0.0, slide_y),
            Vec2::new(card_w, card_h),
        );
        let bottom_bar_h = if bridge.main_btn_visible && !bridge.main_btn_text.is_empty() { 48.0 } else { 2.0 };
        let web_rect = egui::Rect::from_min_max(
            card_rect.min + Vec2::new(2.0, 44.0),
            card_rect.max - Vec2::new(2.0, bottom_bar_h),
        );
        if self.miniapp_view.is_none() {
            let dir = worker::account_home(&worker::home(), self.settings.active_account);
            match video::MiniAppView::new(
                frame,
                &url,
                &dir,
                self.settings.ayu.spoof_webview_as_android,
            ) {
                Ok(v) => self.miniapp_view = Some(v),
                Err(e) => {
                    self.error = e;
                    self.error_at = self.now;
                    rich::open_url(&url);
                    self.miniapp_modal = None;
                    return;
                }
            }
        }
        if let Some(v) = self.miniapp_view.as_mut() {
            v.place((
                (web_rect.min.x * ppp).round() as i32,
                (web_rect.min.y * ppp).round() as i32,
                (web_rect.width() * ppp).round() as u32,
                (web_rect.height() * ppp).round() as u32,
            ));
            let (ext_link, tg_link) = v.take_links();
            if let Some(u) = ext_link {
                rich::open_url(&u);
            }
            if let Some(tgu) = tg_link {
                if let Some(parsed) = rg_core::engine::parse_tg_link(&tgu) {
                    self.send(Cmd::OpenTgLink(parsed));
                    close = true;
                } else {
                    rich::open_url(&tgu);
                }
            }
        }
        egui::Area::new(egui::Id::new("miniapp-overlay"))
            .order(egui::Order::Foreground)
            .fixed_pos(screen.min)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                paint_frosted_scrim(ui.painter(), rect, ease);
                ui.painter().rect_filled(card_rect, egui::CornerRadius::same(14), t2(WINDOW_BG));
                paint_modal_subblur(ui.painter(), card_rect, 14.0, ease);
                let hdr = egui::Rect::from_min_size(card_rect.min, Vec2::new(card_w, 42.0));
                if let Some((r, g, b)) = bridge.header_color {
                    ui.painter().rect_filled(
                        hdr,
                        egui::CornerRadius { nw: 14, ne: 14, sw: 0, se: 0 },
                        Color32::from_rgb(r, g, b),
                    );
                }
                let mut left_x = hdr.left() + 14.0;
                if bridge.back_visible {
                    let br = egui::Rect::from_center_size(egui::pos2(left_x + 12.0, hdr.center().y), Vec2::splat(28.0));
                    let bresp = ui.interact(br, ui.make_persistent_id("miniapp-back"), Sense::click());
                    ui.painter().text(br.center(), egui::Align2::CENTER_CENTER, "←", FontId::proportional(16.0), if bresp.hovered() { ORANGE } else { WHITE });
                    if bresp.on_hover_text("Назад в Mini App").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        back_pressed = true;
                    }
                    left_x += 30.0;
                }
                icons::tg_sized(ui.painter(), "settings/language", egui::pos2(left_x + 8.0, hdr.center().y), Vec2::splat(16.0), ORANGE);
                let hdr_clip = egui::Rect::from_min_max(
                    egui::pos2(left_x + 22.0, hdr.top()),
                    egui::pos2((hdr.right() - 134.0).max(left_x + 60.0), hdr.bottom()),
                );
                ui.painter().with_clip_rect(hdr_clip).text(
                    egui::pos2(left_x + 24.0, hdr.center().y),
                    egui::Align2::LEFT_CENTER,
                    &title,
                    FontId::proportional(14.5),
                    WHITE,
                );
                let btn_x = egui::Rect::from_center_size(egui::pos2(hdr.right() - 22.0, hdr.center().y), Vec2::splat(28.0));
                let xr = ui.interact(btn_x, ui.make_persistent_id("miniapp-close"), Sense::click());
                icons::tg(ui.painter(), "info/info_close", btn_x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
                if xr.on_hover_text("Закрыть Mini App").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    close = true;
                }
                let btn_exp = egui::Rect::from_center_size(egui::pos2(hdr.right() - 52.0, hdr.center().y), Vec2::splat(28.0));
                let expr = ui.interact(btn_exp, ui.make_persistent_id("miniapp-expand"), Sense::click());
                {
                    let ec = btn_exp.center();
                    let col = if expr.hovered() { ORANGE } else { SUBTEXT };
                    let st = egui::Stroke::new(1.5, col);
                    if self.miniapp_expanded {
                        ui.painter().line_segment([ec + Vec2::new(-5.0, -5.0), ec + Vec2::new(-1.5, -1.5)], st);
                        ui.painter().line_segment([ec + Vec2::new(5.0, 5.0), ec + Vec2::new(1.5, 1.5)], st);
                        ui.painter().rect_stroke(egui::Rect::from_center_size(ec, Vec2::splat(7.0)), 1.5, st, egui::StrokeKind::Inside);
                    } else {
                        ui.painter().line_segment([ec + Vec2::new(-5.0, 5.0), ec + Vec2::new(5.0, -5.0)], st);
                        ui.painter().line_segment([ec + Vec2::new(1.0, -5.0), ec + Vec2::new(5.0, -5.0)], st);
                        ui.painter().line_segment([ec + Vec2::new(5.0, -5.0), ec + Vec2::new(5.0, -1.0)], st);
                        ui.painter().line_segment([ec + Vec2::new(-5.0, 5.0), ec + Vec2::new(-1.0, 5.0)], st);
                        ui.painter().line_segment([ec + Vec2::new(-5.0, 5.0), ec + Vec2::new(-5.0, 1.0)], st);
                    }
                }
                if expr.on_hover_text(if self.miniapp_expanded { "Обычный размер" } else { "Развернуть" }).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.miniapp_expanded = !self.miniapp_expanded;
                }
                let btn_ext = egui::Rect::from_center_size(egui::pos2(hdr.right() - 82.0, hdr.center().y), Vec2::splat(28.0));
                let er = ui.interact(btn_ext, ui.make_persistent_id("miniapp-ext"), Sense::click());
                icons::tg_sized(
                    ui.painter(),
                    "info/info_media_forward",
                    btn_ext.center(),
                    Vec2::splat(16.0),
                    if er.hovered() { ORANGE } else { SUBTEXT },
                );
                if er.on_hover_text("Открыть в браузере").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open_ext = true;
                }
                let btn_rel = egui::Rect::from_center_size(egui::pos2(hdr.right() - 112.0, hdr.center().y), Vec2::splat(28.0));
                let rr = ui.interact(btn_rel, ui.make_persistent_id("miniapp-reload"), Sense::click());
                icons::tg_sized(
                    ui.painter(),
                    "mediaview/rotate",
                    btn_rel.center(),
                    Vec2::splat(16.0),
                    if rr.hovered() { WHITE } else { SUBTEXT },
                );
                if rr.on_hover_text("Обновить").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    reload = true;
                }
                if bridge.main_btn_visible && !bridge.main_btn_text.is_empty() {
                    let mb_rect = egui::Rect::from_min_max(
                        egui::pos2(card_rect.left() + 10.0, card_rect.bottom() - 42.0),
                        egui::pos2(card_rect.right() - 10.0, card_rect.bottom() - 8.0),
                    );
                    let mbr = ui.interact(mb_rect, ui.make_persistent_id("miniapp-mainbtn"), Sense::click());
                    let base_c = bridge.main_btn_color.map_or(c_orange(), |(r, g, b)| Color32::from_rgb(r, g, b));
                    let fill_c = if mbr.hovered() { base_c.gamma_multiply(1.12) } else { base_c };
                    ui.painter().rect_filled(mb_rect, egui::CornerRadius::same(10), fill_c);
                    ui.painter().text(mb_rect.center(), egui::Align2::CENTER_CENTER, &bridge.main_btn_text, FontId::proportional(14.0), theme::contrast_ink(fill_c));
                    if mbr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        main_pressed = true;
                    }
                }
                if resp.clicked() && !ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| card_rect.contains(p)) {
                    close = true;
                }
            });
        if back_pressed && let Some(v) = self.miniapp_view.as_ref() {
            v.emit_back_pressed();
        }
        if main_pressed && let Some(v) = self.miniapp_view.as_ref() {
            v.emit_main_button_pressed();
        }
        if reload && let Some(v) = self.miniapp_view.as_ref() {
            v.reload();
        }
        if open_ext {
            rich::open_url(&url);
        }
        if close {
            if let Some(v) = self.miniapp_view.take() {
                v.stop();
            }
            self.miniapp_modal = None;
        }
    }

    #[cfg(all(not(windows), not(target_os = "android")))]
    fn miniapp_fallback_window(&mut self, ctx: &egui::Context) {
        let Some((title, url)) = self.miniapp_modal.clone() else { return };
        let mut open = true;
        egui::Window::new(format!("Mini App · {title}"))
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new(&title).strong().color(WHITE));
                ui.add_space(6.0);
                if orange_button(ui, "Открыть Mini App", Vec2::new(200.0, 34.0)).clicked() {
                    rich::open_url(&url);
                    self.miniapp_modal = None;
                }
            });
        if !open {
            self.miniapp_modal = None;
        }
    }
}

/// Настройки окна ПК.
pub fn native_options() -> eframe::NativeOptions {
    // RG_SIZE=ШxВ — размер окна для проверки узкой раскладки.
    let size = std::env::var("RG_SIZE")
        .ok()
        .and_then(|s| {
            let (w, h) = s.split_once('x')?;
            Some([w.parse::<f32>().ok()?, h.parse::<f32>().ok()?])
        })
        .unwrap_or([1100.0, 720.0]);
    let icon_rgba = win::render_app_icon_rgba(0, 0, false, true, false, 64);
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([420.0, 480.0])
            .with_title("Rustgramm")
            .with_icon(egui::IconData {
                rgba: icon_rgba,
                width: 64,
                height: 64,
            })
            .with_transparent(true)
            .with_decorations(false),
        ..Default::default()
    }
}

/// Запуск интерфейса с готовыми параметрами (ПК и Android).
pub fn run(options: eframe::NativeOptions) -> eframe::Result {
    rg_core::diag::install_panic_hook();
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let (ev_tx, ev_rx): (Sender<Ev>, Receiver<Ev>) = channel();
    eframe::run_native(
        "Rustgramm",
        options,
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            worker::spawn(cmd_rx, ev_tx, cc.egui_ctx.clone());
            Ok(Box::new(App::new(cmd_tx, ev_rx)))
        }),
    )
}

pub fn run_desktop() -> eframe::Result {
    if let Some(mode) = std::env::var_os("RG_DEMO") {
        return run_demo(&mode.to_string_lossy());
    }
    let Some(_single_instance): Option<win::SingleInstanceGuard> = win::acquire_single_instance() else {
        return Ok(());
    };
    std::thread::spawn(|| win::dress_window("Rustgramm"));
    run(native_options())
}

/// Демо-данные для проверки внешнего вида без сети и без аккаунта (`RG_DEMO=1` или `qr`).
fn demo_app(mode: &str) -> App {
    let qr = mode == "qr";
    use grammers_session::types::{PeerAuth, PeerId, PeerRef};
    use rg_core::media::{MediaInfo, MediaKind};
    let (cmd, _rx) = tokio::sync::mpsc::unbounded_channel();
    let (_tx, ev) = channel();
    let mut app = App::new(cmd, ev);
    app.is_demo = true;
    if qr {
        app.stage = Stage::Qr("tg://login?token=Zm9vYmFyYmF6RGVtb1Rva2VuRm9yU2NyZWVuc2hvdA".to_owned());
        return app;
    }
    if mode == "login" {
        app.stage = Stage::NeedPhone;
        return app;
    }
    app.stage = Stage::Ready;
    app.me = "Демо".to_owned();
    let now = 1_763_730_000_i64;
    let names = [
        ("Анна Петрова", "Спасибо, посмотрю вечером 👍", 0, 0, false),
        ("Команда проекта", "Релиз в пятницу, всё готово", 3, 3_600, false),
        ("Мама", "Не забудь позвонить вечером", 12, 90_000, false),
        ("Новости Rust", "Вышла версия 1.100", 1204, 260_000, true),
        ("Дмитрий", "Скинул файл, посмотри", 0, 3_000_000, false),
    ];
    for (i, (n, l, unread, ago, muted)) in names.iter().enumerate() {
        let id = 1000 + i as i64;
        app.dialogs.push(DialogRow {
            peer: PeerRef {
                id: PeerId::user_unchecked(id),
                auth: PeerAuth::default(),
            },
            id,
            name: (*n).to_owned(),
            last: (*l).to_owned(),
            date: now - ago,
            unread: *unread,
            pinned: i == 0,
            muted: *muted,
            notify_explicit: (*muted).then_some(true),
            saved: false,
            archived: false,
            kind: rg_core::engine::DialogKind::User,
            contact: true,
            read_out: 0,
            emoji_status: None,
            verified: i == 0,
            premium: i == 1,
            unread_mentions: 0,
            unread_reactions: 0,
            read_inbox_max_id: 0,
            peer_color: Some((i % 7) as i32),
        });
    }
    for (i, (n, l)) in [("Старый чат", "Давно не писали"), ("Рассылка", "Скидки на всё")].iter().enumerate() {
        let id = 2000 + i as i64;
        app.dialogs.push(DialogRow {
            peer: PeerRef {
                id: PeerId::user_unchecked(id),
                auth: PeerAuth::default(),
            },
            id,
            name: (*n).to_owned(),
            last: (*l).to_owned(),
            date: now - 9_000_000,
            unread: 4,
            pinned: false,
            muted: false,
            notify_explicit: None,
            saved: false,
            archived: true,
            kind: rg_core::engine::DialogKind::User,
            contact: false,
            read_out: 0,
            emoji_status: None,
            verified: false,
            premium: false,
            unread_mentions: 0,
            unread_reactions: 0,
            read_inbox_max_id: 0,
            peer_color: Some((i % 7) as i32),
        });
    }
    app.selected = Some(0);
    let mk = |id: i32, day: i64, text: &str, outgoing, deleted, edits, media| MessageRow {
        deleted,
        edits,
        media,
        ..MessageRow::plain(
            id,
            text.to_owned(),
            outgoing,
            now - day * 86_400 + i64::from(id) * 90,
        )
    };
    app.messages = vec![
        mk(1, 1, "Привет! Ты уже видел новый интерфейс?", false, false, 0, None),
        mk(2, 1, "Да, выглядит как Telegram, только чёрно-белый 😊🔥", true, false, 0, None),
        mk(3, 0, "Это сообщение удалили, но оно сохранилось", false, true, 0, None),
        mk(4, 0, "Исправил опечатку в тексте", true, false, 2, None),
        mk(
            5,
            0,
            "Вот файл с отчётом, посмотри когда будет время, там всё по новому интерфейсу и скорости",
            false,
            false,
            0,
            Some(MediaInfo::basic(MediaKind::File, "report.pdf", 1_572_864)),
        ),
        mk(6, 0, "Спасибо, посмотрю вечером 👍❤️", true, false, 0, None),
        MessageRow {
            entities: vec![
                rg_core::engine::Entity { kind: rg_core::engine::EntityKind::Bold, start: 0, end: 6, url: None },
                rg_core::engine::Entity { kind: rg_core::engine::EntityKind::Italic, start: 7, end: 13, url: None },
                rg_core::engine::Entity { kind: rg_core::engine::EntityKind::Code, start: 14, end: 17, url: None },
                rg_core::engine::Entity { kind: rg_core::engine::EntityKind::Spoiler, start: 18, end: 24, url: None },
                rg_core::engine::Entity { kind: rg_core::engine::EntityKind::Link, start: 25, end: 36, url: None },
            ],
            ..MessageRow::plain(7, "Жирный курсив код секрет example.com 🔥".to_owned(), false, now + 700)
        },
        MessageRow {
            media: Some(MediaInfo {
                duration: 14,
                waveform: (0..60).map(|i| ((i * 7) % 28) as u8 + 2).collect(),
                ..MediaInfo::basic(MediaKind::Voice, "Голосовое сообщение", 40_000)
            }),
            ..MessageRow::plain(8, String::new(), true, now + 800)
        },
        MessageRow {
            media: Some(MediaInfo {
                duration: 73,
                dims: (1280, 720),
                has_thumb: true,
                ..MediaInfo::basic(MediaKind::Video, "Видео", 9_000_000)
            }),
            ..MessageRow::plain(9, "Запись экрана".to_owned(), false, now + 900)
        },
    ];
    app.settings = Settings::default();
    app.now = now;
    if let Some(tab) = mode.strip_prefix("settings") {
        app.settings_open = true;
        app.settings_tab = tab.parse().unwrap_or(0);
    }
    app.drawer_open = mode == "drawer";
    app.in_archive = mode == "archive";
    app.emoji_open = mode == "emoji";
    app
}

/// Запуск демо-окна.
pub fn run_demo(mode: &str) -> eframe::Result {
    let mode = mode.to_owned();
    eframe::run_native(
        "Rustgramm (демо)",
        {
            std::thread::spawn(|| win::dress_window("Rustgramm"));
            native_options()
        },
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(demo_app(&mode)))
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use grammers_session::types::{PeerAuth, PeerId, PeerRef};
    use rg_core::engine::{AccountOverview, DialogKind, NotifyRow, NotifyScope};
    use std::time::{Duration, Instant};

    fn test_dialog(kind: DialogKind, muted: bool, archived: bool, saved: bool, notify_explicit: Option<bool>) -> DialogRow {
        DialogRow {
            peer: PeerRef {
                id: PeerId::user_unchecked(42),
                auth: PeerAuth::default(),
            },
            id: 42,
            name: "Тест".to_owned(),
            last: String::new(),
            date: 0,
            unread: 1,
            pinned: false,
            muted,
            notify_explicit,
            saved,
            archived,
            kind,
            contact: true,
            read_out: 0,
            emoji_status: None,
            verified: false,
            premium: false,
            unread_mentions: 0,
            unread_reactions: 0,
            read_inbox_max_id: 0,
            peer_color: None,
        }
    }

    fn test_account(users: bool, chats: bool, broadcasts: bool) -> AccountOverview {
        AccountOverview {
            notify: vec![
                NotifyRow { scope: NotifyScope::Users, title: "Личные чаты", enabled: users, preview: true },
                NotifyRow { scope: NotifyScope::Chats, title: "Группы", enabled: chats, preview: true },
                NotifyRow { scope: NotifyScope::Broadcasts, title: "Каналы", enabled: broadcasts, preview: true },
            ],
            ..AccountOverview::default()
        }
    }

    #[test]
    fn notify_mute_respects_unknown_archived_saved_explicit_and_global_scopes() {
        let acc_all_muted = test_account(false, false, false);
        let acc_users_only = test_account(true, false, false);

        assert!(is_dialog_muted_for_notify(None, Some(&acc_users_only)));
        assert!(is_dialog_muted_for_notify(
            Some(&test_dialog(DialogKind::User, false, true, false, Some(false))),
            Some(&acc_users_only),
        ));
        assert!(is_dialog_muted_for_notify(
            Some(&test_dialog(DialogKind::User, false, false, true, Some(false))),
            Some(&acc_users_only),
        ));
        assert!(is_dialog_muted_for_notify(
            Some(&test_dialog(DialogKind::User, true, false, false, Some(true))),
            Some(&acc_users_only),
        ));
        assert!(is_dialog_muted_for_notify(
            Some(&test_dialog(DialogKind::User, false, false, false, None)),
            None,
        ));
        assert!(is_dialog_muted_for_notify(
            Some(&test_dialog(DialogKind::Channel, false, false, false, None)),
            Some(&acc_users_only),
        ));
        assert!(!is_dialog_muted_for_notify(
            Some(&test_dialog(DialogKind::Channel, false, false, false, Some(false))),
            Some(&acc_all_muted),
        ));
        assert!(!is_dialog_muted_for_notify(
            Some(&test_dialog(DialogKind::User, false, false, false, None)),
            Some(&acc_users_only),
        ));
    }

    #[test]
    fn notify_gate_suppresses_spam_bursts_above_three_per_second() {
        let mut gate = NotifyGate::default();
        let t0 = Instant::now();

        // 3 сообщения за секунду не считаются спамом и отдаются через 350 мс.
        for i in 0..3 {
            let t = t0 + Duration::from_millis(i * 100);
            assert!(!gate.record_incoming(t, 100));
            gate.enqueue(t, "Иван".to_owned(), format!("msg {i}"));
        }
        // Четвёртое сообщение за ту же секунду гасит весь всплеск ещё до срабатывания таймера 350 мс.
        let t3 = t0 + Duration::from_millis(280);
        assert!(gate.record_incoming(t3, 100));
        assert!(gate.take_ready(t0 + Duration::from_millis(700)).is_none());

        // Во время 6-секундного антиспама новые единичные сообщения тоже игнорируются.
        let t_during = t3 + Duration::from_secs(2);
        assert!(gate.record_incoming(t_during, 200));
        gate.enqueue(t_during, "Анна".to_owned(), "привет".to_owned());
        assert!(gate.take_ready(t_during + Duration::from_millis(500)).is_none());

        // После окончания паузы тишины (>6 сек) одиночные сообщения снова уведомляют.
        let t_after = t3 + Duration::from_secs(7);
        assert!(!gate.record_incoming(t_after, 200));
        gate.enqueue(t_after, "Анна".to_owned(), "на связи".to_owned());
        assert!(gate.take_ready(t_after + Duration::from_millis(200)).is_none());
        let delivered = gate.take_ready(t_after + Duration::from_millis(360));
        assert_eq!(delivered, Some(("Анна".to_owned(), "на связи".to_owned())));
    }

    #[test]
    fn dialogs_sidebar_renders_search_at_top_and_all_chats_visible() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(1100.0, 720.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::Panel::top("titlebar")
                .exact_size(34.0)
                .show(ui, |ui| app.titlebar(ui));
            egui::Panel::left("dialogs")
                .default_size(COLUMN_LEFT_NORMAL)
                .show(ui, |ui| app.dialogs_ui(ui));
            egui::CentralPanel::default().show(ui, |_ui| {});
        });
        out.textures_delta.clear();

        let mut search_y: Option<f32> = None;
        let mut chat_ys: Vec<(String, f32)> = Vec::new();
        let mut visit = |shapes: &[egui::epaint::ClippedShape]| {
            fn walk(shape: &egui::Shape, search_y: &mut Option<f32>, chat_ys: &mut Vec<(String, f32)>) {
                match shape {
                    egui::Shape::Text(t) => {
                        let txt = t.galley.text();
                        if txt.contains("Поиск") && search_y.is_none() {
                            *search_y = Some(t.pos.y);
                        }
                        if txt.contains("Анна") || txt.contains("Команда") || txt.contains("Мама") {
                            chat_ys.push((txt.to_owned(), t.pos.y));
                        }
                    }
                    egui::Shape::Vec(v) => {
                        for s in v {
                            walk(s, search_y, chat_ys);
                        }
                    }
                    _ => {}
                }
            }
            for cs in shapes {
                walk(&cs.shape, &mut search_y, &mut chat_ys);
            }
        };
        visit(&out.shapes);

        let sy = search_y.expect("search bar text must render");
        assert!(sy < 90.0, "search bar pushed down to y={sy}");
        assert!(
            chat_ys.len() >= 3,
            "expected demo chats rendered above fold, got {chat_ys:?}"
        );
        for (name, y) in &chat_ys {
            assert!(*y > 100.0 && *y < 700.0, "chat '{name}' offscreen at y={y}");
        }
    }

    #[test]
    fn login_screen_renders_cleanly_on_desktop_and_narrow_phone() {
        for (w, h) in [(1200.0_f32, 760.0_f32), (380.0_f32, 780.0_f32)] {
            let ctx = egui::Context::default();
            theme::apply(&ctx);
            let mut app = demo_app("login");
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(w, h))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.login_ui(ui));
            });
            out.textures_delta.clear();

            let mut texts = String::new();
            fn collect_text(shape: &egui::Shape, buf: &mut String) {
                match shape {
                    egui::Shape::Text(t) => {
                        buf.push_str(t.galley.text());
                        buf.push('\n');
                    }
                    egui::Shape::Vec(v) => {
                        for s in v {
                            collect_text(s, buf);
                        }
                    }
                    _ => {}
                }
            }
            for cs in &out.shapes {
                collect_text(&cs.shape, &mut texts);
            }
            assert!(texts.contains("Telegram"), "viewport {w}x{h} missing title: {texts}");
            assert!(texts.contains("Страна"), "viewport {w}x{h} missing Country label: {texts}");
            assert!(texts.contains("Номер телефона"), "viewport {w}x{h} missing Phone label: {texts}");
            assert!(texts.contains("QR-КОДУ"), "viewport {w}x{h} missing QR switch: {texts}");
        }
    }

    #[test]
    fn qr_demo_screen_renders_without_panicking() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("qr");
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 820.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| app.login_ui(ui));
        });
        out.textures_delta.clear();
        assert!(!out.shapes.is_empty());
    }

    #[test]
    fn optimistic_send_reconciles_confirm_and_fail_cleanly() {
        let mut app = demo_app("1");
        let did = app.dialogs[0].id;
        let tid = app.push_optimistic_message("Быстрое сообщение".to_owned(), Vec::new(), None, None);
        assert!(tid < 0);
        assert!(app.messages.iter().any(|m| m.id == tid && m.outgoing));
        assert_eq!(app.dialogs[0].last, "Быстрое сообщение");

        let confirmed = MessageRow::plain(9001, "Быстрое сообщение".to_owned(), true, app.now + 10);
        let ctx = egui::Context::default();
        app.handle_event(&ctx, Ev::SentConfirm(did, tid, Box::new(confirmed)));
        assert!(!app.messages.iter().any(|m| m.id == tid), "temp row must be replaced");
        assert!(app.messages.iter().any(|m| m.id == 9001), "confirmed row must exist");

        let tid2 = app.push_optimistic_message("Ошибка сети".to_owned(), Vec::new(), None, None);
        assert!(app.messages.iter().any(|m| m.id == tid2));
        app.handle_event(&ctx, Ev::SendFailed(did, tid2, "timeout".to_owned()));
        assert!(!app.messages.iter().any(|m| m.id == tid2), "failed temp row must be removed");
    }

    #[test]
    fn live_read_inbox_draft_and_reactions_update_dialog_state() {
        let mut app = demo_app("1");
        let did = app.dialogs[0].id;
        app.dialogs[0].unread = 5;
        let ctx = egui::Context::default();
        app.on_live(&ctx, rg_core::engine::Live::ReadInbox(did, 120));
        assert_eq!(app.dialogs[0].unread, 0);

        app.on_live(&ctx, rg_core::engine::Live::Draft(did, "черновик ответа".to_owned(), None));
        assert_eq!(app.drafts.get(&did).map(|(t, _)| t.as_str()), Some("черновик ответа"));

        app.on_live(&ctx, rg_core::engine::Live::MessageReactions(did, 5, 2));
        assert_eq!(app.dialogs[0].unread_reactions, 2);
    }

    #[test]
    fn local_read_clears_unread_and_persists_until_newer_message() {
        let mut app = demo_app("1");
        let did = app.dialogs[1].id;
        let orig_date = app.dialogs[1].date;
        app.dialogs[1].unread = 7;
        app.dialogs[1].unread_mentions = 2;
        app.local_read_dialog(1);
        assert_eq!(app.dialogs[1].unread, 0);
        assert_eq!(app.dialogs[1].unread_mentions, 0);
        assert_eq!(app.local_read_dates.get(&did), Some(&orig_date));

        let ctx = egui::Context::default();
        let mut refreshed = app.dialogs.clone();
        refreshed[1].unread = 7;
        app.handle_event(&ctx, Ev::Dialogs(refreshed));
        assert_eq!(app.dialogs[1].unread, 0, "stale server refresh must stay locally read");

        let mut newer = app.dialogs.clone();
        newer[1].date = orig_date + 60;
        newer[1].unread = 1;
        app.handle_event(&ctx, Ev::Dialogs(newer));
        assert_eq!(app.dialogs[1].unread, 1, "newer message must restore unread badge");
    }

    #[test]
    fn login_keypad_and_country_picker_math_and_alignment() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let accent = c_orange();
        let bg_cut = Color32::from_rgb(24, 25, 30);
        let mut country_rect = egui::Rect::NOTHING;
        let mut kp_buf = "+7".to_owned();
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 844.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                ui.vertical_centered(|ui| {
                    let col_w = (ui.available_width() - 28.0 - 32.0).clamp(260.0, 336.0);
                    egui::Frame::new()
                        .inner_margin(egui::Margin::symmetric(16, 20))
                        .show(ui, |ui| {
                            ui.set_width(col_w);
                            ui.vertical_centered(|ui| {
                                let form_w = ui.available_width().min(336.0);
                                country_rect = App::outlined_login_button(
                                    ui,
                                    "country",
                                    form_w,
                                    48.0,
                                    "Страна",
                                    false,
                                    accent,
                                    bg_cut,
                                    "Россия  (+7)",
                                    "Выберите страну",
                                    LoginButtonRight::Chevron(false),
                                )
                                .rect;
                                App::paint_android_keypad(ui, form_w, &mut kp_buf, true, 15);
                            });
                        });
                });
            });
        });
        out.textures_delta.clear();

        let mut key_xs: Vec<(String, f32)> = Vec::new();
        fn collect_key_pos(shape: &egui::Shape, out: &mut Vec<(String, f32)>) {
            match shape {
                egui::Shape::Text(t) => {
                    let txt = t.galley.text().trim();
                    if matches!(txt, "1" | "3") {
                        out.push((txt.to_owned(), t.pos.x + t.galley.size().x * 0.5));
                    }
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        collect_key_pos(s, out);
                    }
                }
                _ => {}
            }
        }
        for cs in &out.shapes {
            collect_key_pos(&cs.shape, &mut key_xs);
        }
        let k1_x = key_xs.iter().find(|(k, _)| k == "1").map(|(_, x)| *x).expect("key 1");
        let k3_x = key_xs.iter().find(|(k, _)| k == "3").map(|(_, x)| *x).expect("key 3");
        let mid_field = country_rect.center().x;
        let mid_keypad = (k1_x + k3_x) * 0.5;
        assert!(
            (mid_field - mid_keypad).abs() <= 4.0,
            "keypad must be horizontally centered under form field: mid_field={mid_field}, mid_keypad={mid_keypad}, country={country_rect:?}, keys={key_xs:?}"
        );

        let mut app = demo_app("login");
        app.login_country_open = true;
        let mut texts = String::new();
        for _ in 0..2 {
            let raw2 = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 844.0))),
                ..Default::default()
            };
            let mut out2 = ctx.run_ui(raw2, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    app.country_picker_window(ui.ctx(), accent);
                });
            });
            out2.textures_delta.clear();
            texts.clear();
            fn collect_t(shape: &egui::Shape, buf: &mut String) {
                match shape {
                    egui::Shape::Text(t) => {
                        buf.push_str(t.galley.text());
                        buf.push('\n');
                    }
                    egui::Shape::Vec(v) => {
                        for s in v {
                            collect_t(s, buf);
                        }
                    }
                    _ => {}
                }
            }
            for cs in &out2.shapes {
                collect_t(&cs.shape, &mut texts);
            }
        }
        assert!(
            texts.contains("Выбор страны") && texts.contains("Россия"),
            "country picker missing expected text: {texts}"
        );
    }

    fn collect_all_text(shape: &egui::Shape, buf: &mut String) {
        match shape {
            egui::Shape::Text(t) => {
                buf.push_str(t.galley.text());
                buf.push('\n');
            }
            egui::Shape::Vec(v) => {
                for s in v {
                    collect_all_text(s, buf);
                }
            }
            _ => {}
        }
    }

    #[test]
    fn android_phone_login_flow_preserves_code_and_dynamic_length() {
        let mut app = demo_app("login");
        app.stage = Stage::NeedPhone;
        app.login_dial = "+7".into();
        app.login_phone_full = "+7 9991234567".into();
        let norm = countries::normalize_phone(&app.login_dial, "89991234567").unwrap();
        assert_eq!(norm, "+79991234567");

        // Эмулируем Ev::CodeMeta длиной 6 цифр + переход в NeedCode.
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        app.handle_event(&ctx, Ev::CodeDeliveryHint("Код отправлен на email".into()));
        app.handle_event(&ctx, Ev::CodeMeta { length: 6, resend_secs: Some(45) });
        app.handle_event(&ctx, Ev::Stage(Stage::NeedCode));
        assert_eq!(app.login_code_len, 6);
        assert_eq!(app.login_resend_secs, 45);
        assert_eq!(app.stage, Stage::NeedCode);

        // Ввод 5 цифр не сбрасывает и сохраняет код при ошибке.
        app.code = "123456".into();
        app.handle_event(&ctx, Ev::Error("Неверный код подтверждения".into()));
        assert_eq!(app.code, "123456");

        // Переход на 2FA очищает code и рендерит экран пароля без паник.
        app.handle_event(&ctx, Ev::Stage(Stage::NeedPassword("год".into())));
        assert!(app.code.is_empty());
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 844.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            theme::paint_android_glass_backdrop(&ctx, ui.max_rect(), c_orange());
            egui::CentralPanel::default().show(ui, |ui| app.login_ui(ui));
        });
        out.textures_delta.clear();
        let mut txt = String::new();
        for cs in &out.shapes {
            collect_all_text(&cs.shape, &mut txt);
        }
        assert!(txt.contains("Подсказка: год") && txt.contains("облачный пароль"));
    }

    #[test]
    fn glass_backdrop_and_bar_render_cleanly() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        theme::set_opacity(72);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 844.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            theme::paint_android_glass_backdrop(&ctx, ui.max_rect(), c_orange());
            theme::paint_glass_bar(ui.painter(), egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 40.0)), 0.0, 152);
            theme::paint_glass_bar(ui.painter(), egui::Rect::from_min_size(egui::Pos2::new(0.0, 820.0), Vec2::new(390.0, 24.0)), 0.0, 148);
        });
        out.textures_delta.clear();
        assert!(!out.shapes.is_empty());
    }

    #[test]
    fn history_cache_uses_64_slot_lru_eviction() {
        let ctx = egui::Context::default();
        let mut app = demo_app("1");
        app.history_cache.clear();
        app.history_cache_order.clear();
        for did in 1..=64i64 {
            app.handle_event(&ctx, Ev::History(did, vec![MessageRow::plain(1, format!("m{did}"), true, 100 + did)]));
        }
        assert_eq!(app.history_cache.len(), 64);
        // Обращение к первому диалогу поднимает его в хвост LRU, поэтому вытеснится dialog=2.
        app.history_cache_order.retain(|&x| x != 1);
        app.history_cache_order.push_back(1);
        app.handle_event(&ctx, Ev::History(65, vec![MessageRow::plain(1, "m65".into(), true, 200)]));
        assert_eq!(app.history_cache.len(), 64);
        assert!(app.history_cache.contains_key(&1));
        assert!(!app.history_cache.contains_key(&2));
        assert!(app.history_cache.contains_key(&65));
    }

    #[test]
    fn tgs_eviction_protects_visible_docs_and_lottie_scratch_reuses_buffer() {
        let ctx = egui::Context::default();
        let mut app = demo_app("1");
        let tgs_json = br#"{"v":"5.7.4","fr":30,"ip":0,"op":30,"w":64,"h":64,"layers":[{"ty":4,"nm":"s","ip":0,"op":30,"st":0,"ks":{},"shapes":[]}]}"#;
        app.visible_tgs_docs.insert(999);
        app.handle_event(&ctx, Ev::TgsDoc(999, tgs_json.to_vec()));
        for id in 1..=165i64 {
            app.handle_event(&ctx, Ev::TgsDoc(id, tgs_json.to_vec()));
        }
        assert!(app.tgs_docs.contains_key(&999), "visible doc must survive eviction");

        let anim = lottie::TgsAnim::from_bytes(tgs_json).unwrap();
        let mut scratch = lottie::TgsScratch::default();
        let img1 = anim.render_frame_into(0.0, 32, &mut scratch);
        let img2 = anim.render_frame_into(10.0, 32, &mut scratch);
        assert_eq!(img1.size, [32, 32]);
        assert_eq!(img2.size, [32, 32]);
    }

    #[test]
    fn peek_history_contact_geo_reaction_pills_and_typing_wave_render() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        let mut msg = MessageRow::plain(5001, "Проверь пилюли и карточки".into(), false, app.now);
        msg.reaction_items = vec![rg_core::engine::ReactionPill {
            emoji: "🔥".into(),
            custom_doc_id: None,
            count: 7,
            chosen: true,
            paid: false,
        }];
        msg.media = Some(rg_core::media::MediaInfo::basic(
            rg_core::media::MediaKind::Contact {
                phone: "+79995551122".into(),
                first_name: "Алиса".into(),
                last_name: "Тест".into(),
                user_id: 99,
            },
            "Алиса Тест",
            0,
        ));
        let mut geo_msg = MessageRow::plain(5002, String::new(), true, app.now + 1);
        geo_msg.media = Some(rg_core::media::MediaInfo::basic(
            rg_core::media::MediaKind::Geo {
                lat_micro: 55_755_800,
                lon_micro: 37_617_300,
                title: "Кремль".into(),
            },
            "Кремль",
            0,
        ));
        app.messages.push(msg.clone());
        app.messages.push(geo_msg.clone());
        app.handle_event(&ctx, Ev::PeekHistory(123, "Секретный чат".into(), vec![msg]));
        app.contact_modal_open = true;
        app.geo_modal_open = true;
        let selected_before = app.selected;

        let mut txt = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(800.0, 700.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    chat::paint_typing_wave(ui.painter(), ui.ctx(), egui::pos2(20.0, 20.0), ORANGE, "печатает…");
                    app.messages_ui(ui);
                });
                app.peek_window(ui.ctx());
                app.contact_send_window(ui.ctx());
                app.geo_send_window(ui.ctx());
            });
            out.textures_delta.clear();
            txt.clear();
            for cs in &out.shapes {
                collect_all_text(&cs.shape, &mut txt);
            }
        }
        assert_eq!(app.selected, selected_before);
        assert!(txt.contains("Предпросмотр: Секретный чат"), "peek missing: {txt}");
        assert!(txt.contains("🔥 7"), "reaction pill missing: {txt}");
        assert!(txt.contains("+79995551122"), "contact card missing: {txt}");
        assert!(txt.contains("Кремль") && txt.contains("55.75580"), "geo card missing: {txt}");
        assert!(txt.contains("Отправить контакт") && txt.contains("Отправить геопозицию"));
    }

    #[test]
    fn message_row_and_contact_geo_serde_compat() {
        let legacy_json = r#"{"id":42,"text":"hi","outgoing":true,"date":1700000,"deleted":false,"edits":0,"media":null,"sender":"A","reply_to":null,"forwarded_from":null,"pinned":false,"edit_date":null,"views":null,"reactions":0,"sender_id":1,"entities":[],"grouped":null}"#;
        let row: MessageRow = serde_json::from_str(legacy_json).unwrap();
        assert_eq!(row.id, 42);
        assert!(row.reaction_items.is_empty());

        let contact = rg_core::media::MediaInfo::basic(
            rg_core::media::MediaKind::Contact {
                phone: "+79991112233".into(),
                first_name: "Анна".into(),
                last_name: "К".into(),
                user_id: 55,
            },
            "Анна К",
            0,
        );
        let back_c: rg_core::media::MediaInfo =
            serde_json::from_str(&serde_json::to_string(&contact).unwrap()).unwrap();
        assert_eq!(back_c, contact);

        let geo = rg_core::media::MediaInfo::basic(
            rg_core::media::MediaKind::Geo {
                lat_micro: 59_934_280,
                lon_micro: 30_335_099,
                title: "СПб".into(),
            },
            "СПб",
            0,
        );
        let back_g: rg_core::media::MediaInfo =
            serde_json::from_str(&serde_json::to_string(&geo).unwrap()).unwrap();
        assert_eq!(back_g, geo);
    }

    #[test]
    fn matte_backdrop_and_plane_mascot_have_no_circle_or_blue_orange_glow() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 844.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            theme::paint_android_glass_backdrop(&ctx, ui.max_rect(), c_orange());
            egui::CentralPanel::default()
                .frame(egui::Frame::NONE)
                .show(ui, |ui| {
                    login_anim::paint_plane_mascot(ui, true, 1.0);
                    login_anim::paint_keys_mascot(ui);
                });
        });
        out.textures_delta.clear();
        // На фоне нет текстурных градиентов/бликов и нет фоновых кругов за самолётиком.
        fn count_circles(shape: &egui::Shape, cnt: &mut usize) {
            match shape {
                egui::Shape::Circle(c) if c.radius >= 35.0 => *cnt += 1,
                egui::Shape::Vec(v) => v.iter().for_each(|s| count_circles(s, cnt)),
                _ => {}
            }
        }
        let mut big_circles = 0;
        for cs in &out.shapes {
            count_circles(&cs.shape, &mut big_circles);
        }
        assert_eq!(big_circles, 0, "expected no background circle behind mascot");
    }

    #[test]
    fn bot_empty_welcome_commands_popup_and_scrolled_fab_render_cleanly() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        // Выбираем бота без сообщений -> проверяем экран старта и карточку «Что умеет этот бот?».
        let bot_idx = app
            .dialogs
            .iter()
            .position(|d| d.kind == rg_core::engine::DialogKind::Bot)
            .unwrap_or_else(|| {
                let mut d = app.dialogs[0].clone();
                d.id = 999_001;
                d.kind = rg_core::engine::DialogKind::Bot;
                d.name = "Тест Бот".into();
                app.dialogs.push(d);
                app.dialogs.len() - 1
            });
        let bot_id = app.dialogs[bot_idx].id;
        app.selected = Some(bot_idx);
        app.messages.clear();
        app.peer_infos.insert(
            bot_id,
            rg_core::engine::PeerInfo {
                id: bot_id,
                name: "Тест Бот".into(),
                bot_description: "Описание тестового бота".into(),
                bot_commands: vec![
                    ("start".into(), "Запуск".into()),
                    ("menu".into(), "Открыть меню".into()),
                ],
                ..Default::default()
            },
        );
        let mut txt_empty = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 680.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::Panel::bottom("cmp").show(ui, |ui| app.composer_ui(ui));
                egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
            });
            out.textures_delta.clear();
            txt_empty.clear();
            out.shapes.iter().for_each(|cs| collect_all_text(&cs.shape, &mut txt_empty));
        }
        assert!(txt_empty.contains("Что умеет этот бот?"), "welcome missing: {txt_empty}");
        assert!(txt_empty.contains("Запустить бота (/start)"), "start bar missing: {txt_empty}");

        // С сообщениями: кнопка `≡ Меню`, попап команд, `scrolled_up` FAB + @ и ❤ стек.
        app.messages.push(MessageRow::plain(1, "Привет от бота".into(), false, app.now));
        app.cmd_popup_open = true;
        app.scrolled_up = true;
        if let Some(d) = app.dialogs.get_mut(bot_idx) {
            d.unread = 4;
            d.unread_mentions = 2;
            d.unread_reactions = 1;
        }
        let mut txt_full = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 680.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::Panel::bottom("cmp").show(ui, |ui| app.composer_ui(ui));
                egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
            });
            out.textures_delta.clear();
            txt_full.clear();
            out.shapes.iter().for_each(|cs| collect_all_text(&cs.shape, &mut txt_full));
        }
        assert!(txt_full.contains("Меню"), "left bot menu pill missing: {txt_full}");
        assert!(txt_full.contains("Команды бота") && txt_full.contains("/menu"), "bot cmd popup missing: {txt_full}");
        assert!(txt_full.contains('@') && txt_full.contains('❤'), "stacked FABs missing: {txt_full}");
    }

    #[test]
    fn pinned_bar_spans_full_width_and_unpins_only_on_server_event() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        let did = app.dialogs[0].id;
        app.selected = Some(0);
        let pin1 = MessageRow::plain(101, "Первый закреп".into(), false, app.now);
        let pin2 = MessageRow::plain(102, "Второй закреп".into(), false, app.now + 10);
        app.handle_event(&ctx, Ev::PinnedMessagesList(did, vec![pin1.clone(), pin2.clone()]));
        assert_eq!(app.pinned_lists.get(&did).map(Vec::len), Some(2));

        let mut bar_rect = egui::Rect::NOTHING;
        let mut txt = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 600.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::Panel::top("pinned-bar")
                    .frame(
                        egui::Frame::new()
                            .fill(t(SIDEBAR_BG))
                            .inner_margin(egui::Margin::symmetric(0, 0)),
                    )
                    .show(ui, |ui| {
                        bar_rect = ui.max_rect();
                        app.pinned_bar_ui(ui);
                    });
                egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
            });
            out.textures_delta.clear();
            txt.clear();
            out.shapes.iter().for_each(|cs| collect_all_text(&cs.shape, &mut txt));
        }
        assert!(bar_rect.width() >= 800.0, "pinned bar must span full chat width, got {}", bar_rect.width());
        assert!(txt.contains("Закреплённое сообщение"), "pinned title missing: {txt}");
        assert!(txt.contains("1 из 2") || txt.contains("#1"), "multi-pin counter missing: {txt}");

        // Циклический сдвиг индекса и окно подтверждения открепления до прихода Ev::Unpinned.
        app.pinned_cur_idx.insert(did, 1);
        app.confirm_unpin = Some((did, 102));
        let mut confirm_txt = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 600.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                app.confirm_unpin_modal(ui.ctx());
            });
            out.textures_delta.clear();
            confirm_txt.clear();
            out.shapes.iter().for_each(|cs| collect_all_text(&cs.shape, &mut confirm_txt));
        }
        assert!(confirm_txt.contains("Открепить сообщение?"), "confirm unpin modal missing: {confirm_txt}");
        assert!(
            app.pinned_lists.get(&did).is_some_and(|l| l.len() == 2),
            "pin list must not shrink before server Ev::Unpinned"
        );

        app.handle_event(&ctx, Ev::Unpinned(did, 102));
        assert_eq!(app.pinned_lists.get(&did).map(Vec::len), Some(1));
        app.handle_event(&ctx, Ev::Unpinned(did, 101));
        assert!(!app.pinned_msgs.contains_key(&did));
    }

    #[test]
    fn channel_discuss_bar_recent_repliers_and_paid_star_pill_render_faithfully() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        let ch_idx = app
            .dialogs
            .iter()
            .position(|d| d.kind == rg_core::engine::DialogKind::Channel)
            .unwrap_or_else(|| {
                let mut d = app.dialogs[0].clone();
                d.id = -1_001_555_666;
                d.kind = rg_core::engine::DialogKind::Channel;
                d.name = "Новости Rust".into();
                app.dialogs.push(d);
                app.dialogs.len() - 1
            });
        let ch_id = app.dialogs[ch_idx].id;
        app.selected = Some(ch_idx);
        app.peer_infos.insert(
            ch_id,
            rg_core::engine::PeerInfo {
                id: ch_id,
                name: "Новости Rust".into(),
                broadcast: true,
                linked_chat_id: Some(-1_001_777_888),
                paid_reactions_available: true,
                ..Default::default()
            },
        );
        let mut post = MessageRow::plain(501, "Релизная запись канала".into(), false, app.now);
        post.has_comments = true;
        post.replies_count = 12;
        post.recent_repliers = vec![1001, 1002, 1003];
        post.paid_reactions = 9;
        post.reaction_items = vec![rg_core::engine::ReactionPill {
            emoji: "⭐".into(),
            custom_doc_id: None,
            count: 9,
            chosen: false,
            paid: true,
        }];
        app.messages = vec![post];

        let mut txt = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 680.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::Panel::bottom("cmp").show(ui, |ui| app.composer_ui(ui));
                egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
            });
            out.textures_delta.clear();
            txt.clear();
            out.shapes.iter().for_each(|cs| collect_all_text(&cs.shape, &mut txt));
        }
        assert!(txt.contains("Обсудить"), "title-case channel Обсудить button missing: {txt}");
        assert!(!txt.contains("ОБСУДИТЬ"), "ALL-CAPS ОБСУДИТЬ must not be rendered: {txt}");
        assert!(txt.contains("12 комментариев"), "channel comments footer missing: {txt}");
    }

    #[test]
    fn unread_divider_voice_confirmation_and_no_lie_settings_audit() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        app.selected = Some(0);
        app.open_unread_anchor = Some(10);
        app.messages = vec![
            MessageRow::plain(10, "Прочитанное".into(), false, app.now - 60),
            MessageRow::plain(11, "Новое входящее".into(), false, app.now),
        ];
        app.settings.ayu.voice_confirmation = true;
        app.pending_voice_confirm = Some((
            std::path::PathBuf::from("voice_test.ogg"),
            4,
            vec![5, 12, 18],
            None,
        ));

        let mut txt = String::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 680.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::Panel::bottom("cmp").show(ui, |ui| app.composer_ui(ui));
                egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
            });
            out.textures_delta.clear();
            txt.clear();
            out.shapes.iter().for_each(|cs| collect_all_text(&cs.shape, &mut txt));
        }
        assert!(txt.contains("Непрочитанные сообщения"), "unread divider missing: {txt}");
        assert!(
            txt.contains("Отправить") || txt.contains("Голосовое"),
            "voice confirmation preview missing: {txt}"
        );

        let settings_src = concat!(include_str!("settings_ui.rs"), "\n", include_str!("tg_settings.rs"));
        for dead in [
            "local_premium",
            "use_solar_icons",
            "download_speed_boost",
            "upload_speed_boost",
            "archive_on_pull",
            "field_gift_button",
            "field_ai_editor_button",
            "drawer_bots",
            "disable_notifications_delay",
            "improve_link_previews",
            "round_confirmation",
            "disable_ads",
        ] {
            assert!(
                !settings_src.contains(dead),
                "No-Lie Audit failed: inert toggle `{dead}` still present in settings UI"
            );
        }
    }

    #[test]
    fn lang_choices_contain_zero_cjk_tofu() {
        let src = include_str!("tg_settings.rs");
        for bad in ["中文", "日本語", "한국어"] {
            assert!(!src.contains(bad), "unexpected CJK glyph string {bad} in tg_settings.rs");
        }
    }

    #[test]
    fn copy_text_to_clipboard_and_act_copy_update_status() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let _ = copy_text_to_clipboard(&ctx, "Проверка буфера");
        let mut app = demo_app("1");
        app.apply(&ctx, chat::Act::Copy("Тест сообщения".to_owned()));
        assert_eq!(app.status, "Скопировано в буфер обмена");
    }

    #[test]
    fn login_anim_mascots_render_sculpted_geometry_without_circles_on_compact_phone() {
        fn scan_shapes(shape: &egui::Shape, big_circles: &mut usize, paths: &mut usize) {
            match shape {
                egui::Shape::Circle(c) if c.radius >= 38.0 => *big_circles += 1,
                egui::Shape::Path(p) if p.points.len() >= 6 => *paths += 1,
                egui::Shape::Vec(v) => v.iter().for_each(|s| scan_shapes(s, big_circles, paths)),
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let raw = egui::RawInput {
            screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(340.0, 844.0))),
            ..Default::default()
        };
        let mut out = ctx.run_ui(raw, |ui| {
            egui::CentralPanel::default().show(ui, |ui| {
                login_anim::paint_plane_mascot(ui, false, 1.0);
                login_anim::paint_code_monkey(ui, 3, false);
                login_anim::paint_password_monkey(ui, true, 5);
            });
        });
        out.textures_delta.clear();
        let (mut big_circles, mut paths) = (0, 0);
        out.shapes.iter().for_each(|cs| scan_shapes(&cs.shape, &mut big_circles, &mut paths));
        assert_eq!(big_circles, 0, "expected zero circular backdrop behind login mascots");
        assert!(paths >= 3, "expected sculpted bezel/polygon paths on login mascots, got {paths}");
    }

    #[test]
    fn reaction_pills_render_color_emoji_atlas_quads_and_paid_star_mesh() {
        fn inspect_shapes(shape: &egui::Shape, emoji_meshes: &mut usize, star_meshes: &mut usize, txt: &mut String, atlas_id: egui::TextureId) {
            match shape {
                egui::Shape::Mesh(m) if m.texture_id == atlas_id => *emoji_meshes += 1,
                egui::Shape::Rect(r) if r.fill_texture_id() == atlas_id => *emoji_meshes += 1,
                egui::Shape::Mesh(m) if m.indices.len() == 30 => *star_meshes += 1,
                egui::Shape::Text(t) => {
                    txt.push_str(t.galley.text());
                    txt.push('\n');
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        inspect_shapes(s, emoji_meshes, star_meshes, txt, atlas_id);
                    }
                }
                _ => {}
            }
        }
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let loaded_emoji = emoji::Emoji::load(&ctx).expect("bundled Twemoji atlas must decode");
        let atlas_id = loaded_emoji.tex_id();
        let mut app = demo_app("1");
        app.emoji = Some(loaded_emoji);
        app.selected = Some(0);
        let mut msg = MessageRow::plain(77, "Проверка реакций".into(), false, app.now);
        msg.reactions = 5;
        msg.paid_reactions = 9;
        msg.reaction_items = vec![
            rg_core::engine::ReactionPill {
                emoji: "👍".into(),
                custom_doc_id: None,
                count: 5,
                chosen: true,
                paid: false,
            },
            rg_core::engine::ReactionPill {
                emoji: "⭐".into(),
                custom_doc_id: None,
                count: 9,
                chosen: false,
                paid: true,
            },
        ];
        app.messages = vec![msg];

        let mut out_shapes = Vec::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 680.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
            });
            out.textures_delta.clear();
            out_shapes = out.shapes;
        }
        let (mut emoji_meshes, mut star_meshes, mut txt) = (0, 0, String::new());
        for cs in &out_shapes {
            inspect_shapes(&cs.shape, &mut emoji_meshes, &mut star_meshes, &mut txt, atlas_id);
        }
        assert!(emoji_meshes >= 1, "reaction pill must render color emoji from Emoji atlas");
        assert!(star_meshes >= 1, "paid star reaction pill must render gold star mesh");
        assert!(txt.contains('5') && txt.contains('9'), "reaction counters missing: {txt}");
        assert!(!txt.contains("👍 5"), "monochrome raw emoji fallback must not be used when Emoji atlas loaded");
    }

    #[test]
    fn request_previews_queues_custom_emoji_docs_from_reaction_items() {
        let (cmd, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        let mut app = demo_app("1");
        app.cmd = cmd;
        let mut msg = MessageRow::plain(88, "Канальный пост".into(), false, app.now);
        msg.reaction_items = vec![rg_core::engine::ReactionPill {
            emoji: String::new(),
            custom_doc_id: Some(9_001_002),
            count: 3,
            chosen: false,
            paid: false,
        }];
        let rows = vec![msg];
        app.request_reaction_docs(&rows);
        let mut found = false;
        while let Ok(c) = cmd_rx.try_recv() {
            if let Cmd::CustomEmojiDocs(ids) = c
                && ids.contains(&9_001_002)
            {
                found = true;
            }
        }
        assert!(found, "request_reaction_docs must queue Cmd::CustomEmojiDocs for pill custom_doc_id");
    }

    #[test]
    fn optimistic_reaction_toggle_updates_pills_and_records_pop_time() {
        let ctx = egui::Context::default();
        let mut app = demo_app("1");
        let mut msg = MessageRow::plain(91, "Тест клика по реакции".into(), false, app.now);
        msg.reaction_items = vec![rg_core::engine::ReactionPill {
            emoji: "🔥".into(),
            custom_doc_id: None,
            count: 4,
            chosen: false,
            paid: false,
        }];
        app.messages = vec![msg];
        app.apply(&ctx, chat::Act::ReactToggle(91, "🔥".into(), false));
        let pill = &app.messages[0].reaction_items[0];
        assert!(pill.chosen && pill.count == 5, "optimistic ReactToggle should mark chosen and increment count");
        assert!(app.rx_pop.contains_key(&(91, "🔥".to_owned())), "rx_pop must record click timestamp");

        app.apply(&ctx, chat::Act::ReactToggle(91, "🔥".into(), true));
        let pill = &app.messages[0].reaction_items[0];
        assert!(!pill.chosen && pill.count == 4, "second ReactToggle should unchoose and decrement count");
    }

    #[test]
    fn mobile_liquid_glass_tab_bar_and_animated_panels_render_on_phone_viewport() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        app.selected = None;
        app.status = "Скопировано в буфер обмена".into();
        let mut out_shapes = Vec::new();
        for _ in 0..2 {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(390.0, 844.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::Panel::bottom("mobile-glass-tabs")
                    .frame(egui::Frame::NONE.inner_margin(egui::Margin::ZERO))
                    .show(ui, |ui| {
                        glass::floating_tab_bar(
                            ui,
                            "mob-nav",
                            &[
                                ("menu/chats", "Чаты", 3),
                                ("menu/archive", "Архив", 1),
                                ("folders/folders_type_contacts", "Контакты", 0),
                                ("menu/settings", "Настройки", 0),
                            ],
                            0,
                        )
                    });
                egui::CentralPanel::default()
                    .frame(egui::Frame::new().fill(tint_panel()))
                    .show(ui, |ui| app.dialogs_ui(ui));
                glass::paint_toast(&ctx, &app.status, false, 1.0);
            });
            out.textures_delta.clear();
            out_shapes = out.shapes;
        }
        let mut txt = String::new();
        fn collect(s: &egui::Shape, buf: &mut String) {
            match s {
                egui::Shape::Text(t) => {
                    buf.push_str(t.galley.text());
                    buf.push('\n');
                }
                egui::Shape::Vec(v) => v.iter().for_each(|x| collect(x, buf)),
                _ => {}
            }
        }
        out_shapes.iter().for_each(|cs| collect(&cs.shape, &mut txt));
        assert!(
            txt.contains("Чаты") && txt.contains("Архив") && txt.contains("Контакты") && txt.contains("Настройки"),
            "mobile Liquid Glass tabs missing: {txt}"
        );
        assert!(txt.contains("Скопировано в буфер обмена"), "floating glass toast text missing: {txt}");
    }

    #[test]
    fn rapid_chat_switch_and_reorder_send_to_exact_selected_dialog_id() {
        let ctx = egui::Context::default();
        let mut app = demo_app("1");
        let (cmd, mut cmd_rx) = tokio::sync::mpsc::unbounded_channel();
        app.cmd = cmd;
        assert!(app.dialogs.len() >= 2);
        let did1 = app.dialogs[0].id;
        let did2 = app.dialogs[1].id;
        assert_ne!(did1, did2);

        app.open_dialog(0);
        while cmd_rx.try_recv().is_ok() {}

        // Симулируем переключение на второй чат со свежим кэшем (<1500мс).
        app.history_cache.insert(did2, vec![MessageRow::plain(1, "cached".into(), false, app.now)]);
        app.history_fresh_at.insert(did2, std::time::Instant::now());
        let pos2 = app.dialogs.iter().position(|d| d.id == did2).unwrap();
        app.open_dialog(pos2);
        assert_eq!(app.selected_dialog_id(), Some(did2));

        // Сервер переупорядочивает список диалогов прямо перед отправкой (пришло сообщение в did2 -> вверх).
        let mut reordered = app.dialogs.clone();
        reordered.swap(0, 1);
        app.handle_event(&ctx, Ev::Dialogs(reordered));
        assert_eq!(app.dialogs[0].id, did2);

        let cur_did = app.selected_dialog_id().unwrap();
        assert_eq!(cur_did, did2);
        let tid = app.push_optimistic_message("/start".to_owned(), Vec::new(), None, None);
        app.send(Cmd::Send(cur_did, "/start".to_owned(), None, tid));

        let mut saw_open_did2 = false;
        let mut saw_send_did2 = false;
        while let Ok(c) = cmd_rx.try_recv() {
            match c {
                Cmd::Open(id, fetch) => {
                    assert_eq!(id, did2, "Open must switch worker to exact dialog_id");
                    assert!(!fetch, "fresh cache open must still switch worker with fetch=false");
                    saw_open_did2 = true;
                }
                Cmd::Send(id, txt, _, _) => {
                    assert_eq!(id, did2, "Send must never route to stale dialog did1={did1}");
                    assert_eq!(txt, "/start");
                    saw_send_did2 = true;
                }
                _ => {}
            }
        }
        assert!(saw_open_did2 && saw_send_did2, "expected both Cmd::Open(did2, false) and Cmd::Send(did2, ..)");
    }

    #[test]
    fn dialogs_active_indicator_slides_vertically_between_chats() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        assert!(app.dialogs.len() >= 3);

        let run_frame = |app: &mut App, t: f64| {
            let raw = egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(340.0, 640.0))),
                time: Some(t),
                predicted_dt: 0.04,
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.dialogs_ui(ui));
            });
            out.textures_delta.clear();
        };

        // Кадр 1: активен чат 0 -> индикатор строго на 0.0
        app.open_dialog(0);
        run_frame(&mut app, 0.0);
        let y0 = ctx.animate_value_with_time(egui::Id::NULL.with("dialogs-slide-row"), 0.0, 0.22);
        assert!((y0 - 0.0).abs() < 0.01);

        // Переключаемся на чат 2: через 1 шаг индикатор должен плавно скользить между 0 и 2 (без телепорта)
        app.open_dialog(2);
        run_frame(&mut app, 0.04);
        let mid_y = ctx.animate_value_with_time(egui::Id::NULL.with("dialogs-slide-row"), 2.0, 0.22);
        assert!(mid_y > 0.05 && mid_y < 1.95, "indicator must glide between chats smoothly, got {mid_y}");

        // Через серию кадров индикатор достигает целевой строки 2.0
        for step in 2..=12 {
            run_frame(&mut app, step as f64 * 0.04);
        }
        let end_y = ctx.animate_value_with_time(egui::Id::NULL.with("dialogs-slide-row"), 2.0, 0.22);
        assert!((end_y - 2.0).abs() < 0.15, "indicator should settle near target row 2.0, got {end_y}");
    }

    fn collect_shape_text(s: &egui::Shape, out: &mut String) {
        match s {
            egui::Shape::Text(t) => {
                out.push_str(t.galley.text());
                out.push('\n');
            }
            egui::Shape::Vec(v) => v.iter().for_each(|x| collect_shape_text(x, out)),
            _ => {}
        }
    }

    #[test]
    fn notification_center_popover_renders_feed_and_read_all() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        app.notify_popover_open = true;
        app.notify_feed.push_front((100, "Алиса".to_owned(), "Привет! Тестовое уведомление".to_owned(), 1_760_000_000, false));
        let mut out_shapes = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(420.0, 720.0))),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| app.dialogs_ui(ui));
                },
            );
            out.textures_delta.clear();
            out_shapes = out.shapes;
        }
        let mut txt = String::new();
        out_shapes.iter().for_each(|cs| collect_shape_text(&cs.shape, &mut txt));
        assert!(txt.contains("Уведомления"), "expected Уведомления header in {txt}");
        assert!(txt.contains("Прочитать все"), "expected Прочитать все action in {txt}");
        assert!(txt.contains("Алиса") && txt.contains("Привет! Тестовое уведомление"));
    }

    #[test]
    fn apple_spotlight_palette_renders_quick_actions() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        app.palette_open = true;
        let mut out_shapes = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(960.0, 720.0))),
                    ..Default::default()
                },
                |_ui| app.quick_switcher_window(&ctx),
            );
            out.textures_delta.clear();
            out_shapes = out.shapes;
        }
        let mut txt = String::new();
        out_shapes.iter().for_each(|cs| collect_shape_text(&cs.shape, &mut txt));
        assert!(txt.contains("Призрак") && txt.contains("Черновики"), "expected Spotlight shortcuts in {txt}");
    }

    #[test]
    fn multi_select_bar_renders_save_to_saved_and_batch_delete() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        app.selected = Some(0);
        if app.messages.is_empty() {
            app.messages.push(MessageRow::plain(101, "Тест пакета".to_owned(), true, app.now));
        }
        if !app.dialogs.iter().any(|d| d.saved) && let Some(mut d) = app.dialogs.first().cloned() {
            d.id = 999_001;
            d.saved = true;
            d.name = "Избранное".to_owned();
            app.dialogs.push(d);
        }
        let target_id = app.messages.first().map(|m| m.id).unwrap();
        app.selected_msgs.insert(target_id);
        let mut out_shapes = Vec::new();
        for _ in 0..2 {
            let mut out = ctx.run_ui(
                egui::RawInput {
                    screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(900.0, 700.0))),
                    ..Default::default()
                },
                |ui| {
                    egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
                },
            );
            out.textures_delta.clear();
            out_shapes = out.shapes;
        }
        let mut txt = String::new();
        out_shapes.iter().for_each(|cs| collect_shape_text(&cs.shape, &mut txt));
        assert!(txt.contains("Удалить"), "expected Удалить batch button in {txt}");
        assert!(txt.contains("В Избранное"), "expected В Избранное batch button in {txt}");
        assert!(txt.contains("Выбрано: 1"), "expected selection count in {txt}");
    }

    fn has_vibrant_color(s: &egui::Shape) -> bool {
        let non_gray = |c: Color32| {
            let max_c = c.r().max(c.g()).max(c.b());
            let min_c = c.r().min(c.g()).min(c.b());
            c.a() > 40 && max_c.saturating_sub(min_c) >= 40
        };
        match s {
            egui::Shape::Text(t) => non_gray(t.fallback_color) || t.galley.job.sections.iter().any(|sec| non_gray(sec.format.color)),
            egui::Shape::LineSegment { stroke, .. } => non_gray(stroke.color),
            egui::Shape::Rect(r) => non_gray(r.fill) || non_gray(r.stroke.color),
            egui::Shape::Vec(v) => v.iter().any(has_vibrant_color),
            _ => false,
        }
    }

    #[test]
    fn peer_nicknames_and_reply_bars_keep_telegram_colors() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        if let Some(d) = app.dialogs.first_mut() {
            d.kind = rg_core::engine::DialogKind::Group;
            d.peer_color = Some(0);
        }
        app.selected = Some(0);
        let mut msg = MessageRow::plain(777, "Сообщение с цветным ником и ответом".to_owned(), false, app.now);
        msg.sender_id = 42;
        msg.sender = "Борис".to_owned();
        msg.sender_color = Some(3);
        msg.reply_to = Some((776, "Исходный текст".to_owned()));
        msg.forwarded_from = Some("Алиса".to_owned());
        app.messages.push(msg);

        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(820.0, 620.0))),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| app.messages_ui(ui));
            },
        );
        out.textures_delta.clear();
        assert!(
            out.shapes.iter().any(|cs| has_vibrant_color(&cs.shape)),
            "expected vibrant Telegram peer colors on nicknames / reply bars"
        );
    }

    #[test]
    fn folder_reveal_and_gradual_blur_render_cleanly() {
        let ctx = egui::Context::default();
        theme::apply(&ctx);
        let mut app = demo_app("1");
        app.selected = Some(0);
        for k in 0..30 {
            app.messages.push(MessageRow::plain(900 + k, format!("Строка прокрутки #{k}"), k % 2 == 0, app.now + i64::from(k)));
        }
        app.scrolled_up = true;
        let mut out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(egui::Rect::from_min_size(egui::Pos2::ZERO, Vec2::new(760.0, 620.0))),
                ..Default::default()
            },
            |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    app.folder_tabs(ui);
                    app.messages_ui(ui);
                });
            },
        );
        out.textures_delta.clear();
    }
}
