//! Полный набор настроек AyuGram Desktop 7.2.9 (`ayu_settings.h`), кроме отправки отчётов
//! о сбоях: её в Rustgramm нет. Имена полей — те же, что в оригинале, в `snake_case`.
//! Все поля `serde(default)`: старый или неполный файл читается без ошибок.

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

fn percent() -> i32 {
    100
}

fn bubble_radius() -> i32 {
    16
}

fn avatar_corners() -> i32 {
    23
}

fn deleted_mark() -> String {
    "🧹".to_owned()
}

/// Показ пункта контекстного меню сообщения.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum MenuVisibility {
    Hidden,
    #[default]
    Visible,
    /// Только при зажатом Shift.
    WithModifier,
}

impl MenuVisibility {
    /// Показывать ли пункт при текущем состоянии клавиши-модификатора.
    pub fn shown(self, modifier: bool) -> bool {
        match self {
            Self::Hidden => false,
            Self::Visible => true,
            Self::WithModifier => modifier,
        }
    }
}

/// Нижняя кнопка канала.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum ChannelBottomButton {
    Hidden,
    Mute,
    Discuss,
    #[default]
    DiscussWithFallback,
}

/// Как показывать идентификатор пользователя или чата.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum PeerIdDisplay {
    Hidden,
    #[default]
    BotApi,
    Telegram,
}

/// Правило фильтра сообщений (обработка — `rg-ayu::filters`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct Filter {
    pub text: String,
    pub is_regex: bool,
    pub case_sensitive: bool,
    pub enabled: bool,
    /// Чаты (формат Bot API), где правило не действует.
    pub excluded_chats: Vec<i64>,
}

