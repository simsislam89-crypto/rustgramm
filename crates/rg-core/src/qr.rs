//! Вход по QR-коду: `auth.exportLoginToken` опрашивается, пока телефон не подтвердит вход.
//! QR содержит `tg://login?token=…`; токен живёт ~30 секунд и обновляется на каждом опросе.

use grammers_client::client::PasswordToken;
use grammers_client::{Client, InvocationError, tl};
use grammers_session::Session;

/// Результат одного опроса.
pub enum QrStep {
    /// Показать QR с этой ссылкой.
    Token { url: String },
    /// Вход выполнен.
    Done,
    /// Включена двухэтапная защита: нужен облачный пароль.
    NeedPassword(PasswordToken),
}

/// Base64url без заполнения (формат токена в ссылке `tg://login`).
pub fn base64url(data: &[u8]) -> String {
    const ABC: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let n = match *chunk {
            [a, b, c] => u32::from(a) << 16 | u32::from(b) << 8 | u32::from(c),
            [a, b] => u32::from(a) << 16 | u32::from(b) << 8,
            [a] => u32::from(a) << 16,
            _ => 0,
        };
        let chars = chunk.len() + 1;
        for i in 0..chars {
            out.push(ABC[((n >> (18 - 6 * i)) & 63) as usize] as char);
        }
    }
    out
}

pub fn login_url(token: &[u8]) -> String {
    format!("tg://login?token={}", base64url(token))
}

fn needs_password(e: &InvocationError) -> bool {
    matches!(e, InvocationError::Rpc(r) if r.name == "SESSION_PASSWORD_NEEDED")
}

async fn password_step(client: &Client) -> Result<QrStep, InvocationError> {
    let tl::enums::account::Password::Password(p) = client
        .invoke(&tl::functions::account::GetPassword {})
        .await?;
    Ok(QrStep::NeedPassword(PasswordToken::new(p)))
}

/// Один опрос QR-входа.
pub async fn poll<S: Session>(
    client: &Client,
    session: &S,
    api_id: i32,
    api_hash: &str,
) -> Result<QrStep, InvocationError> {
    use tl::enums::auth::LoginToken as T;
    let res = client
        .invoke(&tl::functions::auth::ExportLoginToken {
            api_id,
            api_hash: api_hash.to_owned(),
            except_ids: Vec::new(),
        })
        .await;
    let token = match res {
        Ok(t) => t,
        Err(e) if needs_password(&e) => return password_step(client).await,
        Err(e) => return Err(e),
    };
    match token {
        T::Token(t) => Ok(QrStep::Token {
            url: login_url(&t.token),
        }),
        T::Success(_) => Ok(QrStep::Done),
        T::MigrateTo(m) => {
            let imported = client
                .invoke_in_dc(
                    m.dc_id,
                    &tl::functions::auth::ImportLoginToken { token: m.token },
                )
                .await;
            match imported {
                Ok(_) => {
                    // Основной дата-центр аккаунта — тот, куда нас перенаправили.
                    let _ = session.set_home_dc_id(m.dc_id).await;
                    Ok(QrStep::Done)
                }
                Err(e) if needs_password(&e) => {
                    let _ = session.set_home_dc_id(m.dc_id).await;
                    password_step(client).await
                }
                Err(e) => Err(e),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn base64url_matches_rfc4648_vectors() {
        assert_eq!(base64url(b""), "");
        assert_eq!(base64url(b"f"), "Zg");
        assert_eq!(base64url(b"fo"), "Zm8");
        assert_eq!(base64url(b"foo"), "Zm9v");
        assert_eq!(base64url(b"foob"), "Zm9vYg");
        assert_eq!(base64url(&[0xfb, 0xff, 0xfe]), "-__-");
    }

    #[test]
    fn login_url_has_scheme_and_token() {
        assert_eq!(login_url(b"foo"), "tg://login?token=Zm9v");
    }
}
