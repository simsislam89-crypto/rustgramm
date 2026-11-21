//! Главное меню и окно «Настройки AyuGram»: те же разделы, что в оригинале.

use eframe::egui::{self, RichText, Vec2};
use rg_settings::{
    ChannelBottomButton, MenuVisibility, PeerIdDisplay, SendWithoutSound, Settings,
};

use super::{App, Cmd, theme::*};

const TABS: [&str; 6] = [
    "Призрак",
    "Сохранение и фильтры",
    "Внешний вид",
    "Чаты и меню",
    "Поле ввода",
    "Прочее",
];

fn check(ui: &mut egui::Ui, value: &mut bool, label: &str) {
    ui.checkbox(value, label);
}

fn menu_choice(ui: &mut egui::Ui, value: &mut MenuVisibility, label: &str) {
    ui.horizontal(|ui| {
        ui.label(label);
        egui::ComboBox::from_id_salt(label)
            .selected_text(match value {
                MenuVisibility::Hidden => "скрыт",
                MenuVisibility::Visible => "виден",
                MenuVisibility::WithModifier => "с Shift",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(value, MenuVisibility::Hidden, "скрыт");
                ui.selectable_value(value, MenuVisibility::Visible, "виден");
                ui.selectable_value(value, MenuVisibility::WithModifier, "с Shift");
            });
    });
}

impl App {
    /// Главное меню (три полосы): быстрые переключатели и вход в настройки.
    pub(crate) fn main_menu(&mut self, resp: &egui::Response) {
        let mut s = self.settings.clone();
        let me = self.me.clone();
        let mut open_settings = false;
        egui::Popup::menu(resp).show(|ui| {
            ui.set_min_width(260.0);
            ui.label(RichText::new(&me).strong().color(WHITE));
            ui.separator();
            if s.ayu.drawer_ghost_toggle {
                let mut ghost = s.ghost.active;
                if ui.checkbox(&mut ghost, "Режим призрака").changed() {
                    s.ghost.set_active(ghost);
                }
            }
            if s.ayu.drawer_s_read_toggle {
                check(ui, &mut s.ayu.save_deleted_messages, "Сохранять удалённые сообщения");
                check(ui, &mut s.ayu.save_messages_history, "История правок");
            }
            if s.ayu.drawer_night_mode_toggle {
                ui.weak("Тема: ночная (единственная)");
            }
            let mut streamer = s.ayu.streamer_mode;
            if s.ayu.drawer_streamer_toggle && ui.checkbox(&mut streamer, "Режим стримера").changed() {
                s.ayu.streamer_mode = streamer;
            }
            ui.separator();
            if ui.button("Настройки").clicked() {
                open_settings = true;
                ui.close();
            }
            ui.separator();
            ui.weak("Соединения: только серверы Telegram");
        });
        if open_settings {
            self.settings_open = true;
        }
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(Box::new(s)));
        }
    }

    pub(crate) fn settings_window(&mut self, ctx: &egui::Context) {
        if !self.settings_open {
            return;
        }
        let mut open = true;
        let mut s = self.settings.clone();
        let mut tab = self.settings_tab;
        let mut new_filter = std::mem::take(&mut self.filter_new);
        egui::Window::new("Настройки")
            .open(&mut open)
            .collapsible(false)
            .default_size(Vec2::new(560.0, 520.0))
            .anchor(egui::Align2::CENTER_CENTER, Vec2::ZERO)
            .show(ctx, |ui| {
                ui.horizontal_wrapped(|ui| {
                    for (i, t) in TABS.iter().enumerate() {
                        if ui.selectable_label(tab == i, *t).clicked() {
                            tab = i;
                        }
                    }
                });
                ui.separator();
                egui::ScrollArea::vertical().show(ui, |ui| match tab {
                    0 => ghost_tab(ui, &mut s),
                    1 => saving_tab(ui, &mut s, &mut new_filter),
                    2 => look_tab(ui, &mut s),
                    3 => chats_tab(ui, &mut s),
                    4 => field_tab(ui, &mut s),
                    _ => other_tab(ui, &mut s),
                });
            });
        self.filter_new = new_filter;
        self.settings_tab = tab;
        if !open {
            self.settings_open = false;
        }
        if s != self.settings {
            self.settings = s.clone();
            self.send(Cmd::Settings(Box::new(s)));
        }
    }
}

