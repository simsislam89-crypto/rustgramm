//! «Настройки» Telegram (левая панель → «Настройки»): страницы как в Telegram Desktop —
//! Мой аккаунт, Уведомления и звуки, Конфиденциальность, Данные и память, Папки с чатами,
//! Внешний вид, Язык, Устройства, Энергосбережение — и раздел «Rustgramm» со своими настройками
//! (прозрачность окна, скорость анимаций, масштаб, режим призрака, кэш). Данные страниц Telegram
//! приходят настоящими запросами (`Engine::account_overview`, только чтение); опасные действия
//! (смена номера, удаление аккаунта, завершение чужих сеансов) здесь намеренно отсутствуют.

use std::path::Path;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Vec2};
use rg_core::engine::AccountOverview;

use super::kit::{self, desc, divider, nav_row, section, slider, toggle};
use super::{App, Cmd, icons, theme::*, worker};

const BOX_W: f32 = 380.0;

pub(crate) mod pg {
    pub const MAIN: usize = 0;
    pub const ACCOUNT: usize = 1;
    pub const NOTIFY: usize = 2;
    pub const PRIVACY: usize = 3;
    pub const DATA: usize = 4;
    pub const FOLDERS: usize = 5;
    pub const APPEARANCE: usize = 6;
    pub const LANGUAGE: usize = 7;
    pub const DEVICES: usize = 8;
    pub const POWER: usize = 9;
    pub const RUSTGRAMM: usize = 10;
}

fn title_of(p: usize) -> &'static str {
    match p {
        pg::ACCOUNT => "Мой аккаунт",
        pg::NOTIFY => "Уведомления и звуки",
        pg::PRIVACY => "Конфиденциальность",
        pg::DATA => "Данные и память",
        pg::FOLDERS => "Папки с чатами",
        pg::APPEARANCE => "Внешний вид",
        pg::LANGUAGE => "Язык",
        pg::DEVICES => "Устройства",
        pg::POWER => "Энергосбережение",
        pg::RUSTGRAMM => "Rustgramm",
        _ => "Настройки",
    }
}

/// Состояние окна «Настройки».
#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    pub page: usize,
    pub account: Option<AccountOverview>,
    /// Поля редактирования профиля (имя, фамилия, о себе) и признак, что они заполнены из аккаунта.
    pub first: String,
    pub last: String,
    pub about: String,
    pub filled: bool,
    /// Размеры кэша и загрузок (байт), считаются при открытии страницы «Данные и память».
    pub cache_bytes: u64,
    pub downloads_bytes: u64,
    /// Редактор папки (None — список папок, Some — создание/правка).
    pub edit_folder: Option<rg_core::engine::FolderRow>,
    pub edit_folder_is_new: bool,
}

/// Размер каталога со всем содержимым (байт).
fn dir_size(p: &Path) -> u64 {
    let Ok(rd) = std::fs::read_dir(p) else { return 0 };
    rd.flatten()
        .map(|e| match e.metadata() {
            Ok(m) if m.is_dir() => dir_size(&e.path()),
            Ok(m) => m.len(),
            Err(_) => 0,
        })
        .sum()
}

/// Чистит кэш медиа и предпросмотров; фото профилей остаются (мелкие, быстро нужны при старте).
fn clear_cache(cache: &Path) -> u64 {
    let before = dir_size(cache);
    let _ = std::fs::remove_dir_all(cache.join("media"));
    if let Ok(rd) = std::fs::read_dir(cache) {
        for e in rd.flatten() {
            if e.metadata().is_ok_and(|m| m.is_file()) {
                let _ = std::fs::remove_file(e.path());
            }
        }
    }
    before.saturating_sub(dir_size(cache))
}

fn size_text(b: u64) -> String {
    rg_core::media::human_size(b)
}

/// Строка «название — значение» без нажатия.
fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), kit::ROW_H), Sense::hover());
    let p = ui.painter();
    p.text(Pos2::new(rect.left() + kit::PAD, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), WHITE);
    p.text(Pos2::new(rect.right() - kit::PAD, rect.center().y), Align2::RIGHT_CENTER, value, FontId::proportional(14.0), SUBTEXT);
}

