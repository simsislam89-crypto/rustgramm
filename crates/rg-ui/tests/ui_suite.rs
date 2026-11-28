//! Полный набор модульных и характеристических тестов `rg-ui`
//! (атлас значков Telegram/AyuGram `icons::native_size`, кривые анимаций `ios_spring`/`spring`/`ease_out`,
//! цветовая математика `lerp_color`/`navy`/`avatar_color`, прозрачность `set_opacity`/`t`/`t_btn`/`t2`,
//! маскировка стримера `mask_streamer`, форматирование дат, HTTP Range-парсер `stream::parse_range`).

use std::path::PathBuf;

use eframe::egui::Color32;
use rg_ui::icons::native_size;
use rg_ui::stream::{StreamState, parse_range};
use rg_ui::theme::{
    DEFAULT_OPACITY_PCT, ORANGE, WINDOW_BG, avatar_color, blur_on, day_label, dialog_date,
    ease_out, full_date, hhmm, hhmmss, ios_spring, lerp_color, mask_streamer, navy, opacity_f32,
    same_day, set_blur, set_opacity, spring, t, t_btn, t2, tg, tint_chat, tint_panel, tint_title,
};

macro_rules! icon_tests {
    ($($name:ident: $ico:expr => ($w:expr, $h:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let sz = native_size($ico).expect(concat!("missing icon: ", $ico));
                assert!(
                    (sz.x - ($w as f32)).abs() < 0.01 && (sz.y - ($h as f32)).abs() < 0.01,
                    "icon {} size {:?} != ({}, {})",
                    $ico,
                    sz,
                    $w,
                    $h
                );
            }
        )+
    };
}

