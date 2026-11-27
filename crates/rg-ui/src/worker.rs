//! Фоновый поток: вся работа с сетью и диском, интерфейс получает только события.

use std::path::PathBuf;
use std::sync::mpsc::Sender;

use grammers_client::client::PasswordToken;
use rg_core::auth::{RequestCodeOutcome, SentCodeInfo, SignInOutcome};
use rg_ayu::store::MessageStore;
use rg_core::engine::{
    AudioTrack, ContactRow, DialogKind, DialogRow, DiscussionThread, DraftRow, Engine, FolderRow,
    ForumTopicRow, MessageRow, NotifyScope, PrivacyKeyKind, PrivacyLevel, StickerPackDetails,
    StickerRow, StickerSetRow, StoryRow,
};
use rg_core::{Config, connect_with_updates};
use rg_settings::Settings;
use tokio::sync::mpsc::UnboundedReceiver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stage {
    Starting,
    NeedKeys,
    /// Вход по QR: ссылка для кода (пустая, пока токен не получен).
    Qr(String),
    NeedPhone,
    NeedCode,
    NeedPassword(String),
    Ready,
}

pub enum Cmd {
    Keys(String, String),
    Proxy(String),
    UsePhone,
    UseQr,
    RefreshQr,
    Preview(usize, i32),
    /// Загрузить фото профиля чата с индексом.
    Avatar(usize),
    Download(i32),
    /// Прервать загрузку вложения сообщения.
    CancelFetch(i32),
    /// Какие из этих сообщений уже скачаны в кэш (ответ — `Ev::Cached`).
    CachedMedia(Vec<i32>),
    /// Скачать вложение и открыть программой по умолчанию (безопасные типы) или показать в папке.
    OpenFile(i32),
    RequestCode(String),
    ResendCode,
    Code(String),
    Password(String),
    Open(usize),
    /// Текст, опциональный reply_to и клиентский temp_id (< 0) для мгновенного отображения.
    Send(String, Option<i32>, i32),
    /// Отправить сообщение с пользовательскими Premium-эмодзи `(char_start, char_len, doc_id)`.
    SendRich(String, Vec<(usize, usize, i64)>, Option<i32>, i32),
    /// Отправить сообщение с произвольным форматированием (жирный, курсив, код, спойлер, цитата, Premium-эмодзи).
    SendFormatted(String, Vec<rg_core::engine::Entity>, Option<i32>, i32),
    /// Пакетная пересылка выбранных сообщений в другой чат (`true` — без автора).
    ForwardBatch(Vec<i32>, usize, bool),
    /// Журнал удалённых и изменённых сообщений открытого чата.
    SavedHistory,
    /// Загрузить `.tgs` анимацию стикера сообщения по ID.
    TgsMessage(usize, i32),
    /// Темы супергруппы-форума (`messages.getForumTopics`).
    ForumTopics(usize),
    /// Сообщения выбранной темы форума.
    TopicMessages(usize, i32),
    /// Комментарии к посту канала (`messages.getDiscussionMessage`).
    Discussion(usize, i32),
    /// Отправить комментарий к посту канала в группу обсуждений (`temp_id` < 0).
    SendComment(grammers_session::types::PeerRef, i32, i32, String, i32),
    /// Загрузить активные истории.
    LoadStories,
    /// Отметить историю прочитанной (с проверкой режима призрака).
    ReadStory(grammers_session::types::PeerRef, i32),
    /// Установить автоудаление сообщений в открытом чате (сек., 0 — выкл.).
    SetChatTtl(i32),
    /// Загрузить закреплённое сообщение для шапки чата.
    LoadPinned(usize, i32),
    /// Загрузить текстуры пользовательских Premium-эмодзи по `document_id`.
    CustomEmojiDocs(Vec<i64>),
    /// Создать канал/супергруппу (название, описание, broadcast, @username, участники).
    CreateChannelFull(String, String, bool, Option<String>, Vec<grammers_session::types::PeerRef>),
    /// Пригласить людей в существующий чат/канал с индексом.
    InviteMembers(usize, Vec<grammers_session::types::PeerRef>),
    /// Список участников группы/канала.
    Participants(usize),
    /// Получить ссылку-приглашение группы/канала.
    ExportInvite(usize),
    /// Заблокировать или разблокировать собеседника.
    BlockPeer(usize, bool),
    /// Проголосовать в опросе.
    VotePoll(i32, Vec<u8>),
    /// Поиск любых пользователей Telegram для добавления в канал или группу.
    MemberSearch(String),
    Edit(i32, String),
    /// Отправить файл с подписью (и ответом на сообщение).
    SendFile(PathBuf, String, Option<i32>),
    /// Прежние версии текста сообщения.
    EditHistory(i32),
    /// Удалить сообщение (с резервной копией для гарантированного сохранения при самоудалении).
    Delete {
        id: i32,
        backup: Option<(String, i64, bool, String)>,
    },
    /// Переслать сообщение в чат с таким индексом списка (`true` — без автора, функция AyuGram).
    Forward(i32, usize, bool),
    /// Перевести текст сообщения встроенным переводчиком Telegram.
    Translate(i32, String),
    Pin(i32, bool),
    React(i32, String),
    /// Поиск в открытом чате; пустая строка — выйти из поиска.
    Search(String),
    /// Подгрузить сообщения старше указанного идентификатора.
    Older(i32),
    /// Отметить открытый чат прочитанным принудительно (мимо режима призрака).
    ForceRead,
    /// Отметить ВСЕ непрочитанные чаты прочитанными на сервере (из левого меню / горячей клавиши).
    ForceReadAll,
    /// Выйти из аккаунта: сессия и локальная база стираются, приложение перезапускается.
    LogOut,
    /// Архивировать (true) или вернуть из архива чат с индексом.
    Archive(usize, bool),
    PinDialog(usize, bool),
    MuteDialog(usize, bool),
    /// Прочитать чат с индексом на сервере.
    ReadDialog(usize),
    /// Загрузить список контактов.
    Contacts,
    /// Открыть чат с контактом (добавляется в список, если его там нет).
    OpenContact(grammers_session::types::PeerRef, i64, String),
    /// Фото профиля для чата, которого нет в списке (контакты).
    AvatarPeer(grammers_session::types::PeerRef, i64),
    /// Загрузить наборы стикеров.
    Stickers,
    /// Обновить список чатов (счётчики, последние сообщения).
    RefreshDialogs,
    /// Сведения о чате с номером для правой панели.
    PeerInfo(usize),
    /// Загрузить сохранённые GIF.
    Gifs,
    /// Обзор аккаунта для «Настроек» Telegram.
    Account,
    /// Сохранить имя, фамилию и «О себе».
    UpdateProfile(String, String, String),
    /// Изменить `@username` аккаунта.
    UpdateUsername(String),
    /// Изменить правило приватности (`account.setPrivacy`).
    SetPrivacy(PrivacyKeyKind, PrivacyLevel),
    /// Изменить серверные уведомления для типа чатов (`account.updateNotifySettings`).
    SetNotifyScope(NotifyScope, bool, bool),
    /// Уведомления о присоединении контактов.
    SetContactSignupNotify(bool),
    /// Таймер автоудаления новых сообщений (секунды, 0 — выкл.).
    SetDefaultHistoryTtl(i32),
    /// Срок удаления аккаунта при неактивности (дни).
    SetAccountTtlDays(i32),
    /// Завершить один сеанс по `hash`.
    ResetAuthorization(i64),
    /// Завершить все остальные сеансы.
    ResetOtherAuthorizations,
    /// Срок автозавершения неактивных сеансов (дни).
    SetAuthorizationTtl(i32),
    /// Разблокировать пользователя из списка заблокированных.
    UnblockUser(i64, i64),
    /// Очистить сохранённые платёжные данные.
    ClearPaymentInfo,
    /// Скачать и воспроизвести музыкальный трек из профиля.
    PlayTrack(AudioTrack),
    /// Перевести массив сообщений (для режима «Переводить чаты целиком»).
    TranslateBatch(Vec<(i32, String)>, String),
    /// Создать канал (название, описание, true — канал, false — супергруппа).
    CreateChannel(String, String, bool),
    /// Создать группу с участниками.
    CreateGroup(String, Vec<grammers_session::types::PeerRef>),
    /// Аватарка отправителя сообщения (идентификатор сообщения, идентификатор отправителя).
    SenderAvatar(i32, i64),
    /// Отметить открытый чат прочитанным (по правилам режима призрака).
    MarkRead,
    /// Загрузить стикеры набора с номером.
    StickerSet(usize),
    /// Превью стикера по идентификатору документа.
    StickerThumb(i64),
    /// Отправить стикер (идентификатор документа) в открытый чат, при ответе — на сообщение.
    SendSticker(i64, Option<i32>),
    /// Установленные наборы пользовательских эмодзи.
    EmojiSets,
    /// Рекомендуемые наборы эмодзи (для добавления).
    FeaturedEmoji,
    /// Добавить набор эмодзи себе.
    InstallSet(StickerSetRow),
    /// Содержимое набора (эмодзи или стикеры), заданного строкой.
    LoadSetRow(StickerSetRow),
    /// Миниатюра значка набора (приходит как `StickerImg` с идентификатором набора).
    SetIcon(StickerSetRow),
    /// Фоновая подгрузка короткого GIF в кэш для автозапуска в ленте.
    AutoFetchGif(i32),
    /// Ключевые слова для поиска эмодзи из Telegram.
    EmojiKeywords,
    /// Создать или обновить папку чатов.
    SaveFolder(FolderRow),
    /// Удалить папку чатов.
    DeleteFolder(i32),
    /// Глобальный поиск пользователей, групп и каналов.
    GlobalSearch(String),
    /// Нажать callback-кнопку бота (`(индекс_чата, msg_id, data)`).
    BotCallback(usize, i32, Vec<u8>),
    /// Запустить Mini App / WebApp бота (`messages.requestWebView` / `messages.requestSimpleWebView`).
    OpenMiniApp {
        idx: usize,
        msg_id: Option<i32>,
        title: String,
        url: Option<String>,
        start_param: Option<String>,
        simple: bool,
    },
    /// Открыть внутреннюю ссылку Telegram (`tg://`, `t.me/...`, `@username`) внутри клиента.
    OpenTgLink(rg_core::engine::TgLink),
    /// Тотальная очистка данных у других людей и на сервере без потери аккаунта.
    WipeExternalData(rg_core::engine::WipeScope),
    /// Переключиться на изолированный слот аккаунта (с разрывом старого соединения и сменой отпечатка).
    SwitchAccount(u32),
    /// Добавить новый изолированный аккаунт с уникальным отпечатком устройства и опциональным SOCKS5-прокси.
    AddIsolatedAccount {
        label: String,
        proxy_url: String,
    },
    /// Быстрое добавление аккаунта как в Telegram/AyuGram: сразу создаёт изолированный слот и открывает вход.
    AddAccountAuto,
    /// Окончательно удалить уже удалённое сообщение только у себя из локальной БД/кэша.
    PurgeLocal(i32),
    /// Окончательно убрать все сохранённые удалённые сообщения открытого чата только у себя.
    PurgeAllDeleted,
    /// Удалить дополнительный изолированный слот аккаунта (`slot > 0`).
    RemoveAccountSlot(u32),
    /// Вернуться с ввода кода/пароля обратно к вводу номера телефона.
    BackToPhone,
    /// Опубликовать свою историю (фото или видео).
    PublishStory {
        path: PathBuf,
        caption: String,
        privacy: rg_core::engine::StoryPrivacy,
        period_secs: i32,
        pinned: bool,
        noforwards: bool,
    },
    /// Поставить реакцию на историю.
    ReactStory(grammers_session::types::PeerRef, i32, String),
    /// Загрузить медиа истории для просмотрщика.
    LoadStoryMedia(grammers_session::types::PeerRef, i32),
    /// Отправить записанное голосовое сообщение.
    SendVoice {
        path: PathBuf,
        duration: u32,
        waveform: Vec<u8>,
        reply_to: Option<i32>,
    },
    /// Назначить или снять администратора группы/канала.
    AdminEditAdmin {
        peer: grammers_session::types::PeerRef,
        user: grammers_session::types::PeerRef,
        rights: rg_core::engine::AdminRights,
        rank: String,
    },
    /// Забанить, замутить или разбанить участника.
    AdminEditBanned {
        peer: grammers_session::types::PeerRef,
        user: grammers_session::types::PeerRef,
        rights: rg_core::engine::BannedRights,
    },
    /// Изменить общие права участников группы по умолчанию.
    AdminEditDefaultRights {
        peer: grammers_session::types::PeerRef,
        rights: rg_core::engine::BannedRights,
    },
    /// Загрузить журнал действий администраторов.
    AdminLoadLog {
        peer: grammers_session::types::PeerRef,
        query: String,
    },
    /// Загрузить пригласительные ссылки группы/канала.
    AdminLoadInvites(grammers_session::types::PeerRef),
    /// Создать пригласительную ссылку.
    AdminCreateInvite {
        peer: grammers_session::types::PeerRef,
        title: String,
        expire_date: Option<i32>,
        usage_limit: Option<i32>,
        request_needed: bool,
    },
    /// Отозвать пригласительную ссылку.
    AdminRevokeInvite {
        peer: grammers_session::types::PeerRef,
        link: String,
    },
    /// Загрузить локальный список секретных чатов E2E.
    LoadSecretChats,
    /// Создать новый секретный чат E2E с пользователем.
    StartSecretChat(grammers_session::types::PeerRef, String),
    /// Открыть секретный чат и загрузить его расшифрованную историю.
    OpenSecretChat(i32),
    /// Отправить E2E-сообщение в секретный чат.
    SendSecretText(i32, String),
    /// Задать таймер самоуничтожения в секретном чате.
    SetSecretTtl(i32, i32),
    /// Завершить и удалить секретный чат.
    DiscardSecretChat(i32),
    /// Начать голосовой или видеозвонок.
    StartCall {
        peer: grammers_session::types::PeerRef,
        name: String,
        video: bool,
    },
    /// Принять входящий звонок.
    AcceptCall {
        call_id: i64,
        access_hash: i64,
        video: bool,
    },
    /// Подтвердить исходящий звонок после ответа собеседника.
    ConfirmCall {
        call_id: i64,
        access_hash: i64,
        g_b: Vec<u8>,
    },
    /// Вычислить 4 E2E-эмодзи для принятого входящего звонка.
    DeriveIncomingCallEmojis {
        call_id: i64,
        g_a: Vec<u8>,
    },
    /// Завершить или отклонить звонок.
    DiscardCall {
        call_id: i64,
        access_hash: i64,
        duration: i32,
        video: bool,
        busy: bool,
    },
    /// Проверить наличие новой версии Rustgramm на GitHub Releases.
    CheckUpdates,
    /// Скачать и применить обновление.
    ApplyUpdate(crate::updater::UpdateInfo),
    /// Отправить фото/видео/файл с параметрами встроенного редактора (сжатие, спойлер, таймер).
    SendMediaEdited {
        path: PathBuf,
        caption: String,
        reply_to: Option<i32>,
        opts: rg_core::engine::SendMediaOptions,
        cleanup_temp: bool,
    },
    /// Постраничная догрузка участников большой группы/канала (до 1000+).
    ParticipantsPaged(usize, i32),
    /// Запустить нативный UDP/WebRTC медиа-поток для принятого входящего звонка.
    StartIncomingCallMedia {
        call_id: i64,
        g_a: Vec<u8>,
        endpoints: Vec<rg_core::calls::CallEndpoint>,
        p2p_allowed: bool,
        video: bool,
    },
    /// Включить/выключить микрофон в активном медиа-потоке звонка.
    SetCallMuted(bool),
    /// Обновить статистику RTT/пакетов активного медиа-потока звонка.
    PollCallStats,
    /// Отправить WebRTC ICE/SDP сигнальные данные в рамках звонка.
    SendCallSignaling {
        call_id: i64,
        access_hash: i64,
        data: Vec<u8>,
    },
    /// Запросить баланс и историю транзакций Telegram Stars.
    LoadStarsStatus,
    /// Загрузить витрину подарков Telegram Stars.
    LoadStarGiftsCatalog,
    /// Загрузить подарки в профиле (`None` — свой профиль, `Some(idx)` — диалог).
    LoadSavedGifts(Option<usize>),
    /// Скрыть или показать подарок в своём профиле.
    ToggleSaveStarGift {
        msg_id: Option<i32>,
        saved_id: Option<i64>,
        unsave: bool,
    },
    /// Конвертировать подарок в Telegram Stars.
    ConvertStarGift {
        msg_id: Option<i32>,
        saved_id: Option<i64>,
    },
    /// Отправить платную звёздную реакцию на сообщение.
    SendPaidReaction {
        msg_id: i32,
        count: i32,
        anonymous: bool,
    },
    /// Обновить бизнес-часы работы аккаунта.
    UpdateBusinessHours {
        enabled: bool,
        timezone: String,
        weekly: Vec<(i32, i32)>,
    },
    /// Загрузить быстрые ответы Telegram Бизнес.
    LoadQuickReplies,
    /// Отправить быстрый ответ в открытый чат.
    SendQuickReply {
        shortcut_id: i32,
        msg_ids: Vec<i32>,
    },
    /// Изменить короткое имя быстрого ответа.
    EditQuickReplyShortcut {
        shortcut_id: i32,
        shortcut: String,
    },
    /// Удалить ярлык быстрого ответа.
    DeleteQuickReplyShortcut(i32),
    /// Полное завершение работы (остановка медиа-потока звонка, аудио и сетевого пула).
    Shutdown,
    /// Повторить подключение к Telegram (при обрыве на экране входа / после смены прокси).
    RetryConnect,
    /// Лениво загрузить архивные чаты (только при открытии раздела «Архив»).
    LoadArchive,
    /// Расшифровать голосовое / кружок (Telegram API + локальный оффлайн-фоллбэк без лимитов).
    TranscribeVoice(i32),
    /// Расширенная отправка сообщения (отложка, эффект, от имени канала, без звука).
    SendAdvanced {
        text: String,
        entities: Vec<rg_core::engine::Entity>,
        reply_to: Option<i32>,
        schedule_date: Option<i32>,
        effect: Option<i64>,
        send_as: Option<grammers_session::types::PeerRef>,
        silent: bool,
    },
    /// Загрузить отложенные сообщения / напоминания открытого чата.
    LoadScheduled,
    /// Отправить выбранные отложенные сообщения прямо сейчас.
    SendScheduledNow(Vec<i32>),
    /// Удалить выбранные отложенные сообщения.
    DeleteScheduled(Vec<i32>),
    /// Отправить интерактивный чек-лист (To-Do) в открытый чат.
    SendTodo {
        title: String,
        items: Vec<String>,
        others_can_complete: bool,
        others_can_append: bool,
    },
    /// Переключить галочку пункта чек-листа в сообщении.
    ToggleTodo(i32, i32, bool),
    /// Отправить опрос или викторину в открытый чат.
    SendPoll {
        question: String,
        options: Vec<String>,
        anonymous: bool,
        multiple_choice: bool,
        quiz: bool,
        correct_idx: Option<usize>,
        explanation: String,
    },
    /// Загрузить теги «Избранного» (Saved Messages Tags).
    LoadSavedTags,
    /// Обновить подпись тега «Избранного».
    UpdateSavedTag(String, String),
    /// Загрузить список профилей/каналов для «Отправить от имени».
    LoadSendAs(usize),
    /// Глобальный поиск по хэштегу в публичных каналах.
    SearchHashtag(String),
    /// Изменить акцентный цвет профиля или ответов.
    UpdateProfileColor(bool, Option<i32>),
    /// Загрузить доступные анимированные эффекты сообщений.
    LoadEffects,
    /// Изменить медленный режим (Slow Mode) в супергруппе.
    SetSlowMode(grammers_session::types::PeerRef, i32),
    /// Очистить сессию после завершения работы (если снята галочка «Оставаться в системе»).
    SetKeepSignedIn(bool),
    /// Перевести черновик перед отправкой (`translateBeforeSend`) и при `send_after` сразу отправить.
    TranslateDraft {
        text: String,
        to_lang: String,
        send_after: bool,
        reply_to: Option<i32>,
    },
    /// Запрос результатов инлайн-бота (`@bot запрос`).
    QueryInlineBot {
        bot: String,
        query: String,
        offset: String,
    },
    /// Отправить выбранный результат инлайн-бота в открытый чат.
    SendInlineResult {
        query_id: i64,
        id: String,
        reply_to: Option<i32>,
    },
    /// Загрузить список доступных реакций Telegram.
    LoadAvailableReactions,
    /// Установить быструю реакцию по умолчанию на сервере Telegram.
    SetDefaultReaction(String),
    /// Загрузить стандартные эмодзи-статусы аккаунта.
    LoadDefaultEmojiStatuses,
    /// Установить или сбросить эмодзи-статус текущего аккаунта.
    SetEmojiStatus {
        document_id: Option<i64>,
        until: Option<i32>,
    },
    /// Переключить (поставить или снять) реакцию на сообщении.
    ReactToggle(i32, String, bool),
    /// Подсмотреть последние 20 сообщений чата без пометки прочитанным.
    PeekHistory(usize),
    /// Поиск/фильтрация участников канала или группы (`Все` / `Админы` / `Боты`).
    ParticipantsFiltered(usize, rg_core::engine::ParticipantFilter, String),
    /// Отправить или отменить статус «печатает…» (с проверкой режима призрака).
    SetTyping(usize, bool),
    /// Отправить карточку контакта в открытый чат.
    SendContact {
        phone: String,
        first_name: String,
        last_name: String,
        reply_to: Option<i32>,
    },
    /// Отправить геопозицию в открытый чат.
    SendGeo {
        lat: f64,
        lon: f64,
        reply_to: Option<i32>,
    },
    /// Загрузить списки прочитавших и реакций для окна «Сведения о сообщении».
    LoadMessageActivity(i32),
    /// Скопировать фото сообщения в системный буфер обмена (на ПК).
    CopyPhoto(i32),
    /// Мягкая дозагрузка открытого чата и списка диалогов при возвращении фокуса.
    SyncOpenChat,
    /// Сохранить облачный черновик (`messages.saveDraft`) для диалога с индексом.
    SaveDraft(usize, String, Option<i32>),
    /// Отметить диалог прочитанным или непрочитанным на сервере (`messages.markDialogUnread`).
    MarkDialogUnread(usize, bool),
    /// Перейти к следующему непрочитанному `@` упоминанию в чате с индексом.
    JumpUnreadMention(usize),
    /// Отметить все `@` упоминания в чате прочитанными.
    ReadAllMentions(usize),
    /// Перейти к следующей непрочитанной `❤` реакции в чате с индексом.
    JumpUnreadReaction(usize),
    /// Отметить все `❤` реакции в чате прочитанными.
    ReadAllReactions(usize),
    /// Отправить анимированный бросок/игру (`🎲`, `🎯`, `🏀`, `⚽`, `🎰`, `🎳`).
    SendDice(String, Option<i32>, i32),
    /// Загрузить подробности набора стикеров для окна просмотра и установки.
    LoadStickerPack(i64, i64),
    /// Установить или удалить набор стикеров одним кликом (`install = true/false`).
    InstallStickerPack(i64, i64, bool),
    /// Обновить дату рождения профиля (`account.updateBirthday`).
    UpdateBirthday(Option<(i32, i32, Option<i32>)>),
    /// Обновить публичный `@username` с проверкой (`account.updateUsername`).
    UpdateUsernameChecked(String),
    /// Загрузить похожие каналы (`channels.getChannelRecommendations`).
    SimilarChannels(usize),
    /// Поиск по глобальному журналу удалённых сообщений во всех чатах.
    GlobalDeletedLog(String),
    /// Экспорт журнала удалённых и правок открытого чата в файл (TXT / JSON).
    ExportChatAudit {
        chat_title: String,
        as_json: bool,
    },
    /// Загрузить историю чата на указанную дату (unix timestamp).
    JumpToDate(usize, i32),
    /// Загрузить общие группы с пользователем (`messages.getCommonChats`).
    CommonChats(usize),
    /// Добавить или обновить пользователя в контактах (`contacts.addContact`).
    AddContact {
        idx: usize,
        first_name: String,
        last_name: String,
        phone: String,
    },
    /// Удалить пользователя из контактов (`contacts.deleteContacts`).
    DeleteContact(usize),
    /// 1. Глобальный поиск по тексту сообщений во всех чатах (`messages.searchGlobal`).
    SearchGlobalMessages(String, u8),
    /// 2а. Очистить историю чата (`dialog_id`, `revoke`).
    ClearHistory(i64, bool),
    /// 2б. Покинуть группу или канал (`dialog_id`).
    LeaveChat(i64),
    /// 3а. Загрузить все закреплённые сообщения чата (`dialog_id`).
    LoadPinnedMessages(i64),
    /// 3б. Открепить все закреплённые сообщения чата (`dialog_id`).
    UnpinAllMessages(i64),
    /// 4. Расширенная пересылка AyuGram (`ids`, `to_idx`, без автора, без подписей к медиа, без звука).
    ForwardAdvanced {
        ids: Vec<i32>,
        to_idx: usize,
        drop_author: bool,
        drop_media_captions: bool,
        silent: bool,
    },
    /// 5а. Экспортировать пригласительную ссылку на папку чатов.
    ExportFolderInvite {
        folder_id: i32,
        title: String,
        peers: Vec<grammers_session::types::PeerRef>,
    },
    /// 5б. Проверить пригласительную ссылку на папку чатов (`t.me/addlist/...`).
    CheckFolderInvite(String),
    /// 5в. Присоединиться к папке чатов по ссылке.
    JoinFolderInvite(String, Vec<grammers_session::types::PeerRef>),
    Settings(Box<Settings>),
}