/// Строка с двумя линиями: заголовок и подпись под ним.
fn two_lines(ui: &mut egui::Ui, title: &str, sub: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
    let p = ui.painter();
    p.text(Pos2::new(rect.left() + kit::PAD, rect.center().y - 9.0), Align2::LEFT_CENTER, title, FontId::proportional(14.0), WHITE);
    p.text(Pos2::new(rect.left() + kit::PAD, rect.center().y + 10.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), SUBTEXT);
}

impl App {
    pub(crate) fn open_tg_settings(&mut self) {
        self.tgs.open = true;
        self.tgs.page = pg::MAIN;
        self.tgs.filled = false;
        self.send(Cmd::Account);
    }

    /// Окно «Настройки» Telegram: шапка со стрелкой «назад» и крестиком, страницы.
    #[allow(deprecated)]
    pub(crate) fn tg_settings_window(&mut self, ctx: &egui::Context) {
        let raw = tween(ctx, "tgs", f32::from(u8::from(self.tgs.open)), 0.2);
        if raw < 0.005 && !self.tgs.open {
            return;
        }
        let appear = ios_spring(raw);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("tgs-dim"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                paint_frosted_scrim(ui.painter(), rect, raw);
                if resp.clicked() {
                    self.tgs.open = false;
                }
            });
        let mut s = self.settings.clone();
        let mut page = self.tgs.page;
        let mut close = false;
        let mut advanced = false;
        let mut save_profile = false;
        let mut save_folder: Option<rg_core::engine::FolderRow> = None;
        let mut delete_folder: Option<i32> = None;
        let mut clear = false;
        let mut open_downloads = false;
        let max_h = (screen.height() - 40.0).clamp(280.0, 720.0);
        // Клик по затемнению мог закрыть окно выше по коду.
        let was_open = self.tgs.open;
        let mut st = std::mem::take(&mut self.tgs);
        let me_name = self.me.clone();
        let me_tex = self.dialogs.iter().find(|d| d.saved).and_then(|d| self.avatars.get(&d.id));
        let corners = self.settings.ayu.avatar_corners;
        let streamer = self.settings.ayu.streamer_mode;
        let folders = &self.folders;
        let dialogs = &self.dialogs;
        egui::Area::new(egui::Id::new("tgs-box"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, (1.0 - appear) * 20.0))
            .show(ctx, |ui| {
                ui.set_opacity(raw.min(1.0).powf(0.55));
                let card = egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha((26.0 * opacity_f32()) as u8)))
                    .corner_radius(CornerRadius::same(14))
                    .show(ui, |ui| {
                    let width = BOX_W.min(screen.width() - 16.0);
                    ui.set_width(width);
                    ui.spacing_mut().item_spacing = Vec2::ZERO;
                    let (head, _) = ui.allocate_exact_size(Vec2::new(width, 50.0), Sense::hover());
                    let title = title_of(page);
                    if page != pg::MAIN {
                        let back = Rect::from_center_size(Pos2::new(head.left() + 26.0, head.center().y), Vec2::splat(34.0));
                        let r = ui.interact(back, ui.make_persistent_id("tgs-back"), Sense::click());
                        icons::back(ui.painter(), back.center(), if r.hovered() { WHITE } else { SUBTEXT });
                        if r.clicked() {
                            if page == pg::FOLDERS && st.edit_folder.is_some() {
                                st.edit_folder = None;
                            } else {
                                page = pg::MAIN;
                            }
                        }
                    }
                    ui.painter().text(
                        Pos2::new(head.left() + if page == pg::MAIN { 20.0 } else { 52.0 }, head.center().y),
                        Align2::LEFT_CENTER,
                        title,
                        FontId::proportional(16.0),
                        WHITE,
                    );
                    let x = Rect::from_center_size(Pos2::new(head.right() - 24.0, head.center().y), Vec2::splat(34.0));
                    let xr = ui.interact(x, ui.make_persistent_id("tgs-close"), Sense::click());
                    icons::tg(ui.painter(), "info/info_close", x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
                    if xr.clicked() {
                        close = true;
                    }
                    egui::ScrollArea::vertical()
                        .max_height(max_h - 50.0)
                        .min_scrolled_height(max_h - 50.0)
                        .auto_shrink([false, true])
                        .show(ui, |ui| {
                            ui.spacing_mut().item_spacing = Vec2::ZERO;
                            match page {
                                pg::ACCOUNT => account_page(ui, &mut st, &mut save_profile, streamer),
                                pg::NOTIFY => notify_page(ui, &st),
                                pg::PRIVACY => privacy_page(ui, &st),
                                pg::DATA => data_page(ui, &st, &mut clear, &mut open_downloads),
                                pg::FOLDERS => folders_page(ui, &mut st, folders, dialogs, &mut save_folder, &mut delete_folder),
                                pg::APPEARANCE => appearance_page(ui, &mut s),
                                pg::LANGUAGE => language_page(ui),
                                pg::DEVICES => devices_page(ui, &st, self.now),
                                pg::POWER => power_page(ui, &mut s),
                                pg::RUSTGRAMM => rustgramm_page(ui, &mut s, &mut clear, self.emoji.is_some()),
                                _ => {
                                    if let Some(p) = main_page(ui, &st, &me_name, me_tex, corners, streamer, &mut advanced) {
                                        page = p;
                                    }
                                }
                            }
                        });
                });
                paint_glass_rim(ui.painter(), card.response.rect, 14.0);
            });
        // При входе на страницу «Данные и память» и «Rustgramm» считаем размер кэша.
        if (page == pg::DATA || page == pg::RUSTGRAMM) && st.page != page {
            st.cache_bytes = dir_size(&worker::home().join("cache"));
            st.downloads_bytes = dir_size(&worker::home().join("downloads"));
        }
        st.page = page;
        if clear {
            let freed = clear_cache(&worker::home().join("cache"));
            st.cache_bytes = dir_size(&worker::home().join("cache"));
            self.cached_media.clear();
            self.status = format!("Кэш очищен: освобождено {}", size_text(freed));
        }
        if open_downloads {
            let dir = worker::home().join("downloads");
            let _ = std::fs::create_dir_all(&dir);
            rg_core::media::open_downloaded(&dir);
        }
        if save_profile {
            self.send(Cmd::UpdateProfile(st.first.trim().to_owned(), st.last.trim().to_owned(), st.about.trim().to_owned()));
            st.filled = false;
        }
        if let Some(f) = save_folder {
            self.send(Cmd::SaveFolder(f));
            st.edit_folder = None;
        }
        if let Some(id) = delete_folder {
            self.send(Cmd::DeleteFolder(id));
            st.edit_folder = None;
        }
        self.tgs = st;
        self.tgs.open = was_open && !(close || ctx.input(|i| i.key_pressed(egui::Key::Escape)));
        if advanced {
            self.tgs.open = false;
            self.settings_open = true;
            self.settings_tab = super::settings_ui::page::MENU;
        }
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(Box::new(s)));
        }
    }
}

