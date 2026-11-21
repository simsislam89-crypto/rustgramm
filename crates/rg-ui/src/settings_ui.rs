//! Левая панель (главное меню) и окно «AyuGram Preferences» — как в AyuGram Desktop 7.2.9:
//! категории AyuGram / Filters / General / Appearance / Chats / Other. Порядок строк и
//! подписи сняты с живого окна оригинала.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Vec2};
use rg_settings::{
    ChannelBottomButton, MenuVisibility, PeerIdDisplay, SendWithoutSound, Settings,
};

use super::kit::{self, divider, desc, nav_row, section, slider, sub_toggle, toggle, toggle_ex, value_row};
use super::{App, Cmd, icons, theme::*};

const BOX_W: f32 = 380.0;

/// Номера страниц окна настроек.
pub(crate) mod page {
    pub const MENU: usize = 0;
    pub const AYU: usize = 1;
    pub const FILTERS: usize = 2;
    pub const GENERAL: usize = 3;
    pub const APPEARANCE: usize = 4;
    pub const CHATS: usize = 5;
    pub const OTHER: usize = 6;
    pub const SHARED: usize = 7;
    pub const SHADOW: usize = 8;
}

fn title_of(p: usize) -> &'static str {
    match p {
        page::AYU => "Rustgramm",
        page::FILTERS => "Фильтры",
        page::GENERAL => "Общие",
        page::APPEARANCE => "Внешний вид",
        page::CHATS => "Чаты",
        page::OTHER => "Прочее",
        page::SHARED => "Общие фильтры",
        page::SHADOW => "Теневой бан",
        _ => "Rustgramm",
    }
}

fn menu_value(v: MenuVisibility) -> &'static str {
    match v {
        MenuVisibility::Hidden => "Скрыто",
        MenuVisibility::Visible => "Показано",
        MenuVisibility::WithModifier => "В расширенном меню",
    }
}

fn next_menu(v: MenuVisibility) -> MenuVisibility {
    match v {
        MenuVisibility::Hidden => MenuVisibility::Visible,
        MenuVisibility::Visible => MenuVisibility::WithModifier,
        MenuVisibility::WithModifier => MenuVisibility::Hidden,
    }
}

