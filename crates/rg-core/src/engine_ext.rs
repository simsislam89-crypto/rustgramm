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
    ) -> Result<(), String> {
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
        self.invoke_retry(
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
        Ok(())
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
