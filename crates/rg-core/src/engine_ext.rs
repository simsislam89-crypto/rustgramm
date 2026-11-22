//! Расширенные методы `Engine`: публикация/просмотр историй, отправка голосовых,
//! администрирование групп/каналов, секретные чаты E2E и сигналинг голосовых/видеозвонков.

use std::path::{Path, PathBuf};

use grammers_client::tl;
use grammers_client::update::Update;
use grammers_session::types::PeerRef;
use rg_ayu::ghost;
use rg_ayu::store::{SecretChatRecord, SecretMessageRecord};

use super::{
    AdminLogRow, AdminRights, BannedRights, Engine, InviteLinkRow, Live, StoryPrivacy,
};

fn rand_i64() -> i64 {
    let mut b = [0u8; 8];
    let _ = getrandom::fill(&mut b);
    i64::from_le_bytes(b)
}

fn rand_i32_pos() -> i32 {
    let mut b = [0u8; 4];
    let _ = getrandom::fill(&mut b);
    (i32::from_le_bytes(b) & 0x7fff_ffff).max(1)
}

fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

fn peer_to_input_channel(peer: PeerRef) -> Result<tl::enums::InputChannel, String> {
    match tl::enums::InputPeer::from(peer) {
        tl::enums::InputPeer::Channel(c) => Ok(tl::enums::InputChannel::Channel(tl::types::InputChannel {
            channel_id: c.channel_id,
            access_hash: c.access_hash,
        })),
        _ => Err("Управление доступно только для супергрупп и каналов".to_owned()),
    }
}

fn peer_to_input_user(peer: PeerRef) -> Result<tl::enums::InputUser, String> {
    match tl::enums::InputPeer::from(peer) {
        tl::enums::InputPeer::User(u) => Ok(tl::enums::InputUser::User(tl::types::InputUser {
            user_id: u.user_id,
            access_hash: u.access_hash,
        })),
        tl::enums::InputPeer::PeerSelf => Ok(tl::enums::InputUser::UserSelf),
        _ => Err("Нужен пользователь Telegram".to_owned()),
    }
}

impl Engine {
    // =========================================================================
    // Истории: публикация, реакции и загрузка медиа для просмотрщика
    // =========================================================================

    /// Опубликовать свою историю (фото или видео) через `stories.sendStory`.
    pub async fn send_story(
        &self,
        path: &Path,
        caption: &str,
        privacy: StoryPrivacy,
        period_secs: i32,
        pinned: bool,
        noforwards: bool,
    ) -> Result<(), String> {
        let uploaded = self
            .client
            .upload_file(path)
            .await
            .map_err(|e| format!("Загрузка файла истории: {e}"))?;
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_default();
        let is_video = matches!(ext.as_str(), "mp4" | "mov" | "webm" | "mkv" | "m4v");
        let media = if is_video {
            tl::enums::InputMedia::UploadedDocument(tl::types::InputMediaUploadedDocument {
                nosound_video: false,
                force_file: false,
                spoiler: false,
                file: uploaded.raw,
                thumb: None,
                mime_type: if ext == "webm" { "video/webm" } else { "video/mp4" }.to_owned(),
                attributes: vec![tl::enums::DocumentAttribute::Video(tl::types::DocumentAttributeVideo {
                    round_message: false,
                    supports_streaming: true,
                    nosound: false,
                    duration: 0.0,
                    w: 720,
                    h: 1280,
                    preload_prefix_size: None,
                    video_start_ts: None,
                    video_codec: None,
                })],
                stickers: None,
                video_cover: None,
                video_timestamp: None,
                ttl_seconds: None,
            })
        } else {
            tl::enums::InputMedia::UploadedPhoto(tl::types::InputMediaUploadedPhoto {
                spoiler: false,
                live_photo: false,
                file: uploaded.raw,
                stickers: None,
                ttl_seconds: None,
                video: None,
            })
        };
        let privacy_rules = match privacy {
            StoryPrivacy::Everyone => vec![tl::enums::InputPrivacyRule::InputPrivacyValueAllowAll],
            StoryPrivacy::Contacts => vec![tl::enums::InputPrivacyRule::InputPrivacyValueAllowContacts],
            StoryPrivacy::CloseFriends => vec![tl::enums::InputPrivacyRule::InputPrivacyValueAllowCloseFriends],
            StoryPrivacy::OnlyMe => vec![tl::enums::InputPrivacyRule::InputPrivacyValueDisallowAll],
        };
        let cap = caption.trim();
        self.apply_traffic_jitter().await;
        self.client
            .invoke(&tl::functions::stories::SendStory {
                pinned,
                noforwards,
                fwd_modified: false,
                peer: tl::enums::InputPeer::PeerSelf,
                media,
                media_areas: None,
                caption: (!cap.is_empty()).then(|| cap.to_owned()),
                entities: None,
                privacy_rules,
                random_id: rand_i64(),
                period: Some(period_secs.clamp(21_600, 172_800)),
                fwd_from_id: None,
                fwd_from_story: None,
                albums: None,
                music: None,
            })
            .await
            .map_err(|e| format!("Публикация истории: {e}"))?;
        Ok(())
    }