impl App {
    /// Левая выдвижная панель: профиль, разделы, переключатели ночной темы и призрака.
    #[allow(deprecated)] // Area::new требует Id, а замена Id::new ещё не даёт стабильного id
    pub(crate) fn drawer(&mut self, ctx: &egui::Context) {
        let open = ease_out(tween(ctx, "drawer", f32::from(u8::from(self.drawer_open)), 0.22));
        if open < 0.005 && !self.drawer_open {
            return;
        }
        let screen = ctx.content_rect();
        let w = 275.0_f32.min(screen.width());
        // Затемнение: клик вне панели закрывает её.
        egui::Area::new(egui::Id::new("drawer-dim"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha((110.0 * open) as u8));
                if resp.clicked() {
                    self.drawer_open = false;
                }
            });
        let mut s = self.settings.clone();
        let a = s.ayu.clone();
        let me = self.me.clone();
        let me_tex = self.dialogs.iter().find(|d| d.saved).and_then(|d| self.avatars.get(&d.id));
        let corners = s.ayu.avatar_corners;
        let mut act: Option<&str> = None;
        egui::Area::new(egui::Id::new("drawer"))
            .order(egui::Order::Tooltip)
            .fixed_pos(Pos2::new(-(1.0 - open) * w, 0.0))
            .show(ctx, |ui| {
                let (rect, _) = ui.allocate_exact_size(Vec2::new(w, screen.height()), Sense::hover());
                ui.painter().rect_filled(rect, 0.0, t2(WINDOW_BG));
                let mut child = ui.new_child(egui::UiBuilder::new().max_rect(rect));
                let ui = &mut child;
                ui.spacing_mut().item_spacing = Vec2::ZERO;
                // Шапка: аватар, имя, ссылка на настройки AyuGram.
                let (head, _) = ui.allocate_exact_size(Vec2::new(w, 152.0), Sense::hover());
                draw_avatar(ui, Pos2::new(head.left() + 46.0, head.top() + 46.0), &me, 1, 56.0, me_tex, corners);
                ui.painter().text(
                    Pos2::new(head.left() + 18.0, head.top() + 96.0),
                    Align2::LEFT_CENTER,
                    &me,
                    FontId::proportional(15.0),
                    WHITE,
                );
                let prefs = Rect::from_min_size(
                    Pos2::new(head.left() + 8.0, head.top() + 108.0),
                    Vec2::new(w - 16.0, 28.0),
                );
                let pr = ui.interact(prefs, ui.make_persistent_id("prefs-link"), Sense::click());
                ui.painter().text(
                    Pos2::new(head.left() + 18.0, prefs.center().y),
                    Align2::LEFT_CENTER,
                    "Настройки Rustgramm",
                    FontId::proportional(13.0),
                    ACCENT,
                );
                icons::chevron_down(ui.painter(), Pos2::new(head.right() - 22.0, prefs.center().y), SUBTEXT);
                if pr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    act = Some("prefs");
                }
                let item = |ui: &mut egui::Ui, icon: icons::Ico, text: &str| -> bool {
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 40.0), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, t(WINDOW_OVER));
                    }
                    icons::ico(ui.painter(), Pos2::new(r.left() + 30.0, r.center().y), icon, DIALOG_DATE);
                    ui.painter().text(
                        Pos2::new(r.left() + 64.0, r.center().y),
                        Align2::LEFT_CENTER,
                        text,
                        FontId::proportional(14.0),
                        WHITE,
                    );
                    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
                };
                if a.drawer_my_profile && item(ui, icons::Ico::Profile, "Мой профиль") {
                    act = Some("profile");
                }
                kit::divider(ui);
                if a.drawer_new_group && item(ui, icons::Ico::Group, "Новая группа") {
                    act = Some("group");
                }
                if a.drawer_new_channel && item(ui, icons::Ico::Channel, "Новый канал") {
                    act = Some("channel");
                }
                if a.drawer_contacts && item(ui, icons::Ico::Contacts, "Контакты") {
                    act = Some("contacts");
                }
                if a.drawer_calls && item(ui, icons::Ico::Calls, "Звонки") {
                    act = Some("calls");
                }
                if a.drawer_saved_messages && item(ui, icons::Ico::Saved, "Избранное") {
                    act = Some("saved");
                }
                if a.drawer_l_read_toggle && item(ui, icons::Ico::Read, "Прочитать на сервере") {
                    act = Some("read");
                }
                if item(ui, icons::Ico::Settings, "Настройки") {
                    act = Some("settings");
                }
                if a.drawer_night_mode_toggle {
                    let mut night = true;
                    kit::toggle_icon(ui, icons::Ico::Moon, "Ночной режим", &mut night);
                }
                if a.drawer_ghost_toggle {
                    let mut ghost = s.ghost.active;
                    if kit::toggle_icon(ui, icons::Ico::Ghost, "Режим призрака", &mut ghost) {
                        s.ghost.set_active(ghost);
                    }
                }
                if a.drawer_streamer_toggle {
                    let mut st = s.ayu.streamer_mode;
                    if toggle(ui, "Режим стримера", &mut st) {
                        s.ayu.streamer_mode = st;
                    }
                }
            });
        match act {
            Some("prefs") => {
                self.drawer_open = false;
                self.settings_open = true;
                self.settings_tab = page::MENU;
            }
            Some("settings") => {
                self.drawer_open = false;
                self.open_tg_settings();
            }
            Some("saved") => {
                self.drawer_open = false;
                if let Some(i) = self.dialogs.iter().position(|d| d.saved) {
                    self.open_dialog(i);
                }
            }
            Some("read") => {
                self.drawer_open = false;
                self.send(Cmd::ForceRead);
            }
            Some("contacts") => {
                self.drawer_open = false;
                self.contacts_open = true;
                self.contact_search.clear();
                self.send(Cmd::Contacts);
            }
            Some("profile") => {
                self.drawer_open = false;
                if let Some(i) = self.dialogs.iter().position(|d| d.saved) {
                    self.open_dialog(i);
                    self.info_open = true;
                }
            }
            Some("group") => {
                self.drawer_open = false;
                self.create_kind = 1;
                self.create_title.clear();
                self.create_members.clear();
                self.send(Cmd::Contacts);
            }
            Some("channel") => {
                self.drawer_open = false;
                self.create_kind = 2;
                self.create_title.clear();
                self.create_about.clear();
            }
            Some(other) => {
                self.drawer_open = false;
                let name = match other {
                    "profile" => "Профиль",
                    "group" => "Создание группы",
                    "channel" => "Создание канала",
                    "contacts" => "Контакты",
                    _ => "Звонки",
                };
                self.status = format!("{name}: этот раздел будет в следующем этапе");
            }
            None => {}
        }
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(Box::new(s)));
        }
    }

    /// Окно настроек: заголовок со стрелкой «назад» и крестиком, список строк.
    #[allow(deprecated)]
    pub(crate) fn settings_window(&mut self, ctx: &egui::Context) {
        self.logout_window(ctx);
        let raw = tween(ctx, "prefs", f32::from(u8::from(self.settings_open)), 0.2);
        let appear = spring(raw);
        if raw < 0.005 && !self.settings_open {
            return;
        }
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("prefs-dim"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                ui.painter()
                    .rect_filled(rect, 0.0, Color32::from_black_alpha((140.0 * raw) as u8));
                if resp.clicked() {
                    self.settings_open = false;
                }
            });
        let mut s = self.settings.clone();
        let mut tab = self.settings_tab;
        let mut close = false;
        let mut new_filter = std::mem::take(&mut self.filter_new);
        let mut shadow_new = std::mem::take(&mut self.shadow_new);
        let mut expanded = self.expanded;
        let mut reset = false;
        let mut logout_ask = false;
        let max_h = (screen.height() - 40.0).clamp(240.0, 700.0);
        egui::Area::new(egui::Id::new("prefs-box"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, (1.0 - appear) * 18.0))
            .show(ctx, |ui| {
                ui.set_opacity(raw.min(1.0).powf(0.6));
                egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .corner_radius(CornerRadius::same(10))
                    .show(ui, |ui| {
                        let width = BOX_W.min(screen.width() - 16.0);
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        // Шапка.
                        let (head, _) = ui.allocate_exact_size(Vec2::new(width, 50.0), Sense::hover());
                        if tab != page::MENU {
                            let back = Rect::from_center_size(
                                Pos2::new(head.left() + 26.0, head.center().y),
                                Vec2::splat(34.0),
                            );
                            let r = ui.interact(back, ui.make_persistent_id("prefs-back"), Sense::click());
                            icons::back(ui.painter(), back.center(), if r.hovered() { WHITE } else { SUBTEXT });
                            if r.clicked() {
                                tab = match tab {
                                    page::SHARED | page::SHADOW => page::FILTERS,
                                    _ => page::MENU,
                                };
                            }
                            ui.painter().text(
                                Pos2::new(head.left() + 52.0, head.center().y),
                                Align2::LEFT_CENTER,
                                title_of(tab),
                                FontId::proportional(16.0),
                                WHITE,
                            );
                        }
                        let x = Rect::from_center_size(
                            Pos2::new(head.right() - 24.0, head.center().y),
                            Vec2::splat(34.0),
                        );
                        let xr = ui.interact(x, ui.make_persistent_id("prefs-close"), Sense::click());
                        icons::tg(ui.painter(), "info/info_close", x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
                        if xr.clicked() {
                            close = true;
                        }
                        egui::ScrollArea::vertical()
                            .max_height(max_h - 50.0)
                            .min_scrolled_height(match tab {
                                page::AYU | page::GENERAL | page::APPEARANCE | page::CHATS => max_h - 50.0,
                                _ => 120.0,
                            })
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::ZERO;
                                match tab {
                                    page::AYU => ayu_page(ui, &mut s, &mut expanded),
                                    page::FILTERS => {
                                        if let Some(p) = filters_page(ui, &mut s) {
                                            tab = p;
                                        }
                                    }
                                    page::GENERAL => general_page(ui, &mut s, &mut expanded),
                                    page::APPEARANCE => appearance_page(ui, &mut s),
                                    page::CHATS => chats_page(ui, &mut s, &mut expanded),
                                    page::OTHER => reset = other_page(ui, &mut logout_ask),
                                    page::SHARED => shared_page(ui, &mut s, &mut new_filter),
                                    page::SHADOW => shadow_page(ui, &mut s, &mut shadow_new),
                                    _ => {
                                        if let Some(p) = menu_page(ui) {
                                            tab = p;
                                        }
                                    }
                                }
                            });
                    });
            });
        self.filter_new = new_filter;
        self.shadow_new = shadow_new;
        self.expanded = expanded;
        self.settings_tab = tab;
        if close || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.settings_open = false;
        }
        if logout_ask {
            // Подтверждение рисуется поверх только при закрытом окне настроек.
            self.logout_confirm = true;
            self.settings_open = false;
        }
        if reset {
            s = Settings::first_run();
            self.status = "Настройки сброшены".to_owned();
        }
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(Box::new(s)));
        }
    }
}