icon_tests! {
    ico_001: "archive_userpic" => (24.0, 24.0),
    ico_002: "calendar_down" => (16.0, 16.0),
    ico_003: "calls/call_answer" => (36.0, 36.0),
    ico_004: "calls/call_arrow_in" => (16.0, 16.0),
    ico_005: "calls/call_arrow_out" => (16.0, 16.0),
    ico_006: "calls/call_camera_active" => (36.0, 36.0),
    ico_007: "calls/call_camera_muted" => (36.0, 36.0),
    ico_008: "calls/call_cancel" => (36.0, 36.0),
    ico_009: "calls/call_discard" => (36.0, 36.0),
    ico_010: "calls/call_ended" => (24.0, 24.0),
    ico_011: "calls/call_group" => (36.0, 36.0),
    ico_012: "calls/call_message" => (36.0, 36.0),
    ico_013: "calls/call_record_active" => (36.0, 36.0),
    ico_014: "calls/call_record_muted" => (36.0, 36.0),
    ico_015: "calls/calls_add_people" => (24.0, 24.0),
    ico_016: "calls/calls_more" => (24.0, 24.0),
    ico_017: "calls/calls_present" => (24.0, 24.0),
    ico_018: "calls/calls_settings" => (24.0, 24.0),
    ico_019: "chat/input_attach" => (40.0, 40.0),
    ico_020: "chat/input_autodelete" => (32.0, 32.0),
    ico_021: "chat/input_bot_command" => (40.0, 40.0),
    ico_022: "chat/input_bot_keyboard" => (40.0, 40.0),
    ico_023: "chat/input_comments_expand" => (24.0, 24.0),
    ico_024: "chat/input_comments_hide" => (24.0, 24.0),
    ico_025: "chat/input_draw" => (24.0, 24.0),
    ico_026: "chat/input_edit" => (32.0, 32.0),
    ico_027: "chat/input_forward" => (32.0, 32.0),
    ico_028: "chat/input_gift" => (24.0, 24.0),
    ico_029: "chat/input_like" => (24.0, 24.0),
    ico_030: "chat/input_liked" => (24.0, 24.0),
    ico_031: "chat/input_record" => (40.0, 40.0),
    ico_032: "chat/input_record_filled" => (40.0, 40.0),
    ico_033: "chat/input_replace" => (24.0, 24.0),
    ico_034: "chat/input_reply_quote" => (32.0, 32.0),
    ico_035: "chat/input_save" => (40.0, 40.0),
    ico_036: "chat/input_scheduled" => (40.0, 40.0),
    ico_037: "chat/input_send" => (24.0, 24.0),
    ico_038: "chat/input_send_round" => (24.0, 24.0),
    ico_039: "chat/input_silent" => (40.0, 40.0),
    ico_040: "chat/input_silent_on" => (40.0, 40.0),
    ico_041: "chat/input_smile_face" => (40.0, 40.0),
    ico_042: "chat/input_video" => (40.0, 40.0),
    ico_043: "chat/mini_copy" => (16.0, 16.0),
    ico_044: "chat/mini_info_alert" => (16.0, 16.0),
    ico_045: "chat/mini_lock" => (12.0, 12.0),
    ico_046: "chat/mini_quote" => (16.0, 12.0),
    ico_047: "chat/reactions_expand_panel" => (24.0, 24.0),
    ico_048: "chat/voice_to_text" => (28.0, 22.0),
    ico_049: "chat/voice_to_text_collapse" => (28.0, 22.0),
    ico_050: "contacts_online" => (24.0, 24.0),
    ico_051: "dialogs/avatar_hidden" => (32.0, 32.0),
    ico_052: "dialogs/avatar_notes" => (32.0, 32.0),
    ico_053: "dialogs/dialogs_bot" => (16.0, 11.0),
    ico_054: "dialogs/dialogs_calendar" => (23.0, 22.0),
    ico_055: "dialogs/dialogs_cancel_search" => (18.0, 18.0),
    ico_056: "dialogs/dialogs_channel" => (12.0, 11.0),
    ico_057: "dialogs/dialogs_chat" => (16.0, 10.0),
    ico_058: "dialogs/dialogs_downloads" => (32.0, 32.0),
    ico_059: "dialogs/dialogs_forum" => (19.0, 10.0),
    ico_060: "dialogs/dialogs_lock_off" => (24.0, 24.0),
    ico_061: "dialogs/dialogs_lock_on" => (24.0, 24.0),
    ico_062: "dialogs/dialogs_mention" => (19.0, 19.0),
    ico_063: "dialogs/dialogs_menu" => (32.0, 32.0),
    ico_064: "dialogs/dialogs_menu_unread" => (32.0, 32.0),
    ico_065: "dialogs/dialogs_menu_unread_dot" => (32.0, 32.0),
    ico_066: "dialogs/dialogs_mini_play" => (16.0, 16.0),
    ico_067: "dialogs/dialogs_mute" => (8.0, 8.0),
    ico_068: "dialogs/dialogs_pinned" => (20.0, 20.0),
    ico_069: "dialogs/dialogs_premium" => (18.0, 16.0),
    ico_070: "dialogs/dialogs_reaction" => (19.0, 19.0),
    ico_071: "dialogs/dialogs_received" => (17.0, 10.0),
    ico_072: "dialogs/dialogs_search_from" => (23.0, 22.0),
    ico_073: "dialogs/dialogs_sending" => (11.0, 11.0),
    ico_074: "dialogs/dialogs_sent" => (12.0, 10.0),
    ico_075: "dialogs/dialogs_topic_arrow" => (8.0, 12.0),
    ico_076: "dialogs/dialogs_verified_check" => (18.0, 16.0),
    ico_077: "dialogs/dialogs_verified_star" => (18.0, 16.0),
    ico_078: "dialogs/mini_arrow" => (16.0, 16.0),
    ico_079: "dialogs/mini_tag_lock" => (16.0, 16.0),
    ico_080: "emoji/emoji_activities" => (24.0, 24.0),
    ico_081: "emoji/emoji_back" => (24.0, 24.0),
    ico_082: "emoji/emoji_delete" => (14.0, 14.0),
    ico_083: "emoji/emoji_food" => (24.0, 24.0),
    ico_084: "emoji/emoji_love" => (24.0, 24.0),
    ico_085: "emoji/emoji_nature" => (24.0, 24.0),
    ico_086: "emoji/emoji_objects" => (24.0, 24.0),
    ico_087: "emoji/emoji_recent" => (24.0, 24.0),
    ico_088: "emoji/emoji_search_input" => (24.0, 24.0),
    ico_089: "emoji/emoji_settings" => (24.0, 24.0),
    ico_090: "emoji/emoji_skin" => (24.0, 24.0),
    ico_091: "emoji/emoji_smile" => (24.0, 24.0),
    ico_092: "emoji/emoji_travel" => (24.0, 24.0),
    ico_093: "emoji/premium_lock" => (19.0, 19.0),
    ico_094: "emoji/stickers_premium" => (32.0, 32.0),
    ico_095: "fast_comments" => (24.0, 24.0),
    ico_096: "fast_share" => (24.0, 24.0),
    ico_097: "fast_to_original" => (16.0, 16.0),
    ico_098: "folders/folders_all" => (36.0, 36.0),
    ico_099: "folders/folders_bots" => (36.0, 36.0),
    ico_100: "folders/folders_channels" => (36.0, 36.0),
    ico_101: "folders/folders_custom" => (36.0, 36.0),
    ico_102: "folders/folders_edit" => (36.0, 36.0),
    ico_103: "folders/folders_favorite" => (36.0, 36.0),
    ico_104: "folders/folders_group" => (36.0, 36.0),
    ico_105: "folders/folders_home" => (36.0, 36.0),
    ico_106: "folders/folders_like" => (36.0, 36.0),
    ico_107: "folders/folders_love" => (36.0, 36.0),
    ico_108: "folders/folders_money" => (36.0, 36.0),
    ico_109: "folders/folders_private" => (36.0, 36.0),
    ico_110: "folders/folders_setup" => (36.0, 36.0),
    ico_111: "folders/folders_study" => (36.0, 36.0),
    ico_112: "folders/folders_trade" => (36.0, 36.0),
    ico_113: "folders/folders_travel" => (36.0, 36.0),
    ico_114: "folders/folders_type_archived" => (36.0, 36.0),
    ico_115: "folders/folders_type_bots" => (36.0, 36.0),
    ico_116: "folders/folders_type_channels" => (36.0, 36.0),
    ico_117: "folders/folders_type_contacts" => (36.0, 36.0),
    ico_118: "folders/folders_type_groups" => (36.0, 36.0),
    ico_119: "folders/folders_type_muted" => (36.0, 36.0),
    ico_120: "folders/folders_type_noncontacts" => (36.0, 36.0),
    ico_121: "folders/folders_type_read" => (36.0, 36.0),
    ico_122: "folders/folders_unmuted" => (36.0, 36.0),
    ico_123: "folders/folders_unread" => (36.0, 36.0),
    ico_124: "folders/folders_work" => (36.0, 36.0),
    ico_125: "history_audio_cancel" => (12.0, 12.0),
    ico_126: "history_audio_download" => (12.0, 12.0),
    ico_127: "history_comments" => (24.0, 24.0),
    ico_128: "history_down_arrow" => (52.0, 52.0),
    ico_129: "history_down_circle" => (52.0, 52.0),
    ico_130: "history_file_cancel" => (32.0, 32.0),
    ico_131: "history_file_document" => (32.0, 32.0),
    ico_132: "history_file_download" => (32.0, 32.0),
    ico_133: "history_file_image" => (32.0, 32.0),
    ico_134: "history_file_pause" => (32.0, 32.0),
    ico_135: "history_file_play" => (32.0, 32.0),
    ico_136: "history_pin" => (18.0, 18.0),
    ico_137: "history_received" => (18.0, 11.0),
    ico_138: "history_replies" => (16.0, 11.0),
    ico_139: "history_sent" => (13.0, 11.0),
    ico_140: "history_unread_mention" => (52.0, 52.0),
    ico_141: "history_unread_reaction" => (52.0, 52.0),
    ico_142: "history_views" => (16.0, 11.0),
    ico_143: "info/info_add_member" => (32.0, 32.0),
    ico_144: "info/info_back" => (32.0, 32.0),
    ico_145: "info/info_block" => (32.0, 32.0),
    ico_146: "info/info_close" => (32.0, 32.0),
    ico_147: "info/info_common_groups" => (32.0, 32.0),
    ico_148: "info/info_delete" => (32.0, 32.0),
    ico_149: "info/info_edit" => (32.0, 32.0),
    ico_150: "info/info_information" => (32.0, 32.0),
    ico_151: "info/info_leave" => (32.0, 32.0),
    ico_152: "info/info_media_audio" => (32.0, 32.0),
    ico_153: "info/info_media_delete" => (24.0, 24.0),
    ico_154: "info/info_media_file" => (32.0, 32.0),
    ico_155: "info/info_media_forward" => (24.0, 24.0),
    ico_156: "info/info_media_gif" => (32.0, 32.0),
    ico_157: "info/info_media_link" => (32.0, 32.0),
    ico_158: "info/info_media_photo" => (32.0, 32.0),
    ico_159: "info/info_media_round" => (32.0, 32.0),
    ico_160: "info/info_media_saved" => (32.0, 32.0),
    ico_161: "info/info_media_video" => (32.0, 32.0),
    ico_162: "info/info_media_voice" => (32.0, 32.0),
    ico_163: "info/info_members" => (32.0, 32.0),
    ico_164: "info/info_notifications" => (32.0, 32.0),
    ico_165: "info/info_permissions" => (32.0, 32.0),
    ico_166: "info/info_report" => (32.0, 32.0),
    ico_167: "info/info_rights_lock" => (16.0, 16.0),
    ico_168: "info/info_search" => (32.0, 32.0),
    ico_169: "info/info_share" => (32.0, 32.0),
    ico_170: "info/info_stories_archive" => (32.0, 32.0),
    ico_171: "inline_button_card" => (10.0, 10.0),
    ico_172: "inline_button_copy" => (10.0, 10.0),
    ico_173: "inline_button_switch" => (10.0, 10.0),
    ico_174: "inline_button_url" => (10.0, 10.0),
    ico_175: "inline_button_web" => (10.0, 10.0),
    ico_176: "install_update" => (32.0, 32.0),
    ico_177: "media_pause" => (17.0, 20.0),
    ico_178: "media_play" => (17.0, 22.0),
    ico_179: "media_video_play" => (36.0, 36.0),
    ico_180: "mediaview/download" => (24.0, 24.0),
    ico_181: "mediaview/draw" => (20.0, 20.0),
    ico_182: "mediaview/more" => (5.0, 21.0),
    ico_183: "mediaview/next" => (24.0, 24.0),
    ico_184: "mediaview/recognize" => (24.0, 24.0),
    ico_185: "mediaview/rotate" => (24.0, 24.0),
    ico_186: "mediaview/save_check" => (22.0, 18.0),
    ico_187: "mediaview/viewer_share" => (24.0, 24.0),
    ico_188: "mediaview/views" => (24.0, 24.0),
    ico_189: "menu/add" => (24.0, 24.0),
    ico_190: "menu/admin" => (24.0, 24.0),
    ico_191: "menu/archive" => (24.0, 24.0),
    ico_192: "menu/auto_delete" => (24.0, 24.0),
    ico_193: "menu/block" => (24.0, 24.0),
    ico_194: "menu/bot" => (24.0, 24.0),
    ico_195: "menu/calendar" => (24.0, 24.0),
    ico_196: "menu/calls_receive" => (24.0, 24.0),
    ico_197: "menu/cancel" => (24.0, 24.0),
    ico_198: "menu/channel" => (24.0, 24.0),
    ico_199: "menu/chat_bubble" => (24.0, 24.0),
    ico_200: "menu/chats" => (24.0, 24.0),
    ico_201: "menu/clear" => (24.0, 24.0),
    ico_202: "menu/colors" => (24.0, 24.0),
    ico_203: "menu/copy" => (24.0, 24.0),
    ico_204: "menu/customize" => (24.0, 24.0),
    ico_205: "menu/delete" => (24.0, 24.0),
    ico_206: "menu/devices" => (24.0, 24.0),
    ico_207: "menu/disable" => (24.0, 24.0),
    ico_208: "menu/discussion" => (24.0, 24.0),
    ico_209: "menu/download" => (24.0, 24.0),
    ico_210: "menu/edit" => (24.0, 24.0),
    ico_211: "menu/edited_status" => (24.0, 24.0),
    ico_212: "menu/emoji" => (24.0, 24.0),
    ico_213: "menu/export" => (24.0, 24.0),
    ico_214: "menu/faq" => (24.0, 24.0),
    ico_215: "menu/favorite" => (24.0, 24.0),
    ico_216: "menu/file" => (24.0, 24.0),
    ico_217: "menu/flip" => (24.0, 24.0),
    ico_218: "menu/fonts" => (24.0, 24.0),
    ico_219: "menu/forward" => (24.0, 24.0),
    ico_220: "menu/forwarded_status" => (24.0, 24.0),
    ico_221: "menu/gif" => (24.0, 24.0),
    ico_222: "menu/group_log" => (24.0, 24.0),
    ico_223: "menu/groups" => (24.0, 24.0),
    ico_224: "menu/groups_create" => (24.0, 24.0),
    ico_225: "menu/hide_members" => (24.0, 24.0),
    ico_226: "menu/image" => (24.0, 24.0),
    ico_227: "menu/info" => (24.0, 24.0),
    ico_228: "menu/info_notifications" => (24.0, 24.0),
    ico_229: "menu/invite" => (24.0, 24.0),
    ico_230: "menu/ip_address" => (24.0, 24.0),
    ico_231: "menu/leave" => (24.0, 24.0),
    ico_232: "menu/link" => (24.0, 24.0),
    ico_233: "menu/lock" => (24.0, 24.0),
    ico_234: "menu/manage" => (24.0, 24.0),
    ico_235: "menu/mute" => (24.0, 24.0),
    ico_236: "menu/network" => (24.0, 24.0),
    ico_237: "menu/new_window" => (24.0, 24.0),
    ico_238: "menu/night_mode" => (24.0, 24.0),
    ico_239: "menu/palette" => (24.0, 24.0),
    ico_240: "menu/passcode_finger" => (24.0, 24.0),
    ico_241: "menu/permissions" => (24.0, 24.0),
    ico_242: "menu/personal" => (24.0, 24.0),
    ico_243: "menu/phone" => (24.0, 24.0),
    ico_244: "menu/photo_set" => (24.0, 24.0),
    ico_245: "menu/pin" => (24.0, 24.0),
    ico_246: "menu/premium" => (24.0, 24.0),
    ico_247: "menu/profile" => (24.0, 24.0),
    ico_248: "menu/qr_code" => (24.0, 24.0),
    ico_249: "menu/read" => (24.0, 24.0),
    ico_250: "menu/read_reactions" => (24.0, 24.0),
    ico_251: "menu/read_ticks" => (24.0, 24.0),
    ico_252: "menu/reply" => (24.0, 24.0),
    ico_253: "menu/report" => (24.0, 24.0),
    ico_254: "menu/restore" => (24.0, 24.0),
    ico_255: "menu/save_image" => (24.0, 24.0),
    ico_256: "menu/saved_messages" => (24.0, 24.0),
    ico_257: "menu/search" => (24.0, 24.0),
    ico_258: "menu/select" => (24.0, 24.0),
    ico_259: "menu/send_when_online" => (24.0, 24.0),
    ico_260: "menu/settings" => (24.0, 24.0),
    ico_261: "menu/share" => (24.0, 24.0),
    ico_262: "menu/shop" => (24.0, 24.0),
    ico_263: "menu/shortcut" => (24.0, 24.0),
    ico_264: "menu/show_in_chat" => (24.0, 24.0),
    ico_265: "menu/show_in_folder" => (24.0, 24.0),
    ico_266: "menu/silent" => (24.0, 24.0),
    ico_267: "menu/star" => (24.0, 24.0),
    ico_268: "menu/stats" => (24.0, 24.0),
    ico_269: "menu/stealth" => (24.0, 24.0),
    ico_270: "menu/stickers" => (24.0, 24.0),
    ico_271: "menu/storage" => (24.0, 24.0),
    ico_272: "menu/stories_archive" => (24.0, 24.0),
    ico_273: "menu/submenu_arrow" => (24.0, 24.0),
    ico_274: "menu/tag_filter" => (24.0, 24.0),
    ico_275: "menu/timer" => (24.0, 24.0),
    ico_276: "menu/topics" => (24.0, 24.0),
    ico_277: "menu/translate" => (24.0, 24.0),
    ico_278: "menu/unarchive" => (24.0, 24.0),
    ico_279: "menu/unfavorite" => (24.0, 24.0),
    ico_280: "menu/unmute" => (24.0, 24.0),
    ico_281: "menu/unpin" => (24.0, 24.0),
    ico_282: "menu/unread" => (24.0, 24.0),
    ico_283: "menu/video_chat" => (24.0, 24.0),
    ico_284: "menu/view_replies" => (24.0, 24.0),
    ico_285: "mini_forward" => (16.0, 16.0),
    ico_286: "mini_reply_story" => (16.0, 16.0),
    ico_287: "monitor" => (300.0, 229.0),
    ico_288: "new_chat_photo" => (30.0, 27.0),
    ico_289: "pinned_show_all" => (32.0, 32.0),
    ico_290: "player/panel_close" => (24.0, 24.0),
    ico_291: "player/player_backward" => (24.0, 24.0),
    ico_292: "player/player_forward" => (24.0, 24.0),
    ico_293: "player/player_fullscreen" => (24.0, 24.0),
    ico_294: "player/player_more" => (24.0, 24.0),
    ico_295: "player/player_pause" => (24.0, 24.0),
    ico_296: "player/player_pip" => (24.0, 24.0),
    ico_297: "player/player_play" => (24.0, 24.0),
    ico_298: "player/player_repeat" => (24.0, 24.0),
    ico_299: "player/player_settings" => (24.0, 24.0),
    ico_300: "player/player_shuffle" => (24.0, 24.0),
    ico_301: "player/player_speed" => (24.0, 24.0),
    ico_302: "player/player_volume_off" => (24.0, 24.0),
    ico_303: "player/player_volume_on" => (24.0, 24.0),
    ico_304: "player/playlist_cancel" => (12.0, 12.0),
    ico_305: "player/playlist_download" => (14.0, 18.0),
    ico_306: "player/playlist_pause" => (12.0, 14.0),
    ico_307: "player/playlist_play" => (18.0, 16.0),
    ico_308: "poll_choice_right" => (12.0, 12.0),
    ico_309: "poll_choice_wrong" => (12.0, 12.0),
    ico_310: "poll_select_check" => (20.0, 20.0),
    ico_311: "profile_add_member" => (22.0, 16.0),
    ico_312: "profile_premium" => (18.0, 18.0),
    ico_313: "profile_verified_check" => (18.0, 18.0),
    ico_314: "profile_verified_star" => (18.0, 18.0),
    ico_315: "quiz_timer" => (24.0, 24.0),
    ico_316: "send_media/send_media_delete" => (24.0, 24.0),
    ico_317: "send_media/send_media_replace" => (24.0, 24.0),
    ico_318: "settings/add" => (20.0, 20.0),
    ico_319: "settings/advanced" => (24.0, 24.0),
    ico_320: "settings/archive" => (24.0, 24.0),
    ico_321: "settings/ask_question" => (24.0, 24.0),
    ico_322: "settings/battery" => (24.0, 24.0),
    ico_323: "settings/blocked" => (24.0, 24.0),
    ico_324: "settings/calls" => (24.0, 24.0),
    ico_325: "settings/channel" => (24.0, 24.0),
    ico_326: "settings/chat" => (24.0, 24.0),
    ico_327: "settings/dock" => (24.0, 24.0),
    ico_328: "settings/download" => (24.0, 24.0),
    ico_329: "settings/earn" => (24.0, 24.0),
    ico_330: "settings/email" => (24.0, 24.0),
    ico_331: "settings/emoji" => (24.0, 24.0),
    ico_332: "settings/faq" => (24.0, 24.0),
    ico_333: "settings/folders" => (24.0, 24.0),
    ico_334: "settings/forward" => (24.0, 24.0),
    ico_335: "settings/gift" => (24.0, 24.0),
    ico_336: "settings/group" => (24.0, 24.0),
    ico_337: "settings/interface_scale" => (24.0, 24.0),
    ico_338: "settings/key" => (24.0, 24.0),
    ico_339: "settings/language" => (24.0, 24.0),
    ico_340: "settings/laptop" => (24.0, 24.0),
    ico_341: "settings/lock" => (24.0, 24.0),
    ico_342: "settings/mention" => (24.0, 24.0),
    ico_343: "settings/minus" => (24.0, 24.0),
    ico_344: "settings/night" => (24.0, 24.0),
    ico_345: "settings/notifications" => (24.0, 24.0),
    ico_346: "settings/online" => (24.0, 24.0),
    ico_347: "settings/palette" => (24.0, 24.0),
    ico_348: "settings/photo" => (24.0, 24.0),
    ico_349: "settings/pin" => (24.0, 24.0),
    ico_350: "settings/plus" => (24.0, 24.0),
    ico_351: "settings/position" => (24.0, 24.0),
    ico_352: "settings/reload" => (24.0, 24.0),
    ico_353: "settings/remove" => (20.0, 20.0),
    ico_354: "settings/saved_messages" => (24.0, 24.0),
    ico_355: "settings/settings" => (24.0, 24.0),
    ico_356: "settings/settings_edit_name" => (14.0, 14.0),
    ico_357: "settings/settings_name" => (28.0, 28.0),
    ico_358: "settings/settings_phone_number" => (28.0, 28.0),
    ico_359: "settings/settings_stickers" => (28.0, 28.0),
    ico_360: "settings/sound" => (24.0, 24.0),
    ico_361: "settings/stickers" => (24.0, 24.0),
    ico_362: "settings/stories" => (24.0, 24.0),
    ico_363: "settings/timer" => (24.0, 24.0),
    ico_364: "settings/tips" => (24.0, 24.0),
    ico_365: "settings/topics" => (24.0, 24.0),
    ico_366: "settings/ttl" => (24.0, 24.0),
    ico_367: "settings/user" => (24.0, 24.0),
    ico_368: "settings/video_calls" => (24.0, 24.0),
    ico_369: "title_back" => (16.0, 16.0),
    ico_370: "title_search" => (18.0, 18.0),
    ico_371: "top_bar_call" => (32.0, 32.0),
    ico_372: "top_bar_group_call" => (32.0, 32.0),
    ico_373: "top_bar_profile" => (32.0, 32.0),
    ico_374: "top_bar_search" => (32.0, 32.0),
    ico_375: "upload_chat_photo" => (22.0, 20.0),
    ico_376: "volume_mute" => (15.0, 13.0),
}

