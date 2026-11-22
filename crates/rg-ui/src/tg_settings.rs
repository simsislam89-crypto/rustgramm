//! «Настройки» Telegram и Rustgramm — 1 в 1 по структуре и функциям с AyuGram Desktop:
//! поиск по настройкам, QR профиля, Мой аккаунт, Уведомления и звуки (встроенные и Windows,
//! превью, выбор угла экрана и числа карточек, мигание панели задач, звук, серверные категории),
//! Конфиденциальность (все 12 правил через `account.setPrivacy`, локальный код-пароль,
//! автоудаление, заблокированные с разблокировкой, TTL аккаунта), Настройки чатов,
//! Папки, Продвинутые настройки (данные, кэш, прокси), Энергосбережение, Язык и переводчик,
//! Устройства (завершение отдельных и всех остальных сеансов с подтверждением).

use std::path::Path;

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rg_core::engine::{AccountOverview, NotifyScope, PrivacyKeyKind, PrivacyLevel};

use super::kit::{self, desc, divider, nav_row, section, slider, toggle, value_row};
use super::{App, Cmd, icons, theme::*, worker};

const BOX_W: f32 = 396.0;

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
    pub const PLUGINS: usize = 11;
}

fn title_of(p: usize) -> &'static str {
    match p {
        pg::ACCOUNT => "Мой аккаунт",
        pg::NOTIFY => "Уведомления и звуки",
        pg::PRIVACY => "Конфиденциальность",
        pg::DATA => "Продвинутые настройки",
        pg::FOLDERS => "Папки с чатами",
        pg::APPEARANCE => "Настройки чатов",
        pg::LANGUAGE => "Язык",
        pg::DEVICES => "Активные сеансы",
        pg::POWER => "Энергосбережение",
        pg::RUSTGRAMM => "Rustgramm",
        pg::PLUGINS => "Плагины и телефон",
        _ => "Настройки",
    }
}

/// Языки для выбора в «Язык» и «Не переводить».
const LANG_CHOICES: &[(&str, &str, &str)] = &[
    ("ru", "Русский", "Russian"),
    ("en", "English", "Английский"),
    ("uk", "Українська", "Украинский"),
    ("de", "Deutsch", "Немецкий"),
    ("fr", "Français", "Французский"),
    ("es", "Español", "Испанский"),
    ("it", "Italiano", "Итальянский"),
    ("pt", "Português", "Португальский"),
    ("tr", "Türkçe", "Турецкий"),
    ("zh", "中文", "Китайский"),
    ("ja", "日本語", "Японский"),
    ("ko", "한국어", "Корейский"),
];

fn lang_name(code: &str) -> &'static str {
    LANG_CHOICES
        .iter()
        .find(|(c, _, _)| *c == code)
        .map_or("Русский", |(_, n, _)| *n)
}

/// Состояние окна «Настройки».
#[derive(Default)]
pub(crate) struct State {
    pub open: bool,
    pub page: usize,
    pub account: Option<AccountOverview>,
    /// Поиск по настройкам на главной странице.
    pub search_open: bool,
    pub search: String,
    /// Показ QR-кода своего профиля.
    pub qr_open: bool,
    /// Поля редактирования профиля (имя, фамилия, о себе, @username).
    pub first: String,
    pub last: String,
    pub about: String,
    pub username: String,
    pub filled: bool,
    /// Размеры кэша и загрузок (байт).
    pub cache_bytes: u64,
    pub downloads_bytes: u64,
    /// Редактор папки (None — список папок, Some — создание/правка).
    pub edit_folder: Option<rg_core::engine::FolderRow>,
    pub edit_folder_is_new: bool,
    /// Подэкраны внутри «Конфиденциальности».
    pub privacy_edit: Option<PrivacyKeyKind>,
    pub privacy_choice: Option<PrivacyLevel>,
    pub blocked_open: bool,
    pub passcode_open: bool,
    pub passcode_input: String,
    pub passcode_confirm: String,
    pub passcode_err: String,
    pub auto_delete_open: bool,
    pub account_ttl_open: bool,
    pub confirm_clear_pay: bool,
    /// Подэкраны внутри «Языка».
    pub dnt_open: bool,
    pub lang_search: String,
    /// Подтверждение завершения сеанса (`Some(0)` — все остальные, `Some(hash)` — один).
    pub confirm_kill_session: Option<i64>,
    pub session_ttl_open: bool,
    /// Момент открытия окна и смены страницы (для гарантированной 60fps iOS-анимации).
    pub opened_at: Option<std::time::Instant>,
    pub page_at: Option<std::time::Instant>,
    /// История переходов по страницам настроек (для боковых кнопок мыши Назад / Вперёд).
    pub page_back: Vec<usize>,
    pub page_fwd: Vec<usize>,
    /// Редактор нового пользовательского `.plugin` файла.
    pub new_plugin_open: bool,
    pub new_plugin_code: String,
    pub new_plugin_err: String,
    /// Отдельное модальное окно «Мой профиль» (как в AyuGram, без переброса в «Избранное»).
    pub profile_open: bool,
    pub profile_at: Option<std::time::Instant>,
}

/// Действия, собранные за кадр в окне настроек и выполняемые после отрисовки.
#[derive(Default)]
struct Actions {
    close: bool,
    advanced: bool,
    open_proxy: bool,
    logout: bool,
    lock_now: bool,
    test_notify: bool,
    save_profile: bool,
    save_username: bool,
    save_folder: Option<rg_core::engine::FolderRow>,
    delete_folder: Option<i32>,
    clear_cache: bool,
    open_downloads: bool,
    set_privacy: Option<(PrivacyKeyKind, PrivacyLevel)>,
    set_notify: Option<(NotifyScope, bool, bool)>,
    set_signup_notify: Option<bool>,
    set_history_ttl: Option<i32>,
    set_account_ttl: Option<i32>,
    kill_session: Option<i64>,
    kill_other_sessions: bool,
    set_session_ttl: Option<i32>,
    unblock: Option<(i64, i64)>,
    clear_payments: bool,
}

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

fn clear_cache(cache: &Path) -> u64 {
    let before = dir_size(cache);
    for sub in ["media", "audio", "history"] {
        let _ = std::fs::remove_dir_all(cache.join(sub));
    }
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

fn history_ttl_label(secs: i32) -> &'static str {
    match secs {
        0 => "Выкл",
        86_400 => "1 день",
        604_800 => "1 неделя",
        2_592_000 | 2_678_400 => "1 месяц",
        _ => "Вкл",
    }
}

fn days_months_label(days: i32) -> String {
    if days <= 0 {
        return "6 мес.".to_owned();
    }
    if days < 28 {
        format!("{days} дн.")
    } else {
        format!("{} мес.", (days + 15) / 30)
    }
}

fn info_row(ui: &mut egui::Ui, label: &str, value: &str) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), kit::ROW_H), Sense::hover());
    let p = ui.painter();
    p.text(Pos2::new(rect.left() + kit::PAD, rect.center().y), Align2::LEFT_CENTER, label, FontId::proportional(14.0), WHITE);
    p.text(Pos2::new(rect.right() - kit::PAD, rect.center().y), Align2::RIGHT_CENTER, value, FontId::proportional(14.0), SUBTEXT);
}

fn radio_row(ui: &mut egui::Ui, title: &str, sub: &str, selected: bool) -> bool {
    let h = if sub.is_empty() { 42.0 } else { 52.0 };
    let (rect, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 0.0, t(WINDOW_OVER));
    }
    let p = ui.painter();
    let c = Pos2::new(rect.left() + kit::PAD + 10.0, rect.center().y);
    p.circle_stroke(c, 9.0, Stroke::new(1.8, if selected { ORANGE } else { SUBTEXT }));
    if selected {
        p.circle_filled(c, 4.8, ORANGE);
    }
    let x = rect.left() + kit::PAD + 34.0;
    if sub.is_empty() {
        p.text(Pos2::new(x, rect.center().y), Align2::LEFT_CENTER, title, FontId::proportional(14.0), WHITE);
    } else {
        p.text(Pos2::new(x, rect.center().y - 9.0), Align2::LEFT_CENTER, title, FontId::proportional(14.0), WHITE);
        p.text(Pos2::new(x, rect.center().y + 9.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), SUBTEXT);
    }
    resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
}

fn has_subview(st: &State, page: usize) -> bool {
    st.qr_open
        || (page == pg::FOLDERS && st.edit_folder.is_some())
        || (page == pg::PRIVACY
            && (st.privacy_edit.is_some()
                || st.blocked_open
                || st.passcode_open
                || st.auto_delete_open
                || st.account_ttl_open))
        || (page == pg::LANGUAGE && st.dnt_open)
        || (page == pg::DEVICES && st.session_ttl_open)
        || (page == pg::PLUGINS && st.new_plugin_open)
}

fn pop_subview(st: &mut State, page: &mut usize) {
    if st.qr_open {
        st.qr_open = false;
    } else if *page == pg::FOLDERS && st.edit_folder.is_some() {
        st.edit_folder = None;
    } else if *page == pg::PRIVACY && st.privacy_edit.is_some() {
        st.privacy_edit = None;
        st.privacy_choice = None;
    } else if *page == pg::PRIVACY && st.blocked_open {
        st.blocked_open = false;
    } else if *page == pg::PRIVACY && st.passcode_open {
        st.passcode_open = false;
        st.passcode_input.clear();
        st.passcode_confirm.clear();
        st.passcode_err.clear();
    } else if *page == pg::PRIVACY && st.auto_delete_open {
        st.auto_delete_open = false;
    } else if *page == pg::PRIVACY && st.account_ttl_open {
        st.account_ttl_open = false;
    } else if *page == pg::LANGUAGE && st.dnt_open {
        st.dnt_open = false;
    } else if *page == pg::DEVICES && st.session_ttl_open {
        st.session_ttl_open = false;
    } else if *page == pg::PLUGINS && st.new_plugin_open {
        st.new_plugin_open = false;
        st.new_plugin_err.clear();
    } else {
        *page = pg::MAIN;
    }
}

impl App {
    pub(crate) fn open_tg_settings(&mut self) {
        self.tgs.open = true;
        self.tgs.opened_at = Some(std::time::Instant::now());
        self.tgs.page_at = Some(std::time::Instant::now());
        self.tgs.page = pg::MAIN;
        self.tgs.page_back.clear();
        self.tgs.page_fwd.clear();
        self.tgs.filled = false;
        self.tgs.qr_open = false;
        self.tgs.search_open = false;
        self.tgs.search.clear();
        self.tgs.privacy_edit = None;
        self.tgs.blocked_open = false;
        self.tgs.passcode_open = false;
        self.tgs.auto_delete_open = false;
        self.tgs.account_ttl_open = false;
        self.tgs.dnt_open = false;
        self.tgs.session_ttl_open = false;
        self.tgs.confirm_kill_session = None;
        self.tgs.confirm_clear_pay = false;
        self.tgs.new_plugin_open = false;
        self.send(Cmd::Account);
    }

