//! Фоновый поток: вся работа с сетью и диском, интерфейс получает только события.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use grammers_client::SignInError;
use grammers_client::client::{LoginToken, PasswordToken};
use rg_ayu::store::MessageStore;
use rg_core::engine::{DialogRow, Engine, MessageRow};
use rg_core::{Config, connect_with_updates};
use rg_settings::Settings;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Starting,
    NeedKeys,
    NeedPhone,
    NeedCode,
    NeedPassword(String),
    Ready,
}

pub enum Cmd {
    RequestCode(String),
    Code(String),
    Password(String),
    Open(usize),
    Send(String),
    Settings(Settings),
}

pub enum Ev {
    Stage(Stage),
    Me(String),
    Dialogs(Vec<DialogRow>),
    History(usize, Vec<MessageRow>),
    Settings(Settings),
    Status(String),
    Error(String),
}

fn home() -> PathBuf {
    let dir = std::env::var("RG_HOME").map_or_else(|_| PathBuf::from(".rg"), PathBuf::from);
    let _ = std::fs::create_dir_all(&dir);
    dir
}

fn load_settings(path: &std::path::Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(s) => Settings::from_json(&s).unwrap_or_default(),
        // Первый запуск: сохранение удалённых и правок включено.
        Err(_) => Settings {
            save_deleted: true,
            save_edit_history: true,
            ..Settings::default()
        },
    }
}

pub fn spawn(cmds: UnboundedReceiver<Cmd>, ev: Sender<Ev>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(run(cmds, ev, ctx));
    });
}

struct Out {
    ev: Sender<Ev>,
    ctx: eframe::egui::Context,
}

impl Out {
    fn send(&self, e: Ev) {
        let _ = self.ev.send(e);
        self.ctx.request_repaint();
    }
    fn err(&self, text: impl std::fmt::Display) {
        self.send(Ev::Error(text.to_string()));
    }
}

async fn run(mut cmds: UnboundedReceiver<Cmd>, ev: Sender<Ev>, ctx: eframe::egui::Context) {
    let out = Out { ev, ctx };
    let home = home();
    let Ok(mut cfg) = Config::from_env() else {
        out.send(Ev::Stage(Stage::NeedKeys));
        return;
    };
    cfg.session_path = home.join("main.session").to_string_lossy().into_owned();

    let connected = match connect_with_updates(&cfg).await {
        Ok(c) => c,
        Err(e) => {
            out.err(format!("Не удалось подключиться: {e}"));
            return;
        }
    };
    let settings_path = home.join("settings.json");
    let settings = load_settings(&settings_path);
    let store = match MessageStore::open(&home.join("messages.db").to_string_lossy()).await {
        Ok(s) => s,
        Err(e) => {
            out.err(format!("Хранилище сообщений: {e}"));
            return;
        }
    };
    let engine = Engine::new(connected.client.clone(), settings.clone(), store);
    let mut updates = Some(connected.updates);
    out.send(Ev::Settings(settings));

    let mut login: Option<LoginToken> = None;
    let mut password: Option<PasswordToken> = None;
    let mut dialogs: Vec<DialogRow> = Vec::new();
    let mut current: Option<usize> = None;

    let authorized = connected.client.is_authorized().await.unwrap_or(false);
    if authorized {
        enter(&engine, &out, &mut updates, &mut dialogs).await;
    } else {
        out.send(Ev::Stage(Stage::NeedPhone));
    }

    while let Some(cmd) = cmds.recv().await {
        match cmd {
            Cmd::RequestCode(phone) => match connected.client.request_login_code(&phone, &cfg.api_hash).await {
                Ok(t) => {
                    login = Some(t);
                    out.send(Ev::Stage(Stage::NeedCode));
                }
                Err(e) => out.err(format!("Код не отправлен: {e}")),
            },
            Cmd::Code(code) => {
                let Some(t) = login.as_ref() else { continue };
                match connected.client.sign_in(t, &code).await {
                    Ok(_) => enter(&engine, &out, &mut updates, &mut dialogs).await,
                    Err(SignInError::PasswordRequired(p)) => {
                        let hint = p.hint().unwrap_or("нет").to_owned();
                        password = Some(p);
                        out.send(Ev::Stage(Stage::NeedPassword(hint)));
                    }
                    Err(SignInError::InvalidCode) => out.err("Неверный код"),
                    Err(e) => out.err(format!("Вход не удался: {e}")),
                }
            }
            Cmd::Password(pw) => {
                let Some(p) = password.take() else { continue };
                match connected.client.check_password(p, pw.trim()).await {
                    Ok(_) => enter(&engine, &out, &mut updates, &mut dialogs).await,
                    Err(SignInError::InvalidPassword(p)) => {
                        let hint = p.hint().unwrap_or("нет").to_owned();
                        password = Some(p);
                        out.send(Ev::Stage(Stage::NeedPassword(hint)));
                        out.err("Неверный пароль");
                    }
                    Err(e) => out.err(format!("Вход не удался: {e}")),
                }
            }
            Cmd::Open(i) => {
                current = Some(i);
                let Some(d) = dialogs.get(i) else { continue };
                match engine.history(d.peer, 80).await {
                    Ok(rows) => {
                        out.send(Ev::History(i, rows));
                        match engine.mark_read(d.peer).await {
                            Ok(true) => out.send(Ev::Status(String::new())),
                            Ok(false) => out.send(Ev::Status("Режим призрака: «прочитано» не отправлено".into())),
                            Err(e) => out.err(e),
                        }
                    }
                    Err(e) => out.err(format!("История: {e}")),
                }
            }
            Cmd::Send(text) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                if let Err(e) = engine.send(d.peer, &text).await {
                    out.err(format!("Не отправлено: {e}"));
                    continue;
                }
                if let Ok(rows) = engine.history(d.peer, 80).await {
                    out.send(Ev::History(i, rows));
                }
            }
            Cmd::Settings(s) => {
                let json = s.to_json();
                engine.update_settings(|cur| *cur = s);
                let _ = std::fs::write(&settings_path, json);
            }
        }
    }
}

/// После входа: прогреваем кэш собеседников, запускаем поток обновлений, показываем чаты.
async fn enter(
    engine: &Engine,
    out: &Out,
    updates: &mut Option<tokio::sync::mpsc::Receiver<grammers_session::updates::UpdatesLike>>,
    dialogs: &mut Vec<DialogRow>,
) {
    if let Ok(me) = engine.client.get_me().await {
        out.send(Ev::Me(me.full_name()));
    }
    match engine.dialogs(200).await {
        Ok(d) => {
            *dialogs = d.clone();
            out.send(Ev::Dialogs(d));
        }
        Err(e) => out.err(format!("Чаты: {e}")),
    }
    if let Some(rx) = updates.take() {
        let eng = engine.clone();
        let ctx = out.ctx.clone();
        tokio::spawn(async move {
            let _ = eng.run_updates(rx, |_| ctx.request_repaint()).await;
        });
    }
    out.send(Ev::Stage(Stage::Ready));
}
