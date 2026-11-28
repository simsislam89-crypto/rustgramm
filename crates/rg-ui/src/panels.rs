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
    /// Вставить пользовательский Premium-эмодзи (запасной символ, `document_id`).
    InsertCustom(String, i64),
    /// Загрузить сохранённые GIF.
    Gifs,
    /// Сообщить о видимом на экране анимированном TGS-документе (стикер / эмодзи).
    VisibleDoc(i64),
    /// Принудительно перезагрузить стикеры.
    ReloadSets,
    /// Принудительно перезагрузить GIF.
    ReloadGifs,
    /// Отправить бросок/игру (`🎲`, `🎯`, `🏀`, `⚽`, `🎰`, `🎳`).
    SendDice(String),
}

impl App {
    /// Горизонтальная полоса историй контактов + кнопка «+ Моя история» над вкладками папок (`!disable_stories`).
    pub(crate) fn stories_strip(&mut self, ui: &mut egui::Ui) {
        if self.settings.ayu.disable_stories {
            return;
        }
        let mut clicked: Option<usize> = None;
        let mut open_composer = false;
        let mut need_avatar = Vec::new();
        egui::ScrollArea::horizontal()
            .id_salt("stories-strip")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.spacing_mut().item_spacing.x = 10.0;
                    // Кнопка публикации своей истории («+ Моя история») — чёткий векторный «+» ровно по центру.
                    {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(56.0, 72.0), Sense::click());
                        let center = Pos2::new(rect.center().x, rect.top() + 26.0);
                        let hk = tween(ui.ctx(), "story-add-h", f32::from(u8::from(resp.hovered())), 0.14);
                        let sc = 1.0 + 0.08 * ios_spring_bounce(hk);
                        let accent = c_orange();
                        ui.painter().circle_stroke(
                            center,
                            21.0 * sc,
                            Stroke::new(2.0, lerp_color(accent.gamma_multiply(0.75), accent, hk)),
                        );
                        ui.painter().circle_filled(center, 18.0 * sc, t(WINDOW_OVER));
                        let arm = 6.5 * sc;
                        let ps = Stroke::new(2.2, accent);
                        ui.painter().line_segment(
                            [Pos2::new(center.x - arm, center.y), Pos2::new(center.x + arm, center.y)],
                            ps,
                        );
                        ui.painter().line_segment(
                            [Pos2::new(center.x, center.y - arm), Pos2::new(center.x, center.y + arm)],
                            ps,
                        );
                        ui.painter().text(
                            Pos2::new(rect.center().x, rect.bottom() - 10.0),
                            Align2::CENTER_CENTER,
                            "Моя",
                            FontId::proportional(11.5),
                            if resp.hovered() { accent } else { WHITE },
                        );
                        if resp.on_hover_text("Опубликовать свою историю").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            open_composer = true;
                        }
                    }
                    for (idx, st) in self.stories.iter().enumerate() {
                        if !self.avatars.contains_key(&st.peer_id) && self.avatar_req.insert(st.peer_id) {
                            need_avatar.push((st.peer, st.peer_id));
                        }
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(56.0, 72.0), Sense::click());
                        let center = Pos2::new(rect.center().x, rect.top() + 26.0);
                        let hk = tween(ui.ctx(), ("st-h", st.story_id), f32::from(u8::from(resp.hovered())), 0.14);
                        let sc = 1.0 + 0.06 * ios_spring_bounce(hk);
                        let ring = if st.unread { c_orange() } else { SUBTEXT.gamma_multiply(0.55) };
                        ui.painter().circle_stroke(center, 21.0 * sc, Stroke::new(2.1, ring));
                        draw_avatar(
                            ui,
                            center,
                            &st.author,
                            st.peer_id,
                            36.0 * sc,
                            self.avatars.get(&st.peer_id),
                            self.settings.ayu.avatar_corners,
                        );
                        let short: String = st.author.chars().take(7).collect();
                        ui.painter().text(
                            Pos2::new(rect.center().x, rect.bottom() - 10.0),
                            Align2::CENTER_CENTER,
                            short,
                            FontId::proportional(11.5),
                            if st.unread { WHITE } else { SUBTEXT },
                        );
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            clicked = Some(idx);
                        }
                    }
                    ui.add_space(6.0);
                });
            });
        for (peer, id) in need_avatar {
            self.send(Cmd::AvatarPeer(peer, id));
        }
        if open_composer {
            self.story_composer_open = true;
        }
        if let Some(idx) = clicked {
            if self.settings.ayu.suggest_ghost_mode_before_viewing_story && self.settings.ghost.send_read_stories {
                self.story_ghost_ask = Some(idx);
            } else {
                self.story_open = Some(idx);
                self.story_opened_at = Some(std::time::Instant::now());
                self.story_paused = false;
                if let Some(st) = self.stories.get_mut(idx) {
                    st.unread = false;
                    let (peer, sid) = (st.peer, st.story_id);
                    self.send(Cmd::ReadStory(peer, sid));
                    if !self.story_textures.contains_key(&sid) && self.story_media_req.insert(sid) {
                        self.send(Cmd::LoadStoryMedia(peer, sid));
                    }
                }
            }
        }
        let (line, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 1.0), Sense::hover());
        ui.painter().rect_filled(line, 0.0, Color32::from_rgba_unmultiplied(255, 255, 255, 14));
    }

    /// Полоса папок под строкой поиска: «Все чаты», папки пользователя и фильтр «Непрочитанные».
    pub(crate) fn folder_tabs(&mut self, ui: &mut egui::Ui) {
        let unread_dialogs = self.dialogs.iter().filter(|d| !d.archived && d.unread > 0).count();
        if self.folders.is_empty() && unread_dialogs == 0 && !self.unread_filter_only {
            return;
        }
        let hide_all = self.settings.ayu.hide_all_chats_folder;
        if hide_all && self.folder.is_none() && !self.folders.is_empty() {
            self.folder = self.folders.first().map(|f| f.id);
        }
        let show_cnt = !self.settings.ayu.hide_notification_counters;
        let inc_muted = self.settings.notify.badge_include_muted;
        let count_msgs = self.settings.notify.badge_count_messages;
        let tally = |d: &rg_core::engine::DialogRow| -> i32 {
            if d.unread <= 0 || (!inc_muted && d.muted) {
                0
            } else if count_msgs {
                d.unread
            } else {
                1
            }
        };
        // (идентификатор папки, подпись, непрочитанные с учётом настроек счётчика) — кэш по dialogs_gen.
        let tabs_key = (self.dialogs_gen, self.folders.len(), hide_all, inc_muted, count_msgs);
        if self.folder_tabs_cache_key != tabs_key {
            self.folder_tabs_cache_key = tabs_key;
            let mut tabs: Vec<(Option<i32>, String, i32)> = Vec::with_capacity(self.folders.len() + 1);
            if !hide_all {
                let n = self.dialogs.iter().filter(|d| !d.archived).map(tally).sum();
                tabs.push((None, "Все чаты".to_owned(), n));
            }
            for f in &self.folders {
                let n = self.dialogs.iter().filter(|d| folder_matches(f, d)).map(tally).sum();
                tabs.push((Some(f.id), f.title.clone(), n));
            }
            self.folder_tabs_cache = tabs;
        }
        let mut chosen = None;
        egui::ScrollArea::horizontal()
            .id_salt("folder-tabs")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add_space(8.0);
                    for (id, title, unread) in &self.folder_tabs_cache {
                        let selected = *id == self.folder;
                        let label = if show_cnt && *unread > 0 {
                            format!("{title}  {unread}")
                        } else {
                            title.clone()
                        };
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
                        if color == WHITE {
                            let pos = rect.center() - g.size() * 0.5 - Vec2::new(0.0, 1.0);
                            p.galley(pos, g, WHITE);
                        } else {
                            p.text(rect.center() - Vec2::new(0.0, 1.0), Align2::CENTER_CENTER, label, FontId::proportional(14.0), color);
                        }
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
                    // Кнопка быстрого фильтра «Непрочитанные».
                    if unread_dialogs > 0 || self.unread_filter_only {
                        ui.add_space(4.0);
                        let lbl = if unread_dialogs > 0 {
                            format!("Непрочитанные ({unread_dialogs})")
                        } else {
                            "Непрочитанные".to_owned()
                        };
                        let active = self.unread_filter_only;
                        let bg = if active {
                            t_btn(ORANGE)
                        } else {
                            Color32::from_white_alpha(12)
                        };
                        let fg = if active { super::theme::on_accent() } else { DIALOG_TEXT };
                        let pill = ui
                            .add(
                                egui::Button::new(egui::RichText::new(lbl).size(12.0).color(fg))
                                    .fill(bg)
                                    .corner_radius(CornerRadius::same(11))
                                    .min_size(Vec2::new(0.0, 24.0)),
                            )
                            .on_hover_cursor(egui::CursorIcon::PointingHand);
                        if pill.clicked() {
                            self.unread_filter_only = !self.unread_filter_only;
                        }
                        ui.add_space(8.0);
                    }
                });
            });
        if let Some(id) = chosen {
            self.folder = id;
            self.in_archive = false;
            self.unread_filter_only = false;
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
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, (1.0 - appear) * 28.0))
            .show(ctx, |ui| {
                ui.set_opacity(appear.clamp(0.02, 1.0));
                let card = egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .stroke(Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 28)))
                    .corner_radius(CornerRadius::same(14))
                    .show(ui, |ui| {
                        let width = (380.0_f32 * (0.94 + 0.06 * appear)).min(screen.width() - 16.0);
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
                        let matched_contacts: Vec<usize> = self
                            .contacts
                            .iter()
                            .enumerate()
                            .filter(|(_, c)| needle.is_empty() || c.name.to_lowercase().contains(&needle))
                            .map(|(i, _)| i)
                            .collect();
                        egui::ScrollArea::vertical().max_height(max_h - 110.0).auto_shrink([false, true]).show_rows(
                            ui,
                            56.0,
                            matched_contacts.len().max(1),
                            |ui, range| {
                                if self.contacts.is_empty() {
                                    ui.add_space(20.0);
                                    ui.vertical_centered(|ui| ui.label(egui::RichText::new("Загрузка контактов…").color(SUBTEXT)));
                                    return;
                                }
                                if matched_contacts.is_empty() {
                                    ui.add_space(20.0);
                                    ui.vertical_centered(|ui| ui.label(egui::RichText::new("Ничего не найдено").color(SUBTEXT)));
                                    return;
                                }
                                ui.spacing_mut().item_spacing.y = 0.0;
                                for idx in range {
                                    let Some(&ci) = matched_contacts.get(idx) else { continue };
                                    let c = &self.contacts[ci];
                                    let (rect, resp) = ui.allocate_exact_size(Vec2::new(width, 56.0), Sense::click());
                                    let hover = tween(ui.ctx(), ("contact", c.id), f32::from(u8::from(resp.hovered())), 0.1);
                                    ui.painter().rect_filled(rect, 0.0, lerp_color(Color32::TRANSPARENT, t(DIALOG_OVER), hover));
                                    if !self.avatars.contains_key(&c.id) {
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
                            },
                        );
                        ui.add_space(8.0);
                    });
                paint_modal_subblur(ui.painter(), card.response.rect, 14.0, raw);
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
                    if !self.settings.ayu.show_only_added_emojis_and_stickers {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(cell, 38.0), Sense::click());
                        let k = tween(ui.ctx(), "emadd", f32::from(u8::from(resp.hovered() || self.emoji_add_open)), 0.1);
                        icons::tg(ui.painter(), "menu/add", r.center(), lerp_color(SUBTEXT, WHITE, k));
                        if resp.on_hover_text("Добавить эмодзи").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(StickerAct::AddOpen(!self.emoji_add_open));
                            acts.push(StickerAct::Featured);
                        }
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
        let cell = 44.0;
        let cols = ((ui.available_width() / cell).floor() as usize).max(1);
        let head_h = 30.0;
        let (head, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), head_h), Sense::hover());
        ui.painter().text(
            Pos2::new(head.left() + 8.0, head.bottom() - 6.0),
            Align2::LEFT_BOTTOM,
            format!("{} · {}", set.title, set.count),
            FontId::proportional(12.5),
            Color32::from_rgb(150, 150, 158),
        );
        let total_rows = list.map_or(0, |l| l.len().div_ceil(cols));
        egui::ScrollArea::vertical()
            .id_salt(("pack", set.id))
            .max_height((ui.available_height() - head_h).max(40.0))
            .auto_shrink([false, false])
            .show_rows(ui, cell, total_rows, |ui, range| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let Some(items) = list else { return };
                for row_idx in range {
                    let start = row_idx * cols;
                    let end = (start + cols).min(items.len());
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(cell * cols as f32, cell), Sense::hover());
                    for (k, st) in items[start..end].iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * cell, rect.top()), Vec2::splat(cell));
                        let resp = ui.interact(r, ui.make_persistent_id(("pk", st.id)), Sense::click());
                        let h = tween(ui.ctx(), ("pkh", st.id), f32::from(u8::from(resp.hovered())), 0.1);
                        if h > 0.01 {
                            ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(8), Color32::from_rgba_unmultiplied(255, 255, 255, (26.0 * h) as u8));
                        }
                        let side = 32.0 + 4.0 * h;
                        let area = Rect::from_center_size(r.center(), Vec2::splat(side));
                        if let Some(tex) = self.sticker_tex.get(&st.id) {
                            acts.push(StickerAct::VisibleDoc(st.id));
                            egui::Image::new(tex).fit_to_exact_size(area.size()).paint_at(ui, area);
                        } else if st.has_preview() {
                            acts.push(StickerAct::Thumb(st.id));
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            let fallback = if st.emoji.is_empty() { "✨".to_owned() } else { st.emoji.clone() };
                            acts.push(StickerAct::InsertCustom(fallback, st.id));
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
            ui.add_space(36.0);
            ui.vertical_centered(|ui| {
                if !self.gifs_loaded {
                    ui.add(egui::Spinner::new().size(20.0).color(c_orange()));
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Загрузка сохранённых GIF…").color(SUBTEXT));
                } else {
                    ui.label(egui::RichText::new("Нет сохранённых GIF").color(SUBTEXT));
                    ui.add_space(8.0);
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Обновить").size(12.5).color(WHITE))
                                .fill(t_btn(BUTTON))
                                .corner_radius(CornerRadius::same(10)),
                        )
                        .clicked()
                    {
                        acts.push(StickerAct::ReloadGifs);
                    }
                }
            });
            return;
        }
        let cols = 3usize;
        let width = ui.available_width();
        let cell = width / cols as f32;
        let total_rows = self.gif_list.len().div_ceil(cols);
        egui::ScrollArea::vertical().id_salt("gifs").auto_shrink([false, false]).show_rows(
            ui,
            cell,
            total_rows,
            |ui, range| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for row_idx in range {
                    let start = row_idx * cols;
                    let end = (start + cols).min(self.gif_list.len());
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, cell), Sense::hover());
                    for (k, g) in self.gif_list[start..end].iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * cell, rect.top()), Vec2::splat(cell)).shrink(2.0);
                        let resp = ui.interact(r, ui.make_persistent_id(("gif", g.id)), Sense::click());
                        let h = tween(ui.ctx(), ("gifh", g.id), f32::from(u8::from(resp.hovered())), 0.1);
                        match self.sticker_tex.get(&g.id) {
                            Some(t) => {
                                egui::Image::new(t).fit_to_exact_size(r.size()).corner_radius(8.0).tint(Color32::from_white_alpha((200.0 + 55.0 * h) as u8)).paint_at(ui, r);
                            }
                            None => {
                                if g.has_preview() {
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
            },
        );
    }

    /// Окно «Добавить эмодзи»: рекомендуемые наборы с кнопкой «Добавить».
    pub(crate) fn featured_view(&self, ui: &mut egui::Ui, acts: &mut Vec<StickerAct>) {
        if self.settings.ayu.show_only_added_emojis_and_stickers {
            ui.add_space(40.0);
            ui.vertical_centered(|ui| {
                ui.label(egui::RichText::new("Показываются только установленные наборы").color(SUBTEXT));
            });
            return;
        }
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
            .show_rows(ui, 52.0, self.featured_sets.len(), |ui, range| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                for i in range {
                    let Some(set) = self.featured_sets.get(i) else { continue };
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
                    let icon = Rect::from_center_size(Pos2::new(rect.left() + 28.0, rect.center().y), Vec2::splat(34.0));
                    match set.cover.as_ref().and_then(|c| self.sticker_tex.get(&c.id)) {
                        Some(t) => egui::Image::new(t).fit_to_exact_size(icon.size()).paint_at(ui, icon),
                        None => {
                            if let Some(c) = &set.cover
                                && c.has_preview()
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
                        p.text(btn.center(), Align2::CENTER_CENTER, "Добавить", FontId::proportional(12.5), super::theme::on_accent());
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(StickerAct::Install(i));
                        }
                    }
                }
            });
    }

    /// Вкладка «Стикеры»: Избранные, Недавние и все установленные наборы пользователя + плавные TGS.
    pub(crate) fn sticker_tab(&self, ui: &mut egui::Ui, emoji: Option<&Emoji>, acts: &mut Vec<StickerAct>) {
        if !self.stickers_requested {
            acts.push(StickerAct::LoadSets);
        }
        let has_fav = !self.faved_stickers.is_empty();
        let has_rec = !self.recent_stickers.is_empty();
        let extra = usize::from(has_fav) + usize::from(has_rec);
        let total_tabs = extra + self.sticker_sets.len();
        if total_tabs == 0 {
            ui.add_space(36.0);
            ui.vertical_centered(|ui| {
                if !self.stickers_loaded {
                    ui.add(egui::Spinner::new().size(20.0).color(c_orange()));
                    ui.add_space(6.0);
                    ui.label(egui::RichText::new("Загрузка стикеров…").color(SUBTEXT));
                } else {
                    ui.label(egui::RichText::new("Нет наборов стикеров").color(SUBTEXT));
                    ui.add_space(8.0);
                    if ui
                        .add(
                            egui::Button::new(egui::RichText::new("Обновить").size(12.5).color(WHITE))
                                .fill(t_btn(BUTTON))
                                .corner_radius(CornerRadius::same(10)),
                        )
                        .clicked()
                    {
                        acts.push(StickerAct::ReloadSets);
                    }
                }
            });
            return;
        }
        let recent_cap = if self.settings.ayu.unlimited_recent_stickers || self.settings.extera.unlimited_recent_stickers {
            200
        } else {
            30
        };
        let recent_slice: Vec<rg_core::engine::StickerRow> = self.recent_stickers.iter().take(recent_cap).cloned().collect();
        let cur = self.sticker_cur.min(total_tabs - 1);
        let (title, list): (&str, Option<&[rg_core::engine::StickerRow]>) = if has_fav && cur == 0 {
            ("Избранные стикеры", Some(self.faved_stickers.as_slice()))
        } else if has_rec && cur == usize::from(has_fav) {
            ("Недавние стикеры", Some(recent_slice.as_slice()))
        } else {
            let set_idx = cur - extra;
            let set = &self.sticker_sets[set_idx];
            let l = self.sticker_lists.get(&set.id);
            if l.is_none() {
                acts.push(StickerAct::LoadSet(set_idx));
            }
            (&set.title, l.map(Vec::as_slice))
        };
        let cols = 4usize;
        let width = ui.available_width();
        let cell = (width / cols as f32).max(48.0);
        ui.add_space(4.0);
        ui.label(egui::RichText::new(title).size(12.5).color(Color32::from_rgb(160, 160, 170)));
        ui.add_space(4.0);
        let total_rows = list.map_or(1, |l| l.len().div_ceil(cols));
        egui::ScrollArea::vertical()
            .id_salt(("stk-grid", cur))
            .max_height((ui.available_height() - 44.0).max(80.0))
            .auto_shrink([false, false])
            .show_rows(ui, cell, total_rows, |ui, range| {
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                let Some(items) = list else {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.add(egui::Spinner::new().size(18.0).color(c_orange()));
                    });
                    return;
                };
                for row_idx in range {
                    let start = row_idx * cols;
                    let end = (start + cols).min(items.len());
                    let (rect, _) = ui.allocate_exact_size(Vec2::new(width, cell), Sense::hover());
                    for (k, st) in items[start..end].iter().enumerate() {
                        let r = Rect::from_min_size(Pos2::new(rect.left() + k as f32 * cell, rect.top()), Vec2::splat(cell));
                        let resp = ui.interact(r, ui.make_persistent_id(("stk", st.id)), Sense::click());
                        let h = tween(ui.ctx(), ("stkh", st.id), f32::from(u8::from(resp.hovered())), 0.12);
                        if h > 0.01 {
                            ui.painter().rect_filled(r.shrink(2.0), CornerRadius::same(10), Color32::from_rgba_unmultiplied(255, 255, 255, (22.0 * h) as u8));
                        }
                        let side = (cell - 12.0) * (1.0 + 0.08 * ios_spring_bounce(h));
                        let area = Rect::from_center_size(r.center(), Vec2::splat(side));
                        if let Some(tex) = self.sticker_tex.get(&st.id) {
                            acts.push(StickerAct::VisibleDoc(st.id));
                            egui::Image::new(tex).fit_to_exact_size(area.size()).paint_at(ui, area);
                        } else if st.has_preview() || st.mime == "application/x-tgsticker" {
                            acts.push(StickerAct::Thumb(st.id));
                            ui.painter().rect_filled(area.shrink(10.0), CornerRadius::same(12), Color32::from_rgba_unmultiplied(255, 255, 255, 10));
                        } else if let Some(img) = emoji.and_then(|e| st.emoji.chars().next().and_then(|c| e.image(c, 34.0))) {
                            img.paint_at(ui, Rect::from_center_size(r.center(), Vec2::splat(34.0)));
                        }
                        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(StickerAct::Send(st.id));
                        }
                    }
                }
            });
        ui.add_space(4.0);
        let (bar, _) = ui.allocate_exact_size(Vec2::new(width, 38.0), Sense::hover());
        ui.painter().line_segment([bar.left_top(), bar.right_top()], Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 18)));
        let mut inner = ui.new_child(egui::UiBuilder::new().max_rect(bar));
        egui::ScrollArea::horizontal()
            .id_salt("stk-set-bar")
            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
            .show(&mut inner, |ui| {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing = Vec2::new(4.0, 0.0);
                    ui.set_height(38.0);
                    for tab_i in 0..total_tabs {
                        let (r, resp) = ui.allocate_exact_size(Vec2::new(34.0, 38.0), Sense::click());
                        let sel = tween(ui.ctx(), ("stkseth", tab_i), f32::from(u8::from(tab_i == cur || resp.hovered())), 0.12);
                        let p = ui.painter();
                        let c = r.center();
                        let bg = if tab_i == cur {
                            t_btn(c_orange())
                        } else {
                            lerp_color(Color32::from_rgb(48, 48, 56), Color32::from_rgb(76, 76, 86), sel)
                        };
                        p.circle_filled(c, 13.5, bg);
                        let (ico_name, lbl, tip): (Option<&str>, Option<String>, String) = if has_fav && tab_i == 0 {
                            (Some("menu/favorite"), None, "Избранные стикеры".to_owned())
                        } else if has_rec && tab_i == usize::from(has_fav) {
                            (Some("emoji/emoji_recent"), None, "Недавние стикеры".to_owned())
                        } else {
                            let s = &self.sticker_sets[tab_i - extra];
                            let init = s.title.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_else(|| "S".to_owned());
                            (None, Some(init), s.title.clone())
                        };
                        let fg = if tab_i == cur { super::theme::on_accent() } else { WHITE };
                        if let Some(ico_n) = ico_name {
                            icons::tg_sized(p, ico_n, c, Vec2::splat(16.0), fg);
                        } else if let Some(lbl_s) = lbl {
                            p.text(c, Align2::CENTER_CENTER, lbl_s, FontId::proportional(13.0), fg);
                        }
                        if resp.on_hover_text(tip).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(StickerAct::SelectSet(tab_i));
                        }
                    }
                });
            });
        ui.advance_cursor_after_rect(bar);
        let _ = icons::Ico::Sticker;
    }
}
