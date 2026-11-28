//! Расширенные методы `Engine`: публикация/просмотр историй, отправка голосовых,
//! администрирование групп/каналов, секретные чаты E2E и сигналинг голосовых/видеозвонков.

use std::path::{Path, PathBuf};

use grammers_client::tl;
use grammers_client::update::Update;
use grammers_session::types::PeerRef;
use rg_ayu::ghost;
use rg_ayu::store::{SecretChatRecord, SecretMessageRecord};

use crate::calls::CallEndpoint;

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

    /// Скачивает картинку/миниатюру истории для отображения в просмотрщике (`stories.getStoriesByID`)
    /// с автоматической поддержкой кросс-DC (`dc_id` / `FILE_MIGRATE_X` через `iter_download`).
    pub async fn story_media_path(&self, peer: PeerRef, story_id: i32) -> Result<Option<PathBuf>, String> {
        let dir = self.dirs.cache.join("stories");
        let _ = std::fs::create_dir_all(&dir);
        let dest = dir.join(format!("story_{}_{}.jpg", peer.id.bot_api_dialog_id_unchecked(), story_id));
        if dest.exists() && std::fs::metadata(&dest).is_ok_and(|m| m.len() > 0) {
            return Ok(Some(dest));
        }
        let t0 = std::time::Instant::now();
        let tl::enums::stories::Stories::Stories(st) = self
            .invoke_retry(
                "stories.getStoriesById",
                &tl::functions::stories::GetStoriesById {
                    peer: peer.into(),
                    id: vec![story_id],
                },
            )
            .await
            .map_err(|e| format!("Загрузка истории: {e}"))?;
        let Some(tl::enums::StoryItem::Item(item)) = st.stories.into_iter().next() else {
            return Ok(None);
        };
        let loc_dc: Option<(tl::enums::InputFileLocation, Option<i32>)> = match item.media {
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
                    Some((
                        tl::enums::InputFileLocation::InputPhotoFileLocation(tl::types::InputPhotoFileLocation {
                            id: ph.id,
                            access_hash: ph.access_hash,
                            file_reference: ph.file_reference,
                            thumb_size: sz,
                        }),
                        Some(ph.dc_id),
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
                                tl::enums::PhotoSize::Progressive(x) => Some(x.r#type.clone()),
                                _ => None,
                            })
                        })
                        .unwrap_or_default();
                    Some((
                        tl::enums::InputFileLocation::InputDocumentFileLocation(
                            tl::types::InputDocumentFileLocation {
                                id: doc.id,
                                access_hash: doc.access_hash,
                                file_reference: doc.file_reference,
                                thumb_size: thumb,
                            },
                        ),
                        Some(doc.dc_id),
                    ))
                } else {
                    None
                }
            }
            _ => None,
        };
        let Some((loc, dc_id)) = loc_dc else {
            return Ok(None);
        };
        let dl = super::RawLoc { loc, dc_id };
        let mut it = self.client.iter_download(&dl);
        let mut out = Vec::new();
        while let Some(chunk) = it
            .next()
            .await
            .map_err(|e| format!("Скачивание медиа истории: {e}"))?
        {
            out.extend_from_slice(&chunk);
            if out.len() > 8 * 1024 * 1024 {
                break;
            }
        }
        if out.is_empty() {
            return Ok(None);
        }
        crate::diag::info(
            "STORY",
            &format!(
                "История #{story_id} (dc={:?}) загружена: {} КБ за {} мс",
                dc_id,
                out.len() / 1024,
                t0.elapsed().as_millis()
            ),
        );
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
        let updates = self
            .client
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
        let row = self.row_from_tl_updates(peer, &updates, "[Голосовое сообщение]", cfg.ghost.should_send_without_sound());
        self.remember_row_sync(peer, &row).await;
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

    /// Подтверждает исходящий звонок после `PhoneCallAccepted` (`phone.confirmCall`)
    /// и возвращает 4 E2E-эмодзи, общий 256-байтный ключ и список эндпоинтов медиа-потока.
    pub async fn confirm_call(
        &self,
        call_id: i64,
        access_hash: i64,
        g_b: &[u8],
    ) -> Result<([&'static str; 4], [u8; 256], Vec<CallEndpoint>, bool), String> {
        let (a, g_a, p) = self
            .pending_dh
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&call_id))
            .ok_or_else(|| "DH-сессия звонка не найдена".to_owned())?;
        let shared = crate::secret::compute_shared_key(g_b, &a, &p)?;
        let fp = crate::secret::key_fingerprint(&shared);
        let tl::enums::phone::PhoneCall::Call(pc) = self
            .client
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
        let (endpoints, p2p_allowed) = match pc.phone_call {
            tl::enums::PhoneCall::Call(c) => (
                crate::calls::parse_phone_connections(&c.connections),
                c.p2p_allowed,
            ),
            _ => (Vec::new(), false),
        };
        Ok((crate::calls::call_emojis(&shared, &g_a), shared, endpoints, p2p_allowed))
    }

    /// Вычисляет 4 E2E-эмодзи и 256-байтный ключ для принятого входящего звонка (когда пришёл `PhoneCall::Call` с `g_a`).
    pub fn derive_incoming_call_emojis(
        &self,
        call_id: i64,
        g_a: &[u8],
    ) -> Result<([&'static str; 4], [u8; 256]), String> {
        let (b, _g_b, p) = self
            .pending_dh
            .lock()
            .ok()
            .and_then(|mut m| m.remove(&call_id))
            .ok_or_else(|| "DH-сессия входящего звонка не найдена".to_owned())?;
        let shared = crate::secret::compute_shared_key(g_a, &b, &p)?;
        Ok((crate::calls::call_emojis(&shared, g_a), shared))
    }

    /// Отправляет WebRTC ICE/SDP сигнальные данные в рамках звонка (`phone.sendSignalingData`).
    pub async fn send_call_signaling(
        &self,
        call_id: i64,
        access_hash: i64,
        data: Vec<u8>,
    ) -> Result<(), String> {
        let _ = self
            .client
            .invoke(&tl::functions::phone::SendSignalingData {
                peer: tl::enums::InputPhoneCall::Call(tl::types::InputPhoneCall {
                    id: call_id,
                    access_hash,
                }),
                data,
            })
            .await
            .map_err(|e| format!("Сигналинг звонка: {e}"))?;
        Ok(())
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

    // =========================================================================
    // Telegram Stars, Подарки (Star Gifts), платные реакции и Бизнес-чаты
    // =========================================================================

    /// Запрашивает баланс Telegram Stars и последние транзакции (`payments.getStarsTransactions`).
    pub async fn stars_status(&self) -> Result<StarsSummary, String> {
        let tl::enums::payments::StarsStatus::Status(st) = self
            .client
            .invoke(&tl::functions::payments::GetStarsTransactions {
                inbound: false,
                outbound: false,
                ascending: false,
                ton: false,
                subscription_id: None,
                peer: tl::enums::InputPeer::PeerSelf,
                offset: String::new(),
                limit: 30,
            })
            .await
            .map_err(|e| format!("Баланс Stars: {e}"))?;

        let (balance, nanos) = stars_amount_parts(&st.balance);
        let transactions = st
            .history
            .unwrap_or_default()
            .into_iter()
            .map(|tx| {
                let tl::enums::StarsTransaction::Transaction(t) = tx;
                let (amt, _) = stars_amount_parts(&t.amount);
                let title = t.title.filter(|s| !s.is_empty()).unwrap_or_else(|| {
                    if t.reaction {
                        "Платная реакция".to_owned()
                    } else if t.gift {
                        "Подарок Telegram".to_owned()
                    } else {
                        stars_peer_label(&t.peer)
                    }
                });
                StarsTxRow {
                    id: t.id,
                    amount: amt,
                    date: i64::from(t.date),
                    title,
                    description: t.description.unwrap_or_default(),
                    is_gift: t.gift,
                    is_reaction: t.reaction,
                }
            })
            .collect();
        Ok(StarsSummary {
            balance,
            nanos,
            transactions,
        })
    }

    /// Загружает витрину подарков Telegram Stars (`payments.getStarGifts`).
    pub async fn star_gifts_catalog(&self) -> Result<Vec<StarGiftItem>, String> {
        let res = self
            .client
            .invoke(&tl::functions::payments::GetStarGifts { hash: 0 })
            .await
            .map_err(|e| format!("Каталог подарков: {e}"))?;
        let tl::enums::payments::StarGifts::Gifts(g) = res else {
            return Ok(Vec::new());
        };
        Ok(g.gifts
            .into_iter()
            .map(|gift| match gift {
                tl::enums::StarGift::Gift(x) => StarGiftItem {
                    id: x.id,
                    title: x.title.unwrap_or_else(|| format!("Подарок #{}", x.id)),
                    stars: x.stars,
                    convert_stars: x.convert_stars,
                    limited: x.limited,
                    sold_out: x.sold_out,
                    remains: x.availability_remains,
                    total: x.availability_total,
                    unique: false,
                },
                tl::enums::StarGift::Unique(u) => StarGiftItem {
                    id: u.id,
                    title: format!("{} #{}", u.title, u.num),
                    stars: u.offer_min_stars.map_or(0, i64::from),
                    convert_stars: 0,
                    limited: true,
                    sold_out: false,
                    remains: Some(u.availability_total - u.availability_issued),
                    total: Some(u.availability_total),
                    unique: true,
                },
            })
            .collect())
    }

    /// Загружает подарки, сохранённые в профиле (`payments.getSavedStarGifts`).
    pub async fn saved_star_gifts(&self, peer: PeerRef) -> Result<Vec<SavedGiftItem>, String> {
        let tl::enums::payments::SavedStarGifts::Gifts(res) = self
            .client
            .invoke(&tl::functions::payments::GetSavedStarGifts {
                exclude_unsaved: false,
                exclude_saved: false,
                exclude_unlimited: false,
                exclude_unique: false,
                sort_by_value: false,
                exclude_upgradable: false,
                exclude_unupgradable: false,
                peer_color_available: false,
                exclude_hosted: false,
                peer: tl::enums::InputPeer::from(peer),
                collection_id: None,
                offset: String::new(),
                limit: 40,
            })
            .await
            .map_err(|e| format!("Подарки профиля: {e}"))?;

        Ok(res
            .gifts
            .into_iter()
            .map(|sg| {
                let tl::enums::SavedStarGift::Gift(x) = sg;
                let (title, stars, unique) = match &x.gift {
                    tl::enums::StarGift::Gift(g) => (
                        g.title.clone().unwrap_or_else(|| "Подарок Telegram".to_owned()),
                        g.stars,
                        false,
                    ),
                    tl::enums::StarGift::Unique(u) => (format!("{} #{}", u.title, u.num), 0, true),
                };
                let message = x
                    .message
                    .map(|tl::enums::TextWithEntities::Entities(t)| t.text)
                    .unwrap_or_default();
                SavedGiftItem {
                    msg_id: x.msg_id,
                    saved_id: x.saved_id,
                    title,
                    stars,
                    convert_stars: x.convert_stars.unwrap_or(0),
                    date: i64::from(x.date),
                    message,
                    unsaved: x.unsaved,
                    pinned_to_top: x.pinned_to_top,
                    unique,
                }
            })
            .collect())
    }

    /// Показывает или скрывает подарок в профиле (`payments.saveStarGift`).
    pub async fn toggle_save_star_gift(
        &self,
        msg_id: Option<i32>,
        peer: Option<PeerRef>,
        saved_id: Option<i64>,
        unsave: bool,
    ) -> Result<(), String> {
        let stargift = input_saved_star_gift(msg_id, peer, saved_id)?;
        let _ = self
            .client
            .invoke(&tl::functions::payments::SaveStarGift { unsave, stargift })
            .await
            .map_err(|e| format!("Сохранение подарка: {e}"))?;
        Ok(())
    }

    /// Обменивает полученный подарок на Telegram Stars (`payments.convertStarGift`).
    pub async fn convert_star_gift(
        &self,
        msg_id: Option<i32>,
        peer: Option<PeerRef>,
        saved_id: Option<i64>,
    ) -> Result<(), String> {
        let stargift = input_saved_star_gift(msg_id, peer, saved_id)?;
        let _ = self
            .client
            .invoke(&tl::functions::payments::ConvertStarGift { stargift })
            .await
            .map_err(|e| format!("Обмен подарка: {e}"))?;
        Ok(())
    }

    /// Отправляет платную звёздную реакцию на сообщение/пост (`messages.sendPaidReaction`).
    pub async fn send_paid_reaction(
        &self,
        peer: PeerRef,
        msg_id: i32,
        count: i32,
        anonymous: bool,
    ) -> Result<(), String> {
        let count = count.clamp(1, 2500);
        let privacy = if anonymous {
            Some(tl::enums::PaidReactionPrivacy::Anonymous)
        } else {
            Some(tl::enums::PaidReactionPrivacy::Default)
        };
        let _ = self
            .client
            .invoke(&tl::functions::messages::SendPaidReaction {
                peer: tl::enums::InputPeer::from(peer),
                msg_id,
                count,
                random_id: rand_i64(),
                private: privacy,
            })
            .await
            .map_err(|e| format!("Звёздная реакция: {e}"))?;
        Ok(())
    }

    /// Сохраняет или отключает бизнес-часы работы аккаунта (`account.updateBusinessWorkHours`).
    pub async fn update_business_work_hours(
        &self,
        enabled: bool,
        timezone_id: String,
        weekly_intervals: Vec<(i32, i32)>,
    ) -> Result<(), String> {
        let business_work_hours = if enabled {
            let weekly_open = weekly_intervals
                .into_iter()
                .map(|(start_minute, end_minute)| {
                    tl::enums::BusinessWeeklyOpen::Open(tl::types::BusinessWeeklyOpen {
                        start_minute,
                        end_minute,
                    })
                })
                .collect();
            Some(tl::enums::BusinessWorkHours::Hours(
                tl::types::BusinessWorkHours {
                    open_now: true,
                    timezone_id: if timezone_id.trim().is_empty() {
                        "Europe/Moscow".to_owned()
                    } else {
                        timezone_id
                    },
                    weekly_open,
                },
            ))
        } else {
            None
        };
        let _ = self
            .client
            .invoke(&tl::functions::account::UpdateBusinessWorkHours {
                business_work_hours,
            })
            .await
            .map_err(|e| format!("Бизнес-часы: {e}"))?;
        Ok(())
    }

    /// Загружает список быстрых ответов Telegram Бизнес (`messages.getQuickReplies`).
    pub async fn quick_replies(&self) -> Result<Vec<QuickReplyItem>, String> {
        let res = self
            .client
            .invoke(&tl::functions::messages::GetQuickReplies { hash: 0 })
            .await
            .map_err(|e| format!("Быстрые ответы: {e}"))?;
        let tl::enums::messages::QuickReplies::Replies(r) = res else {
            return Ok(Vec::new());
        };
        Ok(r.quick_replies
            .into_iter()
            .map(|qr| {
                let tl::enums::QuickReply::Reply(q) = qr;
                let preview = r
                    .messages
                    .iter()
                    .find_map(|m| match m {
                        tl::enums::Message::Message(x) if x.id == q.top_message => {
                            Some(x.message.clone())
                        }
                        _ => None,
                    })
                    .unwrap_or_default();
                QuickReplyItem {
                    shortcut_id: q.shortcut_id,
                    shortcut: q.shortcut,
                    top_message: q.top_message,
                    count: q.count,
                    preview,
                }
            })
            .collect())
    }

    /// Отправляет быстрый ответ в текущий чат (`messages.sendQuickReplyMessages`).
    pub async fn send_quick_reply(
        &self,
        peer: PeerRef,
        shortcut_id: i32,
        msg_ids: Vec<i32>,
    ) -> Result<(), String> {
        let ids = if msg_ids.is_empty() { vec![0] } else { msg_ids };
        let random_id: Vec<i64> = ids.iter().map(|_| rand_i64()).collect();
        let _ = self
            .client
            .invoke(&tl::functions::messages::SendQuickReplyMessages {
                peer: tl::enums::InputPeer::from(peer),
                shortcut_id,
                id: ids,
                random_id,
            })
            .await
            .map_err(|e| format!("Отправка быстрого ответа: {e}"))?;
        Ok(())
    }

    /// Переименовывает ярлык быстрого ответа (`messages.editQuickReplyShortcut`).
    pub async fn edit_quick_reply_shortcut(
        &self,
        shortcut_id: i32,
        shortcut: &str,
    ) -> Result<(), String> {
        let clean = shortcut.trim().trim_start_matches('/').to_owned();
        if clean.is_empty() {
            return Err("Введите короткое имя команды".to_owned());
        }
        let _ = self
            .client
            .invoke(&tl::functions::messages::EditQuickReplyShortcut {
                shortcut_id,
                shortcut: clean,
            })
            .await
            .map_err(|e| format!("Быстрый ответ: {e}"))?;
        Ok(())
    }

    /// Удаляет ярлык быстрого ответа (`messages.deleteQuickReplyShortcut`).
    pub async fn delete_quick_reply_shortcut(&self, shortcut_id: i32) -> Result<(), String> {
        let _ = self
            .client
            .invoke(&tl::functions::messages::DeleteQuickReplyShortcut { shortcut_id })
            .await
            .map_err(|e| format!("Удаление быстрого ответа: {e}"))?;
        Ok(())
    }
}

/// Строка транзакции Telegram Stars.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarsTxRow {
    pub id: String,
    pub amount: i64,
    pub date: i64,
    pub title: String,
    pub description: String,
    pub is_gift: bool,
    pub is_reaction: bool,
}

