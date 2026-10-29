//! Runs only in an explicitly requested native test session, with a separate
//! config and a provider that cannot record without a key.
use super::shortcuts::ShortcutField;
use super::*;
use core::prelude::v1::test;
use std::{mem::size_of, path::PathBuf, sync::Mutex};
use windows::Win32::UI::{
    Input::KeyboardAndMouse::{
        RegisterHotKey, SendInput, UnregisterHotKey, INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT,
        KEYEVENTF_KEYUP, MOD_ALT, MOD_CONTROL, MOD_SHIFT, VIRTUAL_KEY,
    },
    WindowsAndMessaging::GetForegroundWindow,
};

fn send_keys(hwnd: isize, keys: &[(u16, bool)]) -> Result<(), String> {
    if unsafe { GetForegroundWindow() }.0 as isize != hwnd {
        return Err("owned shortcut test window lost foreground; no keys sent".into());
    }
    let inputs: Vec<_> = keys
        .iter()
        .map(|&(key, up)| INPUT {
            r#type: INPUT_KEYBOARD,
            Anonymous: INPUT_0 {
                ki: KEYBDINPUT {
                    wVk: VIRTUAL_KEY(key),
                    dwFlags: if up {
                        KEYEVENTF_KEYUP
                    } else {
                        Default::default()
                    },
                    ..Default::default()
                },
            },
        })
        .collect();
    if unsafe { SendInput(&inputs, size_of::<INPUT>() as i32) } != inputs.len() as u32 {
        return Err("could not send shortcut fixture input".into());
    }
    Ok(())
}