fn ghost_tab(ui: &mut egui::Ui, s: &mut Settings) {
    let mut active = s.ghost.active;
    if ui.checkbox(&mut active, "Режим призрака (всё ниже выключается разом)").changed() {
        s.ghost.set_active(active);
    }
    ui.separator();
    let g = &mut s.ghost;
    check(ui, &mut g.send_read_messages, "Отправлять «прочитано» (сообщения)");
    check(ui, &mut g.send_read_stories, "Отправлять просмотры историй");
    check(ui, &mut g.send_online_packets, "Отправлять статус «в сети»");
    check(ui, &mut g.send_upload_progress, "Отправлять прогресс загрузки / «печатает…»");
    check(ui, &mut g.send_offline_after_online, "Сразу «не в сети» после «в сети»");
    check(ui, &mut g.mark_read_after_action, "Отмечать прочитанным после моего сообщения");
    check(ui, &mut g.use_scheduled_messages, "Отправлять через отложенные (в режиме призрака)");
    ui.horizontal(|ui| {
        ui.label("Отправка без звука");
        egui::ComboBox::from_id_salt("silent")
            .selected_text(match g.send_without_sound {
                SendWithoutSound::Never => "никогда",
                SendWithoutSound::InGhostMode => "в режиме призрака",
                SendWithoutSound::Always => "всегда",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut g.send_without_sound, SendWithoutSound::Never, "никогда");
                ui.selectable_value(&mut g.send_without_sound, SendWithoutSound::InGhostMode, "в режиме призрака");
                ui.selectable_value(&mut g.send_without_sound, SendWithoutSound::Always, "всегда");
            });
    });
    ui.separator();
    ui.label(RichText::new("Закрепить значение (режим призрака его не меняет)").strong());
    check(ui, &mut g.locked_read_messages, "«Прочитано»");
    check(ui, &mut g.locked_read_stories, "Просмотры историй");
    check(ui, &mut g.locked_online_packets, "«В сети»");
    check(ui, &mut g.locked_upload_progress, "Прогресс загрузки");
    check(ui, &mut g.locked_offline_after_online, "«Не в сети»");
    check(ui, &mut s.ayu.use_global_ghost_mode, "Один режим призрака для всех аккаунтов");
    check(
        ui,
        &mut s.ayu.suggest_ghost_mode_before_viewing_story,
        "Предлагать режим призрака перед просмотром истории",
    );
}

fn saving_tab(ui: &mut egui::Ui, s: &mut Settings, new_filter: &mut String) {
    let a = &mut s.ayu;
    check(ui, &mut a.save_deleted_messages, "Сохранять удалённые сообщения");
    check(ui, &mut a.save_messages_history, "Сохранять историю правок");
    check(ui, &mut a.save_for_bots, "Сохранять и в чатах с ботами");
    check(ui, &mut a.semi_transparent_deleted_messages, "Удалённые — полупрозрачные");
    ui.horizontal(|ui| {
        ui.label("Метка удалённого");
        ui.add(egui::TextEdit::singleline(&mut a.deleted_mark).desired_width(60.0));
    });
    ui.horizontal(|ui| {
        ui.label("Метка изменённого");
        ui.add(egui::TextEdit::singleline(&mut a.edited_mark).hint_text("изм.").desired_width(60.0));
    });
    ui.separator();
    check(ui, &mut a.filters_enabled, "Фильтры сообщений включены");
    check(ui, &mut a.filters_enabled_in_chats, "Применять фильтры и в списке чатов");
    check(ui, &mut a.hide_from_blocked, "Скрывать сообщения заблокированных");
    check(ui, &mut a.filter_zalgo, "Чистить «зальго» (нагромождение диакритики)");
    ui.separator();
    ui.horizontal(|ui| {
        ui.add(
            egui::TextEdit::singleline(new_filter)
                .hint_text("Текст или регулярное выражение")
                .desired_width(280.0),
        );
        let t = new_filter.trim().to_owned();
        if ui.button("Добавить").clicked() && !t.is_empty() {
            s.filters.push(rg_settings::Filter {
                text: t,
                ..rg_settings::Filter::default()
            });
            new_filter.clear();
            s.ayu.filters_enabled = true;
        }
    });
    let mut remove = None;
    for (i, f) in s.filters.iter_mut().enumerate() {
        ui.horizontal(|ui| {
            ui.checkbox(&mut f.enabled, "");
            ui.label(RichText::new(&f.text).monospace());
            ui.checkbox(&mut f.is_regex, "regex");
            ui.checkbox(&mut f.case_sensitive, "Aa");
            if ui.small_button("✕").clicked() {
                remove = Some(i);
            }
        });
    }
    if let Some(i) = remove {
        s.filters.remove(i);
    }
}

