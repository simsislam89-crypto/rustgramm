//! Карточки вложений в пузыре: фото с просмотром, видео/GIF/кружок с кнопкой воспроизведения,
//! голосовые с настоящей волной, файлы со значком типа. Нажатия собираются в `cards` и
//! регистрируются позже, поверх пузыря, чтобы не перехватываться его контекстным меню.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use rg_core::engine::MessageRow;
use rg_core::media::{MediaInfo, MediaKind, human_size};

use super::chat::Act;
use super::icons;
use super::theme::*;

/// Состояние загрузки вложения для карточки.
#[derive(Clone, Copy, PartialEq)]
pub enum Load {
    /// Не скачано.
    Remote,
    /// Идёт загрузка: скачано и всего (байт).
    Active(u64, u64),
    /// Лежит в кэше, открывается сразу.
    Cached,
}

/// Дуга хода загрузки по часовой стрелке от верха.
fn progress_ring(p: &egui::Painter, c: Pos2, r: f32, frac: f32, color: Color32) {
    let n = 48;
    let end = frac.clamp(0.0, 1.0) * std::f32::consts::TAU;
    let pts: Vec<Pos2> = (0..=n)
        .map(|i| {
            let a = -std::f32::consts::FRAC_PI_2 + end * i as f32 / n as f32;
            c + Vec2::new(a.cos(), a.sin()) * r
        })
        .collect();
    p.add(egui::Shape::line(pts, Stroke::new(2.5, color)));
}

/// Доля загрузки 0..1 (неизвестный размер — 0).
fn load_frac(done: u64, total: u64) -> f32 {
    if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 }
}

/// Круглая кнопка загрузки файла/голосового как в Telegram: на тёмном кружке кольцо хода и
/// крестик отмены, пока идёт загрузка; иначе ничего (рисует вызывающий). Возвращает подпись
/// «скачано / всего» для строки под названием.
fn fetch_overlay(ui: &egui::Ui, c: Pos2, r: f32, load: Load) -> Option<String> {
    let Load::Active(done, total) = load else { return None };
    let p = ui.painter();
    p.circle_filled(c, r, Color32::from_black_alpha(190));
    progress_ring(p, c, r - 2.0, load_frac(done, total).max(0.04), Color32::WHITE);
    icons::tg(p, "history_file_cancel", c, Color32::WHITE);
    ui.ctx().request_repaint();
    Some(format!("{} / {}", human_size(done), human_size(total)))
}

/// «м:сс» для длительности.
pub fn mmss(secs: u32) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Подпись типа файла на значке: расширение или «FILE».
fn ext_badge(name: &str) -> String {
    name.rsplit_once('.')
        .map(|(_, e)| e.chars().take(4).collect::<String>().to_uppercase())
        .filter(|e| !e.is_empty())
        .unwrap_or_else(|| "FILE".to_owned())
}

fn play_triangle(p: &egui::Painter, c: Pos2, r: f32, color: Color32) {
    let pts = vec![
        c + Vec2::new(-r * 0.35, -r * 0.55),
        c + Vec2::new(r * 0.6, 0.0),
        c + Vec2::new(-r * 0.35, r * 0.55),
    ];
    p.add(egui::Shape::convex_polygon(pts, color, Stroke::NONE));
}

