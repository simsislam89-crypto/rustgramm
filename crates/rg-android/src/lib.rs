//! Rustgramm для Android: тонкая оболочка NativeActivity над общим интерфейсом `rg-ui`.
//! Поддерживает две сборки:
//! - Полная (с плагинами, по умолчанию `--features plugins`): `Rustgramm-Android-Full.apk`
//! - Облегчённая (без плагинов, `--no-default-features`): `Rustgramm-Android-Lite.apk`

/// Включена ли система плагинов в этой сборке Android-клиента.
pub const WITH_PLUGINS: bool = cfg!(feature = "plugins");

/// Точка входа NativeActivity. Каталог данных приложения — во внутреннем хранилище
/// (недоступен другим приложениям).
#[cfg(target_os = "android")]
#[unsafe(no_mangle)]
fn android_main(app: android_activity::AndroidApp) {
    if let Some(dir) = app.internal_data_path() {
        let _ = rg_ui::worker::DATA_DIR.set(dir);
    }
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = rg_ui::run(options);
}

/// На не-Android целях крейт пуст: рабочее пространство собирается целиком.
#[cfg(not(target_os = "android"))]
pub const SUPPORTED: bool = false;