fn look_tab(ui: &mut egui::Ui, s: &mut Settings) {
    let a = &mut s.ayu;
    ui.add(egui::Slider::new(&mut a.message_bubble_radius, 0..=20).text("Радиус пузырей"));
    ui.add(egui::Slider::new(&mut a.wide_multiplier_pct, 50..=200).text("Ширина пузырей, %"));
    ui.add(egui::Slider::new(&mut a.avatar_corners, 0..=23).text("Скругление аватарок"));
    check(ui, &mut a.single_corner_radius, "Один радиус для всех углов пузыря");
    check(ui, &mut a.remove_message_tail, "Убрать «хвостик» пузыря");
    check(ui, &mut a.show_background, "Показывать фон чата");
    check(ui, &mut a.disable_custom_backgrounds, "Игнорировать фоны, заданные в чатах");
    check(ui, &mut a.show_header_decorations, "Украшения шапки");
    check(ui, &mut a.show_colorful_replies, "Цветные ответы");
    check(ui, &mut a.simple_quotes_and_replies, "Простые цитаты и ответы");
    check(ui, &mut a.replace_bottom_info_with_icons, "Значки вместо текста внизу пузыря");
    check(ui, &mut a.show_message_seconds, "Секунды во времени сообщения");
    check(ui, &mut a.show_date, "Дата в пузыре");
    check(ui, &mut a.show_reactions, "Панель реакций у сообщений");
    check(ui, &mut a.reveal_spoilers, "Раскрывать спойлеры по наведению");
    check(ui, &mut a.material_switches, "Переключатели в стиле Material");
    check(ui, &mut a.adaptive_cover_color, "Адаптивный цвет обложки профиля");
    check(ui, &mut a.improve_link_previews, "Улучшенные превью ссылок");
    ui.horizontal(|ui| {
        ui.label("Моноширинный шрифт");
        ui.add(egui::TextEdit::singleline(&mut a.mono_font).hint_text("по умолчанию").desired_width(180.0));
    });
    ui.horizontal(|ui| {
        ui.label("Идентификатор чата");
        egui::ComboBox::from_id_salt("peerid")
            .selected_text(match a.show_peer_id {
                PeerIdDisplay::Hidden => "не показывать",
                PeerIdDisplay::BotApi => "Bot API",
                PeerIdDisplay::Telegram => "Telegram",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut a.show_peer_id, PeerIdDisplay::Hidden, "не показывать");
                ui.selectable_value(&mut a.show_peer_id, PeerIdDisplay::BotApi, "Bot API");
                ui.selectable_value(&mut a.show_peer_id, PeerIdDisplay::Telegram, "Telegram");
            });
    });
}

fn chats_tab(ui: &mut egui::Ui, s: &mut Settings) {
    let a = &mut s.ayu;
    ui.label(RichText::new("Контекстное меню сообщения").strong());
    menu_choice(ui, &mut a.menu_reactions_panel, "Панель реакций");
    menu_choice(ui, &mut a.menu_views_panel, "Просмотры");
    menu_choice(ui, &mut a.menu_hide_message, "Скрыть сообщение");
    menu_choice(ui, &mut a.menu_user_messages, "Сообщения пользователя");
    menu_choice(ui, &mut a.menu_message_details, "Сведения о сообщении");
    menu_choice(ui, &mut a.menu_repeat_message, "Повторить сообщение");
    menu_choice(ui, &mut a.menu_add_filter, "Добавить в фильтры");
    ui.separator();
    ui.label(RichText::new("Главное меню").strong());
    check(ui, &mut a.drawer_my_profile, "Мой профиль");
    check(ui, &mut a.drawer_bots, "Боты");
    check(ui, &mut a.drawer_new_group, "Новая группа");
    check(ui, &mut a.drawer_new_channel, "Новый канал");
    check(ui, &mut a.drawer_contacts, "Контакты");
    check(ui, &mut a.drawer_calls, "Звонки");
    check(ui, &mut a.drawer_saved_messages, "Избранное");
    check(ui, &mut a.drawer_ghost_toggle, "Переключатель режима призрака");
    check(ui, &mut a.drawer_s_read_toggle, "Переключатели сохранения");
    check(ui, &mut a.drawer_l_read_toggle, "Переключатель «Не читать»");
    check(ui, &mut a.drawer_night_mode_toggle, "Ночная тема");
    check(ui, &mut a.drawer_streamer_toggle, "Режим стримера");
    ui.separator();
    ui.label(RichText::new("Каналы и группы").strong());
    check(ui, &mut a.show_channel_reactions, "Реакции в каналах");
    check(ui, &mut a.show_group_reactions, "Реакции в группах");
    check(ui, &mut a.show_private_chat_reactions, "Реакции в личных чатах");
    check(ui, &mut a.collapse_similar_channels, "Сворачивать похожие каналы");
    check(ui, &mut a.hide_similar_channels, "Скрывать похожие каналы");
    check(ui, &mut a.quick_admin_shortcuts, "Быстрые действия администратора");
    ui.horizontal(|ui| {
        ui.label("Нижняя кнопка канала");
        egui::ComboBox::from_id_salt("chbtn")
            .selected_text(match a.channel_bottom_button {
                ChannelBottomButton::Hidden => "нет",
                ChannelBottomButton::Mute => "без звука",
                ChannelBottomButton::Discuss => "обсуждение",
                ChannelBottomButton::DiscussWithFallback => "обсуждение или звук",
            })
            .show_ui(ui, |ui| {
                ui.selectable_value(&mut a.channel_bottom_button, ChannelBottomButton::Hidden, "нет");
                ui.selectable_value(&mut a.channel_bottom_button, ChannelBottomButton::Mute, "без звука");
                ui.selectable_value(&mut a.channel_bottom_button, ChannelBottomButton::Discuss, "обсуждение");
                ui.selectable_value(
                    &mut a.channel_bottom_button,
                    ChannelBottomButton::DiscussWithFallback,
                    "обсуждение или звук",
                );
            });
    });
}