macro_rules! range_tests {
    ($($name:ident: ($hdr:expr, $total:expr) => $exp:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(parse_range($hdr, $total), $exp);
            }
        )+
    };
}

range_tests! {
    rng_001: ("bytes=0-0", 1000) => Some((0, 0)),
    rng_002: ("bytes=0-1", 1000) => Some((0, 1)),
    rng_003: ("bytes=0-99", 1000) => Some((0, 99)),
    rng_004: ("bytes=0-999", 1000) => Some((0, 999)),
    rng_005: ("bytes=0-1000", 1000) => Some((0, 999)),
    rng_006: ("bytes=0-999999", 1000) => Some((0, 999)),
    rng_007: ("bytes=0-", 1000) => Some((0, 999)),
    rng_008: ("bytes=1-", 1000) => Some((1, 999)),
    rng_009: ("bytes=100-", 1000) => Some((100, 999)),
    rng_010: ("bytes=500-", 1000) => Some((500, 999)),
    rng_011: ("bytes=999-", 1000) => Some((999, 999)),
    rng_012: ("bytes=1000-", 1000) => None,
    rng_013: ("bytes=1001-", 1000) => None,
    rng_014: ("bytes=500-499", 1000) => None,
    rng_015: ("bytes=500-100", 1000) => None,
    rng_016: ("bytes=128-255", 1024) => Some((128, 255)),
    rng_017: ("bytes=256-511", 1024) => Some((256, 511)),
    rng_018: ("bytes=512-1023", 1024) => Some((512, 1023)),
    rng_019: ("bytes=131072-", 5242880) => Some((131072, 5242879)),
    rng_020: ("bytes=262144-524287", 5242880) => Some((262144, 524287)),
    rng_021: ("  bytes=10-20  ", 100) => Some((10, 20)),
    rng_022: ("bytes= 10 - 20 ", 100) => Some((10, 20)),
    rng_023: ("items=0-10", 100) => None,
    rng_024: ("bytes=", 100) => None,
    rng_025: ("bytes=-10", 100) => None,
    rng_026: ("bytes=abc-def", 100) => None,
    rng_027: ("bytes=0-abc", 100) => None,
    rng_028: ("", 100) => None,
    rng_029: ("bytes=0-0", 0) => None,
    rng_030: ("bytes=0-0", 1) => Some((0, 0)),
    rng_031: ("bytes=1024-2047", 4096) => Some((1024, 2047)),
    rng_032: ("bytes=2048-4095", 4096) => Some((2048, 4095)),
    rng_033: ("bytes=4095-4095", 4096) => Some((4095, 4095)),
    rng_034: ("bytes=4096-4096", 4096) => None,
    rng_035: ("bytes=65536-131071", 1048576) => Some((65536, 131071)),
    rng_036: ("bytes=524288-1048575", 1048576) => Some((524288, 1048575)),
    rng_037: ("bytes=1048575-1048575", 1048576) => Some((1048575, 1048575)),
    rng_038: ("bytes=1048576-", 1048576) => None,
    rng_039: ("bytes=0-65535", 65536) => Some((0, 65535)),
    rng_040: ("bytes=32768-", 65536) => Some((32768, 65535)),
    rng_041: ("bytes=10-10", 50) => Some((10, 10)),
    rng_042: ("bytes=20-20", 50) => Some((20, 20)),
    rng_043: ("bytes=30-30", 50) => Some((30, 30)),
    rng_044: ("bytes=40-40", 50) => Some((40, 40)),
    rng_045: ("bytes=49-49", 50) => Some((49, 49)),
    rng_046: ("bytes=50-50", 50) => None,
    rng_047: ("bytes=0-131071", 10000000) => Some((0, 131071)),
    rng_048: ("bytes=131072-393215", 10000000) => Some((131072, 393215)),
    rng_049: ("bytes=393216-917503", 10000000) => Some((393216, 917503)),
    rng_050: ("bytes=917504-", 10000000) => Some((917504, 9999999)),
}

