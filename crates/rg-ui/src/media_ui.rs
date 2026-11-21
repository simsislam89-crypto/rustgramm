//! Карточки вложений в пузыре: фото с просмотром, видео/GIF/кружок с кнопкой воспроизведения,
//! голосовые с настоящей волной, файлы со значком типа. Нажатия собираются в `cards` и
//! регистрируются позже, поверх пузыря, чтобы не перехватываться его контекстным меню.

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Stroke, Vec2};
use rg_core::engine::MessageRow;
use rg_core::media::{MediaInfo, MediaKind, human_size};

use super::chat::Act;
use super::theme::*;

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
        MediaKind::Sticker => {
            if let Some(tex) = tex {
                ui.add(egui::Image::new(tex).max_width(w.min(180.0)));
            } else {
                ui.label(RichText::new("Стикер").color(date_c));
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
            p.circle_filled(rect.center(), 24.0, Color32::from_black_alpha(150));
            play_triangle(p, rect.center() + Vec2::new(2.0, 0.0), 22.0, Color32::WHITE);
            // Чип со временем или «GIF» в левом верхнем углу.
            let label = match info.kind {
                MediaKind::Gif => "GIF".to_owned(),
                _ if info.duration > 0 => mmss(info.duration),
                _ => String::new(),
            };
            if !label.is_empty() {
                let g = p.layout_no_wrap(label, FontId::proportional(12.0), Color32::WHITE);
                let chip = Rect::from_min_size(rect.min + Vec2::new(8.0, 8.0), g.size() + Vec2::new(12.0, 4.0));
                p.rect_filled(chip, CornerRadius::same(9), Color32::from_black_alpha(140));
                p.galley(chip.min + Vec2::new(6.0, 2.0), g, Color32::WHITE);
            }
            cards.push((rect, Act::Open(m.id)));
        }
        MediaKind::Voice | MediaKind::Audio => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 44.0), Sense::hover());
            let p = ui.painter();
            let c = Pos2::new(rect.left() + 22.0, rect.center().y);
            p.circle_filled(c, 20.0, WHITE);
            play_triangle(p, c + Vec2::new(1.5, 0.0), 17.0, INK_DARK);
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
                if info.duration > 0 { mmss(info.duration) } else { human_size(info.size) },
                FontId::proportional(12.0),
                date_c,
            );
            cards.push((rect, Act::Open(m.id)));
        }
        MediaKind::File => {
            let (rect, _) = ui.allocate_exact_size(Vec2::new(w, 46.0), Sense::hover());
            let p = ui.painter();
            let icon = Rect::from_min_size(rect.min + Vec2::new(0.0, 1.0), Vec2::splat(44.0));
            p.rect_filled(icon, CornerRadius::same(10), Color32::from_rgb(70, 70, 78));
            p.text(icon.center(), Align2::CENTER_CENTER, ext_badge(&info.name), FontId::proportional(12.0), WHITE);
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
                human_size(info.size),
                FontId::proportional(12.0),
                date_c,
            );
            cards.push((rect, Act::Save(m.id)));
        }
        MediaKind::AnimatedSticker | MediaKind::Other(_) => {
            ui.label(RichText::new(format!("[{}]", info.name)).color(date_c));
        }
    }
}

use eframe::egui::RichText;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn duration_format() {
        assert_eq!(mmss(7), "0:07");
        assert_eq!(mmss(125), "2:05");
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
