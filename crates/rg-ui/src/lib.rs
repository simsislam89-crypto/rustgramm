//! Общий интерфейс Rustgramm для ПК и Android в стиле Telegram Desktop (AyuGram v7.2.9),
//! синий везде заменён на оранжевый. Данные уходят только на серверы Telegram (см. `rg-net`).

mod chat;
pub mod emoji;
mod frame;
pub mod icons;
mod kit;
mod media_ui;
mod rich;
mod settings_ui;
pub mod theme;
mod win;
pub mod worker;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Color32, FontId, RichText, Sense, Vec2};
use rg_core::engine::{DialogRow, MessageRow};
use rg_settings::Settings;
use tokio::sync::mpsc::UnboundedSender;

pub use worker::{Cmd, Ev, Stage};

use theme::*;

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
    ui.add(
        egui::Button::new(RichText::new(text).color(Color32::WHITE).strong())
            .fill(BUTTON)
            .min_size(Vec2::new(280.0, 40.0)),
    )
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
            known: HashSet::new(),
            arrived: HashMap::new(),
            chat_opened: std::time::Instant::now(),
            logout_confirm: false,
            shadow_new: String::new(),
            expanded: 0,
            older_pending: false,
            history_end: false,
            scroll_to: None,
        }
    }

    fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }

    /// Предпросмотры только для свежих вложений и по одному разу.
    fn request_previews(&mut self, idx: usize, rows: &[MessageRow]) {
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
    }

    /// Открыть чат из списка.
    fn open_dialog(&mut self, i: usize) {
        self.reset_chat_state();
        self.selected = Some(i);
        self.send(Cmd::Open(i));
    }

    /// Закрыть открытый чат и сбросить состояние ленты.
    fn close_chat(&mut self) {
        self.selected = None;
        self.reset_chat_state();
    }

    fn reset_chat_state(&mut self) {
        self.known.clear();
        self.arrived.clear();
        self.chat_opened = std::time::Instant::now();
        self.messages.clear();
        self.textures.clear();
        self.requested.clear();
        self.found = None;
        self.reply = None;
        self.editing = None;
        self.search_open = false;
        self.chat_search.clear();
        self.hidden.clear();
        self.older_pending = false;
        self.history_end = false;
        self.scroll_to = None;
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(e) = self.ev.try_recv() {
            match e {
                Ev::Stage(s) => {
                    self.stage = s;
                    self.error.clear();
                }
                Ev::Me(n) => self.me = n,
                Ev::Dialogs(d) => self.dialogs = d,
                Ev::History(idx, m) if self.selected == Some(idx) => {
                    // Новые сообщения после первой загрузки появляются плавно.
                    let now_t = ctx.input(|i| i.time);
                    if !self.known.is_empty() {
                        for r in &m {
                            if !self.known.contains(&r.id) {
                                self.arrived.insert(r.id, now_t);
                            }
                        }
                    }
                    self.known.extend(m.iter().map(|r| r.id));
                    self.request_previews(idx, &m);
                    self.messages = m;
                    self.older_pending = false;
                }
                Ev::Older(idx, rows) if self.selected == Some(idx) => {
                    self.older_pending = false;
                    if rows.is_empty() {
                        self.history_end = true;
                    } else {
                        self.scroll_to = self.messages.first().map(|m| m.id);
                        self.request_previews(idx, &rows);
                        let known: HashSet<i32> = self.messages.iter().map(|m| m.id).collect();
                        let mut all: Vec<MessageRow> =
                            rows.into_iter().filter(|r| !known.contains(&r.id)).collect();
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
                }
                Ev::Image(..) => {}
                Ev::Avatar(id, img) => {
                    let tex = ctx.load_texture(format!("a{id}"), *img, egui::TextureOptions::LINEAR);
                    self.avatars.insert(id, tex);
                }
                Ev::Settings(s) => self.settings = *s,
                Ev::Status(s) => self.status = s,
                Ev::Error(e) => {
                    self.error = e;
                    self.error_at = self.now;
                }
            }
        }
    }

    // ---------- вход ----------

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        // Ссылка «Настройки» в правом верхнем углу, как у Telegram Desktop.
        let top = ui.max_rect().right_top() + Vec2::new(-24.0, 22.0);
        let rect = egui::Rect::from_min_size(top - Vec2::new(120.0, 0.0), Vec2::new(120.0, 22.0));
        let r = ui.interact(rect, ui.make_persistent_id("login-settings"), Sense::click());
        ui.painter().text(
            rect.right_center(),
            egui::Align2::RIGHT_CENTER,
            "НАСТРОЙКИ",
            FontId::proportional(14.0),
            ACCENT,
        );
        if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.proxy_open = !self.proxy_open;
        }
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.vertical_centered(|ui| {
                ui.add_space((ui.available_height() * 0.08).max(16.0));
                match self.stage.clone() {
                    Stage::Qr(url) => self.qr_screen(ui, &url),
                    _ => self.form_screen(ui),
                }
                if !self.error.is_empty() {
                    ui.add_space(10.0);
                    ui.colored_label(DANGER, &self.error);
                }
                ui.add_space(24.0);
                ui.label(
                    RichText::new(format!("Rustgramm v{}", env!("CARGO_PKG_VERSION")))
                        .color(SUBTEXT)
                        .size(13.0),
                );
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
                ui.label(RichText::new("Ваш номер телефона").size(18.0).color(WHITE));
                ui.label(RichText::new("Укажите код страны и номер").color(SUBTEXT));
                ui.add_space(8.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.phone)
                        .hint_text("+7 …")
                        .desired_width(280.0)
                        .margin(Vec2::new(10.0, 9.0)),
                );
                ui.add_space(6.0);
                let go = primary_button(ui, "ДАЛЕЕ").clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.phone.trim().is_empty() {
                    self.send(Cmd::RequestCode(self.phone.trim().to_owned()));
                }
                ui.add_space(18.0);
                if link(ui, "Войти по QR-коду").clicked() {
                    self.send(Cmd::UseQr);
                }
            }
            Stage::NeedCode => {
                ui.label(RichText::new("Введите код").size(18.0).color(WHITE));
                ui.label(RichText::new("Мы отправили код в приложение Telegram").color(SUBTEXT));
                ui.add_space(8.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.code)
                        .desired_width(280.0)
                        .margin(Vec2::new(10.0, 9.0)),
                );
                ui.add_space(6.0);
                let go = primary_button(ui, "ВОЙТИ").clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.code.trim().is_empty() {
                    self.send(Cmd::Code(self.code.trim().to_owned()));
                    self.code.clear();
                }
            }
            Stage::NeedPassword(hint) => {
                ui.label(RichText::new("Облачный пароль").size(18.0).color(WHITE));
                ui.label(RichText::new(format!("Подсказка: {hint}")).color(SUBTEXT));
                ui.add_space(8.0);
                let r = ui.add(
                    egui::TextEdit::singleline(&mut self.password)
                        .password(true)
                        .desired_width(280.0)
                        .margin(Vec2::new(10.0, 9.0)),
                );
                ui.add_space(6.0);
                let go = primary_button(ui, "ПРОДОЛЖИТЬ").clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && !self.password.is_empty() {
                    let pw = std::mem::take(&mut self.password);
                    self.send(Cmd::Password(pw));
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
                if ui.button("Сохранить").clicked() {
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
            let w = ui.available_width() - 8.0;
            let (srect, _) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::hover());
            ui.painter()
                .rect_filled(srect, egui::CornerRadius::same(18), t(FILTER_INPUT));
            icons::search(ui.painter(), srect.left_center() + Vec2::new(20.0, 0.0), DIALOG_DATE);
            let inner = srect.shrink2(Vec2::new(38.0, 4.0));
            ui.put(
                inner,
                egui::TextEdit::singleline(&mut self.search)
                    .hint_text("Поиск")
                    .frame(egui::Frame::NONE)
                    .desired_width(inner.width()),
            );
        });
        ui.add_space(4.0);
        let needle = self.search.to_lowercase();
        let now = self.now;
        let mut open = None;
        let mut dact: Option<(usize, DialogAct)> = None;
        let mut toggle_archive = false;
        let archived_total = self.dialogs.iter().filter(|d| d.archived).count();
        let in_archive = self.in_archive;
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            if in_archive {
                if archive_header(ui) {
                    toggle_archive = true;
                }
            } else if archived_total > 0 && needle.is_empty() && archive_row(ui, &self.dialogs) {
                toggle_archive = true;
            }
            for (i, d) in self.dialogs.iter().enumerate() {
                if d.archived != in_archive {
                    continue;
                }
                if !needle.is_empty() && !d.name.to_lowercase().contains(&needle) {
                    continue;
                }
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
                if ui.is_rect_visible(rect) && self.avatar_req.insert(d.id) {
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
                if d.unread > 0 {
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
                        UNREAD
                    };
                    p.rect_filled(b, egui::CornerRadius::same(10), bg);
                    p.text(
                        b.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(12.0),
                        if selected { t(DIALOG_ACTIVE) } else { INK_DARK },
                    );
                    right = b.left() - 6.0;
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
                p.with_clip_rect(name_clip).text(
                    rect.left_top() + Vec2::new(68.0, 12.0),
                    egui::Align2::LEFT_TOP,
                    if d.saved { "Избранное" } else { &d.name },
                    FontId::proportional(15.0),
                    name_c,
                );
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
                if resp.clicked() {
                    open = Some(i);
                }
                egui::Popup::context_menu(&resp).show(|ui| {
                    ui.set_min_width(190.0);
                    let mut item = |ui: &mut egui::Ui, label: &str, a: DialogAct| {
                        if ui.button(label).clicked() {
                            dact = Some((i, a));
                            ui.close();
                        }
                    };
                    item(ui, if d.pinned { "Открепить" } else { "Закрепить" }, DialogAct::Pin(!d.pinned));
                    item(ui, if d.muted { "Включить звук" } else { "Отключить звук" }, DialogAct::Mute(!d.muted));
                    if d.unread > 0 {
                        item(ui, "Прочитать на сервере", DialogAct::Read);
                    }
                    if !d.saved {
                        item(
                            ui,
                            if d.archived { "Вернуть из архива" } else { "В архив" },
                            DialogAct::Archive(!d.archived),
                        );
                    }
                });
            }
        });
        if toggle_archive {
            self.in_archive = !self.in_archive;
        }
        if let Some((i, a)) = dact {
            match a {
                DialogAct::Pin(on) => self.send(Cmd::PinDialog(i, on)),
                DialogAct::Mute(on) => self.send(Cmd::MuteDialog(i, on)),
                DialogAct::Read => self.send(Cmd::ReadDialog(i)),
                DialogAct::Archive(on) => self.send(Cmd::Archive(i, on)),
            }
        }
        if let Some(i) = open {
            self.open_dialog(i);
        }
    }
}