fn main_page(
    ui: &mut egui::Ui,
    st: &State,
    me: &str,
    tex: Option<&egui::TextureHandle>,
    corners: i32,
    streamer: bool,
    advanced: &mut bool,
) -> Option<usize> {
    let w = ui.available_width();
    let (head, _) = ui.allocate_exact_size(Vec2::new(w, 96.0), Sense::hover());
    let (name, phone, user) = match &st.account {
        Some(a) => (
            if a.me.name.is_empty() { me.to_owned() } else { a.me.name.clone() },
            a.me.phone.clone(),
            a.me.username.clone(),
        ),
        None => (me.to_owned(), String::new(), String::new()),
    };
    draw_avatar(ui, Pos2::new(head.left() + 50.0, head.center().y), &name, 1, 56.0, tex, corners);
    let p = ui.painter();
    p.text(Pos2::new(head.left() + 92.0, head.center().y - 14.0), Align2::LEFT_CENTER, &name, FontId::proportional(16.0), WHITE);
    let line = [
        (!phone.is_empty()).then(|| mask_streamer(&format!("+{phone}"), streamer)),
        (!user.is_empty()).then(|| mask_streamer(&format!("@{user}"), streamer)),
    ]
    .into_iter()
    .flatten()
    .collect::<Vec<_>>()
    .join(" · ");
    p.text(
        Pos2::new(head.left() + 92.0, head.center().y + 8.0),
        Align2::LEFT_CENTER,
        if line.is_empty() { "загрузка…" } else { &line },
        FontId::proportional(13.0),
        SUBTEXT,
    );
    divider(ui);
    let mut go = None;
    for (icon, name, p) in [
        ("settings/user", "Мой аккаунт", pg::ACCOUNT),
        ("settings/notifications", "Уведомления и звуки", pg::NOTIFY),
        ("settings/lock", "Конфиденциальность", pg::PRIVACY),
        ("menu/storage", "Данные и память", pg::DATA),
        ("settings/folders", "Папки с чатами", pg::FOLDERS),
        ("settings/palette", "Внешний вид", pg::APPEARANCE),
        ("settings/language", "Язык", pg::LANGUAGE),
        ("settings/laptop", "Устройства", pg::DEVICES),
        ("settings/battery", "Энергосбережение", pg::POWER),
    ] {
        if nav_row(ui, icon, name, "") {
            go = Some(p);
        }
    }
    divider(ui);
    if nav_row(ui, "heart", "Rustgramm", "") {
        go = Some(pg::RUSTGRAMM);
    }
    if nav_row(ui, "menu/customize", "Расширенные настройки Rustgramm", "") {
        *advanced = true;
    }
    desc(ui, &format!("Rustgramm v{} — клиент Telegram на Rust. Данные уходят только на серверы Telegram.", env!("CARGO_PKG_VERSION")));
    go
}