impl App {
    /// Подтверждение выхода из аккаунта.
    #[allow(deprecated)]
    fn logout_window(&mut self, ctx: &egui::Context) {
        if !self.logout_confirm {
            return;
        }
        let mut close = false;
        egui::Window::new("Выйти из аккаунта?")
            .collapsible(false)
            .resizable(false)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label("Сессия на этом устройстве завершится, сохранённые сообщения и кэш этого аккаунта будут стёрты. Приложение перезапустится.");
                ui.horizontal(|ui| {
                    if ui.button(RichText::new("Выйти").color(DANGER)).clicked() {
                        self.send(Cmd::LogOut);
                        close = true;
                    }
                    if ui.button("Отмена").clicked() {
                        close = true;
                    }
                });
            });
        if close {
            self.logout_confirm = false;
        }
    }
}

/// Категории и ссылки (верхний уровень).
fn menu_page(ui: &mut egui::Ui) -> Option<usize> {
    let w = ui.available_width();
    let (head, _) = ui.allocate_exact_size(Vec2::new(w, 130.0), Sense::hover());
    let c = Pos2::new(head.center().x, head.top() + 42.0);
    ui.painter().circle_filled(c, 30.0, navy(0x2b2a4a));
    icons::logo(ui.painter(), c, 20.0, WHITE);
    ui.painter().text(
        Pos2::new(head.center().x, head.top() + 88.0),
        Align2::CENTER_CENTER,
        format!("Rustgramm v{}", env!("CARGO_PKG_VERSION")),
        FontId::proportional(14.0),
        WHITE,
    );
    ui.painter().text(
        Pos2::new(head.center().x, head.top() + 108.0),
        Align2::CENTER_CENTER,
        "Клиент Telegram на Rust: данные только на серверах Telegram",
        FontId::proportional(11.0),
        SUBTEXT,
    );
    divider(ui);
    section(ui, "Категории");
    let mut go = None;
    for (icon, name, p) in [
        ("heart", "Rustgramm", page::AYU),
        ("menu/tag_filter", "Фильтры", page::FILTERS),
        ("grid", "Общие", page::GENERAL),
        ("menu/palette", "Внешний вид", page::APPEARANCE),
        ("menu/chat_bubble", "Чаты", page::CHATS),
        ("menu/favorite", "Прочее", page::OTHER),
    ] {
        if nav_row(ui, icon, name, "") {
            go = Some(p);
        }
    }
    ui.add_space(8.0);
    go
}