fn field_tab(ui: &mut egui::Ui, s: &mut Settings) {
    let a = &mut s.ayu;
    ui.label(RichText::new("Кнопки поля ввода").strong());
    check(ui, &mut a.field_attach_button, "Скрепка");
    check(ui, &mut a.field_commands_button, "Команды бота");
    check(ui, &mut a.field_emoji_button, "Эмодзи");
    check(ui, &mut a.field_microphone_button, "Микрофон");
    check(ui, &mut a.field_auto_delete_button, "Автоудаление");
    check(ui, &mut a.field_gift_button, "Подарок");
    check(ui, &mut a.field_ai_editor_button, "ИИ-редактор");
    check(ui, &mut a.show_attach_popup, "Всплывающее меню скрепки");
    check(ui, &mut a.show_emoji_popup, "Всплывающая панель эмодзи");
    ui.separator();
    ui.label(RichText::new("Подтверждать перед отправкой").strong());
    check(ui, &mut a.sticker_confirmation, "Стикеры");
    check(ui, &mut a.gif_confirmation, "GIF");
    check(ui, &mut a.voice_confirmation, "Голосовые");
    check(ui, &mut a.round_confirmation, "Видеокружки");
}

fn other_tab(ui: &mut egui::Ui, s: &mut Settings) {
    let a = &mut s.ayu;
    check(ui, &mut a.disable_ads, "Отключить рекламу");
    check(ui, &mut a.disable_stories, "Отключить истории");
    check(ui, &mut a.hide_premium_statuses, "Скрывать статусы Premium");
    check(ui, &mut a.local_premium, "Локальный Premium (только интерфейс)");
    check(ui, &mut a.show_only_added_emojis_and_stickers, "Только добавленные эмодзи и стикеры");
    check(ui, &mut a.unlimited_recent_stickers, "Неограниченные недавние стикеры");
    check(ui, &mut a.disable_greeting_sticker, "Без стикера-приветствия");
    check(ui, &mut a.hide_fast_share, "Скрыть быструю пересылку");
    check(ui, &mut a.hide_notification_counters, "Скрыть счётчики уведомлений");
    check(ui, &mut a.hide_notification_badge, "Скрыть значок уведомлений");
    check(ui, &mut a.hide_all_chats_folder, "Скрыть папку «Все чаты»");
    check(ui, &mut a.disable_notifications_delay, "Без задержки уведомлений");
    check(ui, &mut a.disable_open_link_warning, "Не предупреждать об открытии ссылок");
    check(ui, &mut a.show_message_shot, "Снимок сообщения");
    check(ui, &mut a.streamer_mode, "Режим стримера");
    ui.separator();
    ui.label(RichText::new("Веб-приложения").strong());
    check(ui, &mut a.spoof_webview_as_android, "Представляться Android");
    check(ui, &mut a.increase_webview_height, "Увеличить высоту");
    check(ui, &mut a.increase_webview_width, "Увеличить ширину");
    ui.separator();
    ui.weak("Отправка отчётов о сбоях и обновления из интернета в Rustgramm отсутствуют.");
}
