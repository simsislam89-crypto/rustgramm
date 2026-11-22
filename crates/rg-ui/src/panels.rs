//! Панели вокруг списка чатов: вкладки папок, окно контактов, вкладка стикеров в панели ввода.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use rg_core::engine::folder_matches;

use super::emoji::Emoji;
use super::{App, Cmd, icons, theme::*};

/// Что вкладка стикеров просит сделать (выполняется после отрисовки).
pub(crate) enum StickerAct {
    LoadSets,
    LoadSet(usize),
    Thumb(i64),
    Send(i64),
    SelectSet(usize),
    /// Загрузить установленные наборы эмодзи.
    EmojiSets,
    /// Загрузить рекомендуемые наборы эмодзи.
    Featured,
    /// Показать набор эмодзи с номером (или вернуться к обычным эмодзи).
    Pack(Option<usize>),
    /// Открыть или закрыть окно «Добавить эмодзи».
    AddOpen(bool),
    /// Добавить рекомендуемый набор с номером.
    Install(usize),
    /// Загрузить значок набора эмодзи с номером.
    SetIcon(usize),
    /// Загрузить содержимое набора эмодзи с номером.
    LoadPack(usize),
    /// Вставить в поле ввода (обычный эмодзи для набора Premium).
    Insert(String),
    /// Загрузить сохранённые GIF.
    Gifs,
}

