use base64::{engine::general_purpose::STANDARD, Engine};
use std::path::{Path, PathBuf};

// Windows interaction lives behind this small interface; no clipboard or target
// is touched until an actual recognition session has text to deliver.
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    pub type Hwnd = *mut c_void;
    // Only our own injected packets carry this marker. Other automation is
    // treated like user input, since it can also move the insertion point.
    pub const INPUT_MARKER: usize = 0x5642_5254;
    #[repr(C)]
    #[derive(Clone, Copy, Default)]
    pub struct Point {
        pub x: i32,
        pub y: i32,
    }
    #[repr(C)]
    #[derive(Default)]
    pub struct Rect {
        pub left: i32,
        pub top: i32,
        pub right: i32,
        pub bottom: i32,
    }
    #[repr(C)]
    pub struct GuiThreadInfo {
        pub size: u32,
        pub flags: u32,
        pub active: Hwnd,
        pub focus: Hwnd,
        pub capture: Hwnd,
        pub menu: Hwnd,
        pub move_size: Hwnd,
        pub caret: Hwnd,
        pub caret_rect: Rect,
    }
    #[repr(C)]
    pub struct Message {
        pub window: Hwnd,
        pub message: u32,
        pub wparam: usize,
        pub lparam: isize,
        pub time: u32,
        pub point: Point,
        pub private: u32,
    }
    #[repr(C)]
    pub struct KeyboardHook {
        pub vk: u32,
        pub scan: u32,
        pub flags: u32,
        pub time: u32,
        pub extra: usize,
    }
    #[repr(C)]
    pub struct MouseHook {
        pub point: Point,
        pub data: u32,
        pub flags: u32,
        pub time: u32,
        pub extra: usize,
    }
    pub type HookProc = unsafe extern "system" fn(i32, usize, isize) -> isize;
    #[repr(C)]
    pub struct DataBlob {
        pub len: u32,
        pub data: *mut u8,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Keyboard {
        pub vk: u16,
        pub scan: u16,
        pub flags: u32,
        pub time: u32,
        pub extra: usize,
    }
    #[repr(C)]
    #[derive(Clone, Copy)]
    pub struct Mouse {
        pub dx: i32,
        pub dy: i32,
        pub data: u32,
        pub flags: u32,
        pub time: u32,
        pub extra: usize,
    }
    #[repr(C)]
    pub union InputData {
        pub keyboard: Keyboard,
        pub mouse: Mouse,
    }
    #[repr(C)]
    pub struct Input {
        pub kind: u32,
        pub data: InputData,
    }
    #[link(name = "user32")]
    unsafe extern "system" {
        pub fn GetForegroundWindow() -> Hwnd;
        pub fn GetWindowThreadProcessId(window: Hwnd, pid: *mut u32) -> u32;
        pub fn GetGUIThreadInfo(thread: u32, info: *mut GuiThreadInfo) -> i32;
        pub fn GetAncestor(window: Hwnd, flags: u32) -> Hwnd;
        pub fn WindowFromPoint(point: Point) -> Hwnd;
        #[cfg(target_pointer_width = "64")]
        pub fn GetWindowLongPtrW(window: Hwnd, index: i32) -> isize;
        #[cfg(target_pointer_width = "32")]
        pub fn GetWindowLongW(window: Hwnd, index: i32) -> i32;
        pub fn SetWindowsHookExW(kind: i32, callback: HookProc, module: Hwnd, thread: u32) -> Hwnd;
        pub fn UnhookWindowsHookEx(hook: Hwnd) -> i32;
        pub fn CallNextHookEx(hook: Hwnd, code: i32, wparam: usize, lparam: isize) -> isize;
        pub fn GetMessageW(message: *mut Message, window: Hwnd, min: u32, max: u32) -> i32;
        pub fn PeekMessageW(
            message: *mut Message,
            window: Hwnd,
            min: u32,
            max: u32,
            remove: u32,
        ) -> i32;
        pub fn PostThreadMessageW(thread: u32, message: u32, wparam: usize, lparam: isize) -> i32;
        pub fn IsWindow(window: Hwnd) -> i32;
        pub fn SendInput(count: u32, inputs: *const Input, size: i32) -> u32;
        pub fn GetAsyncKeyState(key: i32) -> i16;
        pub fn OpenClipboard(window: Hwnd) -> i32;
        pub fn CloseClipboard() -> i32;
        pub fn EmptyClipboard() -> i32;
        pub fn GetClipboardData(format: u32) -> *mut c_void;
        pub fn SetClipboardData(format: u32, data: *mut c_void) -> *mut c_void;
        pub fn GetClipboardSequenceNumber() -> u32;
    }
    #[link(name = "kernel32")]
    unsafe extern "system" {
        pub fn GlobalAlloc(flags: u32, size: usize) -> *mut c_void;
        pub fn GlobalLock(data: *mut c_void) -> *mut c_void;
        pub fn GlobalUnlock(data: *mut c_void) -> i32;
        pub fn GlobalFree(data: *mut c_void) -> *mut c_void;
        pub fn GlobalSize(data: *mut c_void) -> usize;
        pub fn LocalFree(data: *mut c_void) -> *mut c_void;
        pub fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
        pub fn GetLastError() -> u32;
        pub fn GetCurrentThreadId() -> u32;
        pub fn GetModuleHandleW(name: *const u16) -> Hwnd;
        pub fn OpenProcess(access: u32, inherit: i32, pid: u32) -> *mut c_void;
        pub fn CloseHandle(handle: *mut c_void) -> i32;
    }
    #[link(name = "crypt32")]
    unsafe extern "system" {
        pub fn CryptProtectData(
            input: *const DataBlob,
            description: *const u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
        pub fn CryptUnprotectData(
            input: *const DataBlob,
            description: *mut *mut u16,
            entropy: *const DataBlob,
            reserved: *mut c_void,
            prompt: *mut c_void,
            flags: u32,
            output: *mut DataBlob,
        ) -> i32;
    }
    #[repr(C)]
    #[derive(Default)]
    pub struct MemCounters {
        pub cb: u32,
        pub page_fault: u32,
        pub peak_working: usize,
        pub working: usize,
        pub peak_paged: usize,
        pub paged: usize,
        pub peak_nonpaged: usize,
        pub nonpaged: usize,
        pub pagefile: usize,
        pub peak_pagefile: usize,
    }
    #[link(name = "psapi")]
    unsafe extern "system" {
        pub fn GetProcessMemoryInfo(
            process: *mut c_void,
            counters: *mut MemCounters,
            size: u32,
        ) -> i32;
    }
    #[link(name = "shell32")]
    unsafe extern "system" {
        pub fn SetCurrentProcessExplicitAppUserModelID(app_id: *const u16) -> i32;
    }
    #[link(name = "advapi32")]
    unsafe extern "system" {
        pub fn RegOpenKeyExW(
            key: *mut c_void,
            sub: *const u16,
            options: u32,
            access: u32,
            result: *mut *mut c_void,
        ) -> i32;
        pub fn RegSetValueExW(
            key: *mut c_void,
            name: *const u16,
            reserved: u32,
            kind: u32,
            data: *const u8,
            size: u32,
        ) -> i32;
        pub fn RegDeleteValueW(key: *mut c_void, name: *const u16) -> i32;
        pub fn RegQueryValueExW(
            key: *mut c_void,
            name: *const u16,
            reserved: *mut u32,
            kind: *mut u32,
            data: *mut u8,
            size: *mut u32,
        ) -> i32;
        pub fn RegCloseKey(key: *mut c_void) -> i32;
    }

    pub fn key(vk: u16, up: bool) -> Input {
        Input {
            kind: 1,
            data: InputData {
                keyboard: Keyboard {
                    vk,
                    scan: 0,
                    flags: if up { 2 } else { 0 },
                    time: 0,
                    extra: INPUT_MARKER,
                },
            },
        }
    }
    pub fn send(inputs: &[Input]) -> Result<(), String> {
        let sent = unsafe {
            SendInput(
                inputs.len() as u32,
                inputs.as_ptr(),
                std::mem::size_of::<Input>() as i32,
            )
        };
        if sent != inputs.len() as u32 {
            return Err(
                "Windows 拒绝输入。目标可能以管理员身份运行；识别文本仍可在语音条中复制。".into(),
            );
        }
        Ok(())
    }
}
/// A session's insertion point. Integer handles keep this snapshot Copy + Send;
/// it never reactivates a window or guesses a new target after focus changes.
#[derive(Clone, Copy, Debug)]
pub struct InputTarget {
    window: isize,
    focus: isize,
    thread: u32,
    pid: u32,
    generation: u64,
    revision: u64,
    unavailable: Option<&'static str>,
}

impl InputTarget {
    /// Call before showing the non-activating recording overlay. A failed
    /// capture remains usable for transcript preview, but cannot inject input.
    pub fn capture(toggle_hotkey: &str) -> Self {
        #[cfg(windows)]
        let result = realtime::capture(toggle_hotkey);
        #[cfg(not(windows))]
        let result: Result<Self, &'static str> = {
            let _ = toggle_hotkey;
            Err("全局输入目前仅支持 Windows")
        };
        result.unwrap_or_else(|reason| {
            let window = foreground();
            Self {
                window,
                focus: 0,
                thread: 0,
                pid: process_id(window),
                generation: 0,
                revision: 0,
                unavailable: Some(reason),
            }
        })
    }
    pub fn window(&self) -> isize {
        self.window
    }
    pub fn process_id(&self) -> u32 {
        self.pid
    }
    pub fn valid(&self) -> bool {
        self.validate().is_ok()
    }
    pub fn validate(&self) -> Result<(), String> {
        if let Some(reason) = self.unavailable {
            return Err(reason.into());
        }
        #[cfg(windows)]
        {
            realtime::validate(self)
        }
        #[cfg(not(windows))]
        {
            Err("全局输入目前仅支持 Windows".into())
        }
    }
    /// Call when the session ends, including cancellation. Old copies become
    /// invalid immediately; native hook handles are retired on their own thread.
    pub fn release(&self) {
        #[cfg(windows)]
        realtime::release(self);
    }
}

