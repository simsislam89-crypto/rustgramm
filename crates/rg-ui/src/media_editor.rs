//! Встроенный редактор фото и видео перед отправкой:
//! обрезка (crop), поворот 90°/отражение, кисть рисования и размытие/пикселизация чувствительных данных,
//! выбор сжатия, спойлер и таймер самоуничтожения (`ttl_seconds`).

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use image::{DynamicImage, GenericImageView, Rgba, RgbaImage};
use rg_core::engine::SendMediaOptions;

use super::theme::*;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum EditorTool {
    Crop,
    Draw,
    Blur,
}

#[derive(Debug, Clone)]
pub(crate) struct EditorStroke {
    /// Нормализованные координаты `(0.0..=1.0)` относительно уже обрезанного/повёрнутого превью.
    pub points: Vec<[f32; 2]>,
    pub is_blur: bool,
    pub color: [u8; 4],
    pub radius_norm: f32,
}

pub(crate) struct MediaEditorState {
    pub source_path: PathBuf,
    pub is_video: bool,
    pub orig_img: Option<RgbaImage>,
    pub preview_tex: Option<egui::TextureHandle>,
    pub dirty: bool,
    pub rotation: u8, // 0, 1 (90° CW), 2 (180°), 3 (270° CW)
    pub flip_h: bool,
    /// Нормализованный прямоугольник кропа `[left, top, right, bottom]` в `0.0..=1.0`.
    pub crop: [f32; 4],
    pub tool: EditorTool,
    pub brush_color: Color32,
    pub brush_size: f32,
    pub strokes: Vec<EditorStroke>,
    pub active_stroke: Option<EditorStroke>,
    pub compress: bool,
    pub hd_quality: bool,
    pub spoiler: bool,
    pub ttl_seconds: Option<i32>,
    pub caption: String,
    pub reply_to: Option<i32>,
    pub file_size: u64,
    pub status_msg: String,
}

impl MediaEditorState {
    pub fn open(path: PathBuf, caption: String, reply_to: Option<i32>) -> Self {
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let is_video = matches!(ext.as_str(), "mp4" | "mov" | "webm" | "mkv" | "m4v" | "avi");
        let file_size = std::fs::metadata(&path).map_or(0, |m| m.len());
        let orig_img = if !is_video && matches!(ext.as_str(), "jpg" | "jpeg" | "png" | "webp" | "bmp") {
            std::fs::read(&path)
                .ok()
                .and_then(|b| image::load_from_memory(&b).ok())
                .map(|img| {
                    let (w, h) = img.dimensions();
                    if w > 1920 || h > 1920 {
                        img.thumbnail(1920, 1920).to_rgba8()
                    } else {
                        img.to_rgba8()
                    }
                })
        } else {
            None
        };
        Self {
            source_path: path,
            is_video,
            orig_img,
            preview_tex: None,
            dirty: true,
            rotation: 0,
            flip_h: false,
            crop: [0.0, 0.0, 1.0, 1.0],
            tool: EditorTool::Crop,
            brush_color: ORANGE,
            brush_size: 0.022,
            strokes: Vec::new(),
            active_stroke: None,
            compress: true,
            hd_quality: false,
            spoiler: false,
            ttl_seconds: None,
            caption,
            reply_to,
            file_size,
            status_msg: String::new(),
        }
    }

    pub fn has_visual_edits(&self) -> bool {
        self.rotation != 0
            || self.flip_h
            || (self.crop[0] > 0.002
                || self.crop[1] > 0.002
                || self.crop[2] < 0.998
                || self.crop[3] < 0.998)
            || !self.strokes.is_empty()
    }
}

