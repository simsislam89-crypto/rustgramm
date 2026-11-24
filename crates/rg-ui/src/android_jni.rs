//! Нативная Android JNI-интеграция для Rustgramm:
//! - `AudioRecord`: запись голосовых сообщений и звука звонка (16 кГц моно PCM16);
//! - `MediaPlayer` / `AudioTrack`: звуки уведомлений, гудок, плеер аудио/голосовых и поток звонка;
//! - `Intent.ACTION_OPEN_DOCUMENT` (`RgBridge` DEX): системный выбор фото, видео и файлов из галереи;
//! - Встроенный `android.webkit.WebView` (`RgBridge` DEX) для Mini Apps с JS-бриджем `TelegramWebviewProxy`;
//! - Фоновые системные уведомления через `NotificationManager` + `NotificationChannel`.

use std::ffi::c_void;
use std::path::{Path, PathBuf};

#[cfg(target_os = "android")]
mod imp {
    use super::*;
    use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::time::Instant;

    use jni::JNIEnv;
    use jni::JavaVM;
    use jni::objects::{GlobalRef, JClass, JObject, JString, JValue};
    use jni::sys::jint;

    const BRIDGE_DEX: &[u8] = include_bytes!("../assets/rg_bridge.dex");

    struct AndroidContext {
        vm: JavaVM,
        activity: GlobalRef,
    }

    static CTX: OnceLock<AndroidContext> = OnceLock::new();
    static BRIDGE_CLASS: OnceLock<GlobalRef> = OnceLock::new();
    static ACTIVE_PLAYER: OnceLock<Mutex<Option<GlobalRef>>> = OnceLock::new();
    static CHANNEL_CREATED: AtomicBool = AtomicBool::new(false);

    pub fn init(vm_ptr: *mut c_void, activity_ptr: *mut c_void) {
        if vm_ptr.is_null() || activity_ptr.is_null() {
            return;
        }
        // SAFETY: указатели получены напрямую от `android_activity::AndroidApp`.
        let Ok(vm) = (unsafe { JavaVM::from_raw(vm_ptr.cast()) }) else {
            return;
        };
        let global_act = {
            let Ok(env) = vm.attach_current_thread() else {
                return;
            };
            // SAFETY: `activity_ptr` — валидная локальная/глобальная ссылка на объект Activity.
            let act_obj = unsafe { JObject::from_raw(activity_ptr.cast()) };
            let Ok(global_act) = env.new_global_ref(act_obj) else {
                return;
            };
            global_act
        };
        let _ = CTX.set(AndroidContext {
            vm,
            activity: global_act,
        });
    }

    fn with_env<R>(f: impl FnOnce(&mut JNIEnv<'_>, &JObject<'_>) -> Option<R>) -> Option<R> {
        let ctx = CTX.get()?;
        let mut env = ctx.vm.attach_current_thread().ok()?;
        let act = ctx.activity.as_obj();
        let res = f(&mut env, act);
        if env.exception_check().unwrap_or(false) {
            let _ = env.exception_clear();
        }
        res
    }

    fn sdk_int(env: &mut JNIEnv<'_>) -> i32 {
        env.get_static_field("android/os/Build$VERSION", "SDK_INT", "I")
            .ok()
            .and_then(|v| v.i().ok())
            .unwrap_or(24)
    }

