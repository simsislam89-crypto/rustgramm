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
    React(i32, String),
    ReactToggle(i32, String, bool),
    Delete(i32),
    Repeat(String),
    Details(i32),
    Hide(i32),
    AddFilter(String),
    CopyId(i32),
    Save(i32),
    /// Сохранить сообщение в «Избранное» (или скачать медиа по двойному клику).
    SaveToSaved(i32),
    /// Скопировать фото сообщения в системный буфер обмена (на ПК).
    CopyPhoto(i32),
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
    /// Нажатие инлайн- или реплай-кнопки бота (`(msg_id, текст, действие)`).
    BotBtn(i32, String, rg_core::engine::BotBtnKind),
    /// Окончательно удалить уже удалённое сообщение только у себя из локальной БД.
    PurgeLocal(i32),
    /// Окончательно убрать все удалённые сообщения в открытом чате только у себя.
    PurgeAllDeleted,
    /// Скопировать ссылку `t.me/...` на сообщение или пост канала.
    CopyLink(i32),
    /// Отправить платную звёздную реакцию (`messages.sendPaidReaction`).
    PaidReact(i32),
    /// Расшифровать голосовое или видеосообщение (Telegram + оффлайн без лимита).
    Transcribe(i32),
    /// Переключить галочку пункта интерактивного чек-листа (To-Do).
    ToggleTodo(i32, i32, bool),
    /// Глобальный поиск по хэштегу в публичных каналах.
    SearchHashtag(String),
    /// Повторить анимацию эффекта сообщения.
    ReplayEffect(i32),
    /// Открыть окно просмотра и установки набора стикеров.
    OpenStickerPack(i64, i64),
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
    double_tap_action_out: i32,
    default_reaction: &'a str,
    reactions_policy: &'a rg_core::engine::ChatReactionsPolicy,
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
    /// Активное ожидание ответа callback-кнопки бота `(msg_id, btn_text, clicked_at)`.
    bot_btn_pending: Option<(i32, &'a str, f64)>,
    /// Подсветить целевое сообщение после перехода к реплаю/поиску.
    highlight_msg: Option<i32>,
    /// Можно ли скопировать публичную/приватную ссылку `t.me/` на сообщение в этом чате.
    can_copy_link: bool,
    /// Разрешены ли платные звёздные реакции автором этого канала (`ChannelFull.paid_reactions_available`).
    paid_reactions_available: bool,
    /// Готовые расшифровки голосовых / кружков (`msg_id -> текст`).
    transcriptions: &'a HashMap<i32, String>,
    /// Идущие прямо сейчас расшифровки голосовых (`msg_id`).
    transcribing: &'a std::collections::HashSet<i32>,
    /// Активные вспышки эффектов сообщений (`msg_id -> время старта`).
    effect_bursts: &'a HashMap<i32, f64>,
    /// Кэш разобранных прогонов форматирования по `(msg_id, len)`.
    rich_cache: &'a HashMap<(i32, usize), Vec<super::rich::Run>>,
    /// Показывать точные числа без сокращений (`extera.disable_number_rounding`).
    exact_numbers: bool,
    /// Скрыть боковую кнопку пересылки справа от постов (`extera.hide_share_button`).
    hide_share_button: bool,
    /// Время клика по реакции `(msg_id, emoji)` для упругой вспышки-отклика.
    rx_pop: &'a HashMap<(i32, String), f64>,
}

fn time_label(ts: i64, seconds: bool) -> String {
    if seconds { hhmmss(ts) } else { hhmm(ts) }
}

pub(crate) fn bubble_corner_radius(base: u8, single_corner: bool, outgoing: bool) -> egui::CornerRadius {
    if single_corner || base <= 4 {
        return egui::CornerRadius::same(base);
    }
    if outgoing {
        egui::CornerRadius { nw: base, ne: base, sw: base, se: 4 }
    } else {
        egui::CornerRadius { nw: base, ne: base, sw: 4, se: base }
    }
}

pub(crate) fn extract_mention_prefix(input: &str) -> Option<(usize, &str)> {
    let at_pos = input.rfind('@')?;
    if at_pos > 0 {
        let prev = input[..at_pos].chars().next_back()?;
        if !prev.is_whitespace() {
            return None;
        }
    }
    let tail = &input[at_pos + 1..];
    if tail.chars().any(char::is_whitespace) || tail.len() > 32 {
        return None;
    }
    Some((at_pos, tail))
}

pub(crate) fn apply_mention_completion(input: &mut String, at_pos: usize, candidate: &str) {
    if at_pos <= input.len() && input.is_char_boundary(at_pos) {
        input.replace_range(at_pos.., &format!("@{} ", candidate.trim()));
    }
}

pub(crate) fn cycle_search_idx(cur: usize, total: usize, forward: bool) -> usize {
    if total == 0 {
        return 0;
    }
    if forward {
        (cur + 1) % total
    } else if cur == 0 {
        total - 1
    } else {
        cur - 1
    }
}

pub(crate) fn short(text: &str, n: usize) -> String {
    let one: String = text.chars().filter(|c| *c != '\n').take(n).collect();
    if text.chars().count() > n { format!("{one}…") } else { one }
}

/// Рисует анимированную 3-точечную волну («•••») и подпись статуса набора текста.
pub(crate) fn paint_typing_wave(
    p: &egui::Painter,
    ctx: &egui::Context,
    left_center: Pos2,
    color: Color32,
    label: &str,
) {
    let t = ctx.input(|i| i.time);
    for k in 0..3 {
        let phase = (t * 6.5 - f64::from(k) * 0.95).sin().max(0.0) as f32;
        let cx = left_center.x + 3.0 + k as f32 * 5.0;
        let cy = left_center.y - phase * 2.5;
        let r = 1.6 + 0.5 * phase;
        p.circle_filled(Pos2::new(cx, cy), r, color.gamma_multiply(0.55 + 0.45 * phase));
    }
    p.text(
        Pos2::new(left_center.x + 19.0, left_center.y),
        egui::Align2::LEFT_CENTER,
        label,
        FontId::proportional(13.0),
        color,
    );
    ctx.request_repaint_after(std::time::Duration::from_millis(45));
}

fn plural_comments_ex(n: i32, exact: bool) -> String {
    if n <= 0 {
        return "Прокомментировать".to_owned();
    }
    let m10 = n % 10;
    let m100 = n % 100;
    let word = if m10 == 1 && m100 != 11 {
        "комментарий"
    } else if (2..=4).contains(&m10) && !(12..=14).contains(&m100) {
        "комментария"
    } else {
        "комментариев"
    };
    let num = rg_core::engine::format_compact_count(n, exact);
    format!("{num} {word}")
}

/// Разбирает ввод вида `@bot запрос` для запуска встроенного инлайн-бота (`messages.getInlineBotResults`).
pub fn parse_inline_bot_input(input: &str) -> Option<(String, String)> {
    let trimmed = input.trim_start();
    let after_at = trimmed.strip_prefix('@')?;
    let (bot, q) = after_at.split_once([' ', '\t'])?;
    if bot.len() < 3 || bot.len() > 32 || !bot.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return None;
    }
    Some((bot.to_ascii_lowercase(), q.to_owned()))
}

fn pin_media_snippet(m: &MessageRow) -> String {
    let txt = m.text.trim();
    if !txt.is_empty() {
        return short(txt, 72);
    }
    let Some(mi) = &m.media else {
        return "Сообщение".to_owned();
    };
    use rg_core::media::MediaKind as MK;
    match mi.kind {
        MK::Photo => "Фото".to_owned(),
        MK::Video => "Видео".to_owned(),
        MK::Voice => "Голосовое сообщение".to_owned(),
        MK::Round => "Видеосообщение".to_owned(),
        MK::Sticker | MK::AnimatedSticker => "Стикер".to_owned(),
        MK::Gif => "GIF".to_owned(),
        MK::Audio => format!("Аудио: {}", short(&mi.name, 48)),
        MK::File => format!("Файл: {}", short(&mi.name, 48)),
        MK::Poll { ref question, .. } => format!("Опрос: {}", short(question, 48)),
        MK::WebPage { ref title, ref url, .. } => {
            if !title.is_empty() {
                short(title, 56)
            } else {
                short(url, 56)
            }
        }
        MK::Contact { ref first_name, .. } => format!("Контакт: {first_name}"),
        MK::Geo { .. } => "Геопозиция".to_owned(),
        MK::Dice { ref emoticon, .. } => format!("Игра {emoticon}"),
        MK::Other(ref s) => short(s, 48),
    }
}

