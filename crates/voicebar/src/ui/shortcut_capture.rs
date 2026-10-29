use std::{marker::PhantomData, rc::Rc};

#[derive(Clone, Debug, PartialEq, Eq)]
pub(super) enum CaptureUpdate {
    Pending(String),
    Complete(Result<String, String>),
    Canceled(String),
}

/// Owns a low-level hook installed on the calling UI thread.
pub(super) struct Capture {
    #[cfg(windows)]
    hook: windows::Win32::UI::WindowsAndMessaging::HHOOK,
    #[cfg(windows)]
    token: Rc<()>,
    _thread: PhantomData<Rc<()>>,
}

impl Capture {
    pub(super) fn start(hwnd: isize) -> Result<Self, String> {
        #[cfg(windows)]
        {
            native::start(hwnd)
        }
        #[cfg(not(windows))]
        {
            let _ = hwnd;
            Err("当前平台不支持原生快捷键录入".into())
        }
    }

    pub(super) fn poll(&self) -> CaptureUpdate {
        #[cfg(windows)]
        {
            native::poll(self)
        }
        #[cfg(not(windows))]
        {
            CaptureUpdate::Canceled("当前平台不支持原生快捷键录入".into())
        }
    }
}

#[cfg(windows)]
impl Drop for Capture {
    fn drop(&mut self) {
        native::release(self);
    }
}

#[cfg(any(windows, test))]
mod model {
    use super::CaptureUpdate;
    use std::time::Duration;

