// Native desktop integration. No WebView or UI-framework types cross this seam.
use crate::{config::AppConfig, runtime::Runtime};
use std::sync::Arc;

#[cfg(windows)]
mod menu;

#[cfg(windows)]
mod native {
    use super::*;
    use std::{
        cell::{Cell, RefCell},
        collections::VecDeque,
        ffi::c_void,
        mem::{size_of, zeroed},
        ptr::{null, null_mut},
        sync::{atomic::Ordering, mpsc, OnceLock},
        thread::{self, JoinHandle},
    };

    const WM_TRAY: u32 = 0x8000 + 1;
    const WM_COMMANDS: u32 = 0x8000 + 2;
    const WM_WAKE: u32 = 0x8000 + 3;
    const WM_NULL: u32 = 0;
    const WM_HOTKEY: u32 = 0x0312;
    const WM_TIMER: u32 = 0x0113;
    const WM_CLOSE: u32 = 0x0010;
    const WM_DESTROY: u32 = 0x0002;
    const HEARTBEAT: usize = 1;
    const MOD_NOREPEAT: u32 = 0x4000;
    const SETTINGS: usize = 1;
    const TOGGLE: usize = 2;
    const QUIT: usize = 3;
    const START: usize = 4;
    const STOP: usize = 5;
    const CANCEL: usize = 6;
    const SHOW_OVERLAY: usize = 7;
    const HIDE_OVERLAY: usize = 8;
    const COPY: usize = 9;
    const TRAY_ICON_ID: u32 = 1;
    const GWLP_USERDATA: i32 = -21;
    const GWL_STYLE: i32 = -16;
    const GWL_EXSTYLE: i32 = -20;
    const WS_EX_TOOLWINDOW: u32 = 0x0000_0080;
    const WS_EX_APPWINDOW: u32 = 0x0004_0000;
    const WS_EX_NOACTIVATE: u32 = 0x0800_0000;
    const SWP_NOACTIVATE: u32 = 0x0010;
    const SWP_NOMOVE: u32 = 0x0002;
    const SWP_NOSIZE: u32 = 0x0001;
    const SWP_FRAMECHANGED: u32 = 0x0020;
    const SWP_NOOWNERZORDER: u32 = 0x0200;
    const WAIT_OBJECT_0: u32 = 0;

    #[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
    #[repr(C)]
    struct Point {
        x: i32,
        y: i32,
    }
    #[derive(Clone, Copy, Default)]
    #[repr(C)]
    struct Rect {
        left: i32,
        top: i32,
        right: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct MonitorInfo {
        size: u32,
        monitor: Rect,
        work: Rect,
        flags: u32,
    }
    #[repr(C)]
    struct Margins {
        left: i32,
        right: i32,
        top: i32,
        bottom: i32,
    }
    #[repr(C)]
    struct Message {
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
        time: u32,
        point: Point,
        private: u32,
    }
    #[repr(C)]
    struct WindowClass {
        style: u32,
        proc: Option<unsafe extern "system" fn(isize, u32, usize, isize) -> isize>,
        class_extra: i32,
        window_extra: i32,
        instance: isize,
        icon: isize,
        cursor: isize,
        background: isize,
        menu_name: *const u16,
        class_name: *const u16,
    }
    #[repr(C)]
    struct Guid {
        data1: u32,
        data2: u16,
        data3: u16,
        data4: [u8; 8],
    }
    #[repr(C)]
    struct NotifyIconData {
        size: u32,
        hwnd: isize,
        id: u32,
        flags: u32,
        callback: u32,
        icon: isize,
        tip: [u16; 128],
        state: u32,
        state_mask: u32,
        info: [u16; 256],
        version: u32,
        info_title: [u16; 64],
        info_flags: u32,
        guid: Guid,
        balloon_icon: isize,
    }
    #[repr(C)]
    struct TokenUser {
        sid: *mut c_void,
        attributes: u32,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetLastError() -> u32;
        fn SetLastError(error: u32);
        fn GetCurrentProcess() -> isize;
        fn GetCurrentThreadId() -> u32;
        fn GetModuleHandleW(name: *const u16) -> isize;
        fn CloseHandle(handle: isize) -> i32;
        fn LocalFree(memory: isize) -> isize;
        fn CreateMutexW(attributes: *const c_void, owner: i32, name: *const u16) -> isize;
        fn CreateEventW(
            attributes: *const c_void,
            manual: i32,
            initial: i32,
            name: *const u16,
        ) -> isize;
        fn SetEvent(event: isize) -> i32;
        fn WaitForSingleObject(handle: isize, millis: u32) -> u32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        fn OpenProcessToken(process: isize, access: u32, token: *mut isize) -> i32;
        fn GetTokenInformation(
            token: isize,
            class: i32,
            buffer: *mut c_void,
            length: u32,
            required: *mut u32,
        ) -> i32;
        fn ConvertSidToStringSidW(sid: *const c_void, result: *mut *mut u16) -> i32;
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        fn RegisterClassW(class: *const WindowClass) -> u16;
        fn UnregisterClassW(class: *const u16, instance: isize) -> i32;
        fn CreateWindowExW(
            ex_style: u32,
            class: *const u16,
            title: *const u16,
            style: u32,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            parent: isize,
            menu: isize,
            instance: isize,
            parameter: *mut c_void,
        ) -> isize;
        fn DestroyWindow(hwnd: isize) -> i32;
        fn DefWindowProcW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
        fn GetMessageW(message: *mut Message, hwnd: isize, min: u32, max: u32) -> i32;
        fn TranslateMessage(message: *const Message) -> i32;
        fn DispatchMessageW(message: *const Message) -> isize;
        fn PostMessageW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> i32;
        fn PostQuitMessage(code: i32);
        fn RegisterWindowMessageW(name: *const u16) -> u32;
        fn SetTimer(hwnd: isize, id: usize, millis: u32, callback: *const c_void) -> usize;
        fn KillTimer(hwnd: isize, id: usize) -> i32;
        fn RegisterHotKey(hwnd: isize, id: i32, modifiers: u32, key: u32) -> i32;
        fn UnregisterHotKey(hwnd: isize, id: i32) -> i32;
        fn EndMenu() -> i32;
        fn TrackPopupMenu(
            menu: isize,
            flags: u32,
            x: i32,
            y: i32,
            reserved: i32,
            hwnd: isize,
            rect: *const Rect,
        ) -> i32;
        fn GetCursorPos(point: *mut Point) -> i32;
        fn GetForegroundWindow() -> isize;
        fn SetForegroundWindow(hwnd: isize) -> i32;
        fn MonitorFromPoint(point: Point, flags: u32) -> isize;
        fn GetMonitorInfoW(monitor: isize, info: *mut MonitorInfo) -> i32;
        #[cfg_attr(target_pointer_width = "32", link_name = "GetWindowLongW")]
        fn GetWindowLongPtrW(hwnd: isize, index: i32) -> isize;
        #[cfg_attr(target_pointer_width = "32", link_name = "SetWindowLongW")]
        fn SetWindowLongPtrW(hwnd: isize, index: i32, value: isize) -> isize;
        fn SetWindowPos(
            hwnd: isize,
            after: isize,
            x: i32,
            y: i32,
            width: i32,
            height: i32,
            flags: u32,
        ) -> i32;
        fn ShowWindow(hwnd: isize, command: i32) -> i32;
        fn IsWindow(hwnd: isize) -> i32;
        fn IsWindowVisible(hwnd: isize) -> i32;
        fn IsIconic(hwnd: isize) -> i32;
        fn LoadIconW(instance: isize, name: *const u16) -> isize;
        fn CreateIcon(
            instance: isize,
            width: i32,
            height: i32,
            planes: u8,
            bits_per_pixel: u8,
            and_bits: *const u8,
            xor_bits: *const u8,
        ) -> isize;
        fn SendMessageW(hwnd: isize, message: u32, wparam: usize, lparam: isize) -> isize;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        fn Shell_NotifyIconW(message: u32, data: *const NotifyIconData) -> i32;
    }
    #[link(name = "dwmapi")]
    unsafe extern "system" {
        fn DwmExtendFrameIntoClientArea(hwnd: isize, margins: *const Margins) -> i32;
    }
    #[link(name = "shcore")]
    unsafe extern "system" {
        fn GetDpiForMonitor(monitor: isize, kind: i32, x: *mut u32, y: *mut u32) -> i32;
    }

