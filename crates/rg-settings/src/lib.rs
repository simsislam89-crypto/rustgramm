//! Настройки Rustgramm. Порт `GhostModeAccountSettings` из AyuGram Desktop 7.2.9.
//! Телеметрии, значков «разработчиков» и удалённой конфигурации здесь нет.
//! Старый/неполный файл читается: все поля `serde(default)`.

use serde::{Deserialize, Serialize};

pub mod ayu;
pub mod extera;
pub use ayu::*;
pub use extera::ExteraSettings;

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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct NotifySettings {
    /// Показывать уведомления на рабочем столе.
    pub desktop_notifications: bool,
    /// Мигать иконкой на панели задач при новом сообщении.
    pub flash_taskbar: bool,
    /// Воспроизводить звук уведомления.
    pub allow_sound: bool,
    /// Громкость звука (0..=100).
    pub volume: u8,
    /// Показывать имя отправителя в уведомлении.
    pub show_name: bool,
    /// Показывать текст сообщения в уведомлении.
    pub show_text: bool,
    /// Уведомления о закреплённых сообщениях.
    pub pinned_messages: bool,
    /// Принимать звонки на этом устройстве.
    pub accept_calls: bool,
    /// Учитывать чаты без звука в счётчике непрочитанных.
    pub badge_include_muted: bool,
    /// Считать непрочитанные сообщения (иначе — количество чатов).
    pub badge_count_messages: bool,
    /// Использовать системные toast-уведомления Windows (иначе — встроенные карточки Rustgramm).
    pub use_windows_notifications: bool,
    /// Учитывать системный режим «Не беспокоить».
    pub respect_focus_mode: bool,
    /// Угол экрана для уведомлений: 0 — верх-лево, 1 — верх-право, 2 — низ-лево, 3 — низ-право.
    pub corner: u8,
    /// Максимум одновременных уведомлений на экране (1..=5).
    pub max_count: u8,
}

