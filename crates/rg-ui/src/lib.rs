//! Общий интерфейс Rustgramm для ПК и Android в стиле Telegram Desktop (AyuGram v7.2.9),
//! синий везде заменён на оранжевый. Данные уходят только на серверы Telegram (см. `rg-net`).

pub mod android_jni;
mod audio_io;
mod chat;
mod countries;
pub mod emoji;
mod features_ui;
mod frame;
pub mod icons;
mod kit;
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

use std::collections::{HashMap, HashSet};
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

struct App {
    cmd: UnboundedSender<Cmd>,
    ev: Receiver<Ev>,
    stage: Stage,
    me: String,
    phone: String,
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
    qr_cache: Option<(String, QrImage)>,
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
    login_country_open: bool,
    login_country_query: String,
    login_busy: bool,
    login_last_phone: String,
    login_code_sent_at: Option<std::time::Instant>,
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
    filtered_cache_key: (u64, String, bool, Option<i32>),
    filtered_cache: Vec<usize>,
    archived_total_cache: usize,
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

/// Готовая сетка QR: размер стороны и тёмные модули.
struct QrImage {
    side: usize,
    dark: Vec<bool>,
}

fn qr_image(url: &str) -> Option<QrImage> {
    let code = qrcode::QrCode::new(url.as_bytes()).ok()?;
    Some(QrImage {
        side: code.width(),
        dark: code
            .to_colors()
            .into_iter()
            .map(|c| c == qrcode::Color::Dark)
            .collect(),
    })
}

fn primary_button(ui: &mut egui::Ui, text: &str) -> egui::Response {
    orange_button(ui, text, Vec2::new(280.0, 40.0))
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
            phone: String::new(),
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
            qr_cache: None,
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
                let mut reg = rg_ayu::plugins::PluginRegistry::with_builtins();
                let _ = reg.load_dir(&worker::home().join("plugins"));
                reg
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
            login_country_open: false,
            login_country_query: String::new(),
            login_busy: false,
            login_last_phone: String::new(),
            login_code_sent_at: None,
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
            filtered_cache_key: (u64::MAX, String::new(), false, None),
            filtered_cache: Vec::new(),
            archived_total_cache: 0,
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
        }
    }

    fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }

    /// Предпросмотры только для свежих вложений и по одному разу.
    /// Событие из потока обновлений: сообщения доходят сразу, без перезахода в чат.
    /// Обновляет строку чата в списке по новому сообщению. Возвращает `true`, если нужен полный
    /// перезапрос списка (чат не найден или должен сменить место).
    fn apply_live_to_list(&mut self, key: i64, row: &(i64, bool, String), is_open: bool) -> bool {
        let Some(pos) = self.dialogs.iter().position(|d| d.id == key) else {
            return true;
        };
        let (date, outgoing, text) = row;
        let old = self.dialogs[pos].clone();
        let newer = *date > old.date;
        let d = &mut self.dialogs[pos];
        if newer || *date == old.date {
            d.last.clone_from(text);
        }
        if newer {
            d.date = *date;
            if !outgoing && !is_open {
                d.unread += 1;
            }
        }
        if !newer || old.pinned {
            return false;
        }
        // Уже самый верхний среди незакреплённых той же папки (архив или основной список)?
        self.dialogs
            .iter()
            .any(|o| o.id != key && !o.pinned && o.archived == old.archived && o.date >= old.date)
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
                let row_snapshot = (row.date, row.outgoing, row.text.chars().take(80).collect::<String>());
                if open_id == Some(key) {
                    let idx = self.selected.unwrap_or(0);
                    let now_t = ctx.input(|i| i.time);
                    if self.known.insert(row.id) {
                        self.arrived.insert(row.id, now_t);
                    }
                    self.request_previews(idx, std::slice::from_ref(&row));
                    let incoming = !row.outgoing;
                    match self.messages.iter_mut().find(|m| m.id == row.id) {
                        Some(old) => {
                            let (deleted, edits) = (old.deleted, old.edits);
                            *old = row;
                            old.deleted = deleted;
                            old.edits = edits + usize::from(old.edit_date.is_some());
                        }
                        None => {
                            self.messages.push(row);
                            self.messages.sort_by_key(|m| (m.date, m.id));
                        }
                    }
                    // Окно в фокусе и чат открыт: отмечаем прочитанным (режим призрака учитывается).
                    if incoming && ctx.input(|i| i.focused) {
                        self.send(Cmd::MarkRead);
                    }
                }
                // Строка списка обновляется на месте; полный перезапрос (300 чатов + архив)
                // нужен, только если чат должен подняться вверх или его нет в списке.
                if self.apply_live_to_list(key, &row_snapshot, open_id == Some(key)) {
                    self.refresh_at.get_or_insert(std::time::Instant::now());
                }
                // Уведомление по настройкам (Windows Toast или встроенные карточки + мигание + звук).
                if !row_snapshot.1 && !ctx.input(|i| i.focused) {
                    let dialog = self.dialogs.iter().find(|d| d.id == key);
                    let muted = dialog.is_some_and(|d| d.muted);
                    let cooldown_ok = self.last_toast.is_none_or(|t| t.elapsed().as_secs() >= 2);
                    if !muted && cooldown_ok {
                        self.last_toast = Some(std::time::Instant::now());
                        let title = dialog.map_or("Rustgramm".to_owned(), |d| d.name.clone());
                        let body = if row_snapshot.2.is_empty() {
                            "Вложение".to_owned()
                        } else {
                            row_snapshot.2.clone()
                        };
                        self.push_notification(ctx, &title, &body);
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
                self.refresh_at.get_or_insert(std::time::Instant::now());
            }
            Live::ReadOutbox(key, max) => {
                if let Some(d) = self.dialogs.iter_mut().find(|d| d.id == key) {
                    d.read_out = d.read_out.max(max);
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
                self.push_notification(ctx, "Секретный чат (E2E)", "Новое зашифрованное сообщение");
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
                        Color32::WHITE,
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
                Color32::WHITE,
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
            icons::tg(ui.painter(), "settings/lock", ir.center(), Color32::WHITE);
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
        // В больших группах (1000+ участников) подгружаем аватарки порциями (≤ 24 уникальных),
        // чтобы открытие чата не создавало лавину сетевых запросов.
        if self.dialogs.get(idx).is_some_and(|d| d.id < 0) {
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
                let _ = self.cmd.send(Cmd::Preview(idx, id));
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
            .take(8)
            .map(|r| r.id)
            .collect();
        for id in tgs_want {
            if !self.tgs_msgs.contains_key(&id) && self.tgs_msg_req.insert(id) {
                let _ = self.cmd.send(Cmd::TgsMessage(idx, id));
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
            let _ = self.cmd.send(Cmd::CachedMedia(videos));
        }
        // Короткие GIF до 4 МБ подгружаются в фоне для мгновенного автозапуска в ленте.
        if self.settings.auto_play_gifs {
            let gifs: Vec<i32> = rows
                .iter()
                .rev()
                .filter(|r| {
                    r.media.as_ref().is_some_and(|x| {
                        x.kind == rg_core::media::MediaKind::Gif && x.size > 0 && x.size <= 4 * 1024 * 1024
                    })
                })
                .take(4)
                .map(|r| r.id)
                .collect();
            for id in gifs {
                if !self.gif_paths.contains_key(&id) && self.gif_auto_req.insert(id) {
                    let _ = self.cmd.send(Cmd::AutoFetchGif(id));
                }
            }
        }
        // Загружаем текстуры всех пользовательских Premium-эмодзи из сообщений.
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
        }
        if !custom_ids.is_empty() {
            let _ = self.cmd.send(Cmd::CustomEmojiDocs(custom_ids));
        }
    }

    /// Открыть чат из списка (мгновенно из кэша в памяти, если уже открывался).
    fn open_dialog(&mut self, i: usize) {
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
        if let Some(id) = self.dialogs.get(i).map(|d| d.id) {
            if self.info_req.insert(id) {
                self.send(Cmd::PeerInfo(i));
            }
            if let Some(cached) = self.history_cache.get(&id).cloned()
                && !cached.is_empty()
            {
                self.known.extend(cached.iter().map(|r| r.id));
                self.request_previews(i, &cached);
                self.messages = cached;
                self.history_loading = false;
            } else {
                self.history_loading = true;
            }
        } else {
            self.history_loading = true;
        }
        self.send(Cmd::Open(i));
    }

    /// Закрыть открытый чат и сбросить состояние ленты.
    fn close_chat(&mut self) {
        self.selected = None;
        self.reset_chat_state();
    }

    fn reset_chat_state(&mut self) {
        self.cached_media.clear();
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
        self.search_open = false;
        self.chat_search.clear();
        self.hidden.clear();
        self.older_pending = false;
        self.history_end = false;
        self.scroll_to = None;
        self.highlight_msg = None;
        self.autoscroll_anchor = None;
        self.active_topic = None;
        self.topic_msgs = None;
        self.discussion = None;
        self.comment_input.clear();
        self.user_filter = None;
        self.cmd_popup_open = false;
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

    fn current_tray_state(&self) -> tray::TrayState {
        let mut unread = 0i32;
        let mut unmuted_unread = 0i32;
        for d in &self.dialogs {
            let add = if self.settings.notify.badge_count_messages {
                d.unread
            } else {
                i32::from(d.unread > 0)
            };
            if !d.muted {
                unmuted_unread += add;
                unread += add;
            } else if self.settings.notify.badge_include_muted {
                unread += add;
            }
        }
        tray::TrayState {
            enabled: self.settings.work_mode != rg_settings::WorkMode::WindowOnly,
            window_visible: !self.hidden_to_tray && win::is_window_visible(),
            icon_idx: self.settings.ayu.app_icon.parse().unwrap_or(0),
            unread,
            unread_muted: unread > 0 && unmuted_unread == 0,
            hide_badge: self.settings.ayu.hide_notification_badge,
            desktop_notify: self.settings.notify.desktop_notifications,
            ghost_active: self.settings.ghost.active,
            streamer_mode: self.settings.ayu.streamer_mode,
            show_ghost_toggle: self.settings.ayu.tray_ghost_toggle,
            show_streamer_toggle: self.settings.ayu.tray_streamer_toggle,
        }
    }

    fn sync_tray_and_window(&mut self, ctx: &egui::Context) {
        win::set_capture_excluded(self.settings.ayu.streamer_mode);
        let st = self.current_tray_state();
        if !self.is_demo {
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
        if self.tgs_frame_cache.len() > 750 {
            self.tgs_frame_cache.clear();
        }
        let speed = f64::from(self.settings.anim_speed) / 100.0;
        let t = if speed <= 0.01 || !focused {
            0.0
        } else {
            ctx.input(|i| i.time) * speed
        };
        let mut active = false;

        // Обновляем анимированные Custom Emoji / стикеры панели (`tgs_docs`):
        // если панель эмодзи закрыта, рендерим только 0-й кадр один раз (0% нагрузки CPU в простое!).
        for (&doc_id, anim) in &self.tgs_docs {
            let bucket = if self.emoji_open && focused {
                let f = anim.frame_at_time(t);
                active = true;
                (f.round() as usize / 4) * 4
            } else {
                0
            };
            if bucket == 0 && self.sticker_tex.contains_key(&doc_id) && !self.emoji_open {
                continue;
            }
            let key = (doc_id, bucket);
            let tex = self.tgs_frame_cache.entry(key).or_insert_with(|| {
                let img = anim.render_frame(bucket as f32, 48);
                ctx.load_texture(format!("tgsd_{doc_id}_{bucket}"), img, egui::TextureOptions::LINEAR)
            });
            self.sticker_tex.insert(doc_id, tex.clone());
        }

        // Обновляем анимированные TGS-стикеры в открытом чате (`tgs_msgs`, шаг 4 кадра ≈ 15 FPS).
        if focused {
            for (&msg_id, anim) in &self.tgs_msgs {
                let f = anim.frame_at_time(t);
                let bucket = (f.round() as usize / 4) * 4;
                let key = (-i64::from(msg_id) - 1_000_000, bucket);
                let tex = self.tgs_frame_cache.entry(key).or_insert_with(|| {
                    let img = anim.render_frame(bucket as f32, 128);
                    ctx.load_texture(format!("tgsm_{msg_id}_{bucket}"), img, egui::TextureOptions::LINEAR)
                });
                self.textures.insert(msg_id, tex.clone());
                active = true;
            }
        }

        if active && focused && speed > 0.01 {
            ctx.request_repaint_after(std::time::Duration::from_millis(66));
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
        while let Ok(e) = self.ev.try_recv() {
            match e {
                Ev::Stage(s) => {
                    self.stage = s;
                    self.error.clear();
                }
                Ev::Me(n) => self.me = n,
                Ev::Dialogs(d) => {
                    // Открытый чат ищем по идентификатору: порядок строк мог измениться.
                    let keep = self.selected.and_then(|i| self.dialogs.get(i)).map(|x| x.id);
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
                        for row in batch {
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
                }
                Ev::Contacts(c) => self.contacts = c,
                Ev::Select(id) => {
                    if let Some(i) = self.dialogs.iter().position(|d| d.id == id) {
                        self.in_archive = self.dialogs[i].archived;
                        self.open_dialog(i);
                    }
                }
                Ev::StickerSets(s) => {
                    self.sticker_sets = s;
                    self.sticker_cur = 0;
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
                    ctx.copy_text(link.clone());
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
                    if self.tgs_docs.len() > 80 {
                        self.tgs_docs.clear();
                    }
                    if let Some(anim) = lottie::TgsAnim::from_bytes(&bytes) {
                        self.tgs_docs.insert(id, anim);
                    }
                }
                Ev::TgsMsg(msg_id, bytes) => {
                    if self.tgs_msgs.len() > 30 {
                        self.tgs_msgs.clear();
                    }
                    if let Some(anim) = lottie::TgsAnim::from_bytes(&bytes) {
                        self.tgs_msgs.insert(msg_id, anim);
                    }
                }
                Ev::Gifs(g) => self.gif_list = g,
                Ev::PeerInfo(id, info) => {
                    if let Some(doc_id) = info.emoji_status
                        && !self.sticker_tex.contains_key(&doc_id)
                        && self.custom_emoji_req.insert(doc_id)
                    {
                        self.send(Cmd::CustomEmojiDocs(vec![doc_id]));
                    }
                    if let Some(idx) = self.selected.filter(|&i| self.dialogs.get(i).is_some_and(|d| d.id == id)) {
                        if let Some(pid) = info.pinned_msg_id
                            && self.pinned_msgs.get(&id).is_none_or(|m| m.id != pid)
                            && self.pinned_req.insert((id, pid))
                        {
                            self.send(Cmd::LoadPinned(idx, pid));
                        }
                        if info.forum
                            && !self.forum_topics.contains_key(&id)
                            && self.forum_req.insert(id)
                        {
                            self.send(Cmd::ForumTopics(idx));
                        }
                    }
                    self.peer_infos.insert(id, info);
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
                    self.discussion = Some(thread);
                }
                Ev::Stories(list) => {
                    self.stories = list;
                }
                Ev::PinnedMsg(id, msg) => {
                    self.pinned_msgs.insert(id, msg);
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
                        self.in_archive = self.dialogs[pos].archived;
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
                    self.me.clear();
                    self.stage = Stage::Starting;
                    self.phone.clear();
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
                    // Ограничение кэша текстур стикеров/эмодзи в ОЗУ (до 260 шт.).
                    if self.sticker_tex.len() > 260 {
                        let keep: HashSet<i64> = self.dialogs.iter().filter_map(|d| d.emoji_status).collect();
                        self.sticker_tex.retain(|k, _| keep.contains(k));
                        self.sticker_req.retain(|k| self.sticker_tex.contains_key(k));
                        self.custom_emoji_req.retain(|k| self.sticker_tex.contains_key(k));
                    }
                    let tex = ctx.load_texture(format!("s{id}"), *img, egui::TextureOptions::LINEAR);
                    self.sticker_tex.insert(id, tex);
                }
                Ev::History(idx, m) if self.selected == Some(idx) => {
                    // Плавная каскадная загрузка сообщений как на iPhone.
                    let now_t = ctx.input(|i| i.time);
                    if self.known.is_empty() {
                        let tail = m.len().saturating_sub(12);
                        for (k, r) in m[tail..].iter().enumerate() {
                            self.arrived.insert(r.id, now_t + k as f64 * 0.016);
                        }
                    } else {
                        for r in &m {
                            if !self.known.contains(&r.id) {
                                self.arrived.insert(r.id, now_t);
                            }
                        }
                    }
                    self.known.extend(m.iter().map(|r| r.id));
                    self.request_previews(idx, &m);
                    if let Some(id) = self.dialogs.get(idx).map(|d| d.id) {
                        // Держим в ОЗУ кэш не более 16 последних чатов (остальное — на диске).
                        if self.history_cache.len() >= 16 && !self.history_cache.contains_key(&id)
                            && let Some(&drop_key) = self.history_cache.keys().next()
                        {
                            self.history_cache.remove(&drop_key);
                        }
                        self.history_cache.insert(id, m.clone());
                    }
                    self.messages = m;
                    self.history_loading = false;
                    self.older_pending = false;
                }
                Ev::Older(idx, rows) if self.selected == Some(idx) => {
                    self.older_pending = false;
                    if rows.is_empty() {
                        self.history_end = true;
                    } else {
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
                Ev::Found(idx, rows) if self.selected == Some(idx) => {
                    self.request_previews(idx, &rows);
                    self.found = Some(rows);
                }
                Ev::Found(..) => {}
                Ev::EditHistory(id, v) => self.edit_history = Some((id, v)),
                Ev::History(..) => {}
                Ev::Image(idx, id, img) if self.selected == Some(idx) => {
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
                Ev::StoryImage(sid, img) => {
                    let tex = ctx.load_texture(format!("story-{sid}"), *img, egui::TextureOptions::LINEAR);
                    self.story_textures.insert(sid, tex);
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
                Ev::Status(s) => {
                    self.status = s;
                    self.bot_btn_pending = None;
                }
                Ev::Error(e) => {
                    self.error = e;
                    self.error_at = self.now;
                    self.bot_btn_pending = None;
                }
            }
        }
    }

    // ---------- вход ----------

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        // Верхняя полоса с кнопкой «НАСТРОЙКИ» (не перекрывает статус-бар Android и не уезжает за край).
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            if self.login_busy {
                ui.add(egui::Spinner::new().size(15.0).color(ORANGE));
                ui.label(RichText::new("Связь с Telegram…").size(12.5).color(SUBTEXT));
            }
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.add_space(16.0);
                if ui
                    .add(
                        egui::Button::new(RichText::new("НАСТРОЙКИ").size(13.5).strong().color(ACCENT))
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
                ui.add_space((ui.available_height() * 0.05).max(10.0));
                match self.stage.clone() {
                    Stage::Qr(url) => self.qr_screen(ui, &url),
                    _ => self.form_screen(ui),
                }
                if !self.error.is_empty() {
                    ui.add_space(12.0);
                    egui::Frame::new()
                        .fill(Color32::from_rgba_unmultiplied(220, 68, 68, 36))
                        .stroke(egui::Stroke::new(1.0, DANGER.gamma_multiply(0.6)))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::symmetric(12, 8))
                        .show(ui, |ui| {
                            ui.set_max_width(300.0);
                            ui.label(RichText::new(&self.error).size(12.5).color(Color32::from_rgb(255, 165, 165)));
                        });
                }
                // Если уже есть другой авторизованный аккаунт, можно отменить вход и вернуться обратно.
                if let Some(prev) = self
                    .settings
                    .accounts
                    .iter()
                    .find(|a| a.slot != self.settings.active_account && a.is_authorized_hint())
                    .cloned()
                {
                    ui.add_space(14.0);
                    if link(ui, &format!("← Вернуться к {}", prev.title())).clicked() {
                        let cur = self.settings.active_account;
                        if cur > 0 {
                            self.send(Cmd::RemoveAccountSlot(cur));
                        } else {
                            self.send(Cmd::SwitchAccount(prev.slot));
                        }
                    }
                }
                ui.add_space(20.0);
                ui.label(
                    RichText::new(format!("Rustgramm v{}", env!("CARGO_PKG_VERSION")))
                        .color(SUBTEXT)
                        .size(12.5),
                );
                ui.add_space(18.0);
            });
        });
        self.proxy_window(ui.ctx());
    }

    fn qr_screen(&mut self, ui: &mut egui::Ui, url: &str) {
        let size = 188.0;
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        ui.painter()
            .rect_filled(rect, egui::CornerRadius::same(12), Color32::WHITE);
        if url.is_empty() {
            ui.put(rect, egui::Spinner::new().color(ACCENT));
        } else {
            if self.qr_cache.as_ref().is_none_or(|(u, _)| u != url) {
                self.qr_cache = qr_image(url).map(|q| (url.to_owned(), q));
            }
            if let Some((_, q)) = &self.qr_cache {
                let inner = rect.shrink(12.0);
                let cell = inner.width() / q.side as f32;
                for y in 0..q.side {
                    for x in 0..q.side {
                        if q.dark[y * q.side + x] {
                            let r = egui::Rect::from_min_size(
                                inner.min + Vec2::new(x as f32, y as f32) * cell,
                                Vec2::splat(cell + 0.5),
                            );
                            ui.painter().rect_filled(r, 0.0, Color32::BLACK);
                        }
                    }
                }
                let c = rect.center();
                ui.painter().circle_filled(c, 24.0, Color32::WHITE);
                icons::logo(ui.painter(), c, 20.0, BUTTON);
            }
        }
        ui.add_space(22.0);
        ui.label(
            RichText::new("Сканируйте QR-код в Telegram")
                .size(20.0)
                .strong()
                .color(WHITE),
        );
        ui.add_space(14.0);
        for step in [
            "1.  Откройте Telegram на телефоне",
            "2.  Перейдите в Настройки > Устройства > Подключить устройство",
            "3.  Наведите камеру на этот QR-код",
        ] {
            ui.label(RichText::new(step).size(14.0).color(WHITE));
        }
        ui.add_space(46.0);
        if link(ui, "Войти по номеру телефона").clicked() {
            self.send(Cmd::UsePhone);
        }
    }

    fn form_screen(&mut self, ui: &mut egui::Ui) {
        let (rect, _) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
        icons::logo(ui.painter(), rect.center(), 46.0, BUTTON);
        ui.add_space(14.0);
        ui.label(RichText::new("Rustgramm").size(30.0).strong().color(WHITE));
        ui.add_space(18.0);
        ui.set_max_width(320.0);
        match self.stage.clone() {
            Stage::Starting => {
                ui.label(RichText::new("Подключение к Telegram…").color(SUBTEXT));
                ui.add(egui::Spinner::new().color(ACCENT));
            }
            Stage::NeedKeys => {
                ui.label(
                    RichText::new(
                        "Нужны ключи приложения с my.telegram.org (API development tools). Они хранятся только на этом устройстве.",
                    )
                    .color(SUBTEXT),
                );
                ui.label("api_id");
                ui.add(egui::TextEdit::singleline(&mut self.key_id).desired_width(280.0));
                ui.label("api_hash");
                ui.add(
                    egui::TextEdit::singleline(&mut self.key_hash)
                        .password(true)
                        .desired_width(280.0),
                );
                if primary_button(ui, "Сохранить и продолжить").clicked() {
                    let hash = std::mem::take(&mut self.key_hash);
                    self.send(Cmd::Keys(self.key_id.clone(), hash));
                }
            }
            Stage::NeedPhone => {
                let country = countries::COUNTRIES
                    .get(self.login_country_idx)
                    .copied()
                    .unwrap_or(countries::COUNTRIES[0]);
                ui.label(RichText::new("Ваш номер телефона").size(19.0).strong().color(WHITE));
                ui.label(
                    RichText::new("Выберите страну и введите номер телефона")
                        .size(13.0)
                        .color(SUBTEXT),
                );
                ui.add_space(12.0);
                // Кнопка выбора страны (с поиском по названию/коду).
                let cbtn = egui::Button::new(
                    RichText::new(format!("{}  {} ({})  ▼", country.iso, country.name, country.dial))
                        .size(14.0)
                        .color(WHITE),
                )
                .fill(t(SIDEBAR_BG))
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(28)))
                .corner_radius(egui::CornerRadius::same(10));
                if ui
                    .add_sized([296.0, 38.0], cbtn)
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.login_country_open = !self.login_country_open;
                    self.login_country_query.clear();
                }
                if self.login_country_open {
                    ui.add_space(4.0);
                    egui::Frame::new()
                        .fill(t2(WINDOW_BG))
                        .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(30)))
                        .corner_radius(egui::CornerRadius::same(10))
                        .inner_margin(egui::Margin::same(8))
                        .show(ui, |ui| {
                            ui.set_width(280.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.login_country_query)
                                    .hint_text("Поиск страны или кода (+7, +1, США)…")
                                    .desired_width(276.0),
                            );
                            ui.add_space(4.0);
                            let matches = countries::filter_countries(&self.login_country_query);
                            egui::ScrollArea::vertical()
                                .max_height(175.0)
                                .id_salt("country-picker")
                                .show(ui, |ui| {
                                    for idx in matches {
                                        let c = countries::COUNTRIES[idx];
                                        let sel = idx == self.login_country_idx;
                                        let lbl = format!("{}  {}  {}", c.iso, c.name, c.dial);
                                        if ui
                                            .selectable_label(
                                                sel,
                                                RichText::new(lbl)
                                                    .size(13.0)
                                                    .color(if sel { ORANGE } else { WHITE }),
                                            )
                                            .clicked()
                                        {
                                            self.login_country_idx = idx;
                                            self.login_dial = c.dial.to_owned();
                                            self.login_country_open = false;
                                        }
                                    }
                                });
                        });
                }
                ui.add_space(8.0);
                // Два синхронизированных поля: [Код страны] + [Номер телефона].
                let mut enter_pressed = false;
                ui.allocate_ui_with_layout(
                    Vec2::new(296.0, 40.0),
                    egui::Layout::left_to_right(egui::Align::Center),
                    |ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        let dr = ui.add_sized(
                            [76.0, 38.0],
                            egui::TextEdit::singleline(&mut self.login_dial)
                                .hint_text("+7")
                                .margin(Vec2::new(10.0, 9.0)),
                        );
                        if dr.changed() {
                            if !self.login_dial.starts_with('+') && !self.login_dial.is_empty() {
                                let d: String = self.login_dial.chars().filter(char::is_ascii_digit).collect();
                                self.login_dial = format!("+{d}");
                            }
                            if let Some(idx) = countries::find_country_by_dial(&self.login_dial) {
                                self.login_country_idx = idx;
                            }
                        }
                        let pr = ui.add_sized(
                            [212.0, 38.0],
                            egui::TextEdit::singleline(&mut self.phone)
                                .hint_text(country.hint)
                                .margin(Vec2::new(10.0, 9.0)),
                        );
                        if pr.changed()
                            && let Some((idx, dial, local)) = countries::split_full_phone(&self.phone)
                        {
                            self.login_country_idx = idx;
                            self.login_dial = dial.to_owned();
                            self.phone = local;
                        }
                        if (pr.lost_focus() || dr.lost_focus())
                            && ui.input(|i| i.key_pressed(egui::Key::Enter))
                        {
                            enter_pressed = true;
                        }
                    },
                );
                ui.add_space(12.0);
                let btn_lbl = if self.login_busy { "ОТПРАВКА КОДА…" } else { "ДАЛЕЕ" };
                let clicked = ui
                    .add_enabled_ui(!self.login_busy, |ui| orange_button(ui, btn_lbl, Vec2::new(296.0, 42.0)))
                    .inner
                    .clicked();
                if (clicked || enter_pressed) && !self.login_busy {
                    match countries::normalize_phone(&self.login_dial, &self.phone) {
                        Ok(full_phone) => {
                            self.error.clear();
                            self.login_last_phone = full_phone.clone();
                            self.login_code_sent_at = Some(std::time::Instant::now());
                            self.login_busy = true;
                            self.send(Cmd::RequestCode(full_phone));
                        }
                        Err(msg) => {
                            self.error = msg.to_owned();
                            self.error_at = self.now;
                        }
                    }
                }
                ui.add_space(16.0);
                if link(ui, "Войти по QR-коду").clicked() {
                    self.error.clear();
                    self.send(Cmd::UseQr);
                }
            }
            Stage::NeedCode => {
                ui.label(RichText::new("Введите код подтверждения").size(18.0).strong().color(WHITE));
                if !self.login_last_phone.is_empty() {
                    ui.label(RichText::new(&self.login_last_phone).size(14.0).color(ORANGE));
                }
                ui.label(
                    RichText::new("Код отправлен в приложение Telegram (или по SMS)")
                        .size(12.5)
                        .color(SUBTEXT),
                );
                ui.add_space(10.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.code)
                        .hint_text("5 цифр кода")
                        .desired_width(296.0)
                        .margin(Vec2::new(10.0, 9.0)),
                );
                ui.add_space(10.0);
                let btn_lbl = if self.login_busy { "ПРОВЕРКА…" } else { "ВОЙТИ" };
                let go = ui
                    .add_enabled_ui(!self.login_busy, |ui| orange_button(ui, btn_lbl, Vec2::new(296.0, 42.0)))
                    .inner
                    .clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.code.trim().is_empty() && !self.login_busy {
                    self.error.clear();
                    self.login_busy = true;
                    let clean: String = self.code.chars().filter(char::is_ascii_digit).collect();
                    self.send(Cmd::Code(if clean.is_empty() { self.code.trim().to_owned() } else { clean }));
                    self.code.clear();
                }
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    if link(ui, "← Изменить номер").clicked() {
                        self.error.clear();
                        self.code.clear();
                        self.send(Cmd::BackToPhone);
                    }
                    ui.add_space(18.0);
                    let wait_left = self
                        .login_code_sent_at
                        .map_or(0, |t| 30u64.saturating_sub(t.elapsed().as_secs()));
                    if wait_left > 0 {
                        ui.label(RichText::new(format!("Повторно через {wait_left}с")).size(12.0).color(SUBTEXT));
                        ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                    } else if !self.login_last_phone.is_empty() && link(ui, "Отправить ещё раз").clicked() {
                        self.error.clear();
                        self.login_code_sent_at = Some(std::time::Instant::now());
                        self.login_busy = true;
                        self.send(Cmd::RequestCode(self.login_last_phone.clone()));
                    }
                });
            }
            Stage::NeedPassword(hint) => {
                ui.label(RichText::new("Облачный пароль (2FA)").size(18.0).strong().color(WHITE));
                ui.label(RichText::new(format!("Подсказка: {hint}")).color(SUBTEXT));
                ui.add_space(10.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.password)
                        .password(true)
                        .hint_text("Введите пароль")
                        .desired_width(296.0)
                        .margin(Vec2::new(10.0, 9.0)),
                );
                ui.add_space(10.0);
                let btn_lbl = if self.login_busy { "ПРОВЕРКА…" } else { "ПРОДОЛЖИТЬ" };
                let go = ui
                    .add_enabled_ui(!self.login_busy, |ui| orange_button(ui, btn_lbl, Vec2::new(296.0, 42.0)))
                    .inner
                    .clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.password.is_empty() && !self.login_busy {
                    self.error.clear();
                    self.login_busy = true;
                    let pw = std::mem::take(&mut self.password);
                    self.send(Cmd::Password(pw));
                }
                ui.add_space(12.0);
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
                ui.add(
                    egui::TextEdit::singleline(&mut self.proxy)
                        .hint_text("socks5://host:port")
                        .desired_width(300.0),
                );
                ui.weak("Без прокси соединение идёт напрямую с серверами Telegram.");
                if orange_button(ui, "Сохранить", Vec2::new(110.0, 32.0)).clicked() {
                    self.send(Cmd::Proxy(self.proxy.trim().to_owned()));
                    self.status = "Прокси сохранён: перезапустите приложение".to_owned();
                }
            });
        self.proxy_open = open;
    }

    // ---------- главное окно ----------

    fn dialogs_ui(&mut self, ui: &mut egui::Ui) {
        // Верхняя строка: меню и поле поиска.
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
            if resp.hovered() {
                ui.painter().circle_filled(rect.center(), 20.0, t(WINDOW_OVER));
            }
            icons::menu(ui.painter(), rect.center(), DIALOG_DATE);
            if resp.clicked() {
                self.drawer_open = true;
            }
            let has_lock = !self.settings.security.passcode_hash.is_empty();
            let w = ui.available_width() - 8.0 - if has_lock { 38.0 } else { 0.0 };
            let (srect, _) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::hover());
            ui.painter()
                .rect_filled(srect, egui::CornerRadius::same(18), t(FILTER_INPUT));
            let inner = egui::Rect::from_min_max(
                srect.min + Vec2::new(14.0, 4.0),
                srect.max - Vec2::new(if self.search.is_empty() { 14.0 } else { 32.0 }, 4.0),
            );
            ui.put(
                inner,
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Поиск")
                    .frame(egui::Frame::NONE)
                    .desired_width(inner.width()),
            );
            if !self.search.is_empty() {
                let xr = egui::Rect::from_center_size(egui::pos2(srect.right() - 16.0, srect.center().y), Vec2::splat(24.0));
                let xresp = ui.interact(xr, ui.make_persistent_id("dlg-search-clear"), Sense::click());
                icons::tg(ui.painter(), "info/info_close", xr.center(), if xresp.hovered() { WHITE } else { SUBTEXT });
                if xresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.search.clear();
                    self.global_results.clear();
                }
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
        ui.add_space(4.0);
        let needle = self.search.trim().to_lowercase();
        // Дебаунс глобального поиска в Telegram, когда введено >= 3 символов.
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
        let cache_key = (self.dialogs_gen, needle.clone(), in_archive, self.folder);
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
                    in_scope && (needle.is_empty() || d.name.to_lowercase().contains(&needle))
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
        } else if archived_total > 0 && needle.is_empty() && active_folder.is_none() && archive_row(ui, &self.dialogs) {
            toggle_archive = true;
        }
        let show_global = !needle.is_empty() && !self.global_results.is_empty();
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show_rows(
            ui,
            DIALOG_ROW_HEIGHT,
            filtered.len() + if show_global { self.global_results.len() + 1 } else { 0 },
            |ui, row_range| {
                ui.spacing_mut().item_spacing.y = 0.0;
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
                    let d = &self.dialogs[i];
                    let (rect, resp) = ui.allocate_exact_size(
                        Vec2::new(ui.available_width(), DIALOG_ROW_HEIGHT),
                        Sense::click(),
                    );
                    let selected = self.selected == Some(i);
                    let hover = theme::tween(ui.ctx(), ("dlg", i), f32::from(u8::from(resp.hovered())), 0.12);
                    let fill = if selected {
                        t(DIALOG_ACTIVE)
                    } else {
                        theme::lerp_color(Color32::TRANSPARENT, t(DIALOG_OVER), hover)
                    };
                    let p = ui.painter();
                    p.rect_filled(rect, 0.0, fill);
                    if self.avatar_req.insert(d.id) {
                        let _ = self.cmd.send(Cmd::Avatar(i));
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
                        p.text(
                            b.center(),
                            egui::Align2::CENTER_CENTER,
                            label,
                            FontId::proportional(12.0),
                            if selected { t(DIALOG_ACTIVE) } else { Color32::WHITE },
                        );
                        right = b.left() - 6.0;
                    } else if d.pinned {
                        // Закреплённый чат без непрочитанных: значок кнопки вместо бейджа.
                        let c = egui::pos2(rect.right() - 22.0, rect.bottom() - 24.0);
                        icons::tg(p, "dialogs/dialogs_pinned", c, date_c);
                        right = c.x - 12.0;
                    }
                    let date = if d.date > 0 {
                        dialog_date(d.date, now)
                    } else {
                        String::new()
                    };
                    let date_w = p
                        .layout_no_wrap(date.clone(), FontId::proportional(13.0), date_c)
                        .size()
                        .x;
                    p.text(
                        egui::pos2(rect.right() - 12.0, rect.top() + 12.0),
                        egui::Align2::RIGHT_TOP,
                        &date,
                        FontId::proportional(13.0),
                        date_c,
                    );
                    let clip_right = (rect.right() - 12.0 - date_w - 8.0).max(rect.left() + 70.0);
                    let name_clip = egui::Rect::from_min_max(rect.min, egui::pos2(clip_right, rect.max.y));
                    let text_clip = egui::Rect::from_min_max(
                        rect.min,
                        egui::pos2(right.max(rect.left() + 70.0), rect.max.y),
                    );
                    let title_str = if d.saved { "Избранное" } else { d.name.as_str() };
                    let title_w = p
                        .layout_no_wrap(title_str.to_owned(), FontId::proportional(15.0), name_c)
                        .size()
                        .x;
                    p.with_clip_rect(name_clip).text(
                        rect.left_top() + Vec2::new(68.0, 12.0),
                        egui::Align2::LEFT_TOP,
                        title_str,
                        FontId::proportional(15.0),
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
                            let area = egui::Rect::from_center_size(egui::pos2(badge_x, badge_y), Vec2::splat(16.0));
                            egui::Image::new(tex).fit_to_exact_size(area.size()).paint_at(ui, area);
                        } else {
                            icons::tg(ui.painter(), "dialogs/dialogs_premium", egui::pos2(badge_x, badge_y), ORANGE);
                        }
                        badge_x += 18.0;
                    } else if d.verified && badge_x < clip_right - 8.0 {
                        icons::tg(ui.painter(), "dialogs/dialogs_verified_star", egui::pos2(badge_x, badge_y), ORANGE);
                        badge_x += 18.0;
                    } else if show_prem && d.premium && badge_x < clip_right - 8.0 {
                        icons::tg(ui.painter(), "dialogs/dialogs_premium", egui::pos2(badge_x, badge_y), ORANGE);
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
                        p.with_clip_rect(text_clip).text(
                            rect.left_top() + Vec2::new(68.0, 35.0),
                            egui::Align2::LEFT_TOP,
                            lbl,
                            FontId::proportional(14.0),
                            ORANGE,
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
                    egui::Popup::context_menu(&resp).show(|ui| {
                        ui.set_min_width(210.0);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
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
                        item(
                            ui,
                            if d.muted { "menu/unmute" } else { "menu/mute" },
                            if d.muted { "Включить звук" } else { "Отключить звук" },
                            DialogAct::Mute(!d.muted),
                        );
                        if d.unread > 0 {
                            item(ui, "menu/read", "Прочитать на сервере", DialogAct::Read);
                        }
                    });
                }
            },
        );
        if toggle_archive {
            self.in_archive = !self.in_archive;
        }
        if let Some((peer, id, name)) = open_global {
            self.search.clear();
            self.global_results.clear();
            self.send(Cmd::OpenContact(peer, id, name));
        }
        if let Some((i, a)) = dact {
            match a {
                DialogAct::Pin(on) => self.send(Cmd::PinDialog(i, on)),
                DialogAct::Mute(on) => self.send(Cmd::MuteDialog(i, on)),
                DialogAct::Read => {
                    if let Some(d) = self.dialogs.get_mut(i) {
                        d.unread = 0;
                    }
                    self.send(Cmd::ReadDialog(i));
                }
                DialogAct::Archive(on) => self.send(Cmd::Archive(i, on)),
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
    crate::android_jni::show_notification(1001, title, body);
    #[cfg(all(not(windows), not(target_os = "android")))]
    let _ = (title, body);
}

/// Действие из контекстного меню строки списка чатов.
enum DialogAct {
    Pin(bool),
    Mute(bool),
    Read,
    Archive(bool),
}

/// Значок папки «Архив»: серый круг и значок «ящик со стрелкой» из набора Telegram.
fn paint_archive_icon(p: &egui::Painter, c: egui::Pos2, size: f32) {
    p.circle_filled(c, size / 2.0, Color32::from_rgb(70, 70, 78));
    icons::tg(p, "archive_userpic", c, Color32::WHITE);
}

/// Строка «Архив» в начале списка; `true` — нажата.
fn archive_row(ui: &mut egui::Ui, dialogs: &[DialogRow]) -> bool {
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
    let names: Vec<&str> = dialogs
        .iter()
        .filter(|d| d.archived)
        .take(3)
        .map(|d| d.name.as_str())
        .collect();
    p.with_clip_rect(egui::Rect::from_min_max(
        rect.min,
        egui::pos2(rect.right() - 60.0, rect.max.y),
    ))
    .text(
        rect.left_top() + Vec2::new(68.0, 35.0),
        egui::Align2::LEFT_TOP,
        names.join(", "),
        FontId::proportional(14.0),
        DIALOG_TEXT,
    );
    let unread: i32 = dialogs.iter().filter(|d| d.archived && !d.muted).map(|d| d.unread).sum();
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
        [0.0, 0.0, 0.0, 0.0]
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
        theme::set_blur(self.settings.blur_enabled);
        theme::sync_visuals(&ctx);
        kit::sync_prefs(
            self.settings.ayu.material_switches,
            self.settings.extera.disable_dividers,
        );
        win::set_window_blur(self.settings.blur_enabled && self.settings.window_opacity < 100);
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
        ui.painter()
            .rect_filled(ctx.content_rect(), egui::CornerRadius::ZERO, t(BACKDROP));
        if !self.emoji_tried {
            self.emoji_tried = true;
            self.emoji = emoji::Emoji::load(&ctx);
        }
        self.drain_events(&ctx);
        self.tick_tgs(&ctx);
        self.handle_shortcuts(&ctx);
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
        #[cfg(not(target_os = "android"))]
        {
            egui::Panel::top("titlebar")
                .exact_size(34.0)
                .frame(egui::Frame::new().fill(tint_title()))
                .show(ui, |ui| self.titlebar(ui));
            self.resize_edges(ui);
        }
        #[cfg(target_os = "android")]
        {
            // Безопасный отступ под системный статус-бар Android (часы, Wi-Fi, батарея).
            egui::Panel::top("android-safe-top")
                .exact_size(40.0)
                .frame(egui::Frame::new().fill(tint_title()))
                .show(ui, |_ui| {});
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
        // Узкое окно (телефон): одна колонка — либо список чатов, либо открытый чат.
        let narrow = ui.available_width() < NARROW_WIDTH;
        if narrow && self.selected.is_none() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.dialogs_ui(ui));
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
        // Панель эмодзи/стикеров/GIF в широком окне стоит справа, как в AyuGram, и занимает место панели сведений.
        let emoji_k = theme::tween(&ctx, "emoji-side", f32::from(u8::from(self.emoji_open && self.selected.is_some() && !narrow)), 0.2);
        if emoji_k > 0.01 && self.selected.is_some() {
            egui::Panel::right("emoji-side")
                .exact_size(320.0 * ease_out(emoji_k))
                .frame(egui::Frame::new().fill(tint_panel()).inner_margin(egui::Margin::symmetric(4, 0)))
                .show(ui, |ui| {
                    ui.set_min_width(300.0);
                    self.emoji_panel_body(ui);
                });
            if ctx.input(|i| i.key_pressed(egui::Key::Escape)) && !self.emoji_pinned {
                self.emoji_open = false;
            }
        }
        let info_k = theme::tween(&ctx, "info-panel", f32::from(u8::from(self.info_open && !self.emoji_open && self.selected.is_some() && !narrow)), 0.2);
        if info_k > 0.01 && self.selected.is_some() && emoji_k < 0.01 {
            egui::Panel::right("info")
                .exact_size(320.0 * ease_out(info_k))
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.info_ui(ui));
        }
        egui::Panel::top("header")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(egui::Frame::new().fill(tint_panel()))
            .show(ui, |ui| self.header_ui(ui, narrow));
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
        if !self.status.is_empty() || !self.error.is_empty() {
            egui::Panel::bottom("status")
                .frame(
                    egui::Frame::new()
                        .fill(t(WINDOW_BG))
                        .inner_margin(egui::Margin::symmetric(12, 4)),
                )
                .show(ui, |ui| {
                    if !self.error.is_empty() {
                        ui.colored_label(DANGER, &self.error);
                    } else {
                        ui.label(RichText::new(&self.status).color(SUBTEXT));
                    }
                });
        }
        if self.search_open && self.selected.is_some() {
            egui::Panel::top("chat-search")
                .exact_size(40.0)
                .frame(egui::Frame::new().fill(tint_panel()))
                .show(ui, |ui| self.search_bar(ui));
        }
        if self.selected.is_some() {
            egui::Panel::bottom("composer")
                .frame(
                    egui::Frame::new()
                        .fill(tint_panel())
                        .inner_margin(egui::Margin::symmetric(8, 4)),
                )
                .show(ui, |ui| self.composer_ui(ui));
        }
        // Лента мягко подъезжает при открытии чата (200 мс, кривая iOS без перелёта).
        let push = (self.chat_opened.elapsed().as_secs_f32() / 0.2).clamp(0.0, 1.0);
        if push < 1.0 && self.selected.is_some() {
            ctx.request_repaint();
        }
        let slide = if self.selected.is_some() { (1.0 - ios_spring(push)) * 14.0 } else { 0.0 };
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
                        left: (8.0 + slide.max(0.0)) as i8,
                        right: 8,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| {
                let wp_path = self.settings.theme.wallpaper_path.clone();
                if !wp_path.is_empty() && self.wallpaper_tex.as_ref().is_none_or(|(p, _)| p != &wp_path) {
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
                    } else {
                        self.wallpaper_tex = None;
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
                self.messages_ui(ui);
            });
        if narrow {
            self.emoji_window(&ctx);
        }
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
        self.my_profile_window(ctx);
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
        if let Some((path, caption, reply_to, opts)) =
            media_editor::media_editor_window(ctx, &mut self.media_editor, &worker::home().join("cache"))
        {
            let cleanup_temp = path
                .parent()
                .is_some_and(|p| p.ends_with("edited"));
            self.send(Cmd::SendMediaEdited {
                path,
                caption,
                reply_to,
                opts,
                cleanup_temp,
            });
        }
        #[cfg(target_os = "android")]
        {
            if let Some(picked) = android_jni::poll_picked_file() {
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
            if let Some(tgu) = android_jni::poll_pending_link() {
                if let Some(parsed) = rg_core::engine::parse_tg_link(&tgu) {
                    self.send(Cmd::OpenTgLink(parsed));
                }
                self.miniapp_modal = None;
            }
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
                android_jni::open_webview(&url, &title, self.settings.ayu.spoof_webview_as_android);
            }
        }
        #[cfg(all(not(windows), not(target_os = "android")))]
        {
            let _ = frame;
            self.miniapp_fallback_window(ctx);
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
                ui.painter().text(
                    btn_exp.center(),
                    egui::Align2::CENTER_CENTER,
                    if self.miniapp_expanded { "⤡" } else { "⤢" },
                    FontId::proportional(14.0),
                    if expr.hovered() { ORANGE } else { SUBTEXT },
                );
                if expr.on_hover_text(if self.miniapp_expanded { "Обычный размер" } else { "Развернуть" }).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.miniapp_expanded = !self.miniapp_expanded;
                }
                let btn_ext = egui::Rect::from_center_size(egui::pos2(hdr.right() - 82.0, hdr.center().y), Vec2::splat(28.0));
                let er = ui.interact(btn_ext, ui.make_persistent_id("miniapp-ext"), Sense::click());
                ui.painter().text(btn_ext.center(), egui::Align2::CENTER_CENTER, "↗", FontId::proportional(14.0), if er.hovered() { ORANGE } else { SUBTEXT });
                if er.on_hover_text("Открыть в браузере").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    open_ext = true;
                }
                let btn_rel = egui::Rect::from_center_size(egui::pos2(hdr.right() - 112.0, hdr.center().y), Vec2::splat(28.0));
                let rr = ui.interact(btn_rel, ui.make_persistent_id("miniapp-reload"), Sense::click());
                ui.painter().text(btn_rel.center(), egui::Align2::CENTER_CENTER, "↻", FontId::proportional(14.0), if rr.hovered() { WHITE } else { SUBTEXT });
                if rr.on_hover_text("Обновить").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    reload = true;
                }
                if bridge.main_btn_visible && !bridge.main_btn_text.is_empty() {
                    let mb_rect = egui::Rect::from_min_max(
                        egui::pos2(card_rect.left() + 10.0, card_rect.bottom() - 42.0),
                        egui::pos2(card_rect.right() - 10.0, card_rect.bottom() - 8.0),
                    );
                    let mbr = ui.interact(mb_rect, ui.make_persistent_id("miniapp-mainbtn"), Sense::click());
                    let base_c = bridge.main_btn_color.map_or(ORANGE, |(r, g, b)| Color32::from_rgb(r, g, b));
                    let fill_c = if mbr.hovered() { base_c.gamma_multiply(1.12) } else { base_c };
                    ui.painter().rect_filled(mb_rect, egui::CornerRadius::same(10), fill_c);
                    ui.painter().text(mb_rect.center(), egui::Align2::CENTER_CENTER, &bridge.main_btn_text, FontId::proportional(14.0), Color32::WHITE);
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
            saved: false,
            archived: false,
            kind: rg_core::engine::DialogKind::User,
            contact: true,
            read_out: 0,
            emoji_status: None,
            verified: i == 0,
            premium: i == 1,
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
            saved: false,
            archived: true,
            kind: rg_core::engine::DialogKind::User,
            contact: false,
            read_out: 0,
            emoji_status: None,
            verified: false,
            premium: false,
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
            std::thread::spawn(|| win::dress_window("Rustgramm (демо)"));
            native_options()
        },
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(demo_app(&mode)))
        }),
    )
}
