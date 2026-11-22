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
    rg_ui::android_jni::init(app.vm_as_ptr(), app.activity_as_ptr());
    if let Some(dir) = app.internal_data_path() {
        let _ = rg_ui::worker::DATA_DIR.set(dir);
    }
    // Не блокируем старт приложения запросом разрешений до появления экрана входа —
    // права запрашиваются лениво при записи голоса, выборе файлов и показе уведомлений.
    let options = eframe::NativeOptions {
        android_app: Some(app),
        ..Default::default()
    };
    let _ = rg_ui::run(options);
    rg_ui::android_jni::shutdown_all();
}

/// На не-Android целях крейт пуст: рабочее пространство собирается целиком.
#[cfg(not(target_os = "android"))]
pub const SUPPORTED: bool = false;
