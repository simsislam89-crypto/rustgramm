//! Окно чата: шапка, поиск, пузыри сообщений с контекстным меню, поле ввода.
//! Действия из меню собираются в `Vec<Act>` и выполняются после отрисовки, чтобы не держать
//! заимствования списка сообщений.

use std::collections::HashMap;

use eframe::egui::{self, Align, Color32, FontId, Layout, Pos2, Rect, RichText, Sense, Vec2};
use rg_core::engine::MessageRow;
use rg_settings::AyuSettings;

use super::{App, Cmd, icons, kit, theme::*};

/// Быстрые реакции в контекстном меню.
const REACTIONS: [&str; 6] = ["👍", "❤", "🔥", "😂", "😮", "😢"];

/// Действие над сообщением, выбранное пользователем.
#[derive(Clone)]
pub(crate) enum Act {
    /// Скачать вложение и открыть.
    Open(i32),
    /// Открыть картинку в просмотрщике.
    Viewer(i32),
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
    OpenUrl(String),
    Spoiler(i32, usize),
}

/// Всё, что нужно для рисования пузыря, кроме самого сообщения.
struct Env<'a> {
    avail: f32,
    s: &'a AyuSettings,
    tex: &'a HashMap<i32, egui::TextureHandle>,
    shift: bool,
    scroll_to: Option<i32>,
    group: bool,
    arrived: &'a HashMap<i32, f64>,
    time: f64,
    emoji: Option<&'a super::emoji::Emoji>,
    spoilers: &'a std::collections::HashSet<(i32, usize)>,
    read_out: i32,
    avatars: &'a HashMap<i64, egui::TextureHandle>,
    /// Сообщения, у которых слева рисуется аватарка отправителя (последнее в серии).
    avatar_msgs: &'a std::collections::HashSet<i32>,
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
                avatar(ui, &d.name, d.id, 42.0, self.avatars.get(&d.id), self.settings.ayu.avatar_corners);
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
                // Три точки: меню чата.
                let (rect, dots) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                if dots.hovered() {
                    ui.painter().circle_filled(rect.center(), 20.0, t(WINDOW_OVER));
                }
                icons::dots(ui.painter(), rect.center(), if dots.hovered() { WHITE } else { DIALOG_DATE });
                let sel = self.selected.and_then(|i| self.dialogs.get(i).map(|d| (i, d.pinned, d.muted, d.archived, d.saved)));
                let mut act: Option<Cmd> = None;
                let mut search = false;
                egui::Popup::menu(&dots.on_hover_cursor(egui::CursorIcon::PointingHand)).show(|ui| {
                    ui.set_min_width(210.0);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    if kit::menu_item(ui, "menu/search", "Поиск", false) {
                        search = true;
                        ui.close();
                    }
                    if let Some((i, pinned, muted, archived, saved)) = sel {
                        if kit::menu_item(ui, if pinned { "menu/unpin" } else { "menu/pin" }, if pinned { "Открепить" } else { "Закрепить" }, false) {
                            act = Some(Cmd::PinDialog(i, !pinned));
                            ui.close();
                        }
                        if kit::menu_item(ui, if muted { "menu/unmute" } else { "menu/mute" }, if muted { "Включить звук" } else { "Отключить звук" }, false) {
                            act = Some(Cmd::MuteDialog(i, !muted));
                            ui.close();
                        }
                        if kit::menu_item(ui, "menu/read", "Прочитать на сервере", false) {
                            act = Some(Cmd::ForceRead);
                            ui.close();
                        }
                        if !saved
                            && kit::menu_item(
                                ui,
                                if archived { "menu/unarchive" } else { "menu/archive" },
                                if archived { "Вернуть из архива" } else { "В архив" },
                                false,
                            )
                        {
                            act = Some(Cmd::Archive(i, !archived));
                            ui.close();
                        }
                    }
                });
                if let Some(cmd) = act {
                    self.send(cmd);
                }
                // Переключатель правой панели со сведениями о чате.
                let (prect, ptog) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                if ptog.hovered() {
                    ui.painter().circle_filled(prect.center(), 20.0, t(WINDOW_OVER));
                }
                {
                    let col = if self.info_open || ptog.hovered() { WHITE } else { DIALOG_DATE };
                    let r = Rect::from_center_size(prect.center(), Vec2::new(18.0, 15.0));
                    ui.painter().rect_stroke(r, egui::CornerRadius::same(3), egui::Stroke::new(1.5, col), egui::StrokeKind::Middle);
                    let strip = Rect::from_min_max(egui::pos2(r.right() - 6.0, r.top() + 1.0), egui::pos2(r.right() - 1.0, r.bottom() - 1.0));
                    ui.painter().rect_filled(strip, egui::CornerRadius::same(1), col);
                }
                if ptog.on_hover_text("Сведения о чате").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.info_open = !self.info_open;
                }
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                if resp.hovered() {
                    ui.painter().circle_filled(rect.center(), 20.0, t(WINDOW_OVER));
                }
                icons::search(ui.painter(), rect.center(), if resp.hovered() { WHITE } else { DIALOG_DATE });
                if resp.on_hover_text("Поиск по чату").clicked() || search {
                    self.search_open = !self.search_open;
                    if !self.search_open {
                        self.chat_search.clear();
                        self.found = None;
                    }
                }
            });
        });
    }

    /// Правая панель: аватар, имя, статус и сведения о собеседнике, группе или канале.
    pub(crate) fn info_ui(&mut self, ui: &mut egui::Ui) {
        let Some((i, d)) = self.selected.and_then(|i| self.dialogs.get(i).map(|d| (i, d.clone()))) else { return };
        if self.info_req.insert(d.id) {
            self.send(Cmd::PeerInfo(i));
        }
        let info = self.peer_infos.get(&d.id).cloned();
        let w = ui.available_width();
        ui.set_clip_rect(ui.max_rect());
        let (head, _) = ui.allocate_exact_size(Vec2::new(w, 54.0), Sense::hover());
        ui.painter().text(
            head.left_center() + Vec2::new(18.0, 0.0),
            egui::Align2::LEFT_CENTER,
            if d.id < 0 { "Информация" } else { "Профиль" },
            FontId::proportional(16.0),
            WHITE,
        );
        let x = Rect::from_center_size(head.right_center() - Vec2::new(26.0, 0.0), Vec2::splat(36.0));
        let xr = ui.interact(x, ui.make_persistent_id("info-close"), Sense::click());
        icons::tg(ui.painter(), "info/info_close", x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
        if xr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.info_open = false;
        }
        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (top, _) = ui.allocate_exact_size(Vec2::new(w, 168.0), Sense::hover());
            let c = Pos2::new(top.center().x, top.top() + 52.0);
            let r = Rect::from_center_size(c, Vec2::splat(88.0));
            match self.avatars.get(&d.id) {
                Some(t) => {
                    egui::Image::new(t).fit_to_exact_size(r.size()).corner_radius(44.0).paint_at(ui, r);
                }
                None => {
                    ui.painter().circle_filled(c, 44.0, avatar_color(d.id));
                    ui.painter().text(
                        c,
                        egui::Align2::CENTER_CENTER,
                        d.name.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_default(),
                        FontId::proportional(36.0),
                        Color32::WHITE,
                    );
                }
            }
            ui.painter().text(
                Pos2::new(top.center().x, top.top() + 120.0),
                egui::Align2::CENTER_CENTER,
                &d.name,
                FontId::proportional(18.0),
                WHITE,
            );
            let sub = info
                .as_ref()
                .map(|x| {
                    if x.members > 0 {
                        format!("{} участников", x.members)
                    } else {
                        x.status.clone()
                    }
                })
                .unwrap_or_default();
            ui.painter().text(Pos2::new(top.center().x, top.top() + 144.0), egui::Align2::CENTER_CENTER, sub, FontId::proportional(13.0), SUBTEXT);
            kit::divider(ui);
            let Some(info) = info else {
                ui.add_space(30.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("Загрузка…").color(SUBTEXT)));
                return;
            };
            let row = |ui: &mut egui::Ui, icon: &str, value: &str, label: &str, copy: bool| {
                if value.is_empty() {
                    return;
                }
                let galley = ui.painter().layout(value.to_owned(), FontId::proportional(14.0), WHITE, w - 80.0);
                let h = (galley.size().y + 38.0).max(56.0);
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, t2(WINDOW_OVER));
                }
                icons::tg(ui.painter(), icon, Pos2::new(r.left() + 30.0, r.top() + 24.0), DIALOG_DATE);
                ui.painter().galley(Pos2::new(r.left() + 64.0, r.top() + 12.0), galley.clone(), WHITE);
                ui.painter().text(
                    Pos2::new(r.left() + 64.0, r.top() + 14.0 + galley.size().y + 8.0),
                    egui::Align2::LEFT_CENTER,
                    label,
                    FontId::proportional(12.0),
                    SUBTEXT,
                );
                if copy && resp.on_hover_text("Нажмите, чтобы скопировать").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    ui.ctx().copy_text(value.to_owned());
                }
            };
            row(ui, "menu/phone", &if info.phone.is_empty() { String::new() } else { format!("+{}", info.phone) }, "Телефон", true);
            row(ui, "settings/mention", &if info.username.is_empty() { String::new() } else { format!("@{}", info.username) }, "Имя пользователя", true);
            row(ui, "menu/info", &info.about, if d.id < 0 { "Описание" } else { "О себе" }, true);
            row(ui, "menu/profile", &format!("{}", d.id), "ID", true);
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
                let (r, resp) = ui.allocate_exact_size(Vec2::splat(32.0), Sense::click());
                icons::tg(ui.painter(), "info/info_close", r.center(), if resp.hovered() { WHITE } else { SUBTEXT });
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
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
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(46.0, 46.0), Sense::click());
                icons::attach(ui.painter(), rect.center(), if resp.hovered() { WHITE } else { DIALOG_DATE });
                attach = resp.on_hover_text("Отправить файл").clicked();
            }
            let reserve = 100.0 + if s.field_emoji_button { 46.0 } else { 0.0 };
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
            if s.field_emoji_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(46.0, 46.0), Sense::click());
                self.emoji_btn = rect;
                let k = tween(ui.ctx(), "emoji-btn", f32::from(u8::from(resp.hovered() || self.emoji_open)), 0.12);
                let color = lerp_color(DIALOG_DATE, WHITE, k);
                // Кольцо рисуется кодом (как в Telegram), лицо — значок из набора.
                ui.painter().circle_stroke(rect.center(), 10.5, egui::Stroke::new(1.5, color));
                icons::tg(ui.painter(), "chat/input_smile_face", rect.center(), color);
                if resp.clicked() {
                    self.emoji_pinned = !self.emoji_pinned;
                    self.emoji_open = self.emoji_pinned;
                } else if resp.hovered() && s.show_emoji_popup {
                    self.emoji_open = true;
                }
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
        let appear = tween(ctx, "emoji-panel", f32::from(u8::from(self.emoji_open)), 0.18);
        if appear < 0.01 {
            return;
        }
        let mut picked = None;
        let mut jump = self.emoji_jump.take();
        let mut tab = self.emoji_tab;
        let mut sacts: Vec<super::panels::StickerAct> = Vec::new();
        let screen = ctx.content_rect();
        let size = Vec2::new(344.0_f32.min(screen.width() - 16.0), 340.0_f32.min(screen.height() - 120.0));
        // Панель поднимается над кнопкой с пружинистой посадкой, как лист на iPhone.
        let rise = (1.0 - spring(appear)) * 22.0;
        let pos = egui::pos2(
            (self.emoji_btn.right() - size.x + 20.0).clamp(8.0, screen.width() - size.x - 8.0),
            self.emoji_btn.top() - size.y - 12.0 + rise,
        );
        #[allow(deprecated)]
        let area = egui::Area::new(egui::Id::new("emoji-panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos);
        let inner = area.show(ctx, |ui| {
            ui.set_opacity(appear.clamp(0.0, 1.0));
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)))
                .corner_radius(egui::CornerRadius::same(14))
                .inner_margin(egui::Margin::symmetric(6, 6))
                .show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    // Вкладки «Эмодзи» и «Стикеры».
                    ui.horizontal(|ui| {
                        for (n, name) in ["Эмодзи", "Стикеры", "GIF"].iter().enumerate() {
                            let g = ui.painter().layout_no_wrap((*name).to_owned(), FontId::proportional(14.0), WHITE);
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 24.0, 30.0), Sense::click());
                            let k = tween(ui.ctx(), ("emtab", n), f32::from(u8::from(tab == n)), 0.16);
                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *name, FontId::proportional(14.0), lerp_color(DIALOG_TEXT, WHITE, k));
                            ui.painter().rect_filled(
                                Rect::from_center_size(egui::pos2(r.center().x, r.bottom() - 1.0), Vec2::new((r.width() - 16.0) * k, 2.5)),
                                egui::CornerRadius::same(1),
                                WHITE,
                            );
                            if resp.clicked() {
                                tab = n;
                            }
                        }
                    });
                    if tab == 0 {
                        let h = (ui.available_height() - 42.0).max(60.0);
                        ui.allocate_ui(Vec2::new(ui.available_width(), h), |ui| {
                            if self.emoji_add_open {
                                self.featured_view(ui, &mut sacts);
                            } else if let Some(i) = self.emoji_pack {
                                self.pack_view(ui, i, &mut sacts);
                            } else if let Some(emoji) = self.emoji.as_ref() {
                                picked = emoji.picker(ui, &self.settings.recent_emoji, &mut jump);
                            }
                        });
                        self.emoji_bar(ui, &mut jump, &mut sacts);
                    } else if tab == 1 {
                        self.sticker_tab(ui, self.emoji.as_ref(), &mut sacts);
                    } else {
                        self.gif_tab(ui, &mut sacts);
                    }
                });
        });
        self.emoji_tab = tab;
        self.emoji_jump = jump;
        for a in sacts {
            use super::panels::StickerAct as A;
            match a {
                A::LoadSets => {
                    self.stickers_requested = true;
                    self.send(Cmd::Stickers);
                }
                A::LoadSet(i) => {
                    if let Some(id) = self.sticker_sets.get(i).map(|s| s.id) {
                        // Пустой список-заглушка, пока идёт загрузка, чтобы не слать запрос каждый кадр.
                        if let std::collections::hash_map::Entry::Vacant(e) = self.sticker_lists.entry(id) {
                            e.insert(Vec::new());
                            self.send(Cmd::StickerSet(i));
                        }
                    }
                }
                A::Thumb(id) => {
                    if self.sticker_req.insert(id) {
                        self.send(Cmd::StickerThumb(id));
                    }
                }
                A::Send(id) => {
                    let reply = self.reply.take().map(|(r, _)| r);
                    self.send(Cmd::SendSticker(id, reply));
                }
                A::SelectSet(i) => self.sticker_cur = i,
                A::Gifs => {
                    self.gifs_requested = true;
                    self.send(Cmd::Gifs);
                }
                A::EmojiSets => {
                    self.emoji_sets_requested = true;
                    self.send(Cmd::EmojiSets);
                }
                A::Featured => {
                    if self.emoji_add_open || self.featured_sets.is_empty() {
                        self.send(Cmd::FeaturedEmoji);
                    }
                }
                A::Pack(p) => self.emoji_pack = p,
                A::AddOpen(o) => self.emoji_add_open = o,
                A::Install(i) => {
                    if let Some(set) = self.featured_sets.get(i).cloned() {
                        self.send(Cmd::InstallSet(set));
                    }
                }
                A::SetIcon(i) => {
                    if let Some(set) = self.emoji_sets.get(i).cloned()
                        && self.set_icon_req.insert(set.id)
                    {
                        if set.thumb_version.is_some() {
                            self.send(Cmd::SetIcon(set));
                        } else {
                            self.send(Cmd::LoadSetRow(set));
                        }
                    }
                }
                A::LoadPack(i) => {
                    if let Some(set) = self.emoji_sets.get(i).cloned()
                        && let std::collections::hash_map::Entry::Vacant(e) = self.sticker_lists.entry(set.id)
                    {
                        e.insert(Vec::new());
                        self.send(Cmd::LoadSetRow(set));
                    }
                }
                A::Insert(text) => {
                    self.input.push_str(&text);
                }
            }
        }
        if let Some(c) = picked {
            self.input.push(c);
            let mut recent: String = std::iter::once(c)
                .chain(self.settings.recent_emoji.chars().filter(|x| *x != c))
                .take(24)
                .collect();
            std::mem::swap(&mut recent, &mut self.settings.recent_emoji);
            self.send(Cmd::Settings(Box::new(self.settings.clone())));
        }
        // Закрытие: Esc или курсор ушёл с панели и кнопки (если панель не закреплена кликом).
        let over = ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| {
            inner.response.rect.expand(8.0).contains(p) || self.emoji_btn.expand(4.0).contains(p)
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) || (!over && !self.emoji_pinned) {
            self.emoji_open = false;
            self.emoji_pinned = false;
        }
    }

    pub(crate) fn messages_ui(&mut self, ui: &mut egui::Ui) {
        if self.selected.is_none() {
            ui.centered_and_justified(|ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(280.0, 40.0), Sense::hover());
                ui.painter().rect_filled(rect, egui::CornerRadius::same(20), t(SERVICE_BG));
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
        let fade = (self.chat_opened.elapsed().as_secs_f32() / 0.2).clamp(0.0, 1.0);
        if fade < 1.0 {
            ui.ctx().request_repaint();
        }
        ui.set_opacity(ease_out(fade));
        let now = self.now;
        // Имя отправителя показываем только в группах и каналах (их идентификаторы отрицательные).
        let group = self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.id < 0);
        let shift = ui.input(|i| i.modifiers.shift);
        let viewport_h = ui.available_height();
        let scroll_to = self.scroll_to.take();
        let mut acts: Vec<Act> = Vec::new();
        let mut want_older = false;
        let searching = self.found.is_some();
        let pad = (viewport_h - self.msgs_h).max(0.0);
        let out = {
            let list: &[MessageRow] = self.found.as_deref().unwrap_or(&self.messages);
            let read_out = self.selected.and_then(|i| self.dialogs.get(i)).map_or(0, |d| d.read_out);
            let mut avatar_msgs = std::collections::HashSet::new();
            if group {
                for (k, m) in list.iter().enumerate() {
                    let next = list.get(k + 1);
                    if !m.outgoing && next.is_none_or(|n| n.outgoing || n.sender_id != m.sender_id) {
                        avatar_msgs.insert(m.id);
                    }
                }
            }
            let env = Env {
                read_out,
                avatars: &self.avatars,
                avatar_msgs: &avatar_msgs,
                avail: ui.available_width(),
                s: &self.settings.ayu,
                tex: &self.textures,
                shift,
                scroll_to,
                group,
                arrived: &self.arrived,
                time: ui.input(|i| i.time),
                emoji: self.emoji.as_ref(),
                spoilers: &self.spoilers,
            };
            let hidden = &self.hidden;
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(!searching)
                .show(ui, |ui| {
                    let mut prev_date: Option<i64> = None;
                    // Короткая переписка прижата к низу, как в Telegram.
                    ui.add_space(8.0 + pad);
                    if searching && list.is_empty() {
                        ui.vertical_centered(|ui| ui.label(RichText::new("Ничего не найдено").color(SUBTEXT)));
                    }
                    let visible: Vec<&MessageRow> = list.iter().filter(|m| !hidden.contains(&m.id)).collect();
                    let mut i = 0;
                    while i < visible.len() {
                        let m = visible[i];
                        // Подряд идущие сообщения одного альбома показываем одним пузырём.
                        let mut j = i + 1;
                        if let (Some(g), false) = (m.grouped, m.deleted) {
                            while j < visible.len() && visible[j].grouped == Some(g) && !visible[j].deleted {
                                j += 1;
                            }
                        }
                        let album = &visible[i..j];
                        if prev_date.is_none_or(|p| !same_day(p, m.date)) {
                            day_pill(ui, env.avail, day_label(m.date, now));
                        }
                        prev_date = Some(m.date);
                        if album.len() > 1 {
                            let caption = album.iter().find(|x| !x.text.is_empty()).copied().unwrap_or(album[album.len() - 1]);
                            bubble(ui, caption, album, &env, &mut acts);
                        } else {
                            bubble(ui, m, &[], &env, &mut acts);
                        }
                        ui.add_space(4.0);
                        i = j;
                    }
                    ui.add_space(8.0);
                })
        };
        self.msgs_h = (out.content_size.y - pad).max(0.0);
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
            Act::Open(id) => self.send(Cmd::OpenFile(id)),
            Act::Viewer(id) => {
                self.viewer = Some(id);
                self.viewer_zoom = 1.0;
            }
            Act::History(id) => {
                self.details = Some(id);
                self.send(Cmd::EditHistory(id));
            }
            Act::Goto(id) => self.scroll_to = Some(id),
            Act::OpenUrl(url) => {
                if self.settings.ayu.disable_open_link_warning {
                    super::rich::open_url(&url);
                } else {
                    self.link_confirm = Some(url);
                }
            }
            Act::Spoiler(id, idx) => {
                // Повторный клик скрывает спойлер обратно.
                if !self.spoilers.insert((id, idx)) {
                    self.spoilers.remove(&(id, idx));
                }
            }
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
        if let Some(url) = self.link_confirm.clone() {
            let mut close = false;
            #[allow(deprecated)]
            egui::Window::new("Открыть ссылку?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_max_width(360.0);
                    ui.label("Ссылка откроется в браузере по умолчанию:");
                    ui.label(RichText::new(&url).color(WHITE).strong());
                    ui.horizontal(|ui| {
                        if ui.button("Открыть").clicked() {
                            super::rich::open_url(&url);
                            close = true;
                        }
                        if ui.button("Скопировать").clicked() {
                            ctx.copy_text(url.clone());
                            close = true;
                        }
                        if ui.button("Отмена").clicked() {
                            close = true;
                        }
                    });
                });
            if close {
                self.link_confirm = None;
            }
        }
        if let Some(id) = self.confirm_delete {
            let mut close = false;
            egui::Window::new("Удалить сообщение?")
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.set_max_width(320.0);
                    ui.label("Сообщение удалится в Telegram. Копия останется у вас, если включено «Сохранять удалённые».");
                    ui.add_space(8.0);
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

impl App {
    /// Просмотр фото на весь экран: колесо — масштаб, Esc или клик по фону — закрыть.
    #[allow(deprecated)]
    pub(crate) fn viewer_window(&mut self, ctx: &egui::Context) {
        let Some(id) = self.viewer else { return };
        let screen = ctx.content_rect();
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let zoom_delta = ctx.input(|i| i.smooth_scroll_delta.y);
        if zoom_delta != 0.0 {
            self.viewer_zoom = (self.viewer_zoom * (1.0 + zoom_delta / 400.0)).clamp(0.5, 6.0);
        }
        let tex = self.textures.get(&id).cloned();
        let zoom = self.viewer_zoom;
        let mut acts: Vec<Act> = Vec::new();
        egui::Area::new(egui::Id::new("viewer"))
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                let (rect, bg) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter().rect_filled(rect, 0.0, Color32::from_black_alpha(225));
                if let Some(tex) = &tex {
                    let size = tex.size_vec2();
                    let fit = (screen.width() * 0.9 / size.x).min(screen.height() * 0.82 / size.y).min(3.0);
                    let r = Rect::from_center_size(rect.center(), size * fit * zoom);
                    let img = ui.put(r, egui::Image::new(tex).corner_radius(8).sense(Sense::click()));
                    if bg.clicked() && !img.hovered() {
                        close = true;
                    }
                } else if bg.clicked() {
                    close = true;
                }
                let bar = Rect::from_center_size(
                    egui::pos2(rect.center().x, rect.bottom() - 34.0),
                    Vec2::new(330.0, 36.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                    ui.horizontal(|ui| {
                        if ui.button("Открыть оригинал").clicked() {
                            acts.push(Act::Open(id));
                        }
                        if ui.button("Сохранить").clicked() {
                            acts.push(Act::Save(id));
                        }
                        if ui.button("Закрыть").clicked() {
                            close = true;
                        }
                    });
                });
            });
        for a in acts {
            self.apply(ctx, a);
        }
        if close {
            self.viewer = None;
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
    ui.painter().rect_filled(pill, egui::CornerRadius::same(14), t(SERVICE_BG));
    ui.painter()
        .galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
}

/// Один пузырь сообщения с контекстным меню.
fn bubble(ui: &mut egui::Ui, m: &MessageRow, album: &[&MessageRow], env: &Env, acts: &mut Vec<Act>) {
    let s = env.s;
    let layout = if m.outgoing {
        Layout::right_to_left(Align::TOP)
    } else {
        Layout::left_to_right(Align::TOP)
    };
    let mut k_in = 1.0_f32;
    if let Some(t0) = env.arrived.get(&m.id) {
        k_in = ((env.time - t0) / 0.15).clamp(0.0, 1.0) as f32;
        if k_in < 1.0 {
            ui.ctx().request_repaint();
            // Новое сообщение чуть поднимается снизу за 150 мс, как в Telegram Desktop.
            ui.add_space(((1.0 - spring(k_in)) * 14.0).max(0.0));
        }
    }
    ui.with_layout(layout, |ui| {
        ui.set_opacity(ease_out((k_in * 2.5).min(1.0)));
        ui.add_space(6.0);
        if env.group && !m.outgoing {
            if env.avatar_msgs.contains(&m.id) {
                avatar(ui, &m.sender, m.sender_id, 34.0, env.avatars.get(&m.sender_id), s.avatar_corners);
            } else {
                ui.add_space(34.0);
            }
            ui.add_space(6.0);
        }
        let (mut fill, text_c, date_c) = if m.outgoing {
            (t(MSG_OUT_BG), MSG_OUT_TEXT, MSG_OUT_DATE)
        } else {
            (t(MSG_IN_BG), WHITE, MSG_IN_DATE)
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
        if album.len() > 1 {
            w = max_inner.min(310.0);
        }
        let w = w.min(max_inner);
        let radius = egui::CornerRadius::same(s.bubble_radius());
        let bid = ui.make_persistent_id(("m", m.id));
        let mut cards: Vec<(Rect, Act)> = Vec::new();
        let mut hot: Vec<(Rect, super::rich::Hit)> = Vec::new();
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
                    if album.len() > 1 {
                        super::media_ui::album(ui, album, w, env.tex, &mut cards);
                    } else if let Some(info) = &m.media {
                        super::media_ui::card(ui, m, info, w, env.tex.get(&m.id), text_c, date_c, &mut cards);
                    }
                    if !m.text.is_empty() && !m.entities.is_empty() {
                        let runs = super::rich::build_runs(&m.text, &m.entities);
                        hot.extend(super::rich::render(
                            ui,
                            env.emoji,
                            &runs,
                            15.0,
                            text_c,
                            WHITE,
                            m.id,
                            env.spoilers,
                        ));
                    } else if !m.text.is_empty() {
                        match env.emoji {
                            Some(e) => e.rich_label(ui, &m.text, 15.0, text_c),
                            None => {
                                ui.label(RichText::new(&m.text).color(text_c));
                            }
                        }
                    }
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 4.0;
                        let mut extra: Vec<(&str, String)> = Vec::new();
                        if m.pinned {
                            extra.push(("history_pin", String::new()));
                        }
                        if m.reactions > 0 && reactions_visible(s) {
                            extra.push(("dialogs/dialogs_reaction", m.reactions.to_string()));
                        }
                        if let Some(v) = m.views {
                            extra.push(("history_views", v.to_string()));
                        }
                        let note = m.deleted.then(|| {
                            if s.deleted_mark.is_empty() { "удалено".to_owned() } else { format!("{} удалено", s.deleted_mark) }
                        });
                        let note_w = note.as_ref().map_or(0.0, |n| 20.0 + n.chars().count() as f32 * 6.4);
                        if let Some(n) = &note {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                            icons::tg(ui.painter(), "menu/delete", r.center(), DANGER);
                            ui.label(RichText::new(n).size(12.0).color(DANGER));
                        }
                        let extra_w: f32 = note_w + extra
                            .iter()
                            .map(|(n, t)| {
                                icons::native_size(n).map_or(0.0, |s| s.x)
                                    + t.chars().count() as f32 * 7.0
                                    + 8.0
                            })
                            .sum::<f32>();
                        ui.add_space((w - meta_w - extra_w).max(0.0));
                        for (n, t) in extra {
                            let sz = icons::native_size(n).unwrap_or(Vec2::splat(12.0));
                            let (r, _) = ui.allocate_exact_size(Vec2::new(sz.x, 14.0), Sense::hover());
                            icons::tg(ui.painter(), n, r.center(), date_c);
                            if !t.is_empty() {
                                ui.label(RichText::new(t).size(12.0).color(date_c));
                            }
                        }
                        ui.label(RichText::new(meta).size(12.0).color(date_c));
                        if m.outgoing {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 12.0), Sense::hover());
                            icons::tick(ui.painter(), r.center(), OUT_TICK, m.id <= env.read_out);
                        }
                    });
                });
            });
        let rect = inner.response.rect;
        // Один отклик на весь пузырь: правый клик — меню, левый — по области ссылки, спойлера или карточки.
        let resp = ui.interact(rect, bid, Sense::click());
        let pointer = ui.ctx().input(|i| i.pointer.hover_pos());
        let over = |r: &Rect| pointer.is_some_and(|p| r.contains(p));
        let on_card = cards.iter().find(|(r, _)| over(r)).map(|(_, a)| a.clone());
        let on_hot = hot.iter().find(|(r, _)| over(r)).map(|(_, h)| h.clone());
        if resp.hovered() && (on_card.is_some() || matches!(on_hot, Some(super::rich::Hit::Link(_) | super::rich::Hit::Spoiler(_)))) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        // Спойлер раскрывается и при наведении, если так настроено (как в AyuGram).
        if s.reveal_spoilers
            && let Some(super::rich::Hit::Spoiler(i)) = &on_hot
            && !env.spoilers.contains(&(m.id, *i))
        {
            acts.push(Act::Spoiler(m.id, *i));
        }
        if resp.clicked() {
            if let Some(a) = on_card {
                acts.push(a);
            } else if let Some(h) = on_hot {
                acts.push(match h {
                    super::rich::Hit::Link(u) => Act::OpenUrl(u),
                    super::rich::Hit::Spoiler(i) => Act::Spoiler(m.id, i),
                });
            }
        }
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
                let img = env.emoji.and_then(|em| em.image(e.chars().next()?, 22.0));
                let clicked = match img {
                    Some(img) => ui.add(img.sense(Sense::click())).clicked(),
                    None => ui
                        .add(egui::Button::new(RichText::new(e).size(20.0)).frame(false))
                        .clicked(),
                };
                if clicked {
                    acts.push(Act::React(m.id, e));
                    ui.close();
                }
            }
        });
        ui.separator();
    }
    ui.spacing_mut().item_spacing.y = 0.0;
    let mut item = |ui: &mut egui::Ui, icon: &str, label: &str, act: Act| {
        if kit::menu_item(ui, icon, label, false) {
            acts.push(act);
            ui.close();
        }
    };
    if !m.deleted {
        item(ui, "menu/reply", "Ответить", Act::Reply(m.id, m.text.clone()));
    }
    if !m.text.is_empty() {
        item(ui, "menu/copy", "Копировать текст", Act::Copy(m.text.clone()));
    }
    if m.media.as_ref().is_some_and(|i| i.is_downloadable()) {
        item(ui, "menu/download", "Сохранить файл", Act::Save(m.id));
        item(ui, "menu/show_in_folder", "Открыть", Act::Open(m.id));
    }
    if m.outgoing && !m.deleted {
        item(ui, "menu/edit", "Изменить", Act::Edit(m.id, m.text.clone()));
    }
    if !m.deleted {
        item(ui, "menu/forward", "Переслать", Act::Forward(m.id));
        item(
            ui,
            if m.pinned { "menu/unpin" } else { "menu/pin" },
            if m.pinned { "Открепить" } else { "Закрепить" },
            Act::Pin(m.id, !m.pinned),
        );
    }
    if s.menu_repeat_message.shown(env.shift) && !m.text.is_empty() {
        item(ui, "menu/restore", "Повторить", Act::Repeat(m.text.clone()));
    }
    if s.menu_add_filter.shown(env.shift) && !m.text.is_empty() {
        item(ui, "menu/tag_filter", "Добавить в фильтры", Act::AddFilter(m.text.clone()));
    }
    if s.menu_hide_message.shown(env.shift) {
        item(ui, "menu/stealth", "Скрыть сообщение", Act::Hide(m.id));
    }
    if m.edits > 0 {
        item(ui, "menu/edited_status", "История правок", Act::History(m.id));
    }
    if s.menu_message_details.shown(env.shift) {
        item(ui, "menu/info", "Сведения", Act::Details(m.id));
        item(ui, "menu/copy", "Копировать ID", Act::CopyId(m.id));
    }
    if !m.deleted {
        ui.separator();
        if kit::menu_item(ui, "menu/delete", "Удалить", true) {
            acts.push(Act::Delete(m.id));
            ui.close();
        }
    }
}