fn ghost_counter(s: &Settings) -> usize {
    let g = &s.ghost;
    [
        !g.send_read_messages,
        !g.send_read_stories,
        !g.send_online_packets,
        !g.send_upload_progress,
        g.send_offline_after_online,
    ]
    .iter()
    .filter(|b| **b)
    .count()
}

fn ayu_page(ui: &mut egui::Ui, s: &mut Settings, expanded: &mut u32) {
    section(ui, "Основы призрака");
    let n = ghost_counter(s);
    let mut master = s.ghost.active;
    let open = *expanded & 1 != 0;
    // Строка «Режим призрака n/5 ⌄»: клик по подписи раскрывает список, по переключателю — вкл/выкл.
    if toggle_ex(ui, "Режим призрака", Some((n, 5, open)), false, &mut master) {
        if ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.x < ui.min_rect().right() - 80.0) {
            *expanded ^= 1;
        } else {
            s.ghost.set_active(master);
        }
    }
    if *expanded & 1 != 0 {
        let g = &mut s.ghost;
        let mut v = !g.send_read_messages;
        if sub_toggle(ui, "Не отправлять «прочитано»", &mut v) {
            g.send_read_messages = !v;
        }
        let mut v = !g.send_read_stories;
        if sub_toggle(ui, "Не отправлять просмотры историй", &mut v) {
            g.send_read_stories = !v;
        }
        let mut v = !g.send_online_packets;
        if sub_toggle(ui, "Не отправлять «в сети»", &mut v) {
            g.send_online_packets = !v;
        }
        let mut v = !g.send_upload_progress;
        if sub_toggle(ui, "Не отправлять «печатает…»", &mut v) {
            g.send_upload_progress = !v;
        }
        sub_toggle(ui, "Сразу «не в сети»", &mut g.send_offline_after_online);
    }
    toggle(ui, "Прочитывать при действии", &mut s.ghost.mark_read_after_action);
    desc(ui, "Автоматически отмечает сообщения прочитанными, когда вы отправляете новое или ставите реакцию.");
    toggle(ui, "Отложенные сообщения", &mut s.ghost.use_scheduled_messages);
    desc(ui, "Исходящие отправляются отложенно (примерно через 12 секунд, медиа — дольше). Вы не будете в сети. Не используйте в ненадёжных сетях.");
    let label = match s.ghost.send_without_sound {
        SendWithoutSound::Never => "Никогда",
        SendWithoutSound::InGhostMode => "В режиме призрака",
        SendWithoutSound::Always => "Всегда",
    };
    if value_row(ui, "Отправка без звука", label, false) {
        s.ghost.send_without_sound = match s.ghost.send_without_sound {
            SendWithoutSound::Never => SendWithoutSound::InGhostMode,
            SendWithoutSound::InGhostMode => SendWithoutSound::Always,
            SendWithoutSound::Always => SendWithoutSound::Never,
        };
    }
    desc(ui, "Исходящие сообщения по умолчанию уходят без звука.");
    toggle(
        ui,
        "Предупреждение об историях",
        &mut s.ayu.suggest_ghost_mode_before_viewing_story,
    );
    desc(ui, "Показывает предупреждение перед открытием истории с предложением включить режим призрака.");
    section(ui, "Шпионская часть");
    toggle(ui, "Сохранять удалённые сообщения", &mut s.ayu.save_deleted_messages);
    toggle(ui, "Сохранять историю правок", &mut s.ayu.save_messages_history);
    divider(ui);
    toggle(ui, "Сохранять в чатах с ботами", &mut s.ayu.save_for_bots);
    divider(ui);
    section(ui, "Прочее");
    toggle(ui, "Локальный Telegram Premium", &mut s.ayu.local_premium);
    toggle(ui, "Отключить рекламу", &mut s.ayu.disable_ads);
    ui.add_space(8.0);
}

fn filters_page(ui: &mut egui::Ui, s: &mut Settings) -> Option<usize> {
    section(ui, "Фильтры сообщений");
    toggle(ui, "Включить фильтры", &mut s.ayu.filters_enabled);
    toggle(ui, "Общие фильтры в чатах", &mut s.ayu.filters_enabled_in_chats);
    toggle(ui, "Скрывать заблокированных", &mut s.ayu.hide_from_blocked);
    divider(ui);
    let mut go = None;
    if nav_row(ui, "", "Общие фильтры", "") {
        go = Some(page::SHARED);
    }
    if nav_row(ui, "", "Теневой бан", "") {
        go = Some(page::SHADOW);
    }
    ui.add_space(8.0);
    go
}