/// Рисует карточку вложения шириной `w`. Нажатия добавляет в `cards`.
#[allow(clippy::too_many_arguments)]
pub fn card(
    ui: &mut egui::Ui,
    m: &MessageRow,
    info: &MediaInfo,
    w: f32,
    tex: Option<&egui::TextureHandle>,
    text_c: Color32,
    date_c: Color32,
    cards: &mut Vec<(Rect, Act)>,
    load: Load,
) {
    match &info.kind {
        MediaKind::Photo => {
            if let Some(tex) = tex {
                let r = ui.add(egui::Image::new(tex).max_width(w).corner_radius(10)).rect;
                cards.push((r, Act::Viewer(m.id)));
            } else {
                placeholder(ui, w, 150.0, "Фото", date_c, false);
            }
        }
        MediaKind::Sticker | MediaKind::AnimatedSticker => {
            let is_anim = info.kind == MediaKind::AnimatedSticker;
            if let Some(tex) = tex {
                let r = ui.add(egui::Image::new(tex).max_width(w.min(180.0)).corner_radius(6)).rect;
                if is_anim && info.mime == "video/webm" {
                    cards.push((r, Act::Open(m.id)));
                } else {
                    cards.push((r, Act::Viewer(m.id)));
                }
            } else {
                let (rect, _) = ui.allocate_exact_size(Vec2::splat(136.0_f32.min(w)), Sense::hover());
                ui.painter().rect_filled(
                    rect,
                    CornerRadius::same(14),
                    Color32::from_rgba_unmultiplied(255, 255, 255, (14.0 * opacity_f32()) as u8),
                );
                ui.painter().text(
                    rect.center(),
                    Align2::CENTER_CENTER,
                    &info.name,
                    FontId::proportional(14.0),
                    date_c,
                );
                if is_anim && info.mime == "video/webm" {
                    cards.push((rect, Act::Open(m.id)));
                }
            }
        }
        MediaKind::Video | MediaKind::Gif | MediaKind::Round => {
            let round = info.kind == MediaKind::Round;
            let (cw, ch) = if round {
                (w.min(190.0), w.min(190.0))
            } else {
                let aspect = if info.dims.0 > 0 && info.dims.1 > 0 {
                    (info.dims.1 as f32 / info.dims.0 as f32).clamp(0.5, 1.4)
                } else {
                    0.5625
                };
                (w, (w * aspect).max(90.0))
            };
            let (rect, _) = ui.allocate_exact_size(Vec2::new(cw, ch), Sense::hover());
            let cr = if round { CornerRadius::same((cw / 2.0) as u8) } else { CornerRadius::same(10) };
            match tex {
                Some(tex) => egui::Image::new(tex).corner_radius(cr).fit_to_exact_size(rect.size()).paint_at(ui, rect),
                None => {
                    ui.painter().rect_filled(rect, cr, Color32::from_rgb(34, 34, 40));
                }
            }
            let p = ui.painter();
            let c = rect.center();
            p.circle_filled(c, 26.0, Color32::from_black_alpha(150));
            // Как в Telegram: стрелка загрузки → кольцо с крестиком (идёт загрузка) → play (скачано).
            let mut chip_text = String::new();
            match load {
                Load::Active(done, total) => {
                    progress_ring(p, c, 24.0, load_frac(done, total).max(0.04), Color32::WHITE);
                    icons::tg(p, "history_file_cancel", c, Color32::WHITE);
                    chip_text = format!("{} / {}", human_size(done), human_size(total));
                    ui.ctx().request_repaint();
                }
                Load::Remote => {
                    icons::tg(p, "history_file_download", c, Color32::WHITE);
                    chip_text = human_size(info.size);
                }
                Load::Cached => icons::tg(p, "media_video_play", c, Color32::WHITE),
            }
            // Чип со временем или «GIF» в левом верхнем углу.
            let label = if !chip_text.is_empty() {
                match info.duration {
                    0 => chip_text,
                    d => format!("{} · {}", mmss(d), chip_text),
                }
            } else {
                match info.kind {
                    MediaKind::Gif => "GIF".to_owned(),
                    _ if info.duration > 0 => mmss(info.duration),
                    _ => String::new(),
                }
            };
            if !label.is_empty() {
                let g = p.layout_no_wrap(label, FontId::proportional(12.0), Color32::WHITE);
                let chip = Rect::from_min_size(rect.min + Vec2::new(8.0, 8.0), g.size() + Vec2::new(12.0, 4.0));
                p.rect_filled(chip, CornerRadius::same(9), Color32::from_black_alpha(140));
                p.galley(chip.min + Vec2::new(6.0, 2.0), g, Color32::WHITE);
            }
            let act = if matches!(load, Load::Active(..)) { Act::CancelFetch(m.id) } else { Act::Open(m.id) };
            cards.push((rect, act));
        }
        MediaKind::Voice | MediaKind::Audio => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::hover());
            let p = ui.painter();
            let c = Pos2::new(rect.left() + 22.0, rect.center().y);
            p.circle_filled(c, 20.0, WHITE);
            play_triangle(p, c + Vec2::new(1.5, 0.0), 17.0, INK_DARK);
            let progress = fetch_overlay(ui, c, 20.0, load);
            let p = ui.painter();
            let bars_left = rect.left() + 52.0;
            let bars_w = (rect.right() - bars_left - 4.0).max(40.0);
            if info.kind == MediaKind::Voice {
                // Настоящая волна из сообщения; нет волны — ровные столбики.
                let n = ((bars_w / 4.0) as usize).clamp(8, 40);
                for i in 0..n {
                    let v = if info.waveform.is_empty() {
                        8.0
                    } else {
                        f32::from(info.waveform[i * info.waveform.len() / n]) + 3.0
                    };
                    let h = (v / 34.0 * 22.0).clamp(3.0, 22.0);
                    let x = bars_left + i as f32 * bars_w / n as f32;
                    p.line_segment(
                        [Pos2::new(x, rect.center().y - 4.0 - h / 2.0), Pos2::new(x, rect.center().y - 4.0 + h / 2.0)],
                        Stroke::new(2.0, text_c),
                    );
                }
            } else {
                p.text(
                    Pos2::new(bars_left, rect.center().y - 7.0),
                    Align2::LEFT_CENTER,
                    clip_text(&info.name, 24),
                    FontId::proportional(14.0),
                    text_c,
                );
            }
            p.text(
                Pos2::new(bars_left, rect.bottom() - 8.0),
                Align2::LEFT_CENTER,
                progress.unwrap_or_else(|| if info.duration > 0 { mmss(info.duration) } else { human_size(info.size) }),
                FontId::proportional(12.0),
                date_c,
            );
            cards.push((rect, if matches!(load, Load::Active(..)) { Act::CancelFetch(m.id) } else { Act::Open(m.id) }));
        }
        MediaKind::File => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::hover());
            let p = ui.painter();
            let icon = Rect::from_min_size(rect.min + Vec2::new(0.0, 1.0), Vec2::splat(44.0));
            p.rect_filled(icon, CornerRadius::same(10), Color32::from_rgb(70, 70, 78));
            p.text(icon.center(), Align2::CENTER_CENTER, ext_badge(&info.name), FontId::proportional(12.0), WHITE);
            let progress = fetch_overlay(ui, icon.center(), 22.0, load);
            let p = ui.painter();
            p.text(
                Pos2::new(icon.right() + 10.0, rect.center().y - 8.0),
                Align2::LEFT_CENTER,
                clip_text(&info.name, 26),
                FontId::proportional(14.0),
                text_c,
            );
            p.text(
                Pos2::new(icon.right() + 10.0, rect.center().y + 10.0),
                Align2::LEFT_CENTER,
                progress.unwrap_or_else(|| human_size(info.size)),
                FontId::proportional(12.0),
                date_c,
            );
            cards.push((rect, if matches!(load, Load::Active(..)) { Act::CancelFetch(m.id) } else { Act::Save(m.id) }));
        }
        MediaKind::WebPage { url, site, title, desc } => {
            let mut h = 24.0;
            if !title.is_empty() {
                h += 20.0;
            }
            if !desc.is_empty() {
                h += 34.0;
            }
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
            let p = ui.painter();
            p.rect_filled(
                rect,
                CornerRadius::same(8),
                Color32::from_rgba_unmultiplied(0, 0, 0, (42.0 * opacity_f32()) as u8),
            );
            let bar = Rect::from_min_size(rect.min + Vec2::new(6.0, 6.0), Vec2::new(2.5, rect.height() - 12.0));
            p.rect_filled(bar, CornerRadius::same(1), t_btn(ORANGE));
            let left = rect.left() + 15.0;
            let mut y = rect.top() + 6.0;
            p.text(
                Pos2::new(left, y),
                Align2::LEFT_TOP,
                clip_text(if site.is_empty() { url } else { site }, 34),
                FontId::proportional(12.5),
                ORANGE,
            );
            y += 17.0;
            if !title.is_empty() {
                p.text(
                    Pos2::new(left, y),
                    Align2::LEFT_TOP,
                    clip_text(title, 38),
                    FontId::proportional(13.5),
                    text_c,
                );
                y += 18.0;
            }
            if !desc.is_empty() {
                let g = p.layout(clip_text(desc, 85), FontId::proportional(12.5), date_c, (w - 22.0).max(60.0));
                p.galley(Pos2::new(left, y), g, date_c);
            }
            if !url.is_empty() {
                cards.push((rect, Act::OpenUrl(url.clone())));
            }
        }
        MediaKind::Poll { question, options, option_bytes, total, closed, quiz } => {
            let kind_label = match (*quiz, *closed) {
                (true, true) => "Викторина · завершена",
                (true, false) => "Викторина",
                (false, true) => "Опрос · завершён",
                (false, false) => "Опрос",
            };
            ui.label(RichText::new(kind_label).size(11.5).color(ORANGE));
            ui.label(RichText::new(question).strong().size(14.5).color(text_c));
            ui.add_space(4.0);
            for (idx, (opt, pct, chosen)) in options.iter().enumerate() {
                let (r, _) = ui.allocate_exact_size(Vec2::new(w, 28.0), Sense::hover());
                let p = ui.painter();
                p.rect_filled(
                    r,
                    CornerRadius::same(7),
                    Color32::from_rgba_unmultiplied(255, 255, 255, (14.0 * opacity_f32()) as u8),
                );
                let fill_w = (r.width() * (f32::from(*pct) / 100.0)).clamp(4.0, r.width());
                let fill_r = Rect::from_min_size(r.min, Vec2::new(fill_w, r.height()));
                let bar_c = if *chosen {
                    t_btn(ORANGE)
                } else {
                    Color32::from_rgba_unmultiplied(245, 130, 32, (72.0 * opacity_f32()) as u8)
                };
                p.rect_filled(fill_r, CornerRadius::same(7), bar_c);
                p.text(
                    Pos2::new(r.left() + 10.0, r.center().y),
                    Align2::LEFT_CENTER,
                    clip_text(opt, 28),
                    FontId::proportional(13.0),
                    text_c,
                );
                p.text(
                    Pos2::new(r.right() - 10.0, r.center().y),
                    Align2::RIGHT_CENTER,
                    format!("{pct}%"),
                    FontId::proportional(12.0),
                    text_c,
                );
                if !*closed && let Some(bytes) = option_bytes.get(idx) {
                    cards.push((r, Act::VotePoll(m.id, bytes.clone())));
                }
                ui.add_space(2.0);
            }
            ui.label(RichText::new(format!("Голосов: {total}")).size(12.0).color(date_c));
        }
        MediaKind::Other(_) => {
            ui.label(RichText::new(format!("[{}]", info.name)).color(date_c));
        }
    }
}