/// Сводка баланса и истории Telegram Stars.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct StarsSummary {
    pub balance: i64,
    pub nanos: i32,
    pub transactions: Vec<StarsTxRow>,
}

/// Подарок из каталога Telegram Stars (`StarGift`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StarGiftItem {
    pub id: i64,
    pub title: String,
    pub stars: i64,
    pub convert_stars: i64,
    pub limited: bool,
    pub sold_out: bool,
    pub remains: Option<i32>,
    pub total: Option<i32>,
    pub unique: bool,
}

/// Подарок, сохранённый в профиле (`SavedStarGift`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedGiftItem {
    pub msg_id: Option<i32>,
    pub saved_id: Option<i64>,
    pub title: String,
    pub stars: i64,
    pub convert_stars: i64,
    pub date: i64,
    pub message: String,
    pub unsaved: bool,
    pub pinned_to_top: bool,
    pub unique: bool,
}

/// Элемент быстрого ответа Telegram Бизнес (`/shortcut`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QuickReplyItem {
    pub shortcut_id: i32,
    pub shortcut: String,
    pub top_message: i32,
    pub count: i32,
    pub preview: String,
}

fn stars_amount_parts(a: &tl::enums::StarsAmount) -> (i64, i32) {
    match a {
        tl::enums::StarsAmount::Amount(x) => (x.amount, x.nanos),
        other => (other.amount(), 0),
    }
}

fn stars_peer_label(p: &tl::enums::StarsTransactionPeer) -> String {
    use tl::enums::StarsTransactionPeer as P;
    match p {
        P::AppStore => "App Store".to_owned(),
        P::PlayMarket => "Google Play".to_owned(),
        P::PremiumBot => "Premium Bot".to_owned(),
        P::Fragment => "Fragment".to_owned(),
        P::Ads => "Telegram Ads".to_owned(),
        P::Api => "Bot API".to_owned(),
        P::Peer(_) => "Перевод Stars".to_owned(),
        P::Unsupported => "Telegram Stars".to_owned(),
    }
}

fn input_saved_star_gift(
    msg_id: Option<i32>,
    peer: Option<PeerRef>,
    saved_id: Option<i64>,
) -> Result<tl::enums::InputSavedStarGift, String> {
    if let (Some(p), Some(sid)) = (peer, saved_id) {
        return Ok(tl::enums::InputSavedStarGift::Chat(
            tl::types::InputSavedStarGiftChat {
                peer: tl::enums::InputPeer::from(p),
                saved_id: sid,
            },
        ));
    }
    if let Some(mid) = msg_id {
        return Ok(tl::enums::InputSavedStarGift::User(
            tl::types::InputSavedStarGiftUser { msg_id: mid },
        ));
    }
    Err("Идентификатор подарка не задан".to_owned())
}

// =========================================================================
// Новые функции v0.19.0: безлимитная расшифровка голосовых, отложенные/напоминания,
// эффекты сообщений, интерактивные чеклисты (Todo), опросы/викторины, теги Избранного,
// отправка от имени канала (Send-As), глобальный поиск хэштегов, цвет профиля, Slow Mode.
// =========================================================================

/// Тег-реакция в «Избранном» (`messages.getSavedReactionTags`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SavedTagItem {
    pub emoji: String,
    pub title: String,
    pub count: i32,
}

/// Профиль/канал, от имени которого можно писать в группе/канале (`channels.getSendAs`).
#[derive(Debug, Clone)]
pub struct SendAsItem {
    pub peer: PeerRef,
    pub id: i64,
    pub title: String,
    pub premium_required: bool,
}

/// Доступный анимированный эффект сообщения (`messages.getAvailableEffects`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MessageEffectItem {
    pub id: i64,
    pub emoticon: String,
    pub premium_required: bool,
}

impl Engine {
    /// Расшифровка голосового/видеосообщения в текст (`messages.transcribeAudio`).
    /// Если Telegram ограничивает аккаунт без Premium (`PREMIUM_ACCOUNT_REQUIRED` / лимит 2/нед),
    /// автоматически переключается на безлимитную локальную офлайн-расшифровку (без сторонних серверов!).
    pub async fn transcribe_voice(&self, peer: PeerRef, msg_id: i32) -> Result<String, String> {
        let tg_res = self
            .invoke_retry(
                "messages.transcribeAudio",
                &tl::functions::messages::TranscribeAudio {
                    peer: peer.into(),
                    msg_id,
                },
            )
            .await;
        match tg_res {
            Ok(tl::enums::messages::TranscribedAudio::Audio(ta)) => {
                if !ta.text.trim().is_empty() {
                    crate::diag::info("VOICE", &format!("Расшифровка Telegram OK (#{msg_id})"));
                    return Ok(ta.text);
                }
                if ta.pending {
                    // Короткое ожидание завершения серверной расшифровки Telegram.
                    tokio::time::sleep(std::time::Duration::from_millis(1600)).await;
                    if let Ok(tl::enums::messages::TranscribedAudio::Audio(ta2)) = self
                        .invoke_retry(
                            "messages.transcribeAudio(poll)",
                            &tl::functions::messages::TranscribeAudio {
                                peer: peer.into(),
                                msg_id,
                            },
                        )
                        .await
                        && !ta2.text.trim().is_empty()
                    {
                        return Ok(ta2.text);
                    }
                }
            }
            Err(e) => {
                crate::diag::warn("VOICE", &format!("Серверная расшифровка недоступна ({e}), переход на локальную"));
            }
        }
        // Безлимитная локальная офлайн-расшифровка (0 сторонних хостов):
        // скачиваем голосовое в кэш и прогоняем через локальный whisper-cli (если лежит в `whisper/`) или системный движок ОС.
        let cancel = std::sync::atomic::AtomicBool::new(false);
        let downloaded = self
            .fetch_media(peer, msg_id, false, &cancel, |_, _| {})
            .await
            .map_err(|e| format!("Скачивание аудио для расшифровки: {e}"))?;
        let super::Fetched::Ready(audio_path) = downloaded else {
            return Err("Не удалось скачать голосовое для локальной расшифровки".to_owned());
        };
        let home_whisper = self.dirs.cache.parent().unwrap_or(&self.dirs.cache).join("whisper");
        tokio::task::spawn_blocking(move || local_transcribe_audio(&audio_path, &home_whisper))
            .await
            .map_err(|e| format!("Поток расшифровки: {e}"))?
    }

