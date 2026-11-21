//! Окно чата: шапка, поиск, пузыри сообщений с контекстным меню, поле ввода.
//! Действия из меню собираются в `Vec<Act>` и выполняются после отрисовки, чтобы не держать
//! заимствования списка сообщений.

use std::collections::HashMap;

use eframe::egui::{self, Align, Color32, FontId, Layout, RichText, Sense, Vec2};
use rg_core::engine::MessageRow;
use rg_core::media::{MediaKind, human_size};
use rg_settings::AyuSettings;

use super::{App, Cmd, icons, theme::*};

/// Быстрые реакции в контекстном меню.
const REACTIONS: [&str; 6] = ["👍", "❤", "🔥", "😂", "😮", "😢"];

/// Действие над сообщением, выбранное пользователем.
pub(crate) enum Act {
    Reply(i32, String),
    Copy(String),
    Edit(i32, String),
    Forward(i32),
    Pin(i32, bool),
    React(i32, &'static str),
    Delete(i32),
    Repeat(String),
    Details(i32),
    Hide(i32),
    AddFilter(String),
    CopyId(i32),
    Save(i32),
    History(i32),
    Goto(i32),
}

/// Всё, что нужно для рисования пузыря, кроме самого сообщения.
struct Env<'a> {
    avail: f32,
    s: &'a AyuSettings,
    tex: &'a HashMap<i32, egui::TextureHandle>,
    shift: bool,
    scroll_to: Option<i32>,
    group: bool,
}

fn time_label(ts: i64, seconds: bool) -> String {
    if seconds { hhmmss(ts) } else { hhmm(ts) }
}

fn short(text: &str, n: usize) -> String {
    let one: String = text.chars().filter(|c| *c != '\n').take(n).collect();
    if text.chars().count() > n { format!("{one}…") } else { one }
}