    fn bridge_class<'a>(env: &mut JNIEnv<'a>, act: &JObject<'_>) -> Option<JClass<'a>> {
        if let Some(g) = BRIDGE_CLASS.get() {
            return Some(JClass::from(env.new_local_ref(g.as_obj()).ok()?));
        }
        let parent_loader = env
            .call_method(act, "getClassLoader", "()Ljava/lang/ClassLoader;", &[])
            .ok()?
            .l()
            .ok()?;

        let loader = if sdk_int(env) >= 26 {
            let mut dex_copy = BRIDGE_DEX.to_vec();
            let bb = unsafe { env.new_direct_byte_buffer(dex_copy.as_mut_ptr(), dex_copy.len()) }.ok()?;
            // ByteBuffer.allocateDirect + copy гарантирует жизнь памяти внутри JVM.
            let direct = env
                .call_static_method(
                    "java/nio/ByteBuffer",
                    "allocateDirect",
                    "(I)Ljava/nio/ByteBuffer;",
                    &[JValue::Int(dex_copy.len() as jint)],
                )
                .ok()?
                .l()
                .ok()?;
            let _ = env
                .call_method(
                    &direct,
                    "put",
                    "(Ljava/nio/ByteBuffer;)Ljava/nio/ByteBuffer;",
                    &[JValue::Object(&bb)],
                )
                .ok()?;
            let _ = env.call_method(&direct, "rewind", "()Ljava/nio/Buffer;", &[]);
            env.new_object(
                "dalvik/system/InMemoryDexClassLoader",
                "(Ljava/nio/ByteBuffer;Ljava/lang/ClassLoader;)V",
                &[JValue::Object(&direct), JValue::Object(&parent_loader)],
            )
            .ok()?
        } else {
            let cache_dir = env
                .call_method(act, "getCodeCacheDir", "()Ljava/io/File;", &[])
                .ok()?
                .l()
                .ok()?;
            let cpath = env
                .call_method(&cache_dir, "getAbsolutePath", "()Ljava/lang/String;", &[])
                .ok()?
                .l()
                .ok()?;
            let cstr: String = env.get_string(&JString::from(cpath)).ok()?.into();
            let dex_file = Path::new(&cstr).join("rg_bridge.dex");
            let _ = std::fs::write(&dex_file, BRIDGE_DEX);
            let jdex = env.new_string(dex_file.to_string_lossy().as_ref()).ok()?;
            let jcstr = env.new_string(&cstr).ok()?;
            env.new_object(
                "dalvik/system/DexClassLoader",
                "(Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/ClassLoader;)V",
                &[
                    JValue::Object(&jdex),
                    JValue::Object(&jcstr),
                    JValue::Object(&JObject::null()),
                    JValue::Object(&parent_loader),
                ],
            )
            .ok()?
        };

        let cname = env.new_string("app.rustgramm.RgBridge").ok()?;
        let cls_obj = env
            .call_method(
                &loader,
                "loadClass",
                "(Ljava/lang/String;)Ljava/lang/Class;",
                &[JValue::Object(&cname)],
            )
            .ok()?
            .l()
            .ok()?;
        if let Ok(g) = env.new_global_ref(&cls_obj) {
            let _ = BRIDGE_CLASS.set(g);
        }
        Some(JClass::from(cls_obj))
    }

