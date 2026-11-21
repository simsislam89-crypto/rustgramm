//! Настройки Rustgramm. Порт `GhostModeAccountSettings` из AyuGram Desktop 7.2.9.
//! Телеметрии, значков «разработчиков» и удалённой конфигурации здесь нет.
//! Старый/неполный файл читается: все поля `serde(default)`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SendWithoutSound {
    #[default]
    Never,
    InGhostMode,
    Always,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum TranslationProvider {
    /// Встроенный перевод Telegram (серверы Telegram).
    #[default]
    Telegram,
    /// Сторонний сервис: только после явного разрешения хоста пользователем.
    External,
}

fn yes() -> bool {
    true
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GhostMode {
    pub active: bool,
    pub send_read_messages: bool,
    pub send_read_stories: bool,
    pub send_online_packets: bool,
    pub send_upload_progress: bool,
    pub send_offline_after_online: bool,
    pub mark_read_after_action: bool,
    pub use_scheduled_messages: bool,
    pub send_without_sound: SendWithoutSound,
    /// Параметры, которые пользователь закрепил: режим призрака их не меняет.
    pub locked_read_messages: bool,
    pub locked_read_stories: bool,
    pub locked_online_packets: bool,
    pub locked_upload_progress: bool,
    pub locked_offline_after_online: bool,
}

impl Default for GhostMode {
    fn default() -> Self {
        Self {
            active: false,
            send_read_messages: yes(),
            send_read_stories: yes(),
            send_online_packets: yes(),
            send_upload_progress: yes(),
            send_offline_after_online: false,
            mark_read_after_action: yes(),
            use_scheduled_messages: false,
            send_without_sound: SendWithoutSound::Never,
            locked_read_messages: false,
            locked_read_stories: false,
            locked_online_packets: false,
            locked_upload_progress: false,
            locked_offline_after_online: false,
        }
    }
}

impl GhostMode {
    /// Включить/выключить режим призрака; закреплённые параметры не трогаем.
    pub fn set_active(&mut self, on: bool) {
        self.active = on;
        let v = !on;
        if !self.locked_read_messages {
            self.send_read_messages = v;
        }
        if !self.locked_read_stories {
            self.send_read_stories = v;
        }
        if !self.locked_online_packets {
            self.send_online_packets = v;
        }
        if !self.locked_upload_progress {
            self.send_upload_progress = v;
        }
        if !self.locked_offline_after_online {
            self.send_offline_after_online = on;
        }
    }

    pub fn should_send_without_sound(&self) -> bool {
        match self.send_without_sound {
            SendWithoutSound::Never => false,
            SendWithoutSound::InGhostMode => self.active,
            SendWithoutSound::Always => true,
        }
    }

    pub fn uses_scheduled_messages(&self) -> bool {
        self.active && self.use_scheduled_messages
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct Settings {
    pub ghost: GhostMode,
    pub translation: TranslationProvider,
    /// Хосты, разрешённые пользователем вручную (виден в интерфейсе).
    pub user_hosts: Vec<String>,
    /// Сохранять удалённые и изменённые сообщения локально.
    pub save_deleted: bool,
    pub save_edit_history: bool,
}

impl Settings {
    /// Настройки первого запуска: максимум приватности. Режим призрака включён (Telegram не
    /// получает «прочитано», «в сети» и прогресс загрузки), удалённые сообщения и правки
    /// сохраняются только на этом устройстве. Отправка сообщения по-прежнему отмечает чат
    /// прочитанным, так что переписка выглядит обычной.
    pub fn first_run() -> Self {
        let mut s = Self {
            save_deleted: true,
            save_edit_history: true,
            ..Self::default()
        };
        s.ghost.set_active(true);
        s
    }

    pub fn from_json(s: &str) -> Result<Self, serde_json::Error> {
        serde_json::from_str(s)
    }

    pub fn to_json(&self) -> String {
        // INFALLIBLE: структура из простых полей всегда сериализуется.
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_send_everything_like_stock_telegram() {
        let s = Settings::default();
        assert!(s.ghost.send_read_messages && s.ghost.send_online_packets);
        assert!(!s.ghost.active);
        assert_eq!(s.translation, TranslationProvider::Telegram);
    }

    #[test]
    fn ghost_mode_silences_and_restores() {
        let mut g = GhostMode::default();
        g.set_active(true);
        assert!(!g.send_read_messages && !g.send_online_packets && g.send_offline_after_online);
        g.set_active(false);
        assert!(g.send_read_messages && g.send_online_packets && !g.send_offline_after_online);
    }

    #[test]
    fn first_run_is_private_by_default() {
        let s = Settings::first_run();
        assert!(s.ghost.active);
        assert!(!s.ghost.send_read_messages);
        assert!(!s.ghost.send_online_packets);
        assert!(!s.ghost.send_upload_progress);
        assert!(!s.ghost.send_read_stories);
        assert!(s.ghost.mark_read_after_action);
        assert!(s.save_deleted && s.save_edit_history);
        assert_eq!(s.translation, TranslationProvider::Telegram);
    }

    #[test]
    fn locked_values_survive_ghost_toggle() {
        let mut g = GhostMode {
            locked_read_messages: true,
            ..GhostMode::default()
        };
        g.set_active(true);
        assert!(g.send_read_messages);
        assert!(!g.send_read_stories);
    }

    #[test]
    fn without_sound_rules() {
        let mut g = GhostMode {
            send_without_sound: SendWithoutSound::InGhostMode,
            ..GhostMode::default()
        };
        assert!(!g.should_send_without_sound());
        g.set_active(true);
        assert!(g.should_send_without_sound());
        g.send_without_sound = SendWithoutSound::Always;
        g.set_active(false);
        assert!(g.should_send_without_sound());
    }

    #[test]
    fn old_partial_json_loads() {
        let s = Settings::from_json(r#"{"ghost":{"active":true}}"#).unwrap();
        assert!(s.ghost.active);
        assert!(s.ghost.send_read_stories);
        assert!(Settings::from_json("{}").is_ok());
    }

    #[test]
    fn json_roundtrip() {
        let mut s = Settings::default();
        s.ghost.set_active(true);
        s.save_deleted = true;
        assert_eq!(Settings::from_json(&s.to_json()).unwrap(), s);
    }
}