impl App {
    pub(crate) fn header_ui(&mut self, ui: &mut egui::Ui, narrow: bool) {
        ui.horizontal_centered(|ui| {
            ui.add_space(if narrow { 4.0 } else { 12.0 });
            if narrow {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(44.0), Sense::click());
                icons::back(ui.painter(), rect.center(), WHITE);
                if resp.clicked() {
                    self.close_chat();
                }
            }
            if let Some(d) = self.selected.and_then(|i| self.dialogs.get(i)) {
                avatar(ui, &d.name, d.id, 42.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 0.0;
                    ui.add_space(6.0);
                    ui.label(RichText::new(&d.name).strong().size(16.0).color(WHITE));
                    match self.settings.ayu.show_peer_id {
                        rg_settings::PeerIdDisplay::Hidden => {}
                        rg_settings::PeerIdDisplay::BotApi => {
                            ui.label(RichText::new(format!("ID {}", d.id)).size(12.0).color(SUBTEXT));
                        }
                        rg_settings::PeerIdDisplay::Telegram => {
                            let raw = d.id.unsigned_abs() % 1_000_000_000_000;
                            ui.label(RichText::new(format!("ID {raw}")).size(12.0).color(SUBTEXT));
                        }
                    }
                });
            }
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                ui.add_space(8.0);
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                if resp.hovered() {
                    ui.painter().circle_filled(rect.center(), 20.0, WINDOW_OVER);
                }
                icons::search(ui.painter(), rect.center(), DIALOG_DATE);
                if resp.on_hover_text("Поиск по чату").clicked() {
                    self.search_open = !self.search_open;
                    if !self.search_open {
                        self.chat_search.clear();
                        self.found = None;
                    }
                }
            });
        });
    }

    /// Строка поиска по чату под шапкой.
    pub(crate) fn search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(12.0);
            let w = (ui.available_width() - 120.0).max(80.0);
            let r = ui.add_sized(
                [w, 30.0],
                egui::TextEdit::singleline(&mut self.chat_search)
                    .hint_text("Поиск сообщений")
                    .margin(Vec2::new(8.0, 6.0)),
            );
            r.request_focus();
            let go = ui.button("Найти").clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                self.send(Cmd::Search(self.chat_search.clone()));
            }
            if ui.button("Закрыть").clicked() {
                self.search_open = false;
                self.chat_search.clear();
                self.found = None;
                self.send(Cmd::Search(String::new()));
            }
        });
    }

    /// Панель «ответ / правка» над полем ввода.
    fn context_bar(&mut self, ui: &mut egui::Ui) {
        let (title, body) = if let Some(id) = self.editing {
            ("Редактирование", self.messages.iter().find(|m| m.id == id).map(|m| short(&m.text, 80)))
        } else if let Some((_, snippet)) = &self.reply {
            ("Ответ", Some(short(snippet, 80)))
        } else {
            return;
        };
        let mut cancel = false;
        ui.horizontal(|ui| {
            let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 34.0), Sense::hover());
            ui.painter().rect_filled(bar, 1.5, ACCENT);
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                ui.label(RichText::new(title).color(ACCENT).strong().size(13.0));
                ui.label(RichText::new(body.unwrap_or_default()).color(DIALOG_TEXT).size(13.0));
            });
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                if ui.button("✕").clicked() {
                    cancel = true;
                }
            });
        });
        if cancel {
            self.reply = None;
            if self.editing.take().is_some() {
                self.input.clear();
            }
        }
    }

    pub(crate) fn composer_ui(&mut self, ui: &mut egui::Ui) {
        self.context_bar(ui);
        let s = self.settings.ayu.clone();
        let mut attach = false;
        ui.horizontal(|ui| {
            ui.set_min_height(46.0);
            #[cfg(not(target_os = "android"))]
            if s.field_attach_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
                icons::attach(ui.painter(), rect.center(), if resp.hovered() { ACCENT } else { DIALOG_DATE });
                attach = resp.on_hover_text("Отправить файл").clicked();
            }
            let reserve = 100.0 + if s.field_emoji_button { 36.0 } else { 0.0 };
            let width = (ui.available_width() - reserve).max(80.0);
            let id = ui.make_persistent_id("composer-input");
            let focused = ui.memory(|m| m.has_focus(id));
            // Enter отправляет, Shift+Enter — новая строка.
            let enter = focused
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                && ui.input_mut(|i| i.consume_key(i.modifiers, egui::Key::Enter));
            let r = ui.add_sized(
                [width, 36.0],
                egui::TextEdit::multiline(&mut self.input)
                    .id(id)
                    .desired_rows(1)
                    .hint_text("Написать сообщение…")
                    .frame(egui::Frame::NONE)
                    .margin(Vec2::new(4.0, 8.0)),
            );
            if s.field_emoji_button
                && ui
                    .add(egui::Button::new(RichText::new("😊").size(20.0).color(DIALOG_DATE)).frame(false))
                    .clicked()
            {
                self.emoji_open = !self.emoji_open;
            }
            let (srect, sresp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
            let active = !self.input.trim().is_empty();
            icons::send(ui.painter(), srect.center(), if active { ACCENT } else { DIALOG_DATE });
            if (sresp.clicked() || enter) && active && self.selected.is_some() {
                let text = self.input.trim().to_owned();
                self.input.clear();
                if let Some(id) = self.editing.take() {
                    self.send(Cmd::Edit(id, text));
                } else {
                    let reply = self.reply.take().map(|(id, _)| id);
                    self.send(Cmd::Send(text, reply));
                }
                r.request_focus();
            }
        });
        #[cfg(not(target_os = "android"))]
        if attach
            && self.selected.is_some()
            && let Some(path) = rfd::FileDialog::new().pick_file()
        {
            let reply = self.reply.take().map(|(id, _)| id);
            let caption = std::mem::take(&mut self.input);
            self.send(Cmd::SendFile(path, caption.trim().to_owned(), reply));
        }
        #[cfg(target_os = "android")]
        let _ = attach;
    }

    pub(crate) fn emoji_window(&mut self, ctx: &egui::Context) {
        if !self.emoji_open {
            return;
        }
        egui::Window::new("emoji")
            .title_bar(false)
            .resizable(false)
            .anchor(egui::Align2::RIGHT_BOTTOM, Vec2::new(-70.0, -64.0))
            .show(ctx, |ui| {
                egui::Grid::new("emoji-grid").spacing(Vec2::splat(4.0)).show(ui, |ui| {
                    for (n, e) in super::emoji::EMOJI.iter().enumerate() {
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

    pub(crate) fn messages_ui(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            ui.centered_and_justified(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(280.0, 40.0), Sense::hover());
                ui.painter().rect_filled(rect, egui::CornerRadius::same(20), SERVICE_BG);
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
        let group = self.messages.iter().any(|m| !m.outgoing && !m.sender.is_empty());
        let shift = ui.input(|i| i.modifiers.shift);
        let scroll_to = self.scroll_to.take();
        let mut acts: Vec<Act> = Vec::new();
        let mut want_older = false;
        let searching = self.found.is_some();
        let out = {
            let list: &[MessageRow] = self.found.as_deref().unwrap_or(&self.messages);
            let env = Env {
                avail: ui.available_width(),
                s: &self.settings.ayu,
                tex: &self.textures,
                shift,
                scroll_to,
                group,
            };
            let hidden = &self.hidden;
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(!searching)
                .show(ui, |ui| {
                    let mut prev_date: Option<i64> = None;
                    ui.add_space(8.0);
                    if searching && list.is_empty() {
                        ui.vertical_centered(|ui| ui.label(RichText::new("Ничего не найдено").color(SUBTEXT)));
                    }
                    for m in list.iter().filter(|m| !hidden.contains(&m.id)) {
                        if prev_date.is_none_or(|p| !same_day(p, m.date)) {
                            day_pill(ui, env.avail, day_label(m.date, now));
                        }
                        prev_date = Some(m.date);
                        bubble(ui, m, &env, &mut acts);
                        ui.add_space(4.0);
                    }
                    ui.add_space(8.0);
                })
        };
        // Достигли верха — просим более старую историю.
        if !searching && out.state.clamped_offset().y <= 4.0 && out.content_size.y > out.inner_rect.height() {
            want_older = true;
        }
        if want_older && !self.older_pending && !self.history_end
            && let Some(first) = self.messages.iter().find(|m| !m.deleted)
        {
            self.older_pending = true;
            self.send(Cmd::Older(first.id));
        }
        for a in acts {
            self.apply(ui.ctx(), a);
        }
    }

    fn apply(&mut self, ctx: &egui::Context, a: Act) {
        match a {
            Act::Reply(id, text) => {
                self.editing = None;
                self.reply = Some((id, text));
            }
            Act::Copy(t) => ctx.copy_text(t),
            Act::Edit(id, text) => {
                self.reply = None;
                self.editing = Some(id);
                self.input = text;
            }
            Act::Forward(id) => self.forward = Some(id),
            Act::Pin(id, on) => self.send(Cmd::Pin(id, on)),
            Act::React(id, e) => self.send(Cmd::React(id, e.to_owned())),
            Act::Delete(id) => self.confirm_delete = Some(id),
            Act::Repeat(text) => self.send(Cmd::Send(text, None)),
            Act::Details(id) => {
                self.details = Some(id);
                self.edit_history = None;
                self.send(Cmd::EditHistory(id));
            }
            Act::Hide(id) => {
                self.hidden.insert(id);
            }
            Act::AddFilter(text) => {
                if !text.is_empty() && !self.settings.filters.iter().any(|f| f.text == text) {
                    self.settings.filters.push(rg_settings::Filter {
                        text,
                        ..rg_settings::Filter::default()
                    });
                    self.settings.ayu.filters_enabled = true;
                    self.send(Cmd::Settings(Box::new(self.settings.clone())));
                    self.status = "Фильтр добавлен: Настройки → Фильтры".to_owned();
                }
            }
            Act::CopyId(id) => ctx.copy_text(id.to_string()),
            Act::Save(id) => self.send(Cmd::Download(id)),
            Act::History(id) => {
                self.details = Some(id);
                self.send(Cmd::EditHistory(id));
            }
            Act::Goto(id) => self.scroll_to = Some(id),
        }
    }

    /// Окна поверх чата: пересылка, подтверждение удаления, сведения о сообщении.
    pub(crate) fn chat_windows(&mut self, ctx: &egui::Context) {
        if let Some(id) = self.forward {
            let mut open = true;
            let mut chosen = None;
            egui::Window::new("Переслать")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.forward_filter)
                            .hint_text("Поиск чата")
                            .desired_width(300.0),
                    );
                    let needle = self.forward_filter.to_lowercase();
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for (i, d) in self.dialogs.iter().enumerate() {
                            if !needle.is_empty() && !d.name.to_lowercase().contains(&needle) {
                                continue;
                            }
                            if ui.selectable_label(false, &d.name).clicked() {
                                chosen = Some(i);
                            }
                        }
                    });
                });
            if let Some(i) = chosen {
                self.send(Cmd::Forward(id, i));
                open = false;
            }
            if !open {
                self.forward = None;
                self.forward_filter.clear();
            }
        }
        if let Some(id) = self.confirm_delete {
            let mut close = false;
            egui::Window::new("Удалить сообщение?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Сообщение удалится в Telegram. Копия останется у вас, если включено «Сохранять удалённые».");
                    ui.horizontal(|ui| {
                        if ui.button(RichText::new("Удалить").color(DANGER)).clicked() {
                            self.send(Cmd::Delete(id));
                            close = true;
                        }
                        if ui.button("Отмена").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.confirm_delete = None;
            }
        }
        if let Some(id) = self.details {
            let mut open = true;
            let m = self.messages.iter().chain(self.found.iter().flatten()).find(|m| m.id == id).cloned();
            egui::Window::new("Сведения о сообщении")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    let Some(m) = m else {
                        ui.label("Сообщение недоступно");
                        return;
                    };
                    ui.label(format!("ID: {}", m.id));
                    ui.label(format!("Отправлено: {}", full_date(m.date)));
                    if let Some(e) = m.edit_date {
                        ui.label(format!("Изменено: {}", full_date(e)));
                    }
                    if !m.sender.is_empty() {
                        ui.label(format!("Отправитель: {}", m.sender));
                    }
                    if let Some(f) = &m.forwarded_from {
                        ui.label(format!("Переслано от: {f}"));
                    }
                    if let Some(v) = m.views {
                        ui.label(format!("Просмотры: {v}"));
                    }
                    if m.reactions > 0 {
                        ui.label(format!("Реакции: {}", m.reactions));
                    }
                    if let Some((eid, versions)) = &self.edit_history
                        && *eid == id
                        && !versions.is_empty()
                    {
                        ui.separator();
                        ui.label(RichText::new("Прежние версии").strong());
                        for (n, v) in versions.iter().enumerate() {
                            ui.label(format!("{}. {}", n + 1, short(v, 200)));
                        }
                    }
                });
            if !open {
                self.details = None;
                self.edit_history = None;
            }
        }
    }
}

