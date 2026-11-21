//! Общий интерфейс Rustgramm для ПК и Android в стиле Telegram Desktop (AyuGram v7.2.9),
//! синий везде заменён на оранжевый. Данные уходят только на серверы Telegram (см. `rg-net`).

pub mod emoji;
pub mod icons;
pub mod theme;
pub mod worker;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Vec2};
use rg_core::engine::{DialogRow, MessageRow};
use rg_core::media::{MediaKind, human_size};
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
    settings: Settings,
    status: String,
    error: String,
    textures: HashMap<i32, egui::TextureHandle>,
    requested: HashSet<i32>,
    key_id: String,
    key_hash: String,
    proxy: String,
    proxy_open: bool,
    qr_cache: Option<(String, QrImage)>,
    now: i64,
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
            settings: Settings::default(),
            status: String::new(),
            error: String::new(),
            textures: HashMap::new(),
            requested: HashSet::new(),
            key_id: String::new(),
            key_hash: String::new(),
            proxy: String::new(),
            proxy_open: false,
            qr_cache: None,
            now: 0,
        }
    }

    fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
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
                    // Предпросмотры только для свежих вложений и по одному разу.
                    let want: Vec<i32> = m
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
                    self.messages = m;
                }
                Ev::History(..) => {}
                Ev::Image(idx, id, img) if self.selected == Some(idx) => {
                    let tex = ctx.load_texture(format!("m{id}"), img, egui::TextureOptions::LINEAR);
                    self.textures.insert(id, tex);
                }
                Ev::Image(..) => {}
                Ev::Settings(s) => self.settings = s,
                Ev::Status(s) => self.status = s,
                Ev::Error(e) => self.error = e,
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

    /// Главное меню (три полосы): режим призрака и локальное хранение.
    fn main_menu(&mut self, resp: &egui::Response) {
        let mut s = self.settings.clone();
        let me = self.me.clone();
        egui::Popup::menu(resp).show(|ui| {
            ui.set_min_width(260.0);
            ui.label(RichText::new(&me).strong().color(WHITE));
            ui.separator();
            let mut ghost = s.ghost.active;
            if ui.checkbox(&mut ghost, "Режим призрака").changed() {
                s.ghost.set_active(ghost);
            }
            ui.checkbox(&mut s.save_deleted, "Сохранять удалённые сообщения");
            ui.checkbox(&mut s.save_edit_history, "История правок");
            let mut silent = s.ghost.should_send_without_sound();
            if ui.checkbox(&mut silent, "Отправлять без звука").changed() {
                s.ghost.send_without_sound = if silent {
                    rg_settings::SendWithoutSound::Always
                } else {
                    rg_settings::SendWithoutSound::Never
                };
            }
            ui.separator();
            ui.weak("Соединения: только серверы Telegram");
            ui.weak("Не передаются: модель устройства, версия ОС,");
            ui.weak("язык системы, «в сети», «прочитано» (в режиме призрака)");
        });
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(s));
        }
    }

    fn dialogs_ui(&mut self, ui: &mut egui::Ui) {
        // Верхняя строка: меню и поле поиска.
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
            if resp.hovered() {
                ui.painter().circle_filled(rect.center(), 20.0, WINDOW_OVER);
            }
            icons::menu(ui.painter(), rect.center(), DIALOG_DATE);
            self.main_menu(&resp);
            let w = ui.available_width() - 8.0;
            let (srect, _) = ui.allocate_exact_size(Vec2::new(w, 36.0), Sense::hover());
            ui.painter()
                .rect_filled(srect, egui::CornerRadius::same(18), FILTER_INPUT);
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
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 0.0;
            for (i, d) in self.dialogs.iter().enumerate() {
                if !needle.is_empty() && !d.name.to_lowercase().contains(&needle) {
                    continue;
                }
                let (rect, resp) = ui.allocate_exact_size(
                    Vec2::new(ui.available_width(), DIALOG_ROW_HEIGHT),
                    Sense::click(),
                );
                let selected = self.selected == Some(i);
                let fill = if selected {
                    DIALOG_ACTIVE
                } else if resp.hovered() {
                    DIALOG_OVER
                } else {
                    Color32::TRANSPARENT
                };
                let p = ui.painter();
                p.rect_filled(rect, 0.0, fill);
                draw_avatar(p, rect.left_center() + Vec2::new(32.0, 0.0), &d.name, d.id, DIALOG_PHOTO);
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
                        UNREAD_MUTED
                    } else {
                        UNREAD
                    };
                    p.rect_filled(b, egui::CornerRadius::same(10), bg);
                    p.text(
                        b.center(),
                        egui::Align2::CENTER_CENTER,
                        label,
                        FontId::proportional(12.0),
                        if selected { DIALOG_ACTIVE } else { Color32::WHITE },
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
                    &d.name,
                    FontId::proportional(15.0),
                    name_c,
                );
                p.with_clip_rect(text_clip).text(
                    rect.left_top() + Vec2::new(68.0, 35.0),
                    egui::Align2::LEFT_TOP,
                    &d.last,
                    FontId::proportional(14.0),
                    text_c,
                );
                if resp.clicked() {
                    open = Some(i);
                }
            }
        });
        if let Some(i) = open {
            self.selected = Some(i);
            self.messages.clear();
            self.textures.clear();
            self.requested.clear();
            self.send(Cmd::Open(i));
        }
    }

    fn header_ui(&mut self, ui: &mut egui::Ui, narrow: bool) {
        ui.horizontal_centered(|ui| {
            ui.add_space(if narrow { 4.0 } else { 12.0 });
            if narrow {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::click());
                icons::back(ui.painter(), rect.center(), WHITE);
                if resp.clicked() {
                    self.selected = None;
                    self.messages.clear();
                    self.textures.clear();
                    self.requested.clear();
                }
            }
            if let Some(d) = self.selected.and_then(|i| self.dialogs.get(i)) {
                avatar(ui, &d.name, d.id, 42.0);
                ui.label(RichText::new(&d.name).strong().size(16.0).color(WHITE));
            }
        });
    }

    fn composer_ui(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add(egui::Button::new(RichText::new("📎").size(20.0).color(DIALOG_DATE)).frame(false));
            let width = (ui.available_width() - 100.0).max(80.0);
            let r = ui.add_sized(
                [width, 36.0],
                egui::TextEdit::singleline(&mut self.input)
                    .hint_text("Написать сообщение…")
                    .frame(egui::Frame::NONE)
                    .margin(Vec2::new(4.0, 8.0)),
            );
            let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
            if ui
                .add(egui::Button::new(RichText::new("😊").size(20.0).color(DIALOG_DATE)).frame(false))
                .clicked()
            {
                self.emoji_open = !self.emoji_open;
            }
            let (srect, sresp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
            let active = !self.input.trim().is_empty();
            icons::send(
                ui.painter(),
                srect.center(),
                if active { ACCENT } else { DIALOG_DATE },
            );
            if (sresp.clicked() || enter) && active && self.selected.is_some() {
                let text = std::mem::take(&mut self.input);
                self.send(Cmd::Send(text));
                r.request_focus();
            }
        });
    }

    fn emoji_window(&mut self, ctx: &egui::Context) {
        if !self.emoji_open {
            return;
        }
        egui::Window::new("emoji")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-70.0, -64.0))
            .show(ctx, |ui| {
                egui::Grid::new("emoji-grid").spacing(Vec2::splat(4.0)).show(ui, |ui| {
                    for (n, e) in emoji::EMOJI.iter().enumerate() {
                        let b = egui::Button::new(RichText::new(*e).size(22.0)).frame(false);
                        if ui.add(b).clicked() {
                            self.input.push_str(e);
                        }
                        if (n + 1) % 8 == 0 {
                            ui.end_row();
                        }
                    }
                });
            });
    }

    fn messages_ui(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            ui.centered_and_justified(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(280.0, 40.0), Sense::hover());
                ui.painter()
                    .rect_filled(rect, egui::CornerRadius::same(20), SERVICE_BG);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Выберите, кому написать",
                    FontId::proportional(14.0),
                    Color32::WHITE,
                );
            });
            return;
        }
        let now = self.now;
        let mut download: Option<i32> = None;
        egui::ScrollArea::vertical()
            .auto_shrink([false; 2])
            .stick_to_bottom(true)
            .show(ui, |ui| {
                let avail = ui.available_width();
                let mut prev_date: Option<i64> = None;
                ui.add_space(8.0);
                for m in &self.messages {
                    if prev_date.is_none_or(|p| !same_day(p, m.date)) {
                        // Разделитель дней: плашка по центру.
                        let label = day_label(m.date, now);
                        let galley = ui.painter().layout_no_wrap(
                            label,
                            FontId::proportional(14.0),
                            Color32::WHITE,
                        );
                        let size = galley.size() + Vec2::new(24.0, 8.0);
                        let (rect, _) =
                            ui.allocate_exact_size(Vec2::new(avail, size.y + 8.0), Sense::hover());
                        let pill = egui::Rect::from_center_size(rect.center(), size);
                        ui.painter()
                            .rect_filled(pill, egui::CornerRadius::same(14), SERVICE_BG);
                        ui.painter().galley(
                            pill.center() - galley.size() / 2.0,
                            galley,
                            Color32::WHITE,
                        );
                    }
                    prev_date = Some(m.date);
                    let layout = if m.outgoing {
                        Layout::right_to_left(Align::TOP)
                    } else {
                        Layout::left_to_right(Align::TOP)
                    };
                    ui.with_layout(layout, |ui| {
                        ui.add_space(6.0);
                        let (fill, text_c, date_c) = if m.outgoing {
                            (MSG_OUT_BG, MSG_OUT_TEXT, MSG_OUT_DATE)
                        } else {
                            (MSG_IN_BG, WHITE, MSG_IN_DATE)
                        };
                        // Ширина пузыря по содержимому, не больше msgMaxWidth.
                        let max_inner = MSG_MAX_WIDTH.min(avail * 0.8) - 2.0 * MSG_PAD_X;
                        let galley = ui.painter().layout(
                            m.text.clone(),
                            FontId::proportional(15.0),
                            text_c,
                            max_inner,
                        );
                        let mut meta = hhmm(m.date);
                        if m.edits > 0 {
                            meta = format!("изм. {meta}");
                        }
                        let tick_w = if m.outgoing { 22.0 } else { 0.0 };
                        let meta_w = meta.chars().count() as f32 * 6.4 + tick_w + 4.0;
                        let mut w = galley.size().x.max(meta_w + 8.0);
                        if m.deleted {
                            w = w.max(60.0);
                        }
                        if m.media.is_some() {
                            w = w.max(if self.textures.contains_key(&m.id) { 240.0 } else { 190.0 });
                        }
                        let w = w.min(max_inner);
                        egui::Frame::new()
                            .fill(fill)
                            .corner_radius(egui::CornerRadius::same(MSG_RADIUS))
                            .inner_margin(egui::Margin::symmetric(MSG_PAD_X as i8, MSG_PAD_Y as i8))
                            .show(ui, |ui| {
                                ui.set_width(w);
                                ui.style_mut().visuals.override_text_color = Some(text_c);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 2.0;
                                    if m.deleted {
                                        ui.label(RichText::new("удалено").color(DANGER).strong());
                                    }
                                    if let Some(tex) = self.textures.get(&m.id) {
                                        ui.add(egui::Image::new(tex).max_width(w).corner_radius(10));
                                    }
                                    if let Some(info) = &m.media {
                                        if !self.textures.contains_key(&m.id) {
                                            let size = if info.size > 0 {
                                                format!(" · {}", human_size(info.size))
                                            } else {
                                                String::new()
                                            };
                                            ui.label(format!("[{}{size}]", info.name));
                                        }
                                        let can_save = matches!(
                                            info.kind,
                                            MediaKind::Photo | MediaKind::Sticker | MediaKind::File
                                        );
                                        if can_save && ui.small_button("Скачать").clicked() {
                                            download = Some(m.id);
                                        }
                                    }
                                    if !m.text.is_empty() {
                                        ui.label(RichText::new(&m.text).color(text_c));
                                    }
                                    ui.horizontal(|ui| {
                                        ui.spacing_mut().item_spacing.x = 4.0;
                                        ui.add_space((w - meta_w).max(0.0));
                                        ui.label(RichText::new(meta).size(12.0).color(date_c));
                                        if m.outgoing {
                                            let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 12.0), Sense::hover());
                                            icons::tick(ui.painter(), r.center(), OUT_TICK, true);
                                        }
                                    });
                                });
                            });
                    });
                    ui.add_space(4.0);
                }
                ui.add_space(8.0);
            });
        if let Some(id) = download {
            self.send(Cmd::Download(id));
        }
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_events(&ctx);
        self.now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(self.now, |d| d.as_secs() as i64);
        if self.stage != Stage::Ready {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(WINDOW_BG))
                .show(ui, |ui| self.login_ui(ui));
            return;
        }
        // Узкое окно (телефон): одна колонка — либо список чатов, либо открытый чат.
        let narrow = ui.available_width() < NARROW_WIDTH;
        if narrow && self.selected.is_none() {
            egui::CentralPanel::default()
                .frame(egui::Frame::new().fill(WINDOW_BG))
                .show(ui, |ui| self.dialogs_ui(ui));
            return;
        }
        if !narrow {
            egui::Panel::left("dialogs")
                .default_size(COLUMN_LEFT_NORMAL)
                .size_range(COLUMN_LEFT_MIN..=COLUMN_LEFT_MAX)
                .frame(egui::Frame::new().fill(WINDOW_BG))
                .show(ui, |ui| self.dialogs_ui(ui));
        }
        egui::Panel::top("header")
            .exact_size(TOP_BAR_HEIGHT)
            .frame(egui::Frame::new().fill(WINDOW_BG))
            .show(ui, |ui| self.header_ui(ui, narrow));
        if !self.status.is_empty() || !self.error.is_empty() {
            egui::Panel::bottom("status")
                .frame(
                    egui::Frame::new()
                        .fill(WINDOW_BG)
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
        if self.selected.is_some() {
            egui::Panel::bottom("composer")
                .exact_size(54.0)
                .frame(
                    egui::Frame::new()
                        .fill(COMPOSE_BG)
                        .inner_margin(egui::Margin::symmetric(8, 0)),
                )
                .show(ui, |ui| self.composer_ui(ui));
        }
        egui::CentralPanel::default()
            .frame(
                egui::Frame::new()
                    .fill(CHAT_BG)
                    .inner_margin(egui::Margin::symmetric(8, 0)),
            )
            .show(ui, |ui| self.messages_ui(ui));
        self.emoji_window(&ctx);
    }
}

