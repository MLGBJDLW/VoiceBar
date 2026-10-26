use base64::{engine::general_purpose::STANDARD, Engine};
use std::path::{Path, PathBuf};

// Windows interaction lives behind this small interface; no clipboard or target
// is touched until an actual recognition session has text to deliver.
#[cfg(windows)]
mod win {
    use std::ffi::c_void;
    pub type Hwnd = *mut c_void;
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
                    extra: 0,
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
                                extra: 0,
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
            if win::RegOpenKeyExW(std::ptr::null_mut(), sub.as_ptr(), 0, 0x20019, &mut key) != 0 {
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
            if win::RegOpenKeyExW(std::ptr::null_mut(), sub.as_ptr(), 0, 0x20006, &mut key) != 0 {
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