macro_rules! stream_mime_tests {
    ($($name:ident: $ext:expr => $mime:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let st = StreamState::new(PathBuf::from("a.part"), PathBuf::from("a"), 1024, $ext);
                assert_eq!(st.mime, $mime);
            }
        )+
    };
}

stream_mime_tests! {
    smime_001: "mp4" => "video/mp4",
    smime_002: "MP4" => "video/mp4",
    smime_003: "webm" => "video/webm",
    smime_004: "WEBM" => "video/webm",
    smime_005: "mov" => "video/quicktime",
    smime_006: "mkv" => "video/x-matroska",
    smime_007: "ogv" => "video/ogg",
    smime_008: "gif" => "image/gif",
    smime_009: "mp3" => "audio/mpeg",
    smime_010: "MP3" => "audio/mpeg",
    smime_011: "m4a" => "audio/mp4",
    smime_012: "M4A" => "audio/mp4",
    smime_013: "aac" => "audio/mp4",
    smime_014: "ogg" => "audio/ogg",
    smime_015: "oga" => "audio/ogg",
    smime_016: "opus" => "audio/ogg",
    smime_017: "flac" => "audio/flac",
    smime_018: "FLAC" => "audio/flac",
    smime_019: "wav" => "audio/wav",
    smime_020: "WAV" => "audio/wav",
}