/// Replace only the exact session-owned suffix supplied by Delivery. An error
/// can follow partial delivery: stop this session's automatic input, retain the
/// full transcript, and never retry the whole replacement.
///
/// Real-time clipboard requests deliberately use Unicode packets: asynchronous
/// paste consumption cannot safely delimit a subsequently removable suffix.
pub fn realtime_edit(
    target: &InputTarget,
    remove: &str,
    insert: &str,
    method: &str,
) -> Result<(), String> {
    if !["unicode", "clipboard"].contains(&method) {
        return Err("无效的实时输入方式".into());
    }
    // Smart Backspace can consume an entire indentation level in one press.
    // Never guess how many spaces an arbitrary editor will remove.
    if remove.chars().any(char::is_whitespace) {
        return Err(
            "实时改写不撤回空白，避免编辑器智能退格误删；自动写入暂停，请复制完整结果。".into(),
        );
    }
    // Reject the entire edit before its first Backspace. Editors disagree about
    // surrogate pairs, combining sequences, indentation and line breaks.
    if !remove.chars().chain(insert.chars()).all(realtime_scalar) {
        return Err("实时改写暂不支持表情、组合字符或换行，自动写入暂停；请复制完整结果。".into());
    }
    target.validate()?;
    #[cfg(windows)]
    {
        realtime::edit(target, remove.chars().count(), insert)
    }
    #[cfg(not(windows))]
    {
        Err("全局输入目前仅支持 Windows".into())
    }
}

fn realtime_scalar(ch: char) -> bool {
    // Conservative single-BMP-scalar repertoire: Latin, Greek/Cyrillic,
    // precomposed Hangul, Chinese and Japanese text, common punctuation.
    // Exclude marks, joiners, variation selectors and complex-script clusters;
    // one Backspace must never eat a neighbouring user character.
    matches!(ch as u32,
        0x20..=0x7e | 0xa0..=0x2ff |
        0x370..=0x482 | 0x48a..=0x52f | 0x1e00..=0x1eff |
        0x2000..=0x200a | 0x2010..=0x2027 | 0x2030..=0x205e |
        0x3000..=0x3029 | 0x3030..=0x3098 | 0x309b..=0x30ff |
        0x3400..=0x4dbf | 0x4e00..=0x9fff | 0xac00..=0xd7a3 |
        0xf900..=0xfaff | 0xff01..=0xff60 | 0xffe0..=0xffe6
    )
}

#[cfg(windows)]
mod realtime {
    use super::{win, InputTarget};
    use std::{
        ptr,
        sync::{
            atomic::{AtomicBool, AtomicU64, Ordering},
            mpsc, Arc, Mutex, OnceLock,
        },
        time::{Duration, Instant},
    };

    const UNAVAILABLE: &str = "无法建立安全键鼠监测，自动写入暂停；识别结果仍可复制。";
    const FOCUS_CHANGED: &str = "输入窗口或控件已切换，自动写入暂停；请复制完整结果。";
    const USER_EDITED: &str = "检测到键入、光标操作或滚动，自动写入暂停；请复制完整结果。";
    const WAKE: u32 = 0x8000 + 0x56b;
    static ALIVE: AtomicBool = AtomicBool::new(false);
    static GENERATION: AtomicU64 = AtomicU64::new(1);
    static STATE: Mutex<Option<Guard>> = Mutex::new(None);
    static EDIT_LOCK: Mutex<()> = Mutex::new(());