    /// Отправка сообщения с отложенным временем / напоминанием (`schedule_date`), анимированным
    /// эффектом (`effect`) и выбором отправителя (`send_as` — писать от имени канала).
    #[allow(clippy::too_many_arguments)]
    pub async fn send_message_advanced(
        &self,
        peer: PeerRef,
        text: &str,
        entities: &[super::Entity],
        reply_to: Option<i32>,
        schedule_date: Option<i32>,
        effect: Option<i64>,
        send_as: Option<PeerRef>,
        force_silent: bool,
    ) -> Result<super::MessageRow, String> {
        let cfg = self.settings();
        let tl_ents = super::entities_to_tl(text, entities);
        let reply_header = reply_to.map(|mid| {
            tl::enums::InputReplyTo::Message(tl::types::InputReplyToMessage {
                reply_to_msg_id: mid,
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
        let silent = force_silent || cfg.ghost.should_send_without_sound();
        self.apply_traffic_jitter().await;
        let updates = self
            .invoke_retry(
                "messages.sendMessage(adv)",
                &tl::functions::messages::SendMessage {
                    no_webpage: false,
                    silent,
                    background: false,
                    clear_draft: true,
                    noforwards: false,
                    update_stickersets_order: false,
                    invert_media: false,
                    allow_paid_floodskip: false,
                    peer: peer.into(),
                    reply_to: reply_header,
                    message: text.to_owned(),
                    random_id: rand_i64(),
                    reply_markup: None,
                    entities: (!tl_ents.is_empty()).then_some(tl_ents),
                    schedule_date,
                    schedule_repeat_period: None,
                    send_as: send_as.map(Into::into),
                    quick_reply_shortcut: None,
                    effect,
                    allow_paid_stars: None,
                    suggested_post: None,
                    rich_message: None,
                },
            )
            .await
            .map_err(|e| format!("Отправка: {e}"))?;
        let mut row = self.row_from_tl_updates(peer, &updates, text, silent);
        if row.effect.is_none() {
            row.effect = effect;
        }
        self.remember_row_sync(peer, &row).await;
        self.spawn_after_send(peer, cfg.ghost, &row, true);
        Ok(row)
    }

    /// Загружает очередь отложенных сообщений и напоминаний чата (`messages.getScheduledHistory`).
    pub async fn scheduled_history(&self, peer: PeerRef) -> Result<Vec<super::MessageRow>, String> {
        let res = self
            .invoke_retry(
                "messages.getScheduledHistory",
                &tl::functions::messages::GetScheduledHistory {
                    peer: peer.into(),
                    hash: 0,
                },
            )
            .await
            .map_err(|e| format!("Отложенные: {e}"))?;
        let mut rows = super::tl_messages_to_rows(&res);
        rows.sort_by_key(|r| (r.date, r.id));
        Ok(rows)
    }

    /// Немедленно отправить отложенные сообщения (`messages.sendScheduledMessages`).
    pub async fn send_scheduled_now(&self, peer: PeerRef, ids: Vec<i32>) -> Result<(), String> {
        self.invoke_retry(
            "messages.sendScheduledMessages",
            &tl::functions::messages::SendScheduledMessages {
                peer: peer.into(),
                id: ids,
            },
        )
        .await
        .map_err(|e| format!("Отправка отложенного: {e}"))?;
        Ok(())
    }

    /// Удалить отложенные сообщения (`messages.deleteScheduledMessages`).
    pub async fn delete_scheduled(&self, peer: PeerRef, ids: Vec<i32>) -> Result<(), String> {
        self.invoke_retry(
            "messages.deleteScheduledMessages",
            &tl::functions::messages::DeleteScheduledMessages {
                peer: peer.into(),
                id: ids,
            },
        )
        .await
        .map_err(|e| format!("Удаление отложенного: {e}"))?;
        Ok(())
    }

    /// Создать интерактивный чеклист задач в чате (`InputMedia::Todo`).
    pub async fn send_todo_list(
        &self,
        peer: PeerRef,
        title: &str,
        items: &[String],
        others_can_complete: bool,
        others_can_append: bool,
    ) -> Result<(), String> {
        let list: Vec<tl::enums::TodoItem> = items
            .iter()
            .filter(|s| !s.trim().is_empty())
            .enumerate()
            .map(|(idx, s)| {
                tl::enums::TodoItem::Item(tl::types::TodoItem {
                    id: (idx + 1) as i32,
                    title: tl::enums::TextWithEntities::Entities(tl::types::TextWithEntities {
                        text: s.trim().to_owned(),
                        entities: Vec::new(),
                    }),
                })
            })
            .collect();
        if list.is_empty() {
            return Err("Добавьте хотя бы один пункт задачи".to_owned());
        }
        let media = tl::enums::InputMedia::Todo(tl::types::InputMediaTodo {
            todo: tl::enums::TodoList::List(tl::types::TodoList {
                others_can_append,
                others_can_complete,
                title: tl::enums::TextWithEntities::Entities(tl::types::TextWithEntities {
                    text: title.trim().to_owned(),
                    entities: Vec::new(),
                }),
                list,
            }),
        });
        self.invoke_retry(
            "messages.sendMedia(todo)",
            &tl::functions::messages::SendMedia {
                silent: false,
                background: false,
                clear_draft: false,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: false,
                allow_paid_floodskip: false,
                peer: peer.into(),
                reply_to: None,
                media,
                message: String::new(),
                random_id: rand_i64(),
                reply_markup: None,
                entities: None,
                schedule_date: None,
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            },
        )
        .await
        .map_err(|e| format!("Создание чеклиста: {e}"))?;
        Ok(())
    }

    /// Отметить или снять галочку пункта чеклиста (`messages.toggleTodoCompleted`).
    pub async fn toggle_todo_item(
        &self,
        peer: PeerRef,
        msg_id: i32,
        item_id: i32,
        completed: bool,
    ) -> Result<(), String> {
        let (comp, incomp) = if completed { (vec![item_id], Vec::new()) } else { (Vec::new(), vec![item_id]) };
        self.invoke_retry(
            "messages.toggleTodoCompleted",
            &tl::functions::messages::ToggleTodoCompleted {
                peer: peer.into(),
                msg_id,
                completed: comp,
                incompleted: incomp,
            },
        )
        .await
        .map_err(|e| format!("Чеклист: {e}"))?;
        Ok(())
    }

    /// Создать опрос или викторину прямо в чате (`InputMedia::Poll`).
    #[allow(clippy::too_many_arguments)]
    pub async fn send_poll(
        &self,
        peer: PeerRef,
        question: &str,
        options: &[String],
        anonymous: bool,
        multiple_choice: bool,
        quiz: bool,
        correct_idx: Option<usize>,
        explanation: &str,
    ) -> Result<(), String> {
        let answers: Vec<tl::enums::PollAnswer> = options
            .iter()
            .filter(|s| !s.trim().is_empty())
            .take(10)
            .enumerate()
            .map(|(idx, s)| {
                tl::enums::PollAnswer::Answer(tl::types::PollAnswer {
                    text: tl::enums::TextWithEntities::Entities(tl::types::TextWithEntities {
                        text: s.trim().to_owned(),
                        entities: Vec::new(),
                    }),
                    option: vec![idx as u8],
                    media: None,
                    added_by: None,
                    date: None,
                })
            })
            .collect();
        if answers.len() < 2 {
            return Err("Укажите минимум 2 варианта ответа".to_owned());
        }
        let poll = tl::enums::Poll::Poll(tl::types::Poll {
            id: rand_i64(),
            closed: false,
            public_voters: !anonymous,
            multiple_choice: multiple_choice && !quiz,
            quiz,
            open_answers: false,
            revoting_disabled: false,
            shuffle_answers: false,
            hide_results_until_close: false,
            creator: true,
            subscribers_only: false,
            question: tl::enums::TextWithEntities::Entities(tl::types::TextWithEntities {
                text: question.trim().to_owned(),
                entities: Vec::new(),
            }),
            answers,
            close_period: None,
            close_date: None,
            countries_iso2: None,
            hash: 0,
        });
        let correct_answers = if quiz {
            Some(vec![correct_idx.unwrap_or(0) as i32])
        } else {
            None
        };
        let sol = explanation.trim();
        let media = tl::enums::InputMedia::Poll(Box::new(tl::types::InputMediaPoll {
            poll,
            correct_answers,
            attached_media: None,
            solution: (quiz && !sol.is_empty()).then(|| sol.to_owned()),
            solution_entities: None,
            solution_media: None,
        }));
        self.invoke_retry(
            "messages.sendMedia(poll)",
            &tl::functions::messages::SendMedia {
                silent: false,
                background: false,
                clear_draft: false,
                noforwards: false,
                update_stickersets_order: false,
                invert_media: false,
                allow_paid_floodskip: false,
                peer: peer.into(),
                reply_to: None,
                media,
                message: String::new(),
                random_id: rand_i64(),
                reply_markup: None,
                entities: None,
                schedule_date: None,
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                allow_paid_stars: None,
                suggested_post: None,
            },
        )
        .await
        .map_err(|e| format!("Создание опроса: {e}"))?;
        Ok(())
    }

    /// Загружает теги-реакции в «Избранном» (`messages.getSavedReactionTags`).
    pub async fn saved_reaction_tags(&self) -> Result<Vec<SavedTagItem>, String> {
        let res = self
            .invoke_retry(
                "messages.getSavedReactionTags",
                &tl::functions::messages::GetSavedReactionTags {
                    peer: None,
                    hash: 0,
                },
            )
            .await
            .map_err(|e| format!("Теги Избранного: {e}"))?;
        let tl::enums::messages::SavedReactionTags::Tags(t) = res else {
            return Ok(Vec::new());
        };
        Ok(t.tags
            .into_iter()
            .filter_map(|tag| {
                let tl::enums::SavedReactionTag::Tag(x) = tag;
                let emoji = match x.reaction {
                    tl::enums::Reaction::Emoji(e) => e.emoticon,
                    tl::enums::Reaction::CustomEmoji(_) => "✨".to_owned(),
                    _ => return None,
                };
                Some(SavedTagItem {
                    emoji,
                    title: x.title.unwrap_or_default(),
                    count: x.count,
                })
            })
            .collect())
    }

    /// Переименовывает тег в «Избранном» (`messages.updateSavedReactionTag`).
    pub async fn update_saved_reaction_tag(&self, emoji: &str, title: &str) -> Result<(), String> {
        let t = title.trim();
        self.invoke_retry(
            "messages.updateSavedReactionTag",
            &tl::functions::messages::UpdateSavedReactionTag {
                reaction: tl::enums::Reaction::Emoji(tl::types::ReactionEmoji {
                    emoticon: emoji.to_owned(),
                }),
                title: (!t.is_empty()).then(|| t.to_owned()),
            },
        )
        .await
        .map_err(|e| format!("Обновление тега: {e}"))?;
        Ok(())
    }

    /// Загружает список профилей/каналов «Писать от имени…» (`channels.getSendAs`).
    pub async fn send_as_peers(&self, peer: PeerRef) -> Result<Vec<SendAsItem>, String> {
        let tl::enums::channels::SendAsPeers::Peers(res) = self
            .invoke_retry(
                "channels.getSendAs",
                &tl::functions::channels::GetSendAs {
                    for_paid_reactions: false,
                    for_live_stories: false,
                    peer: peer.into(),
                },
            )
            .await
            .map_err(|e| format!("Писать от имени: {e}"))?;
        let mut out = Vec::new();
        for sp in res.peers {
            let tl::enums::SendAsPeer::Peer(x) = sp;
            match &x.peer {
                tl::enums::Peer::User(u) => {
                    if let Some(usr) = res.users.iter().find(|y| matches!(y, tl::enums::User::User(z) if z.id == u.user_id)) {
                        let tl::enums::User::User(z) = usr else { continue };
                        let title = format!("{} {}", z.first_name.as_deref().unwrap_or(""), z.last_name.as_deref().unwrap_or(""))
                            .trim()
                            .to_owned();
                        out.push(SendAsItem {
                            peer: PeerRef::from(usr),
                            id: z.id,
                            title: if title.is_empty() { "Личный профиль".to_owned() } else { title },
                            premium_required: x.premium_required,
                        });
                    }
                }
                tl::enums::Peer::Channel(c) => {
                    if let Some(ch) = res.chats.iter().find(|y| matches!(y, tl::enums::Chat::Channel(z) if z.id == c.channel_id)) {
                        let tl::enums::Chat::Channel(z) = ch else { continue };
                        out.push(SendAsItem {
                            peer: PeerRef::from(ch),
                            id: -1_000_000_000_000 - z.id,
                            title: z.title.clone(),
                            premium_required: x.premium_required,
                        });
                    }
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// Глобальный поиск постов по хэштегу по всем публичным каналам (`channels.searchPosts`).
    pub async fn search_hashtag_posts(&self, hashtag: &str, limit: i32) -> Result<Vec<super::MessageRow>, String> {
        let tag = hashtag.trim().trim_start_matches('#');
        if tag.is_empty() {
            return Ok(Vec::new());
        }
        let res = self
            .invoke_retry(
                "channels.searchPosts",
                &tl::functions::channels::SearchPosts {
                    hashtag: Some(tag.to_owned()),
                    query: None,
                    offset_rate: 0,
                    offset_peer: tl::enums::InputPeer::Empty,
                    offset_id: 0,
                    limit: limit.clamp(5, 40),
                    allow_paid_stars: None,
                },
            )
            .await
            .map_err(|e| format!("Поиск по #{tag}: {e}"))?;
        Ok(super::tl_messages_to_rows(&res))
    }

    /// Обновляет персональный цвет акцента профиля/ответов (`account.updateColor`).
    pub async fn update_profile_color(&self, for_profile: bool, color_id: Option<i32>) -> Result<(), String> {
        self.invoke_retry(
            "account.updateColor",
            &tl::functions::account::UpdateColor {
                for_profile,
                color: color_id.map(|c| {
                    tl::enums::PeerColor::Color(tl::types::PeerColor {
                        color: Some(c),
                        background_emoji_id: None,
                    })
                }),
            },
        )
        .await
        .map_err(|e| format!("Цвет профиля: {e}"))?;
        Ok(())
    }

    /// Загружает список анимированных эффектов сообщений (`messages.getAvailableEffects`).
    pub async fn available_effects(&self) -> Result<Vec<MessageEffectItem>, String> {
        let res = self
            .invoke_retry(
                "messages.getAvailableEffects",
                &tl::functions::messages::GetAvailableEffects { hash: 0 },
            )
            .await
            .map_err(|e| format!("Эффекты: {e}"))?;
        let tl::enums::messages::AvailableEffects::Effects(e) = res else {
            return Ok(Vec::new());
        };
        Ok(e.effects
            .into_iter()
            .map(|ef| {
                let tl::enums::AvailableEffect::Effect(x) = ef;
                MessageEffectItem {
                    id: x.id,
                    emoticon: x.emoticon,
                    premium_required: x.premium_required,
                }
            })
            .collect())
    }

    /// Включает или отключает медленный режим (Slow Mode) в супергруппе (`channels.toggleSlowMode`).
    pub async fn set_slow_mode(&self, peer: PeerRef, seconds: i32) -> Result<(), String> {
        let channel = peer_to_input_channel(peer)?;
        self.invoke_retry(
            "channels.toggleSlowMode",
            &tl::functions::channels::ToggleSlowMode {
                channel,
                seconds: seconds.max(0),
            },
        )
        .await
        .map_err(|e| format!("Медленный режим: {e}"))?;
        Ok(())
    }

    // =========================================================================
    // Инлайн-боты, быстрые реакции, эмодзи-статус и активность сообщений
    // =========================================================================

    /// Запрос результатов инлайн-бота (`@bot запрос`) с кэшированием `InputUser` бота.
    pub async fn inline_bot_results(
        &self,
        peer: PeerRef,
        bot_username: &str,
        query: &str,
        offset: &str,
    ) -> Result<InlineBotResultsPage, String> {
        let clean_user = bot_username.trim().trim_start_matches('@').to_ascii_lowercase();
        if clean_user.len() < 3 {
            return Err("Некорректное имя бота".to_owned());
        }
        let cached = self
            .inline_bots_cache
            .lock()
            .ok()
            .and_then(|g| g.get(&clean_user).cloned());
        let bot_input = if let Some(u) = cached {
            u
        } else {
            let res = self
                .client
                .invoke(&tl::functions::contacts::ResolveUsername {
                    username: clean_user.clone(),
                    referer: None,
                })
                .await
                .map_err(|e| format!("Поиск @{clean_user}: {e}"))?;
            let tl::enums::contacts::ResolvedPeer::Peer(rp) = res;
            let resolved_user = rp
                .users
                .iter()
                .find_map(|u| match u {
                    tl::enums::User::User(x) => {
                        let uname = x.username.as_deref().unwrap_or_default();
                        if x.bot && uname.eq_ignore_ascii_case(&clean_user) {
                            x.access_hash.map(|ah| {
                                tl::enums::InputUser::User(tl::types::InputUser {
                                    user_id: x.id,
                                    access_hash: ah,
                                })
                            })
                        } else {
                            None
                        }
                    }
                    _ => None,
                })
                .ok_or_else(|| format!("Инлайн-бот @{clean_user} не найден"))?;
            if let Ok(mut g) = self.inline_bots_cache.lock() {
                if g.len() > 64 {
                    g.clear();
                }
                g.insert(clean_user.clone(), resolved_user.clone());
            }
            resolved_user
        };
        let tl::enums::messages::BotResults::Results(res) = self
            .invoke_retry(
                "messages.getInlineBotResults",
                &tl::functions::messages::GetInlineBotResults {
                    bot: bot_input,
                    peer: peer.into(),
                    geo_point: None,
                    query: query.to_owned(),
                    offset: offset.to_owned(),
                },
            )
            .await
            .map_err(|e| format!("Инлайн-бот @{clean_user}: {e}"))?;
        let mut results = Vec::with_capacity(res.results.len());
        for item in res.results {
            match item {
                tl::enums::BotInlineResult::Result(r) => {
                    let thumb_url = r.thumb.as_ref().map(|w| match w {
                        tl::enums::WebDocument::Document(d) => d.url.clone(),
                        tl::enums::WebDocument::NoProxy(d) => d.url.clone(),
                    });
                    results.push(InlineBotResultRow {
                        id: r.id,
                        kind: r.r#type,
                        title: r.title.unwrap_or_default(),
                        description: r.description.unwrap_or_default(),
                        thumb_url,
                        send_text: extract_inline_send_text(&r.send_message),
                    });
                }
                tl::enums::BotInlineResult::BotInlineMediaResult(r) => {
                    results.push(InlineBotResultRow {
                        id: r.id,
                        kind: r.r#type,
                        title: r.title.unwrap_or_default(),
                        description: r.description.unwrap_or_default(),
                        thumb_url: None,
                        send_text: extract_inline_send_text(&r.send_message),
                    });
                }
            }
        }
        Ok(InlineBotResultsPage {
            query_id: res.query_id,
            gallery: res.gallery,
            next_offset: res.next_offset.unwrap_or_default(),
            results,
            bot_username: clean_user,
        })
    }

    /// Отправить выбранный результат инлайн-бота (`messages.sendInlineBotResult`).
    pub async fn send_inline_bot_result(
        &self,
        peer: PeerRef,
        query_id: i64,
        id: &str,
        reply_to: Option<i32>,
    ) -> Result<(), grammers_client::InvocationError> {
        self.client
            .invoke(&tl::functions::messages::SendInlineBotResult {
                silent: false,
                background: false,
                clear_draft: false,
                hide_via: false,
                peer: peer.into(),
                reply_to: reply_to.map(|mid| {
                    tl::enums::InputReplyTo::Message(tl::types::InputReplyToMessage {
                        reply_to_msg_id: mid,
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
                }),
                random_id: rand_i64(),
                query_id,
                id: id.to_owned(),
                schedule_date: None,
                send_as: None,
                quick_reply_shortcut: None,
                allow_paid_stars: None,
            })
            .await?;
        Ok(())
    }

    /// Список доступных реакций Telegram (`messages.getAvailableReactions`).
    pub async fn available_reactions(
        &self,
    ) -> Result<Vec<AvailableReactionRow>, grammers_client::InvocationError> {
        let res = self
            .client
            .invoke(&tl::functions::messages::GetAvailableReactions { hash: 0 })
            .await?;
        let tl::enums::messages::AvailableReactions::Reactions(r) = res else {
            return Ok(Vec::new());
        };
        Ok(r.reactions
            .into_iter()
            .filter_map(|ar| {
                let tl::enums::AvailableReaction::Reaction(x) = ar;
                if x.inactive || x.reaction.is_empty() {
                    None
                } else {
                    Some(AvailableReactionRow {
                        emoji: x.reaction,
                        title: x.title,
                        premium: x.premium,
                    })
                }
            })
            .collect())
    }

    /// Задать быструю реакцию по умолчанию на сервере Telegram (`messages.setDefaultReaction`).
    pub async fn set_default_reaction(
        &self,
        emoji: &str,
    ) -> Result<(), grammers_client::InvocationError> {
        self.client
            .invoke(&tl::functions::messages::SetDefaultReaction {
                reaction: tl::enums::Reaction::Emoji(tl::types::ReactionEmoji {
                    emoticon: emoji.to_owned(),
                }),
            })
            .await?;
        Ok(())
    }

    /// Список стандартных эмодзи-статусов аккаунта (`account.getDefaultEmojiStatuses`).
    pub async fn default_emoji_statuses(&self) -> Result<Vec<i64>, grammers_client::InvocationError> {
        let res = self
            .client
            .invoke(&tl::functions::account::GetDefaultEmojiStatuses { hash: 0 })
            .await?;
        let tl::enums::account::EmojiStatuses::Statuses(s) = res else {
            return Ok(Vec::new());
        };
        Ok(s.statuses
            .into_iter()
            .filter_map(|st| match st {
                tl::enums::EmojiStatus::Status(x) => Some(x.document_id),
                _ => None,
            })
            .collect())
    }

    /// Обновить эмодзи-статус текущего аккаунта (`account.updateEmojiStatus`).
    pub async fn update_emoji_status(
        &self,
        document_id: Option<i64>,
        until: Option<i32>,
    ) -> Result<(), grammers_client::InvocationError> {
        let emoji_status = match document_id {
            Some(id) => tl::enums::EmojiStatus::Status(tl::types::EmojiStatus {
                document_id: id,
                until,
            }),
            None => tl::enums::EmojiStatus::Empty,
        };
        self.client
            .invoke(&tl::functions::account::UpdateEmojiStatus { emoji_status })
            .await?;
        Ok(())
    }

    /// Кто прочитал сообщение в группе/канале (`messages.getMessageReadParticipants`).
    pub async fn message_read_participants(
        &self,
        peer: PeerRef,
        msg_id: i32,
    ) -> Result<Vec<ReadParticipantRow>, String> {
        let list = self
            .client
            .invoke(&tl::functions::messages::GetMessageReadParticipants {
                peer: peer.into(),
                msg_id,
            })
            .await
            .map_err(|e| e.to_string())?;
        let mut raw: Vec<(i64, i32)> = Vec::with_capacity(list.len());
        let mut inputs: Vec<tl::enums::InputUser> = Vec::with_capacity(list.len().min(50));
        for item in list {
            let tl::enums::ReadParticipantDate::Date(d) = item;
            raw.push((d.user_id, d.date));
            if inputs.len() < 50 {
                inputs.push(tl::enums::InputUser::User(tl::types::InputUser {
                    user_id: d.user_id,
                    access_hash: 0,
                }));
            }
        }
        let mut names = std::collections::HashMap::new();
        if !inputs.is_empty()
            && let Ok(users) = self.client.invoke(&tl::functions::users::GetUsers { id: inputs }).await
        {
            for u in users {
                if let tl::enums::User::User(x) = u {
                    let full = [x.first_name.as_deref(), x.last_name.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(" ");
                    let label = if !full.trim().is_empty() {
                        full
                    } else if let Some(un) = x.username.as_deref().filter(|s| !s.is_empty()) {
                        format!("@{un}")
                    } else {
                        format!("ID {}", x.id)
                    };
                    names.insert(x.id, label);
                }
            }
        }
        Ok(raw
            .into_iter()
            .map(|(user_id, date)| ReadParticipantRow {
                name: names.remove(&user_id).unwrap_or_else(|| format!("ID {user_id}")),
                user_id,
                date,
            })
            .collect())
    }

    /// Список поставивших реакции на сообщение (`messages.getMessageReactionsList`).
    pub async fn message_reactions_list(
        &self,
        peer: PeerRef,
        msg_id: i32,
    ) -> Result<Vec<MessageReactionPeerRow>, String> {
        let tl::enums::messages::MessageReactionsList::List(l) = self
            .client
            .invoke(&tl::functions::messages::GetMessageReactionsList {
                peer: peer.into(),
                id: msg_id,
                reaction: None,
                offset: None,
                limit: 50,
            })
            .await
            .map_err(|e| e.to_string())?;
        let mut peer_names = std::collections::HashMap::<i64, String>::new();
        for u in &l.users {
            if let tl::enums::User::User(x) = u {
                let full = [x.first_name.as_deref(), x.last_name.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" ");
                let label = if !full.trim().is_empty() {
                    full
                } else if let Some(un) = x.username.as_deref().filter(|s| !s.is_empty()) {
                    format!("@{un}")
                } else {
                    format!("ID {}", x.id)
                };
                peer_names.insert(x.id, label);
            }
        }
        for c in &l.chats {
            match c {
                tl::enums::Chat::Chat(x) => {
                    peer_names.insert(-x.id, x.title.clone());
                }
                tl::enums::Chat::Channel(x) => {
                    peer_names.insert(-1_000_000_000_000 - x.id, x.title.clone());
                }
                _ => {}
            }
        }
        Ok(l.reactions
            .into_iter()
            .map(|mr| {
                let tl::enums::MessagePeerReaction::Reaction(r) = mr;
                let peer_id = match r.peer_id {
                    tl::enums::Peer::User(u) => u.user_id,
                    tl::enums::Peer::Chat(c) => -c.chat_id,
                    tl::enums::Peer::Channel(c) => -1_000_000_000_000 - c.channel_id,
                };
                let reaction = match r.reaction {
                    tl::enums::Reaction::Emoji(e) => e.emoticon,
                    tl::enums::Reaction::CustomEmoji(c) => format!("✨#{}", c.document_id),
                    tl::enums::Reaction::Paid => "⭐".to_owned(),
                    tl::enums::Reaction::Empty => String::new(),
                };
                MessageReactionPeerRow {
                    name: peer_names
                        .get(&peer_id)
                        .cloned()
                        .unwrap_or_else(|| format!("ID {peer_id}")),
                    peer_id,
                    reaction,
                    date: r.date,
                }
            })
            .collect())
    }

    // =========================================================================
    // Новые 5 функций v0.21.0:
    // 1) Облачные черновики (`messages.saveDraft` / `messages.getAllDrafts`)
    // 2) Отметить непрочитанным + непрочитанные `@` и `❤` (`messages.markDialogUnread`, `getUnreadMentions`, `getUnreadReactions`)
    // 3) Игры / кости в чате (`InputMediaDice`: 🎲 🎯 🏀 ⚽ 🎰 🎳)
    // 4) Просмотр, установка и удаление набора стикеров (`messages.getStickerSet`, `installStickerSet`, `uninstallStickerSet`)
    // 5) Редактор дня рождения (`account.updateBirthday`) и `@username`
    // =========================================================================

    /// 1. Сохраняет облачный черновик чата на сервере Telegram (`messages.saveDraft`).
    pub async fn save_draft(
        &self,
        peer: PeerRef,
        text: &str,
        reply_to: Option<i32>,
    ) -> Result<(), String> {
        let reply_hdr = reply_to.map(|id| {
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
        self.client
            .invoke(&tl::functions::messages::SaveDraft {
                no_webpage: false,
                invert_media: false,
                reply_to: reply_hdr,
                peer: peer.into(),
                message: text.to_owned(),
                entities: None,
                media: None,
                effect: None,
                suggested_post: None,
                rich_message: None,
            })
            .await
            .map_err(|e| format!("Черновик: {e}"))?;
        Ok(())
    }

    /// Загружает все облачные черновики аккаунта (`messages.getAllDrafts`).
    pub async fn get_all_drafts(&self) -> Result<Vec<DraftRow>, String> {
        let res = self
            .client
            .invoke(&tl::functions::messages::GetAllDrafts {})
            .await
            .map_err(|e| format!("Черновики: {e}"))?;
        let upds = match res {
            tl::enums::Updates::Updates(u) => u.updates,
            tl::enums::Updates::Combined(u) => u.updates,
            _ => Vec::new(),
        };
        let mut out = Vec::new();
        for u in upds {
            if let tl::enums::Update::DraftMessage(dm) = u {
                let peer_id = match dm.peer {
                    tl::enums::Peer::User(x) => x.user_id,
                    tl::enums::Peer::Chat(x) => -x.chat_id,
                    tl::enums::Peer::Channel(x) => -1_000_000_000_000 - x.channel_id,
                };
                if let tl::enums::DraftMessage::Message(m) = dm.draft {
                    let reply_to = m.reply_to.and_then(|rt| match rt {
                        tl::enums::InputReplyTo::Message(x) => Some(x.reply_to_msg_id),
                        _ => None,
                    });
                    if !m.message.is_empty() || reply_to.is_some() {
                        out.push(DraftRow {
                            peer_id,
                            text: m.message,
                            reply_to,
                            date: m.date,
                        });
                    }
                }
            }
        }
        Ok(out)
    }

    /// 2. Отмечает чат непрочитанным или прочитанным (`messages.markDialogUnread`).
    pub async fn mark_dialog_unread(&self, peer: PeerRef, unread: bool) -> Result<(), String> {
        self.client
            .invoke(&tl::functions::messages::MarkDialogUnread {
                unread,
                parent_peer: None,
                peer: tl::enums::InputDialogPeer::Peer(tl::types::InputDialogPeer { peer: peer.into() }),
            })
            .await
            .map_err(|e| format!("Отметка непрочитанного: {e}"))?;
        Ok(())
    }

    /// Загружает идентификаторы непрочитанных упоминаний `@` в чате (`messages.getUnreadMentions`).
    pub async fn unread_mentions(&self, peer: PeerRef, limit: i32) -> Result<Vec<i32>, String> {
        let res = self
            .client
            .invoke(&tl::functions::messages::GetUnreadMentions {
                peer: peer.into(),
                top_msg_id: None,
                offset_id: 0,
                add_offset: 0,
                limit: limit.clamp(1, 50),
                max_id: 0,
                min_id: 0,
            })
            .await
            .map_err(|e| format!("Упоминания: {e}"))?;
        Ok(super::tl_messages_to_rows(&res).into_iter().map(|m| m.id).collect())
    }

    /// Отмечает все упоминания `@` прочитанными (`messages.readMentions`).
    pub async fn read_mentions(&self, peer: PeerRef) -> Result<(), String> {
        self.client
            .invoke(&tl::functions::messages::ReadMentions {
                peer: peer.into(),
                top_msg_id: None,
            })
            .await
            .map_err(|e| format!("Чтение упоминаний: {e}"))?;
        Ok(())
    }

    /// Загружает идентификаторы непрочитанных реакций `❤` в чате (`messages.getUnreadReactions`).
    pub async fn unread_reactions(&self, peer: PeerRef, limit: i32) -> Result<Vec<i32>, String> {
        let res = self
            .client
            .invoke(&tl::functions::messages::GetUnreadReactions {
                peer: peer.into(),
                top_msg_id: None,
                saved_peer_id: None,
                offset_id: 0,
                add_offset: 0,
                limit: limit.clamp(1, 50),
                max_id: 0,
                min_id: 0,
            })
            .await
            .map_err(|e| format!("Непрочитанные реакции: {e}"))?;
        Ok(super::tl_messages_to_rows(&res).into_iter().map(|m| m.id).collect())
    }

    /// Отмечает все реакции `❤` прочитанными (`messages.readReactions`).
    pub async fn read_reactions(&self, peer: PeerRef) -> Result<(), String> {
        self.client
            .invoke(&tl::functions::messages::ReadReactions {
                peer: peer.into(),
                top_msg_id: None,
                saved_peer_id: None,
            })
            .await
            .map_err(|e| format!("Чтение реакций: {e}"))?;
        Ok(())
    }

    /// 3. Отправить анимированную мини-игру / бросок (`InputMediaDice`: 🎲 🎯 🏀 ⚽ 🎰 🎳) с мгновенным `MessageRow`.
    pub async fn send_dice(
        &self,
        peer: PeerRef,
        emoticon: &str,
        reply_to: Option<i32>,
    ) -> Result<super::MessageRow, String> {
        let cfg = self.settings();
        let silent = cfg.ghost.should_send_without_sound();
        let reply_header = reply_to.map(|mid| {
            tl::enums::InputReplyTo::Message(tl::types::InputReplyToMessage {
                reply_to_msg_id: mid,
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
        let updates = self
            .invoke_retry(
                "messages.sendMedia(dice)",
                &tl::functions::messages::SendMedia {
                    silent,
                    background: false,
                    clear_draft: false,
                    noforwards: false,
                    update_stickersets_order: false,
                    invert_media: false,
                    allow_paid_floodskip: false,
                    peer: peer.into(),
                    reply_to: reply_header,
                    media: tl::enums::InputMedia::Dice(tl::types::InputMediaDice {
                        emoticon: emoticon.to_owned(),
                    }),
                    message: String::new(),
                    random_id: rand_i64(),
                    reply_markup: None,
                    entities: None,
                    schedule_date: None,
                    schedule_repeat_period: None,
                    send_as: None,
                    quick_reply_shortcut: None,
                    effect: None,
                    allow_paid_stars: None,
                    suggested_post: None,
                },
            )
            .await
            .map_err(|e| format!("Бросок: {e}"))?;
        let row = self.row_from_tl_updates(peer, &updates, "", silent);
        self.remember_row_sync(peer, &row).await;
        self.spawn_after_send(peer, cfg.ghost, &row, true);
        Ok(row)
    }

    /// 4. Подробная информация о наборе стикеров по `id` + `access_hash` (`messages.getStickerSet`).
    pub async fn sticker_pack_details(
        &self,
        set_id: i64,
        access_hash: i64,
    ) -> Result<StickerPackDetails, String> {
        let res = self
            .invoke_retry(
                "messages.getStickerSet",
                &tl::functions::messages::GetStickerSet {
                    stickerset: tl::enums::InputStickerSet::Id(tl::types::InputStickerSetId {
                        id: set_id,
                        access_hash,
                    }),
                    hash: 0,
                },
            )
            .await
            .map_err(|e| format!("Набор стикеров: {e}"))?;
        let tl::enums::messages::StickerSet::Set(s) = res else {
            return Err("Набор стикеров не найден".to_owned());
        };
        let tl::enums::StickerSet::Set(meta) = s.set;
        let stickers = s
            .documents
            .iter()
            .filter_map(|d| {
                let tl::enums::Document::Document(doc) = d else { return None };
                Some(super::sticker_row_of(doc))
            })
            .collect();
        Ok(StickerPackDetails {
            id: meta.id,
            access_hash: meta.access_hash,
            title: meta.title,
            short_name: meta.short_name,
            installed: meta.installed_date.is_some(),
            emojis: meta.emojis,
            stickers,
        })
    }

    /// Установить набор стикеров по `id` и `access_hash`.
    pub async fn install_sticker_pack(&self, set_id: i64, access_hash: i64) -> Result<(), String> {
        self.client
            .invoke(&tl::functions::messages::InstallStickerSet {
                stickerset: tl::enums::InputStickerSet::Id(tl::types::InputStickerSetId {
                    id: set_id,
                    access_hash,
                }),
                archived: false,
            })
            .await
            .map_err(|e| format!("Установка набора: {e}"))?;
        Ok(())
    }

    /// Удалить набор стикеров по `id` и `access_hash` (`messages.uninstallStickerSet`).
    pub async fn uninstall_sticker_pack(&self, set_id: i64, access_hash: i64) -> Result<(), String> {
        self.client
            .invoke(&tl::functions::messages::UninstallStickerSet {
                stickerset: tl::enums::InputStickerSet::Id(tl::types::InputStickerSetId {
                    id: set_id,
                    access_hash,
                }),
            })
            .await
            .map_err(|e| format!("Удаление набора: {e}"))?;
        Ok(())
    }

    /// 5. Обновить дату рождения в профиле (`account.updateBirthday`). `None` сбрасывает дату.
    pub async fn update_birthday(&self, bday: Option<(i32, i32, Option<i32>)>) -> Result<(), String> {
        let birthday = match bday {
            Some((day, month, year)) => {
                validate_birthday(day, month, year)?;
                Some(tl::enums::Birthday::Birthday(tl::types::Birthday {
                    day,
                    month,
                    year: year.filter(|&y| y >= 1900),
                }))
            }
            None => None,
        };
        self.client
            .invoke(&tl::functions::account::UpdateBirthday { birthday })
            .await
            .map_err(|e| format!("День рождения: {e}"))?;
        Ok(())
    }

    /// Проверить и обновить публичный `@username` аккаунта (`account.updateUsername`).
    pub async fn update_username_checked(&self, raw: &str) -> Result<String, String> {
        let clean = validate_username(raw)?;
        self.update_username(&clean)
            .await
            .map_err(|e| format!("Имя пользователя: {e}"))?;
        Ok(clean)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftRow {
    pub peer_id: i64,
    pub text: String,
    pub reply_to: Option<i32>,
    pub date: i32,
}

#[derive(Debug, Clone)]
pub struct StickerPackDetails {
    pub id: i64,
    pub access_hash: i64,
    pub title: String,
    pub short_name: String,
    pub installed: bool,
    pub emojis: bool,
    pub stickers: Vec<super::StickerRow>,
}

/// Человекочитаемая расшифровка выпавшего результата для костей/игр Telegram (`MessageMediaDice`).
pub fn dice_outcome_label(emoticon: &str, value: i32) -> String {
    if value <= 0 {
        return format!("{emoticon} Бросок…");
    }
    match emoticon.trim() {
        "🎲" => format!("🎲 Выпало: {value}"),
        "🎯" => match value {
            6 => "🎯 В яблочко!".to_owned(),
            1 => "🎯 Мимо!".to_owned(),
            v => format!("🎯 Кольцо {v}/6"),
        },
        "🏀" => match value {
            4 | 5 => "🏀 Гол в корзину!".to_owned(),
            3 => "🏀 Отскок от кольца".to_owned(),
            _ => "🏀 Мимо кольца".to_owned(),
        },
        "⚽" => match value {
            3..=5 => "⚽ ГОООЛ!".to_owned(),
            2 => "⚽ Штанга!".to_owned(),
            _ => "⚽ Мимо ворот".to_owned(),
        },
        "🎳" => match value {
            6 => "🎳 СТРАЙК!".to_owned(),
            1 => "🎳 Все кегли мимо".to_owned(),
            v => format!("🎳 Сбито кеглей: {v}"),
        },
        "🎰" => match value {
            64 => "🎰 777 ДЖЕКПОТ!".to_owned(),
            1 | 22 | 43 => "🎰 Три в ряд!".to_owned(),
            _ => format!("🎰 Комбинация #{value}"),
        },
        _ => format!("{emoticon} Результат: {value}"),
    }
}

/// Валидация `@username` по правилам Telegram (пустая строка — сброс, иначе 5..=32 ASCII `[a-zA-Z0-9_]`, начинается с буквы).
pub fn validate_username(raw: &str) -> Result<String, String> {
    let clean = raw.trim().trim_start_matches('@').to_owned();
    if clean.is_empty() {
        return Ok(String::new());
    }
    if !(5..=32).contains(&clean.len()) {
        return Err("Username должен содержать от 5 до 32 символов".to_owned());
    }
    if !clean.chars().next().is_some_and(|c| c.is_ascii_alphabetic()) {
        return Err("Username должен начинаться с латинской буквы".to_owned());
    }
    if !clean.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err("Допустимы только латинские буквы, цифры и '_'".to_owned());
    }
    if clean.ends_with('_') || clean.contains("__") {
        return Err("Username не может заканчиваться на '_' или содержать '__'".to_owned());
    }
    Ok(clean)
}

/// Валидация даты рождения с учётом количества дней в месяце и високосных лет.
pub fn validate_birthday(day: i32, month: i32, year: Option<i32>) -> Result<(), String> {
    if !(1..=12).contains(&month) {
        return Err("Месяц должен быть от 1 до 12".to_owned());
    }
    let leap = match year {
        Some(y) if y >= 1900 => (y % 4 == 0 && y % 100 != 0) || (y % 400 == 0),
        Some(y) if y > 0 => return Err("Год должен быть не меньше 1900".to_owned()),
        _ => true,
    };
    let max_day = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => if leap { 29 } else { 28 },
        _ => 31,
    };
    if !(1..=max_day).contains(&day) {
        return Err(format!("В выбранном месяце от 1 до {max_day} дней"));
    }
    if let Some(y) = year
        && y > 2026
    {
        return Err("Укажите реальный год рождения".to_owned());
    }
    Ok(())
}

#[cfg(test)]
mod v21_ext_tests {
    use super::*;

    #[test]
    fn dice_outcomes_cover_games() {
        assert_eq!(dice_outcome_label("🎲", 6), "🎲 Выпало: 6");
        assert!(dice_outcome_label("🎯", 6).contains("яблочко"));
        assert!(dice_outcome_label("🏀", 5).contains("Гол"));
        assert!(dice_outcome_label("⚽", 4).contains("ГОООЛ"));
        assert!(dice_outcome_label("🎳", 6).contains("СТРАЙК"));
        assert!(dice_outcome_label("🎰", 64).contains("ДЖЕКПОТ"));
    }

    #[test]
    fn username_validation_matches_telegram_rules() {
        assert_eq!(validate_username("  @Rust_Gramm  ").unwrap(), "Rust_Gramm");
        assert_eq!(validate_username("").unwrap(), "");
        assert!(validate_username("abcd").is_err());
        assert!(validate_username("1badname").is_err());
        assert!(validate_username("bad__name").is_err());
        assert!(validate_username("badname_").is_err());
    }

    #[test]
    fn birthday_validation_checks_calendar_bounds() {
        assert!(validate_birthday(29, 2, Some(2000)).is_ok());
        assert!(validate_birthday(29, 2, Some(2001)).is_err());
        assert!(validate_birthday(31, 4, Some(1999)).is_err());
        assert!(validate_birthday(15, 7, None).is_ok());
        assert!(validate_birthday(1, 13, None).is_err());
    }
}

#[derive(Debug, Clone)]
pub struct InlineBotResultRow {
    pub id: String,
    pub kind: String,
    pub title: String,
    pub description: String,
    pub thumb_url: Option<String>,
    pub send_text: String,
}

#[derive(Debug, Clone)]
pub struct InlineBotResultsPage {
    pub query_id: i64,
    pub gallery: bool,
    pub next_offset: String,
    pub results: Vec<InlineBotResultRow>,
    pub bot_username: String,
}

#[derive(Debug, Clone)]
pub struct AvailableReactionRow {
    pub emoji: String,
    pub title: String,
    pub premium: bool,
}

#[derive(Debug, Clone)]
pub struct ReadParticipantRow {
    pub user_id: i64,
    pub name: String,
    pub date: i32,
}

#[derive(Debug, Clone)]
pub struct MessageReactionPeerRow {
    pub peer_id: i64,
    pub name: String,
    pub reaction: String,
    pub date: i32,
}

fn extract_inline_send_text(msg: &tl::enums::BotInlineMessage) -> String {
    match msg {
        tl::enums::BotInlineMessage::MediaAuto(m) => m.message.clone(),
        tl::enums::BotInlineMessage::Text(m) => m.message.clone(),
        _ => String::new(),
    }
}

/// Локальная офлайн-расшифровка голосового сообщения (без лимитов и без сторонних хостов):
/// 1) если пользователь положил локальный `whisper-cli.exe` / `whisper.exe` и `.bin`-модель в `<RG_HOME>/whisper/`, использует его;
/// 2) на Windows также проверяет встроенный системный движок речи (System.Speech) для WAV-файлов.
fn local_transcribe_audio(audio_path: &Path, whisper_dir: &Path) -> Result<String, String> {
    // 1. Локальный whisper-cli (whisper.cpp) без сети.
    for bin_name in ["whisper-cli.exe", "main.exe", "whisper.exe", "whisper-cli"] {
        let bin = whisper_dir.join(bin_name);
        if bin.exists()
            && let Some(model) = std::fs::read_dir(whisper_dir)
                .ok()
                .and_then(|rd| {
                    rd.flatten()
                        .map(|e| e.path())
                        .find(|p| p.extension().and_then(|e| e.to_str()) == Some("bin"))
                })
        {
            let mut cmd = std::process::Command::new(&bin);
            cmd.args(["-m"])
                .arg(&model)
                .args(["-l", "auto", "-nt", "-f"])
                .arg(audio_path);
            #[cfg(windows)]
            {
                use std::os::windows::process::CommandExt;
                cmd.creation_flags(0x0800_0000);
            }
            if let Ok(out) = cmd.output()
                && out.status.success()
            {
                let txt = String::from_utf8_lossy(&out.stdout).trim().to_owned();
                if !txt.is_empty() {
                    return Ok(txt);
                }
            }
        }
    }
    // 2. На Windows для WAV-файлов — встроенный офлайн System.Speech.Recognition (нулевая сеть).
    #[cfg(windows)]
    if audio_path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("wav"))
    {
        use std::os::windows::process::CommandExt;
        let path_str = audio_path.to_string_lossy().replace('\'', "''");
        let ps = format!(
            "Add-Type -AssemblyName System.Speech; \
             $r = New-Object System.Speech.Recognition.SpeechRecognitionEngine; \
             $r.LoadGrammar((New-Object System.Speech.Recognition.DictationGrammar)); \
             $r.SetInputToWaveFile('{path_str}'); \
             $out = @(); while ($true) {{ $res = $r.Recognize(); if (-not $res) {{ break }}; $out += $res.Text }}; \
             Write-Output ($out -join ' ')"
        );
        if let Ok(out) = std::process::Command::new("powershell")
            .args(["-NoProfile", "-NonInteractive", "-Command", &ps])
            .creation_flags(0x0800_0000)
            .output()
            && out.status.success()
        {
            let txt = String::from_utf8_lossy(&out.stdout).trim().to_owned();
            if !txt.is_empty() {
                return Ok(txt);
            }
        }
    }
    Err(format!(
        "Лимит серверной расшифровки Telegram исчерпан. Для безлимитной офлайн-расшифровки положите локальный whisper-cli + модель .bin в {}",
        whisper_dir.display()
    ))
}

/// Строка похожего канала из `channels.getChannelRecommendations`.
#[derive(Debug, Clone)]
pub struct SimilarChannelRow {
    pub peer: PeerRef,
    pub id: i64,
    pub title: String,
    pub username: String,
    pub participants: i32,
}

/// Разбирает дату формата `ДД.ММ.ГГГГ` в unix timestamp (конец указанного дня UTC, 23:59:59).
pub fn parse_date_dmy(input: &str) -> Option<i32> {
    let trimmed = input.trim();
    let mut parts = trimmed.split(['.', '-', '/']);
    let d: i64 = parts.next()?.trim().parse().ok()?;
    let m: i64 = parts.next()?.trim().parse().ok()?;
    let y: i64 = parts.next()?.trim().parse().ok()?;
    if parts.next().is_some() || !(1970..=2099).contains(&y) || !(1..=12).contains(&m) {
        return None;
    }
    let max_d = days_in_month(y as i32, m as u32) as i64;
    if d < 1 || d > max_d {
        return None;
    }
    let days = days_from_civil(y, m, d);
    let ts = days * 86_400 + 86_399;
    i32::try_from(ts).ok()
}

pub fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 => {
            let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
            if leap { 29 } else { 28 }
        }
        _ => 30,
    }
}

/// Преобразует дату (UTC) в число дней от 1970-01-01 (алгоритм Hinnant `days_from_civil`).
pub fn days_from_civil(mut y: i64, m: i64, d: i64) -> i64 {
    if m <= 2 {
        y -= 1;
    }
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let doy = (153 * (if m > 2 { m - 3 } else { m + 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

impl Engine {
    /// Загружает похожие каналы (`channels.getChannelRecommendations`).
    pub async fn similar_channels(&self, peer: PeerRef) -> Result<Vec<SimilarChannelRow>, String> {
        use grammers_session::types::{PeerAuth, PeerId};
        let input_ch = peer_to_input_channel(peer)?;
        let res = self
            .invoke_retry(
                "channels.getChannelRecommendations",
                &tl::functions::channels::GetChannelRecommendations {
                    channel: Some(input_ch),
                },
            )
            .await
            .map_err(|e| format!("Похожие каналы: {e}"))?;
        let chats = match res {
            tl::enums::messages::Chats::Chats(c) => c.chats,
            tl::enums::messages::Chats::Slice(c) => c.chats,
        };
        let mut out = Vec::new();
        for ch in chats {
            if let tl::enums::Chat::Channel(c) = ch
                && let Some(pid) = PeerId::channel(c.id)
            {
                let peer = PeerRef {
                    id: pid,
                    auth: PeerAuth::from_hash(c.access_hash.unwrap_or(0)),
                };
                out.push(SimilarChannelRow {
                    peer,
                    id: -1_000_000_000_000 - c.id,
                    title: c.title,
                    username: c.username.unwrap_or_default(),
                    participants: c.participants_count.unwrap_or(0),
                });
            }
        }
        Ok(out)
    }

    /// Переход к сообщениям на указанную дату (`messages.getHistory { offset_date }`).
    /// При `target_unix <= 1` загружает начало переписки.
    pub async fn history_at_date(
        &self,
        peer: PeerRef,
        target_unix: i32,
    ) -> Result<Vec<super::MessageRow>, String> {
        let (offset_id, offset_date, add_offset, limit) = if target_unix <= 1 {
            (1, 0, -39, 40)
        } else {
            (0, target_unix, -15, 40)
        };
        let res = self
            .invoke_retry(
                "messages.getHistory(date)",
                &tl::functions::messages::GetHistory {
                    peer: peer.into(),
                    offset_id,
                    offset_date,
                    add_offset,
                    limit,
                    max_id: 0,
                    min_id: 0,
                    hash: 0,
                },
            )
            .await
            .map_err(|e| format!("Переход к дате: {e}"))?;
        let mut rows = super::tl_messages_to_rows(&res);
        rows.sort_by_key(|m| (m.date, m.id));
        Ok(rows)
    }

    /// Общие группы/каналы с пользователем (`messages.getCommonChats`).
    pub async fn common_chats(
        &self,
        user: PeerRef,
        limit: i32,
    ) -> Result<Vec<super::ContactRow>, String> {
        use grammers_session::types::{PeerAuth, PeerId};
        let user_id = peer_to_input_user(user)?;
        let res = self
            .invoke_retry(
                "messages.getCommonChats",
                &tl::functions::messages::GetCommonChats {
                    user_id,
                    max_id: 0,
                    limit: limit.clamp(1, 100),
                },
            )
            .await
            .map_err(|e| format!("Общие группы: {e}"))?;
        let chats = match res {
            tl::enums::messages::Chats::Chats(c) => c.chats,
            tl::enums::messages::Chats::Slice(c) => c.chats,
        };
        let mut out = Vec::new();
        for ch in chats {
            match ch {
                tl::enums::Chat::Channel(c) => {
                    let Some(pid) = PeerId::channel(c.id) else {
                        continue;
                    };
                    let peer = PeerRef {
                        id: pid,
                        auth: PeerAuth::from_hash(c.access_hash.unwrap_or(0)),
                    };
                    let status = c
                        .participants_count
                        .map_or_else(|| "Супергруппа".to_owned(), |n| format!("{n} участников"));
                    out.push(super::ContactRow {
                        peer,
                        id: -1_000_000_000_000 - c.id,
                        name: c.title,
                        status,
                    });
                }
                tl::enums::Chat::Chat(c) => {
                    let Some(pid) = PeerId::chat(c.id) else {
                        continue;
                    };
                    let peer = PeerRef {
                        id: pid,
                        auth: PeerAuth::default(),
                    };
                    out.push(super::ContactRow {
                        peer,
                        id: -c.id,
                        name: c.title,
                        status: format!("{} участников", c.participants_count),
                    });
                }
                _ => {}
            }
        }
        Ok(out)
    }

    /// Добавить или обновить пользователя в списке контактов Telegram (`contacts.addContact`).
    pub async fn add_contact(
        &self,
        user: PeerRef,
        first_name: &str,
        last_name: &str,
        phone: &str,
    ) -> Result<(), String> {
        let first = first_name.trim();
        if first.is_empty() {
            return Err("Укажите имя контакта".to_owned());
        }
        let id = peer_to_input_user(user)?;
        self.invoke_retry(
            "contacts.addContact",
            &tl::functions::contacts::AddContact {
                add_phone_privacy_exception: false,
                id,
                first_name: first.to_owned(),
                last_name: last_name.trim().to_owned(),
                phone: phone.trim().to_owned(),
                note: None,
            },
        )
        .await
        .map_err(|e| format!("Добавление контакта: {e}"))?;
        Ok(())
    }

    /// Удалить пользователя из контактов Telegram (`contacts.deleteContacts`).
    pub async fn delete_contact(&self, user: PeerRef) -> Result<(), String> {
        let id = peer_to_input_user(user)?;
        self.invoke_retry(
            "contacts.deleteContacts",
            &tl::functions::contacts::DeleteContacts { id: vec![id] },
        )
        .await
        .map_err(|e| format!("Удаление контакта: {e}"))?;
        Ok(())
    }

    // =========================================================================
    // v0.23.0 — 5 новых функций: глобальный поиск сообщений, очистка/выход,
    // панель закрепов + открепить все, расширенная пересылка AyuGram, папки-ссылки
    // =========================================================================

    /// 1. Глобальный поиск по тексту сообщений во всех чатах (`messages.searchGlobal`).
    ///
    /// `scope`: `0` = все, `1` = личные, `2` = группы, `3` = каналы.
    pub async fn search_global_messages(
        &self,
        query: &str,
        scope: u8,
        limit: i32,
    ) -> Result<Vec<GlobalMessageHit>, String> {
        let q = query.trim();
        if q.chars().count() < 2 {
            return Ok(Vec::new());
        }
        let res = self
            .invoke_retry(
                "messages.searchGlobal",
                &tl::functions::messages::SearchGlobal {
                    broadcasts_only: scope == 3,
                    groups_only: scope == 2,
                    users_only: scope == 1,
                    folder_id: None,
                    community: None,
                    q: q.to_owned(),
                    filter: tl::enums::MessagesFilter::InputMessagesFilterEmpty,
                    min_date: 0,
                    max_date: 0,
                    offset_rate: 0,
                    offset_peer: tl::enums::InputPeer::Empty,
                    offset_id: 0,
                    limit: limit.clamp(1, 60),
                },
            )
            .await
            .map_err(|e| format!("Поиск по сообщениям: {e}"))?;
        let (msgs, chats, users) = match &res {
            tl::enums::messages::Messages::Messages(x) => (&x.messages, &x.chats, &x.users),
            tl::enums::messages::Messages::Slice(x) => (&x.messages, &x.chats, &x.users),
            tl::enums::messages::Messages::ChannelMessages(x) => (&x.messages, &x.chats, &x.users),
            tl::enums::messages::Messages::NotModified(_) => return Ok(Vec::new()),
        };
        if let Some(sess) = &self.session {
            crate::cache_tl_peers(&**sess, users, chats).await;
        }
        let peer_map = build_tl_peer_lookup(users, chats);
        let mut out = Vec::new();
        for m in msgs {
            let tl::enums::Message::Message(x) = m else {
                continue;
            };
            let did = match &x.peer_id {
                tl::enums::Peer::User(u) => u.user_id,
                tl::enums::Peer::Chat(c) => -c.chat_id,
                tl::enums::Peer::Channel(c) => -1_000_000_000_000 - c.channel_id,
            };
            let Some((peer, peer_name)) = peer_map.get(&did).cloned() else {
                continue;
            };
            let snippet = if !x.message.is_empty() {
                x.message.clone()
            } else {
                "[Медиа / Вложение]".to_owned()
            };
            out.push(GlobalMessageHit {
                peer,
                dialog_id: did,
                peer_name,
                msg_id: x.id,
                text: snippet,
                date: i64::from(x.date),
                outgoing: x.out,
            });
        }
        Ok(out)
    }

    /// 2а. Очистить историю чата (`messages.deleteHistory` или `channels.deleteHistory`).
    /// Если включено «Сохранять удалённые», все локально сохранённые сообщения чата отмечаются `deleted=1`.
    pub async fn clear_history(&self, peer: PeerRef, revoke: bool) -> Result<(), String> {
        if self.settings().ayu.save_deleted_messages {
            let _ = self
                .store
                .mark_all_peer_deleted(super::peer_key(&peer))
                .await;
        }
        match peer_to_input_channel(peer) {
            Ok(channel) => {
                self.invoke_retry(
                    "channels.deleteHistory",
                    &tl::functions::channels::DeleteHistory {
                        for_everyone: revoke,
                        channel,
                        max_id: 0,
                    },
                )
                .await
                .map_err(|e| format!("Очистка истории: {e}"))?;
            }
            Err(_) => {
                self.invoke_retry(
                    "messages.deleteHistory",
                    &tl::functions::messages::DeleteHistory {
                        just_clear: true,
                        revoke,
                        peer: peer.into(),
                        max_id: 0,
                        min_date: None,
                        max_date: None,
                    },
                )
                .await
                .map_err(|e| format!("Очистка истории: {e}"))?;
            }
        }
        Ok(())
    }

    /// 2б. Покинуть группу, супергруппу или канал (`channels.leaveChannel` / `messages.deleteChatUser`).
    pub async fn leave_chat(&self, peer: PeerRef) -> Result<(), String> {
        match peer_to_input_channel(peer) {
            Ok(channel) => {
                self.invoke_retry(
                    "channels.leaveChannel",
                    &tl::functions::channels::LeaveChannel { channel },
                )
                .await
                .map_err(|e| format!("Выход из канала/группы: {e}"))?;
                Ok(())
            }
            Err(_) => {
                let id = super::peer_key(&peer);
                if id < 0 && id > -1_000_000_000_000 {
                    self.invoke_retry(
                        "messages.deleteChatUser",
                        &tl::functions::messages::DeleteChatUser {
                            revoke_history: false,
                            chat_id: -id,
                            user_id: tl::enums::InputUser::UserSelf,
                        },
                    )
                    .await
                    .map_err(|e| format!("Выход из группы: {e}"))?;
                    Ok(())
                } else {
                    Err("Нельзя покинуть личный диалог".to_owned())
                }
            }
        }
    }

    /// 3а. Загрузить все закреплённые сообщения чата (`messages.search` + `InputMessagesFilterPinned`).
    pub async fn pinned_messages(
        &self,
        peer: PeerRef,
        limit: i32,
    ) -> Result<Vec<super::MessageRow>, String> {
        let res = self
            .invoke_retry(
                "messages.search(pinned)",
                &tl::functions::messages::Search {
                    peer: peer.into(),
                    q: String::new(),
                    from_id: None,
                    saved_peer_id: None,
                    saved_reaction: None,
                    top_msg_id: None,
                    filter: tl::enums::MessagesFilter::InputMessagesFilterPinned,
                    min_date: 0,
                    max_date: 0,
                    offset_id: 0,
                    add_offset: 0,
                    limit: limit.clamp(1, 100),
                    max_id: 0,
                    min_id: 0,
                    hash: 0,
                },
            )
            .await
            .map_err(|e| format!("Закреплённые сообщения: {e}"))?;
        let mut rows = super::tl_messages_to_rows(&res);
        rows.sort_by_key(|m| (-m.date, -m.id));
        Ok(rows)
    }

    /// 3б. Открепить все закреплённые сообщения чата (`messages.unpinAllMessages`).
    pub async fn unpin_all_messages(&self, peer: PeerRef) -> Result<(), String> {
        self.invoke_retry(
            "messages.unpinAllMessages",
            &tl::functions::messages::UnpinAllMessages {
                peer: peer.into(),
                top_msg_id: None,
                saved_peer_id: None,
            },
        )
        .await
        .map_err(|e| format!("Открепление всех сообщений: {e}"))?;
        Ok(())
    }

    /// 4. Расширенная пересылка AyuGram: без автора, без подписей к медиа, без звука и пачкой (`messages.forwardMessages`).
    pub async fn forward_advanced(
        &self,
        from: PeerRef,
        to: PeerRef,
        ids: &[i32],
        opts: ForwardOptions,
    ) -> Result<(), String> {
        let clean: Vec<i32> = ids.iter().copied().filter(|&id| id > 0).collect();
        if clean.is_empty() {
            return Err("Не выбраны сообщения для пересылки".to_owned());
        }
        let cfg = self.settings();
        let random_id: Vec<i64> = clean.iter().map(|_| rand_i64()).collect();
        let silent = opts.silent || cfg.ghost.should_send_without_sound();
        self.apply_traffic_jitter().await;
        self.invoke_retry(
            "messages.forwardMessages(adv)",
            &tl::functions::messages::ForwardMessages {
                silent,
                background: false,
                with_my_score: false,
                drop_author: opts.drop_author,
                drop_media_captions: opts.drop_media_captions,
                noforwards: false,
                allow_paid_floodskip: false,
                from_ephemeral: false,
                from_peer: from.into(),
                id: clean,
                random_id,
                to_peer: to.into(),
                top_msg_id: None,
                reply_to: None,
                schedule_date: ghost::schedule_date(&cfg.ghost),
                schedule_repeat_period: None,
                send_as: None,
                quick_reply_shortcut: None,
                effect: None,
                video_timestamp: None,
                allow_paid_stars: None,
                suggested_post: None,
            },
        )
        .await
        .map_err(|e| format!("Пересылка: {e}"))?;
        self.send_offline_if_needed(&cfg.ghost).await;
        Ok(())
    }

    /// 5а. Экспорт пригласительной ссылки на папку чатов (`chatlists.exportChatlistInvite`).
    pub async fn export_folder_invite(
        &self,
        folder_id: i32,
        title: &str,
        peers: &[PeerRef],
    ) -> Result<String, String> {
        let input_peers: Vec<tl::enums::InputPeer> = peers.iter().copied().map(Into::into).collect();
        if input_peers.is_empty() {
            return Err("Добавьте хотя бы один чат в папку (явные чаты), чтобы поделиться ссылкой".to_owned());
        }
        let res = self
            .invoke_retry(
                "chatlists.exportChatlistInvite",
                &tl::functions::chatlists::ExportChatlistInvite {
                    chatlist: tl::enums::InputChatlist::DialogFilter(
                        tl::types::InputChatlistDialogFilter { filter_id: folder_id },
                    ),
                    title: title.trim().to_owned(),
                    peers: input_peers,
                },
            )
            .await
            .map_err(|e| format!("Экспорт папки: {e}"))?;
        let tl::enums::chatlists::ExportedChatlistInvite::Invite(exp) = res;
        let tl::enums::ExportedChatlistInvite::Invite(inv) = exp.invite;
        Ok(inv.url)
    }

    /// 5б. Проверить пригласительную ссылку на папку чатов (`chatlists.checkChatlistInvite`).
    pub async fn check_folder_invite(&self, slug_or_url: &str) -> Result<FolderInvitePreview, String> {
        let slug = extract_chatlist_slug(slug_or_url)
            .ok_or_else(|| "Неверный формат ссылки на папку (нужен t.me/addlist/...)".to_owned())?;
        let res = self
            .invoke_retry(
                "chatlists.checkChatlistInvite",
                &tl::functions::chatlists::CheckChatlistInvite { slug: slug.clone() },
            )
            .await
            .map_err(|e| format!("Проверка папки: {e}"))?;
        match res {
            tl::enums::chatlists::ChatlistInvite::Invite(inv) => {
                if let Some(sess) = &self.session {
                    crate::cache_tl_peers(&**sess, &inv.users, &inv.chats).await;
                }
                let lookup = build_tl_peer_lookup(&inv.users, &inv.chats);
                let tl::enums::TextWithEntities::Entities(twe) = inv.title;
                let peers = resolve_peer_list(&inv.peers, &lookup);
                Ok(FolderInvitePreview {
                    slug,
                    title: twe.text,
                    peers,
                    already_joined: false,
                })
            }
            tl::enums::chatlists::ChatlistInvite::Already(al) => {
                if let Some(sess) = &self.session {
                    crate::cache_tl_peers(&**sess, &al.users, &al.chats).await;
                }
                let lookup = build_tl_peer_lookup(&al.users, &al.chats);
                let mut combined = al.missing_peers;
                combined.extend(al.already_peers);
                let peers = resolve_peer_list(&combined, &lookup);
                Ok(FolderInvitePreview {
                    slug,
                    title: format!("Папка #{}", al.filter_id),
                    peers,
                    already_joined: true,
                })
            }
        }
    }

    /// 5в. Присоединиться к папке чатов по ссылке (`chatlists.joinChatlistInvite`).
    pub async fn join_folder_invite(&self, slug: &str, peers: &[PeerRef]) -> Result<(), String> {
        let clean_slug = extract_chatlist_slug(slug).unwrap_or_else(|| slug.trim().to_owned());
        let input_peers: Vec<tl::enums::InputPeer> = peers.iter().copied().map(Into::into).collect();
        self.invoke_retry(
            "chatlists.joinChatlistInvite",
            &tl::functions::chatlists::JoinChatlistInvite {
                slug: clean_slug,
                peers: input_peers,
            },
        )
        .await
        .map_err(|e| format!("Добавление папки: {e}"))?;
        Ok(())
    }
}

/// Результат глобального поиска сообщений (`messages.searchGlobal`).
#[derive(Debug, Clone)]
pub struct GlobalMessageHit {
    pub peer: PeerRef,
    pub dialog_id: i64,
    pub peer_name: String,
    pub msg_id: i32,
    pub text: String,
    pub date: i64,
    pub outgoing: bool,
}

/// Параметры расширенной пересылки сообщений (AyuGram Forward).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ForwardOptions {
    pub drop_author: bool,
    pub drop_media_captions: bool,
    pub silent: bool,
}

/// Предпросмотр приглашения в общую папку чатов (`t.me/addlist/...`).
#[derive(Debug, Clone)]
pub struct FolderInvitePreview {
    pub slug: String,
    pub title: String,
    pub peers: Vec<(PeerRef, i64, String)>,
    pub already_joined: bool,
}

pub fn extract_chatlist_slug(raw: &str) -> Option<String> {
    let s = raw.trim();
    if s.is_empty() {
        return None;
    }
    if let Some(after) = s.split("addlist/").nth(1) {
        let slug = after.split(['?', '#', '/']).next()?.trim();
        return (!slug.is_empty()).then(|| slug.to_owned());
    }
    if let Some(after) = s.split("slug=").nth(1) {
        let slug = after.split(['&', '#']).next()?.trim();
        return (!slug.is_empty()).then(|| slug.to_owned());
    }
    if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-') && s.len() >= 3 {
        return Some(s.to_owned());
    }
    None
}

fn build_tl_peer_lookup(
    users: &[tl::enums::User],
    chats: &[tl::enums::Chat],
) -> std::collections::HashMap<i64, (PeerRef, String)> {
    use grammers_session::types::{PeerAuth, PeerId};
    let mut map = std::collections::HashMap::with_capacity(users.len() + chats.len());
    for u in users {
        if let tl::enums::User::User(x) = u
            && let Some(pid) = PeerId::user(x.id)
        {
            let name = format!(
                "{} {}",
                x.first_name.as_deref().unwrap_or(""),
                x.last_name.as_deref().unwrap_or("")
            )
            .trim()
            .to_owned();
            let peer = PeerRef {
                id: pid,
                auth: PeerAuth::from_hash(x.access_hash.unwrap_or(0)),
            };
            map.insert(x.id, (peer, if name.is_empty() { format!("User {}", x.id) } else { name }));
        }
    }
    for c in chats {
        match c {
            tl::enums::Chat::Channel(x) => {
                if let Some(pid) = PeerId::channel(x.id) {
                    let did = -1_000_000_000_000 - x.id;
                    let peer = PeerRef {
                        id: pid,
                        auth: PeerAuth::from_hash(x.access_hash.unwrap_or(0)),
                    };
                    map.insert(did, (peer, x.title.clone()));
                }
            }
            tl::enums::Chat::Chat(x) => {
                if let Some(pid) = PeerId::chat(x.id) {
                    let peer = PeerRef {
                        id: pid,
                        auth: PeerAuth::default(),
                    };
                    map.insert(-x.id, (peer, x.title.clone()));
                }
            }
            _ => {}
        }
    }
    map
}

fn resolve_peer_list(
    peers: &[tl::enums::Peer],
    lookup: &std::collections::HashMap<i64, (PeerRef, String)>,
) -> Vec<(PeerRef, i64, String)> {
    let mut out = Vec::with_capacity(peers.len());
    for p in peers {
        let did = match p {
            tl::enums::Peer::User(u) => u.user_id,
            tl::enums::Peer::Chat(c) => -c.chat_id,
            tl::enums::Peer::Channel(c) => -1_000_000_000_000 - c.channel_id,
        };
        if let Some((pr, name)) = lookup.get(&did) {
            out.push((*pr, did, name.clone()));
        }
    }
    out
}

// =========================================================================
// Этап 45 (v0.32.0): 5 новых функций Telegram
// =========================================================================

/// Фотография профиля пользователя из `photos.getUserPhotos`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProfilePhotoItem {
    pub id: i64,
    pub access_hash: i64,
    pub file_reference: Vec<u8>,
    pub dc_id: i32,
    pub date: i32,
    pub cache_path: Option<PathBuf>,
}

/// Глобальные настройки приватности (`account.getGlobalPrivacySettings`)
/// и фильтр чувствительного контента (`account.getContentSettings`).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GlobalPrivacyInfo {
    pub archive_and_mute_new_noncontact_peers: bool,
    pub keep_archived_unmuted: bool,
    pub keep_archived_folders: bool,
    pub hide_read_marks: bool,
    pub new_noncontact_peers_require_premium: bool,
    pub display_gifts_button: bool,
    pub noncontact_peers_paid_stars: Option<i64>,
    pub sensitive_enabled: bool,
    pub sensitive_can_change: bool,
}

/// Проверяет и нормализует новое название чата/канала перед отправкой.
pub fn validate_chat_title(title: &str) -> Result<String, String> {
    let clean = title.trim();
    if clean.is_empty() {
        return Err("Название не может быть пустым".to_owned());
    }
    if clean.chars().count() > 128 {
        return Err("Название слишком длинное (макс. 128 символов)".to_owned());
    }
    Ok(clean.to_owned())
}

impl Engine {
    fn extract_msg_input_document(media: &grammers_client::media::Media) -> Option<tl::enums::InputDocument> {
        let doc = match media {
            grammers_client::media::Media::Sticker(s) => s.document.raw.document.as_ref(),
            grammers_client::media::Media::Document(d) => d.raw.document.as_ref(),
            _ => None,
        }?;
        let tl::enums::Document::Document(d) = doc else {
            return None;
        };
        Some(tl::enums::InputDocument::Document(tl::types::InputDocument {
            id: d.id,
            access_hash: d.access_hash,
            file_reference: d.file_reference.clone(),
        }))
    }

    /// 1а. Добавить/убрать стикер из избранного (`messages.faveSticker`).
    pub async fn fave_sticker(
        &self,
        id: i64,
        access_hash: i64,
        file_ref: &[u8],
        unfave: bool,
    ) -> Result<(), String> {
        let doc = tl::enums::InputDocument::Document(tl::types::InputDocument {
            id,
            access_hash,
            file_reference: file_ref.to_vec(),
        });
        self.invoke_retry(
            "messages.faveSticker",
            &tl::functions::messages::FaveSticker { id: doc, unfave },
        )
        .await
        .map_err(|e| format!("Избранные стикеры: {e}"))?;
        Ok(())
    }

    async fn msg_input_document(
        &self,
        peer: PeerRef,
        msg_id: i32,
        kind_label: &str,
    ) -> Result<tl::enums::InputDocument, String> {
        let Some(m) = self
            .message_by_id(peer, msg_id)
            .await
            .map_err(|e| format!("Поиск {kind_label}: {e}"))?
        else {
            return Err(format!("Сообщение ({kind_label}) не найдено"));
        };
        let Some(media) = m.media() else {
            return Err(format!("В сообщении нет {kind_label}"));
        };
        Self::extract_msg_input_document(&media)
            .ok_or_else(|| format!("Документ ({kind_label}) недоступен"))
    }

    /// 1б. Добавить/убрать стикер из избранного по сообщению в ленте.
    pub async fn fave_sticker_from_msg(
        &self,
        peer: PeerRef,
        msg_id: i32,
        unfave: bool,
    ) -> Result<(), String> {
        let doc = self.msg_input_document(peer, msg_id, "стикера").await?;
        self.invoke_retry(
            "messages.faveSticker",
            &tl::functions::messages::FaveSticker { id: doc, unfave },
        )
        .await
        .map_err(|e| format!("Избранные стикеры: {e}"))?;
        Ok(())
    }

    /// 1в. Сохранить или удалить GIF в коллекции (`messages.saveGif`).
    pub async fn save_gif(
        &self,
        id: i64,
        access_hash: i64,
        file_ref: &[u8],
        unsave: bool,
    ) -> Result<(), String> {
        let doc = tl::enums::InputDocument::Document(tl::types::InputDocument {
            id,
            access_hash,
            file_reference: file_ref.to_vec(),
        });
        self.invoke_retry(
            "messages.saveGif",
            &tl::functions::messages::SaveGif { id: doc, unsave },
        )
        .await
        .map_err(|e| format!("Сохранённые GIF: {e}"))?;
        Ok(())
    }

    /// 1г. Сохранить/удалить GIF по сообщению в ленте (`messages.saveGif`).
    pub async fn save_gif_from_msg(
        &self,
        peer: PeerRef,
        msg_id: i32,
        unsave: bool,
    ) -> Result<(), String> {
        let doc = self.msg_input_document(peer, msg_id, "GIF").await?;
        self.invoke_retry(
            "messages.saveGif",
            &tl::functions::messages::SaveGif { id: doc, unsave },
        )
        .await
        .map_err(|e| format!("Сохранённые GIF: {e}"))?;
        Ok(())
    }

    /// 1д. Очистить историю недавних стикеров (`messages.clearRecentStickers`).
    pub async fn clear_recent_stickers(&self) -> Result<(), String> {
        self.invoke_retry(
            "messages.clearRecentStickers",
            &tl::functions::messages::ClearRecentStickers { attached: false },
        )
        .await
        .map_err(|e| format!("Очистка недавних стикеров: {e}"))?;
        Ok(())
    }

    /// 2. Точное время прочтения исходящего сообщения в личном чате (`messages.getOutboxReadDate`).
    pub async fn outbox_read_date(&self, peer: PeerRef, msg_id: i32) -> Result<i32, String> {
        let tl::enums::OutboxReadDate::Date(d) = self
            .invoke_retry(
                "messages.getOutboxReadDate",
                &tl::functions::messages::GetOutboxReadDate {
                    peer: peer.into(),
                    msg_id,
                },
            )
            .await
            .map_err(|e| {
                let s = e.to_string();
                if s.contains("USER_PRIVACY_RESTRICTED") {
                    "Время прочтения скрыто настройками приватности".to_owned()
                } else if s.contains("MESSAGE_ID_INVALID") || s.contains("MESSAGE_NOT_READ") {
                    "Сообщение ещё не прочитано или устарело".to_owned()
                } else {
                    format!("Время прочтения: {s}")
                }
            })?;
        Ok(d.date)
    }

    /// 3а. Изменить название и/или описание группы или канала.
    pub async fn edit_chat_title_and_about(
        &self,
        peer: PeerRef,
        title: Option<&str>,
        about: Option<&str>,
    ) -> Result<(), String> {
        if let Some(raw_title) = title {
            let clean = validate_chat_title(raw_title)?;
            match peer_to_input_channel(peer) {
                Ok(channel) => {
                    self.invoke_retry(
                        "channels.editTitle",
                        &tl::functions::channels::EditTitle { channel, title: clean },
                    )
                    .await
                    .map_err(|e| format!("Изменение названия: {e}"))?;
                }
                Err(_) => {
                    let tl::enums::InputPeer::Chat(c) = tl::enums::InputPeer::from(peer) else {
                        return Err("Нужна группа или канал".to_owned());
                    };
                    self.invoke_retry(
                        "messages.editChatTitle",
                        &tl::functions::messages::EditChatTitle {
                            chat_id: c.chat_id,
                            title: clean,
                        },
                    )
                    .await
                    .map_err(|e| format!("Изменение названия: {e}"))?;
                }
            }
        }
        if let Some(raw_about) = about {
            self.invoke_retry(
                "messages.editChatAbout",
                &tl::functions::messages::EditChatAbout {
                    peer: peer.into(),
                    about: raw_about.trim().chars().take(255).collect(),
                },
            )
            .await
            .map_err(|e| format!("Изменение описания: {e}"))?;
        }
        Ok(())
    }

    /// 3б. Запретить копирование, пересылку и сохранение контента (`messages.toggleNoForwards`).
    pub async fn toggle_no_forwards(&self, peer: PeerRef, enabled: bool) -> Result<(), String> {
        self.invoke_retry(
            "messages.toggleNoForwards",
            &tl::functions::messages::ToggleNoForwards {
                peer: peer.into(),
                enabled,
                request_msg_id: None,
            },
        )
        .await
        .map_err(|e| format!("Защита контента: {e}"))?;
        Ok(())
    }

    /// 3в. Скрывать или показывать историю чата для новых участников (`channels.togglePreHistoryHidden`).
    pub async fn toggle_pre_history_hidden(&self, peer: PeerRef, enabled: bool) -> Result<(), String> {
        let channel = peer_to_input_channel(peer)?;
        self.invoke_retry(
            "channels.togglePreHistoryHidden",
            &tl::functions::channels::TogglePreHistoryHidden { channel, enabled },
        )
        .await
        .map_err(|e| format!("История для новых участников: {e}"))?;
        Ok(())
    }

    /// 4а. Загрузить фото профиля пользователя и закэшировать миниатюры (`photos.getUserPhotos`).
    pub async fn user_profile_photos(
        &self,
        peer: Option<PeerRef>,
        limit: i32,
    ) -> Result<(i64, Vec<ProfilePhotoItem>), String> {
        let (target_id, user_id) = match peer {
            Some(p) => (p.id.bot_api_dialog_id_unchecked(), peer_to_input_user(p)?),
            None => (0_i64, tl::enums::InputUser::UserSelf),
        };
        let res = self
            .invoke_retry(
                "photos.getUserPhotos",
                &tl::functions::photos::GetUserPhotos {
                    user_id,
                    offset: 0,
                    max_id: 0,
                    limit: limit.clamp(1, 20),
                },
            )
            .await
            .map_err(|e| format!("Фото профиля: {e}"))?;
        let raw_photos = match res {
            tl::enums::photos::Photos::Photos(p) => p.photos,
            tl::enums::photos::Photos::Slice(p) => p.photos,
        };
        let dir = self.dirs.cache.join("pphotos");
        let _ = std::fs::create_dir_all(&dir);
        let mut out = Vec::new();
        for p in raw_photos {
            let tl::enums::Photo::Photo(ph) = p else { continue };
            let thumb_size = ph
                .sizes
                .iter()
                .rev()
                .find_map(|s| match s {
                    tl::enums::PhotoSize::Size(x) => Some(x.r#type.clone()),
                    tl::enums::PhotoSize::Progressive(x) => Some(x.r#type.clone()),
                    _ => None,
                })
                .unwrap_or_else(|| "x".to_owned());
            let dest = dir.join(format!("pp_{}.jpg", ph.id));
            if !dest.exists() || std::fs::metadata(&dest).is_ok_and(|m| m.len() == 0) {
                let loc = super::RawLoc {
                    loc: tl::enums::InputFileLocation::InputPhotoFileLocation(tl::types::InputPhotoFileLocation {
                        id: ph.id,
                        access_hash: ph.access_hash,
                        file_reference: ph.file_reference.clone(),
                        thumb_size,
                    }),
                    dc_id: Some(ph.dc_id),
                };
                let _ = self.client.download_media(&loc, &dest).await;
            }
            let cache_path = dest.exists().then_some(dest);
            out.push(ProfilePhotoItem {
                id: ph.id,
                access_hash: ph.access_hash,
                file_reference: ph.file_reference,
                dc_id: ph.dc_id,
                date: ph.date,
                cache_path,
            });
        }
        Ok((target_id, out))
    }

    /// 4б. Загрузить новое фото профиля (`photos.uploadProfilePhoto`).
    pub async fn upload_profile_photo(&self, path: &Path) -> Result<(), String> {
        let uploaded = self
            .client
            .upload_file(path)
            .await
            .map_err(|e| format!("Загрузка файла фото: {e}"))?;
        self.invoke_retry(
            "photos.uploadProfilePhoto",
            &tl::functions::photos::UploadProfilePhoto {
                fallback: false,
                bot: None,
                file: Some(uploaded.raw),
                video: None,
                video_start_ts: None,
                video_emoji_markup: None,
            },
        )
        .await
        .map_err(|e| format!("Установка аватара: {e}"))?;
        Ok(())
    }

    /// 4в. Удалить фото профиля (`photos.deletePhotos`).
    pub async fn delete_profile_photo(
        &self,
        id: i64,
        access_hash: i64,
        file_reference: &[u8],
    ) -> Result<(), String> {
        self.invoke_retry(
            "photos.deletePhotos",
            &tl::functions::photos::DeletePhotos {
                id: vec![tl::enums::InputPhoto::Photo(tl::types::InputPhoto {
                    id,
                    access_hash,
                    file_reference: file_reference.to_vec(),
                })],
            },
        )
        .await
        .map_err(|e| format!("Удаление фото профиля: {e}"))?;
        Ok(())
    }

    /// 5а. Загрузить глобальные настройки приватности и фильтр контента 18+.
    pub async fn load_global_privacy(&self) -> Result<GlobalPrivacyInfo, String> {
        let tl::enums::GlobalPrivacySettings::Settings(gp) = self
            .invoke_retry(
                "account.getGlobalPrivacySettings",
                &tl::functions::account::GetGlobalPrivacySettings {},
            )
            .await
            .map_err(|e| format!("Глобальная приватность: {e}"))?;
        let content = self
            .invoke_retry(
                "account.getContentSettings",
                &tl::functions::account::GetContentSettings {},
            )
            .await
            .ok();
        let (sensitive_enabled, sensitive_can_change) = match content {
            Some(tl::enums::account::ContentSettings::Settings(c)) => {
                (c.sensitive_enabled, c.sensitive_can_change)
            }
            None => (false, false),
        };
        Ok(GlobalPrivacyInfo {
            archive_and_mute_new_noncontact_peers: gp.archive_and_mute_new_noncontact_peers,
            keep_archived_unmuted: gp.keep_archived_unmuted,
            keep_archived_folders: gp.keep_archived_folders,
            hide_read_marks: gp.hide_read_marks,
            new_noncontact_peers_require_premium: gp.new_noncontact_peers_require_premium,
            display_gifts_button: gp.display_gifts_button,
            noncontact_peers_paid_stars: gp.noncontact_peers_paid_stars,
            sensitive_enabled,
            sensitive_can_change,
        })
    }

    /// 5б. Сохранить глобальные настройки приватности и фильтр контента 18+.
    pub async fn save_global_privacy(&self, info: &GlobalPrivacyInfo) -> Result<(), String> {
        let tl::enums::GlobalPrivacySettings::Settings(cur) = self
            .invoke_retry(
                "account.getGlobalPrivacySettings",
                &tl::functions::account::GetGlobalPrivacySettings {},
            )
            .await
            .map_err(|e| format!("Глобальная приватность: {e}"))?;
        let disallowed_gifts = cur.disallowed_gifts;
        self.invoke_retry(
            "account.setGlobalPrivacySettings",
            &tl::functions::account::SetGlobalPrivacySettings {
                settings: tl::enums::GlobalPrivacySettings::Settings(tl::types::GlobalPrivacySettings {
                    archive_and_mute_new_noncontact_peers: info.archive_and_mute_new_noncontact_peers,
                    keep_archived_unmuted: info.keep_archived_unmuted,
                    keep_archived_folders: info.keep_archived_folders,
                    hide_read_marks: info.hide_read_marks,
                    new_noncontact_peers_require_premium: info.new_noncontact_peers_require_premium,
                    display_gifts_button: info.display_gifts_button,
                    noncontact_peers_paid_stars: info.noncontact_peers_paid_stars,
                    disallowed_gifts,
                }),
            },
        )
        .await
        .map_err(|e| format!("Сохранение приватности: {e}"))?;
        if info.sensitive_can_change {
            self.invoke_retry(
                "account.setContentSettings",
                &tl::functions::account::SetContentSettings {
                    sensitive_enabled: info.sensitive_enabled,
                },
            )
            .await
            .map_err(|e| format!("Фильтр контента: {e}"))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod v22_ext_tests {
    use super::*;

    #[test]
    fn parses_dmy_dates_and_rejects_invalid_calendar() {
        let t = parse_date_dmy("01.01.1970").unwrap();
        assert_eq!(t, 86_399);
        assert!(parse_date_dmy("29.02.2024").is_some());
        assert!(parse_date_dmy("29.02.2025").is_none());
        assert!(parse_date_dmy("32.01.2026").is_none());
        assert!(parse_date_dmy("abc").is_none());
    }

    #[test]
    fn extract_chatlist_slug_handles_urls_and_raw_slugs() {
        assert_eq!(
            extract_chatlist_slug("https://t.me/addlist/RustPack_1"),
            Some("RustPack_1".to_owned())
        );
        assert_eq!(
            extract_chatlist_slug("tg://addlist?slug=MyFolder99&ref=1"),
            Some("MyFolder99".to_owned())
        );
        assert_eq!(
            extract_chatlist_slug("JustSlug-12"),
            Some("JustSlug-12".to_owned())
        );
        assert_eq!(extract_chatlist_slug("https://example.com/foo"), None);
    }

    #[test]
    fn peer_lookup_maps_users_groups_and_channels() {
        let u = tl::enums::User::User(tl::types::User {
            is_self: false,
            contact: false,
            mutual_contact: false,
            deleted: false,
            bot: false,
            bot_chat_history: false,
            bot_nochats: false,
            verified: false,
            restricted: false,
            min: false,
            bot_inline_geo: false,
            support: false,
            scam: false,
            apply_min_photo: false,
            fake: false,
            bot_attach_menu: false,
            premium: false,
            attach_menu_enabled: false,
            bot_can_edit: false,
            close_friend: false,
            stories_hidden: false,
            stories_unavailable: false,
            contact_require_premium: false,
            bot_business: false,
            bot_has_main_app: false,
            bot_forum_view: false,
            bot_forum_can_manage_topics: false,
            bot_can_manage_bots: false,
            bot_guestchat: false,
            bot_guard: false,
            bot_forum_sidebar: false,
            id: 42,
            access_hash: Some(999),
            first_name: Some("Анна".to_owned()),
            last_name: None,
            username: None,
            phone: None,
            photo: None,
            status: None,
            bot_info_version: None,
            restriction_reason: None,
            bot_inline_placeholder: None,
            lang_code: None,
            emoji_status: None,
            usernames: None,
            stories_max_id: None,
            color: None,
            profile_color: None,
            bot_active_users: None,
            bot_verification_icon: None,
            send_paid_messages_stars: None,
            linked_community_id: None,
        });
        let map = build_tl_peer_lookup(&[u], &[]);
        assert_eq!(map.get(&42).map(|(_, n)| n.as_str()), Some("Анна"));
    }

    #[test]
    fn chat_title_validation_and_global_privacy_defaults() {
        assert!(validate_chat_title("   ").is_err());
        assert_eq!(validate_chat_title("  Rustgramm Chat  ").unwrap(), "Rustgramm Chat");
        let long_title = "x".repeat(130);
        assert!(validate_chat_title(&long_title).is_err());
        let gp = GlobalPrivacyInfo::default();
        assert!(!gp.hide_read_marks);
        assert!(!gp.sensitive_can_change);
        let ph = ProfilePhotoItem {
            id: 1,
            access_hash: 2,
            file_reference: vec![3],
            dc_id: 2,
            date: 100,
            cache_path: None,
        };
        assert_eq!(ph.dc_id, 2);
    }
}