    /// Открывает настоящее модальное окно «Мой профиль» (как в AyuGram, не переключая чат на «Избранное»).
    pub(crate) fn open_my_profile(&mut self) {
        self.tgs.profile_open = true;
        self.tgs.profile_at = Some(std::time::Instant::now());
        self.tgs.qr_open = false;
        self.send(Cmd::Account);
    }

    #[allow(deprecated)]
    pub(crate) fn tg_settings_window(&mut self, ctx: &egui::Context) {
        self.my_profile_window(ctx);
        let tw = tween(ctx, "tgs", f32::from(u8::from(self.tgs.open)), 0.24);
        let elapsed_k = self
            .tgs
            .opened_at
            .map_or(1.0, |t| (t.elapsed().as_secs_f32() / 0.24).clamp(0.0, 1.0));
        if elapsed_k < 1.0 && self.tgs.open {
            ctx.request_repaint();
        }
        let raw = if self.tgs.open { tw.min(elapsed_k) } else { tw };
        if raw < 0.005 && !self.tgs.open {
            return;
        }
        let appear = ios_spring(raw);
        let page_k = self
            .tgs
            .page_at
            .map_or(1.0, |t| (t.elapsed().as_secs_f32() / 0.18).clamp(0.0, 1.0));
        if page_k < 1.0 {
            ctx.request_repaint();
        }
        let page_slide = (1.0 - ios_spring(page_k)) * 18.0;
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
        let prev_page = page;
        let mut act = Actions::default();
        let max_h = (screen.height() - 36.0).clamp(320.0, 760.0);
        let was_open = self.tgs.open;
        let mut st = std::mem::take(&mut self.tgs);
        let me_name = self.me.clone();
        let me_tex = self.dialogs.iter().find(|d| d.saved).and_then(|d| self.avatars.get(&d.id));
        let corners = self.settings.ayu.avatar_corners;
        let streamer = self.settings.ayu.streamer_mode;
        let folders = &self.folders;
        let dialogs = &self.dialogs;
        let has_emoji = self.emoji.is_some();
        let now = self.now;
        let mut plugins_reg = self.plugins.clone();

        // Боковые кнопки мыши (Назад Extra1 / Вперёд Extra2) внутри окна настроек.
        let mouse_back = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra1));
        let mouse_fwd = ctx.input(|i| i.pointer.button_clicked(egui::PointerButton::Extra2));
        if mouse_back {
            if has_subview(&st, page) {
                pop_subview(&mut st, &mut page);
            } else if let Some(prev) = st.page_back.pop() {
                st.page_fwd.push(page);
                page = prev;
            } else if page != pg::MAIN {
                st.page_fwd.push(page);
                page = pg::MAIN;
            } else {
                act.close = true;
            }
        } else if mouse_fwd && let Some(next) = st.page_fwd.pop() {
            st.page_back.push(page);
            page = next;
        }

        egui::Area::new(egui::Id::new("tgs-box"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_CENTER, Vec2::new(page_slide, (1.0 - appear) * 32.0))
            .show(ctx, |ui| {
                ui.set_opacity(appear.clamp(0.02, 1.0));
                let scale = 0.94 + 0.06 * appear;
                let card = egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .stroke(Stroke::new(1.0, Color32::from_white_alpha((32.0 * opacity_f32()) as u8)))
                    .corner_radius(CornerRadius::same(14))
                    .show(ui, |ui| {
                        let width = (BOX_W * scale).min(screen.width() - 16.0);
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;
                        let (head, _) = ui.allocate_exact_size(Vec2::new(width, 50.0), Sense::hover());
                        let sub_active = has_subview(&st, page);
                        let title = if st.qr_open {
                            "QR-код профиля"
                        } else if page == pg::PRIVACY && st.blocked_open {
                            "Заблокированные"
                        } else if page == pg::PRIVACY && st.passcode_open {
                            "Локальный код-пароль"
                        } else if page == pg::PRIVACY && st.auto_delete_open {
                            "Автоудаление сообщений"
                        } else if page == pg::PRIVACY && st.account_ttl_open {
                            "Удаление аккаунта"
                        } else if let Some(k) = st.privacy_edit {
                            k.title()
                        } else if page == pg::LANGUAGE && st.dnt_open {
                            "Не переводить"
                        } else if page == pg::DEVICES && st.session_ttl_open {
                            "Неактивные сеансы"
                        } else {
                            title_of(page)
                        };
                        if page != pg::MAIN || sub_active {
                            let back = Rect::from_center_size(Pos2::new(head.left() + 26.0, head.center().y), Vec2::splat(34.0));
                            let r = ui.interact(back, ui.make_persistent_id("tgs-back"), Sense::click());
                            icons::back(ui.painter(), back.center(), if r.hovered() { WHITE } else { SUBTEXT });
                            if r.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                if sub_active {
                                    pop_subview(&mut st, &mut page);
                                } else {
                                    st.page_fwd.push(page);
                                    page = st.page_back.pop().unwrap_or(pg::MAIN);
                                }
                            }
                        }
                        ui.painter().text(
                            Pos2::new(head.left() + if page == pg::MAIN && !sub_active { 20.0 } else { 52.0 }, head.center().y),
                            Align2::LEFT_CENTER,
                            title,
                            FontId::proportional(16.0),
                            WHITE,
                        );
                        if page == pg::MAIN && !sub_active {
                            // Поиск по настройкам (как в AyuGram).
                            let sr = Rect::from_center_size(Pos2::new(head.right() - 88.0, head.center().y), Vec2::splat(32.0));
                            let sresp = ui.interact(sr, ui.make_persistent_id("tgs-search-btn"), Sense::click());
                            icons::tg(
                                ui.painter(),
                                "menu/search",
                                sr.center(),
                                if st.search_open || sresp.hovered() { WHITE } else { SUBTEXT },
                            );
                            if sresp.on_hover_text("Поиск настроек").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                st.search_open = !st.search_open;
                                if !st.search_open {
                                    st.search.clear();
                                }
                            }
                            // Меню «три точки» (Редактировать профиль / Выйти).
                            let mr = Rect::from_center_size(Pos2::new(head.right() - 56.0, head.center().y), Vec2::splat(32.0));
                            let mresp = ui.interact(mr, ui.make_persistent_id("tgs-dots"), Sense::click());
                            icons::dots(ui.painter(), mr.center(), if mresp.hovered() { WHITE } else { SUBTEXT });
                            egui::Popup::menu(&mresp.on_hover_cursor(egui::CursorIcon::PointingHand)).show(|ui| {
                                ui.set_min_width(190.0);
                                ui.spacing_mut().item_spacing = Vec2::ZERO;
                                if kit::menu_item(ui, "menu/edit", "Изменить профиль", false) {
                                    st.page_back.push(page);
                                    page = pg::ACCOUNT;
                                    ui.close();
                                }
                                if kit::menu_item(ui, "menu/qr_code", "QR-код профиля", false) {
                                    st.qr_open = true;
                                    ui.close();
                                }
                                if kit::menu_item(ui, "menu/leave", "Выйти из аккаунта", true) {
                                    act.logout = true;
                                    ui.close();
                                }
                            });
                        }
                        let x = Rect::from_center_size(Pos2::new(head.right() - 24.0, head.center().y), Vec2::splat(34.0));
                        let xr = ui.interact(x, ui.make_persistent_id("tgs-close"), Sense::click());
                        icons::tg(ui.painter(), "info/info_close", x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
                        if xr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            act.close = true;
                        }

                        egui::ScrollArea::vertical()
                            .max_height(max_h - 50.0)
                            .min_scrolled_height(max_h - 50.0)
                            .auto_shrink([false, true])
                            .show(ui, |ui| {
                                ui.spacing_mut().item_spacing = Vec2::ZERO;
                                if st.qr_open {
                                    qr_profile_page(ui, &st, &me_name, streamer);
                                    return;
                                }
                                match page {
                                    pg::ACCOUNT => account_page(ui, &mut st, &mut act, streamer),
                                    pg::NOTIFY => notify_page(ui, &st, &mut s, &mut act),
                                    pg::PRIVACY => privacy_page(ui, &mut st, &mut s, &mut page, &mut act),
                                    pg::DATA => data_page(ui, &st, &mut s, &mut act, has_emoji),
                                    pg::FOLDERS => folders_page(ui, &mut st, folders, dialogs, &mut act),
                                    pg::APPEARANCE => appearance_page(ui, &mut s),
                                    pg::LANGUAGE => language_page(ui, &mut st, &mut s),
                                    pg::DEVICES => devices_page(ui, &mut st, now, &mut act),
                                    pg::POWER => power_page(ui, &mut s),
                                    pg::RUSTGRAMM => rustgramm_page(ui, &mut s, &mut act, has_emoji),
                                    pg::PLUGINS => plugins_page(ui, &mut st, &mut s, &mut plugins_reg),
                                    _ => {
                                        if let Some(p) = main_page(ui, &mut st, &mut s, &me_name, me_tex, corners, streamer, &mut act) {
                                            st.page_back.push(page);
                                            st.page_fwd.clear();
                                            page = p;
                                        }
                                    }
                                }
                            });
                    });
                paint_modal_subblur(ui.painter(), card.response.rect, 14.0, raw);
            });
        self.plugins = plugins_reg;
        if page != prev_page {
            st.page_at = Some(std::time::Instant::now());
        }

        if (page == pg::DATA || page == pg::RUSTGRAMM) && st.page != page {
            st.cache_bytes = dir_size(&worker::home().join("cache"));
            st.downloads_bytes = dir_size(&worker::home().join("downloads"));
        }
        st.page = page;
        if act.clear_cache {
            let freed = clear_cache(&worker::home().join("cache"));
            st.cache_bytes = dir_size(&worker::home().join("cache"));
            self.cached_media.clear();
            self.history_cache.clear();
            self.status = format!("Кэш очищен: освобождено {}", size_text(freed));
        }
        if act.open_downloads {
            let dir = worker::home().join("downloads");
            let _ = std::fs::create_dir_all(&dir);
            rg_core::media::open_downloaded(&dir);
        }
        if act.save_profile {
            self.send(Cmd::UpdateProfile(st.first.trim().to_owned(), st.last.trim().to_owned(), st.about.trim().to_owned()));
            st.filled = false;
        }
        if act.save_username {
            self.send(Cmd::UpdateUsername(st.username.trim().to_owned()));
            st.filled = false;
        }
        if let Some(f) = act.save_folder {
            self.send(Cmd::SaveFolder(f));
            st.edit_folder = None;
        }
        if let Some(id) = act.delete_folder {
            self.send(Cmd::DeleteFolder(id));
            st.edit_folder = None;
        }
        if let Some((k, lvl)) = act.set_privacy {
            self.send(Cmd::SetPrivacy(k, lvl));
            st.privacy_edit = None;
            st.privacy_choice = None;
        }
        if let Some((scope, en, prev)) = act.set_notify {
            self.send(Cmd::SetNotifyScope(scope, en, prev));
        }
        if let Some(en) = act.set_signup_notify {
            self.send(Cmd::SetContactSignupNotify(en));
        }
        if let Some(ttl) = act.set_history_ttl {
            self.send(Cmd::SetDefaultHistoryTtl(ttl));
            st.auto_delete_open = false;
        }
        if let Some(days) = act.set_account_ttl {
            self.send(Cmd::SetAccountTtlDays(days));
            st.account_ttl_open = false;
        }
        if let Some(hash) = act.kill_session {
            self.send(Cmd::ResetAuthorization(hash));
            st.confirm_kill_session = None;
        }
        if act.kill_other_sessions {
            self.send(Cmd::ResetOtherAuthorizations);
            st.confirm_kill_session = None;
        }
        if let Some(days) = act.set_session_ttl {
            self.send(Cmd::SetAuthorizationTtl(days));
            st.session_ttl_open = false;
        }
        if let Some((id, ah)) = act.unblock {
            self.send(Cmd::UnblockUser(id, ah));
        }
        if act.clear_payments {
            self.send(Cmd::ClearPaymentInfo);
            st.confirm_clear_pay = false;
        }
        if act.open_proxy {
            self.proxy_open = true;
        }
        if act.logout {
            self.logout_confirm = true;
            act.close = true;
        }
        if act.lock_now && !s.security.passcode_hash.is_empty() {
            self.app_locked = true;
            act.close = true;
        }
        self.tgs = st;
        let esc = ctx.input(|i| i.key_pressed(egui::Key::Escape));
        if esc && (self.tgs.page != pg::MAIN || has_subview(&self.tgs, self.tgs.page)) {
            let mut p = self.tgs.page;
            pop_subview(&mut self.tgs, &mut p);
            self.tgs.page = p;
        } else {
            self.tgs.open = was_open && !(act.close || esc);
        }
        if act.advanced {
            self.tgs.open = false;
            self.settings_open = true;
            self.settings_tab = super::settings_ui::page::MENU;
        }
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(Box::new(s)));
        }
        if act.test_notify {
            self.push_notification(ctx, "Rustgramm", "Проверка уведомлений — всё работает! 😎");
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn main_page(
    ui: &mut egui::Ui,
    st: &mut State,
    s: &mut rg_settings::Settings,
    me: &str,
    tex: Option<&egui::TextureHandle>,
    corners: i32,
    streamer: bool,
    act: &mut Actions,
) -> Option<usize> {
    let w = ui.available_width();
    if st.search_open {
        ui.horizontal(|ui| {
            ui.add_space(14.0);
            ui.add(
                egui::TextEdit::singleline(&mut st.search)
                    .hint_text("Поиск по настройкам…")
                    .desired_width(w - 28.0)
                    .margin(Vec2::new(10.0, 6.0)),
            );
        });
        ui.add_space(6.0);
    }
    let q = st.search.trim().to_lowercase();
    if !q.is_empty() {
        let entries: &[(&str, &str, usize, &str)] = &[
            ("settings/user", "Мой аккаунт · имя, @username, о себе", pg::ACCOUNT, "аккаунт профиль имя фамилия username телефон"),
            ("settings/notifications", "Уведомления и звуки", pg::NOTIFY, "уведомления звук громкость угол панель задач превью"),
            ("settings/lock", "Конфиденциальность и безопасность", pg::PRIVACY, "конфиденциальность приватность пароль код 2fa блокировка автоудаление музыка"),
            ("settings/chat", "Настройки чатов и внешний вид", pg::APPEARANCE, "чат прозрачность блюр фон пузырь скругление тема"),
            ("settings/folders", "Папки с чатами", pg::FOLDERS, "папки фильтры чаты"),
            ("settings/advanced", "Продвинутые настройки · кэш и прокси", pg::DATA, "данные память кэш загрузки прокси socks5"),
            ("settings/battery", "Энергосбережение и анимации", pg::POWER, "энергосбережение анимации скорость gif"),
            ("settings/language", "Язык и переводчик", pg::LANGUAGE, "язык перевод переводить чаты english русский"),
            ("settings/laptop", "Устройства и активные сеансы", pg::DEVICES, "устройства сеансы сессии завершить"),
            ("heart", "Rustgramm · прозрачность и режим призрака", pg::RUSTGRAMM, "rustgramm призрак прозрачность эмодзи"),
        ];
        let mut hit = None;
        for (icon, title, p, keys) in entries {
            if (title.to_lowercase().contains(&q) || keys.contains(&q))
                && nav_row(ui, icon, title, "")
            {
                hit = Some(*p);
            }
        }
        if hit.is_none() && entries.iter().all(|(_, t, _, k)| !t.to_lowercase().contains(&q) && !k.contains(&q)) {
            info_row(ui, "Ничего не найдено", "");
        }
        return hit;
    }

    // Шапка профиля (как на скрине 9 AyuGram: аватар, имя, ID, @username, кнопка QR справа).
    let (head, _) = ui.allocate_exact_size(Vec2::new(w, 92.0), Sense::hover());
    let (name, id, user) = match &st.account {
        Some(a) => (
            if a.me.name.is_empty() { me.to_owned() } else { a.me.name.clone() },
            a.me.id,
            a.me.username.clone(),
        ),
        None => (me.to_owned(), 0, String::new()),
    };
    draw_avatar(ui, Pos2::new(head.left() + 48.0, head.center().y), &name, 1, 56.0, tex, corners);
    let p = ui.painter();
    p.text(Pos2::new(head.left() + 88.0, head.center().y - 18.0), Align2::LEFT_CENTER, &name, FontId::proportional(16.0), WHITE);
    if id != 0 {
        p.text(
            Pos2::new(head.left() + 88.0, head.center().y + 1.0),
            Align2::LEFT_CENTER,
            mask_streamer(&id.to_string(), streamer),
            FontId::proportional(12.5),
            SUBTEXT,
        );
    }
    let sub = if !user.is_empty() {
        mask_streamer(&format!("@{user}"), streamer)
    } else if st.account.is_none() {
        "загрузка…".to_owned()
    } else {
        "Нажмите «Мой аккаунт»".to_owned()
    };
    p.text(Pos2::new(head.left() + 88.0, head.center().y + 19.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.5), SUBTEXT);
    // Кнопка QR-кода справа (как в AyuGram).
    let qr_r = Rect::from_center_size(Pos2::new(head.right() - 28.0, head.center().y), Vec2::splat(36.0));
    let qr_resp = ui.interact(qr_r, ui.make_persistent_id("tgs-qr"), Sense::click());
    if qr_resp.hovered() {
        ui.painter().circle_filled(qr_r.center(), 18.0, t(WINDOW_OVER));
    }
    icons::tg(ui.painter(), "menu/qr_code", qr_r.center(), if qr_resp.hovered() { WHITE } else { ORANGE });
    if qr_resp.on_hover_text("QR-код профиля").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
        st.qr_open = true;
    }

    divider(ui);
    let mut go = None;
    // Первый пункт — как «AyuGram Preferences» в оригинале.
    if nav_row(ui, "menu/customize", "Настройки Rustgramm", "") {
        act.advanced = true;
    }
    let plug_lbl = if cfg!(feature = "plugins") {
        if s.plugins_enabled { "Вкл (Full)" } else { "Выкл" }
    } else {
        "Lite (без плагинов)"
    };
    if nav_row(ui, "settings/stickers", "Плагины и телефон (Full / Lite)", plug_lbl) {
        go = Some(pg::PLUGINS);
    }
    divider(ui);
    let sess_cnt = st
        .account
        .as_ref()
        .map(|a| a.sessions.len().to_string())
        .unwrap_or_default();
    let lang_lbl = lang_name(&s.language.interface_lang);
    for (icon, title, p, val) in [
        ("settings/user", "Мой аккаунт", pg::ACCOUNT, ""),
        ("settings/notifications", "Уведомления и звуки", pg::NOTIFY, ""),
        ("settings/lock", "Конфиденциальность", pg::PRIVACY, ""),
        ("settings/chat", "Настройки чатов", pg::APPEARANCE, ""),
        ("settings/folders", "Папки с чатами", pg::FOLDERS, ""),
        ("settings/advanced", "Продвинутые настройки", pg::DATA, ""),
        ("settings/battery", "Энергосбережение и анимации", pg::POWER, ""),
        ("settings/language", "Язык", pg::LANGUAGE, lang_lbl),
        ("settings/laptop", "Устройства", pg::DEVICES, sess_cnt.as_str()),
    ] {
        if nav_row(ui, icon, title, val) {
            go = Some(p);
        }
    }
    divider(ui);
    // Масштаб интерфейса прямо на главной (как на скринах 8 и 9 AyuGram!).
    if toggle(ui, "Масштаб по умолчанию", &mut s.default_scale) && s.default_scale {
        s.ui_scale = 100;
    }
    let mut sc = i32::from(s.ui_scale);
    let shown = format!("{sc}%");
    if slider(ui, "Масштаб интерфейса", &mut sc, (75, 200), shown) {
        s.ui_scale = sc.clamp(75, 200) as u8;
        s.default_scale = s.ui_scale == 100;
    }
    divider(ui);
    let prem_val = match st.account.as_ref() {
        Some(a) if a.me.premium => "Активна",
        Some(_) => "Нет",
        None => "",
    };
    if nav_row(ui, "menu/star", "Telegram Premium", prem_val) {
        go = Some(pg::RUSTGRAMM);
    }
    if nav_row(ui, "heart", "О Rustgramm", env!("CARGO_PKG_VERSION")) {
        go = Some(pg::RUSTGRAMM);
    }
    if nav_row(ui, "settings/faq", "Вопросы о Telegram (FAQ)", "") {
        rg_core::media::open_downloaded(Path::new("https://telegram.org/faq"));
    }
    desc(ui, &format!("Rustgramm v{} — клиент Telegram на Rust. Данные идут напрямую на серверы Telegram.", env!("CARGO_PKG_VERSION")));
    go
}