/// Применяет поворот, отражение, обрезку, размытие и штрихи к исходному изображению.
pub fn render_edited_rgba(
    src: &RgbaImage,
    rotation: u8,
    flip_h: bool,
    crop: [f32; 4],
    strokes: &[EditorStroke],
) -> RgbaImage {
    let mut dyn_img = DynamicImage::ImageRgba8(src.clone());
    match rotation % 4 {
        1 => dyn_img = dyn_img.rotate90(),
        2 => dyn_img = dyn_img.rotate180(),
        3 => dyn_img = dyn_img.rotate270(),
        _ => {}
    }
    if flip_h {
        dyn_img = dyn_img.fliph();
    }
    let full = dyn_img.to_rgba8();
    let (fw, fh) = (full.width(), full.height());
    let l = ((crop[0].clamp(0.0, 0.95) * fw as f32).round() as u32).min(fw.saturating_sub(2));
    let t = ((crop[1].clamp(0.0, 0.95) * fh as f32).round() as u32).min(fh.saturating_sub(2));
    let r = ((crop[2].clamp(crop[0] + 0.05, 1.0) * fw as f32).round() as u32).clamp(l + 1, fw);
    let b = ((crop[3].clamp(crop[1] + 0.05, 1.0) * fh as f32).round() as u32).clamp(t + 1, fh);

    let mut out = image::imageops::crop_imm(&full, l, t, r - l, b - t).to_image();
    let (ow, oh) = (out.width() as i32, out.height() as i32);
    let base_dim = ow.min(oh).max(16) as f32;

    for st in strokes {
        let rad = (st.radius_norm * base_dim).round().max(3.0) as i32;
        if st.is_blur {
            // Пикселизация/размытие блоков чувствительных данных вдоль штриха.
            let block = (rad / 2).clamp(6, 22);
            let mut visited = std::collections::HashSet::<(i32, i32)>::new();
            for pt in interpolated_points(&st.points, (rad / 2).max(2) as f32 / base_dim) {
                let cx = (pt[0] * ow as f32).round() as i32;
                let cy = (pt[1] * oh as f32).round() as i32;
                let bx0 = ((cx - rad).max(0) / block) * block;
                let by0 = ((cy - rad).max(0) / block) * block;
                let bx1 = ((cx + rad).min(ow - 1) / block) * block;
                let by1 = ((cy + rad).min(oh - 1) / block) * block;
                for by in (by0..=by1).step_by(block as usize) {
                    for bx in (bx0..=bx1).step_by(block as usize) {
                        if !visited.insert((bx, by)) {
                            continue;
                        }
                        let mut sum = [0u64; 4];
                        let mut cnt = 0u64;
                        for py in by..(by + block).min(oh) {
                            for px in bx..(bx + block).min(ow) {
                                let p = out.get_pixel(px as u32, py as u32).0;
                                for k in 0..4 {
                                    sum[k] += u64::from(p[k]);
                                }
                                cnt += 1;
                            }
                        }
                        if cnt > 0 {
                            let avg = Rgba(sum.map(|v| (v.checked_div(cnt).unwrap_or(0)) as u8));
                            for py in by..(by + block).min(oh) {
                                for px in bx..(bx + block).min(ow) {
                                    out.put_pixel(px as u32, py as u32, avg);
                                }
                            }
                        }
                    }
                }
            }
        } else {
            let col = Rgba(st.color);
            for pt in interpolated_points(&st.points, 0.005) {
                let cx = (pt[0] * ow as f32).round() as i32;
                let cy = (pt[1] * oh as f32).round() as i32;
                let r2 = rad * rad;
                for py in (cy - rad).max(0)..=(cy + rad).min(oh - 1) {
                    for px in (cx - rad).max(0)..=(cx + rad).min(ow - 1) {
                        if (px - cx) * (px - cx) + (py - cy) * (py - cy) <= r2 {
                            out.put_pixel(px as u32, py as u32, col);
                        }
                    }
                }
            }
        }
    }
    out
}

