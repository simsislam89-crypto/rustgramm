//! Фоновый поток: вся работа с сетью и диском, интерфейс получает только события.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use grammers_client::SignInError;
use grammers_client::client::{LoginToken, PasswordToken};
use rg_ayu::store::MessageStore;
use rg_core::engine::{
    ContactRow, DialogKind, DialogRow, Engine, FolderRow, MessageRow, StickerRow, StickerSetRow,
};
use rg_core::{Config, connect_with_updates};
use rg_settings::Settings;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Starting,
    NeedKeys,
    /// Вход по QR: ссылка для кода (пустая, пока токен не получен).
    Qr(String),
    NeedPhone,
    NeedCode,
    NeedPassword(String),
    Ready,
}

pub enum Cmd {
    Keys(String, String),
    Proxy(String),
    UsePhone,
    UseQr,
    Preview(usize, i32),
    /// Загрузить фото профиля чата с индексом.
    Avatar(usize),
    Download(i32),
    /// Скачать вложение и открыть программой по умолчанию (безопасные типы) или показать в папке.
    OpenFile(i32),
    RequestCode(String),
    Code(String),
    Password(String),
    Open(usize),
    /// Текст и, при ответе, идентификатор исходного сообщения.
    Send(String, Option<i32>),
    Edit(i32, String),
    /// Отправить файл с подписью (и ответом на сообщение).
    SendFile(PathBuf, String, Option<i32>),
    /// Прежние версии текста сообщения.
    EditHistory(i32),
    Delete(i32),
    /// Переслать сообщение в чат с таким индексом списка.
    Forward(i32, usize),
    Pin(i32, bool),
    React(i32, String),
    /// Поиск в открытом чате; пустая строка — выйти из поиска.
    Search(String),
    /// Подгрузить сообщения старше указанного идентификатора.
    Older(i32),
    /// Отметить открытый чат прочитанным принудительно (мимо режима призрака).
    ForceRead,
    /// Выйти из аккаунта: сессия и локальная база стираются, приложение перезапускается.
    LogOut,
    /// Архивировать (true) или вернуть из архива чат с индексом.
    Archive(usize, bool),
    PinDialog(usize, bool),
    MuteDialog(usize, bool),
    /// Прочитать чат с индексом на сервере.
    ReadDialog(usize),
    /// Загрузить список контактов.
    Contacts,
    /// Открыть чат с контактом (добавляется в список, если его там нет).
    OpenContact(grammers_session::types::PeerRef, i64, String),
    /// Фото профиля для чата, которого нет в списке (контакты).
    AvatarPeer(grammers_session::types::PeerRef, i64),
    /// Загрузить наборы стикеров.
    Stickers,
    /// Обновить список чатов (счётчики, последние сообщения).
    RefreshDialogs,
    /// Сведения о чате с номером для правой панели.
    PeerInfo(usize),
    /// Загрузить сохранённые GIF.
    Gifs,
    /// Создать канал (название, описание, true — канал, false — супергруппа).
    CreateChannel(String, String, bool),
    /// Создать группу с участниками.
    CreateGroup(String, Vec<grammers_session::types::PeerRef>),
    /// Аватарка отправителя сообщения (идентификатор сообщения, идентификатор отправителя).
    SenderAvatar(i32, i64),
    /// Отметить открытый чат прочитанным (по правилам режима призрака).
    MarkRead,
    /// Загрузить стикеры набора с номером.
    StickerSet(usize),
    /// Превью стикера по идентификатору документа.
    StickerThumb(i64),
    /// Отправить стикер (идентификатор документа) в открытый чат, при ответе — на сообщение.
    SendSticker(i64, Option<i32>),
    /// Установленные наборы пользовательских эмодзи.
    EmojiSets,
    /// Рекомендуемые наборы эмодзи (для добавления).
    FeaturedEmoji,
    /// Добавить набор эмодзи себе.
    InstallSet(StickerSetRow),
    /// Содержимое набора (эмодзи или стикеры), заданного строкой.
    LoadSetRow(StickerSetRow),
    /// Миниатюра значка набора (приходит как `StickerImg` с идентификатором набора).
    SetIcon(StickerSetRow),
    Settings(Box<Settings>),
}