    #[derive(Clone, Copy, Debug, PartialEq, Eq)]
    struct Toggle {
        mods: u8,
        vk: u32,
    }
    impl Toggle {
        fn parse(text: &str) -> Result<Self, &'static str> {
            let mut mods = 0;
            let mut vk = None;
            for raw in text.split('+') {
                if vk.is_some() {
                    return Err(UNAVAILABLE);
                }
                let part = raw.trim().to_ascii_lowercase();
                mods |= match part.as_str() {
                    "shift" => 1,
                    "ctrl" | "control" | "commandorcontrol" | "commandorctrl" | "cmdorctrl"
                    | "cmdorcontrol" => 2,
                    "alt" | "option" => 4,
                    "win" | "super" | "meta" | "cmd" | "command" => 8,
                    _ => {
                        vk = Some(virtual_key(&part).ok_or(UNAVAILABLE)?);
                        0
                    }
                };
            }
            Ok(Self {
                mods,
                vk: vk.ok_or(UNAVAILABLE)?,
            })
        }
    }

    fn virtual_key(key: &str) -> Option<u32> {
        let letter = key.strip_prefix("key").unwrap_or(key);
        if letter.len() == 1 && letter.as_bytes()[0].is_ascii_lowercase() {
            return Some(letter.as_bytes()[0].to_ascii_uppercase() as u32);
        }
        let digit = key.strip_prefix("digit").unwrap_or(key);
        if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
            return Some(digit.as_bytes()[0] as u32);
        }
        if let Some(n) = key.strip_prefix('f').and_then(|n| n.parse::<u32>().ok()) {
            if (1..=24).contains(&n) {
                return Some(0x70 + n - 1);
            }
        }
        if let Some(n) = key
            .strip_prefix("numpad")
            .or_else(|| key.strip_prefix("num"))
            .and_then(|n| n.parse::<u32>().ok())
        {
            if n <= 9 {
                return Some(0x60 + n);
            }
        }
        Some(match key {
            "backquote" | "`" => 0xc0,
            "backslash" | "\\" => 0xdc,
            "bracketleft" | "[" => 0xdb,
            "bracketright" | "]" => 0xdd,
            "comma" | "," => 0xbc,
            "equal" | "=" => 0xbb,
            "minus" | "-" => 0xbd,
            "period" | "." => 0xbe,
            "quote" | "'" => 0xde,
            "semicolon" | ";" => 0xba,
            "slash" | "/" => 0xbf,
            "backspace" => 8,
            "tab" => 9,
            "enter" | "numpadenter" | "numenter" => 0x0d,
            "pause" | "pausebreak" | "mediapause" => 0x13,
            "capslock" => 0x14,
            "escape" | "esc" => 0x1b,
            "space" => 0x20,
            "pageup" => 0x21,
            "pagedown" => 0x22,
            "end" => 0x23,
            "home" => 0x24,
            "arrowleft" | "left" => 0x25,
            "arrowup" | "up" => 0x26,
            "arrowright" | "right" => 0x27,
            "arrowdown" | "down" => 0x28,
            "printscreen" => 0x2c,
            "insert" => 0x2d,
            "delete" => 0x2e,
            "numlock" => 0x90,
            "scrolllock" => 0x91,
            "numpadadd" | "numadd" | "numpadplus" | "numplus" => 0x6b,
            "numpaddecimal" | "numdecimal" => 0x6e,
            "numpaddivide" | "numdivide" => 0x6f,
            // Match the shortcut backend's Windows mapping.
            "numpadequal" | "numequal" => 0x45,
            "numpadmultiply" | "nummultiply" => 0x6a,
            "numpadsubtract" | "numsubtract" => 0x6d,
            "audiovolumedown" | "volumedown" => 0xae,
            "audiovolumeup" | "volumeup" => 0xaf,
            "audiovolumemute" | "volumemute" => 0xad,
            "mediaplay" => 0xfa,
            "mediaplaypause" => 0xb3,
            "mediastop" => 0xb2,
            "mediatracknext" => 0xb0,
            "mediatrackprev" | "mediatrackprevious" => 0xb1,
            _ => return None,
        })
    }

    fn modifier(vk: usize) -> bool {
        matches!(vk, 0x10..=0x12 | 0x5b | 0x5c | 0xa0..=0xa5)
    }
    fn modifier_mask(keys: &[bool; 256]) -> u8 {
        u8::from(keys[0xa0] || keys[0xa1])
            | (u8::from(keys[0xa2] || keys[0xa3]) << 1)
            | (u8::from(keys[0xa4] || keys[0xa5]) << 2)
            | (u8::from(keys[0x5b] || keys[0x5c]) << 3)
    }

    struct Guard {
        target: InputTarget,
        toggle: Toggle,
        keys: [bool; 256],
        toggle_down: bool,
        overlay_buttons: u8,
        revision: u64,
    }
    impl Guard {
        fn new(target: InputTarget, toggle: Toggle, keys: [bool; 256]) -> Self {
            Self {
                target,
                toggle,
                keys,
                toggle_down: keys[toggle.vk as usize] && modifier_mask(&keys) == toggle.mods,
                overlay_buttons: 0,
                revision: 0,
            }
        }
        fn key(&mut self, vk: u32, up: bool, ours: bool) {
            if ours {
                return;
            }
            let vk = vk as usize;
            if vk >= self.keys.len() {
                self.revision += 1;
                return;
            }
            self.keys[vk] = !up;
            // LL hooks normally report left/right modifier codes. Accommodate
            // generic codes too without leaving a stale generic "down" bit.
            if (0x10..=0x12).contains(&vk) {
                let left = 0xa0 + (vk - 0x10) * 2;
                self.keys[left] = !up;
                if up {
                    self.keys[left + 1] = false;
                }
            }
            if modifier(vk) {
                return;
            }
            let exact_toggle = vk == self.toggle.vk as usize
                && modifier_mask(&self.keys) == self.toggle.mods
                && !(8..256).any(|k| k != vk && !modifier(k) && self.keys[k]);
            if vk == self.toggle.vk as usize {
                if up && self.toggle_down {
                    self.toggle_down = false;
                    return;
                }
                if !up && exact_toggle {
                    self.toggle_down = true;
                    return;
                }
            }
            self.revision += 1;
        }
        fn mouse(&mut self, message: u32, data: u32, ours: bool, overlay: bool) {
            if ours || message == 0x200 {
                return;
            } // WM_MOUSEMOVE
            let (button, up) = match message {
                0x201 | 0x202 => (1, message == 0x202),
                0x204 | 0x205 => (2, message == 0x205),
                0x207 | 0x208 => (4, message == 0x208),
                0x20b | 0x20c => (if data >> 16 == 1 { 8 } else { 16 }, message == 0x20c),
                _ => (0, false),
            };
            if up && self.overlay_buttons & button != 0 {
                self.overlay_buttons &= !button;
                return;
            }
            if overlay {
                if !up {
                    self.overlay_buttons |= button;
                }
                return;
            }
            self.revision += 1;
        }
    }

    fn focus_snapshot() -> Result<(isize, isize, u32, u32), &'static str> {
        unsafe {
            let window = win::GetForegroundWindow();
            let mut pid = 0;
            let thread = win::GetWindowThreadProcessId(window, &mut pid);
            let mut info: win::GuiThreadInfo = std::mem::zeroed();
            info.size = std::mem::size_of::<win::GuiThreadInfo>() as u32;
            if window.is_null()
                || thread == 0
                || pid == 0
                || win::GetGUIThreadInfo(thread, &mut info) == 0
                || info.focus.is_null()
                || win::IsWindow(info.focus) == 0
                || win::GetAncestor(info.focus, 2) != window
                || info.flags & 0x1e != 0
            // moving, menus or popup menus
            {
                return Err(FOCUS_CHANGED);
            }
            Ok((window as isize, info.focus as isize, thread, pid))
        }
    }

    fn invalidate(target: &InputTarget) {
        if let Ok(mut state) = STATE.lock() {
            if let Some(guard) = state
                .as_mut()
                .filter(|g| g.target.generation == target.generation)
            {
                guard.revision += 1;
            }
        }
    }
    pub(super) fn validate(target: &InputTarget) -> Result<(), String> {
        if !ALIVE.load(Ordering::SeqCst) {
            return Err(UNAVAILABLE.into());
        }
        if focus_snapshot().ok() != Some((target.window, target.focus, target.thread, target.pid)) {
            invalidate(target);
            return Err(FOCUS_CHANGED.into());
        }
        let state = STATE.lock().map_err(|_| UNAVAILABLE)?;
        if state.as_ref().is_some_and(|g| {
            g.target.generation == target.generation && g.revision == target.revision
        }) {
            Ok(())
        } else {
            Err(USER_EDITED.into())
        }
    }

    unsafe extern "system" fn keyboard_hook(code: i32, wparam: usize, lparam: isize) -> isize {
        if code >= 0 && matches!(wparam, 0x100 | 0x101 | 0x104 | 0x105) {
            let event = &*(lparam as *const win::KeyboardHook);
            if let Ok(mut state) = STATE.lock() {
                if let Some(guard) = state.as_mut() {
                    guard.key(
                        event.vk,
                        matches!(wparam, 0x101 | 0x105),
                        event.flags & 0x10 != 0 && event.extra == win::INPUT_MARKER,
                    );
                }
            }
        }
        win::CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
    }
    fn overlay_at(point: win::Point) -> bool {
        unsafe {
            let root = win::GetAncestor(win::WindowFromPoint(point), 2);
            if root.is_null() || super::process_id(root as isize) != std::process::id() {
                return false;
            }
            #[cfg(target_pointer_width = "64")]
            let style = win::GetWindowLongPtrW(root, -20);
            #[cfg(target_pointer_width = "32")]
            let style = win::GetWindowLongW(root, -20) as isize;
            // WS_EX_NOACTIVATE identifies our overlay; a click-through window
            // must never hide clicks that actually reach the underlying editor.
            style & 0x0800_0000 != 0 && style & 0x20 == 0
        }
    }
    unsafe extern "system" fn mouse_hook(code: i32, wparam: usize, lparam: isize) -> isize {
        if code >= 0 && wparam != 0x200 {
            let event = &*(lparam as *const win::MouseHook);
            let overlay = overlay_at(event.point);
            if let Ok(mut state) = STATE.lock() {
                if let Some(guard) = state.as_mut() {
                    guard.mouse(
                        wparam as u32,
                        event.data,
                        event.flags & 1 != 0 && event.extra == win::INPUT_MARKER,
                        overlay,
                    );
                }
            }
        }
        win::CallNextHookEx(ptr::null_mut(), code, wparam, lparam)
    }

    struct Hooks(win::Hwnd, win::Hwnd);
    impl Hooks {
        fn install() -> Result<Self, &'static str> {
            unsafe {
                let module = win::GetModuleHandleW(ptr::null());
                let keyboard = win::SetWindowsHookExW(13, keyboard_hook, module, 0);
                let mouse = win::SetWindowsHookExW(14, mouse_hook, module, 0);
                let hooks = Self(keyboard, mouse);
                if keyboard.is_null() || mouse.is_null() {
                    return Err(UNAVAILABLE); // Drop also releases a partial install.
                }
                Ok(hooks)
            }
        }
    }
    impl Drop for Hooks {
        fn drop(&mut self) {
            unsafe {
                if !self.0.is_null() {
                    win::UnhookWindowsHookEx(self.0);
                }
                if !self.1.is_null() {
                    win::UnhookWindowsHookEx(self.1);
                }
            }
        }
    }

    enum Command {
        Capture(Toggle, u64, mpsc::Sender<Result<InputTarget, &'static str>>),
        Step(
            InputTarget,
            Option<u16>,
            Arc<AtomicBool>,
            mpsc::Sender<Result<bool, String>>,
        ),
        Release(u64),
    }
    struct Monitor {
        commands: mpsc::Sender<Command>,
        thread: u32,
    }
    fn monitor() -> Result<&'static Monitor, &'static str> {
        static MONITOR: OnceLock<Result<Monitor, &'static str>> = OnceLock::new();
        MONITOR
            .get_or_init(|| {
                let (commands, receive) = mpsc::channel();
                let (ready, started) = mpsc::sync_channel(1);
                std::thread::Builder::new()
                    .name("voicebar-input-guard".into())
                    .spawn(move || {
                        let mut message: win::Message = unsafe { std::mem::zeroed() };
                        unsafe {
                            win::PeekMessageW(&mut message, ptr::null_mut(), 0, 0, 0);
                        }
                        ALIVE.store(true, Ordering::SeqCst);
                        let _ = ready.send(unsafe { win::GetCurrentThreadId() });
                        let mut hooks = None;
                        let mut hook_generation = 0;
                        while unsafe { win::GetMessageW(&mut message, ptr::null_mut(), 0, 0) } > 0 {
                            if message.message != WAKE {
                                continue;
                            }
                            let Ok(command) = receive.try_recv() else {
                                continue;
                            };
                            match command {
                                Command::Capture(toggle, generation, reply) => {
                                    hooks.take();
                                    if let Ok(mut state) = STATE.lock() {
                                        *state = None;
                                    }
                                    let result = (|| {
                                        let installed = Hooks::install()?;
                                        let (window, focus, thread, pid) = focus_snapshot()?;
                                        let target = InputTarget {
                                            window,
                                            focus,
                                            thread,
                                            pid,
                                            generation,
                                            revision: 0,
                                            unavailable: None,
                                        };
                                        let mut keys = [false; 256];
                                        for (vk, down) in keys.iter_mut().enumerate() {
                                            *down = unsafe { win::GetAsyncKeyState(vk as i32) < 0 };
                                        }
                                        for vk in [0x10, 0x11, 0x12] {
                                            keys[vk] = false;
                                        }
                                        *STATE.lock().map_err(|_| UNAVAILABLE)? =
                                            Some(Guard::new(target, toggle, keys));
                                        hook_generation = target.generation;
                                        hooks = Some(installed);
                                        Ok(target)
                                    })();
                                    if reply.send(result).is_err() {
                                        if let Ok(mut state) = STATE.lock() {
                                            if state
                                                .as_ref()
                                                .is_some_and(|g| g.target.generation == generation)
                                            {
                                                *state = None;
                                            }
                                        }
                                        if hook_generation == generation {
                                            hooks.take();
                                            hook_generation = 0;
                                        }
                                    }
                                }
                                Command::Step(target, scan, live, reply) => {
                                    let result = (|| {
                                        if !live.load(Ordering::SeqCst) {
                                            return Err(UNAVAILABLE.into());
                                        }
                                        validate(&target)?;
                                        if !super::modifiers_released() {
                                            // No input sent: wait for the toggle to
                                            // finish releasing without invalidation.
                                            return Ok(false);
                                        }
                                        let inputs = match scan {
                                            None => [win::key(8, false), win::key(8, true)],
                                            Some(scan) => [4, 6].map(|flags| win::Input {
                                                kind: 1,
                                                data: win::InputData {
                                                    keyboard: win::Keyboard {
                                                        vk: 0,
                                                        scan,
                                                        flags,
                                                        time: 0,
                                                        extra: win::INPUT_MARKER,
                                                    },
                                                },
                                            }),
                                        };
                                        // Hook callbacks and this validation/injection run on
                                        // one message-pump thread; each down/up pair is atomic.
                                        if !live.load(Ordering::SeqCst) {
                                            return Err(UNAVAILABLE.into());
                                        }
                                        win::send(&inputs)?;
                                        validate(&target)?;
                                        Ok(true)
                                    })();
                                    if result.is_err() {
                                        invalidate(&target);
                                    }
                                    let _ = reply.send(result);
                                }
                                Command::Release(generation) => {
                                    if let Ok(mut state) = STATE.lock() {
                                        if state
                                            .as_ref()
                                            .is_some_and(|g| g.target.generation == generation)
                                        {
                                            *state = None;
                                        }
                                    }
                                    if generation == hook_generation {
                                        hooks.take();
                                        hook_generation = 0;
                                    }
                                }
                            }
                        }
                        ALIVE.store(false, Ordering::SeqCst);
                        if let Ok(mut state) = STATE.lock() {
                            *state = None;
                        }
                    })
                    .map_err(|_| UNAVAILABLE)?;
                let thread = started
                    .recv_timeout(Duration::from_secs(2))
                    .map_err(|_| UNAVAILABLE)?;
                Ok(Monitor { commands, thread })
            })
            .as_ref()
            .map_err(|reason| *reason)
    }
    impl Monitor {
        fn post(&self, command: Command) -> Result<(), &'static str> {
            if !ALIVE.load(Ordering::SeqCst) {
                return Err(UNAVAILABLE);
            }
            self.commands.send(command).map_err(|_| UNAVAILABLE)?;
            if unsafe { win::PostThreadMessageW(self.thread, WAKE, 0, 0) } == 0 {
                return Err(UNAVAILABLE);
            }
            Ok(())
        }
        fn step(&self, target: &InputTarget, scan: Option<u16>) -> Result<bool, String> {
            let (reply, receive) = mpsc::channel();
            let live = Arc::new(AtomicBool::new(true));
            let result = self
                .post(Command::Step(*target, scan, live.clone(), reply))
                .map_err(str::to_string)
                .and_then(|_| {
                    receive
                        .recv_timeout(Duration::from_secs(2))
                        .map_err(|_| UNAVAILABLE.to_string())
                })
                .and_then(|result| result);
            live.store(false, Ordering::SeqCst);
            if result.is_err() {
                invalidate(target);
            }
            result
        }
    }

    pub(super) fn capture(hotkey: &str) -> Result<InputTarget, &'static str> {
        let toggle = Toggle::parse(hotkey)?;
        let generation = GENERATION.fetch_add(1, Ordering::SeqCst);
        let (reply, receive) = mpsc::channel();
        let monitor = monitor()?;
        monitor.post(Command::Capture(toggle, generation, reply))?;
        match receive.recv_timeout(Duration::from_secs(2)) {
            Ok(result) => result,
            Err(_) => {
                // Even if send() raced with recv_timeout(), retire the known
                // generation; a fallback snapshot has no native hooks to own.
                let _ = monitor.post(Command::Release(generation));
                Err(UNAVAILABLE)
            }
        }
    }
    pub(super) fn release(target: &InputTarget) {
        if let Ok(mut state) = STATE.lock() {
            if state
                .as_ref()
                .is_some_and(|g| g.target.generation == target.generation)
            {
                *state = None;
            }
        }
        if let Ok(monitor) = monitor() {
            let _ = monitor.post(Command::Release(target.generation));
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;

        fn target() -> InputTarget {
            InputTarget {
                window: 1,
                focus: 2,
                thread: 3,
                pid: 4,
                generation: 5,
                revision: 0,
                unavailable: None,
            }
        }
        fn guard(hotkey: &str) -> Guard {
            Guard::new(target(), Toggle::parse(hotkey).unwrap(), [false; 256])
        }

        #[test]
        fn configured_toggle_and_its_release_do_not_interrupt_delivery() {
            let toggle = Toggle::parse("Ctrl+Alt+Space").unwrap();
            let mut held = [false; 256];
            held[0xa2] = true;
            held[0xa4] = true;
            held[0x20] = true;
            let mut g = Guard::new(target(), toggle, held);
            // Capture happens while the start shortcut is still held. Modifier
            // releases can precede its final Space release.
            for vk in [0xa2, 0xa4, 0x20] {
                g.key(vk, true, false);
            }
            assert_eq!(g.revision, 0);
            // Stop shortcut, auto-repeat, and a different release order.
            for vk in [0xa2, 0xa4, 0x20, 0x20] {
                g.key(vk, false, false);
            }
            for vk in [0x20, 0xa4, 0xa2] {
                g.key(vk, true, false);
            }
            assert_eq!(g.revision, 0);
        }

        #[test]
        fn only_the_exact_toggle_is_exempt() {
            let mut g = guard("Ctrl+Alt+Space");
            for vk in [0xa2, 0xa4, 0xa0, 0x20] {
                g.key(vk, false, false);
            }
            assert_eq!(g.revision, 1, "an extra Shift must not be ignored");
            let mut g = guard("Ctrl+Alt+Space");
            g.key(0x20, false, false);
            assert_eq!(g.revision, 1, "plain Space is ordinary typing");
            let mut g = guard("Shift+F9");
            for vk in [0xa0, 0x78] {
                g.key(vk, false, false);
            }
            for vk in [0xa0, 0x78] {
                g.key(vk, true, false);
            }
            assert_eq!(g.revision, 0);
            g.key(0x25, false, false);
            assert_eq!(g.revision, 1, "a caret move in the same control pauses");
        }

        #[test]
        fn modifiers_and_our_packets_are_ignored_but_other_input_is_observed() {
            let mut g = guard("F8");
            for vk in [0xa0, 0xa1, 0xa2, 0xa3, 0xa4, 0xa5, 0x5b, 0x5c] {
                g.key(vk, false, false);
                g.key(vk, true, false);
            }
            for vk in [0xe7, 8] {
                g.key(vk, false, true);
                g.key(vk, true, true);
            }
            assert_eq!(g.revision, 0);
            g.key(0x41, false, false);
            assert_eq!(g.revision, 1, "foreign SendInput also changes the target");
        }

        #[test]
        fn mouse_move_and_nonactivating_overlay_click_are_safe() {
            let mut g = guard("F8");
            g.mouse(0x200, 0, false, false);
            g.mouse(0x201, 0, false, true);
            // The stop button may destroy its overlay before mouse-up.
            g.mouse(0x202, 0, false, false);
            g.mouse(0x20a, 0, false, true);
            assert_eq!(g.revision, 0);
            g.mouse(0x201, 0, false, false);
            assert_eq!(g.revision, 1);
        }

        #[test]
        fn editor_clicks_and_both_wheels_pause_delivery() {
            for message in [
                0x201, 0x202, 0x204, 0x205, 0x207, 0x208, 0x20a, 0x20b, 0x20c, 0x20e,
            ] {
                let mut g = guard("F8");
                g.mouse(message, 0, false, false);
                assert_eq!(g.revision, 1, "mouse message {message:#x}");
            }
        }

        #[test]
        fn shortcut_aliases_match_the_registered_windows_keys() {
            assert_eq!(
                Toggle::parse("Control+Option+Space").unwrap(),
                Toggle { mods: 6, vk: 0x20 }
            );
            assert_eq!(
                Toggle::parse("win+shift+KeyA").unwrap(),
                Toggle { mods: 9, vk: 0x41 }
            );
            assert_eq!(
                Toggle::parse("Ctrl+NumpadEnter").unwrap(),
                Toggle { mods: 2, vk: 13 }
            );
            assert!(Toggle::parse("Ctrl+Space+Shift").is_err());
            assert!(Toggle::parse("Ctrl++Space").is_err());
        }
    }

    pub(super) fn edit(
        target: &InputTarget,
        backspaces: usize,
        insert: &str,
    ) -> Result<(), String> {
        let _editing = EDIT_LOCK.lock().map_err(|_| UNAVAILABLE)?;
        let monitor = monitor()?;
        for scan in
            std::iter::repeat_n(None, backspaces).chain(insert.chars().map(|ch| Some(ch as u16)))
        {
            // A configured stop shortcut is allowed to finish releasing. Its
            // modifier/key-up events do not poison the session revision.
            let deadline = Instant::now() + Duration::from_secs(2);
            loop {
                validate(target)?;
                if Instant::now() >= deadline {
                    return Err(
                        "请松开 Ctrl / Alt / Shift / Win；实时写入已暂停，结果仍可复制。".into(),
                    );
                }
                if super::modifiers_released() && monitor.step(target, scan)? {
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            std::thread::sleep(Duration::from_millis(8));
        }
        validate(target)
    }
}

pub fn foreground() -> isize {
    #[cfg(windows)]
    {
        unsafe { win::GetForegroundWindow() as isize }
    }
    #[cfg(not(windows))]
    {
        0
    }
}
pub fn process_id(target: isize) -> u32 {
    #[cfg(windows)]
    {
        let mut id = 0;
        unsafe { win::GetWindowThreadProcessId(target as win::Hwnd, &mut id) };
        id
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        0
    }
}
pub fn target_valid(target: isize) -> bool {
    #[cfg(windows)]
    {
        target != 0 && unsafe { win::IsWindow(target as win::Hwnd) != 0 } && foreground() == target
    }
    #[cfg(not(windows))]
    {
        let _ = target;
        false
    }
}
pub fn modifiers_released() -> bool {
    #[cfg(windows)]
    {
        [0x10, 0x11, 0x12, 0x5b, 0x5c]
            .iter()
            .all(|k| unsafe { win::GetAsyncKeyState(*k) >= 0 })
    }
    #[cfg(not(windows))]
    {
        true
    }
}
pub fn type_text(target: isize, text: &str) -> Result<(), String> {
    if text.is_empty() {
        return Ok(());
    }
    if !target_valid(target) {
        return Err("输入焦点已切换，自动写入暂停；结束后可复制文本。".into());
    }
    if !modifiers_released() {
        return Err("请松开 Ctrl / Alt / Shift 后重试输入。".into());
    }
    #[cfg(windows)]
    {
        // Windows Text Services may translate several queued VK_PACKET events
        // using the last packet's character. Pace complete Unicode scalars so
        // WinUI/IME consumers can process each character before the next one.
        for ch in text.chars() {
            if !target_valid(target) || !modifiers_released() {
                return Err("输入过程中焦点或按键状态变化，已停止写入。".into());
            }
            let mut utf16 = [0u16; 2];
            let units = ch.encode_utf16(&mut utf16);
            let mut inputs = Vec::with_capacity(units.len() * 2);
            for &scan in units.iter() {
                for flags in [4, 6] {
                    inputs.push(win::Input {
                        kind: 1,
                        data: win::InputData {
                            keyboard: win::Keyboard {
                                vk: 0,
                                scan,
                                flags,
                                time: 0,
                                extra: win::INPUT_MARKER,
                            },
                        },
                    });
                }
            }
            win::send(&inputs)?;
            std::thread::sleep(std::time::Duration::from_millis(8));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        Err("全局输入目前仅支持 Windows".into())
    }
}
pub fn copy_text(text: &str) -> Result<(), String> {
    #[cfg(windows)]
    {
        set_clipboard(text).map(|_| ())
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err("剪贴板目前仅支持 Windows".into())
    }
}
#[cfg(windows)]
fn read_clipboard() -> Result<Option<String>, String> {
    unsafe {
        if win::OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("剪贴板暂时被其他应用占用".into());
        }
        let data = win::GetClipboardData(13);
        let result = if data.is_null() {
            None
        } else {
            let ptr = win::GlobalLock(data) as *const u16;
            if ptr.is_null() {
                None
            } else {
                let max = win::GlobalSize(data) / 2;
                let slice = std::slice::from_raw_parts(ptr, max);
                let len = slice.iter().position(|&v| v == 0).unwrap_or(max);
                let s = String::from_utf16_lossy(&slice[..len]);
                win::GlobalUnlock(data);
                Some(s)
            }
        };
        win::CloseClipboard();
        Ok(result)
    }
}
#[cfg(windows)]
fn set_clipboard(text: &str) -> Result<u32, String> {
    let utf16: Vec<_> = text.encode_utf16().chain(std::iter::once(0)).collect();
    unsafe {
        if win::OpenClipboard(std::ptr::null_mut()) == 0 {
            return Err("剪贴板暂时被其他应用占用".into());
        }
        let data = win::GlobalAlloc(0x42, utf16.len() * 2);
        if data.is_null() {
            win::CloseClipboard();
            return Err("无法分配剪贴板内存".into());
        }
        let ptr = win::GlobalLock(data) as *mut u16;
        if ptr.is_null() {
            win::GlobalFree(data);
            win::CloseClipboard();
            return Err("无法写入剪贴板".into());
        }
        std::ptr::copy_nonoverlapping(utf16.as_ptr(), ptr, utf16.len());
        win::GlobalUnlock(data);
        win::EmptyClipboard();
        let success = !win::SetClipboardData(13, data).is_null();
        if !success {
            win::GlobalFree(data);
        }
        let seq = win::GetClipboardSequenceNumber();
        win::CloseClipboard();
        if success {
            Ok(seq)
        } else {
            Err("剪贴板写入失败".into())
        }
    }
}
pub fn paste_text(target: isize, text: &str) -> Result<(), String> {
    if !target_valid(target) || !modifiers_released() {
        return Err("焦点已变化或快捷键尚未松开，未执行粘贴。".into());
    }
    #[cfg(windows)]
    {
        // Preserve text clipboard only. Non-text clipboard formats are not
        // overwritten: fall back to Unicode rather than destroy image/file data.
        let original = read_clipboard()?;
        if original.is_none() {
            return type_text(target, text);
        }
        let sequence = set_clipboard(text)?;
        if !target_valid(target) {
            if let Some(t) = original {
                let _ = set_clipboard(&t);
            }
            return Err("粘贴前焦点变化，已恢复剪贴板。".into());
        }
        win::send(&[
            win::key(0x11, false),
            win::key(0x56, false),
            win::key(0x56, true),
            win::key(0x11, true),
        ])?;
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_millis(700));
            if unsafe { win::GetClipboardSequenceNumber() } == sequence {
                if let Some(t) = original {
                    let _ = set_clipboard(&t);
                }
            }
        });
        Ok(())
    }
    #[cfg(not(windows))]
    {
        let _ = text;
        Err("全局输入目前仅支持 Windows".into())
    }
}
fn crypt(value: &[u8], decrypt: bool) -> Result<Vec<u8>, String> {
    #[cfg(windows)]
    {
        let input = win::DataBlob {
            len: value.len() as u32,
            data: value.as_ptr() as *mut u8,
        };
        let mut output = win::DataBlob {
            len: 0,
            data: std::ptr::null_mut(),
        };
        let ok = unsafe {
            if decrypt {
                win::CryptUnprotectData(
                    &input,
                    std::ptr::null_mut(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    1,
                    &mut output,
                )
            } else {
                win::CryptProtectData(
                    &input,
                    std::ptr::null(),
                    std::ptr::null(),
                    std::ptr::null_mut(),
                    std::ptr::null_mut(),
                    1,
                    &mut output,
                )
            }
        };
        if ok == 0 {
            return Err("Windows 密钥加密/解密失败，请在当前 Windows 账户重新填写密钥。".into());
        }
        let bytes =
            unsafe { std::slice::from_raw_parts(output.data, output.len as usize).to_vec() };
        unsafe { win::LocalFree(output.data as *mut _) };
        Ok(bytes)
    }
    #[cfg(not(windows))]
    {
        let _ = (value, decrypt);
        Err("密钥存储需要 Windows DPAPI".into())
    }
}
pub fn protect(value: &str) -> Result<String, String> {
    crypt(value.as_bytes(), false).map(|b| STANDARD.encode(b))
}
pub fn unprotect(value: &str) -> Result<String, String> {
    let bytes = STANDARD
        .decode(value)
        .map_err(|_| "密钥编码损坏".to_string())?;
    String::from_utf8(crypt(&bytes, true)?).map_err(|_| "密钥解码失败".into())
}
pub fn atomic_replace(from: &Path, to: &Path) -> Result<(), String> {
    #[cfg(windows)]
    {
        use std::os::windows::ffi::OsStrExt;
        let a: Vec<u16> = from.as_os_str().encode_wide().chain(Some(0)).collect();
        let b: Vec<u16> = to.as_os_str().encode_wide().chain(Some(0)).collect();
        if unsafe { win::MoveFileExW(a.as_ptr(), b.as_ptr(), 0x9) } == 0 {
            return Err(format!("保存配置失败，Windows error {}", unsafe {
                win::GetLastError()
            }));
        }
        Ok(())
    }
    #[cfg(not(windows))]
    {
        std::fs::rename(from, to).map_err(|e| e.to_string())
    }
}
fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Startup registration for the current user only: HKCU\...\Run\VoiceBar.
/// No elevation, no scheduled task, and nothing is written when the value
/// already matches, so toggling the option off restores the exact prior state.
pub fn autostart_path() -> Result<PathBuf, String> {
    std::env::current_exe().map_err(|e| format!("无法定位程序路径：{e}"))
}

pub fn autostart_command() -> Result<String, String> {
    let exe = autostart_path()?;
    let path = exe.to_string_lossy();
    if path.contains('"') {
        return Err("程序路径包含引号，无法写入开机自启".into());
    }
    // Quotes keep spaces safe; --hidden only shows the tray icon on logon.
    Ok(format!("\"{path}\" --hidden"))
}

fn autostart_enabled_with(command: &str) -> bool {
    #[cfg(windows)]
    {
        unsafe {
            let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
            let mut key: win::Hwnd = std::ptr::null_mut();
            let current_user = 0x8000_0001u32 as i32 as isize as win::Hwnd;
            if win::RegOpenKeyExW(current_user, sub.as_ptr(), 0, 0x20019, &mut key) != 0 {
                return false;
            }
            let name = wide("VoiceBar");
            let mut kind = 0u32;
            let mut size = 0u32;
            let query = win::RegQueryValueExW(
                key,
                name.as_ptr(),
                std::ptr::null_mut(),
                &mut kind,
                std::ptr::null_mut(),
                &mut size,
            );
            let mut buffer = vec![0u16; ((size as usize) / 2).max(1)];
            let ok = query == 0
                && win::RegQueryValueExW(
                    key,
                    name.as_ptr(),
                    std::ptr::null_mut(),
                    &mut kind,
                    buffer.as_mut_ptr() as *mut u8,
                    &mut size,
                ) == 0;
            win::RegCloseKey(key);
            if !ok || kind != 1 {
                return false;
            }
            let len = buffer.iter().position(|&c| c == 0).unwrap_or(buffer.len());
            let stored = String::from_utf16_lossy(&buffer[..len]);
            stored
                .trim_matches('"')
                .eq_ignore_ascii_case(command.trim_matches('"'))
        }
    }
    #[cfg(not(windows))]
    {
        let _ = command;
        false
    }
}

pub fn autostart_enabled() -> Result<bool, String> {
    autostart_command().map(|c| autostart_enabled_with(&c))
}

pub fn set_autostart(enabled: bool) -> Result<(), String> {
    let command = autostart_command()?;
    #[cfg(windows)]
    {
        unsafe {
            let sub = wide("Software\\Microsoft\\Windows\\CurrentVersion\\Run");
            let mut key: win::Hwnd = std::ptr::null_mut();
            let current_user = 0x8000_0001u32 as i32 as isize as win::Hwnd;
            if win::RegOpenKeyExW(current_user, sub.as_ptr(), 0, 0x20006, &mut key) != 0 {
                return Err("无法打开当前用户的开机自启注册表项".into());
            }
            let name = wide("VoiceBar");
            let code = if enabled {
                let data = wide(&command);
                win::RegSetValueExW(
                    key,
                    name.as_ptr(),
                    0,
                    1,
                    data.as_ptr() as *const u8,
                    (data.len() * 2) as u32,
                )
            } else {
                // 2 = ERROR_FILE_NOT_FOUND: already absent counts as success.
                let removed = win::RegDeleteValueW(key, name.as_ptr());
                if removed == 2 {
                    0
                } else {
                    removed
                }
            };
            win::RegCloseKey(key);
            if code != 0 {
                return Err(format!("写入开机自启失败（Windows error {code}）"));
            }
            if autostart_enabled_with(&command) != enabled {
                return Err("开机自启状态未能生效，请稍后重试".into());
            }
            Ok(())
        }
    }
    #[cfg(not(windows))]
    {
        let _ = (enabled, command);
        Err("开机自启目前仅支持 Windows".into())
    }
}

/// Groups all VoiceBar windows under one taskbar identity so the bundled exe
/// uses the application icon rather than a generic one in the taskbar.
pub fn set_app_id(app_id: &str) {
    #[cfg(windows)]
    {
        let id = wide(app_id);
        unsafe {
            win::SetCurrentProcessExplicitAppUserModelID(id.as_ptr());
        }
    }
    #[cfg(not(windows))]
    {
        let _ = app_id;
    }
}

pub fn working_set(pid: u32) -> u64 {
    #[cfg(windows)]
    {
        unsafe {
            let process = win::OpenProcess(0x410, 0, pid);
            if process.is_null() {
                return 0;
            }
            let mut c = win::MemCounters::default();
            c.cb = std::mem::size_of::<win::MemCounters>() as u32;
            let ok = win::GetProcessMemoryInfo(process, &mut c, c.cb);
            win::CloseHandle(process);
            if ok != 0 {
                c.working as u64
            } else {
                0
            }
        }
    }
    #[cfg(not(windows))]
    {
        let _ = pid;
        0
    }
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn input_target_is_copy_send_and_sync() {
        fn accepts<T: Copy + Send + Sync>() {}
        accepts::<InputTarget>();
    }

    #[test]
    fn realtime_text_rejects_ambiguous_deletion_before_touching_target() {
        for unsafe_text in [
            "😀", "e\u{301}", "\u{200d}", "\u{fe0f}", "\n", "\t", "क", "\u{3099}", "\u{302a}",
        ] {
            let target = InputTarget {
                window: 0,
                focus: 0,
                thread: 0,
                pid: 0,
                generation: 0,
                revision: 0,
                unavailable: Some("fixture must never inject"),
            };
            let error = realtime_edit(&target, "abc", unsafe_text, "unicode").unwrap_err();
            assert!(error.contains("暂不支持"), "{unsafe_text:?}: {error}");
            let error = realtime_edit(&target, unsafe_text, "abc", "clipboard").unwrap_err();
            let expected = if unsafe_text.chars().any(char::is_whitespace) {
                "空白"
            } else {
                "暂不支持"
            };
            assert!(error.contains(expected), "{unsafe_text:?}: {error}");
        }
        assert!("Hello, 世界！é русский 日本語 한글"
            .chars()
            .all(realtime_scalar));
    }

    #[test]
    fn realtime_backspace_never_retracts_smart_editor_whitespace() {
        let target = InputTarget {
            window: 0,
            focus: 0,
            thread: 0,
            pid: 0,
            generation: 0,
            revision: 0,
            unavailable: Some("fixture must never inject"),
        };
        for removed in [" ", "    ", "word word", "abc\t", "\u{3000}"] {
            let error = realtime_edit(&target, removed, "replacement", "unicode").unwrap_err();
            assert!(error.contains("空白"), "{removed:?}: {error}");
        }
    }

    /// Opt-in because this test briefly owns the foreground. It creates and
    /// destroys all its own windows and reserves an otherwise unused shortcut;
    /// no user document, clipboard contents or configuration is changed.
    #[cfg(windows)]
    #[test]
    #[ignore = "creates a foreground Win32 Edit fixture; run with --ignored --test-threads=1"]
    fn realtime_owned_edit_smoke() {
        use std::{ptr, sync::mpsc, time::Duration};
        const FOCUS_EDIT: u32 = 0x8000 + 0x56c;
        const HOTKEY: &str = "Ctrl+Alt+Shift+F24";
        #[link(name = "user32")]
        unsafe extern "system" {
            fn CreateWindowExW(
                ex_style: u32,
                class: *const u16,
                name: *const u16,
                style: u32,
                x: i32,
                y: i32,
                width: i32,
                height: i32,
                parent: win::Hwnd,
                menu: win::Hwnd,
                instance: win::Hwnd,
                parameter: *mut std::ffi::c_void,
            ) -> win::Hwnd;
            fn DestroyWindow(window: win::Hwnd) -> i32;
            fn SetForegroundWindow(window: win::Hwnd) -> i32;
            fn SetFocus(window: win::Hwnd) -> win::Hwnd;
            fn TranslateMessage(message: *const win::Message) -> i32;
            fn DispatchMessageW(message: *const win::Message) -> isize;
            fn SendMessageW(window: win::Hwnd, message: u32, wparam: usize, lparam: isize)
                -> isize;
            fn RegisterHotKey(window: win::Hwnd, id: i32, modifiers: u32, vk: u32) -> i32;
            fn UnregisterHotKey(window: win::Hwnd, id: i32) -> i32;
        }
        struct Fixture {
            window: isize,
            edit: isize,
            other: isize,
            thread: u32,
            join: Option<std::thread::JoinHandle<()>>,
            acknowledged: mpsc::Receiver<()>,
            previous: isize,
        }
        impl Fixture {
            fn new() -> Self {
                let previous = foreground();
                let (ready, receive) = mpsc::channel();
                let (acknowledged, acknowledgement) = mpsc::channel();
                let join = std::thread::spawn(move || unsafe {
                    let module = win::GetModuleHandleW(ptr::null());
                    let window = CreateWindowExW(
                        0,
                        wide("STATIC").as_ptr(),
                        wide("VoiceBar owned input test").as_ptr(),
                        0x10cf0000,
                        200,
                        200,
                        640,
                        250,
                        ptr::null_mut(),
                        ptr::null_mut(),
                        module,
                        ptr::null_mut(),
                    );
                    assert!(!window.is_null(), "could not create fixture window");
                    let make_edit = |y| {
                        CreateWindowExW(
                            0,
                            wide("EDIT").as_ptr(),
                            wide("").as_ptr(),
                            0x50800004,
                            16,
                            y,
                            580,
                            70,
                            window,
                            ptr::null_mut(),
                            module,
                            ptr::null_mut(),
                        )
                    };
                    let edit = make_edit(16);
                    let other = make_edit(102);
                    assert!(!edit.is_null() && !other.is_null());
                    // Only inject this chord if registration succeeds, so an
                    // existing user's shortcut can never be triggered by the test.
                    let reserved = RegisterHotKey(window, 0x56b, 0x4007, 0x87) != 0;
                    if !reserved {
                        DestroyWindow(window);
                        let _ = ready.send(Err("fixture shortcut is already registered"));
                        return;
                    }
                    SetForegroundWindow(window);
                    SetFocus(edit);
                    let _ = ready.send(Ok((
                        window as isize,
                        edit as isize,
                        other as isize,
                        win::GetCurrentThreadId(),
                    )));
                    let mut message: win::Message = std::mem::zeroed();
                    while win::GetMessageW(&mut message, ptr::null_mut(), 0, 0) > 0 {
                        if message.message == FOCUS_EDIT {
                            SetFocus(message.wparam as win::Hwnd);
                            let _ = acknowledged.send(());
                        } else {
                            TranslateMessage(&message);
                            DispatchMessageW(&message);
                        }
                    }
                    UnregisterHotKey(window, 0x56b);
                    DestroyWindow(window);
                });
                let (window, edit, other, thread) = receive
                    .recv_timeout(Duration::from_secs(3))
                    .expect("fixture did not start")
                    .expect("fixture shortcut unavailable");
                Self {
                    window,
                    edit,
                    other,
                    thread,
                    join: Some(join),
                    acknowledged: acknowledgement,
                    previous,
                }
            }
            fn text(&self) -> String {
                let mut buffer = [0u16; 256];
                let len = unsafe {
                    SendMessageW(
                        self.edit as win::Hwnd,
                        0x000d,
                        buffer.len(),
                        buffer.as_mut_ptr() as isize,
                    )
                };
                String::from_utf16_lossy(&buffer[..len as usize])
            }
            fn expect_text(&self, expected: &str) {
                for _ in 0..100 {
                    if self.text() == expected {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                assert_eq!(self.text(), expected);
            }
            fn focus(&self, window: isize) {
                assert_ne!(
                    unsafe { win::PostThreadMessageW(self.thread, FOCUS_EDIT, window as usize, 0) },
                    0
                );
                self.acknowledged
                    .recv_timeout(Duration::from_secs(2))
                    .unwrap();
            }
        }
        impl Drop for Fixture {
            fn drop(&mut self) {
                unsafe {
                    win::PostThreadMessageW(self.thread, 0x0012, 0, 0);
                }
                if let Some(join) = self.join.take() {
                    let _ = join.join();
                }
                if self.previous != 0 && unsafe { win::IsWindow(self.previous as win::Hwnd) } != 0 {
                    unsafe {
                        SetForegroundWindow(self.previous as win::Hwnd);
                    }
                }
            }
        }
        struct Session(InputTarget);
        impl Drop for Session {
            fn drop(&mut self) {
                self.0.release();
            }
        }
        fn physical_keys(keys: &[(u16, bool)]) {
            let inputs: Vec<_> = keys
                .iter()
                .map(|&(key, up)| {
                    let mut input = win::key(key, up);
                    // Unmarked SendInput is deliberately treated like user input.
                    input.data.keyboard.extra = 0;
                    input
                })
                .collect();
            win::send(&inputs).unwrap();
            std::thread::sleep(Duration::from_millis(50));
        }

        let fixture = Fixture::new();
        assert_eq!(
            foreground(),
            fixture.window,
            "fixture could not acquire foreground; no input sent"
        );
        assert!(
            modifiers_released(),
            "release physical modifiers before running this fixture"
        );
        // Always release injected modifier keys, including a panic between
        // the held start chord and its ordinary key-up sequence.
        struct ReleaseChord;
        impl Drop for ReleaseChord {
            fn drop(&mut self) {
                let _ = win::send(&[
                    win::key(0x11, true),
                    win::key(0x12, true),
                    win::key(0x10, true),
                    win::key(0x87, true),
                ]);
            }
        }
        let _release_chord = ReleaseChord;
        // Initial capture while the start chord is held, followed by modifier
        // releases before the final non-modifier release.
        physical_keys(&[(0x11, false), (0x12, false), (0x10, false), (0x87, false)]);
        let session = Session(InputTarget::capture(HOTKEY));
        let target = &session.0;
        assert_eq!(target.window(), fixture.window);
        assert_eq!(target.focus, fixture.edit);
        physical_keys(&[(0x11, true), (0x12, true), (0x10, true), (0x87, true)]);
        target
            .validate()
            .expect("start shortcut release invalidated capture");

        type_text(target.window(), "前文 ").unwrap();
        realtime_edit(target, "", "hel", "unicode").unwrap();
        fixture.expect_text("前文 hel");
        let clipboard_sequence = unsafe { win::GetClipboardSequenceNumber() };
        realtime_edit(target, "hel", "hello 世界", "clipboard").unwrap();
        fixture.expect_text("前文 hello 世界");
        assert_eq!(
            unsafe { win::GetClipboardSequenceNumber() },
            clipboard_sequence,
            "real-time clipboard mode must use Unicode without changing the clipboard"
        );
        assert!(realtime_edit(target, "😀", "oops", "unicode").is_err());
        fixture.expect_text("前文 hello 世界");

        physical_keys(&[
            (0x11, false),
            (0x12, false),
            (0x10, false),
            (0x87, false),
            (0x11, true),
            (0x12, true),
            (0x10, true),
            (0x87, true),
        ]);
        target
            .validate()
            .expect("stop shortcut invalidated delivery");
        realtime_edit(target, "世界", "完成", "unicode").unwrap();
        fixture.expect_text("前文 hello 完成");

        // Same control, changed caret: refuse before the first Backspace.
        physical_keys(&[(0x25, false), (0x25, true)]);
        assert!(!target.valid());
        assert!(realtime_edit(target, "完成", "must not appear", "unicode").is_err());
        fixture.expect_text("前文 hello 完成");

        let session = Session(InputTarget::capture(HOTKEY));
        session.0.validate().unwrap();
        fixture.focus(fixture.other);
        assert!(
            !session.0.valid(),
            "switching controls in one window must pause"
        );
        fixture.focus(fixture.edit);
        assert!(
            !session.0.valid(),
            "returning focus must not revive a paused target"
        );
        assert!(realtime_edit(&session.0, "完成", "must not appear", "unicode").is_err());
        fixture.expect_text("前文 hello 完成");
    }

    #[cfg(windows)]
    #[test]
    fn dpapi_roundtrip() {
        let s = "secret-中文";
        let p = protect(s).unwrap();
        assert_ne!(p, s);
        assert_eq!(unprotect(&p).unwrap(), s);
    }
    #[cfg(windows)]
    #[test]
    fn autostart_command_points_to_this_exe() {
        let command = autostart_command().unwrap();
        let exe = autostart_path().unwrap();
        assert!(command.ends_with(" --hidden"));
        assert_eq!(command, format!("\"{}\" --hidden", exe.to_string_lossy()));
        // Read-only: registry state is reported, not changed.
        let before = autostart_enabled().unwrap();
        assert_eq!(autostart_enabled().unwrap(), before);
    }
    #[cfg(windows)]
    #[test]
    fn native_input_layout_matches_windows() {
        assert_eq!(std::mem::size_of::<win::Input>(), 40);
    }
}