impl App {
    /// Полноширинная закреплённая полоса под шапкой чата (как в Telegram Desktop / AyuGram).
    pub(crate) fn pinned_bar_ui(&mut self, ui: &mut egui::Ui) {
        let Some(did) = self.selected.and_then(|i| self.dialogs.get(i)).map(|d| d.id) else {
            return;
        };
        let list_len = self.pinned_lists.get(&did).map_or(0, Vec::len);
        let cur_idx = self.pinned_cur_idx.get(&did).copied().unwrap_or(0);
        let pin_opt = self
            .pinned_lists
            .get(&did)
            .and_then(|l| l.get(cur_idx.min(l.len().saturating_sub(1))).cloned())
            .or_else(|| self.pinned_msgs.get(&did).cloned());
        let Some(pin) = pin_opt else {
            return;
        };
        let total = list_len.max(1);
        let display_pos = if total > 1 { (cur_idx % total) + 1 } else { 1 };
        let w = ui.available_width();
        let (bar_rect, _) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::hover());
        let p = ui.painter();
        p.line_segment(
            [bar_rect.left_bottom(), bar_rect.right_bottom()],
            egui::Stroke::new(1.0, Color32::from_white_alpha(20)),
        );
        // Сегментированная вертикальная полоска слева (до 5 сегментов, активный — оранжевый).
        let rail_x = bar_rect.left() + 12.0;
        let rail_top = bar_rect.top() + 8.0;
        let rail_h = 30.0;
        let segs = total.min(5);
        let active_seg = (cur_idx % segs).min(segs - 1);
        if segs <= 1 {
            p.rect_filled(
                Rect::from_min_size(Pos2::new(rail_x, rail_top), Vec2::new(2.5, rail_h)),
                1.2,
                c_orange(),
            );
        } else {
            let gap = 2.0;
            let seg_h = ((rail_h - gap * (segs as f32 - 1.0)) / segs as f32).max(3.0);
            for s_idx in 0..segs {
                let sy = rail_top + s_idx as f32 * (seg_h + gap);
                let col = if s_idx == active_seg {
                    c_orange()
                } else {
                    Color32::from_white_alpha(52)
                };
                p.rect_filled(
                    Rect::from_min_size(Pos2::new(rail_x, sy), Vec2::new(2.5, seg_h)),
                    1.2,
                    col,
                );
            }
        }
        let mut content_x = rail_x + 11.0;
        if let Some(tex) = self.textures.get(&pin.id) {
            let th = Rect::from_center_size(Pos2::new(content_x + 15.0, bar_rect.center().y), Vec2::splat(30.0));
            egui::Image::new(tex)
                .corner_radius(egui::CornerRadius::same(5))
                .fit_to_exact_size(th.size())
                .paint_at(ui, th);
            content_x += 38.0;
        }
        let right_btns_w = 72.0;
        let click_rect = Rect::from_min_max(
            Pos2::new(bar_rect.left(), bar_rect.top()),
            Pos2::new((bar_rect.right() - right_btns_w).max(content_x + 40.0), bar_rect.bottom()),
        );
        let bar_resp = ui.interact(click_rect, ui.make_persistent_id(("pin-bar-click", did)), Sense::click());
        if bar_resp.hovered() {
            ui.painter().rect_filled(
                bar_rect.shrink2(Vec2::new(0.0, 1.0)),
                0.0,
                Color32::from_white_alpha(10),
            );
        }
        let title_str = if total > 1 {
            format!("Закреплённое сообщение #{display_pos} из {total}")
        } else {
            "Закреплённое сообщение".to_owned()
        };
        let snippet = pin_media_snippet(&pin);
        let text_clip = Rect::from_min_max(
            Pos2::new(content_x, bar_rect.top()),
            Pos2::new(click_rect.right() - 8.0, bar_rect.bottom()),
        );
        let cp = ui.painter().with_clip_rect(text_clip);
        cp.text(
            Pos2::new(content_x, bar_rect.center().y - 8.0),
            egui::Align2::LEFT_CENTER,
            title_str,
            FontId::proportional(12.0),
            c_orange(),
        );
        cp.text(
            Pos2::new(content_x, bar_rect.center().y + 8.0),
            egui::Align2::LEFT_CENTER,
            snippet,
            FontId::proportional(12.5),
            SUBTEXT,
        );
        // Правые компактные иконки: список всех закрепов и открепление (`✕`).
        let list_rect = Rect::from_center_size(Pos2::new(bar_rect.right() - 52.0, bar_rect.center().y), Vec2::splat(28.0));
        let lresp = ui.interact(list_rect, ui.make_persistent_id(("pin-bar-all", did)), Sense::click());
        if lresp.hovered() {
            ui.painter().circle_filled(list_rect.center(), 13.0, t(WINDOW_OVER));
        }
        icons::tg_sized(
            ui.painter(),
            "menu/pin",
            list_rect.center(),
            Vec2::splat(15.0),
            if lresp.hovered() { WHITE } else { SUBTEXT },
        );
        let unpin_rect = Rect::from_center_size(Pos2::new(bar_rect.right() - 22.0, bar_rect.center().y), Vec2::splat(28.0));
        let uresp = ui.interact(unpin_rect, ui.make_persistent_id(("pin-bar-x", did)), Sense::click());
        if uresp.hovered() {
            ui.painter().circle_filled(unpin_rect.center(), 13.0, t(WINDOW_OVER));
        }
        icons::tg_sized(
            ui.painter(),
            "info/info_close",
            unpin_rect.center(),
            Vec2::splat(14.0),
            if uresp.hovered() { WHITE } else { SUBTEXT },
        );
        if lresp.on_hover_text("Все закреплённые").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.pinned_list_open = true;
            self.pinned_list_did = Some(did);
            self.send(Cmd::LoadPinnedMessages(did));
        } else if uresp.on_hover_text("Открепить").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.confirm_unpin = Some((did, pin.id));
        } else if bar_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if self.messages.iter().any(|m| m.id == pin.id) {
                self.scroll_to = Some(pin.id);
                self.highlight_msg = Some((pin.id, ui.input(|i| i.time)));
            } else {
                self.send(Cmd::JumpToMessage(did, pin.id));
            }
            if total > 1 {
                self.pinned_cur_idx.insert(did, (cur_idx + 1) % total);
            }
        }
    }

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
                let exact_nums = self.settings.extera.disable_number_rounding;
                let sub = if let Some(lbl) = &live_typing {
                    lbl.clone()
                } else if hide_status {
                    String::new()
                } else if d.saved {
                    "Сохраняйте важные сообщения".to_owned()
                } else if let Some(inf) = info {
                    if inf.members > 0 {
                        let cnt = rg_core::engine::format_compact_count(inf.members, exact_nums);
                        if inf.broadcast {
                            format!("{cnt} подписчиков")
                        } else {
                            format!("{cnt} участников")
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
                    let tex_opt = self.sticker_tex.get(&doc_id).cloned();
                    if let Some(tex) = tex_opt {
                        self.visible_tgs_docs.insert(doc_id);
                        egui::Image::new(&tex)
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
                    if live_typing.is_some() {
                        let start_x = if centered { (hrect.center().x - 50.0).max(text_x) } else { text_x };
                        paint_typing_wave(
                            &ui.painter().with_clip_rect(clip_s),
                            ui.ctx(),
                            Pos2::new(start_x, hrect.center().y + 10.0),
                            sub_color,
                            &subtitle,
                        );
                    } else if centered {
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
                // Три точки: меню чата с фирменной пружинной микроанимацией увеличения и раздвижения точек!
                let (rect, dots) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                let dots_open = egui::Popup::is_id_open(ui.ctx(), dots.id.with("popup"));
                let dots_h = tween(
                    ui.ctx(),
                    "hdr-dots-h",
                    f32::from(u8::from(dots.hovered() || dots_open)),
                    0.16,
                );
                let dots_p = tween(
                    ui.ctx(),
                    "hdr-dots-p",
                    f32::from(u8::from(dots.is_pointer_button_down_on())),
                    0.09,
                );
                let dots_sc = spring_scale(dots_h, dots_p);
                if dots_h > 0.01 {
                    let ring_r = 18.0 * (0.78 + 0.22 * ios_spring_bounce(dots_h));
                    let bg = if dots_open {
                        Color32::from_rgba_unmultiplied(c_orange().r(), c_orange().g(), c_orange().b(), (42.0 * dots_h) as u8)
                    } else {
                        t(WINDOW_OVER).gamma_multiply(dots_h)
                    };
                    ui.painter().circle_filled(rect.center(), ring_r, bg);
                }
                {
                    let dc = lerp_color(DIALOG_DATE, if dots_open { c_orange() } else { WHITE }, dots_h);
                    let spread = 5.0 + 1.4 * ios_spring_bounce(dots_h);
                    let r_dot = 1.75 * dots_sc;
                    let c = rect.center();
                    for dy in [-spread, 0.0, spread] {
                        ui.painter().circle_filled(Pos2::new(c.x, c.y + dy), r_dot, dc);
                    }
                }
                let sel = self.selected.and_then(|i| self.dialogs.get(i).map(|d| (i, d.id, d.name.clone(), d.kind, d.pinned, d.muted, d.archived, d.saved, d.unread, d.peer)));
                let mut act: Option<Cmd> = None;
                let mut search = false;
                let mut open_sched = false;
                let mut open_date_jump = false;
                let mut open_global_deleted = false;
                let mut do_local_read = false;
                let mut export_txt = false;
                let mut open_pinned_list: Option<i64> = None;
                let mut ask_chat_action: Option<(i64, String, bool)> = None;
                egui::Popup::menu(&dots.on_hover_cursor(egui::CursorIcon::PointingHand))
                    .frame(kit::compact_popup_frame())
                    .show(|ui| {
                        ui.set_min_width(184.0);
                        ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                        if kit::menu_item(ui, "menu/search", "Поиск", false) {
                            search = true;
                            ui.close();
                        }
                        if kit::menu_item(ui, "settings/timer", "Перейти к дате", false) {
                            open_date_jump = true;
                            ui.close();
                        }
                        if kit::menu_item(ui, "settings/ttl", "Отложенные / Напоминания", false) {
                            open_sched = true;
                            ui.close();
                        }
                        if let Some((_i, did, dname, dkind, pinned, muted, archived, saved, unread, _peer)) = sel.clone() {
                            if kit::menu_item(ui, "menu/pin", "Все закреплённые", false) {
                                open_pinned_list = Some(did);
                                ui.close();
                            }
                            if kit::menu_item(ui, if pinned { "menu/unpin" } else { "menu/pin" }, if pinned { "Открепить" } else { "Закрепить" }, false) {
                                act = Some(Cmd::PinDialog(did, !pinned));
                                ui.close();
                            }
                            if if muted {
                                kit::menu_item(ui, "menu/unmute", "Включить звук", false)
                            } else {
                                kit::menu_item(ui, "menu/mute", "Отключить звук (1 ч)", false)
                            } {
                                act = Some(if muted { Cmd::MuteDialogFor(did, 0) } else { Cmd::MuteDialogFor(did, 3600) });
                                ui.close();
                            }
                            if !muted {
                                if kit::menu_item(ui, "menu/mute", "Отключить на 8 ч", false) {
                                    act = Some(Cmd::MuteDialogFor(did, 8 * 3600));
                                    ui.close();
                                }
                                if kit::menu_item(ui, "menu/mute", "Отключить на 2 д", false) {
                                    act = Some(Cmd::MuteDialogFor(did, 2 * 86_400));
                                    ui.close();
                                }
                                if kit::menu_item(ui, "menu/mute", "Отключить навсегда", false) {
                                    act = Some(Cmd::MuteDialogFor(did, i32::MAX));
                                    ui.close();
                                }
                            }
                            if unread > 0 && kit::menu_item(ui, "menu/read", "Прочитать локально", false) {
                                do_local_read = true;
                                ui.close();
                            }
                            if kit::menu_item(ui, "menu/read", "Прочитать на сервере", false) {
                                act = Some(Cmd::ForceRead(did));
                                ui.close();
                            }
                            if kit::menu_item(ui, "menu/edited_status", "Удалённые и правки", false) {
                                act = Some(Cmd::SavedHistory(did));
                                ui.close();
                            }
                            if kit::menu_item(ui, "menu/edited_status", "Глобальный журнал удалённых", false) {
                                open_global_deleted = true;
                                ui.close();
                            }
                            if kit::menu_item(ui, "menu/download", "Экспорт чата (TXT)", false) {
                                export_txt = true;
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
                                act = Some(Cmd::Archive(did, !archived));
                                ui.close();
                            }
                            kit::menu_separator(ui);
                            if kit::menu_item(ui, "menu/delete", "Очистить историю", false) {
                                ask_chat_action = Some((did, dname.clone(), false));
                                ui.close();
                            }
                            if !saved
                                && matches!(dkind, rg_core::engine::DialogKind::Group | rg_core::engine::DialogKind::Channel)
                                && kit::menu_item(ui, "menu/leave", "Покинуть чат / канал", false)
                            {
                                ask_chat_action = Some((did, dname, true));
                                ui.close();
                            }
                        }
                    });
                if export_txt && let Some((_, did, dname, ..)) = sel.clone() {
                    let body = rg_core::engine::format_chat_export_txt(&dname, did, &self.messages);
                    let dir = super::worker::home().join("exports");
                    let path = dir.join(format!("chat_{}_{}.txt", did, self.now));
                    match std::fs::create_dir_all(&dir).and_then(|_| std::fs::write(&path, body.as_bytes())) {
                        Ok(()) => {
                            self.status = format!("Экспортировано {} сообщ. в {}", self.messages.len(), path.display());
                        }
                        Err(e) => {
                            self.error = format!("Экспорт чата: {e}");
                            self.error_at = self.now;
                        }
                    }
                }
                if self.settings.ayu.quick_admin_shortcuts
                    && let Some((_i, did, dname, dkind, .., dpeer)) = sel
                    && matches!(dkind, rg_core::engine::DialogKind::Group | rg_core::engine::DialogKind::Channel)
                {
                    let (ar, aresp) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
                    let ah = tween(ui.ctx(), "hdr-adm-h", f32::from(u8::from(aresp.hovered())), 0.14);
                    if ah > 0.01 {
                        ui.painter().circle_filled(ar.center(), 16.0, t(WINDOW_OVER).gamma_multiply(ah));
                    }
                    icons::tg_sized(ui.painter(), "menu/permissions", ar.center(), Vec2::splat(16.0), lerp_color(DIALOG_DATE, c_orange(), ah));
                    if aresp.on_hover_text("Управление группой / каналом").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.admin_log.clear();
                        self.admin_invites.clear();
                        self.admin_modal = Some(super::features_ui::AdminModalState {
                            peer: dpeer,
                            chat_id: did,
                            title: dname,
                            tab: 0,
                            selected_user: None,
                            admin_rights: rg_core::engine::AdminRights::standard_moderator(),
                            admin_rank: String::new(),
                            default_rights: rg_core::engine::BannedRights::default(),
                            mute_preset: 1,
                            log_query: String::new(),
                            invite_title: String::new(),
                            invite_limit: String::new(),
                            invite_request: false,
                        });
                        self.send(Cmd::Participants(did));
                        self.send(Cmd::AdminLoadLog { peer: dpeer, query: String::new() });
                        self.send(Cmd::AdminLoadInvites(dpeer));
                    }
                }
                if let Some(did) = open_pinned_list {
                    self.pinned_list_open = true;
                    self.pinned_list_did = Some(did);
                    self.send(Cmd::LoadPinnedMessages(did));
                }
                if let Some((did, dname, is_leave)) = ask_chat_action {
                    self.confirm_chat_action = Some((did, dname, is_leave, false));
                }
                if open_date_jump {
                    self.date_jump_open = true;
                }
                if open_global_deleted {
                    self.global_deleted_open = true;
                    self.send(Cmd::GlobalDeletedLog(self.global_deleted_query.clone()));
                }
                if do_local_read && let Some(i) = self.selected {
                    self.local_read_dialog(i);
                }
                if open_sched && let Some(did) = self.selected_dialog_id() {
                    self.scheduled_open = true;
                    self.send(Cmd::LoadScheduled(did));
                }
                if let Some(cmd) = act {
                    if let Cmd::ForceRead(did) = cmd
                        && let Some(d) = self.dialogs.iter_mut().find(|d| d.id == did)
                    {
                        d.unread = 0;
                    }
                    if let Cmd::MuteDialog(did, on) = cmd {
                        if let Some(pos) = self.dialogs.iter().position(|d| d.id == did) {
                            self.toggle_mute_dialog(pos, on);
                        } else {
                            self.send(Cmd::MuteDialog(did, on));
                        }
                    } else {
                        self.send(cmd);
                    }
                }
                // Переключатель правой панели со сведениями о чате (пружинная микроанимация).
                let (prect, ptog) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                let ph = tween(ui.ctx(), "hdr-info-h", f32::from(u8::from(ptog.hovered() || self.info_open)), 0.14);
                let pp = tween(ui.ctx(), "hdr-info-p", f32::from(u8::from(ptog.is_pointer_button_down_on())), 0.08);
                if ph > 0.01 {
                    ui.painter().circle_filled(prect.center(), 18.0 * (0.85 + 0.15 * ios_spring_bounce(ph)), t(WINDOW_OVER).gamma_multiply(ph));
                }
                {
                    let sc = spring_scale(ph, pp);
                    let col = lerp_color(DIALOG_DATE, if self.info_open { c_orange() } else { WHITE }, ph);
                    let r = Rect::from_center_size(prect.center(), Vec2::new(18.0 * sc, 15.0 * sc));
                    ui.painter().rect_stroke(r, egui::CornerRadius::same(3), egui::Stroke::new(1.5, col), egui::StrokeKind::Middle);
                    let strip = Rect::from_min_max(egui::pos2(r.right() - 6.0 * sc, r.top() + 1.0), egui::pos2(r.right() - 1.0, r.bottom() - 1.0));
                    ui.painter().rect_filled(strip, egui::CornerRadius::same(1), col);
                }
                if ptog.on_hover_text("Сведения о чате").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.info_open = !self.info_open;
                }
                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(40.0), Sense::click());
                let sh = tween(ui.ctx(), "hdr-srch-h", f32::from(u8::from(resp.hovered() || self.search_open)), 0.14);
                if sh > 0.01 {
                    ui.painter().circle_filled(rect.center(), 18.0 * (0.85 + 0.15 * ios_spring_bounce(sh)), t(WINDOW_OVER).gamma_multiply(sh));
                }
                icons::search(ui.painter(), rect.center(), lerp_color(DIALOG_DATE, if self.search_open { c_orange() } else { WHITE }, sh));
                if resp.on_hover_text("Поиск по чату").clicked() || search {
                    self.search_open = !self.search_open;
                    if !self.search_open {
                        self.chat_search.clear();
                        self.found = None;
                    }
                }
                // Кнопки аудио- и видеозвонка в шапке личного чата (как в Telegram/AyuGram).
                if let Some(d) = self.selected.and_then(|i| self.dialogs.get(i)).cloned()
                    && d.kind == rg_core::engine::DialogKind::User
                    && !d.saved
                {
                    let (vr, vresp) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::click());
                    let vh = tween(ui.ctx(), ("hdr-vcall", d.id), f32::from(u8::from(vresp.hovered())), 0.12);
                    if vh > 0.02 {
                        ui.painter().circle_filled(vr.center(), 18.0, t(WINDOW_OVER).gamma_multiply(vh));
                    }
                    icons::tg_sized(ui.painter(), "menu/video_call", vr.center(), Vec2::splat(18.0), lerp_color(DIALOG_DATE, WHITE, vh));
                    if vresp.on_hover_text("Видеозвонок").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.initiate_call(d.peer, d.id, d.name.clone(), true);
                    }
                    let (cr, cresp) = ui.allocate_exact_size(Vec2::splat(38.0), Sense::click());
                    let ch = tween(ui.ctx(), ("hdr-acall", d.id), f32::from(u8::from(cresp.hovered())), 0.12);
                    if ch > 0.02 {
                        ui.painter().circle_filled(cr.center(), 18.0, t(WINDOW_OVER).gamma_multiply(ch));
                    }
                    icons::tg_sized(ui.painter(), "menu/phone_call", cr.center(), Vec2::splat(18.0), lerp_color(DIALOG_DATE, WHITE, ch));
                    if cresp.on_hover_text("Позвонить").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                        self.initiate_call(d.peer, d.id, d.name.clone(), false);
                    }
                }
            });
        });
    }

    /// Правая панель профиля (как в AyuGram Desktop): шапка, быстрые действия, сведения,
    /// общие медиа/файлы/ссылки и список участников с добавлением новых людей.
    pub(crate) fn info_ui(&mut self, ui: &mut egui::Ui) {
        let Some((_i, d)) = self.selected.and_then(|i| self.dialogs.get(i).map(|d| (i, d.clone()))) else { return };
        if self.info_req.insert(d.id) {
            self.send(Cmd::PeerInfo(d.id));
        }
        let is_group = d.id < 0;
        let is_user = !is_group && !d.saved && d.kind == rg_core::engine::DialogKind::User;
        if is_group && self.participants_req.insert(d.id) {
            self.send(Cmd::Participants(d.id));
        }
        let info = self.peer_infos.get(&d.id).cloned();
        if is_group
            && info.as_ref().is_some_and(|x| x.broadcast)
            && !self.settings.ayu.hide_similar_channels
            && self.similar_req.insert(d.id)
        {
            self.send(Cmd::SimilarChannels(d.id));
        }
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
        let mut start_call_video: Option<bool> = None;
        let mut start_secret = false;
        let mut open_admin = false;
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
                    let tex_opt = self.sticker_tex.get(&doc_id).cloned();
                    if let Some(tex) = tex_opt {
                        self.visible_tgs_docs.insert(doc_id);
                        egui::Image::new(&tex)
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
                        ("menu/settings", "Админ", 8),
                    ]
                } else if d.kind == rg_core::engine::DialogKind::User && !d.saved {
                    vec![
                        ("menu/phone_call", "Звонок", 6),
                        ("menu/video_call", "Видео", 7),
                        (if d.muted { "menu/unmute" } else { "menu/mute" }, if d.muted { "Звук" } else { "Тихо" }, 1),
                        ("settings/lock", "E2E", 9),
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
                    let col = if *code == 5 && blocked { DANGER } else if *code == 9 { GREEN } else if h > 0.4 { Color32::WHITE } else { ORANGE };
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
                            1 => cmd_after = Some(Cmd::MuteDialog(d.id, !d.muted)),
                            2 => start_search = true,
                            3 => start_invite = true,
                            4 => cmd_after = Some(Cmd::ExportInvite(d.id)),
                            5 => cmd_after = Some(Cmd::BlockPeer(d.id, !blocked)),
                            6 => start_call_video = Some(false),
                            7 => start_call_video = Some(true),
                            8 => open_admin = true,
                            9 => start_secret = true,
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
                    icons::tg_sized(p, ico_name, ic_r.center(), Vec2::splat(14.0), if is_cur { super::theme::on_accent() } else { Color32::WHITE });
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
                        crate::copy_text_to_clipboard(ui.ctx(), value);
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
                if info.stargifts_count > 0 {
                    row(
                        ui,
                        "settings/gift",
                        &format!("{} подарков Stars", info.stargifts_count),
                        "Подарки в профиле",
                        false,
                    );
                    if !self.peer_saved_gifts.contains_key(&d.id) && self.saved_gifts_req.insert(d.id) {
                        cmd_after = Some(Cmd::LoadSavedGifts(Some(d.id)));
                    }
                    if let Some(gifts) = self.peer_saved_gifts.get(&d.id)
                        && !gifts.is_empty()
                    {
                        ui.horizontal_wrapped(|ui| {
                            for g in gifts.iter().take(6) {
                                ui.label(
                                    RichText::new(format!("🎁 {}", g.title))
                                        .size(11.5)
                                        .color(ORANGE),
                                );
                            }
                        });
                    }
                }
                if !info.invite_link.is_empty() {
                    row(ui, "menu/link", &info.invite_link, "Ссылка-приглашение", true);
                }
                if self.settings.extera.show_id_and_dc != 0 {
                    let id_dc = match info.dc_id {
                        Some(dc) => format!("{} · DC{} ({})", d.id, dc, rg_core::media::dc_location(dc)),
                        None => d.id.to_string(),
                    };
                    row(
                        ui,
                        "menu/profile",
                        &mask_streamer(&id_dc, streamer),
                        if info.dc_id.is_some() { "ID · DC" } else { "ID" },
                        !streamer,
                    );
                }
            } else {
                ui.add_space(16.0);
                ui.vertical_centered(|ui| ui.label(RichText::new("Загрузка сведений…").color(SUBTEXT)));
            }

            kit::divider(ui);
            // Управление контактом для личного диалога (Добавить / Изменить / Удалить).
            let mut contact_save: Option<(String, String, String)> = None;
            let mut contact_del = false;
            if is_user {
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    if d.contact {
                        if ui.button("Изменить контакт").clicked() {
                            if self.contact_edit_open == Some(d.id) {
                                self.contact_edit_open = None;
                            } else {
                                let mut sp = d.name.splitn(2, ' ');
                                self.contact_first = sp.next().unwrap_or("").to_owned();
                                self.contact_last = sp.next().unwrap_or("").to_owned();
                                self.contact_edit_open = Some(d.id);
                            }
                        }
                        if ui.button(RichText::new("Удалить контакт").color(DANGER)).clicked() {
                            contact_del = true;
                        }
                    } else if orange_button(ui, "+ В контакты", Vec2::new(140.0, 28.0)).clicked() {
                        if self.contact_edit_open == Some(d.id) {
                            self.contact_edit_open = None;
                        } else {
                            let mut sp = d.name.splitn(2, ' ');
                            self.contact_first = sp.next().unwrap_or("").to_owned();
                            self.contact_last = sp.next().unwrap_or("").to_owned();
                            self.contact_edit_open = Some(d.id);
                        }
                    }
                });
                if self.contact_edit_open == Some(d.id) {
                    ui.add_space(6.0);
                    ui.horizontal(|ui| {
                        ui.add_space(14.0);
                        ui.add(egui::TextEdit::singleline(&mut self.contact_first).hint_text("Имя").desired_width(110.0));
                        ui.add(egui::TextEdit::singleline(&mut self.contact_last).hint_text("Фамилия").desired_width(110.0));
                        if orange_button(ui, "OK", Vec2::new(42.0, 26.0)).clicked() {
                            let ph = info.as_ref().map_or_else(String::new, |x| x.phone.clone());
                            contact_save = Some((self.contact_first.clone(), self.contact_last.clone(), ph));
                        }
                    });
                }
                ui.add_space(6.0);
                kit::divider(ui);
            }

            // Похожие каналы (`channels.getChannelRecommendations`) с учётом hide / collapse.
            if is_group
                && info.as_ref().is_some_and(|x| x.broadcast)
                && !self.settings.ayu.hide_similar_channels
                && let Some(sim_list) = self.similar_channels.get(&d.id).cloned()
                && !sim_list.is_empty()
            {
                let collapsed_def = self.settings.ayu.collapse_similar_channels;
                let is_open_sim = !collapsed_def || self.similar_expanded.contains(&d.id);
                ui.horizontal(|ui| {
                    ui.add_space(14.0);
                    let lbl = if is_open_sim {
                        format!("Похожие каналы ({}) ▾", sim_list.len())
                    } else {
                        format!("Похожие каналы ({}) ▸", sim_list.len())
                    };
                    if ui.button(RichText::new(lbl).color(ORANGE).size(12.5)).clicked() {
                        if self.similar_expanded.contains(&d.id) {
                            self.similar_expanded.remove(&d.id);
                        } else {
                            self.similar_expanded.insert(d.id);
                        }
                    }
                });
                if is_open_sim {
                    for sc in sim_list.iter().take(12) {
                        let sub_txt = if !sc.username.is_empty() {
                            format!("@{}", sc.username)
                        } else if sc.participants > 0 {
                            format!("{} подп.", sc.participants)
                        } else {
                            String::new()
                        };
                        if kit::nav_row(ui, "menu/channel", &short(&sc.title, 26), &sub_txt) {
                            open_contact = Some((sc.peer, sc.id, sc.title.clone()));
                        }
                    }
                }
                kit::divider(ui);
            }

            // Вкладки общих материалов, участников и общих групп.
            let tabs: &[(&str, usize)] = if is_group {
                &[("Медиа", 0), ("Файлы", 1), ("Ссылки", 2), ("Люди", 3)]
            } else if is_user {
                &[("Медиа", 0), ("Файлы", 1), ("Ссылки", 2), ("Общие", 3)]
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
                                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
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
                    if is_user {
                        if self.common_chats_req.insert(d.id) {
                            cmd_after = Some(Cmd::CommonChats(d.id));
                        }
                        let common = self.common_chats.get(&d.id).cloned().unwrap_or_default();
                        if common.is_empty() {
                            ui.add_space(16.0);
                            ui.vertical_centered(|ui| ui.label(RichText::new("Нет общих групп").size(12.5).color(SUBTEXT)));
                        } else {
                            for c in &common {
                                if kit::nav_row(ui, "menu/group", &short(&c.name, 26), &c.status) {
                                    open_contact = Some((c.peer, c.id, c.name.clone()));
                                }
                            }
                        }
                    } else {
                        use rg_core::engine::ParticipantFilter as PF;
                        let mut refetch = false;
                        ui.horizontal(|ui| {
                            for (flt, lbl) in [
                                (PF::All, "Все"),
                                (PF::Admins, "Админы"),
                                (PF::Bots, "Боты"),
                            ] {
                                let sel = self.participants_filter == flt;
                                let btn = egui::Button::new(RichText::new(lbl).size(12.0).color(if sel { super::theme::on_accent() } else { SUBTEXT }))
                                    .fill(if sel { c_orange() } else { Color32::from_white_alpha(14) })
                                    .corner_radius(egui::CornerRadius::same(8));
                                if ui.add(btn).clicked() && !sel {
                                    self.participants_filter = flt;
                                    refetch = true;
                                }
                            }
                        });
                        ui.add_space(4.0);
                        let q_resp = ui.add(
                            egui::TextEdit::singleline(&mut self.participants_query)
                                .desired_width(w)
                                .hint_text("Поиск участника…"),
                        );
                        if (q_resp.changed() && (self.participants_query.chars().count() >= 2 || self.participants_query.is_empty()))
                            || refetch
                        {
                            cmd_after = Some(Cmd::ParticipantsFiltered(
                                d.id,
                                self.participants_filter,
                                self.participants_query.clone(),
                            ));
                        }
                        ui.add_space(4.0);
                        let q_low = self.participants_query.trim().to_lowercase();
                        let filtered_members: Vec<_> = members_list
                            .iter()
                            .filter(|c| {
                                q_low.is_empty()
                                    || c.name.to_lowercase().contains(&q_low)
                                    || c.status.to_lowercase().contains(&q_low)
                            })
                            .collect();
                        if filtered_members.is_empty() {
                            ui.add_space(12.0);
                            ui.vertical_centered(|ui| ui.label(RichText::new("Список участников недоступен или пуст").size(12.5).color(SUBTEXT)));
                        } else {
                            let total_m = self.participants_total.get(&d.id).copied().unwrap_or(members_list.len() as i32);
                            egui::ScrollArea::vertical()
                                .max_height(300.0)
                                .id_salt(("info-members", d.id))
                                .show_rows(ui, 48.0, filtered_members.len(), |ui, range| {
                                    for row_i in range {
                                        let c = filtered_members[row_i];
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
                                });
                            if self.participants_filter == PF::All
                                && q_low.is_empty()
                                && (members_list.len() as i32) < total_m
                                && members_list.len() < 1000
                            {
                                ui.add_space(6.0);
                                if ui.button(format!("Загрузить ещё ({}/{})", members_list.len(), total_m)).clicked() {
                                    cmd_after = Some(Cmd::ParticipantsPaged(d.id, members_list.len() as i32));
                                }
                            }
                        }
                    }
                }
            }
            ui.add_space(16.0);
            if let Some((fnm, lnm, ph)) = contact_save {
                cmd_after = Some(Cmd::AddContact {
                    dialog_id: d.id,
                    first_name: fnm,
                    last_name: lnm,
                    phone: ph,
                });
            } else if contact_del {
                cmd_after = Some(Cmd::DeleteContact(d.id));
            }
        });

        self.info_tab = info_tab;
        if let Some(track) = play_track {
            self.send(Cmd::PlayTrack(track));
        }
        if toggle_audio {
            self.toggle_audio_playback();
        }
        if let Some(cmd) = cmd_after {
            if let Cmd::MuteDialog(did, on) = cmd {
                if let Some(pos) = self.dialogs.iter().position(|x| x.id == did) {
                    self.toggle_mute_dialog(pos, on);
                } else {
                    self.send(Cmd::MuteDialog(did, on));
                }
            } else {
                self.send(cmd);
            }
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
            self.send(Cmd::Download(d.id, mid));
        }
        if let Some(video) = start_call_video {
            self.initiate_call(d.peer, d.id, d.name.clone(), video);
        }
        if start_secret {
            self.secret_open = true;
            self.send(Cmd::StartSecretChat(d.peer, d.name.clone()));
        }
        if open_admin {
            self.admin_log.clear();
            self.admin_invites.clear();
            self.admin_modal = Some(super::features_ui::AdminModalState {
                peer: d.peer,
                chat_id: d.id,
                title: d.name.clone(),
                tab: 0,
                selected_user: None,
                admin_rights: rg_core::engine::AdminRights::standard_moderator(),
                admin_rank: String::new(),
                default_rights: rg_core::engine::BannedRights::default(),
                mute_preset: 1,
                log_query: String::new(),
                invite_title: String::new(),
                invite_limit: String::new(),
                invite_request: false,
            });
            self.send(Cmd::Participants(d.id));
            self.send(Cmd::AdminLoadLog { peer: d.peer, query: String::new() });
            self.send(Cmd::AdminLoadInvites(d.peer));
        }
        if let Some(u) = open_url {
            self.apply(ui.ctx(), Act::OpenUrl(u));
        }
    }

    /// Строка поиска по чату под шапкой: текст, навигация ↑/↓ с «k из N» и фильтры `messages.search`.
    pub(crate) fn search_bar(&mut self, ui: &mut egui::Ui) {
        ui.vertical(|ui| {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                let found_len = self.found.as_ref().map_or(0, Vec::len);
                let nav_w = if found_len > 0 { 116.0 } else { 0.0 };
                let w = (ui.available_width() - 168.0 - nav_w).max(110.0);
                let r = ui.add_sized(
                    [w, 28.0],
                    egui::TextEdit::singleline(&mut self.chat_search)
                        .hint_text("Поиск сообщений")
                        .margin(Vec2::new(8.0, 5.0)),
                );
                #[cfg(not(target_os = "android"))]
                if self.chat_search.is_empty() && found_len == 0 {
                    r.request_focus();
                }
                self.hook_android_text(&r, "chat_search", "Поиск по чату", "Текст сообщения", &self.chat_search.clone(), 0);
                let go = orange_button(ui, "Найти", Vec2::new(68.0, 28.0)).clicked()
                    || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                if go && let Some(did) = self.selected_dialog_id() {
                    self.send(Cmd::SearchFiltered(did, self.chat_search.clone(), self.chat_search_filter));
                }
                if found_len > 0 {
                    let cur_one = (self.chat_search_idx.min(found_len - 1)) + 1;
                    ui.label(RichText::new(format!("{cur_one} из {found_len}")).size(12.0).color(SUBTEXT));
                    let mut jump_dir: Option<bool> = None;
                    if ui.small_button("↑").on_hover_text("Предыдущее совпадение").clicked() {
                        jump_dir = Some(false);
                    }
                    if ui.small_button("↓").on_hover_text("Следующее совпадение").clicked() {
                        jump_dir = Some(true);
                    }
                    if let Some(forward) = jump_dir
                        && let Some(rows) = &self.found
                    {
                        self.chat_search_idx = cycle_search_idx(self.chat_search_idx, rows.len(), forward);
                        if let Some(hit) = rows.get(self.chat_search_idx) {
                            let mid = hit.id;
                            if self.messages.iter().any(|m| m.id == mid) {
                                self.scroll_to = Some(mid);
                                self.highlight_msg = Some((mid, ui.input(|i| i.time)));
                            } else if let Some(did) = self.selected.and_then(|i| self.dialogs.get(i)).map(|d| d.id) {
                                self.send(Cmd::JumpToMessage(did, mid));
                            }
                        }
                    }
                }
                if ui.add_sized([72.0, 28.0], egui::Button::new("Закрыть")).clicked() {
                    self.search_open = false;
                    self.chat_search.clear();
                    self.chat_search_filter = rg_core::engine::ChatSearchFilter::All;
                    self.chat_search_idx = 0;
                    self.found = None;
                    if let Some(did) = self.selected_dialog_id() {
                        self.send(Cmd::Search(did, String::new()));
                    }
                }
            });
            ui.add_space(3.0);
            ui.horizontal(|ui| {
                ui.add_space(10.0);
                ui.spacing_mut().item_spacing.x = 5.0;
                for flt in rg_core::engine::ChatSearchFilter::ALL {
                    let sel = self.chat_search_filter == flt;
                    let btn = egui::Button::new(
                        RichText::new(flt.label())
                            .size(11.5)
                            .color(if sel { super::theme::on_accent() } else { SUBTEXT }),
                    )
                    .fill(if sel { t_btn(c_orange()) } else { Color32::from_white_alpha(14) })
                    .corner_radius(egui::CornerRadius::same(11))
                    .min_size(Vec2::new(0.0, 22.0));
                    if ui.add(btn).clicked() {
                        self.chat_search_filter = flt;
                        self.chat_search_idx = 0;
                        if let Some(did) = self.selected_dialog_id() {
                            self.send(Cmd::SearchFiltered(did, self.chat_search.clone(), flt));
                        }
                    }
                }
            });
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
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
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
        if self.discussion.is_some() {
            return;
        }
        self.channel_bottom_bar(ui);
        self.context_bar(ui);
        let s = self.settings.ayu.clone();
        let bot_cmds: Vec<(String, String)> = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .and_then(|d| self.peer_infos.get(&d.id))
            .map(|x| x.bot_commands.clone())
            .unwrap_or_default();
        let has_cmds = !bot_cmds.is_empty() || !self.quick_replies.is_empty();
        let cur_ttl = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .and_then(|d| self.peer_infos.get(&d.id))
            .and_then(|x| x.ttl_period);
        let bot_webapp = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .and_then(|d| self.peer_infos.get(&d.id))
            .and_then(|x| x.bot_menu_webapp.clone());
        let reply_kb: Option<Vec<Vec<rg_core::engine::BotButton>>> = self
            .messages
            .iter()
            .rev()
            .find(|m| !m.reply_keyboard.is_empty())
            .map(|m| m.reply_keyboard.clone());
        let has_reply_kb = reply_kb.is_some();
        let mut bot_act: Option<Act> = None;
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
        // Telegram-попап команд бота + живой автокомплит при вводе `/` (клавиши ↑/↓/Enter).
        let slash_filter = if self.input.starts_with('/') && !self.input.contains(' ') && !bot_cmds.is_empty() {
            Some(self.input.trim_start_matches('/').to_lowercase())
        } else {
            None
        };
        let popup_cmds: Vec<(String, String)> = if let Some(ref f) = slash_filter {
            bot_cmds
                .iter()
                .filter(|(c, _)| f.is_empty() || c.to_lowercase().starts_with(f))
                .cloned()
                .collect()
        } else if self.cmd_popup_open {
            bot_cmds.clone()
        } else {
            Vec::new()
        };
        if (self.cmd_popup_open && has_cmds) || (!popup_cmds.is_empty() && slash_filter.is_some()) {
            let qrs = self.quick_replies.clone();
            let total_rows = popup_cmds.len() + if slash_filter.is_none() { qrs.len() } else { 0 };
            if total_rows > 0 {
                if self.cmd_sel_idx >= total_rows {
                    self.cmd_sel_idx = 0;
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                    self.cmd_sel_idx = (self.cmd_sel_idx + 1) % total_rows;
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                    self.cmd_sel_idx = if self.cmd_sel_idx == 0 { total_rows - 1 } else { self.cmd_sel_idx - 1 };
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Escape)) {
                    self.cmd_popup_open = false;
                }
            }
            let mut send_qr = None;
            let mut send_cmd_now: Option<String> = None;
            let mut fill_cmd: Option<String> = None;
            if let Some((c, _)) = popup_cmds.get(self.cmd_sel_idx)
                && ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
            {
                fill_cmd = Some(format!("/{c} "));
            }
            let shift_held = ui.input(|i| i.modifiers.shift);
            kit::compact_popup_frame()
                .inner_margin(egui::Margin::symmetric(6, 5))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Команды бота").size(12.0).strong().color(c_orange()));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new("↑↓ · Enter / Shift — вставить").size(10.5).color(SUBTEXT));
                        });
                    });
                    ui.add_space(3.0);
                    egui::ScrollArea::vertical()
                        .id_salt("bot-cmd-popup-scroll")
                        .max_height(196.0)
                        .show(ui, |ui| {
                            for (idx, (cmd, desc)) in popup_cmds.iter().enumerate() {
                                let selected = idx == self.cmd_sel_idx;
                                let row_w = ui.available_width();
                                let (rr, rresp) = ui.allocate_exact_size(Vec2::new(row_w, 28.0), Sense::click());
                                if selected || rresp.hovered() {
                                    ui.painter().rect_filled(
                                        rr,
                                        egui::CornerRadius::same(7),
                                        if selected {
                                            t_btn(c_orange()).gamma_multiply(0.22)
                                        } else {
                                            Color32::from_white_alpha(18)
                                        },
                                    );
                                }
                                let cmd_t = format!("/{cmd}");
                                let cmd_gal = ui.painter().layout_no_wrap(cmd_t.clone(), FontId::proportional(13.0), c_orange());
                                ui.painter().text(
                                    Pos2::new(rr.left() + 8.0, rr.center().y),
                                    egui::Align2::LEFT_CENTER,
                                    &cmd_t,
                                    FontId::proportional(13.0),
                                    c_orange(),
                                );
                                if !desc.is_empty() {
                                    let desc_x = rr.left() + 16.0 + cmd_gal.size().x;
                                    let avail_desc = ((rr.right() - 8.0) - desc_x).max(20.0);
                                    let max_c = ((avail_desc / 6.6) as usize).max(6);
                                    ui.painter().with_clip_rect(rr).text(
                                        Pos2::new(desc_x, rr.center().y),
                                        egui::Align2::LEFT_CENTER,
                                        short(desc, max_c),
                                        FontId::proportional(12.0),
                                        DIALOG_DATE,
                                    );
                                }
                                if rresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    if shift_held {
                                        fill_cmd = Some(format!("/{cmd} "));
                                    } else {
                                        send_cmd_now = Some(format!("/{cmd}"));
                                    }
                                    self.cmd_popup_open = false;
                                }
                            }
                            if slash_filter.is_none() {
                                for qr in &qrs {
                                    let lbl = format!("⚡ /{} ({})", qr.shortcut, qr.count);
                                    if ui.small_button(lbl).on_hover_text("Отправить быстрый ответ").clicked() {
                                        send_qr = Some(qr.shortcut_id);
                                        self.cmd_popup_open = false;
                                    }
                                }
                            }
                        });
                });
            if let Some(txt) = fill_cmd {
                self.input = txt;
                self.cmd_popup_open = false;
            } else if let Some(cmd_msg) = send_cmd_now {
                self.input.clear();
                if let Some(did) = self.selected_dialog_id() {
                    let temp_id = self.push_optimistic_message(cmd_msg.clone(), Vec::new(), None, None);
                    self.send(Cmd::Send(did, cmd_msg, None, temp_id));
                }
            }
            if let Some(sid) = send_qr
                && let Some(did) = self.selected_dialog_id()
            {
                self.send(Cmd::SendQuickReply {
                    dialog_id: did,
                    shortcut_id: sid,
                    msg_ids: Vec::new(),
                });
            }
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
                                if let Some(did) = self.selected_dialog_id() {
                                    self.send(Cmd::SetChatTtl(did, sec));
                                }
                                self.ttl_popup_open = false;
                            }
                        }
                    });
                });
        }
        // Живая полноширинная панель записи голосового сообщения (заменяет строку ввода во время записи).
        if let Some(rec) = self.voice_rec.as_ref() {
            ui.ctx().request_repaint_after(std::time::Duration::from_millis(50));
            let secs = rec.elapsed_secs();
            let level = rec.live_level();
            let bars = rec.recent_bars();
            let mut cancel_rec = false;
            let mut send_rec = false;
            ui.horizontal(|ui| {
                ui.set_min_height(46.0);
                ui.add_space(6.0);
                let pulse = ((ui.input(|i| i.time) * 5.0).sin() as f32 * 0.5 + 0.5).clamp(0.25, 1.0);
                let (dr, _) = ui.allocate_exact_size(Vec2::new(18.0, 32.0), Sense::hover());
                ui.painter().circle_filled(dr.center(), 5.0 + level * 2.5, DANGER.gamma_multiply(pulse));
                ui.label(
                    RichText::new(format!("{:02}:{:02}", (secs as u32) / 60, (secs as u32) % 60))
                        .monospace()
                        .strong()
                        .size(14.5)
                        .color(WHITE),
                );
                let wave_w = (ui.available_width() - 190.0).clamp(60.0, 380.0);
                let (wr, _) = ui.allocate_exact_size(Vec2::new(wave_w, 30.0), Sense::hover());
                let count = ((wave_w / 5.0) as usize).clamp(10, 64);
                for idx in 0..count {
                    let v = bars
                        .get(bars.len().saturating_sub(count) + idx)
                        .copied()
                        .unwrap_or(0.08);
                    let h = (4.0 + v * 24.0).min(26.0);
                    let x = wr.left() + idx as f32 * (wave_w / count as f32) + 2.0;
                    ui.painter().line_segment(
                        [Pos2::new(x, wr.center().y - h * 0.5), Pos2::new(x, wr.center().y + h * 0.5)],
                        egui::Stroke::new(2.5, ORANGE),
                    );
                }
                if ui.button("Отмена").clicked() {
                    cancel_rec = true;
                }
                if orange_button(ui, "Отправить", Vec2::new(98.0, 32.0)).clicked() {
                    send_rec = true;
                }
            });
            if cancel_rec {
                if let Some(r) = self.voice_rec.take() {
                    r.cancel();
                }
            } else if send_rec && let Some(r) = self.voice_rec.take() {
                let dest = super::worker::home()
                    .join("cache")
                    .join(format!("voice_{}.wav", self.now));
                match r.finish_to_wav(&dest) {
                    Ok((dur, peaks)) => {
                        let reply_to = self.reply.take().map(|(id, _)| id);
                        if s.voice_confirmation {
                            self.pending_voice_confirm = Some((dest, dur, peaks, reply_to));
                        } else if let Some(did) = self.selected_dialog_id() {
                            self.send(Cmd::SendVoice {
                                dialog_id: did,
                                path: dest,
                                duration: dur,
                                waveform: peaks,
                                reply_to,
                            });
                        }
                    }
                    Err(e) => {
                        self.error = e;
                        self.error_at = self.now;
                    }
                }
            }
            return;
        }
        // Панель подтверждения голосового перед отправкой (`ayu.voice_confirmation`).
        if let Some((vpath, vdur, vpeaks, vreply)) = self.pending_voice_confirm.clone() {
            let mut do_send = false;
            let mut do_discard = false;
            kit::compact_popup_frame()
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(
                            RichText::new(format!("🎤 Голосовое сообщение ({} с)", vdur))
                                .size(13.0)
                                .color(WHITE),
                        );
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if ui.button(RichText::new("Удалить").color(DANGER)).clicked() {
                                do_discard = true;
                            }
                            if orange_button(ui, "Отправить", Vec2::new(98.0, 28.0)).clicked() {
                                do_send = true;
                            }
                        });
                    });
                });
            if do_send {
                self.pending_voice_confirm = None;
                if let Some(did) = self.selected_dialog_id() {
                    self.send(Cmd::SendVoice {
                        dialog_id: did,
                        path: vpath,
                        duration: vdur,
                        waveform: vpeaks,
                        reply_to: vreply,
                    });
                }
            } else if do_discard {
                self.pending_voice_confirm = None;
                let _ = std::fs::remove_file(vpath);
            }
            ui.add_space(3.0);
        }
        // Индикатор медленного режима (Slow Mode) над строкой ввода, если в группе действует задержка.
        let cur_dlg_info = self.selected.and_then(|i| {
            self.dialogs.get(i).map(|d| (i, d.id, d.kind))
        });
        if let Some((_idx, cid, kind)) = cur_dlg_info {
            if matches!(kind, rg_core::engine::DialogKind::Group | rg_core::engine::DialogKind::Channel)
                && cid < -1_000_000_000_000
                && self.send_as_req.insert(cid)
            {
                self.send(Cmd::LoadSendAs(cid));
            }
            if let Some(info) = self.peer_infos.get(&cid) {
                if info.slowmode_next_send_date > 0 && i64::from(info.slowmode_next_send_date) > self.now {
                    let left = (i64::from(info.slowmode_next_send_date) - self.now).max(1);
                    ui.ctx().request_repaint_after(std::time::Duration::from_secs(1));
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format!("Медленный режим: следующая отправка через {left} сек."))
                                .size(12.0)
                                .color(c_orange()),
                        );
                    });
                } else if info.slowmode_seconds > 0 {
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        ui.label(
                            RichText::new(format!("Медленный режим: интервал {} сек.", info.slowmode_seconds))
                                .size(11.5)
                                .color(SUBTEXT),
                        );
                    });
                }
            }
        }
        // Плашка выбранного анимированного эффекта сообщения (если задан).
        if let Some(eff_id) = self.selected_effect {
            let eff_label = self
                .available_effects
                .iter()
                .find(|e| e.id == eff_id)
                .map(|e| format!("{} Эффект", e.emoticon))
                .unwrap_or_else(|| "✨ Эффект сообщения".to_owned());
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new(format!("Эффект: {eff_label}")).size(12.0).color(c_orange()));
                if ui.small_button("×").clicked() {
                    self.selected_effect = None;
                }
            });
        }
        // Всплывающий выбор «Отправить от имени» (Send As) в группах и каналах.
        if self.send_as_popup_open
            && let Some((_, cid, _)) = cur_dlg_info
            && let Some(list) = self.send_as_map.get(&cid).cloned()
        {
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(28)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.label(RichText::new("Отправлять сообщения от имени:").size(12.0).strong().color(WHITE));
                    ui.add_space(4.0);
                    ui.horizontal_wrapped(|ui| {
                        let cur_sel = self.selected_send_as.get(&cid).copied();
                        for item in &list {
                            let active = cur_sel.is_some_and(|p| p.id == item.peer.id);
                            let lbl = item.title.clone();
                            if ui
                                .selectable_label(active, RichText::new(lbl).size(12.5).color(if active { c_orange() } else { WHITE }))
                                .clicked()
                            {
                                self.selected_send_as.insert(cid, item.peer);
                                self.send_as_popup_open = false;
                            }
                        }
                    });
                });
            ui.add_space(3.0);
        }
        // Плашка быстрого отложенного таймера / напоминания (если открыта).
        let mut send_scheduled_now = false;
        let mut send_silent_now = false;
        if self.schedule_picker_open {
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(28)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Отложить через:").size(12.5).color(WHITE));
                        for (mins, lbl) in [(5, "5 мин"), (15, "15 мин"), (60, "1 час"), (180, "3 часа"), (1440, "Завтра")] {
                            let sel = self.schedule_delay_mins == mins;
                            if ui.selectable_label(sel, RichText::new(lbl).size(12.0).color(if sel { c_orange() } else { WHITE })).clicked() {
                                self.schedule_delay_mins = mins;
                            }
                        }
                        if orange_button(ui, "Запланировать", Vec2::new(112.0, 26.0)).clicked() {
                            send_scheduled_now = true;
                        }
                        if ui.small_button("×").clicked() {
                            self.schedule_picker_open = false;
                        }
                    });
                });
            ui.add_space(3.0);
        }
        // Плашка выбора анимированного эффекта при клике из меню Отправки.
        if self.send_menu_open {
            if !self.effects_requested {
                self.effects_requested = true;
                self.send(Cmd::LoadEffects);
            }
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(28)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        if ui.button("Отложить / Напоминание").clicked() {
                            self.schedule_picker_open = true;
                            self.send_menu_open = false;
                        }
                        if ui.button("Без звука").clicked() {
                            send_silent_now = true;
                            self.send_menu_open = false;
                        }
                        if ui.button("Чек-лист").clicked() {
                            self.todo_modal_open = true;
                            self.send_menu_open = false;
                        }
                        if ui.button("📊 Опрос / Викторина").clicked() {
                            self.poll_modal_open = true;
                            self.send_menu_open = false;
                        }
                        if ui.small_button("×").clicked() {
                            self.send_menu_open = false;
                        }
                    });
                    if !self.available_effects.is_empty() {
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            ui.label(RichText::new("✨ Эффект:").size(12.0).color(SUBTEXT));
                            for eff in &self.available_effects {
                                let sel = self.selected_effect == Some(eff.id);
                                if ui
                                    .selectable_label(
                                        sel,
                                        RichText::new(&eff.emoticon).size(16.0),
                                    )
                                    .on_hover_text(if eff.premium_required { "Premium эффект" } else { "Эффект сообщения" })
                                    .clicked()
                                {
                                    self.selected_effect = if sel { None } else { Some(eff.id) };
                                    self.send_menu_open = false;
                                }
                            }
                        });
                    }
                });
            ui.add_space(3.0);
        }
        // Всплывающие результаты инлайн-бота (`@bot запрос`).
        if let Some((bot, query)) = parse_inline_bot_input(&self.input) {
            let key = (bot.clone(), query.clone());
            if self.inline_bot_key.as_ref() != Some(&key) {
                self.inline_bot_key = Some(key);
                self.inline_bot_at = Some(std::time::Instant::now());
                self.inline_bot_error.clear();
            }
            if let Some(at) = self.inline_bot_at {
                if at.elapsed().as_millis() >= 320 {
                    self.inline_bot_at = None;
                    if let Some(did) = self.selected_dialog_id() {
                        self.inline_bot_loading = true;
                        self.send(Cmd::QueryInlineBot {
                            dialog_id: did,
                            bot: bot.clone(),
                            query: query.clone(),
                            offset: String::new(),
                        });
                    }
                } else {
                    ui.ctx().request_repaint_after(std::time::Duration::from_millis(80));
                }
            }
            let reply_id = self.reply.as_ref().map(|(id, _)| *id);
            let mut chosen_result: Option<(i64, String)> = None;
            let mut next_offset_req: Option<String> = None;
            let page_opt = self
                .inline_bot_page
                .as_ref()
                .filter(|p| p.bot_username == bot);
            if self.inline_bot_loading || !self.inline_bot_error.is_empty() || page_opt.is_some() {
                egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(28)))
                    .corner_radius(egui::CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(10, 6))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(format!("@{bot}")).strong().size(12.5).color(c_orange()));
                            if self.inline_bot_loading {
                                ui.add(egui::Spinner::new().size(12.0).color(c_orange()));
                            }
                            if !self.inline_bot_error.is_empty() {
                                ui.label(RichText::new(&self.inline_bot_error).size(12.0).color(DANGER));
                            }
                        });
                        if let Some(page) = page_opt {
                            if page.results.is_empty() && !self.inline_bot_loading {
                                ui.label(RichText::new("Нет результатов").size(12.5).color(SUBTEXT));
                            } else {
                                egui::ScrollArea::vertical().id_salt("inline-bot-popup").max_height(190.0).show(ui, |ui| {
                                    for res in &page.results {
                                        let title = if res.title.is_empty() {
                                            short(&res.send_text, 48)
                                        } else {
                                            res.title.clone()
                                        };
                                        let sub = if !res.description.is_empty() {
                                            short(&res.description, 64)
                                        } else if !res.send_text.is_empty() {
                                            short(&res.send_text, 64)
                                        } else {
                                            res.kind.clone()
                                        };
                                        let lbl = if sub.is_empty() {
                                            title
                                        } else {
                                            format!("{title} — {sub}")
                                        };
                                        if ui.selectable_label(false, lbl).clicked() {
                                            chosen_result = Some((page.query_id, res.id.clone()));
                                        }
                                    }
                                    if !page.next_offset.is_empty() && ui.small_button("Ещё результаты…").clicked() {
                                        next_offset_req = Some(page.next_offset.clone());
                                    }
                                });
                            }
                        }
                    });
                ui.add_space(3.0);
            }
            if let Some((qid, rid)) = chosen_result {
                if let Some(did) = self.selected_dialog_id() {
                    self.send(Cmd::SendInlineResult {
                        dialog_id: did,
                        query_id: qid,
                        id: rid,
                        reply_to: reply_id,
                    });
                }
                self.input.clear();
                self.reply = None;
                self.inline_bot_key = None;
                self.inline_bot_page = None;
                self.inline_bot_at = None;
            } else if let Some(off) = next_offset_req
                && let Some(did) = self.selected_dialog_id()
            {
                self.inline_bot_loading = true;
                self.send(Cmd::QueryInlineBot {
                    dialog_id: did,
                    bot,
                    query,
                    offset: off,
                });
            }
        } else if self.inline_bot_key.is_some() || self.inline_bot_page.is_some() || !self.inline_bot_error.is_empty() {
            self.inline_bot_key = None;
            self.inline_bot_at = None;
            self.inline_bot_page = None;
            self.inline_bot_loading = false;
            self.inline_bot_error.clear();
        }
        // Плашка «Перевести перед отправкой» (`translateBeforeSend`).
        if self.translate_draft_popup && !self.input.trim().is_empty() {
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(28)))
                .corner_radius(egui::CornerRadius::same(12))
                .inner_margin(egui::Margin::symmetric(10, 6))
                .show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        ui.label(RichText::new("Язык:").size(12.0).color(SUBTEXT));
                        for code in ["en", "ru", "uk", "de", "es", "fr", "zh", "ja", "tr", "ar"] {
                            let sel = self.settings.extera.target_language.eq_ignore_ascii_case(code);
                            if ui.selectable_label(sel, code).clicked() {
                                self.settings.extera.target_language = code.to_owned();
                                self.send(Cmd::Settings(Box::new(self.settings.clone())));
                            }
                        }
                        if self.draft_translating {
                            ui.add(egui::Spinner::new().size(13.0).color(c_orange()));
                        } else {
                            let lang = self.settings.extera.target_language.clone();
                            let draft = self.input.trim().to_owned();
                            let rep = self.reply.as_ref().map(|(id, _)| *id);
                            if ui.small_button("Перевести в поле").clicked() {
                                self.draft_translating = true;
                                self.send(Cmd::TranslateDraft {
                                    dialog_id: self.selected_dialog_id(),
                                    text: draft.clone(),
                                    to_lang: lang.clone(),
                                    send_after: false,
                                    reply_to: None,
                                });
                            }
                            if orange_button(ui, "Перевести и отправить", Vec2::new(164.0, 26.0)).clicked() {
                                self.draft_translating = true;
                                self.reply = None;
                                self.send(Cmd::TranslateDraft {
                                    dialog_id: self.selected_dialog_id(),
                                    text: draft,
                                    to_lang: lang,
                                    send_after: true,
                                    reply_to: rep,
                                });
                            }
                        }
                        if ui.small_button("×").clicked() {
                            self.translate_draft_popup = false;
                        }
                    });
                });
            ui.add_space(3.0);
        }

        let mut attach = false;
        let has_send_as = cur_dlg_info
            .and_then(|(_, cid, _)| self.send_as_map.get(&cid))
            .is_some_and(|v| v.len() > 1);
        let line_cnt = self.input.lines().count().clamp(1, 5);
        let row_h = (44.0 + (line_cnt - 1) as f32 * 18.0).min(120.0);
        let is_bot_chat = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .is_some_and(|d| d.kind == rg_core::engine::DialogKind::Bot);
        if is_bot_chat && self.messages.is_empty() && !self.search_open {
            ui.horizontal(|ui| {
                ui.add_space(6.0);
                let bw = (ui.available_width() - 12.0).max(160.0);
                if orange_button(ui, "Запустить бота (/start)", Vec2::new(bw, 38.0)).clicked()
                    && let Some(did) = self.selected_dialog_id()
                {
                    let temp_id = self.push_optimistic_message("/start".to_owned(), Vec::new(), None, None);
                    self.send(Cmd::Send(did, "/start".to_owned(), None, temp_id));
                }
            });
            return;
        }
        // Автодополнение `@упоминаний` участников в группах и каналах.
        if let Some((_idx, cid, kind)) = cur_dlg_info
            && matches!(kind, rg_core::engine::DialogKind::Group | rg_core::engine::DialogKind::Channel)
            && let Some((at_pos, m_prefix)) = extract_mention_prefix(&self.input).map(|(p, q)| (p, q.to_owned()))
        {
            if !self.participants.contains_key(&cid) && self.participants_req.insert(cid) {
                self.send(Cmd::Participants(cid));
            }
            let needle = m_prefix.to_ascii_lowercase();
            let mut candidates: Vec<(i64, String, String)> = Vec::new();
            let mut seen_ids: std::collections::HashSet<i64> = std::collections::HashSet::new();
            if let Some(parts) = self.participants.get(&cid) {
                for p in parts {
                    if !p.name.trim().is_empty() && seen_ids.insert(p.id) {
                        candidates.push((p.id, p.name.clone(), p.status.clone()));
                    }
                }
            }
            for m in self.messages.iter().rev() {
                if m.sender_id != 0 && !m.sender.trim().is_empty() && seen_ids.insert(m.sender_id) {
                    candidates.push((m.sender_id, m.sender.clone(), String::new()));
                }
                if candidates.len() >= 80 {
                    break;
                }
            }
            let matched: Vec<(i64, String, String)> = candidates
                .into_iter()
                .filter(|(_, name, _)| needle.is_empty() || name.to_ascii_lowercase().contains(&needle))
                .take(8)
                .collect();
            if !matched.is_empty() {
                if self.mention_sel_idx >= matched.len() {
                    self.mention_sel_idx = 0;
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) {
                    self.mention_sel_idx = (self.mention_sel_idx + 1) % matched.len();
                }
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) {
                    self.mention_sel_idx = if self.mention_sel_idx == 0 {
                        matched.len() - 1
                    } else {
                        self.mention_sel_idx - 1
                    };
                }
                let mut picked: Option<String> = None;
                if ui.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Tab))
                    && let Some((_, name, _)) = matched.get(self.mention_sel_idx)
                {
                    picked = Some(name.clone());
                }
                kit::compact_popup_frame()
                    .inner_margin(egui::Margin::symmetric(6, 4))
                    .show(ui, |ui| {
                        ui.label(RichText::new("Упомянуть участника").size(11.5).strong().color(c_orange()));
                        ui.add_space(2.0);
                        for (mi, (uid, name, st)) in matched.iter().enumerate() {
                            let sel = mi == self.mention_sel_idx;
                            let (rr, rresp) = ui.allocate_exact_size(Vec2::new(ui.available_width(), 28.0), Sense::click());
                            if sel || rresp.hovered() {
                                ui.painter().rect_filled(
                                    rr,
                                    egui::CornerRadius::same(6),
                                    if sel { t_btn(c_orange()).gamma_multiply(0.22) } else { t2(WINDOW_OVER) },
                                );
                            }
                            draw_avatar(ui, Pos2::new(rr.left() + 14.0, rr.center().y), name, *uid, 20.0, self.avatars.get(uid), 23);
                            ui.painter().text(
                                Pos2::new(rr.left() + 30.0, rr.center().y),
                                egui::Align2::LEFT_CENTER,
                                short(name, 28),
                                FontId::proportional(13.0),
                                WHITE,
                            );
                            if !st.is_empty() {
                                ui.painter().text(
                                    Pos2::new(rr.right() - 8.0, rr.center().y),
                                    egui::Align2::RIGHT_CENTER,
                                    short(st, 18),
                                    FontId::proportional(11.5),
                                    SUBTEXT,
                                );
                            }
                            if rresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                picked = Some(name.clone());
                            }
                        }
                    });
                ui.add_space(2.0);
                if let Some(name) = picked {
                    apply_mention_completion(&mut self.input, at_pos, &name);
                }
            }
        }
        ui.allocate_ui_with_layout(
            Vec2::new(ui.available_width(), row_h),
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
            ui.spacing_mut().item_spacing.x = 2.0;
            // Кнопка меню бота слева (≡), как в Telegram Desktop (Mini App или список команд бота).
            if let Some((w_lbl, w_url)) = &bot_webapp {
                let lbl = short(w_lbl, 11);
                let bw = (lbl.chars().count() as f32 * 7.6 + 34.0).clamp(72.0, 140.0);
                let (mrect, mresp) = ui.allocate_exact_size(Vec2::new(bw, 30.0), Sense::click());
                let mh = tween(ui.ctx(), "cmp-botapp-h", f32::from(u8::from(mresp.hovered())), 0.12);
                let mfill = lerp_color(t_btn(c_orange()), t_btn(ORANGE_OVER), mh);
                ui.painter().rect_filled(mrect, egui::CornerRadius::same(15), mfill);
                let bot_fg = super::theme::on_accent();
                icons::tg_sized(
                    ui.painter(),
                    "dialogs/dialogs_menu",
                    Pos2::new(mrect.left() + 14.0, mrect.center().y),
                    Vec2::splat(14.0),
                    bot_fg,
                );
                ui.painter().text(
                    Pos2::new(mrect.left() + 26.0, mrect.center().y),
                    egui::Align2::LEFT_CENTER,
                    lbl,
                    FontId::proportional(12.5),
                    bot_fg,
                );
                if mresp
                    .on_hover_text(format!("Меню бота: {w_lbl}"))
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                    && let Some(did) = self.selected_dialog_id()
                {
                    self.send(Cmd::OpenMiniApp {
                        dialog_id: did,
                        msg_id: None,
                        title: w_lbl.clone(),
                        url: Some(w_url.clone()),
                        start_param: None,
                        simple: false,
                    });
                }
                ui.add_space(2.0);
            } else if !bot_cmds.is_empty() {
                let (mrect, mresp) = ui.allocate_exact_size(Vec2::new(72.0, 30.0), Sense::click());
                let mh = tween(ui.ctx(), "cmp-botmenu-h", f32::from(u8::from(mresp.hovered() || self.cmd_popup_open)), 0.12);
                let mfill = if self.cmd_popup_open {
                    t_btn(c_orange())
                } else {
                    lerp_color(Color32::from_white_alpha(20), Color32::from_white_alpha(36), mh)
                };
                let bm_fg = if self.cmd_popup_open { super::theme::on_accent() } else { Color32::WHITE };
                ui.painter().rect_filled(mrect, egui::CornerRadius::same(15), mfill);
                icons::tg_sized(
                    ui.painter(),
                    "dialogs/dialogs_menu",
                    Pos2::new(mrect.left() + 14.0, mrect.center().y),
                    Vec2::splat(14.0),
                    bm_fg,
                );
                ui.painter().text(
                    Pos2::new(mrect.left() + 26.0, mrect.center().y),
                    egui::Align2::LEFT_CENTER,
                    "Меню",
                    FontId::proportional(12.5),
                    bm_fg,
                );
                if mresp.on_hover_text("Команды и меню бота").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.cmd_popup_open = !self.cmd_popup_open;
                    self.ttl_popup_open = false;
                }
                ui.add_space(2.0);
            }
            // Все иконки поля ввода стоят на одной линии (высота слота 40 px, визуальный размер 19×19 px!).
            if s.field_attach_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(36.0, 40.0), Sense::click());
                let ah = tween(ui.ctx(), "cmp-att-h", f32::from(u8::from(resp.hovered())), 0.12);
                if ah > 0.02 {
                    ui.painter().circle_filled(rect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(ah));
                }
                icons::attach(ui.painter(), rect.center(), lerp_color(DIALOG_DATE, WHITE, ah));
                let resp = resp
                    .on_hover_text(if s.show_attach_popup {
                        "Меню вложений (файл, опрос, чек-лист, контакт)"
                    } else {
                        "Прикрепить файл · ПКМ: Чек-лист / Опрос / Отложенные"
                    })
                    .on_hover_cursor(egui::CursorIcon::PointingHand);
                let mut render_att_items = |ui: &mut egui::Ui, att: &mut bool| {
                    ui.set_min_width(178.0);
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                    if kit::menu_item(ui, "menu/photo", "Фото / Видео / Файл", false) {
                        *att = true;
                        ui.close();
                    }
                    if kit::menu_item(ui, "menu/select", "Чек-лист (To-Do)", false) {
                        self.todo_modal_open = true;
                        ui.close();
                    }
                    if kit::menu_item(ui, "menu/info", "Опрос / Викторина", false) {
                        self.poll_modal_open = true;
                        ui.close();
                    }
                    if kit::menu_item(ui, "settings/ttl", "Отложенные сообщения", false) {
                        if let Some(did) = self.selected_dialog_id() {
                            self.scheduled_open = true;
                            self.send(Cmd::LoadScheduled(did));
                        }
                        ui.close();
                    }
                    kit::menu_separator(ui);
                    if kit::menu_item(ui, "contacts", "Контакт", false) {
                        self.contact_modal_open = true;
                        ui.close();
                    }
                    if kit::menu_item(ui, "menu/search", "Геопозиция (координаты)", false) {
                        self.geo_modal_open = true;
                        ui.close();
                    }
                };
                if s.show_attach_popup {
                    egui::Popup::menu(&resp)
                        .frame(kit::compact_popup_frame())
                        .show(|ui| render_att_items(ui, &mut attach));
                } else {
                    attach = resp.clicked();
                    egui::Popup::context_menu(&resp)
                        .frame(kit::compact_popup_frame())
                        .show(|ui| render_att_items(ui, &mut attach));
                }
            }
            if has_send_as && let Some((_, cid, _)) = cur_dlg_info {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 40.0), Sense::click());
                let sh = tween(ui.ctx(), "cmp-sas-h", f32::from(u8::from(resp.hovered() || self.send_as_popup_open)), 0.12);
                let sel_id = self
                    .selected_send_as
                    .get(&cid)
                    .map(|p| p.id.bot_api_dialog_id_unchecked())
                    .unwrap_or(0);
                let sel_name = self
                    .send_as_map
                    .get(&cid)
                    .and_then(|v| v.iter().find(|x| x.id == sel_id).or_else(|| v.first()))
                    .map(|x| x.title.clone())
                    .unwrap_or_else(|| self.me.clone());
                draw_avatar(
                    ui,
                    rect.center(),
                    &sel_name,
                    sel_id,
                    24.0,
                    self.avatars.get(&sel_id),
                    23,
                );
                if sh > 0.05 {
                    ui.painter().circle_stroke(rect.center(), 13.0, egui::Stroke::new(1.5, c_orange().gamma_multiply(sh)));
                }
                if resp.on_hover_text(format!("От имени: {sel_name}")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.send_as_popup_open = !self.send_as_popup_open;
                }
            }
            if s.field_commands_button && has_cmds {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 40.0), Sense::click());
                let ch = tween(ui.ctx(), "cmp-cmd-h", f32::from(u8::from(resp.hovered() || self.cmd_popup_open)), 0.12);
                if ch > 0.02 {
                    ui.painter().circle_filled(rect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(ch));
                }
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "/",
                    FontId::proportional(17.5),
                    if self.cmd_popup_open { c_orange() } else { lerp_color(DIALOG_DATE, WHITE, ch) },
                );
                if resp.on_hover_text("Команды бота").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.cmd_popup_open = !self.cmd_popup_open;
                    self.ttl_popup_open = false;
                }
            }
            // Правая часть композера: кнопка меню Mini App теперь вынесена влево (`≡`), как в Telegram.
            // Точный расчёт правой части — все слоты 36×40, визуальные иконки 19×19.
            let has_draft = !self.input.trim().is_empty();
            let right_w = 40.0
                + 36.0
                + if has_draft { 36.0 } else { 0.0 }
                + if has_reply_kb { 36.0 } else { 0.0 }
                + if s.field_emoji_button { 38.0 } else { 0.0 }
                + if s.field_auto_delete_button { 36.0 } else { 0.0 };
            let width = (ui.available_width() - right_w).max(96.0);
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
            let inp_focus = tween(ui.ctx(), "cmp-inp-focus", f32::from(u8::from(focused)), 0.14);
            let inp_stroke = egui::Stroke::new(
                1.0 + 0.35 * inp_focus,
                lerp_color(
                    Color32::from_white_alpha(if super::theme::mobile_glass_active() { 38 } else { 22 }),
                    c_orange().gamma_multiply(0.75),
                    inp_focus,
                ),
            );
            let inp_fill = if super::theme::mobile_glass_active() {
                Color32::from_rgba_unmultiplied(20, 22, 30, ((196.0 + 18.0 * inp_focus) * opacity_f32()) as u8)
            } else {
                Color32::from_rgba_unmultiplied(18, 20, 26, (215.0 * opacity_f32()) as u8)
            };
            let text_out = egui::Frame::new()
                .fill(inp_fill)
                .stroke(inp_stroke)
                .corner_radius(egui::CornerRadius::same(18))
                .inner_margin(egui::Margin::symmetric(10, 2))
                .show(ui, |ui| {
                    let capsule_r = ui.max_rect();
                    let ptr = ui.ctx().input(|i| i.pointer.hover_pos());
                    super::glass::paint_surface_interactive(
                        ui.painter(),
                        capsule_r,
                        18.0,
                        if inp_focus > 0.25 { super::glass::GlassVariant::Tinted(c_orange()) } else { super::glass::GlassVariant::Clear },
                        0.15 + 0.3 * inp_focus,
                        ptr,
                    );
                    ui.allocate_ui(Vec2::new((width - 22.0).max(74.0), (row_h - 10.0).max(34.0)), |ui| {
                        egui::TextEdit::multiline(&mut self.input)
                            .id(id)
                            .desired_width((width - 22.0).max(74.0))
                            .desired_rows(1)
                            .hint_text("Написать сообщение…")
                            .frame(egui::Frame::NONE)
                            .margin(Vec2::new(2.0, 7.0))
                            .show(ui)
                    })
                    .inner
                })
                .inner;
            if let Some(cr) = text_out.cursor_range {
                self.composer_sel = Some((cr.primary.index.into(), cr.secondary.index.into()));
            }
            let r = text_out.response;
            self.hook_android_text(&r, "chat_input", "Сообщение", "Написать сообщение…", &self.input.clone(), 0);
            if r.changed() && self.input != before_input {
                if self.input_undo.last() != Some(&before_input) {
                    self.input_undo.push(before_input);
                    if self.input_undo.len() > 64 {
                        self.input_undo.remove(0);
                    }
                }
                self.input_redo.clear();
                if let Some(idx) = self.selected {
                    self.draft_dirty_at = Some((idx, ui.input(|i| i.time)));
                    if self.input.trim().is_empty() {
                        if let Some((prev_idx, _)) = self.last_typing_sent.take()
                            && let Some(prev_did) = self.dialog_id_at(prev_idx)
                        {
                            self.send(Cmd::SetTyping(prev_did, false));
                        }
                    } else {
                        let should_ping = self.last_typing_sent.is_none_or(|(pi, t0)| {
                            pi != idx || t0.elapsed().as_secs_f32() >= 4.5
                        });
                        if should_ping {
                            self.last_typing_sent = Some((idx, std::time::Instant::now()));
                            if let Some(did) = self.dialog_id_at(idx) {
                                self.send(Cmd::SetTyping(did, true));
                            }
                        }
                    }
                }
            }
            if self.input.is_empty() && !self.input_custom_emojis.is_empty() {
                self.input_custom_emojis.clear();
            }
            // Кнопка панели форматирования [Aa] — выровнена строго по центру и одного размера с остальными.
            {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 40.0), Sense::click());
                let fh = tween(ui.ctx(), "cmp-fmt-h", f32::from(u8::from(resp.hovered() || self.format_bar_open)), 0.12);
                if fh > 0.02 {
                    ui.painter().circle_filled(rect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(fh));
                }
                ui.painter().text(
                    rect.center(),
                    egui::Align2::CENTER_CENTER,
                    "Aa",
                    FontId::proportional(15.0),
                    if self.format_bar_open { c_orange() } else { lerp_color(DIALOG_DATE, WHITE, fh) },
                );
                if resp.on_hover_text("Форматирование (**жирный**, __курсив__, `код`, ||спойлер||)").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.format_bar_open = !self.format_bar_open;
                }
            }
            if has_draft {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 40.0), Sense::click());
                let th = tween(ui.ctx(), "cmp-tr-h", f32::from(u8::from(resp.hovered() || self.translate_draft_popup)), 0.12);
                if th > 0.02 {
                    ui.painter().circle_filled(rect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(th));
                }
                icons::tg_sized(
                    ui.painter(),
                    "settings/language",
                    rect.center(),
                    Vec2::splat(18.0),
                    if self.translate_draft_popup { c_orange() } else { lerp_color(DIALOG_DATE, WHITE, th) },
                );
                if resp
                    .on_hover_text("Перевести перед отправкой")
                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                    .clicked()
                {
                    self.translate_draft_popup = !self.translate_draft_popup;
                }
            } else {
                self.translate_draft_popup = false;
            }
            if has_reply_kb {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 40.0), Sense::click());
                let kh = tween(ui.ctx(), "cmp-kb-h", f32::from(u8::from(resp.hovered() || self.bot_keyboard_open)), 0.12);
                if kh > 0.02 {
                    ui.painter().circle_filled(rect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(kh));
                }
                let kc = if self.bot_keyboard_open { c_orange() } else { lerp_color(DIALOG_DATE, WHITE, kh) };
                let p = ui.painter();
                let c = rect.center();
                let kb_rect = Rect::from_center_size(c, Vec2::new(18.0, 13.0));
                p.rect_stroke(kb_rect, egui::CornerRadius::same(2), egui::Stroke::new(1.5, kc), egui::StrokeKind::Inside);
                for kx in [-4.5, 0.0, 4.5] {
                    p.circle_filled(Pos2::new(c.x + kx, c.y - 2.0), 1.1, kc);
                }
                p.rect_filled(Rect::from_center_size(Pos2::new(c.x, c.y + 2.5), Vec2::new(9.0, 1.8)), 0.5, kc);
                if resp.on_hover_text("Клавиатура бота").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.bot_keyboard_open = !self.bot_keyboard_open;
                }
            }
            if s.field_auto_delete_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(34.0, 40.0), Sense::click());
                let th = tween(ui.ctx(), "cmp-ttl-h", f32::from(u8::from(resp.hovered() || self.ttl_popup_open)), 0.12);
                if th > 0.02 {
                    ui.painter().circle_filled(rect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(th));
                }
                let active_ttl = cur_ttl.is_some_and(|v| v > 0);
                icons::tg_sized(
                    ui.painter(),
                    "settings/ttl",
                    rect.center(),
                    Vec2::splat(19.0),
                    if active_ttl || self.ttl_popup_open { c_orange() } else { lerp_color(DIALOG_DATE, WHITE, th) },
                );
                if resp.on_hover_text("Таймер автоудаления сообщений").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.ttl_popup_open = !self.ttl_popup_open;
                    self.cmd_popup_open = false;
                }
            }
            if s.field_emoji_button {
                let (rect, resp) = ui.allocate_exact_size(Vec2::new(36.0, 40.0), Sense::click());
                self.emoji_btn = rect;
                let k = tween(ui.ctx(), "emoji-btn", f32::from(u8::from(resp.hovered() || self.emoji_open || self.emoji_hover_open)), 0.14);
                let sc = spring_scale(k, 0.0);
                if k > 0.02 {
                    ui.painter().circle_filled(rect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(k));
                }
                let color = lerp_color(DIALOG_DATE, if self.emoji_open { c_orange() } else { WHITE }, k);
                // Единый визуальный размер 19 px (радиус 9.0) — идеально на одной линии с остальными иконками!
                ui.painter().circle_stroke(rect.center(), 9.0 * sc.min(1.08), egui::Stroke::new(1.5, color));
                icons::tg_sized(ui.painter(), "chat/input_smile_face", rect.center(), Vec2::splat(13.0 * sc.min(1.08)), color);
                let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                let is_scrolling = ui.input(|i| i.smooth_scroll_delta.y.abs() > 0.5);
                let any_down = ui.input(|i| i.pointer.any_down());
                let now_em = ui.input(|i| i.time);
                if resp.clicked() {
                    self.emoji_pinned = !self.emoji_pinned;
                    self.emoji_open = self.emoji_pinned;
                    self.emoji_hover_open = false;
                    self.emoji_hover_since = None;
                } else if resp.hovered() && s.show_emoji_popup && !is_scrolling && !any_down {
                    if ui.ctx().content_rect().width() >= super::NARROW_WIDTH
                        && !self.emoji_open
                        && !self.emoji_hover_open
                    {
                        let t0 = *self.emoji_hover_since.get_or_insert(now_em);
                        if now_em - t0 >= 0.26 {
                            self.emoji_hover_open = true;
                            self.emoji_leave_since = None;
                        } else {
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(40));
                        }
                    }
                } else if !self.emoji_hover_open {
                    self.emoji_hover_since = None;
                }
            }
            let (srect, sresp) = ui.allocate_exact_size(Vec2::new(38.0, 40.0), Sense::click());
            let active = !self.input.trim().is_empty();
            let mic_mode = s.field_microphone_button && !active && self.editing.is_none() && self.voice_rec.is_none();
            let send_k = tween(ui.ctx(), "send-active", f32::from(u8::from(active)), 0.14);
            let send_h = tween(ui.ctx(), "send-hover", f32::from(u8::from(sresp.hovered())), 0.12);
            let send_p = tween(ui.ctx(), "send-press", f32::from(u8::from(sresp.is_pointer_button_down_on())), 0.08);
            let sc = spring_scale(send_h, send_p);
            if send_k > 0.02 {
                let r_btn = 16.0 * sc.min(1.12);
                let base = lerp_color(c_orange(), ORANGE_OVER, send_h);
                let fill = Color32::from_rgba_unmultiplied(
                    base.r(),
                    base.g(),
                    base.b(),
                    (f32::from(t_btn(base).a()) * send_k) as u8,
                );
                ui.painter().circle_filled(srect.center(), r_btn, fill);
            } else if mic_mode && send_h > 0.02 {
                ui.painter().circle_filled(srect.center(), 16.0, t(WINDOW_OVER).gamma_multiply(send_h));
            }
            if mic_mode {
                let mc = lerp_color(DIALOG_DATE, c_orange(), send_h);
                let c = srect.center();
                let cap = Rect::from_center_size(Pos2::new(c.x, c.y - 2.5), Vec2::new(7.5, 11.5));
                ui.painter().rect_filled(cap, egui::CornerRadius::same(4), mc);
                ui.painter().line_segment([Pos2::new(c.x - 6.0, c.y - 0.5), Pos2::new(c.x - 6.0, c.y + 3.0)], egui::Stroke::new(1.5, mc));
                ui.painter().line_segment([Pos2::new(c.x + 6.0, c.y - 0.5), Pos2::new(c.x + 6.0, c.y + 3.0)], egui::Stroke::new(1.5, mc));
                ui.painter().line_segment([Pos2::new(c.x - 6.0, c.y + 3.0), Pos2::new(c.x + 6.0, c.y + 3.0)], egui::Stroke::new(1.5, mc));
                ui.painter().line_segment([Pos2::new(c.x, c.y + 3.0), Pos2::new(c.x, c.y + 7.5)], egui::Stroke::new(1.5, mc));
                ui.painter().line_segment([Pos2::new(c.x - 3.8, c.y + 7.5), Pos2::new(c.x + 3.8, c.y + 7.5)], egui::Stroke::new(1.5, mc));
            } else {
                let arrow_c = if active { super::theme::on_accent() } else { DIALOG_DATE };
                icons::send(ui.painter(), srect.center(), arrow_c);
                if self.selected_effect.is_some() {
                    ui.painter().circle_filled(
                        srect.right_top() + Vec2::new(-5.0, 6.0),
                        4.5,
                        c_orange(),
                    );
                }
            }
            let sresp = sresp
                .on_hover_text(if mic_mode {
                    "Записать голосовое · ПКМ: Меню отправки"
                } else {
                    "Отправить · ПКМ: Без звука / Отложить / Эффекты"
                })
                .on_hover_cursor(egui::CursorIcon::PointingHand);
            egui::Popup::context_menu(&sresp)
                .frame(kit::compact_popup_frame())
                .show(|ui| {
                    ui.set_min_width(178.0);
                    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
                    if active && kit::menu_item(ui, "menu/mute", "Отправить без звука", false) {
                        send_silent_now = true;
                        ui.close();
                    }
                    if kit::menu_item(ui, "settings/ttl", "Отложить сообщение", false) {
                        self.schedule_picker_open = true;
                        ui.close();
                    }
                    if kit::menu_item(ui, "dialogs/dialogs_premium", "Эффекты / Опрос / Чек-лист", false) {
                        self.send_menu_open = !self.send_menu_open;
                        ui.close();
                    }
                });
            if mic_mode && sresp.clicked() && self.selected.is_some() {
                match super::audio_io::VoiceRecorder::start() {
                    Ok(rec) => self.voice_rec = Some(rec),
                    Err(e) => {
                        self.error = e;
                        self.error_at = self.now;
                    }
                }
            }
            if (sresp.clicked() || enter || send_scheduled_now || send_silent_now)
                && active
                && self.selected.is_some()
            {
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
                if let Some(did) = self.selected_dialog_id() {
                    if let Some(id) = self.editing.take() {
                        self.send(Cmd::Edit(did, id, clean));
                    } else {
                        let reply = self.reply.take().map(|(id, _)| id);
                        let effect = self.selected_effect.take();
                        let schedule_date = if send_scheduled_now {
                            self.schedule_picker_open = false;
                            Some((self.now + i64::from(self.schedule_delay_mins.max(1)) * 60) as i32)
                        } else {
                            None
                        };
                        let send_as = cur_dlg_info.and_then(|(_, cid, _)| self.selected_send_as.get(&cid).copied());
                        if schedule_date.is_some() || effect.is_some() || send_as.is_some() {
                            self.send(Cmd::SendAdvanced {
                                dialog_id: did,
                                text: clean,
                                entities: ents,
                                reply_to: reply,
                                schedule_date,
                                effect,
                                send_as,
                                silent: send_silent_now,
                            });
                        } else if send_silent_now && ents.is_empty() {
                            let temp_id = self.push_optimistic_message(clean.clone(), Vec::new(), reply, None);
                            self.send(Cmd::SendSilent(did, clean, reply, temp_id));
                        } else if send_silent_now {
                            self.send(Cmd::SendAdvanced {
                                dialog_id: did,
                                text: clean,
                                entities: ents,
                                reply_to: reply,
                                schedule_date: None,
                                effect: None,
                                send_as: None,
                                silent: true,
                            });
                        } else if ents.is_empty() {
                            let temp_id = self.push_optimistic_message(clean.clone(), Vec::new(), reply, None);
                            self.send(Cmd::Send(did, clean, reply, temp_id));
                        } else {
                            let temp_id = self.push_optimistic_message(clean.clone(), ents.clone(), reply, None);
                            self.send(Cmd::SendFormatted(did, clean, ents, reply, temp_id));
                        }
                    }
                }
                r.request_focus();
            }
        });
        // Реплай-клавиатура бота под строкой ввода (`ReplyKeyboardMarkup`) с цветами и плавными анимациями.
        let kb_k = tween(
            ui.ctx(),
            "bot-reply-kb",
            f32::from(u8::from(self.bot_keyboard_open && reply_kb.is_some())),
            0.18,
        );
        if kb_k > 0.02
            && let Some(rows) = reply_kb
        {
            ui.add_space(4.0 * kb_k);
            let avail_w = (ui.available_width() - 8.0).max(120.0);
            for (r_i, row) in rows.iter().enumerate() {
                if row.is_empty() {
                    continue;
                }
                let cols = row.len() as f32;
                let gap = 6.0;
                let bw = ((avail_w - gap * (cols - 1.0).max(0.0)) / cols).max(48.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (c_i, btn) in row.iter().enumerate() {
                        let (rect, resp) = ui.allocate_exact_size(Vec2::new(bw, 36.0 * kb_k), Sense::click());
                        let hk = tween(ui.ctx(), ("rkb-h", r_i, c_i), f32::from(u8::from(resp.hovered())), 0.12);
                        let pk = tween(ui.ctx(), ("rkb-p", r_i, c_i), f32::from(u8::from(resp.is_pointer_button_down_on())), 0.08);
                        let draw_r = rect.shrink(pk * 1.4);
                        let (base_fill, hover_fill, border_c) = bot_btn_palette(btn.style);
                        let fill = lerp_color(base_fill, hover_fill, hk).gamma_multiply(kb_k);
                        ui.painter().rect_filled(draw_r, egui::CornerRadius::same(9), fill);
                        ui.painter().rect_stroke(
                            draw_r,
                            egui::CornerRadius::same(9),
                            egui::Stroke::new(1.0, border_c.gamma_multiply(0.5 + hk * 0.5)),
                            egui::StrokeKind::Inside,
                        );
                        let suffix = match &btn.kind {
                            rg_core::engine::BotBtnKind::WebView { .. } => " ↗",
                            _ => "",
                        };
                        let text_clip = draw_r.shrink2(Vec2::new(6.0, 2.0));
                        ui.painter().with_clip_rect(text_clip).text(
                            draw_r.center(),
                            egui::Align2::CENTER_CENTER,
                            format!("{}{suffix}", short(&btn.text, 28)),
                            FontId::proportional(13.5),
                            Color32::WHITE.gamma_multiply(kb_k),
                        );
                        let resp = resp.on_hover_cursor(egui::CursorIcon::PointingHand);
                if resp.clicked() {
                            bot_act = Some(Act::BotBtn(0, btn.text.clone(), btn.kind.clone()));
                        }
                    }
                });
                ui.add_space(4.0 * kb_k);
            }
        }
        if let Some(a) = bot_act {
            let ctx = ui.ctx().clone();
            self.apply(&ctx, a);
        }
        #[cfg(not(target_os = "android"))]
        if attach
            && self.selected.is_some()
            && let Some(path) = rfd::FileDialog::new().pick_file()
        {
            let reply = self.reply.take().map(|(id, _)| id);
            let caption = std::mem::take(&mut self.input);
            self.media_editor = Some(super::media_editor::MediaEditorState::open(
                path,
                caption.trim().to_owned(),
                reply,
            ));
        }
        #[cfg(target_os = "android")]
        if attach && self.selected.is_some() {
            self.android_pick_target = Some(super::AndroidPickTarget::ChatAttach);
            let dir = crate::worker::home().join("cache").join("picked");
            crate::android_jni::pick_file("*/*", &dir);
        }
    }

    /// Содержимое панели эмодзи/стикеров/GIF (одно и для правой панели, и для всплывающего окна).
    pub(crate) fn emoji_panel_body(&mut self, ui: &mut egui::Ui) {
        let mut picked = None;
        let mut jump = self.emoji_jump.take();
        let mut tab = self.emoji_tab;
        let mut sacts: Vec<super::panels::StickerAct> = Vec::new();
        // Вкладки #13 Segmented Tab Switcher (монохромное стекло Liquid Glass + кнопка закрепления).
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 4.0;
            let pin_w = 34.0;
            let seg_w = (ui.available_width() - pin_w - 8.0).max(160.0);
            ui.allocate_ui(Vec2::new(seg_w, 34.0), |ui| {
                if let Some(n) = super::glass::segmented_tabs(ui, "emoji-panel-seg", &["Эмодзи", "Стикеры", "GIF"], tab) {
                    tab = n;
                    if n == 0 {
                        // Возврат к вкладке «Эмодзи» всегда сбрасывает просматриваемый кастомный набор обратно к базовым эмодзи.
                        self.emoji_pack = None;
                        self.emoji_add_open = false;
                    }
                }
            });
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
            let show_games = !self.emoji_add_open && self.emoji_pack.is_none() && self.emoji_search.is_empty();
            if show_games {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(RichText::new("Игры:").size(11.5).color(SUBTEXT));
                    for em in ["🎲", "🎯", "🏀", "⚽", "🎰", "🎳"] {
                        let r = ui.add(
                            egui::Button::new(RichText::new(em).size(15.5))
                                .fill(Color32::from_white_alpha(12))
                                .corner_radius(egui::CornerRadius::same(8))
                                .min_size(Vec2::new(30.0, 24.0)),
                        );
                        if r.on_hover_text(format!("Бросить {em}")).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            sacts.push(super::panels::StickerAct::SendDice(em.to_owned()));
                        }
                    }
                });
                ui.add_space(4.0);
            }
            let h = (ui.available_height() - if show_games { 48.0 } else { 42.0 }).max(60.0);
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
                    self.stickers_loaded = false;
                    self.send(Cmd::Stickers);
                }
                A::ReloadSets => {
                    self.stickers_requested = true;
                    self.stickers_loaded = false;
                    self.sticker_lists.clear();
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
                    } else if let Some(did) = self.selected_dialog_id() {
                        self.reply.take();
                        self.send(Cmd::SendSticker(did, id, reply));
                    }
                }
                A::SelectSet(i) => self.sticker_cur = i,
                A::Gifs => {
                    self.gifs_requested = true;
                    self.gifs_loaded = false;
                    self.send(Cmd::Gifs);
                }
                A::ReloadGifs => {
                    self.gifs_requested = true;
                    self.gifs_loaded = false;
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
                A::SendDice(em) => {
                    if let Some(did) = self.selected_dialog_id() {
                        let reply = self.reply.take().map(|(r, _)| r);
                        let media = rg_core::media::MediaInfo::basic(
                            rg_core::media::MediaKind::Dice {
                                emoticon: em.clone(),
                                value: 0,
                            },
                            &em,
                            0,
                        );
                        let temp_id = self.push_optimistic_message(String::new(), Vec::new(), reply, Some(media));
                        self.send(Cmd::SendDice(did, em, reply, temp_id));
                    }
                }
                A::VisibleDoc(doc_id) => {
                    self.visible_tgs_docs.insert(doc_id);
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

    /// Всплывающее матовое мини-окно над кнопкой эмодзи (работает И при наведении на ПК, И на узком экране!).
    pub(crate) fn emoji_window(&mut self, ctx: &egui::Context) {
        let screen = ctx.content_rect();
        let is_narrow = screen.width() < super::NARROW_WIDTH;
        let want_open = if is_narrow {
            self.emoji_open
        } else {
            self.emoji_hover_open && !self.emoji_open
        };
        let appear = tween(ctx, "emoji-panel", f32::from(u8::from(want_open)), 0.16);
        if appear < 0.01 {
            return;
        }
        let size = Vec2::new(344.0_f32.min(screen.width() - 16.0), 348.0_f32.min(screen.height() - 110.0));
        let rise = (1.0 - ios_spring_bounce(appear)) * 18.0;
        let pos = egui::pos2(
            (self.emoji_btn.right() - size.x + 20.0).clamp(8.0, (screen.width() - size.x - 8.0).max(8.0)),
            (self.emoji_btn.top() - size.y - 10.0 + rise).max(40.0),
        );
        #[allow(deprecated)]
        let area = egui::Area::new(egui::Id::new("emoji-panel"))
            .order(egui::Order::Foreground)
            .fixed_pos(pos);
        let inner = area.show(ctx, |ui| {
            ui.set_opacity(ios_spring(appear).clamp(0.02, 1.0));
            let card = egui::Frame::new()
                .fill(Color32::TRANSPARENT)
                .corner_radius(egui::CornerRadius::same(16))
                .inner_margin(egui::Margin::symmetric(8, 8))
                .show(ui, |ui| {
                    ui.set_min_size(size);
                    ui.set_max_size(size);
                    let bg_rect = ui.max_rect().expand(8.0);
                    paint_frosted_glass_panel(ui.painter(), bg_rect, 16.0, appear);
                    self.emoji_panel_body(ui);
                });
            let _ = card;
        });
        // Закрытие: Esc, клик вне панели на узком экране, либо 0.22с задержка после ухода курсора на ПК.
        let now_w = ctx.input(|i| i.time);
        let any_down = ctx.input(|i| i.pointer.any_down());
        let any_click = ctx.input(|i| i.pointer.any_click());
        let ptr_pos = ctx.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
        let over = ptr_pos.is_some_and(|p| {
            inner.response.rect.expand(14.0).contains(p) || self.emoji_btn.expand(10.0).contains(p)
        });
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.emoji_open = false;
            self.emoji_hover_open = false;
            self.emoji_pinned = false;
            self.emoji_hover_since = None;
            self.emoji_leave_since = None;
        } else if is_narrow {
            if any_click && !over && !self.emoji_pinned {
                self.emoji_open = false;
            }
        } else if over || any_down {
            self.emoji_leave_since = None;
        } else if appear > 0.35 && self.emoji_hover_open {
            let t0 = *self.emoji_leave_since.get_or_insert(now_w);
            if now_w - t0 >= 0.22 {
                self.emoji_hover_open = false;
                self.emoji_hover_since = None;
                self.emoji_leave_since = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(35));
            }
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
        if self.discussion.is_some() {
            self.discussion_panel(ui);
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
        // Панель тегов «Избранного» (Saved Messages Tags) в верхней части чата «Избранное».
        if self.selected.and_then(|i| self.dialogs.get(i)).is_some_and(|d| d.saved) {
            if !self.saved_tags_req {
                self.saved_tags_req = true;
                self.send(Cmd::LoadSavedTags);
            }
            if !self.saved_tags.is_empty() {
                egui::Frame::new()
                    .fill(t2(WINDOW_BG))
                    .inner_margin(egui::Margin::symmetric(10, 4))
                    .show(ui, |ui| {
                        egui::ScrollArea::horizontal()
                            .id_salt("saved-tags-bar")
                            .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysHidden)
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    let all_sel = self.saved_tag_filter.is_none();
                                    if ui
                                        .selectable_label(all_sel, RichText::new("Все").size(12.5).color(if all_sel { c_orange() } else { WHITE }))
                                        .clicked()
                                    {
                                        self.saved_tag_filter = None;
                                    }
                                    for tag in self.saved_tags.clone() {
                                        let sel = self.saved_tag_filter.as_deref() == Some(tag.emoji.as_str());
                                        let lbl = if tag.title.is_empty() {
                                            format!("{} {}", tag.emoji, tag.count)
                                        } else {
                                            format!("{} {} ({})", tag.emoji, tag.title, tag.count)
                                        };
                                        let r = ui.selectable_label(
                                            sel,
                                            RichText::new(lbl).size(12.5).color(if sel { c_orange() } else { WHITE }),
                                        );
                                        if r.clicked() {
                                            self.saved_tag_filter = if sel { None } else { Some(tag.emoji.clone()) };
                                        }
                                        if r.secondary_clicked() {
                                            self.saved_tag_edit = Some((tag.emoji.clone(), tag.title.clone()));
                                        }
                                    }
                                });
                            });
                    });
            }
            if let Some((em, mut title)) = self.saved_tag_edit.clone() {
                let mut save = false;
                let mut close = false;
                ui.horizontal(|ui| {
                    ui.add_space(10.0);
                    ui.label(RichText::new(format!("Тег {em}:")).size(12.0).color(SUBTEXT));
                    ui.add(egui::TextEdit::singleline(&mut title).hint_text("Название тега").desired_width(140.0));
                    if ui.small_button("Сохранить").clicked() {
                        save = true;
                    }
                    if ui.small_button("×").clicked() {
                        close = true;
                    }
                });
                if save {
                    self.send(Cmd::UpdateSavedTag(em, title));
                    self.saved_tag_edit = None;
                } else if close {
                    self.saved_tag_edit = None;
                } else {
                    self.saved_tag_edit = Some((em, title));
                }
            }
        }
        if let Some((_idx, cid, _, _, _)) = dlg_meta
            && let Some(topics) = self.forum_topics.get(&cid).cloned()
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
                        if ui.small_button(if self.new_topic_open { "×" } else { "+ Тема" }).clicked() {
                            self.new_topic_open = !self.new_topic_open;
                        }
                    });
                });
            if self.new_topic_open {
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    let r = ui.add(
                        egui::TextEdit::singleline(&mut self.new_topic_title)
                            .hint_text("Название новой темы…")
                            .desired_width(180.0),
                    );
                    let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                    let valid = !self.new_topic_title.trim().is_empty();
                    if (orange_button(ui, "Создать", Vec2::new(76.0, 24.0)).clicked() || enter) && valid {
                        let title = std::mem::take(&mut self.new_topic_title).trim().to_owned();
                        self.new_topic_open = false;
                        self.send(Cmd::CreateForumTopic(cid, title));
                    }
                });
            }
            if let Some(pick) = picked_topic {
                self.active_topic = pick;
                if let Some(tid) = pick
                    && let Some(did) = self.selected_dialog_id()
                {
                    self.topic_msgs = None;
                    self.send(Cmd::TopicMessages(did, tid));
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
        if self.date_jump_active
            && let Some(did) = self.selected_dialog_id()
        {
            egui::Frame::new()
                .fill(t2(WINDOW_BG))
                .inner_margin(egui::Margin::symmetric(10, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new("Просмотр истории на выбранную дату").size(12.5).color(ORANGE));
                        if ui.small_button("← К новым сообщениям").clicked() {
                            self.date_jump_active = false;
                            self.history_loading = true;
                            self.send(Cmd::Open(did, true));
                        }
                    });
                });
        }
        let viewport_h = ui.available_height();
        let clip_rect = ui.clip_rect();
        // Быстрый скролл при нажатии колёсика мыши (Middle Click Auto-Scroll) и ускорение колеса (Shift/Alt/Middle ×3.5).
        let ptr_pos = ui.input(|i| i.pointer.hover_pos().or_else(|| i.pointer.interact_pos()));
        let mid_pressed = ui.input(|i| i.pointer.button_pressed(egui::PointerButton::Middle));
        let mid_down = ui.input(|i| i.pointer.button_down(egui::PointerButton::Middle));
        let mid_released = ui.input(|i| i.pointer.button_released(egui::PointerButton::Middle));
        let any_other_click = ui.input(|i| {
            i.pointer.button_clicked(egui::PointerButton::Primary)
                || i.pointer.button_clicked(egui::PointerButton::Secondary)
                || i.key_pressed(egui::Key::Escape)
        });
        // Автоскролл работает И при зажатии колёсика мыши (hold + drag), И при клике (toggle).
        if mid_pressed
            && let Some(p) = ptr_pos
            && clip_rect.contains(p)
        {
            if self.autoscroll_anchor.is_some() && !self.autoscroll_holding {
                self.autoscroll_anchor = None;
                self.autoscroll_holding = false;
                self.autoscroll_dragged = false;
            } else {
                self.autoscroll_anchor = Some(p);
                self.autoscroll_holding = true;
                self.autoscroll_dragged = false;
            }
        }
        if self.autoscroll_holding
            && mid_down
            && let (Some(anchor), Some(cur)) = (self.autoscroll_anchor, ptr_pos)
            && (cur - anchor).length() > 7.0
        {
            self.autoscroll_dragged = true;
        }
        if mid_released && self.autoscroll_holding {
            self.autoscroll_holding = false;
            if self.autoscroll_dragged {
                self.autoscroll_anchor = None;
                self.autoscroll_dragged = false;
            }
        } else if any_other_click {
            self.autoscroll_anchor = None;
            self.autoscroll_holding = false;
            self.autoscroll_dragged = false;
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
            if diff.abs() > 5.0 {
                ui.ctx().request_repaint();
                let sign = diff.signum();
                let mag = (diff.abs() - 5.0).powf(1.12) * 0.28;
                sign * mag.min(95.0)
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
        let cur_avail = ui.available_width();
        if (cur_avail - self.msg_last_avail).abs() > 2.0 {
            self.msg_heights.clear();
            self.msg_last_avail = cur_avail;
        }
        let is_bot = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .is_some_and(|d| d.kind == rg_core::engine::DialogKind::Bot);
        let bot_desc = self
            .selected
            .and_then(|i| self.dialogs.get(i))
            .and_then(|d| self.peer_infos.get(&d.id))
            .map(|x| x.bot_description.clone())
            .unwrap_or_default();
        let mut measured_heights: Vec<(i32, f32)> = Vec::new();
        // Кэшируем разбор форматирования до сборки immutable Env (0 переаллокаций на скролле).
        if self.rich_runs_cache.len() > 2000 {
            self.rich_runs_cache.clear();
        }
        {
            let topic_slice = self.active_topic.and_then(|tid| {
                self.topic_msgs
                    .as_ref()
                    .filter(|(mtid, _)| *mtid == tid)
                    .map(|(_, rows)| rows.as_slice())
            });
            let src: &[MessageRow] = if searching {
                self.found.as_deref().unwrap_or(&self.messages)
            } else {
                topic_slice.unwrap_or(&self.messages)
            };
            for m in src {
                if !m.text.is_empty() && !m.entities.is_empty() {
                    self.rich_runs_cache
                        .entry((m.id, m.text.len()))
                        .or_insert_with(|| super::rich::build_runs(&m.text, &m.entities));
                }
            }
        }
        let (out, out_rendered, out_docs) = {
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
            let now_t = ui.input(|i| i.time);
            let active_highlight = self
                .highlight_msg
                .filter(|(_, t0)| now_t - *t0 < 1.8)
                .map(|(id, _)| id);
            if active_highlight.is_some() {
                ui.ctx().request_repaint_after(std::time::Duration::from_millis(60));
            } else {
                self.highlight_msg = None;
            }
            let cur_dialog = self.selected.and_then(|i| self.dialogs.get(i));
            let cur_info = cur_dialog.and_then(|d| self.peer_infos.get(&d.id));
            let can_copy_link = cur_dialog.is_some_and(|d| {
                d.id < -1_000_000_000_000 || cur_info.is_some_and(|p| !p.username.trim().is_empty())
            });
            // Платные звёздные реакции доступны ТОЛЬКО в каналах, где автор включил их на сервере!
            let paid_reactions_available =
                is_channel && cur_info.is_some_and(|p| p.paid_reactions_available);
            static DEFAULT_RX_POLICY: rg_core::engine::ChatReactionsPolicy =
                rg_core::engine::ChatReactionsPolicy::All { allow_custom: true };
            let reactions_policy = cur_info
                .map(|p| &p.available_reactions)
                .unwrap_or(&DEFAULT_RX_POLICY);
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
                bot_btn_pending: self
                    .bot_btn_pending
                    .as_ref()
                    .filter(|(_, _, t0)| now_t - *t0 < 6.0)
                    .map(|(mid, lbl, t0)| (*mid, lbl.as_str(), *t0)),
                highlight_msg: active_highlight,
                can_copy_link,
                paid_reactions_available,
                transcriptions: &self.transcriptions,
                transcribing: &self.transcribing,
                effect_bursts: &self.effect_bursts,
                rich_cache: &self.rich_runs_cache,
                exact_numbers: self.settings.extera.disable_number_rounding,
                hide_share_button: self.settings.extera.hide_share_button,
                rx_pop: &self.rx_pop,
                avail: cur_avail,
                s: &self.settings.ayu,
                tex: &self.textures,
                shift,
                scroll_to,
                group,
                is_channel,
                has_discussion,
                format_secs: self.settings.ayu.show_message_seconds || self.settings.extera.format_time_with_seconds,
                double_tap_action: self.settings.extera.double_tap_action,
                double_tap_action_out: self.settings.extera.double_tap_action_out_owner,
                default_reaction: if self.settings.extera.default_reaction.is_empty() {
                    "👍"
                } else {
                    self.settings.extera.default_reaction.as_str()
                },
                reactions_policy,
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
            let heights = &self.msg_heights;
            let unread_anchor = if !searching { self.open_unread_anchor } else { None };
            let disable_greeting = self.settings.ayu.disable_greeting_sticker;
            let mut rendered_ids: Vec<i32> = Vec::with_capacity(32);
            let mut rendered_docs: Vec<i64> = Vec::with_capacity(16);
            let scroll_out = egui::ScrollArea::vertical()
                .auto_shrink([false; 2])
                .stick_to_bottom(!searching && self.autoscroll_anchor.is_none() && self.active_topic.is_none())
                .show_viewport(ui, |ui, viewport| {
                    if (fast_wheel_delta + autoscroll_dy).abs() > 0.1 {
                        ui.scroll_with_delta(Vec2::new(0.0, fast_wheel_delta + autoscroll_dy));
                    }
                    let origin_y = ui.cursor().min.y;
                    let mut prev_date: Option<i64> = None;
                    let mut unread_shown = false;
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
                    if visible.is_empty() && !searching {
                        if is_bot {
                            ui.vertical_centered(|ui| {
                                ui.add_space(16.0);
                                egui::Frame::new()
                                    .fill(t2(WINDOW_BG))
                                    .corner_radius(egui::CornerRadius::same(14))
                                    .inner_margin(egui::Margin::same(16))
                                    .show(ui, |ui| {
                                        ui.set_max_width(340.0);
                                        ui.label(RichText::new("Что умеет этот бот?").strong().size(15.0).color(ORANGE));
                                        ui.add_space(6.0);
                                        let desc_txt = if bot_desc.trim().is_empty() {
                                            "Нажмите «СТАРТ», чтобы запустить бота и загрузить его меню и кнопки."
                                        } else {
                                            bot_desc.trim()
                                        };
                                        ui.label(RichText::new(desc_txt).size(13.5).color(WHITE));
                                        ui.add_space(10.0);
                                        if orange_button(ui, "СТАРТ (/start)", Vec2::new(220.0, 36.0)).clicked() {
                                            acts.push(Act::Repeat("/start".to_owned()));
                                        }
                                    });
                            });
                        } else if !disable_greeting {
                            ui.vertical_centered(|ui| {
                                ui.add_space(20.0);
                                egui::Frame::new()
                                    .fill(t2(WINDOW_BG))
                                    .corner_radius(egui::CornerRadius::same(14))
                                    .inner_margin(egui::Margin::same(14))
                                    .show(ui, |ui| {
                                        ui.set_max_width(280.0);
                                        ui.label(RichText::new("Сообщений пока нет…").strong().size(14.0).color(WHITE));
                                        ui.add_space(6.0);
                                        if orange_button(ui, "Поздороваться 👋", Vec2::new(190.0, 32.0)).clicked() {
                                            acts.push(Act::Repeat("Привет! 👋".to_owned()));
                                        }
                                    });
                            });
                        }
                    }
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
                        let new_day = prev_date.is_none_or(|p| !same_day(p, m.date));
                        prev_date = Some(m.date);
                        let show_unread = !unread_shown
                            && unread_anchor.is_some_and(|a| !m.outgoing && m.id > 0 && m.id > a);
                        if show_unread {
                            unread_shown = true;
                        }
                        // Виртуализация для больших чатов: сообщения далеко за пределами видимого окна
                        // с известной высотой пропускаются без виджетов (ui.add_space) и не грузят CPU.
                        let cur_rel_y = ui.cursor().min.y - origin_y;
                        if album.len() == 1
                            && env.scroll_to != Some(m.id)
                            && let Some(&cached_h) = heights.get(&m.id)
                            && (cur_rel_y + cached_h + 36.0 < viewport.min.y - 360.0
                                || cur_rel_y > viewport.max.y + 360.0)
                        {
                            if new_day {
                                ui.add_space(34.0);
                            }
                            if show_unread {
                                ui.add_space(28.0);
                            }
                            ui.add_space(cached_h + 4.0);
                            i = j;
                            continue;
                        }
                        if new_day && day_pill(ui, env.avail, day_label(m.date, now)) {
                            self.date_jump_open = true;
                        }
                        if show_unread {
                            let (ur, _) = ui.allocate_exact_size(Vec2::new(env.avail, 26.0), Sense::hover());
                            ui.painter().rect_filled(ur.shrink2(Vec2::new(0.0, 2.0)), 0.0, Color32::from_white_alpha(12));
                            ui.painter().text(
                                ur.center(),
                                egui::Align2::CENTER_CENTER,
                                "Непрочитанные сообщения",
                                FontId::proportional(12.0),
                                c_orange(),
                            );
                        }
                        for item in album {
                            rendered_ids.push(item.id);
                            for e in &item.entities {
                                if let rg_core::engine::EntityKind::CustomEmoji(doc_id) = e.kind {
                                    rendered_docs.push(doc_id);
                                }
                            }
                            for pill in &item.reaction_items {
                                if let Some(doc_id) = pill.custom_doc_id {
                                    rendered_docs.push(doc_id);
                                }
                            }
                        }
                        let y_before = ui.cursor().min.y;
                        if album.len() > 1 {
                            let caption = album.iter().find(|x| !x.text.is_empty()).copied().unwrap_or(album[album.len() - 1]);
                            bubble(ui, caption, album, show_avatar, &env, &mut acts, &mut gif_candidate);
                        } else {
                            bubble(ui, m, &[], show_avatar, &env, &mut acts, &mut gif_candidate);
                            let h = (ui.cursor().min.y - y_before).max(20.0);
                            measured_heights.push((m.id, h));
                        }
                        ui.add_space(4.0);
                        i = j;
                    }
                    ui.add_space(8.0);
                });
            (scroll_out, rendered_ids, rendered_docs)
        };
        self.visible_tgs_msgs.clear();
        self.visible_tgs_msgs.extend(out_rendered);
        self.visible_tgs_docs.extend(out_docs);
        if self.msg_heights.len() > 4000 {
            let live: std::collections::HashSet<i32> = self.messages.iter().map(|m| m.id).collect();
            self.msg_heights.retain(|id, _| live.contains(id));
        }
        for (mid, h) in measured_heights {
            self.msg_heights.insert(mid, h);
        }
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
        // Плавающие кнопки быстрых переходов (@ упоминания, ❤ реакции) и анимированная кнопка «Вниз» с векторной стрелкой.
        if self.selected_msgs.is_empty() {
            let show_cnt = !self.settings.ayu.hide_notification_counters;
            let (mentions_cnt, reactions_cnt) = self
                .selected
                .and_then(|i| self.dialogs.get(i))
                .map_or((0, 0), |d| (d.unread_mentions, d.unread_reactions));
            let down_raw = tween(
                ui.ctx(),
                "scroll-down-fab-vis",
                f32::from(u8::from(self.scrolled_up)),
                0.18,
            );
            let down_k = ios_spring_bounce(down_raw);
            let square = self.settings.extera.square_fab;
            let mut fab_y = clip_rect.bottom() - 32.0 - 54.0 * down_raw;

            if down_raw > 0.02 {
                let slide_y = (1.0 - down_k) * 16.0;
                let center = Pos2::new(clip_rect.right() - 32.0, clip_rect.bottom() - 32.0 + slide_y);
                let fab_sz = (42.0 * (0.68 + 0.32 * down_k)).max(4.0);
                let fab_rect = Rect::from_center_size(center, Vec2::splat(fab_sz));
                let resp = ui.interact(fab_rect, ui.make_persistent_id("scroll-down-fab"), Sense::click());
                let hk = tween(ui.ctx(), "scroll-down-fab-h", f32::from(u8::from(resp.hovered())), 0.12);
                let base_bg = lerp_color(
                    Color32::from_rgba_unmultiplied(29, 29, 34, (236.0 * down_raw) as u8),
                    Color32::from_rgba_unmultiplied(44, 44, 52, (246.0 * down_raw) as u8),
                    hk,
                );
                let edge = Color32::from_rgba_unmultiplied(
                    lerp_color(Color32::WHITE, c_orange(), hk).r(),
                    lerp_color(Color32::WHITE, c_orange(), hk).g(),
                    lerp_color(Color32::WHITE, c_orange(), hk).b(),
                    ((36.0 + 90.0 * hk) * down_raw) as u8,
                );
                let p = ui.painter();
                let shadow_a = (44.0 * down_raw) as u8;
                if super::theme::mobile_glass_active() {
                    let g_rect = if square {
                        fab_rect
                    } else {
                        Rect::from_center_size(center, Vec2::splat(fab_sz))
                    };
                    let g_rad = if square { 13.0 } else { fab_sz * 0.5 };
                    super::glass::paint_surface(p, g_rect, g_rad, super::glass::GlassVariant::Clear, hk);
                } else if square {
                    p.rect_filled(fab_rect.translate(Vec2::new(0.0, 2.0)).expand(2.0), egui::CornerRadius::same(14), Color32::from_black_alpha(shadow_a));
                    p.rect_filled(fab_rect, egui::CornerRadius::same(13), base_bg);
                    p.rect_stroke(fab_rect, egui::CornerRadius::same(13), egui::Stroke::new(1.0, edge), egui::StrokeKind::Inside);
                } else {
                    let rad = fab_sz * 0.5;
                    p.circle_filled(center + Vec2::new(0.0, 2.0), rad + 2.0, Color32::from_black_alpha(shadow_a));
                    p.circle_filled(center, rad, base_bg);
                    p.circle_stroke(center, rad, egui::Stroke::new(1.0, edge));
                }
                // Чёткий векторный шеврон вниз вместо системного символа.
                let chev_s = 6.2 * (0.7 + 0.3 * down_k);
                let chev_c = lerp_color(WHITE, c_orange(), hk).gamma_multiply(down_raw.clamp(0.0, 1.0));
                let pts = [
                    Pos2::new(center.x - chev_s, center.y - chev_s * 0.35),
                    Pos2::new(center.x, center.y + chev_s * 0.52),
                    Pos2::new(center.x + chev_s, center.y - chev_s * 0.35),
                ];
                p.add(egui::Shape::line(pts.to_vec(), egui::Stroke::new(2.1, chev_c)));

                if unread_cnt > 0 && !self.settings.ayu.hide_notification_counters {
                    let badge = Rect::from_center_size(center - Vec2::new(0.0, 22.0 * down_k), Vec2::new(28.0, 17.0));
                    p.rect_filled(badge, egui::CornerRadius::same(8), t_btn(c_orange()).gamma_multiply(down_raw));
                    p.text(
                        badge.center(),
                        egui::Align2::CENTER_CENTER,
                        unread_cnt.min(999).to_string(),
                        FontId::proportional(10.5),
                        super::theme::on_accent().gamma_multiply(down_raw),
                    );
                }
                if resp.on_hover_text("Вниз").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                    self.scroll_to = self.messages.last().map(|m| m.id);
                }
            }

            if mentions_cnt > 0 && let Some(sel) = self.selected {
                let c = Pos2::new(clip_rect.right() - 32.0, fab_y);
                let r = Rect::from_center_size(c, Vec2::splat(40.0));
                let resp = ui.interact(r, ui.make_persistent_id("mention-jump-fab"), Sense::click());
                if super::theme::mobile_glass_active() {
                    let v = if resp.hovered() {
                        super::glass::GlassVariant::Tinted(ORANGE)
                    } else {
                        super::glass::GlassVariant::Regular
                    };
                    super::glass::paint_surface(ui.painter(), r, if square { 13.0 } else { 20.0 }, v, 0.25);
                } else {
                    let bg = if resp.hovered() { t_btn(ORANGE) } else { t2(WINDOW_BG) };
                    if square {
                        ui.painter().rect_filled(r, egui::CornerRadius::same(13), bg);
                        ui.painter().rect_stroke(r, egui::CornerRadius::same(13), egui::Stroke::new(1.0, Color32::from_white_alpha(32)), egui::StrokeKind::Inside);
                    } else {
                        ui.painter().circle_filled(c, 20.0, bg);
                        ui.painter().circle_stroke(c, 20.0, egui::Stroke::new(1.0, Color32::from_white_alpha(32)));
                    }
                }
                ui.painter().text(c, egui::Align2::CENTER_CENTER, "@", FontId::proportional(17.0), if resp.hovered() && !super::theme::mobile_glass_active() { super::theme::on_accent() } else { WHITE });
                if show_cnt {
                    let b = Rect::from_center_size(c - Vec2::new(0.0, 22.0), Vec2::new(24.0, 16.0));
                    ui.painter().rect_filled(b, egui::CornerRadius::same(8), t_btn(ORANGE));
                    ui.painter().text(b.center(), egui::Align2::CENTER_CENTER, mentions_cnt.min(99).to_string(), FontId::proportional(10.5), super::theme::on_accent());
                }
                if resp.clicked() {
                    if let Some(did) = self.dialog_id_at(sel) {
                        self.send(Cmd::JumpUnreadMention(did));
                    }
                } else if resp.secondary_clicked()
                    && let Some(did) = self.dialog_id_at(sel)
                {
                    self.send(Cmd::ReadAllMentions(did));
                }
                let _ = resp.on_hover_text("Следующее @упоминание (ПКМ — прочитать все)").on_hover_cursor(egui::CursorIcon::PointingHand);
                fab_y -= 54.0;
            }
            if reactions_cnt > 0 && let Some(sel) = self.selected {
                let c = Pos2::new(clip_rect.right() - 32.0, fab_y);
                let r = Rect::from_center_size(c, Vec2::splat(40.0));
                let resp = ui.interact(r, ui.make_persistent_id("react-jump-fab"), Sense::click());
                let rose = Color32::from_rgb(232, 74, 95);
                if super::theme::mobile_glass_active() {
                    let v = if resp.hovered() {
                        super::glass::GlassVariant::Tinted(rose)
                    } else {
                        super::glass::GlassVariant::Regular
                    };
                    super::glass::paint_surface(ui.painter(), r, if square { 13.0 } else { 20.0 }, v, 0.25);
                } else {
                    let bg = if resp.hovered() { rose } else { t2(WINDOW_BG) };
                    if square {
                        ui.painter().rect_filled(r, egui::CornerRadius::same(13), bg);
                        ui.painter().rect_stroke(r, egui::CornerRadius::same(13), egui::Stroke::new(1.0, Color32::from_white_alpha(32)), egui::StrokeKind::Inside);
                    } else {
                        ui.painter().circle_filled(c, 20.0, bg);
                        ui.painter().circle_stroke(c, 20.0, egui::Stroke::new(1.0, Color32::from_white_alpha(32)));
                    }
                }
                ui.painter().text(c, egui::Align2::CENTER_CENTER, "❤", FontId::proportional(15.0), WHITE);
                if show_cnt {
                    let b = Rect::from_center_size(c - Vec2::new(0.0, 22.0), Vec2::new(24.0, 16.0));
                    ui.painter().rect_filled(b, egui::CornerRadius::same(8), rose);
                    ui.painter().text(b.center(), egui::Align2::CENTER_CENTER, reactions_cnt.min(99).to_string(), FontId::proportional(10.5), Color32::WHITE);
                }
                if resp.clicked() {
                    if let Some(did) = self.dialog_id_at(sel) {
                        self.send(Cmd::JumpUnreadReaction(did));
                    }
                } else if resp.secondary_clicked()
                    && let Some(did) = self.dialog_id_at(sel)
                {
                    self.send(Cmd::ReadAllReactions(did));
                }
                let _ = resp.on_hover_text("Следующая реакция ❤ (ПКМ — прочитать все)").on_hover_cursor(egui::CursorIcon::PointingHand);
            }
        }
        // Отрисовка автоскролла колёсиком мыши: иконка прикреплена прямо к курсору (`cur`), а не застывает в воздухе.
        if let Some(anchor) = self.autoscroll_anchor {
            let p = ui.painter();
            let badge_pos = ptr_pos.unwrap_or(anchor);
            let dy = badge_pos.y - anchor.y;
            // Тонкая точка-ориентир старта и направляющая линия к курсору.
            p.circle_filled(anchor, 3.5, t_btn(ORANGE).gamma_multiply(0.65));
            p.line_segment(
                [anchor, badge_pos],
                egui::Stroke::new(1.2, t_btn(ORANGE).gamma_multiply(0.45)),
            );
            // Круглый значок автоскролла, движущийся вместе с курсором.
            p.circle_filled(badge_pos, 15.0, t2(WINDOW_BG));
            p.circle_stroke(badge_pos, 15.0, egui::Stroke::new(1.6, t_btn(ORANGE)));
            p.circle_filled(badge_pos, 2.8, t_btn(ORANGE));
            p.text(
                Pos2::new(badge_pos.x, badge_pos.y - 8.5),
                egui::Align2::CENTER_CENTER,
                "▲",
                FontId::proportional(8.5),
                if dy < -5.0 { ORANGE } else { WHITE },
            );
            p.text(
                Pos2::new(badge_pos.x, badge_pos.y + 8.5),
                egui::Align2::CENTER_CENTER,
                "▼",
                FontId::proportional(8.5),
                if dy > 5.0 { ORANGE } else { WHITE },
            );
            ui.ctx().set_cursor_icon(egui::CursorIcon::ResizeVertical);
        }
        // Достигли верха — просим более старую историю.
        if !searching && out.state.clamped_offset().y <= 4.0 && out.content_size.y > out.inner_rect.height() {
            want_older = true;
        }
        if want_older && !self.older_pending && !self.history_end
            && let Some(first) = self.messages.iter().find(|m| !m.deleted)
            && let Some(did) = self.selected_dialog_id()
        {
            self.older_pending = true;
            self.send(Cmd::Older(did, first.id));
        }
        for a in acts {
            self.apply(ui.ctx(), a);
        }
        // Плавающая панель действий при множественном выделении сообщений.
        if !self.selected_msgs.is_empty() {
            let bar_w = 560.0_f32.min(clip_rect.width() - 16.0);
            let bar = Rect::from_min_size(
                Pos2::new(clip_rect.center().x - bar_w * 0.5, clip_rect.bottom() - 48.0),
                Vec2::new(bar_w, 40.0),
            );
            super::glass::paint_surface_interactive(
                ui.painter(),
                bar,
                14.0,
                super::glass::GlassVariant::Regular,
                0.18,
                ptr_pos,
            );
            let mut clear_sel = false;
            let mut open_fwd = false;
            let mut open_shot = false;
            let mut save_to_saved = false;
            let mut delete_batch = false;
            let mut copy_sel = !ui.ctx().egui_wants_keyboard_input()
                && ui.input(|i| i.modifiers.command && i.key_pressed(egui::Key::C));
            let count = self.selected_msgs.len();
            let has_saved_dlg = self.dialogs.iter().any(|d| d.saved);
            ui.scope_builder(egui::UiBuilder::new().max_rect(bar.shrink2(Vec2::new(10.0, 4.0))), |ui| {
                ui.horizontal_centered(|ui| {
                    ui.label(RichText::new(format!("Выбрано: {count}")).strong().color(WHITE));
                    ui.add_space(4.0);
                    if orange_button(ui, "Переслать", Vec2::new(86.0, 28.0)).clicked() {
                        open_fwd = true;
                    }
                    if has_saved_dlg && ui.button("В Избранное").clicked() {
                        save_to_saved = true;
                    }
                    if ui.button("Снимок").clicked() {
                        open_shot = true;
                    }
                    if ui.button("Копия").clicked() {
                        copy_sel = true;
                    }
                    if ui.button(RichText::new("Удалить").color(DANGER)).clicked() {
                        delete_batch = true;
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
                    crate::copy_text_to_clipboard(ui.ctx(), &text);
                    self.status = "Выделенные тексты скопированы".to_owned();
                }
                clear_sel = true;
            }
            if save_to_saved
                && let Some(did) = self.selected_dialog_id()
                && let Some(saved_did) = self.dialogs.iter().find(|d| d.saved).map(|d| d.id)
            {
                let ids: Vec<i32> = self
                    .messages
                    .iter()
                    .filter(|m| self.selected_msgs.contains(&m.id) && m.id > 0)
                    .map(|m| m.id)
                    .collect();
                if !ids.is_empty() {
                    self.send(Cmd::ForwardBatch(did, ids, saved_did, false));
                }
                clear_sel = true;
            }
            if delete_batch && let Some(did) = self.selected_dialog_id() {
                let backups: Vec<(i32, String, i64, bool, String)> = self
                    .messages
                    .iter()
                    .filter(|m| self.selected_msgs.contains(&m.id) && m.id > 0)
                    .map(|m| (m.id, m.text.clone(), m.date, m.outgoing, m.sender.clone()))
                    .collect();
                if !backups.is_empty() {
                    self.send(Cmd::DeleteBatch { dialog_id: did, backups });
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

    fn discussion_panel(&mut self, ui: &mut egui::Ui) {
        let Some(disc) = self.discussion.clone() else {
            return;
        };
        let avail = ui.available_width();
        let ch_name = if !disc.channel_name.is_empty() {
            disc.channel_name.clone()
        } else {
            self.selected
                .and_then(|i| self.dialogs.get(i))
                .map(|d| d.name.clone())
                .unwrap_or_default()
        };
        let post_txt = if !disc.post_preview.is_empty() {
            disc.post_preview.clone()
        } else {
            self.messages
                .iter()
                .find(|m| m.id == disc.post_id)
                .map(|m| m.text.clone())
                .unwrap_or_default()
        };
        // Верхняя полоса ветки обсуждения (← Комментарии).
        let mut go_back = false;
        let (hdr, _) = ui.allocate_exact_size(Vec2::new(avail, 44.0), Sense::hover());
        ui.painter().rect_filled(hdr, 0.0, t(WINDOW_BG));
        ui.painter().line_segment(
            [hdr.left_bottom(), hdr.right_bottom()],
            egui::Stroke::new(1.0, Color32::from_white_alpha(18)),
        );
        let back_r = Rect::from_min_size(hdr.min + Vec2::new(8.0, 6.0), Vec2::splat(32.0));
        let back_resp = ui.interact(back_r, ui.make_persistent_id("disc-back"), Sense::click());
        if back_resp.hovered() {
            ui.painter().circle_filled(back_r.center(), 15.0, t(WINDOW_OVER));
        }
        icons::tg(ui.painter(), "title_back", back_r.center(), if back_resp.hovered() { WHITE } else { ORANGE });
        if back_resp.on_hover_text("Назад к каналу").on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
            || ui.input(|i| i.key_pressed(egui::Key::Escape))
        {
            go_back = true;
        }
        ui.painter().text(
            Pos2::new(back_r.right() + 10.0, hdr.center().y - 7.0),
            egui::Align2::LEFT_CENTER,
            format!("Комментарии · {}", disc.comments.len()),
            FontId::proportional(15.0),
            WHITE,
        );
        if !ch_name.is_empty() {
            ui.painter().text(
                Pos2::new(back_r.right() + 10.0, hdr.center().y + 9.0),
                egui::Align2::LEFT_CENTER,
                &ch_name,
                FontId::proportional(12.0),
                SUBTEXT,
            );
        }

        let has_reply_bar = self.disc_reply_to.is_some();
        let bottom_h = 54.0 + if has_reply_bar { 30.0 } else { 0.0 };
        let body_h = (ui.available_height() - bottom_h).max(80.0);
        let mut pick_reply: Option<(i32, String)> = None;
        ui.allocate_ui(Vec2::new(avail, body_h), |ui| {
            egui::ScrollArea::vertical()
                .id_salt(("disc-scroll", disc.post_id))
                .auto_shrink([false; 2])
                .stick_to_bottom(true)
                .show(ui, |ui| {
                    ui.add_space(8.0);
                    // Карточка исходного поста сверху ленты комментариев.
                    egui::Frame::new()
                        .fill(t2(WINDOW_BG))
                        .corner_radius(egui::CornerRadius::same(12))
                        .inner_margin(egui::Margin::symmetric(12, 10))
                        .show(ui, |ui| {
                            ui.set_max_width((avail - 28.0).min(640.0));
                            ui.horizontal(|ui| {
                                let (bar, _) = ui.allocate_exact_size(Vec2::new(3.0, 26.0), Sense::hover());
                                ui.painter().rect_filled(bar, 1.5, ORANGE);
                                ui.vertical(|ui| {
                                    ui.label(RichText::new(format!("Пост #{}", disc.post_id)).strong().size(12.5).color(ORANGE));
                                    let body = if post_txt.trim().is_empty() {
                                        "[Публикация с медиа]".to_owned()
                                    } else {
                                        short(&post_txt, 220)
                                    };
                                    ui.label(RichText::new(body).size(13.5).color(WHITE));
                                });
                            });
                        });
                    ui.add_space(6.0);
                    day_pill(ui, avail, plural_comments_ex(disc.comments.len() as i32, self.settings.extera.disable_number_rounding));
                    ui.add_space(4.0);
                    if disc.comments.is_empty() {
                        ui.add_space(28.0);
                        ui.vertical_centered(|ui| {
                            ui.label(RichText::new("Комментариев пока нет. Будьте первым!").size(14.0).color(SUBTEXT));
                        });
                    } else {
                        for c in &disc.comments {
                            let who = if c.outgoing && !self.me.is_empty() {
                                self.me.clone()
                            } else if c.sender.is_empty() {
                                "Участник".to_owned()
                            } else {
                                c.sender.clone()
                            };
                            let is_out = c.outgoing;
                            let layout = if is_out {
                                Layout::right_to_left(Align::TOP)
                            } else {
                                Layout::left_to_right(Align::TOP)
                            };
                            ui.with_layout(layout, |ui| {
                                ui.add_space(10.0);
                                if !is_out {
                                    avatar(ui, &who, c.sender_id, 30.0, self.avatars.get(&c.sender_id), self.settings.ayu.avatar_corners);
                                    ui.add_space(4.0);
                                }
                                let bg = if is_out { t(MSG_OUT_BG) } else { t(MSG_IN_BG) };
                                egui::Frame::new()
                                    .fill(bg)
                                    .corner_radius(egui::CornerRadius::same(13))
                                    .inner_margin(egui::Margin::symmetric(11, 7))
                                    .show(ui, |ui| {
                                        ui.set_max_width((avail * 0.72).clamp(180.0, 520.0));
                                        ui.vertical(|ui| {
                                            if !is_out {
                                                ui.label(RichText::new(&who).strong().size(12.5).color(ORANGE));
                                            }
                                            if let Some((_, rp_txt)) = &c.reply_to {
                                                ui.label(RichText::new(format!("Ответ: {}", short(rp_txt, 48))).size(11.5).color(ORANGE));
                                            }
                                            let body = if c.text.is_empty() { "[Вложение]" } else { c.text.as_str() };
                                            ui.label(RichText::new(body).size(14.5).color(if is_out { MSG_OUT_TEXT } else { WHITE }));
                                            ui.horizontal(|ui| {
                                                let meta_c = if is_out { MSG_OUT_DATE } else { MSG_IN_DATE };
                                                ui.label(RichText::new(hhmm(c.date)).size(11.5).color(meta_c));
                                                if is_out && c.id < 0 {
                                                    let (sr, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                                    icons::tg_sized(ui.painter(), "dialogs/dialogs_sending", sr.center(), Vec2::splat(11.0), meta_c);
                                                }
                                                if c.reactions > 0 {
                                                    let (rr, _) = ui.allocate_exact_size(Vec2::splat(12.0), Sense::hover());
                                                    icons::tg_sized(ui.painter(), "dialogs/dialogs_reaction", rr.center(), Vec2::splat(11.0), ORANGE);
                                                    ui.label(RichText::new(c.reactions.to_string()).size(11.5).color(ORANGE));
                                                }
                                                if c.id > 0
                                                    && ui.add(egui::Button::new(RichText::new("Ответить").size(11.5).color(ORANGE)).frame(false)).clicked()
                                                {
                                                    pick_reply = Some((c.id, who.clone()));
                                                }
                                            });
                                        });
                                    });
                            });
                            ui.add_space(5.0);
                        }
                    }
                    ui.add_space(8.0);
                });
        });

        if let Some(pr) = pick_reply {
            self.disc_reply_to = Some(pr);
        }
        // Нижняя строка ввода комментария на всю ширину панели.
        let mut send_txt: Option<String> = None;
        if let Some((_, ref rname)) = self.disc_reply_to.clone() {
            let mut clr = false;
            ui.horizontal(|ui| {
                ui.add_space(12.0);
                ui.label(RichText::new(format!("Ответ: {rname}")).size(12.0).color(ORANGE));
                if ui.small_button("✕").clicked() {
                    clr = true;
                }
            });
            if clr {
                self.disc_reply_to = None;
            }
        }
        ui.horizontal(|ui| {
            ui.add_space(10.0);
            let iw = (ui.available_width() - 56.0).max(120.0);
            let te = ui.add(
                egui::TextEdit::singleline(&mut self.comment_input)
                    .hint_text("Написать комментарий…")
                    .desired_width(iw)
                    .margin(Vec2::new(10.0, 8.0)),
            );
            self.hook_android_text(&te, "comment_input", "Комментарий", "Написать комментарий…", &self.comment_input.clone(), 0);
            let pressed_enter = (te.has_focus() || te.lost_focus()) && ui.input(|i| i.key_pressed(egui::Key::Enter));
            let (send_r, send_resp) = ui.allocate_exact_size(Vec2::new(36.0, 34.0), Sense::click());
            let shk = tween(ui.ctx(), "cmt-send-h", f32::from(u8::from(send_resp.hovered())), 0.12);
            ui.painter().rect_filled(
                send_r,
                egui::CornerRadius::same(17),
                lerp_color(t_btn(c_orange()), t_btn(ORANGE_OVER), shk),
            );
            icons::tg_sized(ui.painter(), "chat/input_send", send_r.center(), Vec2::splat(18.0), super::theme::on_accent());
            let clicked_send = send_resp
                .on_hover_text("Отправить комментарий")
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked();
            if (clicked_send || pressed_enter) && !self.comment_input.trim().is_empty() {
                send_txt = Some(std::mem::take(&mut self.comment_input).trim().to_owned());
            }
        });

        if let Some(txt) = send_txt {
            let reply_picked = self.disc_reply_to.take();
            let target_reply = reply_picked.as_ref().map_or(disc.root_msg_id, |(id, _)| *id);
            let tid = self.next_temp_id();
            let mut opt_row = MessageRow::plain(tid, txt.clone(), true, self.now.max(1));
            opt_row.sender = self.me.clone();
            if let Some((rid, rname)) = reply_picked {
                opt_row.reply_to = Some((rid, rname));
            }
            if let Some(cur_disc) = self.discussion.as_mut() {
                cur_disc.comments.push(opt_row);
            }
            self.send(Cmd::SendComment(disc.disc_peer, target_reply, disc.post_id, txt, tid));
        }
        if go_back {
            self.discussion = None;
            self.disc_reply_to = None;
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
        let linked_chat_id = info.and_then(|x| x.linked_chat_id);
        let has_linked = linked_chat_id.is_some();
        let chan_id = d.id;
        let muted = d.muted;
        let want_discuss = match btn_mode {
            CB::Hidden => return,
            CB::Mute => false,
            CB::Discuss => has_linked,
            CB::DiscussWithFallback => has_linked,
        };
        if btn_mode == CB::Discuss && !has_linked {
            return;
        }
        let w = ui.available_width();
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::hover());
        let mute_btn_w = if want_discuss { 44.0 } else { 0.0 };
        let primary_rect = Rect::from_min_max(rect.min, Pos2::new((rect.right() - mute_btn_w).max(rect.left() + 80.0), rect.bottom()));
        let resp = ui.interact(primary_rect, ui.make_persistent_id(("ch-bar-p", idx)), Sense::click());
        let hk = tween(ui.ctx(), ("ch-bar", idx), f32::from(u8::from(resp.hovered())), 0.12);
        let bg = lerp_color(t(WINDOW_BG), t(WINDOW_OVER), hk);
        ui.painter().rect_filled(rect, 0.0, t(WINDOW_BG));
        if hk > 0.01 {
            ui.painter().rect_filled(primary_rect, 0.0, bg);
        }
        ui.painter().line_segment(
            [rect.left_top(), rect.right_top()],
            egui::Stroke::new(1.0, Color32::from_white_alpha(20)),
        );
        let accent = c_orange();
        let (lbl, icon_name) = if want_discuss {
            ("Обсудить", "history_comments")
        } else if muted {
            ("Включить звук", "menu/unmute")
        } else {
            ("Выключить звук", "menu/mute")
        };
        let label_g = ui.painter().layout_no_wrap(lbl.to_owned(), FontId::proportional(14.0), accent);
        let total_w = 18.0 + 7.0 + label_g.size().x;
        let start_x = primary_rect.center().x - total_w * 0.5;
        icons::tg(ui.painter(), icon_name, Pos2::new(start_x + 9.0, primary_rect.center().y), accent);
        ui.painter().galley(
            Pos2::new(start_x + 25.0, primary_rect.center().y - label_g.size().y * 0.5),
            label_g,
            accent,
        );
        let mut toggle_mute = false;
        if want_discuss {
            let mr = Rect::from_center_size(Pos2::new(rect.right() - 22.0, rect.center().y), Vec2::splat(34.0));
            let mresp = ui.interact(mr, ui.make_persistent_id(("ch-bar-m", idx)), Sense::click());
            if mresp.hovered() {
                ui.painter().circle_filled(mr.center(), 15.0, t(WINDOW_OVER));
            }
            icons::tg_sized(
                ui.painter(),
                if muted { "menu/unmute" } else { "menu/mute" },
                mr.center(),
                Vec2::splat(16.0),
                if mresp.hovered() { WHITE } else { SUBTEXT },
            );
            if mresp
                .on_hover_text(if muted { "Включить звук" } else { "Выключить звук" })
                .on_hover_cursor(egui::CursorIcon::PointingHand)
                .clicked()
            {
                toggle_mute = true;
            }
        }
        if toggle_mute {
            self.toggle_mute_dialog(idx, !muted);
        } else if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            if want_discuss {
                if let Some(lcid) = linked_chat_id {
                    self.send(Cmd::OpenLinkedDiscussion(chan_id, lcid));
                }
            } else {
                self.toggle_mute_dialog(idx, !muted);
            }
        }
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

    pub(crate) fn apply(&mut self, ctx: &egui::Context, a: Act) {
        let cur_did = self.selected_dialog_id();
        match a {
            Act::Reply(id, text) => {
                self.editing = None;
                self.reply = Some((id, text));
            }
            Act::Copy(t) => {
                crate::copy_text_to_clipboard(ctx, &t);
                self.status = "Скопировано в буфер обмена".to_owned();
            }
            Act::Edit(id, text) => {
                self.reply = None;
                self.editing = Some(id);
                self.input = text;
            }
            Act::Forward(id) => self.forward = Some(id),
            Act::Pin(id, on) => {
                if let Some(did) = cur_did {
                    self.send(Cmd::Pin(did, id, on));
                }
            }
            Act::React(id, e) => {
                self.rx_pop.insert((id, e.clone()), ctx.input(|i| i.time));
                if let Some(m) = self.messages.iter_mut().find(|m| m.id == id) {
                    if let Some(p) = m.reaction_items.iter_mut().find(|p| !p.paid && p.emoji == e) {
                        if p.chosen {
                            p.chosen = false;
                            p.count = (p.count - 1).max(0);
                        } else {
                            p.chosen = true;
                            p.count += 1;
                        }
                        m.reaction_items.retain(|x| x.count > 0);
                    } else {
                        m.reaction_items.push(rg_core::engine::ReactionPill {
                            emoji: e.clone(),
                            custom_doc_id: None,
                            count: 1,
                            chosen: true,
                            paid: false,
                        });
                    }
                }
                if let Some(did) = cur_did {
                    self.send(Cmd::React(did, id, e));
                }
            }
            Act::ReactToggle(id, e, chosen) => {
                self.rx_pop.insert((id, e.clone()), ctx.input(|i| i.time));
                if let Some(m) = self.messages.iter_mut().find(|m| m.id == id)
                    && let Some(p) = m.reaction_items.iter_mut().find(|p| !p.paid && p.emoji == e)
                {
                    if chosen {
                        p.chosen = false;
                        p.count = (p.count - 1).max(0);
                    } else {
                        p.chosen = true;
                        p.count += 1;
                    }
                    m.reaction_items.retain(|x| x.count > 0);
                }
                if let Some(did) = cur_did {
                    self.send(Cmd::ReactToggle(did, id, e, chosen));
                }
            }
            Act::PaidReact(id) => self.paid_react_msg = Some(id),
            Act::Delete(id) => self.confirm_delete = Some(id),
            Act::Repeat(text) => {
                if let Some(did) = cur_did {
                    let tid = self.push_optimistic_message(text.clone(), Vec::new(), None, None);
                    self.send(Cmd::Send(did, text, None, tid));
                }
            }
            Act::OpenStickerPack(set_id, access_hash) => {
                self.send(Cmd::LoadStickerPack(set_id, access_hash));
            }
            Act::Details(id) => {
                self.details = Some(id);
                self.details_tab = 0;
                self.edit_history = None;
                self.msg_activity = None;
                if let Some(did) = cur_did {
                    self.send(Cmd::EditHistory(did, id));
                    self.send(Cmd::LoadMessageActivity(did, id));
                }
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
            Act::CopyId(id) => {
                let s = id.to_string();
                crate::copy_text_to_clipboard(ctx, &s);
                self.status = format!("ID {s} скопирован");
            }
            Act::Save(id) => {
                if let Some(did) = cur_did {
                    self.send(Cmd::Download(did, id));
                }
            }
            Act::SaveToSaved(id) => {
                if let (Some(from_did), Some(saved_did)) = (cur_did, self.dialogs.iter().find(|d| d.saved).map(|d| d.id)) {
                    self.send(Cmd::Forward(from_did, id, saved_did, false));
                    self.status = "Пересылка в «Избранное»…".to_owned();
                } else if let Some(did) = cur_did {
                    self.send(Cmd::Download(did, id));
                }
            }
            Act::CopyPhoto(id) => {
                if let Some(did) = cur_did {
                    self.send(Cmd::CopyPhoto(did, id));
                }
            }
            Act::Open(id) => {
                if let Some(did) = cur_did {
                    self.send(Cmd::OpenFile(did, id));
                }
            }
            Act::CancelFetch(id) => self.send(Cmd::CancelFetch(id)),
            Act::Viewer(id) => {
                self.viewer = Some(id);
                self.viewer_zoom = 1.0;
            }
            Act::History(id) => {
                self.details = Some(id);
                self.details_tab = 0;
                self.edit_history = None;
                self.msg_activity = None;
                if let Some(did) = cur_did {
                    self.send(Cmd::EditHistory(did, id));
                    self.send(Cmd::LoadMessageActivity(did, id));
                }
            }
            Act::Goto(id) => {
                if self.messages.iter().any(|m| m.id == id) {
                    self.scroll_to = Some(id);
                    self.highlight_msg = Some((id, ctx.input(|i| i.time)));
                } else if let Some(did) = cur_did {
                    self.send(Cmd::JumpToMessage(did, id));
                }
            }
            Act::PurgeLocal(id) => {
                self.messages.retain(|m| m.id != id);
                self.msg_heights.remove(&id);
                if let Some(did) = cur_did
                    && let Some(cached) = self.history_cache.get_mut(&did)
                {
                    cached.retain(|m| m.id != id);
                }
                if let Some(did) = cur_did {
                    self.send(Cmd::PurgeLocal(did, id));
                }
                self.status = "Удалённое сообщение скрыто у вас".to_owned();
            }
            Act::PurgeAllDeleted => {
                self.messages.retain(|m| !m.deleted);
                self.msg_heights.clear();
                if let Some(did) = cur_did
                    && let Some(cached) = self.history_cache.get_mut(&did)
                {
                    cached.retain(|m| !m.deleted);
                }
                if let Some(did) = cur_did {
                    self.send(Cmd::PurgeAllDeleted(did));
                }
                self.status = "Все удалённые сообщения этого чата убраны у вас".to_owned();
            }
            Act::CopyLink(msg_id) => {
                if let Some(d) = self.selected.and_then(|i| self.dialogs.get(i)) {
                    let uname = self
                        .peer_infos
                        .get(&d.id)
                        .map(|p| p.username.trim())
                        .unwrap_or("");
                    let link = if !uname.is_empty() {
                        Some(format!("https://t.me/{uname}/{msg_id}"))
                    } else if d.id < -1_000_000_000_000 {
                        let raw_ch = (-d.id) - 1_000_000_000_000;
                        Some(format!("https://t.me/c/{raw_ch}/{msg_id}"))
                    } else {
                        None
                    };
                    if let Some(url) = link {
                        crate::copy_text_to_clipboard(ctx, &url);
                        self.status = "Ссылка на сообщение скопирована".to_owned();
                    }
                }
            }
            Act::OpenUrl(url) => {
                // Внутренняя маршрутизация ссылок Telegram (`tg://`, `https://t.me/...`, `@username`) внутри клиента.
                if self.handle_tg_uri(&url) {
                    return;
                }
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
                if let Some(did) = cur_did {
                    self.send(Cmd::VotePoll(did, id, option));
                }
            }
            Act::ToggleSelect(id) => {
                if !self.selected_msgs.remove(&id) {
                    self.selected_msgs.insert(id);
                }
            }
            Act::Discuss(post_id) => {
                if let Some(did) = cur_did {
                    self.send(Cmd::Discussion(did, post_id));
                }
            }
            Act::FilterUser(uid, uname) => {
                self.user_filter = Some((
                    uid,
                    if uname.is_empty() { format!("ID {uid}") } else { uname },
                ));
            }
            Act::BotBtn(msg_id, label, kind) => {
                use rg_core::engine::BotBtnKind as K;
                match kind {
                    K::Text => {
                        if let Some(did) = cur_did {
                            let tid = self.push_optimistic_message(label.clone(), Vec::new(), None, None);
                            self.send(Cmd::Send(did, label, None, tid));
                        }
                    }
                    K::Callback(data) => {
                        if let Some(did) = cur_did {
                            self.bot_btn_pending = Some((msg_id, label, ctx.input(|i| i.time)));
                            self.send(Cmd::BotCallback(did, msg_id, data));
                        }
                    }
                    K::Url(u) => {
                        self.apply(ctx, Act::OpenUrl(u));
                    }
                    K::WebView { url, simple } => {
                        if let Some(did) = cur_did {
                            self.send(Cmd::OpenMiniApp {
                                dialog_id: did,
                                msg_id: (msg_id > 0).then_some(msg_id),
                                title: label,
                                url: Some(url),
                                start_param: None,
                                simple,
                            });
                        }
                    }
                    K::Copy(txt) => {
                        crate::copy_text_to_clipboard(ctx, &txt);
                        self.status = "Скопировано в буфер обмена".to_owned();
                    }
                    K::SwitchInline { query, .. } => {
                        let uname = self
                            .selected
                            .and_then(|i| self.dialogs.get(i))
                            .and_then(|d| self.peer_infos.get(&d.id))
                            .map(|p| p.username.trim().trim_start_matches('@'))
                            .filter(|s| !s.is_empty())
                            .map(str::to_owned);
                        self.input = match uname {
                            Some(bot) => format!("@{bot} {query}"),
                            None => query,
                        };
                    }
                    K::UserProfile(uid) => {
                        self.send(Cmd::OpenTgLink(rg_core::engine::TgLink::UserId(uid)));
                    }
                }
            }
            Act::Transcribe(msg_id) => {
                if let Some(did) = cur_did
                    && self.transcribing.insert(msg_id)
                {
                    self.send(Cmd::TranscribeVoice(did, msg_id));
                }
            }
            Act::ToggleTodo(msg_id, item_id, completed) => {
                // Оптимистично переключаем галочку локально + отправляем на сервер Telegram.
                if let Some(m) = self.messages.iter_mut().find(|m| m.id == msg_id)
                    && let Some(todo) = m.todo.as_mut()
                    && let Some(it) = todo.items.iter_mut().find(|x| x.id == item_id)
                {
                    it.done = completed;
                }
                if let Some(did) = cur_did {
                    self.send(Cmd::ToggleTodo(did, msg_id, item_id, completed));
                }
            }
            Act::SearchHashtag(tag) => {
                self.hashtag_modal = Some((tag.clone(), Vec::new()));
                self.send(Cmd::SearchHashtag(tag));
            }
            Act::ReplayEffect(msg_id) => {
                self.effect_bursts.insert(msg_id, ctx.input(|i| i.time));
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
                    let r_fwd = ui.add(
                        egui::TextEdit::singleline(&mut self.forward_filter)
                            .hint_text("Поиск чата")
                            .desired_width(300.0),
                    );
                    self.hook_android_text(&r_fwd, "forward_filter", "Поиск чата", "Имя чата или группы", &self.forward_filter.clone(), 0);
                    ui.checkbox(&mut self.forward_no_author, "Скрыть автора (без подписи)");
                    ui.checkbox(&mut self.forward_no_captions, "Скрыть подписи к медиа");
                    ui.checkbox(&mut self.forward_silent, "Отправить без звука");
                    let needle = self.forward_filter.to_lowercase();
                    let matched: Vec<usize> = self
                        .dialogs
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| needle.is_empty() || d.name.to_lowercase().contains(&needle))
                        .map(|(i, _)| i)
                        .collect();
                    egui::ScrollArea::vertical().max_height(320.0).show_rows(
                        ui,
                        22.0,
                        matched.len(),
                        |ui, range| {
                            for idx in range {
                                if let Some(&di) = matched.get(idx)
                                    && let Some(d) = self.dialogs.get(di)
                                    && ui.selectable_label(false, &d.name).clicked()
                                {
                                    chosen = Some(di);
                                }
                            }
                        },
                    );
                });
            if let Some(i) = chosen
                && let (Some(from_id), Some(to_id)) = (self.selected_dialog_id(), self.dialog_id_at(i))
            {
                self.send(Cmd::ForwardAdvanced {
                    from_id,
                    ids: vec![id],
                    to_id,
                    drop_author: self.forward_no_author,
                    drop_media_captions: self.forward_no_captions,
                    silent: self.forward_silent,
                });
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
                            crate::copy_text_to_clipboard(ctx, &url);
                            self.status = "Ссылка скопирована".to_owned();
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
                            let backup = self
                                .messages
                                .iter()
                                .chain(self.found.iter().flatten())
                                .find(|m| m.id == id)
                                .map(|m| {
                                    let txt = if !m.text.trim().is_empty() {
                                        m.text.clone()
                                    } else if let Some(med) = &m.media {
                                        format!("[{}]", med.name)
                                    } else {
                                        String::new()
                                    };
                                    (txt, m.date, m.outgoing, m.sender.clone())
                                });
                            if self.settings.ayu.save_deleted_messages {
                                for m in self.messages.iter_mut().filter(|m| m.id == id) {
                                    m.deleted = true;
                                }
                            }
                            if let Some(did) = self.selected_dialog_id() {
                                self.send(Cmd::Delete { dialog_id: did, id, backup });
                            }
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
            let mut tab = self.details_tab;
            let mut copied_lbl: Option<String> = None;
            let activity = self.msg_activity.as_ref().filter(|(mid, _, _)| *mid == id);
            egui::Window::new("Сведения о сообщении")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(360.0)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    let Some(m) = m else {
                        ui.label("Сообщение недоступно");
                        return;
                    };
                    ui.horizontal(|ui| {
                        ui.selectable_value(&mut tab, 0, "Метаданные");
                        ui.selectable_value(
                            &mut tab,
                            1,
                            match activity {
                                Some((_, readers, _)) => format!("Прочитали ({})", readers.len()),
                                None => "Прочитали".to_owned(),
                            },
                        );
                        ui.selectable_value(
                            &mut tab,
                            2,
                            match activity {
                                Some((_, _, reacts)) => format!("Реакции ({})", reacts.len()),
                                None => "Реакции".to_owned(),
                            },
                        );
                    });
                    ui.separator();
                    match tab {
                        1 => {
                            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                                match activity {
                                    None => {
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Spinner::new().size(14.0).color(c_orange()));
                                            ui.label(RichText::new("Загрузка списка…").color(SUBTEXT));
                                        });
                                    }
                                    Some((_, readers, _)) if readers.is_empty() => {
                                        ui.label(RichText::new("Нет сведений (доступно для своих сообщений в группах)").color(SUBTEXT));
                                    }
                                    Some((_, readers, _)) => {
                                        for r in readers {
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(&r.name).color(WHITE));
                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    ui.label(RichText::new(full_date(i64::from(r.date))).size(11.5).color(SUBTEXT));
                                                });
                                            });
                                        }
                                    }
                                }
                            });
                        }
                        2 => {
                            egui::ScrollArea::vertical().max_height(240.0).show(ui, |ui| {
                                match activity {
                                    None => {
                                        ui.horizontal(|ui| {
                                            ui.add(egui::Spinner::new().size(14.0).color(c_orange()));
                                            ui.label(RichText::new("Загрузка реакций…").color(SUBTEXT));
                                        });
                                    }
                                    Some((_, _, reacts)) if reacts.is_empty() => {
                                        ui.label(RichText::new("Реакций нет").color(SUBTEXT));
                                    }
                                    Some((_, _, reacts)) => {
                                        for r in reacts {
                                            ui.horizontal(|ui| {
                                                ui.label(RichText::new(&r.reaction).size(15.0));
                                                ui.label(RichText::new(&r.name).color(WHITE));
                                                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                                    ui.label(RichText::new(full_date(i64::from(r.date))).size(11.5).color(SUBTEXT));
                                                });
                                            });
                                        }
                                    }
                                }
                            });
                        }
                        _ => {
                            let mut copy_row = |ui: &mut egui::Ui, label: &str, val: &str| {
                                if ui
                                    .selectable_label(false, format!("{label}: {val}"))
                                    .on_hover_text("Нажмите, чтобы скопировать")
                                    .clicked()
                                {
                                    crate::copy_text_to_clipboard(ctx, val);
                                    copied_lbl = Some(label.to_owned());
                                }
                            };
                            copy_row(ui, "ID сообщения", &m.id.to_string());
                            copy_row(ui, "Отправлено", &full_date(m.date));
                            if let Some(e) = m.edit_date {
                                copy_row(ui, "Изменено", &full_date(e));
                            }
                            if !m.sender.is_empty() {
                                copy_row(ui, "Отправитель", &format!("{} (ID {})", m.sender, m.sender_id));
                            } else if m.sender_id != 0 {
                                copy_row(ui, "ID отправителя", &m.sender_id.to_string());
                            }
                            if let Some(f) = &m.forwarded_from {
                                copy_row(ui, "Переслано от", f);
                            }
                            if let Some(v) = m.views {
                                ui.label(format!("Просмотры: {v}"));
                            }
                            if m.reactions > 0 {
                                ui.label(format!("Реакции: {}", m.reactions));
                            }
                            if let Some(mi) = &m.media {
                                ui.separator();
                                ui.label(RichText::new("Медиа").strong());
                                if !mi.name.is_empty() {
                                    copy_row(ui, "Файл", &mi.name);
                                }
                                if let Some(dc) = mi.dc_id {
                                    copy_row(ui, "Дата-центр", &format!("DC{dc} ({})", rg_core::media::dc_location(dc)));
                                }
                                if !mi.mime.is_empty() {
                                    copy_row(ui, "MIME", &mi.mime);
                                }
                                if mi.size > 0 {
                                    ui.label(format!("Размер: {} байт", mi.size));
                                }
                                if mi.dims.0 > 0 && mi.dims.1 > 0 {
                                    ui.label(format!("Разрешение: {}×{}", mi.dims.0, mi.dims.1));
                                }
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
                        }
                    }
                });
            self.details_tab = tab;
            if let Some(lbl) = copied_lbl {
                self.status = format!("Скопировано: {lbl}");
            }
            if !open {
                self.details = None;
                self.details_tab = 0;
                self.edit_history = None;
                self.msg_activity = None;
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
                    let r_fwd = ui.add(
                        egui::TextEdit::singleline(&mut self.forward_filter)
                            .hint_text("Поиск чата")
                            .desired_width(300.0),
                    );
                    self.hook_android_text(&r_fwd, "forward_filter", "Поиск чата", "Имя чата или группы", &self.forward_filter.clone(), 0);
                    ui.checkbox(&mut self.forward_no_author, "Скрыть автора (без подписи)");
                    ui.checkbox(&mut self.forward_no_captions, "Скрыть подписи к медиа");
                    ui.checkbox(&mut self.forward_silent, "Отправить без звука");
                    let needle = self.forward_filter.to_lowercase();
                    let matched: Vec<usize> = self
                        .dialogs
                        .iter()
                        .enumerate()
                        .filter(|(_, d)| needle.is_empty() || d.name.to_lowercase().contains(&needle))
                        .map(|(i, _)| i)
                        .collect();
                    egui::ScrollArea::vertical().max_height(320.0).show_rows(
                        ui,
                        22.0,
                        matched.len(),
                        |ui, range| {
                            for idx in range {
                                if let Some(&di) = matched.get(idx)
                                    && let Some(d) = self.dialogs.get(di)
                                    && ui.selectable_label(false, &d.name).clicked()
                                {
                                    chosen = Some(di);
                                }
                            }
                        },
                    );
                });
            if let Some(i) = chosen
                && let (Some(from_id), Some(to_id)) = (self.selected_dialog_id(), self.dialog_id_at(i))
            {
                self.send(Cmd::ForwardAdvanced {
                    from_id,
                    ids,
                    to_id,
                    drop_author: self.forward_no_author,
                    drop_media_captions: self.forward_no_captions,
                    silent: self.forward_silent,
                });
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
                let save_target = rfd::FileDialog::new()
                    .set_file_name("rustgramm_shot.png")
                    .add_filter("PNG", &["png"])
                    .save_file();
                #[cfg(target_os = "android")]
                let save_target = Some(
                    super::worker::home()
                        .join("cache")
                        .join(format!("rustgramm_shot_{}.png", self.now)),
                );
                if let Some(path) = save_target
                    && let Some(img) = image::RgbaImage::from_raw(modal.dims.0, modal.dims.1, modal.rgba.clone())
                {
                    if let Some(p) = path.parent() {
                        let _ = std::fs::create_dir_all(p);
                    }
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
            let mut export_fmt: Option<bool> = None;
            let mut open_global = false;
            let chat_title = self
                .selected
                .and_then(|i| self.dialogs.get(i))
                .map_or_else(|| "Чат".to_owned(), |d| d.name.clone());
            egui::Window::new("Удалённые и история правок")
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([440.0, 480.0])
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if ui.small_button("Экспорт TXT").clicked() {
                            export_fmt = Some(false);
                        }
                        if ui.small_button("Экспорт JSON").clicked() {
                            export_fmt = Some(true);
                        }
                        if ui.small_button("Глобальный журнал").clicked() {
                            open_global = true;
                        }
                    });
                    ui.separator();
                    if entries.is_empty() {
                        ui.label(RichText::new("В этом чате пока нет сохранённых удалённых сообщений или правок.").color(SUBTEXT));
                        return;
                    }
                    egui::ScrollArea::vertical().max_height(380.0).show(ui, |ui| {
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
                                        crate::copy_text_to_clipboard(ctx, &e.text);
                                        self.status = "Текст из истории скопирован".to_owned();
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
            if let Some(as_json) = export_fmt
                && let Some(did) = self.selected_dialog_id()
            {
                self.send(Cmd::ExportChatAudit { dialog_id: did, chat_title, as_json });
            }
            if open_global {
                self.audit_history = None;
                self.global_deleted_open = true;
                self.send(Cmd::GlobalDeletedLog(self.global_deleted_query.clone()));
            } else if !open {
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
                    if let Some(did) = self.selected_dialog_id() {
                        self.send(Cmd::SendFile(did, p, c, if k == 0 { reply } else { None }));
                    }
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
        // Просмотрщик активных историй 1-в-1 в стиле Telegram (вертикальная 9:16 карточка,
        // верхние полоски прогресса, аватар/время, пауза/звук, полноразмерное фото, просмотры + лайк внизу).
        if let Some(idx) = self.story_open {
            let mut next_idx = Some(idx);
            let mut close_viewer = false;
            let mut react_emoji: Option<&'static str> = None;
            let mut reply_send: Option<String> = None;
            let mut retry_load = false;
            if let Some(st) = self.stories.get(idx).cloned() {
                if !self.story_textures.contains_key(&st.story_id)
                    && !self.story_errors.contains_key(&st.story_id)
                    && self.story_media_req.insert(st.story_id)
                {
                    self.send(Cmd::LoadStoryMedia(st.peer, st.story_id));
                }
                let elapsed = if self.story_paused {
                    3.0
                } else {
                    self.story_opened_at
                        .get_or_insert_with(std::time::Instant::now)
                        .elapsed()
                        .as_secs_f32()
                };
                let progress = (elapsed / 8.0).clamp(0.0, 1.0);
                if !self.story_paused && progress < 1.0 {
                    ctx.request_repaint_after(std::time::Duration::from_millis(40));
                } else if !self.story_paused && progress >= 1.0 {
                    if idx + 1 < self.stories.len() {
                        next_idx = Some(idx + 1);
                    } else {
                        close_viewer = true;
                    }
                }
                let screen = ctx.content_rect();
                #[allow(deprecated)]
                egui::Area::new(egui::Id::new("story-scrim"))
                    .order(egui::Order::Foreground)
                    .fixed_pos(Pos2::ZERO)
                    .show(ctx, |ui| {
                        let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                        ui.painter().rect_filled(r, 0.0, Color32::from_black_alpha(205));
                        if resp.clicked() {
                            close_viewer = true;
                        }
                    });
                let card_h = (screen.height() - 48.0).clamp(440.0, 740.0);
                let card_w = (card_h * 0.5625).clamp(320.0, (screen.width() - 32.0).min(430.0));
                #[allow(deprecated)]
                egui::Area::new(egui::Id::new("story-card-9x16"))
                    .order(egui::Order::Tooltip)
                    .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                    .show(ctx, |ui| {
                        let (card_rect, _) = ui.allocate_exact_size(Vec2::new(card_w, card_h), Sense::hover());
                        let p = ui.painter().clone();
                        let cr = egui::CornerRadius::same(18);
                        p.rect_filled(card_rect, cr, Color32::from_rgb(16, 17, 22));

                        // Область контента 9:16 между верхней шапкой и нижней полосой.
                        let head_h = 62.0;
                        let foot_h = 54.0;
                        let media_rect = Rect::from_min_max(
                            Pos2::new(card_rect.left(), card_rect.top() + head_h),
                            Pos2::new(card_rect.right(), card_rect.bottom() - foot_h),
                        );

                        // Полноразмерное медиа истории (0 мс при предзагрузке!).
                        if let Some(tex) = self.story_textures.get(&st.story_id) {
                            let sz = tex.size_vec2();
                            let scale = (media_rect.width() / sz.x.max(1.0))
                                .min(media_rect.height() / sz.y.max(1.0));
                            let draw_sz = sz * scale;
                            let img_r = Rect::from_center_size(media_rect.center(), draw_sz);
                            egui::Image::new(tex)
                                .fit_to_exact_size(draw_sz)
                                .corner_radius(10.0)
                                .paint_at(ui, img_r);
                        } else if let Some(err) = self.story_errors.get(&st.story_id) {
                            let err_r = Rect::from_center_size(media_rect.center(), Vec2::new(card_w - 40.0, 96.0));
                            ui.scope_builder(egui::UiBuilder::new().max_rect(err_r), |ui| {
                                ui.vertical_centered(|ui| {
                                    ui.label(RichText::new(err).size(13.0).color(SUBTEXT));
                                    ui.add_space(8.0);
                                    if orange_button(ui, "Повторить", Vec2::new(118.0, 30.0)).clicked() {
                                        retry_load = true;
                                    }
                                });
                            });
                        } else {
                            let sp_r = Rect::from_center_size(media_rect.center(), Vec2::splat(36.0));
                            ui.put(sp_r, egui::Spinner::new().size(28.0).color(c_orange()));
                        }

                        // Невидимые зоны клика слева/справа для мгновенного листания историй.
                        let left_tap = Rect::from_min_max(
                            media_rect.left_top(),
                            Pos2::new(media_rect.left() + media_rect.width() * 0.28, media_rect.bottom()),
                        );
                        let right_tap = Rect::from_min_max(
                            Pos2::new(media_rect.right() - media_rect.width() * 0.28, media_rect.top()),
                            media_rect.right_bottom(),
                        );
                        if idx > 0
                            && ui.interact(left_tap, ui.make_persistent_id("st-prev-tap"), Sense::click()).clicked()
                        {
                            next_idx = Some(idx - 1);
                        }
                        if ui.interact(right_tap, ui.make_persistent_id("st-next-tap"), Sense::click()).clicked() {
                            if idx + 1 < self.stories.len() {
                                next_idx = Some(idx + 1);
                            } else {
                                close_viewer = true;
                            }
                        }

                        // Подпись истории поверх нижней кромки фото (если есть).
                        if !st.caption.is_empty() {
                            let cap_r = Rect::from_min_max(
                                Pos2::new(media_rect.left() + 10.0, media_rect.bottom() - 56.0),
                                Pos2::new(media_rect.right() - 10.0, media_rect.bottom() - 6.0),
                            );
                            p.rect_filled(cap_r, egui::CornerRadius::same(10), Color32::from_black_alpha(150));
                            p.with_clip_rect(cap_r.shrink(8.0)).text(
                                Pos2::new(cap_r.left() + 10.0, cap_r.center().y),
                                egui::Align2::LEFT_CENTER,
                                short(&st.caption, 85),
                                FontId::proportional(13.0),
                                Color32::WHITE,
                            );
                        }

                        // Верхняя шапка (как на скрине 8): аватар + имя + время + пауза/звук/закрыть + прогресс-бар.
                        let top_r = Rect::from_min_size(card_rect.min, Vec2::new(card_w, head_h));
                        p.rect_filled(
                            top_r,
                            egui::CornerRadius { nw: 18, ne: 18, sw: 0, se: 0 },
                            Color32::from_rgb(20, 22, 28),
                        );
                        let av_c = Pos2::new(top_r.left() + 26.0, top_r.top() + 26.0);
                        p.circle_stroke(av_c, 16.5, egui::Stroke::new(1.8, Color32::from_rgb(138, 232, 92)));
                        draw_avatar(
                            ui,
                            av_c,
                            &st.author,
                            st.peer_id,
                            30.0,
                            self.avatars.get(&st.peer_id),
                            23,
                        );
                        p.text(
                            Pos2::new(top_r.left() + 50.0, top_r.top() + 18.0),
                            egui::Align2::LEFT_CENTER,
                            short(&st.author, 24),
                            FontId::proportional(13.5),
                            Color32::WHITE,
                        );
                        let ago_s = (self.now - i64::from(st.date)).max(0);
                        let ago = if ago_s < 60 {
                            "только что".to_owned()
                        } else if ago_s < 3600 {
                            format!("{} мин. назад", ago_s / 60)
                        } else {
                            format!("{} ч. назад", ago_s / 3600)
                        };
                        p.text(
                            Pos2::new(top_r.left() + 50.0, top_r.top() + 34.0),
                            egui::Align2::LEFT_CENTER,
                            ago,
                            FontId::proportional(11.5),
                            SUBTEXT,
                        );
                        // Кнопки справа в шапке: Пауза, Звук, Закрыть.
                        let close_r = Rect::from_center_size(Pos2::new(top_r.right() - 22.0, top_r.top() + 25.0), Vec2::splat(28.0));
                        let mute_r = Rect::from_center_size(Pos2::new(top_r.right() - 54.0, top_r.top() + 25.0), Vec2::splat(28.0));
                        let pause_r = Rect::from_center_size(Pos2::new(top_r.right() - 86.0, top_r.top() + 25.0), Vec2::splat(28.0));
                        let c_resp = ui.interact(close_r, ui.make_persistent_id("st-close"), Sense::click());
                        let m_resp = ui.interact(mute_r, ui.make_persistent_id("st-mute"), Sense::click());
                        let p_resp = ui.interact(pause_r, ui.make_persistent_id("st-pause"), Sense::click());
                        p.text(pause_r.center(), egui::Align2::CENTER_CENTER, if self.story_paused { "▶" } else { "⏸" }, FontId::proportional(14.0), if p_resp.hovered() { WHITE } else { SUBTEXT });
                        p.text(mute_r.center(), egui::Align2::CENTER_CENTER, if self.story_muted { "🔇" } else { "🔊" }, FontId::proportional(14.0), if m_resp.hovered() { WHITE } else { SUBTEXT });
                        p.text(close_r.center(), egui::Align2::CENTER_CENTER, "✕", FontId::proportional(14.0), if c_resp.hovered() { WHITE } else { SUBTEXT });
                        if p_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.story_paused = !self.story_paused;
                        }
                        if m_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            self.story_muted = !self.story_muted;
                        }
                        if c_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            close_viewer = true;
                        }
                        // Тонкая сегментированная полоска прогресса внизу шапки (как в Telegram).
                        let total_s = self.stories.len().max(1);
                        let pr_rect = Rect::from_min_size(
                            Pos2::new(top_r.left() + 12.0, top_r.bottom() - 8.0),
                            Vec2::new(card_w - 24.0, 2.5),
                        );
                        let gap = 3.0;
                        let seg_w = ((pr_rect.width() - gap * (total_s as f32 - 1.0)) / total_s as f32).max(6.0);
                        for s_i in 0..total_s {
                            let seg = Rect::from_min_size(
                                Pos2::new(pr_rect.left() + s_i as f32 * (seg_w + gap), pr_rect.top()),
                                Vec2::new(seg_w, 2.5),
                            );
                            p.rect_filled(seg, egui::CornerRadius::same(2), Color32::from_white_alpha(48));
                            let fill_f = if s_i < idx { 1.0 } else if s_i == idx { progress } else { 0.0 };
                            if fill_f > 0.0 {
                                p.rect_filled(
                                    Rect::from_min_size(seg.min, Vec2::new(seg_w * fill_f, 2.5)),
                                    egui::CornerRadius::same(2),
                                    Color32::WHITE,
                                );
                            }
                        }

                        // Нижняя полоса (как на скрине 8): слева просмотры 👁, в центре быстрый ответ, справа сердце ♥ + счётчик!
                        let bot_r = Rect::from_min_size(
                            Pos2::new(card_rect.left(), card_rect.bottom() - foot_h),
                            Vec2::new(card_w, foot_h),
                        );
                        p.rect_filled(
                            bot_r,
                            egui::CornerRadius { nw: 0, ne: 0, sw: 18, se: 18 },
                            Color32::from_rgb(18, 20, 26),
                        );
                        let views_txt = if st.views > 0 {
                            format!("👁 {}", st.views)
                        } else {
                            "👁 1".to_owned()
                        };
                        p.text(
                            Pos2::new(bot_r.left() + 16.0, bot_r.center().y),
                            egui::Align2::LEFT_CENTER,
                            views_txt,
                            FontId::proportional(13.0),
                            SUBTEXT,
                        );
                        // Компактное поле ответа по центру.
                        let reply_r = Rect::from_center_size(
                            Pos2::new(bot_r.center().x + 4.0, bot_r.center().y),
                            Vec2::new((card_w - 178.0).max(120.0), 30.0),
                        );
                        p.rect_filled(reply_r, egui::CornerRadius::same(15), Color32::from_white_alpha(14));
                        ui.scope_builder(egui::UiBuilder::new().max_rect(reply_r.shrink2(Vec2::new(10.0, 4.0))), |ui| {
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.story_reply_text)
                                    .hint_text("Ответить…")
                                    .frame(egui::Frame::NONE)
                                    .desired_width(reply_r.width() - 20.0),
                            );
                            if cfg!(target_os = "android")
                                && !self.android_input_pending
                                && (r.clicked() || r.gained_focus())
                            {
                                self.android_input_pending = true;
                                super::android_jni::prompt_text_input(
                                    "story_reply",
                                    "Ответ на историю",
                                    "Ответить…",
                                    &self.story_reply_text,
                                    0,
                                );
                            }
                            if r.lost_focus()
                                && ui.input(|i| i.key_pressed(egui::Key::Enter))
                                && !self.story_reply_text.trim().is_empty()
                            {
                                reply_send = Some(std::mem::take(&mut self.story_reply_text));
                            }
                        });
                        // Справа сердце ♥ + счётчик реакций с пружинным откликом!
                        let liked = self.story_liked.contains(&st.story_id);
                        let heart_r = Rect::from_center_size(
                            Pos2::new(bot_r.right() - 36.0, bot_r.center().y),
                            Vec2::new(56.0, 32.0),
                        );
                        let h_resp = ui.interact(heart_r, ui.make_persistent_id(("st-heart", st.story_id)), Sense::click());
                        let hk = tween(ui.ctx(), ("st-hk", st.story_id), f32::from(u8::from(liked || h_resp.hovered())), 0.16);
                        let heart_cnt = st.reactions + i32::from(liked);
                        let hcol = if liked { Color32::from_rgb(245, 86, 106) } else { lerp_color(SUBTEXT, WHITE, hk) };
                        let hc = if heart_cnt > 0 {
                            Pos2::new(heart_r.left() + 16.0, heart_r.center().y)
                        } else {
                            heart_r.center()
                        };
                        icons::tg_sized(
                            ui.painter(),
                            "dialogs/dialogs_reaction",
                            hc,
                            Vec2::splat(16.0 + 2.0 * ios_spring_bounce(hk)),
                            hcol,
                        );
                        if heart_cnt > 0 {
                            p.text(
                                Pos2::new(heart_r.left() + 28.0, heart_r.center().y),
                                egui::Align2::LEFT_CENTER,
                                heart_cnt.to_string(),
                                FontId::proportional(13.5),
                                hcol,
                            );
                        }
                        if h_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            if self.story_liked.insert(st.story_id) {
                                react_emoji = Some("❤");
                            } else {
                                self.story_liked.remove(&st.story_id);
                            }
                        }

                        // Стеклянная рамка карточки.
                        p.rect_stroke(card_rect, cr, egui::Stroke::new(1.0, Color32::from_white_alpha(32)), egui::StrokeKind::Inside);
                    });

                if ctx.input(|i| i.key_pressed(egui::Key::ArrowLeft)) && idx > 0 {
                    next_idx = Some(idx - 1);
                }
                if ctx.input(|i| i.key_pressed(egui::Key::ArrowRight)) {
                    if idx + 1 < self.stories.len() {
                        next_idx = Some(idx + 1);
                    } else {
                        close_viewer = true;
                    }
                }
                if ctx.input(|i| i.key_pressed(egui::Key::Space)) {
                    self.story_paused = !self.story_paused;
                }
                if retry_load {
                    self.story_errors.remove(&st.story_id);
                    self.send(Cmd::LoadStoryMedia(st.peer, st.story_id));
                }
                if let Some(em) = react_emoji {
                    self.send(Cmd::ReactStory(st.peer, st.story_id, em.to_owned()));
                }
                if let Some(reply_txt) = reply_send
                    && let Some(pos) = self.dialogs.iter().position(|d| d.id == st.peer_id)
                {
                    self.open_dialog(pos);
                    let msg = format!("Ответ на историю: {reply_txt}");
                    let tid = self.push_optimistic_message(msg.clone(), Vec::new(), None, None);
                    self.send(Cmd::Send(st.peer_id, msg, None, tid));
                    self.status = "Отправка ответа на историю…".to_owned();
                }
                if close_viewer || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
                    self.story_open = None;
                    self.story_opened_at = None;
                    self.story_paused = false;
                } else {
                    if next_idx != Some(idx) {
                        self.story_opened_at = Some(std::time::Instant::now());
                        self.story_paused = false;
                        if let Some(ni) = next_idx {
                            let info = self.stories.get_mut(ni).map(|ns| {
                                let was_unread = ns.unread;
                                ns.unread = false;
                                (ns.peer, ns.story_id, was_unread)
                            });
                            if let Some((peer, sid, was_unread)) = info {
                                if was_unread && self.settings.ghost.send_read_stories {
                                    self.send(Cmd::ReadStory(peer, sid));
                                }
                                if !self.story_textures.contains_key(&sid) && self.story_media_req.insert(sid) {
                                    self.send(Cmd::LoadStoryMedia(peer, sid));
                                }
                            }
                        }
                    }
                    self.story_open = next_idx;
                }
            } else {
                self.story_open = None;
                self.story_opened_at = None;
            }
        }
        // Модальное окно просмотра и установки набора стикеров.
        if let Some(pack) = self.sticker_pack_modal.clone() {
            let mut open = true;
            let mut send_doc: Option<i64> = None;
            let mut toggle_install: Option<bool> = None;
            egui::Window::new(&pack.title)
                .open(&mut open)
                .collapsible(false)
                .resizable(true)
                .default_size([380.0, 440.0])
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.horizontal(|ui| {
                        if !pack.short_name.is_empty() {
                            ui.label(RichText::new(format!("t.me/addstickers/{}", pack.short_name)).size(12.0).color(SUBTEXT));
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            ui.label(RichText::new(format!("{} шт.", pack.stickers.len())).size(12.0).color(ORANGE));
                        });
                    });
                    ui.add_space(6.0);
                    egui::ScrollArea::vertical()
                        .id_salt(("stk-pack-grid", pack.id))
                        .max_height(330.0)
                        .show(ui, |ui| {
                            let cols = 4usize;
                            let cell = ((ui.available_width() - 8.0) / cols as f32).clamp(64.0, 92.0);
                            for chunk in pack.stickers.chunks(cols) {
                                ui.horizontal(|ui| {
                                    for st in chunk {
                                        let (r, resp) = ui.allocate_exact_size(Vec2::splat(cell), Sense::click());
                                        if resp.hovered() {
                                            ui.painter().rect_filled(r.shrink(2.0), egui::CornerRadius::same(10), Color32::from_white_alpha(20));
                                        }
                                        let area = Rect::from_center_size(r.center(), Vec2::splat(cell - 12.0));
                                        if let Some(tex) = self.sticker_tex.get(&st.id) {
                                            egui::Image::new(tex).fit_to_exact_size(area.size()).paint_at(ui, area);
                                        } else {
                                            ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, if st.emoji.is_empty() { "🖼" } else { &st.emoji }, FontId::proportional(26.0), WHITE);
                                        }
                                        if resp.on_hover_text(format!("Отправить {}", st.emoji)).on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                            send_doc = Some(st.id);
                                        }
                                    }
                                });
                            }
                        });
                    ui.add_space(8.0);
                    ui.horizontal(|ui| {
                        let btn_lbl = if pack.installed { "Удалить набор" } else { "Добавить стикеры" };
                        if pack.installed {
                            if ui.add_sized([ui.available_width(), 36.0], egui::Button::new(RichText::new(btn_lbl).color(Color32::from_rgb(235, 92, 86)))).clicked() {
                                toggle_install = Some(false);
                            }
                        } else if orange_button(ui, btn_lbl, Vec2::new(ui.available_width(), 36.0)).clicked() {
                            toggle_install = Some(true);
                        }
                    });
                });
            if let Some(doc_id) = send_doc {
                let reply = self.reply.take().map(|(r, _)| r);
                self.sticker_pack_modal = None;
                if let Some(did) = self.selected_dialog_id() {
                    self.send(Cmd::SendSticker(did, doc_id, reply));
                }
            } else if let Some(install) = toggle_install {
                self.send(Cmd::InstallStickerPack(pack.id, pack.access_hash, install));
            } else if !open {
                self.sticker_pack_modal = None;
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
                            if let Some(did) = self.selected_dialog_id() {
                                self.send(Cmd::SendSticker(did, doc_id, reply));
                            }
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
        // Всплывающее уведомление (`alert = true`) от инлайн-кнопки бота.
        if let Some(msg) = self.bot_alert.clone() {
            let mut open = true;
            egui::Window::new("Уведомление бота")
                .open(&mut open)
                .collapsible(false)
                .resizable(false)
                .default_width(340.0)
                .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
                .show(ctx, |ui| {
                    ui.label(RichText::new(&msg).size(14.0).color(WHITE));
                    ui.add_space(10.0);
                    if orange_button(ui, "ОК", Vec2::new(100.0, 32.0)).clicked() {
                        self.bot_alert = None;
                    }
                });
            if !open {
                self.bot_alert = None;
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
                    Vec2::new(470.0_f32.min(rect.width() - 24.0), 38.0),
                );
                ui.scope_builder(egui::UiBuilder::new().max_rect(bar), |ui| {
                    ui.horizontal(|ui| {
                        if orange_button(ui, "Сохранить", Vec2::new(105.0, 32.0)).clicked() {
                            acts.push(Act::Save(id));
                        }
                        if ui.button("Копировать фото").clicked() {
                            acts.push(Act::CopyPhoto(id));
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
                && let Some(did) = self.selected_dialog_id()
            {
                self.send(Cmd::Preview(did, new_id));
            }
        } else if close {
            self.viewer = None;
        }
    }
}

fn day_pill(ui: &mut egui::Ui, avail: f32, label: String) -> bool {
    let galley = ui
        .painter()
        .layout_no_wrap(label, FontId::proportional(14.0), Color32::WHITE);
    let size = galley.size() + Vec2::new(24.0, 8.0);
    let (rect, _) = ui.allocate_exact_size(Vec2::new(avail, size.y + 8.0), Sense::hover());
    let pill = egui::Rect::from_center_size(rect.center(), size);
    let resp = ui.interact(pill, ui.make_persistent_id(("day-pill", rect.min.y as i32)), Sense::click());
    let bg = if resp.hovered() {
        t_btn(ORANGE)
    } else {
        Color32::from_rgba_unmultiplied(20, 22, 28, (200.0 * opacity_f32()) as u8)
    };
    ui.painter().rect_filled(pill, egui::CornerRadius::same(12), bg);
    ui.painter().rect_stroke(
        pill,
        egui::CornerRadius::same(12),
        egui::Stroke::new(1.0, Color32::from_white_alpha(if resp.hovered() { 44 } else { 20 })),
        egui::StrokeKind::Inside,
    );
    ui.painter()
        .galley(pill.center() - galley.size() / 2.0, galley, Color32::WHITE);
    resp.on_hover_text("Перейти к дате")
        .on_hover_cursor(egui::CursorIcon::PointingHand)
        .clicked()
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
            k_in = (dt / 0.18).clamp(0.0, 1.0) as f32;
            if k_in < 1.0 {
                ui.ctx().request_repaint();
            }
        }
        // НЕ добавляем динамический `ui.add_space(...)` — это полностью убирает дёргание сообщений при входе в чат!
    }
    let ease_in = ios_spring(k_in);
    let mut bubble_w = 210.0_f32;
    // Стикер без текста, ответа и пересылки рисуется НА ПОЛНОСТЬЮ ПРОЗРАЧНОМ ФОНЕ (как в Telegram!).
    let is_sticker_only = album.len() <= 1
        && m.text.is_empty()
        && m.rich_blocks.is_empty()
        && m.todo.is_none()
        && m.reply_to.is_none()
        && m.forwarded_from.is_none()
        && m.media.as_ref().is_some_and(|i| {
            matches!(
                i.kind,
                rg_core::media::MediaKind::Sticker | rg_core::media::MediaKind::AnimatedSticker
            )
        });
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
        if is_sticker_only {
            fill = Color32::TRANSPARENT;
        }
        let max_inner = (MSG_MAX_WIDTH * s.wide_multiplier()).min(env.avail * 0.8) - 2.0 * MSG_PAD_X;
        let galley = ui
            .painter()
            .layout(m.text.clone(), FontId::proportional(15.0), text_c, max_inner);
        let edited = m.edit_date.is_some() || m.edits > 0;
        let mut meta = time_label(m.date, env.format_secs);
        if edited {
            let mark = if s.replace_bottom_info_with_icons {
                "ред."
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
            w = if is_sticker_only {
                182.0_f32.min(max_inner)
            } else {
                w.max(if env.tex.contains_key(&m.id) { 240.0 } else { 190.0 })
            };
        }
        if !m.rich_blocks.is_empty() {
            w = w.max(max_inner.min(370.0));
        }
        if m.todo.is_some() {
            w = w.max(max_inner.min(290.0));
        }
        if album.len() > 1 {
            w = max_inner.min(310.0);
        }
        let w = w.min(max_inner);
        bubble_w = w;
        let radius = bubble_corner_radius(s.bubble_radius(), s.single_corner_radius, m.outgoing);
        let bid = ui.make_persistent_id(("m", m.id));
        let mut cards: Vec<(Rect, Act)> = Vec::new();
        let mut hot: Vec<(Rect, super::rich::Hit)> = Vec::new();
        let selected = env.selected_msgs.contains(&m.id);
        let highlighted = env.highlight_msg == Some(m.id);
        let mut frame = egui::Frame::new()
            .fill(fill)
            .corner_radius(radius)
            .inner_margin(if is_sticker_only {
                egui::Margin::same(2)
            } else {
                egui::Margin::symmetric(MSG_PAD_X as i8, MSG_PAD_Y as i8)
            });
        if selected {
            frame = frame.stroke(egui::Stroke::new(2.0, ORANGE));
        } else if highlighted {
            frame = frame.stroke(egui::Stroke::new(2.0, ORANGE.gamma_multiply(0.85)));
        } else if !is_sticker_only {
            let stroke_col = if m.outgoing {
                Color32::from_rgba_unmultiplied(c_orange().r(), c_orange().g(), c_orange().b(), 52)
            } else {
                Color32::from_white_alpha(18)
            };
            frame = frame.stroke(egui::Stroke::new(1.0, stroke_col));
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
                        let bar_c = if s.show_colorful_replies { c_orange() } else { date_c };
                        let snippet_t = if snippet.is_empty() { "сообщение" } else { snippet.as_str() };
                        let resp = if s.simple_quotes_and_replies {
                            ui.label(RichText::new(format!("Ответ: {}", short(snippet_t, 45))).size(12.5).color(bar_c))
                        } else {
                            let tint = Color32::from_rgba_unmultiplied(bar_c.r(), bar_c.g(), bar_c.b(), 22);
                            egui::Frame::new()
                                .fill(tint)
                                .corner_radius(egui::CornerRadius::same(6))
                                .inner_margin(egui::Margin::symmetric(7, 4))
                                .show(ui, |ui| {
                                    ui.set_width((w - 14.0).max(80.0));
                                    ui.horizontal(|ui| {
                                        let (bar, _) = ui.allocate_exact_size(Vec2::new(2.5, 28.0), Sense::hover());
                                        ui.painter().rect_filled(bar, 1.2, bar_c);
                                        ui.vertical(|ui| {
                                            ui.spacing_mut().item_spacing.y = 0.0;
                                            ui.label(RichText::new("Ответ").size(11.5).strong().color(bar_c));
                                            ui.label(RichText::new(short(snippet_t, 54)).size(12.5).color(date_c));
                                        });
                                    });
                                })
                                .response
                        };
                        if ui
                            .interact(resp.rect, ui.make_persistent_id(("q", m.id)), Sense::click())
                            .on_hover_cursor(egui::CursorIcon::PointingHand)
                            .clicked()
                        {
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
                            // Кэшированный GIF или WebM видео-стикер, целиком видимый в окне ленты — автозапуск на прозрачном фоне.
                            if (info.kind == rg_core::media::MediaKind::Gif
                                || (info.kind == rg_core::media::MediaKind::AnimatedSticker
                                    && info.mime == "video/webm"))
                                && env.gif_paths.contains_key(&m.id)
                                && env.clip_rect.contains_rect(*card_rect)
                            {
                                *gif_candidate = Some((m.id, *card_rect));
                            }
                        }
                    }
                    // Кнопка расшифровки голосового/кружка («Aa», без лимитов: Telegram + оффлайн) и карточка текста.
                    if let Some(info) = &m.media
                        && matches!(info.kind, rg_core::media::MediaKind::Voice | rg_core::media::MediaKind::Round)
                        && !m.deleted
                    {
                        ui.add_space(3.0);
                        if let Some(tr_txt) = env.transcriptions.get(&m.id) {
                            egui::Frame::new()
                                .fill(Color32::from_white_alpha(12))
                                .stroke(egui::Stroke::new(1.0, c_orange().gamma_multiply(0.35)))
                                .corner_radius(egui::CornerRadius::same(8))
                                .inner_margin(egui::Margin::symmetric(8, 5))
                                .show(ui, |ui| {
                                    ui.label(RichText::new("Aa Расшифровка").size(11.0).color(c_orange()));
                                    ui.label(RichText::new(tr_txt).size(13.5).color(text_c));
                                });
                        } else if env.transcribing.contains(&m.id) {
                            ui.horizontal(|ui| {
                                ui.add(egui::Spinner::new().size(13.0).color(c_orange()));
                                ui.label(RichText::new("Расшифровка…").size(12.0).color(date_c));
                            });
                        } else {
                            let tr_btn = egui::Button::new(RichText::new("→A  Расшифровать").size(11.5).color(c_orange()))
                                .fill(Color32::from_rgba_unmultiplied(c_orange().r(), c_orange().g(), c_orange().b(), 24))
                                .corner_radius(egui::CornerRadius::same(8));
                            let tr_r = ui.add(tr_btn);
                            cards.push((tr_r.rect, Act::Transcribe(m.id)));
                        }
                    }
                    // Современные структурированные сообщения ботов (`RichMessage`: таблицы, цитаты, списки — фикс `eroot`!).
                    if !m.rich_blocks.is_empty() {
                        render_rich_blocks(ui, &m.rich_blocks, w, text_c, date_c, m.id, env, &mut hot);
                    } else if !m.text.is_empty() && !m.entities.is_empty() {
                        let cached = env.rich_cache.get(&(m.id, m.text.len()));
                        let fallback;
                        let runs = match cached {
                            Some(r) => r.as_slice(),
                            None => {
                                fallback = super::rich::build_runs(&m.text, &m.entities);
                                fallback.as_slice()
                            }
                        };
                        hot.extend(super::rich::render(
                            ui,
                            env.emoji,
                            env.custom_tex,
                            runs,
                            15.0,
                            text_c,
                            c_orange(),
                            m.id,
                            env.spoilers,
                        ));
                    } else if !m.text.is_empty() {
                        match env.emoji.filter(|e| e.contains(&m.text)) {
                            Some(e) => e.rich_label(ui, &m.text, 15.0, text_c),
                            None => {
                                ui.add(egui::Label::new(galley));
                            }
                        }
                    }
                    // Интерактивный чек-лист (To-Do, Layer 233).
                    if let Some(todo) = &m.todo {
                        ui.add_space(4.0);
                        let done_cnt = todo.items.iter().filter(|x| x.done).count();
                        let total_cnt = todo.items.len().max(1);
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&todo.title).strong().size(14.5).color(text_c));
                            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                                ui.label(RichText::new(format!("{done_cnt}/{total_cnt}")).size(11.5).color(c_orange()));
                            });
                        });
                        let (bar_r, _) = ui.allocate_exact_size(Vec2::new(w, 4.0), Sense::hover());
                        ui.painter().rect_filled(bar_r, 2.0, Color32::from_white_alpha(20));
                        let frac = done_cnt as f32 / total_cnt as f32;
                        if frac > 0.0 {
                            ui.painter().rect_filled(
                                Rect::from_min_size(bar_r.min, Vec2::new(bar_r.width() * frac, 4.0)),
                                2.0,
                                c_orange(),
                            );
                        }
                        ui.add_space(4.0);
                        for it in &todo.items {
                            let row_r = ui.horizontal(|ui| {
                                let (cb_r, _) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::hover());
                                if it.done {
                                    ui.painter().rect_filled(cb_r, egui::CornerRadius::same(5), c_orange());
                                    ui.painter().text(cb_r.center(), egui::Align2::CENTER_CENTER, "✓", FontId::proportional(12.0), super::theme::on_accent());
                                } else {
                                    ui.painter().rect_stroke(cb_r, egui::CornerRadius::same(5), egui::Stroke::new(1.5, date_c), egui::StrokeKind::Inside);
                                }
                                let mut rt = RichText::new(&it.text).size(13.5).color(if it.done { date_c } else { text_c });
                                if it.done {
                                    rt = rt.strikethrough();
                                }
                                ui.label(rt);
                            }).response.rect;
                            cards.push((row_r, Act::ToggleTodo(m.id, it.id, !it.done)));
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
                        if m.effect.is_some() {
                            extra.push(("dialogs/dialogs_premium", "✨".to_owned()));
                        }
                        if m.reaction_items.is_empty() && m.reactions > 0 && reactions_visible(s) {
                            extra.push((
                                "dialogs/dialogs_reaction",
                                rg_core::engine::format_compact_count(m.reactions, env.exact_numbers),
                            ));
                        }
                        if m.reaction_items.is_empty() && m.paid_reactions > 0 {
                            extra.push((
                                "settings/favorite",
                                format!("★{}", rg_core::engine::format_compact_count(m.paid_reactions, env.exact_numbers)),
                            ));
                        }
                        if let Some(mi) = &m.media {
                            if mi.spoiler {
                                extra.push(("menu/stealth", "Спойлер".to_owned()));
                            }
                            if let Some(ttl) = mi.ttl_seconds {
                                extra.push(("settings/ttl", format!("{ttl}с")));
                            }
                        }
                        if let Some(v) = m.views {
                            extra.push((
                                "history_views",
                                rg_core::engine::format_compact_count(v, env.exact_numbers),
                            ));
                        }
                        let note_w = if m.deleted { 17.0 } else { 0.0 };
                        let extra_w: f32 = note_w + extra
                            .iter()
                            .map(|(n, t)| {
                                icons::native_size(n).map_or(0.0, |s| s.x)
                                    + t.chars().count() as f32 * 7.0
                                    + 8.0
                            })
                            .sum::<f32>();
                        ui.add_space((w - meta_w - extra_w - if is_sticker_only { 14.0 } else { 0.0 }).max(0.0));
                        // Для стикеров на прозрачном фоне время рисуется в аккуратной полупрозрачной тёмной пилюле.
                        let meta_pill_frame = if is_sticker_only {
                            egui::Frame::new()
                                .fill(Color32::from_black_alpha(130))
                                .corner_radius(egui::CornerRadius::same(9))
                                .inner_margin(egui::Margin::symmetric(6, 2))
                        } else {
                            egui::Frame::NONE
                        };
                        let pill_resp = meta_pill_frame.show(ui, |ui| {
                            ui.horizontal(|ui| {
                                ui.spacing_mut().item_spacing.x = 4.0;
                                for (n, t) in extra {
                                    let sz = icons::native_size(n).unwrap_or(Vec2::splat(12.0));
                                    let (r, _) = ui.allocate_exact_size(Vec2::new(sz.x, 14.0), Sense::hover());
                                    icons::tg(ui.painter(), n, r.center(), if is_sticker_only { Color32::WHITE } else { date_c });
                                    if !t.is_empty() {
                                        ui.label(RichText::new(t).size(12.0).color(if is_sticker_only { Color32::WHITE } else { date_c }));
                                    }
                                }
                                if m.deleted {
                                    let custom = s.deleted_mark.trim();
                                    let default_icon = custom.is_empty() || matches!(custom, "🧹" | "🗑" | "🗑️");
                                    if default_icon {
                                        let (r, resp) = ui.allocate_exact_size(Vec2::new(14.0, 14.0), Sense::hover());
                                        icons::tg_sized(ui.painter(), "menu/delete", r.center(), Vec2::splat(13.0), date_c);
                                        resp.on_hover_text("Сообщение удалено отправителем");
                                    } else {
                                        let resp = ui.label(RichText::new(custom).size(11.5).color(date_c));
                                        resp.on_hover_text("Сообщение удалено отправителем");
                                    }
                                }
                                ui.label(RichText::new(meta).size(12.0).color(if is_sticker_only { Color32::WHITE } else { date_c }));
                                if m.outgoing {
                                    if m.id < 0 {
                                        let (r, _) = ui.allocate_exact_size(Vec2::new(14.0, 12.0), Sense::hover());
                                        icons::tg_sized(ui.painter(), "dialogs/dialogs_sending", r.center(), Vec2::splat(11.0), if is_sticker_only { Color32::WHITE } else { OUT_TICK });
                                    } else {
                                        let (r, _) = ui.allocate_exact_size(Vec2::new(18.0, 12.0), Sense::hover());
                                        icons::tick(ui.painter(), r.center(), if is_sticker_only { Color32::WHITE } else { OUT_TICK }, m.id <= env.read_out);
                                    }
                                }
                            });
                        }).response;
                        if m.effect.is_some() {
                            cards.push((pill_resp.rect, Act::ReplayEffect(m.id)));
                        }
                    });
                    if !m.deleted && !m.reaction_items.is_empty() && reactions_visible(s) {
                        ui.add_space(5.0);
                        let glass_m = super::theme::mobile_glass_active();
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::new(5.0, 5.0);
                            for pill in &m.reaction_items {
                                let cnt_str = rg_core::engine::format_compact_count(pill.count, env.exact_numbers);
                                let font_id = FontId::proportional(12.0);
                                let has_color_emoji = !pill.paid
                                    && pill.custom_doc_id.is_none()
                                    && env.emoji.and_then(|em| em.image_str(&pill.emoji, 18.0)).is_some();
                                let has_custom = pill.custom_doc_id.is_some();
                                let text_col = if pill.paid {
                                    Color32::from_rgb(255, 228, 136)
                                } else if pill.chosen {
                                    Color32::WHITE
                                } else {
                                    Color32::from_rgb(235, 236, 242)
                                };
                                let count_gal = ui.painter().layout_no_wrap(cnt_str.clone(), font_id.clone(), text_col);
                                let pw = if pill.paid || has_custom || has_color_emoji {
                                    (8.0 + 18.0 + 5.0 + count_gal.size().x + 9.0).max(48.0)
                                } else {
                                    let lbl = format!("{} {cnt_str}", pill.emoji);
                                    let g = ui.painter().layout_no_wrap(lbl, font_id.clone(), text_col);
                                    (g.size().x + 18.0).max(44.0)
                                };
                                let (pr, presp) = ui.allocate_exact_size(Vec2::new(pw, 26.0), Sense::hover());
                                let hk = tween(ui.ctx(), ("rxp", m.id, &pill.emoji), f32::from(u8::from(presp.hovered())), 0.12);
                                let click_pop = env
                                    .rx_pop
                                    .get(&(m.id, pill.emoji.clone()))
                                    .map(|&t0| {
                                        let dt = ((env.time - t0) / 0.24).clamp(0.0, 1.0) as f32;
                                        if dt < 1.0 {
                                            ui.ctx().request_repaint();
                                            (dt * std::f32::consts::PI).sin() * 0.26
                                        } else {
                                            0.0
                                        }
                                    })
                                    .unwrap_or(0.0);
                                let pop = 1.0 + 0.09 * ios_spring_bounce(hk) + click_pop;
                                let (bg, border) = if pill.chosen && !pill.paid {
                                    (
                                        t_btn(c_orange()).gamma_multiply(0.32 + 0.14 * hk),
                                        t_btn(c_orange()).gamma_multiply(0.90),
                                    )
                                } else if pill.paid {
                                    (
                                        Color32::from_rgba_unmultiplied(245, 190, 50, (38.0 + 24.0 * hk) as u8),
                                        Color32::from_rgba_unmultiplied(245, 195, 55, 185),
                                    )
                                } else {
                                    (
                                        Color32::from_rgba_unmultiplied(255, 255, 255, (20.0 + 16.0 * hk) as u8),
                                        Color32::from_white_alpha((30.0 + 26.0 * hk) as u8),
                                    )
                                };
                                ui.painter().rect_filled(pr, egui::CornerRadius::same(13), bg);
                                ui.painter().rect_stroke(
                                    pr,
                                    egui::CornerRadius::same(13),
                                    egui::Stroke::new(1.0, border),
                                    egui::StrokeKind::Inside,
                                );
                                if glass_m {
                                    ui.painter().line_segment(
                                        [Pos2::new(pr.left() + 10.0, pr.top() + 0.8), Pos2::new(pr.right() - 10.0, pr.top() + 0.8)],
                                        egui::Stroke::new(1.0, Color32::from_white_alpha(64)),
                                    );
                                }
                                let icon_c = Pos2::new(pr.left() + 16.5, pr.center().y);
                                let text_pos = Pos2::new(pr.left() + 30.0, pr.center().y);
                                if pill.paid {
                                    icons::paint_star(
                                        ui.painter(),
                                        icon_c,
                                        6.2 * pop,
                                        Color32::from_rgb(245, 195, 55),
                                    );
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::LEFT_CENTER,
                                        &cnt_str,
                                        font_id,
                                        text_col,
                                    );
                                } else if let Some(did) = pill.custom_doc_id {
                                    if let Some(tex) = env.custom_tex.get(&did) {
                                        let ir = Rect::from_center_size(icon_c, Vec2::splat(18.0 * pop));
                                        egui::Image::new(tex).fit_to_exact_size(ir.size()).paint_at(ui, ir);
                                    } else {
                                        let pulse = 0.5 + 0.5 * ((env.time * 4.2).sin() as f32);
                                        ui.ctx().request_repaint_after(std::time::Duration::from_millis(45));
                                        ui.painter().circle_filled(
                                            icon_c,
                                            7.5,
                                            Color32::from_white_alpha((26.0 + 28.0 * pulse) as u8),
                                        );
                                    }
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::LEFT_CENTER,
                                        &cnt_str,
                                        font_id,
                                        text_col,
                                    );
                                } else if let Some(img) = env.emoji.and_then(|em| em.image_str(&pill.emoji, 18.0 * pop)) {
                                    let ir = Rect::from_center_size(icon_c, Vec2::splat(18.0 * pop));
                                    img.paint_at(ui, ir);
                                    ui.painter().text(
                                        text_pos,
                                        egui::Align2::LEFT_CENTER,
                                        &cnt_str,
                                        font_id,
                                        text_col,
                                    );
                                } else {
                                    let lbl = format!("{} {cnt_str}", pill.emoji);
                                    ui.painter().text(
                                        pr.center(),
                                        egui::Align2::CENTER_CENTER,
                                        &lbl,
                                        font_id,
                                        text_col,
                                    );
                                }
                                if pill.paid {
                                    if env.paid_reactions_available {
                                        cards.push((pr, Act::PaidReact(m.id)));
                                    }
                                } else if pill.chosen || env.reactions_policy.allows(&pill.emoji) {
                                    cards.push((pr, Act::ReactToggle(m.id, pill.emoji.clone(), pill.chosen)));
                                }
                            }
                        });
                    }
                    if ((env.is_channel && env.has_discussion) || m.has_comments) && !m.deleted && m.id > 0 {
                        ui.add_space(4.0);
                        let lbl = plural_comments_ex(m.replies_count, env.exact_numbers);
                        let (row_r, row_resp) = ui.allocate_exact_size(Vec2::new(w.max(150.0), 32.0), Sense::click());
                        let hk = tween(ui.ctx(), ("disc-row-h", m.id), f32::from(u8::from(row_resp.hovered())), 0.12);
                        let full_l = row_r.left() - MSG_PAD_X;
                        let full_r = row_r.right() + MSG_PAD_X;
                        if hk > 0.01 {
                            let hrect = Rect::from_min_max(
                                Pos2::new(full_l + 1.0, row_r.top() + 1.0),
                                Pos2::new(full_r - 1.0, row_r.bottom() + 2.0),
                            );
                            let bot_r = radius.se.min(10);
                            ui.painter().rect_filled(
                                hrect,
                                egui::CornerRadius { nw: 0, ne: 0, sw: bot_r, se: bot_r },
                                Color32::from_rgba_unmultiplied(c_orange().r(), c_orange().g(), c_orange().b(), (24.0 * hk) as u8),
                            );
                        }
                        ui.painter().line_segment(
                            [Pos2::new(full_l, row_r.top()), Pos2::new(full_r, row_r.top())],
                            egui::Stroke::new(1.0, Color32::from_white_alpha(22)),
                        );
                        let mut text_left = row_r.left() + 4.0;
                        if !m.recent_repliers.is_empty() {
                            let step = 13.0;
                            for (ai, &uid) in m.recent_repliers.iter().take(3).enumerate() {
                                let ac = Pos2::new(text_left + 9.0 + ai as f32 * step, row_r.center().y + 1.0);
                                ui.painter().circle_filled(ac, 10.5, fill);
                                if let Some(tex) = env.avatars.get(&uid) {
                                    let ar = Rect::from_center_size(ac, Vec2::splat(19.0));
                                    egui::Image::new(tex)
                                        .corner_radius(egui::CornerRadius::same(10))
                                        .fit_to_exact_size(ar.size())
                                        .paint_at(ui, ar);
                                } else {
                                    ui.painter().circle_filled(ac, 9.0, avatar_color(uid));
                                    let glyph = ((uid.unsigned_abs() % 26) as u8 + b'A') as char;
                                    ui.painter().text(ac, egui::Align2::CENTER_CENTER, glyph.to_string(), FontId::proportional(9.5), Color32::WHITE);
                                }
                            }
                            text_left += (m.recent_repliers.len().min(3) as f32 - 1.0) * step + 23.0;
                        } else {
                            icons::tg_sized(
                                ui.painter(),
                                "history_comments",
                                Pos2::new(row_r.left() + 10.0, row_r.center().y + 1.0),
                                Vec2::splat(16.0),
                                c_orange(),
                            );
                            text_left = row_r.left() + 26.0;
                        }
                        ui.painter().text(
                            Pos2::new(text_left, row_r.center().y + 1.0),
                            egui::Align2::LEFT_CENTER,
                            lbl,
                            FontId::proportional(13.0),
                            c_orange(),
                        );
                        let chev_x = row_r.right() - 8.0 + 2.0 * hk;
                        let chev_y = row_r.center().y + 1.0;
                        ui.painter().add(egui::Shape::line(
                            vec![
                                Pos2::new(chev_x - 3.5, chev_y - 4.0),
                                Pos2::new(chev_x, chev_y),
                                Pos2::new(chev_x - 3.5, chev_y + 4.0),
                            ],
                            egui::Stroke::new(1.6, c_orange()),
                        ));
                        cards.push((row_r, Act::Discuss(m.id)));
                    }
                });
            });
        let rect = inner.response.rect;
        // Хвостик пузыря (не рисуется у стикеров на прозрачном фоне и выключается `remove_message_tail`).
        if !is_sticker_only && !s.remove_message_tail && (m.outgoing || show_avatar) {
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
        // Пружинная вспышка частиц при эффекте сообщения (`Message.effect`).
        if let Some(&t0) = env.effect_bursts.get(&m.id) {
            let dt = (env.time - t0) as f32;
            if (0.0..1.1).contains(&dt) {
                ui.ctx().request_repaint();
                let prog = (dt / 1.1).clamp(0.0, 1.0);
                let fade = (1.0 - prog).powf(1.4);
                let origin = rect.center();
                for k in 0..14 {
                    let ang = k as f32 * (std::f32::consts::TAU / 14.0) + 0.3;
                    let dist = (28.0 + (k % 3) as f32 * 18.0) * ios_spring(prog);
                    let pos = origin + Vec2::new(ang.cos() * dist, ang.sin() * dist - prog * 16.0);
                    let col = match k % 3 {
                        0 => c_orange(),
                        1 => Color32::from_rgb(255, 224, 92),
                        _ => Color32::from_rgb(255, 130, 180),
                    }
                    .gamma_multiply(fade);
                    ui.painter().circle_filled(pos, (3.8 * fade).max(1.0), col);
                }
            }
        }
        // Быстрые кнопки справа от пузыря (пересылка и быстрое копирование текста).
        let can_fast_share = !s.hide_fast_share && !env.hide_share_button && !m.deleted && !m.outgoing && (env.is_channel || m.forwarded_from.is_some());
        if can_fast_share {
            let f_center = Pos2::new(rect.right() + 18.0, rect.center().y);
            let f_rect = Rect::from_center_size(f_center, Vec2::splat(24.0));
            let fresp = ui.interact(f_rect, ui.make_persistent_id(("fshare", m.id)), Sense::click());
            let bg = if fresp.hovered() { t_btn(ORANGE) } else { t2(WINDOW_BG) };
            ui.painter().circle_filled(f_center, 11.5, bg);
            icons::tg_sized(ui.painter(), "info/info_media_forward", f_center, Vec2::splat(14.0), if fresp.hovered() { super::theme::on_accent() } else { WHITE });
            if fresp.on_hover_text("Быстрая пересылка").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Act::Forward(m.id));
            }
        }
        if !m.deleted && !m.text.is_empty() && !m.outgoing {
            let c_y = if can_fast_share { rect.center().y + 28.0 } else { rect.center().y };
            let c_center = Pos2::new(rect.right() + 18.0, c_y);
            let c_rect = Rect::from_center_size(c_center, Vec2::splat(22.0));
            let cresp = ui.interact(c_rect, ui.make_persistent_id(("fcopy", m.id)), Sense::click());
            let hovered_msg = rect.expand2(Vec2::new(40.0, 4.0));
            let show_copy = cresp.hovered() || ui.ctx().input(|i| i.pointer.hover_pos().is_some_and(|p| hovered_msg.contains(p)));
            if show_copy {
                let bg = if cresp.hovered() { t_btn(ORANGE) } else { t2(WINDOW_BG) };
                ui.painter().circle_filled(c_center, 10.5, bg);
                icons::tg_sized(ui.painter(), "menu/copy", c_center, Vec2::splat(12.5), if cresp.hovered() { super::theme::on_accent() } else { WHITE });
            }
            if cresp.on_hover_text("Копировать текст").on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                acts.push(Act::Copy(m.text.clone()));
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
        if resp.hovered()
            && (on_card.is_some()
                || matches!(
                    on_hot,
                    Some(
                        super::rich::Hit::Link(_)
                            | super::rich::Hit::Hashtag(_)
                            | super::rich::Hit::Spoiler(_)
                    )
                ))
        {
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
            let code = if m.outgoing { env.double_tap_action_out } else { env.double_tap_action };
            let eff_react = env
                .reactions_policy
                .default_allowed(env.default_reaction)
                .unwrap_or("");
            if let Some(a) = double_tap_act_for(code, m.outgoing, m.id, &m.text, eff_react) {
                acts.push(a);
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
                    super::rich::Hit::Hashtag(tag) => Act::SearchHashtag(tag),
                    super::rich::Hit::Spoiler(i) => Act::Spoiler(m.id, i),
                });
            }
        }
        if env.scroll_to == Some(m.id) {
            resp.scroll_to_me(Some(Align::Center));
        }
        egui::Popup::context_menu(&resp)
            .frame(kit::compact_popup_frame())
            .show(|ui| menu(ui, m, env, acts));
    });
    // Инлайн-кнопки бота прямо под пузырём сообщения (`ReplyInlineMarkup`) в стиле Telegram (закруглённый нижний ряд).
    if !m.inline_buttons.is_empty() && !m.deleted {
        let total_w = (bubble_w + 2.0 * MSG_PAD_X).max(220.0).min(env.avail * 0.84);
        let left_pad = if env.group && !m.outgoing { 46.0 } else { 6.0 };
        let last_r = m.inline_buttons.len().saturating_sub(1);
        for (r_idx, row) in m.inline_buttons.iter().enumerate() {
            if row.is_empty() {
                continue;
            }
            ui.add_space(3.0);
            ui.with_layout(layout, |ui| {
                ui.add_space(left_pad);
                let cols = row.len() as f32;
                let gap = 3.0;
                let bw = ((total_w - gap * (cols - 1.0).max(0.0)) / cols).max(46.0);
                let row_cr = if r_idx == last_r {
                    egui::CornerRadius { nw: 6, ne: 6, sw: 12, se: 12 }
                } else {
                    egui::CornerRadius::same(6)
                };
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = gap;
                    for (c_idx, btn) in row.iter().enumerate() {
                        let (brect, bresp) = ui.allocate_exact_size(Vec2::new(bw, 34.0), Sense::click());
                        let hk = tween(ui.ctx(), ("ibtn-h", m.id, r_idx, c_idx), f32::from(u8::from(bresp.hovered())), 0.12);
                        let pk = tween(ui.ctx(), ("ibtn-p", m.id, r_idx, c_idx), f32::from(u8::from(bresp.is_pointer_button_down_on())), 0.08);
                        let draw_r = brect.shrink(pk * 1.2);
                        let (base_fill, hover_fill, border_c) = bot_btn_palette(btn.style);
                        let bfill = lerp_color(base_fill, hover_fill, hk);
                        ui.painter().rect_filled(draw_r, row_cr, bfill);
                        ui.painter().rect_stroke(
                            draw_r,
                            row_cr,
                            egui::Stroke::new(1.0, border_c.gamma_multiply(0.38 + hk * 0.45)),
                            egui::StrokeKind::Inside,
                        );
                        // Анимация бегущего блика ожидания, пока бот обрабатывает callback.
                        let is_pending = env
                            .bot_btn_pending
                            .is_some_and(|(pmid, plbl, _)| pmid == m.id && plbl == btn.text);
                        if is_pending {
                            ui.ctx().request_repaint_after(std::time::Duration::from_millis(32));
                            let phase = ((env.time * 1.8) % 1.0) as f32;
                            let sw = draw_r.width() * 0.35;
                            let sx = draw_r.left() - sw + phase * (draw_r.width() + sw);
                            let shimmer = Rect::from_min_size(Pos2::new(sx, draw_r.top()), Vec2::new(sw, draw_r.height()));
                            ui.painter()
                                .with_clip_rect(draw_r)
                                .rect_filled(shimmer, egui::CornerRadius::same(6), Color32::from_white_alpha(52));
                        }
                        // Волна-подсветка при нажатии.
                        if pk > 0.05 {
                            let rad = draw_r.width() * 0.55 * pk;
                            ui.painter()
                                .with_clip_rect(draw_r)
                                .circle_filled(draw_r.center(), rad, Color32::from_white_alpha((34.0 * pk) as u8));
                        }
                        // Значок действия в правом верхнем углу кнопки (официальные векторные иконки Telegram).
                        let corner_ico = match &btn.kind {
                            rg_core::engine::BotBtnKind::Url(_)
                            | rg_core::engine::BotBtnKind::WebView { .. } => Some("info/info_media_forward"),
                            rg_core::engine::BotBtnKind::SwitchInline { .. } => Some("chat/input_forward"),
                            rg_core::engine::BotBtnKind::Copy(_) => Some("inline_button_copy"),
                            _ => None,
                        };
                        if let Some(ico_n) = corner_ico {
                            icons::tg_sized(
                                ui.painter(),
                                ico_n,
                                Pos2::new(draw_r.right() - 8.0, draw_r.top() + 8.0),
                                Vec2::splat(10.0),
                                Color32::from_white_alpha(180),
                            );
                        }
                        // Необязательный пользовательский эмодзи-значок кнопки (`KeyboardButtonStyle.icon`).
                        let mut text_center = draw_r.center();
                        if let Some(icon_id) = btn.icon_id
                            && let Some(tex) = env.custom_tex.get(&icon_id)
                        {
                            let ir = Rect::from_center_size(Pos2::new(draw_r.left() + 16.0, draw_r.center().y), Vec2::splat(16.0));
                            egui::Image::new(tex).fit_to_exact_size(ir.size()).paint_at(ui, ir);
                            text_center.x += 8.0;
                        }
                        let max_chars = ((bw / 7.2) as usize).clamp(4, 34);
                        let lbl = short(&btn.text, max_chars);
                        ui.painter().with_clip_rect(draw_r.shrink2(Vec2::new(8.0, 2.0))).text(
                            text_center,
                            egui::Align2::CENTER_CENTER,
                            lbl,
                            FontId::proportional(13.0),
                            Color32::WHITE,
                        );
                        if bresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                            acts.push(Act::BotBtn(m.id, btn.text.clone(), btn.kind.clone()));
                        }
                    }
                });
            });
        }
    }
}