fn loading(ui: &mut egui::Ui, st: &State) -> bool {
    if st.account.is_some() {
        return false;
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 80.0), Sense::hover());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Загрузка…", FontId::proportional(14.0), SUBTEXT);
    true
}

fn account_page(ui: &mut egui::Ui, st: &mut State, save: &mut bool, streamer: bool) {
    if loading(ui, st) {
        return;
    }
    if !st.filled && let Some(a) = &st.account {
        let (f, l) = a.me.name.split_once(' ').map_or((a.me.name.as_str(), ""), |(f, l)| (f, l));
        st.first = f.to_owned();
        st.last = l.to_owned();
        st.about = a.me.about.clone();
        st.filled = true;
    }
    let a = st.account.clone().unwrap_or_default();
    section(ui, "Профиль");
    for (label, text) in [("Имя", &mut st.first), ("Фамилия", &mut st.last)] {
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.allocate_ui_with_layout(
                Vec2::new(84.0, 30.0),
                egui::Layout::left_to_right(egui::Align::Center),
                |ui| {
                    ui.label(RichText::new(label).color(SUBTEXT).size(13.0));
                },
            );
            let fw = (ui.available_width() - kit::PAD).max(120.0);
            ui.add_sized([fw, 30.0], egui::TextEdit::singleline(text).margin(Vec2::new(8.0, 6.0)));
        });
        ui.add_space(4.0);
    }
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        ui.allocate_ui_with_layout(
            Vec2::new(84.0, 30.0),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.label(RichText::new("О себе").color(SUBTEXT).size(13.0));
            },
        );
        let fw = (ui.available_width() - kit::PAD).max(120.0);
        ui.add_sized(
            [fw, 60.0],
            egui::TextEdit::multiline(&mut st.about).margin(Vec2::new(8.0, 6.0)).desired_rows(3),
        );
    });
    ui.add_space(8.0);
    let changed = {
        let (f, l) = a.me.name.split_once(' ').map_or((a.me.name.as_str(), ""), |(f, l)| (f, l));
        st.first.trim() != f || st.last.trim() != l || st.about.trim() != a.me.about
    };
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        if orange_button_enabled(ui, changed && !st.first.trim().is_empty(), "Сохранить", Vec2::new(140.0, 34.0)).clicked() {
            *save = true;
        }
    });
    ui.add_space(8.0);
    divider(ui);
    section(ui, "Данные аккаунта");
    info_row(ui, "Телефон", &if a.me.phone.is_empty() { "—".to_owned() } else { mask_streamer(&format!("+{}", a.me.phone), streamer) });
    info_row(ui, "Имя пользователя", &if a.me.username.is_empty() { "—".to_owned() } else { mask_streamer(&format!("@{}", a.me.username), streamer) });
    desc(ui, "Смена номера, имени пользователя и удаление аккаунта здесь не предусмотрены: делайте это в официальном клиенте Telegram.");
}

