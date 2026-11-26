//! Close to tray: when the setting is on, closing the window hides it and leaves an
//! icon in the notification area, so the process (and the phone bridge thread) keeps
//! running. Ending the process from Task Manager still takes the bridge offline.
//! Windows only; elsewhere closing quits as before.
use super::Editor;
use super::i18n::tr;
use eframe::egui;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum TrayEvent {
    Open,
    Quit,
}

pub(super) fn supported() -> bool {
    cfg!(windows)
}

/// The native handle of the main window, as an integer so it can cross threads.
pub(super) fn window_of(frame: &eframe::Frame) -> Option<isize> {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    match frame.window_handle().ok()?.as_raw() {
        RawWindowHandle::Win32(handle) => Some(handle.hwnd.get()),
        _ => None,
    }
}

#[cfg(windows)]
mod native {
    use super::TrayEvent;
    use eframe::egui;
    use std::cell::RefCell;
    use std::sync::mpsc::Sender;
    use windows_sys::Win32::Foundation::{HWND, LPARAM, LRESULT, POINT, WPARAM};
    use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
    use windows_sys::Win32::UI::Shell::{
        NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
    };
    use windows_sys::Win32::UI::WindowsAndMessaging::{
        AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DestroyWindow,
        DispatchMessageW, GetCursorPos, GetMessageW, GetSystemMetrics, HWND_MESSAGE,
        IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTCOLOR, LoadIconW, LoadImageW, MF_STRING, MSG,
        PostMessageW, PostQuitMessage, RegisterClassW, SM_CXSMICON, SW_HIDE, SW_SHOW,
        SetForegroundWindow, ShowWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON, TrackPopupMenu,
        TranslateMessage, WM_APP, WM_CLOSE, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP,
        WM_RBUTTONUP, WNDCLASSW,
    };

    const CALLBACK: u32 = WM_APP + 1;
    const OPEN: usize = 1;
    const QUIT: usize = 2;

    struct State {
        tx: Sender<TrayEvent>,
        main: isize,
        ctx: egui::Context,
    }

    thread_local! {
        static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
    }

    fn wide(text: &str) -> Vec<u16> {
        text.encode_utf16().chain(std::iter::once(0)).collect()
    }

    /// Shows the main window again, reports the choice and removes the icon.
    fn finish(icon_window: HWND, event: TrayEvent) {
        STATE.with(|s| {
            if let Some(state) = s.borrow().as_ref() {
                // SAFETY: `main` is the live top-level window of this process.
                unsafe {
                    ShowWindow(state.main as HWND, SW_SHOW);
                    SetForegroundWindow(state.main as HWND);
                }
                let _ = state.tx.send(event);
                state.ctx.request_repaint();
            }
        });
        // SAFETY: the icon window belongs to this thread.
        unsafe { DestroyWindow(icon_window) };
    }

    fn menu_choice(hwnd: HWND) -> usize {
        let open = wide(super::tr("Open Covalos"));
        let quit = wide(super::tr("Quit"));
        // SAFETY: the menu is created and destroyed on this thread; the window has to be
        // foreground for the menu to close on an outside click.
        unsafe {
            let menu = CreatePopupMenu();
            AppendMenuW(menu, MF_STRING, OPEN, open.as_ptr());
            AppendMenuW(menu, MF_STRING, QUIT, quit.as_ptr());
            let mut at = POINT { x: 0, y: 0 };
            GetCursorPos(&mut at);
            SetForegroundWindow(hwnd);
            let choice = TrackPopupMenu(
                menu,
                TPM_RETURNCMD | TPM_RIGHTBUTTON,
                at.x,
                at.y,
                0,
                hwnd,
                std::ptr::null(),
            );
            DestroyMenu(menu);
            choice as usize
        }
    }