    pub(super) fn cancellation_reason(
        elapsed: Duration,
        visible: bool,
        foreground: bool,
    ) -> Option<&'static str> {
        if elapsed >= Duration::from_secs(15) {
            Some("按键录入已超时，请重新点击录入")
        } else if !visible {
            Some("设置窗口已隐藏或关闭，已取消按键录入")
        } else if !foreground {
            Some("设置窗口已失去焦点，已取消按键录入")
        } else {
            None
        }
    }

    pub(super) fn keyboard_vk(vk: u32) -> bool {
        // VK_CANCEL is Ctrl+Break, not a mouse button.
        vk == 0x03 || (0x08..=0xfe).contains(&vk)
    }

    pub(super) fn physical_vk(vk: u32, scan: u32, extended: bool) -> Option<usize> {
        if !keyboard_vk(vk) {
            return None;
        }
        Some(match vk {
            0x10 => {
                if scan & 0xff == 0x36 {
                    0xa1 // Right Shift.
                } else {
                    0xa0
                }
            }
            0x11 => {
                if extended {
                    0xa3 // Right Ctrl.
                } else {
                    0xa2
                }
            }
            0x12 => {
                if extended {
                    0xa5 // Right Alt.
                } else {
                    0xa4
                }
            }
            _ => vk as usize,
        })
    }

    fn modifier(vk: usize) -> bool {
        matches!(vk, 0xa0..=0xa5 | 0x5b | 0x5c)
    }

    pub(super) fn key_name(vk: usize) -> Option<String> {
        if (0x30..=0x39).contains(&vk) || (0x41..=0x5a).contains(&vk) {
            return char::from_u32(vk as u32).map(|key| key.to_string());
        }
        if (0x70..=0x87).contains(&vk) {
            return Some(format!("F{}", vk - 0x70 + 1));
        }
        if (0x60..=0x69).contains(&vk) {
            return Some(format!("Numpad{}", vk - 0x60));
        }
        Some(
            match vk {
                0x08 => "Backspace",
                0x09 => "Tab",
                // Windows RegisterHotKey cannot distinguish NumpadEnter.
                0x0d => "Enter",
                0x13 => "Pause",
                0x14 => "CapsLock",
                0x1b => "Escape",
                0x20 => "Space",
                0x21 => "PageUp",
                0x22 => "PageDown",
                0x23 => "End",
                0x24 => "Home",
                0x25 => "ArrowLeft",
                0x26 => "ArrowUp",
                0x27 => "ArrowRight",
                0x28 => "ArrowDown",
                0x2c => "PrintScreen",
                0x2d => "Insert",
                0x2e => "Delete",
                0x6a => "NumpadMultiply",
                0x6b => "NumpadAdd",
                0x6d => "NumpadSubtract",
                0x6e => "NumpadDecimal",
                0x6f => "NumpadDivide",
                0x90 => "NumLock",
                0x91 => "ScrollLock",
                0xba => "Semicolon",
                0xbb => "Equal",
                0xbc => "Comma",
                0xbd => "Minus",
                0xbe => "Period",
                0xbf => "Slash",
                0xc0 => "Backquote",
                0xdb => "BracketLeft",
                0xdc => "Backslash",
                0xdd => "BracketRight",
                0xde => "Quote",
                _ => return None,
            }
            .into(),
        )
    }

    pub(super) struct Machine {
        held: [bool; 256],
        preexisting: [bool; 256],
        candidate: Option<Result<String, String>>,
        terminal: Option<CaptureUpdate>,
    }

    impl Machine {
        pub(super) fn new(mut held: [bool; 256]) -> Self {
            for (vk, down) in held.iter_mut().enumerate() {
                // Generic modifier VKs alias their physical left/right keys.
                if !keyboard_vk(vk as u32) || matches!(vk, 0x10..=0x12) {
                    *down = false;
                }
            }
            Self {
                held,
                preexisting: held,
                candidate: None,
                terminal: None,
            }
        }

        fn modifiers(&self) -> String {
            [
                (0xa2, 0xa3, "Ctrl"),
                (0xa4, 0xa5, "Alt"),
                (0xa0, 0xa1, "Shift"),
                (0x5b, 0x5c, "Super"),
            ]
            .into_iter()
            .filter_map(|(left, right, name)| (self.held[left] || self.held[right]).then_some(name))
            .collect::<Vec<_>>()
            .join("+")
        }

        pub(super) fn poll(&self) -> CaptureUpdate {
            if let Some(update) = &self.terminal {
                return update.clone();
            }
            CaptureUpdate::Pending(match &self.candidate {
                Some(Ok(chord)) => chord.clone(),
                Some(Err(reason)) => reason.clone(),
                None => self.modifiers(),
            })
        }

        pub(super) fn cancel(&mut self, reason: impl Into<String>) {
            if !matches!(self.terminal, Some(CaptureUpdate::Canceled(_))) {
                self.terminal = Some(CaptureUpdate::Canceled(reason.into()));
            }
        }

        /// Returns whether the hook should consume this event.
        pub(super) fn key(&mut self, vk: u32, scan: u32, extended: bool, down: bool) -> bool {
            if self.terminal.is_some() {
                return false;
            }
            let Some(key) = physical_vk(vk, scan, extended) else {
                return true;
            };

            // A key pressed before installation already reached Windows.
            // Let its release through to avoid leaving OS modifier state stuck.
            let pass_preexisting_release = !down && self.preexisting[key];

            if down {
                if self.held[key] {
                    return true; // Repeated keydown cannot replace a candidate.
                }
                self.held[key] = true;
                if !modifier(key) && self.candidate.is_none() {
                    let modifiers = self.modifiers();
                    self.candidate = Some(
                        key_name(key)
                            .map(|name| {
                                if modifiers.is_empty() {
                                    name
                                } else {
                                    format!("{modifiers}+{name}")
                                }
                            })
                            .ok_or_else(|| {
                                format!("不支持此按键（VK 0x{vk:02X}），请使用其他组合键")
                            }),
                    );
                }
            } else {
                self.held[key] = false;
                self.preexisting[key] = false;
            }

            // Also wait for incidental/preexisting keys: dropping the hook
            // while they remain down would expose their subsequent repeats.
            if !self.held.iter().any(|down| *down) {
                if let Some(result) = self.candidate.take() {
                    self.terminal = Some(CaptureUpdate::Complete(result));
                }
            }
            !pass_preexisting_release
        }
    }
}

#[cfg(windows)]
mod native {
    use super::{
        model::{cancellation_reason, keyboard_vk, Machine},
        Capture, CaptureUpdate,
    };
    use std::{cell::RefCell, marker::PhantomData, panic::catch_unwind, rc::Rc, time::Instant};
    use windows::Win32::{
        Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
        System::{LibraryLoader::GetModuleHandleW, Threading::GetCurrentThreadId},
        UI::{
            Input::KeyboardAndMouse::GetAsyncKeyState,
            WindowsAndMessaging::{
                CallNextHookEx, GetForegroundWindow, GetWindowThreadProcessId, IsWindow,
                IsWindowVisible, SetWindowsHookExW, UnhookWindowsHookEx, KBDLLHOOKSTRUCT,
                WH_KEYBOARD_LL, WM_KEYDOWN, WM_KEYUP, WM_SYSKEYDOWN, WM_SYSKEYUP,
            },
        },
    };