fn notify_page(ui: &mut egui::Ui, st: &State) {
    if loading(ui, st) {
        return;
    }
    let a = st.account.as_ref().expect("проверено выше");
    section(ui, "Уведомления о сообщениях");
    for n in &a.notify {
        two_lines(ui, n.title, &format!("{} · текст сообщения {}", if n.enabled { "Включены" } else { "Выключены" }, if n.preview { "показывается" } else { "скрыт" }));
    }
    if a.notify.is_empty() {
        info_row(ui, "Нет данных", "");
    }
    desc(ui, "Значения хранятся в Telegram. Звук отдельного чата меняется в его меню («Без звука»). Уведомления Windows и звуки Rustgramm — в разработке.");
}

fn privacy_page(ui: &mut egui::Ui, st: &State) {
    if loading(ui, st) {
        return;
    }
    let a = st.account.as_ref().expect("проверено выше");
    section(ui, "Кто видит");
    for r in &a.privacy {
        info_row(ui, r.title, &r.rule);
    }
    desc(ui, "Это текущие правила вашего аккаунта. Изменить их можно в официальном клиенте Telegram — Rustgramm ничего не меняет в приватности сам.");
    divider(ui);
    section(ui, &format!("Заблокированные пользователи ({})", a.blocked.len()));
    if a.blocked.is_empty() {
        info_row(ui, "Список пуст", "");
    }
    for n in a.blocked.iter().take(30) {
        info_row(ui, n, "");
    }
}

fn data_page(ui: &mut egui::Ui, st: &State, clear: &mut bool, open_dl: &mut bool) {
    section(ui, "Память на этом устройстве");
    info_row(ui, "Кэш медиа", &size_text(st.cache_bytes));
    if nav_row(ui, "menu/clear", "Очистить кэш", "") {
        *clear = true;
    }
    desc(ui, "Удаляются скачанные для просмотра ролики и предпросмотры. Фото профилей и сообщения остаются.");
    divider(ui);
    section(ui, "Загрузки");
    info_row(ui, "Сохранённые файлы", &size_text(st.downloads_bytes));
    if nav_row(ui, "menu/show_in_folder", "Открыть папку загрузок", "") {
        *open_dl = true;
    }
}

