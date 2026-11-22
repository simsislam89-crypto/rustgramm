//! Интерактивные окна новых функций этапа 28:
//! - Плашка автообновления при выходе нового релиза на GitHub;
//! - Публикация и расширенный просмотр историй (с загрузкой медиа, реакциями и прогрессом);
//! - Голосовые и видеозвонки (MTProto E2E сигналинг, 4 проверочных эмодзи, история звонков);
//! - Секретные чаты E2E (8×8 визуальный ключ, таймер самоуничтожения, шифрованная переписка);
//! - Администрирование групп и каналов (админы, баны/муты, общие права, журнал действий, ссылки).

use eframe::egui::{self, Align2, Color32, CornerRadius, FontId, Pos2, Rect, RichText, Sense, Stroke, Vec2};
use rg_core::calls::CallState;
use rg_core::engine::{AdminRights, BannedRights, StoryPrivacy};

use super::{App, Cmd, audio_io, theme::*};

/// Активное состояние звонка в интерфейсе.
#[derive(Debug, Clone)]
pub(crate) struct ActiveCallUi {
    pub call_id: i64,
    pub access_hash: i64,
    pub peer_name: String,
    pub peer_id: i64,
    pub video: bool,
    pub outgoing: bool,
    pub state: CallState,
    pub connected_at: Option<std::time::Instant>,
    pub muted: bool,
    pub speaker: bool,
    pub camera_on: bool,
    pub emojis: Option<[&'static str; 4]>,
    pub relays: usize,
}

/// Состояние окна администрирования группы/канала.
#[derive(Debug, Clone)]
pub(crate) struct AdminModalState {
    pub peer: grammers_session::types::PeerRef,
    pub chat_id: i64,
    pub title: String,
    pub tab: u8, // 0: Участники, 1: Права группы, 2: Журнал, 3: Ссылки
    pub selected_user: Option<(grammers_session::types::PeerRef, i64, String)>,
    pub admin_rights: AdminRights,
    pub admin_rank: String,
    pub default_rights: BannedRights,
    pub mute_preset: u8, // 0: 10м, 1: 1ч, 2: 24ч, 3: навсегда
    pub log_query: String,
    pub invite_title: String,
    pub invite_limit: String,
    pub invite_request: bool,
}

impl App {
    // =========================================================================
    // 1. Плашка автообновления при новом релизе на GitHub
    // =========================================================================

    pub(crate) fn update_banner_ui(&mut self, ui: &mut egui::Ui) {
        let Some(info) = self.update_available.clone() else {
            return;
        };
        if self.update_dismissed {
            return;
        }
        let k = tween(ui.ctx(), "update-banner", 1.0, 0.22);
        let h = 42.0 * k;
        if h < 4.0 {
            return;
        }
        let (rect, _) = ui.allocate_exact_size(Vec2::new(ui.available_width(), h), Sense::hover());
        let p = ui.painter();
        p.rect_filled(rect, 0.0, Color32::from_rgba_unmultiplied(245, 130, 32, (36.0 * k) as u8));
        p.line_segment(
            [Pos2::new(rect.left(), rect.bottom()), Pos2::new(rect.right(), rect.bottom())],
            Stroke::new(1.0, ORANGE.gamma_multiply(0.45 * k)),
        );

        let btn_close = Rect::from_center_size(Pos2::new(rect.right() - 20.0, rect.center().y), Vec2::splat(24.0));
        let btn_upd = Rect::from_center_size(Pos2::new(rect.right() - 104.0, rect.center().y), Vec2::new(124.0, 26.0));
        let text_clip = Rect::from_min_max(
            Pos2::new(rect.left() + 14.0, rect.top()),
            Pos2::new((btn_upd.left() - 10.0).max(rect.left() + 60.0), rect.bottom()),
        );
        let label = if let Some((done, total)) = self.update_progress {
            let pct = (done.saturating_mul(100)).checked_div(total).unwrap_or(0).min(100);
            format!("Скачивание Rustgramm v{}: {pct}%", info.version)
        } else if !info.notes.is_empty() {
            format!("Вышло обновление Rustgramm v{} — {}", info.version, info.notes)
        } else {
            format!("Доступно обновление Rustgramm v{}", info.version)
        };
        p.with_clip_rect(text_clip).text(
            Pos2::new(rect.left() + 14.0, rect.center().y),
            Align2::LEFT_CENTER,
            label,
            FontId::proportional(13.0),
            WHITE,
        );

        if let Some((done, total)) = self.update_progress {
            let frac = if total > 0 { (done as f32 / total as f32).clamp(0.0, 1.0) } else { 0.0 };
            let bar = Rect::from_min_size(Pos2::new(rect.left(), rect.bottom() - 2.5), Vec2::new(rect.width() * frac, 2.5));
            p.rect_filled(bar, 0.0, ORANGE);
        }

        let ur = ui.interact(btn_upd, ui.make_persistent_id("upd-apply-btn"), Sense::click());
        let uh = tween(ui.ctx(), "upd-apply-h", f32::from(u8::from(ur.hovered())), 0.12);
        let bg = lerp_color(t_btn(ORANGE), ORANGE, uh);
        p.rect_filled(btn_upd, CornerRadius::same(7), bg);
        p.text(
            btn_upd.center(),
            Align2::CENTER_CENTER,
            if self.update_progress.is_some() { "Загрузка…" } else { "Обновить" },
            FontId::proportional(12.5),
            Color32::WHITE,
        );
        if self.update_progress.is_none()
            && ur.on_hover_cursor(egui::CursorIcon::PointingHand).clicked()
        {
            self.update_progress = Some((0, info.size.max(1)));
            self.send(Cmd::ApplyUpdate(info));
        }

        let xr = ui.interact(btn_close, ui.make_persistent_id("upd-close-btn"), Sense::click());
        p.text(
            btn_close.center(),
            Align2::CENTER_CENTER,
            "✕",
            FontId::proportional(12.0),
            if xr.hovered() { WHITE } else { SUBTEXT },
        );
        if xr.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
            self.update_dismissed = true;
        }
    }