pub enum Ev {
    Image(usize, i32, Box<eframe::egui::ColorImage>),
    /// Фото профиля: идентификатор чата (Bot API) и картинка.
    Avatar(i64, Box<eframe::egui::ColorImage>),
    Stage(Stage),
    Me(String),
    Dialogs(Vec<DialogRow>),
    /// История чата по стабильному Bot API `dialog_id` (не ломается при перестановке списка чатов).
    History(i64, Vec<MessageRow>),
    /// Ошибка загрузки истории (снимает бесконечный спиннер).
    HistoryError(i64, String),
    /// Более старые сообщения (добавляются в начало); пустой список — история закончилась.
    Older(usize, Vec<MessageRow>),
    /// Результат поиска по чату.
    Found(usize, Vec<MessageRow>),
    EditHistory(i32, Vec<String>),
    Settings(Box<Settings>),
    Folders(Vec<FolderRow>),
    Contacts(Vec<ContactRow>),
    /// Выбрать чат по идентификатору.
    Select(i64),
    StickerSets(Vec<StickerSetRow>),
    FavedStickers(Vec<StickerRow>),
    RecentStickers(Vec<StickerRow>),
    StickersLoaded,
    StickerList(i64, Vec<StickerRow>),
    StickerImg(i64, Box<eframe::egui::ColorImage>),
    EmojiSets(Vec<StickerSetRow>),
    FeaturedEmoji(Vec<StickerSetRow>),
    /// Событие из потока обновлений Telegram.
    Live(rg_core::engine::Live),
    Gifs(Vec<StickerRow>),
    GifsLoaded,
    Account(Box<rg_core::engine::AccountOverview>),
    /// Сведения о чате (идентификатор чата Bot API).
    PeerInfo(i64, Box<rg_core::engine::PeerInfo>),
    /// Ролик скачан: открыть во встроенном плеере.
    PlayVideo(PathBuf),
    /// Потоковое воспроизведение ролика по локальному адресу до окончания загрузки (адрес, подпись).
    StreamVideo(String, String),
    /// Аудио/музыка/голосовое готово к воспроизведению во встроенном мини-плеере (путь, название, длительность).
    PlayAudio(PathBuf, String, u32),
    /// Потоковое воспроизведение музыки по локальному HTTP Range адресу после первых 128 КБ.
    StreamAudio(String, String, u32),
    /// Короткий GIF скачан в кэш и готов к автозапуску в ленте.
    GifReady(i32, PathBuf),
    /// Ключевые слова для поиска эмодзи из Telegram.
    EmojiKeywords(Vec<(String, Vec<String>)>),
    /// Результаты глобального поиска.
    GlobalFound(Vec<ContactRow>),
    /// Переведённый текст сообщения (идентификатор сообщения, перевод).
    Translated(i32, String),
    /// Участники группы/канала (идентификатор чата, список).
    Participants(i64, Vec<ContactRow>),
    /// Ссылка-приглашение группы/канала.
    InviteLink(i64, String),
    /// Найденные пользователи для добавления в канал/группу.
    MemberSearch(Vec<ContactRow>),
    /// Ход загрузки вложения: сообщение, скачано, всего (байт).
    Progress(i32, u64, u64),
    /// Загрузка вложения закончилась (успешно, отменена или с ошибкой).
    FetchEnd(i32),
    /// Сообщения, вложения которых уже лежат в кэше.
    Cached(Vec<i32>),
    /// Журнал сохранённых удалённых и изменённых сообщений чата.
    SavedHistory(i64, Vec<rg_ayu::store::SavedAudit>),
    /// Загруженные `.tgs` (gzipped Lottie) байты для стикера/CustomEmoji по `document_id`.
    TgsDoc(i64, Vec<u8>),
    /// Загруженные `.tgs` байты анимированного стикера в ленте по `msg_id`.
    TgsMsg(i32, Vec<u8>),
    /// Темы форума супергруппы.
    ForumTopics(i64, Vec<ForumTopicRow>),
    /// Сообщения выбранной темы форума.
    TopicMessages(i64, i32, Vec<MessageRow>),
    /// Загруженная ветка комментариев к посту канала.
    Discussion(DiscussionThread),
    /// Активные истории.
    Stories(Vec<StoryRow>),
    /// Закреплённое сообщение чата.
    PinnedMsg(i64, Box<MessageRow>),
    /// Готовый URL для открытия встроенного окна Mini App / WebApp бота.
    MiniAppReady {
        title: String,
        url: String,
    },
    /// Всплывающее окно-уведомление от callback-кнопки бота (`alert = true`).
    BotAlert(String),
    /// Переход к чату и конкретному сообщению по внутренней ссылке.
    NavigateTo {
        id: i64,
        post: Option<i32>,
    },
    /// Локально скрыто уже удалённое сообщение (удалено только у себя).
    LocalPurged(i64, i32),
    /// Локально скрыты все удалённые сообщения чата.
    LocalPurgedAll(i64),
    /// Бесшовное переключение слота аккаунта без перезапуска процесса (для Android и ПК).
    ResetSession,
    /// Индикатор сетевой операции на экране входа (отправка/проверка кода).
    LoginBusy(bool),
    /// Подсказка о том, куда отправлен код подтверждения («SMS», «приложение Telegram» и т.п.).
    CodeDeliveryHint(String),
    /// Ожидаемая длина кода и таймаут повторной отправки (в секундах).
    CodeMeta { length: u8, resend_secs: Option<i32> },
    /// Картинка истории для просмотрщика (`story_id`).
    StoryImage(i32, Box<eframe::egui::ColorImage>),
    /// Ошибка загрузки медиа истории (`story_id`, причина).
    StoryError(i32, String),
    /// Расшифровка голосового / видеосообщения (`msg_id`, текст).
    Transcribed(i32, String),
    /// Отложенные сообщения / напоминания для чата `dialog_id`.
    ScheduledHistory(i64, Vec<MessageRow>),
    /// Теги «Избранного».
    SavedTags(Vec<rg_core::engine::SavedTagItem>),
    /// Список доступных отправителей «От имени» для чата `dialog_id`.
    SendAsList(i64, Vec<rg_core::engine::SendAsItem>),
    /// Глобальные результаты поиска по хэштегу `(tag, messages)`.
    HashtagResults(String, Vec<MessageRow>),
    /// Доступные эффекты сообщений.
    AvailableEffects(Vec<rg_core::engine::MessageEffectItem>),
    /// Записи журнала действий группы/канала.
    AdminLog(Vec<rg_core::engine::AdminLogRow>),
    /// Пригласительные ссылки группы/канала.
    AdminInvites(Vec<rg_core::engine::InviteLinkRow>),
    /// Список локальных секретных чатов E2E.
    SecretChats(Vec<rg_ayu::store::SecretChatRecord>),
    /// Сообщения выбранного секретного чата.
    SecretMessages(i32, Vec<rg_ayu::store::SecretMessageRecord>),
    /// Исходящий звонок инициирован на сервере Telegram.
    CallStarted {
        call_id: i64,
        access_hash: i64,
        peer_name: String,
        video: bool,
    },
    /// 4 защитных эмодзи E2E для активного звонка.
    CallEmojis([&'static str; 4]),
    /// Статистика нативного UDP/WebRTC медиа-потока звонка.
    CallMediaStats(rg_core::call_media::CallMediaStats),
    /// Потоковая догрузка чатов (до 5000+) без торможения интерфейса.
    DialogsBatch(Vec<DialogRow>, bool),
    /// Страница участников группы/канала `(chat_id, rows, offset, total)`.
    ParticipantsPage(i64, Vec<ContactRow>, i32, i32),
    /// Баланс и транзакции Telegram Stars.
    StarsStatus(rg_core::engine::StarsSummary),
    /// Предпросмотр чата без отметки о прочтении (`dialog_id`, заголовок, последние сообщения).
    PeekHistory(i64, String, Vec<MessageRow>),
    /// Витрина подарков Telegram Stars.
    StarGiftsCatalog(Vec<rg_core::engine::StarGiftItem>),
    /// Подарки в профиле `(peer_id, gifts)`.
    SavedGifts(i64, Vec<rg_core::engine::SavedGiftItem>),
    /// Быстрые ответы Telegram Бизнес.
    QuickReplies(Vec<rg_core::engine::QuickReplyItem>),
    /// Найдена новая версия Rustgramm на GitHub Releases.
    UpdateAvailable(Box<crate::updater::UpdateInfo>),
    /// Прогресс скачивания обновления `(скачано, всего)`.
    UpdateProgress(u64, u64),
    /// Обновление скачано и готово к перезапуску / открытию.
    UpdateReady(String),
    /// Переведённый текст черновика перед отправкой.
    DraftTranslated(String),
    /// Результаты инлайн-бота (`@bot запрос`).
    InlineBotResults {
        bot: String,
        query: String,
        append: bool,
        page: rg_core::engine::InlineBotResultsPage,
    },
    InlineBotError(String),
    /// Доступные реакции Telegram.
    AvailableReactions(Vec<rg_core::engine::AvailableReactionRow>),
    /// Идентификаторы документов стандартных эмодзи-статусов.
    DefaultEmojiStatuses(Vec<i64>),
    /// Списки прочитавших и реакций для окна «Сведения о сообщении».
    MessageActivity {
        msg_id: i32,
        readers: Vec<rg_core::engine::ReadParticipantRow>,
        reactions: Vec<rg_core::engine::MessageReactionPeerRow>,
    },
    /// Подтверждение оптимистично отправленного сообщения (`dialog_id`, `temp_id`, реальное сообщение).
    SentConfirm(i64, i32, Box<MessageRow>),
    /// Сбой отправки оптимистичного сообщения (`dialog_id`, `temp_id`, текст ошибки).
    SendFailed(i64, i32, String),
    /// Подтверждение комментария в обсуждении (`post_id`, `temp_id`, реальный комментарий).
    CommentConfirmed(i32, i32, Box<MessageRow>),
    /// Сбой отправки комментария (`post_id`, `temp_id`, текст ошибки).
    CommentFailed(i32, i32, String),
    /// Загруженные с сервера облачные черновики.
    Drafts(Vec<DraftRow>),
    /// Перейти к конкретному сообщению открытого чата (`dialog_id`, `msg_id`).
    JumpToMessage(i64, i32),
    /// Загруженный набор стикеров для модального окна просмотра.
    StickerPack(Box<StickerPackDetails>),
    /// Набор стикеров установлен/удалён (`set_id`, `installed`).
    StickerPackInstalled(i64, bool),
    /// Обновлён публичный `@username`.
    UsernameUpdated(String),
    /// Обновлена дата рождения аккаунта.
    BirthdayUpdated(Option<(i32, i32, Option<i32>)>),
    /// Похожие каналы (`channel_id`, список).
    SimilarChannels(i64, Vec<rg_core::engine::SimilarChannelRow>),
    /// Результаты глобального поиска по удалённым сообщениям.
    GlobalDeletedLog {
        query: String,
        items: Vec<rg_ayu::store::Saved>,
        stats: (usize, usize, usize),
    },
    /// История чата на выбранную дату (`dialog_id`, сообщения, `target_unix`).
    DateJumpHistory(i64, Vec<MessageRow>, i32),
    /// Общие группы с пользователем (`user_id`, список).
    CommonChats(i64, Vec<ContactRow>),
    /// Обновление статуса контакта (`user_id`, в контактах, отображаемое имя).
    ContactUpdated(i64, bool, String),
    /// 1. Результаты глобального поиска сообщений во всех чатах.
    GlobalMessages(String, Vec<rg_core::engine::GlobalMessageHit>),
    /// 2а. История чата очищена (`dialog_id`).
    HistoryCleared(i64),
    /// 2б. Пользователь покинул группу/канал (`dialog_id`).
    LeftChat(i64),
    /// 3а. Список всех закреплённых сообщений чата.
    PinnedMessagesList(i64, Vec<MessageRow>),
    /// 3б. Все сообщения в чате откреплены (`dialog_id`).
    AllUnpinned(i64),
    /// 5а. Готовая пригласительная ссылка на папку чатов (`t.me/addlist/...`).
    FolderInviteUrl(String),
    /// 5б. Предпросмотр папки по приглашению `t.me/addlist/...`.
    FolderInvitePreview(rg_core::engine::FolderInvitePreview),
    Status(String),
    Error(String),
}

/// Каталог данных на Android задаёт оболочка до запуска интерфейса.
pub static DATA_DIR: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();

/// Ключи, вшитые при сборке (`RG_BUILTIN_API_ID` и `RG_BUILTIN_API_HASH`),
/// или стандартные открытые ключи Telegram Desktop (`tdesktop`) как запасной вариант,
/// чтобы вход с телефона и ПК сразу открывал ввод номера / QR без запроса API.
fn builtin_keys() -> Option<Config> {
    let id = option_env!("RG_BUILTIN_API_ID").filter(|s| !s.trim().is_empty()).unwrap_or("2040");
    let hash = option_env!("RG_BUILTIN_API_HASH")
        .filter(|s| !s.trim().is_empty())
        .unwrap_or("b18441a1ff607e10a989891a5462e627");
    Config::from_lookup(|k| match k {
        "RG_API_ID" => Some(id.to_owned()),
        "RG_API_HASH" => Some(hash.to_owned()),
        "RG_PROXY" => std::env::var("RG_PROXY").ok(),
        _ => None,
    })
    .ok()
}

pub(crate) fn home() -> PathBuf {
    #[cfg(test)]
    {
        let dir = std::env::var("RG_HOME")
            .map(PathBuf::from)
            .ok()
            .or_else(|| DATA_DIR.get().cloned())
            .or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Rustgramm")))
            .unwrap_or_else(|| PathBuf::from(".rg"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
    #[cfg(not(test))]
    {
        static CACHED_HOME: std::sync::OnceLock<PathBuf> = std::sync::OnceLock::new();
        CACHED_HOME
            .get_or_init(|| {
                let dir = std::env::var("RG_HOME")
                    .map(PathBuf::from)
                    .ok()
                    .or_else(|| DATA_DIR.get().cloned())
                    .or_else(|| std::env::var_os("APPDATA").map(|a| PathBuf::from(a).join("Rustgramm")))
                    .unwrap_or_else(|| PathBuf::from(".rg"));
                let _ = std::fs::create_dir_all(&dir);
                dir
            })
            .clone()
    }
}

/// Изолированный каталог для слота аккаунта (`0` — корень для обратной совместимости, `1..N` — `accounts/slot_N`).
pub(crate) fn account_home(root: &std::path::Path, slot: u32) -> PathBuf {
    if slot == 0 {
        root.to_path_buf()
    } else {
        let dir = root.join("accounts").join(format!("slot_{slot}"));
        let _ = std::fs::create_dir_all(&dir);
        dir
    }
}

fn keys_from_file(home: &std::path::Path) -> Option<Config> {
    let text = std::fs::read_to_string(home.join("api.keys")).ok()?;
    let mut lines = text.lines();
    let api_id: i32 = lines.next()?.trim().parse().ok()?;
    let api_hash = lines.next()?.trim().to_owned();
    Config::from_lookup(|k| match k {
        "RG_API_ID" => Some(api_id.to_string()),
        "RG_API_HASH" => Some(api_hash.clone()),
        "RG_PROXY" => std::env::var("RG_PROXY").ok(),
        _ => None,
    })
    .ok()
}

/// После выхода из аккаунта удаляет сессию и кэш предыдущего входа, но СОХРАНЯЕТ `messages.db`,
/// чтобы архив удалённых сообщений и правок переживал перезаход в аккаунт.
fn wipe_if_logged_out(home: &std::path::Path) {
    let flag = home.join("logout.flag");
    if !flag.exists() {
        return;
    }
    for name in [
        "main.session",
        "main.session-shm",
        "main.session-wal",
        "dialogs_cache.json",
    ] {
        let _ = std::fs::remove_file(home.join(name));
    }
    let _ = std::fs::remove_dir_all(home.join("cache"));
    let _ = std::fs::remove_file(flag);
}

/// Лёгкий дисковый кэш списка чатов и папок для мгновенного показа при старте (< 30 мс).
#[derive(serde::Serialize, serde::Deserialize, Default)]
struct DiskCache {
    me: String,
    dialogs: Vec<CachedDialog>,
    folders: Vec<CachedFolder>,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedDialog {
    id: i64,
    name: String,
    last: String,
    date: i64,
    unread: i32,
    pinned: bool,
    muted: bool,
    #[serde(default)]
    notify_explicit: Option<bool>,
    saved: bool,
    archived: bool,
    kind: u8,
    contact: bool,
    read_out: i32,
    #[serde(default)]
    emoji_status: Option<i64>,
    #[serde(default)]
    verified: bool,
    #[serde(default)]
    premium: bool,
}

#[derive(serde::Serialize, serde::Deserialize)]
struct CachedFolder {
    id: i32,
    title: String,
    contacts: bool,
    non_contacts: bool,
    groups: bool,
    broadcasts: bool,
    bots: bool,
    exclude_muted: bool,
    exclude_read: bool,
    exclude_archived: bool,
    include: Vec<i64>,
    exclude: Vec<i64>,
}

fn kind_to_u8(k: DialogKind) -> u8 {
    match k {
        DialogKind::User => 0,
        DialogKind::Bot => 1,
        DialogKind::Group => 2,
        DialogKind::Channel => 3,
    }
}

fn u8_to_kind(v: u8) -> DialogKind {
    match v {
        1 => DialogKind::Bot,
        2 => DialogKind::Group,
        3 => DialogKind::Channel,
        _ => DialogKind::User,
    }
}

async fn fetch_and_push_dialogs(
    eng: &Engine,
    tx: &tokio::sync::mpsc::Sender<Vec<DialogRow>>,
    flag: &std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    if let Ok(list) = eng.dialogs(200).await {
        flag.store(false, std::sync::atomic::Ordering::Release);
        let _ = tx.send(list).await;
    }
}

fn spawn_save_disk_cache(
    home: &std::path::Path,
    me: &str,
    dialogs: &[DialogRow],
    folders: &[FolderRow],
) {
    let h = home.to_path_buf();
    let me_c = me.to_owned();
    let d_c = dialogs.to_vec();
    let f_c = folders.to_vec();
    tokio::task::spawn_blocking(move || save_disk_cache(&h, &me_c, &d_c, &f_c));
}

fn spawn_refresh_dialogs(
    engine: &Engine,
    tx: &tokio::sync::mpsc::Sender<Vec<DialogRow>>,
    in_flight: &std::sync::Arc<std::sync::atomic::AtomicBool>,
    last_at: &mut std::time::Instant,
    force: bool,
) {
    if !force && last_at.elapsed() < std::time::Duration::from_secs(4) {
        return;
    }
    if in_flight
        .compare_exchange(
            false,
            true,
            std::sync::atomic::Ordering::AcqRel,
            std::sync::atomic::Ordering::Acquire,
        )
        .is_err()
    {
        return;
    }
    *last_at = std::time::Instant::now();
    let eng = engine.clone();
    let tx_c = tx.clone();
    let flag = std::sync::Arc::clone(in_flight);
    tokio::spawn(async move {
        let res = eng.dialogs(200).await;
        flag.store(false, std::sync::atomic::Ordering::Release);
        if let Ok(list) = res {
            let _ = tx_c.send(list).await;
        }
    });
}

fn save_disk_cache(home: &std::path::Path, me: &str, dialogs: &[DialogRow], folders: &[FolderRow]) {
    let cache = DiskCache {
        me: me.to_owned(),
        dialogs: dialogs
            .iter()
            .map(|d| CachedDialog {
                id: d.id,
                name: d.name.clone(),
                last: d.last.clone(),
                date: d.date,
                unread: d.unread,
                pinned: d.pinned,
                muted: d.muted,
                notify_explicit: d.notify_explicit,
                saved: d.saved,
                archived: d.archived,
                kind: kind_to_u8(d.kind),
                contact: d.contact,
                read_out: d.read_out,
                emoji_status: d.emoji_status,
                verified: d.verified,
                premium: d.premium,
            })
            .collect(),
        folders: folders
            .iter()
            .map(|f| CachedFolder {
                id: f.id,
                title: f.title.clone(),
                contacts: f.contacts,
                non_contacts: f.non_contacts,
                groups: f.groups,
                broadcasts: f.broadcasts,
                bots: f.bots,
                exclude_muted: f.exclude_muted,
                exclude_read: f.exclude_read,
                exclude_archived: f.exclude_archived,
                include: f.include.clone(),
                exclude: f.exclude.clone(),
            })
            .collect(),
    };
    if let Ok(json) = serde_json::to_vec(&cache) {
        let tmp = home.join("dialogs_cache.tmp");
        let dst = home.join("dialogs_cache.json");
        if std::fs::write(&tmp, json).is_ok() {
            let _ = std::fs::rename(&tmp, &dst);
        }
    }
}

fn load_disk_cache(home: &std::path::Path) -> Option<(String, Vec<DialogRow>, Vec<FolderRow>)> {
    use grammers_session::types::{PeerAuth, PeerId, PeerRef};
    let bytes = std::fs::read(home.join("dialogs_cache.json")).ok()?;
    let cache: DiskCache = serde_json::from_slice(&bytes).ok()?;
    if cache.dialogs.is_empty() {
        return None;
    }
    let dialogs = cache
        .dialogs
        .into_iter()
        .map(|d| DialogRow {
            peer: PeerRef {
                id: PeerId::user_unchecked(d.id),
                auth: PeerAuth::default(),
            },
            id: d.id,
            name: d.name,
            last: d.last,
            date: d.date,
            unread: d.unread,
            pinned: d.pinned,
            muted: d.muted,
            notify_explicit: d.notify_explicit,
            saved: d.saved,
            archived: d.archived,
            kind: u8_to_kind(d.kind),
            contact: d.contact,
            read_out: d.read_out,
            emoji_status: d.emoji_status,
            verified: d.verified,
            premium: d.premium,
            unread_mentions: 0,
            unread_reactions: 0,
        })
        .collect();
    let folders = cache
        .folders
        .into_iter()
        .map(|f| FolderRow {
            id: f.id,
            title: f.title,
            contacts: f.contacts,
            non_contacts: f.non_contacts,
            groups: f.groups,
            broadcasts: f.broadcasts,
            bots: f.bots,
            exclude_muted: f.exclude_muted,
            exclude_read: f.exclude_read,
            exclude_archived: f.exclude_archived,
            include: f.include,
            exclude: f.exclude,
        })
        .collect();
    Some((cache.me, dialogs, folders))
}

/// Дисковый кэш последних сообщений чата (`cache/history/<id>.json`) — мгновенное открытие после перезапуска.
fn save_history_cache(home: &std::path::Path, dialog_id: i64, rows: &[MessageRow]) {
    let dir = home.join("cache").join("history");
    if std::fs::create_dir_all(&dir).is_err() {
        return;
    }
    let tail = rows.len().saturating_sub(60);
    if let Ok(bytes) = serde_json::to_vec(&rows[tail..]) {
        let tmp = dir.join(format!("{dialog_id}.tmp"));
        let dst = dir.join(format!("{dialog_id}.json"));
        if std::fs::write(&tmp, bytes).is_ok() {
            let _ = std::fs::rename(&tmp, &dst);
        }
    }
}

fn load_history_cache(home: &std::path::Path, dialog_id: i64) -> Option<Vec<MessageRow>> {
    let bytes = std::fs::read(home.join("cache").join("history").join(format!("{dialog_id}.json"))).ok()?;
    let rows: Vec<MessageRow> = serde_json::from_slice(&bytes).ok()?;
    (!rows.is_empty()).then_some(rows)
}

fn load_settings(path: &std::path::Path) -> Settings {
    match std::fs::read_to_string(path) {
        Ok(s) => Settings::from_json(&s).unwrap_or_default(),
        // Первый запуск: максимум приватности (см. `Settings::first_run`).
        Err(_) => Settings::first_run(),
    }
}

pub fn spawn(cmds: UnboundedReceiver<Cmd>, ev: Sender<Ev>, ctx: eframe::egui::Context) {
    std::thread::spawn(move || {
        let cores = std::thread::available_parallelism().map_or(4, |n| n.get());
        let (threads, blocking) = if cfg!(target_os = "android") {
            (cores.clamp(2, 4), 6)
        } else {
            (cores.clamp(2, 6), 12)
        };
        let Ok(rt) = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(threads)
            .max_blocking_threads(blocking)
            .enable_all()
            .build()
        else {
            return;
        };
        rt.block_on(run(cmds, ev, ctx));
    });
}

#[derive(Clone)]
struct Out {
    ev: Sender<Ev>,
    ctx: eframe::egui::Context,
}

impl Out {
    fn send(&self, e: Ev) {
        let _ = self.ev.send(e);
        self.ctx.request_repaint();
    }
    fn err(&self, text: impl std::fmt::Display) {
        let s = text.to_string();
        rg_core::diag::error("worker", &s);
        self.send(Ev::Error(s));
    }
}

async fn run(mut cmds: UnboundedReceiver<Cmd>, ev: Sender<Ev>, ctx: eframe::egui::Context) {
    let out = Out { ev, ctx };
    let root_home = home();
    let settings_path = root_home.join("settings.json");
    rg_core::diag::init(&root_home);

    // Однократная анонимная проверка обновлений GitHub Releases при старте (если разрешено в настройках).
    {
        let initial_s = load_settings(&settings_path);
        rg_core::diag::set_enabled(initial_s.diag_logging);
        rg_core::diag::info(
            "boot",
            &format!(
                "Rustgramm v{} start (active_account={}, opacity={}%, diag={})",
                env!("CARGO_PKG_VERSION"),
                initial_s.active_account,
                initial_s.window_opacity,
                initial_s.diag_logging
            ),
        );
        if initial_s.privacy_shield.check_updates {
            let out_upd = out.clone();
            tokio::spawn(async move {
                let res = tokio::task::spawn_blocking(|| {
                    crate::updater::check_latest_release(env!("CARGO_PKG_VERSION"))
                })
                .await;
                if let Ok(Ok(Some(info))) = res {
                    out_upd.send(Ev::UpdateAvailable(Box::new(info)));
                }
            });
        }
    }

    let mut pending_auth_cmd: Option<Cmd> = None;
    let mut keep_signed_in = true;
    let mut auth_retry_count = 0u8;
    'account_loop: loop {
        let mut settings = load_settings(&settings_path);
        if settings.accounts.is_empty() {
            settings.accounts.push(rg_settings::AccountProfile::default());
        }
        let home = account_home(&root_home, settings.active_account);
        wipe_if_logged_out(&home);
        if settings.privacy_shield.ephemeral_local_cache {
            let _ = std::fs::remove_file(home.join("dialogs_cache.json"));
            let _ = std::fs::remove_dir_all(home.join("cache").join("history"));
        }
        // Ключи: переменные окружения, файл `api.keys` или встроенные открытые ключи.
        let mut cfg = loop {
            if let Some(c) = Config::from_env()
                .ok()
                .or_else(|| keys_from_file(&root_home))
                .or_else(builtin_keys)
            {
                break c;
            }
            out.send(Ev::Stage(Stage::NeedKeys));
            match cmds.recv().await {
                Some(Cmd::Keys(id, hash)) => match id.trim().parse::<i32>() {
                    Ok(n) if n > 0 && !hash.trim().is_empty() => {
                        let _ = std::fs::write(root_home.join("api.keys"), format!("{n}\n{}\n", hash.trim()));
                    }
                    _ => out.err("api_id — число, api_hash — строка из 32 символов"),
                },
                Some(Cmd::SwitchAccount(slot)) => {
                    let mut s = settings.clone();
                    s.active_account = slot;
                    let _ = std::fs::write(&settings_path, s.to_json());
                    out.send(Ev::ResetSession);
                    continue 'account_loop;
                }
                Some(_) => {}
                None => return,
            }
        };
        cfg.session_path = home.join("main.session").to_string_lossy().into_owned();
        if cfg.proxy_url.is_none() {
            cfg.proxy_url = std::fs::read_to_string(home.join("proxy.txt"))
                .or_else(|_| std::fs::read_to_string(root_home.join("proxy.txt")))
                .ok()
                .map(|s| s.trim().to_owned())
                .filter(|s| s.starts_with("socks5://"));
        }
        if settings.privacy_shield.anti_correlation_accounts
            && let Some(prof) = settings.accounts.iter().find(|a| a.slot == settings.active_account)
        {
            cfg.device_model = Some(prof.device_model.clone());
            cfg.system_version = Some(prof.system_version.clone());
            cfg.app_version = Some(prof.app_version.clone());
            cfg.lang_code = Some(prof.lang_code.clone());
            if !prof.proxy_url.is_empty() && prof.proxy_url.starts_with("socks5://") {
                cfg.proxy_url = Some(prof.proxy_url.clone());
            }
        }

        out.send(Ev::Settings(Box::new(settings.clone())));

        // Мгновенный старт из локального кэша (если сессия уже есть и не включён RAM-only режим).
        let mut cached_me = String::new();
        let mut cached_folders: Vec<FolderRow> = Vec::new();
        let mut dialogs: Vec<DialogRow> = Vec::new();
        if !settings.privacy_shield.ephemeral_local_cache
            && home.join("main.session").exists()
            && let Some((me, dlgs, flds)) = load_disk_cache(&home)
        {
            cached_me = me.clone();
            cached_folders = flds.clone();
            let preload_ids: Vec<i64> = dlgs.iter().take(28).map(|d| d.id).collect();
            dialogs = dlgs.clone();
            if !me.is_empty() {
                out.send(Ev::Me(me));
            }
            if !flds.is_empty() {
                out.send(Ev::Folders(flds));
            }
            out.send(Ev::Dialogs(dlgs));
            out.send(Ev::Stage(Stage::Ready));
            let av_dir = home.join("cache").join("avatars");
            let out_av = out.clone();
            tokio::task::spawn_blocking(move || {
                for id in preload_ids {
                    let p = av_dir.join(format!("{id}.jpg"));
                    if p.exists()
                        && let Some(img) = decode_square(&p, 112)
                    {
                        out_av.send(Ev::Avatar(id, Box::new(img)));
                    }
                }
            });
        }

        let connected = match tokio::time::timeout(
            std::time::Duration::from_secs(14),
            connect_with_updates(&cfg),
        )
        .await
        {
            Ok(Ok(c)) => c,
            other => {
                // Сохраняем Stage::Ready, если оффлайн-кэш чатов уже показан пользователю.
                let err_msg = match other {
                    Ok(Err(e)) => {
                        rg_core::diag::error("connect", &format!("connect_with_updates failed: {e}"));
                        friendly_login_error(&e.to_string())
                    }
                    Err(_) => {
                        rg_core::diag::error("connect", "connect_with_updates timed out (14s)");
                        "Таймаут подключения к Telegram (14 с) — проверьте интернет/VPN или прокси в «Прокси / Сеть»".to_owned()
                    }
                    Ok(Ok(_)) => unreachable!(),
                };
                out.send(Ev::LoginBusy(false));
                if dialogs.is_empty() {
                    out.send(Ev::Stage(Stage::NeedPhone));
                }
                out.err(err_msg);
                loop {
                    match cmds.recv().await {
                        Some(Cmd::Proxy(url)) => {
                            let trimmed = url.trim().to_owned();
                            if trimmed.is_empty() || trimmed.starts_with("socks5://") {
                                let _ = std::fs::write(home.join("proxy.txt"), &trimmed);
                                out.send(Ev::Status("Прокси обновлён — переподключение…".to_owned()));
                                continue 'account_loop;
                            } else {
                                out.err("Допустим только адрес вида socks5://host:port");
                            }
                        }
                        Some(Cmd::RequestCode(phone)) => {
                            pending_auth_cmd = Some(Cmd::RequestCode(phone));
                            out.send(Ev::LoginBusy(true));
                            out.send(Ev::Status("Подключение к Telegram и отправка кода…".to_owned()));
                            continue 'account_loop;
                        }
                        Some(Cmd::UseQr) => {
                            pending_auth_cmd = Some(Cmd::UseQr);
                            out.send(Ev::Status("Подключение к Telegram для QR…".to_owned()));
                            continue 'account_loop;
                        }
                        Some(Cmd::RetryConnect | Cmd::UsePhone) => {
                            out.send(Ev::Status("Повторное подключение к Telegram…".to_owned()));
                            continue 'account_loop;
                        }
                        Some(Cmd::SwitchAccount(slot)) => {
                            let mut s = settings.clone();
                            s.active_account = slot;
                            let _ = std::fs::write(&settings_path, s.to_json());
                            out.send(Ev::ResetSession);
                            continue 'account_loop;
                        }
                        Some(Cmd::RemoveAccountSlot(slot)) => {
                            if slot > 0 {
                                let mut s = settings.clone();
                                s.accounts.retain(|a| a.slot != slot);
                                if s.active_account == slot {
                                    s.active_account = 0;
                                }
                                let _ = std::fs::write(&settings_path, s.to_json());
                                out.send(Ev::ResetSession);
                                continue 'account_loop;
                            }
                        }
                        Some(Cmd::SetKeepSignedIn(keep)) => {
                            keep_signed_in = keep;
                        }
                        Some(Cmd::Settings(s)) => {
                            rg_core::diag::set_enabled(s.diag_logging);
                            let _ = std::fs::write(&settings_path, s.to_json());
                        }
                        Some(Cmd::Shutdown) | None => return,
                        Some(_) => {}
                    }
                }
            }
        };
        let store_res = if settings.privacy_shield.ephemeral_local_cache {
            MessageStore::in_memory().await
        } else {
            MessageStore::open(&home.join("messages.db").to_string_lossy()).await
        };
        let store = match store_res {
            Ok(s) => s,
            Err(e) => {
                out.err(format!("Хранилище сообщений: {e}"));
                return;
            }
        };
        let engine = Engine::new(connected.client.clone(), settings, store)
            .with_session(connected.session.clone())
            .with_dirs(&home.join("cache"), &home.join("downloads"));
        // Умеренная конкурентность без голодания UI-потока + выделенный быстрый пул для эмодзи и стикеров.
        let (med_lim, av_lim, em_lim) = if cfg!(target_os = "android") {
            (4, 6, 10)
        } else {
            (8, 12, 18)
        };
        let limit = std::sync::Arc::new(tokio::sync::Semaphore::new(med_lim));
        let avatar_limit = std::sync::Arc::new(tokio::sync::Semaphore::new(av_lim));
        let emoji_limit = std::sync::Arc::new(tokio::sync::Semaphore::new(em_lim));
        let mut updates = Some(connected.updates);
        let (dialog_stream_tx, mut dialog_stream_rx) =
            tokio::sync::mpsc::channel::<(Vec<DialogRow>, bool)>(64);
        let (dialog_refresh_tx, mut dialog_refresh_rx) =
            tokio::sync::mpsc::channel::<Vec<DialogRow>>(8);
        let refresh_in_flight = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let mut last_dialog_refresh = std::time::Instant::now()
            .checked_sub(std::time::Duration::from_secs(10))
            .unwrap_or_else(std::time::Instant::now);
        type ActiveCallMedia = std::sync::Arc<
            std::sync::Mutex<
                Option<(rg_core::call_media::CallMediaSession, crate::audio_io::CallAudioPump)>,
            >,
        >;
        let call_media_slot: ActiveCallMedia = std::sync::Arc::default();

        let mut login: Option<SentCodeInfo> = None;
        let mut password: Option<PasswordToken> = None;
        let mut current: Option<usize> = None;
        let open_gen = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
        let sticker_sets: std::sync::Arc<std::sync::Mutex<Vec<StickerSetRow>>> =
            std::sync::Arc::default();
        let sticker_docs: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i64, StickerRow>>> =
            std::sync::Arc::default();
        let fetches: std::sync::Arc<std::sync::Mutex<std::collections::HashMap<i32, std::sync::Arc<std::sync::atomic::AtomicBool>>>> =
            std::sync::Arc::default();

        // На Android при отсутствии main.session не ждём 12с проверки авторизации — сразу открываем ввод номера.
        let session_has_data = std::fs::metadata(home.join("main.session")).is_ok_and(|m| m.len() > 2048);
        let authorized = if cfg!(target_os = "android") && !session_has_data {
            false
        } else {
            match tokio::time::timeout(
                std::time::Duration::from_secs(8),
                connected.client.is_authorized(),
            )
            .await
            {
                Ok(Ok(ok)) => ok,
                Ok(Err(e)) => {
                    rg_core::diag::warn("auth", &format!("is_authorized error: {e}"));
                    false
                }
                Err(_) => {
                    rg_core::diag::warn("auth", "is_authorized timeout (8s)");
                    false
                }
            }
        };
        let mut qr_mode = false;
        let mut qr_fail_streak = 0u32;
        if authorized {
            pending_auth_cmd = None;
            enter(&engine, &connected.session, &out, &mut updates, &mut dialogs, &home, &settings_path, &mut cached_me, &mut cached_folders, &dialog_stream_tx).await;
        } else if let Some(pending) = pending_auth_cmd.take() {
            match pending {
                Cmd::RequestCode(phone) => {
                    out.send(Ev::Stage(Stage::NeedPhone));
                    out.send(Ev::LoginBusy(true));
                    out.send(Ev::Status("Подключение к серверам Telegram…".to_owned()));
                    let res = tokio::time::timeout(
                        std::time::Duration::from_secs(20),
                        rg_core::auth::request_login_code(
                            &connected.client,
                            &*connected.session,
                            Some(&*connected.pool_handle),
                            cfg.api_id,
                            &cfg.api_hash,
                            &phone,
                        ),
                    )
                    .await;
                    out.send(Ev::LoginBusy(false));
                    out.send(Ev::Status(String::new()));
                    match res {
                        Ok(Ok(RequestCodeOutcome::CodeSent(info))) => {
                            out.send(Ev::CodeDeliveryHint(info.delivery_hint.clone()));
                            out.send(Ev::CodeMeta {
                                length: info.code_length,
                                resend_secs: info.timeout_secs,
                            });
                            login = Some(info);
                            out.send(Ev::Stage(Stage::NeedCode));
                        }
                        Ok(Ok(RequestCodeOutcome::AlreadyAuthorized)) => {
                            enter(&engine, &connected.session, &out, &mut updates, &mut dialogs, &home, &settings_path, &mut cached_me, &mut cached_folders, &dialog_stream_tx).await;
                        }
                        Ok(Err(e)) => out.err(friendly_login_error(&e)),
                        Err(_) => out.err("Сервер Telegram не ответил за 20 с — проверьте интернет, VPN или укажите SOCKS5 прокси в «Прокси»"),
                    }
                }
                Cmd::UseQr | Cmd::RefreshQr => {
                    qr_mode = true;
                    out.send(Ev::Stage(Stage::Qr(String::new())));
                }
                _ => {
                    out.send(Ev::Stage(Stage::NeedPhone));
                }
            }
        } else if cfg!(target_os = "android") {
            qr_mode = false;
            out.send(Ev::Stage(Stage::NeedPhone));
        } else {
            qr_mode = true;
            out.send(Ev::Stage(Stage::Qr(String::new())));
        }
        let mut qr_next_poll_at = tokio::time::Instant::now();
        let mut last_url = String::new();

        #[allow(clippy::never_loop)]
        let switch_now: bool = loop {
        let cmd = tokio::select! {
            c = cmds.recv() => match c { Some(c) => c, None => break false },
            maybe_upd = async {
                match updates.as_mut() {
                    Some(rx) => rx.recv().await,
                    None => std::future::pending().await,
                }
            }, if qr_mode && updates.is_some() => {
                match maybe_upd {
                    Some(upd) => {
                        if rg_core::qr::has_login_token_update(&upd) {
                            qr_next_poll_at = tokio::time::Instant::now();
                        }
                    }
                    None => {
                        updates = None;
                    }
                }
                continue;
            }
            Some((batch, done)) = dialog_stream_rx.recv() => {
                if !batch.is_empty() {
                    let mut known: std::collections::HashSet<i64> = dialogs.iter().map(|d| d.id).collect();
                    let mut added = Vec::with_capacity(batch.len());
                    for row in batch {
                        if known.insert(row.id) {
                            dialogs.push(row.clone());
                            added.push(row);
                        }
                    }
                    if !added.is_empty() || done {
                        out.send(Ev::DialogsBatch(added, done));
                    }
                } else if done {
                    out.send(Ev::DialogsBatch(Vec::new(), true));
                }
                if done && !engine.settings().privacy_shield.ephemeral_local_cache {
                    spawn_save_disk_cache(&home, &cached_me, &dialogs, &cached_folders);
                }
                continue;
            }
            Some(mut fresh) = dialog_refresh_rx.recv() => {
                let keep = current.and_then(|i| dialogs.get(i)).map(|d| d.id);
                let fresh_ids: std::collections::HashSet<i64> = fresh.iter().map(|d| d.id).collect();
                for old in dialogs.drain(..) {
                    if !fresh_ids.contains(&old.id) {
                        fresh.push(old);
                    }
                }
                dialogs = fresh;
                current = keep.and_then(|id| dialogs.iter().position(|d| d.id == id));
                out.send(Ev::Dialogs(dialogs.clone()));
                if !engine.settings().privacy_shield.ephemeral_local_cache {
                    spawn_save_disk_cache(&home, &cached_me, &dialogs, &cached_folders);
                }
                continue;
            }
            _ = tokio::time::sleep_until(qr_next_poll_at), if qr_mode => {
                let qr_res = tokio::time::timeout(
                    std::time::Duration::from_secs(15),
                    rg_core::qr::poll(&connected.client, &*connected.session, cfg.api_id, &cfg.api_hash),
                )
                .await;
                match qr_res {
                    Ok(Ok(rg_core::qr::QrStep::Token { url, expires })) => {
                        qr_fail_streak = 0;
                        let now_unix = std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs() as i64);
                        let rem = (i64::from(expires) - now_unix - 1).clamp(12, 26) as u64;
                        qr_next_poll_at = tokio::time::Instant::now() + std::time::Duration::from_secs(rem);
                        if url != last_url {
                            last_url = url.clone();
                            out.send(Ev::Stage(Stage::Qr(url)));
                        }
                    }
                    Ok(Ok(rg_core::qr::QrStep::Expired)) => {
                        qr_fail_streak = 0;
                        qr_next_poll_at = tokio::time::Instant::now() + std::time::Duration::from_millis(250);
                    }
                    Ok(Ok(rg_core::qr::QrStep::Done)) => {
                        qr_mode = false;
                        enter(&engine, &connected.session, &out, &mut updates, &mut dialogs, &home, &settings_path, &mut cached_me, &mut cached_folders, &dialog_stream_tx).await;
                    }
                    Ok(Ok(rg_core::qr::QrStep::NeedPassword(p))) => {
                        qr_mode = false;
                        let hint = p.hint().unwrap_or("нет").to_owned();
                        password = Some(p);
                        out.send(Ev::Stage(Stage::NeedPassword(hint)));
                    }
                    Ok(Err(e)) => {
                        qr_fail_streak = (qr_fail_streak + 1).min(5);
                        let wait_s = u64::from(qr_fail_streak) * 3;
                        qr_next_poll_at = tokio::time::Instant::now() + std::time::Duration::from_secs(wait_s);
                        out.err(friendly_login_error(&e.to_string()));
                    }
                    Err(_) => {
                        qr_fail_streak = (qr_fail_streak + 1).min(5);
                        let wait_s = u64::from(qr_fail_streak) * 3;
                        qr_next_poll_at = tokio::time::Instant::now() + std::time::Duration::from_secs(wait_s);
                        out.err("Таймаут загрузки QR-кода — проверьте сеть или прокси");
                    }
                }
                continue;
            }
        };
        let play = matches!(cmd, Cmd::OpenFile(_));
        match cmd {
            Cmd::Keys(..) => {}
            Cmd::SetKeepSignedIn(keep) => {
                keep_signed_in = keep;
            }
            Cmd::Proxy(url) => {
                let trimmed = url.trim().to_owned();
                if trimmed.is_empty() || trimmed.starts_with("socks5://") {
                    let _ = std::fs::write(home.join("proxy.txt"), &trimmed);
                    out.send(Ev::Status(if trimmed.is_empty() {
                        "Прокси отключён — переподключение…".to_owned()
                    } else {
                        "Прокси сохранён — переподключение…".to_owned()
                    }));
                    connected.pool_handle.quit();
                    tokio::time::sleep(std::time::Duration::from_millis(80)).await;
                    out.send(Ev::ResetSession);
                    break true;
                } else {
                    out.err("Допустим только адрес вида socks5://host:port");
                }
            }
            Cmd::UsePhone | Cmd::BackToPhone => {
                qr_mode = false;
                qr_fail_streak = 0;
                login = None;
                password = None;
                out.send(Ev::LoginBusy(false));
                out.send(Ev::Stage(Stage::NeedPhone));
            }
            Cmd::UseQr | Cmd::RefreshQr => {
                qr_mode = true;
                qr_fail_streak = 0;
                qr_next_poll_at = tokio::time::Instant::now();
                last_url.clear();
                out.send(Ev::LoginBusy(false));
                out.send(Ev::Stage(Stage::Qr(String::new())));
            }
            Cmd::RequestCode(phone) => {
                out.send(Ev::LoginBusy(true));
                out.send(Ev::Status("Подключение к серверам Telegram…".to_owned()));
                let res = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    rg_core::auth::request_login_code(
                        &connected.client,
                        &*connected.session,
                        Some(&*connected.pool_handle),
                        cfg.api_id,
                        &cfg.api_hash,
                        &phone,
                    ),
                )
                .await;
                out.send(Ev::LoginBusy(false));
                out.send(Ev::Status(String::new()));
                match res {
                    Ok(Ok(RequestCodeOutcome::CodeSent(info))) => {
                        auth_retry_count = 0;
                        out.send(Ev::CodeDeliveryHint(info.delivery_hint.clone()));
                        out.send(Ev::CodeMeta {
                            length: info.code_length,
                            resend_secs: info.timeout_secs,
                        });
                        login = Some(info);
                        out.send(Ev::Stage(Stage::NeedCode));
                    }
                    Ok(Ok(RequestCodeOutcome::AlreadyAuthorized)) => {
                        auth_retry_count = 0;
                        enter(&engine, &connected.session, &out, &mut updates, &mut dialogs, &home, &settings_path, &mut cached_me, &mut cached_folders, &dialog_stream_tx).await;
                    }
                    Ok(Err(e)) => {
                        auth_retry_count = 0;
                        out.err(friendly_login_error(&e));
                    }
                    Err(_) if auth_retry_count == 0 => {
                        auth_retry_count += 1;
                        rg_core::diag::warn("auth", "request_login_code 20s timeout — reconnecting pool and retrying");
                        connected.pool_handle.quit();
                        pending_auth_cmd = Some(Cmd::RequestCode(phone));
                        break true;
                    }
                    Err(_) => {
                        auth_retry_count = 0;
                        out.err(
                            "Сервер Telegram не ответил за 20 с — проверьте интернет, VPN или укажите SOCKS5 прокси в «Прокси»",
                        );
                    }
                }
            }
            Cmd::ResendCode => {
                let Some(t) = login.as_ref() else {
                    out.err("Сессия кода устарела — введите номер телефона заново");
                    out.send(Ev::Stage(Stage::NeedPhone));
                    continue;
                };
                out.send(Ev::LoginBusy(true));
                out.send(Ev::Status("Повторный запрос кода…".to_owned()));
                let res = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    rg_core::auth::resend_login_code(
                        &connected.client,
                        &*connected.session,
                        Some(&*connected.pool_handle),
                        &t.phone,
                        &t.phone_code_hash,
                    ),
                )
                .await;
                out.send(Ev::LoginBusy(false));
                out.send(Ev::Status(String::new()));
                match res {
                    Ok(Ok(RequestCodeOutcome::CodeSent(info))) => {
                        out.send(Ev::CodeDeliveryHint(info.delivery_hint.clone()));
                        out.send(Ev::CodeMeta {
                            length: info.code_length,
                            resend_secs: info.timeout_secs,
                        });
                        login = Some(info);
                    }
                    Ok(Ok(RequestCodeOutcome::AlreadyAuthorized)) => {
                        enter(&engine, &connected.session, &out, &mut updates, &mut dialogs, &home, &settings_path, &mut cached_me, &mut cached_folders, &dialog_stream_tx).await;
                    }
                    Ok(Err(e)) => out.err(friendly_login_error(&e)),
                    Err(_) => out.err("Таймаут повторной отправки кода — проверьте сеть"),
                }
            }
            Cmd::Code(code) => {
                let Some(t) = login.as_ref() else {
                    out.err("Сессия кода устарела — введите номер телефона заново");
                    out.send(Ev::Stage(Stage::NeedPhone));
                    continue;
                };
                out.send(Ev::LoginBusy(true));
                out.send(Ev::Status("Проверка кода…".to_owned()));
                let res = tokio::time::timeout(
                    std::time::Duration::from_secs(20),
                    rg_core::auth::sign_in(
                        &connected.client,
                        &*connected.session,
                        Some(&*connected.pool_handle),
                        &t.phone,
                        &t.phone_code_hash,
                        code.trim(),
                    ),
                )
                .await;
                out.send(Ev::LoginBusy(false));
                out.send(Ev::Status(String::new()));
                match res {
                    Ok(Ok(SignInOutcome::SignedIn)) => enter(&engine, &connected.session, &out, &mut updates, &mut dialogs, &home, &settings_path, &mut cached_me, &mut cached_folders, &dialog_stream_tx).await,
                    Ok(Ok(SignInOutcome::NeedPassword(p))) => {
                        let hint = p.hint().unwrap_or("нет").to_owned();
                        password = Some(p);
                        out.send(Ev::Stage(Stage::NeedPassword(hint)));
                    }
                    Ok(Ok(SignInOutcome::SignUpRequired)) => {
                        out.err("Этот номер ещё не зарегистрирован в Telegram — создайте аккаунт в официальном приложении");
                    }
                    Ok(Err(e)) => out.err(friendly_login_error(&e)),
                    Err(_) => out.err("Таймаут проверки кода — попробуйте снова"),
                }
            }
            Cmd::Password(pw) => {
                out.send(Ev::LoginBusy(true));
                out.send(Ev::Status("Проверка облачного пароля…".to_owned()));
                let token = match password.take() {
                    Some(p) => Ok(p),
                    None => {
                        rg_core::auth::refresh_password_token(
                            &connected.client,
                            &*connected.session,
                            Some(&*connected.pool_handle),
                        )
                        .await
                    }
                };
                match token {
                    Ok(p) => {
                        match rg_core::auth::check_password_safe(
                            &connected.client,
                            &*connected.session,
                            Some(&*connected.pool_handle),
                            p,
                            pw.trim(),
                        )
                        .await
                        {
                            rg_core::auth::CheckPasswordOutcome::SignedIn => {
                                out.send(Ev::LoginBusy(false));
                                out.send(Ev::Status(String::new()));
                                enter(&engine, &connected.session, &out, &mut updates, &mut dialogs, &home, &settings_path, &mut cached_me, &mut cached_folders, &dialog_stream_tx).await;
                            }
                            rg_core::auth::CheckPasswordOutcome::InvalidPassword(p) => {
                                let hint = p.hint().unwrap_or("нет").to_owned();
                                password = Some(p);
                                out.send(Ev::LoginBusy(false));
                                out.send(Ev::Status(String::new()));
                                out.send(Ev::Stage(Stage::NeedPassword(hint)));
                                out.err("Неверный облачный пароль");
                            }
                            rg_core::auth::CheckPasswordOutcome::Error { token: recovered, message } => {
                                password = recovered;
                                out.send(Ev::LoginBusy(false));
                                out.send(Ev::Status(String::new()));
                                out.err(friendly_login_error(&message));
                            }
                        }
                    }
                    Err(e) => {
                        out.send(Ev::LoginBusy(false));
                        out.send(Ev::Status(String::new()));
                        out.err(friendly_login_error(&e));
                    }
                }
            }
            Cmd::Open(i) => {
                current = Some(i);
                let Some(d) = dialogs.get(i) else { continue };
                let seq = open_gen.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
                let ephemeral = engine.settings().privacy_shield.ephemeral_local_cache;
                let (eng, out, peer, did, home_c, open_gen_c) =
                    (engine.clone(), out.clone(), d.peer, d.id, home.clone(), open_gen.clone());
                rg_core::diag::info("chat", &format!("open dialog id={did} name={}", d.name));
                tokio::spawn(async move {
                    // Мгновенно отдаём кэш истории с диска в blocking-потоке (< 2 мс), пока сеть забирает свежие.
                    if !ephemeral {
                        let h_read = home_c.clone();
                        if let Ok(Some(cached)) =
                            tokio::task::spawn_blocking(move || load_history_cache(&h_read, did)).await
                            && open_gen_c.load(std::sync::atomic::Ordering::SeqCst) == seq
                        {
                            out.send(Ev::History(did, cached));
                        }
                    }
                    // 45 свежих сообщений приходят одним быстрым RPC с автоповтором 500-ошибок.
                    match eng.history(peer, 45).await {
                        Ok(rows) => {
                            if open_gen_c.load(std::sync::atomic::Ordering::SeqCst) != seq {
                                return;
                            }
                            if !ephemeral {
                                let rows_save = rows.clone();
                                let h2 = home_c.clone();
                                tokio::task::spawn_blocking(move || save_history_cache(&h2, did, &rows_save));
                            }
                            out.send(Ev::History(did, rows));
                            match eng.mark_read(peer).await {
                                Ok(true) => out.send(Ev::Status(String::new())),
                                Ok(false) => out.send(Ev::Status("Режим призрака: «прочитано» не отправлено".into())),
                                Err(e) => out.err(e),
                            }
                        }
                        Err(e) => {
                            rg_core::diag::error("chat", &format!("history id={did} failed: {e}"));
                            if open_gen_c.load(std::sync::atomic::Ordering::SeqCst) == seq {
                                out.send(Ev::HistoryError(did, format!("История: {e}")));
                            }
                        }
                    }
                });
            }
            Cmd::Send(text, reply_to, temp_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.send_reply(peer, &text, reply_to).await {
                        Ok(row) => out.send(Ev::SentConfirm(did, temp_id, Box::new(row))),
                        Err(e) => {
                            let msg = format!("Не отправлено: {e}");
                            out.send(Ev::SendFailed(did, temp_id, msg.clone()));
                            out.err(msg);
                        }
                    }
                });
            }
            Cmd::SendRich(text, custom, reply_to, temp_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.send_rich(peer, &text, &custom, reply_to).await {
                        Ok(row) => out.send(Ev::SentConfirm(did, temp_id, Box::new(row))),
                        Err(e) => {
                            let msg = format!("Не отправлено: {e}");
                            out.send(Ev::SendFailed(did, temp_id, msg.clone()));
                            out.err(msg);
                        }
                    }
                });
            }
            Cmd::SendFormatted(text, ents, reply_to, temp_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.send_formatted(peer, &text, &ents, reply_to).await {
                        Ok(row) => out.send(Ev::SentConfirm(did, temp_id, Box::new(row))),
                        Err(e) => {
                            let msg = format!("Не отправлено: {e}");
                            out.send(Ev::SendFailed(did, temp_id, msg.clone()));
                            out.err(msg);
                        }
                    }
                });
            }
            Cmd::ForwardBatch(ids, to, no_author) => {
                let Some(i) = current else { continue };
                let (Some(from), Some(dest)) = (dialogs.get(i), dialogs.get(to)) else { continue };
                let (eng, out, from_peer, dest_peer, dest_name) = (
                    engine.clone(),
                    out.clone(),
                    from.peer,
                    dest.peer,
                    dest.name.clone(),
                );
                tokio::spawn(async move {
                    let n = ids.len();
                    match eng.forward_batch(from_peer, dest_peer, &ids, no_author).await {
                        Ok(()) => out.send(Ev::Status(format!(
                            "Переслано сообщений ({n}){}: {dest_name}",
                            if no_author { " без автора" } else { "" },
                        ))),
                        Err(e) => out.err(format!("Пакетная пересылка: {e}")),
                    }
                });
            }
            Cmd::SavedHistory => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let list = engine.audit_peer(&d.peer).await;
                out.send(Ev::SavedHistory(d.id, list));
            }
            Cmd::TgsMessage(i, msg_id) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, limit, peer) = (engine.clone(), out.clone(), emoji_limit.clone(), d.peer);
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(bytes)) = eng.tgs_message_bytes(peer, msg_id).await {
                        out.send(Ev::TgsMsg(msg_id, bytes));
                    }
                });
            }
            Cmd::ForumTopics(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok(list) = eng.forum_topics(peer).await {
                        out.send(Ev::ForumTopics(id, list));
                    }
                });
            }
            Cmd::TopicMessages(i, topic_id) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok(rows) = eng.topic_messages(peer, topic_id, 60).await {
                        out.send(Ev::TopicMessages(id, topic_id, rows));
                    }
                });
            }
            Cmd::Discussion(i, post_id) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.discussion_comments(peer, post_id).await {
                        Ok(Some(th)) => out.send(Ev::Discussion(th)),
                        Ok(None) => out.err("У этого поста нет обсуждения"),
                        Err(e) => out.err(format!("Комментарии: {e}")),
                    }
                });
            }
            Cmd::SendComment(disc_peer, root_msg_id, post_id, text, temp_id) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.send_reply(disc_peer, &text, Some(root_msg_id)).await {
                        Ok(row) => out.send(Ev::CommentConfirmed(post_id, temp_id, Box::new(row))),
                        Err(e) => {
                            let msg = format!("Комментарий: {e}");
                            out.send(Ev::CommentFailed(post_id, temp_id, msg.clone()));
                            out.err(msg);
                        }
                    }
                });
            }
            Cmd::LoadStories => {
                let (eng, out, limit) = (engine.clone(), out.clone(), avatar_limit.clone());
                tokio::spawn(async move {
                    match eng.stories().await {
                        Ok(st) => {
                            preload_stories(&eng, &out, &limit, &st);
                            out.send(Ev::Stories(st));
                        }
                        Err(e) => rg_core::diag::warn("stories", &format!("stories load failed: {e}")),
                    }
                });
            }
            Cmd::ReadStory(peer, story_id) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Ok(false) = eng.read_story(peer, story_id).await {
                        out.send(Ev::Status("Режим призрака: просмотр истории скрыт".into()));
                    }
                });
            }
            Cmd::SetChatTtl(period) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.set_chat_ttl(peer, period).await {
                        Ok(()) => {
                            out.send(Ev::Status(if period == 0 {
                                "Автоудаление отключено".into()
                            } else {
                                format!("Автоудаление: {} дн.", period / 86_400)
                            }));
                            if let Ok(info) = eng.peer_info(peer).await {
                                out.send(Ev::PeerInfo(did, Box::new(info)));
                            }
                        }
                        Err(e) => out.err(format!("Автоудаление: {e}")),
                    }
                });
            }
            Cmd::LoadPinned(i, msg_id) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok(Some(m)) = eng.message_row_by_id(peer, msg_id).await {
                        out.send(Ev::PinnedMsg(id, Box::new(m)));
                    }
                });
            }
            Cmd::CustomEmojiDocs(ids) => {
                let (eng, out, limit, cache_dir) = (
                    engine.clone(),
                    out.clone(),
                    emoji_limit.clone(),
                    home.join("cache"),
                );
                tokio::spawn(async move {
                    // Мгновенная локальная подгрузка уже кэшированных `.tgs` / `.webp` эмодзи без RPC-запроса.
                    let missing = tokio::task::spawn_blocking({
                        let ids = ids.clone();
                        let out = out.clone();
                        move || {
                            let mut miss = Vec::new();
                            for id in ids {
                                let tgs_p = cache_dir.join("tgs").join(format!("{id}.tgs"));
                                if let Ok(bytes) = std::fs::read(&tgs_p)
                                    && !bytes.is_empty()
                                {
                                    out.send(Ev::TgsDoc(id, bytes));
                                    continue;
                                }
                                let webp_p = cache_dir.join("stickers").join(format!("{id}.webp"));
                                let thumb_p = cache_dir.join("stickers").join(format!("{id}_t.webp"));
                                if let Some(img) = decode_square(&webp_p, 128)
                                    .or_else(|| decode_square(&thumb_p, 128))
                                {
                                    out.send(Ev::StickerImg(id, Box::new(img)));
                                    continue;
                                }
                                miss.push(id);
                            }
                            miss
                        }
                    })
                    .await
                    .unwrap_or(ids);
                    if missing.is_empty() {
                        return;
                    }
                    if let Ok(rows) = eng.custom_emoji_docs(&missing).await {
                        for row in rows {
                            let (e2, o2, lim) = (eng.clone(), out.clone(), limit.clone());
                            tokio::spawn(async move {
                                let Ok(_permit) = lim.acquire().await else { return };
                                if row.mime == "application/x-tgsticker"
                                    && let Ok(Some(bytes)) = e2.tgs_sticker_bytes(&row).await
                                {
                                    o2.send(Ev::TgsDoc(row.id, bytes));
                                    return;
                                }
                                if let Ok(Some(path)) = e2.sticker_file(&row).await
                                    && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 128)).await
                                {
                                    o2.send(Ev::StickerImg(row.id, Box::new(img)));
                                }
                            });
                        }
                    }
                });
            }
            Cmd::CreateChannelFull(title, about, broadcast, username, members) => {
                let (eng, out, tx, flag) = (
                    engine.clone(),
                    out.clone(),
                    dialog_refresh_tx.clone(),
                    std::sync::Arc::clone(&refresh_in_flight),
                );
                tokio::spawn(async move {
                    match eng
                        .create_channel_full(&title, &about, broadcast, username.as_deref(), &members)
                        .await
                    {
                        Ok(()) => {
                            out.send(Ev::Status(format!("Создано: «{title}»")));
                            if let Ok(list) = eng.dialogs(200).await {
                                flag.store(false, std::sync::atomic::Ordering::Release);
                                let _ = tx.send(list).await;
                            }
                        }
                        Err(e) => out.err(format!("Не создано: {e}")),
                    }
                });
            }
            Cmd::InviteMembers(i, members) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.invite_members(peer, &members).await {
                        Ok(()) => {
                            out.send(Ev::Status("Участники приглашены".into()));
                            if let Ok(p) = eng.channel_participants(peer, 80).await {
                                out.send(Ev::Participants(id, p));
                            }
                            if let Ok(info) = eng.peer_info(peer).await {
                                out.send(Ev::PeerInfo(id, Box::new(info)));
                            }
                        }
                        Err(e) => out.err(format!("Приглашение: {e}")),
                    }
                });
            }
            Cmd::Participants(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok(list) = eng.channel_participants(peer, 80).await {
                        out.send(Ev::Participants(id, list));
                    }
                });
            }
            Cmd::ExportInvite(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.export_invite_link(peer).await {
                        Ok(link) if !link.is_empty() => out.send(Ev::InviteLink(id, link)),
                        Ok(_) => out.err("Ссылка-приглашение недоступна"),
                        Err(e) => out.err(format!("Ссылка: {e}")),
                    }
                });
            }
            Cmd::BlockPeer(i, block) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.set_blocked(peer, block).await {
                        Ok(()) => {
                            out.send(Ev::Status(if block { "Пользователь заблокирован".into() } else { "Пользователь разблокирован".into() }));
                            if let Ok(info) = eng.peer_info(peer).await {
                                out.send(Ev::PeerInfo(id, Box::new(info)));
                            }
                        }
                        Err(e) => out.err(format!("Блокировка: {e}")),
                    }
                });
            }
            Cmd::VotePoll(msg_id, option) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.vote_poll(peer, msg_id, option).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Голосование: {e}")),
                    }
                });
            }
            Cmd::MemberSearch(q) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    let list = eng.global_search(&q, 15).await.unwrap_or_default();
                    // Оставляем только пользователей (id > 0), которых можно пригласить в канал/группу.
                    let users: Vec<ContactRow> = list.into_iter().filter(|c| c.id > 0).collect();
                    out.send(Ev::MemberSearch(users));
                });
            }
            Cmd::SendFile(path, caption, reply_to) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                out.send(Ev::Status("Отправка файла…".into()));
                tokio::spawn(async move {
                    match eng.send_file(peer, &path, &caption, reply_to).await {
                        Ok(()) => {
                            out.send(Ev::Status(String::new()));
                            reload(&eng, &out, peer, i).await;
                        }
                        Err(e) => out.err(format!("Файл не отправлен: {e}")),
                    }
                });
            }
            Cmd::EditHistory(id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    out.send(Ev::EditHistory(id, eng.edit_history(&peer, id).await));
                });
            }
            Cmd::Archive(i, on) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, tx, flag) = (
                    engine.clone(),
                    out.clone(),
                    d.peer,
                    dialog_refresh_tx.clone(),
                    std::sync::Arc::clone(&refresh_in_flight),
                );
                tokio::spawn(async move {
                    match eng.set_archived(peer, on).await {
                        Ok(()) => fetch_and_push_dialogs(&eng, &tx, &flag).await,
                        Err(e) => out.err(format!("Архив: {e}")),
                    }
                });
            }
            Cmd::PinDialog(i, on) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, tx, flag) = (
                    engine.clone(),
                    out.clone(),
                    d.peer,
                    dialog_refresh_tx.clone(),
                    std::sync::Arc::clone(&refresh_in_flight),
                );
                tokio::spawn(async move {
                    match eng.set_pinned_dialog(peer, on).await {
                        Ok(()) => fetch_and_push_dialogs(&eng, &tx, &flag).await,
                        Err(e) => out.err(format!("Закрепление: {e}")),
                    }
                });
            }
            Cmd::MuteDialog(i, on) => {
                let Some(d) = dialogs.get_mut(i) else { continue };
                let peer = d.peer;
                d.muted = on;
                d.notify_explicit = Some(on);
                out.send(Ev::Dialogs(dialogs.clone()));
                let (eng, out, tx, flag) = (
                    engine.clone(),
                    out.clone(),
                    dialog_refresh_tx.clone(),
                    std::sync::Arc::clone(&refresh_in_flight),
                );
                tokio::spawn(async move {
                    match eng.set_muted(peer, on).await {
                        Ok(()) => fetch_and_push_dialogs(&eng, &tx, &flag).await,
                        Err(e) => out.err(format!("Звук: {e}")),
                    }
                });
            }
            Cmd::ReadDialog(i) => {
                let Some(peer) = dialogs.get(i).map(|d| d.peer) else { continue };
                if let Some(d) = dialogs.get_mut(i) {
                    d.unread = 0;
                }
                spawn_save_disk_cache(&home, &cached_me, &dialogs, &cached_folders);
                out.send(Ev::Dialogs(dialogs.clone()));
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.force_read(peer).await {
                        Ok(()) => out.send(Ev::Status("Чат отмечен прочитанным на сервере".into())),
                        Err(e) => out.err(format!("Не отмечено: {e}")),
                    }
                });
            }
            Cmd::Contacts => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.contacts().await {
                        Ok(c) => out.send(Ev::Contacts(c)),
                        Err(e) => out.err(format!("Контакты: {e}")),
                    }
                });
            }
            Cmd::OpenContact(peer, id, name) => {
                if !dialogs.iter().any(|d| d.id == id) {
                    dialogs.insert(
                        0,
                        DialogRow {
                            peer,
                            id,
                            name,
                            last: String::new(),
                            date: 0,
                            unread: 0,
                            pinned: false,
                            muted: false,
                            notify_explicit: None,
                            saved: false,
                            archived: false,
                            kind: DialogKind::User,
                            contact: true,
                            read_out: 0,
                            emoji_status: None,
                            verified: false,
                            premium: false,
                            unread_mentions: 0,
                            unread_reactions: 0,
                        },
                    );
                    current = current.map(|i| i + 1);
                    out.send(Ev::Dialogs(dialogs.clone()));
                }
                out.send(Ev::Select(id));
            }
            Cmd::AvatarPeer(peer, id) => {
                let (eng, out, limit) = (engine.clone(), out.clone(), avatar_limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.avatar(peer).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 112)).await
                    {
                        out.send(Ev::Avatar(id, Box::new(img)));
                    }
                });
            }
            Cmd::Stickers => {
                let (eng, out, sets_arc, docs_arc) = (
                    engine.clone(),
                    out.clone(),
                    sticker_sets.clone(),
                    sticker_docs.clone(),
                );
                tokio::spawn(async move {
                    let (sets_res, faved_res, recent_res) = tokio::join!(
                        eng.sticker_sets(),
                        eng.faved_stickers(),
                        eng.recent_stickers()
                    );
                    if let Ok(fav) = faved_res {
                        if let Ok(mut d) = docs_arc.lock() {
                            for st in &fav {
                                d.insert(st.id, st.clone());
                            }
                        }
                        out.send(Ev::FavedStickers(fav));
                    }
                    if let Ok(rec) = recent_res {
                        if let Ok(mut d) = docs_arc.lock() {
                            for st in &rec {
                                d.insert(st.id, st.clone());
                            }
                        }
                        out.send(Ev::RecentStickers(rec));
                    }
                    match sets_res {
                        Ok(s) => {
                            if let Ok(mut guard) = sets_arc.lock() {
                                *guard = s.clone();
                            }
                            out.send(Ev::StickerSets(s));
                        }
                        Err(e) => out.err(format!("Стикеры: {e}")),
                    }
                    out.send(Ev::StickersLoaded);
                });
            }
            Cmd::StickerSet(i) => {
                let set = sticker_sets
                    .lock()
                    .ok()
                    .and_then(|g| g.get(i).cloned());
                let Some(set) = set else { continue };
                let (eng, out, docs_arc) = (engine.clone(), out.clone(), sticker_docs.clone());
                tokio::spawn(async move {
                    match eng.sticker_set(&set).await {
                        Ok(list) => {
                            if let Ok(mut d) = docs_arc.lock() {
                                for st in &list {
                                    d.insert(st.id, st.clone());
                                }
                            }
                            out.send(Ev::StickerList(set.id, list));
                        }
                        Err(e) => out.err(format!("Набор стикеров: {e}")),
                    }
                });
            }
            Cmd::StickerThumb(doc) => {
                let row = sticker_docs.lock().ok().and_then(|g| g.get(&doc).cloned());
                let Some(row) = row else { continue };
                let (eng, out, limit) = (engine.clone(), out.clone(), emoji_limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if row.mime == "application/x-tgsticker"
                        && let Ok(Some(bytes)) = eng.tgs_sticker_bytes(&row).await
                    {
                        out.send(Ev::TgsDoc(doc, bytes));
                        return;
                    }
                    if let Ok(Some(path)) = eng.sticker_file(&row).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 160)).await
                    {
                        out.send(Ev::StickerImg(doc, Box::new(img)));
                    }
                });
            }
            Cmd::RefreshDialogs => {
                spawn_refresh_dialogs(
                    &engine,
                    &dialog_refresh_tx,
                    &refresh_in_flight,
                    &mut last_dialog_refresh,
                    false,
                );
            }
            Cmd::SenderAvatar(msg_id, sender_id) => {
                let Some(d) = current.and_then(|i| dialogs.get(i)) else { continue };
                let (eng, out, limit, peer) = (engine.clone(), out.clone(), avatar_limit.clone(), d.peer);
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    let Some(r) = eng.sender_ref(peer, msg_id).await else { return };
                    if let Ok(Some(path)) = eng.avatar(r).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 96)).await
                    {
                        out.send(Ev::Avatar(sender_id, Box::new(img)));
                    }
                });
            }
            Cmd::Gifs => {
                let (eng, out, docs_arc) = (engine.clone(), out.clone(), sticker_docs.clone());
                tokio::spawn(async move {
                    match eng.saved_gifs().await {
                        Ok(list) => {
                            if let Ok(mut d) = docs_arc.lock() {
                                for g in &list {
                                    d.insert(g.id, g.clone());
                                }
                            }
                            out.send(Ev::Gifs(list));
                        }
                        Err(e) => out.err(format!("GIF: {e}")),
                    }
                    out.send(Ev::GifsLoaded);
                });
            }
            Cmd::CreateChannel(title, about, broadcast) => {
                let (eng, out, tx, flag) = (
                    engine.clone(),
                    out.clone(),
                    dialog_refresh_tx.clone(),
                    std::sync::Arc::clone(&refresh_in_flight),
                );
                tokio::spawn(async move {
                    match eng.create_channel(&title, &about, broadcast).await {
                        Ok(()) => {
                            out.send(Ev::Status(format!("Создано: «{title}»")));
                            fetch_and_push_dialogs(&eng, &tx, &flag).await;
                        }
                        Err(e) => out.err(format!("Не создано: {e}")),
                    }
                });
            }
            Cmd::CreateGroup(title, members) => {
                let (eng, out, tx, flag) = (
                    engine.clone(),
                    out.clone(),
                    dialog_refresh_tx.clone(),
                    std::sync::Arc::clone(&refresh_in_flight),
                );
                tokio::spawn(async move {
                    match eng.create_group(&title, &members).await {
                        Ok(()) => {
                            out.send(Ev::Status(format!("Группа «{title}» создана")));
                            fetch_and_push_dialogs(&eng, &tx, &flag).await;
                        }
                        Err(e) => out.err(format!("Группа не создана: {e}")),
                    }
                });
            }
            Cmd::PeerInfo(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.peer_info(peer).await {
                        Ok(info) => {
                            // Фоновая предзагрузка первого трека из профиля, чтобы музыка играла за 0 мс по клику.
                            if let Some(first_track) = info.saved_music.first().cloned() {
                                let eng_bg = eng.clone();
                                tokio::spawn(async move {
                                    let _ = eng_bg.download_track(&first_track).await;
                                });
                            }
                            out.send(Ev::PeerInfo(id, Box::new(info)));
                        }
                        Err(e) => out.err(format!("Сведения о чате: {e}")),
                    }
                });
            }
            Cmd::MarkRead => {
                let Some(d) = current.and_then(|i| dialogs.get(i)) else { continue };
                let (eng, peer) = (engine.clone(), d.peer);
                tokio::spawn(async move {
                    let _ = eng.mark_read(peer).await;
                });
            }
            Cmd::EmojiSets => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.emoji_sets().await {
                        Ok(s) => out.send(Ev::EmojiSets(s)),
                        Err(e) => out.err(format!("Наборы эмодзи: {e}")),
                    }
                });
            }
            Cmd::FeaturedEmoji => {
                let (eng, out, docs_arc) = (engine.clone(), out.clone(), sticker_docs.clone());
                tokio::spawn(async move {
                    match eng.featured_emoji_sets().await {
                        Ok(s) => {
                            if let Ok(mut d) = docs_arc.lock() {
                                for set in &s {
                                    if let Some(c) = &set.cover {
                                        d.insert(c.id, (**c).clone());
                                    }
                                }
                            }
                            out.send(Ev::FeaturedEmoji(s));
                        }
                        Err(e) => out.err(format!("Рекомендуемые эмодзи: {e}")),
                    }
                });
            }
            Cmd::InstallSet(set) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.install_set(&set).await {
                        Ok(()) => {
                            out.send(Ev::Status(format!("Добавлен набор «{}»", set.title)));
                            if let Ok(s) = eng.emoji_sets().await {
                                out.send(Ev::EmojiSets(s));
                            }
                            if let Ok(s) = eng.featured_emoji_sets().await {
                                out.send(Ev::FeaturedEmoji(s));
                            }
                        }
                        Err(e) => out.err(format!("Набор не добавлен: {e}")),
                    }
                });
            }
            Cmd::LoadSetRow(set) => {
                let (eng, out, docs_arc) = (engine.clone(), out.clone(), sticker_docs.clone());
                tokio::spawn(async move {
                    match eng.sticker_set(&set).await {
                        Ok(list) => {
                            if let Ok(mut d) = docs_arc.lock() {
                                for st in &list {
                                    d.insert(st.id, st.clone());
                                }
                            }
                            out.send(Ev::StickerList(set.id, list));
                        }
                        Err(e) => out.err(format!("Набор: {e}")),
                    }
                });
            }
            Cmd::SetIcon(set) => {
                let (eng, out, limit) = (engine.clone(), out.clone(), emoji_limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.set_icon(&set).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 112)).await
                    {
                        out.send(Ev::StickerImg(set.id, Box::new(img)));
                    }
                });
            }
            Cmd::SendSticker(doc, reply) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let Some(row) = sticker_docs.lock().ok().and_then(|g| g.get(&doc).cloned()) else {
                    continue;
                };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.send_sticker(peer, &row, reply).await {
                        Ok(sent) => out.send(Ev::SentConfirm(did, 0, Box::new(sent))),
                        Err(e) => out.err(format!("Стикер не отправлен: {e}")),
                    }
                });
            }
            Cmd::LogOut => {
                match connected.client.sign_out().await {
                    Ok(_) => {}
                    Err(e) => {
                        // Сессия на сервере уже недействительна — локально всё равно чистим.
                        out.err(format!("Выход: {e}"));
                    }
                }
                let _ = std::fs::write(home.join("logout.flag"), b"1");
                let mut s = engine.settings();
                if let Some(a) = s.accounts.iter_mut().find(|a| a.slot == s.active_account) {
                    a.display_name.clear();
                    a.username.clear();
                    a.phone.clear();
                    a.user_id = 0;
                }
                let _ = std::fs::write(&settings_path, s.to_json());
                connected.pool_handle.quit();
                tokio::time::sleep(std::time::Duration::from_millis(120)).await;
                out.send(Ev::ResetSession);
                break true;
            }
            Cmd::ForceRead => {
                let Some(i) = current else { continue };
                let Some(peer) = dialogs.get(i).map(|d| d.peer) else { continue };
                if let Some(d) = dialogs.get_mut(i) {
                    d.unread = 0;
                }
                spawn_save_disk_cache(&home, &cached_me, &dialogs, &cached_folders);
                out.send(Ev::Dialogs(dialogs.clone()));
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.force_read(peer).await {
                        Ok(()) => out.send(Ev::Status("Отмечено прочитанным на сервере".into())),
                        Err(e) => out.err(format!("Не отмечено: {e}")),
                    }
                });
            }
            Cmd::ForceReadAll => {
                let mut peers: Vec<grammers_session::types::PeerRef> = dialogs
                    .iter()
                    .filter(|d| d.unread > 0)
                    .map(|d| d.peer)
                    .collect();
                if let Some(cur_peer) = current.and_then(|i| dialogs.get(i)).map(|d| d.peer)
                    && !peers.iter().any(|p| p.id == cur_peer.id)
                {
                    peers.push(cur_peer);
                }
                for d in &mut dialogs {
                    d.unread = 0;
                }
                spawn_save_disk_cache(&home, &cached_me, &dialogs, &cached_folders);
                out.send(Ev::Dialogs(dialogs.clone()));
                if peers.is_empty() {
                    out.send(Ev::Status("Все чаты уже прочитаны на сервере".into()));
                    continue;
                }
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    let count = eng.force_read_many(&peers).await;
                    out.send(Ev::Status(format!("Прочитано на сервере: {count} чатов")));
                });
            }
            Cmd::Edit(id, text) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.edit(peer, id, &text).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Не изменено: {e}")),
                    }
                });
            }
            Cmd::Delete { id, backup } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    let res = if let Some((text, date, outgoing, sender)) = backup {
                        eng.delete_with_backup(peer, &[(id, text, date, outgoing, sender)])
                            .await
                    } else {
                        eng.delete(peer, &[id]).await
                    };
                    match res {
                        Ok(_) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Не удалено: {e}")),
                    }
                });
            }
            Cmd::Forward(id, to, no_author) => {
                let Some(i) = current else { continue };
                let (Some(from), Some(dest)) = (dialogs.get(i), dialogs.get(to)) else { continue };
                let (eng, out, from_peer, dest_peer, dest_name) = (
                    engine.clone(),
                    out.clone(),
                    from.peer,
                    dest.peer,
                    dest.name.clone(),
                );
                tokio::spawn(async move {
                    let res = if no_author {
                        eng.forward_no_author(from_peer, dest_peer, id).await
                    } else {
                        eng.forward(from_peer, dest_peer, &[id]).await
                    };
                    match res {
                        Ok(()) => out.send(Ev::Status(format!(
                            "Переслано{}: {}",
                            if no_author { " без автора" } else { "" },
                            dest_name
                        ))),
                        Err(e) => out.err(format!("Не переслано: {e}")),
                    }
                });
            }
            Cmd::Translate(id, text) => {
                let (eng, out) = (engine.clone(), out.clone());
                let lang = eng.settings().language.translate_to;
                tokio::spawn(async move {
                    match eng.translate_text(&text, if lang.is_empty() { "ru" } else { &lang }).await {
                        Ok(tr) if !tr.trim().is_empty() => out.send(Ev::Translated(id, tr)),
                        Ok(_) => out.err("Перевод пуст"),
                        Err(e) => out.err(format!("Перевод: {e}")),
                    }
                });
            }
            Cmd::Pin(id, pinned) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.pin(peer, id, pinned).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Закрепление: {e}")),
                    }
                });
            }
            Cmd::React(id, emoji) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.react(peer, id, &emoji).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Реакция: {e}")),
                    }
                });
            }
            Cmd::Search(q) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    if q.trim().is_empty() {
                        reload(&eng, &out, peer, i).await;
                        return;
                    }
                    match eng.search(peer, q.trim(), 100).await {
                        Ok(rows) => out.send(Ev::Found(i, rows)),
                        Err(e) => out.err(format!("Поиск: {e}")),
                    }
                });
            }
            Cmd::Older(before) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.history_before(peer, before, 60).await {
                        Ok(rows) => out.send(Ev::Older(i, rows)),
                        Err(e) => out.err(format!("История: {e}")),
                    }
                });
            }
            Cmd::Avatar(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, limit, peer, id) = (engine.clone(), out.clone(), avatar_limit.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.avatar(peer).await
                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 112)).await
                    {
                        out.send(Ev::Avatar(id, Box::new(img)));
                    }
                });
            }
            Cmd::Preview(i, msg_id) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, limit, peer) = (engine.clone(), out.clone(), limit.clone(), d.peer);
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    if let Ok(Some(path)) = eng.preview_path(peer, msg_id).await
                        && let Ok(Some(img)) =
                            tokio::task::spawn_blocking(move || decode(&path)).await
                    {
                        out.send(Ev::Image(i, msg_id, Box::new(img)));
                    }
                });
            }
            Cmd::OpenFile(msg_id) | Cmd::Download(msg_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                let cancel = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                {
                    let mut f = fetches.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
                    if f.contains_key(&msg_id) {
                        continue;
                    }
                    f.insert(msg_id, cancel.clone());
                }
                let fetches = fetches.clone();
                let rt_handle = tokio::runtime::Handle::current();
                tokio::spawn(async move {
                    let o2 = out.clone();
                    let stream_slot: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<crate::stream::StreamState>>>> =
                        std::sync::Arc::default();
                    let s_prog = stream_slot.clone();
                    let s_open = stream_slot.clone();
                    let o3 = out.clone();
                    let streamed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let streamed_c = streamed.clone();
                    let last_prog_ms = std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0));
                    let prog_started = std::time::Instant::now();
                    let res = eng
                        .fetch_media_stream(
                            peer,
                            msg_id,
                            !play,
                            &cancel,
                            move |done, total| {
                                if let Some(st) = s_prog.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
                                    st.done.store(done, std::sync::atomic::Ordering::Release);
                                }
                                let now_ms = prog_started.elapsed().as_millis() as u64;
                                let prev = last_prog_ms.load(std::sync::atomic::Ordering::Relaxed);
                                if done >= total || now_ms.saturating_sub(prev) >= 80 {
                                    last_prog_ms.store(now_ms, std::sync::atomic::Ordering::Relaxed);
                                    o2.send(Ev::Progress(msg_id, done, total));
                                }
                            },
                            move |part, dest, total, ext| {
                                if play
                                    && cfg!(windows)
                                    && matches!(ext.to_ascii_lowercase().as_str(), "mp4" | "webm" | "mov" | "mkv" | "m4v" | "gif" | "ogv")
                                {
                                    let st = crate::stream::StreamState::new(part.to_path_buf(), dest.to_path_buf(), total, ext);
                                    st.done.store(384 * 1024, std::sync::atomic::Ordering::Release);
                                    *s_open.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(st.clone());
                                    streamed_c.store(true, std::sync::atomic::Ordering::Relaxed);
                                    let label = dest.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_default();
                                    let o_url = o3.clone();
                                    rt_handle.spawn(async move {
                                        if let Some(url) = crate::stream::start(st).await {
                                            o_url.send(Ev::StreamVideo(url, label));
                                        }
                                    });
                                }
                            },
                        )
                        .await;
                    fetches.lock().unwrap_or_else(std::sync::PoisonError::into_inner).remove(&msg_id);
                    out.send(Ev::FetchEnd(msg_id));
                    if let Some(st) = stream_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
                        if matches!(res, Ok(rg_core::engine::Fetched::Ready(_))) {
                            st.finished.store(true, std::sync::atomic::Ordering::Release);
                        } else {
                            st.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    match res {
                        Ok(rg_core::engine::Fetched::Ready(p)) => {
                            let ext = p.extension().and_then(|e| e.to_str()).map(str::to_lowercase).unwrap_or_default();
                            if play {
                                out.send(Ev::Cached(vec![msg_id]));
                            }
                            if play && cfg!(windows) && matches!(ext.as_str(), "mp4" | "webm" | "mov" | "mkv" | "m4v" | "gif" | "ogv") {
                                if !streamed.load(std::sync::atomic::Ordering::Relaxed) {
                                    out.send(Ev::PlayVideo(p));
                                }
                            } else if play && cfg!(windows) && matches!(ext.as_str(), "mp3" | "m4a" | "ogg" | "oga" | "opus" | "flac" | "wav" | "aac") {
                                let title = p.file_name().map(|n| n.to_string_lossy().into_owned()).unwrap_or_else(|| "Аудио".to_owned());
                                out.send(Ev::PlayAudio(p, title, 0));
                            } else if play {
                                rg_core::media::open_downloaded(&p);
                                out.send(Ev::Status(format!("Открыто: {}", p.display())));
                            } else {
                                out.send(Ev::Status(format!("Сохранено: {}", p.display())));
                            }
                        }
                        Ok(rg_core::engine::Fetched::Cancelled) => {}
                        Ok(rg_core::engine::Fetched::Missing) => out.err("Вложение недоступно"),
                        Err(e) => out.err(format!("Загрузка: {e}")),
                    }
                });
            }
            Cmd::AutoFetchGif(msg_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, limit) = (engine.clone(), out.clone(), d.peer, limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    let cancel = std::sync::atomic::AtomicBool::new(false);
                    if let Ok(rg_core::engine::Fetched::Ready(p)) = eng.fetch_media(peer, msg_id, false, &cancel, |_, _| {}).await {
                        out.send(Ev::Cached(vec![msg_id]));
                        out.send(Ev::GifReady(msg_id, p));
                    }
                });
            }
            Cmd::EmojiKeywords => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    let (ru, en) = tokio::join!(eng.emoji_keywords("ru"), eng.emoji_keywords("en"));
                    let mut all = ru.unwrap_or_default();
                    all.extend(en.unwrap_or_default());
                    if !all.is_empty() {
                        out.send(Ev::EmojiKeywords(all));
                    }
                });
            }
            Cmd::SaveFolder(folder) => {
                let (eng, out, h, me_c, d_c) = (
                    engine.clone(),
                    out.clone(),
                    home.clone(),
                    cached_me.clone(),
                    dialogs.clone(),
                );
                tokio::spawn(async move {
                    match eng.save_folder(&folder).await {
                        Ok(()) => {
                            out.send(Ev::Status(format!("Папка «{}» сохранена", folder.title)));
                            if let Ok(f) = eng.folders().await {
                                if !eng.settings().privacy_shield.ephemeral_local_cache {
                                    spawn_save_disk_cache(&h, &me_c, &d_c, &f);
                                }
                                out.send(Ev::Folders(f));
                            }
                        }
                        Err(e) => out.err(format!("Папка не сохранена: {e}")),
                    }
                });
            }
            Cmd::DeleteFolder(id) => {
                let (eng, out, h, me_c, d_c) = (
                    engine.clone(),
                    out.clone(),
                    home.clone(),
                    cached_me.clone(),
                    dialogs.clone(),
                );
                tokio::spawn(async move {
                    match eng.delete_folder(id).await {
                        Ok(()) => {
                            out.send(Ev::Status("Папка удалена".into()));
                            if let Ok(f) = eng.folders().await {
                                if !eng.settings().privacy_shield.ephemeral_local_cache {
                                    spawn_save_disk_cache(&h, &me_c, &d_c, &f);
                                }
                                out.send(Ev::Folders(f));
                            }
                        }
                        Err(e) => out.err(format!("Папка не удалена: {e}")),
                    }
                });
            }
            Cmd::GlobalSearch(q) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.global_search(&q, 15).await {
                        Ok(list) => out.send(Ev::GlobalFound(list)),
                        Err(_) => out.send(Ev::GlobalFound(Vec::new())),
                    }
                });
            }
            Cmd::CancelFetch(msg_id) => {
                if let Some(c) = fetches.lock().unwrap_or_else(std::sync::PoisonError::into_inner).get(&msg_id) {
                    c.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            Cmd::CachedMedia(ids) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::task::spawn_blocking(move || {
                    let found = eng.cached_media_batch(peer, &ids);
                    for id in &found {
                        if let Some(p) = eng.cached_media(peer, *id) {
                            out.send(Ev::GifReady(*id, p));
                        }
                    }
                    if !found.is_empty() {
                        out.send(Ev::Cached(found));
                    }
                });
            }
            Cmd::Account => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.account_overview().await {
                        Ok(a) => out.send(Ev::Account(Box::new(a))),
                        Err(e) => out.err(format!("Настройки аккаунта: {e}")),
                    }
                });
            }
            Cmd::UpdateProfile(first, last, about) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.update_profile(&first, &last, &about).await {
                        Ok(()) => {
                            out.send(Ev::Status("Профиль сохранён".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Профиль не сохранён: {e}")),
                    }
                });
            }
            Cmd::UpdateUsername(username) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.update_username(&username).await {
                        Ok(()) => {
                            out.send(Ev::Status("Имя пользователя обновлено".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Имя пользователя: {e}")),
                    }
                });
            }
            Cmd::SetPrivacy(key, level) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_privacy(key, level).await {
                        Ok(()) => {
                            out.send(Ev::Status(format!("{}: {}", key.title(), level.label())));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Приватность: {e}")),
                    }
                });
            }
            Cmd::SetNotifyScope(scope, enabled, preview) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_notify_scope(scope, enabled, preview).await {
                        Ok(()) => {
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Уведомления: {e}")),
                    }
                });
            }
            Cmd::SetContactSignupNotify(enabled) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_contact_signup_notify(enabled).await {
                        Ok(()) => {
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Уведомления о контактах: {e}")),
                    }
                });
            }
            Cmd::SetDefaultHistoryTtl(period) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_default_history_ttl(period).await {
                        Ok(()) => {
                            out.send(Ev::Status("Таймер автоудаления обновлён".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Автоудаление: {e}")),
                    }
                });
            }
            Cmd::SetAccountTtlDays(days) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_account_ttl_days(days).await {
                        Ok(()) => {
                            out.send(Ev::Status("Срок хранения аккаунта обновлён".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Срок аккаунта: {e}")),
                    }
                });
            }
            Cmd::ResetAuthorization(hash) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.reset_authorization(hash).await {
                        Ok(()) => {
                            out.send(Ev::Status("Сеанс завершён".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Завершение сеанса: {e}")),
                    }
                });
            }
            Cmd::ResetOtherAuthorizations => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.reset_other_authorizations().await {
                        Ok(()) => {
                            out.send(Ev::Status("Все остальные сеансы завершены".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Завершение сеансов: {e}")),
                    }
                });
            }
            Cmd::SetAuthorizationTtl(days) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_authorization_ttl(days).await {
                        Ok(()) => {
                            out.send(Ev::Status("Срок неактивных сеансов обновлён".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Срок сеансов: {e}")),
                    }
                });
            }
            Cmd::UnblockUser(id, access_hash) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.unblock_user(id, access_hash).await {
                        Ok(()) => {
                            out.send(Ev::Status("Пользователь разблокирован".into()));
                            if let Ok(a) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(a)));
                            }
                        }
                        Err(e) => out.err(format!("Разблокировка: {e}")),
                    }
                });
            }
            Cmd::ClearPaymentInfo => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.clear_payment_info().await {
                        Ok(()) => out.send(Ev::Status("Платёжные данные и адреса очищены".into())),
                        Err(e) => out.err(format!("Очистка платёжных данных: {e}")),
                    }
                });
            }
            Cmd::PlayTrack(track) => {
                let (eng, out) = (engine.clone(), out.clone());
                let title = track.display_title();
                let dur = track.duration;
                out.send(Ev::Status(format!("Загрузка трека «{title}»…")));
                let rt_handle = tokio::runtime::Handle::current();
                tokio::spawn(async move {
                    let stream_slot: std::sync::Arc<std::sync::Mutex<Option<std::sync::Arc<crate::stream::StreamState>>>> =
                        std::sync::Arc::default();
                    let s_prog = stream_slot.clone();
                    let streamed = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
                    let streamed_c = streamed.clone();
                    let out_stream = out.clone();
                    let title_stream = title.clone();
                    let res = eng
                        .download_track_streaming(&track, move |done, total, part, dest| {
                            if let Some(st) = s_prog.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
                                st.done.store(done, std::sync::atomic::Ordering::Release);
                                return;
                            }
                            if cfg!(windows)
                                && done >= 128 * 1024
                                && done < total
                                && !streamed_c.swap(true, std::sync::atomic::Ordering::SeqCst)
                            {
                                let ext = dest.extension().and_then(|e| e.to_str()).unwrap_or("mp3");
                                let st = crate::stream::StreamState::new(part.to_path_buf(), dest.to_path_buf(), total, ext);
                                st.done.store(done, std::sync::atomic::Ordering::Release);
                                *s_prog.lock().unwrap_or_else(std::sync::PoisonError::into_inner) = Some(st.clone());
                                let o_url = out_stream.clone();
                                let t_url = title_stream.clone();
                                rt_handle.spawn(async move {
                                    if let Some(url) = crate::stream::start(st).await {
                                        o_url.send(Ev::Status(String::new()));
                                        o_url.send(Ev::StreamAudio(url, t_url, dur));
                                    }
                                });
                            }
                        })
                        .await;
                    if let Some(st) = stream_slot.lock().unwrap_or_else(std::sync::PoisonError::into_inner).as_ref() {
                        if res.is_ok() {
                            st.finished.store(true, std::sync::atomic::Ordering::Release);
                        } else {
                            st.cancelled.store(true, std::sync::atomic::Ordering::Relaxed);
                        }
                    }
                    match res {
                        Ok(path) => {
                            out.send(Ev::Status(String::new()));
                            if !streamed.load(std::sync::atomic::Ordering::SeqCst) {
                                out.send(Ev::PlayAudio(path, title, dur));
                            }
                        }
                        Err(e) => out.err(format!("Музыка: {e}")),
                    }
                });
            }
            Cmd::TranslateBatch(items, lang) => {
                let (eng, out) = (engine.clone(), out.clone());
                let to_lang = if lang.is_empty() { "ru".to_owned() } else { lang };
                out.send(Ev::Status("Перевод чата…".into()));
                tokio::spawn(async move {
                    for (id, text) in items.into_iter().take(25) {
                        if let Ok(tr) = eng.translate_text(&text, &to_lang).await
                            && !tr.trim().is_empty()
                        {
                            out.send(Ev::Translated(id, tr));
                        }
                    }
                    out.send(Ev::Status(String::new()));
                });
            }
            Cmd::BotCallback(idx, msg_id, data) => {
                let Some(d) = dialogs.get(idx) else { continue };
                let (eng, out, peer, title) = (engine.clone(), out.clone(), d.peer, d.name.clone());
                out.send(Ev::Status("Выполнение команды кнопки…".into()));
                tokio::spawn(async move {
                    match eng.press_bot_callback(peer, msg_id, data).await {
                        Ok((msg, url, alert)) => {
                            if let Some(m) = msg {
                                if alert {
                                    out.send(Ev::BotAlert(m));
                                    out.send(Ev::Status(String::new()));
                                } else {
                                    out.send(Ev::Status(m));
                                }
                            } else {
                                out.send(Ev::Status(String::new()));
                            }
                            if let Some(u) = url {
                                out.send(Ev::MiniAppReady { title, url: u });
                            }
                            reload(&eng, &out, peer, idx).await;
                        }
                        Err(e) => out.err(format!("Кнопка бота: {e}")),
                    }
                });
            }
            Cmd::OpenMiniApp {
                idx,
                msg_id,
                title,
                url,
                start_param,
                simple,
            } => {
                let Some(d) = dialogs.get(idx) else { continue };
                let spoof = engine.settings().ayu.spoof_webview_as_android;
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                out.send(Ev::Status(format!("Запуск Mini App «{title}»…")));
                tokio::spawn(async move {
                    match eng
                        .request_webview_url(peer, msg_id, url, start_param, simple, spoof)
                        .await
                    {
                        Ok(final_url) => {
                            out.send(Ev::Status(String::new()));
                            out.send(Ev::MiniAppReady {
                                title,
                                url: final_url,
                            });
                        }
                        Err(e) => out.err(format!("Mini App: {e}")),
                    }
                });
            }
            Cmd::OpenTgLink(link) => {
                use rg_core::engine::TgLink as L;
                match link {
                    L::Settings => {}
                    L::Proxy(url) => {
                        let _ = std::fs::write(home.join("proxy.txt"), &url);
                        out.send(Ev::Status(format!("Прокси сохранён: {url} (перезапустите для применения)")));
                    }
                    L::PrivatePost { channel_id, msg_id } => {
                        let bot_id = -1_000_000_000_000 - channel_id;
                        out.send(Ev::NavigateTo {
                            id: bot_id,
                            post: msg_id,
                        });
                    }
                    L::UserId(uid) => {
                        out.send(Ev::NavigateTo {
                            id: uid,
                            post: None,
                        });
                    }
                    L::Invite(hash) => {
                        out.send(Ev::Status("Проверка приглашения…".into()));
                        match engine.open_or_import_invite(&hash).await {
                            Ok((peer, id, name)) => {
                                if !dialogs.iter().any(|d| d.id == id) {
                                    dialogs.insert(
                                        0,
                                        DialogRow {
                                            peer,
                                            id,
                                            name,
                                            last: String::new(),
                                            date: 0,
                                            unread: 0,
                                            pinned: false,
                                            muted: false,
                                            notify_explicit: None,
                                            saved: false,
                                            archived: false,
                                            kind: if id < -1_000_000_000_000 {
                                                DialogKind::Channel
                                            } else {
                                                DialogKind::Group
                                            },
                                            contact: false,
                                            read_out: 0,
                                            emoji_status: None,
                                            verified: false,
                                            premium: false,
                                            unread_mentions: 0,
                                            unread_reactions: 0,
                                        },
                                    );
                                    out.send(Ev::Dialogs(dialogs.clone()));
                                }
                                out.send(Ev::Status(String::new()));
                                out.send(Ev::NavigateTo { id, post: None });
                            }
                            Err(e) => out.err(format!("Приглашение: {e}")),
                        }
                    }
                    L::ChatlistInvite(slug) => {
                        let (eng, out) = (engine.clone(), out.clone());
                        out.send(Ev::Status("Проверка папки…".into()));
                        tokio::spawn(async move {
                            match eng.check_folder_invite(&slug).await {
                                Ok(prev) => {
                                    out.send(Ev::Status(String::new()));
                                    out.send(Ev::FolderInvitePreview(prev));
                                }
                                Err(e) => out.err(e),
                            }
                        });
                    }
                    L::Resolve {
                        domain,
                        post,
                        start,
                        startapp,
                    } => {
                        out.send(Ev::Status(format!("Переход к @{domain}…")));
                        match engine.resolve_username(&domain).await {
                            Ok((peer, id, name)) => {
                                if !dialogs.iter().any(|d| d.id == id) {
                                    dialogs.insert(
                                        0,
                                        DialogRow {
                                            peer,
                                            id,
                                            name: name.clone(),
                                            last: String::new(),
                                            date: 0,
                                            unread: 0,
                                            pinned: false,
                                            muted: false,
                                            notify_explicit: None,
                                            saved: false,
                                            archived: false,
                                            kind: if id < -1_000_000_000_000 {
                                                DialogKind::Channel
                                            } else if id < 0 {
                                                DialogKind::Group
                                            } else if domain.to_ascii_lowercase().ends_with("bot") {
                                                DialogKind::Bot
                                            } else {
                                                DialogKind::User
                                            },
                                            contact: false,
                                            read_out: 0,
                                            emoji_status: None,
                                            verified: false,
                                            premium: false,
                                            unread_mentions: 0,
                                            unread_reactions: 0,
                                        },
                                    );
                                    out.send(Ev::Dialogs(dialogs.clone()));
                                }
                                out.send(Ev::Status(String::new()));
                                out.send(Ev::NavigateTo { id, post });
                                if let Some(param) = start {
                                    let cmd_txt = if param.is_empty() {
                                        "/start".to_owned()
                                    } else {
                                        format!("/start {param}")
                                    };
                                    let _ = engine.send_reply(peer, &cmd_txt, None).await;
                                    if let Some(pos) = dialogs.iter().position(|d| d.id == id) {
                                        reload(&engine, &out, peer, pos).await;
                                    }
                                }
                                if let Some(app_param) = startapp {
                                    let spoof = engine.settings().ayu.spoof_webview_as_android;
                                    if let Ok(u) = engine
                                        .request_webview_url(peer, None, None, Some(app_param), true, spoof)
                                        .await
                                    {
                                        out.send(Ev::MiniAppReady { title: name, url: u });
                                    }
                                }
                            }
                            Err(e) => out.err(format!("Ссылка @{domain}: {e}")),
                        }
                    }
                }
            }
            Cmd::WipeExternalData(scope) => {
                let (eng, out, home_c) = (engine.clone(), out.clone(), home.clone());
                out.send(Ev::Status("Выполняется тотальная очистка переписок и следов…".into()));
                tokio::spawn(async move {
                    if scope.wipe_local_storage {
                        let _ = std::fs::remove_file(home_c.join("dialogs_cache.json"));
                        let _ = std::fs::remove_dir_all(home_c.join("cache"));
                    }
                    match eng.wipe_external_data(scope).await {
                        Ok(report) => out.send(Ev::Status(report)),
                        Err(e) => out.err(format!("Очистка: {e}")),
                    }
                });
            }
            Cmd::SwitchAccount(slot) => {
                let mut s = engine.settings();
                if s.active_account == slot {
                    continue;
                }
                rg_core::diag::info("account", &format!("switch {} -> {slot}", s.active_account));
                s.active_account = slot;
                let _ = std::fs::write(&settings_path, s.to_json());
                connected.pool_handle.quit();
                tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                out.send(Ev::ResetSession);
                break true;
            }
            Cmd::AddIsolatedAccount { label, proxy_url } => {
                let mut s = engine.settings();
                let next_slot = s.accounts.iter().map(|a| a.slot).max().unwrap_or(0) + 1;
                let entropy = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                let prof = rg_settings::AccountProfile::isolated(next_slot, label, proxy_url, entropy);
                s.accounts.push(prof);
                s.active_account = next_slot;
                let _ = std::fs::write(&settings_path, s.to_json());
                connected.pool_handle.quit();
                tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                out.send(Ev::ResetSession);
                break true;
            }
            Cmd::AddAccountAuto => {
                let mut s = engine.settings();
                let next_slot = s.accounts.iter().map(|a| a.slot).max().unwrap_or(0) + 1;
                let entropy = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map_or(0, |d| d.as_nanos() as u64);
                let prof = rg_settings::AccountProfile::isolated(next_slot, String::new(), String::new(), entropy);
                s.accounts.push(prof);
                s.active_account = next_slot;
                let _ = std::fs::write(&settings_path, s.to_json());
                connected.pool_handle.quit();
                tokio::time::sleep(std::time::Duration::from_millis(15)).await;
                out.send(Ev::ResetSession);
                break true;
            }
            Cmd::PurgeLocal(msg_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did, h_dir) =
                    (engine.clone(), out.clone(), d.peer, d.id, home.clone());
                tokio::spawn(async move {
                    eng.purge_local_deleted(&peer, msg_id).await;
                    if let Some(mut rows) = load_history_cache(&h_dir, did) {
                        rows.retain(|m| m.id != msg_id);
                        save_history_cache(&h_dir, did, &rows);
                    }
                    out.send(Ev::LocalPurged(did, msg_id));
                });
            }
            Cmd::PurgeAllDeleted => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did, h_dir) =
                    (engine.clone(), out.clone(), d.peer, d.id, home.clone());
                tokio::spawn(async move {
                    eng.purge_all_local_deleted(&peer).await;
                    if let Some(mut rows) = load_history_cache(&h_dir, did) {
                        rows.retain(|m| !m.deleted);
                        save_history_cache(&h_dir, did, &rows);
                    }
                    out.send(Ev::LocalPurgedAll(did));
                });
            }
            Cmd::RemoveAccountSlot(slot) => {
                if slot == 0 {
                    continue;
                }
                let mut s = engine.settings();
                s.accounts.retain(|a| a.slot != slot);
                let _ = std::fs::remove_dir_all(root_home.join("accounts").join(format!("slot_{slot}")));
                let need_restart = s.active_account == slot;
                if need_restart {
                    s.active_account = 0;
                }
                let json = s.to_json();
                engine.update_settings(|cur| *cur = s.clone());
                let _ = std::fs::write(&settings_path, json);
                out.send(Ev::Settings(Box::new(s)));
                if need_restart {
                    connected.pool_handle.quit();
                    tokio::time::sleep(std::time::Duration::from_millis(180)).await;
                    out.send(Ev::ResetSession);
                    break true;
                }
            }
            Cmd::PublishStory {
                path,
                caption,
                privacy,
                period_secs,
                pinned,
                noforwards,
            } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    out.send(Ev::Status("Публикация истории…".to_owned()));
                    match eng
                        .send_story(&path, &caption, privacy, period_secs, pinned, noforwards)
                        .await
                    {
                        Ok(()) => {
                            out.send(Ev::Status("История опубликована".to_owned()));
                            if let Ok(st) = eng.stories().await {
                                out.send(Ev::Stories(st));
                            }
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::ReactStory(peer, story_id, emoji) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.react_story(peer, story_id, &emoji).await {
                        Ok(()) => out.send(Ev::Status(format!("Реакция {emoji} отправлена"))),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::LoadStoryMedia(peer, story_id) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.story_media_path(peer, story_id).await {
                        Ok(Some(path)) => {
                            if let Ok(Some(img)) =
                                tokio::task::spawn_blocking(move || decode_story(&path)).await
                            {
                                out.send(Ev::StoryImage(story_id, Box::new(img)));
                            } else {
                                out.send(Ev::StoryError(
                                    story_id,
                                    "Видео-история (нажмите «Открыть файл» или обновите)".to_owned(),
                                ));
                            }
                        }
                        Ok(None) => out.send(Ev::StoryError(
                            story_id,
                            "История недоступна или истекла".to_owned(),
                        )),
                        Err(e) => {
                            rg_core::diag::warn("stories", &format!("story {story_id} media err: {e}"));
                            out.send(Ev::StoryError(story_id, format!("Загрузка истории: {e}")));
                        }
                    }
                });
            }
            Cmd::SendVoice {
                path,
                duration,
                waveform,
                reply_to,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    out.send(Ev::Status("Отправка голосового…".to_owned()));
                    let res = eng.send_voice(peer, &path, duration, &waveform, reply_to).await;
                    let _ = std::fs::remove_file(&path);
                    match res {
                        Ok(()) => {
                            out.send(Ev::Status("Голосовое отправлено".to_owned()));
                            reload(&eng, &out, peer, i).await;
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AdminEditAdmin { peer, user, rights, rank } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.edit_admin(peer, user, rights, &rank).await {
                        Ok(()) => out.send(Ev::Status("Права администратора обновлены".to_owned())),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AdminEditBanned { peer, user, rights } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.edit_banned(peer, user, rights).await {
                        Ok(()) => out.send(Ev::Status("Ограничения участника обновлены".to_owned())),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AdminEditDefaultRights { peer, rights } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.edit_default_banned_rights(peer, rights).await {
                        Ok(()) => out.send(Ev::Status("Общие права группы сохранены".to_owned())),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AdminLoadLog { peer, query } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.admin_log(peer, &query, 50).await {
                        Ok(rows) => out.send(Ev::AdminLog(rows)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AdminLoadInvites(peer) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.chat_invites(peer).await {
                        Ok(rows) => out.send(Ev::AdminInvites(rows)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AdminCreateInvite {
                peer,
                title,
                expire_date,
                usage_limit,
                request_needed,
            } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng
                        .create_invite(peer, &title, expire_date, usage_limit, request_needed)
                        .await
                    {
                        Ok(inv) => {
                            out.send(Ev::Status(format!("Создана ссылка: {}", inv.link)));
                            if let Ok(rows) = eng.chat_invites(peer).await {
                                out.send(Ev::AdminInvites(rows));
                            }
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AdminRevokeInvite { peer, link } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.revoke_invite(peer, &link).await {
                        Ok(()) => {
                            out.send(Ev::Status("Ссылка отозвана".to_owned()));
                            if let Ok(rows) = eng.chat_invites(peer).await {
                                out.send(Ev::AdminInvites(rows));
                            }
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::LoadSecretChats => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    out.send(Ev::SecretChats(eng.secret_chats().await));
                });
            }
            Cmd::StartSecretChat(peer, name) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.request_secret_chat(peer, &name).await {
                        Ok(rec) => {
                            out.send(Ev::Status(format!("Запрос секретного чата отправлен: {}", rec.peer_name)));
                            out.send(Ev::SecretChats(eng.secret_chats().await));
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::OpenSecretChat(chat_id) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    out.send(Ev::SecretMessages(chat_id, eng.secret_messages(chat_id).await));
                });
            }
            Cmd::SendSecretText(chat_id, text) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.send_secret_text(chat_id, &text).await {
                        Ok(_) => out.send(Ev::SecretMessages(chat_id, eng.secret_messages(chat_id).await)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::SetSecretTtl(chat_id, ttl_secs) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_secret_chat_ttl(chat_id, ttl_secs).await {
                        Ok(()) => {
                            out.send(Ev::SecretChats(eng.secret_chats().await));
                            out.send(Ev::Status(if ttl_secs == 0 {
                                "Таймер самоуничтожения отключён".to_owned()
                            } else {
                                format!("Таймер самоуничтожения: {ttl_secs} с")
                            }));
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::DiscardSecretChat(chat_id) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    let _ = eng.discard_secret_chat(chat_id).await;
                    out.send(Ev::SecretChats(eng.secret_chats().await));
                    out.send(Ev::Status("Секретный чат завершён и стёрт".to_owned()));
                });
            }
            Cmd::StartCall { peer, name, video } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.start_call(peer, video).await {
                        Ok((call_id, access_hash)) => out.send(Ev::CallStarted {
                            call_id,
                            access_hash,
                            peer_name: name,
                            video,
                        }),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::AcceptCall {
                call_id,
                access_hash,
                video: _,
            } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Err(e) = eng.accept_call(call_id, access_hash).await {
                        out.err(e);
                    }
                });
            }
            Cmd::ConfirmCall {
                call_id,
                access_hash,
                g_b,
            } => {
                let (eng, out, cms) = (engine.clone(), out.clone(), call_media_slot.clone());
                let relay_only = eng.settings().privacy_shield.block_activity_telemetry;
                tokio::spawn(async move {
                    match eng.confirm_call(call_id, access_hash, &g_b).await {
                        Ok((emojis, shared_key, endpoints, p2p_allowed)) => {
                            out.send(Ev::CallEmojis(emojis));
                            if let Ok(mut guard) = cms.lock() {
                                if let Some((mut old_s, mut old_p)) = guard.take() {
                                    old_p.stop();
                                    old_s.stop();
                                }
                                let (pb_tx, pb_rx) = tokio::sync::mpsc::channel(32);
                                if let Ok(sess) = rg_core::call_media::CallMediaSession::start(
                                    rg_core::call_media::CallMediaConfig {
                                        shared_key,
                                        endpoints,
                                        p2p_allowed,
                                        is_outgoing: true,
                                        video: false,
                                        relay_only_privacy: relay_only,
                                    },
                                    pb_tx,
                                ) && let Some(mic_tx) = sess.sender()
                                {
                                    let pump = crate::audio_io::CallAudioPump::start(mic_tx, pb_rx);
                                    out.send(Ev::CallMediaStats(sess.stats()));
                                    *guard = Some((sess, pump));
                                }
                            }
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::DeriveIncomingCallEmojis { call_id, g_a } => {
                if let Ok((emojis, _shared)) = engine.derive_incoming_call_emojis(call_id, &g_a) {
                    out.send(Ev::CallEmojis(emojis));
                }
            }
            Cmd::StartIncomingCallMedia {
                call_id,
                g_a,
                endpoints,
                p2p_allowed,
                video,
            } => {
                let relay_only = engine.settings().privacy_shield.block_activity_telemetry;
                if let Ok((emojis, shared_key)) = engine.derive_incoming_call_emojis(call_id, &g_a) {
                    out.send(Ev::CallEmojis(emojis));
                    if let Ok(mut guard) = call_media_slot.lock() {
                        if let Some((mut old_s, mut old_p)) = guard.take() {
                            old_p.stop();
                            old_s.stop();
                        }
                        let (pb_tx, pb_rx) = tokio::sync::mpsc::channel(32);
                        if let Ok(sess) = rg_core::call_media::CallMediaSession::start(
                            rg_core::call_media::CallMediaConfig {
                                shared_key,
                                endpoints,
                                p2p_allowed,
                                is_outgoing: false,
                                video,
                                relay_only_privacy: relay_only,
                            },
                            pb_tx,
                        ) && let Some(mic_tx) = sess.sender()
                        {
                            let pump = crate::audio_io::CallAudioPump::start(mic_tx, pb_rx);
                            out.send(Ev::CallMediaStats(sess.stats()));
                            *guard = Some((sess, pump));
                        }
                    }
                }
            }
            Cmd::SetCallMuted(muted) => {
                if let Ok(guard) = call_media_slot.lock()
                    && let Some((_, pump)) = guard.as_ref()
                {
                    pump.set_muted(muted);
                }
            }
            Cmd::PollCallStats => {
                if let Ok(guard) = call_media_slot.lock()
                    && let Some((sess, _)) = guard.as_ref()
                {
                    out.send(Ev::CallMediaStats(sess.stats()));
                }
            }
            Cmd::SendCallSignaling {
                call_id,
                access_hash,
                data,
            } => {
                let eng = engine.clone();
                tokio::spawn(async move {
                    let _ = eng.send_call_signaling(call_id, access_hash, data).await;
                });
            }
            Cmd::DiscardCall {
                call_id,
                access_hash,
                duration,
                video,
                busy,
            } => {
                if let Ok(mut guard) = call_media_slot.lock()
                    && let Some((mut sess, mut pump)) = guard.take()
                {
                    pump.stop();
                    sess.stop();
                }
                let eng = engine.clone();
                tokio::spawn(async move {
                    let _ = eng.discard_call(call_id, access_hash, duration, video, busy).await;
                });
            }
            Cmd::SendMediaEdited {
                path,
                caption,
                reply_to,
                opts,
                cleanup_temp,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                out.send(Ev::Status("Отправка медиа…".into()));
                tokio::spawn(async move {
                    match eng
                        .send_media_with_options(peer, &path, &caption, reply_to, opts)
                        .await
                    {
                        Ok(()) => {
                            out.send(Ev::Status(String::new()));
                            reload(&eng, &out, peer, i).await;
                        }
                        Err(e) => out.err(format!("Медиа не отправлено: {e}")),
                    }
                    if cleanup_temp {
                        let _ = std::fs::remove_file(&path);
                    }
                });
            }
            Cmd::ParticipantsPaged(i, offset) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok((rows, total)) = eng.channel_participants_paged(peer, offset, 200).await {
                        out.send(Ev::ParticipantsPage(id, rows, offset, total));
                    }
                });
            }
            Cmd::LoadStarsStatus => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.stars_status().await {
                        Ok(st) => out.send(Ev::StarsStatus(st)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::LoadStarGiftsCatalog => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.star_gifts_catalog().await {
                        Ok(list) => out.send(Ev::StarGiftsCatalog(list)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::LoadSavedGifts(idx_opt) => {
                let (eng, out) = (engine.clone(), out.clone());
                let target = idx_opt
                    .and_then(|i| dialogs.get(i))
                    .map(|d| (d.peer, d.id));
                tokio::spawn(async move {
                    let (peer, id) = if let Some(t) = target {
                        t
                    } else if let Ok(me) = eng.client.get_me().await {
                        (
                            me.to_ref().await.ok().flatten().unwrap_or(grammers_session::types::PeerRef {
                                id: me.id(),
                                auth: grammers_session::types::PeerAuth::default(),
                            }),
                            me.raw.id(),
                        )
                    } else {
                        return;
                    };
                    match eng.saved_star_gifts(peer).await {
                        Ok(list) => out.send(Ev::SavedGifts(id, list)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::ToggleSaveStarGift {
                msg_id,
                saved_id,
                unsave,
            } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.toggle_save_star_gift(msg_id, None, saved_id, unsave).await {
                        Ok(()) => out.send(Ev::Status(if unsave {
                            "Подарок скрыт из профиля".to_owned()
                        } else {
                            "Подарок закреплён в профиле".to_owned()
                        })),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::ConvertStarGift { msg_id, saved_id } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.convert_star_gift(msg_id, None, saved_id).await {
                        Ok(()) => {
                            out.send(Ev::Status("Подарок обменян на Telegram Stars".to_owned()));
                            if let Ok(st) = eng.stars_status().await {
                                out.send(Ev::StarsStatus(st));
                            }
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::SendPaidReaction {
                msg_id,
                count,
                anonymous,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.send_paid_reaction(peer, msg_id, count, anonymous).await {
                        Ok(()) => {
                            out.send(Ev::Status(format!("Отправлено ⭐ ×{count}")));
                            reload(&eng, &out, peer, i).await;
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::UpdateBusinessHours {
                enabled,
                timezone,
                weekly,
            } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.update_business_work_hours(enabled, timezone, weekly).await {
                        Ok(()) => out.send(Ev::Status(if enabled {
                            "Бизнес-часы работы сохранены".to_owned()
                        } else {
                            "Бизнес-часы отключены".to_owned()
                        })),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::LoadQuickReplies => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.quick_replies().await {
                        Ok(list) => out.send(Ev::QuickReplies(list)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::SendQuickReply {
                shortcut_id,
                msg_ids,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.send_quick_reply(peer, shortcut_id, msg_ids).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::EditQuickReplyShortcut {
                shortcut_id,
                shortcut,
            } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.edit_quick_reply_shortcut(shortcut_id, &shortcut).await {
                        Ok(()) => {
                            out.send(Ev::Status(format!("Ярлык /{shortcut} обновлён")));
                            if let Ok(list) = eng.quick_replies().await {
                                out.send(Ev::QuickReplies(list));
                            }
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::DeleteQuickReplyShortcut(shortcut_id) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.delete_quick_reply_shortcut(shortcut_id).await {
                        Ok(()) => {
                            out.send(Ev::Status("Быстрый ответ удалён".to_owned()));
                            if let Ok(list) = eng.quick_replies().await {
                                out.send(Ev::QuickReplies(list));
                            }
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::Shutdown => {
                if let Ok(mut guard) = call_media_slot.lock()
                    && let Some((mut sess, mut pump)) = guard.take()
                {
                    pump.stop();
                    sess.stop();
                }
                connected.pool_handle.quit();
                break false;
            }
            Cmd::CheckUpdates => {
                let out_u = out.clone();
                tokio::spawn(async move {
                    out_u.send(Ev::Status("Проверка обновлений Rustgramm…".to_owned()));
                    let res = tokio::task::spawn_blocking(|| {
                        crate::updater::check_latest_release(env!("CARGO_PKG_VERSION"))
                    })
                    .await;
                    match res {
                        Ok(Ok(Some(info))) => out_u.send(Ev::UpdateAvailable(Box::new(info))),
                        Ok(Ok(None)) => out_u.send(Ev::Status(format!(
                            "У вас установлена актуальная версия v{}",
                            env!("CARGO_PKG_VERSION")
                        ))),
                        Ok(Err(e)) => out_u.err(e),
                        Err(_) => out_u.err("Ошибка фоновой проверки обновлений"),
                    }
                });
            }
            Cmd::ApplyUpdate(info) => {
                let out_u = out.clone();
                tokio::spawn(async move {
                    let o_prog = out_u.clone();
                    let res = tokio::task::spawn_blocking(move || {
                        crate::updater::download_and_apply_update(&info, move |done, total| {
                            o_prog.send(Ev::UpdateProgress(done, total));
                        })
                    })
                    .await;
                    match res {
                        Ok(Ok(_path)) => {
                            out_u.send(Ev::UpdateReady(if cfg!(windows) {
                                "Обновление скачано — Rustgramm перезапускается…".to_owned()
                            } else {
                                "Открыта загрузка нового APK в браузере".to_owned()
                            }));
                            if cfg!(windows) {
                                tokio::time::sleep(std::time::Duration::from_millis(700)).await;
                                std::process::exit(0);
                            }
                        }
                        Ok(Err(e)) => out_u.err(format!("Обновление: {e}")),
                        Err(_) => out_u.err("Ошибка загрузки обновления"),
                    }
                });
            }
            Cmd::RetryConnect => {
                connected.pool_handle.quit();
                tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                break true;
            }
            Cmd::LoadArchive => {
                let (eng, tx_stream, out) =
                    (engine.clone(), dialog_stream_tx.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.archived(100).await {
                        Ok(list) => {
                            if !list.is_empty() {
                                let _ = tx_stream.send((list, true)).await;
                            }
                        }
                        Err(e) => out.err(format!("Архив: {e}")),
                    }
                });
            }
            Cmd::TranscribeVoice(msg_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    out.send(Ev::Status("Расшифровка голосового…".to_owned()));
                    match eng.transcribe_voice(peer, msg_id).await {
                        Ok(text) => {
                            out.send(Ev::Transcribed(msg_id, text));
                            out.send(Ev::Status(String::new()));
                        }
                        Err(e) => out.err(format!("Расшифровка: {e}")),
                    }
                });
            }
            Cmd::SendAdvanced {
                text,
                entities,
                reply_to,
                schedule_date,
                effect,
                send_as,
                silent,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did, is_sched) =
                    (engine.clone(), out.clone(), d.peer, d.id, schedule_date.is_some());
                tokio::spawn(async move {
                    match eng
                        .send_message_advanced(
                            peer,
                            &text,
                            &entities,
                            reply_to,
                            schedule_date,
                            effect,
                            send_as,
                            silent,
                        )
                        .await
                    {
                        Ok(sent) => {
                            if is_sched {
                                out.send(Ev::Status("Сообщение запланировано".to_owned()));
                                if let Ok(rows) = eng.scheduled_history(peer).await {
                                    out.send(Ev::ScheduledHistory(did, rows));
                                }
                            } else {
                                out.send(Ev::SentConfirm(did, 0, Box::new(sent)));
                            }
                        }
                        Err(e) => out.err(format!("Отправка: {e}")),
                    }
                });
            }
            Cmd::LoadScheduled => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.scheduled_history(peer).await {
                        Ok(rows) => out.send(Ev::ScheduledHistory(did, rows)),
                        Err(e) => out.err(format!("Отложенные: {e}")),
                    }
                });
            }
            Cmd::SendScheduledNow(ids) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.send_scheduled_now(peer, ids).await {
                        Ok(()) => {
                            out.send(Ev::Status("Отложенное сообщение отправлено сейчас".to_owned()));
                            if let Ok(rows) = eng.scheduled_history(peer).await {
                                out.send(Ev::ScheduledHistory(did, rows));
                            }
                            reload(&eng, &out, peer, i).await;
                        }
                        Err(e) => out.err(format!("Отложенные: {e}")),
                    }
                });
            }
            Cmd::DeleteScheduled(ids) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.delete_scheduled(peer, ids).await {
                        Ok(()) => {
                            out.send(Ev::Status("Отложенное сообщение удалено".to_owned()));
                            if let Ok(rows) = eng.scheduled_history(peer).await {
                                out.send(Ev::ScheduledHistory(did, rows));
                            }
                        }
                        Err(e) => out.err(format!("Отложенные: {e}")),
                    }
                });
            }
            Cmd::SendTodo {
                title,
                items,
                others_can_complete,
                others_can_append,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng
                        .send_todo_list(peer, &title, &items, others_can_complete, others_can_append)
                        .await
                    {
                        Ok(()) => {
                            out.send(Ev::Status("Чек-лист отправлен".to_owned()));
                            reload(&eng, &out, peer, i).await;
                        }
                        Err(e) => out.err(format!("Чек-лист: {e}")),
                    }
                });
            }
            Cmd::ToggleTodo(msg_id, item_id, completed) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.toggle_todo_item(peer, msg_id, item_id, completed).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Чек-лист: {e}")),
                    }
                });
            }
            Cmd::SendPoll {
                question,
                options,
                anonymous,
                multiple_choice,
                quiz,
                correct_idx,
                explanation,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng
                        .send_poll(
                            peer,
                            &question,
                            &options,
                            anonymous,
                            multiple_choice,
                            quiz,
                            correct_idx,
                            &explanation,
                        )
                        .await
                    {
                        Ok(()) => {
                            out.send(Ev::Status("Опрос отправлен".to_owned()));
                            reload(&eng, &out, peer, i).await;
                        }
                        Err(e) => out.err(format!("Опрос: {e}")),
                    }
                });
            }
            Cmd::LoadSavedTags => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Ok(tags) = eng.saved_reaction_tags().await {
                        out.send(Ev::SavedTags(tags));
                    }
                });
            }
            Cmd::UpdateSavedTag(emoji, title) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.update_saved_reaction_tag(&emoji, &title).await {
                        Ok(()) => {
                            out.send(Ev::Status("Тег Избранного обновлён".to_owned()));
                            if let Ok(tags) = eng.saved_reaction_tags().await {
                                out.send(Ev::SavedTags(tags));
                            }
                        }
                        Err(e) => out.err(format!("Тег: {e}")),
                    }
                });
            }
            Cmd::LoadSendAs(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                if d.peer.id.kind() != grammers_session::types::PeerKind::Channel {
                    continue;
                }
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok(list) = eng.send_as_peers(peer).await {
                        out.send(Ev::SendAsList(did, list));
                    }
                });
            }
            Cmd::SearchHashtag(tag) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.search_hashtag_posts(&tag, 30).await {
                        Ok(rows) => out.send(Ev::HashtagResults(tag, rows)),
                        Err(e) => out.err(format!("Хэштег #{tag}: {e}")),
                    }
                });
            }
            Cmd::UpdateProfileColor(for_profile, color_id) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.update_profile_color(for_profile, color_id).await {
                        Ok(()) => out.send(Ev::Status("Цвет профиля обновлён".to_owned())),
                        Err(e) => out.err(format!("Цвет профиля: {e}")),
                    }
                });
            }
            Cmd::LoadEffects => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Ok(eff) = eng.available_effects().await {
                        out.send(Ev::AvailableEffects(eff));
                    }
                });
            }
            Cmd::ReactToggle(msg_id, emoji, remove) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.toggle_reaction(peer, msg_id, &emoji, remove).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Реакция: {e}")),
                    }
                });
            }
            Cmd::PeekHistory(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did, name) = (engine.clone(), out.clone(), d.peer, d.id, d.name.clone());
                tokio::spawn(async move {
                    match eng.history(peer, 20).await {
                        Ok(rows) => out.send(Ev::PeekHistory(did, name, rows)),
                        Err(e) => out.err(format!("Предпросмотр чата: {e}")),
                    }
                });
            }
            Cmd::ParticipantsFiltered(i, filter, q) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, id) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok((rows, total)) = eng.channel_participants_filtered(peer, filter, &q, 0, 200).await {
                        out.send(Ev::ParticipantsPage(id, rows, 0, total));
                    }
                });
            }
            Cmd::SetTyping(i, typing) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, peer) = (engine.clone(), d.peer);
                tokio::spawn(async move {
                    let _ = eng.set_typing(peer, typing).await;
                });
            }
            Cmd::SendContact {
                phone,
                first_name,
                last_name,
                reply_to,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.send_contact(peer, &phone, &first_name, &last_name, reply_to).await {
                        Ok(_) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Отправка контакта: {e}")),
                    }
                });
            }
            Cmd::SendGeo { lat, lon, reply_to } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.send_geo(peer, lat, lon, reply_to).await {
                        Ok(_) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Отправка геопозиции: {e}")),
                    }
                });
            }
            Cmd::SetSlowMode(peer, seconds) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_slow_mode(peer, seconds).await {
                        Ok(()) => out.send(Ev::Status(if seconds == 0 {
                            "Медленный режим отключён".to_owned()
                        } else {
                            format!("Медленный режим: {seconds} сек.")
                        })),
                        Err(e) => out.err(format!("Медленный режим: {e}")),
                    }
                });
            }
            Cmd::TranslateDraft {
                text,
                to_lang,
                send_after,
                reply_to,
            } => {
                let target_peer = current.and_then(|i| dialogs.get(i).map(|d| (i, d.peer)));
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.translate_text(&text, &to_lang).await {
                        Ok(tr) => {
                            if send_after {
                                if let Some((_idx, peer)) = target_peer {
                                    let did = peer.id.bot_api_dialog_id_unchecked();
                                    match eng.send_reply(peer, &tr, reply_to).await {
                                        Ok(sent) => {
                                            out.send(Ev::SentConfirm(did, 0, Box::new(sent)));
                                            out.send(Ev::DraftTranslated(String::new()));
                                            out.send(Ev::Status(format!("Переведено ({to_lang}) и отправлено")));
                                        }
                                        Err(e) => {
                                            out.send(Ev::DraftTranslated(tr));
                                            out.err(format!("Отправка перевода: {e}"));
                                        }
                                    }
                                } else {
                                    out.send(Ev::DraftTranslated(tr));
                                }
                            } else {
                                out.send(Ev::DraftTranslated(tr));
                            }
                        }
                        Err(e) => {
                            out.send(Ev::DraftTranslated(text));
                            out.err(format!("Перевод перед отправкой: {e}"));
                        }
                    }
                });
            }
            Cmd::QueryInlineBot { bot, query, offset } => {
                let Some(peer) = current.and_then(|i| dialogs.get(i)).map(|d| d.peer) else { continue };
                let (eng, out) = (engine.clone(), out.clone());
                let append = !offset.is_empty();
                tokio::spawn(async move {
                    match eng.inline_bot_results(peer, &bot, &query, &offset).await {
                        Ok(page) => out.send(Ev::InlineBotResults {
                            bot,
                            query,
                            append,
                            page,
                        }),
                        Err(e) => out.send(Ev::InlineBotError(e)),
                    }
                });
            }
            Cmd::SendInlineResult {
                query_id,
                id,
                reply_to,
            } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer) = (engine.clone(), out.clone(), d.peer);
                tokio::spawn(async move {
                    match eng.send_inline_bot_result(peer, query_id, &id, reply_to).await {
                        Ok(()) => reload(&eng, &out, peer, i).await,
                        Err(e) => out.err(format!("Инлайн-отправка: {e}")),
                    }
                });
            }
            Cmd::LoadAvailableReactions => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Ok(list) = eng.available_reactions().await {
                        out.send(Ev::AvailableReactions(list));
                    }
                });
            }
            Cmd::SetDefaultReaction(emoji) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.set_default_reaction(&emoji).await {
                        Ok(()) => out.send(Ev::Status(format!("Быстрая реакция: {emoji}"))),
                        Err(e) => out.err(format!("Быстрая реакция: {e}")),
                    }
                });
            }
            Cmd::LoadDefaultEmojiStatuses => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.default_emoji_statuses().await {
                        Ok(ids) => out.send(Ev::DefaultEmojiStatuses(ids)),
                        Err(e) => out.err(format!("Эмодзи-статусы: {e}")),
                    }
                });
            }
            Cmd::SetEmojiStatus { document_id, until } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.update_emoji_status(document_id, until).await {
                        Ok(()) => out.send(Ev::Status(if document_id.is_some() {
                            "Эмодзи-статус обновлён".to_owned()
                        } else {
                            "Эмодзи-статус сброшен".to_owned()
                        })),
                        Err(e) => out.err(format!("Эмодзи-статус: {e}")),
                    }
                });
            }
            Cmd::LoadMessageActivity(msg_id) => {
                let Some(peer) = current.and_then(|i| dialogs.get(i)).map(|d| d.peer) else { continue };
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    let (readers_res, reacts_res) = tokio::join!(
                        eng.message_read_participants(peer, msg_id),
                        eng.message_reactions_list(peer, msg_id),
                    );
                    out.send(Ev::MessageActivity {
                        msg_id,
                        readers: readers_res.unwrap_or_default(),
                        reactions: reacts_res.unwrap_or_default(),
                    });
                });
            }
            Cmd::CopyPhoto(msg_id) => {
                let Some(peer) = current.and_then(|i| dialogs.get(i)).map(|d| d.peer) else { continue };
                let (eng, out, limit) = (engine.clone(), out.clone(), limit.clone());
                tokio::spawn(async move {
                    let Ok(_permit) = limit.acquire().await else { return };
                    let cancel = std::sync::atomic::AtomicBool::new(false);
                    match eng.fetch_media(peer, msg_id, false, &cancel, |_, _| {}).await {
                        Ok(rg_core::engine::Fetched::Ready(path)) => {
                            #[cfg(not(target_os = "android"))]
                            {
                                let res = tokio::task::spawn_blocking(move || -> Result<(), String> {
                                    let img = image::open(&path).map_err(|e| format!("Декодирование фото: {e}"))?;
                                    let rgba = img.to_rgba8();
                                    let (w, h) = (rgba.width() as usize, rgba.height() as usize);
                                    let mut cb = arboard::Clipboard::new().map_err(|e| format!("Буфер обмена: {e}"))?;
                                    cb.set_image(arboard::ImageData {
                                        width: w,
                                        height: h,
                                        bytes: std::borrow::Cow::Owned(rgba.into_raw()),
                                    })
                                    .map_err(|e| format!("Копирование фото: {e}"))?;
                                    Ok(())
                                })
                                .await;
                                match res {
                                    Ok(Ok(())) => out.send(Ev::Status("Фото скопировано в буфер обмена".to_owned())),
                                    Ok(Err(e)) => out.err(e),
                                    Err(e) => out.err(format!("Копирование фото: {e}")),
                                }
                            }
                            #[cfg(target_os = "android")]
                            {
                                let _ = path;
                                out.send(Ev::Status("Копирование фото в буфер поддерживается на ПК".to_owned()));
                            }
                        }
                        Ok(_) => out.err("Фото недоступно".to_owned()),
                        Err(e) => out.err(format!("Копирование фото: {e}")),
                    }
                });
            }
            Cmd::SyncOpenChat => {
                let open_peer = current.and_then(|i| dialogs.get(i)).map(|d| (d.peer, d.id));
                let (eng, out) = (engine.clone(), out.clone());
                let home_c = home.clone();
                let ephemeral = eng.settings().privacy_shield.ephemeral_local_cache;
                tokio::spawn(async move {
                    if let Some((peer, did)) = open_peer
                        && let Ok(rows) = eng.history(peer, 45).await
                    {
                        if !ephemeral {
                            save_history_cache(&home_c, did, &rows);
                        }
                        out.send(Ev::History(did, rows));
                    }
                    if let Ok(dr) = eng.get_all_drafts().await {
                        out.send(Ev::Drafts(dr));
                    }
                });
            }
            Cmd::SaveDraft(i, text, reply_to) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, peer) = (engine.clone(), d.peer);
                tokio::spawn(async move {
                    let _ = eng.save_draft(peer, &text, reply_to).await;
                });
            }
            Cmd::MarkDialogUnread(i, unread) => {
                let Some(d) = dialogs.get_mut(i) else { continue };
                let peer = d.peer;
                d.unread = if unread { d.unread.max(1) } else { 0 };
                spawn_save_disk_cache(&home, &cached_me, &dialogs, &cached_folders);
                out.send(Ev::Dialogs(dialogs.clone()));
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Err(e) = eng.mark_dialog_unread(peer, unread).await {
                        out.err(format!("Отметка непрочитанного: {e}"));
                    }
                });
            }
            Cmd::JumpUnreadMention(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.unread_mentions(peer, 1).await {
                        Ok(ids) if !ids.is_empty() => {
                            out.send(Ev::JumpToMessage(did, ids[0]));
                        }
                        Ok(_) => {
                            let _ = eng.read_mentions(peer).await;
                            out.send(Ev::JumpToMessage(did, 0));
                        }
                        Err(e) => out.err(format!("Упоминания: {e}")),
                    }
                });
            }
            Cmd::ReadAllMentions(i) => {
                let Some(d) = dialogs.get_mut(i) else { continue };
                let peer = d.peer;
                d.unread_mentions = 0;
                out.send(Ev::Dialogs(dialogs.clone()));
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Err(e) = eng.read_mentions(peer).await {
                        out.err(format!("Упоминания: {e}"));
                    }
                });
            }
            Cmd::JumpUnreadReaction(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.unread_reactions(peer, 1).await {
                        Ok(ids) if !ids.is_empty() => {
                            out.send(Ev::JumpToMessage(did, ids[0]));
                        }
                        Ok(_) => {
                            let _ = eng.read_reactions(peer).await;
                        }
                        Err(e) => out.err(format!("Реакции: {e}")),
                    }
                });
            }
            Cmd::ReadAllReactions(i) => {
                let Some(d) = dialogs.get_mut(i) else { continue };
                let peer = d.peer;
                d.unread_reactions = 0;
                out.send(Ev::Dialogs(dialogs.clone()));
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    if let Err(e) = eng.read_reactions(peer).await {
                        out.err(format!("Реакции: {e}"));
                    }
                });
            }
            Cmd::SendDice(emoticon, reply_to, temp_id) => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.send_dice(peer, &emoticon, reply_to).await {
                        Ok(sent) => out.send(Ev::SentConfirm(did, temp_id, Box::new(sent))),
                        Err(e) => {
                            out.send(Ev::SendFailed(did, temp_id, e.clone()));
                            out.err(format!("Игра: {e}"));
                        }
                    }
                });
            }
            Cmd::LoadStickerPack(set_id, access_hash) => {
                let (eng, out, docs_arc, limit) = (
                    engine.clone(),
                    out.clone(),
                    sticker_docs.clone(),
                    avatar_limit.clone(),
                );
                tokio::spawn(async move {
                    match eng.sticker_pack_details(set_id, access_hash).await {
                        Ok(pack) => {
                            if let Ok(mut d) = docs_arc.lock() {
                                for s in &pack.stickers {
                                    d.insert(s.id, s.clone());
                                }
                            }
                            for s in pack.stickers.iter().take(24).cloned() {
                                let (eng_s, out_s, lim) = (eng.clone(), out.clone(), limit.clone());
                                tokio::spawn(async move {
                                    let Ok(_permit) = lim.acquire().await else { return };
                                    if s.mime == "application/x-tgsticker"
                                        && let Ok(Some(bytes)) = eng_s.tgs_sticker_bytes(&s).await
                                    {
                                        out_s.send(Ev::TgsDoc(s.id, bytes));
                                    }
                                    if let Ok(Some(path)) = eng_s.sticker_file(&s).await
                                        && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_square(&path, 144)).await
                                    {
                                        out_s.send(Ev::StickerImg(s.id, Box::new(img)));
                                    }
                                });
                            }
                            out.send(Ev::StickerPack(Box::new(pack)));
                        }
                        Err(e) => out.err(format!("Стикерпак: {e}")),
                    }
                });
            }
            Cmd::InstallStickerPack(set_id, access_hash, install) => {
                let (eng, out, sets_arc) = (engine.clone(), out.clone(), sticker_sets.clone());
                tokio::spawn(async move {
                    let res = if install {
                        eng.install_sticker_pack(set_id, access_hash).await
                    } else {
                        eng.uninstall_sticker_pack(set_id, access_hash).await
                    };
                    match res {
                        Ok(()) => {
                            out.send(Ev::StickerPackInstalled(set_id, install));
                            out.send(Ev::Status(if install {
                                "Набор стикеров добавлен".into()
                            } else {
                                "Набор стикеров удалён".into()
                            }));
                            if let Ok(sets) = eng.sticker_sets().await {
                                if let Ok(mut g) = sets_arc.lock() {
                                    *g = sets.clone();
                                }
                                out.send(Ev::StickerSets(sets));
                            }
                        }
                        Err(e) => out.err(format!("Набор стикеров: {e}")),
                    }
                });
            }
            Cmd::UpdateBirthday(bday) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.update_birthday(bday).await {
                        Ok(()) => {
                            out.send(Ev::BirthdayUpdated(bday));
                            out.send(Ev::Status(if bday.is_some() {
                                "Дата рождения обновлена".into()
                            } else {
                                "Дата рождения удалена".into()
                            }));
                            if let Ok(acc) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(acc)));
                            }
                        }
                        Err(e) => out.err(format!("Дата рождения: {e}")),
                    }
                });
            }
            Cmd::UpdateUsernameChecked(raw) => {
                let (eng, out, sp) = (engine.clone(), out.clone(), settings_path.clone());
                tokio::spawn(async move {
                    match eng.update_username_checked(&raw).await {
                        Ok(uname) => {
                            let mut s = eng.settings();
                            if let Some(prof) = s.accounts.iter_mut().find(|a| a.slot == s.active_account) {
                                prof.username = uname.clone();
                            }
                            let json = s.to_json();
                            eng.update_settings(|cur| *cur = s.clone());
                            let _ = std::fs::write(&sp, json);
                            out.send(Ev::Settings(Box::new(s)));
                            out.send(Ev::UsernameUpdated(uname.clone()));
                            out.send(Ev::Status(if uname.is_empty() {
                                "Публичный @username удалён".into()
                            } else {
                                format!("Установлен @{uname}")
                            }));
                            if let Ok(acc) = eng.account_overview().await {
                                out.send(Ev::Account(Box::new(acc)));
                            }
                        }
                        Err(e) => out.err(format!("Имя пользователя: {e}")),
                    }
                });
            }
            Cmd::SimilarChannels(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok(list) = eng.similar_channels(peer).await {
                        out.send(Ev::SimilarChannels(did, list));
                    }
                });
            }
            Cmd::GlobalDeletedLog(query) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    let (items, stats) = tokio::join!(eng.global_deleted(&query, 200), eng.store_stats());
                    out.send(Ev::GlobalDeletedLog { query, items, stats });
                });
            }
            Cmd::ExportChatAudit { chat_title, as_json } => {
                let Some(i) = current else { continue };
                let Some(d) = dialogs.get(i) else { continue };
                let list = engine.audit_peer(&d.peer).await;
                let did = d.id;
                let ext = if as_json { "json" } else { "txt" };
                let content = if as_json {
                    rg_ayu::store::export_audit_json(&chat_title, &list)
                } else {
                    rg_ayu::store::export_audit_txt(&chat_title, &list)
                };
                let default_name = format!("audit_{did}.{ext}");
                let dl_dir = home.join("downloads");
                let out_c = out.clone();
                tokio::task::spawn_blocking(move || {
                    #[cfg(not(target_os = "android"))]
                    let picked = rfd::FileDialog::new()
                        .set_file_name(&default_name)
                        .add_filter(if as_json { "JSON" } else { "TXT" }, &[ext])
                        .save_file();
                    #[cfg(target_os = "android")]
                    let picked = {
                        let _ = std::fs::create_dir_all(&dl_dir);
                        Some(dl_dir.join(&default_name))
                    };
                    let _ = &dl_dir;
                    if let Some(path) = picked {
                        match std::fs::write(&path, content.as_bytes()) {
                            Ok(()) => out_c.send(Ev::Status(format!(
                                "Экспортировано записей ({}) → {}",
                                list.len(),
                                path.display()
                            ))),
                            Err(e) => out_c.err(format!("Экспорт: {e}")),
                        }
                    }
                });
            }
            Cmd::JumpToDate(i, target_unix) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    match eng.history_at_date(peer, target_unix).await {
                        Ok(rows) => out.send(Ev::DateJumpHistory(did, rows, target_unix)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::CommonChats(i) => {
                let Some(d) = dialogs.get(i) else { continue };
                let (eng, out, peer, did) = (engine.clone(), out.clone(), d.peer, d.id);
                tokio::spawn(async move {
                    if let Ok(list) = eng.common_chats(peer, 50).await {
                        out.send(Ev::CommonChats(did, list));
                    }
                });
            }
            Cmd::AddContact { idx, first_name, last_name, phone } => {
                let Some(d) = dialogs.get(idx) else { continue };
                let full_name = format!("{} {}", first_name.trim(), last_name.trim()).trim().to_owned();
                match engine.add_contact(d.peer, &first_name, &last_name, &phone).await {
                    Ok(()) => {
                        let did = d.id;
                        if let Some(dm) = dialogs.get_mut(idx) {
                            dm.contact = true;
                            if !full_name.is_empty() {
                                dm.name = full_name.clone();
                            }
                        }
                        out.send(Ev::ContactUpdated(did, true, full_name));
                        out.send(Ev::Status("Контакт добавлен".into()));
                    }
                    Err(e) => out.err(e),
                }
            }
            Cmd::DeleteContact(idx) => {
                let Some(d) = dialogs.get(idx) else { continue };
                let did = d.id;
                let keep_name = d.name.clone();
                match engine.delete_contact(d.peer).await {
                    Ok(()) => {
                        if let Some(dm) = dialogs.get_mut(idx) {
                            dm.contact = false;
                        }
                        out.send(Ev::ContactUpdated(did, false, keep_name));
                        out.send(Ev::Status("Удалён из контактов".into()));
                    }
                    Err(e) => out.err(e),
                }
            }
            Cmd::SearchGlobalMessages(q, scope) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.search_global_messages(&q, scope, 40).await {
                        Ok(hits) => out.send(Ev::GlobalMessages(q, hits)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::ClearHistory(did, revoke) => {
                let Some((idx, peer)) = dialogs.iter().enumerate().find(|(_, d)| d.id == did).map(|(i, d)| (i, d.peer)) else { continue };
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.clear_history(peer, revoke).await {
                        Ok(()) => {
                            out.send(Ev::HistoryCleared(did));
                            out.send(Ev::Status("История чата очищена".into()));
                            reload(&eng, &out, peer, idx).await;
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::LeaveChat(did) => {
                let Some(pos) = dialogs.iter().position(|d| d.id == did) else { continue };
                let peer = dialogs[pos].peer;
                let (eng, out, tx, flag) = (
                    engine.clone(),
                    out.clone(),
                    dialog_refresh_tx.clone(),
                    std::sync::Arc::clone(&refresh_in_flight),
                );
                tokio::spawn(async move {
                    match eng.leave_chat(peer).await {
                        Ok(()) => {
                            out.send(Ev::LeftChat(did));
                            out.send(Ev::Status("Вы покинули чат".into()));
                            fetch_and_push_dialogs(&eng, &tx, &flag).await;
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::LoadPinnedMessages(did) => {
                let Some(peer) = dialogs.iter().find(|d| d.id == did).map(|d| d.peer) else { continue };
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.pinned_messages(peer, 80).await {
                        Ok(rows) => out.send(Ev::PinnedMessagesList(did, rows)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::UnpinAllMessages(did) => {
                let Some((idx, peer)) = dialogs.iter().enumerate().find(|(_, d)| d.id == did).map(|(i, d)| (i, d.peer)) else { continue };
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.unpin_all_messages(peer).await {
                        Ok(()) => {
                            out.send(Ev::AllUnpinned(did));
                            out.send(Ev::Status("Все закрепления сняты".into()));
                            reload(&eng, &out, peer, idx).await;
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::ForwardAdvanced {
                ids,
                to_idx,
                drop_author,
                drop_media_captions,
                silent,
            } => {
                let Some(i) = current else { continue };
                let (Some(from), Some(dest)) = (dialogs.get(i), dialogs.get(to_idx)) else { continue };
                let (eng, out, fp, tp, dname) =
                    (engine.clone(), out.clone(), from.peer, dest.peer, dest.name.clone());
                tokio::spawn(async move {
                    let opts = rg_core::engine::ForwardOptions {
                        drop_author,
                        drop_media_captions,
                        silent,
                    };
                    match eng.forward_advanced(fp, tp, &ids, opts).await {
                        Ok(()) => {
                            let mut tags = Vec::new();
                            if drop_author {
                                tags.push("без автора");
                            }
                            if drop_media_captions {
                                tags.push("без подписи");
                            }
                            if silent {
                                tags.push("тихо");
                            }
                            let note = if tags.is_empty() {
                                String::new()
                            } else {
                                format!(" ({})", tags.join(", "))
                            };
                            out.send(Ev::Status(format!("Переслано{note}: {dname}")));
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::ExportFolderInvite {
                folder_id,
                title,
                peers,
            } => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.export_folder_invite(folder_id, &title, &peers).await {
                        Ok(url) => out.send(Ev::FolderInviteUrl(url)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::CheckFolderInvite(slug_or_url) => {
                let (eng, out) = (engine.clone(), out.clone());
                tokio::spawn(async move {
                    match eng.check_folder_invite(&slug_or_url).await {
                        Ok(prev) => out.send(Ev::FolderInvitePreview(prev)),
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::JoinFolderInvite(slug, peers) => {
                let (eng, out, h, me_c, d_c) = (
                    engine.clone(),
                    out.clone(),
                    home.clone(),
                    cached_me.clone(),
                    dialogs.clone(),
                );
                tokio::spawn(async move {
                    match eng.join_folder_invite(&slug, &peers).await {
                        Ok(()) => {
                            if let Ok(f) = eng.folders().await {
                                if !eng.settings().privacy_shield.ephemeral_local_cache {
                                    spawn_save_disk_cache(&h, &me_c, &d_c, &f);
                                }
                                out.send(Ev::Folders(f));
                            }
                            out.send(Ev::Status("Папка добавлена".into()));
                        }
                        Err(e) => out.err(e),
                    }
                });
            }
            Cmd::Settings(s) => {
                let s = *s;
                rg_core::diag::set_enabled(s.diag_logging);
                let json = s.to_json();
                let ephem = s.privacy_shield.ephemeral_local_cache;
                engine.update_settings(|cur| *cur = s);
                let h = home.clone();
                let sp = settings_path.clone();
                tokio::task::spawn_blocking(move || {
                    if ephem {
                        let _ = std::fs::remove_file(h.join("dialogs_cache.json"));
                        let _ = std::fs::remove_dir_all(h.join("cache").join("history"));
                    }
                    let tmp = sp.with_extension("json.tmp");
                    if std::fs::write(&tmp, json).is_ok() {
                        let _ = std::fs::rename(&tmp, &sp);
                    }
                });
            }
        }
        };
        if let Ok(mut guard) = call_media_slot.lock()
            && let Some((mut sess, mut pump)) = guard.take()
        {
            pump.stop();
            sess.stop();
        }
        if !switch_now {
            connected.pool_handle.quit();
            if !keep_signed_in {
                let _ = std::fs::write(home.join("logout.flag"), b"1");
            }
            break 'account_loop;
        }
    }
}

/// Обновить основной список чатов (без принудительной загрузки архива — архив подгружается лениво).
/// Перечитать последние сообщения открытого чата и обновить локальный дисковый кэш (сохраняя deleted=true).
async fn reload(engine: &Engine, out: &Out, peer: grammers_session::types::PeerRef, _idx: usize) {
    let did = peer.id.bot_api_dialog_id_unchecked();
    if let Ok(rows) = engine.history(peer, 45).await {
        if !engine.settings().privacy_shield.ephemeral_local_cache {
            let acc_dir = account_home(&home(), engine.settings().active_account);
            let rows_save = rows.clone();
            tokio::task::spawn_blocking(move || save_history_cache(&acc_dir, did, &rows_save));
        }
        out.send(Ev::History(did, rows));
    }
}

/// Мгновенная фоновая предзагрузка первых 6 историй, чтобы они открывались за 0 мс.
fn preload_stories(
    eng: &Engine,
    out: &Out,
    limit: &std::sync::Arc<tokio::sync::Semaphore>,
    stories: &[StoryRow],
) {
    for st in stories.iter().take(6) {
        let (eng_s, out_s, lim, peer, sid) = (
            eng.clone(),
            out.clone(),
            limit.clone(),
            st.peer,
            st.story_id,
        );
        tokio::spawn(async move {
            let Ok(_permit) = lim.acquire().await else { return };
            if let Ok(Some(path)) = eng_s.story_media_path(peer, sid).await
                && let Ok(Some(img)) = tokio::task::spawn_blocking(move || decode_story(&path)).await
            {
                out_s.send(Ev::StoryImage(sid, Box::new(img)));
            }
        });
    }
}

/// После входа:
/// 1) СРАЗУ запускаем поток live-обновлений (чтобы новые сообщения приходили за 0 мс);
/// 2) Загружаем ТОЛЬКО первые 5 основных чатов (`folder_id = 0`), БЕЗ загрузки архива;
/// 3) Остальные основные чаты (до 5000+) догружаем в фоне мягкими порциями;
/// 4) Истории предзагружаем сразу (0 мс при открытии).
#[allow(clippy::too_many_arguments)]
async fn enter(
    engine: &Engine,
    session: &std::sync::Arc<rg_core::GuardedSession<grammers_session::storages::SqliteSession>>,
    out: &Out,
    updates: &mut Option<tokio::sync::mpsc::Receiver<grammers_session::updates::UpdatesLike>>,
    dialogs: &mut Vec<DialogRow>,
    home: &std::path::Path,
    settings_path: &std::path::Path,
    cached_me: &mut String,
    cached_folders: &mut Vec<FolderRow>,
    dialog_stream_tx: &tokio::sync::mpsc::Sender<(Vec<DialogRow>, bool)>,
) {
    // 1. Запускаем загрузку первых 5 чатов и папок параллельно с получением `get_me()`.
    let (tx, mut rx) = tokio::sync::mpsc::channel::<(Vec<DialogRow>, bool)>(64);
    let eng_stream = engine.clone();
    let stream_handle = tokio::spawn(async move {
        if let Err(e) = eng_stream.dialogs_stream(5, 5000, tx).await {
            rg_core::diag::warn("dialogs", &format!("dialogs_stream ended with err: {e}"));
        }
    });

    let (me_res, folders_res) = tokio::join!(engine.client.get_me(), engine.folders());
    if let Ok(me) = me_res {
        rg_core::cache_tl_peers(&**session, std::slice::from_ref(&me.raw), &[]).await;
        *cached_me = me.full_name();
        out.send(Ev::Me(cached_me.clone()));
        let uname = me.username().unwrap_or_default().to_owned();
        let phone = me.phone().unwrap_or_default().to_owned();
        let uid = me.raw.id();
        let mut s = engine.settings();
        if let Some(prof) = s.accounts.iter_mut().find(|a| a.slot == s.active_account) {
            prof.display_name.clone_from(cached_me);
            prof.username = uname;
            prof.phone = phone;
            prof.user_id = uid;
            let json = s.to_json();
            engine.update_settings(|cur| *cur = s.clone());
            let _ = std::fs::write(settings_path, json);
            out.send(Ev::Settings(Box::new(s)));
        }
    }

    // 2. Инициализируем состояние pts/qts при свежем входе (`updates.getState`), затем запускаем live-обновления.
    {
        use grammers_session::Session as _;
        if let Ok(cur) = session.updates_state().await
            && cur.pts == 0
            && cur.seq == 0
            && let Ok(Ok(grammers_client::tl::enums::updates::State::State(st))) = tokio::time::timeout(
                std::time::Duration::from_secs(5),
                engine.client.invoke(&grammers_client::tl::functions::updates::GetState {}),
            )
            .await
        {
            let _ = session
                .set_update_state(grammers_session::types::UpdateState::All(
                    grammers_session::types::UpdatesState {
                        pts: st.pts,
                        qts: st.qts,
                        date: st.date,
                        seq: st.seq,
                        channels: Vec::new(),
                    },
                ))
                .await;
        }
    }
    if let Some(rx_u) = updates.take() {
        let eng = engine.clone();
        let out_u = out.clone();
        tokio::spawn(async move {
            let probe = eng.clone();
            let _ = eng
                .run_updates(rx_u, move |upd| {
                    if let Some(ev) = probe.live(upd) {
                        out_u.send(Ev::Live(ev));
                    }
                    if matches!(
                        upd,
                        grammers_client::update::Update::Raw(r)
                            if matches!(
                                r.raw,
                                grammers_client::tl::enums::Update::Encryption(_)
                                    | grammers_client::tl::enums::Update::NewEncryptedMessage(_)
                            )
                    ) {
                        let eng_s = probe.clone();
                        let out_s = out_u.clone();
                        let upd_c = upd.clone();
                        tokio::spawn(async move {
                            if let Some(ev) = eng_s.process_secret_update(&upd_c).await {
                                out_s.send(Ev::Live(ev));
                            }
                        });
                    }
                })
                .await;
        });
    }

    let first_batch = rx.recv().await;
    let mut stream_done = true;
    if let Some((d, done)) = first_batch {
        stream_done = done;
        if !done && !dialogs.is_empty() && d.len() < dialogs.len() {
            // Сохраняем кэшированный список чатов поверх начального батча, обновляя свежие строки сверху.
            let mut fresh_ids: std::collections::HashSet<i64> =
                std::collections::HashSet::with_capacity(d.len());
            let mut merged = Vec::with_capacity(dialogs.len() + d.len());
            for row in d {
                if fresh_ids.insert(row.id) {
                    merged.push(row);
                }
            }
            for old in dialogs.iter() {
                if !fresh_ids.contains(&old.id) {
                    merged.push(old.clone());
                }
            }
            *dialogs = merged.clone();
            out.send(Ev::Dialogs(merged));
        } else if !d.is_empty() || dialogs.is_empty() {
            *dialogs = d.clone();
            out.send(Ev::Dialogs(d));
        }
    }
    if !stream_done {
        let fwd_tx = dialog_stream_tx.clone();
        tokio::spawn(async move {
            while let Some((batch, done)) = rx.recv().await {
                if fwd_tx.send((batch, done)).await.is_err() || done {
                    break;
                }
            }
            drop(stream_handle);
        });
    }
    if let Ok(f) = folders_res {
        *cached_folders = f.clone();
        out.send(Ev::Folders(f));
    }
    if stream_done && !engine.settings().privacy_shield.ephemeral_local_cache {
        spawn_save_disk_cache(home, cached_me, dialogs, cached_folders);
    }
    // Разовая фоновая очистка разросшихся старых живых записей messages.db (> 1500 на чат), удалённые не трогаются.
    {
        let eng_prune = engine.clone();
        tokio::spawn(async move {
            eng_prune.prune_old_live(1500).await;
        });
    }
    // 3. Обзор аккаунта, истории (с мгновенной предзагрузкой картинок!), секретные чаты и черновики — в фоне.
    {
        let eng = engine.clone();
        let out_acc = out.clone();
        let story_lim = std::sync::Arc::new(tokio::sync::Semaphore::new(6));
        tokio::spawn(async move {
            let (acc_res, st_res, sec_list, drafts_res) = tokio::join!(
                eng.account_overview(),
                eng.stories(),
                eng.secret_chats(),
                eng.get_all_drafts(),
            );
            if let Ok(a) = acc_res {
                out_acc.send(Ev::Account(Box::new(a)));
            }
            if let Ok(st) = st_res {
                preload_stories(&eng, &out_acc, &story_lim, &st);
                out_acc.send(Ev::Stories(st));
            }
            if !sec_list.is_empty() {
                out_acc.send(Ev::SecretChats(sec_list));
            }
            if let Ok(dr) = drafts_res
                && !dr.is_empty()
            {
                out_acc.send(Ev::Drafts(dr));
            }
        });
    }
    // 4. Прогрев локальной БД и кэша истории первых 10 чатов (0 мс при открытии + перехват оффлайн-удалений).
    let prefetch: Vec<(grammers_session::types::PeerRef, i64)> = dialogs
        .iter()
        .filter(|d| !d.archived)
        .take(10)
        .map(|d| (d.peer, d.id))
        .collect();
    if !prefetch.is_empty() && !engine.settings().privacy_shield.ephemeral_local_cache {
        let eng = engine.clone();
        let h_dir = home.to_owned();
        tokio::spawn(async move {
            for (idx, (peer, id)) in prefetch.into_iter().enumerate() {
                if idx >= 3 {
                    tokio::time::sleep(std::time::Duration::from_millis(180)).await;
                }
                let lim = if idx < 3 { 45 } else { 25 };
                if let Ok(rows) = eng.history(peer, lim).await {
                    let h2 = h_dir.clone();
                    let _ = tokio::task::spawn_blocking(move || save_history_cache(&h2, id, &rows)).await;
                }
            }
        });
    }
    out.send(Ev::Stage(Stage::Ready));
}

/// Переводит технические ошибки MTProto при входе в понятные русские подсказки.
fn friendly_login_error(raw: &str) -> String {
    let up = raw.to_ascii_uppercase();
    if up.contains("PHONE_NUMBER_INVALID") {
        "Неверный номер телефона — проверьте выбранную страну и цифры".to_owned()
    } else if up.contains("PHONE_NUMBER_BANNED") {
        "Этот номер заблокирован в Telegram".to_owned()
    } else if up.contains("PHONE_PASSWORD_FLOOD") || up.contains("PHONE_NUMBER_FLOOD") || up.contains("FLOOD_WAIT") {
        format!("Слишком много попыток входа — подождите немного ({raw})")
    } else if up.contains("PHONE_CODE_EXPIRED") || up.contains("PHONE_CODE_HASH_EXPIRED") {
        "Срок действия кода истёк — запросите новый код".to_owned()
    } else if up.contains("PHONE_CODE_INVALID") {
        "Неверный код подтверждения".to_owned()
    } else if up.contains("PASSWORD_HASH_INVALID") {
        "Неверный облачный пароль".to_owned()
    } else if up.contains("SRP_ID_INVALID") {
        "Параметры 2FA устарели — повторите ввод пароля ещё раз".to_owned()
    } else if up.contains("SESSION_EXPIRED") {
        "Сессия авторизации истекла — запросите код заново".to_owned()
    } else if up.contains("API_ID_INVALID") || up.contains("APP_VERSION") {
        format!("Ошибка параметров клиента Telegram ({raw}) — проверьте API ключи в «Настройках»")
    } else if up.contains("UPDATE_APP_TO_LOGIN") {
        "Сервер Telegram требует обновить версию клиента".to_owned()
    } else if up.contains("AUTH_RESTART") {
        "Telegram попросил перезапустить вход — нажмите «ДАЛЕЕ» ещё раз".to_owned()
    } else if up.contains("MIGRATE") || up.contains("303") {
        format!("Сбой переключения DC ({raw}) — нажмите «ДАЛЕЕ» ещё раз")
    } else if up.contains("IO")
        || up.contains("TIMED OUT")
        || up.contains("TIMEOUT")
        || up.contains("CONNECTION")
        || up.contains("RESET")
        || up.contains("NETWORK")
        || up.contains("BROKEN PIPE")
        || up.contains("EOF")
    {
        format!("Нет связи с серверами Telegram ({raw}). Проверьте VPN или SOCKS5 прокси в «НАСТРОЙКИ»")
    } else {
        format!("Ошибка входа: {raw}")
    }
}

/// Декодирует фото профиля и обрезает до квадрата `side` × `side`.
fn decode_square(path: &std::path::Path, side: u32) -> Option<eframe::egui::ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.resize_to_fill(side, side, image::imageops::FilterType::Triangle).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// Декодирует картинку и уменьшает до 512 px по большей стороне (экономит ~36% ОЗУ на текстурах).
fn decode(path: &std::path::Path) -> Option<eframe::egui::ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(512, 512).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

/// Декодирует картинку истории в высоком чётком разрешении (до 1080 px) для 9:16 просмотрщика.
fn decode_story(path: &std::path::Path) -> Option<eframe::egui::ColorImage> {
    let bytes = std::fs::read(path).ok()?;
    let img = image::load_from_memory(&bytes).ok()?.thumbnail(1080, 1080).to_rgba8();
    let size = [img.width() as usize, img.height() as usize];
    Some(eframe::egui::ColorImage::from_rgba_unmultiplied(size, img.as_raw()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use grammers_session::types::{PeerAuth, PeerId, PeerRef};

    #[test]
    fn disk_cache_roundtrip_is_fast() {
        let dir = std::env::temp_dir().join("rg-disk-cache-test");
        let _ = std::fs::create_dir_all(&dir);
        let dialogs: Vec<DialogRow> = (0..300)
            .map(|i| DialogRow {
                peer: PeerRef {
                    id: PeerId::user_unchecked(1000 + i),
                    auth: PeerAuth::default(),
                },
                id: 1000 + i,
                name: format!("Чат {i}"),
                last: format!("Последнее сообщение {i}"),
                date: 1_763_700_000 - i * 60,
                unread: (i % 5) as i32,
                pinned: i < 3,
                muted: i % 7 == 0,
                notify_explicit: (i % 7 == 0).then_some(true),
                saved: i == 0,
                archived: i > 250,
                kind: DialogKind::User,
                contact: true,
                read_out: 42,
                emoji_status: (i == 1).then_some(543210987),
                verified: i == 2,
                premium: i == 1,
                unread_mentions: 0,
                unread_reactions: 0,
            })
            .collect();
        let folders = vec![FolderRow {
            id: 2,
            title: "Работа".to_owned(),
            contacts: true,
            non_contacts: false,
            groups: true,
            broadcasts: false,
            bots: false,
            exclude_muted: false,
            exclude_read: false,
            exclude_archived: true,
            include: vec![1001],
            exclude: vec![],
        }];
        let t0 = std::time::Instant::now();
        save_disk_cache(&dir, "Тест", &dialogs, &folders);
        let (me, loaded_d, loaded_f) = load_disk_cache(&dir).unwrap();
        let elapsed = t0.elapsed();
        assert_eq!(me, "Тест");
        assert_eq!(loaded_d.len(), 300);
        assert_eq!(loaded_f.len(), 1);
        assert_eq!(loaded_f[0].title, "Работа");
        assert!(elapsed.as_millis() < 500, "disk cache took {elapsed:?}");
        let sample_msgs = vec![
            MessageRow::plain(1, "Привет".to_owned(), true, 1_763_700_000),
            MessageRow::plain(2, "Ответ".to_owned(), false, 1_763_700_010),
        ];
        save_history_cache(&dir, 4242, &sample_msgs);
        let loaded_msgs = load_history_cache(&dir, 4242).unwrap();
        assert_eq!(loaded_msgs, sample_msgs);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn builtin_keys_always_provide_public_fallback_without_prompt() {
        let cfg = builtin_keys().expect("builtin_keys must fall back cleanly without API prompt");
        assert!(cfg.api_id > 0);
        assert!(!cfg.api_hash.is_empty());
    }
}