fn qr_profile_page(ui: &mut egui::Ui, st: &State, me: &str, streamer: bool) {
    let user = st.account.as_ref().map(|a| a.me.username.as_str()).unwrap_or("");
    let url = if !user.is_empty() {
        format!("https://t.me/{user}")
    } else {
        "https://t.me/".to_owned()
    };
    ui.add_space(18.0);
    ui.vertical_centered(|ui| {
        if let Ok(code) = qrcode::QrCode::new(url.as_bytes()) {
            let side = code.width();
            let dark: Vec<bool> = code.to_colors().into_iter().map(|c| c == qrcode::Color::Dark).collect();
            let sz = 210.0;
            let (rect, _) = ui.allocate_exact_size(Vec2::splat(sz), Sense::hover());
            ui.painter().rect_filled(rect, CornerRadius::same(14), Color32::WHITE);
            let inner = rect.shrink(14.0);
            let cell = inner.width() / side as f32;
            for y in 0..side {
                for x in 0..side {
                    if dark[y * side + x] {
                        let r = Rect::from_min_size(inner.min + Vec2::new(x as f32, y as f32) * cell, Vec2::splat(cell + 0.4));
                        ui.painter().rect_filled(r, 0.0, Color32::BLACK);
                    }
                }
            }
        }
        ui.add_space(12.0);
        ui.label(RichText::new(me).size(16.0).strong().color(WHITE));
        let link_text = if user.is_empty() {
            "Задайте @имя в разделе «Мой аккаунт»".to_owned()
        } else {
            mask_streamer(&format!("@{user}"), streamer)
        };
        ui.label(RichText::new(link_text).size(13.5).color(ORANGE));
        ui.add_space(12.0);
        if !user.is_empty() && orange_button(ui, "Скопировать ссылку", Vec2::new(190.0, 34.0)).clicked() {
            ui.ctx().copy_text(url);
        }
        ui.add_space(18.0);
    });
}