fn shared_page(ui: &mut egui::Ui, s: &mut Settings, new_filter: &mut String) {
    section(ui, "Правила");
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        ui.add(
            egui::TextEdit::singleline(new_filter)
                .hint_text("Текст или регулярное выражение")
                .desired_width(220.0),
        );
        let t = new_filter.trim().to_owned();
        if ui.button("Добавить").clicked() && !t.is_empty() {
            s.filters.push(rg_settings::Filter {
                text: t,
                ..rg_settings::Filter::default()
            });
            new_filter.clear();
            s.ayu.filters_enabled = true;
        }
    });
    ui.add_space(6.0);
    let mut remove = None;
    for (i, f) in s.filters.iter_mut().enumerate() {
        toggle(ui, &f.text.chars().take(28).collect::<String>(), &mut f.enabled);
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.checkbox(&mut f.is_regex, "regex");
            ui.checkbox(&mut f.case_sensitive, "Aa");
            if ui.small_button("Удалить").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        s.filters.remove(i);
    }
    if s.filters.is_empty() {
        desc(ui, "Правил пока нет. Сообщения, подходящие под правило, скрываются в ленте.");
    }
}

fn shadow_page(ui: &mut egui::Ui, s: &mut Settings, new_id: &mut String) {
    section(ui, "Скрытые пользователи");
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        ui.add(
            egui::TextEdit::singleline(new_id)
                .hint_text("ID пользователя")
                .desired_width(200.0),
        );
        if ui.button("Добавить").clicked()
            && let Ok(id) = new_id.trim().parse::<i64>()
            && !s.ayu.shadow_ban_ids.contains(&id)
        {
            s.ayu.shadow_ban_ids.push(id);
            new_id.clear();
        }
    });
    ui.add_space(6.0);
    let mut remove = None;
    for (i, id) in s.ayu.shadow_ban_ids.iter().enumerate() {
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.label(RichText::new(id.to_string()).color(WHITE));
            if ui.small_button("Убрать").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        s.ayu.shadow_ban_ids.remove(i);
    }
    desc(ui, "Сообщения этих пользователей не показываются в ленте. Идентификатор виден в «Сведениях» сообщения.");
}

fn general_page(ui: &mut egui::Ui, s: &mut Settings, expanded: &mut u32) {
    section(ui, "Перевод сообщений");
    value_row(ui, "Переводчик", "Telegram", true);
    divider(ui);
    section(ui, "Общие");
    let a = &mut s.ayu;
    toggle(ui, "Отключить истории", &mut a.disable_stories);
    toggle(ui, "Не предупреждать об открытии ссылок", &mut a.disable_open_link_warning);
    let n = usize::from(a.collapse_similar_channels) + usize::from(a.hide_similar_channels);
    let mut any = n > 0;
    if toggle_ex(ui, "Отключить похожие каналы", Some((n, 2, *expanded & 2 != 0)), false, &mut any) {
        if ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.x < ui.min_rect().right() - 80.0) {
            *expanded ^= 2;
        } else {
            a.collapse_similar_channels = any;
            a.hide_similar_channels = any;
        }
    }
    if *expanded & 2 != 0 {
        sub_toggle(ui, "Сворачивать", &mut a.collapse_similar_channels);
        sub_toggle(ui, "Скрывать", &mut a.hide_similar_channels);
    }
    toggle(ui, "Отключить задержку уведомлений", &mut a.disable_notifications_delay);
    divider(ui);
    toggle_ex(ui, "Чистка «зальго»", None, true, &mut a.filter_zalgo);
    toggle(ui, "Улучшенные превью ссылок", &mut a.improve_link_previews);
    let n = [a.sticker_confirmation, a.gif_confirmation, a.voice_confirmation, a.round_confirmation]
        .iter()
        .filter(|b| **b)
        .count();
    let mut any = n > 0;
    if toggle_ex(ui, "Подтверждение отправки", Some((n, 4, *expanded & 4 != 0)), false, &mut any) {
        if ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.x < ui.min_rect().right() - 80.0) {
            *expanded ^= 4;
        } else {
            a.sticker_confirmation = any;
            a.gif_confirmation = any;
            a.voice_confirmation = any;
            a.round_confirmation = any;
        }
    }
    if *expanded & 4 != 0 {
        sub_toggle(ui, "Стикеры", &mut a.sticker_confirmation);
        sub_toggle(ui, "GIF", &mut a.gif_confirmation);
        sub_toggle(ui, "Голосовые", &mut a.voice_confirmation);
        sub_toggle(ui, "Видеокружки", &mut a.round_confirmation);
    }
    toggle(ui, "Секунды во времени сообщения", &mut a.show_message_seconds);
    let id = match a.show_peer_id {
        PeerIdDisplay::Hidden => "Скрыт",
        PeerIdDisplay::BotApi => "Bot API",
        PeerIdDisplay::Telegram => "Telegram",
    };
    if value_row(ui, "Показывать ID", id, false) {
        a.show_peer_id = match a.show_peer_id {
            PeerIdDisplay::Hidden => PeerIdDisplay::BotApi,
            PeerIdDisplay::BotApi => PeerIdDisplay::Telegram,
            PeerIdDisplay::Telegram => PeerIdDisplay::Hidden,
        };
    }
    divider(ui);
    section(ui, "Веб-приложения");
    toggle(ui, "Представляться Android", &mut a.spoof_webview_as_android);
    let n = usize::from(a.increase_webview_height) + usize::from(a.increase_webview_width);
    let mut any = n > 0;
    if toggle_ex(ui, "Большое окно", Some((n, 2, *expanded & 8 != 0)), false, &mut any) {
        if ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.x < ui.min_rect().right() - 80.0) {
            *expanded ^= 8;
        } else {
            a.increase_webview_height = any;
            a.increase_webview_width = any;
        }
    }
    if *expanded & 8 != 0 {
        sub_toggle(ui, "Выше", &mut a.increase_webview_height);
        sub_toggle(ui, "Шире", &mut a.increase_webview_width);
    }
    ui.add_space(8.0);
}