macro_rules! curve_tests {
    ($($name:ident: $t:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let k: f32 = $t;
                let s1 = ios_spring(k);
                let s2 = spring(k);
                let s3 = ease_out(k);
                assert!((0.0..=1.0).contains(&s1), "ios_spring({}) = {}", k, s1);
                assert!((0.0..=1.0).contains(&s2), "spring({}) = {}", k, s2);
                assert!((0.0..=1.0).contains(&s3), "ease_out({}) = {}", k, s3);
                if k > 0.0 && k < 1.0 {
                    assert!(s1 >= k, "ios_spring({}) = {} should be >= k", k, s1);
                    assert!(s2 >= k, "spring({}) = {} should be >= k", k, s2);
                }
            }
        )+
    };
}

curve_tests! {
    crv_000: 0.00, crv_001: 0.01, crv_002: 0.02, crv_003: 0.03, crv_004: 0.04,
    crv_005: 0.05, crv_006: 0.06, crv_007: 0.07, crv_008: 0.08, crv_009: 0.09,
    crv_010: 0.10, crv_011: 0.11, crv_012: 0.12, crv_013: 0.13, crv_014: 0.14,
    crv_015: 0.15, crv_016: 0.16, crv_017: 0.17, crv_018: 0.18, crv_019: 0.19,
    crv_020: 0.20, crv_021: 0.21, crv_022: 0.22, crv_023: 0.23, crv_024: 0.24,
    crv_025: 0.25, crv_026: 0.26, crv_027: 0.27, crv_028: 0.28, crv_029: 0.29,
    crv_030: 0.30, crv_031: 0.31, crv_032: 0.32, crv_033: 0.33, crv_034: 0.34,
    crv_035: 0.35, crv_036: 0.36, crv_037: 0.37, crv_038: 0.38, crv_039: 0.39,
    crv_040: 0.40, crv_041: 0.41, crv_042: 0.42, crv_043: 0.43, crv_044: 0.44,
    crv_045: 0.45, crv_046: 0.46, crv_047: 0.47, crv_048: 0.48, crv_049: 0.49,
    crv_050: 0.50, crv_051: 0.51, crv_052: 0.52, crv_053: 0.53, crv_054: 0.54,
    crv_055: 0.55, crv_056: 0.56, crv_057: 0.57, crv_058: 0.58, crv_059: 0.59,
    crv_060: 0.60, crv_061: 0.61, crv_062: 0.62, crv_063: 0.63, crv_064: 0.64,
    crv_065: 0.65, crv_066: 0.66, crv_067: 0.67, crv_068: 0.68, crv_069: 0.69,
    crv_070: 0.70, crv_071: 0.71, crv_072: 0.72, crv_073: 0.73, crv_074: 0.74,
    crv_075: 0.75, crv_076: 0.76, crv_077: 0.77, crv_078: 0.78, crv_079: 0.79,
    crv_080: 0.80, crv_081: 0.81, crv_082: 0.82, crv_083: 0.83, crv_084: 0.84,
    crv_085: 0.85, crv_086: 0.86, crv_087: 0.87, crv_088: 0.88, crv_089: 0.89,
    crv_090: 0.90, crv_091: 0.91, crv_092: 0.92, crv_093: 0.93, crv_094: 0.94,
    crv_095: 0.95, crv_096: 0.96, crv_097: 0.97, crv_098: 0.98, crv_099: 0.99,
    crv_100: 1.00,
}