/// Палитра цветов кнопок бота (`(обычный_фон, фон_наведения, кромка)`) по `KeyboardButtonStyle`.
pub(crate) fn bot_btn_palette(style: u8) -> (Color32, Color32, Color32) {
    match style {
        // Primary — фирменный синий Telegram.
        1 => (
            Color32::from_rgba_unmultiplied(46, 139, 230, 220),
            Color32::from_rgba_unmultiplied(64, 156, 245, 245),
            Color32::from_rgba_unmultiplied(120, 190, 255, 160),
        ),
        // Danger — красный.
        2 => (
            Color32::from_rgba_unmultiplied(216, 68, 68, 215),
            Color32::from_rgba_unmultiplied(235, 84, 84, 242),
            Color32::from_rgba_unmultiplied(255, 145, 145, 150),
        ),
        // Success — зелёный.
        3 => (
            Color32::from_rgba_unmultiplied(42, 166, 94, 215),
            Color32::from_rgba_unmultiplied(56, 188, 112, 242),
            Color32::from_rgba_unmultiplied(130, 235, 170, 150),
        ),
        // Матовая полупрозрачная стеклянная кнопка бота (без синего/оранжевого блика).
        _ => (
            Color32::from_rgba_unmultiplied(30, 30, 36, 210),
            Color32::from_rgba_unmultiplied(44, 44, 54, 232),
            Color32::from_white_alpha(38),
        ),
    }
}