    fn wide(value: &str) -> Vec<u16> {
        value.encode_utf16().chain(Some(0)).collect()
    }
    fn os_error(operation: &str) -> String {
        format!("{operation}：{}", std::io::Error::last_os_error())
    }

    struct Handle(isize);
    impl Drop for Handle {
        fn drop(&mut self) {
            if self.0 != 0 {
                unsafe {
                    CloseHandle(self.0);
                }
            }
        }
    }
    // A named mutex need not be owned by a thread. Its open handle is the lock,
    // so crashes release it and a duplicate never releases the original's lock.
    struct Instance {
        settings_event: Handle,
        _mutex: Handle,
    }
    impl Instance {
        fn acquire(explicit_hidden: bool) -> Result<Option<Self>, String> {
            let sid = current_user_sid()?;
            let event_name = wide(&format!("Global\\VoiceBar.GPUI.{sid}.Settings"));
            let mutex_name = wide(&format!("Global\\VoiceBar.GPUI.{sid}.Instance"));
            unsafe {
                // Make the auto-reset event first: a duplicate can signal during
                // startup, before the first process has created a message window.
                let event = Handle(CreateEventW(null(), 0, 0, event_name.as_ptr()));
                if event.0 == 0 {
                    return Err(os_error("无法创建单实例通知事件"));
                }
                let mutex = CreateMutexW(null(), 0, mutex_name.as_ptr());
                let already_exists = GetLastError() == 183;
                let mutex = Handle(mutex);
                if mutex.0 == 0 {
                    return Err(os_error("无法创建当前用户的单实例锁"));
                }
                if already_exists {
                    if !explicit_hidden && SetEvent(event.0) == 0 {
                        return Err(os_error("VoiceBar 已在运行，但无法通知它打开设置"));
                    }
                    return Ok(None);
                }
                Ok(Some(Self {
                    _mutex: mutex,
                    settings_event: event,
                }))
            }
        }
    }

    fn current_user_sid() -> Result<String, String> {
        unsafe {
            let mut token = 0;
            if OpenProcessToken(GetCurrentProcess(), 0x0008, &mut token) == 0 {
                return Err(os_error("无法读取当前 Windows 用户"));
            }
            let token = Handle(token);
            let mut length = 0;
            GetTokenInformation(token.0, 1, null_mut(), 0, &mut length);
            if length == 0 {
                return Err(os_error("无法查询当前用户标识长度"));
            }
            // TOKEN_USER contains a pointer, so a byte-vector would not express
            // the alignment required by the ABI.
            let mut buffer = vec![0usize; (length as usize).div_ceil(size_of::<usize>())];
            if GetTokenInformation(token.0, 1, buffer.as_mut_ptr().cast(), length, &mut length) == 0
            {
                return Err(os_error("无法读取当前用户标识"));
            }
            let user = &*(buffer.as_ptr() as *const TokenUser);
            let mut string = null_mut();
            if ConvertSidToStringSidW(user.sid, &mut string) == 0 {
                return Err(os_error("无法转换当前用户标识"));
            }
            let mut count = 0;
            while *string.add(count) != 0 {
                count += 1;
            }
            let sid = String::from_utf16_lossy(std::slice::from_raw_parts(string, count));
            LocalFree(string as isize);
            Ok(sid)
        }
    }

    enum Command {
        Save {
            config: Box<AppConfig>,
            reply: mpsc::SyncSender<Result<(), String>>,
        },
        Shutdown,
    }

    /// Own this for the whole GPUI application lifetime, including tray-only time.
    /// All shortcut registration and config transactions run on its native thread.
    pub struct Desktop {
        sender: mpsc::Sender<Command>,
        hwnd: isize,
        thread_id: u32,
        worker: Option<JoinHandle<()>>,
        _instance: Instance,
    }
    impl Desktop {
        pub fn new(runtime: Arc<Runtime>, explicit_hidden: bool) -> Result<Option<Self>, String> {
            let instance = match Instance::acquire(explicit_hidden)? {
                Some(instance) => instance,
                None => return Ok(None),
            };
            Self::launch(runtime, explicit_hidden, instance)
        }

        fn launch(
            runtime: Arc<Runtime>,
            explicit_hidden: bool,
            instance: Instance,
        ) -> Result<Option<Self>, String> {
            let config = runtime
                .config
                .lock()
                .map_err(|_| "配置锁已损坏".to_string())?
                .clone();
            let (sender, receiver) = mpsc::channel();
            let (ready, acknowledgement) = mpsc::sync_channel(1);
            let settings_event = instance.settings_event.0;
            runtime
                .settings_requested
                .store(!config.starts_hidden(explicit_hidden), Ordering::Release);
            let worker = thread::Builder::new()
                .name("voicebar-native".into())
                .spawn(move || {
                    let result = NativeThread::new(runtime, receiver, settings_event, &config);
                    match result {
                        Ok(mut desktop) => {
                            let identity = (desktop.window.hwnd, unsafe { GetCurrentThreadId() });
                            if ready.send(Ok(identity)).is_ok() {
                                desktop.run();
                            }
                        }
                        Err(error) => {
                            let _ = ready.send(Err(error));
                        }
                    }
                })
                .map_err(|error| format!("无法创建桌面消息线程：{error}"))?;
            match acknowledgement.recv() {
                Ok(Ok((hwnd, thread_id))) => Ok(Some(Self {
                    sender,
                    hwnd,
                    thread_id,
                    worker: Some(worker),
                    _instance: instance,
                })),
                Ok(Err(error)) => {
                    let _ = worker.join();
                    Err(error)
                }
                Err(_) => {
                    let _ = worker.join();
                    Err("桌面消息线程初始化意外中断".into())
                }
            }
        }

        #[cfg(test)]
        pub(crate) fn isolated_preview(runtime: Arc<Runtime>) -> Result<Self, String> {
            let settings_event = Handle(unsafe { CreateEventW(null(), 0, 0, null()) });
            if settings_event.0 == 0 {
                return Err(os_error("无法创建独立预览事件"));
            }
            Self::launch(
                runtime,
                false,
                Instance {
                    settings_event,
                    _mutex: Handle(0),
                },
            )?
            .ok_or_else(|| "无法创建独立预览桌面".into())
        }

