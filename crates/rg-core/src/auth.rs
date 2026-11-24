//! Надёжная авторизация по номеру телефона и коду:
//! - прямой `auth.sendCode` без паники `grammers-client` на `SentCode::PaymentRequired`;
//! - явный перенос дата-центра (`PHONE_MIGRATE_X` / `NETWORK_MIGRATE_X` 303);
//! - автоповтор при `AUTH_RESTART`;
//! - понятная подсказка способа доставки кода (`App`, `Sms`, `Call`, `FragmentSms`, `EmailCode`).

use grammers_client::client::PasswordToken;
use grammers_client::{Client, InvocationError, tl};
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
}

/// Результат проверки кода подтверждения.
pub enum SignInOutcome {
    SignedIn,
    NeedPassword(PasswordToken),
    SignUpRequired,
}

/// Человекочитаемое описание канала доставки кода по `tl::enums::auth::SentCodeType`.
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

async fn invoke_send_code_with_migrate<S: Session>(
    client: &Client,
    session: &S,
    req: &tl::functions::auth::SendCode,
) -> Result<tl::enums::auth::SentCode, InvocationError> {
    match client.invoke(req).await {
        Ok(res) => Ok(res),
        Err(InvocationError::Rpc(err)) if err.code == 303 => {
            if let Some(new_dc) = err.value.map(|v| v as i32) {
                let _ = session.set_home_dc_id(new_dc).await;
                match client.invoke_in_dc(new_dc, req).await {
                    Ok(r) => return Ok(r),
                    Err(e2) => return Err(e2),
                }
            }
            Err(InvocationError::Rpc(err))
        }
        Err(e) => Err(e),
    }
}

/// Запрос кода авторизации по номеру телефона без паник и с корректной миграцией DC.
pub async fn request_login_code<S: Session>(
    client: &Client,
    session: &S,
    api_id: i32,
    api_hash: &str,
    phone: &str,
) -> Result<RequestCodeOutcome, String> {
    let clean_phone = phone.trim().to_owned();
    let req = send_code_request(&clean_phone, api_id, api_hash);
    let mut res = invoke_send_code_with_migrate(client, session, &req).await;
    if let Err(InvocationError::Rpc(ref r)) = res
        && r.name.eq_ignore_ascii_case("AUTH_RESTART")
    {
        res = invoke_send_code_with_migrate(client, session, &req).await;
    }
    let sent = res.map_err(|e| e.to_string())?;
    match sent {
        tl::enums::auth::SentCode::Code(c) => {
            let hint = describe_sent_code_type(&c.r#type).to_owned();
            Ok(RequestCodeOutcome::CodeSent(SentCodeInfo {
                phone: clean_phone,
                phone_code_hash: c.phone_code_hash,
                delivery_hint: hint,
                timeout_secs: c.timeout,
            }))
        }
        tl::enums::auth::SentCode::Success(_) => Ok(RequestCodeOutcome::AlreadyAuthorized),
        tl::enums::auth::SentCode::PaymentRequired(_) => Err(
            "Telegram требует подтверждение SMS-платежа для этого номера — используйте вход по QR-коду"
                .to_owned(),
        ),
    }
}

/// Проверка введённого кода подтверждения и переход к 2FA при необходимости.
pub async fn sign_in(
    client: &Client,
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
    match client.invoke(&req).await {
        Ok(tl::enums::auth::Authorization::Authorization(_)) => Ok(SignInOutcome::SignedIn),
        Ok(tl::enums::auth::Authorization::SignUpRequired(_)) => Ok(SignInOutcome::SignUpRequired),
        Err(err) if err.is("SESSION_PASSWORD_NEEDED") => {
            let tl::enums::account::Password::Password(p) = client
                .invoke(&tl::functions::account::GetPassword {})
                .await
                .map_err(|e| e.to_string())?;
            Ok(SignInOutcome::NeedPassword(PasswordToken::new(p)))
        }
        Err(err) => Err(err.to_string()),
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
    }
}
