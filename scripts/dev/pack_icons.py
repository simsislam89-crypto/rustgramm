import os
from PIL import Image

base = "tdesktop/Telegram/Resources/icons/"
names = """
menu/unread menu/show_in_folder archive_userpic menu/chat_bubble menu/tag_filter menu/profile menu/groups menu/channel menu/calls_receive menu/saved_messages menu/settings menu/night_mode menu/stealth menu/read menu/search menu/chats menu/archive menu/unarchive menu/mute menu/unmute menu/pin menu/unpin menu/reply menu/forward menu/copy menu/delete menu/edit menu/select menu/report menu/translate menu/link menu/share menu/info menu/add menu/stickers menu/emoji menu/gif menu/image menu/file menu/lock menu/devices menu/faq menu/photo_set menu/video_chat menu/block menu/leave menu/show_in_chat menu/save_image menu/download menu/silent menu/favorite menu/unfavorite menu/send_when_online menu/timer menu/phone menu/calendar menu/invite menu/groups_create menu/discussion menu/new_window menu/bot menu/clear menu/export menu/forwarded_status menu/edited_status menu/manage menu/permissions menu/storage menu/network menu/personal menu/premium menu/qr_code menu/shop menu/star menu/topics menu/view_replies menu/hide_members menu/admin menu/cancel menu/restore menu/flip menu/shortcut menu/ip_address menu/passcode_finger menu/colors menu/fonts menu/palette menu/customize menu/info_notifications menu/group_log menu/disable menu/stats menu/auto_delete menu/stories_archive menu/read_reactions menu/read_ticks menu/submenu_arrow
chat/input_attach chat/input_smile_face chat/input_send chat/input_send_round chat/input_record chat/input_record_filled chat/input_silent chat/input_silent_on chat/input_scheduled chat/input_forward chat/input_edit chat/input_video chat/input_reply_quote chat/input_save chat/input_like chat/input_liked chat/input_bot_command chat/input_bot_keyboard chat/input_gift chat/input_comments_expand chat/input_comments_hide chat/input_draw chat/input_replace chat/input_autodelete chat/reactions_expand_panel chat/mini_quote chat/mini_lock chat/mini_copy chat/mini_info_alert chat/voice_to_text chat/voice_to_text_collapse chat/menu_sidebar1 chat/menu_sidebar2 chat/menu_sidebar3
dialogs/dialogs_menu dialogs/dialogs_cancel_search dialogs/dialogs_pinned dialogs/dialogs_mute dialogs/dialogs_sent dialogs/dialogs_received dialogs/dialogs_sending dialogs/dialogs_verified_star dialogs/dialogs_verified_check dialogs/dialogs_mention dialogs/dialogs_reaction dialogs/dialogs_premium dialogs/dialogs_downloads dialogs/dialogs_calendar dialogs/dialogs_lock_on dialogs/dialogs_lock_off dialogs/dialogs_chat dialogs/dialogs_channel dialogs/dialogs_bot dialogs/dialogs_forum dialogs/dialogs_mini_play dialogs/dialogs_topic_arrow dialogs/mini_arrow dialogs/avatar_notes dialogs/avatar_hidden dialogs/archive_userpic dialogs/dialogs_search_from dialogs/dialogs_menu_unread dialogs/dialogs_menu_unread_dot dialogs/mini_tag_lock
info/info_search info/info_close info/info_back info/info_edit info/info_members info/info_media_photo info/info_media_video info/info_media_file info/info_media_audio info/info_media_voice info/info_media_gif info/info_media_round info/info_media_saved info/info_media_link info/info_media_forward info/info_media_delete info/info_add_member info/info_notifications info/info_permissions info/info_report info/info_share info/info_block info/info_leave info/info_delete info/info_information info/info_common_groups info/info_stories_archive info/info_rights_lock
settings/notifications settings/lock settings/language settings/folders settings/sound settings/stickers settings/stories settings/advanced settings/ttl settings/archive settings/emoji settings/photo settings/key settings/laptop settings/battery settings/night settings/palette settings/user settings/calls settings/chat settings/channel settings/group settings/devices settings/saved_messages settings/settings settings/download settings/blocked settings/forward settings/mention settings/online settings/email settings/dock settings/faq settings/ask_question settings/tips settings/interface_scale settings/position settings/timer settings/video_calls settings/topics settings/gift settings/earn settings/reload settings/pin settings/plus settings/minus settings/remove settings/add settings/settings_name settings/settings_phone_number settings/settings_edit_name settings/settings_stickers
top_bar_search title_menu_dots top_bar_call top_bar_group_call top_bar_profile title_back title_search history_pin history_received history_sent history_views history_down_arrow history_down_circle history_replies history_comments history_unread_mention history_unread_reaction history_file_document history_file_download history_file_cancel history_file_pause history_file_play history_file_image history_audio_download history_audio_cancel media_play media_pause media_video_play profile_verified_star profile_verified_check profile_premium profile_add_member pinned_show_all inline_button_url inline_button_copy inline_button_switch inline_button_web inline_button_card mini_forward mini_reply_story fast_comments fast_share fast_to_original contacts_online calendar_down poll_choice_right poll_choice_wrong poll_select_check quiz_timer volume_mute monitor new_chat_photo upload_chat_photo install_update
emoji/emoji_recent emoji/emoji_smile emoji/emoji_nature emoji/emoji_food emoji/emoji_travel emoji/emoji_activities emoji/emoji_objects emoji/emoji_love emoji/emoji_skin emoji/emoji_delete emoji/emoji_back emoji/emoji_settings emoji/emoji_search_input emoji/stickers_premium emoji/premium_lock
folders/folders_all folders/folders_custom folders/folders_edit folders/folders_group folders/folders_channels folders/folders_bots folders/folders_private folders/folders_unread folders/folders_setup folders/folders_type_archived folders/folders_type_bots folders/folders_type_channels folders/folders_type_contacts folders/folders_type_groups folders/folders_type_muted folders/folders_type_noncontacts folders/folders_type_read folders/folders_unmuted folders/folders_like folders/folders_love folders/folders_favorite folders/folders_home folders/folders_work folders/folders_study folders/folders_travel folders/folders_trade folders/folders_money
calls/call_answer calls/call_cancel calls/call_discard calls/call_camera_active calls/call_camera_muted calls/call_message calls/call_arrow_in calls/call_arrow_out calls/call_ended calls/call_record_active calls/call_record_muted calls/call_group calls/calls_add_people calls/calls_more calls/calls_present calls/calls_settings
mediaview/download mediaview/more mediaview/next mediaview/rotate mediaview/viewer_share mediaview/views mediaview/save_check mediaview/draw mediaview/recognize
player/player_play player/player_pause player/player_backward player/player_forward player/player_volume_on player/player_volume_off player/player_speed player/player_fullscreen player/player_more player/player_repeat player/player_shuffle player/playlist_play player/playlist_pause player/playlist_download player/playlist_cancel player/player_pip player/player_settings player/panel_close
send_media/send_media_delete send_media/send_media_replace
"""
names = sorted(set(names.split()))
imgs = []
miss = []
for n in names:
    p = base + n + "@2x.png"
    if not os.path.exists(p):
        miss.append(n)
        continue
    imgs.append((n, Image.open(p).convert("L")))
print("ok", len(imgs), "missing", miss)
W = 1024
x = y = rowh = 0
pos = {}
imgs.sort(key=lambda t: -t[1].size[1])
for n, im in imgs:
    w, h = im.size
    if x + w + 1 > W:
        x = 0
        y += rowh + 1
        rowh = 0
    pos[n] = (x, y, w, h)
    x += w + 1
    rowh = max(rowh, h)
H = y + rowh + 1
atlas = Image.new("L", (W, H), 0)
for n, im in imgs:
    atlas.paste(im, pos[n][:2])
out = "D:/working WIN/rustgramm/crates/rg-ui/assets/"
atlas.save(out + "icons.png", optimize=True)
with open(out + "icons.idx", "w", encoding="utf-8", newline="\n") as f:
    for n, (a, b, c, d) in sorted(pos.items()):
        f.write(f"{n} {a} {b} {c} {d}\n")
print(atlas.size, os.path.getsize(out + "icons.png"))