fn day_pill(ui: &mut egui::Ui, avail: f32, label: String) {
    let galley = ui
        .painter()
        .layout_no_wrap(label, FontId::proportional(14.0), Color32::WHITE);
    let size = galley.size() + Vec2::new(24.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, size.y + 8.0), Sense::hover());
    let pill = egui::Rect::from_center_size(rect.center(), size);
    ui.painter().rect_filled(pill, egui::CornerRadius::same(14), SERVICE_BG);
    ui.painter()
        .galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// Один пузырь сообщения с контекстным меню.
fn bubble(ui: &mut egui::Ui, m: &MessageRow, env: &Env, acts: &mut Vec<Act>) {
    let s = env.s;
    let layout = if m.outgoing {
        Layout::right_to_left(Align::TOP)
    } else {
        Layout::left_to_right(Align::TOP)
    };
    ui.with_layout(layout, |ui| {
        ui.add_space(6.0);
        let (mut fill, text_c, date_c) = if m.outgoing {
            (MSG_OUT_BG, MSG_OUT_TEXT, MSG_OUT_DATE)
        } else {
            (MSG_IN_BG, WHITE, MSG_IN_DATE)
        };
        if m.deleted && s.semi_transparent_deleted_messages {
            fill = fill.gamma_multiply(0.55);
        }
        let max_inner = (MSG_MAX_WIDTH * s.wide_multiplier()).min(env.avail * 0.8) - 2.0 * MSG_PAD_X;
        let galley = ui
            .painter()
            .layout(m.text.clone(), FontId::proportional(15.0), text_c, max_inner);
        let edited = m.edit_date.is_some() || m.edits > 0;
        let mut meta = time_label(m.date, s.show_message_seconds);
        if edited {
            let mark = if s.edited_mark.is_empty() { "изм." } else { s.edited_mark.as_str() };
            meta = format!("{mark} {meta}");
        }
        let tick_w = if m.outgoing { 22.0 } else { 0.0 };
        let meta_w = meta.chars().count() as f32 * 6.4 + tick_w + 4.0;
        let mut w = galley.size().x.max(meta_w + 8.0);
        if m.deleted {
            w = w.max(90.0);
        }
        if m.reply_to.is_some() || m.forwarded_from.is_some() {
            w = w.max(180.0);
        }
        if m.media.is_some() {
            w = w.max(if env.tex.contains_key(&m.id) { 240.0 } else { 190.0 });
        }
        let w = w.min(max_inner);
        let radius = egui::CornerRadius::same(s.bubble_radius());
        let inner = egui::Frame::new()
            .fill(fill)
            .corner_radius(radius)
            .inner_margin(egui::Margin::symmetric(MSG_PAD_X as i8, MSG_PAD_Y as i8))
            .show(ui, |ui| {
                ui.set_width(w);
                ui.style_mut().visuals.override_text_color = Some(text_c);
                ui.style_mut().interaction.selectable_labels = false;
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    if env.group && !m.outgoing && !m.sender.is_empty() {
                        let c = avatar_color(m.sender.bytes().fold(7i64, |a, b| a * 31 + i64::from(b)));
                        ui.label(RichText::new(&m.sender).strong().size(13.0).color(c));
                    }
                    if let Some(f) = &m.forwarded_from {
                        ui.label(RichText::new(format!("Переслано от {f}")).size(12.0).color(ACCENT));
                    }
                    if let Some((rid, snippet)) = &m.reply_to {
                        let bar_c = if s.show_colorful_replies { ACCENT } else { date_c };
                        let resp = ui
                            .horizontal(|ui| {
                                let (bar, _) = ui.allocate_exact_size(Vec2::new(2.5, 30.0), Sense::hover());
                                ui.painter().rect_filled(bar, 1.0, bar_c);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(RichText::new("Ответ").size(12.0).color(bar_c));
                                    let t = if snippet.is_empty() { "сообщение" } else { snippet.as_str() };
                                    ui.label(RichText::new(short(t, 60)).size(13.0).color(date_c));
                                });
                            })
                            .response;
                        if ui.interact(resp.rect, ui.make_persistent_id(("q", m.id)), Sense::click()).clicked() {
                            acts.push(Act::Goto(*rid));
                        }
                    }
                    if m.deleted {
                        let mark = if s.deleted_mark.is_empty() { String::new() } else { format!("{} ", s.deleted_mark) };
                        ui.label(RichText::new(format!("{mark}удалено")).color(DANGER).strong());
                    }
                    if let Some(tex) = env.tex.get(&m.id) {
                        ui.add(egui::Image::new(tex).max_width(w).corner_radius(10));
                    }
                    if let Some(info) = &m.media
                        && !env.tex.contains_key(&m.id)
                    {
                        let size = if info.size > 0 {
                            format!(" · {}", human_size(info.size))
                        } else {
                            String::new()
                        };
                        ui.label(format!("[{}{size}]", info.name));
                    }
                    if !m.text.is_empty() {
                        ui.label(RichText::new(&m.text).color(text_c));
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let mut extra = String::new();
                        if m.pinned {
                            extra.push_str("📌 ");
                        }
                        if m.reactions > 0 && reactions_visible(s) {
                            extra.push_str(&format!("♥ {} ", m.reactions));
                        }
                        if let Some(v) = m.views {
                            extra.push_str(&format!("👁 {v} "));
                        }
                        let extra_w = extra.chars().count() as f32 * 7.0;
                        ui.add_space((w - meta_w - extra_w).max(0.0));
                        if !extra.is_empty() {
                            ui.label(RichText::new(extra).size(12.0).color(date_c));
                        }
                        ui.label(RichText::new(meta).size(12.0).color(date_c));
                        if m.outgoing {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 12.0), Sense::hover());
                            icons::tick(ui.painter(), r.center(), OUT_TICK, true);
                        }
                    });
                });
            });
        let rect = inner.response.rect;
        let resp = ui.interact(rect, ui.make_persistent_id(("m", m.id)), Sense::click());
        if env.scroll_to == Some(m.id) {
            resp.scroll_to_me(Some(Align::Center));
        }
        egui::Popup::context_menu(&resp).show(|ui| menu(ui, m, env, acts));
    });
}