    struct Session {
        token: Rc<()>,
        hwnd: isize,
        started: Instant,
        machine: Machine,
    }

    struct Snapshot {
        token: Rc<()>,
        hwnd: isize,
        started: Instant,
    }

    thread_local! {
        static SESSION: RefCell<Option<Session>> = const { RefCell::new(None) };
    }

    fn snapshot() -> Option<Snapshot> {
        SESSION
            .try_with(|cell| {
                let slot = cell.try_borrow().ok()?;
                let session = slot.as_ref()?;
                Some(Snapshot {
                    token: session.token.clone(),
                    hwnd: session.hwnd,
                    started: session.started,
                })
            })
            .ok()
            .flatten()
    }

    // Callers must keep f free of Win32 calls and other reentrant operations.
    fn access<T>(token: &Rc<()>, f: impl FnOnce(&mut Machine) -> T) -> Option<T> {
        SESSION
            .try_with(|cell| {
                let mut slot = cell.try_borrow_mut().ok()?;
                let session = slot.as_mut()?;
                if !Rc::ptr_eq(token, &session.token) {
                    return None;
                }
                Some(f(&mut session.machine))
            })
            .ok()
            .flatten()
    }

    fn clear(token: &Rc<()>) {
        let removed = SESSION.try_with(|cell| {
            let mut slot = cell.try_borrow_mut().ok()?;
            if slot
                .as_ref()
                .is_some_and(|session| Rc::ptr_eq(token, &session.token))
            {
                slot.take()
            } else {
                None
            }
        });
        drop(removed);
    }