fn loading(ui: &mut egui::Ui, st: &State) -> bool {
    if st.account.is_some() {
        return false;
    }
    let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 80.0), Sense::hover());
    ui.painter().text(rect.center(), Align2::CENTER_CENTER, "Загрузка…", FontId::proportional(14.0), SUBTEXT);
    true
}

fn account_page(ui: &mut egui::Ui, st: &mut State, act: &mut Actions, streamer: bool) {
    if loading(ui, st) {
        return;
    }
    if !st.filled && let Some(a) = &st.account {
        let (f, l) = a.me.name.split_once(' ').map_or((a.me.name.as_str(), ""), |(f, l)| (f, l));
        st.first = f.to_owned();
        st.last = l.to_owned();
        st.about = a.me.about.clone();
        st.username = a.me.username.clone();
        st.filled = true;
    }
    let a = st.account.clone().unwrap_or_default();
    section(ui, "Профиль");
    for (label, text) in [("Имя", &mut st.first), ("Фамилия", &mut st.last)] {
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.allocate_ui_with_layout(Vec2::new(84.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
                ui.label(RichText::new(label).color(SUBTEXT).size(13.0));
            });
            let fw = (ui.available_width() - kit::PAD).max(120.0);
            ui.add_sized([fw, 30.0], egui::TextEdit::singleline(text).margin(Vec2::new(8.0, 6.0)));
        });
        ui.add_space(4.0);
    }
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        ui.allocate_ui_with_layout(Vec2::new(84.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(RichText::new("О себе").color(SUBTEXT).size(13.0));
        });
        let fw = (ui.available_width() - kit::PAD).max(120.0);
        ui.add_sized([fw, 60.0], egui::TextEdit::multiline(&mut st.about).margin(Vec2::new(8.0, 6.0)).desired_rows(3));
    });
    ui.add_space(8.0);
    let changed = {
        let (f, l) = a.me.name.split_once(' ').map_or((a.me.name.as_str(), ""), |(f, l)| (f, l));
        st.first.trim() != f || st.last.trim() != l || st.about.trim() != a.me.about
    };
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        if orange_button_enabled(ui, changed && !st.first.trim().is_empty(), "Сохранить профиль", Vec2::new(160.0, 34.0)).clicked() {
            act.save_profile = true;
        }
    });
    ui.add_space(8.0);
    divider(ui);
    section(ui, "Имя пользователя (@username)");
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        ui.allocate_ui_with_layout(Vec2::new(84.0, 30.0), egui::Layout::left_to_right(egui::Align::Center), |ui| {
            ui.label(RichText::new("@имя").color(SUBTEXT).size(13.0));
        });
        let fw = (ui.available_width() - kit::PAD).max(120.0);
        ui.add_sized([fw, 30.0], egui::TextEdit::singleline(&mut st.username).hint_text("username").margin(Vec2::new(8.0, 6.0)));
    });
    ui.add_space(6.0);
    let un_changed = st.username.trim().trim_start_matches('@') != a.me.username;
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        if orange_button_enabled(ui, un_changed, "Обновить @имя", Vec2::new(150.0, 32.0)).clicked() {
            act.save_username = true;
        }
    });
    ui.add_space(8.0);
    divider(ui);
    section(ui, "Данные аккаунта");
    info_row(ui, "Телефон", &if a.me.phone.is_empty() { "—".to_owned() } else { mask_streamer(&format!("+{}", a.me.phone), streamer) });
    if a.me.id != 0 {
        info_row(ui, "ID", &mask_streamer(&a.me.id.to_string(), streamer));
    }
    if !a.me.birthday.is_empty() {
        info_row(ui, "День рождения", &a.me.birthday);
    }
    if nav_row(ui, "menu/qr_code", "Показать QR-код профиля", "") {
        st.qr_open = true;
    }
}

fn notify_page(ui: &mut egui::Ui, st: &State, s: &mut rg_settings::Settings, act: &mut Actions) {
    let w = ui.available_width();
    section(ui, "Общие настройки");
    toggle(ui, "Уведомления на рабочем столе", &mut s.notify.desktop_notifications);
    toggle(ui, "Мигать значком на панели задач", &mut s.notify.flash_taskbar);
    toggle(ui, "Звук уведомлений", &mut s.notify.allow_sound);
    let mut vol = i32::from(s.notify.volume);
    let shown = format!("{vol}%");
    if slider(ui, "Громкость", &mut vol, (0, 100), shown) {
        s.notify.volume = vol.clamp(0, 100) as u8;
    }

    // Интерактивная карточка-превью уведомления (как «Dino Rex / It's morning in Tokyo» в AyuGram!).
    ui.add_space(6.0);
    let (card_wrap, _) = ui.allocate_exact_size(Vec2::new(w, 66.0), Sense::hover());
    let card = Rect::from_min_max(
        Pos2::new(card_wrap.left() + kit::PAD, card_wrap.top() + 2.0),
        Pos2::new(card_wrap.right() - kit::PAD, card_wrap.bottom() - 2.0),
    );
    let p = ui.painter();
    p.rect_filled(card, CornerRadius::same(10), t(WINDOW_OVER));
    p.circle_filled(Pos2::new(card.left() + 30.0, card.center().y), 19.0, ORANGE);
    p.text(Pos2::new(card.left() + 30.0, card.center().y), Align2::CENTER_CENTER, "R", FontId::proportional(17.0), Color32::WHITE);
    let title_str = if s.notify.show_name { "Rustgramm" } else { "Новое сообщение" };
    let body_str = if s.notify.show_text { "Доброе утро в Telegram! 😎" } else { "У вас новое сообщение" };
    p.text(Pos2::new(card.left() + 60.0, card.center().y - 9.0), Align2::LEFT_CENTER, title_str, FontId::proportional(13.5), WHITE);
    p.text(Pos2::new(card.left() + 60.0, card.center().y + 10.0), Align2::LEFT_CENTER, body_str, FontId::proportional(12.5), SUBTEXT);

    // Пилюли-галочки [✓ Имя] [✓ Текст] прямо под превью (как в AyuGram!).
    ui.add_space(6.0);
    ui.horizontal(|ui| {
        ui.add_space((w - 178.0).max(kit::PAD) * 0.5);
        for (lbl, flag) in [("Имя", &mut s.notify.show_name), ("Текст", &mut s.notify.show_text)] {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(84.0, 28.0), Sense::click());
            let bg = if *flag { t_btn(ORANGE) } else { t(WINDOW_OVER) };
            ui.painter().rect_filled(r, CornerRadius::same(14), bg);
            let txt = if *flag { format!("✓ {lbl}") } else { lbl.to_owned() };
            ui.painter().text(r.center(), Align2::CENTER_CENTER, txt, FontId::proportional(12.5), Color32::WHITE);
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                *flag = !*flag;
            }
            ui.add_space(10.0);
        }
    });
    ui.add_space(8.0);
    divider(ui);

    // Серверные настройки для категорий чатов (Личные / Группы / Каналы).
    section(ui, "Уведомления для чатов");
    if let Some(a) = &st.account {
        for n in &a.notify {
            let mut en = n.enabled;
            let mut prev = n.preview;
            let sub = format!(
                "{} · текст {}",
                if en { "Включены" } else { "Выключены" },
                if prev { "показан" } else { "скрыт" }
            );
            let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 50.0), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(rect, 0.0, t(WINDOW_OVER));
            }
            let icon = match n.scope {
                NotifyScope::Users => "settings/user",
                NotifyScope::Chats => "settings/group",
                NotifyScope::Broadcasts => "settings/channel",
            };
            icons::tg(ui.painter(), icon, Pos2::new(rect.left() + kit::PAD + 12.0, rect.center().y), SUBTEXT);
            let x = rect.left() + kit::PAD + 44.0;
            ui.painter().text(Pos2::new(x, rect.center().y - 9.0), Align2::LEFT_CENTER, n.title, FontId::proportional(14.0), WHITE);
            ui.painter().text(Pos2::new(x, rect.center().y + 9.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), SUBTEXT);
            // Переключатель справа.
            let sw_r = Rect::from_center_size(Pos2::new(rect.right() - kit::PAD - 17.0, rect.center().y), Vec2::new(42.0, 26.0));
            let track = Rect::from_center_size(sw_r.center(), Vec2::new(34.0, 14.0));
            ui.painter().rect_filled(track, CornerRadius::same(7), if en { t_btn(ORANGE) } else { t_btn(navy(0x4b5866)) });
            let kx = if en { track.right() - 7.0 } else { track.left() + 7.0 };
            ui.painter().circle_filled(Pos2::new(kx, track.center().y), 6.0, Color32::WHITE);
            let click_pos = resp.interact_pointer_pos();
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                let over_sw = click_pos.is_some_and(|pt| sw_r.expand(8.0).contains(pt));
                if over_sw {
                    en = !en;
                } else {
                    prev = !prev;
                }
                act.set_notify = Some((n.scope, en, prev));
            }
        }
    } else {
        info_row(ui, "Загрузка настроек Telegram…", "");
    }
    desc(ui, "Нажмите на переключатель, чтобы включить или выключить категорию; нажмите на строку, чтобы скрыть или показать текст.");

    divider(ui);
    section(ui, "События");
    let mut signup = st.account.as_ref().is_none_or(|a| a.contact_signup_notify);
    if toggle(ui, "Контакт присоединился к Telegram", &mut signup) {
        act.set_signup_notify = Some(signup);
    }
    toggle(ui, "Закреплённые сообщения", &mut s.notify.pinned_messages);

    divider(ui);
    section(ui, "Звонки");
    toggle(ui, "Принимать звонки на этом устройстве", &mut s.notify.accept_calls);

    divider(ui);
    section(ui, "Счётчик непрочитанных");
    toggle(ui, "Учитывать чаты без звука", &mut s.notify.badge_include_muted);
    toggle(ui, "Считать непрочитанные сообщения", &mut s.notify.badge_count_messages);

    divider(ui);
    section(ui, "Интеграция с системой");
    toggle(ui, "Использовать уведомления Windows", &mut s.notify.use_windows_notifications);
    toggle(ui, "Учитывать системный режим фокусировки", &mut s.notify.respect_focus_mode);

    divider(ui);
    section(ui, "Расположение на экране");
    // Интерактивный монитор с 4 кликабельными углами (1 в 1 со скрином 2 AyuGram!).
    let (mon_wrap, _) = ui.allocate_exact_size(Vec2::new(w, 160.0), Sense::hover());
    let mon = Rect::from_center_size(Pos2::new(mon_wrap.center().x, mon_wrap.top() + 70.0), Vec2::new(210.0, 122.0));
    let p = ui.painter();
    // Подставка монитора.
    p.rect_filled(Rect::from_center_size(Pos2::new(mon.center().x, mon.bottom() + 10.0), Vec2::new(34.0, 18.0)), 0.0, Color32::from_rgb(130, 134, 144));
    p.rect_filled(Rect::from_center_size(Pos2::new(mon.center().x, mon.bottom() + 20.0), Vec2::new(72.0, 5.0)), CornerRadius::same(2), Color32::from_rgb(180, 184, 194));
    // Корпус и экран монитора.
    p.rect_filled(mon, CornerRadius::same(8), Color32::from_rgb(225, 228, 236));
    let scr = mon.shrink(6.0);
    p.rect_filled(scr, CornerRadius::same(4), Color32::from_rgb(28, 36, 48));
    let corners_pos = [
        (0u8, Pos2::new(scr.left() + 8.0, scr.top() + 8.0)),
        (1u8, Pos2::new(scr.right() - 56.0, scr.top() + 8.0)),
        (2u8, Pos2::new(scr.left() + 8.0, scr.bottom() - 40.0)),
        (3u8, Pos2::new(scr.right() - 56.0, scr.bottom() - 40.0)),
    ];
    for (idx, pos) in corners_pos {
        let slot = Rect::from_min_size(pos, Vec2::new(48.0, 32.0));
        let sr = ui.interact(slot, ui.make_persistent_id(("ncorner", idx)), Sense::click());
        let active = s.notify.corner == idx;
        let fill = if active { t_btn(ORANGE) } else if sr.hovered() { Color32::from_white_alpha(45) } else { Color32::from_black_alpha(110) };
        ui.painter().rect_filled(slot, CornerRadius::same(4), fill);
        let bars = if active { s.notify.max_count.clamp(1, 3) } else { 1 };
        for b in 0..bars {
            let y = slot.top() + 5.0 + f32::from(b) * 8.0;
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(slot.left() + 5.0, y), Vec2::new(slot.width() - 10.0, 5.0)),
                CornerRadius::same(2),
                if active { Color32::WHITE } else { Color32::from_white_alpha(100) },
            );
        }
        if sr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            s.notify.corner = idx;
        }
    }

    section(ui, "Количество уведомлений");
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        let step_w = (w - 2.0 * kit::PAD) / 5.0;
        for n in 1u8..=5 {
            let (r, resp) = ui.allocate_exact_size(Vec2::new(step_w, 34.0), Sense::click());
            let sel = s.notify.max_count == n;
            ui.painter().rect_filled(
                Rect::from_min_size(Pos2::new(r.left() + 2.0, r.top() + 2.0), Vec2::new(r.width() - 4.0, 3.0)),
                CornerRadius::same(1),
                if sel { ORANGE } else { Color32::from_white_alpha(26) },
            );
            ui.painter().text(
                Pos2::new(r.center().x, r.center().y + 4.0),
                Align2::CENTER_CENTER,
                n.to_string(),
                FontId::proportional(13.5),
                if sel { ORANGE } else { SUBTEXT },
            );
            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                s.notify.max_count = n;
            }
        }
    });
    ui.add_space(8.0);
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        if orange_button(ui, "Проверить уведомление", Vec2::new(200.0, 34.0)).clicked() {
            act.test_notify = true;
        }
    });
    ui.add_space(12.0);
}

