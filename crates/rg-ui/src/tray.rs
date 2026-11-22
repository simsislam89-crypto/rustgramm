//! Системный трей Windows (`tray.cpp` + `platform/win/tray_win.cpp`):
//! - значок в области уведомлений (`Shell_NotifyIconW`) с динамическим бейджем непрочитанных
//!   и учётом 12 выбранных иконок приложения / режима призрака,
//! - восстановление значка при перезапуске Explorer (`TaskbarCreated`),
//! - контекстное меню на русском: Открыть/Свернуть, Уведомления, Режим призрака,
//!   Режим стримера и Выход (выбор отображаемых переключателей — в настройках Rustgramm),
//! - левый клик — показать/свернуть окно.

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TrayState {
    pub enabled: bool,
    pub window_visible: bool,
    pub icon_idx: usize,
    pub unread: i32,
    pub unread_muted: bool,
    pub hide_badge: bool,
    pub desktop_notify: bool,
    pub ghost_active: bool,
    pub streamer_mode: bool,
    pub show_ghost_toggle: bool,
    pub show_streamer_toggle: bool,
}

impl Default for TrayState {
    fn default() -> Self {
        Self {
            enabled: true,
            window_visible: true,
            icon_idx: 0,
            unread: 0,
            unread_muted: false,
            hide_badge: false,
            desktop_notify: true,
            ghost_active: false,
            streamer_mode: false,
            show_ghost_toggle: true,
            show_streamer_toggle: false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrayEvent {
    ToggleWindow,
    ToggleNotifications,
    ToggleGhost,
    ToggleStreamer,
    Quit,
}

/// Формирует подсказку на иконке в трее (tooltip до 127 символов для `NOTIFYICONDATAW`).
pub fn format_tooltip(st: &TrayState) -> String {
    let mut parts = Vec::new();
    if st.unread > 0 && !st.hide_badge {
        parts.push(format!("непрочитанных: {}", st.unread));
    }
    if st.ghost_active {
        parts.push("призрак".to_owned());
    }
    if st.streamer_mode {
        parts.push("стример".to_owned());
    }
    if parts.is_empty() {
        "Rustgramm".to_owned()
    } else {
        let mut full = format!("Rustgramm — {}", parts.join(", "));
        if full.chars().count() > 120 {
            full = full.chars().take(120).collect();
        }
        full
    }
}

/// Пункты контекстного меню трея (для отрисовки Win32 и юнит-тестов).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MenuEntry {
    pub id: u16,
    pub label: &'static str,
    pub checked: bool,
}

pub const IDM_TOGGLE_WIN: u16 = 1001;
pub const IDM_TOGGLE_NOTIFY: u16 = 1002;
pub const IDM_TOGGLE_GHOST: u16 = 1003;
pub const IDM_TOGGLE_STREAMER: u16 = 1004;
pub const IDM_QUIT: u16 = 1005;

pub fn build_menu_entries(st: &TrayState) -> Vec<MenuEntry> {
    let mut items = vec![
        MenuEntry {
            id: IDM_TOGGLE_WIN,
            label: if st.window_visible {
                "Свернуть в трей"
            } else {
                "Открыть Rustgramm"
            },
            checked: false,
        },
        MenuEntry {
            id: IDM_TOGGLE_NOTIFY,
            label: if st.desktop_notify {
                "Выключить уведомления"
            } else {
                "Включить уведомления"
            },
            checked: false,
        },
    ];
    if st.show_ghost_toggle {
        items.push(MenuEntry {
            id: IDM_TOGGLE_GHOST,
            label: if st.ghost_active {
                "Выключить режим призрака"
            } else {
                "Включить режим призрака"
            },
            checked: st.ghost_active,
        });
    }
    if st.show_streamer_toggle {
        items.push(MenuEntry {
            id: IDM_TOGGLE_STREAMER,
            label: if st.streamer_mode {
                "Выключить режим стримера"
            } else {
                "Включить режим стримера"
            },
            checked: st.streamer_mode,
        });
    }
    items.push(MenuEntry {
        id: IDM_QUIT,
        label: "Выйти из Rustgramm",
        checked: false,
    });
    items
}

#[cfg(windows)]
mod imp {
    use std::ffi::c_void;
    use std::sync::atomic::{AtomicIsize, Ordering};
    use std::sync::mpsc::{Receiver, Sender, channel};
    use std::sync::{Arc, Mutex, OnceLock};
    use std::thread::JoinHandle;

    use eframe::egui;

    use super::{
        IDM_QUIT, IDM_TOGGLE_GHOST, IDM_TOGGLE_NOTIFY, IDM_TOGGLE_STREAMER, IDM_TOGGLE_WIN, TrayEvent,
        TrayState, build_menu_entries, format_tooltip,
    };
    use crate::win::{DestroyIcon, render_app_icon_rgba, rgba_to_hicon, wide};

    const WM_USER: u32 = 0x0400;
    const WM_TRAY_ICON: u32 = WM_USER + 1;
    const WM_TRAY_SYNC: u32 = WM_USER + 2;
    const WM_TRAY_QUIT: u32 = WM_USER + 3;
    const WM_DESTROY: u32 = 0x0002;
    const WM_CLOSE: u32 = 0x0010;
    const WM_COMMAND: u32 = 0x0111;
    const WM_LBUTTONUP: u32 = 0x0202;
    const WM_RBUTTONUP: u32 = 0x0205;
    const WM_CONTEXTMENU: u32 = 0x007B;
    const WM_NULL: u32 = 0x0000;

    const NIM_ADD: u32 = 0x0000_0000;
    const NIM_MODIFY: u32 = 0x0000_0001;
    const NIM_DELETE: u32 = 0x0000_0002;
    const NIF_MESSAGE: u32 = 0x0000_0001;
    const NIF_ICON: u32 = 0x0000_0002;
    const NIF_TIP: u32 = 0x0000_0004;

    const MF_STRING: u32 = 0x0000_0000;
    const MF_SEPARATOR: u32 = 0x0000_0800;
    const MF_CHECKED: u32 = 0x0000_0008;
    const TPM_RIGHTBUTTON: u32 = 0x0002;
    const TPM_RETURNCMD: u32 = 0x0100;
    const TPM_NONOTIFY: u32 = 0x0080;

    type WndProcFn = unsafe extern "system" fn(*mut c_void, u32, usize, isize) -> isize;

    #[repr(C)]
    struct Guid(u32, u16, u16, [u8; 8]);

    #[repr(C)]
    struct NotifyIconDataW {
        cb_size: u32,
        hwnd: *mut c_void,
        u_id: u32,
        u_flags: u32,
        u_callback_message: u32,
        h_icon: *mut c_void,
        sz_tip: [u16; 128],
        dw_state: u32,
        dw_state_mask: u32,
        sz_info: [u16; 256],
        u_timeout_or_version: u32,
        sz_info_title: [u16; 64],
        dw_info_flags: u32,
        guid_item: Guid,
        h_balloon_icon: *mut c_void,
    }

    #[repr(C)]
    struct WndClassExW {
        cb_size: u32,
        style: u32,
        lpfn_wnd_proc: WndProcFn,
        cb_cls_extra: i32,
        cb_wnd_extra: i32,
        h_instance: *mut c_void,
        h_icon: *mut c_void,
        h_cursor: *mut c_void,
        hbr_background: *mut c_void,
        lpsz_menu_name: *const u16,
        lpsz_class_name: *const u16,
        h_icon_sm: *mut c_void,
    }

    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }

    #[repr(C)]
    struct Msg {
        hwnd: *mut c_void,
        message: u32,
        w_param: usize,
        l_param: isize,
        time: u32,
        pt: Point,
        l_private: u32,
    }

    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassExW(wc: *const WndClassExW) -> u16;
        fn CreateWindowExW(
            dw_ex_style: u32,
            class_name: *const u16,
            window_name: *const u16,
            dw_style: u32,
            x: i32,
            y: i32,
            n_width: i32,
            n_height: i32,
            hwnd_parent: *mut c_void,
            h_menu: *mut c_void,
            h_instance: *mut c_void,
            lp_param: *mut c_void,
        ) -> *mut c_void;
        fn DestroyWindow(hwnd: *mut c_void) -> i32;
        fn DefWindowProcW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> isize;
        fn GetMessageW(lp_msg: *mut Msg, hwnd: *mut c_void, w_min: u32, w_max: u32) -> i32;
        fn TranslateMessage(lp_msg: *const Msg) -> i32;
        fn DispatchMessageW(lp_msg: *const Msg) -> isize;
        fn PostMessageW(hwnd: *mut c_void, msg: u32, wparam: usize, lparam: isize) -> i32;
        fn PostQuitMessage(exit_code: i32);
        fn RegisterWindowMessageW(lp_string: *const u16) -> u32;
        fn GetCursorPos(lp_point: *mut Point) -> i32;
        fn SetForegroundWindow(hwnd: *mut c_void) -> i32;
        fn CreatePopupMenu() -> *mut c_void;
        fn AppendMenuW(h_menu: *mut c_void, u_flags: u32, u_id_new_item: usize, lp_new_item: *const u16) -> i32;
        fn TrackPopupMenu(
            h_menu: *mut c_void,
            u_flags: u32,
            x: i32,
            y: i32,
            n_reserved: i32,
            hwnd: *mut c_void,
            prc_rect: *const c_void,
        ) -> i32;
        fn DestroyMenu(h_menu: *mut c_void) -> i32;
    }