use eframe::egui::RichText;
use std::collections::HashMap;

fn placeholder(ui: &mut egui::Ui, w: f32, h: f32, label: &str, color: Color32, play: bool) {
    let (rect, _) = ui.allocate_exact_size(Vec2::new(w, h), Sense::hover());
    ui.painter().rect_filled(rect, CornerRadius::same(10), Color32::from_rgb(34, 34, 40));
    ui.painter()
        .text(rect.center(), Align2::CENTER_CENTER, label, FontId::proportional(14.0), color);
    let _ = play;
}

/// Обрезает длинное имя с «…» посередине расширения не затрагивая.
fn clip_text(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_owned();
    }
    let head: String = s.chars().take(max.saturating_sub(1)).collect();
    format!("{head}…")
}

/// Обрезка картинки «по заполнению»: доли текстуры, которые попадают в ячейку нужной формы.
pub fn cover_uv(tex_size: Vec2, cell: Vec2) -> Rect {
    if tex_size.x <= 0.0 || tex_size.y <= 0.0 || cell.x <= 0.0 || cell.y <= 0.0 {
        return Rect::from_min_max(Pos2::ZERO, Pos2::new(1.0, 1.0));
    }
    let (ta, ca) = (tex_size.x / tex_size.y, cell.x / cell.y);
    if ta > ca {
        let w = ca / ta;
        Rect::from_min_max(Pos2::new((1.0 - w) / 2.0, 0.0), Pos2::new((1.0 + w) / 2.0, 1.0))
    } else {
        let h = ta / ca;
        Rect::from_min_max(Pos2::new(0.0, (1.0 - h) / 2.0), Pos2::new(1.0, (1.0 + h) / 2.0))
    }
}

