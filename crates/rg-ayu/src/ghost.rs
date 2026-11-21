//! Политика режима призрака: решает, отправлять ли служебный запрос в Telegram.
//! Решение принимается по идентификатору конструктора TL, поэтому его можно
//! поставить перед любым `client.invoke`.

use grammers_tl_types::{Identifiable, functions};
use rg_settings::GhostMode;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Send,
    Drop,
}

impl Verdict {
    fn when(send: bool) -> Self {
        if send { Self::Send } else { Self::Drop }
    }
}

/// Запросы «прочитано» для сообщений и их содержимого.
const READ_MESSAGES: [u32; 4] = [
    functions::messages::ReadHistory::CONSTRUCTOR_ID,
    functions::channels::ReadHistory::CONSTRUCTOR_ID,
    functions::messages::ReadMessageContents::CONSTRUCTOR_ID,
    functions::channels::ReadMessageContents::CONSTRUCTOR_ID,
];

/// Запросы «просмотрено» для историй.
const READ_STORIES: [u32; 2] = [
    functions::stories::ReadStories::CONSTRUCTOR_ID,
    functions::stories::IncrementStoryViews::CONSTRUCTOR_ID,
];

/// Решение для запроса по идентификатору конструктора.
/// Статус «в сети» и «печатает» зависят от аргументов — для них отдельные функции.
pub fn verdict(g: &GhostMode, constructor_id: u32) -> Verdict {
    if READ_MESSAGES.contains(&constructor_id) {
        return Verdict::when(g.send_read_messages);
    }
    if READ_STORIES.contains(&constructor_id) {
        return Verdict::when(g.send_read_stories);
    }
    Verdict::Send
}

/// `account.updateStatus`: пакет «в сети» (offline = false) гасится, «не в сети» уходит всегда.
pub fn verdict_status(g: &GhostMode, offline: bool) -> Verdict {
    Verdict::when(offline || g.send_online_packets)
}

/// `messages.setTyping`: гасится только прогресс загрузки файлов.
pub fn verdict_typing(g: &GhostMode, is_upload_action: bool) -> Verdict {
    Verdict::when(!is_upload_action || g.send_upload_progress)
}

/// Нужно ли после отправки сообщения самим отметить чат прочитанным.
pub fn should_mark_read_after_action(g: &GhostMode) -> bool {
    g.active && g.mark_read_after_action
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ghost() -> GhostMode {
        let mut g = GhostMode::default();
        g.set_active(true);
        g
    }

    #[test]
    fn normal_mode_sends_everything() {
        let g = GhostMode::default();
        for id in READ_MESSAGES.iter().chain(READ_STORIES.iter()) {
            assert_eq!(verdict(&g, *id), Verdict::Send);
        }
        assert_eq!(verdict_status(&g, false), Verdict::Send);
        assert_eq!(verdict_typing(&g, true), Verdict::Send);
    }

    #[test]
    fn ghost_mode_drops_read_receipts() {
        let g = ghost();
        for id in READ_MESSAGES.iter().chain(READ_STORIES.iter()) {
            assert_eq!(verdict(&g, *id), Verdict::Drop);
        }
        assert_eq!(verdict_status(&g, false), Verdict::Drop);
        assert_eq!(verdict_status(&g, true), Verdict::Send);
        assert_eq!(verdict_typing(&g, true), Verdict::Drop);
        assert_eq!(verdict_typing(&g, false), Verdict::Send);
    }

    #[test]
    fn unrelated_requests_pass() {
        let g = ghost();
        assert_eq!(
            verdict(&g, functions::messages::SendMessage::CONSTRUCTOR_ID),
            Verdict::Send
        );
    }

    #[test]
    fn locked_setting_keeps_sending() {
        let mut g = GhostMode {
            locked_read_messages: true,
            ..GhostMode::default()
        };
        g.set_active(true);
        assert_eq!(
            verdict(&g, functions::messages::ReadHistory::CONSTRUCTOR_ID),
            Verdict::Send
        );
    }

    #[test]
    fn mark_read_after_action_only_in_ghost() {
        assert!(!should_mark_read_after_action(&GhostMode::default()));
        assert!(should_mark_read_after_action(&ghost()));
    }
}