fn appearance_page(ui: &mut egui::Ui, s: &mut Settings) {
    section(ui, "Окно");
    let mut o = i32::from(s.window_opacity);
    let shown = format!("{o}%");
    if slider(ui, "Прозрачность окна", &mut o, (15, 100), shown) {
        s.window_opacity = o.clamp(15, 100) as u8;
    }
    desc(ui, "Чем меньше значение, тем сильнее окно просвечивает. По умолчанию 86%.");
    divider(ui);
    let a = &mut s.ayu;
    section(ui, "Значок приложения");
    // Сетка 4×3 кружков; цвета повторяют набор оригинала.
    let palette = [
        0x3a3260, 0x8f6af0, 0x5865f2, 0x14150f, 0xd9423f, 0xf4f4f4, 0xf4f4f4, 0xf6f0f4, 0x1f8f86, 0x2a2840,
        0x6a4fb0, 0xd7332f,
    ];
    let cur: usize = a.app_icon.parse().unwrap_or(0);
    let w = ui.available_width();
    for row in 0..3 {
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 64.0), Sense::hover());
        for col in 0..4 {
            let i = row * 4 + col;
            let c = Pos2::new(rect.left() + w * (col as f32 + 0.5) / 4.0, rect.center().y);
            let r = Rect::from_center_size(c, Vec2::splat(52.0));
            let resp = ui.interact(r, ui.make_persistent_id(("icon", i)), Sense::click());
            if cur == i {
                ui.painter().rect_filled(r, CornerRadius::same(8), t(WINDOW_OVER));
            }
            ui.painter().circle_filled(c, 20.0, navy(palette[i]));
            icons::logo(ui.painter(), c, 13.0, Color32::WHITE);
            if resp.clicked() {
                a.app_icon = i.to_string();
            }
        }
    }
    divider(ui);
    toggle(ui, "Скрыть значок уведомлений", &mut a.hide_notification_badge);
    desc(ui, "Скрывает счётчик на значке приложения в панели задач и в трее.");
    let corners = if a.avatar_corners >= 23 { "КРУГ" } else { "СКРУГЛЕНИЕ" };
    section(ui, &format!("Скругление аватарок  [{corners}]"));
    let mut v = a.avatar_corners;
    if slider(ui, "", &mut v, (0, 23), String::new()) {
        a.avatar_corners = v;
    }
    toggle(ui, "Один радиус для всех форм", &mut a.single_corner_radius);
    desc(ui, "Форумы получат ту же форму аватарок, что и чаты.");
    section(ui, "Оформление");
    toggle(ui, "Переключатели MD3", &mut a.material_switches);
    toggle(ui, "Отключить фоны чатов", &mut a.disable_custom_backgrounds);
    toggle(ui, "Скрыть статусы Premium", &mut a.hide_premium_statuses);
    let font = if a.mono_font.is_empty() { "По умолчанию".to_owned() } else { a.mono_font.clone() };
    if value_row(ui, "Моноширинный шрифт", &font, false) {
        a.mono_font = if a.mono_font.is_empty() { "Consolas".to_owned() } else { String::new() };
    }
    divider(ui);
    section(ui, "Папки чатов");
    toggle(ui, "Скрыть счётчики уведомлений", &mut a.hide_notification_counters);
    toggle(ui, "Скрыть вкладку «Все чаты»", &mut a.hide_all_chats_folder);
    divider(ui);
    section(ui, "Элементы трея");
    toggle(ui, "Режим призрака", &mut a.tray_ghost_toggle);
    toggle(ui, "Режим стримера", &mut a.tray_streamer_toggle);
    divider(ui);
    section(ui, "Элементы меню");
    toggle(ui, "Мой профиль", &mut a.drawer_my_profile);
    toggle(ui, "Боты", &mut a.drawer_bots);
    toggle(ui, "Новая группа", &mut a.drawer_new_group);
    toggle(ui, "Новый канал", &mut a.drawer_new_channel);
    toggle(ui, "Контакты", &mut a.drawer_contacts);
    toggle(ui, "Звонки", &mut a.drawer_calls);
    toggle(ui, "Избранное", &mut a.drawer_saved_messages);
    toggle(ui, "Прочитать на сервере", &mut a.drawer_l_read_toggle);
    toggle(ui, "Секретное чтение", &mut a.drawer_s_read_toggle);
    toggle(ui, "Ночной режим", &mut a.drawer_night_mode_toggle);
    toggle(ui, "Режим призрака", &mut a.drawer_ghost_toggle);
    toggle(ui, "Режим стримера", &mut a.drawer_streamer_toggle);
    ui.add_space(8.0);
}