impl Default for NotifySettings {
    fn default() -> Self {
        Self {
            desktop_notifications: true,
            flash_taskbar: true,
            allow_sound: true,
            volume: 100,
            show_name: true,
            show_text: true,
            pinned_messages: true,
            accept_calls: true,
            badge_include_muted: false,
            badge_count_messages: true,
            use_windows_notifications: false,
            respect_focus_mode: true,
            corner: 3,
            max_count: 3,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct LanguageSettings {
    /// Код языка интерфейса (`"ru"`, `"en"`, …).
    pub interface_lang: String,
    /// Показывать пункт «Перевести» в контекстном меню сообщений.
    pub show_translate_button: bool,
    /// Показывать кнопку перевода всего чата в шапке.
    pub translate_entire_chats: bool,
    /// Языки, которые не нужно переводить (ISO 639-1).
    pub do_not_translate: Vec<String>,
    /// Целевой язык встроенного переводчика Telegram.
    pub translate_to: String,
}

impl Default for LanguageSettings {
    fn default() -> Self {
        Self {
            interface_lang: "ru".to_owned(),
            show_translate_button: true,
            translate_entire_chats: false,
            do_not_translate: vec!["ru".to_owned()],
            translate_to: "ru".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct SecuritySettings {
    /// Хеш локального код-пароля (пусто — код-пароль отключён).
    pub passcode_hash: String,
    /// Автоблокировка при бездействии, минут (0 — только вручную).
    pub auto_lock_mins: u16,
    /// Показывать частые контакты вверху поиска.
    pub suggest_frequent_contacts: bool,
}

impl Default for SecuritySettings {
    fn default() -> Self {
        Self {
            passcode_hash: String::new(),
            auto_lock_mins: 0,
            suggest_frequent_contacts: true,
        }
    }
}

/// Устойчивый солёный 128-битный хеш локального код-пароля (без внешних крейтов).
pub fn hash_passcode(pin: &str) -> String {
    let mut h1: u64 = 0xcbf2_9ce4_8422_2325;
    let mut h2: u64 = 0x8422_2325_cbf2_9ce4;
    for round in 0u64..1024 {
        for &b in b"rustgramm-local-passcode-v1:" {
            h1 ^= u64::from(b) ^ round;
            h1 = h1.wrapping_mul(0x0100_0000_01b3);
        }
        for &b in pin.as_bytes() {
            h1 ^= u64::from(b);
            h1 = h1.wrapping_mul(0x0100_0000_01b3);
            h2 = h2.wrapping_mul(0x9e37_79b9_7f4a_7c15) ^ (u64::from(b) + round);
        }
    }
    format!("{h1:016x}{h2:016x}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub ghost: GhostMode,
    pub translation: TranslationProvider,
    /// Хосты, разрешённые пользователем вручную (виден в интерфейсе).
    pub user_hosts: Vec<String>,
    /// Остальные настройки AyuGram (внешний вид, фильтры, меню, поле ввода).
    pub ayu: AyuSettings,
    /// Настройки exteraGram (основа Android-версии).
    pub extera: ExteraSettings,
    /// Включена ли система плагинов (полная версия для телефона и ПК).
    pub plugins_enabled: bool,
    /// Идентификаторы отключённых пользователем плагинов.
    pub disabled_plugins: Vec<String>,
    /// Фильтры сообщений.
    pub filters: Vec<Filter>,
    /// Настройки уведомлений и звуков.
    pub notify: NotifySettings,
    /// Настройки языка и встроенного переводчика.
    pub language: LanguageSettings,
    /// Локальная безопасность (код-пароль, частые контакты).
    pub security: SecuritySettings,
    /// Непрозрачность окна, проценты (15..=100); по умолчанию 100 (непрозрачное на старте).
    pub window_opacity: u8,
    /// Размытие стекла (Windows 11 Acrylic при прозрачности < 100% и блюр-анимации в стиле iOS).
    pub blur_enabled: bool,
    /// Стиль фона чата: 0 — тёмный, 1 — мягкий градиент, 2 — тонкий узор.
    pub chat_bg_style: u8,
    /// Автоматическая подгрузка и воспроизведение GIF в ленте без звука.
    pub auto_play_gifs: bool,
    /// Недавно выбранные эмодзи (строка символов, новые впереди).
    pub recent_emoji: String,
    /// Скорость анимаций, проценты (0 — без анимаций, 100 — как в Telegram, до 200).
    pub anim_speed: u8,
    /// Масштаб интерфейса по умолчанию (100%).
    pub default_scale: bool,
    /// Масштаб интерфейса, проценты (75..=200).
    pub ui_scale: u8,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            ghost: GhostMode::default(),
            translation: TranslationProvider::default(),
            user_hosts: Vec::new(),
            ayu: AyuSettings::default(),
            extera: ExteraSettings::default(),
            plugins_enabled: true,
            disabled_plugins: vec!["extera.typography".to_owned()],
            filters: Vec::new(),
            notify: NotifySettings::default(),
            language: LanguageSettings::default(),
            security: SecuritySettings::default(),
            window_opacity: 100,
            blur_enabled: true,
            chat_bg_style: 0,
            auto_play_gifs: true,
            recent_emoji: String::new(),
            anim_speed: 100,
            default_scale: true,
            ui_scale: 100,
        }
    }
}

impl Settings {
    /// Настройки первого запуска: максимум приватности. Режим призрака включён (Telegram не
    /// получает «прочитано», «в сети» и прогресс загрузки), удалённые сообщения и правки
    /// сохраняются только на этом устройстве. Отправка сообщения по-прежнему отмечает чат
    /// прочитанным, так что переписка выглядит обычной.
    pub fn first_run() -> Self {
        let mut s = Self::default();
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
        assert!(s.ayu.save_deleted_messages && s.ayu.save_messages_history);
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
    fn opacity_defaults_to_100_and_survives_old_files() {
        let def = Settings::default();
        assert_eq!(def.window_opacity, 100);
        assert!(def.blur_enabled);
        assert!(def.auto_play_gifs);
        assert_eq!(def.chat_bg_style, 0);
        let from_empty = Settings::from_json("{}").unwrap();
        assert_eq!(from_empty.window_opacity, 100);
        assert!(from_empty.blur_enabled);
        assert_eq!(Settings::from_json(r#"{"window_opacity":40}"#).unwrap().window_opacity, 40);
    }

    #[test]
    fn json_roundtrip() {
        let mut s = Settings::default();
        s.ghost.set_active(true);
        s.ayu.save_deleted_messages = false;
        s.notify.corner = 1;
        s.notify.max_count = 4;
        s.language.translate_entire_chats = true;
        s.security.passcode_hash = hash_passcode("1234");
        assert_eq!(Settings::from_json(&s.to_json()).unwrap(), s);
    }

    #[test]
    fn passcode_hash_is_stable_and_distinct() {
        assert_eq!(hash_passcode("4242"), hash_passcode("4242"));
        assert_ne!(hash_passcode("4242"), hash_passcode("4243"));
        assert_eq!(hash_passcode("0000").len(), 32);
    }
}