    /// Поставить реакцию на историю (`stories.sendReaction`).
    pub async fn react_story(&self, peer: PeerRef, story_id: i32, emoji: &str) -> Result<(), String> {
        self.apply_traffic_jitter().await;
        self.client
            .invoke(&tl::functions::stories::SendReaction {
                add_to_recent: true,
                peer: peer.into(),
                story_id,
                reaction: tl::enums::Reaction::Emoji(tl::types::ReactionEmoji {
                    emoticon: emoji.to_owned(),
                }),
            })
            .await
            .map_err(|e| format!("Реакция на историю: {e}"))?;
        Ok(())
    }

    /// Скачивает картинку/миниатюру истории для отображения в просмотрщике (`stories.getStoriesByID`).
    pub async fn story_media_path(&self, peer: PeerRef, story_id: i32) -> Result<Option<PathBuf>, String> {
        let dir = self.dirs.cache.join("stories");
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join(format!("story_{}_{}.jpg", peer.id.bot_api_dialog_id_unchecked(), story_id));
        if dest.exists() && std::fs::metadata(&dest).is_ok_and(|m| m.len() > 0) {
            return Ok(Some(dest));
        }
        let tl::enums::stories::Stories::Stories(st) = self
            .client
            .invoke(&tl::functions::stories::GetStoriesById {
                peer: peer.into(),
                id: vec![story_id],
            })
            .await
            .map_err(|e| format!("Загрузка истории: {e}"))?;
        let Some(tl::enums::StoryItem::Item(item)) = st.stories.into_iter().next() else {
            return Ok(None);
        };
        let loc = match item.media {
            tl::enums::MessageMedia::Photo(p) => {
                if let Some(tl::enums::Photo::Photo(ph)) = p.photo {
                    let sz = ph
                        .sizes
                        .iter()
                        .rev()
                        .find_map(|s| match s {
                            tl::enums::PhotoSize::Size(x) => Some(x.r#type.clone()),
                            tl::enums::PhotoSize::Progressive(x) => Some(x.r#type.clone()),
                            _ => None,
                        })
                        .unwrap_or_else(|| "x".to_owned());
                    Some(tl::enums::InputFileLocation::InputPhotoFileLocation(
                        tl::types::InputPhotoFileLocation {
                            id: ph.id,
                            access_hash: ph.access_hash,
                            file_reference: ph.file_reference,
                            thumb_size: sz,
                        },
                    ))
                } else {
                    None
                }
            }
            tl::enums::MessageMedia::Document(d) => {
                if let Some(tl::enums::Document::Document(doc)) = d.document {
                    let thumb = doc
                        .thumbs
                        .as_ref()
                        .and_then(|ts| {
                            ts.iter().rev().find_map(|s| match s {
                                tl::enums::PhotoSize::Size(x) => Some(x.r#type.clone()),
                                _ => None,
                            })
                        })
                        .unwrap_or_default();
                    Some(tl::enums::InputFileLocation::InputDocumentFileLocation(
                        tl::types::InputDocumentFileLocation {
                            id: doc.id,
                            access_hash: doc.access_hash,
                            file_reference: doc.file_reference,
                            thumb_size: thumb,
                        },
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some(location) = loc else {
            return Ok(None);
        };
        let mut out = Vec::new();
        let mut offset = 0i64;
        loop {
            let res = self
                .client
                .invoke(&tl::functions::upload::GetFile {
                    precise: false,
                    cdn_supported: false,
                    location: location.clone(),
                    offset,
                    limit: 512 * 1024,
                })
                .await
                .map_err(|e| format!("Скачивание медиа истории: {e}"))?;
            let chunk = match res {
                tl::enums::upload::File::File(f) => f.bytes,
                tl::enums::upload::File::CdnRedirect(_) => break,
            };
            let n = chunk.len();
            out.extend_from_slice(&chunk);
            offset += n as i64;
            if n < 512 * 1024 || out.len() > 8 * 1024 * 1024 {
                break;
            }
        }
        if out.is_empty() {
            return Ok(None);
        }
        std::fs::write(&dest, &out).map_err(|e| e.to_string())?;
        Ok(Some(dest))
    }

    // =========================================================================
    // Отправка записанного голосового сообщения (`DocumentAttributeAudio { voice: true }`)
    // =========================================================================

    /// Отправляет записанное голосовое сообщение с волной (`waveform`) и длительностью.
    pub async fn send_voice(
        &self,
        peer: PeerRef,
        path: &Path,
        duration_secs: u32,
        waveform_peaks: &[u8],
        reply_to: Option<i32>,
    ) -> Result<(), String> {
        let cfg = self.settings();
        let ext = path
            .extension()
            .and_then(|e| e.to_str())
            .map(str::to_ascii_lowercase)
            .unwrap_or_else(|| "wav".to_owned());
        let mime = match ext.as_str() {
            "ogg" | "opus" => "audio/ogg",
            "m4a" | "aac" => "audio/mp4",
            "mp3" => "audio/mpeg",
            _ => "audio/wav",
        };
        let mut file = tokio::fs::File::open(path)
            .await
            .map_err(|e| format!("Чтение голосового: {e}"))?;
        let len = file.metadata().await.map_or(0, |m| m.len()) as usize;
        let uploaded = self
            .client
            .upload_stream(&mut file, len, format!("voice.{ext}"))
            .await
            .map_err(|e| format!("Загрузка голосового: {e}"))?;
        let wf = (!waveform_peaks.is_empty()).then(|| crate::media::encode_waveform(waveform_peaks));
        let media = tl::enums::InputMedia::UploadedDocument(tl::types::InputMediaUploadedDocument {
            nosound_video: false,
            force_file: false,
            spoiler: false,
            file: uploaded.raw,
            thumb: None,
            mime_type: mime.to_owned(),
            attributes: vec![tl::enums::DocumentAttribute::Audio(tl::types::DocumentAttributeAudio {
                voice: true,
                duration: duration_secs.max(1) as i32,
                title: None,
                performer: None,
                waveform: wf,
            })],
            stickers: None,
            video_cover: None,
            video_timestamp: None,
            ttl_seconds: None,
        });
        let reply = reply_to.map(|id| {
            tl::enums::InputReplyTo::Message(tl::types::InputReplyToMessage {
                reply_to_msg_id: id,
                top_msg_id: None,
                reply_to_peer_id: None,
                quote_text: None,
                quote_entities: None,
                quote_offset: None,
                monoforum_peer_id: None,
                todo_item_id: None,
                poll_option: None,
                quote_rich: None,
            })
        });
        self.apply_traffic_jitter().await;
        self.client
            .invoke(&tl::functions::messages::SendMedia {
                silent: cfg.ghost.should_send_without_sound(),
                background: false,
                clear_draft: false,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: false,
                allow_paid_floodskip: false,
                peer: peer.into(),
                reply_to: reply,
                media,
                message: String::new(),
                random_id: rand_i64(),
                reply_markup: None,
                entities: None,
                schedule_date: ghost::schedule_date(&cfg.ghost),
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            })
            .await
            .map_err(|e| format!("Отправка голосового: {e}"))?;
        self.send_offline_if_needed(&cfg.ghost).await;
        Ok(())
    }

    // =========================================================================
    // Администрирование групп и каналов
    // =========================================================================

    /// Назначить или снять администратора (`channels.editAdmin`).
    pub async fn edit_admin(
        &self,
        peer: PeerRef,
        user: PeerRef,
        rights: AdminRights,
        rank: &str,
    ) -> Result<(), String> {
        let channel = peer_to_input_channel(peer)?;
        let user_id = peer_to_input_user(user)?;
        self.client
            .invoke(&tl::functions::channels::EditAdmin {
                channel,
                user_id,
                admin_rights: rights.to_tl(),
                rank: (!rank.trim().is_empty()).then(|| rank.trim().to_owned()),
            })
            .await
            .map_err(|e| format!("Права администратора: {e}"))?;
        Ok(())
    }

    /// Забанить, замутить или разбанить участника (`channels.editBanned`).
    pub async fn edit_banned(
        &self,
        peer: PeerRef,
        user: PeerRef,
        rights: BannedRights,
    ) -> Result<(), String> {
        let channel = peer_to_input_channel(peer)?;
        self.client
            .invoke(&tl::functions::channels::EditBanned {
                channel,
                participant: user.into(),
                banned_rights: rights.to_tl(),
            })
            .await
            .map_err(|e| format!("Ограничение участника: {e}"))?;
        Ok(())
    }

    /// Изменить общие права всех участников группы по умолчанию (`messages.editChatDefaultBannedRights`).
    pub async fn edit_default_banned_rights(
        &self,
        peer: PeerRef,
        rights: BannedRights,
    ) -> Result<(), String> {
        self.client
            .invoke(&tl::functions::messages::EditChatDefaultBannedRights {
                peer: peer.into(),
                banned_rights: rights.to_tl(),
            })
            .await
            .map_err(|e| format!("Общие права группы: {e}"))?;
        Ok(())
    }

    /// Журнал недавних действий супергруппы/канала (`channels.getAdminLog`).
    pub async fn admin_log(
        &self,
        peer: PeerRef,
        query: &str,
        limit: i32,
    ) -> Result<Vec<AdminLogRow>, String> {
        let channel = peer_to_input_channel(peer)?;
        let tl::enums::channels::AdminLogResults::Results(res) = self
            .client
            .invoke(&tl::functions::channels::GetAdminLog {
                channel,
                q: query.trim().to_owned(),
                events_filter: None,
                admins: None,
                max_id: 0,
                min_id: 0,
                limit: limit.clamp(1, 50),
            })
            .await
            .map_err(|e| format!("Журнал действий: {e}"))?;
        let user_name = |uid: i64| -> String {
            res.users
                .iter()
                .find_map(|u| match u {
                    tl::enums::User::User(y) if y.id == uid => {
                        let n = format!(
                            "{} {}",
                            y.first_name.as_deref().unwrap_or(""),
                            y.last_name.as_deref().unwrap_or("")
                        )
                        .trim()
                        .to_owned();
                        Some(if n.is_empty() { format!("ID {uid}") } else { n })
                    }
                    _ => None,
                })
                .unwrap_or_else(|| format!("ID {uid}"))
        };
        let mut out = Vec::new();
        for tl::enums::ChannelAdminLogEvent::Event(ev) in res.events {
            use tl::enums::ChannelAdminLogEventAction as A;
            let summary = match &ev.action {
                A::ChangeTitle(x) => format!("изменил(а) название: «{}» → «{}»", x.prev_value, x.new_value),
                A::ChangeAbout(x) => format!("изменил(а) описание: «{}»", x.new_value),
                A::ChangeUsername(x) => format!("изменил(а) @имя: @{} → @{}", x.prev_value, x.new_value),
                A::ChangePhoto(_) => "обновил(а) фото группы/канала".to_owned(),
                A::DeleteMessage(x) => match &x.message {
                    tl::enums::Message::Message(m) => format!("удалил(а) сообщение: «{}»", m.message.chars().take(60).collect::<String>()),
                    _ => "удалил(а) сообщение".to_owned(),
                },
                A::EditMessage(x) => match &x.new_message {
                    tl::enums::Message::Message(m) => format!("изменил(а) сообщение: «{}»", m.message.chars().take(60).collect::<String>()),
                    _ => "изменил(а) сообщение".to_owned(),
                },
                A::UpdatePinned(_) => "изменил(а) закреплённое сообщение".to_owned(),
                A::ParticipantJoin => "вступил(а) в группу".to_owned(),
                A::ParticipantLeave => "покинул(а) группу".to_owned(),
                A::ParticipantInvite(_) => "пригласил(а) участника".to_owned(),
                A::ParticipantToggleBan(_) => "изменил(а) бан/мут участника".to_owned(),
                A::ParticipantToggleAdmin(_) => "изменил(а) права администратора".to_owned(),
                A::DefaultBannedRights(_) => "изменил(а) права участников по умолчанию".to_owned(),
                A::ToggleSlowMode(x) => format!("медленный режим: {} с", x.new_value),
                A::ChangeHistoryTtl(x) => format!("автоудаление: {} с", x.new_value),
                A::ExportedInviteRevoke(_) => "отозвал(а) ссылку-приглашение".to_owned(),
                A::ExportedInviteDelete(_) => "удалил(а) ссылку-приглашение".to_owned(),
                _ => "административное действие".to_owned(),
            };
            out.push(AdminLogRow {
                id: ev.id,
                date: ev.date,
                actor: user_name(ev.user_id),
                summary,
            });
        }
        Ok(out)
    }

    /// Список пригласительных ссылок группы/канала (`messages.getExportedChatInvites`).
    pub async fn chat_invites(&self, peer: PeerRef) -> Result<Vec<InviteLinkRow>, String> {
        let tl::enums::messages::ExportedChatInvites::Invites(res) = self
            .client
            .invoke(&tl::functions::messages::GetExportedChatInvites {
                revoked: false,
                peer: peer.into(),
                admin_id: tl::enums::InputUser::UserSelf,
                offset_date: None,
                offset_link: None,
                limit: 30,
            })
            .await
            .map_err(|e| format!("Пригласительные ссылки: {e}"))?;
        let mut out = Vec::new();
        for inv in res.invites {
            if let tl::enums::ExportedChatInvite::ChatInviteExported(x) = inv {
                out.push(InviteLinkRow {
                    link: x.link,
                    title: x.title.unwrap_or_default(),
                    usage: x.usage.unwrap_or(0),
                    usage_limit: x.usage_limit,
                    expire_date: x.expire_date,
                    revoked: x.revoked,
                    request_needed: x.request_needed,
                });
            }
        }
        Ok(out)
    }

    /// Создать новую пригласительную ссылку (`messages.exportChatInvite`).
    pub async fn create_invite(
        &self,
        peer: PeerRef,
        title: &str,
        expire_date: Option<i32>,
        usage_limit: Option<i32>,
        request_needed: bool,
    ) -> Result<InviteLinkRow, String> {
        let inv = self
            .client
            .invoke(&tl::functions::messages::ExportChatInvite {
                legacy_revoke_permanent: false,
                request_needed,
                peer: peer.into(),
                expire_date,
                usage_limit,
                title: (!title.trim().is_empty()).then(|| title.trim().to_owned()),
                subscription_pricing: None,
            })
            .await
            .map_err(|e| format!("Создание ссылки: {e}"))?;
        match inv {
            tl::enums::ExportedChatInvite::ChatInviteExported(x) => Ok(InviteLinkRow {
                link: x.link,
                title: x.title.unwrap_or_default(),
                usage: x.usage.unwrap_or(0),
                usage_limit: x.usage_limit,
                expire_date: x.expire_date,
                revoked: x.revoked,
                request_needed: x.request_needed,
            }),
            _ => Err("Не удалось получить ссылку".to_owned()),
        }
    }

    /// Отозвать пригласительную ссылку (`messages.editExportedChatInvite`).
    pub async fn revoke_invite(&self, peer: PeerRef, link: &str) -> Result<(), String> {
        self.client
            .invoke(&tl::functions::messages::EditExportedChatInvite {
                revoked: true,
                peer: peer.into(),
                link: link.to_owned(),
                expire_date: None,
                usage_limit: None,
                request_needed: None,
                title: None,
            })
            .await
            .map_err(|e| format!("Отзыв ссылки: {e}"))?;
        Ok(())
    }

    // =========================================================================
    // Секретные чаты (MTProto 2.0 E2E)
    // =========================================================================

    /// Запрашивает и проверяет 2048-битные DH-параметры сервера (`messages.getDhConfig`).
    pub async fn dh_params(&self) -> Result<(i32, Vec<u8>, Vec<u8>), String> {
        let res = self
            .client
            .invoke(&tl::functions::messages::GetDhConfig {
                version: 0,
                random_length: 256,
            })
            .await
            .map_err(|e| format!("DH-параметры: {e}"))?;
        match res {
            tl::enums::messages::DhConfig::Config(c) => {
                crate::secret::validate_dh_params(c.g, &c.p)?;
                Ok((c.g, c.p, c.random))
            }
            tl::enums::messages::DhConfig::NotModified(_) => {
                Err("Сервер не вернул параметры DH".to_owned())
            }
        }
    }

    /// Создаёт исходящий секретный чат E2E с пользователем (`messages.requestEncryption`).
    pub async fn request_secret_chat(
        &self,
        user: PeerRef,
        peer_name: &str,
    ) -> Result<SecretChatRecord, String> {
        let user_id = peer_to_input_user(user)?;
        let (g, p, rnd) = self.dh_params().await?;
        let (a, g_a) = crate::secret::generate_dh_keypair(g, &p, &rnd)?;
        let random_id = rand_i32_pos();
        let res = self
            .client
            .invoke(&tl::functions::messages::RequestEncryption {
                user_id,
                random_id,
                g_a: g_a.clone(),
            })
            .await
            .map_err(|e| format!("Создание секретного чата: {e}"))?;
        let (chat_id, access_hash, participant_id) = match res {
            tl::enums::EncryptedChat::Waiting(w) => (w.id, w.access_hash, w.participant_id),
            tl::enums::EncryptedChat::Chat(c) => (c.id, c.access_hash, c.participant_id),
            _ => return Err("Неожиданный ответ сервера на requestEncryption".to_owned()),
        };
        if let Ok(mut map) = self.pending_dh.lock() {
            map.insert(i64::from(chat_id), (a, g_a, p));
        }
        let rec = SecretChatRecord {
            chat_id,
            access_hash,
            peer_user_id: participant_id,
            peer_name: peer_name.to_owned(),
            auth_key: Vec::new(),
            key_fingerprint: 0,
            ttl_secs: 0,
            state: "waiting".to_owned(),
            created_at: unix_now(),
            is_admin: true,
        };
        let _ = self.store.save_secret_chat(&rec).await;
        Ok(rec)
    }

    /// Принимает входящий запрос секретного чата (`messages.acceptEncryption`).
    pub async fn accept_secret_chat(
        &self,
        chat_id: i32,
        access_hash: i64,
        g_a: &[u8],
        peer_user_id: i64,
        peer_name: &str,
    ) -> Result<SecretChatRecord, String> {
        let (g, p, rnd) = self.dh_params().await?;
        let (b, g_b) = crate::secret::generate_dh_keypair(g, &p, &rnd)?;
        let shared = crate::secret::compute_shared_key(g_a, &b, &p)?;
        let fp = crate::secret::key_fingerprint(&shared);
        self.client
            .invoke(&tl::functions::messages::AcceptEncryption {
                peer: tl::enums::InputEncryptedChat::Chat(tl::types::InputEncryptedChat {
                    chat_id,
                    access_hash,
                }),
                g_b,
                key_fingerprint: fp,
            })
            .await
            .map_err(|e| format!("Принятие секретного чата: {e}"))?;
        let rec = SecretChatRecord {
            chat_id,
            access_hash,
            peer_user_id,
            peer_name: peer_name.to_owned(),
            auth_key: shared.to_vec(),
            key_fingerprint: fp,
            ttl_secs: 0,
            state: "ready".to_owned(),
            created_at: unix_now(),
            is_admin: false,
        };
        let _ = self.store.save_secret_chat(&rec).await;
        Ok(rec)
    }

    /// Отправляет зашифрованное E2E-сообщение в активный секретный чат (`messages.sendEncrypted`).
    pub async fn send_secret_text(&self, chat_id: i32, text: &str) -> Result<SecretMessageRecord, String> {
        let chats = self.store.list_secret_chats().await.unwrap_or_default();
        let Some(chat) = chats.into_iter().find(|c| c.chat_id == chat_id) else {
            return Err("Секретный чат не найден".to_owned());
        };
        if chat.state != "ready" || chat.auth_key.len() != 256 {
            return Err("Ожидаем подтверждение E2E-ключа от собеседника".to_owned());
        }
        let mut key = [0u8; 256];
        key.copy_from_slice(&chat.auth_key);
        let random_id = rand_i64();
        let tl_plain = crate::secret::encode_secret_text(random_id, chat.ttl_secs, text, 0, 0);
        let data = crate::secret::encrypt_secret_payload(&key, &tl_plain, chat.is_admin);
        let cfg = self.settings();
        self.apply_traffic_jitter().await;
        self.client
            .invoke(&tl::functions::messages::SendEncrypted {
                silent: cfg.ghost.should_send_without_sound(),
                peer: tl::enums::InputEncryptedChat::Chat(tl::types::InputEncryptedChat {
                    chat_id: chat.chat_id,
                    access_hash: chat.access_hash,
                }),
                random_id,
                data,
            })
            .await
            .map_err(|e| format!("Отправка E2E: {e}"))?;
        let now = unix_now();
        let msg = SecretMessageRecord {
            id: 0,
            chat_id,
            random_id,
            outgoing: true,
            text: text.to_owned(),
            date: now,
            ttl_secs: chat.ttl_secs,
            expires_at: if chat.ttl_secs > 0 { now + i64::from(chat.ttl_secs) } else { 0 },
        };
        let _ = self.store.save_secret_message(&msg).await;
        Ok(msg)
    }

    /// Устанавливает таймер самоуничтожения сообщений в секретном чате (`messages.sendEncryptedService`).
    pub async fn set_secret_chat_ttl(&self, chat_id: i32, ttl_secs: i32) -> Result<(), String> {
        let chats = self.store.list_secret_chats().await.unwrap_or_default();
        let Some(chat) = chats.into_iter().find(|c| c.chat_id == chat_id) else {
            return Err("Секретный чат не найден".to_owned());
        };
        if chat.state == "ready" && chat.auth_key.len() == 256 {
            let mut key = [0u8; 256];
            key.copy_from_slice(&chat.auth_key);
            let random_id = rand_i64();
            let tl_plain = crate::secret::encode_secret_set_ttl(random_id, ttl_secs, 0, 0);
            let data = crate::secret::encrypt_secret_payload(&key, &tl_plain, chat.is_admin);
            self.client
                .invoke(&tl::functions::messages::SendEncryptedService {
                    peer: tl::enums::InputEncryptedChat::Chat(tl::types::InputEncryptedChat {
                        chat_id: chat.chat_id,
                        access_hash: chat.access_hash,
                    }),
                    random_id,
                    data,
                })
                .await
                .map_err(|e| format!("Таймер E2E: {e}"))?;
        }
        let _ = self.store.set_secret_ttl(chat_id, ttl_secs).await;
        Ok(())
    }

    /// Завершает и стирает секретный чат (`messages.discardEncryption`).
    pub async fn discard_secret_chat(&self, chat_id: i32) -> Result<(), String> {
        let _ = self
            .client
            .invoke(&tl::functions::messages::DiscardEncryption {
                delete_history: true,
                chat_id,
            })
            .await;
        if let Ok(mut map) = self.pending_dh.lock() {
            map.remove(&i64::from(chat_id));
        }
        let _ = self.store.delete_secret_chat(chat_id).await;
        Ok(())
    }

    pub async fn secret_chats(&self) -> Vec<SecretChatRecord> {
        self.store.list_secret_chats().await.unwrap_or_default()
    }

    pub async fn secret_messages(&self, chat_id: i32) -> Vec<SecretMessageRecord> {
        self.store
            .secret_messages(chat_id, unix_now())
            .await
            .unwrap_or_default()
    }

    /// Обрабатывает входящие обновления `Update::Encryption` и `Update::NewEncryptedMessage`.
    pub async fn process_secret_update(&self, upd: &Update) -> Option<Live> {
        let Update::Raw(raw) = upd else { return None };
        use tl::enums::Update as U;
        match &raw.raw {
            U::Encryption(enc) => match &enc.chat {
                tl::enums::EncryptedChat::Requested(req) => {
                    // Входящий запрос секретного чата — автоматически принимаем DH и переходим в `ready`.
                    let name = format!("Пользователь {}", req.admin_id);
                    if let Ok(rec) = self
                        .accept_secret_chat(req.id, req.access_hash, &req.g_a, req.admin_id, &name)
                        .await
                    {
                        return Some(Live::SecretChatUpdate(rec));
                    }
                    None
                }
                tl::enums::EncryptedChat::Chat(c) => {
                    // Собеседник принял наш запрос: завершаем вычисление общего ключа `g_b^a mod p`.
                    let dh = self
                        .pending_dh
                        .lock()
                        .ok()
                        .and_then(|mut m| m.remove(&i64::from(c.id)));
                    if let Some((a, _g_a, p)) = dh
                        && let Ok(shared) = crate::secret::compute_shared_key(&c.g_a_or_b, &a, &p)
                    {
                        let fp = crate::secret::key_fingerprint(&shared);
                        if fp == c.key_fingerprint {
                            let existing = self
                                .store
                                .list_secret_chats()
                                .await
                                .unwrap_or_default()
                                .into_iter()
                                .find(|x| x.chat_id == c.id);
                            let peer_name = existing
                                .as_ref()
                                .map(|x| x.peer_name.clone())
                                .unwrap_or_else(|| format!("Секретный чат #{}", c.id));
                            let rec = SecretChatRecord {
                                chat_id: c.id,
                                access_hash: c.access_hash,
                                peer_user_id: c.participant_id,
                                peer_name,
                                auth_key: shared.to_vec(),
                                key_fingerprint: fp,
                                ttl_secs: existing.as_ref().map_or(0, |x| x.ttl_secs),
                                state: "ready".to_owned(),
                                created_at: existing.as_ref().map_or_else(unix_now, |x| x.created_at),
                                is_admin: true,
                            };
                            let _ = self.store.save_secret_chat(&rec).await;
                            return Some(Live::SecretChatUpdate(rec));
                        }
                    }
                    None
                }
                tl::enums::EncryptedChat::Discarded(d) => {
                    let _ = self.store.delete_secret_chat(d.id).await;
                    Some(Live::SecretChatUpdate(SecretChatRecord {
                        chat_id: d.id,
                        access_hash: 0,
                        peer_user_id: 0,
                        peer_name: String::new(),
                        auth_key: Vec::new(),
                        key_fingerprint: 0,
                        ttl_secs: 0,
                        state: "discarded".to_owned(),
                        created_at: unix_now(),
                        is_admin: false,
                    }))
                }
                _ => None,
            },
            U::NewEncryptedMessage(em) => {
                let (chat_id, random_id, date, bytes) = match &em.message {
                    tl::enums::EncryptedMessage::Message(m) => (m.chat_id, m.random_id, i64::from(m.date), m.bytes.as_slice()),
                    tl::enums::EncryptedMessage::Service(m) => (m.chat_id, m.random_id, i64::from(m.date), m.bytes.as_slice()),
                };
                let chats = self.store.list_secret_chats().await.unwrap_or_default();
                let chat = chats.into_iter().find(|c| c.chat_id == chat_id)?;
                if chat.auth_key.len() != 256 {
                    return None;
                }
                let mut key = [0u8; 256];
                key.copy_from_slice(&chat.auth_key);
                let plain = crate::secret::decrypt_secret_payload(&key, bytes, !chat.is_admin).ok()?;
                match crate::secret::decode_secret_payload(&plain) {
                    crate::secret::SecretPayload::Text { ttl, text, .. } => {
                        let eff_ttl = if ttl > 0 { ttl } else { chat.ttl_secs };
                        let now = unix_now();
                        let msg = SecretMessageRecord {
                            id: 0,
                            chat_id,
                            random_id,
                            outgoing: false,
                            text,
                            date,
                            ttl_secs: eff_ttl,
                            expires_at: if eff_ttl > 0 { now + i64::from(eff_ttl) } else { 0 },
                        };
                        let _ = self.store.save_secret_message(&msg).await;
                        Some(Live::SecretMsg(msg))
                    }
                    crate::secret::SecretPayload::SetTtl { ttl, .. } => {
                        let _ = self.store.set_secret_ttl(chat_id, ttl).await;
                        let mut updated = chat;
                        updated.ttl_secs = ttl;
                        Some(Live::SecretChatUpdate(updated))
                    }
                    crate::secret::SecretPayload::Unknown => None,
                }
            }
            _ => None,
        }
    }

    // =========================================================================
    // Голосовые и видеозвонки (`phone.*`)
    // =========================================================================

    /// Запрашивает ICE/STUN/TURN конфигурацию звонков (`phone.getCallConfig`).
    pub async fn call_config(&self) -> Result<String, String> {
        let tl::enums::DataJson::Json(j) = self
            .client
            .invoke(&tl::functions::phone::GetCallConfig {})
            .await
            .map_err(|e| format!("Конфигурация звонков: {e}"))?;
        Ok(j.data)
    }

    /// Начинает исходящий голосовой или видеозвонок (`phone.requestCall`).
    pub async fn start_call(&self, user: PeerRef, video: bool) -> Result<(i64, i64), String> {
        let user_id = peer_to_input_user(user)?;
        let (g, p, rnd) = self.dh_params().await?;
        let (a, g_a) = crate::secret::generate_dh_keypair(g, &p, &rnd)?;
        let g_a_hash = crate::calls::ga_hash(&g_a).to_vec();
        let tl::enums::phone::PhoneCall::Call(pc) = self
            .client
            .invoke(&tl::functions::phone::RequestCall {
                video,
                user_id,
                random_id: rand_i32_pos(),
                g_a_hash,
                protocol: crate::calls::default_call_protocol(),
            })
            .await
            .map_err(|e| format!("Вызов не удался: {e}"))?;
        let (call_id, access_hash) = match pc.phone_call {
            tl::enums::PhoneCall::Waiting(w) => (w.id, w.access_hash),
            tl::enums::PhoneCall::Requested(r) => (r.id, r.access_hash),
            tl::enums::PhoneCall::Call(c) => (c.id, c.access_hash),
            _ => return Err("Сервер отклонил вызов".to_owned()),
        };
        if let Ok(mut map) = self.pending_dh.lock() {
            map.insert(call_id, (a, g_a, p));
        }
        Ok((call_id, access_hash))
    }

    /// Принимает входящий голосовой/видеозвонок (`phone.acceptCall`).
    pub async fn accept_call(&self, call_id: i64, access_hash: i64) -> Result<(), String> {
        let (g, p, rnd) = self.dh_params().await?;
        let (b, g_b) = crate::secret::generate_dh_keypair(g, &p, &rnd)?;
        self.client
            .invoke(&tl::functions::phone::AcceptCall {
                peer: tl::enums::InputPhoneCall::Call(tl::types::InputPhoneCall {
                    id: call_id,
                    access_hash,
                }),
                g_b: g_b.clone(),
                protocol: crate::calls::default_call_protocol(),
            })
            .await
            .map_err(|e| format!("Принятие звонка: {e}"))?;
        if let Ok(mut map) = self.pending_dh.lock() {
            map.insert(call_id, (b, g_b, p));
        }
        Ok(())
    }

    /// Подтверждает исходящий звонок после `PhoneCallAccepted` (`phone.confirmCall`) и возвращает 4 E2E-эмодзи.
    pub async fn confirm_call(
        &self,
        call_id: i64,
        access_hash: i64,
        g_b: &[u8],
    ) -> Result<[&'static str; 4], String> {
        let (a, g_a, p) = self
            .pending_dh
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&call_id))
            .ok_or_else(|| "DH-сессия звонка не найдена".to_owned())?;
        let shared = crate::secret::compute_shared_key(g_b, &a, &p)?;
        let fp = crate::secret::key_fingerprint(&shared);
        self.client
            .invoke(&tl::functions::phone::ConfirmCall {
                peer: tl::enums::InputPhoneCall::Call(tl::types::InputPhoneCall {
                    id: call_id,
                    access_hash,
                }),
                g_a: g_a.clone(),
                key_fingerprint: fp,
                protocol: crate::calls::default_call_protocol(),
            })
            .await
            .map_err(|e| format!("Подтверждение звонка: {e}"))?;
        Ok(crate::calls::call_emojis(&shared, &g_a))
    }

    /// Вычисляет 4 E2E-эмодзи для принятого входящего звонка (когда пришёл `PhoneCall::Call` с `g_a`).
    pub fn derive_incoming_call_emojis(
        &self,
        call_id: i64,
        g_a: &[u8],
    ) -> Result<[&'static str; 4], String> {
        let (b, _g_b, p) = self
            .pending_dh
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&call_id))
            .ok_or_else(|| "DH-сессия входящего звонка не найдена".to_owned())?;
        let shared = crate::secret::compute_shared_key(g_a, &b, &p)?;
        Ok(crate::calls::call_emojis(&shared, g_a))
    }

    /// Завершает или отклоняет звонок (`phone.discardCall`).
    pub async fn discard_call(
        &self,
        call_id: i64,
        access_hash: i64,
        duration: i32,
        video: bool,
        busy: bool,
    ) -> Result<(), String> {
        if let Ok(mut map) = self.pending_dh.lock() {
            map.remove(&call_id);
        }
        let reason = if busy {
            tl::enums::PhoneCallDiscardReason::Busy
        } else {
            tl::enums::PhoneCallDiscardReason::Hangup
        };
        let _ = self
            .client
            .invoke(&tl::functions::phone::DiscardCall {
                video,
                peer: tl::enums::InputPhoneCall::Call(tl::types::InputPhoneCall {
                    id: call_id,
                    access_hash,
                }),
                duration: duration.max(0),
                reason,
                connection_id: 0,
            })
            .await;
        Ok(())
    }
}