fn folders_page(
    ui: &mut egui::Ui,
    st: &mut State,
    folders: &[rg_core::engine::FolderRow],
    dialogs: &[rg_core::engine::DialogRow],
    save_folder: &mut Option<rg_core::engine::FolderRow>,
    delete_folder: &mut Option<i32>,
) {
    if let Some(f) = st.edit_folder.as_mut() {
        section(ui, if st.edit_folder_is_new { "Новая папка" } else { "Изменить папку" });
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.add_sized(
                [ui.available_width() - kit::PAD, 32.0],
                egui::TextEdit::singleline(&mut f.title)
                    .hint_text("Название папки (до 12 символов)")
                    .margin(Vec2::new(8.0, 6.0)),
            );
        });
        ui.add_space(8.0);
        divider(ui);
        section(ui, "Включить типы чатов");
        toggle(ui, "Контакты", &mut f.contacts);
        toggle(ui, "Не контакты", &mut f.non_contacts);
        toggle(ui, "Группы", &mut f.groups);
        toggle(ui, "Каналы", &mut f.broadcasts);
        toggle(ui, "Боты", &mut f.bots);
        divider(ui);
        section(ui, "Исключить");
        toggle(ui, "Без звука", &mut f.exclude_muted);
        toggle(ui, "Прочитанные", &mut f.exclude_read);
        toggle(ui, "Архивированные", &mut f.exclude_archived);
        ui.add_space(10.0);
        let has_any = f.contacts || f.non_contacts || f.groups || f.broadcasts || f.bots || !f.include.is_empty();
        let valid = !f.title.trim().is_empty() && has_any;
        let is_new = st.edit_folder_is_new;
        let id = f.id;
        let cloned = f.clone();
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            if orange_button_enabled(ui, valid, "Сохранить", Vec2::new(130.0, 34.0)).clicked() {
                let mut clean = cloned;
                clean.title = clean.title.trim().chars().take(12).collect();
                *save_folder = Some(clean);
            }
            ui.add_space(8.0);
            if !is_new && ui.button(RichText::new("Удалить").color(DANGER)).clicked() {
                *delete_folder = Some(id);
            }
        });
        ui.add_space(10.0);
        return;
    }
    section(ui, "Мои папки");
    if folders.is_empty() {
        info_row(ui, "Папок пока нет", "");
    }
    for f in folders {
        let n = dialogs.iter().filter(|d| rg_core::engine::folder_matches(f, d)).count();
        if nav_row(ui, "settings/folders", &f.title, &format!("{n}")) {
            st.edit_folder = Some(f.clone());
            st.edit_folder_is_new = false;
        }
    }
    ui.add_space(10.0);
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        if orange_button(ui, "Создать папку", Vec2::new(160.0, 34.0)).clicked() {
            let next_id = (2..=255).find(|id| !folders.iter().any(|f| f.id == *id)).unwrap_or(2);
            st.edit_folder = Some(rg_core::engine::FolderRow {
                id: next_id,
                title: String::new(),
                contacts: true,
                non_contacts: false,
                groups: false,
                broadcasts: false,
                bots: false,
                exclude_muted: false,
                exclude_read: false,
                exclude_archived: true,
                include: Vec::new(),
                exclude: Vec::new(),
            });
            st.edit_folder_is_new = true;
        }
    });
    ui.add_space(10.0);
    desc(ui, "Папки синхронизируются с серверами Telegram. Нажмите на папку, чтобы изменить её правила или удалить.");
}

fn appearance_page(ui: &mut egui::Ui, s: &mut rg_settings::Settings) {
    section(ui, "Прозрачность и стекло");
    let mut o = i32::from(s.window_opacity);
    let shown = format!("{o}%");
    if slider(ui, "Прозрачность окна и кнопок", &mut o, (15, 100), shown) {
        s.window_opacity = o.clamp(15, 100) as u8;
    }
    toggle(ui, "Размытие стекла (блюр)", &mut s.blur_enabled);
    desc(ui, "На старте окно непрозрачное (100%). Снизьте прозрачность — фон окна и кнопок станет стеклянным с размытием как на iPhone.");
    divider(ui);
    section(ui, "Фон чата");
    let styles = [(0u8, "Сплошной тёмный"), (1, "Мягкий градиент"), (2, "Узор Telegram")];
    for (id, label) in styles {
        let mark = if s.chat_bg_style == id { "✓" } else { "" };
        if nav_row(ui, "settings/palette", label, mark) {
            s.chat_bg_style = id;
        }
    }
    divider(ui);
    section(ui, "Масштаб интерфейса");
    let mut v = i32::from(s.ui_scale);
    let shown = format!("{v}%");
    if slider(ui, "Масштаб", &mut v, (80, 150), shown) {
        s.ui_scale = v.clamp(80, 150) as u8;
    }
    desc(ui, "Применяется сразу. По умолчанию 100%.");
}

fn language_page(ui: &mut egui::Ui) {
    section(ui, "Язык интерфейса");
    info_row(ui, "Русский", "✓");
    desc(ui, "Интерфейс Rustgramm написан на русском. Языковые пакеты Telegram не загружаются: приложение обращается только к серверам Telegram.");
}