    // =========================================================================
    // 2. Публикация своей истории (`stories.sendStory`)
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn story_composer_window(&mut self, ctx: &egui::Context) {
        if !self.story_composer_open {
            return;
        }
        let mut open = true;
        let mut cancel = false;
        let mut publish = false;
        egui::Window::new("Новая история")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(390.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                ui.label(RichText::new("Фото или видео для публикации в Telegram Stories").size(12.5).color(SUBTEXT));
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    #[cfg(not(target_os = "android"))]
                    if orange_button(ui, "Выбрать файл…", Vec2::new(145.0, 32.0)).clicked()
                        && let Some(p) = rfd::FileDialog::new()
                            .add_filter("Медиа", &["jpg", "jpeg", "png", "webp", "mp4", "mov", "webm"])
                            .pick_file()
                    {
                        self.story_file_path = p.to_string_lossy().into_owned();
                    }
                    let shown = if self.story_file_path.is_empty() {
                        "Файл не выбран".to_owned()
                    } else {
                        std::path::Path::new(&self.story_file_path)
                            .file_name()
                            .map(|n| n.to_string_lossy().into_owned())
                            .unwrap_or_else(|| self.story_file_path.clone())
                    };
                    ui.label(RichText::new(shown).size(12.5).color(WHITE));
                });
                #[cfg(target_os = "android")]
                {
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.story_file_path)
                            .hint_text("Путь к фото/видео (/sdcard/DCIM/...)")
                            .desired_width(360.0),
                    );
                }
                ui.add_space(8.0);
                ui.label(RichText::new("Подпись (необязательно)").size(12.5).color(WHITE));
                ui.add(
                    egui::TextEdit::multiline(&mut self.story_caption)
                        .desired_rows(2)
                        .desired_width(360.0)
                        .hint_text("Добавьте текст к истории…"),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("Кто увидит историю").size(12.5).color(WHITE));
                ui.horizontal_wrapped(|ui| {
                    for (val, lbl) in [
                        (StoryPrivacy::Everyone, "Все"),
                        (StoryPrivacy::Contacts, "Контакты"),
                        (StoryPrivacy::CloseFriends, "Близкие"),
                        (StoryPrivacy::OnlyMe, "Только я"),
                    ] {
                        if ui.selectable_label(self.story_privacy == val, lbl).clicked() {
                            self.story_privacy = val;
                        }
                    }
                });
                ui.add_space(6.0);
                ui.label(RichText::new("Срок жизни").size(12.5).color(WHITE));
                ui.horizontal(|ui| {
                    for (secs, lbl) in [(21_600, "6 ч"), (43_200, "12 ч"), (86_400, "24 ч"), (172_800, "48 ч")] {
                        if ui.selectable_label(self.story_period == secs, lbl).clicked() {
                            self.story_period = secs;
                        }
                    }
                });
                ui.add_space(6.0);
                ui.checkbox(&mut self.story_pinned, "Сохранить в профиле после истечения");
                ui.checkbox(&mut self.story_noforwards, "Запретить пересылку и скриншоты");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    let ready = !self.story_file_path.trim().is_empty();
                    if ui
                        .add_enabled_ui(ready, |ui| orange_button(ui, "Опубликовать", Vec2::new(160.0, 34.0)))
                        .inner
                        .clicked()
                    {
                        publish = true;
                    }
                    if ui.button("Отмена").clicked() {
                        cancel = true;
                    }
                });
            });
        if publish {
            let path = std::path::PathBuf::from(self.story_file_path.trim());
            self.send(Cmd::PublishStory {
                path,
                caption: self.story_caption.clone(),
                privacy: self.story_privacy,
                period_secs: self.story_period,
                pinned: self.story_pinned,
                noforwards: self.story_noforwards,
            });
            self.story_composer_open = false;
            self.story_caption.clear();
            self.story_file_path.clear();
        } else if !open || cancel || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.story_composer_open = false;
        }
    }

    // =========================================================================
    // 3. Голосовые и видеозвонки (`phone.*` E2E + история звонков)
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn call_window(&mut self, ctx: &egui::Context) {
        let Some(mut call) = self.active_call.clone() else {
            return;
        };
        ctx.request_repaint_after(std::time::Duration::from_millis(200));
        let mut accept_clicked = false;
        let mut end_clicked = false;
        let mut close_ended = false;
        let elapsed_active = call
            .connected_at
            .map_or(0u32, |t| t.elapsed().as_secs() as u32);
        let status_txt = match &call.state {
            CallState::RingingOut => "Вызов…".to_owned(),
            CallState::RingingIn => {
                if call.video {
                    "Входящий видеозвонок…".to_owned()
                } else {
                    "Входящий звонок…".to_owned()
                }
            }
            CallState::Connecting => "Обмен ключами E2E…".to_owned(),
            CallState::Active => format!(
                "{:02}:{:02} · E2E защищён{}",
                elapsed_active / 60,
                elapsed_active % 60,
                if call.relays > 0 { format!(" ({} узл.)", call.relays) } else { String::new() }
            ),
            CallState::Ended(r) => r.clone(),
        };

        egui::Window::new(if call.video { "Видеозвонок" } else { "Голосовой звонок" })
            .collapsible(false)
            .resizable(false)
            .default_width(320.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.vertical_centered(|ui| {
                    ui.add_space(8.0);
                    let pulse = (ctx.input(|i| i.time) * 3.2).sin() as f32 * 0.5 + 0.5;
                    let (av_r, _) = ui.allocate_exact_size(Vec2::splat(96.0), Sense::hover());
                    if matches!(call.state, CallState::RingingIn | CallState::RingingOut | CallState::Connecting) {
                        ui.painter().circle_stroke(
                            av_r.center(),
                            40.0 + pulse * 8.0,
                            Stroke::new(2.0, ORANGE.gamma_multiply(0.55 - pulse * 0.35)),
                        );
                    }
                    draw_avatar(
                        ui,
                        av_r.center(),
                        &call.peer_name,
                        call.peer_id,
                        72.0,
                        self.avatars.get(&call.peer_id),
                        self.settings.ayu.avatar_corners,
                    );
                    ui.add_space(8.0);
                    ui.label(RichText::new(&call.peer_name).size(18.0).strong().color(WHITE));
                    ui.label(RichText::new(&status_txt).size(13.0).color(if call.state == CallState::Active { GREEN } else { SUBTEXT }));
                    if let Some(em) = call.emojis {
                        ui.add_space(8.0);
                        egui::Frame::new()
                            .fill(t(SIDEBAR_BG))
                            .corner_radius(CornerRadius::same(10))
                            .inner_margin(egui::Margin::symmetric(14, 6))
                            .show(ui, |ui| {
                                ui.label(
                                    RichText::new(format!("{}  {}  {}  {}", em[0], em[1], em[2], em[3]))
                                        .size(20.0)
                                        .color(WHITE),
                                );
                                ui.label(
                                    RichText::new("Сверьте 4 символа с собеседником (E2E)")
                                        .size(11.0)
                                        .color(SUBTEXT),
                                );
                            });
                    }
                    if call.video && call.state == CallState::Active {
                        ui.add_space(6.0);
                        ui.label(
                            RichText::new(if call.camera_on {
                                "Камера включена (сигналинг E2E активен)"
                            } else {
                                "Камера выключена"
                            })
                            .size(11.5)
                            .color(SUBTEXT),
                        );
                    }
                    ui.add_space(14.0);
                    ui.horizontal(|ui| {
                        ui.add_space(12.0);
                        match call.state {
                            CallState::RingingIn => {
                                let (ar, aresp) = ui.allocate_exact_size(Vec2::new(124.0, 36.0), Sense::click());
                                ui.painter().rect_filled(ar, CornerRadius::same(18), GREEN);
                                ui.painter().text(ar.center(), Align2::CENTER_CENTER, "Принять", FontId::proportional(13.5), Color32::WHITE);
                                if aresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    accept_clicked = true;
                                }
                                ui.add_space(12.0);
                                let (dr, dresp) = ui.allocate_exact_size(Vec2::new(124.0, 36.0), Sense::click());
                                ui.painter().rect_filled(dr, CornerRadius::same(18), DANGER);
                                ui.painter().text(dr.center(), Align2::CENTER_CENTER, "Отклонить", FontId::proportional(13.5), Color32::WHITE);
                                if dresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    end_clicked = true;
                                }
                            }
                            CallState::Ended(_) => {
                                if orange_button(ui, "Закрыть", Vec2::new(240.0, 34.0)).clicked() {
                                    close_ended = true;
                                }
                            }
                            _ => {
                                if ui.button(if call.muted { "Микр. выкл" } else { "Микрофон" }).clicked() {
                                    call.muted = !call.muted;
                                }
                                if call.video && ui.button(if call.camera_on { "Камера вкл" } else { "Камера выкл" }).clicked() {
                                    call.camera_on = !call.camera_on;
                                }
                                if ui.button(if call.speaker { "Динамик" } else { "Обычный" }).clicked() {
                                    call.speaker = !call.speaker;
                                }
                                let (er, eresp) = ui.allocate_exact_size(Vec2::new(104.0, 32.0), Sense::click());
                                ui.painter().rect_filled(er, CornerRadius::same(16), DANGER);
                                ui.painter().text(er.center(), Align2::CENTER_CENTER, "Сбросить", FontId::proportional(13.0), Color32::WHITE);
                                if eresp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    end_clicked = true;
                                }
                            }
                        }
                    });
                    ui.add_space(6.0);
                });
            });

        if accept_clicked {
            audio_io::stop_ringtone();
            call.state = CallState::Connecting;
            self.send(Cmd::AcceptCall {
                call_id: call.call_id,
                access_hash: call.access_hash,
                video: call.video,
            });
            self.active_call = Some(call);
        } else if end_clicked {
            audio_io::stop_ringtone();
            let busy = call.state == CallState::RingingIn;
            self.send(Cmd::DiscardCall {
                call_id: call.call_id,
                access_hash: call.access_hash,
                duration: elapsed_active as i32,
                video: call.video,
                busy,
            });
            self.call_history.insert(
                0,
                rg_core::calls::CallLogEntry {
                    peer_name: call.peer_name.clone(),
                    peer_id: call.peer_id,
                    video: call.video,
                    outgoing: call.outgoing,
                    duration: elapsed_active,
                    status: if busy { "Отклонён".to_owned() } else { "Завершён".to_owned() },
                    timestamp: self.now,
                },
            );
            self.active_call = None;
        } else if close_ended {
            audio_io::stop_ringtone();
            self.active_call = None;
        } else {
            self.active_call = Some(call);
        }
    }

    #[allow(deprecated)]
    pub(crate) fn calls_history_window(&mut self, ctx: &egui::Context) {
        if !self.calls_open {
            return;
        }
        let mut open = true;
        let mut start_call: Option<(grammers_session::types::PeerRef, i64, String, bool)> = None;
        egui::Window::new("Звонки")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(390.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(380.0);
                if self.call_history.is_empty() {
                    ui.label(RichText::new("История звонков пуста. Выберите контакт ниже для вызова:").size(12.5).color(SUBTEXT));
                } else {
                    ui.label(RichText::new("Недавние звонки").strong().size(13.0).color(ORANGE));
                    egui::ScrollArea::vertical().max_height(150.0).id_salt("calls-hist").show(ui, |ui| {
                        for entry in &self.call_history {
                            ui.horizontal(|ui| {
                                let dir = if entry.outgoing { "↗" } else { "↙" };
                                let kind = if entry.video { "Видео" } else { "Аудио" };
                                ui.label(RichText::new(format!("{dir} {} · {kind}", entry.peer_name)).color(WHITE));
                                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                    ui.label(RichText::new(format!("{} ({}с)", entry.status, entry.duration)).size(11.5).color(SUBTEXT));
                                });
                            });
                        }
                    });
                }
                ui.separator();
                ui.label(RichText::new("Быстрый вызов из личных чатов").strong().size(13.0).color(WHITE));
                egui::ScrollArea::vertical().max_height(220.0).id_salt("calls-peers").show(ui, |ui| {
                    for d in self.dialogs.iter().filter(|d| d.kind == rg_core::engine::DialogKind::User && !d.saved).take(25) {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(&d.name).size(13.5).color(WHITE));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.small_button("Видео").clicked() {
                                    start_call = Some((d.peer, d.id, d.name.clone(), true));
                                }
                                if ui.small_button("Позвонить").clicked() {
                                    start_call = Some((d.peer, d.id, d.name.clone(), false));
                                }
                            });
                        });
                    }
                });
            });
        if let Some((peer, pid, name, video)) = start_call {
            self.calls_open = false;
            self.initiate_call(peer, pid, name, video);
        }
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.calls_open = false;
        }
    }

    pub(crate) fn initiate_call(
        &mut self,
        peer: grammers_session::types::PeerRef,
        peer_id: i64,
        name: String,
        video: bool,
    ) {
        if self.settings.notify.allow_sound && self.settings.notify.volume > 0 {
            audio_io::play_ringtone(self.settings.notify.volume);
        }
        self.active_call = Some(ActiveCallUi {
            call_id: 0,
            access_hash: 0,
            peer_name: name.clone(),
            peer_id,
            video,
            outgoing: true,
            state: CallState::RingingOut,
            connected_at: None,
            muted: false,
            speaker: false,
            camera_on: video,
            emojis: None,
            relays: 0,
        });
        self.send(Cmd::StartCall { peer, name, video });
    }

    // =========================================================================
    // 4. Секретные чаты E2E (MTProto 2.0)
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn secret_chats_window(&mut self, ctx: &egui::Context) {
        if !self.secret_open {
            return;
        }
        let mut open = true;
        let mut select_chat: Option<i32> = None;
        let mut start_with: Option<(grammers_session::types::PeerRef, String)> = None;
        let mut send_msg: Option<(i32, String)> = None;
        let mut set_ttl: Option<(i32, i32)> = None;
        let mut discard_id: Option<i32> = None;

        egui::Window::new("Секретные чаты (E2E MTProto 2.0)")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(660.0, 480.0))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.columns(2, |cols| {
                    // Левая колонка: список секретных чатов + создание нового.
                    cols[0].set_min_width(210.0);
                    cols[0].horizontal(|ui| {
                        ui.label(RichText::new("Ваши секретные чаты").strong().color(WHITE));
                        if ui.small_button("Обновить").clicked() {
                            self.send(Cmd::LoadSecretChats);
                        }
                    });
                    cols[0].add_space(4.0);
                    egui::ScrollArea::vertical().max_height(210.0).id_salt("sec-list").show(&mut cols[0], |ui| {
                        if self.secret_chats.is_empty() {
                            ui.label(RichText::new("Нет активных секретных чатов").size(12.0).color(SUBTEXT));
                        }
                        for c in &self.secret_chats {
                            let sel = self.secret_selected == Some(c.chat_id);
                            let badge = match c.state.as_str() {
                                "ready" => "E2E",
                                "waiting" => "ожидание",
                                _ => &c.state,
                            };
                            let lbl = format!("{} [{}]", c.peer_name, badge);
                            if ui.selectable_label(sel, RichText::new(lbl).color(if sel { ORANGE } else { WHITE })).clicked() {
                                select_chat = Some(c.chat_id);
                            }
                        }
                    });
                    cols[0].separator();
                    cols[0].label(RichText::new("+ Начать новый секретный чат").size(12.5).color(ORANGE));
                    egui::ScrollArea::vertical().max_height(175.0).id_salt("sec-new").show(&mut cols[0], |ui| {
                        for d in self.dialogs.iter().filter(|d| d.kind == rg_core::engine::DialogKind::User && !d.saved).take(25) {
                            if ui.button(&d.name).clicked() {
                                start_with = Some((d.peer, d.name.clone()));
                            }
                        }
                    });

                    // Правая колонка: выбранный секретный чат, сверка ключа, таймер и переписка.
                    if let Some(cid) = self.secret_selected
                        && let Some(chat) = self.secret_chats.iter().find(|c| c.chat_id == cid).cloned()
                    {
                        cols[1].horizontal(|ui| {
                            ui.label(RichText::new(&chat.peer_name).strong().size(15.0).color(GREEN));
                            if ui.small_button(if self.secret_show_key { "Скрыть ключ" } else { "Ключ E2E" }).clicked() {
                                self.secret_show_key = !self.secret_show_key;
                            }
                            if ui.small_button(RichText::new("Удалить").color(DANGER)).clicked() {
                                discard_id = Some(cid);
                            }
                        });
                        cols[1].horizontal_wrapped(|ui| {
                            ui.label(RichText::new("Самоуничтожение:").size(11.5).color(SUBTEXT));
                            for (s, lbl) in [(0, "Выкл"), (5, "5с"), (15, "15с"), (60, "1м"), (3600, "1ч"), (86400, "1д")] {
                                if ui.selectable_label(chat.ttl_secs == s, lbl).clicked() {
                                    set_ttl = Some((cid, s));
                                }
                            }
                        });
                        if self.secret_show_key && chat.auth_key.len() == 256 {
                            let mut key = [0u8; 256];
                            key.copy_from_slice(&chat.auth_key);
                            let grid = rg_core::secret::key_visual_grid(&key);
                            let hex = rg_core::secret::key_hex_fingerprint(&key);
                            egui::Frame::new()
                                .fill(t(SIDEBAR_BG))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(egui::Margin::same(10))
                                .show(&mut cols[1], |ui| {
                                    ui.horizontal(|ui| {
                                        let (gr, _) = ui.allocate_exact_size(Vec2::splat(64.0), Sense::hover());
                                        let palette = [
                                            Color32::from_rgb(24, 24, 30),
                                            Color32::from_rgb(120, 74, 32),
                                            Color32::from_rgb(214, 114, 28),
                                            ORANGE,
                                        ];
                                        for (ry, row) in grid.iter().enumerate() {
                                            for (cx, &cell) in row.iter().enumerate() {
                                                let r = Rect::from_min_size(
                                                    gr.min + Vec2::new(cx as f32 * 8.0, ry as f32 * 8.0),
                                                    Vec2::splat(7.5),
                                                );
                                                ui.painter().rect_filled(r, 1.0, palette[usize::from(cell.min(3))]);
                                            }
                                        }
                                        ui.vertical(|ui| {
                                            ui.label(RichText::new("Отпечаток ключа шифрования").strong().size(12.0).color(WHITE));
                                            ui.label(RichText::new(hex).monospace().size(11.0).color(ORANGE));
                                            ui.label(RichText::new("Ключ хранится только на этом устройстве").size(11.0).color(SUBTEXT));
                                        });
                                    });
                                });
                        }
                        cols[1].separator();
                        egui::ScrollArea::vertical()
                            .max_height(270.0)
                            .stick_to_bottom(true)
                            .id_salt(("sec-msgs", cid))
                            .show(&mut cols[1], |ui| {
                                if chat.state != "ready" {
                                    ui.label(RichText::new("Ожидаем, пока собеседник откроет Telegram и примет ключ E2E…").color(SUBTEXT));
                                }
                                for m in &self.secret_messages {
                                    let align = if m.outgoing { egui::Layout::right_to_left(egui::Align::Min) } else { egui::Layout::left_to_right(egui::Align::Min) };
                                    ui.with_layout(align, |ui| {
                                        let bg = if m.outgoing { t(MSG_OUT_BG) } else { t(MSG_IN_BG) };
                                        egui::Frame::new()
                                            .fill(bg)
                                            .corner_radius(CornerRadius::same(10))
                                            .inner_margin(egui::Margin::symmetric(10, 6))
                                            .show(ui, |ui| {
                                                ui.vertical(|ui| {
                                                    ui.label(RichText::new(&m.text).size(13.5).color(WHITE));
                                                    if m.ttl_secs > 0 {
                                                        let rem = (m.expires_at - self.now).max(0);
                                                        ui.label(RichText::new(format!("исчезнет через {rem}с")).size(10.5).color(ORANGE));
                                                    }
                                                });
                                            });
                                    });
                                    ui.add_space(3.0);
                                }
                            });
                        cols[1].add_space(6.0);
                        cols[1].horizontal(|ui| {
                            let r = ui.add(
                                egui::TextEdit::singleline(&mut self.secret_input)
                                    .hint_text("Зашифрованное сообщение…")
                                    .desired_width(ui.available_width() - 92.0),
                            );
                            let go = orange_button(ui, "Отпр.", Vec2::new(82.0, 28.0)).clicked()
                                || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                            if go && !self.secret_input.trim().is_empty() {
                                send_msg = Some((cid, std::mem::take(&mut self.secret_input)));
                            }
                        });
                    } else {
                        cols[1].vertical_centered(|ui| {
                            ui.add_space(80.0);
                            ui.label(RichText::new("Выберите секретный чат слева или начните новый").color(SUBTEXT));
                            ui.label(RichText::new("Сквозное шифрование MTProto 2.0 (AES-256-IGE + DH 2048)").size(11.5).color(SUBTEXT));
                        });
                    }
                });
            });

        if let Some(cid) = select_chat {
            self.secret_selected = Some(cid);
            self.secret_show_key = false;
            self.send(Cmd::OpenSecretChat(cid));
        }
        if let Some((peer, name)) = start_with {
            self.send(Cmd::StartSecretChat(peer, name));
        }
        if let Some((cid, txt)) = send_msg {
            self.send(Cmd::SendSecretText(cid, txt));
        }
        if let Some((cid, ttl)) = set_ttl {
            self.send(Cmd::SetSecretTtl(cid, ttl));
        }
        if let Some(cid) = discard_id {
            self.send(Cmd::DiscardSecretChat(cid));
            if self.secret_selected == Some(cid) {
                self.secret_selected = None;
                self.secret_messages.clear();
            }
        }
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.secret_open = false;
        }
    }

    // =========================================================================
    // 5. Администрирование групп и каналов
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn admin_window(&mut self, ctx: &egui::Context) {
        let Some(mut st) = self.admin_modal.clone() else {
            return;
        };
        let mut open = true;
        let members = self.participants.get(&st.chat_id).cloned().unwrap_or_default();
        let mut cmds: Vec<Cmd> = Vec::new();
        let mut copy_link: Option<String> = None;

        egui::Window::new(format!("Управление · {}", st.title))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(620.0, 480.0))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    for (idx, lbl) in [
                        (0u8, "Участники и баны"),
                        (1, "Права группы"),
                        (2, "Журнал действий"),
                        (3, "Ссылки"),
                    ] {
                        if ui.selectable_label(st.tab == idx, lbl).clicked() {
                            st.tab = idx;
                            if idx == 2 {
                                cmds.push(Cmd::AdminLoadLog { peer: st.peer, query: st.log_query.clone() });
                            } else if idx == 3 {
                                cmds.push(Cmd::AdminLoadInvites(st.peer));
                            }
                        }
                    }
                });
                ui.separator();
                match st.tab {
                    0 => {
                        ui.columns(2, |cols| {
                            cols[0].label(RichText::new(format!("Участники ({})", members.len())).strong().color(WHITE));
                            egui::ScrollArea::vertical().max_height(360.0).id_salt("adm-mems").show(&mut cols[0], |ui| {
                                for m in &members {
                                    let sel = st.selected_user.as_ref().is_some_and(|(_, id, _)| *id == m.id);
                                    if ui.selectable_label(sel, format!("{}  {}", m.name, m.status)).clicked() {
                                        st.selected_user = Some((m.peer, m.id, m.name.clone()));
                                    }
                                }
                            });
                            if let Some((upeer, _uid, uname)) = st.selected_user.clone() {
                                cols[1].label(RichText::new(&uname).strong().size(15.0).color(ORANGE));
                                cols[1].add_space(4.0);
                                cols[1].label(RichText::new("Права администратора:").size(12.5).color(WHITE));
                                cols[1].checkbox(&mut st.admin_rights.delete_messages, "Удаление сообщений");
                                cols[1].checkbox(&mut st.admin_rights.ban_users, "Бан и мут участников");
                                cols[1].checkbox(&mut st.admin_rights.invite_users, "Приглашать людей");
                                cols[1].checkbox(&mut st.admin_rights.pin_messages, "Закреплять сообщения");
                                cols[1].checkbox(&mut st.admin_rights.change_info, "Изменять профиль группы");
                                cols[1].checkbox(&mut st.admin_rights.post_messages, "Публиковать посты (каналы)");
                                cols[1].checkbox(&mut st.admin_rights.manage_topics, "Управлять темами");
                                cols[1].checkbox(&mut st.admin_rights.add_admins, "Назначать админов");
                                cols[1].horizontal(|ui| {
                                    ui.label("Должность:");
                                    ui.add(egui::TextEdit::singleline(&mut st.admin_rank).desired_width(140.0).hint_text("Модератор"));
                                });
                                cols[1].horizontal(|ui| {
                                    if orange_button(ui, "Сохранить админа", Vec2::new(145.0, 30.0)).clicked() {
                                        cmds.push(Cmd::AdminEditAdmin {
                                            peer: st.peer,
                                            user: upeer,
                                            rights: st.admin_rights,
                                            rank: st.admin_rank.clone(),
                                        });
                                    }
                                    if ui.button("Снять админа").clicked() {
                                        cmds.push(Cmd::AdminEditAdmin {
                                            peer: st.peer,
                                            user: upeer,
                                            rights: AdminRights::default(),
                                            rank: String::new(),
                                        });
                                    }
                                });
                                cols[1].separator();
                                cols[1].label(RichText::new("Ограничения (мут / бан):").size(12.5).color(WHITE));
                                cols[1].horizontal(|ui| {
                                    for (i, lbl) in [(0, "10 мин"), (1, "1 час"), (2, "24 ч"), (3, "Навсегда")] {
                                        if ui.selectable_label(st.mute_preset == i, lbl).clicked() {
                                            st.mute_preset = i;
                                        }
                                    }
                                });
                                let until = match st.mute_preset {
                                    0 => (self.now + 600) as i32,
                                    1 => (self.now + 3600) as i32,
                                    2 => (self.now + 86_400) as i32,
                                    _ => 0,
                                };
                                cols[1].horizontal(|ui| {
                                    if ui.button("Замутить").clicked() {
                                        cmds.push(Cmd::AdminEditBanned {
                                            peer: st.peer,
                                            user: upeer,
                                            rights: BannedRights::mute(until),
                                        });
                                    }
                                    if ui.button(RichText::new("Забанить").color(DANGER)).clicked() {
                                        cmds.push(Cmd::AdminEditBanned {
                                            peer: st.peer,
                                            user: upeer,
                                            rights: BannedRights::full_ban(),
                                        });
                                    }
                                    if ui.button("Снять ограничения").clicked() {
                                        cmds.push(Cmd::AdminEditBanned {
                                            peer: st.peer,
                                            user: upeer,
                                            rights: BannedRights::default(),
                                        });
                                    }
                                });
                            } else {
                                cols[1].label(RichText::new("Выберите участника слева для настройки прав, мута или бана").color(SUBTEXT));
                            }
                        });
                    }
                    1 => {
                        ui.label(RichText::new("Запреты по умолчанию для всех участников группы (галочка = запрещено):").size(12.5).color(SUBTEXT));
                        ui.add_space(6.0);
                        ui.checkbox(&mut st.default_rights.send_messages, "Запретить отправку сообщений");
                        ui.checkbox(&mut st.default_rights.send_media, "Запретить медиа (фото, видео, голосовые, файлы)");
                        ui.checkbox(&mut st.default_rights.send_stickers, "Запретить стикеры, GIF и инлайн-ботов");
                        ui.checkbox(&mut st.default_rights.send_polls, "Запретить создание опросов");
                        ui.checkbox(&mut st.default_rights.embed_links, "Запретить предпросмотр ссылок");
                        ui.checkbox(&mut st.default_rights.invite_users, "Запретить добавление участников");
                        ui.checkbox(&mut st.default_rights.pin_messages, "Запретить закрепление сообщений");
                        ui.checkbox(&mut st.default_rights.change_info, "Запретить изменение профиля группы");
                        ui.add_space(10.0);
                        if orange_button(ui, "Применить права группы", Vec2::new(200.0, 34.0)).clicked() {
                            cmds.push(Cmd::AdminEditDefaultRights {
                                peer: st.peer,
                                rights: st.default_rights,
                            });
                        }
                    }
                    2 => {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut st.log_query).hint_text("Фильтр по тексту…").desired_width(240.0));
                            if orange_button(ui, "Обновить", Vec2::new(100.0, 28.0)).clicked() {
                                cmds.push(Cmd::AdminLoadLog { peer: st.peer, query: st.log_query.clone() });
                            }
                        });
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().max_height(350.0).id_salt("adm-log").show(ui, |ui| {
                            if self.admin_log.is_empty() {
                                ui.label(RichText::new("Событий нет (или нужны права администратора)").color(SUBTEXT));
                            }
                            for ev in &self.admin_log {
                                egui::Frame::new()
                                    .fill(t(SIDEBAR_BG))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(egui::Margin::symmetric(10, 6))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&ev.actor).strong().color(ORANGE));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(RichText::new(full_date(i64::from(ev.date))).size(11.0).color(SUBTEXT));
                                            });
                                        });
                                        ui.label(RichText::new(&ev.summary).size(12.5).color(WHITE));
                                    });
                                ui.add_space(4.0);
                            }
                        });
                    }
                    _ => {
                        ui.horizontal(|ui| {
                            ui.add(egui::TextEdit::singleline(&mut st.invite_title).hint_text("Название ссылки").desired_width(150.0));
                            ui.add(egui::TextEdit::singleline(&mut st.invite_limit).hint_text("Лимит").desired_width(60.0));
                            ui.checkbox(&mut st.invite_request, "По заявке");
                            if orange_button(ui, "Создать", Vec2::new(90.0, 28.0)).clicked() {
                                let lim = st.invite_limit.trim().parse::<i32>().ok().filter(|&n| n > 0);
                                cmds.push(Cmd::AdminCreateInvite {
                                    peer: st.peer,
                                    title: st.invite_title.clone(),
                                    expire_date: None,
                                    usage_limit: lim,
                                    request_needed: st.invite_request,
                                });
                                st.invite_title.clear();
                                st.invite_limit.clear();
                            }
                        });
                        ui.add_space(6.0);
                        egui::ScrollArea::vertical().max_height(340.0).id_salt("adm-inv").show(ui, |ui| {
                            for inv in &self.admin_invites {
                                ui.horizontal(|ui| {
                                    let ttl = if inv.title.is_empty() { inv.link.as_str() } else { inv.title.as_str() };
                                    ui.label(RichText::new(format!("{ttl} ({} исп.)", inv.usage)).color(WHITE));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button(RichText::new("Отозвать").color(DANGER)).clicked() {
                                            cmds.push(Cmd::AdminRevokeInvite {
                                                peer: st.peer,
                                                link: inv.link.clone(),
                                            });
                                        }
                                        if ui.small_button("Копировать").clicked() {
                                            copy_link = Some(inv.link.clone());
                                        }
                                    });
                                });
                                ui.separator();
                            }
                        });
                    }
                }
            });

        for c in cmds {
            self.send(c);
        }
        if let Some(l) = copy_link {
            ctx.copy_text(l.clone());
            self.status = format!("Скопировано: {l}");
        }
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.admin_modal = None;
        } else {
            self.admin_modal = Some(st);
        }
    }
}