fn privacy_page(
    ui: &mut egui::Ui,
    st: &mut State,
    s: &mut rg_settings::Settings,
    page: &mut usize,
    act: &mut Actions,
) {
    // Подэкран редактирования одного правила приватности.
    if let Some(key) = st.privacy_edit {
        section(ui, "Кто может видеть");
        let cur = st.privacy_choice.unwrap_or(PrivacyLevel::Nobody);
        for (lvl, title) in [
            (PrivacyLevel::Everybody, "Все"),
            (PrivacyLevel::Contacts, "Мои контакты"),
            (PrivacyLevel::Nobody, "Никто"),
        ] {
            if radio_row(ui, title, "", cur == lvl) {
                st.privacy_choice = Some(lvl);
            }
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            if orange_button(ui, "Сохранить в Telegram", Vec2::new(190.0, 34.0)).clicked() {
                act.set_privacy = Some((key, cur));
            }
        });
        ui.add_space(10.0);
        desc(ui, "Изменение сразу применяется на серверах Telegram для вашего аккаунта.");
        return;
    }

    // Подэкран заблокированных пользователей с кнопкой «Разблокировать».
    if st.blocked_open {
        let Some(a) = st.account.as_ref() else {
            loading(ui, st);
            return;
        };
        section(ui, &format!("Заблокированные ({})", a.blocked.len()));
        if a.blocked.is_empty() {
            info_row(ui, "Список пуст", "");
        }
        for u in &a.blocked {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 52.0), Sense::hover());
            let p = ui.painter();
            p.text(Pos2::new(rect.left() + kit::PAD, rect.center().y - 8.0), Align2::LEFT_CENTER, &u.name, FontId::proportional(14.0), WHITE);
            if !u.username.is_empty() {
                p.text(Pos2::new(rect.left() + kit::PAD, rect.center().y + 10.0), Align2::LEFT_CENTER, format!("@{}", u.username), FontId::proportional(12.0), SUBTEXT);
            }
            let btn = Rect::from_center_size(Pos2::new(rect.right() - 72.0, rect.center().y), Vec2::new(112.0, 28.0));
            let br = ui.interact(btn, ui.make_persistent_id(("unblk", u.id)), Sense::click());
            p.rect_filled(btn, CornerRadius::same(8), if br.hovered() { t_btn(ORANGE) } else { t(WINDOW_OVER) });
            p.text(btn.center(), Align2::CENTER_CENTER, "Разблокировать", FontId::proportional(12.0), WHITE);
            if br.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                act.unblock = Some((u.id, u.access_hash));
            }
        }
        return;
    }

    // Подэкран локального код-пароля.
    if st.passcode_open {
        let enabled = !s.security.passcode_hash.is_empty();
        section(ui, if enabled { "Код-пароль включён" } else { "Задать локальный код-пароль" });
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.add_sized(
                [ui.available_width() - 2.0 * kit::PAD, 32.0],
                egui::TextEdit::singleline(&mut st.passcode_input)
                    .password(true)
                    .hint_text(if enabled { "Новый код-пароль" } else { "Введите код-пароль (от 4 символов)" })
                    .margin(Vec2::new(8.0, 6.0)),
            );
        });
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.add_sized(
                [ui.available_width() - 2.0 * kit::PAD, 32.0],
                egui::TextEdit::singleline(&mut st.passcode_confirm)
                    .password(true)
                    .hint_text("Повторите код-пароль")
                    .margin(Vec2::new(8.0, 6.0)),
            );
        });
        if !st.passcode_err.is_empty() {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(kit::PAD);
                ui.colored_label(DANGER, &st.passcode_err);
            });
        }
        ui.add_space(10.0);
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            if orange_button(ui, if enabled { "Сменить пароль" } else { "Включить" }, Vec2::new(150.0, 34.0)).clicked() {
                if st.passcode_input.chars().count() < 4 {
                    st.passcode_err = "Минимум 4 символа".to_owned();
                } else if st.passcode_input != st.passcode_confirm {
                    st.passcode_err = "Пароли не совпадают".to_owned();
                } else {
                    s.security.passcode_hash = rg_settings::hash_passcode(&st.passcode_input);
                    st.passcode_input.clear();
                    st.passcode_confirm.clear();
                    st.passcode_err.clear();
                    st.passcode_open = false;
                }
            }
            if enabled {
                ui.add_space(8.0);
                if ui.button(RichText::new("Отключить").color(DANGER)).clicked() {
                    s.security.passcode_hash.clear();
                    st.passcode_open = false;
                }
            }
        });
        if enabled {
            ui.add_space(8.0);
            divider(ui);
            if nav_row(ui, "settings/lock", "Заблокировать приложение сейчас", "") {
                act.lock_now = true;
            }
        }
        ui.add_space(8.0);
        desc(ui, "Код-пароль хранится только на этом устройстве в виде хеша и блокирует окно Rustgramm по требованию или при запуске.");
        return;
    }

    // Подэкран автоудаления сообщений.
    if st.auto_delete_open {
        let cur = st.account.as_ref().map_or(0, |a| a.default_history_ttl);
        section(ui, "Таймер для новых чатов");
        for (secs, lbl) in [(0, "Выключено"), (86_400, "Через 1 день"), (604_800, "Через 1 неделю"), (2_678_400, "Через 1 месяц")] {
            if radio_row(ui, lbl, "", cur == secs) {
                act.set_history_ttl = Some(secs);
            }
        }
        return;
    }

    // Подэкран удаления аккаунта при неактивности.
    if st.account_ttl_open {
        let cur = st.account.as_ref().map_or(180, |a| a.account_ttl_days);
        section(ui, "Удалить аккаунт, если не захожу");
        for (days, lbl) in [(30, "1 месяц"), (90, "3 месяца"), (180, "6 месяцев"), (365, "1 год"), (548, "18 месяцев"), (730, "2 года")] {
            if radio_row(ui, lbl, "", (cur - days).abs() <= 5) {
                act.set_account_ttl = Some(days);
            }
        }
        return;
    }

    if loading(ui, st) {
        return;
    }
    let a = st.account.clone().unwrap_or_default();

    section(ui, "Безопасность");
    nav_row(ui, "settings/key", "Облачный пароль (2FA)", if a.has_2fa { "Вкл" } else { "Выкл" });
    if nav_row(ui, "settings/ttl", "Автоудаление сообщений", history_ttl_label(a.default_history_ttl)) {
        st.auto_delete_open = true;
    }
    if nav_row(
        ui,
        "menu/passcode_finger",
        "Локальный код-пароль",
        if s.security.passcode_hash.is_empty() { "Выкл" } else { "Вкл" },
    ) {
        st.passcode_open = true;
    }
    if !a.login_email.is_empty() {
        nav_row(ui, "settings/email", "Почта для входа", &a.login_email);
    }
    if nav_row(ui, "settings/blocked", "Заблокированные пользователи", &a.blocked.len().to_string()) {
        st.blocked_open = true;
    }
    if nav_row(ui, "settings/laptop", "Активные сеансы", &a.sessions.len().to_string()) {
        *page = pg::DEVICES;
    }

    divider(ui);
    section(ui, "Конфиденциальность");
    for r in &a.privacy {
        if value_row(ui, r.title, &r.rule, false) {
            st.privacy_edit = Some(r.key);
            st.privacy_choice = Some(r.level);
        }
    }

    divider(ui);
    section(ui, "Боты и сайты");
    if !st.confirm_clear_pay {
        if nav_row(ui, "menu/clear", "Очистить платёжные данные и адреса", "") {
            st.confirm_clear_pay = true;
        }
    } else {
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            if orange_button(ui, "Подтвердить очистку", Vec2::new(170.0, 30.0)).clicked() {
                act.clear_payments = true;
            }
            ui.add_space(8.0);
            if ui.button("Отмена").clicked() {
                st.confirm_clear_pay = false;
            }
        });
        ui.add_space(6.0);
    }

    divider(ui);
    section(ui, "Частые контакты");
    toggle(ui, "Показывать частые контакты", &mut s.security.suggest_frequent_contacts);
    desc(ui, "Отображать людей, с которыми вы часто общаетесь, в верхней части поиска.");

    divider(ui);
    section(ui, "Удаление аккаунта");
    if value_row(ui, "Если я не захожу", &days_months_label(a.account_ttl_days), false) {
        st.account_ttl_open = true;
    }
}