impl App {
    /// Полоса папок под строкой поиска: «Все чаты» и папки пользователя с счётчиками.
    pub(crate) fn folder_tabs(&mut self, ui: &mut egui::Ui) {
        if self.folders.is_empty() {
            return;
        }
        let hide_all = self.settings.ayu.hide_all_chats_folder;
        if hide_all && self.folder.is_none() {
            self.folder = self.folders.first().map(|f| f.id);
        }
        // (идентификатор папки, подпись, непрочитанные без заглушённых)
        let mut tabs: Vec<(Option<i32>, String, i32)> = Vec::new();
        if !hide_all {
            let n = self.dialogs.iter().filter(|d| !d.archived && !d.muted).map(|d| d.unread).sum();
            tabs.push((None, "Все чаты".to_owned(), n));
        }
        for f in &self.folders {
            let n = self
                .dialogs
                .iter()
                .filter(|d| folder_matches(f, d) && !d.muted)
                .map(|d| d.unread)
                .sum();
            tabs.push((Some(f.id), f.title.clone(), n));
        }
        let mut chosen = None;
        egui::ScrollArea::horizontal()
            .id_salt("folder-tabs")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_space(8.0);
                    for (id, title, unread) in &tabs {
                        let selected = *id == self.folder;
                        let label = if *unread > 0 { format!("{title}  {unread}") } else { title.clone() };
                        let g = ui.painter().layout_no_wrap(
                            label.clone(),
                            FontId::proportional(14.0),
                            WHITE,
                        );
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(g.size().x + 20.0, 34.0), Sense::click());
                        let k = tween(ui.ctx(), ("ftab", id.unwrap_or(-1)), f32::from(u8::from(selected)), 0.18);
                        let hover = tween(ui.ctx(), ("ftabh", id.unwrap_or(-1)), f32::from(u8::from(resp.hovered())), 0.12);
                        let p = ui.painter();
                        let color = lerp_color(lerp_color(DIALOG_TEXT, WHITE, hover), WHITE, k);
                        p.text(rect.center() - Vec2::new(0.0, 1.0), Align2::CENTER_CENTER, label, FontId::proportional(14.0), color);
                        // Полоска под выбранной вкладкой растёт от центра.
                        let w = (rect.width() - 14.0) * k;
                        p.rect_filled(
                            Rect::from_center_size(Pos2::new(rect.center().x, rect.bottom() - 1.5), Vec2::new(w, 2.5)),
                            CornerRadius::same(1),
                            t_btn(ORANGE),
                        );
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            chosen = Some(*id);
                        }
                    }
                });
            });
        if let Some(id) = chosen {
            self.folder = id;
            self.in_archive = false;
        }
        let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(line, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 14));
    }

    /// Окно «Контакты»: поиск и список с фото и статусом; клик открывает чат.
    #[allow(deprecated)]
    pub(crate) fn contacts_window(&mut self, ctx: &egui::Context) {
        let raw = tween(ctx, "contacts-win", f32::from(u8::from(self.contacts_open)), 0.2);
        if raw < 0.005 && !self.contacts_open {
            return;
        }
        let appear = ios_spring(raw);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("contacts-dim"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                paint_frosted_scrim(ui.painter(), rect, raw);
                if resp.clicked() {
                    self.contacts_open = false;
                }
            });
        let mut open_contact = None;
        let mut want_avatar = Vec::new();
        let max_h = (screen.height() - 60.0).clamp(240.0, 640.0);
        egui::Area::new(egui::Id::new("contacts-box"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, (1.0 - appear) * 18.0))
            .show(ctx, |ui| {
                ui.set_opacity(raw.min(1.0).powf(0.6));
                egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)))
                    .corner_radius(CornerRadius::same(12))
                    .show(ui, |ui| {
                        let width = 380.0_f32.min(screen.width() - 16.0);
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        let (head, _) = ui.allocate_exact_size(Vec2::new(width, 50.0), Sense::hover());
                        ui.painter().text(
                            Pos2::new(head.left() + 20.0, head.center().y),
                            Align2::LEFT_CENTER,
                            "Контакты",
                            FontId::proportional(16.0),
                            WHITE,
                        );
                        let x = Rect::from_center_size(Pos2::new(head.right() - 24.0, head.center().y), Vec2::splat(34.0));
                        let xr = ui.interact(x, ui.make_persistent_id("contacts-x"), Sense::click());
                        icons::tg(ui.painter(), "info/info_close", x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
                        if xr.clicked() {
                            self.contacts_open = false;
                        }
                        ui.horizontal(|ui| {
                            ui.add_space(14.0);
                            ui.add(
                                egui::TextEdit::singleline(&mut self.contact_search)
                                    .hint_text("Поиск контактов")
                                    .desired_width(width - 28.0)
                                    .margin(Vec2::new(10.0, 7.0)),
                            );
                        });
                        ui.add_space(6.0);
                        let needle = self.contact_search.to_lowercase();
                        egui::ScrollArea::vertical().max_height(max_h - 110.0).auto_shrink([false, true]).show(ui, |ui| {
                            if self.contacts.is_empty() {
                                ui.add_space(20.0);
                                ui.vertical_centered(|ui| ui.label(egui::RichText::new("Загрузка контактов…").color(SUBTEXT)));
                            }
                            for c in self.contacts.iter().filter(|c| needle.is_empty() || c.name.to_lowercase().contains(&needle)) {
                                let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::click());
                                let hover = tween(ui.ctx(), ("contact", c.id), f32::from(u8::from(resp.hovered())), 0.1);
                                ui.painter().rect_filled(rect, 0.0, lerp_color(Color32::TRANSPARENT, t(DIALOG_OVER), hover));
                                if ui.is_rect_visible(rect) && !self.avatars.contains_key(&c.id) {
                                    want_avatar.push((c.peer, c.id));
                                }
                                draw_avatar(ui, rect.left_center() + Vec2::new(34.0, 0.0), &c.name, c.id, 40.0, self.avatars.get(&c.id), self.settings.ayu.avatar_corners);
                                let p = ui.painter();
                                p.text(rect.left_top() + Vec2::new(66.0, 11.0), Align2::LEFT_TOP, &c.name, FontId::proportional(15.0), WHITE);
                                let online = c.status == "в сети";
                                p.text(
                                    rect.left_top() + Vec2::new(66.0, 32.0),
                                    Align2::LEFT_TOP,
                                    &c.status,
                                    FontId::proportional(13.0),
                                    if online { GREEN } else { SUBTEXT },
                                );
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    open_contact = Some((c.peer, c.id, c.name.clone()));
                                }
                            }
                        });
                        ui.add_space(8.0);
                    });
            });
        for (peer, id) in want_avatar {
            if self.contact_av_req.insert(id) {
                self.send(Cmd::AvatarPeer(peer, id));
            }
        }
        if let Some((peer, id, name)) = open_contact {
            self.contacts_open = false;
            self.send(Cmd::OpenContact(peer, id, name));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.contacts_open = false;
        }
    }

    /// Нижняя полоса вкладки «Эмодзи»: разделы (значки Telegram), наборы эмодзи и «+».
    pub(crate) fn emoji_bar(&self, ui: &mut egui::Ui, jump: &mut Option<usize>, acts: &mut Vec<StickerAct>) {
        if !self.emoji_sets_requested {
            acts.push(StickerAct::EmojiSets);
        }
        ui.add_space(2.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 38.0), Sense::hover());
        ui.painter().line_segment(
            [bar.left_top(), bar.right_top()],
            Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18)),
        );
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(bar));
        let cell = 36.0;
        egui::ScrollArea::horizontal()
            .id_salt("emoji-bar")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut inner, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    ui.set_height(38.0);
                    for (cat, icon) in super::emoji::CATEGORY_ICONS.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(cell, 38.0), Sense::click());
                        let active = self.emoji_pack.is_none() && !self.emoji_add_open;
                        let k = tween(ui.ctx(), ("emcat", cat), f32::from(u8::from(resp.hovered())), 0.1);
                        let c = lerp_color(SUBTEXT, WHITE, if active { 0.4 + 0.6 * k } else { k });
                        icons::tg(ui.painter(), icon, r.center(), c);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            *jump = Some(cat);
                            acts.push(StickerAct::Pack(None));
                            acts.push(StickerAct::AddOpen(false));
                        }
                    }
                    for (i, set) in self.emoji_sets.iter().enumerate() {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(cell, 38.0), Sense::click());
                        let sel = self.emoji_pack == Some(i);
                        let k = tween(ui.ctx(), ("empack", i), f32::from(u8::from(sel || resp.hovered())), 0.1);
                        let side = 22.0 + 2.0 * k;
                        let area = Rect::from_center_size(r.center(), Vec2::splat(side));
                        let first = self.sticker_lists.get(&set.id).and_then(|l| l.first());
                        let tex = self
                            .sticker_tex
                            .get(&set.id)
                            .or_else(|| first.and_then(|f| self.sticker_tex.get(&f.id)));
                        if let Some(t) = tex {
                            egui::Image::new(t)
                                .fit_to_exact_size(area.size())
                                .tint(Color32::from_white_alpha((150.0 + 105.0 * k) as u8))
                                .paint_at(ui, area);
                        } else {
                            if ui.is_rect_visible(r) {
                                acts.push(StickerAct::SetIcon(i));
                            }
                            let initial = set.title.chars().next().map(|c| c.to_uppercase().to_string()).unwrap_or_default();
                            ui.painter().circle_filled(r.center(), 10.0, Color32::from_rgb(60, 60, 68));
                            ui.painter().text(r.center(), Align2::CENTER_CENTER, initial, FontId::proportional(12.0), WHITE);
                        }
                        if sel {
                            ui.painter().rect_filled(
                                Rect::from_center_size(Pos2::new(r.center().x, r.bottom() - 2.0), Vec2::new(18.0, 2.5)),
                                CornerRadius::same(1),
                                WHITE,
                            );
                        }
                        if resp.on_hover_text(&set.title).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(StickerAct::Pack(Some(i)));
                            acts.push(StickerAct::AddOpen(false));
                        }
                    }
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(cell, 38.0), Sense::click());
                    let k = tween(ui.ctx(), "emadd", f32::from(u8::from(resp.hovered() || self.emoji_add_open)), 0.1);
                    icons::tg(ui.painter(), "menu/add", r.center(), lerp_color(SUBTEXT, WHITE, k));
                    if resp.on_hover_text("Добавить эмодзи").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        acts.push(StickerAct::AddOpen(!self.emoji_add_open));
                        acts.push(StickerAct::Featured);
                    }
                });
            });
        ui.advance_cursor_after_rect(bar);
    }

    /// Содержимое набора эмодзи: сетка миниатюр, нажатие вставляет обычный эмодзи.
    pub(crate) fn pack_view(&self, ui: &mut egui::Ui, i: usize, acts: &mut Vec<StickerAct>) {
        let Some(set) = self.emoji_sets.get(i) else { return };
        let list = self.sticker_lists.get(&set.id);
        if list.is_none() {
            acts.push(StickerAct::LoadPack(i));
        }
        let cell = 40.0;
        let cols = ((ui.available_width() / cell).floor() as usize).max(1);
        egui::ScrollArea::vertical()
            .id_salt(("pack", set.id))
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let (head, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 30.0), Sense::hover());
                ui.painter().text(
                    Pos2::new(head.left() + 8.0, head.bottom() - 6.0),
                    Align2::LEFT_BOTTOM,
                    format!("{} · {}", set.title, set.count),
                    FontId::proportional(12.5),
                    Color32::from_rgb(150, 150, 158),
                );
                for row in list.map(|l| l.chunks(cols)).into_iter().flatten() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(cell * cols as f32, cell), Sense::hover());
                    for (k, st) in row.iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * cell, rect.top()), Vec2::splat(cell));
                        let resp = ui.interact(r, ui.make_persistent_id(("pk", st.id)), Sense::click());
                        let h = tween(ui.ctx(), ("pkh", st.id), f32::from(u8::from(resp.hovered())), 0.1);
                        if h > 0.01 {
                            ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(8), Color32::from_rgba_unmultiplied(255, 255, 255, (26.0 * h) as u8));
                        }
                        let side = 28.0 + 4.0 * h;
                        let area = Rect::from_center_size(r.center(), Vec2::splat(side));
                        if let Some(tex) = self.sticker_tex.get(&st.id) {
                            egui::Image::new(tex).fit_to_exact_size(area.size()).paint_at(ui, area);
                        } else if st.has_preview() && ui.is_rect_visible(r) {
                            acts.push(StickerAct::Thumb(st.id));
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !st.emoji.is_empty() {
                            acts.push(StickerAct::Insert(st.emoji.clone()));
                        }
                    }
                }
            });
    }

    /// Вкладка GIF: сетка сохранённых анимаций (миниатюры), нажатие отправляет.
    pub(crate) fn gif_tab(&self, ui: &mut egui::Ui, acts: &mut Vec<StickerAct>) {
        if !self.gifs_requested {
            acts.push(StickerAct::Gifs);
        }
        if self.gif_list.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(if self.gifs_requested { "Нет сохранённых GIF" } else { "Загрузка…" }).color(SUBTEXT));
            });
            return;
        }
        let cols = 3usize;
        let width = ui.available_width();
        let cell = width / cols as f32;
        egui::ScrollArea::vertical().id_salt("gifs").auto_shrink([false, false]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            for row in self.gif_list.chunks(cols) {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(width, cell), Sense::hover());
                for (k, g) in row.iter().enumerate() {
                    let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * cell, rect.top()), Vec2::splat(cell)).shrink(2.0);
                    let resp = ui.interact(r, ui.make_persistent_id(("gif", g.id)), Sense::click());
                    let h = tween(ui.ctx(), ("gifh", g.id), f32::from(u8::from(resp.hovered())), 0.1);
                    match self.sticker_tex.get(&g.id) {
                        Some(t) => {
                            egui::Image::new(t).fit_to_exact_size(r.size()).corner_radius(8.0).tint(Color32::from_white_alpha((200.0 + 55.0 * h) as u8)).paint_at(ui, r);
                        }
                        None => {
                            if g.has_preview() && ui.is_rect_visible(r) {
                                acts.push(StickerAct::Thumb(g.id));
                            }
                            ui.painter().rect_filled(r, CornerRadius::same(8), Color32::from_rgba_unmultiplied(255, 255, 255, 12));
                        }
                    }
                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        acts.push(StickerAct::Send(g.id));
                    }
                }
            }
        });
    }

    /// Окно «Добавить эмодзи»: рекомендуемые наборы с кнопкой «Добавить».
    pub(crate) fn featured_view(&self, ui: &mut egui::Ui, acts: &mut Vec<StickerAct>) {
        if self.featured_sets.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Загрузка…").color(SUBTEXT));
            });
            return;
        }
        egui::ScrollArea::vertical()
            .id_salt("featured")
            .max_height(ui.available_height())
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for (i, set) in self.featured_sets.iter().enumerate() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
                    let icon = Rect::from_center_size(Pos2::new(rect.left() + 28.0, rect.center().y), Vec2::splat(34.0));
                    match set.cover.as_ref().and_then(|c| self.sticker_tex.get(&c.id)) {
                        Some(t) => egui::Image::new(t).fit_to_exact_size(icon.size()).paint_at(ui, icon),
                        None => {
                            if let Some(c) = &set.cover
                                && c.has_preview()
                                && ui.is_rect_visible(rect)
                            {
                                acts.push(StickerAct::Thumb(c.id));
                            }
                            ui.painter().circle_filled(icon.center(), 15.0, Color32::from_rgb(60, 60, 68));
                        }
                    }
                    ui.painter().text(
                        Pos2::new(rect.left() + 54.0, rect.center().y - 8.0),
                        Align2::LEFT_CENTER,
                        &set.title,
                        FontId::proportional(14.0),
                        WHITE,
                    );
                    ui.painter().text(
                        Pos2::new(rect.left() + 54.0, rect.center().y + 9.0),
                        Align2::LEFT_CENTER,
                        format!("{} эмодзи", set.count),
                        FontId::proportional(12.0),
                        SUBTEXT,
                    );
                    let btn = Rect::from_center_size(Pos2::new(rect.right() - 52.0, rect.center().y), Vec2::new(86.0, 28.0));
                    let resp = ui.interact(btn, ui.make_persistent_id(("feat", set.id)), Sense::click());
                    let k = tween(ui.ctx(), ("feath", set.id), f32::from(u8::from(resp.hovered() && !set.installed)), 0.1);
                    let p = ui.painter();
                    if set.installed {
                        p.text(btn.center(), Align2::CENTER_CENTER, "Добавлено", FontId::proportional(12.5), SUBTEXT);
                    } else {
                        p.rect_filled(btn, CornerRadius::same(14), t_btn(lerp_color(ORANGE, ORANGE_OVER, k)));
                        p.text(btn.center(), Align2::CENTER_CENTER, "Добавить", FontId::proportional(12.5), Color32::WHITE);
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(StickerAct::Install(i));
                        }
                    }
                }
            });
    }

    /// Вкладка «Стикеры»: сетка выбранного набора и полоса наборов внизу.
    pub(crate) fn sticker_tab(&self, ui: &mut egui::Ui, emoji: Option<&Emoji>, acts: &mut Vec<StickerAct>) {
        if !self.stickers_requested {
            acts.push(StickerAct::LoadSets);
        }
        if self.sticker_sets.is_empty() {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new(if self.stickers_requested { "Нет наборов стикеров" } else { "Загрузка…" }).color(SUBTEXT));
            });
            return;
        }
        let cur = self.sticker_cur.min(self.sticker_sets.len() - 1);
        let set = &self.sticker_sets[cur];
        let list = self.sticker_lists.get(&set.id);
        if list.is_none() {
            acts.push(StickerAct::LoadSet(cur));
        }
        let cols = 4usize;
        let width = ui.available_width();
        let cell = width / cols as f32;
        egui::ScrollArea::vertical()
            .max_height(ui.available_height() - 44.0)
            .auto_shrink([false, false])
            .show(ui, |ui| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                ui.add_space(4.0);
                ui.label(egui::RichText::new(&set.title).size(12.5).color(Color32::from_rgb(150, 150, 158)));
                ui.add_space(4.0);
                for row in list.map(|l| l.chunks(cols)).into_iter().flatten() {
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, cell), Sense::hover());
                    for (k, st) in row.iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * cell, rect.top()), Vec2::splat(cell));
                        let resp = ui.interact(r, ui.make_persistent_id(("stk", st.id)), Sense::click());
                        let h = tween(ui.ctx(), ("stkh", st.id), f32::from(u8::from(resp.hovered())), 0.1);
                        if h > 0.01 {
                            ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(10), Color32::from_rgba_unmultiplied(255, 255, 255, (22.0 * h) as u8));
                        }
                        let side = (cell - 14.0) * (1.0 + 0.06 * h);
                        let area = Rect::from_center_size(r.center(), Vec2::splat(side));
                        if let Some(tex) = self.sticker_tex.get(&st.id) {
                            egui::Image::new(tex).fit_to_exact_size(area.size()).paint_at(ui, area);
                        } else if st.has_preview() {
                            if ui.is_rect_visible(r) {
                                acts.push(StickerAct::Thumb(st.id));
                            }
                            ui.painter().rect_filled(area.shrink(10.0), CornerRadius::same(12), Color32::from_rgba_unmultiplied(255, 255, 255, 10));
                        } else if let Some(img) = emoji.and_then(|e| st.emoji.chars().next().and_then(|c| e.image(c, 34.0))) {
                            // Анимированные стикеры (TGS/WebM) пока не воспроизводятся: показываем эмодзи.
                            img.paint_at(ui, Rect::from_center_size(r.center(), Vec2::splat(34.0)));
                        }
                        if resp.clicked() {
                            acts.push(StickerAct::Send(st.id));
                        }
                    }
                }
            });
        ui.add_space(4.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 36.0), Sense::hover());
        ui.painter().line_segment([bar.left_top(), bar.right_top()], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18)));
        let n = self.sticker_sets.len().min(9);
        for (i, s) in self.sticker_sets.iter().take(n).enumerate() {
            let c = Pos2::new(bar.left() + bar.width() * (i as f32 + 0.5) / n as f32, bar.center().y);
            let r = Rect::from_center_size(c, Vec2::splat(30.0));
            let resp = ui.interact(r, ui.make_persistent_id(("stkset", i)), Sense::click());
            let sel = tween(ui.ctx(), ("stkseth", i), f32::from(u8::from(i == cur || resp.hovered())), 0.12);
            let p = ui.painter();
            p.circle_filled(c, 13.0, lerp_color(Color32::from_rgb(52, 52, 58), Color32::from_rgb(88, 88, 96), sel));
            let initial = s.title.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_default();
            p.text(c, Align2::CENTER_CENTER, initial, FontId::proportional(13.0), WHITE);
            if resp.clicked() {
                acts.push(StickerAct::SelectSet(i));
            }
        }
        let _ = icons::Ico::Sticker;
    }
}
