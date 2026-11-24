//! Настройки exteraGram (основа Android-версии AyuGram), ветка `exteraSquad/exteraGram`,
//! файл `ExteraConfig.java`. Не перенесены: `useGoogleCrashlytics`, `useGoogleAnalytics`,
//! `checkUpdatesOnLaunch` (телеметрия и проверка обновлений) и признаки сборки.
//! Поля используются мобильным интерфейсом; на ПК часть из них не имеет смысла.

use serde::{Deserialize, Serialize};

fn yes() -> bool {
    true
}

fn one() -> i32 {
    1
}

fn default_quick_reaction() -> String {
    "👍".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct ExteraSettings {
    // --- Внешний вид ---
    pub avatar_corners: i32,
    pub hide_action_bar_status: bool,
    pub hide_all_chats: bool,
    pub center_title: bool,
    pub tab_counter: bool,
    pub tab_icons: i32,
    pub tab_style: i32,
    pub title_text: i32,
    pub use_solar_icons: bool,
    pub square_fab: bool,
    pub force_blur: bool,
    pub force_snow: bool,
    pub use_system_fonts: bool,
    pub new_switch_style: bool,
    pub disable_dividers: bool,
    pub use_l_navigation: bool,
    pub event_type: i32,
    pub alternative_open_animation: bool,
    pub change_status: bool,
    // --- Боковое меню ---
    pub new_group: bool,
    pub new_secret_chat: bool,
    pub new_channel: bool,
    pub contacts: bool,
    pub calls: bool,
    pub people_nearby: bool,
    pub archived_chats: bool,
    pub saved_messages: bool,
    pub scan_qr: bool,
    // --- Общие ---
    pub disable_number_rounding: bool,
    pub format_time_with_seconds: bool,
    pub tablet_mode: i32,
    pub download_speed_boost: i32,
    pub upload_speed_boost: bool,
    pub hide_phone_number: bool,
    pub show_id_and_dc: i32,
    pub archive_on_pull: bool,
    pub disable_unarchive_swipe: bool,
    // --- Стикеры ---
    pub sticker_size: i32,
    pub sticker_shape: i32,
    pub hide_sticker_time: bool,
    pub unlimited_recent_stickers: bool,
    pub hide_categories: bool,
    // --- Чаты ---
    pub double_tap_action: i32,
    pub double_tap_action_out_owner: i32,
    pub double_tap_seek_duration: i32,
    pub bottom_button: i32,
    pub hide_keyboard_on_scroll: bool,
    pub permissions_shortcut: bool,
    pub administrators_shortcut: bool,
    pub members_shortcut: bool,
    pub recent_actions_shortcut: bool,
    pub disable_jump_to_next_channel: bool,
    pub show_action_timestamps: bool,
    pub hide_share_button: bool,
    pub show_copy_photo_button: bool,
    pub show_clear_button: bool,
    pub show_save_message_button: bool,
    pub show_details_button: bool,
    pub show_report_button: bool,
    pub show_history_button: bool,
    pub add_comma_after_mention: bool,
    // --- Камера и медиа ---
    pub send_photos_quality: i32,
    pub hide_camera_tile: bool,
    pub hide_photo_counter: bool,
    pub static_zoom: bool,
    pub video_messages_camera: i32,
    pub remember_last_used_camera: bool,
    pub camera_type: i32,
    pub camera_resolution: i32,
    pub use_camera_x_optimized_mode: bool,
    pub pause_on_minimize: bool,
    pub disable_playback: bool,
    /// Язык перевода (код).
    pub target_language: String,
    /// Быстрая реакция по умолчанию для двойного нажатия.
    #[serde(default = "default_quick_reaction")]
    pub default_reaction: String,
}

impl Default for ExteraSettings {
    fn default() -> Self {
        Self {
            avatar_corners: 30,
            hide_action_bar_status: false,
            hide_all_chats: false,
            center_title: false,
            tab_counter: yes(),
            tab_icons: one(),
            tab_style: one(),
            title_text: 0,
            use_solar_icons: yes(),
            square_fab: yes(),
            force_blur: false,
            force_snow: false,
            use_system_fonts: yes(),
            new_switch_style: yes(),
            disable_dividers: false,
            use_l_navigation: false,
            event_type: 0,
            alternative_open_animation: yes(),
            change_status: yes(),
            new_group: yes(),
            new_secret_chat: false,
            new_channel: false,
            contacts: yes(),
            calls: yes(),
            people_nearby: false,
            archived_chats: yes(),
            saved_messages: yes(),
            scan_qr: yes(),
            disable_number_rounding: false,
            format_time_with_seconds: false,
            tablet_mode: 0,
            download_speed_boost: 0,
            upload_speed_boost: false,
            hide_phone_number: false,
            show_id_and_dc: one(),
            archive_on_pull: false,
            disable_unarchive_swipe: yes(),
            sticker_size: 14,
            sticker_shape: one(),
            hide_sticker_time: false,
            unlimited_recent_stickers: false,
            hide_categories: yes(),
            double_tap_action: one(),
            double_tap_action_out_owner: one(),
            double_tap_seek_duration: one(),
            bottom_button: 2,
            hide_keyboard_on_scroll: yes(),
            permissions_shortcut: false,
            administrators_shortcut: false,
            members_shortcut: false,
            recent_actions_shortcut: yes(),
            disable_jump_to_next_channel: false,
            show_action_timestamps: yes(),
            hide_share_button: yes(),
            show_copy_photo_button: yes(),
            show_clear_button: yes(),
            show_save_message_button: false,
            show_details_button: false,
            show_report_button: yes(),
            show_history_button: false,
            add_comma_after_mention: yes(),
            send_photos_quality: one(),
            hide_camera_tile: false,
            hide_photo_counter: false,
            static_zoom: false,
            video_messages_camera: 0,
            remember_last_used_camera: false,
            camera_type: one(),
            camera_resolution: 0,
            use_camera_x_optimized_mode: false,
            pause_on_minimize: yes(),
            disable_playback: yes(),
            target_language: "en".to_owned(),
            default_reaction: default_quick_reaction(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_match_exteragram() {
        let e = ExteraSettings::default();
        assert_eq!(e.avatar_corners, 30);
        assert!(e.tab_counter && e.square_fab && e.recent_actions_shortcut);
        assert!(!e.hide_all_chats && !e.upload_speed_boost);
        assert_eq!(e.bottom_button, 2);
    }

    #[test]
    fn partial_json_loads() {
        let e: ExteraSettings = serde_json::from_str(r#"{"center_title":true}"#).unwrap();
        assert!(e.center_title && e.tab_counter);
    }
}