/// Показывать ли счётчик реакций у сообщений.
fn reactions_visible(s: &AyuSettings) -> bool {
    s.show_private_chat_reactions || s.show_group_reactions || s.show_channel_reactions
}

/// Выбор действия при двойном клике по сообщению (входящие vs свои).
fn double_tap_act_for(code: i32, outgoing: bool, msg_id: i32, text: &str, default_reaction: &str) -> Option<Act> {
    match code {
        1 => (!default_reaction.trim().is_empty()).then(|| Act::React(msg_id, default_reaction.to_owned())),
        2 => Some(Act::Reply(msg_id, text.to_owned())),
        3 if !text.is_empty() => Some(Act::Copy(text.to_owned())),
        4 => Some(Act::Forward(msg_id)),
        5 if outgoing && !text.is_empty() => Some(Act::Edit(msg_id, text.to_owned())),
        6 => Some(Act::SaveToSaved(msg_id)),
        7 if !text.is_empty() => Some(Act::Translate(msg_id, text.to_owned())),
        _ => None,
    }
}

/// Контекстное меню сообщения: обычные пункты Telegram и пункты AyuGram по их видимости.
fn menu(ui: &mut egui::Ui, m: &MessageRow, env: &Env, acts: &mut Vec<Act>) {
    let s = env.s;
    ui.set_min_width(180.0);
    let show_star_reaction = env.is_channel && env.paid_reactions_available;
    if s.menu_reactions_panel.shown(env.shift) && !m.deleted {
        let allowed = env.reactions_policy.quick_list(&REACTIONS);
        if !allowed.is_empty() || show_star_reaction {
            egui::Frame::new()
                .fill(Color32::from_rgba_unmultiplied(255, 255, 255, 10))
                .stroke(egui::Stroke::new(1.0, Color32::from_white_alpha(18)))
                .corner_radius(egui::CornerRadius::same(14))
                .inner_margin(egui::Margin::symmetric(6, 4))
                .show(ui, |ui| {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 3.0;
                        if show_star_reaction {
                            let (s_rect, s_resp) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::click());
                            let shk = tween(ui.ctx(), ("mstar", m.id), f32::from(u8::from(s_resp.hovered())), 0.12);
                            let s_fill = Color32::from_rgba_unmultiplied(245, 190, 50, (44.0 + 36.0 * shk) as u8);
                            ui.painter().rect(
                                s_rect,
                                egui::CornerRadius::same(14),
                                s_fill,
                                egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(245, 195, 55, 190)),
                                egui::StrokeKind::Inside,
                            );
                            icons::paint_star(
                                ui.painter(),
                                s_rect.center(),
                                6.5 * (1.0 + 0.1 * ios_spring_bounce(shk)),
                                Color32::from_rgb(245, 195, 55),
                            );
                            if s_resp
                                .on_hover_text("Звёздная реакция (Telegram Stars)")
                                .on_hover_cursor(egui::CursorIcon::PointingHand)
                                .clicked()
                            {
                                acts.push(Act::PaidReact(m.id));
                                ui.close();
                            }
                        }
                        for e in &allowed {
                            let (r_rect, r_resp) = ui.allocate_exact_size(Vec2::new(28.0, 28.0), Sense::click());
                            let hk = tween(ui.ctx(), ("mrx", m.id, e), f32::from(u8::from(r_resp.hovered())), 0.12);
                            if hk > 0.02 {
                                ui.painter().circle_filled(
                                    r_rect.center(),
                                    13.5,
                                    Color32::from_white_alpha((26.0 * hk) as u8),
                                );
                            }
                            let scale_sz = 19.5 * (1.0 + 0.16 * ios_spring_bounce(hk));
                            if let Some(img) = env.emoji.and_then(|em| em.image_str(e, scale_sz)) {
                                img.paint_at(ui, Rect::from_center_size(r_rect.center(), Vec2::splat(scale_sz)));
                            } else {
                                ui.painter().text(
                                    r_rect.center(),
                                    egui::Align2::CENTER_CENTER,
                                    e,
                                    FontId::proportional(17.5 * (1.0 + 0.12 * hk)),
                                    Color32::WHITE,
                                );
                            }
                            if r_resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                acts.push(Act::React(m.id, e.clone()));
                                ui.close();
                            }
                        }
                    });
                });
            kit::menu_separator(ui);
        }
    }
    ui.spacing_mut().item_spacing = Vec2::new(0.0, 1.0);
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
    if m.media.as_ref().is_some_and(|i| i.kind == rg_core::media::MediaKind::Photo) {
        item(ui, "menu/copy", "Копировать фото", Act::CopyPhoto(m.id));
    }
    if m.media.as_ref().is_some_and(|i| i.is_downloadable()) {
        item(ui, "menu/download", "Сохранить файл", Act::Save(m.id));
        item(ui, "menu/show_in_folder", "Открыть", Act::Open(m.id));
    }
    if m.outgoing && !m.deleted {
        item(ui, "menu/edit", "Изменить", Act::Edit(m.id, m.text.clone()));
    }
    if !m.deleted {
        if m.media.as_ref().is_some_and(|i| {
            matches!(i.kind, rg_core::media::MediaKind::Voice | rg_core::media::MediaKind::Round)
        }) {
            item(ui, "settings/language", "Расшифровать голос", Act::Transcribe(m.id));
        }
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
        ui.label(RichText::new(format!("Просмотров: {v}")).size(12.5).color(SUBTEXT));
    }
    if ((env.is_channel && env.has_discussion) || m.has_comments) && !m.deleted && m.id > 0 {
        item(ui, "menu/reply", "Комментарии", Act::Discuss(m.id));
    }
    if let Some((set_id, access_hash)) = m.media.as_ref().and_then(|i| i.sticker_set) {
        item(ui, "menu/stickers", "Набор стикеров", Act::OpenStickerPack(set_id, access_hash));
    }
    if s.menu_user_messages.shown(env.shift) && env.group && m.sender_id != 0 {
        item(ui, "menu/search", "Сообщения участника", Act::FilterUser(m.sender_id, m.sender.clone()));
    }
    if !m.deleted && m.id > 0 && env.can_copy_link {
        item(ui, "info/info_link", "Копировать ссылку", Act::CopyLink(m.id));
    }
    if s.menu_message_details.shown(env.shift) {
        item(ui, "menu/info", "Сведения", Act::Details(m.id));
        item(ui, "menu/copy", "Копировать ID", Act::CopyId(m.id));
    }
    if !m.deleted {
        kit::menu_separator(ui);
        if kit::menu_item(ui, "menu/delete", "Удалить", true) {
            acts.push(Act::Delete(m.id));
            ui.close();
        }
    } else {
        kit::menu_separator(ui);
        if kit::menu_item(ui, "menu/delete", "Удалить у себя", true) {
            acts.push(Act::PurgeLocal(m.id));
            ui.close();
        }
        if kit::menu_item(ui, "menu/clear", "Убрать все удалённые", false) {
            acts.push(Act::PurgeAllDeleted);
            ui.close();
        }
    }
}