macro_rules! streamer_mask_tests {
    ($($name:ident: ($val:expr, $on:expr) => $exp:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                assert_eq!(mask_streamer($val, $on), $exp);
            }
        )+
    };
}

streamer_mask_tests! {
    smk_001: ("+79991234567", true) => "+• ••• ••• •• ••",
    smk_002: ("+19296463185", true) => "+• ••• ••• •• ••",
    smk_003: ("+19702876230", true) => "+• ••• ••• •• ••",
    smk_004: ("+380501234567", true) => "+• ••• ••• •• ••",
    smk_005: ("+375291234567", true) => "+• ••• ••• •• ••",
    smk_006: ("+4915123456789", true) => "+• ••• ••• •• ••",
    smk_007: ("@erootsupport", true) => "@••••••",
    smk_008: ("@sudoyp", true) => "@••••••",
    smk_009: ("@durov", true) => "@••••••",
    smk_010: ("@telegram", true) => "@••••••",
    smk_011: ("@rustgramm", true) => "@••••••",
    smk_012: ("8872283255", true) => "••••••",
    smk_013: ("8529489873", true) => "••••••",
    smk_014: ("123456789", true) => "••••••",
    smk_015: ("+79991234567", false) => "+79991234567",
    smk_016: ("@erootsupport", false) => "@erootsupport",
    smk_017: ("8872283255", false) => "8872283255",
    smk_018: ("", true) => "",
    smk_019: ("", false) => "",
    smk_020: ("ID 8872283255", true) => "••••••",
}

