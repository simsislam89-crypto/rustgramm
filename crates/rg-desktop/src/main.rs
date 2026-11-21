//! Rustgramm для ПК: один процесс, один поток интерфейса и один поток с tokio.
//! Данные уходят только на серверы Telegram (см. `rg-net`).

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod worker;

use std::collections::{HashMap, HashSet};
use std::sync::mpsc::{Receiver, Sender, channel};

use eframe::egui;
use rg_core::engine::{DialogRow, MessageRow};
use rg_core::media::{MediaKind, human_size};
use rg_settings::Settings;
use tokio::sync::mpsc::UnboundedSender;

pub use worker::{Cmd, Ev, Stage};

struct App {
    cmd: UnboundedSender<Cmd>,
    ev: Receiver<Ev>,
    stage: Stage,
    me: String,
    phone: String,
    code: String,
    password: String,
    dialogs: Vec<DialogRow>,
    selected: Option<usize>,
    messages: Vec<MessageRow>,
    input: String,
    settings: Settings,
    status: String,
    error: String,
    textures: HashMap<i32, egui::TextureHandle>,
    requested: HashSet<i32>,
    key_id: String,
    key_hash: String,
}

impl App {
    fn new(cmd: UnboundedSender<Cmd>, ev: Receiver<Ev>) -> Self {
        Self {
            cmd,
            ev,
            stage: Stage::Starting,
            me: String::new(),
            phone: String::new(),
            code: String::new(),
            password: String::new(),
            dialogs: Vec::new(),
            selected: None,
            messages: Vec::new(),
            input: String::new(),
            settings: Settings::default(),
            status: String::new(),
            error: String::new(),
            textures: HashMap::new(),
            requested: HashSet::new(),
            key_id: String::new(),
            key_hash: String::new(),
        }
    }

    fn send(&self, c: Cmd) {
        let _ = self.cmd.send(c);
    }

    fn drain_events(&mut self, ctx: &egui::Context) {
        while let Ok(e) = self.ev.try_recv() {
            match e {
                Ev::Stage(s) => {
                    self.stage = s;
                    self.error.clear();
                }
                Ev::Me(n) => self.me = n,
                Ev::Dialogs(d) => self.dialogs = d,
                Ev::History(idx, m) if self.selected == Some(idx) => {
                    // Предпросмотры только для свежих вложений и по одному разу.
                    let want: Vec<i32> = m
                        .iter()
                        .rev()
                        .filter(|r| r.media.as_ref().is_some_and(|x| x.has_preview()))
                        .take(30)
                        .map(|r| r.id)
                        .collect();
                    for id in want {
                        if self.requested.insert(id) {
                            let _ = self.cmd.send(Cmd::Preview(idx, id));
                        }
                    }
                    self.messages = m;
                }
                Ev::Image(idx, id, img) if self.selected == Some(idx) => {
                    let tex = ctx.load_texture(format!("m{id}"), img, egui::TextureOptions::LINEAR);
                    self.textures.insert(id, tex);
                }
                Ev::Image(..) => {}
                Ev::History(..) => {}
                Ev::Settings(s) => self.settings = s,
                Ev::Status(s) => self.status = s,
                Ev::Error(e) => self.error = e,
            }
        }
    }

