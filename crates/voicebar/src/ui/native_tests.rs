//! Native render and message-loop checks. Uses only an isolated configuration.
use super::*;
use core::prelude::v1::test;
use std::{ffi::c_void, path::Path, sync::Mutex};

#[repr(C)]
#[derive(Default)]
struct Rect {
    left: i32,
    top: i32,
    right: i32,
    bottom: i32,
}
#[repr(C)]
struct BitmapInfo {
    size: u32,
    width: i32,
    height: i32,
    planes: u16,
    bits: u16,
    compression: u32,
    image_size: u32,
    x_resolution: i32,
    y_resolution: i32,
    colors_used: u32,
    colors_important: u32,
    color: u32,
}
#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowRect(hwnd: isize, rect: *mut Rect) -> i32;
    fn GetDC(hwnd: isize) -> isize;
    fn ReleaseDC(hwnd: isize, dc: isize) -> i32;
    fn GetDpiForWindow(hwnd: isize) -> u32;
    fn SetWindowPos(
        hwnd: isize,
        after: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        flags: u32,
    ) -> i32;
    fn FindWindowExW(parent: isize, after: isize, class: *const u16, title: *const u16) -> isize;
    fn GetWindowThreadProcessId(hwnd: isize, pid: *mut u32) -> u32;
}
#[link(name = "gdi32")]
unsafe extern "system" {
    fn CreateCompatibleDC(dc: isize) -> isize;
    fn CreateCompatibleBitmap(dc: isize, width: i32, height: i32) -> isize;
    fn SelectObject(dc: isize, object: isize) -> isize;
    fn BitBlt(
        dc: isize,
        x: i32,
        y: i32,
        width: i32,
        height: i32,
        source: isize,
        sx: i32,
        sy: i32,
        operation: u32,
    ) -> i32;
    fn GetDIBits(
        dc: isize,
        bitmap: isize,
        start: u32,
        lines: u32,
        pixels: *mut c_void,
        info: *mut BitmapInfo,
        usage: u32,
    ) -> i32;
    fn DeleteObject(object: isize) -> i32;
    fn DeleteDC(dc: isize) -> i32;
}

/// Capture only a known test-owned window's exact screen rectangle. PrintWindow
/// does not capture GPUI's GPU composition. No whole-desktop images are retained.
fn capture(hwnd: isize, name: &str) -> Result<(), String> {
    unsafe {
        let mut rect = Rect::default();
        if GetWindowRect(hwnd, &mut rect) == 0 {
            return Err("test window has no bounds".into());
        }
        let (width, height) = (rect.right - rect.left, rect.bottom - rect.top);
        if width <= 0 || height <= 0 {
            return Err("empty capture".into());
        }
        let screen = GetDC(0);
        let dc = CreateCompatibleDC(screen);
        let bitmap = CreateCompatibleBitmap(screen, width, height);
        if screen == 0 || dc == 0 || bitmap == 0 {
            return Err("capture allocation failed".into());
        }
        let old = SelectObject(dc, bitmap);
        let copied = BitBlt(
            dc, 0, 0, width, height, screen, rect.left, rect.top, 0x40cc0020,
        );
        SelectObject(dc, old);
        let mut pixels = vec![0u8; width as usize * height as usize * 4];
        let mut info = BitmapInfo {
            size: 40,
            width,
            height: -height,
            planes: 1,
            bits: 32,
            compression: 0,
            image_size: pixels.len() as u32,
            x_resolution: 0,
            y_resolution: 0,
            colors_used: 0,
            colors_important: 0,
            color: 0,
        };
        let read = GetDIBits(
            dc,
            bitmap,
            0,
            height as u32,
            pixels.as_mut_ptr().cast(),
            &mut info,
            0,
        );
        DeleteObject(bitmap);
        DeleteDC(dc);
        ReleaseDC(0, screen);
        if copied == 0 || read != height {
            return Err("capture did not complete".into());
        }
        let mut bytes = Vec::with_capacity(54 + pixels.len());
        bytes.extend_from_slice(b"BM");
        bytes.extend_from_slice(&((54 + pixels.len()) as u32).to_le_bytes());
        bytes.extend_from_slice(&[0; 4]);
        bytes.extend_from_slice(&54u32.to_le_bytes());
        bytes.extend_from_slice(&40u32.to_le_bytes());
        bytes.extend_from_slice(&width.to_le_bytes());
        bytes.extend_from_slice(&(-height).to_le_bytes());
        bytes.extend_from_slice(&1u16.to_le_bytes());
        bytes.extend_from_slice(&32u16.to_le_bytes());
        bytes.extend_from_slice(&[0; 24]);
        bytes.extend_from_slice(&pixels);
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.nexa/tmp/ui-refresh");
        std::fs::create_dir_all(&root).map_err(|e| e.to_string())?;
        std::fs::write(root.join(format!("{name}.bmp")), bytes).map_err(|e| e.to_string())?;
        eprintln!(
            "capture {name}: hwnd={hwnd} [{},{},{},{}]",
            rect.left, rect.top, rect.right, rect.bottom
        );
        Ok(())
    }
}
fn owned_menu() -> isize {
    let class: Vec<u16> = "#32768".encode_utf16().chain(Some(0)).collect();
    let mut after = 0;
    loop {
        after = unsafe { FindWindowExW(0, after, class.as_ptr(), std::ptr::null()) };
        if after == 0 {
            return 0;
        }
        let mut pid = 0;
        unsafe {
            GetWindowThreadProcessId(after, &mut pid);
        }
        if pid == std::process::id() {
            return after;
        }
    }
}