fn data_page(ui: &mut egui::Ui, st: &State, s: &mut rg_settings::Settings, act: &mut Actions, atlas: bool) {
    section(ui, "Память на этом устройстве");
    info_row(ui, "Кэш медиа и истории", &size_text(st.cache_bytes));
    if nav_row(ui, "menu/clear", "Очистить кэш", "") {
        act.clear_cache = true;
    }
    desc(ui, "Удаляются скачанные ролики, музыка и кэш истории. Фото профилей и сохранённые удалённые сообщения остаются.");
    divider(ui);
    section(ui, "Загрузки и сеть");
    info_row(ui, "Сохранённые файлы", &size_text(st.downloads_bytes));
    if nav_row(ui, "menu/show_in_folder", "Открыть папку загрузок", "") {
        act.open_downloads = true;
    }
    if nav_row(ui, "settings/advanced", "Настройки соединения и прокси (SOCKS5)", "") {
        act.open_proxy = true;
    }
    divider(ui);
    section(ui, "Режим призрака и медиа");
    let mut ghost = s.ghost.active;
    if toggle(ui, "Режим призрака", &mut ghost) {
        s.ghost.set_active(ghost);
    }
    toggle(ui, "Автозапуск GIF в ленте", &mut s.auto_play_gifs);
    let atlas_path = worker::home().join("emoji-apple.png");
    info_row(ui, "Атлас эмодзи", if atlas && atlas_path.exists() { "Apple (локально)" } else { "Twemoji" });
}

fn folders_page(
    ui: &mut egui::Ui,
    st: &mut State,
    folders: &[rg_core::engine::FolderRow],
    dialogs: &[rg_core::engine::DialogRow],
    act: &mut Actions,
) {
    if let Some(f) = st.edit_folder.as_mut() {
        section(ui, if st.edit_folder_is_new { "Новая папка" } else { "Изменить папку" });
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.add_sized(
                [ui.available_width() - 2.0 * kit::PAD, 32.0],
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
                act.save_folder = Some(clean);
            }
            ui.add_space(8.0);
            if !is_new && ui.button(RichText::new("Удалить").color(DANGER)).clicked() {
                act.delete_folder = Some(id);
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
    toggle(ui, "Размытие стекла (Acrylic / iOS)", &mut s.blur_enabled);
    desc(ui, "На старте окно непрозрачное (100%). Снизьте прозрачность — фон окна и кнопок станет стеклянным с системным размытием.");
    divider(ui);
    section(ui, "Фон чата");
    for (id, label) in [(0u8, "Сплошной тёмный"), (1, "Мягкий градиент"), (2, "Узор Telegram")] {
        let mark = if s.chat_bg_style == id { "✓" } else { "" };
        if nav_row(ui, "settings/palette", label, mark) {
            s.chat_bg_style = id;
        }
    }
    divider(ui);
    section(ui, "Пузыри и аватарки");
    let mut rad = s.ayu.message_bubble_radius;
    let shown = format!("{rad} px");
    if slider(ui, "Скругление сообщений", &mut rad, (4, 22), shown) {
        s.ayu.message_bubble_radius = rad;
    }
    let mut wide = s.ayu.wide_multiplier_pct;
    let shown = format!("{wide}%");
    if slider(ui, "Ширина пузыря", &mut wide, (80, 150), shown) {
        s.ayu.wide_multiplier_pct = wide;
    }
    let mut av = s.ayu.avatar_corners;
    let shown = format!("{av}");
    if slider(ui, "Скругление аватарок", &mut av, (0, 23), shown) {
        s.ayu.avatar_corners = av;
    }
}

fn language_page(ui: &mut egui::Ui, st: &mut State, s: &mut rg_settings::Settings) {
    if st.dnt_open {
        section(ui, "Не переводить эти языки");
        for (code, native, ru) in LANG_CHOICES {
            let mut has = s.language.do_not_translate.iter().any(|c| c == code);
            let label = format!("{native} ({ru})");
            if toggle(ui, &label, &mut has) {
                if has {
                    if !s.language.do_not_translate.iter().any(|c| c == code) {
                        s.language.do_not_translate.push((*code).to_owned());
                    }
                } else {
                    s.language.do_not_translate.retain(|c| c != code);
                }
            }
        }
        return;
    }
    section(ui, "Перевод сообщений");
    toggle(ui, "Показывать кнопку «Перевести»", &mut s.language.show_translate_button);
    toggle(ui, "Переводить чаты целиком", &mut s.language.translate_entire_chats);
    let dnt_n = s.language.do_not_translate.len();
    let dnt_lbl = match dnt_n {
        0 => "Нет".to_owned(),
        1 => "1 язык".to_owned(),
        2..=4 => format!("{dnt_n} языка"),
        _ => format!("{dnt_n} языков"),
    };
    if value_row(ui, "Не переводить", &dnt_lbl, false) {
        st.dnt_open = true;
    }
    desc(ui, "Кнопка «Перевести» появляется в контекстном меню сообщений; режим «Переводить чаты целиком» добавляет полосу перевода сверху чата.");
    divider(ui);
    section(ui, "Язык перевода и интерфейса");
    ui.horizontal(|ui| {
        ui.add_space(kit::PAD);
        ui.add_sized(
            [ui.available_width() - 2.0 * kit::PAD, 32.0],
            egui::TextEdit::singleline(&mut st.lang_search)
                .hint_text("Поиск языка…")
                .margin(Vec2::new(8.0, 6.0)),
        );
    });
    ui.add_space(4.0);
    let q = st.lang_search.trim().to_lowercase();
    for (code, native, ru) in LANG_CHOICES {
        if !q.is_empty() && !native.to_lowercase().contains(&q) && !ru.to_lowercase().contains(&q) && !code.contains(&q) {
            continue;
        }
        let sel = s.language.translate_to == *code;
        if radio_row(ui, native, ru, sel) {
            s.language.translate_to = (*code).to_owned();
            if *code == "ru" || *code == "en" {
                s.language.interface_lang = (*code).to_owned();
            }
        }
    }
}

fn devices_page(ui: &mut egui::Ui, st: &mut State, now: i64, act: &mut Actions) {
    if st.session_ttl_open {
        let cur = st.account.as_ref().map_or(180, |a| a.session_ttl_days);
        section(ui, "Завершать неактивные сеансы через");
        for (days, lbl) in [(7, "1 неделю"), (30, "1 месяц"), (90, "3 месяца"), (180, "6 месяцев"), (365, "1 год")] {
            if radio_row(ui, lbl, "", (cur - days).abs() <= 5) {
                act.set_session_ttl = Some(days);
            }
        }
        return;
    }
    if loading(ui, st) {
        return;
    }
    let a = st.account.clone().unwrap_or_default();
    let w = ui.available_width();
    if let Some(cur) = a.sessions.iter().find(|x| x.current) {
        section(ui, "Это устройство");
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 60.0), Sense::hover());
        let p = ui.painter();
        p.circle_filled(Pos2::new(rect.left() + 34.0, rect.center().y), 18.0, t_btn(ORANGE));
        icons::tg(p, "settings/laptop", Pos2::new(rect.left() + 34.0, rect.center().y), Color32::WHITE);
        p.text(Pos2::new(rect.left() + 64.0, rect.center().y - 16.0), Align2::LEFT_CENTER, &cur.device, FontId::proportional(14.0), WHITE);
        p.text(Pos2::new(rect.left() + 64.0, rect.center().y + 1.0), Align2::LEFT_CENTER, cur.app.trim(), FontId::proportional(12.5), WHITE);
        p.text(Pos2::new(rect.left() + 64.0, rect.center().y + 17.0), Align2::LEFT_CENTER, &cur.place, FontId::proportional(12.0), SUBTEXT);

        // Красная кнопка «Завершить все остальные сеансы» (как на скрине 6 AyuGram!).
        let has_others = a.sessions.iter().any(|x| !x.current);
        if has_others {
            if st.confirm_kill_session == Some(0) {
                ui.horizontal(|ui| {
                    ui.add_space(kit::PAD);
                    ui.colored_label(DANGER, "Выйти со всех других устройств?");
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    ui.add_space(kit::PAD);
                    if ui.button(RichText::new("Да, завершить").color(DANGER)).clicked() {
                        act.kill_other_sessions = true;
                    }
                    ui.add_space(8.0);
                    if ui.button("Отмена").clicked() {
                        st.confirm_kill_session = None;
                    }
                });
                ui.add_space(8.0);
            } else {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 42.0), Sense::click());
                if resp.hovered() {
                    ui.painter().rect_filled(r, 0.0, t(WINDOW_OVER));
                }
                icons::tg(ui.painter(), "menu/delete", Pos2::new(r.left() + 34.0, r.center().y), DANGER);
                ui.painter().text(
                    Pos2::new(r.left() + 64.0, r.center().y),
                    Align2::LEFT_CENTER,
                    "Завершить все остальные сеансы",
                    FontId::proportional(14.0),
                    DANGER,
                );
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    st.confirm_kill_session = Some(0);
                }
            }
            desc(ui, "Выход со всех устройств, кроме текущего.");
        }
        divider(ui);
    }

    section(ui, "Активные сеансы");
    let mut any = false;
    for x in a.sessions.iter().filter(|x| !x.current) {
        any = true;
        let ago = now - x.active;
        let when = if ago < 300 {
            "в сети".to_owned()
        } else if ago < 86_400 {
            "сегодня".to_owned()
        } else {
            format!("{} дн. назад", ago / 86_400)
        };
        let is_android = x.platform.to_lowercase().contains("android") || x.app.to_lowercase().contains("android");
        let badge_c = if is_android { GREEN } else { Color32::from_rgb(66, 146, 232) };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 64.0), Sense::hover());
        let p = ui.painter();
        p.circle_filled(Pos2::new(rect.left() + 34.0, rect.center().y), 18.0, badge_c);
        icons::tg(p, if is_android { "menu/phone" } else { "settings/laptop" }, Pos2::new(rect.left() + 34.0, rect.center().y), Color32::WHITE);
        p.text(Pos2::new(rect.left() + 64.0, rect.center().y - 16.0), Align2::LEFT_CENTER, &x.device, FontId::proportional(14.0), WHITE);
        p.text(Pos2::new(rect.left() + 64.0, rect.center().y + 1.0), Align2::LEFT_CENTER, x.app.trim(), FontId::proportional(12.5), WHITE);
        let sub = [x.place.as_str(), when.as_str()].into_iter().filter(|s| !s.is_empty()).collect::<Vec<_>>().join(" · ");
        p.text(Pos2::new(rect.left() + 64.0, rect.center().y + 17.0), Align2::LEFT_CENTER, sub, FontId::proportional(12.0), SUBTEXT);

        let xr = Rect::from_center_size(Pos2::new(rect.right() - 24.0, rect.center().y), Vec2::splat(30.0));
        let xresp = ui.interact(xr, ui.make_persistent_id(("ksess", x.hash)), Sense::click());
        icons::tg(ui.painter(), "info/info_close", xr.center(), if xresp.hovered() { DANGER } else { SUBTEXT });
        if xresp.on_hover_text("Завершить сеанс").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            st.confirm_kill_session = Some(x.hash);
        }
        if st.confirm_kill_session == Some(x.hash) {
            ui.horizontal(|ui| {
                ui.add_space(64.0);
                if ui.button(RichText::new("Завершить этот сеанс").color(DANGER)).clicked() {
                    act.kill_session = Some(x.hash);
                }
                ui.add_space(8.0);
                if ui.button("Отмена").clicked() {
                    st.confirm_kill_session = None;
                }
            });
            ui.add_space(6.0);
        }
    }
    if !any {
        info_row(ui, "Других сеансов нет", "");
    }
    divider(ui);
    if value_row(ui, "Завершать неактивные через", &days_months_label(a.session_ttl_days), false) {
        st.session_ttl_open = true;
    }
}