macro_rules! avatar_shade_tests {
    ($($name:ident: $id:expr),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let c = avatar_color($id);
                assert_eq!(c, avatar_color(-($id as i64)));
                let max_c = c.r().max(c.g()).max(c.b());
                let min_c = c.r().min(c.g()).min(c.b());
                assert!(max_c.saturating_sub(min_c) >= 40, "expected vibrant Telegram color, got {c:?}");
                assert_eq!(c.a(), 255);
            }
        )+
    };
}

avatar_shade_tests! {
    avc_001: 1, avc_002: 2, avc_003: 3, avc_004: 4, avc_005: 5,
    avc_006: 6, avc_007: 7, avc_008: 8, avc_009: 9, avc_010: 10,
    avc_011: 11, avc_012: 12, avc_013: 13, avc_014: 14, avc_015: 15,
    avc_016: 16, avc_017: 17, avc_018: 18, avc_019: 19, avc_020: 20,
    avc_021: 21, avc_022: 22, avc_023: 23, avc_024: 24, avc_025: 25,
    avc_026: 26, avc_027: 27, avc_028: 28, avc_029: 29, avc_030: 30,
    avc_031: 100, avc_032: 1000, avc_033: 10000, avc_034: 100000, avc_035: 1000000,
    avc_036: 8872283255, avc_037: 8529489873, avc_038: 777000, avc_039: 424242, avc_040: 999999999,
}

