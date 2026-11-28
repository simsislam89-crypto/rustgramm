//! Надёжная авторизация по номеру телефона и коду:
//! - прямой `auth.sendCode` без паники `grammers-client` на `SentCode::PaymentRequired`;
//! - явный перенос дата-центра (`PHONE_MIGRATE_X` / `NETWORK_MIGRATE_X` 303);
//! - автоповтор при `AUTH_RESTART`;
//! - понятная подсказка способа доставки кода (`App`, `Sms`, `Call`, `FragmentSms`, `EmailCode`).

use grammers_client::client::{PasswordToken, SignInError};
use grammers_client::{Client, InvocationError, tl};
use grammers_mtsender::SenderPoolHandle;
use grammers_session::Session;

/// Результат запроса кода подтверждения.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RequestCodeOutcome {
    /// Код отправлен — нужен ввод кода.
    CodeSent(SentCodeInfo),
    /// Сессия уже авторизована сервером (`SentCode::Success`).
    AlreadyAuthorized,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SentCodeInfo {
    pub phone: String,
    pub phone_code_hash: String,
    pub delivery_hint: String,
    pub timeout_secs: Option<i32>,
    pub code_length: u8,
}

/// Результат проверки кода подтверждения.
pub enum SignInOutcome {
    SignedIn,
    NeedPassword(PasswordToken),
    SignUpRequired,
}

/// Человекочитаемое описание канала доставки кода по `tl::enums::auth::SentCodeType`.
pub fn sent_code_length(kind: &tl::enums::auth::SentCodeType) -> u8 {
    use tl::enums::auth::SentCodeType as S;
    let raw = match kind {
        S::App(t) => t.length,
        S::Sms(t) => t.length,
        S::Call(t) => t.length,
        S::MissedCall(t) => t.length,
        S::EmailCode(t) => t.length,
        S::FragmentSms(t) => t.length,
        S::FirebaseSms(t) => t.length,
        S::FlashCall(_) | S::SetUpEmailRequired(_) | S::SmsWord(_) | S::SmsPhrase(_) => 5,
    };
    raw.clamp(4, 12) as u8
}

pub fn describe_sent_code_type(kind: &tl::enums::auth::SentCodeType) -> &'static str {
    use tl::enums::auth::SentCodeType as S;
    match kind {
        S::App(_) => "Код отправлен в приложение Telegram на другом устройстве",
        S::Sms(_) | S::SmsWord(_) | S::SmsPhrase(_) => "Код отправлен по SMS на ваш номер",
        S::Call(_) | S::FlashCall(_) | S::MissedCall(_) => "Ожидайте проверочный звонок на ваш номер",
        S::FragmentSms(_) => "Код отправлен через Fragment",
        S::EmailCode(_) | S::SetUpEmailRequired(_) => "Код подтверждения отправлен на электронную почту",
        S::FirebaseSms(_) => "Код отправлен на устройство (SMS / push)",
    }
}

fn send_code_request(phone: &str, api_id: i32, api_hash: &str) -> tl::functions::auth::SendCode {
    tl::functions::auth::SendCode {
        phone_number: phone.to_owned(),
        api_id,
        api_hash: api_hash.to_owned(),
        settings: tl::types::CodeSettings {
            allow_flashcall: false,
            current_number: false,
            allow_app_hash: false,
            allow_missed_call: false,
            allow_firebase: false,
            logout_tokens: None,
            token: None,
            app_sandbox: None,
            unknown_number: false,
        }
        .into(),
    }
}