    fn login_ui(&mut self, ui: &mut egui::Ui) {
        ui.add_space(40.0);
        ui.vertical_centered(|ui| {
            ui.heading("Вход в Telegram");
            ui.add_space(12.0);
            ui.set_max_width(320.0);
            match self.stage.clone() {
                Stage::Starting => {
                    ui.label("Подключение к Telegram…");
                }
                Stage::NeedKeys => {
                    ui.label("Нужны ключи приложения с my.telegram.org (API development tools). Они хранятся только у вас на диске.");
                    ui.label("api_id");
                    ui.add(egui::TextEdit::singleline(&mut self.key_id));
                    ui.label("api_hash");
                    ui.add(egui::TextEdit::singleline(&mut self.key_hash).password(true));
                    if ui.button("Сохранить и продолжить").clicked() {
                        let hash = std::mem::take(&mut self.key_hash);
                        self.send(Cmd::Keys(self.key_id.clone(), hash));
                    }
                }
                Stage::NeedPhone => {
                    ui.label("Номер телефона в международном формате");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.phone).hint_text("+1…"));
                    let go = ui.button("Получить код").clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go && !self.phone.trim().is_empty() {
                        self.send(Cmd::RequestCode(self.phone.trim().to_owned()));
                    }
                }
                Stage::NeedCode => {
                    ui.label("Код из Telegram");
                    let r = ui.add(egui::TextEdit::singleline(&mut self.code));
                    let go = ui.button("Войти").clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go && !self.code.trim().is_empty() {
                        self.send(Cmd::Code(self.code.trim().to_owned()));
                        self.code.clear();
                    }
                }
                Stage::NeedPassword(hint) => {
                    ui.label(format!("Облачный пароль (подсказка: {hint})"));
                    let r = ui.add(egui::TextEdit::singleline(&mut self.password).password(true));
                    let go = ui.button("Продолжить").clicked()
                        || (r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter)));
                    if go && !self.password.is_empty() {
                        let pw = std::mem::take(&mut self.password);
                        self.send(Cmd::Password(pw));
                    }
                }
                Stage::Ready => {}
            }
            if !self.error.is_empty() {
                ui.add_space(8.0);
                ui.colored_label(egui::Color32::from_rgb(220, 60, 60), &self.error);
            }
        });
    }

    fn settings_bar(&mut self, ui: &mut egui::Ui) {
        let mut s = self.settings.clone();
        ui.horizontal_wrapped(|ui| {
            ui.strong("Rustgramm");
            ui.label(&self.me);
            ui.separator();
            let mut ghost = s.ghost.active;
            if ui.checkbox(&mut ghost, "Призрак").changed() {
                s.ghost.set_active(ghost);
            }
            ui.checkbox(&mut s.save_deleted, "Сохранять удалённые");
            ui.checkbox(&mut s.save_edit_history, "История правок");
            let mut silent = s.ghost.should_send_without_sound();
            if ui.checkbox(&mut silent, "Без звука").changed() {
                s.ghost.send_without_sound = if silent {
                    rg_settings::SendWithoutSound::Always
                } else {
                    rg_settings::SendWithoutSound::Never
                };
            }
            ui.separator();
            ui.weak("Сервер: только Telegram");
        });
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(s));
        }
    }

    fn chat_ui(&mut self, ui: &mut egui::Ui) {
        egui::Panel::left("dialogs").default_size(260.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                for (i, d) in self.dialogs.iter().enumerate() {
                    let sel = self.selected == Some(i);
                    let resp = ui
                        .selectable_label(sel, egui::RichText::new(&d.name).strong())
                        .on_hover_text(&d.last);
                    ui.weak(&d.last);
                    ui.add_space(4.0);
                    if resp.clicked() {
                        self.selected = Some(i);
                        self.messages.clear();
                        self.textures.clear();
                        self.requested.clear();
                        let _ = self.cmd.send(Cmd::Open(i));
                    }
                }
            });
        });
        egui::Panel::bottom("composer").show(ui, |ui| {
            ui.horizontal(|ui| {
                let r = ui.add_sized(
                    [ui.available_width() - 80.0, 28.0],
                    egui::TextEdit::singleline(&mut self.input).hint_text("Сообщение"),
                );
                let enter = r.lost_focus() && ui.input(|i| i.key_pressed(egui::Key::Enter));
                if (ui.button("Отправить").clicked() || enter)
                    && !self.input.trim().is_empty()
                    && self.selected.is_some()
                {
                    let text = std::mem::take(&mut self.input);
                    self.send(Cmd::Send(text));
                    r.request_focus();
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            if self.selected.is_none() {
                ui.centered_and_justified(|ui| {
                    ui.weak("Выберите чат слева");
                });
                return;
            }
            let mut download: Option<i32> = None;
            egui::ScrollArea::vertical().stick_to_bottom(true).show(ui, |ui| {
                for m in &self.messages {
                    let layout = if m.outgoing {
                        egui::Layout::right_to_left(egui::Align::TOP)
                    } else {
                        egui::Layout::left_to_right(egui::Align::TOP)
                    };
                    ui.with_layout(layout, |ui| {
                        egui::Frame::group(ui.style()).show(ui, |ui| {
                            ui.set_max_width(ui.available_width() * 0.8);
                            if m.deleted {
                                ui.colored_label(egui::Color32::from_rgb(220, 60, 60), "удалено");
                            }
                            if let Some(tex) = self.textures.get(&m.id) {
                                ui.add(egui::Image::new(tex).max_width(320.0));
                            }
                            if let Some(info) = &m.media {
                                if !self.textures.contains_key(&m.id) {
                                    let size = if info.size > 0 {
                                        format!(" · {}", human_size(info.size))
                                    } else {
                                        String::new()
                                    };
                                    ui.label(format!("[{}{size}]", info.name));
                                }
                                let can_save = matches!(
                                    info.kind,
                                    MediaKind::Photo | MediaKind::Sticker | MediaKind::File
                                );
                                if can_save && ui.small_button("Скачать").clicked() {
                                    download = Some(m.id);
                                }
                            }
                            if !m.text.is_empty() {
                                ui.label(&m.text);
                            }
                            if m.edits > 0 {
                                ui.weak(format!("изменено: {}", m.edits));
                            }
                        });
                    });
                }
            });
            if let Some(id) = download {
                self.send(Cmd::Download(id));
            }
        });
    }
}

impl eframe::App for App {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        self.drain_events(&ctx);
        if self.stage != Stage::Ready {
            self.login_ui(ui);
            return;
        }
        egui::Panel::top("bar").show(ui, |ui| self.settings_bar(ui));
        if !self.status.is_empty() {
            egui::Panel::bottom("status").show(ui, |ui| {
                ui.weak(&self.status);
            });
        }
        self.chat_ui(ui);
    }
}

fn main() -> eframe::Result {
    let (cmd_tx, cmd_rx) = tokio::sync::mpsc::unbounded_channel();
    let (ev_tx, ev_rx): (Sender<Ev>, Receiver<Ev>) = channel();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([1000.0, 680.0])
            .with_title("Rustgramm"),
        ..Default::default()
    };
    eframe::run_native(
        "Rustgramm",
        options,
        Box::new(move |cc| {
            worker::spawn(cmd_rx, ev_tx, cc.egui_ctx.clone());
            Ok(Box::new(App::new(cmd_tx, ev_rx)))
        }),
    )
}