        #[cfg(test)]
        pub(crate) fn preview_context_menu(&self, x: i16, y: i16) {
            let position = (x as u16 as u32) | ((y as u16 as u32) << 16);
            unsafe {
                PostMessageW(
                    self.hwnd,
                    WM_TRAY,
                    position as usize,
                    ((TRAY_ICON_ID << 16) | 0x007b) as isize,
                );
            }
        }

        #[cfg(test)]
        pub(crate) fn preview_menu_open(&self) -> bool {
            use windows::Win32::UI::WindowsAndMessaging::{GetGUIThreadInfo, GUITHREADINFO};
            let mut info = GUITHREADINFO {
                cbSize: size_of::<GUITHREADINFO>() as u32,
                ..Default::default()
            };
            unsafe { GetGUIThreadInfo(self.thread_id, &mut info).is_ok() && info.flags.0 & 4 != 0 }
        }

        #[cfg(test)]
        pub(crate) fn dismiss_preview_menu(&self) {
            unsafe {
                PostMessageW(self.hwnd, WM_HOTKEY, 0, 0);
            }
        }

        /// Safe to call from the GPUI thread. This waits for a definite result;
        /// a timeout must not report failure while a config write is still queued.
        pub fn save_config(&self, config: AppConfig) -> Result<(), String> {
            if unsafe { GetCurrentThreadId() } == self.thread_id {
                return Err("请从界面线程调用配置保存，不能阻塞桌面消息线程".into());
            }
            let (reply, receive) = mpsc::sync_channel(1);
            self.sender
                .send(Command::Save {
                    config: Box::new(config),
                    reply,
                })
                .map_err(|_| "桌面消息线程已关闭，配置未保存".to_string())?;
            // The heartbeat also drains commands, so a failed wake cannot leave
            // an already-enqueued mutation running after an early error return.
            unsafe {
                PostMessageW(self.hwnd, WM_COMMANDS, 0, 0);
            }
            receive
                .recv()
                .map_err(|_| "桌面消息线程中断，无法确认配置保存结果".to_string())?
        }
    }
    impl Drop for Desktop {
        fn drop(&mut self) {
            let _ = self.sender.send(Command::Shutdown);
            unsafe {
                PostMessageW(self.hwnd, WM_COMMANDS, 0, 0);
            }
            if let Some(worker) = self.worker.take() {
                let _ = worker.join();
            }
            // _instance is deliberately released after the worker has removed
            // its tray icon, unregistered shortcuts and destroyed its window.
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    enum NativeEvent {
        Action(usize),
        Hotkey(usize, isize),
        Popup(Option<Point>),
        TaskbarCreated,
    }

    fn tray_event(wparam: usize, lparam: isize) -> Option<NativeEvent> {
        let code = (lparam as u32) & 0xffff;
        let icon = ((lparam as u32) >> 16) & 0xffff;
        let v4 = icon == TRAY_ICON_ID;
        if !v4 && !(icon == 0 && wparam == TRAY_ICON_ID as usize) {
            return None;
        }
        match code {
            0x0202 | 0x0203 | 0x0400 | 0x0401 => Some(NativeEvent::Action(SETTINGS)),
            0x0205 | 0x007b => {
                let point = v4
                    .then(|| Point {
                        x: (wparam as u16) as i16 as i32,
                        y: ((wparam >> 16) as u16) as i16 as i32,
                    })
                    .filter(|p| *p != Point { x: -1, y: -1 });
                Some(NativeEvent::Popup(point))
            }
            _ => None,
        }
    }

    // Windows delivers shell notifications both as sent and posted messages.
    // Keep their storage independent of NativeThread: TrackPopupMenu dispatches
    // reentrant window callbacks while the outer loop is borrowed.
    struct MessageEvents {
        pending: RefCell<VecDeque<NativeEvent>>,
        receiver: RefCell<mpsc::Receiver<Command>>,
        commands: RefCell<VecDeque<Command>>,
        menu_open: Cell<bool>,
        active_menu: Cell<*const menu::Popup>,
        taskbar_created: u32,
        settings_event: isize,
    }
    impl MessageEvents {
        fn collect_commands(&self) -> bool {
            while let Ok(command) = self.receiver.borrow().try_recv() {
                self.commands.borrow_mut().push_back(command);
            }
            !self.commands.borrow().is_empty()
        }
        fn enqueue(&self, hwnd: isize, event: NativeEvent) {
            // A legacy right-up and a v4 context callback must not open two menus.
            if matches!(event, NativeEvent::Popup(_))
                && (self.menu_open.get()
                    || self
                        .pending
                        .borrow()
                        .iter()
                        .any(|e| matches!(e, NativeEvent::Popup(_))))
            {
                return;
            }
            self.pending.borrow_mut().push_back(event);
            unsafe {
                if self.menu_open.get() {
                    EndMenu();
                }
                PostMessageW(hwnd, WM_WAKE, 0, 0);
            }
        }
    }

    struct MessageWindow {
        hwnd: isize,
        instance: isize,
        class_name: Vec<u16>,
        owns_class: bool,
        events: Box<MessageEvents>,
    }
    impl MessageWindow {
        fn new(
            receiver: mpsc::Receiver<Command>,
            settings_event: isize,
            taskbar_created: u32,
        ) -> Result<Self, String> {
            unsafe {
                let instance = GetModuleHandleW(null());
                if instance == 0 {
                    return Err(os_error("无法取得程序模块句柄"));
                }
                let class_name = wide("VoiceBar.GPUI.NativeMessageWindow.v1");
                let mut class: WindowClass = zeroed();
                class.proc = Some(window_proc);
                class.instance = instance;
                class.class_name = class_name.as_ptr();
                let atom = RegisterClassW(&class);
                if atom == 0 && GetLastError() != 1410 {
                    return Err(os_error("无法注册桌面消息窗口"));
                }
                let mut window = Self {
                    hwnd: 0,
                    instance,
                    class_name,
                    owns_class: atom != 0,
                    events: Box::new(MessageEvents {
                        pending: RefCell::new(VecDeque::new()),
                        receiver: RefCell::new(receiver),
                        commands: RefCell::new(VecDeque::new()),
                        menu_open: Cell::new(false),
                        active_menu: Cell::new(null()),
                        taskbar_created,
                        settings_event,
                    }),
                };
                window.hwnd = CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    window.class_name.as_ptr(),
                    wide("VoiceBar native events").as_ptr(),
                    0x8000_0000,
                    0,
                    0,
                    0,
                    0,
                    0,
                    0,
                    instance,
                    null_mut(),
                );
                if window.hwnd == 0 {
                    return Err(os_error("无法创建桌面消息窗口"));
                }
                SetLastError(0);
                if SetWindowLongPtrW(
                    window.hwnd,
                    GWLP_USERDATA,
                    (&*window.events as *const MessageEvents) as isize,
                ) == 0
                    && GetLastError() != 0
                {
                    return Err(os_error("无法绑定桌面消息队列"));
                }
                if SetTimer(window.hwnd, HEARTBEAT, 100, null()) == 0 {
                    return Err(os_error("无法启动桌面消息心跳"));
                }
                Ok(window)
            }
        }
    }
    impl Drop for MessageWindow {
        fn drop(&mut self) {
            unsafe {
                if self.hwnd != 0 {
                    KillTimer(self.hwnd, HEARTBEAT);
                    SetWindowLongPtrW(self.hwnd, GWLP_USERDATA, 0);
                    DestroyWindow(self.hwnd);
                }
                if self.owns_class {
                    UnregisterClassW(self.class_name.as_ptr(), self.instance);
                }
            }
        }
    }
    unsafe extern "system" fn window_proc(
        hwnd: isize,
        message: u32,
        wparam: usize,
        lparam: isize,
    ) -> isize {
        if message == WM_DESTROY {
            PostQuitMessage(0);
            return 0;
        }
        let events = GetWindowLongPtrW(hwnd, GWLP_USERDATA) as *const MessageEvents;
        if let Some(events) = events.as_ref() {
            if let Some(menu) = events.active_menu.get().as_ref() {
                if let Some(result) = menu.message(message, wparam, lparam) {
                    return result;
                }
            }
            match message {
                WM_TRAY => {
                    if let Some(event) = tray_event(wparam, lparam) {
                        events.enqueue(hwnd, event);
                    }
                    return 0;
                }
                WM_HOTKEY => {
                    events.enqueue(hwnd, NativeEvent::Hotkey(wparam, lparam));
                    return 0;
                }
                WM_CLOSE => {
                    events.enqueue(hwnd, NativeEvent::Action(QUIT));
                    return 0;
                }
                WM_COMMANDS | WM_TIMER => {
                    // An idle heartbeat must not dismiss the menu. Only a real
                    // save/shutdown wakes the outer loop, even if PostMessage failed.
                    if events.collect_commands() && events.menu_open.get() {
                        EndMenu();
                    }
                    if events.settings_event != 0
                        && WaitForSingleObject(events.settings_event, 0) == WAIT_OBJECT_0
                    {
                        events.enqueue(hwnd, NativeEvent::Action(SETTINGS));
                    }
                    return 0;
                }
                WM_WAKE => return 0,
                other if other == events.taskbar_created && other != 0 => {
                    events.enqueue(hwnd, NativeEvent::TaskbarCreated);
                    return 0;
                }
                _ => {}
            }
        }
        DefWindowProcW(hwnd, message, wparam, lparam)
    }