impl Default for Filter {
    fn default() -> Self {
        Self {
            text: String::new(),
            is_regex: false,
            case_sensitive: false,
            enabled: true,
            excluded_chats: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct AyuSettings {
    // --- Внешний вид ---
    pub show_background: bool,
    pub show_date: bool,
    pub show_reactions: bool,
    pub show_header_decorations: bool,
    pub show_colorful_replies: bool,
    pub reveal_spoilers: bool,
    pub semi_transparent_deleted_messages: bool,
    pub disable_custom_backgrounds: bool,
    /// Радиус углов пузыря сообщения, px.
    pub message_bubble_radius: i32,
    /// Множитель ширины пузыря, проценты (100 = как в Telegram).
    pub wide_multiplier_pct: i32,
    pub material_switches: bool,
    pub remove_message_tail: bool,
    pub avatar_corners: i32,
    pub single_corner_radius: bool,
    pub simple_quotes_and_replies: bool,
    pub replace_bottom_info_with_icons: bool,
    pub adaptive_cover_color: bool,
    pub improve_link_previews: bool,
    pub mono_font: String,
    pub deleted_mark: String,
    pub edited_mark: String,
    pub show_message_seconds: bool,
    pub show_peer_id: PeerIdDisplay,
    // --- Сохранение ---
    pub save_deleted_messages: bool,
    pub save_messages_history: bool,
    pub save_for_bots: bool,
    // --- Фильтры ---
    pub filters_enabled: bool,
    pub filters_enabled_in_chats: bool,
    pub hide_from_blocked: bool,
    pub filter_zalgo: bool,
    // --- Чистка интерфейса ---
    pub disable_ads: bool,
    pub disable_stories: bool,
    pub hide_premium_statuses: bool,
    pub show_only_added_emojis_and_stickers: bool,
    pub collapse_similar_channels: bool,
    pub hide_similar_channels: bool,
    pub hide_notification_counters: bool,
    pub hide_notification_badge: bool,
    pub hide_all_chats_folder: bool,
    pub disable_greeting_sticker: bool,
    pub hide_fast_share: bool,
    pub unlimited_recent_stickers: bool,
    pub disable_notifications_delay: bool,
    pub disable_open_link_warning: bool,
    // --- Веб-приложения ---
    pub spoof_webview_as_android: bool,
    pub increase_webview_height: bool,
    pub increase_webview_width: bool,
    // --- Премиум и реакции ---
    pub local_premium: bool,
    pub show_channel_reactions: bool,
    pub show_group_reactions: bool,
    pub show_private_chat_reactions: bool,
    // --- Контекстное меню сообщения ---
    pub menu_reactions_panel: MenuVisibility,
    pub menu_views_panel: MenuVisibility,
    pub menu_hide_message: MenuVisibility,
    pub menu_user_messages: MenuVisibility,
    pub menu_message_details: MenuVisibility,
    pub menu_repeat_message: MenuVisibility,
    pub menu_add_filter: MenuVisibility,
    // --- Поле ввода ---
    pub field_attach_button: bool,
    pub field_commands_button: bool,
    pub field_emoji_button: bool,
    pub field_microphone_button: bool,
    pub field_auto_delete_button: bool,
    pub field_gift_button: bool,
    pub field_ai_editor_button: bool,
    pub show_attach_popup: bool,
    pub show_emoji_popup: bool,
    pub sticker_confirmation: bool,
    pub gif_confirmation: bool,
    pub voice_confirmation: bool,
    pub round_confirmation: bool,
    // --- Боковое меню ---
    pub drawer_my_profile: bool,
    pub drawer_bots: bool,
    pub drawer_new_group: bool,
    pub drawer_new_channel: bool,
    pub drawer_contacts: bool,
    pub drawer_calls: bool,
    pub drawer_saved_messages: bool,
    pub drawer_l_read_toggle: bool,
    pub drawer_s_read_toggle: bool,
    pub drawer_night_mode_toggle: bool,
    pub drawer_ghost_toggle: bool,
    pub drawer_streamer_toggle: bool,
    pub tray_ghost_toggle: bool,
    pub tray_streamer_toggle: bool,
    // --- Прочее ---
    pub channel_bottom_button: ChannelBottomButton,
    pub quick_admin_shortcuts: bool,
    pub show_message_shot: bool,
    pub streamer_mode: bool,
    pub use_global_ghost_mode: bool,
    pub suggest_ghost_mode_before_viewing_story: bool,
    pub app_icon: String,
    /// Пользователи, чьи сообщения скрыты (Shadow Ban), идентификаторы Bot API.
    pub shadow_ban_ids: Vec<i64>,
}

impl Default for AyuSettings {
    fn default() -> Self {
        Self {
            show_background: yes(),
            show_date: false,
            show_reactions: false,
            show_header_decorations: yes(),
            show_colorful_replies: yes(),
            reveal_spoilers: yes(),
            semi_transparent_deleted_messages: false,
            disable_custom_backgrounds: false,
            message_bubble_radius: bubble_radius(),
            wide_multiplier_pct: percent(),
            material_switches: yes(),
            remove_message_tail: false,
            avatar_corners: avatar_corners(),
            single_corner_radius: false,
            simple_quotes_and_replies: false,
            replace_bottom_info_with_icons: yes(),
            adaptive_cover_color: yes(),
            improve_link_previews: false,
            mono_font: String::new(),
            deleted_mark: deleted_mark(),
            edited_mark: String::new(),
            show_message_seconds: false,
            show_peer_id: PeerIdDisplay::BotApi,
            save_deleted_messages: yes(),
            save_messages_history: yes(),
            save_for_bots: false,
            filters_enabled: false,
            filters_enabled_in_chats: false,
            hide_from_blocked: false,
            filter_zalgo: false,
            disable_ads: yes(),
            disable_stories: false,
            hide_premium_statuses: false,
            show_only_added_emojis_and_stickers: false,
            collapse_similar_channels: yes(),
            hide_similar_channels: false,
            hide_notification_counters: false,
            hide_notification_badge: false,
            hide_all_chats_folder: false,
            disable_greeting_sticker: false,
            hide_fast_share: false,
            unlimited_recent_stickers: false,
            disable_notifications_delay: false,
            disable_open_link_warning: false,
            spoof_webview_as_android: false,
            increase_webview_height: false,
            increase_webview_width: false,
            local_premium: false,
            show_channel_reactions: yes(),
            show_group_reactions: yes(),
            show_private_chat_reactions: yes(),
            menu_reactions_panel: MenuVisibility::Visible,
            menu_views_panel: MenuVisibility::Visible,
            menu_hide_message: MenuVisibility::Hidden,
            menu_user_messages: MenuVisibility::WithModifier,
            menu_message_details: MenuVisibility::WithModifier,
            menu_repeat_message: MenuVisibility::Hidden,
            menu_add_filter: MenuVisibility::Visible,
            field_attach_button: yes(),
            field_commands_button: yes(),
            field_emoji_button: yes(),
            field_microphone_button: yes(),
            field_auto_delete_button: yes(),
            field_gift_button: yes(),
            field_ai_editor_button: yes(),
            show_attach_popup: yes(),
            show_emoji_popup: yes(),
            sticker_confirmation: false,
            gif_confirmation: false,
            voice_confirmation: false,
            round_confirmation: false,
            drawer_my_profile: yes(),
            drawer_bots: yes(),
            drawer_new_group: yes(),
            drawer_new_channel: yes(),
            drawer_contacts: yes(),
            drawer_calls: yes(),
            drawer_saved_messages: yes(),
            drawer_l_read_toggle: false,
            drawer_s_read_toggle: yes(),
            drawer_night_mode_toggle: yes(),
            drawer_ghost_toggle: yes(),
            drawer_streamer_toggle: false,
            tray_ghost_toggle: yes(),
            tray_streamer_toggle: false,
            channel_bottom_button: ChannelBottomButton::DiscussWithFallback,
            quick_admin_shortcuts: yes(),
            show_message_shot: yes(),
            streamer_mode: false,
            use_global_ghost_mode: yes(),
            suggest_ghost_mode_before_viewing_story: yes(),
            app_icon: String::new(),
            shadow_ban_ids: Vec::new(),
        }
    }
}

impl AyuSettings {
    /// Метка «удалено» с учётом пустого значения (пустая метка — без значка).
    pub fn deleted_label(&self) -> &str {
        &self.deleted_mark
    }

    /// Радиус пузыря в допустимых пределах (как ползунок оригинала: 0..=20).
    pub fn bubble_radius(&self) -> u8 {
        self.message_bubble_radius.clamp(0, 20) as u8
    }

    /// Множитель ширины пузыря 0.5..=2.0.
    pub fn wide_multiplier(&self) -> f32 {
        self.wide_multiplier_pct.clamp(50, 200) as f32 / 100.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_ayugram() {
        let a = AyuSettings::default();
        assert!(a.save_deleted_messages && a.disable_ads && !a.local_premium);
        assert_eq!(a.message_bubble_radius, 16);
        assert_eq!(a.deleted_mark, "🧹");
        assert_eq!(a.menu_message_details, MenuVisibility::WithModifier);
        assert_eq!(a.show_peer_id, PeerIdDisplay::BotApi);
    }

    #[test]
    fn menu_visibility_rules() {
        assert!(!MenuVisibility::Hidden.shown(true));
        assert!(MenuVisibility::Visible.shown(false));
        assert!(!MenuVisibility::WithModifier.shown(false));
        assert!(MenuVisibility::WithModifier.shown(true));
    }

    #[test]
    fn partial_json_keeps_other_defaults() {
        let a: AyuSettings = serde_json::from_str(r#"{"local_premium":true}"#).unwrap();
        assert!(a.local_premium && a.show_background);
    }

    #[test]
    fn limits_are_clamped() {
        let a = AyuSettings {
            message_bubble_radius: 99,
            wide_multiplier_pct: 5,
            ..AyuSettings::default()
        };
        assert_eq!(a.bubble_radius(), 20);
        assert!((a.wide_multiplier() - 0.5).abs() < f32::EPSILON);
    }
}
