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
                    if orange_button(ui, "Выбрать из галереи…", Vec2::new(200.0, 32.0)).clicked() {
                        self.android_pick_target = Some(super::AndroidPickTarget::Story);
                        let dir = crate::worker::home().join("cache").join("picked");
                        crate::android_jni::pick_file("image/* video/*", &dir);
                    }
                    ui.add_space(4.0);
                    ui.add(
                        egui::TextEdit::singleline(&mut self.story_file_path)
                            .hint_text("Или введите путь (/sdcard/DCIM/...)")
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
                    if call.state == CallState::Active {
                        if let Some(st) = &self.call_media_stats {
                            let net_line = if st.connected {
                                format!(
                                    "{} · {} мс · ↑{} / ↓{} пак.",
                                    st.transport_label, st.rtt_ms, st.tx_packets, st.rx_packets
                                )
                            } else if !st.last_error.is_empty() {
                                st.last_error.clone()
                            } else {
                                format!("{} · соединение… (↑{} пак.)", st.transport_label, st.tx_packets)
                            };
                            ui.label(RichText::new(net_line).size(11.0).color(if st.connected { GREEN } else { SUBTEXT }));
                        }
                        if self.call_stats_tick.is_none_or(|t| t.elapsed().as_millis() >= 900) {
                            self.call_stats_tick = Some(std::time::Instant::now());
                            self.send(Cmd::PollCallStats);
                        }
                    }
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
                                    self.send(Cmd::SetCallMuted(call.muted));
                                    self.active_call = Some(call.clone());
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
            .default_size(Vec2::new(680.0, 490.0))
            .frame(
                egui::Frame::new()
                    .fill(Color32::from_rgba_unmultiplied(18, 20, 26, 246))
                    .stroke(Stroke::new(1.0, Color32::from_white_alpha(34)))
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(egui::Margin::same(14)),
            )
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.columns(2, |cols| {
                    // Левая колонка: список секретных чатов + создание нового (без обрезки и без квадратов!).
                    cols[0].set_min_width(220.0);
                    cols[0].horizontal(|ui| {
                        ui.label(RichText::new("Ваши секретные чаты").strong().color(WHITE));
                        if ui.small_button("Обновить").clicked() {
                            self.send(Cmd::LoadSecretChats);
                        }
                    });
                    cols[0].add_space(4.0);
                    egui::ScrollArea::vertical().max_height(190.0).id_salt("sec-list").show(&mut cols[0], |ui| {
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
                            if ui.selectable_label(sel, RichText::new(lbl).color(if sel { c_orange() } else { WHITE })).clicked() {
                                select_chat = Some(c.chat_id);
                            }
                        }
                    });
                    cols[0].separator();
                    cols[0].label(RichText::new("+ Начать новый секретный чат").size(12.5).color(c_orange()));
                    cols[0].add_space(4.0);
                    egui::ScrollArea::vertical().max_height(190.0).id_salt("sec-new").show(&mut cols[0], |ui| {
                        for d in self.dialogs.iter().filter(|d| d.kind == rg_core::engine::DialogKind::User && !d.saved).take(30) {
                            let safe_name = if d.name.trim().is_empty() {
                                format!("Пользователь {}", d.id)
                            } else {
                                d.name.clone()
                            };
                            let (r, resp) = ui.allocate_exact_size(Vec2::new(ui.available_width() - 8.0, 32.0), Sense::click());
                            let hk = tween(ui.ctx(), ("sec-u", d.id), f32::from(u8::from(resp.hovered())), 0.1);
                            ui.painter().rect_filled(
                                r,
                                CornerRadius::same(8),
                                lerp_color(Color32::from_white_alpha(12), Color32::from_white_alpha(26), hk),
                            );
                            let inner_r = r.shrink2(Vec2::new(10.0, 4.0));
                            ui.scope_builder(egui::UiBuilder::new().max_rect(inner_r), |ui| {
                                ui.horizontal_centered(|ui| {
                                    if let Some(em) = self.emoji.as_ref() {
                                        em.rich_label(ui, &safe_name, 13.5, WHITE);
                                    } else {
                                        ui.label(RichText::new(&safe_name).size(13.5).color(WHITE));
                                    }
                                });
                            });
                            if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                start_with = Some((d.peer, safe_name));
                            }
                            ui.add_space(4.0);
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
                        ui.add_space(12.0);
                        ui.separator();
                        ui.label(RichText::new("Медленный режим (Slow Mode)").strong().color(WHITE));
                        ui.add_space(4.0);
                        ui.horizontal_wrapped(|ui| {
                            for (secs, lbl) in [
                                (0, "Выкл"),
                                (10, "10 с"),
                                (30, "30 с"),
                                (60, "1 мин"),
                                (300, "5 мин"),
                                (900, "15 мин"),
                                (3600, "1 час"),
                            ] {
                                let sel = self.admin_slowmode_secs == secs;
                                if ui.selectable_label(sel, lbl).clicked() {
                                    self.admin_slowmode_secs = secs;
                                    cmds.push(Cmd::SetSlowMode(st.peer, secs));
                                }
                            }
                        });
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

    // =========================================================================
    // 6. Telegram Stars и Подарки (Star Gifts)
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn stars_gifts_window(&mut self, ctx: &egui::Context) {
        if !self.stars_modal_open {
            return;
        }
        let mut open = true;
        let mut cmds = Vec::new();
        egui::Window::new("Telegram Stars и Подарки")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(470.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(460.0);
                // Карточка баланса
                egui::Frame::new()
                    .fill(t(SIDEBAR_BG))
                    .corner_radius(CornerRadius::same(12))
                    .inner_margin(egui::Margin::symmetric(14, 10))
                    .show(ui, |ui| {
                        ui.horizontal(|ui| {
                            let bal = self.stars_summary.as_ref().map_or(0, |s| s.balance);
                            ui.label(RichText::new(format!("⭐ {bal} Telegram Stars")).size(18.0).strong().color(ORANGE));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                if ui.button("Обновить").clicked() {
                                    cmds.push(Cmd::LoadStarsStatus);
                                    cmds.push(Cmd::LoadStarGiftsCatalog);
                                    cmds.push(Cmd::LoadSavedGifts(None));
                                }
                            });
                        });
                    });
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    for (t_idx, lbl) in [(0u8, "Транзакции"), (1, "Мои подарки"), (2, "Каталог подарков")] {
                        if ui.selectable_label(self.stars_tab == t_idx, lbl).clicked() {
                            self.stars_tab = t_idx;
                            match t_idx {
                                0 => cmds.push(Cmd::LoadStarsStatus),
                                1 => cmds.push(Cmd::LoadSavedGifts(None)),
                                _ => cmds.push(Cmd::LoadStarGiftsCatalog),
                            }
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().max_height(330.0).id_salt("stars-scroll").show(ui, |ui| {
                    match self.stars_tab {
                        0 => {
                            let Some(sum) = self.stars_summary.as_ref() else {
                                ui.label(RichText::new("Загрузка баланса и операций…").color(SUBTEXT));
                                return;
                            };
                            if sum.transactions.is_empty() {
                                ui.label(RichText::new("Операций Telegram Stars пока нет").color(SUBTEXT));
                            }
                            for tx in &sum.transactions {
                                egui::Frame::new()
                                    .fill(t(SIDEBAR_BG))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(egui::Margin::symmetric(10, 6))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(&tx.title).strong().color(WHITE));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                let sign = if tx.amount >= 0 { "+" } else { "" };
                                                let col = if tx.amount >= 0 { GREEN } else { ORANGE };
                                                ui.label(RichText::new(format!("{sign}{} ⭐", tx.amount)).strong().color(col));
                                            });
                                        });
                                        if !tx.description.is_empty() {
                                            ui.label(RichText::new(&tx.description).size(11.5).color(SUBTEXT));
                                        }
                                        ui.label(RichText::new(full_date(tx.date)).size(11.0).color(SUBTEXT));
                                    });
                                ui.add_space(4.0);
                            }
                        }
                        1 => {
                            if self.my_saved_gifts.is_empty() {
                                ui.label(RichText::new("У вас пока нет сохранённых подарков").color(SUBTEXT));
                            }
                            for g in &self.my_saved_gifts {
                                egui::Frame::new()
                                    .fill(t(SIDEBAR_BG))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(egui::Margin::symmetric(10, 8))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            ui.label(RichText::new(format!("🎁 {}", g.title)).strong().color(WHITE));
                                            if g.stars > 0 {
                                                ui.label(RichText::new(format!("{} ⭐", g.stars)).color(ORANGE));
                                            }
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                if g.convert_stars > 0
                                                    && ui.small_button(format!("Обменять ({} ⭐)", g.convert_stars)).clicked()
                                                {
                                                    cmds.push(Cmd::ConvertStarGift {
                                                        msg_id: g.msg_id,
                                                        saved_id: g.saved_id,
                                                    });
                                                }
                                                let lbl = if g.unsaved { "Показать в профиле" } else { "Скрыть" };
                                                if ui.small_button(lbl).clicked() {
                                                    cmds.push(Cmd::ToggleSaveStarGift {
                                                        msg_id: g.msg_id,
                                                        saved_id: g.saved_id,
                                                        unsave: !g.unsaved,
                                                    });
                                                }
                                            });
                                        });
                                        if !g.message.is_empty() {
                                            ui.label(RichText::new(&g.message).size(12.0).color(SUBTEXT));
                                        }
                                    });
                                ui.add_space(4.0);
                            }
                        }
                        _ => {
                            if self.star_gifts_catalog.is_empty() {
                                ui.label(RichText::new("Загрузка каталога подарков…").color(SUBTEXT));
                            }
                            for g in &self.star_gifts_catalog {
                                egui::Frame::new()
                                    .fill(t(SIDEBAR_BG))
                                    .corner_radius(CornerRadius::same(8))
                                    .inner_margin(egui::Margin::symmetric(10, 6))
                                    .show(ui, |ui| {
                                        ui.horizontal(|ui| {
                                            let badge = if g.unique { "✨" } else { "🎁" };
                                            ui.label(RichText::new(format!("{badge} {}", g.title)).strong().color(WHITE));
                                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                                ui.label(RichText::new(format!("{} ⭐", g.stars)).strong().color(ORANGE));
                                            });
                                        });
                                        if let (Some(rem), Some(tot)) = (g.remains, g.total) {
                                            ui.label(
                                                RichText::new(format!(
                                                    "Лимитированный: осталось {rem} из {tot}{}",
                                                    if g.sold_out { " (распродан)" } else { "" }
                                                ))
                                                .size(11.5)
                                                .color(SUBTEXT),
                                            );
                                        }
                                    });
                                ui.add_space(4.0);
                            }
                        }
                    }
                });
            });
        for c in cmds {
            self.send(c);
        }
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.stars_modal_open = false;
        }
    }

    // =========================================================================
    // 7. Платная звёздная реакция (`messages.sendPaidReaction`)
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn paid_reaction_modal(&mut self, ctx: &egui::Context) {
        let Some(msg_id) = self.paid_react_msg else {
            return;
        };
        let mut open = true;
        let mut send_now = false;
        egui::Window::new("Звёздная реакция ⭐")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(300.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new("Выберите количество Telegram Stars:").color(WHITE));
                ui.add_space(6.0);
                ui.horizontal_wrapped(|ui| {
                    for amt in [1, 5, 10, 25, 50, 100] {
                        if ui
                            .selectable_label(self.paid_react_count == amt, format!("⭐ {amt}"))
                            .clicked()
                        {
                            self.paid_react_count = amt;
                        }
                    }
                });
                ui.add_space(6.0);
                ui.checkbox(&mut self.paid_react_anon, "Отправить анонимно (скрыть имя)");
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if orange_button(
                        ui,
                        &format!("Отправить ⭐ {}", self.paid_react_count),
                        Vec2::new(165.0, 34.0),
                    )
                    .clicked()
                    {
                        send_now = true;
                    }
                    if ui.button("Отмена").clicked() {
                        self.paid_react_msg = None;
                    }
                });
            });
        if send_now {
            self.send(Cmd::SendPaidReaction {
                msg_id,
                count: self.paid_react_count,
                anonymous: self.paid_react_anon,
            });
            self.paid_react_msg = None;
        } else if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.paid_react_msg = None;
        }
    }

    // =========================================================================
    // 8. Telegram Бизнес: рабочие часы и быстрые ответы (`/shortcut`)
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn business_window(&mut self, ctx: &egui::Context) {
        if !self.business_open {
            return;
        }
        let mut open = true;
        let mut cmds = Vec::new();
        egui::Window::new("Telegram Бизнес — часы работы и быстрые ответы")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(450.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(440.0);
                ui.label(RichText::new("Часы работы").strong().color(ORANGE));
                ui.checkbox(&mut self.biz_hours_enabled, "Показывать график работы в профиле");
                if self.biz_hours_enabled {
                    ui.horizontal(|ui| {
                        ui.label("Часовой пояс:");
                        ui.add(egui::TextEdit::singleline(&mut self.biz_timezone).desired_width(170.0));
                    });
                    ui.horizontal(|ui| {
                        ui.label("Начало (час):");
                        ui.add(egui::DragValue::new(&mut self.biz_start_h).range(0..=23));
                        ui.label("Конец (час):");
                        ui.add(egui::DragValue::new(&mut self.biz_end_h).range(1..=24));
                        ui.checkbox(&mut self.biz_weekdays_only, "Пн–Пт");
                    });
                }
                ui.add_space(4.0);
                if orange_button(ui, "Сохранить часы работы", Vec2::new(200.0, 32.0)).clicked() {
                    let mut intervals = Vec::new();
                    let days = if self.biz_weekdays_only { 5 } else { 7 };
                    let s_m = (self.biz_start_h.clamp(0, 23)) * 60;
                    let e_m = (self.biz_end_h.clamp(1, 24)) * 60;
                    for day in 0..days {
                        let base = day * 1440;
                        intervals.push((base + s_m, base + e_m.max(s_m + 60)));
                    }
                    cmds.push(Cmd::UpdateBusinessHours {
                        enabled: self.biz_hours_enabled,
                        timezone: self.biz_timezone.clone(),
                        weekly: intervals,
                    });
                }
                ui.add_space(10.0);
                ui.separator();
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Быстрые ответы (/ярлык)").strong().color(ORANGE));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Обновить").clicked() {
                            cmds.push(Cmd::LoadQuickReplies);
                        }
                    });
                });
                ui.add_space(4.0);
                egui::ScrollArea::vertical().max_height(210.0).id_salt("biz-qr").show(ui, |ui| {
                    if self.quick_replies.is_empty() {
                        ui.label(
                            RichText::new("Быстрых ответов нет (настраиваются в Telegram Business)")
                                .size(12.0)
                                .color(SUBTEXT),
                        );
                    }
                    for qr in &self.quick_replies {
                        egui::Frame::new()
                            .fill(t(SIDEBAR_BG))
                            .corner_radius(CornerRadius::same(8))
                            .inner_margin(egui::Margin::symmetric(10, 6))
                            .show(ui, |ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(format!("/{}", qr.shortcut)).strong().color(ORANGE));
                                    ui.label(RichText::new(format!("({} сообщ.)", qr.count)).size(11.5).color(SUBTEXT));
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button(RichText::new("Удалить").color(DANGER)).clicked() {
                                            cmds.push(Cmd::DeleteQuickReplyShortcut(qr.shortcut_id));
                                        }
                                        if self.selected.is_some()
                                            && ui.small_button("Отправить в чат").clicked()
                                        {
                                            cmds.push(Cmd::SendQuickReply {
                                                shortcut_id: qr.shortcut_id,
                                                msg_ids: Vec::new(),
                                            });
                                        }
                                    });
                                });
                                if !qr.preview.is_empty() {
                                    ui.label(RichText::new(&qr.preview).size(12.0).color(WHITE));
                                }
                            });
                        ui.add_space(4.0);
                    }
                });
            });
        for c in cmds {
            self.send(c);
        }
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.business_open = false;
        }
    }

    // =========================================================================
    // 9. Темы `.tdesktop-theme` / `.attheme`, живой конструктор палитры и паки иконок AyuGram
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn themes_window(&mut self, ctx: &egui::Context) {
        if !self.themes_open {
            return;
        }
        let mut open = true;
        let mut changed = false;
        #[allow(unused_mut)]
        let mut picked_theme_path: Option<std::path::PathBuf> = None;
        #[allow(unused_mut)]
        let mut picked_wp_path: Option<std::path::PathBuf> = None;
        #[allow(unused_mut)]
        let mut export_path: Option<std::path::PathBuf> = None;

        egui::Window::new("Темы, палитра и иконки AyuGram")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(480.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.set_max_width(470.0);
                egui::ScrollArea::vertical().max_height(520.0).id_salt("themes-scroll").show(ui, |ui| {
                    ui.horizontal_wrapped(|ui| {
                        #[cfg(not(target_os = "android"))]
                        if orange_button(ui, "Импорт .tdesktop-theme / .attheme", Vec2::new(250.0, 32.0)).clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .add_filter("Темы Telegram", &["tdesktop-theme", "attheme"])
                                .pick_file()
                        {
                            picked_theme_path = Some(p);
                        }
                        #[cfg(target_os = "android")]
                        if orange_button(ui, "Импорт темы с телефона", Vec2::new(220.0, 32.0)).clicked() {
                            self.android_pick_target = Some(super::AndroidPickTarget::Theme);
                            let dir = crate::worker::home().join("themes");
                            crate::android_jni::pick_file("*/*", &dir);
                        }
                        #[cfg(not(target_os = "android"))]
                        if ui.button("Экспорт").clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .set_file_name("rustgramm.tdesktop-theme")
                                .save_file()
                        {
                            export_path = Some(p);
                        }
                        if ui.button("Сброс").clicked() {
                            self.settings.theme = rg_settings::CustomThemeSettings::default();
                            self.wallpaper_tex = None;
                            changed = true;
                        }
                    });
                    ui.add_space(8.0);
                    ui.label(RichText::new("Готовые пресеты").strong().color(ORANGE));
                    ui.horizontal_wrapped(|ui| {
                        for (title, preset) in crate::theme_pack::builtin_presets() {
                            let active = self.settings.theme.active_theme_name == preset.active_theme_name;
                            if ui.selectable_label(active, title).clicked() {
                                let wp = self.settings.theme.wallpaper_path.clone();
                                let dim = self.settings.theme.wallpaper_dim;
                                let ip = self.settings.theme.icon_pack.clone();
                                self.settings.theme = preset;
                                self.settings.theme.wallpaper_path = wp;
                                self.settings.theme.wallpaper_dim = dim;
                                self.settings.theme.icon_pack = ip;
                                changed = true;
                            }
                        }
                    });

                    ui.add_space(10.0);
                    ui.label(RichText::new("Живой конструктор палитры").strong().color(ORANGE));
                    let mut color_row = |ui: &mut egui::Ui, label: &str, hex: &mut String, def: Color32| {
                        ui.horizontal(|ui| {
                            ui.label(RichText::new(label).size(12.5).color(WHITE));
                            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                let mut c = crate::theme_pack::parse_hex_color(hex).unwrap_or(def);
                                if ui.color_edit_button_srgba(&mut c).changed() {
                                    *hex = crate::theme_pack::color_to_hex(c);
                                    changed = true;
                                }
                            });
                        });
                    };
                    color_row(ui, "Главный акцент", &mut self.settings.theme.accent_hex, ORANGE);
                    color_row(ui, "Фон окон и карточек", &mut self.settings.theme.window_bg_hex, WINDOW_BG);
                    color_row(ui, "Панель списка чатов", &mut self.settings.theme.sidebar_bg_hex, SIDEBAR_BG);
                    color_row(ui, "Фон ленты сообщений", &mut self.settings.theme.chat_bg_hex, CHAT_BG);
                    color_row(ui, "Входящий пузырь", &mut self.settings.theme.msg_in_bg_hex, MSG_IN_BG);
                    color_row(ui, "Исходящий пузырь", &mut self.settings.theme.msg_out_bg_hex, MSG_OUT_BG);
                    color_row(ui, "Основной текст", &mut self.settings.theme.text_primary_hex, WHITE);
                    color_row(ui, "Вторичный текст", &mut self.settings.theme.text_secondary_hex, SUBTEXT);

                    ui.add_space(10.0);
                    ui.label(RichText::new("Кастомный фон чата").strong().color(ORANGE));
                    ui.horizontal(|ui| {
                        #[cfg(not(target_os = "android"))]
                        if ui.button("Выбрать картинку фона…").clicked()
                            && let Some(p) = rfd::FileDialog::new()
                                .add_filter("Изображения", &["jpg", "jpeg", "png", "webp"])
                                .pick_file()
                        {
                            picked_wp_path = Some(p);
                        }
                        if !self.settings.theme.wallpaper_path.is_empty() && ui.button("Убрать фон").clicked() {
                            self.settings.theme.wallpaper_path.clear();
                            self.wallpaper_tex = None;
                            changed = true;
                        }
                    });
                    let mut dim = self.settings.theme.wallpaper_dim;
                    if ui.add(egui::Slider::new(&mut dim, 0..=90).text("Затемнение фона (%)")).changed() {
                        self.settings.theme.wallpaper_dim = dim;
                        changed = true;
                    }

                    ui.add_space(10.0);
                    ui.label(RichText::new("Пак иконок AyuGram").strong().color(ORANGE));
                    ui.horizontal_wrapped(|ui| {
                        for (id, lbl) in [
                            ("default", "Стандартный"),
                            ("ayu_bold", "AyuGram Bold"),
                            ("ayu_minimal", "AyuGram Minimal"),
                        ] {
                            if ui
                                .selectable_label(self.settings.theme.icon_pack == id, lbl)
                                .clicked()
                            {
                                self.settings.theme.icon_pack = id.to_owned();
                                changed = true;
                            }
                        }
                    });
                    // Пользовательские паки из `<RG_HOME>/icon_packs/<name>/`
                    let packs_dir = crate::worker::home().join("icon_packs");
                    if let Ok(rd) = std::fs::read_dir(&packs_dir) {
                        ui.horizontal_wrapped(|ui| {
                            for entry in rd.flatten() {
                                if entry.path().is_dir()
                                    && let Some(name) = entry.file_name().to_str()
                                    && ui
                                        .selectable_label(self.settings.theme.icon_pack == name, format!("📁 {name}"))
                                        .clicked()
                                {
                                    self.settings.theme.icon_pack = name.to_owned();
                                    changed = true;
                                }
                            }
                        });
                    }
                });
            });

        if let Some(p) = picked_theme_path {
            self.apply_imported_theme(ctx, &p);
            changed = true;
        }
        if let Some(wp) = picked_wp_path {
            let themes_dir = crate::worker::home().join("themes");
            let _ = std::fs::create_dir_all(&themes_dir);
            let ext = wp.extension().and_then(|e| e.to_str()).unwrap_or("jpg");
            let dst = themes_dir.join(format!("custom_wallpaper.{ext}"));
            if std::fs::copy(&wp, &dst).is_ok() {
                self.settings.theme.wallpaper_path = dst.to_string_lossy().into_owned();
                self.wallpaper_tex = None;
                changed = true;
            }
        }
        if let Some(ep) = export_path {
            let txt = crate::theme_pack::export_tdesktop_colors(&self.settings.theme);
            if std::fs::write(&ep, txt).is_ok() {
                self.status = "Тема экспортирована в .tdesktop-theme".to_owned();
            }
        }
        if changed {
            sync_custom_theme(&self.settings.theme);
            crate::icons::set_icon_pack(
                &self.settings.theme.icon_pack,
                Some(&crate::worker::home().join("icon_packs")),
            );
            self.send(Cmd::Settings(Box::new(self.settings.clone())));
        }
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.themes_open = false;
        }
    }

    // =========================================================================
    // 10. Новые функции Telegram (v0.19.0): Отложенные, Чек-листы, Опросы, Хэштеги, Ctrl+K
    // =========================================================================

    #[allow(deprecated)]
    pub(crate) fn scheduled_window(&mut self, ctx: &egui::Context) {
        if !self.scheduled_open {
            return;
        }
        let mut open = true;
        let mut send_now: Option<i32> = None;
        let mut delete_id: Option<i32> = None;
        let mut refresh = false;
        egui::Window::new("⏰ Отложенные сообщения и напоминания")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(440.0, 420.0))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("Запланированные сообщения этого чата").size(12.5).color(SUBTEXT));
                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                        if ui.small_button("Обновить").clicked() {
                            refresh = true;
                        }
                    });
                });
                ui.separator();
                if self.scheduled_msgs.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Нет отложенных сообщений").color(SUBTEXT));
                        ui.label(
                            RichText::new("Нажмите ПКМ по кнопке отправки в чате → «Отложить / Напоминание»")
                                .size(12.0)
                                .color(SUBTEXT),
                        );
                    });
                } else {
                    egui::ScrollArea::vertical().max_height(320.0).show(ui, |ui| {
                        for m in &self.scheduled_msgs {
                            egui::Frame::new()
                                .fill(t(SIDEBAR_BG))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width() - 8.0);
                                    ui.horizontal(|ui| {
                                        ui.label(RichText::new(full_date(m.date)).size(11.5).color(c_orange()));
                                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                            if ui.small_button(RichText::new("Удалить").color(DANGER)).clicked() {
                                                delete_id = Some(m.id);
                                            }
                                            if ui.small_button("Отправить сейчас").clicked() {
                                                send_now = Some(m.id);
                                            }
                                        });
                                    });
                                    ui.label(RichText::new(&m.text).size(13.5).color(WHITE));
                                });
                            ui.add_space(6.0);
                        }
                    });
                }
            });
        if refresh {
            self.send(Cmd::LoadScheduled);
        }
        if let Some(id) = send_now {
            self.send(Cmd::SendScheduledNow(vec![id]));
        }
        if let Some(id) = delete_id {
            self.send(Cmd::DeleteScheduled(vec![id]));
        }
        if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.scheduled_open = false;
        }
    }

    #[allow(deprecated)]
    pub(crate) fn todo_creator_window(&mut self, ctx: &egui::Context) {
        if !self.todo_modal_open {
            return;
        }
        let mut open = true;
        let mut submit = false;
        egui::Window::new("📋 Новый чек-лист (To-Do)")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(390.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new("Заголовок списка").size(12.5).color(SUBTEXT));
                ui.add(
                    egui::TextEdit::singleline(&mut self.todo_title)
                        .hint_text("Например: Задачи на сегодня")
                        .desired_width(360.0),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("Пункты").size(12.5).color(SUBTEXT));
                let mut remove_idx: Option<usize> = None;
                let todo_len = self.todo_items.len();
                for (i, item) in self.todo_items.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        ui.label(RichText::new(format!("{}.", i + 1)).color(c_orange()));
                        ui.add(
                            egui::TextEdit::singleline(item)
                                .hint_text("Пункт чек-листа…")
                                .desired_width(300.0),
                        );
                        if todo_len > 2 && ui.small_button("×").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                }
                if let Some(ri) = remove_idx {
                    self.todo_items.remove(ri);
                }
                if self.todo_items.len() < 15 && ui.small_button("+ Добавить пункт").clicked() {
                    self.todo_items.push(String::new());
                }
                ui.add_space(6.0);
                ui.checkbox(&mut self.todo_others_complete, "Разрешить собеседникам отмечать пункты");
                ui.checkbox(&mut self.todo_others_append, "Разрешить добавлять новые пункты");
                ui.add_space(10.0);
                let valid = !self.todo_title.trim().is_empty()
                    && self.todo_items.iter().any(|x| !x.trim().is_empty());
                if ui
                    .add_enabled_ui(valid, |ui| orange_button(ui, "Отправить чек-лист", Vec2::new(200.0, 34.0)))
                    .inner
                    .clicked()
                {
                    submit = true;
                }
            });
        if submit {
            let items: Vec<String> = self
                .todo_items
                .iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            self.send(Cmd::SendTodo {
                title: self.todo_title.trim().to_owned(),
                items,
                others_can_complete: self.todo_others_complete,
                others_can_append: self.todo_others_append,
            });
            self.todo_title.clear();
            self.todo_items = vec![String::new(), String::new()];
            self.todo_modal_open = false;
        } else if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.todo_modal_open = false;
        }
    }

    #[allow(deprecated)]
    pub(crate) fn poll_creator_window(&mut self, ctx: &egui::Context) {
        if !self.poll_modal_open {
            return;
        }
        let mut open = true;
        let mut submit = false;
        egui::Window::new("📊 Новый опрос / Викторина")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(400.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new("Вопрос").size(12.5).color(SUBTEXT));
                ui.add(
                    egui::TextEdit::singleline(&mut self.poll_question)
                        .hint_text("Задайте вопрос…")
                        .desired_width(370.0),
                );
                ui.add_space(8.0);
                ui.label(RichText::new("Варианты ответа").size(12.5).color(SUBTEXT));
                let mut remove_idx: Option<usize> = None;
                let poll_len = self.poll_options.len();
                for (i, opt) in self.poll_options.iter_mut().enumerate() {
                    ui.horizontal(|ui| {
                        if self.poll_quiz {
                            ui.radio_value(&mut self.poll_correct, i, "");
                        } else {
                            ui.label(RichText::new(format!("{}.", i + 1)).color(c_orange()));
                        }
                        ui.add(
                            egui::TextEdit::singleline(opt)
                                .hint_text(format!("Вариант {}", i + 1))
                                .desired_width(305.0),
                        );
                        if poll_len > 2 && ui.small_button("×").clicked() {
                            remove_idx = Some(i);
                        }
                    });
                }
                if let Some(ri) = remove_idx {
                    self.poll_options.remove(ri);
                    self.poll_correct = self.poll_correct.min(self.poll_options.len().saturating_sub(1));
                }
                if self.poll_options.len() < 10 && ui.small_button("+ Добавить вариант").clicked() {
                    self.poll_options.push(String::new());
                }
                ui.add_space(6.0);
                ui.checkbox(&mut self.poll_anonymous, "Анонимное голосование");
                if ui.checkbox(&mut self.poll_quiz, "Режим викторины (один правильный ответ)").changed() && self.poll_quiz {
                    self.poll_multiple = false;
                }
                if !self.poll_quiz {
                    ui.checkbox(&mut self.poll_multiple, "Выбор нескольких ответов");
                } else {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.poll_explanation)
                            .hint_text("Пояснение к ответу (необязательно)")
                            .desired_width(370.0),
                    );
                }
                ui.add_space(10.0);
                let opts_ok = self.poll_options.iter().filter(|x| !x.trim().is_empty()).count() >= 2;
                let valid = !self.poll_question.trim().is_empty() && opts_ok;
                if ui
                    .add_enabled_ui(valid, |ui| orange_button(ui, "Отправить опрос", Vec2::new(190.0, 34.0)))
                    .inner
                    .clicked()
                {
                    submit = true;
                }
            });
        if submit {
            let options: Vec<String> = self
                .poll_options
                .iter()
                .map(|s| s.trim().to_owned())
                .filter(|s| !s.is_empty())
                .collect();
            self.send(Cmd::SendPoll {
                question: self.poll_question.trim().to_owned(),
                options,
                anonymous: self.poll_anonymous,
                multiple_choice: self.poll_multiple && !self.poll_quiz,
                quiz: self.poll_quiz,
                correct_idx: self.poll_quiz.then_some(self.poll_correct),
                explanation: self.poll_explanation.trim().to_owned(),
            });
            self.poll_question.clear();
            self.poll_options = vec![String::new(), String::new()];
            self.poll_explanation.clear();
            self.poll_modal_open = false;
        } else if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.poll_modal_open = false;
        }
    }

    #[allow(deprecated)]
    pub(crate) fn hashtag_search_window(&mut self, ctx: &egui::Context) {
        let Some((mut tag, rows)) = self.hashtag_modal.clone() else {
            return;
        };
        let mut open = true;
        let mut search_again: Option<String> = None;
        egui::Window::new(format!("Глобальный поиск #{tag}"))
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size(Vec2::new(480.0, 440.0))
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal(|ui| {
                    ui.label(RichText::new("#").strong().color(c_orange()));
                    let r = ui.add(egui::TextEdit::singleline(&mut tag).desired_width(300.0));
                    if (orange_button(ui, "Искать", Vec2::new(84.0, 28.0)).clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter))))
                        && !tag.trim().is_empty()
                    {
                        search_again = Some(tag.trim().trim_start_matches('#').to_owned());
                    }
                });
                ui.separator();
                if rows.is_empty() {
                    ui.add_space(24.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Ищем публикации по хэштегу в публичных каналах…").color(SUBTEXT));
                    });
                } else {
                    egui::ScrollArea::vertical().max_height(350.0).show(ui, |ui| {
                        for m in &rows {
                            egui::Frame::new()
                                .fill(t(SIDEBAR_BG))
                                .corner_radius(CornerRadius::same(10))
                                .inner_margin(egui::Margin::same(10))
                                .show(ui, |ui| {
                                    ui.set_width(ui.available_width() - 8.0);
                                    ui.horizontal(|ui| {
                                        let who = if m.sender.is_empty() { "Публичный канал" } else { &m.sender };
                                        ui.label(RichText::new(who).strong().size(12.5).color(c_orange()));
                                        ui.label(RichText::new(full_date(m.date)).size(11.0).color(SUBTEXT));
                                    });
                                    let txt = if m.text.is_empty() { "[Медиа]" } else { &m.text };
                                    ui.label(RichText::new(txt).size(13.0).color(WHITE));
                                });
                            ui.add_space(6.0);
                        }
                    });
                }
            });
        if let Some(new_tag) = search_again {
            self.hashtag_modal = Some((new_tag.clone(), Vec::new()));
            self.send(Cmd::SearchHashtag(new_tag));
        } else if !open || ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.hashtag_modal = None;
        } else if let Some((cur_tag, _)) = self.hashtag_modal.as_mut() {
            *cur_tag = tag;
        }
    }

    /// 10. Палитра быстрого перехода `Ctrl+K` (Quick Switcher) по всем чатам (до 5000+) и командам.
    #[allow(deprecated)]
    pub(crate) fn quick_switcher_window(&mut self, ctx: &egui::Context) {
        let raw = tween(ctx, "ctrl-k-palette", f32::from(u8::from(self.palette_open)), 0.16);
        if raw < 0.01 && !self.palette_open {
            return;
        }
        let appear = ios_spring(raw);
        let screen = ctx.content_rect();
        egui::Area::new(egui::Id::new("palette-dim"))
            .order(egui::Order::Foreground)
            .fixed_pos(Pos2::ZERO)
            .show(ctx, |ui| {
                let (r, resp) = ui.allocate_exact_size(screen.size(), Sense::click());
                paint_frosted_scrim(ui.painter(), r, raw);
                if resp.clicked() {
                    self.palette_open = false;
                }
            });

        #[derive(Clone)]
        enum PaletteHit {
            Dialog(usize),
            Cmd(&'static str),
        }
        let q = self.palette_query.trim().to_lowercase();
        let mut hits: Vec<(String, String, PaletteHit)> = Vec::new();
        for (idx, d) in self.dialogs.iter().enumerate() {
            let name = if d.saved { "Избранное" } else { d.name.as_str() };
            if q.is_empty() || name.to_lowercase().contains(&q) {
                let sub = if d.archived { "Архив" } else if d.saved { "Личное хранилище" } else { &d.last };
                hits.push((name.to_owned(), sub.chars().take(44).collect(), PaletteHit::Dialog(idx)));
                if hits.len() >= 14 {
                    break;
                }
            }
        }
        const ACTIONS: [(&str, &str, &str); 9] = [
            ("Настройки Telegram", "Профиль, приватность, прокси, логи", "settings"),
            ("Мои звёзды и подарки ⭐", "Баланс Stars и витрина подарков", "stars"),
            ("Telegram Бизнес", "Часы работы и быстрые ответы", "business"),
            ("Темы и иконки AyuGram", ".tdesktop-theme / .attheme / паки иконок", "themes"),
            ("Секретные чаты (E2E)", "Шифрование MTProto 2.0", "secret"),
            ("Переключить Режим призрака", "Не читать сообщения и истории", "ghost"),
            ("Создать чек-лист (To-Do)", "Интерактивный список в открытый чат", "todo"),
            ("Создать опрос / викторину", "В открытый чат", "poll"),
            ("Добавить ещё один аккаунт", "Без ограничений на количество аккаунтов", "add_acc"),
        ];
        for (title, sub, key) in ACTIONS {
            if q.is_empty() || title.to_lowercase().contains(&q) || sub.to_lowercase().contains(&q) {
                hits.push((format!("⚡ {title}"), sub.to_owned(), PaletteHit::Cmd(key)));
            }
        }
        if !hits.is_empty() {
            self.palette_sel = self.palette_sel.min(hits.len() - 1);
        } else {
            self.palette_sel = 0;
        }

        let mut chosen: Option<PaletteHit> = None;
        if self.palette_open {
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowDown)) && !hits.is_empty() {
                self.palette_sel = (self.palette_sel + 1) % hits.len();
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::ArrowUp)) && !hits.is_empty() {
                self.palette_sel = if self.palette_sel == 0 { hits.len() - 1 } else { self.palette_sel - 1 };
            }
            if ctx.input_mut(|i| i.consume_key(egui::Modifiers::NONE, egui::Key::Enter))
                && let Some((_, _, h)) = hits.get(self.palette_sel)
            {
                chosen = Some(h.clone());
            }
        }

        let card_w = 460.0_f32.min(screen.width() - 28.0);
        egui::Area::new(egui::Id::new("palette-box"))
            .order(egui::Order::Tooltip)
            .anchor(Align2::CENTER_TOP, Vec2::new(0.0, 68.0 + (1.0 - appear) * 16.0))
            .show(ctx, |ui| {
                ui.set_opacity(appear.clamp(0.02, 1.0));
                egui::Frame::new()
                    .fill(Color32::TRANSPARENT)
                    .corner_radius(CornerRadius::same(16))
                    .inner_margin(egui::Margin::same(12))
                    .show(ui, |ui| {
                        ui.set_width(card_w);
                        paint_frosted_glass_panel(ui.painter(), ui.max_rect().expand(12.0), 16.0, raw);
                        let inp = ui.add(
                            egui::TextEdit::singleline(&mut self.palette_query)
                                .hint_text("Быстрый переход: чат, команда, настройки… (Ctrl+K)")
                                .desired_width(card_w - 8.0)
                                .margin(Vec2::new(10.0, 8.0)),
                        );
                        if self.palette_open {
                            inp.request_focus();
                        }
                        ui.add_space(8.0);
                        egui::ScrollArea::vertical().max_height(330.0).id_salt("palette-list").show(ui, |ui| {
                            for (i, (title, sub, hit)) in hits.iter().enumerate() {
                                let sel = i == self.palette_sel;
                                let (r, resp) = ui.allocate_exact_size(Vec2::new(card_w - 8.0, 40.0), Sense::click());
                                if sel || resp.hovered() {
                                    ui.painter().rect_filled(
                                        r,
                                        CornerRadius::same(9),
                                        if sel {
                                            Color32::from_rgba_unmultiplied(c_orange().r(), c_orange().g(), c_orange().b(), 44)
                                        } else {
                                            t(WINDOW_OVER)
                                        },
                                    );
                                }
                                ui.painter().text(
                                    Pos2::new(r.left() + 12.0, r.center().y - 7.0),
                                    Align2::LEFT_CENTER,
                                    title,
                                    FontId::proportional(13.5),
                                    if sel { c_orange() } else { WHITE },
                                );
                                ui.painter().text(
                                    Pos2::new(r.left() + 12.0, r.center().y + 9.0),
                                    Align2::LEFT_CENTER,
                                    sub,
                                    FontId::proportional(11.0),
                                    SUBTEXT,
                                );
                                if resp.on_hover_cursor(egui::CursorIcon::PointingHand).clicked() {
                                    chosen = Some(hit.clone());
                                }
                            }
                        });
                    });
            });

        if let Some(hit) = chosen {
            self.palette_open = false;
            match hit {
                PaletteHit::Dialog(idx) => {
                    if let Some(d) = self.dialogs.get(idx) {
                        self.in_archive = d.archived;
                    }
                    self.open_dialog(idx);
                }
                PaletteHit::Cmd("settings") => self.open_tg_settings(),
                PaletteHit::Cmd("stars") => {
                    self.stars_modal_open = true;
                    self.send(Cmd::LoadStarsStatus);
                    self.send(Cmd::LoadSavedGifts(None));
                    self.send(Cmd::LoadStarGiftsCatalog);
                }
                PaletteHit::Cmd("business") => {
                    self.business_open = true;
                    self.send(Cmd::LoadQuickReplies);
                }
                PaletteHit::Cmd("themes") => self.themes_open = true,
                PaletteHit::Cmd("secret") => {
                    self.secret_open = true;
                    self.send(Cmd::LoadSecretChats);
                }
                PaletteHit::Cmd("ghost") => {
                    let on = !self.settings.ghost.active;
                    self.settings.ghost.set_active(on);
                    self.send(Cmd::Settings(Box::new(self.settings.clone())));
                }
                PaletteHit::Cmd("todo") if self.selected.is_some() => self.todo_modal_open = true,
                PaletteHit::Cmd("poll") if self.selected.is_some() => self.poll_modal_open = true,
                PaletteHit::Cmd("add_acc") => self.send(Cmd::AddAccountAuto),
                PaletteHit::Cmd(_) => {}
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.palette_open = false;
        }
    }

    /// Модальное окно выбора эмодзи-статуса аккаунта и таймера действия (`account.updateEmojiStatus`).
    pub(crate) fn emoji_status_window(&mut self, ctx: &egui::Context) {
        if !self.emoji_status_open {
            return;
        }
        if !self.default_emoji_statuses_req {
            self.default_emoji_statuses_req = true;
            self.send(Cmd::LoadDefaultEmojiStatuses);
        }
        if self.emoji_sets.is_empty() && !self.emoji_sets_requested {
            self.emoji_sets_requested = true;
            self.send(Cmd::EmojiSets);
        }
        let mut candidates: Vec<i64> = self.default_emoji_statuses.clone();
        for set in self.emoji_sets.iter().take(6) {
            if let Some(list) = self.sticker_lists.get(&set.id) {
                for row in list.iter().take(24) {
                    if !candidates.contains(&row.id) {
                        candidates.push(row.id);
                    }
                }
            }
        }
        let missing: Vec<i64> = candidates
            .iter()
            .copied()
            .filter(|id| !self.sticker_tex.contains_key(id) && self.custom_emoji_req.insert(*id))
            .take(48)
            .collect();
        if !missing.is_empty() {
            self.send(Cmd::CustomEmojiDocs(missing));
        }

        let mut open = self.emoji_status_open;
        let mut pick: Option<Option<i64>> = None;
        egui::Window::new("Эмодзи-статус профиля")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(370.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new("Срок действия:").size(12.5).color(SUBTEXT));
                ui.horizontal_wrapped(|ui| {
                    for (lbl, secs) in [
                        ("Навсегда", 0),
                        ("1 час", 3600),
                        ("2 часа", 7200),
                        ("8 часов", 28800),
                        ("1 день", 86400),
                        ("7 дней", 604800),
                    ] {
                        let sel = self.emoji_status_ttl == secs;
                        if ui.selectable_label(sel, lbl).clicked() {
                            self.emoji_status_ttl = secs;
                        }
                    }
                });
                ui.add_space(6.0);
                if ui.button("Сбросить эмодзи-статус").clicked() {
                    pick = Some(None);
                }
                ui.separator();
                if candidates.is_empty() {
                    ui.vertical_centered(|ui| {
                        ui.add_space(16.0);
                        ui.add(egui::Spinner::new().size(20.0).color(c_orange()));
                        ui.add_space(6.0);
                        ui.label(RichText::new("Загрузка статусов…").color(SUBTEXT));
                        ui.add_space(12.0);
                    });
                } else {
                    egui::ScrollArea::vertical().max_height(260.0).show(ui, |ui| {
                        ui.horizontal_wrapped(|ui| {
                            ui.spacing_mut().item_spacing = Vec2::splat(6.0);
                            for doc_id in &candidates {
                                let (rect, resp) = ui.allocate_exact_size(Vec2::splat(36.0), Sense::click());
                                if resp.hovered() {
                                    ui.painter().rect_filled(rect, CornerRadius::same(8), t(WINDOW_OVER));
                                }
                                if let Some(tex) = self.sticker_tex.get(doc_id) {
                                    egui::Image::new(tex)
                                        .fit_to_exact_size(Vec2::splat(26.0))
                                        .paint_at(ui, Rect::from_center_size(rect.center(), Vec2::splat(26.0)));
                                } else {
                                    ui.painter().text(
                                        rect.center(),
                                        Align2::CENTER_CENTER,
                                        "✨",
                                        FontId::proportional(18.0),
                                        WHITE,
                                    );
                                }
                                if resp
                                    .on_hover_text(format!("ID {doc_id}"))
                                    .on_hover_cursor(egui::CursorIcon::PointingHand)
                                    .clicked()
                                {
                                    pick = Some(Some(*doc_id));
                                }
                            }
                        });
                    });
                }
            });
        self.emoji_status_open = open;
        if let Some(choice) = pick {
            let until = match (choice, self.emoji_status_ttl) {
                (Some(_), secs) if secs > 0 => Some((self.now + i64::from(secs)) as i32),
                _ => None,
            };
            self.send(Cmd::SetEmojiStatus {
                document_id: choice,
                until,
            });
            self.emoji_status_open = false;
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.emoji_status_open = false;
        }
    }

    #[allow(deprecated)]
    pub(crate) fn global_deleted_window(&mut self, ctx: &egui::Context) {
        if !self.global_deleted_open {
            return;
        }
        let mut open = self.global_deleted_open;
        let mut reload_q: Option<String> = None;
        let mut jump_to: Option<(i64, i32)> = None;
        egui::Window::new("Глобальный журнал удалённых")
            .open(&mut open)
            .collapsible(false)
            .resizable(true)
            .default_size([460.0, 480.0])
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                let (tot, del, ed) = self.global_deleted_stats;
                ui.label(
                    RichText::new(format!("Сохранено: {tot} · Удалено: {del} · Правок: {ed}"))
                        .size(12.0)
                        .color(SUBTEXT),
                );
                ui.add_space(4.0);
                ui.horizontal(|ui| {
                    let resp = ui.add(
                        egui::TextEdit::singleline(&mut self.global_deleted_query)
                            .hint_text("Поиск по тексту удалённых сообщений…")
                            .desired_width(300.0),
                    );
                    if resp.changed() {
                        reload_q = Some(self.global_deleted_query.clone());
                    }
                    if ui.button("Обновить").clicked() {
                        reload_q = Some(self.global_deleted_query.clone());
                    }
                });
                ui.separator();
                if self.global_deleted_items.is_empty() {
                    ui.add_space(16.0);
                    ui.vertical_centered(|ui| {
                        ui.label(RichText::new("Удалённых сообщений по запросу не найдено.").color(SUBTEXT));
                    });
                } else {
                    egui::ScrollArea::vertical().max_height(370.0).show(ui, |ui| {
                        for item in &self.global_deleted_items {
                            let chat_name = self
                                .dialogs
                                .iter()
                                .find(|d| d.id == item.peer)
                                .map_or_else(|| format!("Чат {}", item.peer), |d| d.name.clone());
                            ui.group(|ui| {
                                ui.horizontal(|ui| {
                                    ui.label(RichText::new(chat_name).strong().color(ORANGE));
                                    ui.label(
                                        RichText::new(format!("#{} · {}", item.msg_id, full_date(item.date)))
                                            .size(11.5)
                                            .color(SUBTEXT),
                                    );
                                    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                                        if ui.small_button("Копия").clicked() {
                                            ctx.copy_text(item.text.clone());
                                        }
                                        if self.dialogs.iter().any(|d| d.id == item.peer)
                                            && ui.small_button("В чат").clicked()
                                        {
                                            jump_to = Some((item.peer, item.msg_id));
                                        }
                                    });
                                });
                                ui.label(RichText::new(&item.text).color(WHITE));
                            });
                            ui.add_space(3.0);
                        }
                    });
                }
            });
        self.global_deleted_open = open;
        if let Some(q) = reload_q {
            self.send(Cmd::GlobalDeletedLog(q));
        }
        if let Some((pid, mid)) = jump_to {
            self.global_deleted_open = false;
            if let Some(i) = self.dialogs.iter().position(|d| d.id == pid) {
                self.open_dialog(i);
                self.scroll_to = Some(mid);
                self.highlight_msg = Some((mid, ctx.input(|k| k.time)));
            }
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.global_deleted_open = false;
        }
    }

    #[allow(deprecated)]
    pub(crate) fn date_jump_window(&mut self, ctx: &egui::Context) {
        if !self.date_jump_open {
            return;
        }
        let Some(sel) = self.selected else {
            self.date_jump_open = false;
            return;
        };
        let mut open = self.date_jump_open;
        let mut target_ts: Option<i32> = None;
        let now_i32 = i32::try_from(self.now).unwrap_or(i32::MAX);
        egui::Window::new("Переход к дате")
            .open(&mut open)
            .collapsible(false)
            .resizable(false)
            .default_width(330.0)
            .anchor(Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.label(RichText::new("Быстрый переход:").size(12.5).color(SUBTEXT));
                ui.horizontal_wrapped(|ui| {
                    for (lbl, ts) in [
                        ("Сегодня", now_i32),
                        ("Вчера", now_i32.saturating_sub(86_400)),
                        ("Неделя", now_i32.saturating_sub(7 * 86_400)),
                        ("Месяц", now_i32.saturating_sub(30 * 86_400)),
                        ("Год", now_i32.saturating_sub(365 * 86_400)),
                        ("Начало чата", 1),
                    ] {
                        if ui.button(lbl).clicked() {
                            target_ts = Some(ts);
                        }
                    }
                });
                ui.separator();
                ui.horizontal(|ui| {
                    ui.add(
                        egui::TextEdit::singleline(&mut self.date_jump_input)
                            .hint_text("ДД.ММ.ГГГГ")
                            .desired_width(140.0),
                    );
                    if orange_button(ui, "Перейти", Vec2::new(110.0, 28.0)).clicked() {
                        if let Some(ts) = rg_core::engine::parse_date_dmy(&self.date_jump_input) {
                            target_ts = Some(ts);
                        } else {
                            self.error = "Введите дату в формате ДД.ММ.ГГГГ".to_owned();
                            self.error_at = self.now;
                        }
                    }
                });
                ui.separator();
                let month_names = [
                    "Январь", "Февраль", "Март", "Апрель", "Май", "Июнь",
                    "Июль", "Август", "Сентябрь", "Октябрь", "Ноябрь", "Декабрь",
                ];
                let m_name = month_names.get((self.date_jump_month as usize).saturating_sub(1)).copied().unwrap_or("");
                ui.horizontal(|ui| {
                    if ui.button("‹").clicked() {
                        if self.date_jump_month <= 1 {
                            self.date_jump_month = 12;
                            self.date_jump_year -= 1;
                        } else {
                            self.date_jump_month -= 1;
                        }
                    }
                    ui.label(RichText::new(format!("{m_name} {}", self.date_jump_year)).strong().color(WHITE));
                    if ui.button("›").clicked() {
                        if self.date_jump_month >= 12 {
                            self.date_jump_month = 1;
                            self.date_jump_year += 1;
                        } else {
                            self.date_jump_month += 1;
                        }
                    }
                });
                let total_days = rg_core::engine::days_in_month(self.date_jump_year, self.date_jump_month);
                // День недели первого числа месяца (1970-01-01 был четверг -> +3 mod 7 для Пн=0).
                let first_day_num = rg_core::engine::days_from_civil(
                    i64::from(self.date_jump_year),
                    i64::from(self.date_jump_month),
                    1,
                );
                let start_wd = ((first_day_num + 3).rem_euclid(7)) as u32;
                egui::Grid::new("date-jump-cal").spacing(Vec2::splat(4.0)).show(ui, |ui| {
                    for wd in ["Пн", "Вт", "Ср", "Чт", "Пт", "Сб", "Вс"] {
                        ui.label(RichText::new(wd).size(11.5).color(SUBTEXT));
                    }
                    ui.end_row();
                    let mut col = 0u32;
                    for _ in 0..start_wd {
                        ui.label("");
                        col += 1;
                    }
                    for day in 1..=total_days {
                        if ui.add_sized([34.0, 26.0], egui::Button::new(day.to_string())).clicked() {
                            let d = rg_core::engine::days_from_civil(
                                i64::from(self.date_jump_year),
                                i64::from(self.date_jump_month),
                                i64::from(day),
                            );
                            if let Ok(ts) = i32::try_from(d * 86_400 + 86_399) {
                                target_ts = Some(ts);
                            }
                        }
                        col += 1;
                        if col >= 7 {
                            ui.end_row();
                            col = 0;
                        }
                    }
                });
            });
        self.date_jump_open = open;
        if let Some(ts) = target_ts {
            self.date_jump_open = false;
            self.history_loading = true;
            self.send(Cmd::JumpToDate(sel, ts));
        }
        if ctx.input(|i| i.key_pressed(egui::Key::Escape)) {
            self.date_jump_open = false;
        }
    }
}