    struct Tray {
        data: NotifyIconData,
        added: bool,
    }
    impl Tray {
        fn new(hwnd: isize) -> Result<Self, String> {
            let mut data: NotifyIconData = unsafe { zeroed() };
            data.size = size_of::<NotifyIconData>() as u32;
            data.hwnd = hwnd;
            data.id = TRAY_ICON_ID;
            data.flags = 0x01 | 0x02 | 0x04; // NIF_MESSAGE | NIF_ICON | NIF_TIP
            data.callback = WM_TRAY;
            data.icon = app_icon()?;
            let tip = wide("VoiceBar · 语音输入 · 单击打开设置");
            let length = tip.len().min(data.tip.len());
            data.tip[..length].copy_from_slice(&tip[..length]);
            let mut tray = Self { data, added: false };
            tray.add()?;
            Ok(tray)
        }
        fn add(&mut self) -> Result<(), String> {
            self.added = false;
            if unsafe { Shell_NotifyIconW(0, &self.data) } == 0 {
                return Err(os_error("无法创建系统托盘图标"));
            }
            self.added = true;
            self.data.version = 4;
            // Old shell versions still deliver the legacy mouse notifications.
            unsafe {
                Shell_NotifyIconW(4, &self.data);
            }
            Ok(())
        }
    }
    impl Drop for Tray {
        fn drop(&mut self) {
            if self.added {
                unsafe {
                    Shell_NotifyIconW(2, &self.data);
                }
            }
        }
    }

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Shortcut {
        modifiers: u32,
        key: u32,
    }
    #[derive(Clone, Copy)]
    struct Bindings {
        toggle: Shortcut,
        cancel: Option<Shortcut>,
    }
    impl Bindings {
        fn parse(config: &AppConfig) -> Result<Self, String> {
            let toggle = parse_shortcut(&config.hotkey)
                .map_err(|error| format!("开始/结束快捷键格式无效：{error}"))?;
            let cancel = if config.cancel_hotkey.trim().is_empty() {
                None
            } else {
                Some(
                    parse_shortcut(&config.cancel_hotkey)
                        .map_err(|error| format!("取消快捷键格式无效：{error}"))?,
                )
            };
            if cancel == Some(toggle) {
                return Err("取消快捷键不能与开始/结束快捷键相同".into());
            }
            Ok(Self { toggle, cancel })
        }
        fn contains(self, shortcut: Shortcut) -> bool {
            self.toggle == shortcut || self.cancel == Some(shortcut)
        }
    }
    struct Registration {
        id: i32,
        shortcut: Shortcut,
    }
    struct Hotkeys {
        hwnd: isize,
        bindings: Bindings,
        registered: Vec<Registration>,
        next_id: i32,
    }
    impl Hotkeys {
        fn new(hwnd: isize, config: &AppConfig) -> Result<Self, String> {
            let bindings = Bindings::parse(config)?;
            let mut keys = Self {
                hwnd,
                bindings,
                registered: Vec::new(),
                next_id: 1,
            };
            keys.add(bindings.toggle, "开始/结束")?;
            if let Some(cancel) = bindings.cancel {
                keys.add(cancel, "取消")?;
            }
            Ok(keys)
        }
        fn add(&mut self, shortcut: Shortcut, label: &str) -> Result<(), String> {
            if self.registered.iter().any(|item| item.shortcut == shortcut) {
                return Ok(());
            }
            // Application hotkey IDs are in 0x0000..=0xbfff. Avoid IDs that
            // are still registered even after many settings edits.
            let id = (0..0xbfff)
                .find_map(|_| {
                    let id = self.next_id;
                    self.next_id = if id == 0xbfff { 1 } else { id + 1 };
                    (!self.registered.iter().any(|item| item.id == id)).then_some(id)
                })
                .ok_or("无法分配全局快捷键编号")?;
            if unsafe {
                RegisterHotKey(
                    self.hwnd,
                    id,
                    shortcut.modifiers | MOD_NOREPEAT,
                    shortcut.key,
                )
            } == 0
            {
                return Err(os_error(&format!("{label}快捷键已被占用或不可用")));
            }
            self.registered.push(Registration { id, shortcut });
            Ok(())
        }
        fn retain(&mut self, keep: impl Fn(Shortcut) -> bool) -> Vec<String> {
            let hwnd = self.hwnd;
            let mut errors = Vec::new();
            self.registered.retain(|item| {
                if keep(item.shortcut) {
                    return true;
                }
                if unsafe { UnregisterHotKey(hwnd, item.id) } != 0 {
                    false
                } else {
                    // Keep ownership so shutdown/next save retries cleanup.
                    errors.push(os_error("无法注销旧全局快捷键"));
                    true
                }
            });
            errors
        }
        fn save(&mut self, runtime: &Runtime, config: AppConfig) -> Result<(), String> {
            let new = Bindings::parse(&config)?;
            let old = self.bindings;
            // Keep both old chords registered until persistence commits. A
            // swap only changes their roles; it never attempts a double register.
            let preparation = self.add(new.toggle, "开始/结束").and_then(|_| {
                if let Some(cancel) = new.cancel {
                    self.add(cancel, "取消")
                } else {
                    Ok(())
                }
            });
            if let Err(error) = preparation {
                let cleanup = self.retain(|key| old.contains(key));
                return Err(with_cleanup(error, cleanup));
            }
            if let Err(error) = runtime.save_config(config) {
                let cleanup = self.retain(|key| old.contains(key));
                return Err(with_cleanup(error, cleanup));
            }
            self.bindings = new;
            let cleanup = self.retain(|key| new.contains(key));
            if !cleanup.is_empty() {
                // Persistence already committed. Do not falsely tell the caller
                // it failed; obsolete registrations are inert and remain owned.
                runtime.set_status("error", format!("配置已保存，但{}", cleanup.join("；")));
            }
            Ok(())
        }
        fn action(&self, id: usize, payload: isize) -> Option<usize> {
            let registered = self.registered.iter().find(|item| item.id as usize == id)?;
            let shortcut = Shortcut {
                modifiers: (payload as u32) & 0x000f,
                key: ((payload as u32) >> 16) & 0xffff,
            };
            // Also validate the chord to ignore stale queued IDs after an edit.
            if registered.shortcut != shortcut {
                return None;
            }
            if shortcut == self.bindings.toggle {
                Some(TOGGLE)
            } else if Some(shortcut) == self.bindings.cancel {
                Some(0)
            } else {
                None
            }
        }
    }
    impl Drop for Hotkeys {
        fn drop(&mut self) {
            for registration in &self.registered {
                unsafe {
                    UnregisterHotKey(self.hwnd, registration.id);
                }
            }
        }
    }
    fn with_cleanup(error: String, cleanup: Vec<String>) -> String {
        if cleanup.is_empty() {
            error
        } else {
            format!("{error}；清理失败：{}", cleanup.join("；"))
        }
    }