pub enum Ev {
    Image(usize, i32, Box<eframe::egui::ColorImage>),
    /// Фото профиля: идентификатор чата (Bot API) и картинка.
    Avatar(i64, Box<eframe::egui::ColorImage>),
    Stage(Stage),
    Me(String),
    Dialogs(Vec<DialogRow>),
    History(usize, Vec<MessageRow>),
    /// Более старые сообщения (добавляются в начало); пустой список — история закончилась.
    Older(usize, Vec<MessageRow>),
    /// Результат поиска по чату.
    Found(usize, Vec<MessageRow>),
    EditHistory(i32, Vec<String>),
    Settings(Box<Settings>),
    Folders(Vec<FolderRow>),
    Contacts(Vec<ContactRow>),
    /// Выбрать чат по идентификатору.
    Select(i64),
    StickerSets(Vec<StickerSetRow>),
    StickerList(i64, Vec<StickerRow>),
    StickerImg(i64, Box<eframe::egui::ColorImage>),
    EmojiSets(Vec<StickerSetRow>),
    FeaturedEmoji(Vec<StickerSetRow>),
    /// Событие из потока обновлений Telegram.
    Live(rg_core::engine::Live),
    Gifs(Vec<StickerRow>),
    /// Сведения о чате (идентификатор чата Bot API).
    PeerInfo(i64, rg_core::engine::PeerInfo),
    /// Ролик скачан: открыть во встроенном плеере.
    PlayVideo(PathBuf),
    Status(String),
    Error(String),
}

/// Каталог данных на Android задаёт оболочка до запуска интерфейса.
pub static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Ключи, вшитые при сборке (`RG_BUILTIN_API_ID` и `RG_BUILTIN_API_HASH`).
fn builtin_keys() -> Option<Config> {
    let id = option_env!("RG_BUILTIN_API_ID")?;
    let hash = option_env!("RG_BUILTIN_API_HASH")?;
    Config::from_lookup(|k| match k {
        "RG_API_ID" => Some(id.to_owned()),
        "RG_API_HASH" => Some(hash.to_owned()),
        "RG_PROXY" => std::env::var("RG_PROXY").ok(),
        _ => None,
    })
    .ok()
}

pub(crate) fn home() -> PathBuf {
    let dir = std::env::var("RG_HOME")
        .map(PathBuf::from)
        .ok()
        .or_else(|| DATA_DIR.get().cloned())
        .or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Rustgramm")))
        .unwrap_or_else(|| PathBuf::from(".rg"));
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn keys_from_file(home: &std::path::Path) -> Option<Config> {
    let text = std::fs::read_to_string(home.join("api.keys")).ok()?;
    let mut lines = text.lines();
    let api_id: i32 = lines.next()?.trim().parse().ok()?;
    let api_hash = lines.next()?.trim().to_owned();
    Config::from_lookup(|k| match k {
        "RG_API_ID" => Some(api_id.to_string()),
        "RG_API_HASH" => Some(api_hash.clone()),
        "RG_PROXY" => std::env::var("RG_PROXY").ok(),
        _ => None,
    })
    .ok()
}

/// После выхода из аккаунта удаляет сессию, базу сообщений и кэш предыдущего аккаунта.
/// Вызывается до подключения: файлы ещё не открыты. Настройки и загрузки остаются.
fn wipe_if_logged_out(home: &std::path::Path) {
    let flag = home.join("logout.flag");
    if !flag.exists() {
        return;
    }
    for name in [
        "main.session",
        "main.session-shm",
        "main.session-wal",
        "messages.db",
        "messages.db-shm",
        "messages.db-wal",
    ] {
        let _ = std::fs::remove_file(home.join(name));
    }
    let _ = std::fs::remove_dir_all(home.join("cache"));
    let _ = std::fs::remove_file(flag);
}

/// Запускает новую копию приложения и завершает текущую.
fn restart_app() {
    if let Ok(exe) = std::env::current_exe() {
        let _ = std::process::Command::new(exe).spawn();
    }
    std::process::exit(0);
}

fn load_settings(path: &std::path::Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(s) => Settings::from_json(&s).unwrap_or_default(),
        // Первый запуск: максимум приватности (см. `Settings::first_run`).
        Err(_) => Settings::first_run(),
    }
}