fn chats_page(ui: &mut egui::Ui, s: &mut Settings, expanded: &mut u32) {
    let a = &mut s.ayu;
    section(ui, "Стикеры и эмодзи");
    toggle(ui, "Только добавленные эмодзи и стикеры", &mut a.show_only_added_emojis_and_stickers);
    toggle(ui, "Неограниченные недавние стикеры", &mut a.unlimited_recent_stickers);
    let hidden = [!a.show_channel_reactions, !a.show_group_reactions, !a.show_private_chat_reactions];
    let n = hidden.iter().filter(|b| **b).count();
    let mut any = n > 0;
    if toggle_ex(ui, "Скрыть реакции", Some((n, 3, *expanded & 16 != 0)), false, &mut any) {
        if ui.input(|i| i.pointer.interact_pos()).is_some_and(|p| p.x < ui.min_rect().right() - 80.0) {
            *expanded ^= 16;
        } else {
            a.show_channel_reactions = !any;
            a.show_group_reactions = !any;
            a.show_private_chat_reactions = !any;
        }
    }
    if *expanded & 16 != 0 {
        let mut v = !a.show_channel_reactions;
        if sub_toggle(ui, "В каналах", &mut v) {
            a.show_channel_reactions = !v;
        }
        let mut v = !a.show_group_reactions;
        if sub_toggle(ui, "В группах", &mut v) {
            a.show_group_reactions = !v;
        }
        let mut v = !a.show_private_chat_reactions;
        if sub_toggle(ui, "В личных чатах", &mut v) {
            a.show_private_chat_reactions = !v;
        }
    }
    divider(ui);
    section(ui, "Группы и каналы");
    let btn = match a.channel_bottom_button {
        ChannelBottomButton::Hidden => "Скрыта",
        ChannelBottomButton::Mute => "Без звука",
        ChannelBottomButton::Discuss => "Обсудить",
        ChannelBottomButton::DiscussWithFallback => "Обсудить",
    };
    if value_row(ui, "Нижняя кнопка канала", btn, false) {
        a.channel_bottom_button = match a.channel_bottom_button {
            ChannelBottomButton::Hidden => ChannelBottomButton::Mute,
            ChannelBottomButton::Mute => ChannelBottomButton::Discuss,
            ChannelBottomButton::Discuss => ChannelBottomButton::DiscussWithFallback,
            ChannelBottomButton::DiscussWithFallback => ChannelBottomButton::Hidden,
        };
    }
    toggle(ui, "Быстрые действия администратора", &mut a.quick_admin_shortcuts);
    toggle(ui, "Отключить стикер-приветствие", &mut a.disable_greeting_sticker);
    toggle(ui, "Снимок сообщения", &mut a.show_message_shot);
    desc(ui, "Позволяет сделать снимок выбранных сообщений.");
    divider(ui);
    section(ui, "Сообщения");
    toggle(ui, "Значки вместо текста", &mut a.replace_bottom_info_with_icons);
    toggle(ui, "Убрать хвостик пузыря", &mut a.remove_message_tail);
    toggle(ui, "Скрыть боковую кнопку «Поделиться»", &mut a.hide_fast_share);
    let mut plain = !a.show_colorful_replies;
    if toggle(ui, "Отключить цветные ответы", &mut plain) {
        a.show_colorful_replies = !plain;
    }
    toggle_ex(ui, "Полупрозрачные удалённые", None, true, &mut a.semi_transparent_deleted_messages);
    divider(ui);
    let mut r = a.message_bubble_radius;
    let shown = r.to_string();
    if slider(ui, "Радиус пузыря", &mut r, (0, 20), shown) {
        a.message_bubble_radius = r;
    }
    divider(ui);
    let mut m = a.wide_multiplier_pct;
    let shown = format!("{:.2}", f64::from(m) / 100.0);
    if slider(ui, "Множитель ширины сообщений", &mut m, (50, 200), shown) {
        a.wide_multiplier_pct = m;
    }
    desc(ui, "Ширину сообщений можно изменить для удобства на широких мониторах.");
    section(ui, "Контекстное меню");
    for (label, v) in [
        ("Панель реакций", &mut a.menu_reactions_panel),
        ("Просмотры", &mut a.menu_views_panel),
        ("Скрыть", &mut a.menu_hide_message),
        ("Сообщения пользователя", &mut a.menu_user_messages),
        ("Сведения", &mut a.menu_message_details),
        ("Повторить", &mut a.menu_repeat_message),
        ("Добавить в фильтры", &mut a.menu_add_filter),
    ] {
        if value_row(ui, label, menu_value(*v), false) {
            *v = next_menu(*v);
        }
    }
    divider(ui);
    section(ui, "Поле ввода");
    toggle(ui, "Скрепка", &mut a.field_attach_button);
    toggle(ui, "Команды", &mut a.field_commands_button);
    toggle(ui, "Эмодзи", &mut a.field_emoji_button);
    toggle(ui, "Микрофон", &mut a.field_microphone_button);
    toggle(ui, "Автоудаление", &mut a.field_auto_delete_button);
    toggle(ui, "Подарок", &mut a.field_gift_button);
    toggle(ui, "ИИ-редактор", &mut a.field_ai_editor_button);
    toggle(ui, "Всплывающее меню скрепки", &mut a.show_attach_popup);
    toggle(ui, "Всплывающая панель эмодзи", &mut a.show_emoji_popup);
    ui.add_space(8.0);
}