    fn parse_shortcut(text: &str) -> Result<Shortcut, String> {
        let mut modifiers = 0;
        let mut key = None;
        for part in text.split('+') {
            let part = part.trim().to_ascii_lowercase();
            let modifier = match part.as_str() {
                "ctrl" | "control" | "commandorcontrol" | "cmdorctrl" | "controlorcmd" => {
                    Some(0x0002)
                }
                "alt" | "option" => Some(0x0001),
                "shift" => Some(0x0004),
                "win" | "windows" | "super" | "meta" | "cmd" | "command" => Some(0x0008),
                _ => None,
            };
            if let Some(modifier) = modifier {
                modifiers |= modifier;
                continue;
            }
            if key.is_some() {
                return Err("每个快捷键只能包含一个普通按键".into());
            }
            key = Some(key_code(&part).ok_or_else(|| {
                format!("不支持按键「{part}」；可使用字母、数字、F1–F24、Space、Escape 或方向键")
            })?);
        }
        Ok(Shortcut {
            modifiers,
            key: key.ok_or("快捷键必须包含普通按键")?,
        })
    }
    fn key_code(key: &str) -> Option<u32> {
        let bytes = key.as_bytes();
        if bytes.len() == 1 && bytes[0].is_ascii_alphanumeric() {
            return Some(bytes[0].to_ascii_uppercase() as u32);
        }
        if let Some(number) = key
            .strip_prefix('f')
            .and_then(|value| value.parse::<u32>().ok())
        {
            if (1..=24).contains(&number) {
                return Some(0x70 + number - 1);
            }
        }
        if let Some(letter) = key.strip_prefix("key") {
            if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() {
                return Some(letter.as_bytes()[0].to_ascii_uppercase() as u32);
            }
        }
        if let Some(digit) = key.strip_prefix("digit") {
            if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
                return Some(digit.as_bytes()[0] as u32);
            }
        }
        if let Some(digit) = key.strip_prefix("numpad") {
            if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
                return Some(0x60 + (digit.as_bytes()[0] - b'0') as u32);
            }
        }
        Some(match key {
            "space" | "spacebar" => 0x20,
            "escape" | "esc" => 0x1b,
            "enter" | "return" | "numpadenter" => 0x0d,
            "tab" => 0x09,
            "backspace" => 0x08,
            "delete" | "del" => 0x2e,
            "insert" | "ins" => 0x2d,
            "home" => 0x24,
            "end" => 0x23,
            "pageup" | "pgup" => 0x21,
            "pagedown" | "pgdn" => 0x22,
            "left" | "arrowleft" => 0x25,
            "up" | "arrowup" => 0x26,
            "right" | "arrowright" => 0x27,
            "down" | "arrowdown" => 0x28,
            "capslock" => 0x14,
            "numlock" => 0x90,
            "scrolllock" => 0x91,
            "pause" | "break" => 0x13,
            "printscreen" => 0x2c,
            "numpadmultiply" | "multiply" => 0x6a,
            "numpadadd" | "add" => 0x6b,
            "numpadsubtract" | "subtract" => 0x6d,
            "numpaddecimal" | "decimal" => 0x6e,
            "numpaddivide" | "divide" => 0x6f,
            "semicolon" | ";" => 0xba,
            "equal" | "equals" | "plus" | "=" => 0xbb,
            "comma" | "," => 0xbc,
            "minus" | "-" => 0xbd,
            "period" | "." => 0xbe,
            "slash" | "/" => 0xbf,
            "backquote" | "grave" | "`" => 0xc0,
            "bracketleft" | "[" => 0xdb,
            "backslash" | "\\" => 0xdc,
            "bracketright" | "]" => 0xdd,
            "quote" | "apostrophe" | "'" => 0xde,
            "mediaplaypause" => 0xb3,
            "mediastop" => 0xb2,
            "medianexttrack" | "mediatracknext" => 0xb0,
            "mediaprevioustrack" | "mediatrackprevious" => 0xb1,
            "audiovolumemute" | "volumemute" => 0xad,
            "audiovolumedown" | "volumedown" => 0xae,
            "audiovolumeup" | "volumeup" => 0xaf,
            _ => return None,
        })
    }