macro_rules! color_lerp_tests {
    ($($name:ident: ($k:expr, $exp_r:expr)),+ $(,)?) => {
        $(
            #[test]
            fn $name() {
                let c = lerp_color(Color32::from_rgb(0, 100, 200), Color32::from_rgb(200, 100, 0), $k);
                assert_eq!(c.r(), $exp_r);
                assert_eq!(c.g(), 100);
                assert_eq!(c.b(), 200 - $exp_r);
            }
        )+
    };
}

color_lerp_tests! {
    clrp_00: (0.00, 0),
    clrp_01: (0.05, 10),
    clrp_02: (0.10, 20),
    clrp_03: (0.15, 30),
    clrp_04: (0.20, 40),
    clrp_05: (0.25, 50),
    clrp_06: (0.30, 60),
    clrp_07: (0.35, 70),
    clrp_08: (0.40, 80),
    clrp_09: (0.45, 90),
    clrp_10: (0.50, 100),
    clrp_11: (0.55, 110),
    clrp_12: (0.60, 120),
    clrp_13: (0.65, 130),
    clrp_14: (0.70, 140),
    clrp_15: (0.75, 150),
    clrp_16: (0.80, 160),
    clrp_17: (0.85, 170),
    clrp_18: (0.90, 180),
    clrp_19: (0.95, 190),
    clrp_20: (1.00, 200),
}

#[test]
fn theme_opacity_and_blur_state() {
    set_blur(false);
    assert!(!blur_on());
    set_blur(true);
    assert!(blur_on());

    set_opacity(15);
    assert!((opacity_f32() - 0.149).abs() < 0.01);
    assert!(t(WINDOW_BG).a() < 45);
    assert!(t_btn(ORANGE).a() < 45);
    assert!(t2(WINDOW_BG).a() >= 44);
    assert!(tint_panel().a() <= 7);
    assert!(tint_chat().a() <= 70);
    assert!(tint_title().a() <= 11);

    set_opacity(DEFAULT_OPACITY_PCT);
    assert_eq!(DEFAULT_OPACITY_PCT, 70);
    assert_eq!(t(WINDOW_BG).a(), 178);
    assert_eq!(t_btn(ORANGE).a(), 178);
    set_opacity(100);
    assert_eq!(t(WINDOW_BG).a(), 255);
    assert_eq!(t_btn(ORANGE).a(), 255);
    set_opacity(DEFAULT_OPACITY_PCT);
}

#[test]
fn navy_and_tg_grey_conversion() {
    for hex in [0x000000, 0x112233, 0x40a7e3, 0xf58220, 0xffffff] {
        let a = navy(hex);
        let b = tg(hex);
        assert_eq!(a, b);
        assert_eq!(a.r(), a.g());
        assert_eq!(a.g(), a.b());
    }
}

#[test]
fn time_and_date_formatters() {
    let ts = 1_763_700_000;
    assert_eq!(hhmm(ts).len(), 5);
    assert_eq!(hhmmss(ts).len(), 8);
    assert_eq!(full_date(ts).len(), 19);
    assert!(same_day(ts, ts + 5));
    assert!(!same_day(ts, ts + 3 * 86_400));
    assert!(!dialog_date(ts, ts + 60).is_empty());
    assert!(!day_label(ts, ts).is_empty());
}