/// Показывать ли счётчик реакций у сообщений.
fn reactions_visible(s: &AyuSettings) -> bool {
    s.show_private_chat_reactions || s.show_group_reactions || s.show_channel_reactions
}

/// Контекстное меню сообщения: обычные пункты Telegram и пункты AyuGram по их видимости.
fn menu(ui: &mut egui::Ui, m: &MessageRow, env: &Env, acts: &mut Vec<Act>) {
    let s = env.s;
    ui.set_min_width(210.0);
    if s.menu_reactions_panel.shown(env.shift) && !m.deleted {
        ui.horizontal(|ui| {
            for e in REACTIONS {
                if ui.add(egui::Button::new(RichText::new(e).size(20.0)).frame(false)).clicked() {
                    acts.push(Act::React(m.id, e));
                    ui.close();
                }
            }
        });
        ui.separator();
    }
    let mut item = |ui: &mut egui::Ui, label: &str, act: Act| {
        if ui.button(label).clicked() {
            acts.push(act);
            ui.close();
        }
    };
    if !m.deleted {
        item(ui, "Ответить", Act::Reply(m.id, m.text.clone()));
    }
    if !m.text.is_empty() {
        item(ui, "Копировать текст", Act::Copy(m.text.clone()));
    }
    if m.media.as_ref().is_some_and(|i| matches!(i.kind, MediaKind::Photo | MediaKind::Sticker | MediaKind::File)) {
        item(ui, "Сохранить файл", Act::Save(m.id));
    }
    if m.outgoing && !m.deleted {
        item(ui, "Изменить", Act::Edit(m.id, m.text.clone()));
    }
    if !m.deleted {
        item(ui, "Переслать", Act::Forward(m.id));
        item(
            ui,
            if m.pinned { "Открепить" } else { "Закрепить" },
            Act::Pin(m.id, !m.pinned),
        );
    }
    if s.menu_repeat_message.shown(env.shift) && !m.text.is_empty() {
        item(ui, "Повторить", Act::Repeat(m.text.clone()));
    }
    if s.menu_add_filter.shown(env.shift) && !m.text.is_empty() {
        item(ui, "Добавить в фильтры", Act::AddFilter(m.text.clone()));
    }
    if s.menu_hide_message.shown(env.shift) {
        item(ui, "Скрыть сообщение", Act::Hide(m.id));
    }
    if m.edits > 0 {
        item(ui, "История правок", Act::History(m.id));
    }
    if s.menu_message_details.shown(env.shift) {
        item(ui, "Сведения", Act::Details(m.id));
        item(ui, "Копировать ID", Act::CopyId(m.id));
    }
    if !m.deleted {
        ui.separator();
        if ui.button(RichText::new("Удалить").color(DANGER)).clicked() {
            acts.push(Act::Delete(m.id));
            ui.close();
        }
    }
}