fn interpolated_points(pts: &[[f32; 2]], step: f32) -> Vec<[f32; 2]> {
    if pts.len() <= 1 {
        return pts.to_vec();
    }
    let mut out = Vec::with_capacity(pts.len() * 4);
    for w in pts.windows(2) {
        let dx = w[1][0] - w[0][0];
        let dy = w[1][1] - w[0][1];
        let dist = (dx * dx + dy * dy).sqrt();
        let steps = (dist / step.max(0.003)).ceil().clamp(1.0, 80.0) as usize;
        for i in 0..=steps {
            let f = i as f32 / steps as f32;
            out.push([w[0][0] + dx * f, w[0][1] + dy * f]);
        }
    }
    out
}

/// Сохраняет отредактированное изображение во временный файл кэша перед отправкой.
pub fn export_edited_image(state: &MediaEditorState, cache_dir: &Path) -> Result<PathBuf, String> {
    if !state.has_visual_edits() && (!state.compress || !state.hd_quality) {
        return Ok(state.source_path.clone());
    }
    let Some(src) = state.orig_img.as_ref() else {
        return Ok(state.source_path.clone());
    };
    let mut edited = render_edited_rgba(src, state.rotation, state.flip_h, state.crop, &state.strokes);
    if state.compress {
        let max_side = if state.hd_quality { 2560 } else { 1280 };
        if edited.width() > max_side || edited.height() > max_side {
            edited = DynamicImage::ImageRgba8(edited)
                .thumbnail(max_side, max_side)
                .to_rgba8();
        }
    }
    let dir = cache_dir.join("edited");
    let _ = std::fs::create_dir_all(&dir);
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis());
    let ext = if state.compress { "jpg" } else { "png" };
    let out_path = dir.join(format!("rg_edit_{ts}.{ext}"));
    if state.compress {
        let rgb = DynamicImage::ImageRgba8(edited).to_rgb8();
        rgb.save_with_format(&out_path, image::ImageFormat::Jpeg)
            .map_err(|e| format!("Сохранение JPEG: {e}"))?;
    } else {
        edited
            .save_with_format(&out_path, image::ImageFormat::Png)
            .map_err(|e| format!("Сохранение PNG: {e}"))?;
    }
    Ok(out_path)
}