/// Настройки окна ПК.
pub fn native_options() -> eframe::NativeOptions {
    eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1100.0, 720.0])
            .with_min_inner_size([420.0, 480.0])
            .with_title("Rustgramm"),
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
        return run_demo(mode.to_string_lossy() == "qr");
    }
    run(native_options())
}

/// Демо-данные для проверки внешнего вида без сети и без аккаунта (`RG_DEMO=1` или `qr`).
fn demo_app(qr: bool) -> App {
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
        });
    }
    app.selected = Some(0);
    let mk = |id: i32, day: i64, text: &str, outgoing, deleted, edits, media| MessageRow {
        id,
        text: text.to_owned(),
        outgoing,
        date: now - day * 86_400 + i64::from(id) * 90,
        deleted,
        edits,
        media,
    };
    app.messages = vec![
        mk(1, 1, "Привет! Ты уже видел новый интерфейс?", false, false, 0, None),
        mk(2, 1, "Да, выглядит как Telegram, только оранжевый 😊", true, false, 0, None),
        mk(3, 0, "Это сообщение удалили, но оно сохранилось", false, true, 0, None),
        mk(4, 0, "Исправил опечатку в тексте", true, false, 2, None),
        mk(
            5,
            0,
            "Вот файл с отчётом, посмотри когда будет время, там всё по новому интерфейсу и скорости",
            false,
            false,
            0,
            Some(MediaInfo {
                kind: MediaKind::File,
                name: "report.pdf".to_owned(),
                size: 1_572_864,
            }),
        ),
        mk(6, 0, "Спасибо, посмотрю вечером 👍", true, false, 0, None),
    ];
    app.settings = Settings::default();
    app.now = now;
    app
}

/// Запуск демо-окна.
pub fn run_demo(qr: bool) -> eframe::Result {
    eframe::run_native(
        "Rustgramm (демо)",
        native_options(),
        Box::new(move |cc| {
            theme::apply(&cc.egui_ctx);
            Ok(Box::new(demo_app(qr)))
        }),
    )
}