pub fn spawn(cmds: UnboundedReceiver<Cmd>, ev: Sender<Ev>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(run(cmds, ev, ctx));
    });
}

#[derive(Clone)]
struct Out {
    ev: Sender<Ev>,
    ctx: eframe::egui::Context,
}

impl Out {
    fn send(&self, e: Ev) {
        let _ = self.ev.send(e);
        self.ctx.request_repaint();
    }
    fn err(&self, text: impl std::fmt::Display) {
        self.send(Ev::Error(text.to_string()));
    }
}

async fn run(mut cmds: UnboundedReceiver<Cmd>, ev: Sender<Ev>, ctx: eframe::egui::Context) {
    let out = Out { ev, ctx };
    let home = home();
    wipe_if_logged_out(&home);
    // Ключи: переменные окружения или файл `api.keys` (две строки: api_id, api_hash).
    let mut cfg = loop {
        if let Some(c) = Config::from_env()
            .ok()
            .or_else(|| keys_from_file(&home))
            .or_else(builtin_keys) {
            break c;
        }
        out.send(Ev::Stage(Stage::NeedKeys));
        match cmds.recv().await {
            Some(Cmd::Keys(id, hash)) => match id.trim().parse::<i32>() {
                Ok(n) if n > 0 && !hash.trim().is_empty() => {
                    let _ = std::fs::write(home.join("api.keys"), format!("{n}
{}
", hash.trim()));
                }
                _ => out.err("api_id — число, api_hash — строка из 32 символов"),
            },
            Some(_) => {}
            None => return,
        }
    };
    cfg.session_path = home.join("main.session").to_string_lossy().into_owned();
    if cfg.proxy_url.is_none() {
        cfg.proxy_url = std::fs::read_to_string(home.join("proxy.txt"))
            .ok()
            .map(|s| s.trim().to_owned())
            .filter(|s| s.starts_with("socks5://"));
    }

    let connected = match connect_with_updates(&cfg).await {
        Ok(c) => c,
        Err(e) => {
            out.err(format!("Не удалось подключиться: {e}"));
            return;
        }
    };
    let settings_path = home.join("settings.json");
    let settings = load_settings(&settings_path);
    let store = match MessageStore::open(&home.join("messages.db").to_string_lossy()).await {
        Ok(s) => s,
        Err(e) => {
            out.err(format!("Хранилище сообщений: {e}"));
            return;
        }
    };
    let engine = Engine::new(connected.client.clone(), settings.clone(), store)
        .with_dirs(&home.join("cache"), &home.join("downloads"));
    let limit = std::sync::Arc::new(tokio::sync::Semaphore::new(8));
    // Фото профилей маленькие: качаем много штук одновременно.
    let avatar_limit = std::sync::Arc::new(tokio::sync::Semaphore::new(24));
    let mut updates = Some(connected.updates);
    out.send(Ev::Settings(Box::new(settings)));

    let mut login: Option<LoginToken> = None;
    let mut password: Option<PasswordToken> = None;
    let mut dialogs: Vec<DialogRow> = Vec::new();
    let mut current: Option<usize> = None;
    let mut sticker_sets: Vec<StickerSetRow> = Vec::new();
    let mut sticker_docs: std::collections::HashMap<i64, StickerRow> = std::collections::HashMap::new();

    let authorized = connected.client.is_authorized().await.unwrap_or(false);
    let mut qr_mode = false;
    if authorized {
        enter(&engine, &out, &mut updates, &mut dialogs).await;
    } else {
        qr_mode = true;
        out.send(Ev::Stage(Stage::Qr(String::new())));
    }
    let mut tick = tokio::time::interval(std::time::Duration::from_secs(2));
    let mut last_url = String::new();

    loop {
        let cmd = tokio::select! {
            c = cmds.recv() => match c { Some(c) => c, None => break },
            _ = tick.tick(), if qr_mode => {
                match rg_core::qr::poll(&connected.client, &*connected.session, cfg.api_id, &cfg.api_hash).await {
                    Ok(rg_core::qr::QrStep::Token { url }) => {
                        if url != last_url {
                            last_url = url.clone();
                            out.send(Ev::Stage(Stage::Qr(url)));
                        }
                    }
                    Ok(rg_core::qr::QrStep::Done) => {
                        qr_mode = false;
                        enter(&engine, &out, &mut updates, &mut dialogs).await;
                    }
                    Ok(rg_core::qr::QrStep::NeedPassword(p)) => {
                        qr_mode = false;
                        let hint = p.hint().unwrap_or("нет").to_owned();
                        password = Some(p);
                        out.send(Ev::Stage(Stage::NeedPassword(hint)));
                    }
                    Err(e) => out.err(format!("QR-вход: {e}")),
                }
                continue;
            }
        };
        match cmd {
            Cmd::Keys(..) => {}
            Cmd::Proxy(url) => {
                // Прокси вступает в силу при следующем запуске; принимается только socks5.
                if url.is_empty() || url.starts_with("socks5://") {
                    let _ = std::fs::write(home.join("proxy.txt"), url);
                } else {
                    out.err("Допустим только адрес вида socks5://host:port");
                }
            }
            Cmd::UsePhone => {
                qr_mode = false;
                out.send(Ev::Stage(Stage::NeedPhone));
            }
            Cmd::UseQr => {
                qr_mode = true;
                last_url.clear();
                out.send(Ev::Stage(Stage::Qr(String::new())));
            }
            Cmd::RequestCode(phone) => match connected.client.request_login_code(&phone, &cfg.api_hash).await {
                Ok(t) => {
                    login = Some(t);
                    out.send(Ev::Stage(Stage::NeedCode));
                }
                Err(e) => out.err(format!("Код не отправлен: {e}")),
            },
            Cmd::Code(code) => {
                let Some(t) = login.as_ref() else { continue };
                match connected.client.sign_in(t, &code).await {
                    Ok(_) => enter(&engine, &out, &mut updates, &mut dialogs).await,
                    Err(SignInError::PasswordRequired(p)) => {
                        let hint = p.hint().unwrap_or("нет").to_owned();
                        password = Some(p);
                        out.send(Ev::Stage(Stage::NeedPassword(hint)));
                    }
                    Err(SignInError::InvalidCode) => out.err("Неверный код"),
                    Err(e) => out.err(format!("Вход не удался: {e}")),
                }
            }
            Cmd::Password(pw) => {
                let Some(p) = password.take() else { continue };
                match connected.client.check_password(p, pw.trim()).await {
                    Ok(_) => enter(&engine, &out, &mut updates, &mut dialogs).await,
                    Err(SignInError::InvalidPassword(p)) => {
                        let hint = p.hint().unwrap_or("нет").to_owned();
                        password = Some(p);
                        out.send(Ev::Stage(Stage::NeedPassword(hint)));
                        out.err("Неверный пароль");
                    }
                    Err(e) => out.err(format!("Вход не удался: {e}")),
                }
            }
            Cmd::Open(i) => {
                current = Some(i);
                let Some(d) = dialogs.get(i) else { continue };
                match engine.history(d.peer, 80).await {
                    Ok(rows) => {
                        out.send(Ev::History(i, rows));
                        match engine.mark_read(d.peer).await {
                            Ok(true) => out.send(Ev::Status(String::new())),
                            Ok(false) => out.send(Ev::Status("Режим призрака: «прочитано» не отправлено".into())),
                            Err(e) => out.err(e),
                        }
                    }
                    Err(e) => out.err(format!("История: {e}")),
                }
            }
            Cmd::Send(text, reply_to) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                if let Err(e) = engine.send_reply(d.peer, &text, reply_to).await {
                    out.err(format!("Не отправлено: {e}"));
                    continue;
                }
                reload(&engine, &out, d.peer, i).await;
            }
            Cmd::SendFile(path, caption, reply_to) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                out.send(Ev::Status("Отправка файла…".into()));
                match engine.send_file(d.peer, &path, &caption, reply_to).await {
                    Ok(()) => {
                        out.send(Ev::Status(String::new()));
                        reload(&engine, &out, d.peer, i).await;
                    }
                    Err(e) => out.err(format!("Файл не отправлен: {e}")),
                }
            }
            Cmd::EditHistory(id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                out.send(Ev::EditHistory(id, engine.edit_history(&d.peer, id).await));
            }
            Cmd::Archive(i, on) => {
                let Some(d) = dialogs.get(i) else { continue };
                match engine.set_archived(d.peer, on).await {
                    Ok(()) => refresh_keep(&engine, &out, &mut dialogs, &mut current).await,
                    Err(e) => out.err(format!("Архив: {e}")),
                }
            }
            Cmd::PinDialog(i, on) => {
                let Some(d) = dialogs.get(i) else { continue };
                match engine.set_pinned_dialog(d.peer, on).await {
                    Ok(()) => refresh_keep(&engine, &out, &mut dialogs, &mut current).await,
                    Err(e) => out.err(format!("Закрепление: {e}")),
                }
            }
            Cmd::MuteDialog(i, on) => {
                let Some(d) = dialogs.get(i) else { continue };
                match engine.set_muted(d.peer, on).await {
                    Ok(()) => refresh_keep(&engine, &out, &mut dialogs, &mut current).await,
                    Err(e) => out.err(format!("Звук: {e}")),
                }
            }
            Cmd::ReadDialog(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                match engine.force_read(d.peer).await {
                    Ok(()) => refresh_keep(&engine, &out, &mut dialogs, &mut current).await,
                    Err(e) => out.err(format!("Не отмечено: {e}")),
                }
            }
            Cmd::Contacts => match engine.contacts().await {
                Ok(c) => out.send(Ev::Contacts(c)),
                Err(e) => out.err(format!("Контакты: {e}")),
            },
            Cmd::OpenContact(peer, id, name) => {
                if !dialogs.iter().any(|d| d.id == id) {
                    dialogs.insert(
                        0,
                        DialogRow {
                            peer,
                            id,
                            name,
                            last: String::new(),
                            date: 0,
                            unread: 0,
                            pinned: false,
                            muted: false,
                            saved: false,
                            archived: false,
                            kind: DialogKind::User,
                            contact: true,
                            read_out: 0,
                        },
                    );
                    current = current.map(|i| i + 1);
                    out.send(Ev::Dialogs(dialogs.clone()));
                }
                out.send(Ev::Select(id));
            }
            Cmd::AvatarPeer(peer, id) => {
                let (eng, out, limit) = (engine.clone(), out.clone(), avatar_limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.avatar(peer).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 112)).await
                    {
                        out.send(Ev::Avatar(id, Box::new(img)));
                    }
                });
            }
            Cmd::Stickers => match engine.sticker_sets().await {
                Ok(s) => {
                    sticker_sets = s.clone();
                    out.send(Ev::StickerSets(s));
                }
                Err(e) => out.err(format!("Стикеры: {e}")),
            },
            Cmd::StickerSet(i) => {
                let Some(set) = sticker_sets.get(i).cloned() else { continue };
                match engine.sticker_set(&set).await {
                    Ok(list) => {
                        for st in &list {
                            sticker_docs.insert(st.id, st.clone());
                        }
                        out.send(Ev::StickerList(set.id, list));
                    }
                    Err(e) => out.err(format!("Набор стикеров: {e}")),
                }
            }
            Cmd::StickerThumb(doc) => {
                let Some(row) = sticker_docs.get(&doc).cloned() else { continue };
                let (eng, out, limit) = (engine.clone(), out.clone(), avatar_limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.sticker_file(&row).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 144)).await
                    {
                        out.send(Ev::StickerImg(doc, Box::new(img)));
                    }
                });
            }
            Cmd::RefreshDialogs => {
                refresh_keep(&engine, &out, &mut dialogs, &mut current).await;
            }
            Cmd::SenderAvatar(msg_id, sender_id) => {
                let Some(d) = current.and_then(|i| dialogs.get(i)) else { continue };
                let (eng, out, limit, peer) = (engine.clone(), out.clone(), avatar_limit.clone(), d.peer);
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    let Some(r) = eng.sender_ref(peer, msg_id).await else { return };
                    if let Ok(Some(path)) = eng.avatar(r).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 96)).await
                    {
                        out.send(Ev::Avatar(sender_id, Box::new(img)));
                    }
                });
            }
            Cmd::Gifs => match engine.saved_gifs().await {
                Ok(list) => {
                    for g in &list {
                        sticker_docs.insert(g.id, g.clone());
                    }
                    out.send(Ev::Gifs(list));
                }
                Err(e) => out.err(format!("GIF: {e}")),
            },
            Cmd::CreateChannel(title, about, broadcast) => match engine.create_channel(&title, &about, broadcast).await {
                Ok(()) => {
                    out.send(Ev::Status(format!("Создано: «{title}»")));
                    refresh_keep(&engine, &out, &mut dialogs, &mut current).await;
                }
                Err(e) => out.err(format!("Не создано: {e}")),
            },
            Cmd::CreateGroup(title, members) => match engine.create_group(&title, &members).await {
                Ok(()) => {
                    out.send(Ev::Status(format!("Группа «{title}» создана")));
                    refresh_keep(&engine, &out, &mut dialogs, &mut current).await;
                }
                Err(e) => out.err(format!("Группа не создана: {e}")),
            },
            Cmd::PeerInfo(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.peer_info(peer).await {
                        Ok(info) => out.send(Ev::PeerInfo(id, info)),
                        Err(e) => out.err(format!("Сведения о чате: {e}")),
                    }
                });
            }
            Cmd::MarkRead => {
                let Some(d) = current.and_then(|i| dialogs.get(i)) else { continue };
                let _ = engine.mark_read(d.peer).await;
            }
            Cmd::EmojiSets => match engine.emoji_sets().await {
                Ok(s) => out.send(Ev::EmojiSets(s)),
                Err(e) => out.err(format!("Наборы эмодзи: {e}")),
            },
            Cmd::FeaturedEmoji => match engine.featured_emoji_sets().await {
                Ok(s) => {
                    for set in &s {
                        if let Some(c) = &set.cover {
                            sticker_docs.insert(c.id, (**c).clone());
                        }
                    }
                    out.send(Ev::FeaturedEmoji(s));
                }
                Err(e) => out.err(format!("Рекомендуемые эмодзи: {e}")),
            },
            Cmd::InstallSet(set) => match engine.install_set(&set).await {
                Ok(()) => {
                    out.send(Ev::Status(format!("Добавлен набор «{}»", set.title)));
                    if let Ok(s) = engine.emoji_sets().await {
                        out.send(Ev::EmojiSets(s));
                    }
                    if let Ok(s) = engine.featured_emoji_sets().await {
                        out.send(Ev::FeaturedEmoji(s));
                    }
                }
                Err(e) => out.err(format!("Набор не добавлен: {e}")),
            },
            Cmd::LoadSetRow(set) => match engine.sticker_set(&set).await {
                Ok(list) => {
                    for st in &list {
                        sticker_docs.insert(st.id, st.clone());
                    }
                    out.send(Ev::StickerList(set.id, list));
                }
                Err(e) => out.err(format!("Набор: {e}")),
            },
            Cmd::SetIcon(set) => {
                let (eng, out, limit) = (engine.clone(), out.clone(), avatar_limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.set_icon(&set).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 96)).await
                    {
                        out.send(Ev::StickerImg(set.id, Box::new(img)));
                    }
                });
            }
            Cmd::SendSticker(doc, reply) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let Some(row) = sticker_docs.get(&doc).cloned() else { continue };
                match engine.send_sticker(d.peer, &row, reply).await {
                    Ok(()) => reload(&engine, &out, d.peer, i).await,
                    Err(e) => out.err(format!("Стикер не отправлен: {e}")),
                }
            }
            Cmd::LogOut => {
                match connected.client.sign_out().await {
                    Ok(_) => {}
                    Err(e) => {
                        // Сессия на сервере уже недействительна — локально всё равно чистим.
                        out.err(format!("Выход: {e}"));
                    }
                }
                let _ = std::fs::write(home.join("logout.flag"), b"1");
                restart_app();
            }
            Cmd::ForceRead => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                match engine.force_read(d.peer).await {
                    Ok(()) => out.send(Ev::Status("Отмечено прочитанным на сервере".into())),
                    Err(e) => out.err(format!("Не отмечено: {e}")),
                }
            }
            Cmd::Edit(id, text) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                match engine.edit(d.peer, id, &text).await {
                    Ok(()) => reload(&engine, &out, d.peer, i).await,
                    Err(e) => out.err(format!("Не изменено: {e}")),
                }
            }
            Cmd::Delete(id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                match engine.delete(d.peer, &[id]).await {
                    Ok(_) => reload(&engine, &out, d.peer, i).await,
                    Err(e) => out.err(format!("Не удалено: {e}")),
                }
            }
            Cmd::Forward(id, to) => {
                let Some(i) = current else { continue };
                let (Some(from), Some(dest)) = (dialogs.get(i), dialogs.get(to)) else { continue };
                match engine.forward(from.peer, dest.peer, &[id]).await {
                    Ok(()) => out.send(Ev::Status(format!("Переслано: {}", dest.name))),
                    Err(e) => out.err(format!("Не переслано: {e}")),
                }
            }
            Cmd::Pin(id, pinned) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                match engine.pin(d.peer, id, pinned).await {
                    Ok(()) => reload(&engine, &out, d.peer, i).await,
                    Err(e) => out.err(format!("Закрепление: {e}")),
                }
            }
            Cmd::React(id, emoji) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                match engine.react(d.peer, id, &emoji).await {
                    Ok(()) => reload(&engine, &out, d.peer, i).await,
                    Err(e) => out.err(format!("Реакция: {e}")),
                }
            }
            Cmd::Search(q) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                if q.trim().is_empty() {
                    reload(&engine, &out, d.peer, i).await;
                    continue;
                }
                match engine.search(d.peer, q.trim(), 100).await {
                    Ok(rows) => out.send(Ev::Found(i, rows)),
                    Err(e) => out.err(format!("Поиск: {e}")),
                }
            }
            Cmd::Older(before) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                match engine.history_before(d.peer, before, 60).await {
                    Ok(rows) => out.send(Ev::Older(i, rows)),
                    Err(e) => out.err(format!("История: {e}")),
                }
            }
            Cmd::Avatar(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, limit, peer, id) = (engine.clone(), out.clone(), avatar_limit.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.avatar(peer).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 112)).await
                    {
                        out.send(Ev::Avatar(id, Box::new(img)));
                    }
                });
            }
            Cmd::Preview(i, msg_id) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, limit, peer) = (engine.clone(), out.clone(), limit.clone(), d.peer);
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.preview_path(peer, msg_id).await
                        && let Ok(Some(img)) =
                            tokio::task::spawn_blocking(move || decode(&path)).await
                    {
                        out.send(Ev::Image(i, msg_id, Box::new(img)));
                    }
                });
            }
            Cmd::OpenFile(msg_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    out.send(Ev::Status("Загрузка файла…".into()));
                    match eng.download_file(peer, msg_id).await {
                        Ok(Some(p)) => {
                            let ext = p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
                            if cfg!(windows) && matches!(ext.as_str(), "mp4" | "webm" | "mov" | "mkv" | "m4v" | "gif" | "ogv") {
                                out.send(Ev::PlayVideo(p));
                                out.send(Ev::Status(String::new()));
                            } else {
                                rg_core::media::open_downloaded(&p);
                                out.send(Ev::Status(format!("Сохранено: {}", p.display())));
                            }
                        }
                        Ok(None) => out.err("Вложение недоступно"),
                        Err(e) => out.err(format!("Загрузка: {e}")),
                    }
                });
            }
            Cmd::Download(msg_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.download_file(peer, msg_id).await {
                        Ok(Some(p)) => out.send(Ev::Status(format!("Сохранено: {}", p.display()))),
                        Ok(None) => out.err("Вложение недоступно"),
                        Err(e) => out.err(format!("Загрузка: {e}")),
                    }
                });
            }
            Cmd::Settings(s) => {
                let s = *s;
                let json = s.to_json();
                engine.update_settings(|cur| *cur = s);
                let _ = std::fs::write(&settings_path, json);
            }
        }
    }
}