fn power_page(ui: &mut egui::Ui, s: &mut rg_settings::Settings) {
    section(ui, "Анимации и производительность");
    let mut on = s.anim_speed > 0;
    if toggle(ui, "Плавные анимации", &mut on) {
        s.anim_speed = if on { 100 } else { 0 };
    }
    let mut a = i32::from(s.anim_speed);
    let shown = if a == 0 { "выкл.".to_owned() } else { format!("{a}%") };
    if slider(ui, "Скорость анимаций", &mut a, (0, 200), shown) {
        s.anim_speed = a.clamp(0, 200) as u8;
    }
    toggle(ui, "Автозапуск GIF в ленте", &mut s.auto_play_gifs);
    toggle(ui, "Размытие стекла (блюр)", &mut s.blur_enabled);
    desc(ui, "Отключение анимаций и автозапуска GIF снижает нагрузку на GPU и ОЗУ до минимума.");
}

fn rustgramm_page(ui: &mut egui::Ui, s: &mut rg_settings::Settings, act: &mut Actions, atlas: bool) {
    section(ui, "Окно и стекло");
    let mut o = i32::from(s.window_opacity);
    let shown = format!("{o}%");
    if slider(ui, "Прозрачность окна и кнопок", &mut o, (15, 100), shown) {
        s.window_opacity = o.clamp(15, 100) as u8;
    }
    toggle(ui, "Размытие стекла (блюр)", &mut s.blur_enabled);
    toggle(ui, "Автозапуск GIF в ленте", &mut s.auto_play_gifs);
    divider(ui);
    section(ui, "Приватность");
    let mut ghost = s.ghost.active;
    if toggle(ui, "Режим призрака", &mut ghost) {
        s.ghost.set_active(ghost);
    }
    if nav_row(ui, "menu/customize", "Расширенные настройки Rustgramm", "") {
        act.advanced = true;
    }
    divider(ui);
    section(ui, "Оформление и данные");
    info_row(ui, "Тема", "Тёмная · оранжевые кнопки");
    let atlas_path = worker::home().join("emoji-apple.png");
    info_row(ui, "Эмодзи", if atlas && atlas_path.exists() { "Apple (локально)" } else { "Twemoji" });
    if nav_row(ui, "menu/clear", "Очистить кэш", "") {
        act.clear_cache = true;
    }
    info_row(ui, "Версия", env!("CARGO_PKG_VERSION"));
}

fn plugins_page(
    ui: &mut egui::Ui,
    st: &mut State,
    s: &mut rg_settings::Settings,
    reg: &mut rg_ayu::plugins::PluginRegistry,
) {
    section(ui, "Режим сборки клиента");
    info_row(
        ui,
        "Сборка",
        if cfg!(feature = "plugins") {
            "Полная (Full · с плагинами)"
        } else {
            "Облегчённая (Lite · без плагинов)"
        },
    );
    if !cfg!(feature = "plugins") {
        desc(ui, "Эта сборка скомпилирована с флагом --no-default-features (Lite без плагинов). Для полной версии используйте сборку Rustgramm-Android-Full / стандартный билд.");
        return;
    }
    toggle(ui, "Включить систему плагинов", &mut s.plugins_enabled);
    desc(ui, "Плагины работают локально на Rust (команды /shrug, /flip, /calc, /upper, макросы и ваши файлы .plugin в папке plugins/).");

    divider(ui);
    section(ui, &format!("Установленные плагины ({})", reg.plugins.len()));
    let w = ui.available_width();
    let mut remove_id: Option<String> = None;
    let mut toggled = false;
    for p in &mut reg.plugins {
        let (rect, resp) = ui.allocate_exact_size(Vec2::new(w, 60.0), Sense::click());
        if resp.hovered() {
            ui.painter().rect_filled(rect, 0.0, t(WINDOW_OVER));
        }
        let painter = ui.painter();
        painter.text(
            Pos2::new(rect.left() + kit::PAD, rect.top() + 14.0),
            Align2::LEFT_CENTER,
            format!("{}  v{}", p.name, p.version),
            FontId::proportional(14.0),
            WHITE,
        );
        let desc_clip = Rect::from_min_max(
            Pos2::new(rect.left() + kit::PAD, rect.top() + 26.0),
            Pos2::new(rect.right() - 80.0, rect.bottom()),
        );
        painter.with_clip_rect(desc_clip).text(
            Pos2::new(rect.left() + kit::PAD, rect.top() + 36.0),
            Align2::LEFT_CENTER,
            &p.description,
            FontId::proportional(12.0),
            SUBTEXT,
        );
        let sw_c = Pos2::new(rect.right() - kit::PAD - 17.0, rect.center().y);
        let track = Rect::from_center_size(sw_c, Vec2::new(34.0, 14.0));
        painter.rect_filled(track, CornerRadius::same(7), if p.enabled { t_btn(ORANGE) } else { t_btn(navy(0x4b5866)) });
        let kx = if p.enabled { track.right() - 7.0 } else { track.left() + 7.0 };
        painter.circle_filled(Pos2::new(kx, track.center().y), 6.0, Color32::WHITE);
        if !p.builtin {
            let del_r = Rect::from_center_size(Pos2::new(track.left() - 22.0, rect.center().y), Vec2::splat(26.0));
            let dr = ui.interact(del_r, ui.make_persistent_id(("delplg", &p.id)), Sense::click());
            icons::tg(ui.painter(), "menu/delete", del_r.center(), if dr.hovered() { DANGER } else { SUBTEXT });
            if dr.on_hover_text("Удалить плагин").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                remove_id = Some(p.id.clone());
            }
        }
        if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            p.enabled = !p.enabled;
            toggled = true;
        }
    }
    if let Some(id) = remove_id {
        reg.remove(&id);
        let _ = std::fs::remove_file(worker::home().join("plugins").join(format!("{id}.plugin")));
        toggled = true;
    }
    if toggled {
        s.disabled_plugins = reg.disabled_ids();
    }

    ui.add_space(8.0);
    if !st.new_plugin_open {
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            if orange_button(ui, "Создать / установить .plugin", Vec2::new(220.0, 34.0)).clicked() {
                st.new_plugin_open = true;
                if st.new_plugin_code.is_empty() {
                    st.new_plugin_code = "__id__ = \"user.greeting\"\n__name__ = \"Мои команды\"\n__version__ = \"1.0\"\n# cmd: /gm => \"Доброе утро! ☀️ {args}\"\n# replace: \":heart:\" => \"❤️\"".to_owned();
                }
            }
        });
    } else {
        section(ui, "Код плагина (.plugin)");
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            ui.add_sized(
                [w - 2.0 * kit::PAD, 110.0],
                egui::TextEdit::multiline(&mut st.new_plugin_code)
                    .code_editor()
                    .desired_rows(5),
            );
        });
        if !st.new_plugin_err.is_empty() {
            ui.horizontal(|ui| {
                ui.add_space(kit::PAD);
                ui.colored_label(DANGER, &st.new_plugin_err);
            });
        }
        ui.add_space(6.0);
        ui.horizontal(|ui| {
            ui.add_space(kit::PAD);
            if orange_button(ui, "Установить плагин", Vec2::new(165.0, 32.0)).clicked() {
                match rg_ayu::plugins::parse_plugin(&st.new_plugin_code) {
                    Ok(p) => {
                        let dir = worker::home().join("plugins");
                        let _ = std::fs::create_dir_all(&dir);
                        let _ = std::fs::write(dir.join(format!("{}.plugin", p.id)), &st.new_plugin_code);
                        reg.upsert(p);
                        st.new_plugin_err.clear();
                        st.new_plugin_open = false;
                    }
                    Err(e) => st.new_plugin_err = e,
                }
            }
            ui.add_space(8.0);
            if ui.button("Отмена").clicked() {
                st.new_plugin_open = false;
                st.new_plugin_err.clear();
            }
        });
    }
    ui.add_space(8.0);
    if nav_row(ui, "menu/show_in_folder", "Открыть папку плагинов", "") {
        let dir = worker::home().join("plugins");
        let _ = std::fs::create_dir_all(&dir);
        rg_core::media::open_downloaded(&dir);
    }

    divider(ui);
    section(ui, "Настройки телефона (exteraGram / Android)");
    toggle(ui, "Иконки Solar и квадратная FAB-кнопка", &mut s.extera.use_solar_icons);
    toggle(ui, "Центрировать заголовок шапки", &mut s.extera.center_title);
    toggle(ui, "Скрыть статус в шапке", &mut s.extera.hide_action_bar_status);
    toggle(ui, "Скрывать клавиатуру при прокрутке чата", &mut s.extera.hide_keyboard_on_scroll);
    toggle(ui, "Ускорение загрузки медиа (Speed Boost)", &mut s.extera.upload_speed_boost);
    toggle(ui, "Показывать секунды во времени", &mut s.extera.format_time_with_seconds);
    toggle(ui, "Архив по свайпу вниз", &mut s.extera.archive_on_pull);
}