    #[link(name = "shell32")]
    unsafe extern "system" {
        fn Shell_NotifyIconW(dw_message: u32, lp_data: *const NotifyIconDataW) -> i32;
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetModuleHandleW(module_name: *const u16) -> *mut c_void;
    }

    struct Runtime {
        state: TrayState,
        tx: Sender<TrayEvent>,
        ctx: egui::Context,
        added: bool,
        icon: isize,
        taskbar_msg: u32,
    }

    static RUNTIME: OnceLock<Mutex<Option<Arc<Mutex<Runtime>>>>> = OnceLock::new();

    fn runtime_slot() -> &'static Mutex<Option<Arc<Mutex<Runtime>>>> {
        RUNTIME.get_or_init(|| Mutex::new(None))
    }

    fn build_nid(hwnd: *mut c_void, hicon: *mut c_void, tip: &str, flags: u32) -> NotifyIconDataW {
        let mut sz_tip = [0u16; 128];
        for (i, unit) in tip.encode_utf16().take(127).enumerate() {
            sz_tip[i] = unit;
        }
        NotifyIconDataW {
            cb_size: std::mem::size_of::<NotifyIconDataW>() as u32,
            hwnd,
            u_id: 1,
            u_flags: flags,
            u_callback_message: WM_TRAY_ICON,
            h_icon: hicon,
            sz_tip,
            dw_state: 0,
            dw_state_mask: 0,
            sz_info: [0u16; 256],
            u_timeout_or_version: 0,
            sz_info_title: [0u16; 64],
            dw_info_flags: 0,
            guid_item: Guid(0, 0, 0, [0; 8]),
            h_balloon_icon: std::ptr::null_mut(),
        }
    }

    fn sync_icon(hwnd: *mut c_void, rt: &mut Runtime) {
        if !rt.state.enabled {
            if rt.added {
                let nid = build_nid(hwnd, std::ptr::null_mut(), "Rustgramm", 0);
                // SAFETY: валидная структура NotifyIconDataW.
                unsafe {
                    Shell_NotifyIconW(NIM_DELETE, &raw const nid);
                }
                rt.added = false;
            }
            return;
        }
        let rgba = render_app_icon_rgba(
            rt.state.icon_idx,
            rt.state.unread,
            rt.state.unread_muted,
            rt.state.hide_badge,
            rt.state.ghost_active,
            32,
        );
        let new_hicon = rgba_to_hicon(&rgba, 32, 32);
        let tip = format_tooltip(&rt.state);
        let flags = NIF_MESSAGE | NIF_TIP | if new_hicon.is_null() { 0 } else { NIF_ICON };
        let use_icon = if new_hicon.is_null() {
            rt.icon as *mut c_void
        } else {
            new_hicon
        };
        let nid = build_nid(hwnd, use_icon, &tip, flags);
        let cmd = if rt.added { NIM_MODIFY } else { NIM_ADD };
        // SAFETY: валидные hwnd и hicon.
        let ok = unsafe { Shell_NotifyIconW(cmd, &raw const nid) };
        if ok != 0 {
            rt.added = true;
        }
        if !new_hicon.is_null() {
            if rt.icon != 0 {
                // SAFETY: старый HICON больше не используется Shell_NotifyIconW.
                unsafe {
                    DestroyIcon(rt.icon as *mut c_void);
                }
            }
            rt.icon = new_hicon as isize;
        }
    }

    fn show_context_menu(hwnd: *mut c_void, rt: &Runtime) {
        let entries = build_menu_entries(&rt.state);
        // SAFETY: вызовы user32 CreatePopupMenu/AppendMenuW/TrackPopupMenu в UI-потоке трея.
        unsafe {
            let menu = CreatePopupMenu();
            if menu.is_null() {
                return;
            }
            for e in &entries {
                if e.id == IDM_QUIT {
                    AppendMenuW(menu, MF_SEPARATOR, 0, std::ptr::null());
                }
                let mut flags = MF_STRING;
                if e.checked {
                    flags |= MF_CHECKED;
                }
                let wlabel = wide(e.label);
                AppendMenuW(menu, flags, usize::from(e.id), wlabel.as_ptr());
            }
            let mut pt = Point { x: 0, y: 0 };
            GetCursorPos(&raw mut pt);
            SetForegroundWindow(hwnd);
            let cmd = TrackPopupMenu(
                menu,
                TPM_RIGHTBUTTON | TPM_RETURNCMD | TPM_NONOTIFY,
                pt.x,
                pt.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            PostMessageW(hwnd, WM_NULL, 0, 0);
            DestroyMenu(menu);
            if cmd > 0 {
                let ev = match cmd as u16 {
                    IDM_TOGGLE_WIN => Some(TrayEvent::ToggleWindow),
                    IDM_TOGGLE_NOTIFY => Some(TrayEvent::ToggleNotifications),
                    IDM_TOGGLE_GHOST => Some(TrayEvent::ToggleGhost),
                    IDM_TOGGLE_STREAMER => Some(TrayEvent::ToggleStreamer),
                    IDM_QUIT => Some(TrayEvent::Quit),
                    _ => None,
                };
                if let Some(ev) = ev {
                    let _ = rt.tx.send(ev);
                    rt.ctx.request_repaint();
                }
            }
        }
    }

    unsafe extern "system" fn wnd_proc(
        hwnd: *mut c_void,
        msg: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        let slot = runtime_slot()
            .lock()
            .ok()
            .and_then(|g| g.clone());
        if let Some(arc) = slot
            && let Ok(mut rt) = arc.lock()
        {
            if rt.taskbar_msg != 0 && msg == rt.taskbar_msg {
                rt.added = false;
                sync_icon(hwnd, &mut rt);
                return 0;
            }
            match msg {
                WM_TRAY_SYNC => {
                    sync_icon(hwnd, &mut rt);
                    return 0;
                }
                WM_TRAY_QUIT | WM_CLOSE => {
                    if rt.added {
                        let nid = build_nid(hwnd, std::ptr::null_mut(), "Rustgramm", 0);
                        // SAFETY: удаление иконки при завершении.
                        unsafe {
                            Shell_NotifyIconW(NIM_DELETE, &raw const nid);
                        }
                        rt.added = false;
                    }
                    if rt.icon != 0 {
                        // SAFETY: освобождение кэшированного HICON.
                        unsafe {
                            DestroyIcon(rt.icon as *mut c_void);
                        }
                        rt.icon = 0;
                    }
                    // SAFETY: закрытие вспомогательного окна.
                    unsafe {
                        DestroyWindow(hwnd);
                    }
                    return 0;
                }
                WM_TRAY_ICON => {
                    let code = (lparam as u32) & 0xFFFF;
                    if code == WM_LBUTTONUP {
                        let _ = rt.tx.send(TrayEvent::ToggleWindow);
                        rt.ctx.request_repaint();
                    } else if code == WM_RBUTTONUP || code == WM_CONTEXTMENU {
                        show_context_menu(hwnd, &rt);
                    }
                    return 0;
                }
                WM_COMMAND => {
                    let id = (wparam & 0xFFFF) as u16;
                    let ev = match id {
                        IDM_TOGGLE_WIN => Some(TrayEvent::ToggleWindow),
                        IDM_TOGGLE_NOTIFY => Some(TrayEvent::ToggleNotifications),
                        IDM_TOGGLE_GHOST => Some(TrayEvent::ToggleGhost),
                        IDM_TOGGLE_STREAMER => Some(TrayEvent::ToggleStreamer),
                        IDM_QUIT => Some(TrayEvent::Quit),
                        _ => None,
                    };
                    if let Some(ev) = ev {
                        let _ = rt.tx.send(ev);
                        rt.ctx.request_repaint();
                    }
                    return 0;
                }
                WM_DESTROY => {
                    // SAFETY: остановка цикла сообщений потока трея.
                    unsafe {
                        PostQuitMessage(0);
                    }
                    return 0;
                }
                _ => {}
            }
        }
        // SAFETY: стандартная обработка оконных сообщений.
        unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) }
    }

    pub struct TrayManager {
        shared: Arc<Mutex<Runtime>>,
        hwnd: Arc<AtomicIsize>,
        rx: Receiver<TrayEvent>,
        last_sent: TrayState,
        join: Option<JoinHandle<()>>,
    }

    impl TrayManager {
        pub fn start(ctx: egui::Context, initial: TrayState) -> Self {
            let (tx, rx) = channel();
            let taskbar_msg = {
                let w = wide("TaskbarCreated");
                // SAFETY: UTF-16 строка с нулём.
                unsafe { RegisterWindowMessageW(w.as_ptr()) }
            };
            let rt = Arc::new(Mutex::new(Runtime {
                state: initial.clone(),
                tx,
                ctx,
                added: false,
                icon: 0,
                taskbar_msg,
            }));
            if let Ok(mut guard) = runtime_slot().lock() {
                *guard = Some(Arc::clone(&rt));
            }
            let hwnd_cell = Arc::new(AtomicIsize::new(0));
            let hwnd_clone = Arc::clone(&hwnd_cell);
            let rt_clone = Arc::clone(&rt);
            let join = std::thread::Builder::new()
                .name("rg-tray".into())
                .spawn(move || {
                    let cls = wide("RustgrammTrayWnd");
                    let title = wide("RustgrammTray");
                    // SAFETY: регистрация скрытого окна трея и цикл GetMessageW в собственном потоке.
                    unsafe {
                        let hinst = GetModuleHandleW(std::ptr::null());
                        let wc = WndClassExW {
                            cb_size: std::mem::size_of::<WndClassExW>() as u32,
                            style: 0,
                            lpfn_wnd_proc: wnd_proc,
                            cb_cls_extra: 0,
                            cb_wnd_extra: 0,
                            h_instance: hinst,
                            h_icon: std::ptr::null_mut(),
                            h_cursor: std::ptr::null_mut(),
                            hbr_background: std::ptr::null_mut(),
                            lpsz_menu_name: std::ptr::null(),
                            lpsz_class_name: cls.as_ptr(),
                            h_icon_sm: std::ptr::null_mut(),
                        };
                        RegisterClassExW(&raw const wc);
                        let hwnd = CreateWindowExW(
                            0,
                            cls.as_ptr(),
                            title.as_ptr(),
                            0,
                            0,
                            0,
                            0,
                            0,
                            std::ptr::null_mut(),
                            std::ptr::null_mut(),
                            hinst,
                            std::ptr::null_mut(),
                        );
                        if hwnd.is_null() {
                            return;
                        }
                        hwnd_clone.store(hwnd as isize, Ordering::Release);
                        if let Ok(mut guard) = rt_clone.lock() {
                            sync_icon(hwnd, &mut guard);
                        }
                        let mut msg: Msg = std::mem::zeroed();
                        while GetMessageW(&raw mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                            TranslateMessage(&raw const msg);
                            DispatchMessageW(&raw const msg);
                        }
                    }
                })
                .ok();
            Self {
                shared: rt,
                hwnd: hwnd_cell,
                rx,
                last_sent: initial,
                join,
            }
        }

        pub fn sync(&mut self, next: TrayState) {
            if next == self.last_sent {
                return;
            }
            self.last_sent = next.clone();
            if let Ok(mut guard) = self.shared.lock() {
                guard.state = next;
            }
            let raw = self.hwnd.load(Ordering::Acquire);
            if raw != 0 {
                // SAFETY: валидный HWND вспомогательного окна.
                unsafe {
                    PostMessageW(raw as *mut c_void, WM_TRAY_SYNC, 0, 0);
                }
            }
        }

        pub fn poll(&self) -> Vec<TrayEvent> {
            let mut out = Vec::new();
            while let Ok(ev) = self.rx.try_recv() {
                out.push(ev);
            }
            out
        }

        pub fn shutdown(&mut self) {
            let raw = self.hwnd.load(Ordering::Acquire);
            if raw != 0 {
                // SAFETY: уведомление потока трея об удалении иконки и выходе.
                unsafe {
                    PostMessageW(raw as *mut c_void, WM_TRAY_QUIT, 0, 0);
                }
            }
            if let Some(j) = self.join.take() {
                let _ = j.join();
            }
        }
    }

    impl Drop for TrayManager {
        fn drop(&mut self) {
            self.shutdown();
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use super::{TrayEvent, TrayState};
    use eframe::egui;
    use std::sync::mpsc::{Receiver, channel};

    pub struct TrayManager {
        _rx: Receiver<TrayEvent>,
    }

    impl TrayManager {
        pub fn start(_ctx: egui::Context, _initial: TrayState) -> Self {
            let (_tx, rx) = channel();
            Self { _rx: rx }
        }
        pub fn sync(&mut self, _next: TrayState) {}
        pub fn poll(&self) -> Vec<TrayEvent> {
            Vec::new()
        }
        pub fn shutdown(&mut self) {}
    }
}

pub use imp::TrayManager;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_reflects_visibility_and_toggles() {
        let mut st = TrayState::default();
        let entries = build_menu_entries(&st);
        assert_eq!(entries[0].label, "Свернуть в трей");
        assert!(entries.iter().any(|e| e.id == IDM_TOGGLE_GHOST));
        assert!(!entries.iter().any(|e| e.id == IDM_TOGGLE_STREAMER));

        st.window_visible = false;
        st.ghost_active = true;
        st.show_streamer_toggle = true;
        st.streamer_mode = true;
        let entries2 = build_menu_entries(&st);
        assert_eq!(entries2[0].label, "Открыть Rustgramm");
        assert!(entries2.iter().any(|e| e.id == IDM_TOGGLE_GHOST && e.checked));
        assert!(entries2.iter().any(|e| e.id == IDM_TOGGLE_STREAMER && e.checked));
    }

    #[test]
    fn tooltip_includes_unread_and_modes() {
        let st = TrayState {
            unread: 12,
            ghost_active: true,
            streamer_mode: true,
            ..TrayState::default()
        };
        let tip = format_tooltip(&st);
        assert!(tip.contains("непрочитанных: 12"));
        assert!(tip.contains("призрак"));
        assert!(tip.contains("стример"));
    }

    #[test]
    fn tooltip_and_menu_honor_hide_badge_and_notification_flags() {
        let st = TrayState {
            unread: 25,
            hide_badge: true,
            desktop_notify: false,
            show_ghost_toggle: false,
            ..TrayState::default()
        };
        let tip = format_tooltip(&st);
        assert!(!tip.contains("непрочитанных"));
        let entries = build_menu_entries(&st);
        assert!(!entries.iter().any(|e| e.id == IDM_TOGGLE_GHOST));
        assert!(entries.iter().any(|e| e.id == IDM_TOGGLE_NOTIFY && !e.checked));
    }
}