/// Страница «Прочее». Возвращает `true`, если нажато «Сбросить настройки».
fn other_page(ui: &mut egui::Ui, logout_ask: &mut bool) -> bool {
    section(ui, "Прочее");
    let mut reset = false;
    if nav_row(ui, "menu/link", "Зарегистрировать tg:// ссылки", "") {
        register_url_scheme(ui.ctx());
    }
    if nav_row(ui, "menu/restore", "Сбросить настройки", "") {
        reset = true;
    }
    if nav_row(ui, "menu/leave", "Выйти из аккаунта", "") {
        *logout_ask = true;
    }
    desc(ui, "Отчёты о сбоях, проверка обновлений и любая телеметрия в Rustgramm отсутствуют: соединения идут только на серверы Telegram.");
    reset
}

/// Регистрирует Rustgramm обработчиком ссылок `tg://` для текущего пользователя.
fn register_url_scheme(ctx: &egui::Context) {
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        if let Ok(exe) = std::env::current_exe() {
            let exe = exe.to_string_lossy().into_owned();
            let run = |args: &[&str]| {
                let _ = std::process::Command::new("reg")
                    .args(args)
                    .creation_flags(0x0800_0000)
                    .status();
            };
            run(&["add", r"HKCU\Software\Classes\tg", "/ve", "/d", "URL:Telegram Link", "/f"]);
            run(&["add", r"HKCU\Software\Classes\tg", "/v", "URL Protocol", "/d", "", "/f"]);
            let cmd = format!("\"{exe}\" \"%1\"");
            run(&["add", r"HKCU\Software\Classes\tg\shell\open\command", "/ve", "/d", &cmd, "/f"]);
        }
    }
    ctx.request_repaint();
}

impl App {
    /// Окно «Новая группа» (название и участники из контактов) и «Новый канал» (название и описание).
    #[allow(deprecated)]
    pub(crate) fn create_window(&mut self, ctx: &egui::Context) {
        if self.create_kind == 0 {
            return;
        }
        let group = self.create_kind == 1;
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        let mut create = false;
        egui::Area::new(egui::Id::new("create-chat"))
            .order(egui::Order::Foreground)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .corner_radius(CornerRadius::same(10))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(255, 255, 255, 22)))
                    .inner_margin(egui::Margin::same(16))
                    .show(ui, |ui| {
                        ui.set_width(340.0);
                        ui.label(RichText::new(if group { "Новая группа" } else { "Новый канал" }).size(17.0).color(WHITE));
                        ui.add_space(10.0);
                        ui.add(
                            egui::TextEdit::singleline(&mut self.create_title)
                                .hint_text(if group { "Название группы" } else { "Название канала" })
                                .desired_width(f32::INFINITY),
                        );
                        if group {
                            ui.add_space(8.0);
                            ui.label(RichText::new("Участники").size(12.5).color(SUBTEXT));
                            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::ZERO;
                                if self.contacts.is_empty() {
                                    ui.label(RichText::new("Загрузка контактов…").color(SUBTEXT));
                                }
                                for c in &self.contacts {
                                    let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 34.0), Sense::click());
                                    if resp.hovered() {
                                        ui.painter().rect_filled(r, CornerRadius::same(6), t2(WINDOW_OVER));
                                    }
                                    let on = self.create_members.contains(&c.id);
                                    let box_r = Rect::from_center_size(Pos2::new(r.left() + 14.0, r.center().y), Vec2::splat(16.0));
                                    ui.painter().rect_stroke(box_r, CornerRadius::same(4), egui::Stroke::new(1.5, if on { WHITE } else { SUBTEXT }), egui::StrokeKind::Middle);
                                    if on {
                                        icons::tg(ui.painter(), "info/info_close", box_r.center(), WHITE);
                                    }
                                    ui.painter().text(Pos2::new(r.left() + 36.0, r.center().y), Align2::LEFT_CENTER, &c.name, FontId::proportional(14.0), WHITE);
                                    if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() && !self.create_members.remove(&c.id) {
                                        self.create_members.insert(c.id);
                                    }
                                }
                            });
                        } else {
                            ui.add_space(8.0);
                            ui.add(
                                egui::TextEdit::multiline(&mut self.create_about)
                                    .hint_text("Описание (необязательно)")
                                    .desired_rows(3)
                                    .desired_width(f32::INFINITY),
                            );
                        }
                        ui.add_space(12.0);
                        ui.horizontal(|ui| {
                            let ok = !self.create_title.trim().is_empty() && (!group || !self.create_members.is_empty());
                            if ui.add_enabled(ok, egui::Button::new("Создать")).clicked() {
                                create = true;
                            }
                            if ui.button("Отмена").clicked() {
                                close = true;
                            }
                        });
                    });
            });
        if create {
            let title = self.create_title.trim().to_owned();
            if group {
                let members: Vec<_> = self.contacts.iter().filter(|c| self.create_members.contains(&c.id)).map(|c| c.peer).collect();
                self.send(Cmd::CreateGroup(title, members));
            } else {
                self.send(Cmd::CreateChannel(title, self.create_about.trim().to_owned(), true));
            }
            close = true;
        }
        if close {
            self.create_kind = 0;
        }
    }
}