    struct NativeThread {
        runtime: Arc<Runtime>,
        // Drop resources before their owner window.
        hotkeys: Hotkeys,
        tray: Tray,
        window: MessageWindow,
    }
    impl NativeThread {
        fn new(
            runtime: Arc<Runtime>,
            commands: mpsc::Receiver<Command>,
            settings_event: isize,
            config: &AppConfig,
        ) -> Result<Self, String> {
            let taskbar_created =
                unsafe { RegisterWindowMessageW(wide("TaskbarCreated").as_ptr()) };
            if taskbar_created == 0 {
                return Err(os_error("无法注册系统托盘恢复通知"));
            }
            let window = MessageWindow::new(commands, settings_event, taskbar_created)?;
            let hotkeys = Hotkeys::new(window.hwnd, config)?;
            let tray = Tray::new(window.hwnd)?;
            Ok(Self {
                runtime,
                hotkeys,
                tray,
                window,
            })
        }
        fn run(&mut self) {
            loop {
                if !self.process_commands() || self.runtime.quit_requested.load(Ordering::Acquire) {
                    break;
                }
                loop {
                    // Release the queue borrow before any Win32 call can dispatch.
                    let event = self.window.events.pending.borrow_mut().pop_front();
                    let Some(event) = event else {
                        break;
                    };
                    match event {
                        NativeEvent::Action(action) => self.action(action),
                        NativeEvent::Hotkey(id, payload) => {
                            if let Some(action) = self.hotkeys.action(id, payload) {
                                self.action(action);
                            }
                        }
                        NativeEvent::Popup(point) => {
                            if let Err(error) = self.popup(point) {
                                self.runtime.set_status("error", error);
                            }
                        }
                        NativeEvent::TaskbarCreated => {
                            if let Err(error) = self.tray.add() {
                                self.runtime.set_status("error", error);
                            }
                        }
                    }
                    if self.runtime.quit_requested.load(Ordering::Acquire) {
                        break;
                    }
                }
                if self.runtime.quit_requested.load(Ordering::Acquire) {
                    break;
                }
                let mut message: Message = unsafe { zeroed() };
                let result = unsafe { GetMessageW(&mut message, 0, 0, 0) };
                if result <= 0 {
                    if result < 0 {
                        self.runtime
                            .set_status("error", os_error("桌面消息循环意外中断"));
                    }
                    break;
                }
                unsafe {
                    TranslateMessage(&message);
                    DispatchMessageW(&message);
                }
            }
            self.runtime.stop(true);
            self.runtime.overlay_visible.store(false, Ordering::Release);
            self.runtime.quit_requested.store(true, Ordering::Release);
        }
        fn process_commands(&mut self) -> bool {
            self.window.events.collect_commands();
            loop {
                let command = self.window.events.commands.borrow_mut().pop_front();
                let Some(command) = command else {
                    return true;
                };
                match command {
                    Command::Shutdown => return false,
                    Command::Save { config, reply } => {
                        let result = self.hotkeys.save(&self.runtime, *config);
                        let _ = reply.send(result);
                    }
                }
            }
        }
        fn action(&self, action: usize) {
            match action {
                SETTINGS => self
                    .runtime
                    .settings_requested
                    .store(true, Ordering::Release),
                TOGGLE => {
                    let active = self.runtime.active.lock().unwrap().is_some();
                    self.action(if active { STOP } else { START });
                }
                START => {
                    if self.runtime.active.lock().unwrap().is_none() {
                        if let Err(error) = self.runtime.start() {
                            self.runtime.set_status("error", error);
                        }
                        self.runtime.overlay_visible.store(true, Ordering::Release);
                    }
                }
                STOP => self.runtime.stop(false),
                CANCEL | 0 => self.runtime.stop(true),
                SHOW_OVERLAY => self.runtime.overlay_visible.store(true, Ordering::Release),
                HIDE_OVERLAY => self.runtime.dismiss_overlay(),
                COPY => {
                    if let Err(error) = self.runtime.copy_transcript() {
                        self.runtime.set_status("error", error);
                    }
                }
                QUIT => {
                    self.runtime.stop(true);
                    self.runtime.quit_requested.store(true, Ordering::Release);
                }
                _ => {}
            }
        }
        fn popup(&self, position: Option<Point>) -> Result<(), String> {
            struct OpenMenu<'a>(&'a MessageEvents);
            impl Drop for OpenMenu<'_> {
                fn drop(&mut self) {
                    self.0.active_menu.set(null());
                    self.0.menu_open.set(false);
                }
            }
            unsafe {
                let previous = GetForegroundWindow();
                let mut cursor = position.unwrap_or_default();
                if position.is_none() && GetCursorPos(&mut cursor) == 0 {
                    return Err(os_error("无法取得托盘菜单位置"));
                }
                let active = self.runtime.active.lock().unwrap().is_some();
                let visible = self.runtime.overlay_visible.load(Ordering::Acquire)
                    || self.runtime.overlay_preview().is_some();
                let has_text = {
                    let text = self.runtime.transcript.lock().unwrap();
                    !text.stable.trim().is_empty() || !text.partial.trim().is_empty()
                };
                let config = self.runtime.config.lock().unwrap().clone();
                let (mut dpi_x, mut dpi_y) = (96, 96);
                if GetDpiForMonitor(MonitorFromPoint(cursor, 2), 0, &mut dpi_x, &mut dpi_y) < 0 {
                    dpi_x = 96;
                }
                let menu = menu::Popup::new(
                    self.window.hwnd,
                    dpi_x,
                    tray_menu(active, visible, has_text, &config.hotkey),
                )?;
                SetForegroundWindow(self.window.hwnd);
                self.window.events.active_menu.set(&menu);
                self.window.events.menu_open.set(true);
                let open = OpenMenu(&self.window.events);
                let command = TrackPopupMenu(
                    menu.handle(),
                    0x0100 | 0x0080 | 0x0002,
                    cursor.x,
                    cursor.y,
                    0,
                    self.window.hwnd,
                    null(),
                );
                drop(open);
                PostMessageW(self.window.hwnd, WM_NULL, 0, 0);
                if GetForegroundWindow() == self.window.hwnd
                    && previous != 0
                    && IsWindow(previous) != 0
                {
                    SetForegroundWindow(previous);
                }
                if command as usize == START && (previous == 0 || GetForegroundWindow() != previous)
                {
                    return Err("未能恢复输入窗口，请在目标输入框中按开始快捷键".into());
                }
                if command != 0 {
                    self.action(command as usize);
                }
                Ok(())
            }
        }
    }

    use menu::Item as TrayMenuItem;
    fn tray_menu(active: bool, visible: bool, has_text: bool, hotkey: &str) -> Vec<TrayMenuItem> {
        let item = |action, label: String, enabled| TrayMenuItem {
            action,
            label,
            enabled,
            separator: false,
        };
        let separator = || TrayMenuItem {
            action: 0,
            label: String::new(),
            enabled: false,
            separator: true,
        };
        vec![
            item(SETTINGS, "打开设置".into(), true),
            separator(),
            item(
                if active { STOP } else { START },
                format!(
                    "{}\t{hotkey}",
                    if active {
                        "结束录音"
                    } else {
                        "开始录音"
                    }
                ),
                true,
            ),
            item(CANCEL, "取消本次录音".into(), active),
            item(COPY, "复制识别文字".into(), has_text),
            separator(),
            item(
                if visible { HIDE_OVERLAY } else { SHOW_OVERLAY },
                if visible {
                    "收起语音条".into()
                } else {
                    "显示语音条".into()
                },
                true,
            ),
            separator(),
            item(QUIT, "退出 VoiceBar".into(), true),
        ]
    }

    #[cfg(test)]
    mod tray_tests {
        use super::*;

        #[test]
        fn legacy_and_v4_callbacks_keep_icon_identity_and_signed_coordinates() {
            assert_eq!(tray_event(1, 0x0205), Some(NativeEvent::Popup(None)));
            let packed = ((-120i16 as u16 as u32) | ((-45i16 as u16 as u32) << 16)) as usize;
            assert_eq!(
                tray_event(packed, (1 << 16) | 0x007b),
                Some(NativeEvent::Popup(Some(Point { x: -120, y: -45 })))
            );
            assert_eq!(
                tray_event(0xffff_ffff, (1 << 16) | 0x007b),
                Some(NativeEvent::Popup(None))
            );
            assert_eq!(tray_event(1, (2 << 16) | 0x007b), None);
            assert_eq!(tray_event(1, 0x0202), Some(NativeEvent::Action(SETTINGS)));
            assert_eq!(
                tray_event(0, (1 << 16) | 0x0401),
                Some(NativeEvent::Action(SETTINGS))
            );
        }

        #[test]
        fn menu_uses_explicit_session_actions_and_disables_unavailable_results() {
            let idle = tray_menu(false, false, false, "F8");
            assert!(idle
                .iter()
                .any(|i| i.action == START && i.label.ends_with("F8")));
            assert!(!idle.iter().find(|i| i.action == CANCEL).unwrap().enabled);
            assert!(!idle.iter().find(|i| i.action == COPY).unwrap().enabled);
            let active = tray_menu(true, true, true, "F8");
            assert!(active.iter().any(|i| i.action == STOP));
            assert!(!active
                .iter()
                .any(|i| i.action == START || i.action == TOGGLE));
            assert!(active.iter().any(|i| i.action == HIDE_OVERLAY));
            assert!(active.iter().find(|i| i.action == COPY).unwrap().enabled);
        }

        #[test]
        fn sent_shell_messages_are_queued_and_idle_heartbeat_keeps_menu_open() {
            let (sender, receiver) = mpsc::channel();
            let window = MessageWindow::new(receiver, 0, 0xc123).unwrap();
            unsafe {
                SendMessageW(window.hwnd, WM_TRAY, 1, 0x0205);
                SendMessageW(window.hwnd, WM_TRAY, 1, 0x0205);
            }
            assert_eq!(
                window.events.pending.borrow_mut().pop_front(),
                Some(NativeEvent::Popup(None))
            );
            assert!(window.events.pending.borrow().is_empty());
            window.events.menu_open.set(true);
            unsafe {
                SendMessageW(window.hwnd, WM_TIMER, HEARTBEAT, 0);
            }
            assert!(window.events.menu_open.get());
            sender.send(Command::Shutdown).unwrap();
            unsafe {
                SendMessageW(window.hwnd, WM_TIMER, HEARTBEAT, 0);
            }
            assert!(matches!(
                window.events.commands.borrow_mut().pop_front(),
                Some(Command::Shutdown)
            ));
            unsafe {
                SendMessageW(window.hwnd, 0xc123, 0, 0);
            }
            assert_eq!(
                window.events.pending.borrow_mut().pop_front(),
                Some(NativeEvent::TaskbarCreated)
            );
            window.events.menu_open.set(false);
        }
    }

    // The process-lifetime icon must outlive every HWND that receives WM_SETICON.
    // A single 32px icon is reused by the tray and both GPUI windows.
    fn app_icon() -> Result<isize, String> {
        static ICON: OnceLock<isize> = OnceLock::new();
        if let Some(icon) = ICON.get() {
            return Ok(*icon);
        }
        unsafe {
            let instance = GetModuleHandleW(null());
            let mut icon = LoadIconW(instance, 1usize as *const u16);
            if icon == 0 {
                let mut rgba = [0u8; 32 * 32 * 4];
                let mut mask = [0xffu8; 32 * 4];
                for y in 0..32usize {
                    for x in 0..32usize {
                        let dx = x as f64 - 15.5;
                        let dy = y as f64 - 15.5;
                        let mut color = if dx * dx + dy * dy < 240.0 {
                            [30, 32, 24, 255]
                        } else {
                            [0, 0, 0, 0]
                        };
                        for (bar, height) in [5i32, 9, 14, 10, 6].into_iter().enumerate() {
                            if x >= 7 + bar * 4 && x < 9 + bar * 4 && (y as i32 - 16).abs() < height
                            {
                                color = [207, 232, 167, 255];
                            }
                        }
                        rgba[(y * 32 + x) * 4..(y * 32 + x + 1) * 4].copy_from_slice(&color);
                        if color[3] != 0 {
                            mask[y * 4 + x / 8] &= !(0x80 >> (x % 8));
                        }
                    }
                }
                icon = CreateIcon(instance, 32, 32, 1, 32, mask.as_ptr(), rgba.as_ptr());
            }
            if icon == 0 {
                icon = LoadIconW(0, 32512usize as *const u16);
            }
            if icon == 0 {
                return Err(os_error("无法载入 VoiceBar 图标"));
            }
            // get_or_init serializes publication; windows normally request this
            // only after the native thread has initialized the tray.
            Ok(*ICON.get_or_init(|| icon))
        }
    }

    /// Call on the GPUI UI thread with a transparent, undecorated 560x210
    /// logical-pixel window. Keep GPUI's compositor styles intact: adding
    /// WS_EX_LAYERED/SetLayeredWindowAttributes breaks composition swap chains.
    pub fn configure_overlay(hwnd: isize, config: &AppConfig) -> Result<(), String> {
        unsafe {
            if hwnd == 0 || IsWindow(hwnd) == 0 {
                return Err("语音条窗口句柄无效".into());
            }
            if !config.appearance.scale.is_finite()
                || !(0.65..=1.8).contains(&config.appearance.scale)
            {
                return Err("语音条缩放应为 0.65–1.8".into());
            }
            let mut cursor = Point::default();
            if GetCursorPos(&mut cursor) == 0 {
                return Err(os_error("无法取得鼠标位置"));
            }
            let monitor = MonitorFromPoint(cursor, 2); // MONITOR_DEFAULTTONEAREST
            if monitor == 0 {
                return Err("未找到鼠标所在显示器".into());
            }
            let mut info: MonitorInfo = zeroed();
            info.size = size_of::<MonitorInfo>() as u32;
            if GetMonitorInfoW(monitor, &mut info) == 0 {
                return Err(os_error("无法读取显示器工作区"));
            }
            let mut dpi_x = 0;
            let mut dpi_y = 0;
            let result = GetDpiForMonitor(monitor, 0, &mut dpi_x, &mut dpi_y);
            if result < 0 || dpi_x == 0 || dpi_y == 0 {
                return Err(format!(
                    "无法读取显示器缩放比例（Windows 错误 0x{:08X}）",
                    result as u32
                ));
            }
            let (x, y, width, height) = overlay_bounds(config, info.work, dpi_x as f64 / 96.0)?;
            SetLastError(0);
            let previous = GetWindowLongPtrW(hwnd, GWL_EXSTYLE);
            if previous == 0 && GetLastError() != 0 {
                return Err(os_error("无法读取语音条窗口样式"));
            }
            let ex_style =
                (previous as u32 | WS_EX_NOACTIVATE | WS_EX_TOOLWINDOW) & !WS_EX_APPWINDOW;
            set_style(hwnd, GWL_EXSTYLE, ex_style)?;
            SetLastError(0);
            let previous = GetWindowLongPtrW(hwnd, GWL_STYLE);
            if previous == 0 && GetLastError() != 0 {
                return Err(os_error("无法读取语音条边框样式"));
            }
            // WS_CAPTION | WS_THICKFRAME | WS_SYSMENU | WS_MINIMIZEBOX | WS_MAXIMIZEBOX
            set_style(hwnd, GWL_STYLE, previous as u32 & !0x00cf_0000)?;
            let margins = Margins {
                left: -1,
                right: -1,
                top: -1,
                bottom: -1,
            };
            let result = DwmExtendFrameIntoClientArea(hwnd, &margins);
            if result < 0 {
                return Err(format!(
                    "无法启用语音条透明背景（Windows 错误 0x{:08X}）",
                    result as u32
                ));
            }
            if SetWindowPos(
                hwnd,
                -1,
                x,
                y,
                width,
                height,
                SWP_NOACTIVATE | SWP_FRAMECHANGED | SWP_NOOWNERZORDER,
            ) == 0
            {
                return Err(os_error("无法定位语音条窗口"));
            }
            Ok(())
        }
    }
    unsafe fn set_style(hwnd: isize, index: i32, value: u32) -> Result<(), String> {
        SetLastError(0);
        if SetWindowLongPtrW(hwnd, index, value as isize) == 0 && GetLastError() != 0 {
            return Err(os_error("无法设置语音条窗口样式"));
        }
        Ok(())
    }
    fn overlay_bounds(
        config: &AppConfig,
        work: Rect,
        dpi: f64,
    ) -> Result<(i32, i32, i32, i32), String> {
        let work_width = work.right as f64 - work.left as f64;
        let work_height = work.bottom as f64 - work.top as f64;
        if work_width <= 0.0 || work_height <= 0.0 {
            return Err("显示器工作区大小无效".into());
        }
        let width = (560.0 * config.appearance.scale * dpi)
            .round()
            .clamp(1.0, work_width);
        let height = (210.0 * config.appearance.scale * dpi)
            .round()
            .clamp(1.0, work_height);
        let anchor = config.appearance.anchor.as_str();
        if ![
            "top-left",
            "top",
            "top-right",
            "left",
            "center",
            "right",
            "bottom-left",
            "bottom",
            "bottom-right",
        ]
        .contains(&anchor)
        {
            return Err("无效的语音条弹出位置".into());
        }
        let margin = 16.0 * dpi;
        let x = if anchor.ends_with("left") {
            work.left as f64 + margin
        } else if anchor.ends_with("right") {
            work.right as f64 - width - margin
        } else {
            work.left as f64 + (work_width - width) / 2.0
        };
        let y = if anchor.starts_with("top") {
            work.top as f64 + margin
        } else if anchor.starts_with("bottom") {
            work.bottom as f64 - height - margin
        } else {
            work.top as f64 + (work_height - height) / 2.0
        };
        let x = (x + config.appearance.offset_x as f64 * dpi)
            .clamp(work.left as f64, work.right as f64 - width);
        let y = (y + config.appearance.offset_y as f64 * dpi)
            .clamp(work.top as f64, work.bottom as f64 - height);
        Ok((
            x.round() as i32,
            y.round() as i32,
            width as i32,
            height as i32,
        ))
    }

    #[cfg(test)]
    mod overlay_tests {
        use super::*;

        #[test]
        fn all_anchors_and_signed_offsets_stay_in_the_monitor_work_area() {
            for dpi in [1.0, 1.5, 2.0] {
                for anchor in [
                    "top-left",
                    "top",
                    "top-right",
                    "left",
                    "center",
                    "right",
                    "bottom-left",
                    "bottom",
                    "bottom-right",
                ] {
                    for offset in [(-9000, -9000), (-35, 27), (9000, 9000)] {
                        let mut config = AppConfig::default();
                        config.appearance.anchor = anchor.into();
                        config.appearance.offset_x = offset.0;
                        config.appearance.offset_y = offset.1;
                        let work = Rect {
                            left: -1920,
                            top: -300,
                            right: 0,
                            bottom: 740,
                        };
                        let (x, y, w, h) = overlay_bounds(&config, work, dpi).unwrap();
                        assert!(x >= -1920 && y >= -300, "{anchor}, dpi {dpi}");
                        assert!(x + w <= 0 && y + h <= 740, "{anchor}, dpi {dpi}");
                    }
                }
            }
        }

        #[test]
        fn bottom_right_position_applies_dpi_and_negative_offsets() {
            let mut config = AppConfig::default();
            config.appearance.anchor = "bottom-right".into();
            config.appearance.offset_x = -20;
            config.appearance.offset_y = -10;
            let work = Rect {
                left: 0,
                top: 0,
                right: 1920,
                bottom: 1040,
            };
            assert_eq!(
                overlay_bounds(&config, work, 1.5).unwrap(),
                (1026, 686, 840, 315)
            );
        }
    }

    pub fn show_window(hwnd: isize, activate: bool) {
        unsafe {
            if hwnd == 0 || IsWindow(hwnd) == 0 {
                eprintln!("无法显示窗口：窗口句柄无效");
                return;
            }
            if activate {
                ShowWindow(hwnd, if IsIconic(hwnd) != 0 { 9 } else { 5 });
                if SetForegroundWindow(hwnd) == 0 && GetForegroundWindow() != hwnd {
                    eprintln!("Windows 暂未允许激活 VoiceBar 窗口");
                }
            } else {
                ShowWindow(hwnd, 4); // SW_SHOWNOACTIVATE
                if SetWindowPos(
                    hwnd,
                    0,
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | 0x0004,
                ) == 0
                {
                    eprintln!("{}", os_error("无法无焦点显示窗口"));
                }
            }
        }
    }
    pub fn hide_window(hwnd: isize) {
        unsafe {
            if hwnd != 0 && IsWindow(hwnd) != 0 {
                ShowWindow(hwnd, 0);
            }
        }
    }
    pub fn is_visible(hwnd: isize) -> bool {
        hwnd != 0 && unsafe { IsWindow(hwnd) != 0 && IsWindowVisible(hwnd) != 0 }
    }
    pub fn set_window_icon(hwnd: isize) {
        if hwnd == 0 || unsafe { IsWindow(hwnd) } == 0 {
            return;
        }
        match app_icon() {
            Ok(icon) => unsafe {
                SendMessageW(hwnd, 0x0080, 0, icon);
                SendMessageW(hwnd, 0x0080, 1, icon);
            },
            Err(error) => eprintln!("{error}"),
        }
    }
}

#[cfg(windows)]
pub use native::{
    configure_overlay, hide_window, is_visible, set_window_icon, show_window, Desktop,
};

#[cfg(not(windows))]
pub struct Desktop;
#[cfg(not(windows))]
impl Desktop {
    pub fn new(_runtime: Arc<Runtime>, _explicit_hidden: bool) -> Result<Option<Self>, String> {
        Err("当前原生桌面集成仅支持 Windows".into())
    }
    pub fn save_config(&self, _config: AppConfig) -> Result<(), String> {
        Err("当前原生桌面集成仅支持 Windows".into())
    }
}
#[cfg(not(windows))]
pub fn configure_overlay(_hwnd: isize, _config: &AppConfig) -> Result<(), String> {
    Err("当前语音条仅支持 Windows".into())
}
#[cfg(not(windows))]
pub fn show_window(_hwnd: isize, _activate: bool) {}
#[cfg(not(windows))]
pub fn hide_window(_hwnd: isize) {}
#[cfg(not(windows))]
pub fn is_visible(_hwnd: isize) -> bool {
    false
}
#[cfg(not(windows))]
pub fn set_window_icon(_hwnd: isize) {}
