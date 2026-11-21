//! Интерактивный вход. Переменные окружения: `RG_API_ID`, `RG_API_HASH`, `RG_PHONE`, `RG_SESSION`.
//! Код из Telegram и облачный пароль вводите вы сами в терминале (пароль не отображается
//! и нигде не сохраняется; в сессию попадает только ключ авторизации).

use std::io::{BufRead, Write};

use grammers_client::SignInError;
use rg_core::{Config, connect};

fn prompt(msg: &str) -> std::io::Result<String> {
    print!("{msg}");
    std::io::stdout().flush()?;
    let mut line = String::new();
    std::io::stdin().lock().read_line(&mut line)?;
    Ok(line.trim().to_owned())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let cfg = Config::from_env().map_err(|e| format!("{e:?}"))?;
    let phone = std::env::var("RG_PHONE")?;
    let client = connect(&cfg).await?;

    if client.is_authorized().await? {
        println!("Уже вошли.");
    } else {
        let token = client.request_login_code(&phone, &cfg.api_hash).await?;
        println!("Код отправлен в Telegram.");
        let code = prompt("Код из Telegram: ")?;
        match client.sign_in(&token, &code).await {
            Ok(_) => {}
            Err(SignInError::PasswordRequired(pw)) => {
                let hint = pw.hint().unwrap_or("нет").to_owned();
                let password = rpassword::prompt_password(format!("Облачный пароль (подсказка: {hint}): "))?;
                client.check_password(pw, password.trim()).await?;
            }
            Err(e) => return Err(e.into()),
        }
        println!("Вход выполнен.");
    }

    let me = client.get_me().await?;
    println!("Аккаунт: {}", me.full_name());
    let mut dialogs = client.iter_dialogs();
    let mut n = 0;
    while let Some(d) = dialogs.next().await? {
        if n < 5 {
            println!("- {}", d.peer().name().unwrap_or_default());
        }
        n += 1;
    }
    println!("Диалогов: {n}");
    Ok(())
}
