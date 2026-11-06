//! Run a loopback server with `node tests/funasr-fixture.mjs 38219`, set
//! VOICEBAR_FIXTURE_PORT=38219, then run this ignored test with --test-threads=1.
//! Uses the default capture device and sends only F22 to a test-owned window.
use super::*;

#[test]
#[ignore = "requires a visible Windows desktop, a capture device and the loopback FunASR fixture"]
fn holds_release_or_time_out_and_keep_recent_results() {
    let port: u16 = std::env::var("VOICEBAR_FIXTURE_PORT")
        .expect("start tests/funasr-fixture.mjs and set VOICEBAR_FIXTURE_PORT")
        .parse()
        .unwrap();
    let outcome = Arc::new(Mutex::new(None));
    let completed = outcome.clone();
    Application::new().run(move |cx| {
        init_theme(cx);
        let mut config = AppConfig::default();
        config.provider.endpoint = format!("ws://localhost:{port}");
        config.injection.mode = "preview".into();
        config.hotkey = "F22".into();
        config.cancel_hotkey.clear();
        config.hold_to_talk = true;
        config.max_recording_secs = 5;
        config.history_enabled = true;
        config.update_check = "off".into();
        let runtime = Arc::new(Runtime::new(config, Default::default()));
        let desktop = Rc::new(Desktop::isolated_preview(runtime.clone()).unwrap());
        let state = runtime.clone();
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::centered(size(px(1120.), px(820.)), cx)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("VoiceBar · 听写验证（独立配置）".into()),
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                move |window, cx| {
                    let view = cx.new(|cx| Settings::new(state, desktop, window, cx));
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )
            .unwrap();
        let hwnd = window.update(cx, |_, w, _| native_handle(w)).unwrap();
        desktop::show_window(hwnd, true);
        cx.spawn(async move |cx| {
            let result: Result<(), String> = async {
                macro_rules! until {
                    ($condition:expr, $seconds:expr, $description:expr) => {{
                        let deadline = Instant::now() + Duration::from_secs($seconds);
                        while !($condition) {
                            if Instant::now() >= deadline {
                                return Err(format!(
                                    "timed out waiting for {}: {}",
                                    $description,
                                    runtime.status.lock().unwrap().message
                                ));
                            }
                            cx.background_executor()
                                .timer(Duration::from_millis(30))
                                .await;
                            window.update(cx, |_, w, _| w.refresh()).unwrap();
                        }
                    }};
                }
                until!(crate::platform::foreground() == hwnd, 8, "owned foreground");
                let probe = crate::audio::MicrophoneTest::start(runtime.clone(), String::new());
                let mut mic_result;
                until!(
                    {
                        mic_result = probe.result();
                        mic_result.is_some()
                    },
                    10,
                    "local mic check"
                );
                let mic_result = mic_result.unwrap();
                if !mic_result.starts_with("测试完成")
                    && !mic_result.starts_with("未检测到明显声音")
                {
                    return Err(mic_result);
                }
                eprintln!("standalone microphone: {mic_result}");
                drop(probe);

                const F22: u16 = 0x85;
                for expected in 1..=2 {
                    send_keys(hwnd, &[(F22, false)])?;
                    let held = async {
                        // The fixture emits non-empty text only after audio flows.
                        until!(
                            {
                                let recording = runtime.active.lock().unwrap().is_some();
                                let transcript = runtime.transcript.lock().unwrap();
                                recording
                                    && (!transcript.partial.is_empty()
                                        || !transcript.stable.is_empty())
                            },
                            4,
                            "streamed fixture transcript"
                        );
                        if runtime.active.lock().unwrap().is_none() {
                            return Err("recording ended before key release".into());
                        }
                        if expected == 2 {
                            // With preview output, the time limit finishes while F22 is still down.
                            until!(
                                runtime.recent_results().len() == 2,
                                8,
                                "held session time limit"
                            );
                        }
                        Ok(())
                    }
                    .await;
                    send_keys(hwnd, &[(F22, true)])?;
                    held?;
                    until!(
                        runtime.recent_results().len() == expected,
                        4,
                        "recent result"
                    );
                    let recent = runtime.recent_results();
                    if recent[0].label != "已完成" || recent[0].preview != "VoiceBar 流式输入验证。"
                    {
                        return Err(format!("unexpected recent result: {}", recent[0].preview));
                    }
                    eprintln!(
                        "completed session {expected}: id={}, preview={}",
                        recent[0].id, recent[0].preview
                    );
                }
                let recent = runtime.recent_results();
                if recent[0].id == recent[1].id {
                    return Err("expected two separately recoverable sessions".into());
                }
                Ok(())
            }
            .await;
            runtime.stop(true);
            *completed.lock().unwrap() = Some(result);
            cx.update(|cx| cx.quit()).unwrap();
        })
        .detach();
    });
    outcome
        .lock()
        .unwrap()
        .take()
        .expect("test did not complete")
        .unwrap();
}