fn map_sent_code(clean_phone: String, sent: tl::enums::auth::SentCode) -> Result<RequestCodeOutcome, String> {
    match sent {
        tl::enums::auth::SentCode::Code(c) => {
            let hint = describe_sent_code_type(&c.r#type).to_owned();
            let code_length = sent_code_length(&c.r#type);
            Ok(RequestCodeOutcome::CodeSent(SentCodeInfo {
                phone: clean_phone,
                phone_code_hash: c.phone_code_hash,
                delivery_hint: hint,
                timeout_secs: c.timeout,
                code_length,
            }))
        }
        tl::enums::auth::SentCode::Success(_) => Ok(RequestCodeOutcome::AlreadyAuthorized),
        tl::enums::auth::SentCode::PaymentRequired(_) => Err(
            "Telegram требует подтверждение SMS-платежа для этого номера — используйте вход по QR-коду"
                .to_owned(),
        ),
    }
}

async fn invoke_with_dc_migrate<S, R>(
    client: &Client,
    session: &S,
    pool_handle: Option<&SenderPoolHandle>,
    req: &R,
) -> Result<R::Return, InvocationError>
where
    S: Session,
    R: tl::RemoteCall + Send + Sync,
{
    let mut dc_id: Option<i32> = None;
    let mut prev_dc = session.home_dc_id().unwrap_or(2);
    let mut transient = 0u8;
    for _hop in 0..4 {
        let res = match dc_id {
            Some(dc) => client.invoke_in_dc(dc, req).await,
            None => client.invoke(req).await,
        };
        match res {
            Ok(v) => return Ok(v),
            Err(InvocationError::Rpc(err)) if err.code == 303 => {
                if let Some(new_dc) = err.value.map(|v| v as i32) {
                    let _ = session.set_home_dc_id(new_dc).await;
                    if let Some(h) = pool_handle
                        && prev_dc != new_dc
                    {
                        h.disconnect_from_dc(prev_dc);
                    }
                    prev_dc = new_dc;
                    dc_id = Some(new_dc);
                    continue;
                }
                return Err(InvocationError::Rpc(err));
            }
            Err(InvocationError::Io(_) | InvocationError::Deserialize(_)) if transient < 2 => {
                transient += 1;
                if let Some(h) = pool_handle {
                    h.disconnect_from_dc(prev_dc);
                }
                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
            }
            Err(InvocationError::Rpc(err)) if err.code >= 500 && transient < 2 => {
                transient += 1;
                tokio::time::sleep(std::time::Duration::from_millis(350)).await;
            }
            Err(e) => return Err(e),
        }
    }
    Err(InvocationError::Io(std::io::Error::other(
        "Слишком много перенаправлений DC (303)",
    )))
}

/// Запрос кода авторизации по номеру телефона без паник и с корректной миграцией DC.
pub async fn request_login_code<S: Session>(
    client: &Client,
    session: &S,
    pool_handle: Option<&SenderPoolHandle>,
    api_id: i32,
    api_hash: &str,
    phone: &str,
) -> Result<RequestCodeOutcome, String> {
    let clean_phone = phone.trim().to_owned();
    let req = send_code_request(&clean_phone, api_id, api_hash);
    let mut res = invoke_with_dc_migrate(client, session, pool_handle, &req).await;
    for _ in 0..2 {
        if let Err(InvocationError::Rpc(ref r)) = res
            && r.name.eq_ignore_ascii_case("AUTH_RESTART")
        {
            tokio::time::sleep(std::time::Duration::from_millis(300)).await;
            res = invoke_with_dc_migrate(client, session, pool_handle, &req).await;
        } else {
            break;
        }
    }
    let sent = res.map_err(|e| e.to_string())?;
    map_sent_code(clean_phone, sent)
}

/// Повторная отправка кода (`auth.resendCode`, например переход к SMS / звонку).
pub async fn resend_login_code<S: Session>(
    client: &Client,
    session: &S,
    pool_handle: Option<&SenderPoolHandle>,
    phone: &str,
    phone_code_hash: &str,
) -> Result<RequestCodeOutcome, String> {
    let clean_phone = phone.trim().to_owned();
    let req = tl::functions::auth::ResendCode {
        phone_number: clean_phone.clone(),
        phone_code_hash: phone_code_hash.to_owned(),
        reason: None,
    };
    let sent = invoke_with_dc_migrate(client, session, pool_handle, &req)
        .await
        .map_err(|e| e.to_string())?;
    map_sent_code(clean_phone, sent)
}

/// Проверка введённого кода подтверждения и переход к 2FA при необходимости.
pub async fn sign_in<S: Session>(
    client: &Client,
    session: &S,
    pool_handle: Option<&SenderPoolHandle>,
    phone: &str,
    phone_code_hash: &str,
    code: &str,
) -> Result<SignInOutcome, String> {
    let clean_code = code.trim().to_owned();
    let req = tl::functions::auth::SignIn {
        phone_number: phone.to_owned(),
        phone_code_hash: phone_code_hash.to_owned(),
        phone_code: Some(clean_code),
        email_verification: None,
    };
    match invoke_with_dc_migrate(client, session, pool_handle, &req).await {
        Ok(auth @ tl::enums::auth::Authorization::Authorization(_)) => {
            crate::cache_tl_auth_user(session, &auth).await;
            Ok(SignInOutcome::SignedIn)
        }
        Ok(tl::enums::auth::Authorization::SignUpRequired(_)) => Ok(SignInOutcome::SignUpRequired),
        Err(InvocationError::Rpc(err)) if err.name.eq_ignore_ascii_case("SESSION_PASSWORD_NEEDED") => {
            let tl::enums::account::Password::Password(p) =
                invoke_with_dc_migrate(client, session, pool_handle, &tl::functions::account::GetPassword {})
                    .await
                    .map_err(|e| e.to_string())?;
            Ok(SignInOutcome::NeedPassword(PasswordToken::new(p)))
        }
        Err(err) => Err(err.to_string()),
    }
}

/// Исход проверки облачного пароля (2FA) без безвозвратной потери токена при сетевой ошибке/таймауте.
pub enum CheckPasswordOutcome {
    SignedIn,
    InvalidPassword(PasswordToken),
    Error {
        token: Option<PasswordToken>,
        message: String,
    },
}

/// Получает свежий `PasswordToken` (`account.getPassword`).
pub async fn refresh_password_token<S: Session>(
    client: &Client,
    session: &S,
    pool_handle: Option<&SenderPoolHandle>,
) -> Result<PasswordToken, String> {
    let tl::enums::account::Password::Password(p) =
        invoke_with_dc_migrate(client, session, pool_handle, &tl::functions::account::GetPassword {})
            .await
            .map_err(|e| e.to_string())?;
    Ok(PasswordToken::new(p))
}

/// Проверка облачного пароля с автоповтором `SRP_ID_INVALID` и сохранением/обновлением токена при сбое.
async fn refresh_password_token_bounded<S: Session>(
    client: &Client,
    session: &S,
    pool_handle: Option<&SenderPoolHandle>,
) -> Option<PasswordToken> {
    tokio::time::timeout(
        std::time::Duration::from_secs(15),
        refresh_password_token(client, session, pool_handle),
    )
    .await
    .ok()
    .and_then(Result::ok)
}

pub async fn check_password_safe<S: Session>(
    client: &Client,
    session: &S,
    pool_handle: Option<&SenderPoolHandle>,
    token: PasswordToken,
    password: &str,
) -> CheckPasswordOutcome {
    let pw = password.trim().to_owned();
    let res = tokio::time::timeout(
        std::time::Duration::from_secs(45),
        client.check_password(token, pw.clone()),
    )
    .await;
    match res {
        Ok(Ok(_)) => CheckPasswordOutcome::SignedIn,
        Ok(Err(SignInError::InvalidPassword(t))) => CheckPasswordOutcome::InvalidPassword(t),
        Ok(Err(SignInError::Other(err))) if err.is("SRP_ID_INVALID") => {
            let fresh = match tokio::time::timeout(
                std::time::Duration::from_secs(20),
                refresh_password_token(client, session, pool_handle),
            )
            .await
            {
                Ok(Ok(t)) => t,
                Ok(Err(e)) => {
                    return CheckPasswordOutcome::Error {
                        token: None,
                        message: e,
                    };
                }
                Err(_) => {
                    return CheckPasswordOutcome::Error {
                        token: None,
                        message: "Таймаут обновления 2FA-токена (SRP_ID_INVALID)".to_owned(),
                    };
                }
            };
            match tokio::time::timeout(
                std::time::Duration::from_secs(45),
                client.check_password(fresh, pw),
            )
            .await
            {
                Ok(Ok(_)) => CheckPasswordOutcome::SignedIn,
                Ok(Err(SignInError::InvalidPassword(t))) => CheckPasswordOutcome::InvalidPassword(t),
                Ok(Err(e)) => CheckPasswordOutcome::Error {
                    token: refresh_password_token_bounded(client, session, pool_handle).await,
                    message: e.to_string(),
                },
                Err(_) => CheckPasswordOutcome::Error {
                    token: refresh_password_token_bounded(client, session, pool_handle).await,
                    message: "Таймаут проверки облачного пароля".to_owned(),
                },
            }
        }
        Ok(Err(err)) => CheckPasswordOutcome::Error {
            token: refresh_password_token_bounded(client, session, pool_handle).await,
            message: err.to_string(),
        },
        Err(_) => CheckPasswordOutcome::Error {
            token: refresh_password_token_bounded(client, session, pool_handle).await,
            message: "Таймаут проверки облачного пароля (45 с)".to_owned(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sent_code_type_hints_cover_variants() {
        let app = tl::enums::auth::SentCodeType::App(tl::types::auth::SentCodeTypeApp { length: 5 });
        let sms = tl::enums::auth::SentCodeType::Sms(tl::types::auth::SentCodeTypeSms { length: 5 });
        let call = tl::enums::auth::SentCodeType::Call(tl::types::auth::SentCodeTypeCall { length: 5 });
        let frag = tl::enums::auth::SentCodeType::FragmentSms(tl::types::auth::SentCodeTypeFragmentSms {
            url: String::new(),
            length: 5,
        });
        assert!(describe_sent_code_type(&app).contains("приложение Telegram"));
        assert!(describe_sent_code_type(&sms).contains("SMS"));
        assert!(describe_sent_code_type(&call).contains("звонок"));
        assert!(describe_sent_code_type(&frag).contains("Fragment"));
        assert_eq!(sent_code_length(&app), 5);
        assert_eq!(sent_code_length(&sms), 5);
        let mail = tl::enums::auth::SentCodeType::EmailCode(tl::types::auth::SentCodeTypeEmailCode {
            apple_signin_allowed: false,
            google_signin_allowed: false,
            email_pattern: "a***@b.c".to_owned(),
            length: 6,
            reset_available_period: None,
            reset_pending_date: None,
        });
        assert_eq!(sent_code_length(&mail), 6);
    }
}