#[test]
fn shortcut_save_transaction_preserves_live_bindings_on_conflict() {
    let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../.nexa/tmp/shortcut-capture")
        .join(uuid::Uuid::new_v4().to_string());
    let path = root.join("settings.json");
    let mut config = AppConfig::default();
    config.hotkey = "Ctrl+Alt+Shift+F20".into();
    config.cancel_hotkey = "Ctrl+Alt+Shift+F19".into();
    config.update_check = "off".into();
    let runtime = Arc::new(Runtime::new(config.clone(), path.clone()));
    let desktop = Desktop::isolated_preview(runtime.clone()).unwrap();
    config.hotkey = "Ctrl+Alt+Shift+F18".into();
    config.cancel_hotkey.clear();
    desktop.save_config(config.clone()).unwrap();
    assert_eq!(crate::config::load(&path).unwrap(), config);

    let mut occupied = config.clone();
    occupied.hotkey = "Ctrl+Alt+Shift+F17".into();
    let other = Arc::new(Runtime::new(occupied.clone(), root.join("other.json")));
    let reservation = Desktop::isolated_preview(other).unwrap();
    let error = desktop.save_config(occupied).unwrap_err();
    assert!(error.contains("占用"), "{error}");
    assert_eq!(*runtime.config.lock().unwrap(), config);
    assert_eq!(crate::config::load(&path).unwrap(), config);

    // A new owner cannot take the old chord after the failed save.
    let probe = Arc::new(Runtime::new(config.clone(), root.join("probe.json")));
    assert!(Desktop::isolated_preview(probe).is_err());
    drop(reservation);
    drop(desktop);
    // Releasing the original owner makes the same chord available.
    let probe = Arc::new(Runtime::new(config, root.join("probe.json")));
    drop(Desktop::isolated_preview(probe).unwrap());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
#[ignore = "foreground owned GPUI fixture with keyboard capture, global shortcut and isolated persistence checks"]
fn records_keys_without_triggering_registered_actions() {
    let outcome = Arc::new(Mutex::new(None));
    let completed = outcome.clone();
    Application::new().run(move |cx| {
        init_theme(cx);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../.nexa/tmp/shortcut-capture");
        let path = root.join(format!("native-{}.json", uuid::Uuid::new_v4()));
        let mut config = AppConfig::default();
        config.hotkey = "Ctrl+Alt+Shift+F24".into();
        config.cancel_hotkey.clear();
        config.provider.kind = "openai-realtime".into();
        config.provider.endpoint = "wss://api.openai.com/v1/realtime?intent=transcription".into();
        config.update_check = "off".into();
        let runtime = Arc::new(Runtime::new(config, path.clone()));
        let desktop = Rc::new(Desktop::isolated_preview(runtime.clone()).unwrap());
        let slot = Rc::new(std::cell::RefCell::new(None));
        let view_slot = slot.clone();
        let state = runtime.clone();
        let system = desktop.clone();
        let settings = cx.open_window(WindowOptions {
            window_bounds: Some(WindowBounds::centered(size(px(1160.), px(840.)), cx)),
            titlebar: Some(TitlebarOptions {
                title: Some("VoiceBar · 快捷键验证（独立配置）".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            window_min_size: Some(size(px(880.), px(650.))),
            focus: true,
            show: true,
            ..Default::default()
        }, move |window, cx| {
            let view = cx.new(|cx| {
                let mut settings = Settings::new(state, system, window, cx);
                settings.tab = 3;
                settings
            });
            *view_slot.borrow_mut() = Some(view.clone());
            cx.new(|cx| Root::new(view, window, cx))
        }).unwrap();
        let view = slot.borrow_mut().take().unwrap();
        let hwnd = settings.update(cx, |_, w, _| native_handle(w)).unwrap();
        desktop::show_window(hwnd, true);
        eprintln!("shortcut fixture ready: hwnd={hwnd}");
        cx.spawn(async move |cx| {
            let result: Result<(), String> = async {
                macro_rules! pump {
                    ($count:expr) => {
                        for _ in 0..$count {
                            cx.background_executor().timer(Duration::from_millis(30)).await;
                            view.update(cx, |v, cx| v.tick(cx)).unwrap();
                            settings.update(cx, |_, w, _| w.refresh()).unwrap();
                        }
                    };
                }
                macro_rules! record {
                    ($field:expr, $keys:expr) => {{
                        settings.update(cx, |_, window, cx| {
                            view.update(cx, |v, cx| v.begin_shortcut($field, window, cx));
                        }).unwrap();
                        assert!(view.read_with(cx, |v, _| v.shortcut_recording.is_some()).unwrap(), "capture did not start");
                        send_keys(hwnd, $keys)?;
                        pump!(5);
                        assert!(view.read_with(cx, |v, _| v.shortcut_recording.is_none()).unwrap(), "capture did not finish");
                    }};
                }
                let deadline = Instant::now() + Duration::from_secs(45);
                while unsafe { GetForegroundWindow() }.0 as isize != hwnd && Instant::now() < deadline {
                    pump!(1);
                }
                if unsafe { GetForegroundWindow() }.0 as isize != hwnd {
                    return Err("activate the owned shortcut fixture to run this native test".into());
                }
                pump!(10);
                // Recording the currently registered chord must consume it.
                record!(ShortcutField::Toggle, &[
                    (0xa2,false),(0xa4,false),(0xa0,false),(0x87,false),(0x87,false),
                    (0x87,true),(0xa0,true),(0xa4,true),(0xa2,true),
                ]);
                assert_eq!(runtime.status.lock().unwrap().state, "idle");
                assert!(runtime.active.lock().unwrap().is_none());
                assert!(!path.exists(), "recording a chord must not save");

                record!(ShortcutField::Cancel, &[
                    (0xa2,false),(0xa0,false),(0x85,false),(0x85,true),(0xa0,true),(0xa2,true),
                ]);
                assert_eq!(view.read_with(cx, |v, _| v.draft.cancel_hotkey.clone()).unwrap(), "Ctrl+Shift+F22");
                assert!(runtime.config.lock().unwrap().cancel_hotkey.is_empty());
                view.update(cx, |v, cx| v.save(cx)).unwrap();
                let saved = crate::config::load(&path)?;
                assert_eq!(saved.cancel_hotkey, "Ctrl+Shift+F22");
                assert!(!view.read_with(cx, |v, _| v.dirty).unwrap());

                // Reserve a different chord and prove a failed save preserves
                // the old disk configuration and old live global registration.
                struct Reservation;
                impl Drop for Reservation {
                    fn drop(&mut self) { unsafe { let _ = UnregisterHotKey(None, 0x7b10); } }
                }
                unsafe { RegisterHotKey(None, 0x7b10, MOD_CONTROL | MOD_ALT | MOD_SHIFT, 0x84) }
                    .map_err(|e| e.to_string())?;
                let reservation = Reservation;
                record!(ShortcutField::Toggle, &[
                    (0xa2,false),(0xa4,false),(0xa0,false),(0x84,false),
                    (0x84,true),(0xa0,true),(0xa4,true),(0xa2,true),
                ]);
                view.update(cx, |v, cx| v.save(cx)).unwrap();
                assert!(view.read_with(cx, |v, _| v.message.contains("占用")).unwrap());
                assert_eq!(crate::config::load(&path)?, saved);
                assert_eq!(*runtime.config.lock().unwrap(), saved);
                drop(reservation);
                send_keys(hwnd, &[
                    (0xa2,false),(0xa4,false),(0xa0,false),(0x87,false),
                    (0x87,true),(0xa0,true),(0xa4,true),(0xa2,true),
                ])?;
                pump!(6);
                // Missing fixture API key prevents microphone/network use and
                // makes the old toggle's continued registration observable.
                assert_eq!(runtime.status.lock().unwrap().state, "error");
                assert!(runtime.active.lock().unwrap().is_none());
                runtime.set_status("idle", "快捷键原生验证通过");

                record!(ShortcutField::Cancel, &[
                    (0xa2,false),(0xa4,false),(0xa0,false),(0x84,false),
                    (0x84,true),(0xa0,true),(0xa4,true),(0xa2,true),
                ]);
                assert_eq!(view.read_with(cx, |v, _| v.draft.cancel_hotkey.clone()).unwrap(), "Ctrl+Shift+F22");
                assert!(view.read_with(cx, |v, _| v.message.contains("相同")).unwrap());

                record!(ShortcutField::Cancel, &[(0x1b,false),(0x1b,true)]);
                assert_eq!(view.read_with(cx, |v, app| v.form_config(app).unwrap().cancel_hotkey).unwrap(), "Escape");
                assert_eq!(crate::config::load(&path)?, saved);

                settings.update(cx, |_, window, cx| {
                    view.update(cx, |v, cx| v.begin_shortcut(ShortcutField::Toggle, window, cx));
                }).unwrap();
                settings.update(cx, |_, window, cx| {
                    view.update(cx, |v, _| v.shortcut_focus[1].focus(window));
                }).unwrap();
                pump!(3);
                assert!(view.read_with(cx, |v, _| v.shortcut_recording.is_none()).unwrap(), "losing field focus must cancel capture");
                settings.update(cx, |_, window, cx| {
                    view.update(cx, |v, cx| v.begin_shortcut(ShortcutField::Toggle, window, cx));
                }).unwrap();
                desktop::hide_window(hwnd);
                pump!(3);
                assert!(view.read_with(cx, |v, _| v.shortcut_recording.is_none()).unwrap(), "hiding settings must cancel capture");
                desktop::show_window(hwnd, true);
                pump!(3);

                eprintln!("shortcut native checks passed: registered chord interception, repeat, draft-only capture, save/reload, OS conflict rollback, duplicate rejection, Escape");
                let inspect = root.join("inspect.json");
                if inspect.exists() {
                    eprintln!("shortcut inspection ready: hwnd={hwnd}");
                    let deadline = Instant::now() + Duration::from_secs(180);
                    let mut previous = String::new();
                    while Instant::now() < deadline {
                        let done = std::fs::read_to_string(&inspect).ok()
                            .and_then(|s| serde_json::from_str::<Value>(&s).ok())
                            .and_then(|v| v["done"].as_bool()).unwrap_or(false);
                        if done { break; }
                        pump!(1);
                        let snapshot = view.read_with(cx, |v, _| serde_json::json!({
                            "hotkey": v.draft.hotkey,
                            "cancelHotkey": v.draft.cancel_hotkey,
                            "dirty": v.dirty,
                            "recording": v.shortcut_recording.is_some(),
                            "message": v.message,
                        }).to_string()).unwrap();
                        if snapshot != previous {
                            std::fs::write(root.join("inspection-state.json"), &snapshot).map_err(|e| e.to_string())?;
                            previous = snapshot;
                        }
                    }
                }
                assert!(runtime.active.lock().unwrap().is_none());
                std::fs::remove_file(&path).map_err(|e| e.to_string())?;
                Ok(())
            }.await;
            view.update(cx, |v, cx| v.cancel_shortcut(cx)).unwrap();
            *completed.lock().unwrap() = Some(result);
            cx.update(|cx| cx.quit()).unwrap();
        }).detach();
    });
    outcome
        .lock()
        .unwrap()
        .take()
        .expect("native shortcut test finished")
        .unwrap();
}