#[test]
#[ignore = "opens an isolated themed native tray menu, captures pixels and checks menu keyboard dispatch"]
fn themed_tray_menu_renders_and_keeps_native_keyboard_behavior() {
    use windows::Win32::{
        Foundation::{HWND, LPARAM, WPARAM},
        UI::WindowsAndMessaging::{PostMessageW, WM_KEYDOWN},
    };
    let mut config = AppConfig::default();
    config.hotkey = "Ctrl+Alt+Shift+F23".into();
    config.cancel_hotkey.clear();
    config.update_check = "off".into();
    // Even an unexpected menu command cannot start a microphone or connection.
    config.provider.kind = "openai-realtime".into();
    let runtime = Arc::new(Runtime::new(config, Default::default()));
    let desktop = Desktop::isolated_preview(runtime.clone()).unwrap();
    fn wait_for(mut condition: impl FnMut() -> bool) {
        for _ in 0..150 {
            if condition() {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        assert!(condition(), "native menu did not reach the expected state");
    }
    let key = |menu: isize, vk: usize| unsafe {
        PostMessageW(
            Some(HWND(menu as *mut _)),
            WM_KEYDOWN,
            WPARAM(vk),
            LPARAM(0),
        )
        .unwrap();
        std::thread::sleep(Duration::from_millis(150));
    };
    desktop.preview_context_menu(1180, 760);
    wait_for(|| desktop.preview_menu_open() && owned_menu() != 0);
    std::thread::sleep(Duration::from_millis(300));
    let menu = owned_menu();
    capture(menu, "tray-theme-idle").unwrap();
    // Native modal-menu keyboard handling: select first action, activate settings.
    key(menu, 0x28);
    capture(menu, "tray-theme-selected").unwrap();
    key(menu, 0x0d);
    wait_for(|| !desktop.preview_menu_open());
    assert!(
        runtime.settings_requested.swap(false, Ordering::AcqRel),
        "Enter did not invoke selected settings action"
    );
    assert!(runtime.active.lock().unwrap().is_none());

    // Native Down skips separators and the two disabled idle actions.
    desktop.preview_context_menu(1180, 760);
    wait_for(|| desktop.preview_menu_open() && owned_menu() != 0);
    let menu = owned_menu();
    key(menu, 0x24); // Home makes the initial selection independent of cursor position.
    for _ in 0..2 {
        key(menu, 0x28);
    }
    capture(menu, "tray-theme-disabled-skip").unwrap();
    key(menu, 0x0d);
    wait_for(|| !desktop.preview_menu_open());
    assert!(
        runtime.overlay_visible.load(Ordering::Acquire),
        "disabled actions were not skipped"
    );
    assert!(runtime.active.lock().unwrap().is_none());

    // Render the recording state using a synthetic session only.
    let (stop, _receiver) = tokio::sync::mpsc::unbounded_channel();
    *runtime.active.lock().unwrap() = Some(crate::runtime::Active { stop, id: 1 });
    runtime.set_transcript("菜单主题验证文字", "", false);
    desktop.preview_context_menu(1180, 760);
    wait_for(|| desktop.preview_menu_open() && owned_menu() != 0);
    std::thread::sleep(Duration::from_millis(300));
    let menu = owned_menu();
    capture(menu, "tray-theme-active").unwrap();
    key(menu, 0x1b);
    wait_for(|| !desktop.preview_menu_open());
    *runtime.active.lock().unwrap() = None;
    eprintln!("themed tray menu passed: idle/selected/active captures, native Down/Enter/Escape, disabled action skipping");
}

#[test]
#[ignore = "renders isolated settings, long-text overlays at 65/100/180%, and a real tray menu; no microphone or user configuration"]
fn hidden_overlay_renders_and_repositions() {
    let outcome = Arc::new(Mutex::new(None));
    let completed = outcome.clone();
    Application::new().run(move |cx| {
        init_theme(cx);
        let mut config = AppConfig::default();
        config.hotkey = "Ctrl+Alt+Shift+F24".into();
        config.cancel_hotkey.clear();
        config.update_check = "off".into();
        let runtime = Arc::new(Runtime::new(config, Default::default()));
        let desktop = Rc::new(Desktop::isolated_preview(runtime.clone()).unwrap());
        let slot = Rc::new(std::cell::RefCell::new(None));
        let view_slot = slot.clone();
        let state = runtime.clone();
        let system = desktop.clone();
        let settings = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1160.), px(840.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("VoiceBar · UI 验证预览".into()), appears_transparent: true, ..Default::default()
            }),
            window_min_size: Some(size(px(880.), px(650.))),
            focus: false, show: true, ..Default::default()
        }, move |window, cx| {
            let view = cx.new(|cx| Settings::new(state, system, window, cx));
            *view_slot.borrow_mut() = Some(view.clone());
            cx.new(|cx| Root::new(view, window, cx))
        }).unwrap();
        let view = slot.borrow_mut().take().unwrap();
        let settings_hwnd = settings.update(cx, |_, w, _| native_handle(w)).unwrap();
        let overlay = open_overlay(runtime.clone(), cx).unwrap();
        let hwnd = overlay.update(cx, |_, w, _| native_handle(w)).unwrap();
        cx.spawn(async move |cx| {
            let result = async {
                desktop::show_window(settings_hwnd, true);
                // Foreground activation can be denied while the user works in
                // another app. Raise this isolated fixture without taking focus,
                // so screen pixels actually belong to the verified test window.
                unsafe { SetWindowPos(settings_hwnd, -1, 0, 0, 0, 0, 0x0013); }
                for narrow in [false, true] {
                    if narrow {
                        let dpi = unsafe { GetDpiForWindow(settings_hwnd) } as f64 / 96.;
                        unsafe { SetWindowPos(settings_hwnd, 0, 0, 0, (880. * dpi) as i32, (650. * dpi) as i32, 0x0016); }
                    }
                    for tab in 0..4 {
                        view.update(cx, |v, cx| {
                            v.tab = tab;
                            v.demo_until = Some(Instant::now() + Duration::from_secs(10));
                            cx.notify();
                        }).unwrap();
                        for _ in 0..45 {
                            cx.background_executor().timer(Duration::from_millis(16)).await;
                            view.update(cx, |v, cx| v.tick(cx)).unwrap();
                            settings.update(cx, |_, window, _| window.refresh()).unwrap();
                        }
                        // Normal GPUI windows use a no-redirection swap chain;
                        // GDI screen copies can omit them. Use Computer Use's
                        // verified window capture for settings visual inspection.
                        eprintln!("settings rendered: tab={tab}, narrow={narrow}, hwnd={settings_hwnd}");
                        if !narrow && tab == 0 {
                            let probe = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../.nexa/tmp/ui-refresh/inspect.json");
                            if probe.exists() {
                                eprintln!("settings visual inspection ready: hwnd={settings_hwnd}");
                                let deadline = Instant::now() + Duration::from_secs(70);
                                let mut selection = (0usize, false);
                                while Instant::now() < deadline {
                                    let request = std::fs::read_to_string(&probe).ok()
                                        .and_then(|s| serde_json::from_str::<Value>(&s).ok()).unwrap_or(Value::Null);
                                    if request["done"].as_bool() == Some(true) { break; }
                                    let requested = (request["tab"].as_u64().unwrap_or(0).min(3) as usize,
                                        request["narrow"].as_bool().unwrap_or(false));
                                    if requested != selection {
                                        selection = requested;
                                        view.update(cx, |v, cx| {
                                            v.tab = selection.0;
                                            v.demo_until = Some(Instant::now() + Duration::from_secs(10));
                                            cx.notify();
                                        }).unwrap();
                                        let dpi = unsafe { GetDpiForWindow(settings_hwnd) } as f64 / 96.;
                                        let (w, h) = if selection.1 { (880., 650.) } else { (1160., 840.) };
                                        unsafe { SetWindowPos(settings_hwnd, 0, 0, 0, (w*dpi) as i32, (h*dpi) as i32, 0x0016); }
                                    }
                                    cx.background_executor().timer(Duration::from_millis(30)).await;
                                    view.update(cx, |v, cx| v.tick(cx)).unwrap();
                                    settings.update(cx, |_, w, _| w.refresh()).unwrap();
                                }
                            }
                        }
                    }
                }
                desktop::hide_window(settings_hwnd);
                runtime.set_transcript("把想说的话，变成眼前的文字。这个长文本用于验证浮框在不同缩放比例下仍然可以完整滚动阅读。", "继续表达，让思绪自然流动。", false);
                runtime.set_status("listening", "输入位置已变化或检测到手动编辑，自动写入暂停；识别继续，可复制结果。".repeat(4));
                let (stop, _receiver) = tokio::sync::mpsc::unbounded_channel();
                *runtime.active.lock().unwrap() = Some(crate::runtime::Active { stop, id: 1 });
                for scale in [0.65, 1.0, 1.8] {
                    for anchor in ["top", "bottom-right"] {
                        let mut config = runtime.config.lock().unwrap().clone();
                        config.appearance.scale = scale;
                        config.appearance.anchor = anchor.into();
                        *runtime.config.lock().unwrap() = config.clone();
                        overlay.update(cx, |v, _, _| v.layout.lock().unwrap().clear()).unwrap();
                        desktop::configure_overlay(hwnd, &config)?;
                        desktop::show_window(hwnd, false);
                        overlay.update(cx, |_, w, _| w.refresh()).unwrap();
                        for _ in 0..40 {
                            {
                                let mut level = runtime.level.lock().unwrap();
                                level.updated_at = Some(Instant::now());
                                level.audio = AudioFrame { loudness: 0.6, speaking: true, bands: vec![0.6; BAND_COUNT], ..Default::default() };
                            }
                            cx.background_executor().timer(Duration::from_millis(16)).await;
                            overlay.update(cx, |v, _, cx| v.tick(cx)).unwrap();
                        }
                        let (viewport, layout) = overlay.update(cx, |v, w, _| (w.viewport_size(), v.layout.lock().unwrap().clone())).unwrap();
                        if !desktop::is_visible(hwnd) || layout.len() != 3 { return Err(format!("overlay was not painted: {layout:?}")); }
                        let w = f32::from(viewport.width);
                        let h = f32::from(viewport.height);
                        let margin = 10. * (w / 560.).min(h / 210.);
                        for (region, children) in &layout {
                            for b in children {
                                let (x, y, bw, bh) = (f32::from(b.origin.x), f32::from(b.origin.y), f32::from(b.size.width), f32::from(b.size.height));
                                if x < margin - 1. || y < margin - 1. || x + bw > w - margin + 1. || y + bh > h - margin + 1. || bw <= 0. || bh <= 0. {
                                    return Err(format!("overflow {region}: {b:?}, viewport={viewport:?}, scale={scale}"));
                                }
                            }
                        }
                        eprintln!("layout passed: scale={scale}, anchor={anchor}, viewport={viewport:?}");
                        capture(hwnd, &format!("overlay-{anchor}-{scale}"))?;
                    }
                }
                *runtime.active.lock().unwrap() = None;
                desktop::hide_window(hwnd);
                desktop.preview_context_menu(1180, 760);
                for _ in 0..110 { cx.background_executor().timer(Duration::from_millis(16)).await; }
                if !desktop.preview_menu_open() { return Err("tray menu dismissed before 1.7 seconds".into()); }
                let menu = owned_menu();
                if menu == 0 { return Err("no test-owned menu window".into()); }
                capture(menu, "tray-menu")?;
                desktop.dismiss_preview_menu();
                for _ in 0..20 { cx.background_executor().timer(Duration::from_millis(16)).await; }
                if desktop.preview_menu_open() { return Err("tray menu did not dismiss".into()); }
                eprintln!("native menu passed: sent v4 callback, idle heartbeats, dismissal");
                Ok::<(), String>(())
            }.await;
            desktop.dismiss_preview_menu();
            *completed.lock().unwrap() = Some(result);
            let _ = cx.update(|cx| cx.quit());
        }).detach();
    });
    let result = outcome
        .lock()
        .unwrap()
        .take()
        .expect("native verification did not complete");
    assert!(result.is_ok(), "{}", result.unwrap_err());
}