/// Загрузить основной список и архив одновременно. Основной запрос возвращает и чаты из архива
/// с верным признаком `archived` (по `folder_id` диалога), поэтому он главный; из запроса архива
/// берём только то, чего в основном списке нет (далёкие архивные чаты). Сам запрос архива
/// возвращает и «Избранное», так что его признак не используется.
async fn refresh_dialogs(engine: &Engine, out: &Out, dialogs: &mut Vec<DialogRow>) {
    let (main, arch) = tokio::join!(engine.dialogs(300), engine.archived(100));
    match main {
        Ok(mut d) => {
            for x in arch.unwrap_or_default() {
                if !d.iter().any(|y| y.id == x.id) {
                    d.push(x);
                }
            }
            *dialogs = d.clone();
            out.send(Ev::Dialogs(d));
        }
        Err(e) => out.err(format!("Чаты: {e}")),
    }
}

/// Обновить список чатов, сохранив открытый чат (по идентификатору, не по номеру строки).
async fn refresh_keep(engine: &Engine, out: &Out, dialogs: &mut Vec<DialogRow>, current: &mut Option<usize>) {
    let keep = current.and_then(|i| dialogs.get(i)).map(|d| d.id);
    refresh_dialogs(engine, out, dialogs).await;
    *current = keep.and_then(|id| dialogs.iter().position(|d| d.id == id));
}