/// Сетка альбома: по два превью в ряд (нечётное последнее — на всю ширину).
pub fn album(
    ui: &mut egui::Ui,
    items: &[&MessageRow],
    w: f32,
    tex: &HashMap<i32, egui::TextureHandle>,
    cards: &mut Vec<(Rect, Act)>,
) {
    let gap = 3.0;
    let mut rest = items;
    while !rest.is_empty() {
        let n = if rest.len() == 1 { 1 } else { 2 };
        let (row, tail) = rest.split_at(n);
        rest = tail;
        let cw = (w - gap * (n as f32 - 1.0)) / n as f32;
        let ch = if n == 1 { cw * 0.62 } else { cw * 0.8 };
        let (rect, _) = ui.allocate_exact_size(Vec2::new(w, ch), Sense::hover());
        for (k, m) in row.iter().enumerate() {
            let cell = Rect::from_min_size(
                Pos2::new(rect.left() + k as f32 * (cw + gap), rect.top()),
                Vec2::new(cw, ch),
            );
            let cr = CornerRadius::same(6);
            match tex.get(&m.id) {
                Some(t) => {
                    egui::Image::new(t)
                        .uv(cover_uv(t.size_vec2(), cell.size()))
                        .corner_radius(cr)
                        .fit_to_exact_size(cell.size())
                        .paint_at(ui, cell);
                }
                None => {
                    ui.painter().rect_filled(cell, cr, Color32::from_rgb(34, 34, 40));
                }
            }
            let is_video = m
                .media
                .as_ref()
                .is_some_and(|i| matches!(i.kind, MediaKind::Video | MediaKind::Gif | MediaKind::Round));
            if is_video {
                let p = ui.painter();
                p.circle_filled(cell.center(), 18.0, Color32::from_black_alpha(150));
                play_triangle(p, cell.center() + Vec2::new(1.5, 0.0), 16.0, Color32::WHITE);
            }
            cards.push((cell, if is_video { Act::Open(m.id) } else { Act::Viewer(m.id) }));
        }
        ui.add_space(gap);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cover_crops_the_longer_side() {
        // Широкая картинка в квадратной ячейке: обрезаются бока.
        let uv = cover_uv(Vec2::new(200.0, 100.0), Vec2::new(50.0, 50.0));
        assert!((uv.width() - 0.5).abs() < 1e-5 && (uv.height() - 1.0).abs() < 1e-5);
        // Высокая — обрезаются верх и низ.
        let uv = cover_uv(Vec2::new(100.0, 200.0), Vec2::new(50.0, 50.0));
        assert!((uv.height() - 0.5).abs() < 1e-5 && (uv.width() - 1.0).abs() < 1e-5);
        // Вырожденные размеры не ломают расчёт.
        assert_eq!(cover_uv(Vec2::ZERO, Vec2::ZERO).width(), 1.0);
    }

    #[test]
    fn duration_format() {
        assert_eq!(mmss(7), "0:07");
        assert_eq!(mmss(125), "2:05");
    }

    #[test]
    fn load_fraction_is_clamped() {
        assert_eq!(load_frac(0, 0), 0.0);
        assert_eq!(load_frac(50, 100), 0.5);
        assert_eq!(load_frac(300, 100), 1.0);
    }

    #[test]
    fn extension_badge() {
        assert_eq!(ext_badge("report.pdf"), "PDF");
        assert_eq!(ext_badge("archive.tar.gz"), "GZ");
        assert_eq!(ext_badge("noext"), "FILE");
        assert_eq!(ext_badge("a.verylongext"), "VERY");
    }

    #[test]
    fn long_names_are_clipped() {
        assert_eq!(clip_text("короткое", 20), "короткое");
        assert_eq!(clip_text("очень_длинное_имя_файла.txt", 10).chars().count(), 10);
    }
}