fn devices_page(ui: &mut egui::Ui, st: &State, now: i64) {
    if loading(ui, st) {
        return;
    }
    let a = st.account.as_ref().expect("проверено выше");
    if let Some(cur) = a.sessions.iter().find(|x| x.current) {
        section(ui, "Это устройство");
        two_lines(ui, &format!("{} · {}", cur.app.trim(), cur.device), &format!("{} · {}", cur.platform.trim(), cur.place));
        divider(ui);
    }
    section(ui, "Активные сеансы");
    let mut any = false;
    for x in a.sessions.iter().filter(|x| !x.current) {
        any = true;
        let ago = now - x.active;
        let when = if ago < 300 { "в сети".to_owned() } else if ago < 86_400 { "сегодня".to_owned() } else { format!("{} дн. назад", ago / 86_400) };
        two_lines(ui, &format!("{} · {}", x.app.trim(), x.device), &[x.place.as_str(), x.ip.as_str(), when.as_str()].iter().filter(|p| !p.is_empty()).copied().collect::<Vec<_>>().join(" · "));
    }
    if !any {
        info_row(ui, "Других сеансов нет", "");
    }
    desc(ui, &format!("Сеансы без активности завершаются через {} дн. Завершить чужие сеансы можно только в официальном клиенте — Rustgramm этого не делает.", a.session_ttl_days.max(1)));
}

fn power_page(ui: &mut egui::Ui, s: &mut rg_settings::Settings) {
    section(ui, "Анимации");
    let mut on = s.anim_speed > 0;
    if toggle(ui, "Плавные анимации", &mut on) {
        s.anim_speed = if on { 100 } else { 0 };
    }
    desc(ui, "Без анимаций окна и меню появляются мгновенно, процессор в простое не просыпается. Тонкая настройка скорости — в разделе «Rustgramm».");
}

fn rustgramm_page(ui: &mut egui::Ui, s: &mut rg_settings::Settings, clear: &mut bool, atlas: bool) {
    section(ui, "Окно и стекло");
    let mut o = i32::from(s.window_opacity);
    let shown = format!("{o}%");
    if slider(ui, "Прозрачность окна и кнопок", &mut o, (15, 100), shown) {
        s.window_opacity = o.clamp(15, 100) as u8;
    }
    toggle(ui, "Размытие стекла (блюр)", &mut s.blur_enabled);
    toggle(ui, "Автозапуск GIF в ленте", &mut s.auto_play_gifs);
    desc(ui, "По умолчанию окно непрозрачное (100%). При снижении процента фон окна и кнопок плавно становится прозрачнее с размытием.");
    let mut a = i32::from(s.anim_speed);
    let shown = if a == 0 { "выкл.".to_owned() } else { format!("{a}%") };
    if slider(ui, "Скорость анимаций", &mut a, (0, 200), shown) {
        s.anim_speed = a.clamp(0, 200) as u8;
    }
    let mut z = i32::from(s.ui_scale);
    let shown = format!("{z}%");
    if slider(ui, "Масштаб интерфейса", &mut z, (80, 150), shown) {
        s.ui_scale = z.clamp(80, 150) as u8;
    }
    divider(ui);
    section(ui, "Приватность");
    let mut ghost = s.ghost.active;
    if toggle(ui, "Режим призрака", &mut ghost) {
        s.ghost.set_active(ghost);
    }
    desc(ui, "Не отправляет «прочитано», «в сети» и прогресс загрузки. Подробные параметры — в «Расширенных настройках».");
    divider(ui);
    section(ui, "Оформление и данные");
    info_row(ui, "Тема", "Тёмная · оранжевые кнопки");
    let atlas_path = worker::home().join("emoji-apple.png");
    info_row(ui, "Эмодзи", if atlas && atlas_path.exists() { "Apple (локально)" } else { "Twemoji" });
    desc(ui, &format!("Атлас Apple-эмодзи берётся из файла {} (в программу не вшит: права на эти картинки не наши).", atlas_path.display()));
    if nav_row(ui, "menu/clear", "Очистить кэш", "") {
        *clear = true;
    }
    divider(ui);
    info_row(ui, "Версия", env!("CARGO_PKG_VERSION"));
}