    pub fn ensure_runtime_permissions() {
        with_env(|env, act| {
            let sdk = sdk_int(env);
            let mut need = Vec::new();
            let check = |env: &mut JNIEnv<'_>, perm: &str| -> bool {
                let Ok(jp) = env.new_string(perm) else { return true };
                env.call_method(
                    act,
                    "checkSelfPermission",
                    "(Ljava/lang/String;)I",
                    &[JValue::Object(&jp)],
                )
                .ok()
                .and_then(|v| v.i().ok())
                == Some(0)
            };
            if sdk >= 23 && !check(env, "android.permission.RECORD_AUDIO") {
                need.push("android.permission.RECORD_AUDIO");
            }
            if sdk >= 33 && !check(env, "android.permission.POST_NOTIFICATIONS") {
                need.push("android.permission.POST_NOTIFICATIONS");
            }
            if !need.is_empty() {
                let scls = env.find_class("java/lang/String").ok()?;
                let arr = env.new_object_array(need.len() as i32, scls, JObject::null()).ok()?;
                for (i, p) in need.iter().enumerate() {
                    if let Ok(jp) = env.new_string(p) {
                        let _ = env.set_object_array_element(&arr, i as i32, jp);
                    }
                }
                let _ = env.call_method(
                    act,
                    "requestPermissions",
                    "([Ljava/lang/String;I)V",
                    &[JValue::Object(&arr), JValue::Int(44001)],
                );
            }
            Some(())
        });
    }

    pub fn stop_audio() {
        let slot = ACTIVE_PLAYER.get_or_init(|| Mutex::new(None));
        if let Ok(mut guard) = slot.lock()
            && let Some(player_ref) = guard.take()
        {
            with_env(|env, _| {
                let p = player_ref.as_obj();
                let _ = env.call_method(p, "stop", "()V", &[]);
                let _ = env.call_method(p, "release", "()V", &[]);
                Some(())
            });
        }
    }

    pub fn play_file(path: &Path, looping: bool, volume: f32) {
        stop_audio();
        let path_str = path.to_string_lossy().into_owned();
        let vol = volume.clamp(0.0, 1.0);
        with_env(|env, _| {
            let mp = env.new_object("android/media/MediaPlayer", "()V", &[]).ok()?;
            let jpath = env.new_string(&path_str).ok()?;
            env.call_method(&mp, "setDataSource", "(Ljava/lang/String;)V", &[JValue::Object(&jpath)])
                .ok()?;
            env.call_method(&mp, "setLooping", "(Z)V", &[JValue::Bool(u8::from(looping))])
                .ok()?;
            env.call_method(&mp, "setVolume", "(FF)V", &[JValue::Float(vol), JValue::Float(vol)])
                .ok()?;
            env.call_method(&mp, "prepare", "()V", &[]).ok()?;
            env.call_method(&mp, "start", "()V", &[]).ok()?;
            if let Ok(g) = env.new_global_ref(&mp)
                && let Ok(mut guard) = ACTIVE_PLAYER.get_or_init(|| Mutex::new(None)).lock()
            {
                *guard = Some(g);
            }
            Some(())
        });
    }

    pub fn play_wav_bytes(wav: &[u8], looping: bool, volume: f32) {
        let dir = crate::worker::home().join("cache");
        let _ = std::fs::create_dir_all(&dir);
        let p = dir.join(if looping { "ringtone.wav" } else { "chime.wav" });
        if std::fs::write(&p, wav).is_ok() {
            play_file(&p, looping, volume);
        }
    }

    pub fn show_notification(id: i32, title: &str, body: &str) {
        with_env(|env, act| {
            let jsvc = env.new_string("notification").ok()?;
            let nm = env
                .call_method(
                    act,
                    "getSystemService",
                    "(Ljava/lang/String;)Ljava/lang/Object;",
                    &[JValue::Object(&jsvc)],
                )
                .ok()?
                .l()
                .ok()?;
            if nm.is_null() {
                return None;
            }
            let chan_id = env.new_string("rustgramm_messages").ok()?;
            if sdk_int(env) >= 26 && !CHANNEL_CREATED.swap(true, Ordering::Relaxed) {
                let chan_name = env.new_string("Сообщения").ok()?;
                let ch = env
                    .new_object(
                        "android/app/NotificationChannel",
                        "(Ljava/lang/String;Ljava/lang/CharSequence;I)V",
                        &[
                            JValue::Object(&chan_id),
                            JValue::Object(&chan_name),
                            JValue::Int(4), // IMPORTANCE_HIGH
                        ],
                    )
                    .ok()?;
                let _ = env.call_method(
                    &nm,
                    "createNotificationChannel",
                    "(Landroid/app/NotificationChannel;)V",
                    &[JValue::Object(&ch)],
                );
            }
            let builder = if sdk_int(env) >= 26 {
                env.new_object(
                    "android/app/Notification$Builder",
                    "(Landroid/content/Context;Ljava/lang/String;)V",
                    &[JValue::Object(act), JValue::Object(&chan_id)],
                )
                .ok()?
            } else {
                env.new_object(
                    "android/app/Notification$Builder",
                    "(Landroid/content/Context;)V",
                    &[JValue::Object(act)],
                )
                .ok()?
            };
            let jt = env.new_string(title).ok()?;
            let jb = env.new_string(body).ok()?;
            let _ = env.call_method(
                &builder,
                "setContentTitle",
                "(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;",
                &[JValue::Object(&jt)],
            );
            let _ = env.call_method(
                &builder,
                "setContentText",
                "(Ljava/lang/CharSequence;)Landroid/app/Notification$Builder;",
                &[JValue::Object(&jb)],
            );
            // Системная иконка сообщения Android (`android.R.drawable.sym_action_chat` = 17301641).
            let _ = env.call_method(
                &builder,
                "setSmallIcon",
                "(I)Landroid/app/Notification$Builder;",
                &[JValue::Int(1_730_1641)],
            );
            let _ = env.call_method(
                &builder,
                "setAutoCancel",
                "(Z)Landroid/app/Notification$Builder;",
                &[JValue::Bool(1)],
            );
            let notif = env
                .call_method(&builder, "build", "()Landroid/app/Notification;", &[])
                .ok()?
                .l()
                .ok()?;
            let _ = env.call_method(
                &nm,
                "notify",
                "(ILandroid/app/Notification;)V",
                &[JValue::Int(id), JValue::Object(&notif)],
            );
            Some(())
        });
    }

    pub fn pick_file(mime: &str, dest_dir: &Path) {
        let _ = std::fs::create_dir_all(dest_dir);
        let mime_owned = mime.to_owned();
        let dir_owned = dest_dir.to_string_lossy().into_owned();
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let jm = env.new_string(&mime_owned).ok()?;
            let jd = env.new_string(&dir_owned).ok()?;
            let _ = env.call_static_method(
                cls,
                "pickFile",
                "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;)V",
                &[JValue::Object(act), JValue::Object(&jm), JValue::Object(&jd)],
            );
            Some(())
        });
    }

    pub fn poll_picked_file() -> Option<PathBuf> {
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let res = env
                .call_static_method(cls, "pollPickedFile", "()Ljava/lang/String;", &[])
                .ok()?
                .l()
                .ok()?;
            if res.is_null() {
                return None;
            }
            let s: String = env.get_string(&JString::from(res)).ok()?.into();
            (!s.is_empty()).then(|| PathBuf::from(s))
        })
    }

    pub fn open_webview(url: &str, title: &str, spoof_android: bool) {
        let u = url.to_owned();
        let t = title.to_owned();
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let ju = env.new_string(&u).ok()?;
            let jt = env.new_string(&t).ok()?;
            let _ = env.call_static_method(
                cls,
                "openWebView",
                "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;Z)V",
                &[
                    JValue::Object(act),
                    JValue::Object(&ju),
                    JValue::Object(&jt),
                    JValue::Bool(u8::from(spoof_android)),
                ],
            );
            Some(())
        });
    }

    pub fn close_webview() {
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let _ = env.call_static_method(
                cls,
                "closeWebView",
                "(Landroid/app/Activity;)V",
                &[JValue::Object(act)],
            );
            Some(())
        });
    }

    pub fn poll_pending_link() -> Option<String> {
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let res = env
                .call_static_method(cls, "pollPendingLink", "()Ljava/lang/String;", &[])
                .ok()?
                .l()
                .ok()?;
            if res.is_null() {
                return None;
            }
            let s: String = env.get_string(&JString::from(res)).ok()?.into();
            (!s.is_empty()).then_some(s)
        })
    }

    pub fn show_soft_input() {
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let _ = env.call_static_method(
                cls,
                "showSoftInput",
                "(Landroid/app/Activity;)V",
                &[JValue::Object(act)],
            );
            Some(())
        });
    }

    pub fn prompt_text_input(tag: &str, title: &str, hint: &str, initial: &str, kind: i32) {
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let jtag = env.new_string(tag).ok()?;
            let jtitle = env.new_string(title).ok()?;
            let jhint = env.new_string(hint).ok()?;
            let jinit = env.new_string(initial).ok()?;
            let _ = env.call_static_method(
                cls,
                "promptTextInput",
                "(Landroid/app/Activity;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;Ljava/lang/String;I)V",
                &[
                    JValue::Object(act),
                    JValue::Object(&jtag),
                    JValue::Object(&jtitle),
                    JValue::Object(&jhint),
                    JValue::Object(&jinit),
                    JValue::Int(kind),
                ],
            );
            Some(())
        });
    }

    pub fn poll_text_input() -> Option<(String, String)> {
        with_env(|env, act| {
            let cls = bridge_class(env, act)?;
            let tag_obj = env
                .call_static_method(&cls, "pollInputTag", "()Ljava/lang/String;", &[])
                .ok()?
                .l()
                .ok()?;
            if tag_obj.is_null() {
                return None;
            }
            let tag: String = env.get_string(&JString::from(tag_obj)).ok()?.into();
            let val_obj = env
                .call_static_method(&cls, "pollInputResult", "()Ljava/lang/String;", &[])
                .ok()?
                .l()
                .ok()?;
            if tag.is_empty() || val_obj.is_null() {
                return Some((String::new(), String::new()));
            }
            let val: String = env.get_string(&JString::from(val_obj)).ok()?.into();
            Some((tag, val))
        })
    }

    pub fn shutdown_all() {
        stop_audio();
        close_webview();
    }

    /// Запись микрофона на Android через `android.media.AudioRecord` (16 кГц моно PCM16).
    pub struct AndroidVoiceRecorder {
        stop_flag: Arc<AtomicBool>,
        samples: Arc<Mutex<Vec<i16>>>,
        level_bits: Arc<AtomicU32>,
        bars: Arc<Mutex<Vec<f32>>>,
        started: Instant,
        handle: Option<std::thread::JoinHandle<()>>,
    }

    impl AndroidVoiceRecorder {
        pub fn start() -> Result<Self, String> {
            ensure_runtime_permissions();
            let stop_flag = Arc::new(AtomicBool::new(false));
            let samples = Arc::new(Mutex::new(Vec::<i16>::with_capacity(16_000 * 10)));
            let level_bits = Arc::new(AtomicU32::new(0));
            let bars = Arc::new(Mutex::new(Vec::<f32>::with_capacity(64)));
            let (tx, rx) = std::sync::mpsc::channel::<Result<(), String>>();

            let sf = stop_flag.clone();
            let sm = samples.clone();
            let lb = level_bits.clone();
            let br = bars.clone();

            let handle = std::thread::Builder::new()
                .name("rg-android-rec".into())
                .spawn(move || {
                    let Some(ctx) = CTX.get() else {
                        let _ = tx.send(Err("Android JVM не инициализирована".into()));
                        return;
                    };
                    let Ok(mut env) = ctx.vm.attach_current_thread() else {
                        let _ = tx.send(Err("Не удалось подключить поток к JNI".into()));
                        return;
                    };
                    const SR: i32 = 16_000;
                    const CH_IN_MONO: i32 = 16;
                    const ENC_PCM16: i32 = 2;
                    let min_buf = env
                        .call_static_method(
                            "android/media/AudioRecord",
                            "getMinBufferSize",
                            "(III)I",
                            &[JValue::Int(SR), JValue::Int(CH_IN_MONO), JValue::Int(ENC_PCM16)],
                        )
                        .ok()
                        .and_then(|v| v.i().ok())
                        .unwrap_or(4096)
                        .max(4096);
                    let Ok(rec) = env.new_object(
                        "android/media/AudioRecord",
                        "(IIIII)V",
                        &[
                            JValue::Int(1), // MediaRecorder.AudioSource.MIC
                            JValue::Int(SR),
                            JValue::Int(CH_IN_MONO),
                            JValue::Int(ENC_PCM16),
                            JValue::Int(min_buf * 2),
                        ],
                    ) else {
                        let _ = tx.send(Err("Нет разрешения RECORD_AUDIO или микрофон занят".into()));
                        return;
                    };
                    let state = env
                        .call_method(&rec, "getState", "()I", &[])
                        .ok()
                        .and_then(|v| v.i().ok())
                        .unwrap_or(0);
                    if state != 1 {
                        let _ = env.call_method(&rec, "release", "()V", &[]);
                        let _ = tx.send(Err("AudioRecord не готов (проверьте разрешение на микрофон)".into()));
                        return;
                    }
                    let _ = env.call_method(&rec, "startRecording", "()V", &[]);
                    let _ = tx.send(Ok(()));

                    let chunk = 800usize;
                    let Ok(jbuf) = env.new_short_array(chunk as i32) else {
                        let _ = env.call_method(&rec, "release", "()V", &[]);
                        return;
                    };
                    let mut host_buf = vec![0i16; chunk];
                    while !sf.load(Ordering::Relaxed) {
                        let read_n = env
                            .call_method(
                                &rec,
                                "read",
                                "([SII)I",
                                &[JValue::Object(&jbuf), JValue::Int(0), JValue::Int(chunk as i32)],
                            )
                            .ok()
                            .and_then(|v| v.i().ok())
                            .unwrap_or(0);
                        if read_n > 0 {
                            let n = (read_n as usize).min(chunk);
                            if env.get_short_array_region(&jbuf, 0, &mut host_buf[..n]).is_ok() {
                                let mut sum_sq = 0.0_f64;
                                if let Ok(mut g) = sm.lock() {
                                    g.extend_from_slice(&host_buf[..n]);
                                }
                                for &s in &host_buf[..n] {
                                    let f = f64::from(s);
                                    sum_sq += f * f;
                                }
                                let rms = ((sum_sq / n.max(1) as f64).sqrt() / 12_000.0).clamp(0.0, 1.0) as f32;
                                lb.store(rms.to_bits(), Ordering::Relaxed);
                                if let Ok(mut bg) = br.lock() {
                                    bg.push(rms);
                                    if bg.len() > 48 {
                                        bg.remove(0);
                                    }
                                }
                            }
                        } else {
                            std::thread::sleep(std::time::Duration::from_millis(16));
                        }
                    }
                    let _ = env.call_method(&rec, "stop", "()V", &[]);
                    let _ = env.call_method(&rec, "release", "()V", &[]);
                })
                .map_err(|e| format!("Поток AudioRecord: {e}"))?;

            match rx.recv_timeout(std::time::Duration::from_secs(3)) {
                Ok(Ok(())) => Ok(Self {
                    stop_flag,
                    samples,
                    level_bits,
                    bars,
                    started: Instant::now(),
                    handle: Some(handle),
                }),
                Ok(Err(e)) => Err(e),
                Err(_) => Err("Таймаут инициализации AudioRecord".into()),
            }
        }

        pub fn elapsed_secs(&self) -> f32 {
            self.started.elapsed().as_secs_f32()
        }

        pub fn live_level(&self) -> f32 {
            f32::from_bits(self.level_bits.load(Ordering::Relaxed))
        }

        pub fn recent_bars(&self) -> Vec<f32> {
            self.bars.lock().map(|g| g.clone()).unwrap_or_default()
        }

        pub fn finish_to_wav(mut self, dest: &Path) -> Result<(u32, Vec<u8>), String> {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
            let data = self.samples.lock().map(|g| g.clone()).unwrap_or_default();
            if data.len() < 1600 {
                return Err("Запись слишком короткая".into());
            }
            let dur = ((data.len() as f32) / 16_000.0).round().max(1.0) as u32;
            let peaks = crate::audio_io::compute_waveform_peaks(&data, 63);
            let wav = crate::audio_io::pcm16_to_wav(&data, 16_000);
            if let Some(p) = dest.parent() {
                let _ = std::fs::create_dir_all(p);
            }
            std::fs::write(dest, &wav).map_err(|e| format!("Сохранение голосового: {e}"))?;
            Ok((dur, peaks))
        }

        pub fn cancel(mut self) {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }

    impl Drop for AndroidVoiceRecorder {
        fn drop(&mut self) {
            self.stop_flag.store(true, Ordering::Relaxed);
            if let Some(h) = self.handle.take() {
                let _ = h.join();
            }
        }
    }
}

#[cfg(not(target_os = "android"))]
mod imp {
    use super::*;

    pub fn init(_vm_ptr: *mut c_void, _activity_ptr: *mut c_void) {}
    pub fn ensure_runtime_permissions() {}
    pub fn stop_audio() {}
    pub fn play_file(_path: &Path, _looping: bool, _volume: f32) {}
    pub fn play_wav_bytes(_wav: &[u8], _looping: bool, _volume: f32) {}
    pub fn show_notification(_id: i32, _title: &str, _body: &str) {}
    pub fn pick_file(_mime: &str, _dest_dir: &Path) {}
    pub fn poll_picked_file() -> Option<PathBuf> {
        None
    }
    pub fn open_webview(_url: &str, _title: &str, _spoof_android: bool) {}
    pub fn close_webview() {}
    pub fn poll_pending_link() -> Option<String> {
        None
    }
    pub fn show_soft_input() {}
    pub fn prompt_text_input(_tag: &str, _title: &str, _hint: &str, _initial: &str, _kind: i32) {}
    pub fn poll_text_input() -> Option<(String, String)> {
        None
    }
    pub fn shutdown_all() {}
}

pub use imp::*;