/// Действие из контекстного меню строки списка чатов.
enum DialogAct {
    Pin(bool),
    Mute(bool),
    Read,
    Archive(bool),
}

/// Значок папки «Архив»: скруглённый квадрат со стрелкой вниз в коробку.
fn paint_archive_icon(p: &egui::Painter, c: egui::Pos2, size: f32) {
    p.circle_filled(c, size / 2.0, Color32::from_rgb(70, 70, 78));
    let s = egui::Stroke::new(1.8, Color32::WHITE);
    let k = size / 48.0;
    let top = c + Vec2::new(0.0, -4.0) * k;
    p.line_segment([top + Vec2::new(0.0, -8.0) * k, top + Vec2::new(0.0, 6.0) * k], s);
    p.line_segment([top + Vec2::new(-5.0, 1.0) * k, top + Vec2::new(0.0, 6.0) * k], s);
    p.line_segment([top + Vec2::new(5.0, 1.0) * k, top + Vec2::new(0.0, 6.0) * k], s);
    p.line_segment([c + Vec2::new(-9.0, 9.0) * k, c + Vec2::new(9.0, 9.0) * k], s);
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

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        theme::set_opacity(self.settings.window_opacity);
        if !self.emoji_tried {
            self.emoji_tried = true;
            self.emoji = emoji::Emoji::load(&ctx);
        }
        self.drain_events(&ctx);
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
                .frame(egui::Frame::new().fill(t(TITLE_BG)))
                .show(ui, |ui| self.titlebar(ui));
            self.resize_edges(ui);
        }
        if self.stage != Stage::Ready {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t(WINDOW_BG)))
                .show(ui, |ui| self.login_ui(ui));
            return;
        }
        // Узкое окно (телефон): одна колонка — либо список чатов, либо открытый чат.
        let narrow = ui.available_width() < NARROW_WIDTH;
        if narrow && self.selected.is_none() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(t(WINDOW_BG)))
                .show(ui, |ui| self.dialogs_ui(ui));
            return;
        }
        if !narrow {
            egui::Panel::left("dialogs")
                .default_size(COLUMN_LEFT_NORMAL)
                .size_range(COLUMN_LEFT_MIN..=COLUMN_LEFT_MAX)
                .frame(egui::Frame::new().fill(t(WINDOW_BG)))
                .show(ui, |ui| self.dialogs_ui(ui));
        }
        egui::Panel::top("header")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(egui::Frame::new().fill(t(WINDOW_BG)))
            .show(ui, |ui| self.header_ui(ui, narrow));
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
                .frame(egui::Frame::new().fill(t(WINDOW_BG)))
                .show(ui, |ui| self.search_bar(ui));
        }
        if self.selected.is_some() {
            egui::Panel::bottom("composer")
                .frame(
                    egui::Frame::new()
                        .fill(t(COMPOSE_BG))
                        .inner_margin(egui::Margin::symmetric(8, 4)),
                )
                .show(ui, |ui| self.composer_ui(ui));
        }
        // Лента въезжает справа с пружиной при открытии чата (переход как на iPhone).
        let push = (self.chat_opened.elapsed().as_secs_f32() / 0.5).clamp(0.0, 1.0);
        if push < 1.0 && self.selected.is_some() {
            ctx.request_repaint();
        }
        let slide = if self.selected.is_some() { (1.0 - spring(push)) * 36.0 } else { 0.0 };
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(t(CHAT_BG))
                    .inner_margin(egui::Margin {
                        left: (8.0 + slide.max(0.0)) as i8,
                        right: 8,
                        top: 0,
                        bottom: 0,
                    }),
            )
            .show(ui, |ui| self.messages_ui(ui));
        self.emoji_window(&ctx);
        self.chat_windows(&ctx);
        self.viewer_window(&ctx);
        self.drawer(&ctx);
        self.settings_window(&ctx);
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
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size(size)
            .with_min_inner_size([420.0, 480.0])
            .with_title("Rustgramm")
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