impl App {
    /// Модальное окно «Мой профиль» (как в AyuGram на скрине 10 — поверх текущего чата, без перехода в «Избранное»).
    #[allow(deprecated)]
    pub(crate) fn my_profile_window(&mut self, ctx: &egui::Context) {
        let tw = tween(ctx, "my-prof-win", f32::from(u8::from(self.tgs.profile_open)), 0.24);
        let elapsed_k = self
            .tgs
            .profile_at
            .map_or(1.0, |t| (t.elapsed().as_secs_f32() / 0.24).clamp(0.0, 1.0));
        if elapsed_k < 1.0 && self.tgs.profile_open {
            ctx.request_repaint();
        }
        let raw = if self.tgs.profile_open { tw.min(elapsed_k) } else { tw };
        if raw < 0.005 && !self.tgs.profile_open {
            return;
        }
        let appear = ios_spring(raw);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("my-prof-dim"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let (rect, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                paint_frosted_scrim(ui.painter(), rect, raw);
                if resp.clicked() {
                    self.tgs.profile_open = false;
                }
            });

        let me_info = self.tgs.account.as_ref().map(|a| a.me.clone()).unwrap_or_default();
        let name = if me_info.name.is_empty() { self.me.clone() } else { me_info.name.clone() };
        let me_tex = self.dialogs.iter().find(|d| d.saved).and_then(|d| self.avatars.get(&d.id));
        let corners = self.settings.ayu.avatar_corners;
        let streamer = self.settings.ayu.streamer_mode;
        let cur_audio_title = self.audio_title.clone();
        let cur_audio_playing = self.audio_is_playing();
        let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape) || i.pointer.button_clicked(egui::PointerButton::Extra1));
        let mut edit_account = false;
        let mut show_qr = false;
        let mut play_track: Option<rg_core::engine::AudioTrack> = None;
        let mut toggle_audio = false;

        egui::Area::new(egui::Id::new("my-prof-box"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_CENTER, Vec2::new(0.0, (1.0 - appear) * 32.0))
            .show(ctx, |ui| {
                ui.set_opacity(appear.clamp(0.02, 1.0));
                let scale = 0.94 + 0.06 * appear;
                let card = egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .stroke(Stroke::new(1.0, Color32::from_white_alpha((32.0 * opacity_f32()) as u8)))
                    .corner_radius(CornerRadius::same(14))
                    .show(ui, |ui| {
                        let width = (376.0 * scale).min(screen.width() - 16.0);
                        ui.set_width(width);
                        ui.spacing_mut().item_spacing = Vec2::ZERO;

                        // Верхняя панель: аватар по центру, карандаш редактирования и крестик справа (1 в 1 со скрином 10 AyuGram!).
                        let (top, _) = ui.allocate_exact_size(Vec2::new(width, 168.0), Sense::hover());
                        let ed_r = Rect::from_center_size(Pos2::new(top.right() - 58.0, top.top() + 24.0), Vec2::splat(32.0));
                        let ed_resp = ui.interact(ed_r, ui.make_persistent_id("mp-edit"), Sense::click());
                        icons::tg(ui.painter(), "info/info_edit", ed_r.center(), if ed_resp.hovered() { WHITE } else { SUBTEXT });
                        if ed_resp.on_hover_text("Изменить профиль").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            edit_account = true;
                        }
                        let x_r = Rect::from_center_size(Pos2::new(top.right() - 24.0, top.top() + 24.0), Vec2::splat(32.0));
                        let x_resp = ui.interact(x_r, ui.make_persistent_id("mp-close"), Sense::click());
                        icons::tg(ui.painter(), "info/info_close", x_r.center(), if x_resp.hovered() { WHITE } else { SUBTEXT });
                        if x_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            close = true;
                        }

                        draw_avatar(ui, Pos2::new(top.center().x, top.top() + 58.0), &name, 1, 82.0, me_tex, corners);
                        let p = ui.painter();
                        let ng = p.layout_no_wrap(name.clone(), FontId::proportional(17.5), WHITE);
                        let has_badge = me_info.emoji_status.is_some() || me_info.verified || me_info.premium;
                        let nx = top.center().x - if has_badge { 10.0 } else { 0.0 };
                        p.text(Pos2::new(nx, top.top() + 120.0), Align2::CENTER_CENTER, &name, FontId::proportional(17.5), WHITE);
                        if has_badge {
                            let bc = Pos2::new(nx + ng.size().x * 0.5 + 12.0, top.top() + 120.0);
                            if let Some(doc_id) = me_info.emoji_status
                                && let Some(tex) = self.sticker_tex.get(&doc_id)
                            {
                                egui::Image::new(tex)
                                    .fit_to_exact_size(Vec2::splat(17.0))
                                    .paint_at(ui, Rect::from_center_size(bc, Vec2::splat(17.0)));
                            } else {
                                icons::tg(ui.painter(), "dialogs/dialogs_premium", bc, ORANGE);
                            }
                        }
                        p.text(
                            Pos2::new(top.center().x, top.top() + 144.0),
                            Align2::CENTER_CENTER,
                            if me_info.status.is_empty() { "в сети" } else { &me_info.status },
                            FontId::proportional(13.0),
                            SUBTEXT,
                        );

                        // Музыка в вашем профиле (если добавлена).
                        if !me_info.saved_music.is_empty() {
                            for track in me_info.saved_music.iter().take(2) {
                                let title = track.display_title();
                                let is_cur = cur_audio_title == title && !title.is_empty();
                                let (mr, mresp) = ui.allocate_exact_size(Vec2::new(width, 44.0), Sense::click());
                                let c_rect = Rect::from_min_max(mr.min + Vec2::new(14.0, 2.0), mr.max - Vec2::new(14.0, 2.0));
                                ui.painter().rect_filled(
                                    c_rect,
                                    CornerRadius::same(10),
                                    if is_cur { t_btn(ORANGE).gamma_multiply(0.28) } else { t(WINDOW_OVER) },
                                );
                                let ic_r = Rect::from_min_size(c_rect.min + Vec2::new(6.0, 5.0), Vec2::splat(30.0));
                                ui.painter().rect_filled(ic_r, CornerRadius::same(7), t_btn(ORANGE));
                                icons::tg_sized(
                                    ui.painter(),
                                    if is_cur && cur_audio_playing { "media_pause" } else { "media_play" },
                                    ic_r.center(),
                                    Vec2::splat(14.0),
                                    Color32::WHITE,
                                );
                                ui.painter().text(
                                    Pos2::new(ic_r.right() + 10.0, c_rect.center().y),
                                    Align2::LEFT_CENTER,
                                    &title,
                                    FontId::proportional(13.0),
                                    WHITE,
                                );
                                if mresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    if is_cur {
                                        toggle_audio = true;
                                    } else {
                                        play_track = Some(track.clone());
                                    }
                                }
                            }
                            ui.add_space(6.0);
                        }

                        divider(ui);

                        // Поля профиля (как на скрине 10 справа: Mobile, Bio, @username + QR, ID + DC).
                        let field_row = |ui: &mut egui::Ui, val: &str, lbl: &str, val_color: Color32, with_qr: bool| -> bool {
                            if val.is_empty() {
                                return false;
                            }
                            let g = ui.painter().layout(val.to_owned(), FontId::proportional(14.0), val_color, (width - 64.0).max(100.0));
                            let h = (g.size().y + 32.0).max(52.0);
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(width, h), Sense::click());
                            if resp.hovered() {
                                ui.painter().rect_filled(r, 0.0, t(WINDOW_OVER));
                            }
                            ui.painter().galley(Pos2::new(r.left() + 20.0, r.top() + 9.0), g.clone(), val_color);
                            ui.painter().text(
                                Pos2::new(r.left() + 20.0, r.top() + 13.0 + g.size().y + 7.0),
                                Align2::LEFT_CENTER,
                                lbl,
                                FontId::proportional(12.0),
                                SUBTEXT,
                            );
                            let mut qr_clicked = false;
                            if with_qr {
                                let qr_box = Rect::from_center_size(Pos2::new(r.right() - 28.0, r.center().y), Vec2::splat(32.0));
                                let qresp = ui.interact(qr_box, ui.make_persistent_id("mp-qr"), Sense::click());
                                icons::tg(ui.painter(), "menu/qr_code", qr_box.center(), if qresp.hovered() { WHITE } else { ORANGE });
                                if qresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    qr_clicked = true;
                                }
                            } else if resp.on_hover_text("Нажмите, чтобы скопировать").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                ui.ctx().copy_text(val.to_owned());
                            }
                            qr_clicked
                        };

                        if !me_info.phone.is_empty() {
                            field_row(ui, &mask_streamer(&format!("+{}", me_info.phone), streamer), "Мобильный", WHITE, false);
                        }
                        if !me_info.about.is_empty() {
                            field_row(ui, &me_info.about, "О себе", WHITE, false);
                        }
                        if !me_info.username.is_empty()
                            && field_row(ui, &mask_streamer(&format!("@{}", me_info.username), streamer), "Имя пользователя", ORANGE, true)
                        {
                            show_qr = true;
                        }
                        if !me_info.birthday.is_empty() {
                            field_row(ui, &me_info.birthday, "День рождения", WHITE, false);
                        }
                        if me_info.id != 0 {
                            field_row(ui, &mask_streamer(&me_info.id.to_string(), streamer), "DC · ID аккаунта", ORANGE, false);
                        }

                        divider(ui);
                        let (foot, _) = ui.allocate_exact_size(Vec2::new(width, 62.0), Sense::hover());
                        ui.painter().text(
                            foot.center(),
                            Align2::CENTER_CENTER,
                            "Здесь отображаются ваши истории и музыка профиля.",
                            FontId::proportional(12.5),
                            SUBTEXT,
                        );
                    });
                paint_modal_subblur(ui.painter(), card.response.rect, 14.0, raw);
            });

        if let Some(tr) = play_track {
            self.send(Cmd::PlayTrack(tr));
        }
        if toggle_audio {
            self.toggle_audio_playback();
        }
        if edit_account {
            self.tgs.profile_open = false;
            self.open_tg_settings();
            self.tgs.page = pg::ACCOUNT;
        } else if show_qr {
            self.tgs.profile_open = false;
            self.open_tg_settings();
            self.tgs.qr_open = true;
        } else if close {
            self.tgs.profile_open = false;
        }
    }
}
