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
    /// Прервать идущую загрузку вложения.
    CancelFetch(i32),
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
    /// Перевести текст сообщения встроенным переводчиком Telegram.
    Translate(i32, String),
    /// Сохранить карточку-снимок сообщения (Message Shot, функция AyuGram).
    Shot(i32),
    /// Проголосовать в опросе (идентификатор сообщения, байты варианта).
    VotePoll(i32, Vec<u8>),
    /// Выбрать / снять выделение сообщения (мультивыбор для пересылки и снимка).
    ToggleSelect(i32),
    /// Открыть ветку комментариев к посту канала.
    Discuss(i32),
    /// Фильтровать ленту по конкретному отправителю.
    FilterUser(i64, String),
}

/// Всё, что нужно для рисования пузыря, кроме самого сообщения.
struct Env<'a> {
    avail: f32,
    s: &'a AyuSettings,
    tex: &'a HashMap<i32, egui::TextureHandle>,
    shift: bool,
    scroll_to: Option<i32>,
    group: bool,
    is_channel: bool,
    has_discussion: bool,
    format_secs: bool,
    double_tap_action: i32,
    arrived: &'a HashMap<i32, f64>,
    img_arrived: &'a HashMap<i32, f64>,
    time: f64,
    emoji: Option<&'a super::emoji::Emoji>,
    spoilers: &'a std::collections::HashSet<(i32, usize)>,
    read_out: i32,
    avatars: &'a HashMap<i64, egui::TextureHandle>,
    /// Идущие загрузки вложений: сообщение → (скачано, всего).
    fetching: &'a HashMap<i32, (u64, u64)>,
    /// Вложения, уже лежащие в кэше.
    cached: &'a std::collections::HashSet<i32>,
    /// Скачанные GIF (путь на диске) для автозапуска в ленте.
    gif_paths: &'a HashMap<i32, std::path::PathBuf>,
    /// Переведённые тексты сообщений.
    translated: &'a HashMap<i32, String>,
    /// Текстуры пользовательских Premium-эмодзи (`document_id` -> текстура).
    custom_tex: &'a HashMap<i64, egui::TextureHandle>,
    /// Показывать кнопку «Перевести» в контекстном меню (из настроек языка).
    show_translate: bool,
    /// Видимая область ленты (для аккуратного размещения встроенного GIF).
    clip_rect: Rect,
    /// Выбранные сообщения (режим множественного выделения).
    selected_msgs: &'a std::collections::HashSet<i32>,
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
                let info = self.peer_infos.get(&d.id);
                let live_typing = self
                    .typing
                    .get(&d.id)
                    .filter(|(_, at)| at.elapsed().as_secs() < 6)
                    .map(|(lbl, _)| lbl.clone());
                let hide_status = self.settings.extera.hide_action_bar_status;
                let sub = if let Some(lbl) = &live_typing {
                    lbl.clone()
                } else if hide_status {
                    String::new()
                } else if d.saved {
                    "Сохраняйте важные сообщения".to_owned()
                } else if let Some(inf) = info {
                    if inf.members > 0 {
                        if inf.broadcast {
                            format!("{} подписчиков", inf.members)
                        } else {
                            format!("{} участников", inf.members)
                        }
                    } else if !inf.status.is_empty() {
                        inf.status.clone()
                    } else {
                        String::new()
                    }
                } else {
                    match d.kind {
                        rg_core::engine::DialogKind::Bot => "бот".to_owned(),
                        rg_core::engine::DialogKind::Channel => "канал".to_owned(),
                        rg_core::engine::DialogKind::Group => "группа".to_owned(),
                        rg_core::engine::DialogKind::User => String::new(),
                    }
                };
                let id_note = match self.settings.ayu.show_peer_id {
                    rg_settings::PeerIdDisplay::Hidden => String::new(),
                    rg_settings::PeerIdDisplay::BotApi => format!("ID {}", d.id),
                    rg_settings::PeerIdDisplay::Telegram => {
                        let raw = d.id.unsigned_abs() % 1_000_000_000_000;
                        format!("ID {raw}")
                    }
                };
                let subtitle = match (sub.is_empty(), id_note.is_empty()) {
                    (false, false) => format!("{sub} · {id_note}"),
                    (false, true) => sub,
                    (true, false) => id_note,
                    (true, true) => String::new(),
                };
                let online = live_typing.is_none() && subtitle.starts_with("в сети");
                let sub_color = if live_typing.is_some() {
                    ORANGE
                } else if online {
                    GREEN
                } else {
                    SUBTEXT
                };
                let title_str = if d.saved { "Избранное" } else { d.name.as_str() };
                let show_prem = !self.settings.ayu.hide_premium_statuses;
                let emoji_id = if show_prem {
                    d.emoji_status.or_else(|| info.and_then(|x| x.emoji_status))
                } else {
                    None
                };
                let verified = d.verified || info.is_some_and(|x| x.verified);
                let premium = show_prem && (d.premium || info.is_some_and(|x| x.premium));
                let hdr_w = (ui.available_width() - 150.0).max(160.0);
                let (hrect, hresp) = ui.allocate_exact_size(Vec2::new(hdr_w, TOP_BAR_HEIGHT - 4.0), Sense::click());
                draw_avatar(
                    ui,
                    Pos2::new(hrect.left() + 20.0, hrect.center().y),
                    title_str,
                    d.id,
                    40.0,
                    self.avatars.get(&d.id),
                    self.settings.ayu.avatar_corners,
                );
                let text_x = hrect.left() + 50.0;
                let title_y = if subtitle.is_empty() { hrect.center().y } else { hrect.center().y - 9.0 };
                let p = ui.painter();
                let title_galley = p.layout_no_wrap(title_str.to_owned(), FontId::proportional(15.5), WHITE);
                let title_w = title_galley.size().x.min((hrect.right() - text_x - 24.0).max(40.0));
                let centered = self.settings.extera.center_title;
                let title_start_x = if centered {
                    (hrect.center().x - title_w * 0.5).max(text_x)
                } else {
                    text_x
                };
                let clip_t = Rect::from_min_max(
                    Pos2::new(title_start_x, hrect.top()),
                    Pos2::new(title_start_x + title_w, hrect.bottom()),
                );
                p.with_clip_rect(clip_t).text(
                    Pos2::new(title_start_x, title_y),
                    egui::Align2::LEFT_CENTER,
                    title_str,
                    FontId::proportional(15.5),
                    WHITE,
                );
                // Значок Premium-эмодзи статуса / галочки / Premium прямо рядом с именем (как в AyuGram!).
                let badge_c = Pos2::new(title_start_x + title_w + 11.0, title_y);
                if let Some(doc_id) = emoji_id {
                    if let Some(tex) = self.sticker_tex.get(&doc_id) {
                        egui::Image::new(tex)
                            .fit_to_exact_size(Vec2::splat(16.0))
                            .paint_at(ui, Rect::from_center_size(badge_c, Vec2::splat(16.0)));
                    } else {
                        icons::tg(ui.painter(), "dialogs/dialogs_premium", badge_c, ORANGE);
                    }
                } else if verified {
                    icons::tg(ui.painter(), "dialogs/dialogs_verified_star", badge_c, ORANGE);
                } else if premium {
                    icons::tg(ui.painter(), "dialogs/dialogs_premium", badge_c, ORANGE);
                }
                if !subtitle.is_empty() {
                    let clip_s = Rect::from_min_max(Pos2::new(text_x, hrect.top()), Pos2::new(hrect.right(), hrect.bottom()));
                    if centered {
                        ui.painter().with_clip_rect(clip_s).text(
                            Pos2::new(hrect.center().x.max(text_x + 40.0), hrect.center().y + 10.0),
                            egui::Align2::CENTER_CENTER,
                            subtitle,
                            FontId::proportional(12.5),
                            sub_color,
                        );
                    } else {
                        ui.painter().with_clip_rect(clip_s).text(
                            Pos2::new(text_x, hrect.center().y + 10.0),
                            egui::Align2::LEFT_CENTER,
                            subtitle,
                            FontId::proportional(12.5),
                            sub_color,
                        );
                    }
                }
                if hresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.info_open = !self.info_open;
                }
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
                        if kit::menu_item(ui, "menu/edited_status", "Удалённые и правки", false) {
                            act = Some(Cmd::SavedHistory);
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
                    if matches!(cmd, Cmd::ForceRead)
                        && let Some(i) = self.selected
                        && let Some(d) = self.dialogs.get_mut(i)
                    {
                        d.unread = 0;
                    }
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

    /// Правая панель профиля (как в AyuGram Desktop): шапка, быстрые действия, сведения,
    /// общие медиа/файлы/ссылки и список участников с добавлением новых людей.
    pub(crate) fn info_ui(&mut self, ui: &mut egui::Ui) {
        let Some((i, d)) = self.selected.and_then(|i| self.dialogs.get(i).map(|d| (i, d.clone()))) else { return };
        if self.info_req.insert(d.id) {
            self.send(Cmd::PeerInfo(i));
        }
        let is_group = d.id < 0;
        if is_group && self.participants_req.insert(d.id) {
            self.send(Cmd::Participants(i));
        }
        let info = self.peer_infos.get(&d.id).cloned();
        let members_list = self.participants.get(&d.id).cloned().unwrap_or_default();
        let w = ui.available_width();
        ui.set_clip_rect(ui.max_rect());
        let (head, _) = ui.allocate_exact_size(Vec2::new(w, 54.0), Sense::hover());
        ui.painter().text(
            head.left_center() + Vec2::new(18.0, 0.0),
            egui::Align2::LEFT_CENTER,
            if is_group {
                if info.as_ref().is_some_and(|x| x.broadcast) { "Канал" } else { "Группа" }
            } else {
                "Профиль"
            },
            FontId::proportional(16.0),
            WHITE,
        );
        let x = Rect::from_center_size(head.right_center() - Vec2::new(26.0, 0.0), Vec2::splat(36.0));
        let xr = ui.interact(x, ui.make_persistent_id("info-close"), Sense::click());
        icons::tg(ui.painter(), "info/info_close", x.center(), if xr.hovered() { WHITE } else { SUBTEXT });
        if xr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.info_open = false;
        }
        let mut open_contact: Option<(grammers_session::types::PeerRef, i64, String)> = None;
        let mut open_viewer: Option<i32> = None;
        let mut open_save: Option<i32> = None;
        let mut open_url: Option<String> = None;
        let mut cmd_after: Option<Cmd> = None;
        let mut play_track: Option<rg_core::engine::AudioTrack> = None;
        let mut toggle_audio = false;
        let mut start_invite = false;
        let mut start_search = false;
        let mut info_tab = self.info_tab;
        let cur_audio_title = self.audio_title.clone();
        let cur_audio_playing = self.audio_is_playing();

        egui::ScrollArea::vertical().auto_shrink([false; 2]).show(ui, |ui| {
            ui.spacing_mut().item_spacing = Vec2::ZERO;
            let (top, _) = ui.allocate_exact_size(Vec2::new(w, 160.0), Sense::hover());
            let c = Pos2::new(top.center().x, top.top() + 50.0);
            let r = Rect::from_center_size(c, Vec2::splat(84.0));
            match self.avatars.get(&d.id) {
                Some(t) => {
                    egui::Image::new(t).fit_to_exact_size(r.size()).corner_radius(42.0).paint_at(ui, r);
                }
                None => {
                    ui.painter().circle_filled(c, 42.0, avatar_color(d.id));
                    ui.painter().text(
                        c,
                        egui::Align2::CENTER_CENTER,
                        d.name.chars().next().map(|ch| ch.to_uppercase().to_string()).unwrap_or_default(),
                        FontId::proportional(34.0),
                        Color32::WHITE,
                    );
                }
            }
            let pname = if d.saved { "Избранное" } else { d.name.as_str() };
            let name_galley = ui.painter().layout_no_wrap(pname.to_owned(), FontId::proportional(17.5), WHITE);
            let show_prem = !self.settings.ayu.hide_premium_statuses;
            let emoji_id = if show_prem {
                info.as_ref().and_then(|x| x.emoji_status).or(d.emoji_status)
            } else {
                None
            };
            let verified = info.as_ref().is_some_and(|x| x.verified) || d.verified;
            let premium = show_prem && (info.as_ref().is_some_and(|x| x.premium) || d.premium);
            let has_badge = emoji_id.is_some() || verified || premium;
            let name_x = top.center().x - if has_badge { 10.0 } else { 0.0 };
            ui.painter().text(
                Pos2::new(name_x, top.top() + 114.0),
                egui::Align2::CENTER_CENTER,
                pname,
                FontId::proportional(17.5),
                WHITE,
            );
            if has_badge {
                let bc = Pos2::new(name_x + name_galley.size().x * 0.5 + 12.0, top.top() + 114.0);
                if let Some(doc_id) = emoji_id {
                    if let Some(tex) = self.sticker_tex.get(&doc_id) {
                        egui::Image::new(tex)
                            .fit_to_exact_size(Vec2::splat(17.0))
                            .paint_at(ui, Rect::from_center_size(bc, Vec2::splat(17.0)));
                    } else {
                        icons::tg(ui.painter(), "dialogs/dialogs_premium", bc, ORANGE);
                    }
                } else if verified {
                    icons::tg(ui.painter(), "dialogs/dialogs_verified_star", bc, ORANGE);
                } else if premium {
                    icons::tg(ui.painter(), "dialogs/dialogs_premium", bc, ORANGE);
                }
            }
            let sub = info
                .as_ref()
                .map(|x| {
                    if x.members > 0 {
                        if x.broadcast { format!("{} подписчиков", x.members) } else { format!("{} участников", x.members) }
                    } else {
                        x.status.clone()
                    }
                })
                .unwrap_or_default();
            let sub_c = if sub == "в сети" { GREEN } else { SUBTEXT };
            ui.painter().text(Pos2::new(top.center().x, top.top() + 138.0), egui::Align2::CENTER_CENTER, sub, FontId::proportional(13.0), sub_c);

            // Полоса быстрых действий профиля (как в AyuGram/iOS Telegram).
            let blocked = info.as_ref().is_some_and(|x| x.blocked);
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                let actions: Vec<(&str, &str, u8)> = if is_group {
                    vec![
                        (if d.muted { "menu/unmute" } else { "menu/mute" }, if d.muted { "Звук" } else { "Тихо" }, 1),
                        ("menu/search", "Поиск", 2),
                        ("menu/add", "Люди", 3),
                        ("menu/link", "Ссылка", 4),
                    ]
                } else {
                    vec![
                        (if d.muted { "menu/unmute" } else { "menu/mute" }, if d.muted { "Звук" } else { "Тихо" }, 1),
                        ("menu/search", "Поиск", 2),
                        ("menu/block", if blocked { "Разблок" } else { "Блок" }, 5),
                    ]
                };
                let bw = ((w - 24.0 - (actions.len() as f32 - 1.0) * 6.0) / actions.len() as f32).max(56.0);
                for (k, (ico, lbl, code)) in actions.iter().enumerate() {
                    if k > 0 {
                        ui.add_space(6.0);
                    }
                    let (br, bresp) = ui.allocate_exact_size(Vec2::new(bw, 52.0), Sense::click());
                    let h = tween(ui.ctx(), ("pqa", d.id, *code), f32::from(u8::from(bresp.hovered())), 0.12);
                    let fill = lerp_color(t(WINDOW_OVER), t_btn(ORANGE), h * 0.45);
                    ui.painter().rect_filled(br, egui::CornerRadius::same(10), fill);
                    let col = if *code == 5 && blocked { DANGER } else if h > 0.4 { Color32::WHITE } else { ORANGE };
                    icons::tg(ui.painter(), ico, Pos2::new(br.center().x, br.top() + 18.0), col);
                    ui.painter().text(
                        Pos2::new(br.center().x, br.bottom() - 12.0),
                        egui::Align2::CENTER_CENTER,
                        *lbl,
                        FontId::proportional(11.5),
                        WHITE,
                    );
                    if bresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        match code {
                            1 => cmd_after = Some(Cmd::MuteDialog(i, !d.muted)),
                            2 => start_search = true,
                            3 => start_invite = true,
                            4 => cmd_after = Some(Cmd::ExportInvite(i)),
                            5 => cmd_after = Some(Cmd::BlockPeer(i, !blocked)),
                            _ => {}
                        }
                    }
                }
            });
            ui.add_space(10.0);
            // Музыка в профиле (Saved Music, 1 в 1 как на скринах 10–12 AyuGram под быстрыми кнопками!).
            if let Some(info) = &info
                && !info.saved_music.is_empty()
            {
                for track in info.saved_music.iter().take(4) {
                    let title = track.display_title();
                    let is_cur = cur_audio_title == title && !title.is_empty();
                    let (mr, mresp) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::click());
                    let card = Rect::from_min_max(mr.min + Vec2::new(10.0, 2.0), mr.max - Vec2::new(10.0, 2.0));
                    let fill = if is_cur {
                        t_btn(ORANGE).gamma_multiply(0.28)
                    } else if mresp.hovered() {
                        t(WINDOW_OVER)
                    } else {
                        Color32::from_white_alpha((12.0 * opacity_f32()) as u8)
                    };
                    let p = ui.painter();
                    p.rect_filled(card, egui::CornerRadius::same(10), fill);
                    let ic_r = Rect::from_min_size(card.min + Vec2::new(6.0, 6.0), Vec2::splat(30.0));
                    p.rect_filled(ic_r, egui::CornerRadius::same(7), if is_cur { t_btn(ORANGE) } else { t(BUTTON) });
                    let ico_name = if is_cur && cur_audio_playing {
                        "media_pause"
                    } else {
                        "media_play"
                    };
                    icons::tg_sized(p, ico_name, ic_r.center(), Vec2::splat(14.0), Color32::WHITE);
                    let clip = Rect::from_min_max(Pos2::new(ic_r.right() + 8.0, card.top()), Pos2::new(card.right() - 8.0, card.bottom()));
                    p.with_clip_rect(clip).text(
                        Pos2::new(ic_r.right() + 8.0, card.center().y - if track.duration > 0 { 6.0 } else { 0.0 }),
                        egui::Align2::LEFT_CENTER,
                        &title,
                        FontId::proportional(13.0),
                        if is_cur { ORANGE } else { WHITE },
                    );
                    if track.duration > 0 {
                        p.with_clip_rect(clip).text(
                            Pos2::new(ic_r.right() + 8.0, card.center().y + 9.0),
                            egui::Align2::LEFT_CENTER,
                            super::media_ui::mmss(track.duration),
                            FontId::proportional(11.0),
                            SUBTEXT,
                        );
                    }
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
            kit::divider(ui);

            if let Some(info) = &info {
                let row = |ui: &mut egui::Ui, icon: &str, value: &str, label: &str, copy: bool| {
                    if value.is_empty() {
                        return;
                    }
                    let galley = ui.painter().layout(value.to_owned(), FontId::proportional(14.0), WHITE, (w - 80.0).max(80.0));
                    let h = (galley.size().y + 34.0).max(52.0);
                    let (r, resp) = ui.allocate_exact_size(Vec2::new(w, h), Sense::click());
                    if resp.hovered() {
                        ui.painter().rect_filled(r, 0.0, t(WINDOW_OVER));
                    }
                    icons::tg(ui.painter(), icon, Pos2::new(r.left() + 28.0, r.top() + 24.0), DIALOG_DATE);
                    ui.painter().galley(Pos2::new(r.left() + 58.0, r.top() + 10.0), galley.clone(), WHITE);
                    ui.painter().text(
                        Pos2::new(r.left() + 58.0, r.top() + 12.0 + galley.size().y + 8.0),
                        egui::Align2::LEFT_CENTER,
                        label,
                        FontId::proportional(12.0),
                        SUBTEXT,
                    );
                    if copy && resp.on_hover_text("Нажмите, чтобы скопировать").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        ui.ctx().copy_text(value.to_owned());
                    }
                };
                let streamer = self.settings.ayu.streamer_mode;
                let mask_phone = streamer || self.settings.extera.hide_phone_number;
                row(
                    ui,
                    "menu/phone",
                    &if info.phone.is_empty() {
                        String::new()
                    } else {
                        mask_streamer(&format!("+{}", info.phone), mask_phone)
                    },
                    "Телефон",
                    !mask_phone,
                );
                row(ui, "settings/mention", &if info.username.is_empty() { String::new() } else { mask_streamer(&format!("@{}", info.username), streamer) }, "Имя пользователя", !streamer);
                row(ui, "menu/info", &info.about, if is_group { "Описание" } else { "О себе" }, true);
                if !info.birthday.is_empty() {
                    row(ui, "settings/gift", &info.birthday, "День рождения", true);
                }
                if !info.business_hours.is_empty() {
                    row(ui, "settings/timer", &info.business_hours, "Часы работы", false);
                }
                if !info.invite_link.is_empty() {
                    row(ui, "menu/link", &info.invite_link, "Ссылка-приглашение", true);
                }
                row(ui, "menu/profile", &mask_streamer(&format!("{}", d.id), streamer), "ID", !streamer);
            } else {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("Загрузка сведений…").color(SUBTEXT)));
            }

            kit::divider(ui);
            // Вкладки общих материалов и участников (Медиа / Файлы / Ссылки / Участники).
            let tabs: &[(&str, usize)] = if is_group {
                &[("Медиа", 0), ("Файлы", 1), ("Ссылки", 2), ("Люди", 3)]
            } else {
                &[("Медиа", 0), ("Файлы", 1), ("Ссылки", 2)]
            };
            if info_tab >= tabs.len() {
                info_tab = 0;
            }
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 0.0;
                let tw = w / tabs.len() as f32;
                for (name, tid) in tabs {
                    let (tr, tresp) = ui.allocate_exact_size(Vec2::new(tw, 36.0), Sense::click());
                    let k = tween(ui.ctx(), ("inftab", *tid), f32::from(u8::from(info_tab == *tid)), 0.14);
                    ui.painter().text(tr.center(), egui::Align2::CENTER_CENTER, *name, FontId::proportional(13.0), lerp_color(SUBTEXT, WHITE, k));
                    ui.painter().rect_filled(
                        Rect::from_center_size(Pos2::new(tr.center().x, tr.bottom() - 1.5), Vec2::new((tw - 14.0) * k, 2.5)),
                        egui::CornerRadius::same(1),
                        t_btn(ORANGE),
                    );
                    if tresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        info_tab = *tid;
                    }
                }
            });
            ui.add_space(6.0);

            match info_tab {
                0 => {
                    // Сетка 3xN миниатюр фото и видео из чата.
                    let media_ids: Vec<i32> = self
                        .messages
                        .iter()
                        .rev()
                        .filter(|m| {
                            m.media.as_ref().is_some_and(|x| {
                                matches!(x.kind, rg_core::media::MediaKind::Photo | rg_core::media::MediaKind::Video | rg_core::media::MediaKind::Gif)
                            })
                        })
                        .map(|m| m.id)
                        .collect();
                    if media_ids.is_empty() {
                        ui.add_space(16.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("Нет медиа в загруженной истории").size(12.5).color(SUBTEXT)));
                    } else {
                        let cols = 3usize;
                        let cell = (w - 16.0) / cols as f32;
                        for row_ids in media_ids.chunks(cols) {
                            let (rr, _) = ui.allocate_exact_size(Vec2::new(w, cell), Sense::hover());
                            for (k, &mid) in row_ids.iter().enumerate() {
                                let cell_r = Rect::from_min_size(Pos2::new(rr.left() + 8.0 + k as f32 * cell, rr.top()), Vec2::splat(cell)).shrink(2.0);
                                let resp = ui.interact(cell_r, ui.make_persistent_id(("im", mid)), Sense::click());
                                match self.textures.get(&mid) {
                                    Some(t) => {
                                        egui::Image::new(t).fit_to_exact_size(cell_r.size()).corner_radius(6.0).paint_at(ui, cell_r);
                                    }
                                    None => {
                                        ui.painter().rect_filled(cell_r, egui::CornerRadius::same(6), t(WINDOW_OVER));
                                    }
                                }
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    open_viewer = Some(mid);
                                }
                            }
                        }
                    }
                }
                1 => {
                    let files: Vec<(i32, String, u64)> = self
                        .messages
                        .iter()
                        .rev()
                        .filter_map(|m| {
                            let inf = m.media.as_ref()?;
                            matches!(inf.kind, rg_core::media::MediaKind::File | rg_core::media::MediaKind::Audio)
                                .then(|| (m.id, inf.name.clone(), inf.size))
                        })
                        .collect();
                    if files.is_empty() {
                        ui.add_space(16.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("Нет файлов в загруженной истории").size(12.5).color(SUBTEXT)));
                    } else {
                        for (mid, name, sz) in files {
                            if kit::nav_row(ui, "menu/download", &short(&name, 26), &rg_core::media::human_size(sz)) {
                                open_save = Some(mid);
                            }
                        }
                    }
                }
                2 => {
                    let mut links: Vec<String> = Vec::new();
                    for m in self.messages.iter().rev() {
                        if let Some(rg_core::media::MediaInfo { kind: rg_core::media::MediaKind::WebPage { url, .. }, .. }) = &m.media
                            && !url.is_empty()
                        {
                            links.push(url.clone());
                        }
                        for e in &m.entities {
                            if e.kind == rg_core::engine::EntityKind::Link {
                                let u = e.url.clone().unwrap_or_else(|| m.text.chars().skip(e.start).take(e.end.saturating_sub(e.start)).collect());
                                if !u.is_empty() {
                                    links.push(u);
                                }
                            }
                        }
                    }
                    if links.is_empty() {
                        ui.add_space(16.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("Нет ссылок в загруженной истории").size(12.5).color(SUBTEXT)));
                    } else {
                        for u in links.into_iter().take(40) {
                            if kit::nav_row(ui, "menu/link", &short(&u, 32), "") {
                                open_url = Some(u);
                            }
                        }
                    }
                }
                _ => {
                    if members_list.is_empty() {
                        ui.add_space(16.0);
                        ui.vertical_centered(|ui| ui.label(RichText::new("Список участников недоступен или пуст").size(12.5).color(SUBTEXT)));
                    } else {
                        for c in &members_list {
                            let (mr, mresp) = ui.allocate_exact_size(Vec2::new(w, 48.0), Sense::click());
                            if mresp.hovered() {
                                ui.painter().rect_filled(mr, 0.0, t(WINDOW_OVER));
                            }
                            draw_avatar(ui, mr.left_center() + Vec2::new(30.0, 0.0), &c.name, c.id, 34.0, self.avatars.get(&c.id), self.settings.ayu.avatar_corners);
                            ui.painter().text(mr.left_top() + Vec2::new(56.0, 9.0), egui::Align2::LEFT_TOP, &c.name, FontId::proportional(13.5), WHITE);
                            ui.painter().text(mr.left_top() + Vec2::new(56.0, 27.0), egui::Align2::LEFT_TOP, &c.status, FontId::proportional(11.5), SUBTEXT);
                            if mresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                open_contact = Some((c.peer, c.id, c.name.clone()));
                            }
                        }
                    }
                }
            }
            ui.add_space(16.0);
        });

        self.info_tab = info_tab;
        if let Some(track) = play_track {
            self.send(Cmd::PlayTrack(track));
        }
        if toggle_audio {
            self.toggle_audio_playback();
        }
        if let Some(cmd) = cmd_after {
            self.send(cmd);
        }
        if start_search {
            self.search_open = true;
        }
        if start_invite {
            self.create_kind = 3;
            self.create_members.clear();
            self.create_member_query.clear();
            self.create_member_results.clear();
            self.create_member_map.clear();
            self.send(Cmd::Contacts);
        }
        if let Some((peer, id, name)) = open_contact {
            self.send(Cmd::OpenContact(peer, id, name));
        }
        if let Some(mid) = open_viewer {
            self.viewer = Some(mid);
            self.viewer_zoom = 1.0;
        }
        if let Some(mid) = open_save {
            self.send(Cmd::Download(mid));
        }
        if let Some(u) = open_url {
            self.apply(ui.ctx(), Act::OpenUrl(u));
        }
    }

    /// Строка поиска по чату под шапкой (без пустых зазоров в строке).
    pub(crate) fn search_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(10.0);
            let w = (ui.available_width() - 168.0).max(120.0);
            let r = ui.add_sized(
                [w, 30.0],
                egui::TextEdit::singleline(&mut self.chat_search)
                    .hint_text("Поиск сообщений")
                    .margin(Vec2::new(8.0, 6.0)),
            );
            r.request_focus();
            let go = orange_button(ui, "Найти", Vec2::new(72.0, 30.0)).clicked()
                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
            if go {
                self.send(Cmd::Search(self.chat_search.clone()));
            }
            if ui.add_sized([74.0, 30.0], egui::Button::new("Закрыть")).clicked() {
                self.search_open = false;
                self.chat_search.clear();
                self.found = None;
                self.send(Cmd::Search(String::new()));
            }
        });
    }

    /// Полоса перевода всего чата (появляется под шапкой, когда включено «Переводить чаты целиком»).
    pub(crate) fn translate_chat_bar(&mut self, ui: &mut egui::Ui) {
        ui.horizontal_centered(|ui| {
            ui.add_space(14.0);
            let (ir, _) = ui.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
            icons::tg(ui.painter(), "settings/language", ir.center(), ORANGE);
            ui.add_space(4.0);
            let lang = self.settings.language.translate_to.clone();
            let all_tr = !self.messages.is_empty()
                && self
                    .messages
                    .iter()
                    .rev()
                    .filter(|m| !m.text.trim().is_empty())
                    .take(10)
                    .all(|m| self.translated.contains_key(&m.id));
            let lbl = if all_tr {
                "Показать оригинал".to_owned()
            } else {
                format!("Перевести чат ({})", lang.to_uppercase())
            };
            if ui
                .add(egui::Label::new(RichText::new(lbl).color(ORANGE).size(13.0)).sense(Sense::click()))
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                if all_tr {
                    self.translated.clear();
                } else {
                    let items: Vec<(i32, String)> = self
                        .messages
                        .iter()
                        .rev()
                        .filter(|m| !m.text.trim().is_empty() && !self.translated.contains_key(&m.id))
                        .take(25)
                        .map(|m| (m.id, m.text.clone()))
                        .collect();
                    if !items.is_empty() {
                        self.send(Cmd::TranslateBatch(items, lang));
                    }
                }
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
        self.channel_bottom_bar(ui);
        self.context_bar(ui);
        let s = self.settings.ayu.clone();
        let has_cmds = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .and_then(|d| self.peer_infos.get(&d.id))
            .is_some_and(|x| !x.bot_commands.is_empty());
        let bot_cmds = if has_cmds {
            self.selected
                .and_then(|i| self.dialogs.get(i))
                .and_then(|d| self.peer_infos.get(&d.id))
                .map(|x| x.bot_commands.clone())
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let cur_ttl = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .and_then(|d| self.peer_infos.get(&d.id))
            .and_then(|x| x.ttl_period);
        // Необязательная полоса форматирования над строкой ввода (`[Aa]`).
        if self.format_bar_open {
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.spacing_mut().item_spacing.x = 6.0;
                let sel = self.composer_sel;
                if ui.small_button(RichText::new("B").strong()).on_hover_text("Жирный (Ctrl+B)").clicked() {
                    super::rich::wrap_selection(&mut self.input, sel, "**", "**");
                }
                if ui.small_button(RichText::new("I").italics()).on_hover_text("Курсив (Ctrl+I)").clicked() {
                    super::rich::wrap_selection(&mut self.input, sel, "__", "__");
                }
                if ui.small_button(RichText::new("S").strikethrough()).on_hover_text("Зачёркнутый (Ctrl+Shift+X)").clicked() {
                    super::rich::wrap_selection(&mut self.input, sel, "~~", "~~");
                }
                if ui.small_button("</>").on_hover_text("Моноширинный (Ctrl+Shift+M)").clicked() {
                    super::rich::wrap_selection(&mut self.input, sel, "`", "`");
                }
                if ui.small_button("||").on_hover_text("Спойлер (Ctrl+Shift+P)").clicked() {
                    super::rich::wrap_selection(&mut self.input, sel, "||", "||");
                }
                if ui.small_button("Цитата").clicked() {
                    super::rich::wrap_selection(&mut self.input, sel, "> ", "");
                }
            });
        }
        // Всплывающий выбор команд бота (`/`).
        if self.cmd_popup_open && !bot_cmds.is_empty() {
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        for (cmd, desc) in &bot_cmds {
                            let lbl = if desc.is_empty() {
                                format!("/{cmd}")
                            } else {
                                format!("/{cmd} — {desc}")
                            };
                            if ui.small_button(lbl).clicked() {
                                self.input = format!("/{cmd} ");
                                self.cmd_popup_open = false;
                            }
                        }
                    });
                });
        }
        // Всплывающий выбор таймера автоудаления (`field_auto_delete_button`).
        if self.ttl_popup_open {
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .corner_radius(egui::CornerRadius::same(10))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Автоудаление:").size(12.5).color(SUBTEXT));
                        for (lbl, sec) in [("Выкл.", 0), ("24 часа", 86_400), ("7 дней", 604_800), ("1 месяц", 2_678_400)] {
                            let active = cur_ttl.unwrap_or(0) == sec;
                            let txt = if active { RichText::new(lbl).color(ORANGE).strong() } else { RichText::new(lbl) };
                            if ui.small_button(txt).clicked() {
                                self.send(Cmd::SetChatTtl(sec));
                                self.ttl_popup_open = false;
                            }
                        }
                    });
                });
        }
        let mut attach = false;
        ui.horizontal(|ui| {
            ui.set_min_height(46.0);
            ui.spacing_mut().item_spacing.x = 4.0;
            #[cfg(not(target_os = "android"))]
            if s.field_attach_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(42.0, 44.0), Sense::click());
                icons::attach(ui.painter(), rect.center(), if resp.hovered() { WHITE } else { DIALOG_DATE });
                attach = resp.on_hover_text("Отправить файл").clicked();
            }
            if s.field_commands_button && has_cmds {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 44.0), Sense::click());
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "/",
                    FontId::proportional(17.0),
                    if self.cmd_popup_open { ORANGE } else if resp.hovered() { WHITE } else { DIALOG_DATE },
                );
                if resp.on_hover_text("Команды бота").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.cmd_popup_open = !self.cmd_popup_open;
                    self.ttl_popup_open = false;
                }
            }
            // Точный расчёт правой части (без лишнего пустого зазора в строке ввода).
            let right_w = 44.0
                + 34.0
                + if s.field_emoji_button { 46.0 } else { 0.0 }
                + if s.field_auto_delete_button { 36.0 } else { 0.0 };
            let width = (ui.available_width() - right_w).max(80.0);
            let id = ui.make_persistent_id("composer-input");
            let focused = ui.memory(|m| m.has_focus(id));
            // Горячие клавиши в поле ввода:
            // - Стрелка Вверх в пустом поле — редактировать последнее своё сообщение
            // - Ctrl+Z / Ctrl+Y / Ctrl+Shift+Z — откат / возврат текста
            // - Ctrl+B / Ctrl+I / Ctrl+Shift+X / Ctrl+Shift+M / Ctrl+Shift+P — форматирование выделения
            if focused {
                if self.input.is_empty()
                    && self.editing.is_none()
                    && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp))
                    && let Some(last) = self.messages.iter().rev().find(|m| m.outgoing && !m.deleted && !m.text.is_empty())
                {
                    self.reply = None;
                    self.editing = Some(last.id);
                    self.input = last.text.clone();
                }
                let undo = ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::Z));
                let redo = ui.input_mut(|i| {
                    i.consume_key(egui::Modifiers::COMMAND, egui::Key::Y)
                        || i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::Z)
                });
                if undo {
                    if let Some(prev) = self.input_undo.pop() {
                        let cur = std::mem::replace(&mut self.input, prev);
                        self.input_redo.push(cur);
                    }
                } else if redo
                    && let Some(next) = self.input_redo.pop()
                {
                    let cur = std::mem::replace(&mut self.input, next);
                    self.input_undo.push(cur);
                }
                let sel = self.composer_sel;
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::B)) {
                    super::rich::wrap_selection(&mut self.input, sel, "**", "**");
                } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND, egui::Key::I)) {
                    super::rich::wrap_selection(&mut self.input, sel, "__", "__");
                } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::X)) {
                    super::rich::wrap_selection(&mut self.input, sel, "~~", "~~");
                } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::M)) {
                    super::rich::wrap_selection(&mut self.input, sel, "`", "`");
                } else if ui.input_mut(|i| i.consume_key(egui::Modifiers::COMMAND | egui::Modifiers::SHIFT, egui::Key::P)) {
                    super::rich::wrap_selection(&mut self.input, sel, "||", "||");
                }
            }
            let before_input = self.input.clone();
            // Enter отправляет, Shift+Enter — новая строка.
            let enter = focused
                && ui.input(|i| i.key_pressed(egui::Key::Enter) && !i.modifiers.shift)
                && ui.input_mut(|i| i.consume_key(i.modifiers, egui::Key::Enter));
            let text_out = ui
                .allocate_ui(Vec2::new(width, 36.0), |ui| {
                    egui::TextEdit::multiline(&mut self.input)
                        .id(id)
                        .desired_width(width)
                        .desired_rows(1)
                        .hint_text("Написать сообщение…")
                        .frame(egui::Frame::NONE)
                        .margin(Vec2::new(4.0, 8.0))
                        .show(ui)
                })
                .inner;
            if let Some(cr) = text_out.cursor_range {
                self.composer_sel = Some((cr.primary.index.into(), cr.secondary.index.into()));
            }
            let r = text_out.response;
            if r.changed() && self.input != before_input {
                if self.input_undo.last() != Some(&before_input) {
                    self.input_undo.push(before_input);
                    if self.input_undo.len() > 64 {
                        self.input_undo.remove(0);
                    }
                }
                self.input_redo.clear();
            }
            if self.input.is_empty() && !self.input_custom_emojis.is_empty() {
                self.input_custom_emojis.clear();
            }
            // Кнопка панели форматирования [Aa].
            {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(30.0, 44.0), Sense::click());
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Aa",
                    FontId::proportional(13.5),
                    if self.format_bar_open { ORANGE } else if resp.hovered() { WHITE } else { DIALOG_DATE },
                );
                if resp.on_hover_text("Форматирование (**жирный**, __курсив__, `код`, ||спойлер||)").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.format_bar_open = !self.format_bar_open;
                }
            }
            if s.field_auto_delete_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(32.0, 44.0), Sense::click());
                let active_ttl = cur_ttl.is_some_and(|v| v > 0);
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "⏱",
                    FontId::proportional(15.0),
                    if active_ttl || self.ttl_popup_open { ORANGE } else if resp.hovered() { WHITE } else { DIALOG_DATE },
                );
                if resp.on_hover_text("Таймер автоудаления сообщений").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.ttl_popup_open = !self.ttl_popup_open;
                    self.cmd_popup_open = false;
                }
            }
            if s.field_emoji_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(42.0, 44.0), Sense::click());
                self.emoji_btn = rect;
                let k = tween(ui.ctx(), "emoji-btn", f32::from(u8::from(resp.hovered() || self.emoji_open)), 0.12);
                let color = lerp_color(DIALOG_DATE, WHITE, k);
                // Кольцо рисуется кодом (как в Telegram), лицо — значок из набора.
                ui.painter().circle_stroke(rect.center(), 10.5, egui::Stroke::new(1.5, color));
                icons::tg(ui.painter(), "chat/input_smile_face", rect.center(), color);
                if resp.clicked() {
                    self.emoji_pinned = !self.emoji_pinned;
                    self.emoji_open = self.emoji_pinned;
                } else if resp.hovered() && s.show_emoji_popup && ui.ctx().content_rect().width() < super::NARROW_WIDTH {
                    self.emoji_open = true;
                }
            }
            let (srect, sresp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
            let active = !self.input.trim().is_empty();
            let send_k = tween(ui.ctx(), "send-active", f32::from(u8::from(active)), 0.14);
            let send_h = tween(ui.ctx(), "send-hover", f32::from(u8::from(active && sresp.hovered())), 0.12);
            if send_k > 0.02 {
                let r_btn = 16.5 + 1.2 * send_h;
                let base = lerp_color(ORANGE, ORANGE_OVER, send_h);
                let fill = Color32::from_rgba_unmultiplied(
                    base.r(),
                    base.g(),
                    base.b(),
                    (f32::from(t_btn(base).a()) * send_k) as u8,
                );
                ui.painter().circle_filled(srect.center(), r_btn, fill);
            }
            let arrow_c = if active { Color32::WHITE } else { DIALOG_DATE };
            icons::send(ui.painter(), srect.center(), arrow_c);
            let sresp = if active { sresp.on_hover_cursor(egui::CursorIcon::PointingHand) } else { sresp };
            if (sresp.clicked() || enter) && active && self.selected.is_some() {
                let lead = self.input.chars().take_while(|c| c.is_whitespace()).count();
                let raw_text = self.input.trim().to_owned();
                let text = if cfg!(feature = "plugins") && self.settings.plugins_enabled {
                    match self.plugins.apply_outgoing(&raw_text) {
                        rg_ayu::plugins::HookOutcome::Modified(s) => s,
                        rg_ayu::plugins::HookOutcome::Unchanged => raw_text,
                    }
                } else {
                    raw_text
                };
                let char_total = text.chars().count();
                let custom: Vec<(usize, usize, i64)> = std::mem::take(&mut self.input_custom_emojis)
                    .into_iter()
                    .filter_map(|(st, ln, id)| {
                        let ns = st.checked_sub(lead)?;
                        (ns + ln <= char_total).then_some((ns, ln, id))
                    })
                    .collect();
                self.input.clear();
                self.input_undo.clear();
                self.input_redo.clear();
                self.composer_sel = None;
                let (clean, ents) = super::rich::parse_formatted(&text, &custom);
                if let Some(id) = self.editing.take() {
                    self.send(Cmd::Edit(id, clean));
                } else {
                    let reply = self.reply.take().map(|(id, _)| id);
                    if ents.is_empty() {
                        self.send(Cmd::Send(clean, reply));
                    } else {
                        self.send(Cmd::SendFormatted(clean, ents, reply));
                    }
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

    /// Содержимое панели эмодзи/стикеров/GIF (одно и для правой панели, и для всплывающего окна).
    pub(crate) fn emoji_panel_body(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        let mut jump = self.emoji_jump.take();
        let mut tab = self.emoji_tab;
        let mut sacts: Vec<super::panels::StickerAct> = Vec::new();
        // Вкладки как в AyuGram: на всю ширину, оранжевое подчёркивание у выбранной + кнопка закрепления.
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            let pin_w = 34.0;
            let w = ((ui.available_width() - pin_w) / 3.0).max(50.0);
            for (n, name) in ["Эмодзи", "Стикеры", "GIF"].iter().enumerate() {
                let (r, resp) = ui.allocate_exact_size(Vec2::new(w, 34.0), Sense::click());
                let k = tween(ui.ctx(), ("emtab", n), f32::from(u8::from(tab == n)), 0.16);
                ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, *name, FontId::proportional(14.0), lerp_color(DIALOG_TEXT, WHITE, k));
                ui.painter().rect_filled(
                    Rect::from_center_size(egui::pos2(r.center().x, r.bottom() - 1.0), Vec2::new(44.0 * k, 2.5)),
                    egui::CornerRadius::same(1),
                    t_btn(ORANGE),
                );
                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    tab = n;
                }
            }
            let (pr, presp) = ui.allocate_exact_size(Vec2::new(pin_w, 34.0), Sense::click());
            let pin_c = if self.emoji_pinned { ORANGE } else if presp.hovered() { WHITE } else { SUBTEXT };
            icons::tg(ui.painter(), "dialogs/dialogs_pinned", pr.center(), pin_c);
            if presp
                .on_hover_text(if self.emoji_pinned { "Открепить панель" } else { "Закрепить панель" })
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                self.emoji_pinned = !self.emoji_pinned;
            }
        });
        if tab == 0 {
            if !self.emoji_kw_requested {
                self.emoji_kw_requested = true;
                self.send(Cmd::EmojiKeywords);
            }
            // Поле поиска эмодзи сверху, как в AyuGram.
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                let sw = (ui.available_width() - 12.0).max(80.0);
                let (srect, _) = ui.allocate_exact_size(Vec2::new(sw, 30.0), Sense::hover());
                ui.painter().rect_filled(srect, egui::CornerRadius::same(15), t(FILTER_INPUT));
                let inner = Rect::from_min_max(
                    srect.min + Vec2::new(12.0, 3.0),
                    srect.max - Vec2::new(if self.emoji_search.is_empty() { 12.0 } else { 28.0 }, 3.0),
                );
                ui.put(
                    inner,
                    egui::TextEdit::singleline(&mut self.emoji_search)
                        .hint_text("Поиск эмодзи")
                        .frame(egui::Frame::NONE)
                        .desired_width(inner.width()),
                );
                if !self.emoji_search.is_empty() {
                    let xr = Rect::from_center_size(Pos2::new(srect.right() - 15.0, srect.center().y), Vec2::splat(22.0));
                    let xresp = ui.interact(xr, ui.make_persistent_id("em-search-clear"), Sense::click());
                    icons::tg(ui.painter(), "info/info_close", xr.center(), if xresp.hovered() { WHITE } else { SUBTEXT });
                    if xresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.emoji_search.clear();
                    }
                }
            });
            ui.add_space(4.0);
            let h = (ui.available_height() - 42.0).max(60.0);
            let query = self.emoji_search.clone();
            ui.allocate_ui(Vec2::new(ui.available_width(), h), |ui| {
                if self.emoji_add_open {
                    self.featured_view(ui, &mut sacts);
                } else if let Some(i) = self.emoji_pack {
                    self.pack_view(ui, i, &mut sacts);
                } else if let Some(emoji) = self.emoji.as_ref() {
                    picked = emoji.picker(ui, &self.settings.recent_emoji, &query, &mut jump);
                }
            });
            self.emoji_bar(ui, &mut jump, &mut sacts);
        } else if tab == 1 {
            self.sticker_tab(ui, self.emoji.as_ref(), &mut sacts);
        } else {
            self.gif_tab(ui, &mut sacts);
        }
        self.emoji_tab = tab;
        self.emoji_jump = jump;
        self.emoji_apply(sacts, picked);
    }

    fn emoji_apply(&mut self, sacts: Vec<super::panels::StickerAct>, picked: Option<char>) {
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
                    let is_gif = self.emoji_tab == 2;
                    let confirm = if is_gif {
                        self.settings.ayu.gif_confirmation
                    } else {
                        self.settings.ayu.sticker_confirmation
                    };
                    let reply = self.reply.as_ref().map(|(r, _)| *r);
                    if confirm {
                        self.pending_confirm_media = Some((is_gif, id, reply));
                    } else {
                        self.reply.take();
                        self.send(Cmd::SendSticker(id, reply));
                    }
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
                A::InsertCustom(text, doc_id) => {
                    let start = self.input.chars().count();
                    let len = text.chars().count();
                    self.input.push_str(&text);
                    if len > 0 {
                        self.input_custom_emojis.push((start, len, doc_id));
                    }
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
    }

    /// Всплывающее окно над полем ввода (только узкое окно; в широком панель стоит справа).
    pub(crate) fn emoji_window(&mut self, ctx: &egui::Context) {
        let appear = tween(ctx, "emoji-panel", f32::from(u8::from(self.emoji_open)), 0.18);
        if appear < 0.01 {
            return;
        }
        let screen = ctx.content_rect();
        let size = Vec2::new(344.0_f32.min(screen.width() - 16.0), 340.0_f32.min(screen.height() - 120.0));
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
                    self.emoji_panel_body(ui);
                });
        });
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
        let fade = (self.chat_opened.elapsed().as_secs_f32() / 0.22).clamp(0.0, 1.0);
        if fade < 1.0 {
            ui.ctx().request_repaint();
        }
        ui.set_opacity(ios_spring(fade));
        let now = self.now;
        let dlg_meta = self.selected.and_then(|i| {
            self.dialogs.get(i).map(|d| {
                (
                    i,
                    d.id,
                    d.kind == rg_core::engine::DialogKind::Channel,
                    d.unread,
                    self.peer_infos
                        .get(&d.id)
                        .and_then(|x| x.linked_chat_id)
                        .is_some(),
                )
            })
        });
        let group = dlg_meta.is_some_and(|(_, id, _, _, _)| id < 0);
        let is_channel = dlg_meta.is_some_and(|(_, _, ch, _, _)| ch);
        let has_discussion = dlg_meta.is_some_and(|(_, _, _, _, disc)| disc);
        let unread_cnt = dlg_meta.map_or(0, |(_, _, _, u, _)| u);
        let shift = ui.input(|i| i.modifiers.shift);
        if let Some((idx, cid, _, _, _)) = dlg_meta {
            if let Some(pin) = self.pinned_msgs.get(&cid).cloned() {
                egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 28.0), Sense::hover());
                            ui.painter().rect_filled(bar, 1.5, ORANGE);
                            let txt_w = (ui.available_width() - 32.0).max(80.0);
                            let preview = if pin.text.is_empty() {
                                "Вложение".to_owned()
                            } else {
                                short(&pin.text, 55)
                            };
                            let resp = ui
                                .allocate_ui(Vec2::new(txt_w, 28.0), |ui| {
                                    ui.vertical(|ui| {
                                        ui.spacing_mut().item_spacing.y = 0.0;
                                        ui.label(RichText::new("Закреплённое сообщение").size(11.5).color(ORANGE).strong());
                                        ui.label(RichText::new(preview).size(12.5).color(SUBTEXT));
                                    });
                                })
                                .response;
                            if ui.interact(resp.rect, ui.make_persistent_id("pin-banner"), Sense::click()).clicked() {
                                if self.messages.iter().any(|m| m.id == pin.id) {
                                    self.scroll_to = Some(pin.id);
                                } else {
                                    self.status = "Закреплённое сообщение выше в истории".to_owned();
                                }
                            }
                            if ui.small_button("✕").on_hover_text("Открепить").clicked() {
                                self.send(Cmd::Pin(pin.id, false));
                                self.pinned_msgs.remove(&cid);
                            }
                        });
                    });
            }
            if let Some(topics) = self.forum_topics.get(&cid).cloned()
                && !topics.is_empty()
            {
                let mut picked_topic: Option<Option<i32>> = None;
                let show_cnt = !self.settings.ayu.hide_notification_counters;
                egui::ScrollArea::horizontal()
                    .id_salt("forum-topics-bar")
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.add_space(6.0);
                            if ui.selectable_label(self.active_topic.is_none(), "Все сообщения").clicked() {
                                picked_topic = Some(None);
                            }
                            for t_row in &topics {
                                let lbl = if show_cnt && t_row.unread > 0 {
                                    format!("{} ({})", t_row.title, t_row.unread)
                                } else {
                                    t_row.title.clone()
                                };
                                if ui.selectable_label(self.active_topic == Some(t_row.id), lbl).clicked() {
                                    picked_topic = Some(Some(t_row.id));
                                }
                            }
                        });
                    });
                if let Some(pick) = picked_topic {
                    self.active_topic = pick;
                    if let Some(tid) = pick {
                        self.topic_msgs = None;
                        self.send(Cmd::TopicMessages(idx, tid));
                    }
                }
            }
        }
        if let Some((_, uname)) = self.user_filter.clone() {
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("Фильтр по отправителю: {uname}")).size(12.5).color(ORANGE));
                        if ui.small_button("Сбросить").clicked() {
                            self.user_filter = None;
                        }
                    });
                });
        }
        let viewport_h = ui.available_height();
        let clip_rect = ui.clip_rect();
        // Быстрый скролл при нажатии колёсика мыши (Middle Click Auto-Scroll) и ускорение колеса (Shift/Alt/Middle ×3.5).
        let ptr_pos = ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
        let mid_clicked = ui.input(|i| i.pointer.button_clicked(egui::PointerButton::Middle));
        let mid_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        let any_other_click = ui.input(|i| {
            i.pointer.button_clicked(egui::PointerButton::Primary)
                || i.pointer.button_clicked(egui::PointerButton::Secondary)
                || i.key_pressed(egui::Key::Escape)
        });
        if mid_clicked
            && let Some(p) = ptr_pos
            && clip_rect.contains(p)
        {
            if self.autoscroll_anchor.is_some() {
                self.autoscroll_anchor = None;
            } else {
                self.autoscroll_anchor = Some(p);
            }
        } else if any_other_click {
            self.autoscroll_anchor = None;
        }
        let fast_wheel_delta = ui.input(|i| {
            if (i.modifiers.shift || i.modifiers.alt || mid_down) && i.smooth_scroll_delta.y.abs() > 0.1 {
                i.smooth_scroll_delta.y * 2.5
            } else {
                0.0
            }
        });
        let autoscroll_dy = if let (Some(anchor), Some(cur)) = (self.autoscroll_anchor, ptr_pos) {
            let diff = anchor.y - cur.y;
            if diff.abs() > 6.0 {
                ui.ctx().request_repaint();
                diff * 0.24
            } else {
                0.0
            }
        } else {
            0.0
        };
        let scroll_to = self.scroll_to.take();
        let mut acts: Vec<Act> = Vec::new();
        let mut want_older = false;
        let searching = self.found.is_some();
        let loading = self.history_loading && !searching && self.messages.is_empty();
        self.inline_gif_rect = None;
        let mut gif_candidate: Option<(i32, Rect)> = None;
        if loading {
            paint_message_skeleton(ui, ui.available_width());
            return;
        }
        let pad = (viewport_h - self.msgs_h).max(0.0);
        let out = {
            let topic_slice = self.active_topic.and_then(|tid| {
                self.topic_msgs
                    .as_ref()
                    .filter(|(mtid, _)| *mtid == tid)
                    .map(|(_, rows)| rows.as_slice())
            });
            let list: &[MessageRow] = if searching {
                self.found.as_deref().unwrap_or(&self.messages)
            } else {
                topic_slice.unwrap_or(&self.messages)
            };
            let read_out = self.selected.and_then(|i| self.dialogs.get(i)).map_or(0, |d| d.read_out);
            let env = Env {
                read_out,
                avatars: &self.avatars,
                fetching: &self.fetching,
                cached: &self.cached_media,
                gif_paths: &self.gif_paths,
                translated: &self.translated,
                custom_tex: &self.sticker_tex,
                show_translate: self.settings.language.show_translate_button,
                clip_rect,
                selected_msgs: &self.selected_msgs,
                avail: ui.available_width(),
                s: &self.settings.ayu,
                tex: &self.textures,
                shift,
                scroll_to,
                group,
                is_channel,
                has_discussion,
                format_secs: self.settings.ayu.show_message_seconds || self.settings.extera.format_time_with_seconds,
                double_tap_action: self.settings.extera.double_tap_action,
                arrived: &self.arrived,
                img_arrived: &self.img_arrived,
                time: ui.input(|i| i.time),
                emoji: self.emoji.as_ref(),
                spoilers: &self.spoilers,
            };
            let hidden = &self.hidden;
            let hide_blocked = self.settings.ayu.hide_from_blocked;
            let blocked_ids = &self.blocked_ids;
            let user_filter = self.user_filter.as_ref().map(|(id, _)| *id);
            egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(!searching && self.autoscroll_anchor.is_none() && self.active_topic.is_none())
                .show(ui, |ui| {
                    if (fast_wheel_delta + autoscroll_dy).abs() > 0.1 {
                        ui.scroll_with_delta(Vec2::new(0.0, fast_wheel_delta + autoscroll_dy));
                    }
                    let mut prev_date: Option<i64> = None;
                    // Короткая переписка прижата к низу, как в Telegram.
                    ui.add_space(8.0 + pad);
                    if searching && list.is_empty() {
                        ui.vertical_centered(|ui| ui.label(RichText::new("Ничего не найдено").color(SUBTEXT)));
                    }
                    let visible: Vec<&MessageRow> = list
                        .iter()
                        .filter(|m| {
                            !hidden.contains(&m.id)
                                && (!hide_blocked || !blocked_ids.contains(&m.sender_id))
                                && user_filter.is_none_or(|uid| m.sender_id == uid)
                        })
                        .collect();
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
                        let show_avatar = group
                            && !m.outgoing
                            && visible.get(j).is_none_or(|n| n.outgoing || n.sender_id != m.sender_id);
                        if prev_date.is_none_or(|p| !same_day(p, m.date)) {
                            day_pill(ui, env.avail, day_label(m.date, now));
                        }
                        prev_date = Some(m.date);
                        if album.len() > 1 {
                            let caption = album.iter().find(|x| !x.text.is_empty()).copied().unwrap_or(album[album.len() - 1]);
                            bubble(ui, caption, album, show_avatar, &env, &mut acts, &mut gif_candidate);
                        } else {
                            bubble(ui, m, &[], show_avatar, &env, &mut acts, &mut gif_candidate);
                        }
                        ui.add_space(4.0);
                        i = j;
                    }
                    ui.add_space(8.0);
                })
        };
        self.inline_gif_rect = gif_candidate;
        self.msgs_h = (out.content_size.y - pad).max(0.0);
        self.scrolled_up = out.content_size.y > out.inner_rect.height() + 20.0
            && (out.state.clamped_offset().y + out.inner_rect.height() < out.content_size.y - 48.0);
        if self.settings.extera.hide_keyboard_on_scroll && ui.input(|i| i.smooth_scroll_delta.y.abs() > 3.0) {
            if !self.emoji_pinned {
                self.emoji_open = false;
            }
            self.cmd_popup_open = false;
            self.ttl_popup_open = false;
        }
        // Плавающая кнопка прокрутки вниз (учитывает настройку square_fab).
        if self.scrolled_up && self.selected_msgs.is_empty() {
            let center = Pos2::new(clip_rect.right() - 28.0, clip_rect.bottom() - 28.0);
            let fab_rect = Rect::from_center_size(center, Vec2::splat(40.0));
            let resp = ui.interact(fab_rect, ui.make_persistent_id("scroll-down-fab"), Sense::click());
            let bg = if resp.hovered() { t_btn(ORANGE) } else { t2(WINDOW_BG) };
            if self.settings.extera.square_fab {
                ui.painter().rect_filled(fab_rect, egui::CornerRadius::same(13), bg);
                ui.painter().rect_stroke(
                    fab_rect,
                    egui::CornerRadius::same(13),
                    egui::Stroke::new(1.0, Color32::from_white_alpha(32)),
                    egui::StrokeKind::Inside,
                );
            } else {
                ui.painter().circle_filled(center, 20.0, bg);
                ui.painter().circle_stroke(center, 20.0, egui::Stroke::new(1.0, Color32::from_white_alpha(32)));
            }
            ui.painter().text(center, egui::Align2::CENTER_CENTER, "↓", FontId::proportional(18.0), WHITE);
            if unread_cnt > 0 && !self.settings.ayu.hide_notification_counters {
                let badge = Rect::from_center_size(center - Vec2::new(0.0, 22.0), Vec2::new(26.0, 16.0));
                ui.painter().rect_filled(badge, egui::CornerRadius::same(8), t_btn(ORANGE));
                ui.painter().text(badge.center(), egui::Align2::CENTER_CENTER, unread_cnt.min(999).to_string(), FontId::proportional(10.5), Color32::WHITE);
            }
            if resp.on_hover_text("Вниз").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                self.scroll_to = self.messages.last().map(|m| m.id);
            }
        }
        // Отрисовка якоря быстрого скролла колёсиком мыши (Middle Click Auto-Scroll).
        if let Some(anchor) = self.autoscroll_anchor {
            let p = ui.painter();
            if let Some(cur) = ptr_pos {
                p.line_segment(
                    [anchor, Pos2::new(anchor.x, cur.y)],
                    egui::Stroke::new(1.5, t_btn(ORANGE).gamma_multiply(0.55)),
                );
            }
            p.circle_filled(anchor, 16.0, t2(WINDOW_BG));
            p.circle_stroke(anchor, 16.0, egui::Stroke::new(1.5, t_btn(ORANGE)));
            p.circle_filled(anchor, 3.0, t_btn(ORANGE));
            p.text(
                Pos2::new(anchor.x, anchor.y - 9.0),
                egui::Align2::CENTER_CENTER,
                "▲",
                FontId::proportional(8.0),
                WHITE,
            );
            p.text(
                Pos2::new(anchor.x, anchor.y + 9.0),
                egui::Align2::CENTER_CENTER,
                "▼",
                FontId::proportional(8.0),
                WHITE,
            );
        }
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
        // Плавающая панель действий при множественном выделении сообщений.
        if !self.selected_msgs.is_empty() {
            let bar_w = 460.0_f32.min(clip_rect.width() - 24.0);
            let bar = Rect::from_min_size(
                Pos2::new(clip_rect.center().x - bar_w * 0.5, clip_rect.bottom() - 48.0),
                Vec2::new(bar_w, 40.0),
            );
            ui.painter().rect_filled(bar, egui::CornerRadius::same(12), t2(WINDOW_BG));
            ui.painter().rect_stroke(
                bar,
                egui::CornerRadius::same(12),
                egui::Stroke::new(1.0, Color32::from_white_alpha(36)),
                egui::StrokeKind::Inside,
            );
            let mut clear_sel = false;
            let mut open_fwd = false;
            let mut open_shot = false;
            let mut copy_sel = false;
            let count = self.selected_msgs.len();
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(Vec2::new(10.0, 4.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new(format!("Выбрано: {count}")).strong().color(WHITE));
                    ui.add_space(6.0);
                    if orange_button(ui, "Переслать", Vec2::new(96.0, 28.0)).clicked() {
                        open_fwd = true;
                    }
                    if ui.button("Снимок").clicked() {
                        open_shot = true;
                    }
                    if ui.button("Копия").clicked() {
                        copy_sel = true;
                    }
                    if ui.button("Снять").clicked() {
                        clear_sel = true;
                    }
                });
            });
            if copy_sel {
                let text = self
                    .messages
                    .iter()
                    .filter(|m| self.selected_msgs.contains(&m.id) && !m.text.is_empty())
                    .map(|m| m.text.as_str())
                    .collect::<Vec<_>>()
                    .join("\n");
                if !text.is_empty() {
                    ui.ctx().copy_text(text);
                    self.status = "Выделенные тексты скопированы".to_owned();
                }
                clear_sel = true;
            }
            if open_fwd {
                let ids: Vec<i32> = self
                    .messages
                    .iter()
                    .filter(|m| self.selected_msgs.contains(&m.id))
                    .map(|m| m.id)
                    .collect();
                if !ids.is_empty() {
                    self.forward_batch = Some(ids);
                }
            }
            if open_shot {
                let rows: Vec<MessageRow> = self
                    .messages
                    .iter()
                    .filter(|m| self.selected_msgs.contains(&m.id))
                    .cloned()
                    .collect();
                if !rows.is_empty() {
                    self.open_shot_modal(rows);
                    clear_sel = true;
                }
            }
            if clear_sel {
                self.selected_msgs.clear();
            }
        }
    }

    fn channel_bottom_bar(&mut self, ui: &mut egui::Ui) {
        use rg_settings::ChannelBottomButton as CB;
        let btn_mode = self.settings.ayu.channel_bottom_button;
        if btn_mode == CB::Hidden {
            return;
        }
        let Some(idx) = self.selected else { return };
        let Some(d) = self.dialogs.get(idx) else { return };
        if d.kind != rg_core::engine::DialogKind::Channel {
            return;
        }
        let info = self.peer_infos.get(&d.id);
        let has_linked = info.and_then(|x| x.linked_chat_id).is_some();
        let muted = d.muted;
        let last_post_id = self.messages.last().map(|m| m.id);
        let want_discuss = match btn_mode {
            CB::Hidden => return,
            CB::Mute => false,
            CB::Discuss => has_linked,
            CB::DiscussWithFallback => has_linked,
        };
        if btn_mode == CB::Discuss && !has_linked {
            return;
        }
        ui.horizontal(|ui| {
            let w = ui.available_width();
            if want_discuss {
                if orange_button(ui, "Обсудить", Vec2::new(w, 34.0)).clicked()
                    && let Some(pid) = last_post_id
                {
                    self.send(Cmd::Discussion(idx, pid));
                }
            } else {
                let lbl = if muted { "Включить звук" } else { "Выключить звук" };
                if orange_button(ui, lbl, Vec2::new(w, 34.0)).clicked() {
                    self.send(Cmd::MuteDialog(idx, !muted));
                }
            }
        });
        ui.add_space(4.0);
    }

    fn open_shot_modal(&mut self, rows: Vec<MessageRow>) {
        self.shot_modal = Some(super::ShotModal {
            messages: rows,
            theme: super::shot::ShotTheme::Night,
            cached_key: None,
            rgba: Vec::new(),
            dims: (0, 0),
            preview_tex: None,
        });
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
            Act::CancelFetch(id) => self.send(Cmd::CancelFetch(id)),
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
            Act::Translate(id, text) => {
                self.status = "Перевод…".to_owned();
                self.send(Cmd::Translate(id, text));
            }
            Act::Shot(id) => {
                if let Some(m) = self.messages.iter().find(|m| m.id == id).cloned() {
                    self.open_shot_modal(vec![m]);
                }
            }
            Act::VotePoll(id, option) => {
                self.send(Cmd::VotePoll(id, option));
            }
            Act::ToggleSelect(id) => {
                if !self.selected_msgs.remove(&id) {
                    self.selected_msgs.insert(id);
                }
            }
            Act::Discuss(post_id) => {
                if let Some(idx) = self.selected {
                    self.send(Cmd::Discussion(idx, post_id));
                }
            }
            Act::FilterUser(uid, uname) => {
                self.user_filter = Some((
                    uid,
                    if uname.is_empty() { format!("ID {uid}") } else { uname },
                ));
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
                    ui.checkbox(&mut self.forward_no_author, "Скрыть автора (без подписи)");
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
                self.send(Cmd::Forward(id, i, self.forward_no_author));
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
                        if orange_button(ui, "Открыть", Vec2::new(96.0, 32.0)).clicked() {
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
        // Пакетная пересылка выделенных сообщений (`ayu_forward.cpp`).
        if let Some(ids) = self.forward_batch.clone() {
            let mut open = true;
            let mut chosen = None;
            egui::Window::new(format!("Переслать ({})", ids.len()))
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
                    ui.checkbox(&mut self.forward_no_author, "Скрыть автора (без подписи)");
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
                self.send(Cmd::ForwardBatch(ids, i, self.forward_no_author));
                self.selected_msgs.clear();
                open = false;
            }
            if !open {
                self.forward_batch = None;
                self.forward_filter.clear();
            }
        }
        // Интерактивный «Снимок сообщения» (`message_shot_box.cpp`).
        if self.shot_modal.is_some() {
            let chat_title = self
                .selected
                .and_then(|i| self.dialogs.get(i))
                .map_or("Чат", |d| d.name.as_str())
                .to_owned();
            let mut close = ctx.input(|i| i.key_pressed(egui::Key::Escape));
            let mut ayu_changed = false;
            let mut save_png = false;
            let mut copy_clip = false;
            let mut s_copy = self.settings.clone();
            if let Some(modal) = self.shot_modal.as_mut() {
                let key = (
                    modal.theme,
                    s_copy.ayu.show_background,
                    s_copy.ayu.show_date,
                    s_copy.ayu.show_reactions,
                    s_copy.ayu.show_header_decorations,
                    s_copy.ayu.show_colorful_replies,
                    s_copy.ayu.reveal_spoilers,
                );
                if modal.cached_key != Some(key) {
                    let refs: Vec<&MessageRow> = modal.messages.iter().collect();
                    let (w, h, rgba) = super::shot::render_message_shot(&super::shot::ShotInput {
                        chat_title: &chat_title,
                        messages: &refs,
                        ayu: &s_copy.ayu,
                        theme: modal.theme,
                    });
                    let img = egui::ColorImage::from_rgba_unmultiplied([w as usize, h as usize], &rgba);
                    modal.preview_tex = Some(ctx.load_texture("msg-shot-preview", img, egui::TextureOptions::LINEAR));
                    modal.rgba = rgba;
                    modal.dims = (w, h);
                    modal.cached_key = Some(key);
                }
                let mut open = true;
                egui::Window::new("Снимок сообщения")
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.set_max_width(480.0);
                        if let Some(tex) = &modal.preview_tex {
                            let sz = tex.size_vec2();
                            let scale = (450.0 / sz.x).min(290.0 / sz.y).min(1.0);
                            egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                                ui.centered_and_justified(|ui| {
                                    ui.add(egui::Image::new(tex).fit_to_exact_size(sz * scale).corner_radius(8));
                                });
                            });
                        }
                        ui.add_space(6.0);
                        ui.horizontal_wrapped(|ui| {
                            for th in super::shot::ShotTheme::ALL {
                                if ui.selectable_label(modal.theme == th, th.title()).clicked() {
                                    modal.theme = th;
                                }
                            }
                        });
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ayu_changed |= ui.checkbox(&mut s_copy.ayu.show_background, "Фон").changed();
                            ayu_changed |= ui.checkbox(&mut s_copy.ayu.show_date, "Дата").changed();
                            ayu_changed |= ui.checkbox(&mut s_copy.ayu.show_reactions, "Реакции").changed();
                            ayu_changed |= ui.checkbox(&mut s_copy.ayu.show_header_decorations, "Заголовок").changed();
                            ayu_changed |= ui.checkbox(&mut s_copy.ayu.show_colorful_replies, "Цветные ответы").changed();
                            ayu_changed |= ui.checkbox(&mut s_copy.ayu.reveal_spoilers, "Спойлеры").changed();
                        });
                        ui.add_space(8.0);
                        ui.horizontal(|ui| {
                            if orange_button(ui, "Копировать", Vec2::new(130.0, 32.0)).clicked() {
                                copy_clip = true;
                            }
                            if ui.button("Сохранить PNG…").clicked() {
                                save_png = true;
                            }
                            if ui.button("Закрыть").clicked() {
                                close = true;
                            }
                        });
                    });
                if !open {
                    close = true;
                }
            }
            if ayu_changed {
                self.settings = s_copy.clone();
                self.send(Cmd::Settings(Box::new(s_copy)));
            }
            if copy_clip && let Some(modal) = &self.shot_modal {
                match super::shot::copy_shot_to_clipboard(modal.dims.0, modal.dims.1, &modal.rgba) {
                    Ok(()) => {
                        self.status = "Снимок скопирован в буфер обмена".to_owned();
                        close = true;
                    }
                    Err(e) => self.error = format!("Буфер: {e}"),
                }
            }
            if save_png && let Some(modal) = &self.shot_modal {
                #[cfg(not(target_os = "android"))]
                if let Some(path) = rfd::FileDialog::new()
                    .set_file_name("rustgramm_shot.png")
                    .add_filter("PNG", &["png"])
                    .save_file()
                    && let Some(img) = image::RgbaImage::from_raw(modal.dims.0, modal.dims.1, modal.rgba.clone())
                {
                    if img.save_with_format(&path, image::ImageFormat::Png).is_ok() {
                        self.status = format!("Снимок сохранён: {}", path.display());
                        close = true;
                    } else {
                        self.error = "Не удалось записать PNG".to_owned();
                    }
                }
            }
            if close {
                self.shot_modal = None;
            }
        }
        // Журнал удалённых и изменённых сообщений чата (`ayu/ui/message_history`).
        if let Some((_peer_id, entries)) = self.audit_history.clone() {
            let mut open = true;
            egui::Window::new("Удалённые и история правок")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([430.0, 460.0])
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    if entries.is_empty() {
                        ui.label(RichText::new("В этом чате пока нет сохранённых удалённых сообщений или правок.").color(SUBTEXT));
                        return;
                    }
                    egui::ScrollArea::vertical().max_height(400.0).show(ui, |ui| {
                        for e in &entries {
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    let badge = if e.deleted {
                                        RichText::new("УДАЛЕНО").color(DANGER).strong()
                                    } else {
                                        RichText::new("ИЗМЕНЕНО").color(ORANGE).strong()
                                    };
                                    ui.label(badge);
                                    ui.label(RichText::new(format!("#{} · {}", e.msg_id, full_date(e.date))).size(11.5).color(SUBTEXT));
                                    if ui.small_button("Копия").clicked() {
                                        ctx.copy_text(e.text.clone());
                                    }
                                });
                                if !e.edits.is_empty() {
                                    for (idx, (old, at)) in e.edits.iter().enumerate() {
                                        ui.label(RichText::new(format!("Версия {} ({}): {}", idx + 1, full_date(*at), old)).size(12.5).color(SUBTEXT));
                                    }
                                    ui.label(RichText::new(format!("Текущий: {}", e.text)).color(WHITE));
                                } else {
                                    ui.label(RichText::new(&e.text).color(WHITE));
                                }
                            });
                            ui.add_space(4.0);
                        }
                    });
                });
            if !open {
                self.audit_history = None;
            }
        }
        // Перетаскивание файлов (Drag & Drop) — подтверждение и подпись.
        if !self.drop_paths.is_empty() {
            let mut send_all = false;
            let mut cancel = false;
            egui::Window::new("Отправка файлов")
                .collapsible(false)
                .resizable(false)
                .default_width(360.0)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(RichText::new(format!("Файлов выбрано: {}", self.drop_paths.len())).color(WHITE).strong());
                    ui.add_space(4.0);
                    egui::ScrollArea::vertical().max_height(140.0).show(ui, |ui| {
                        for p in &self.drop_paths {
                            let name = p.file_name().and_then(|n| n.to_str()).unwrap_or("файл");
                            ui.label(RichText::new(format!("• {name}")).size(12.5).color(SUBTEXT));
                        }
                    });
                    ui.add_space(6.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.drop_caption)
                            .hint_text("Подпись (необязательно)")
                            .desired_width(330.0),
                    );
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if orange_button(ui, "Отправить", Vec2::new(120.0, 32.0)).clicked() {
                            send_all = true;
                        }
                        if ui.button("Отмена").clicked() {
                            cancel = true;
                        }
                    });
                });
            if send_all {
                let paths = std::mem::take(&mut self.drop_paths);
                let cap = std::mem::take(&mut self.drop_caption);
                let reply = self.reply.take().map(|(id, _)| id);
                for (k, p) in paths.into_iter().enumerate() {
                    let c = if k == 0 { cap.trim().to_owned() } else { String::new() };
                    self.send(Cmd::SendFile(p, c, if k == 0 { reply } else { None }));
                }
            } else if cancel {
                self.drop_paths.clear();
                self.drop_caption.clear();
            }
        }
        // Запрос включения режима призрака перед просмотром истории (`suggest_ghost_mode_before_viewing_story`).
        if let Some(idx) = self.story_ghost_ask {
            let mut open = true;
            egui::Window::new("Просмотр истории")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label("Скрыть отметку о просмотре этой истории (режим призрака)?");
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if orange_button(ui, "Смотреть анонимно", Vec2::new(165.0, 32.0)).clicked() {
                            self.settings.ghost.send_read_stories = false;
                            self.send(Cmd::Settings(Box::new(self.settings.clone())));
                            self.story_ghost_ask = None;
                            self.story_open = Some(idx);
                        }
                        if ui.button("Открыть обычно").clicked() {
                            self.story_ghost_ask = None;
                            self.story_open = Some(idx);
                            if let Some(st) = self.stories.get_mut(idx) {
                                st.unread = false;
                                let (peer, sid) = (st.peer, st.story_id);
                                self.send(Cmd::ReadStory(peer, sid));
                            }
                        }
                        if ui.button("Отмена").clicked() {
                            self.story_ghost_ask = None;
                        }
                    });
                });
            if !open {
                self.story_ghost_ask = None;
            }
        }
        // Просмотрщик активных историй.
        if let Some(idx) = self.story_open {
            let mut next_idx = Some(idx);
            let mut open = true;
            if let Some(st) = self.stories.get(idx).cloned() {
                egui::Window::new(format!("История · {}", st.author))
                    .open(&mut open)
                    .collapsible(false)
                    .resizable(false)
                    .default_width(380.0)
                    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        ui.horizontal(|ui| {
                            draw_avatar(
                                ui,
                                ui.cursor().min + Vec2::splat(20.0),
                                &st.author,
                                st.peer_id,
                                36.0,
                                self.avatars.get(&st.peer_id),
                                self.settings.ayu.avatar_corners,
                            );
                            ui.add_space(46.0);
                            ui.vertical(|ui| {
                                ui.label(RichText::new(&st.author).strong().color(WHITE));
                                ui.label(RichText::new(full_date(i64::from(st.date))).size(11.5).color(SUBTEXT));
                            });
                        });
                        ui.add_space(10.0);
                        egui::Frame::new()
                            .fill(t(SIDEBAR_BG))
                            .corner_radius(egui::CornerRadius::same(12))
                            .inner_margin(egui::Margin::same(14))
                            .show(ui, |ui| {
                                ui.set_min_width(330.0);
                                let body = if st.caption.is_empty() {
                                    "Медиа-история Telegram"
                                } else {
                                    st.caption.as_str()
                                };
                                ui.label(RichText::new(body).size(15.0).color(WHITE));
                            });
                        ui.add_space(10.0);
                        ui.horizontal(|ui| {
                            if idx > 0 && ui.button("← Предыдущая").clicked() {
                                next_idx = Some(idx - 1);
                            }
                            if idx + 1 < self.stories.len() && orange_button(ui, "Следующая →", Vec2::new(130.0, 30.0)).clicked() {
                                next_idx = Some(idx + 1);
                            }
                            if ui.button("Закрыть").clicked() {
                                next_idx = None;
                            }
                        });
                    });
                if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.story_open = None;
                } else {
                    if next_idx != Some(idx)
                        && let Some(ni) = next_idx
                        && let Some(ns) = self.stories.get_mut(ni)
                        && ns.unread
                    {
                        ns.unread = false;
                        if self.settings.ghost.send_read_stories {
                            let (peer, sid) = (ns.peer, ns.story_id);
                            self.send(Cmd::ReadStory(peer, sid));
                        }
                    }
                    self.story_open = next_idx;
                }
            } else {
                self.story_open = None;
            }
        }
        // Комментарии к посту канала (`messages.getDiscussionMessage` + `messages.getReplies`).
        if let Some(disc) = self.discussion.clone() {
            let mut open = true;
            let mut send_txt: Option<String> = None;
            egui::Window::new(format!("Комментарии · пост #{}", disc.post_id))
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([420.0, 460.0])
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    egui::ScrollArea::vertical().max_height(340.0).show(ui, |ui| {
                        if disc.comments.is_empty() {
                            ui.label(RichText::new("Комментариев пока нет. Будьте первым!").color(SUBTEXT));
                        } else {
                            for c in &disc.comments {
                                ui.group(|ui| {
                                    ui.horizontal(|ui| {
                                        let who = if c.sender.is_empty() { "Участник" } else { c.sender.as_str() };
                                        ui.label(RichText::new(who).strong().size(13.0).color(ORANGE));
                                        ui.label(RichText::new(hhmm(c.date)).size(11.5).color(SUBTEXT));
                                    });
                                    let body = if c.text.is_empty() { "[Вложение]" } else { c.text.as_str() };
                                    ui.label(RichText::new(body).color(WHITE));
                                });
                                ui.add_space(4.0);
                            }
                        }
                    });
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        let r = ui.add(
                            egui::TextEdit::singleline(&mut self.comment_input)
                                .hint_text("Написать комментарий…")
                                .desired_width(290.0),
                        );
                        let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                        if (orange_button(ui, "Отправить", Vec2::new(96.0, 30.0)).clicked() || enter)
                            && !self.comment_input.trim().is_empty()
                        {
                            send_txt = Some(std::mem::take(&mut self.comment_input).trim().to_owned());
                        }
                    });
                });
            if let Some(txt) = send_txt {
                self.send(Cmd::SendComment(disc.disc_peer, disc.root_msg_id, disc.post_id, txt));
            }
            if !open {
                self.discussion = None;
            }
        }
        // Подтверждение перед отправкой стикера или GIF (`sticker_confirmation` / `gif_confirmation`).
        if let Some((is_gif, doc_id, reply)) = self.pending_confirm_media {
            let title = if is_gif { "Отправить GIF?" } else { "Отправить стикер?" };
            let mut open = true;
            egui::Window::new(title)
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(if is_gif { "Подтвердите отправку выбранного GIF." } else { "Подтвердите отправку выбранного стикера." });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        if orange_button(ui, "Отправить", Vec2::new(110.0, 32.0)).clicked() {
                            self.reply.take();
                            self.pending_confirm_media = None;
                            self.send(Cmd::SendSticker(doc_id, reply));
                        }
                        if ui.button("Отмена").clicked() {
                            self.pending_confirm_media = None;
                        }
                    });
                });
            if !open {
                self.pending_confirm_media = None;
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
        let media_ids: Vec<i32> = self
            .messages
            .iter()
            .filter(|m| {
                m.id == id
                    || self.textures.contains_key(&m.id)
                    || m.media
                        .as_ref()
                        .is_some_and(|i| i.kind == rg_core::media::MediaKind::Photo)
            })
            .map(|m| m.id)
            .collect();
        let pos = media_ids.iter().position(|&mid| mid == id);
        let prev_id = pos.and_then(|p| p.checked_sub(1)).and_then(|i| media_ids.get(i).copied());
        let next_id = pos.and_then(|p| media_ids.get(p + 1).copied());
        let mut step_to: Option<i32> = None;
        if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft))
            && let Some(pid) = prev_id
        {
            step_to = Some(pid);
        } else if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight))
            && let Some(nid) = next_id
        {
            step_to = Some(nid);
        }
        let tex = self.textures.get(&id).cloned();
        let zoom = self.viewer_zoom;
        let mut acts: Vec<Act> = Vec::new();
        egui::Area::new(egui::Id::new("viewer"))
            .order(egui::Order::Tooltip)
            .fixed_pos(egui::Pos2::ZERO)
            .show(ctx, |ui| {
                let (rect, bg) = ui.allocate_exact_size(screen.size(), Sense::click());
                paint_frosted_scrim(ui.painter(), rect, 1.0);
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
                if let Some(pid) = prev_id {
                    let lr = Rect::from_center_size(Pos2::new(rect.left() + 34.0, rect.center().y), Vec2::splat(44.0));
                    let lresp = ui.interact(lr, ui.make_persistent_id("viewer-prev"), Sense::click());
                    ui.painter().circle_filled(lr.center(), 20.0, t2(WINDOW_BG));
                    ui.painter().text(lr.center(), egui::Align2::CENTER_CENTER, "‹", FontId::proportional(24.0), WHITE);
                    if lresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        step_to = Some(pid);
                    }
                }
                if let Some(nid) = next_id {
                    let rr = Rect::from_center_size(Pos2::new(rect.right() - 34.0, rect.center().y), Vec2::splat(44.0));
                    let rresp = ui.interact(rr, ui.make_persistent_id("viewer-next"), Sense::click());
                    ui.painter().circle_filled(rr.center(), 20.0, t2(WINDOW_BG));
                    ui.painter().text(rr.center(), egui::Align2::CENTER_CENTER, "›", FontId::proportional(24.0), WHITE);
                    if rresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        step_to = Some(nid);
                    }
                }
                let bar = Rect::from_center_size(
                    egui::pos2(rect.center().x, rect.bottom() - 36.0),
                    Vec2::new(360.0, 38.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                    ui.horizontal(|ui| {
                        if orange_button(ui, "Сохранить", Vec2::new(105.0, 32.0)).clicked() {
                            acts.push(Act::Save(id));
                        }
                        if ui.button("Открыть оригинал").clicked() {
                            acts.push(Act::Open(id));
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
        if let Some(new_id) = step_to {
            self.viewer = Some(new_id);
            self.viewer_zoom = 1.0;
            if !self.textures.contains_key(&new_id)
                && let Some(idx) = self.selected
            {
                self.send(Cmd::Preview(idx, new_id));
            }
        } else if close {
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

/// Плавные переливающиеся скелетон-пузыри в стиле iOS, пока грузится история нового чата.
fn paint_message_skeleton(ui: &mut egui::Ui, avail: f32) {
    let time = ui.input(|i| i.time) as f32;
    ui.ctx().request_repaint();
    let op = opacity_f32();
    let items = [
        (false, 220.0_f32, 46.0_f32),
        (false, 160.0, 38.0),
        (true, 240.0, 52.0),
        (false, 280.0, 62.0),
        (true, 180.0, 40.0),
        (true, 210.0, 48.0),
    ];
    let total_h: f32 = items.iter().map(|(_, _, h)| h + 10.0).sum();
    let pad = (ui.available_height() - total_h - 16.0).max(16.0);
    ui.add_space(pad);
    for (idx, (out, w, h)) in items.iter().enumerate() {
        let phase = ((time * 1.4 - idx as f32 * 0.14).sin() * 0.5 + 0.5).clamp(0.0, 1.0);
        let (row, _) = ui.allocate_exact_size(Vec2::new(avail, h + 8.0), Sense::hover());
        let bw = w.min(avail * 0.72);
        let x = if *out { row.right() - bw - 14.0 } else { row.left() + 14.0 };
        let r = Rect::from_min_size(Pos2::new(x, row.top() + 4.0), Vec2::new(bw, *h));
        let base = if *out { MSG_OUT_BG } else { MSG_IN_BG };
        let a = ((110.0 + 65.0 * phase) * op) as u8;
        let fill = Color32::from_rgba_unmultiplied(base.r(), base.g(), base.b(), a);
        ui.painter().rect_filled(r, egui::CornerRadius::same(15), fill);
        // Мягкий бегущий блик.
        let shine_x = r.left() + (r.width() + 80.0) * ((time * 0.75 + idx as f32 * 0.11).fract()) - 40.0;
        let shine = Rect::from_min_size(Pos2::new(shine_x, r.top()), Vec2::new(48.0, *h)).intersect(r);
        if shine.width() > 1.0 {
            ui.painter().rect_filled(
                shine,
                egui::CornerRadius::same(12),
                Color32::from_white_alpha((18.0 * op) as u8),
            );
        }
    }
}

/// Один пузырь сообщения с контекстным меню и плавным iOS-выходом из блюра.
fn bubble(
    ui: &mut egui::Ui,
    m: &MessageRow,
    album: &[&MessageRow],
    show_avatar: bool,
    env: &Env,
    acts: &mut Vec<Act>,
    gif_candidate: &mut Option<(i32, Rect)>,
) {
    let s = env.s;
    let layout = if m.outgoing {
        Layout::right_to_left(Align::TOP)
    } else {
        Layout::left_to_right(Align::TOP)
    };
    let mut k_in = 1.0_f32;
    if let Some(t0) = env.arrived.get(&m.id) {
        let dt = env.time - t0;
        if dt < 0.0 {
            k_in = 0.0;
            ui.ctx().request_repaint();
        } else {
            k_in = (dt / 0.24).clamp(0.0, 1.0) as f32;
            if k_in < 1.0 {
                ui.ctx().request_repaint();
            }
        }
        if k_in < 1.0 {
            // Плавный выезд снизу по кривой iOS (без перелёта).
            ui.add_space(((1.0 - ios_spring(k_in)) * 16.0).max(0.0));
        }
    }
    let ease_in = ios_spring(k_in);
    ui.with_layout(layout, |ui| {
        ui.set_opacity(ease_in.powf(0.75).clamp(0.0, 1.0));
        ui.add_space(6.0);
        if env.group && !m.outgoing {
            if show_avatar {
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
        let mut meta = time_label(m.date, env.format_secs);
        if edited {
            let mark = if s.replace_bottom_info_with_icons {
                "✎"
            } else if s.edited_mark.is_empty() {
                "изм."
            } else {
                s.edited_mark.as_str()
            };
            meta = format!("{mark} {meta}");
        }
        let tick_w = if m.outgoing { 22.0 } else { 0.0 };
        let meta_text_w = ui
            .painter()
            .layout_no_wrap(meta.clone(), FontId::proportional(12.0), date_c)
            .size()
            .x;
        let meta_w = meta_text_w + tick_w + 6.0;
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
        let selected = env.selected_msgs.contains(&m.id);
        let mut frame = egui::Frame::new()
            .fill(fill)
            .corner_radius(radius)
            .inner_margin(egui::Margin::symmetric(MSG_PAD_X as i8, MSG_PAD_Y as i8));
        if selected {
            frame = frame.stroke(egui::Stroke::new(2.0, ORANGE));
        }
        let inner = frame.show(ui, |ui| {
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
                        let snippet_t = if snippet.is_empty() { "сообщение" } else { snippet.as_str() };
                        let resp = if s.simple_quotes_and_replies {
                            ui.label(RichText::new(format!("↪ Ответ: {}", short(snippet_t, 45))).size(12.5).color(bar_c))
                        } else {
                            ui.horizontal(|ui| {
                                let (bar, _) = ui.allocate_exact_size(Vec2::new(2.5, 30.0), Sense::hover());
                                ui.painter().rect_filled(bar, 1.0, bar_c);
                                ui.vertical(|ui| {
                                    ui.spacing_mut().item_spacing.y = 0.0;
                                    ui.label(RichText::new("Ответ").size(12.0).color(bar_c));
                                    ui.label(RichText::new(short(snippet_t, 60)).size(13.0).color(date_c));
                                });
                            })
                            .response
                        };
                        if ui.interact(resp.rect, ui.make_persistent_id(("q", m.id)), Sense::click()).clicked() {
                            acts.push(Act::Goto(*rid));
                        }
                    }
                    if album.len() > 1 {
                        super::media_ui::album(ui, album, w, env.tex, &mut cards);
                    } else if let Some(info) = &m.media {
                        let load = match env.fetching.get(&m.id) {
                            Some(&(d, t)) => super::media_ui::Load::Active(d, t),
                            None if env.cached.contains(&m.id) => super::media_ui::Load::Cached,
                            None => super::media_ui::Load::Remote,
                        };
                        let before = cards.len();
                        super::media_ui::card(ui, m, info, w, env.tex.get(&m.id), text_c, date_c, &mut cards, load);
                        if let Some((card_rect, _)) = cards.get(before) {
                            // Плавное проявление свежезагруженной картинки (эффект выхода из размытия).
                            if let Some(t0) = env.img_arrived.get(&m.id) {
                                let ik = ((env.time - t0) / 0.22).clamp(0.0, 1.0) as f32;
                                if ik < 1.0 {
                                    ui.ctx().request_repaint();
                                    let mist = (1.0 - ios_spring(ik)).powi(2);
                                    ui.painter().rect_filled(
                                        *card_rect,
                                        egui::CornerRadius::same(10),
                                        Color32::from_rgba_unmultiplied(30, 32, 38, (150.0 * mist) as u8),
                                    );
                                }
                            }
                            // Кэшированный GIF, целиком видимый в окне ленты — кандидат для автозапуска.
                            if info.kind == rg_core::media::MediaKind::Gif
                                && env.gif_paths.contains_key(&m.id)
                                && env.clip_rect.contains_rect(*card_rect)
                            {
                                *gif_candidate = Some((m.id, *card_rect));
                            }
                        }
                    }
                    if !m.text.is_empty() && !m.entities.is_empty() {
                        let runs = super::rich::build_runs(&m.text, &m.entities);
                        hot.extend(super::rich::render(
                            ui,
                            env.emoji,
                            env.custom_tex,
                            &runs,
                            15.0,
                            text_c,
                            ORANGE,
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
                    if let Some(tr) = env.translated.get(&m.id) {
                        ui.add_space(4.0);
                        ui.horizontal(|ui| {
                            let (bar, _) = ui.allocate_exact_size(Vec2::new(2.5, 28.0), Sense::hover());
                            ui.painter().rect_filled(bar, 1.0, t_btn(ORANGE));
                            ui.vertical(|ui| {
                                ui.spacing_mut().item_spacing.y = 0.0;
                                ui.label(RichText::new("Перевод").size(11.5).color(ORANGE));
                                ui.label(RichText::new(tr).size(14.0).color(text_c));
                            });
                        });
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
                        // Как в AyuGram: маленькое приглушённое ведро слева от времени, без надписи.
                        let note_w = if m.deleted { 17.0 } else { 0.0 };
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
                        if m.deleted {
                            let mark = if s.deleted_mark.is_empty() { "🗑" } else { s.deleted_mark.as_str() };
                            let resp = ui.label(RichText::new(mark).size(11.5).color(date_c));
                            resp.on_hover_text("удалено");
                        }
                        ui.label(RichText::new(meta).size(12.0).color(date_c));
                        if m.outgoing {
                            let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 12.0), Sense::hover());
                            icons::tick(ui.painter(), r.center(), OUT_TICK, m.id <= env.read_out);
                        }
                    });
                    if env.is_channel && env.has_discussion && !m.deleted {
                        ui.add_space(2.0);
                        ui.separator();
                        let cr = ui.add(
                            egui::Button::new(RichText::new("💬 Комментарии").size(12.5).color(ORANGE))
                                .frame(false),
                        );
                        if cr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(Act::Discuss(m.id));
                        }
                    }
                });
            });
        let rect = inner.response.rect;
        // Хвостик пузыря (выключается настройкой `remove_message_tail`).
        if !s.remove_message_tail && (m.outgoing || show_avatar) {
            let tail = if m.outgoing {
                vec![
                    Pos2::new(rect.right() - 4.0, rect.bottom() - 10.0),
                    Pos2::new(rect.right() + 5.0, rect.bottom() - 2.0),
                    Pos2::new(rect.right() - 8.0, rect.bottom() - 2.0),
                ]
            } else {
                vec![
                    Pos2::new(rect.left() + 4.0, rect.bottom() - 10.0),
                    Pos2::new(rect.left() - 5.0, rect.bottom() - 2.0),
                    Pos2::new(rect.left() + 8.0, rect.bottom() - 2.0),
                ]
            };
            ui.painter().add(egui::Shape::convex_polygon(tail, fill, egui::Stroke::NONE));
        }
        // Кнопка быстрой пересылки справа от поста канала / пересланного сообщения (`!hide_fast_share`).
        if !s.hide_fast_share && !m.deleted && !m.outgoing && (env.is_channel || m.forwarded_from.is_some()) {
            let f_center = Pos2::new(rect.right() + 18.0, rect.center().y);
            let f_rect = Rect::from_center_size(f_center, Vec2::splat(24.0));
            let fresp = ui.interact(f_rect, ui.make_persistent_id(("fshare", m.id)), Sense::click());
            let bg = if fresp.hovered() { t_btn(ORANGE) } else { t2(WINDOW_BG) };
            ui.painter().circle_filled(f_center, 11.5, bg);
            ui.painter().text(f_center, egui::Align2::CENTER_CENTER, "↪", FontId::proportional(13.0), WHITE);
            if fresp.on_hover_text("Быстрая пересылка").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Act::Forward(m.id));
            }
        }
        // Мягкая дымка фокусировки (iOS blur-dissolve), тающая по мере появления сообщения.
        if k_in < 0.99 && blur_on() {
            let mist = (1.0 - ease_in).powi(2);
            ui.painter().rect_filled(
                rect,
                radius,
                Color32::from_rgba_unmultiplied(210, 218, 232, (36.0 * mist * opacity_f32()) as u8),
            );
        }
        // Один отклик на весь пузырь: правый клик — меню, двойной — быстрое действие, левый — ссылка/спойлер/карточка.
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
        if resp.double_clicked() && !m.deleted {
            match env.double_tap_action {
                1 => acts.push(Act::React(m.id, "👍")),
                2 => acts.push(Act::Reply(m.id, m.text.clone())),
                3 if !m.text.is_empty() => acts.push(Act::Copy(m.text.clone())),
                4 => acts.push(Act::Forward(m.id)),
                5 if m.outgoing && !m.text.is_empty() => acts.push(Act::Edit(m.id, m.text.clone())),
                _ => {}
            }
        } else if resp.clicked() {
            let ctrl = ui.ctx().input(|i| i.modifiers.command);
            if ctrl || !env.selected_msgs.is_empty() {
                acts.push(Act::ToggleSelect(m.id));
            } else if let Some(a) = on_card {
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
        if env.show_translate {
            item(ui, "settings/language", "Перевести", Act::Translate(m.id, m.text.clone()));
        }
    }
    if s.show_message_shot && (!m.text.is_empty() || m.media.is_some()) {
        item(ui, "menu/photo", "Снимок сообщения", Act::Shot(m.id));
    }
    if m.media.as_ref().is_some_and(|i| i.is_downloadable()) {
        item(ui, "menu/download", "Сохранить файл", Act::Save(m.id));
        item(ui, "menu/show_in_folder", "Открыть", Act::Open(m.id));
    }
    if m.outgoing && !m.deleted {
        item(ui, "menu/edit", "Изменить", Act::Edit(m.id, m.text.clone()));
    }
    if !m.deleted {
        item(ui, "menu/select", if env.selected_msgs.contains(&m.id) { "Снять выбор" } else { "Выбрать" }, Act::ToggleSelect(m.id));
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
    if s.menu_views_panel.shown(env.shift)
        && let Some(v) = m.views
    {
        ui.label(RichText::new(format!("👁 Просмотров: {v}")).size(12.5).color(SUBTEXT));
    }
    if env.is_channel && env.has_discussion && !m.deleted {
        item(ui, "menu/reply", "Комментарии", Act::Discuss(m.id));
    }
    if s.menu_user_messages.shown(env.shift) && env.group && m.sender_id != 0 {
        item(ui, "menu/search", "Сообщения участника", Act::FilterUser(m.sender_id, m.sender.clone()));
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