/// Перечитать последние сообщения открытого чата.
async fn reload(engine: &Engine, out: &Out, peer: grammers_session::types::PeerRef, idx: usize) {
    if let Ok(rows) = engine.history(peer, 80).await {
        out.send(Ev::History(idx, rows));
    }
}

/// После входа: прогреваем кэш собеседников, запускаем поток обновлений, показываем чаты.
async fn enter(
    engine: &Engine,
    out: &Out,
    updates: &mut Option<tokio::sync::mpsc::Receiver<grammers_session::updates::UpdatesLike>>,
    dialogs: &mut Vec<DialogRow>,
) {
    if let Ok(me) = engine.client.get_me().await {
        out.send(Ev::Me(me.full_name()));
    }
    refresh_dialogs(engine, out, dialogs).await;
    if let Ok(f) = engine.folders().await {
        out.send(Ev::Folders(f));
    }
    if let Some(rx) = updates.take() {
        let eng = engine.clone();
        let ctx = out.ctx.clone();
        let out = out.clone();
        tokio::spawn(async move {
            let probe = eng.clone();
            let _ = eng
                .run_updates(rx, move |upd| {
                    if let Some(ev) = probe.live(upd) {
                        out.send(Ev::Live(ev));
                    }
                    ctx.request_repaint();
                })
                .await;
        });
    }
    out.send(Ev::Stage(Stage::Ready));
}

/// Декодирует фото профиля и обрезает до квадрата `side` × `side`.
fn decode_square(path: &std::path::Path, side: u32) -> Option<eframe::egui::ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.resize_to_fill(side, side, image::imageops::FilterType::Triangle).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// Декодирует картинку и уменьшает до 640 px по большей стороне.
fn decode(path: &std::path::Path) -> Option<eframe::egui::ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(640, 640).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}