    unsafe extern "system" fn proc(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> LRESULT {
        match msg {
            CALLBACK => {
                let mouse = (lparam & 0xffff) as u32;
                if mouse == WM_LBUTTONUP || mouse == WM_LBUTTONDBLCLK {
                    finish(hwnd, TrayEvent::Open);
                } else if mouse == WM_RBUTTONUP {
                    match menu_choice(hwnd) {
                        OPEN => finish(hwnd, TrayEvent::Open),
                        QUIT => finish(hwnd, TrayEvent::Quit),
                        _ => {}
                    }
                }
                0
            }
            WM_DESTROY => {
                // SAFETY: the icon was added for this window with id 1.
                unsafe {
                    let mut data: NOTIFYICONDATAW = std::mem::zeroed();
                    data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
                    data.hWnd = hwnd;
                    data.uID = 1;
                    Shell_NotifyIconW(NIM_DELETE, &data);
                    PostQuitMessage(0);
                }
                0
            }
            // SAFETY: default handling for every other message.
            _ => unsafe { DefWindowProcW(hwnd, msg, wparam, lparam) },
        }
    }

    /// Runs on its own thread: adds the icon, hides the main window and pumps messages
    /// until the icon window is destroyed. Reports the icon window handle first.
    pub(super) fn run(
        main: isize,
        tx: Sender<TrayEvent>,
        ready: Sender<Result<isize, String>>,
        ctx: egui::Context,
    ) {
        STATE.with(|s| *s.borrow_mut() = Some(State { tx, main, ctx }));
        let class = wide("CovalosTrayWindow");
        let tip = super::tr("COVALOS is running. The phone bridge stays connected.");
        // SAFETY: plain Win32 calls with valid, owned buffers; every handle is used on
        // this thread only.
        unsafe {
            let instance = GetModuleHandleW(std::ptr::null());
            let mut wc: WNDCLASSW = std::mem::zeroed();
            wc.lpfnWndProc = Some(proc);
            wc.hInstance = instance;
            wc.lpszClassName = class.as_ptr();
            // Registering again in the same process fails harmlessly.
            RegisterClassW(&wc);
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                class.as_ptr(),
                0,
                0,
                0,
                0,
                0,
                HWND_MESSAGE,
                std::ptr::null_mut(),
                instance,
                std::ptr::null(),
            );
            if hwnd.is_null() {
                let _ = ready.send(Err("Could not create the tray icon.".into()));
                return;
            }
            let size = GetSystemMetrics(SM_CXSMICON);
            // Group icon 1 is the app icon embedded by build.rs.
            let mut icon = LoadImageW(instance, 1 as _, IMAGE_ICON, size, size, LR_DEFAULTCOLOR);
            if icon.is_null() {
                icon = LoadIconW(std::ptr::null_mut(), IDI_APPLICATION) as _;
            }
            let mut data: NOTIFYICONDATAW = std::mem::zeroed();
            data.cbSize = size_of::<NOTIFYICONDATAW>() as u32;
            data.hWnd = hwnd;
            data.uID = 1;
            data.uFlags = NIF_MESSAGE | NIF_ICON | NIF_TIP;
            data.uCallbackMessage = CALLBACK;
            data.hIcon = icon as _;
            for (slot, unit) in data.szTip.iter_mut().zip(tip.encode_utf16().take(127)) {
                *slot = unit;
            }
            if Shell_NotifyIconW(NIM_ADD, &data) == 0 {
                DestroyWindow(hwnd);
                let _ = ready.send(Err("Could not create the tray icon.".into()));
                return;
            }
            ShowWindow(main as HWND, SW_HIDE);
            let _ = ready.send(Ok(hwnd as isize));
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, std::ptr::null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
        }
        STATE.with(|s| *s.borrow_mut() = None);
    }

    pub(super) fn close(icon_window: isize) {
        // SAFETY: posting to a window that is already gone is harmless.
        unsafe { PostMessageW(icon_window as HWND, WM_CLOSE, 0, 0) };
    }
}

/// A live tray icon. Dropping it removes the icon.
pub(super) struct Tray {
    events: std::sync::mpsc::Receiver<TrayEvent>,
    #[cfg_attr(not(windows), allow(dead_code))]
    icon_window: isize,
    thread: Option<std::thread::JoinHandle<()>>,
}

impl Tray {
    /// Adds the icon and hides `main`.
    #[cfg(windows)]
    pub(super) fn hide(main: isize, ctx: &egui::Context) -> Result<Self, String> {
        let (tx, events) = std::sync::mpsc::channel();
        let (ready_tx, ready) = std::sync::mpsc::channel();
        let ctx = ctx.clone();
        let thread = std::thread::spawn(move || native::run(main, tx, ready_tx, ctx));
        let icon_window = ready
            .recv()
            .map_err(|_| "Could not create the tray icon.".to_owned())??;
        Ok(Self {
            events,
            icon_window,
            thread: Some(thread),
        })
    }

    #[cfg(not(windows))]
    pub(super) fn hide(_main: isize, _ctx: &egui::Context) -> Result<Self, String> {
        Err("The tray is only available on Windows for now.".into())
    }

    pub(super) fn poll(&self) -> Option<TrayEvent> {
        self.events.try_recv().ok()
    }
}

impl Drop for Tray {
    fn drop(&mut self) {
        #[cfg(windows)]
        native::close(self.icon_window);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Editor {
    /// The window's close button: hide to the tray when enabled, otherwise exit.
    pub(super) fn close_window(&mut self, ctx: &egui::Context) {
        if self.preferences.close_to_tray
            && supported()
            && self.tray.is_none()
            && let Some(main) = self.main_window
        {
            match Tray::hide(main, ctx) {
                Ok(tray) => {
                    self.tray = Some(tray);
                    return;
                }
                Err(e) => self.notice = Some((true, tr(&e).to_owned())),
            }
        }
        self.request(super::Pending::Exit, ctx);
    }

    pub(super) fn tick_tray(&mut self, ctx: &egui::Context) {
        let Some(event) = self.tray.as_ref().and_then(Tray::poll) else {
            return;
        };
        self.tray = None;
        if event == TrayEvent::Quit {
            self.request(super::Pending::Exit, ctx);
        }
    }

    pub(super) fn tray_setting(&mut self, ui: &mut egui::Ui) {
        let available = supported();
        super::shell::setting_row(
            ui,
            "Keep running in the tray",
            if available {
                "Closing the window keeps COVALOS in the notification area so the phone bridge stays connected. Ending it from Task Manager takes it offline."
            } else {
                "Available on Windows."
            },
            |ui| {
                ui.add_enabled_ui(available, |ui| {
                    super::theme::checkbox(ui, &mut self.preferences.close_to_tray, "");
                });
            },
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_is_offered_where_it_works() {
        assert_eq!(supported(), cfg!(windows));
    }

    #[test]
    fn close_to_tray_is_off_by_default() {
        assert!(!super::super::Preferences::default().close_to_tray);
    }
}