/// Рендерит современные структурированные сообщения ботов (`RichMessage`, Layer 233:
/// таблицы с ценами, цитаты с акцентной полосой, заголовки, списки с кастомными эмодзи, разделители).
#[allow(clippy::too_many_arguments)]
fn render_rich_blocks(
    ui: &mut egui::Ui,
    blocks: &[rg_core::rich_msg::RichBlock],
    w: f32,
    text_c: Color32,
    date_c: Color32,
    msg_id: i32,
    env: &Env,
    hot: &mut Vec<(Rect, super::rich::Hit)>,
) {
    use rg_core::rich_msg::RichBlock;
    let render_rich_text = |ui: &mut egui::Ui,
                            txt: &str,
                            ents: &[rg_core::engine::Entity],
                            size: f32,
                            col: Color32,
                            bold: bool,
                            hot: &mut Vec<(Rect, super::rich::Hit)>| {
        if txt.is_empty() {
            return;
        }
        let mut runs = super::rich::build_runs(txt, ents);
        if bold {
            for r in &mut runs {
                r.bold = true;
            }
        }
        hot.extend(super::rich::render(
            ui,
            env.emoji,
            env.custom_tex,
            &runs,
            size,
            col,
            c_orange(),
            msg_id,
            env.spoilers,
        ));
    };

    for blk in blocks {
        match blk {
            RichBlock::Heading { level, text, entities } => {
                ui.add_space(4.0);
                let sz = match level {
                    1 => 17.0,
                    2 => 15.5,
                    _ => 14.5,
                };
                render_rich_text(ui, text, entities, sz, text_c, true, hot);
                ui.add_space(2.0);
            }
            RichBlock::Paragraph { text, entities } => {
                render_rich_text(ui, text, entities, 14.5, text_c, false, hot);
                ui.add_space(2.0);
            }
            RichBlock::Blockquote { text, entities, caption } => {
                ui.add_space(4.0);
                let accent = c_orange();
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 26))
                    .stroke(egui::Stroke::new(1.0, Color32::from_rgba_unmultiplied(accent.r(), accent.g(), accent.b(), 72)))
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin { left: 10, right: 8, top: 6, bottom: 6 })
                    .show(ui, |ui| {
                        ui.set_width((w - 20.0).max(100.0));
                        let bar_r = Rect::from_min_size(
                            Pos2::new(ui.max_rect().left() - 10.0, ui.max_rect().top() - 6.0),
                            Vec2::new(3.5, ui.available_height().max(26.0) + 12.0),
                        );
                        ui.painter().rect_filled(bar_r, 2.0, accent);
                        render_rich_text(ui, text, entities, 13.5, text_c, false, hot);
                        if !caption.is_empty() {
                            ui.label(RichText::new(caption).size(11.5).italics().color(date_c));
                        }
                    });
                ui.add_space(4.0);
            }
            RichBlock::Preformatted { code, language } => {
                ui.add_space(3.0);
                egui::Frame::new()
                    .fill(Color32::from_black_alpha(80))
                    .corner_radius(egui::CornerRadius::same(7))
                    .inner_margin(egui::Margin::same(8))
                    .show(ui, |ui| {
                        ui.set_width((w - 16.0).max(100.0));
                        if !language.is_empty() {
                            ui.label(RichText::new(language).size(10.5).color(c_orange()));
                        }
                        ui.label(RichText::new(code).monospace().size(12.5).color(text_c));
                    });
                ui.add_space(3.0);
            }
            RichBlock::Table { title, rows, bordered, striped } => {
                if rows.is_empty() {
                    continue;
                }
                ui.add_space(5.0);
                if !title.is_empty() {
                    ui.label(RichText::new(title).strong().size(13.5).color(text_c));
                    ui.add_space(2.0);
                }
                let cols = rows.iter().map(Vec::len).max().unwrap_or(1).max(1);
                let col_w = ((w - 8.0) / cols as f32).max(52.0);
                let tbl_frame = egui::Frame::new()
                    .fill(Color32::from_white_alpha(8))
                    .stroke(if *bordered {
                        egui::Stroke::new(1.0, Color32::from_white_alpha(26))
                    } else {
                        egui::Stroke::new(1.0, Color32::from_white_alpha(14))
                    })
                    .corner_radius(egui::CornerRadius::same(8))
                    .inner_margin(egui::Margin::same(4));
                tbl_frame.show(ui, |ui| {
                    ui.set_width((w - 8.0).max(120.0));
                    ui.spacing_mut().item_spacing.y = 0.0;
                    for (r_i, row) in rows.iter().enumerate() {
                        let is_hdr = r_i == 0 && row.iter().any(|c| c.header);
                        let row_bg = if is_hdr {
                            Color32::from_white_alpha(18)
                        } else if *striped && r_i % 2 == 1 {
                            Color32::from_white_alpha(7)
                        } else {
                            Color32::TRANSPARENT
                        };
                        egui::Frame::new()
                            .fill(row_bg)
                            .inner_margin(egui::Margin::symmetric(4, 4))
                            .show(ui, |ui| {
                                ui.horizontal_top(|ui| {
                                    ui.spacing_mut().item_spacing.x = 4.0;
                                    for cell in row {
                                        let cw = (col_w - 6.0).max(40.0);
                                        ui.allocate_ui_with_layout(
                                            Vec2::new(cw, 0.0),
                                            Layout::top_down(Align::LEFT),
                                            |ui| {
                                                ui.set_width(cw);
                                                render_rich_text(
                                                    ui,
                                                    &cell.text,
                                                    &cell.entities,
                                                    if is_hdr || cell.header { 12.5 } else { 13.0 },
                                                    if is_hdr || cell.header { date_c } else { text_c },
                                                    is_hdr || cell.header,
                                                    hot,
                                                );
                                            },
                                        );
                                    }
                                });
                            });
                        if r_i + 1 < rows.len() {
                            let (lr, _) = ui.allocate_exact_size(Vec2::new(w - 16.0, 1.0), Sense::hover());
                            ui.painter().rect_filled(lr, 0.0, Color32::from_white_alpha(14));
                        }
                    }
                });
                ui.add_space(5.0);
            }
            RichBlock::List { ordered, items } => {
                ui.add_space(3.0);
                for (i, item) in items.iter().enumerate() {
                    ui.horizontal_top(|ui| {
                        if item.checkbox {
                            let (cb_r, _) = ui.allocate_exact_size(Vec2::splat(16.0), Sense::hover());
                            let rbox = cb_r.shrink(1.0);
                            if item.checked {
                                ui.painter().rect_filled(rbox, egui::CornerRadius::same(4), c_orange());
                                let c = rbox.center();
                                let st = egui::Stroke::new(1.8, super::theme::on_accent());
                                ui.painter().line_segment([c + Vec2::new(-3.2, 0.2), c + Vec2::new(-0.8, 2.8)], st);
                                ui.painter().line_segment([c + Vec2::new(-0.8, 2.8), c + Vec2::new(3.5, -2.5)], st);
                            } else {
                                ui.painter().rect_stroke(
                                    rbox,
                                    egui::CornerRadius::same(4),
                                    egui::Stroke::new(1.4, c_orange().gamma_multiply(0.8)),
                                    egui::StrokeKind::Inside,
                                );
                            }
                        } else if *ordered {
                            ui.label(RichText::new(format!("{}.", i + 1)).size(13.5).strong().color(c_orange()));
                        } else {
                            ui.label(RichText::new("•").size(13.5).strong().color(c_orange()));
                        }
                        render_rich_text(ui, &item.text, &item.entities, 14.0, text_c, false, hot);
                    });
                }
                ui.add_space(3.0);
            }
            RichBlock::Divider => {
                ui.add_space(4.0);
                let (lr, _) = ui.allocate_exact_size(Vec2::new(w, 1.0), Sense::hover());
                ui.painter().rect_filled(lr, 0.0, Color32::from_white_alpha(26));
                ui.add_space(4.0);
            }
            RichBlock::Footer { text, entities } => {
                ui.add_space(2.0);
                render_rich_text(ui, text, entities, 12.0, date_c, false, hot);
            }
            RichBlock::Media { caption, .. } => {
                if !caption.is_empty() {
                    ui.label(RichText::new(caption).size(12.5).color(date_c));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_inline_bot_input_handles_valid_and_invalid_queries() {
        assert_eq!(parse_inline_bot_input(""), None);
        assert_eq!(parse_inline_bot_input("@ab cats"), None);
        assert_eq!(parse_inline_bot_input("@gif"), None);
        assert_eq!(parse_inline_bot_input("hello @gif cats"), None);
        assert_eq!(
            parse_inline_bot_input("@gif "),
            Some(("gif".to_owned(), String::new()))
        );
        assert_eq!(
            parse_inline_bot_input("@My_Bot cats cute"),
            Some(("my_bot".to_owned(), "cats cute".to_owned()))
        );
    }

    #[test]
    fn double_tap_act_for_separates_incoming_and_outgoing_actions() {
        assert!(matches!(
            double_tap_act_for(1, false, 10, "hi", "🔥"),
            Some(Act::React(10, ref e)) if e == "🔥"
        ));
        assert!(double_tap_act_for(5, false, 10, "hi", "👍").is_none());
        assert!(matches!(
            double_tap_act_for(5, true, 10, "hi", "👍"),
            Some(Act::Edit(10, ref t)) if t == "hi"
        ));
        assert!(matches!(
            double_tap_act_for(6, false, 11, "x", "👍"),
            Some(Act::SaveToSaved(11))
        ));
        assert!(matches!(
            double_tap_act_for(7, false, 12, "bonjour", "👍"),
            Some(Act::Translate(12, ref t)) if t == "bonjour"
        ));
    }

    #[test]
    fn bubble_corner_radius_respects_single_corner_and_direction() {
        assert_eq!(bubble_corner_radius(14, true, false), egui::CornerRadius::same(14));
        assert_eq!(bubble_corner_radius(14, true, true), egui::CornerRadius::same(14));
        assert_eq!(
            bubble_corner_radius(14, false, true),
            egui::CornerRadius { nw: 14, ne: 14, sw: 14, se: 4 }
        );
        assert_eq!(
            bubble_corner_radius(14, false, false),
            egui::CornerRadius { nw: 14, ne: 14, sw: 4, se: 14 }
        );
        assert_eq!(bubble_corner_radius(3, false, false), egui::CornerRadius::same(3));
    }

    #[test]
    fn mention_prefix_and_completion_work_at_boundaries() {
        assert_eq!(extract_mention_prefix("@ali"), Some((0, "ali")));
        assert_eq!(extract_mention_prefix("Привет @bob"), Some((13, "bob")));
        assert_eq!(extract_mention_prefix("mail@example.com"), None);
        assert_eq!(extract_mention_prefix("привет @ivan "), None);

        let mut s = "Привет @bo".to_owned();
        let (pos, _) = extract_mention_prefix(&s).expect("prefix must exist");
        apply_mention_completion(&mut s, pos, "boris_dev");
        assert_eq!(s, "Привет @boris_dev ");
    }

    #[test]
    fn search_cycle_pin_snippet_and_plural_comments_behave_accurately() {
        assert_eq!(cycle_search_idx(0, 0, true), 0);
        assert_eq!(cycle_search_idx(0, 4, true), 1);
        assert_eq!(cycle_search_idx(3, 4, true), 0);
        assert_eq!(cycle_search_idx(0, 4, false), 3);

        let mut m = MessageRow::plain(7, String::new(), false, 1000);
        m.media = Some(rg_core::media::MediaInfo::basic(
            rg_core::media::MediaKind::Voice,
            "voice.ogg",
            2048,
        ));
        assert_eq!(pin_media_snippet(&m), "Голосовое сообщение");

        assert_eq!(plural_comments_ex(0, false), "Прокомментировать");
        assert_eq!(plural_comments_ex(1, false), "1 комментарий");
        assert_eq!(plural_comments_ex(24, true), "24 комментария");
        assert_eq!(plural_comments_ex(1200, true), "1 200 комментариев");
        assert_eq!(plural_comments_ex(1200, false), "1.2K комментариев");
    }

    #[test]
    fn menu_places_gold_star_reaction_in_top_strip_only_when_paid_available() {
        fn count_star_meshes_and_text(shape: &egui::Shape, meshes: &mut usize, texts: &mut String) {
            match shape {
                egui::Shape::Mesh(m) if m.indices.len() == 30 => *meshes += 1,
                egui::Shape::Text(t) => {
                    texts.push_str(t.galley.text());
                    texts.push('\n');
                }
                egui::Shape::Vec(v) => {
                    for s in v {
                        count_star_meshes_and_text(s, meshes, texts);
                    }
                }
                _ => {}
            }
        }

        let s = AyuSettings::default();
        let tex = HashMap::new();
        let avatars = HashMap::new();
        let fetching = HashMap::new();
        let cached = std::collections::HashSet::new();
        let gif_paths = HashMap::new();
        let translated = HashMap::new();
        let custom_tex = HashMap::new();
        let selected_msgs = std::collections::HashSet::new();
        let transcriptions = HashMap::new();
        let transcribing = std::collections::HashSet::new();
        let effect_bursts = HashMap::new();
        let rich_cache = HashMap::new();
        let spoilers = std::collections::HashSet::new();
        let arrived = HashMap::new();
        let img_arrived = HashMap::new();
        let policy = rg_core::engine::ChatReactionsPolicy::All { allow_custom: false };
        let msg = MessageRow::plain(42, "Пост в канале".into(), false, 1_763_700_000);

        let run_case = |paid_available: bool| -> (usize, String) {
            let ctx = egui::Context::default();
            let env = Env {
                avail: 360.0,
                s: &s,
                tex: &tex,
                shift: false,
                scroll_to: None,
                group: false,
                is_channel: true,
                has_discussion: true,
                format_secs: false,
                double_tap_action: 1,
                double_tap_action_out: 1,
                default_reaction: "👍",
                reactions_policy: &policy,
                arrived: &arrived,
                img_arrived: &img_arrived,
                time: 1.0,
                emoji: None,
                spoilers: &spoilers,
                read_out: 0,
                avatars: &avatars,
                fetching: &fetching,
                cached: &cached,
                gif_paths: &gif_paths,
                translated: &translated,
                custom_tex: &custom_tex,
                show_translate: false,
                clip_rect: Rect::from_min_size(Pos2::ZERO, Vec2::new(400.0, 500.0)),
                selected_msgs: &selected_msgs,
                bot_btn_pending: None,
                highlight_msg: None,
                can_copy_link: true,
                paid_reactions_available: paid_available,
                transcriptions: &transcriptions,
                transcribing: &transcribing,
                effect_bursts: &effect_bursts,
                rich_cache: &rich_cache,
                exact_numbers: false,
                hide_share_button: false,
                rx_pop: &HashMap::new(),
            };
            let mut acts = Vec::new();
            let raw = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(Pos2::ZERO, Vec2::new(320.0, 480.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(raw, |ui| {
                egui::CentralPanel::default().show(ui, |ui| {
                    menu(ui, &msg, &env, &mut acts);
                });
            });
            out.textures_delta.clear();
            let mut meshes = 0;
            let mut txt = String::new();
            for cs in &out.shapes {
                count_star_meshes_and_text(&cs.shape, &mut meshes, &mut txt);
            }
            (meshes, txt)
        };

        let (meshes_on, txt_on) = run_case(true);
        assert!(meshes_on >= 1, "star reaction mesh missing when paid reactions enabled");
        assert!(
            !txt_on.contains("Звёздная реакция"),
            "duplicate text menu item must not appear in menu body: {txt_on}"
        );

        let (meshes_off, txt_off) = run_case(false);
        assert_eq!(meshes_off, 0, "no star mesh expected when paid reactions disabled");
        assert!(!txt_off.contains("Звёздная реакция"));
    }
}