/// Возвращает `Some((path, caption, reply_to, SendMediaOptions))` если пользователь нажал «Отправить»,
/// либо `None`, если редактор ещё открыт или закрыт.
#[allow(deprecated)]
pub(crate) fn media_editor_window(
    ctx: &egui::Context,
    state_opt: &mut Option<MediaEditorState>,
    cache_dir: &Path,
) -> Option<(PathBuf, String, Option<i32>, SendMediaOptions)> {
    let state = state_opt.as_mut()?;
    let mut send_now = false;
    let mut cancel = ctx.input(|i| i.key_pressed(egui::Key::Escape));

    let screen = ctx.content_rect();
    let card_w = (screen.width() - 40.0).clamp(320.0, 680.0);
    let card_h = (screen.height() - 40.0).clamp(400.0, 700.0);

    egui::Area::new(egui::Id::new("media-editor-overlay"))
        .order(egui::Order::Foreground)
        .fixed_pos(screen.min)
        .show(ctx, |ui| {
            let (bg_r, bg_resp) = ui.allocate_exact_size(screen.size(), Sense::click());
            paint_frosted_scrim(ui.painter(), bg_r, 1.0);
            let card_rect = Rect::from_center_size(screen.center(), Vec2::new(card_w, card_h));
            ui.painter().rect_filled(card_rect, CornerRadius::same(14), t2(WINDOW_BG));
            paint_modal_subblur(ui.painter(), card_rect, 14.0, 1.0);

            if bg_resp.clicked() && !ctx.input(|i| i.pointer.hover_pos()).is_some_and(|p| card_rect.contains(p)) {
                cancel = true;
            }

            let inner = card_rect.shrink(14.0);
            ui.scope_builder(egui::UiBuilder::new().max_rect(inner), |ui| {
                ui.horizontal(|ui| {
                    ui.label(
                        RichText::new(if state.is_video {
                            "Отправка видео"
                        } else if state.orig_img.is_some() {
                            "Редактор медиа"
                        } else {
                            "Отправка файла"
                        })
                        .size(16.0)
                        .strong()
                        .color(WHITE),
                    );
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.button("✕").clicked() {
                            cancel = true;
                        }
                    });
                });
                ui.add_space(6.0);

                // Область превью изображения или карточки видео/файла.
                let preview_h = (card_h - 245.0).max(160.0);
                let (prev_r, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), preview_h), Sense::hover());
                ui.painter().rect_filled(prev_r, CornerRadius::same(10), t(SIDEBAR_BG));

                if let Some(orig) = state.orig_img.as_ref() {
                    if state.dirty || state.preview_tex.is_none() {
                        // Превью без кропа для инструмента Crop, или уже отредактированное для Draw/Blur.
                        let disp = if state.tool == EditorTool::Crop {
                            render_edited_rgba(orig, state.rotation, state.flip_h, [0.0, 0.0, 1.0, 1.0], &state.strokes)
                        } else {
                            render_edited_rgba(orig, state.rotation, state.flip_h, state.crop, &state.strokes)
                        };
                        let img = egui::ColorImage::from_rgba_unmultiplied(
                            [disp.width() as usize, disp.height() as usize],
                            disp.as_raw(),
                        );
                        state.preview_tex = Some(ctx.load_texture("media-editor-prev", img, egui::TextureOptions::LINEAR));
                        state.dirty = false;
                    }
                    if let Some(tex) = state.preview_tex.as_ref() {
                        let ts = tex.size_vec2();
                        let scale = (prev_r.width() / ts.x.max(1.0)).min(prev_r.height() / ts.y.max(1.0)) * 0.94;
                        let draw_sz = ts * scale;
                        let img_rect = Rect::from_center_size(prev_r.center(), draw_sz);
                        egui::Image::new(tex).fit_to_exact_size(draw_sz).paint_at(ui, img_rect);

                        if state.spoiler {
                            ui.painter().rect_filled(
                                img_rect,
                                CornerRadius::same(6),
                                Color32::from_rgba_unmultiplied(18, 18, 22, 135),
                            );
                            ui.painter().text(
                                img_rect.center(),
                                Align2::CENTER_CENTER,
                                "Спойлер (скрыто до нажатия)",
                                FontId::proportional(13.0),
                                WHITE,
                            );
                        }

                        // Интерактивный ввод по изображению: обрезка или рисование/размытие.
                        let resp = ui.interact(img_rect, ui.make_persistent_id("editor-canvas"), Sense::drag());
                        match state.tool {
                            EditorTool::Crop => {
                                let cr = Rect::from_min_max(
                                    Pos2::new(
                                        img_rect.left() + state.crop[0] * img_rect.width(),
                                        img_rect.top() + state.crop[1] * img_rect.height(),
                                    ),
                                    Pos2::new(
                                        img_rect.left() + state.crop[2] * img_rect.width(),
                                        img_rect.top() + state.crop[3] * img_rect.height(),
                                    ),
                                );
                                ui.painter().rect_stroke(
                                    cr,
                                    CornerRadius::ZERO,
                                    Stroke::new(2.0, ORANGE),
                                    egui::StrokeKind::Inside,
                                );
                                if resp.dragged() && let Some(pos) = resp.interact_pointer_pos() {
                                    let nx = ((pos.x - img_rect.left()) / img_rect.width()).clamp(0.0, 1.0);
                                    let ny = ((pos.y - img_rect.top()) / img_rect.height()).clamp(0.0, 1.0);
                                    // Сдвигаем ближайший край рамки обрезки.
                                    if (nx - state.crop[0]).abs() < (nx - state.crop[2]).abs() {
                                        state.crop[0] = nx.min(state.crop[2] - 0.12);
                                    } else {
                                        state.crop[2] = nx.max(state.crop[0] + 0.12);
                                    }
                                    if (ny - state.crop[1]).abs() < (ny - state.crop[3]).abs() {
                                        state.crop[1] = ny.min(state.crop[3] - 0.12);
                                    } else {
                                        state.crop[3] = ny.max(state.crop[1] + 0.12);
                                    }
                                }
                            }
                            EditorTool::Draw | EditorTool::Blur => {
                                let is_blur = state.tool == EditorTool::Blur;
                                if resp.dragged() && let Some(pos) = resp.interact_pointer_pos() {
                                    let nx = ((pos.x - img_rect.left()) / img_rect.width()).clamp(0.0, 1.0);
                                    let ny = ((pos.y - img_rect.top()) / img_rect.height()).clamp(0.0, 1.0);
                                    let st = state.active_stroke.get_or_insert_with(|| EditorStroke {
                                        points: Vec::new(),
                                        is_blur,
                                        color: [
                                            state.brush_color.r(),
                                            state.brush_color.g(),
                                            state.brush_color.b(),
                                            255,
                                        ],
                                        radius_norm: state.brush_size,
                                    });
                                    st.points.push([nx, ny]);
                                }
                                if resp.drag_stopped()
                                    && let Some(st) = state.active_stroke.take()
                                    && !st.points.is_empty()
                                {
                                    state.strokes.push(st);
                                    state.dirty = true;
                                }
                                if let Some(st) = &state.active_stroke {
                                    let col = if st.is_blur {
                                        Color32::from_white_alpha(150)
                                    } else {
                                        state.brush_color
                                    };
                                    let pts: Vec<Pos2> = st
                                        .points
                                        .iter()
                                        .map(|p| {
                                            Pos2::new(
                                                img_rect.left() + p[0] * img_rect.width(),
                                                img_rect.top() + p[1] * img_rect.height(),
                                            )
                                        })
                                        .collect();
                                    if pts.len() >= 2 {
                                        ui.painter().add(egui::Shape::line(pts, Stroke::new(8.0, col)));
                                    }
                                }
                            }
                        }
                    }
                } else {
                    let fname = state
                        .source_path
                        .file_name()
                        .map(|n| n.to_string_lossy().into_owned())
                        .unwrap_or_else(|| "Файл".to_owned());
                    ui.painter().text(
                        prev_r.center() - Vec2::new(0.0, 12.0),
                        Align2::CENTER_CENTER,
                        fname,
                        FontId::proportional(15.0),
                        WHITE,
                    );
                    ui.painter().text(
                        prev_r.center() + Vec2::new(0.0, 12.0),
                        Align2::CENTER_CENTER,
                        rg_core::media::human_size(state.file_size),
                        FontId::proportional(12.5),
                        SUBTEXT,
                    );
                }

                ui.add_space(6.0);
                // Панель инструментов (для картинок).
                if state.orig_img.is_some() {
                    ui.horizontal_wrapped(|ui| {
                        for (t_mode, lbl) in [
                            (EditorTool::Crop, "Обрезка"),
                            (EditorTool::Draw, "Рисовать"),
                            (EditorTool::Blur, "Размытие"),
                        ] {
                            if ui.selectable_label(state.tool == t_mode, lbl).clicked() {
                                state.tool = t_mode;
                                state.dirty = true;
                            }
                        }
                        if ui.button("↻ 90°").clicked() {
                            state.rotation = (state.rotation + 1) % 4;
                            state.dirty = true;
                        }
                        if ui.button("⇄ Отразить").clicked() {
                            state.flip_h = !state.flip_h;
                            state.dirty = true;
                        }
                        if !state.strokes.is_empty() && ui.button("Отменить штрих").clicked() {
                            state.strokes.pop();
                            state.dirty = true;
                        }
                        if state.tool == EditorTool::Crop {
                            if ui.button("1:1").clicked() {
                                state.crop = [0.125, 0.125, 0.875, 0.875];
                            }
                            if ui.button("Сброс").clicked() {
                                state.crop = [0.0, 0.0, 1.0, 1.0];
                            }
                        }
                        if state.tool == EditorTool::Draw {
                            for col in [ORANGE, Color32::WHITE, Color32::BLACK, DANGER, GREEN] {
                                let (r, resp) = ui.allocate_exact_size(Vec2::splat(18.0), Sense::click());
                                ui.painter().circle_filled(r.center(), 7.5, col);
                                if state.brush_color == col {
                                    ui.painter().circle_stroke(r.center(), 9.0, Stroke::new(1.5, WHITE));
                                }
                                if resp.clicked() {
                                    state.brush_color = col;
                                }
                            }
                        }
                    });
                    ui.add_space(4.0);
                }

                // Опции сжатия, спойлера и таймера самоуничтожения.
                ui.horizontal_wrapped(|ui| {
                    if state.orig_img.is_some() {
                        ui.checkbox(&mut state.compress, "Сжать фото");
                        if state.compress {
                            ui.checkbox(&mut state.hd_quality, "HD (2560px)");
                        }
                    }
                    ui.checkbox(&mut state.spoiler, "Спойлер");
                    ui.label(RichText::new("Таймер:").size(12.0).color(SUBTEXT));
                    for (sec, lbl) in [
                        (None, "Выкл"),
                        (Some(3), "3с"),
                        (Some(5), "5с"),
                        (Some(10), "10с"),
                        (Some(30), "30с"),
                        (Some(60), "1м"),
                    ] {
                        if ui.selectable_label(state.ttl_seconds == sec, lbl).clicked() {
                            state.ttl_seconds = sec;
                        }
                    }
                });
                ui.add_space(6.0);
                ui.horizontal(|ui| {
                    let cap_w = (ui.available_width() - 140.0).max(140.0);
                    ui.add_sized(
                        [cap_w, 34.0],
                        egui::TextEdit::singleline(&mut state.caption)
                            .hint_text("Добавить подпись…"),
                    );
                    if orange_button(ui, "Отправить", Vec2::new(128.0, 34.0)).clicked() {
                        send_now = true;
                    }
                });
                if !state.status_msg.is_empty() {
                    ui.label(RichText::new(&state.status_msg).size(12.0).color(DANGER));
                }
            });
        });

    if cancel {
        *state_opt = None;
        return None;
    }
    if send_now {
        match export_edited_image(state, cache_dir) {
            Ok(out_path) => {
                let res = (
                    out_path,
                    state.caption.trim().to_owned(),
                    state.reply_to,
                    SendMediaOptions {
                        compress: state.compress,
                        spoiler: state.spoiler,
                        ttl_seconds: state.ttl_seconds,
                    },
                );
                *state_opt = None;
                return Some(res);
            }
            Err(e) => {
                state.status_msg = e;
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn crop_rotate_and_blur_modify_pixels_correctly() {
        let mut img = RgbaImage::new(40, 20);
        for y in 0..20 {
            for x in 0..40 {
                let c = if x < 20 { [255, 0, 0, 255] } else { [0, 0, 255, 255] };
                img.put_pixel(x, y, Rgba(c));
            }
        }
        let rot = render_edited_rgba(&img, 1, false, [0.0, 0.0, 1.0, 1.0], &[]);
        assert_eq!((rot.width(), rot.height()), (20, 40));

        let cropped = render_edited_rgba(&img, 0, false, [0.5, 0.0, 1.0, 1.0], &[]);
        assert_eq!(cropped.width(), 20);
        assert_eq!(cropped.get_pixel(5, 5).0, [0, 0, 255, 255]);

        let blur_stroke = EditorStroke {
            points: vec![[0.5, 0.5], [0.52, 0.5]],
            is_blur: true,
            color: [0, 0, 0, 255],
            radius_norm: 0.25,
        };
        let blurred = render_edited_rgba(&img, 0, false, [0.0, 0.0, 1.0, 1.0], &[blur_stroke]);
        // Граница красного/синего после размытия даёт смешанный фиолетовый цвет.
        let p = blurred.get_pixel(20, 10).0;
        assert!(p[0] > 0 && p[2] > 0);
    }
}