    fn invalid_reason(state: &Snapshot) -> Option<&'static str> {
        // Never retain a RefCell borrow while querying Windows.
        let hwnd = HWND(state.hwnd as *mut _);
        let (visible, foreground) = unsafe {
            (
                IsWindow(Some(hwnd)).as_bool() && IsWindowVisible(hwnd).as_bool(),
                GetForegroundWindow() == hwnd,
            )
        };
        cancellation_reason(state.started.elapsed(), visible, foreground)
    }

    pub(super) fn start(hwnd: isize) -> Result<Capture, String> {
        let window = HWND(hwnd as *mut _);
        if hwnd == 0 {
            return Err("设置窗口尚未准备好，请稍后重试".into());
        }
        unsafe {
            if GetWindowThreadProcessId(window, None) != GetCurrentThreadId() {
                return Err("必须在设置窗口所属的界面线程中开始按键录入".into());
            }
        }

        let started = Instant::now();
        let token = Rc::new(());
        let scope = Snapshot {
            token: token.clone(),
            hwnd,
            started,
        };
        if let Some(reason) = invalid_reason(&scope) {
            return Err(reason.into());
        }

        let module = unsafe { GetModuleHandleW(None) }
            .map_err(|error| format!("无法获取按键录入模块：{error}"))?;
        let mut held = [false; 256];
        for (vk, down) in held.iter_mut().enumerate() {
            if keyboard_vk(vk as u32) && !matches!(vk, 0x10..=0x12) {
                *down = unsafe { GetAsyncKeyState(vk as i32) < 0 };
            }
        }

        SESSION
            .try_with(|cell| {
                let mut slot = cell
                    .try_borrow_mut()
                    .map_err(|_| "按键录入正忙，请稍后重试".to_string())?;
                if slot.is_some() {
                    return Err("已有按键录入正在进行，请先结束该次录入".into());
                }
                *slot = Some(Session {
                    token: token.clone(),
                    hwnd,
                    started,
                    machine: Machine::new(held),
                });
                Ok::<(), String>(())
            })
            .map_err(|_| "界面线程已退出，无法开始按键录入".to_string())??;

        let hook = match unsafe {
            SetWindowsHookExW(
                WH_KEYBOARD_LL,
                Some(keyboard_hook),
                Some(HINSTANCE(module.0)),
                0,
            )
        } {
            Ok(hook) => hook,
            Err(error) => {
                clear(&token);
                return Err(format!("无法开始原生按键录入：{error}"));
            }
        };

        let capture = Capture {
            hook,
            token,
            _thread: PhantomData,
        };
        // Installation itself can dispatch messages; validate again afterward.
        if let CaptureUpdate::Canceled(reason) = poll(&capture) {
            return Err(reason);
        }
        Ok(capture)
    }

    pub(super) fn poll(capture: &Capture) -> CaptureUpdate {
        let Some(state) = snapshot() else {
            return CaptureUpdate::Canceled("按键录入已结束".into());
        };
        if !Rc::ptr_eq(&capture.token, &state.token) {
            return CaptureUpdate::Canceled("按键录入已结束".into());
        }

        let reason = invalid_reason(&state);
        access(&capture.token, |machine| {
            if let Some(reason) = reason {
                machine.cancel(reason);
            }
            machine.poll()
        })
        .unwrap_or_else(|| CaptureUpdate::Canceled("按键录入状态已失效".into()))
    }

    pub(super) fn release(capture: &Capture) {
        // Clear before Unhook, which may reenter the message loop.
        clear(&capture.token);
        unsafe {
            let _ = UnhookWindowsHookEx(capture.hook);
        }
    }

    fn on_key(vk: u32, scan: u32, extended: bool, down: bool) -> bool {
        let Some(state) = snapshot() else {
            return false;
        };
        let reason = invalid_reason(&state);
        access(&state.token, |machine| {
            if let Some(reason) = reason {
                machine.cancel(reason);
                false
            } else {
                machine.key(vk, scan, extended, down)
            }
        })
        .unwrap_or(false)
    }

    unsafe extern "system" fn keyboard_hook(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
        let consumed = if code >= 0
            && lparam.0 != 0
            && matches!(
                wparam.0 as u32,
                WM_KEYDOWN | WM_KEYUP | WM_SYSKEYDOWN | WM_SYSKEYUP
            ) {
            match catch_unwind(|| {
                let event = unsafe { &*(lparam.0 as *const KBDLLHOOKSTRUCT) };
                // Deliberately accept injected input, including SendInput tests.
                on_key(
                    event.vkCode,
                    event.scanCode,
                    event.flags.0 & 0x01 != 0,
                    matches!(wparam.0 as u32, WM_KEYDOWN | WM_SYSKEYDOWN),
                )
            }) {
                Ok(consumed) => consumed,
                Err(_) => {
                    if let Some(state) = snapshot() {
                        let _ = access(&state.token, |machine| {
                            machine.cancel("按键录入异常，已取消录入");
                        });
                    }
                    false
                }
            }
        } else {
            false
        };

        if consumed {
            LRESULT(1)
        } else {
            // No TLS borrow survives into another hook.
            unsafe { CallNextHookEx(None, code, wparam, lparam) }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        model::{cancellation_reason, key_name, physical_vk, Machine},
        CaptureUpdate,
    };
    use std::time::Duration;

    fn key(machine: &mut Machine, vk: u32, down: bool) -> bool {
        machine.key(vk, 0, false, down)
    }

    fn pending(text: &str) -> CaptureUpdate {
        CaptureUpdate::Pending(text.into())
    }

    fn complete(text: &str) -> CaptureUpdate {
        CaptureUpdate::Complete(Ok(text.into()))
    }

    #[test]
    fn left_and_right_modifiers_share_names_without_finishing() {
        assert_eq!(physical_vk(0x10, 0x36, false), Some(0xa1));
        assert_eq!(physical_vk(0x10, 0x2a, false), Some(0xa0));
        assert_eq!(physical_vk(0x11, 0, true), Some(0xa3));
        assert_eq!(physical_vk(0x12, 0, true), Some(0xa5));
        assert_eq!(physical_vk(0x11, 0, false), Some(0xa2));
        assert_eq!(physical_vk(0x12, 0, false), Some(0xa4));

        let mut machine = Machine::new([false; 256]);
        for vk in [0xa2, 0xa3, 0xa4, 0xa5, 0xa0, 0xa1, 0x5b, 0x5c] {
            key(&mut machine, vk, true);
        }
        assert_eq!(machine.poll(), pending("Ctrl+Alt+Shift+Super"));
        for vk in [0xa2, 0xa4, 0xa0, 0x5b] {
            key(&mut machine, vk, false);
        }
        assert_eq!(machine.poll(), pending("Ctrl+Alt+Shift+Super"));
        for vk in [0xa3, 0xa5, 0xa1, 0x5c] {
            key(&mut machine, vk, false);
        }
        assert_eq!(machine.poll(), pending(""));
    }

    #[test]
    fn repeats_cannot_replace_candidate_and_every_key_must_release() {
        let mut machine = Machine::new([false; 256]);
        for vk in [0xa2, 0xa0, 0x41, 0x41, 0x42] {
            assert!(key(&mut machine, vk, true));
        }
        assert_eq!(machine.poll(), pending("Ctrl+Shift+A"));
        for vk in [0xa2, 0x41, 0x42] {
            assert!(key(&mut machine, vk, false));
            assert_eq!(machine.poll(), pending("Ctrl+Shift+A"));
        }
        assert!(key(&mut machine, 0xa0, false));
        assert_eq!(machine.poll(), complete("Ctrl+Shift+A"));
        assert!(!key(&mut machine, 0x43, true));
    }

    #[test]
    fn preexisting_keys_ignore_repeat_and_mouse_state_cannot_block_completion() {
        let mut held = [false; 256];
        held[0xa2] = true;
        held[0x41] = true;
        held[0x01] = true; // Mouse button must be discarded.
        let mut machine = Machine::new(held);
        assert!(key(&mut machine, 0x41, true));
        assert_eq!(machine.poll(), pending("Ctrl"));
        assert!(!key(&mut machine, 0x41, false));
        assert!(key(&mut machine, 0x87, true));
        assert!(key(&mut machine, 0x87, false));
        assert_eq!(machine.poll(), pending("Ctrl+F24"));
        assert!(!key(&mut machine, 0xa2, false));
        assert_eq!(machine.poll(), complete("Ctrl+F24"));
    }

    #[test]
    fn unsupported_key_reports_error_after_releasing_its_chord() {
        let mut machine = Machine::new([false; 256]);
        key(&mut machine, 0xa2, true);
        key(&mut machine, 0xa6, true); // BrowserBack is not a supported shortcut.
        key(&mut machine, 0xa2, false);
        assert!(matches!(machine.poll(), CaptureUpdate::Pending(_)));
        key(&mut machine, 0xa6, false);
        assert!(matches!(machine.poll(), CaptureUpdate::Complete(Err(_))));
        assert_eq!(physical_vk(0xffff, 0, false), None);
    }

    #[test]
    fn escape_is_recorded_and_never_means_cancel() {
        let mut machine = Machine::new([false; 256]);
        key(&mut machine, 0x1b, true);
        assert_eq!(machine.poll(), pending("Escape"));
        key(&mut machine, 0x1b, false);
        assert_eq!(machine.poll(), complete("Escape"));
        assert_eq!(key_name(0x0d).as_deref(), Some("Enter"));
    }

    #[test]
    fn invalid_scope_cancellation_is_permanent() {
        assert_eq!(
            cancellation_reason(Duration::from_millis(14_999), true, true),
            None
        );
        for (elapsed, visible, foreground) in [
            (Duration::from_secs(15), true, true),
            (Duration::ZERO, false, true),
            (Duration::ZERO, true, false),
        ] {
            let mut machine = Machine::new([false; 256]);
            key(&mut machine, 0x41, true);
            let reason = cancellation_reason(elapsed, visible, foreground)
                .expect("this scope must be invalid");
            machine.cancel(reason);
            assert!(!key(&mut machine, 0x41, false));
            assert!(!key(&mut machine, 0x42, true));
            machine.cancel("后来的原因不能覆盖首次取消");
            assert_eq!(machine.poll(), CaptureUpdate::Canceled(reason.into()));
        }
    }

    #[test]
    fn emitted_key_names_are_accepted_by_the_configuration_parser() {
        for vk in 0..256 {
            if let Some(name) = key_name(vk) {
                let chord = format!("Ctrl+Alt+Shift+Super+{name}");
                assert!(
                    chord.parse::<global_hotkey::hotkey::HotKey>().is_ok(),
                    "unsupported emitted shortcut: {chord}"
                );
            }
        }
    }
}
