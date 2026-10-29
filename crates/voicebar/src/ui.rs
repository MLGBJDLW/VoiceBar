use crate::{
    config::{AppConfig, Appearance},
    desktop::{self, Desktop},
    runtime::Runtime,
    updater, voice_shape,
    voice_visual::{AudioFrame, Motion, VisualFrame, BAND_COUNT},
};
use gpui::{prelude::*, *};
use gpui_component::{
    input::{Input, InputEvent, InputState},
    switch::Switch,
    Root,
};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};
use serde_json::Value;
use std::{
    collections::BTreeMap,
    rc::Rc,
    sync::{atomic::Ordering, mpsc, Arc},
    time::{Duration, Instant},
};

mod chrome;
mod shortcut_capture;
mod shortcuts;
use chrome::*;

const TITLEBAR_HEIGHT: f32 = 42.;
const CONTROL_HOVER: u32 = 0x2a303b;
const CLOSE_HOVER: u32 = 0xb6434c;
const TABS: [&str; 4] = ["工作台", "外观与动效", "识别引擎", "输入与启动"];
const PROVIDERS: [(&str, &str, &str, &str); 7] = [
    (
        "funasr",
        "FunASR · 本机",
        "ws://localhost:10095",
        "paraformer",
    ),
    (
        "dashscope",
        "阿里云 Paraformer",
        "wss://dashscope.aliyuncs.com/api-ws/v1/inference",
        "paraformer-realtime-v2",
    ),
    (
        "openai-realtime",
        "OpenAI Realtime",
        "wss://api.openai.com/v1/realtime?intent=transcription",
        "gpt-live-transcribe",
    ),
    (
        "deepgram",
        "Deepgram Nova",
        "wss://api.deepgram.com/v1/listen",
        "nova-3",
    ),
    ("xfyun", "科大讯飞", "wss://iat-api.xfyun.cn/v2/iat", "iat"),
    (
        "openai-compatible",
        "OpenAI 兼容转写",
        "https://api.openai.com/v1/audio/transcriptions",
        "whisper-1",
    ),
    (
        "whispercpp",
        "whisper.cpp · 本机",
        "http://localhost:8080/inference",
        "whisper",
    ),
];
const FIELDS: &[(&str, &str)] = &[
    ("provider.endpoint", "服务地址 / Endpoint"),
    ("provider.model", "模型名称"),
    ("provider.language", "语言 / zh、en、auto"),
    ("provider.apiKey", "API Key / Token"),
    ("provider.appId", "讯飞 APPID"),
    ("provider.apiSecret", "讯飞 APISecret"),
    ("correction.endpoint", "校对服务 / Chat Completions"),
    ("correction.model", "校对模型"),
    ("correction.apiKey", "校对 API Key"),
    ("correction.prompt", "校对指令"),
    ("micDevice", "麦克风 / 留空跟随系统默认"),
    ("maxRecordingSecs", "录音上限 / 秒"),
    ("silenceStopSecs", "停顿自动结束 / 秒（0 = 关闭）"),
    ("appearance.accent", "声浪颜色 / #RRGGBB"),
    ("appearance.opacity", "背景透明度 / 0.3–1"),
    ("appearance.intensity", "声浪强度 / 0–3"),
    ("appearance.scale", "语音条大小 / 0.65–1.8"),
    ("appearance.offsetX", "水平偏移 / px"),
    ("appearance.offsetY", "垂直偏移 / px"),
    ("hotwords", "专有词与热词 / 每行一个"),
];

fn pointer(path: &str) -> String {
    format!("/{}", path.replace('.', "/"))
}
fn native_handle(window: &Window) -> isize {
    match HasWindowHandle::window_handle(window).map(|h| h.as_raw()) {
        Ok(RawWindowHandle::Win32(handle)) => handle.hwnd.get(),
        _ => 0,
    }
}
// Keep window operations in GPUI's native hit-test path, so resizing, snapping,
// double-click maximize and close-to-tray use the platform's existing behavior.
fn window_control(id: &'static str, area: WindowControlArea, maximized: bool) -> Stateful<Div> {
    div()
        .id(id)
        .window_control_area(area)
        .w(px(46.))
        .h(px(TITLEBAR_HEIGHT))
        .flex()
        .flex_shrink_0()
        .items_center()
        .justify_center()
        .hover(move |style| {
            style.bg(rgb(if area == WindowControlArea::Close {
                CLOSE_HOVER
            } else {
                CONTROL_HOVER
            }))
        })
        .child(
            canvas(
                |_, _, _| (),
                move |bounds, _, window, _| {
                    let segments: &[&[(f32, f32)]] = match area {
                        WindowControlArea::Min => &[&[(2., 7.), (12., 7.)]],
                        WindowControlArea::Max if maximized => &[
                            &[(5., 2.), (12., 2.), (12., 9.)],
                            &[(2., 5.), (9., 5.), (9., 12.), (2., 12.), (2., 5.)],
                        ],
                        WindowControlArea::Max => {
                            &[&[(2., 2.), (12., 2.), (12., 12.), (2., 12.), (2., 2.)]]
                        }
                        WindowControlArea::Close => {
                            &[&[(3., 3.), (11., 11.)], &[(11., 3.), (3., 11.)]]
                        }
                        WindowControlArea::Drag => &[],
                    };
                    for points in segments {
                        let mut path = PathBuilder::stroke(px(1.1));
                        for (index, &(x, y)) in points.iter().enumerate() {
                            let p = bounds.origin + point(px(x), px(y));
                            if index == 0 {
                                path.move_to(p);
                            } else {
                                path.line_to(p);
                            }
                        }
                        if let Ok(path) = path.build() {
                            window.paint_path(path, rgb(TEXT));
                        }
                    }
                },
            )
            .size(px(14.)),
        )
}

fn settings_titlebar(window: &Window) -> Div {
    div()
        .flex()
        .w_full()
        .h(px(TITLEBAR_HEIGHT))
        .flex_shrink_0()
        .bg(rgb(BG))
        .border_b_1()
        .border_color(rgb(LINE))
        .child(
            row()
                .id("window-drag-region")
                .window_control_area(WindowControlArea::Drag)
                .h_full()
                .flex_1()
                .min_w_0()
                .px_5()
                .gap_3()
                .child(
                    div().flex().items_center().gap(px(3.)).children(
                        [8., 14., 20., 14., 8.].into_iter().map(|height| {
                            div().w(px(2.)).h(px(height)).rounded_full().bg(rgb(MINT))
                        }),
                    ),
                )
                .child(
                    div()
                        .text_size(px(13.))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child("VoiceBar"),
                )
                .child(note("桌面语音工作空间")),
        )
        .child(window_control(
            "window-minimize",
            WindowControlArea::Min,
            false,
        ))
        .child(window_control(
            "window-maximize",
            WindowControlArea::Max,
            window.is_maximized(),
        ))
        .child(window_control(
            "window-close",
            WindowControlArea::Close,
            false,
        ))
}

enum Job {
    Connection(Result<String, String>),
    Update(Result<updater::UpdateInfo, String>),
    Installed(Result<(), String>),
    Microphones(Result<Vec<String>, String>),
}
pub struct Settings {
    runtime: Arc<Runtime>,
    desktop: Rc<Desktop>,
    draft: AppConfig,
    inputs: BTreeMap<&'static str, Entity<InputState>>,
    shortcut_focus: [FocusHandle; 2],
    shortcut_recording: Option<shortcuts::ShortcutRecording>,
    _subscriptions: Vec<Subscription>,
    tab: usize,
    dirty: bool,
    message: String,
    job: Option<mpsc::Receiver<Job>>,
    microphones: Vec<String>,
    update: Option<updater::UpdateInfo>,
    demo_until: Option<Instant>,
    voice: VoiceMotion,
    last_snapshot: String,
}

impl Settings {
    fn new(
        runtime: Arc<Runtime>,
        desktop: Rc<Desktop>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let draft = runtime.config.lock().unwrap().masked();
        let json = serde_json::to_value(&draft).unwrap();
        let mut inputs = BTreeMap::new();
        let mut subscriptions = Vec::new();
        for &(path, _) in FIELDS {
            let value = if path == "hotwords" {
                draft.hotwords.join("\n")
            } else {
                let v = json.pointer(&pointer(path)).unwrap();
                v.as_str()
                    .map(str::to_owned)
                    .unwrap_or_else(|| v.to_string())
            };
            let input = cx.new(|cx| {
                InputState::new(window, cx)
                    .default_value(value)
                    .masked(path.ends_with("apiKey") || path.ends_with("apiSecret"))
                    .multi_line(path == "hotwords" || path == "correction.prompt")
                    .rows(if path == "hotwords" || path == "correction.prompt" {
                        3
                    } else {
                        1
                    })
            });
            subscriptions.push(
                cx.subscribe(&input, move |this, _, event: &InputEvent, cx| {
                    if matches!(event, InputEvent::Change) {
                        this.dirty = true;
                        if path.starts_with("appearance.")
                            && this.runtime.overlay_preview().is_some()
                        {
                            if let Ok(appearance) = this.form_appearance(cx) {
                                let _ = this.runtime.preview_overlay(appearance);
                            }
                        }
                        cx.notify();
                    }
                }),
            );
            inputs.insert(path, input);
        }
        let (tx, rx) = mpsc::channel();
        runtime.executor.spawn_blocking(move || {
            let _ = tx.send(Job::Microphones(crate::audio::devices()));
        });
        let mut settings = Self {
            runtime,
            desktop,
            draft,
            inputs,
            shortcut_focus: [cx.focus_handle(), cx.focus_handle()],
            shortcut_recording: None,
            _subscriptions: subscriptions,
            tab: 0,
            dirty: false,
            message: "设置会在保存后生效。关闭窗口后继续在托盘待命。".into(),
            job: Some(rx),
            microphones: vec![],
            update: None,
            demo_until: None,
            voice: VoiceMotion::default(),
            last_snapshot: String::new(),
        };
        settings.subscribe_shortcuts(window, cx);
        settings
    }
    fn value(&self, path: &str, cx: &App) -> String {
        self.inputs[path].read(cx).value().to_string()
    }
    fn form_config(&self, cx: &App) -> Result<AppConfig, String> {
        let mut json = serde_json::to_value(&self.draft).map_err(|e| e.to_string())?;
        for &(path, label) in FIELDS {
            let text = self.value(path, cx);
            let target = json
                .pointer_mut(&pointer(path))
                .ok_or_else(|| format!("缺少设置：{label}"))?;
            *target = match target {
                Value::String(_) => Value::String(text),
                Value::Number(_)
                    if path == "appearance.offsetX" || path == "appearance.offsetY" =>
                {
                    Value::from(
                        text.trim()
                            .parse::<i64>()
                            .map_err(|_| format!("{label} 需要整数"))?,
                    )
                }
                Value::Number(n) if n.is_u64() => Value::from(
                    text.trim()
                        .parse::<u64>()
                        .map_err(|_| format!("{label} 需要非负整数"))?,
                ),
                Value::Number(n) if n.is_i64() => Value::from(
                    text.trim()
                        .parse::<i64>()
                        .map_err(|_| format!("{label} 需要整数"))?,
                ),
                Value::Number(_) => {
                    let n = text
                        .trim()
                        .parse::<f64>()
                        .map_err(|_| format!("{label} 需要数值"))?;
                    serde_json::Number::from_f64(n)
                        .map(Value::Number)
                        .ok_or_else(|| format!("{label} 需要有限数值"))?
                }
                Value::Array(_) => Value::Array(
                    text.lines()
                        .map(str::trim)
                        .filter(|v| !v.is_empty())
                        .map(|s| Value::String(s.into()))
                        .collect(),
                ),
                _ => return Err(format!("无法编辑：{label}")),
            };
        }
        serde_json::from_value(json).map_err(|e| e.to_string())
    }
    fn choice(&mut self, path: &str, value: Value) {
        let mut json = serde_json::to_value(&self.draft).unwrap();
        *json.pointer_mut(&pointer(path)).unwrap() = value;
        self.draft = serde_json::from_value(json).unwrap();
        self.dirty = true;
    }
    fn set_field(&mut self, path: &str, value: &str, window: &mut Window, cx: &mut Context<Self>) {
        self.inputs[path].update(cx, |state, cx| {
            state.set_value(value.to_owned(), window, cx)
        });
    }
    fn field(&self, path: &'static str) -> Div {
        let label = FIELDS.iter().find(|(p, _)| *p == path).unwrap().1;
        column()
            .gap_2()
            .w_full()
            .min_w_0()
            .child(note(label))
            .child(
                Input::new(&self.inputs[path])
                    .when(path == "hotwords" || path == "correction.prompt", |input| {
                        input.h(px(90.))
                    }),
            )
    }
    fn pair(&self, a: &'static str, b: &'static str) -> Div {
        row()
            .items_start()
            .child(self.field(a).flex_1().w_auto())
            .child(self.field(b).flex_1().w_auto())
    }
    fn choices(&self, path: &'static str, options: &[(&str, &str)], cx: &Context<Self>) -> Div {
        let json = serde_json::to_value(&self.draft).unwrap();
        let current = json
            .pointer(&pointer(path))
            .unwrap()
            .as_str()
            .unwrap_or("")
            .to_owned();
        row()
            .flex_wrap()
            .gap_2()
            .children(options.iter().map(|(value, label)| {
                let value = value.to_string();
                let selected = value == current;
                button(
                    SharedString::from(format!("{path}-{value}")),
                    label.to_string(),
                )
                .when(selected, |button| {
                    button
                        .border_color(rgb(0x4a7568))
                        .text_color(rgb(MINT))
                        .bg(rgb(TINT))
                })
                .on_click(cx.listener(move |this, _, _, cx| {
                    this.choice(path, Value::String(value.clone()));
                    if path.starts_with("appearance.") {
                        this.preview_overlay(cx);
                    }
                    cx.notify();
                }))
            }))
    }
    fn toggle(
        &self,
        path: &'static str,
        label: &'static str,
        description: &'static str,
        cx: &Context<Self>,
    ) -> Div {
        let json = serde_json::to_value(&self.draft).unwrap();
        let checked = json
            .pointer(&pointer(path))
            .and_then(Value::as_bool)
            .unwrap_or(false);
        row()
            .justify_between()
            .py_3()
            .border_b_1()
            .border_color(rgb(LINE))
            .child(
                column()
                    .gap_1()
                    .flex_1()
                    .child(div().text_size(px(14.)).child(label))
                    .child(note(description)),
            )
            .child(Switch::new(path).checked(checked).on_click(cx.listener(
                move |this, value: &bool, _, cx| {
                    this.choice(path, Value::Bool(*value));
                    cx.notify();
                },
            )))
    }
    fn save(&mut self, cx: &mut Context<Self>) {
        self.cancel_shortcut(cx);
        let result = self.form_config(cx).and_then(|config| {
            self.desktop.save_config(config)?;
            self.draft = self.runtime.config.lock().unwrap().masked();
            Ok(())
        });
        match result {
            Ok(()) => {
                self.dirty = false;
                if self.runtime.overlay_preview().is_some() {
                    let _ = self.runtime.preview_overlay(self.draft.appearance.clone());
                }
                self.message = "设置已保存，快捷键、外观与启动选项已生效。".into();
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }
    fn save_appearance(&mut self, cx: &mut Context<Self>) {
        let result = self.form_appearance(cx).and_then(|appearance| {
            let mut config = self.runtime.config.lock().unwrap().clone();
            config.appearance = appearance;
            self.desktop.save_config(config)?;
            self.draft.appearance = self.runtime.config.lock().unwrap().appearance.clone();
            let saved = self.runtime.config.lock().unwrap().masked();
            self.dirty = self.form_config(cx).map_or(true, |draft| draft != saved);
            self.runtime.preview_overlay(self.draft.appearance.clone())
        });
        self.message = match result {
            Ok(()) => "外观与弹出位置已保存，正在桌面预览 10 秒。".into(),
            Err(error) => error,
        };
        cx.notify();
    }

    fn preview_overlay(&mut self, cx: &mut Context<Self>) {
        self.message = match self
            .form_appearance(cx)
            .and_then(|a| self.runtime.preview_overlay(a))
        {
            Ok(()) => {
                "正在预览当前位置与动效 · 10 秒后收起；点击「保存外观与位置」保留调整。".into()
            }
            Err(error) => error,
        };
        cx.notify();
    }

    fn test_connection(&mut self, cx: &mut Context<Self>) {
        if self.job.is_some() {
            self.message = "请等待当前操作完成。".into();
            return;
        }
        match self.form_config(cx) {
            Ok(mut config) => {
                config.merge_secrets(&self.runtime.config.lock().unwrap());
                let (tx, rx) = mpsc::channel();
                self.job = Some(rx);
                self.message = "正在测试连接…".into();
                self.runtime.executor.spawn(async move {
                    let result = crate::providers::check_connection(config.provider)
                        .await
                        .map_err(|e| e.to_string());
                    let _ = tx.send(Job::Connection(result));
                });
            }
            Err(error) => self.message = error,
        }
        cx.notify();
    }
    fn check_update(&mut self, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.message = "正在检查更新…".into();
        self.runtime.executor.spawn(async move {
            let _ = tx.send(Job::Update(updater::check_for_update().await));
        });
        cx.notify();
    }
    fn install_update(&mut self, cx: &mut Context<Self>) {
        if self.job.is_some() {
            return;
        }
        self.runtime.stop(true);
        let (tx, rx) = mpsc::channel();
        self.job = Some(rx);
        self.message = "正在下载并验证更新…".into();
        self.runtime.executor.spawn(async move {
            let _ = tx.send(Job::Installed(updater::install_update().await));
        });
        cx.notify();
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        self.poll_shortcut(cx);
        if let Some(result) = self.job.as_ref().and_then(|rx| rx.try_recv().ok()) {
            self.job = None;
            match result {
                Job::Connection(result) => self.message = result.unwrap_or_else(|e| e),
                Job::Microphones(result) => match result {
                    Ok(mics) => self.microphones = mics,
                    Err(e) => self.message = e,
                },
                Job::Update(result) => match result {
                    Ok(info) => {
                        self.message = if info.available {
                            format!(
                                "发现新版本 {}，可点击安装。",
                                info.version.as_deref().unwrap_or("")
                            )
                        } else {
                            "已是最新版本。".into()
                        };
                        self.update = Some(info);
                    }
                    Err(error) => self.message = error,
                },
                Job::Installed(result) => match result {
                    Ok(()) => {
                        self.message = "更新安装程序已启动。".into();
                        self.runtime.quit_requested.store(true, Ordering::SeqCst);
                    }
                    Err(error) => self.message = error,
                },
            }
            cx.notify();
        }
        let status = self.runtime.status.lock().unwrap().clone();
        let text = self.runtime.transcript.lock().unwrap().clone();
        let snapshot = format!(
            "{}{}{}{}",
            status.state, status.message, text.stable, text.partial
        );
        if snapshot != self.last_snapshot
            || status.state == "listening"
            || self.demo_until.is_some()
            || self.voice.moving
        {
            self.last_snapshot = snapshot;
            cx.notify();
        }
        if self.demo_until.is_some_and(|until| Instant::now() > until) {
            self.demo_until = None;
            cx.notify();
        }
    }
    fn form_appearance(&self, cx: &App) -> Result<Appearance, String> {
        let mut a = self.draft.appearance.clone();
        a.accent = self.value("appearance.accent", cx).trim().to_owned();
        for (path, target) in [
            ("appearance.opacity", &mut a.opacity),
            ("appearance.intensity", &mut a.intensity),
            ("appearance.scale", &mut a.scale),
        ] {
            let label = FIELDS.iter().find(|(p, _)| *p == path).unwrap().1;
            *target = self
                .value(path, cx)
                .trim()
                .parse::<f64>()
                .map_err(|_| format!("{label} 需要数值"))?;
        }
        for (path, target) in [
            ("appearance.offsetX", &mut a.offset_x),
            ("appearance.offsetY", &mut a.offset_y),
        ] {
            let label = FIELDS.iter().find(|(p, _)| *p == path).unwrap().1;
            *target = self
                .value(path, cx)
                .trim()
                .parse::<i32>()
                .map_err(|_| format!("{label} 需要整数"))?;
        }
        a.validate()?;
        Ok(a)
    }

    fn appearance(&self, cx: &App) -> Appearance {
        self.form_appearance(cx)
            .unwrap_or_else(|_| self.draft.appearance.clone())
    }
}

impl Settings {
    fn overview(&self, visual: VisualFrame, wide: bool, cx: &Context<Self>) -> Div {
        let status = self.runtime.status.lock().unwrap().clone();
        let transcript = self.runtime.transcript.lock().unwrap().clone();
        let active = self.runtime.active.lock().unwrap().is_some();
        let saved = self.runtime.config.lock().unwrap().clone();
        let text = format!("{}{}", transcript.stable, transcript.partial);
        let provider = PROVIDERS
            .iter()
            .find(|p| p.0 == saved.provider.kind)
            .map(|p| p.1)
            .unwrap_or("自定义引擎");
        let mode = match saved.injection.mode.as_str() {
            "realtime" => "边说边写",
            "stable" => "逐段写入",
            "final" => "结束后写入",
            _ => "仅预览",
        };
        let can_install =
            self.update.as_ref().is_some_and(|u| u.available) && updater::can_self_update();
        column().gap_5()
            .child(
                card().p_6().gap_5().bg(rgb(0x1b2729)).border_color(rgb(0x344b47))
                    .child(row().justify_between()
                        .child(eyebrow("VOICE TO WORDS").text_color(rgb(0x9bb8af)))
                        .child(badge(state_label(&status.state), state_color(&status.state))))
                    .child(row().items_center().gap_6()
                        .child(column().flex_1().gap_4()
                            .child(div().text_size(px(if wide { 30. } else { 25. })).line_height(px(39.))
                                .font_weight(FontWeight::SEMIBOLD).child("少一点打字，多一点表达。"))
                            .child(note("在任意输入框按下快捷键，让思绪自然成文。").text_color(rgb(0xafc4bf)))
                            .child(keycaps(&saved.hotkey))
                            .child(row().flex_wrap().gap_2()
                                .child(primary("record", if active { "结束录音" } else { "测试录音" })
                                    .child(icon(if active { Glyph::Stop } else { Glyph::Mic }, 0x10251d, 15.))
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if this.runtime.active.lock().unwrap().is_some() { this.runtime.stop(false); }
                                        else if let Err(error) = this.runtime.start() { this.message = error; }
                                        cx.notify();
                                    })))
                                .child(button("input-guide", "配置快捷键")
                                    .bg(rgba(0xffffff08)).on_click(cx.listener(|this, _, _, cx| {
                                        this.tab = 3; cx.notify();
                                    })))))
                        .when(wide, |r| r.child(
                            column().w(px(210.)).h(px(142.)).justify_center().items_center().gap_3()
                                .rounded(px(16.)).bg(rgba(0x101d2080))
                                .child(waveform(self.appearance(cx), visual).w(px(180.)).h(px(70.)))
                                .child(note("让声音看得见").text_color(rgb(0x8cad9f)))
                        )))
                    .child(note("测试录音仅在设置中预览；向其他应用写入时，先聚焦输入框，再使用快捷键。")
                        .text_size(px(11.)).text_color(rgb(0x9bb1aa)))
            )
            .child(
                row().gap_3().children([
                    (Glyph::Engine, "识别服务", provider.to_string(), 2),
                    (Glyph::Keyboard, "输入方式", mode.to_string(), 3),
                    (Glyph::Wave, "浮框外观", match saved.appearance.effect.as_str() {
                        "spectrum" => "声纹频带", "orbit" => "涟漪轨道", "bars" => "节奏音柱", _ => "流动丝带",
                    }.to_string(), 1),
                ].into_iter().enumerate().map(|(i, (glyph, label, value, tab))| {
                    card().id(("quick-setting", i)).flex_1().min_w_0().p_4().gap_3()
                        .cursor_pointer().hover(|s| s.bg(rgb(0x202630)).border_color(rgb(0x3b4754)))
                        .child(row().gap_2().child(icon(glyph, MUTED, 16.)).child(note(label)))
                        .child(row().justify_between().gap_1()
                            .child(div().flex_1().min_w_0().text_ellipsis().text_size(px(13.))
                                .font_weight(FontWeight::MEDIUM).child(value))
                            .child(icon(Glyph::Arrow, MUTED, 13.)))
                        .on_click(cx.listener(move |this, _, _, cx| { this.tab = tab; cx.notify(); }))
                }))
            )
            .child(
                card().gap_4()
                    .child(row().justify_between()
                        .child(row().gap_2().child(icon(Glyph::Spark, MINT, 17.))
                            .child(title("本次识别")).when(transcript.corrected, |r| r.child(badge("已校对", MINT))))
                        .child(button("copy-result", "复制文字").child(icon(Glyph::Copy, MUTED, 14.))
                            .on_click(cx.listener(|this, _, _, cx| {
                                this.message = this.runtime.copy_transcript().map(|_| "已复制。".into()).unwrap_or_else(|e| e);
                                cx.notify();
                            }))))
                    .child(div().id("transcript").min_h(px(94.)).max_h(px(220.)).w_full().min_w_0()
                        .p_4().rounded(px(10.)).bg(rgb(INSET)).overflow_y_scroll()
                        .text_size(px(14.)).line_height(px(25.))
                        .when(text.is_empty(), |s| s.text_color(rgb(MUTED)))
                        .child(if text.is_empty() { "从一句话开始。\n识别的文字会在这里呈现，随时可以复制。".to_string() } else { text }))
                    .child(note(status.message))
            )
            .child(
                card().gap_3()
                    .child(row().justify_between()
                        .child(column().gap_1().child(title("保持最新")).child(note(format!("VoiceBar {} · Windows", env!("CARGO_PKG_VERSION")))))
                        .child(button("check-update", if self.job.is_some() { "操作进行中…" } else { "检查更新" })
                            .on_click(cx.listener(|this, _, _, cx| this.check_update(cx))))
                        .when(can_install, |r| r.child(primary("install-update", "下载并安装")
                            .on_click(cx.listener(|this, _, _, cx| this.install_update(cx))))))
                    .child(self.choices("updateCheck", &[("auto", "自动检查"), ("manual", "手动检查"), ("off", "关闭检查")], cx))
                    .when(!updater::can_self_update(), |c| c.child(note("便携版可从 GitHub Releases 下载新版本。")))
            )
    }
    fn appearance_panel(&self, cx: &Context<Self>) -> Div {
        let appearance = self.appearance(cx);
        let anchors = [
            ("top-left", "左上"),
            ("top", "顶部"),
            ("top-right", "右上"),
            ("left", "左侧"),
            ("center", "中央"),
            ("right", "右侧"),
            ("bottom-left", "左下"),
            ("bottom", "底部"),
            ("bottom-right", "右下"),
        ];
        let grid = column()
            .gap_2()
            .w(px(220.))
            .flex_shrink_0()
            .p_3()
            .rounded(px(12.))
            .bg(rgb(INSET))
            .children(anchors.chunks(3).map(|items| {
                row().gap_2().children(items.iter().map(|(anchor, label)| {
                    let anchor = anchor.to_string();
                    let selected = self.draft.appearance.anchor == anchor;
                    button(SharedString::from(format!("anchor-{anchor}")), *label)
                        .flex_1()
                        .px_0()
                        .h(px(34.))
                        .when(selected, |b| {
                            b.bg(rgb(TINT))
                                .border_color(rgb(0x527d6e))
                                .text_color(rgb(MINT))
                        })
                        .on_click(cx.listener(move |this, _, _, cx| {
                            this.choice("appearance.anchor", Value::String(anchor.clone()));
                            this.preview_overlay(cx);
                            cx.notify();
                        }))
                }))
            }))
            .child(
                div()
                    .h(px(3.))
                    .w(px(60.))
                    .mx_auto()
                    .mt_2()
                    .rounded_full()
                    .bg(rgb(0x3b4553)),
            );
        column()
            .gap_5()
            .child(
                card()
                    .child(title("选择一种声音节奏"))
                    .child(
                        row().gap_3().children(
                            [
                                ("ribbon", "流动丝带", "柔和 · 连续"),
                                ("spectrum", "声纹频带", "细腻 · 丰富"),
                                ("orbit", "涟漪轨道", "轻盈 · 环绕"),
                                ("bars", "节奏音柱", "利落 · 清晰"),
                            ]
                            .into_iter()
                            .map(|(effect, label, description)| {
                                let selected = appearance.effect == effect;
                                let mut sample = appearance.clone();
                                sample.effect = effect.into();
                                let frame = VisualFrame {
                                    amplitude: 0.6,
                                    width: 0.82,
                                    bands: (0..BAND_COUNT)
                                        .map(|i| 0.25 + (i as f32 * 0.65).sin().abs() * 0.5)
                                        .collect(),
                                    phase: 1.2,
                                    pause: 0.,
                                    pulse: 0.,
                                };
                                column()
                                    .id(SharedString::from(format!("effect-{effect}")))
                                    .flex_1()
                                    .min_w_0()
                                    .gap_2()
                                    .p_3()
                                    .rounded(px(11.))
                                    .border_1()
                                    .bg(rgb(INSET))
                                    .border_color(rgb(if selected { 0x527d6e } else { LINE }))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x222c32)))
                                    .child(waveform(sample, frame).w_full().h(px(40.)))
                                    .child(
                                        div()
                                            .text_size(px(12.))
                                            .text_color(rgb(if selected { MINT } else { TEXT }))
                                            .child(label),
                                    )
                                    .child(note(description).text_size(px(10.)))
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.choice(
                                            "appearance.effect",
                                            Value::String(effect.into()),
                                        );
                                        this.preview_overlay(cx);
                                        cx.notify();
                                    }))
                            }),
                        ),
                    )
                    .child(
                        row().gap_3().flex_wrap().child(note("快速配色")).children(
                            [
                                ("薄荷", "#a3ead2", 0xa3ead2),
                                ("冰蓝", "#8fcbff", 0x8fcbff),
                                ("紫藤", "#b6a3ff", 0xb6a3ff),
                                ("珊瑚", "#f5a8bd", 0xf5a8bd),
                                ("暖金", "#efca8d", 0xefca8d),
                            ]
                            .into_iter()
                            .map(|(label, hex, color)| {
                                row()
                                    .id(SharedString::from(format!("accent-{color}")))
                                    .gap_2()
                                    .px_2()
                                    .py_1()
                                    .rounded(px(7.))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgb(0x2b3340)))
                                    .child(
                                        div()
                                            .size(px(16.))
                                            .rounded_full()
                                            .bg(rgb(color))
                                            .border_2()
                                            .border_color(rgb(
                                                if appearance.accent.eq_ignore_ascii_case(hex) {
                                                    TEXT
                                                } else {
                                                    color
                                                },
                                            )),
                                    )
                                    .child(note(label).text_size(px(11.)))
                                    .on_click(cx.listener(move |this, _, window, cx| {
                                        this.set_field("appearance.accent", hex, window, cx);
                                        this.dirty = true;
                                        this.preview_overlay(cx);
                                        cx.notify();
                                    }))
                            }),
                        ),
                    )
                    .child(self.pair("appearance.accent", "appearance.opacity"))
                    .child(self.pair("appearance.intensity", "appearance.scale")),
            )
            .child(
                card().child(title("放在最顺眼的位置")).child(
                    row().items_start().flex_wrap().gap_5().child(grid).child(
                        column()
                            .flex_1()
                            .min_w(px(230.))
                            .gap_3()
                            .child(note(
                                "选择位置即可在桌面预览。浮框会留在鼠标所在显示器的工作区内。",
                            ))
                            .child(self.pair("appearance.offsetX", "appearance.offsetY"))
                            .child(
                                row()
                                    .flex_wrap()
                                    .gap_2()
                                    .child(button("preview-position", "预览此位置").on_click(
                                        cx.listener(|this, _, _, cx| this.preview_overlay(cx)),
                                    ))
                                    .child(primary("save-appearance", "保存外观与位置").on_click(
                                        cx.listener(|this, _, _, cx| this.save_appearance(cx)),
                                    )),
                            )
                            .child(
                                note("预览持续 10 秒。录音期间也可以保存外观与位置。")
                                    .text_size(px(11.)),
                            ),
                    ),
                ),
            )
            .child(
                card()
                    .child(title("舒适的动态反馈"))
                    .child(self.toggle(
                        "appearance.combine",
                        "柔光效果",
                        "为声浪边缘增加一层轻柔光晕。",
                        cx,
                    ))
                    .child(self.choices(
                        "appearance.motion",
                        &[("full", "完整动效"), ("reduced", "减少动态")],
                        cx,
                    ))
                    .child(
                        row()
                            .gap_3()
                            .child(note("动效帧率"))
                            .children([30u32, 60].map(|fps| {
                                button(("fps", fps as usize), format!("{fps} fps"))
                                    .when(self.draft.appearance.fps == fps, |b| {
                                        b.bg(rgb(TINT))
                                            .border_color(rgb(0x527d6e))
                                            .text_color(rgb(MINT))
                                    })
                                    .on_click(cx.listener(move |this, _, _, cx| {
                                        this.draft.appearance.fps = fps;
                                        this.dirty = true;
                                        cx.notify();
                                    }))
                            })),
                    )
                    .child(note(
                        "减少动态会冻结连续运动，保留音量反馈；强度仅改变视觉幅度。",
                    )),
            )
    }
    fn engine_panel(&self, cx: &Context<Self>) -> Div {
        let options: Vec<_> = PROVIDERS.iter().map(|p| (p.0, p.1)).collect();
        let selected = self.draft.provider.kind.clone();
        let provider_choices =
            row()
                .flex_wrap()
                .gap_3()
                .children(options.iter().map(|(kind, label)| {
                    let kind = kind.to_string();
                    let selected = kind == selected;
                    let local = matches!(kind.as_str(), "funasr" | "whispercpp");
                    let batch = matches!(kind.as_str(), "openai-compatible" | "whispercpp");
                    column()
                        .id(SharedString::from(format!("provider-{kind}")))
                        .w(relative(0.48))
                        .p_4()
                        .gap_2()
                        .rounded(px(10.))
                        .border_1()
                        .bg(rgb(if selected { TINT } else { INSET }))
                        .border_color(rgb(if selected { 0x527d6e } else { LINE }))
                        .cursor_pointer()
                        .hover(|s| s.border_color(rgb(0x527d6e)))
                        .child(
                            row()
                                .justify_between()
                                .gap_2()
                                .child(
                                    div()
                                        .flex_1()
                                        .min_w_0()
                                        .text_ellipsis()
                                        .text_size(px(12.))
                                        .text_color(rgb(if selected { MINT } else { TEXT }))
                                        .child(label.to_string()),
                                )
                                .when(selected, |r| r.child(icon(Glyph::Check, MINT, 14.))),
                        )
                        .child(
                            note(format!(
                                "{} · {}",
                                if local {
                                    "本地服务"
                                } else {
                                    "云端服务"
                                },
                                if batch {
                                    "结束后转写"
                                } else {
                                    "实时流式"
                                }
                            ))
                            .text_size(px(10.)),
                        )
                        .on_click(cx.listener(move |this, _, window, cx| {
                            if this.draft.provider.kind != kind {
                                let p = PROVIDERS.iter().find(|p| p.0 == kind).unwrap();
                                this.draft.provider.kind = kind.clone();
                                this.draft.provider.endpoint = p.2.into();
                                this.draft.provider.model = p.3.into();
                                this.draft.provider.api_key.clear();
                                this.draft.provider.api_secret.clear();
                                this.set_field("provider.endpoint", p.2, window, cx);
                                this.set_field("provider.model", p.3, window, cx);
                                this.set_field("provider.apiKey", "", window, cx);
                                this.set_field("provider.apiSecret", "", window, cx);
                                this.dirty = true;
                            }
                            cx.notify();
                        }))
                }));
        column()
            .gap_5()
            .child(
                card()
                    .child(title("声音，由谁来理解"))
                    .child(provider_choices)
                    .child(note(if self.draft.provider.is_batch() {
                        "录完后转写。边说边写需要选择流式识别服务。"
                    } else {
                        "实时流式识别。请使用该服务自己的密钥，本机模型需先启动服务。"
                    }))
                    .child(self.field("provider.endpoint"))
                    .child(self.pair("provider.model", "provider.language"))
                    .child(self.field("provider.apiKey"))
                    .when(selected == "xfyun", |card| {
                        card.child(self.pair("provider.appId", "provider.apiSecret"))
                    })
                    .child(note(
                        "密钥通过当前 Windows 账户加密保存；占位符表示已保存，清空后保存即删除。",
                    ))
                    .child(
                        row()
                            .child(
                                button("test-connection", "测试连接").on_click(
                                    cx.listener(|this, _, _, cx| this.test_connection(cx)),
                                ),
                            )
                            .child(note("测试当前表单，不录音、不保存。")),
                    )
                    .child(self.field("hotwords")),
            )
            .child(
                card()
                    .child(self.toggle(
                        "correction.enabled",
                        "识别后，再校对一次",
                        "开启后将转写文本发送给下方独立服务，修正错别字与标点。",
                        cx,
                    ))
                    .when(self.draft.correction.enabled, |card| {
                        card.child(self.field("correction.endpoint"))
                            .child(self.pair("correction.model", "correction.apiKey"))
                            .child(self.field("correction.prompt"))
                    }),
            )
    }
    fn input_panel(&self, cx: &Context<Self>) -> Div {
        column().gap_5()
            .child(card().child(self.shortcut_controls(cx))
                .child(self.field("micDevice"))
                .child(row().flex_wrap().gap_2()
                    .child(button("mic-default","系统默认").on_click(cx.listener(|this,_,window,cx|this.set_field("micDevice","",window,cx))))
                    .children(self.microphones.iter().enumerate().map(|(i,name)|{
                        let name=name.clone();button(("mic",i),name.clone()).on_click(cx.listener(move|this,_,window,cx|this.set_field("micDevice",&name,window,cx)))
                    })))
                .child(self.pair("maxRecordingSecs","silenceStopSecs"))
                .child(note("说话后安静 1–30 秒自动结束；0 关闭。嘈杂环境或长口述建议手动结束。")))
            .child(card().child(title("文字，落在光标所在"))
                .child(self.choices("injection.mode",&[("realtime","边说边写"),("stable","确认片段逐段写入"),("final","结束后统一写入"),("preview","只预览 · 手动复制")],cx))
                .when(self.draft.injection.mode!="realtime",|card|card.child(self.choices("injection.method",&[("unicode","Unicode 按键"),("clipboard","剪贴板粘贴")],cx)))
                .child(note(match self.draft.injection.mode.as_str(){
                    "realtime"=>"实时写入固定使用 Unicode 按键，只修订本次临时文本。移动光标、切换控件或手动编辑后暂停自动写入，完整结果仍可复制。",
                    "stable"=>"只写入服务已确认的片段。校对结果保留为预览，不自动改写已输入的文字。",
                    "final"=>"结束后统一写入，开启校对时写入校对后的结果。",
                    _=>"识别文字只显示在语音条中，请手动复制。",
                }))
                .child(note("实时修订遇到表情、组合字符、换行或需撤回空白时暂停，避免退格误删。终端中不自动按回车。")))
            .child(card().child(title("安静待命"))
                .child(self.toggle("autostart","开机时自动启动","登录 Windows 后仅在托盘待命，无需管理员权限。",cx))
                .child(self.toggle("silentStart","静默启动","下次手动启动也只在托盘待命，单击托盘图标打开设置。",cx))
                .child(note("关闭设置窗口会收起界面。托盘菜单可以开始录音、打开设置或退出程序。")))
    }
}

impl Settings {
    fn preview_card(&self, visual: VisualFrame, cx: &Context<Self>) -> Div {
        card()
            .gap_3()
            .p_5()
            .child(
                row()
                    .justify_between()
                    .child(
                        row()
                            .gap_2()
                            .child(icon(Glyph::Wave, MINT, 17.))
                            .child(title("声音的形状")),
                    )
                    .child(
                        button(
                            "demo",
                            if self.demo_until.is_some() {
                                "正在演示…"
                            } else {
                                "播放动效"
                            },
                        )
                        .child(icon(Glyph::Play, MUTED, 12.))
                        .on_click(cx.listener(|this, _, _, cx| {
                            this.demo_until = Some(Instant::now() + Duration::from_secs(10));
                            cx.notify();
                        })),
                    ),
            )
            .child(
                div()
                    .h(px(118.))
                    .rounded(px(12.))
                    .bg(rgb(INSET))
                    .flex()
                    .items_center()
                    .justify_center()
                    .child(
                        waveform(self.appearance(cx), visual)
                            .w(relative(0.88))
                            .h(px(80.)),
                    ),
            )
            .child(
                row()
                    .justify_between()
                    .flex_wrap()
                    .gap_2()
                    .child(note("真实录音时跟随音量起伏；演示包含说话与停顿。"))
                    .child(
                        button("float-preview", "预览桌面浮框")
                            .on_click(cx.listener(|this, _, _, cx| this.preview_overlay(cx))),
                    ),
            )
    }
}
impl Render for Settings {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let status = self.runtime.status.lock().unwrap().clone();
        let wide = window.viewport_size().width >= px(1050.);
        let demo_time = self.demo_until.map(|until| {
            10.0 - until
                .saturating_duration_since(Instant::now())
                .as_secs_f32()
        });
        let visual = self.voice.frame(
            &self.runtime,
            self.draft.appearance.motion == "reduced",
            demo_time,
        );
        let panel = match self.tab {
            0 => self.overview(visual.clone(), wide, cx),
            1 => self.appearance_panel(cx),
            2 => self.engine_panel(cx),
            _ => self.input_panel(cx),
        };
        let descriptions = [
            "把声音变成文字，让注意力留在想法上。",
            "调出属于你的色彩、节奏与桌面位置。",
            "连接你信任的识别服务，保持表达流畅。",
            "设置顺手的快捷键，让 VoiceBar 随时就位。",
        ];
        let navigation = [
            (Glyph::Home, "记录与预览"),
            (Glyph::Wave, "浮框、颜色与动效"),
            (Glyph::Engine, "服务连接与文本校对"),
            (Glyph::Keyboard, "快捷键与桌面行为"),
        ];
        let content = row()
            .flex_1()
            .min_h_0()
            .w_full()
            .gap_0()
            .items_start()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .text_size(px(14.))
            .child(
                column()
                    .w(px(if wide { 214. } else { 190. }))
                    .flex_shrink_0()
                    .h_full()
                    .gap_1()
                    .px_4()
                    .py_5()
                    .bg(rgb(SIDEBAR))
                    .border_r_1()
                    .border_color(rgb(LINE))
                    .child(
                        row()
                            .px_2()
                            .gap_3()
                            .child(
                                div()
                                    .size(px(36.))
                                    .rounded(px(11.))
                                    .bg(rgb(TINT))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(Glyph::Wave, MINT, 23.)),
                            )
                            .child(
                                column()
                                    .gap_0()
                                    .child(
                                        div()
                                            .text_size(px(17.))
                                            .font_weight(FontWeight::SEMIBOLD)
                                            .child("VoiceBar"),
                                    )
                                    .child(note("你的桌面语音伙伴").text_size(px(10.))),
                            ),
                    )
                    .child(eyebrow("工作空间").px_3().mt_8().mb_3())
                    .children(TABS.iter().enumerate().map(|(i, label)| {
                        let selected = self.tab == i;
                        row()
                            .id(("tab", i))
                            .w_full()
                            .px_3()
                            .py_3()
                            .gap_3()
                            .rounded(px(10.))
                            .cursor_pointer()
                            .text_color(rgb(if selected { MINT } else { MUTED }))
                            .when(selected, |s| s.bg(rgb(0x25332f)))
                            .hover(|s| s.bg(rgb(0x222a31)))
                            .child(icon(
                                navigation[i].0,
                                if selected { MINT } else { MUTED },
                                19.,
                            ))
                            .child(
                                column()
                                    .flex_1()
                                    .gap_1()
                                    .child(
                                        div()
                                            .text_size(px(13.))
                                            .font_weight(FontWeight::MEDIUM)
                                            .child(label.to_string()),
                                    )
                                    .child(note(navigation[i].1).text_size(px(10.)).text_color(
                                        rgb(if selected { 0xa0b9b0 } else { 0x788394 }),
                                    )),
                            )
                            .on_click(cx.listener(move |this, _, _, cx| {
                                this.cancel_shortcut(cx);
                                this.tab = i;
                                cx.notify();
                            }))
                    }))
                    .child(div().flex_1())
                    .child(
                        column()
                            .p_3()
                            .gap_2()
                            .rounded(px(10.))
                            .border_1()
                            .border_color(rgb(LINE))
                            .bg(rgb(0x1b2028))
                            .child(note("随时唤起").text_size(px(11.)))
                            .child(keycaps(&self.runtime.config.lock().unwrap().hotkey))
                            .child(note("在输入框内开始说话").text_size(px(10.))),
                    )
                    .child(
                        row()
                            .px_2()
                            .mt_4()
                            .justify_between()
                            .child(
                                note(format!("v{}", env!("CARGO_PKG_VERSION"))).text_size(px(10.)),
                            )
                            .child(note("桌面版").text_size(px(10.))),
                    ),
            )
            .child(
                column()
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .gap_0()
                    .child(
                        row()
                            .px_6()
                            .h(px(62.))
                            .flex_shrink_0()
                            .justify_between()
                            .border_b_1()
                            .border_color(rgb(LINE))
                            .child(
                                row()
                                    .gap_2()
                                    .child(note("VoiceBar"))
                                    .child(note("/"))
                                    .child(div().text_size(px(12.)).child(TABS[self.tab])),
                            )
                            .child(
                                row()
                                    .gap_3()
                                    .child(if self.dirty {
                                        badge("有未保存的更改", WARNING)
                                    } else {
                                        badge("已同步", MINT)
                                    })
                                    .child(
                                        primary("save", "保存更改")
                                            .child(icon(Glyph::Check, 0x10251d, 14.))
                                            .on_click(cx.listener(|this, _, _, cx| this.save(cx))),
                                    ),
                            ),
                    )
                    .child(
                        column()
                            .id(("page-scroll", self.tab))
                            .overflow_y_scroll()
                            .flex_1()
                            .min_h_0()
                            .px(px(if wide { 32. } else { 24. }))
                            .py_6()
                            .gap_5()
                            .child(
                                row()
                                    .justify_between()
                                    .items_start()
                                    .gap_4()
                                    .child(
                                        column()
                                            .flex_1()
                                            .gap_2()
                                            .child(eyebrow(
                                                [
                                                    "YOUR VOICE, YOUR FLOW",
                                                    "MAKE IT YOURS",
                                                    "CONNECTED TO YOUR VOICE",
                                                    "ALWAYS WITHIN REACH",
                                                ][self.tab],
                                            ))
                                            .child(
                                                div()
                                                    .text_size(px(28.))
                                                    .line_height(px(38.))
                                                    .font_weight(FontWeight::SEMIBOLD)
                                                    .child(TABS[self.tab]),
                                            )
                                            .child(note(descriptions[self.tab])),
                                    )
                                    .when(self.tab == 0, |r| {
                                        r.child(
                                            button("overview-preview", "预览浮框")
                                                .bg(rgb(PANEL))
                                                .child(icon(Glyph::Wave, MUTED, 15.))
                                                .on_click(cx.listener(|this, _, _, cx| {
                                                    this.preview_overlay(cx)
                                                })),
                                        )
                                    }),
                            )
                            .when(self.tab == 1, |c| c.child(self.preview_card(visual, cx)))
                            .child(panel)
                            .child(div().h(px(8.))),
                    )
                    .child(
                        row()
                            .px_6()
                            .py_3()
                            .gap_3()
                            .flex_shrink_0()
                            .bg(rgb(SIDEBAR))
                            .border_t_1()
                            .border_color(rgb(LINE))
                            .child(div().size(px(5.)).flex_shrink_0().rounded_full().bg(rgb(
                                if self.job.is_some() {
                                    WARNING
                                } else {
                                    state_color(&status.state)
                                },
                            )))
                            .child(
                                note(self.message.clone())
                                    .flex_1()
                                    .min_w_0()
                                    .text_size(px(11.)),
                            ),
                    ),
            );
        column()
            .size_full()
            .gap_0()
            .overflow_hidden()
            .bg(rgb(BG))
            .text_color(rgb(TEXT))
            .font_family("Microsoft YaHei UI")
            .child(settings_titlebar(window))
            .child(content)
    }
}

fn waveform(appearance: Appearance, frame: VisualFrame) -> Canvas<()> {
    canvas(
        |_, _, _| (),
        move |bounds, _, window, _| {
            let color =
                u32::from_str_radix(appearance.accent.trim_start_matches('#'), 16).unwrap_or(MINT);
            for stroke in voice_shape::paths(
                &appearance.effect,
                &frame,
                f32::from(bounds.size.width),
                f32::from(bounds.size.height),
                appearance.intensity as f32,
                appearance.combine,
            ) {
                let mut path = PathBuilder::stroke(px(stroke.width));
                for (i, (x, y)) in stroke.points.into_iter().enumerate() {
                    let p = bounds.origin + point(px(x), px(y));
                    if i == 0 {
                        path.move_to(p);
                    } else {
                        path.line_to(p);
                    }
                }
                let mut ink = rgb(color);
                ink.a = stroke.alpha;
                if let Ok(path) = path.build() {
                    window.paint_path(path, ink);
                }
            }
        },
    )
}

/// Both preview and desktop use the same analysis and elapsed-time envelopes.
struct VoiceMotion {
    motion: Motion,
    tick: Instant,
    session_id: u64,
    previous_state: String,
    pulse_id: u64,
    utterance: u64,
    moving: bool,
}
impl Default for VoiceMotion {
    fn default() -> Self {
        Self {
            motion: Motion::default(),
            tick: Instant::now(),
            session_id: 0,
            previous_state: String::new(),
            pulse_id: 0,
            utterance: 0,
            moving: false,
        }
    }
}
impl VoiceMotion {
    fn frame(&mut self, runtime: &Runtime, reduced: bool, demo: Option<f32>) -> VisualFrame {
        let now = Instant::now();
        let dt = now.duration_since(self.tick).as_secs_f32();
        self.tick = now;
        let state = runtime.status.lock().unwrap().state.clone();
        let level = runtime.level.lock().unwrap().clone();
        let utterance = runtime.transcript.lock().unwrap().utterance;
        if self.session_id != level.session_id {
            self.motion = Motion::default();
            self.session_id = level.session_id;
            self.utterance = utterance;
            self.previous_state.clear();
        }
        let mut audio = if state == "listening"
            && level
                .updated_at
                .is_some_and(|t| now.duration_since(t) < Duration::from_millis(250))
        {
            level.audio
        } else {
            AudioFrame::default()
        };
        if utterance != self.utterance
            || (state == "processing" && self.previous_state == "listening")
        {
            self.pulse_id = self.pulse_id.wrapping_add(1);
        }
        self.utterance = utterance;
        self.previous_state = state.clone();
        if let Some(t) = demo {
            // Explicit, labelled preview: phrase / short gap / quiet / resume.
            let cycle = t.rem_euclid(4.0);
            let loudness = if cycle < 1.2 {
                0.75
            } else if cycle < 1.5 {
                0.0
            } else if cycle < 2.3 {
                0.30
            } else if cycle < 3.5 {
                0.0
            } else {
                0.9
            };
            audio = AudioFrame {
                loudness,
                speaking: loudness > 0.0,
                bands: (0..BAND_COUNT)
                    .map(|i| loudness * (0.30 + 0.70 * (i as f32 * 0.52 + t * 1.8).sin().abs()))
                    .collect(),
                ..AudioFrame::default()
            };
        }
        let visual = self.motion.advance(
            &audio,
            dt,
            demo.is_some() || matches!(state.as_str(), "listening" | "processing" | "done"),
            reduced,
            self.pulse_id,
        );
        self.moving = visual.amplitude > 0.0 || visual.width > 0.3501 || visual.pulse > 0.001;
        visual
    }
}

#[cfg(test)]
mod visual_tests {
    use super::{AppConfig, AudioFrame, Duration, Instant, Runtime, VoiceMotion, BAND_COUNT};
    use core::prelude::v1::test;
    fn runtime() -> Runtime {
        Runtime::new(AppConfig::default(), std::path::PathBuf::new())
    }
    #[test]
    fn stale_audio_settles_and_a_new_session_starts_at_rest() {
        let runtime = runtime();
        runtime.set_status("listening", "");
        {
            let mut level = runtime.level.lock().unwrap();
            level.session_id = 1;
            level.updated_at = Some(Instant::now());
            level.audio = AudioFrame {
                loudness: 0.9,
                speaking: true,
                bands: vec![0.9; BAND_COUNT],
                ..AudioFrame::default()
            };
        }
        let mut motion = VoiceMotion::default();
        for _ in 0..10 {
            motion.tick = Instant::now() - Duration::from_millis(40);
            motion.frame(&runtime, false, None);
        }
        runtime.level.lock().unwrap().updated_at =
            Some(Instant::now() - Duration::from_millis(300));
        let mut frame = motion.frame(&runtime, false, None);
        for _ in 0..30 {
            motion.tick = Instant::now() - Duration::from_millis(40);
            frame = motion.frame(&runtime, false, None);
        }
        assert_eq!(frame.amplitude, 0.0);
        assert_eq!(frame.width, 0.35);
        runtime.level.lock().unwrap().session_id = 2;
        frame = motion.frame(&runtime, false, None);
        assert_eq!(frame.width, 0.35);
        assert_eq!(frame.pulse, 0.0);
    }
    #[test]
    fn confirmation_pulses_do_not_follow_partial_revisions_or_duplicates() {
        let runtime = runtime();
        runtime.set_transcript("", "你", false);
        runtime.set_transcript("", "你好", false);
        assert_eq!(runtime.transcript.lock().unwrap().utterance, 0);
        runtime.set_transcript("你好。", "", false);
        assert_eq!(runtime.transcript.lock().unwrap().utterance, 1);
        runtime.set_transcript("你好。", "下一", false);
        runtime.set_transcript("你好。", "下一句", false);
        assert_eq!(runtime.transcript.lock().unwrap().utterance, 1);
        runtime.set_transcript("你好。", "", false);
        assert_eq!(runtime.transcript.lock().unwrap().utterance, 1);
        runtime.set_transcript("你好。下一句。", "", false);
        assert_eq!(runtime.transcript.lock().unwrap().utterance, 2);
        runtime.set_transcript("你好！下一句。", "", true);
        assert_eq!(runtime.transcript.lock().unwrap().utterance, 2);
        runtime.set_transcript("", "", false);
        assert_eq!(runtime.transcript.lock().unwrap().utterance, 0);
    }
}

struct Overlay {
    runtime: Arc<Runtime>,
    voice: VoiceMotion,
    listening_since: Option<Instant>,
    last_snapshot: String,
    #[cfg(test)]
    layout: Arc<std::sync::Mutex<BTreeMap<&'static str, Vec<Bounds<Pixels>>>>>,
}
impl Overlay {
    fn probe(&self, _name: &'static str, element: Div) -> Div {
        #[cfg(test)]
        {
            let layout = self.layout.clone();
            element.on_children_prepainted(move |bounds, _, _| {
                layout.lock().unwrap().insert(_name, bounds);
            })
        }
        #[cfg(not(test))]
        {
            element
        }
    }
    fn tick(&mut self, cx: &mut Context<Self>) {
        let listening = self.runtime.status.lock().unwrap().state == "listening";
        if listening && self.listening_since.is_none() {
            self.listening_since = self
                .runtime
                .level
                .lock()
                .unwrap()
                .started_at
                .or_else(|| Some(Instant::now()));
        }
        if !listening {
            self.listening_since = None;
        }
        let status = self.runtime.status.lock().unwrap().clone();
        let transcript = self.runtime.transcript.lock().unwrap().clone();
        let snapshot = format!(
            "{}{}{}{}{}",
            status.state,
            status.message,
            transcript.stable,
            transcript.partial,
            transcript.utterance
        );
        if listening
            || self.runtime.overlay_preview().is_some()
            || self.voice.moving
            || snapshot != self.last_snapshot
        {
            self.last_snapshot = snapshot;
            cx.notify();
        }
    }
}
impl Render for Overlay {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut config = self.runtime.config.lock().unwrap().clone();
        let preview = self.runtime.overlay_preview();
        let active = self.runtime.active.lock().unwrap().is_some();
        let preview_only = preview.is_some() && !active;
        if let Some(preview) = &preview {
            config.appearance = preview.appearance.clone();
        }
        let status = self.runtime.status.lock().unwrap().clone();
        let transcript = self.runtime.transcript.lock().unwrap().clone();
        let demo = preview
            .as_ref()
            .filter(|_| preview_only)
            .map(|p| p.started_at.elapsed().as_secs_f32());
        let visual = self
            .voice
            .frame(&self.runtime, config.appearance.motion == "reduced", demo);
        let text = if preview_only {
            "让表达自然流动，文字即刻呈现。".to_string()
        } else {
            format!("{}{}", transcript.stable, transcript.partial)
        };
        let seconds = self
            .listening_since
            .map(|s| s.elapsed().as_secs())
            .unwrap_or(0);
        // Appearance.scale changes the native viewport. Scale every dimension too,
        // so 65% and high-DPI bars keep the same proportions and usable controls.
        let viewport = window.viewport_size();
        let scale = (f32::from(viewport.width) / 560.)
            .min(f32::from(viewport.height) / 210.)
            .clamp(0.1, 1.8);
        let color = if preview_only {
            MINT
        } else {
            state_color(&status.state)
        };
        let alpha = (config.appearance.opacity.clamp(0.3, 1.) * 255.) as u32;
        let label = if preview_only {
            "外观预览"
        } else {
            state_label(&status.state)
        };
        column()
            .size_full()
            .p(px(10. * scale))
            .gap_0()
            .overflow_hidden()
            .text_color(rgb(TEXT))
            .font_family("Microsoft YaHei UI")
            .child(
                self.probe("card", column())
                    .id("overlay-card")
                    .size_full()
                    .min_h_0()
                    .gap(px(10. * scale))
                    .p(px(14. * scale))
                    .rounded(px(20. * scale))
                    .border_1()
                    .border_color(rgba(0xb4c6d039))
                    .bg(rgba((PANEL << 8) | alpha))
                    .when(config.appearance.combine, |c| c.shadow_lg())
                    .overflow_hidden()
                    .child(
                        self.probe("header", row())
                            .h(px(32. * scale))
                            .gap(px(9. * scale))
                            .flex_shrink_0()
                            .child(
                                div()
                                    .size(px(30. * scale))
                                    .flex_shrink_0()
                                    .rounded_full()
                                    .bg(rgba((color << 8) | 24))
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .child(icon(Glyph::Mic, color, 16. * scale)),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .text_color(rgb(color))
                                    .text_size(px(11. * scale))
                                    .whitespace_nowrap()
                                    .child(label),
                            )
                            .child(
                                waveform(config.appearance.clone(), visual)
                                    .flex_1()
                                    .min_w_0()
                                    .h(px(28. * scale)),
                            )
                            .child(
                                div()
                                    .flex_shrink_0()
                                    .font_family("Consolas")
                                    .text_color(rgb(MUTED))
                                    .text_size(px(11. * scale))
                                    .child(format!("{:02}:{:02}", seconds / 60, seconds % 60)),
                            )
                            .child(
                                div()
                                    .id("overlay-stop")
                                    .flex_shrink_0()
                                    .h(px(30. * scale))
                                    .px(px(10. * scale))
                                    .flex()
                                    .items_center()
                                    .gap(px(5. * scale))
                                    .rounded(px(9. * scale))
                                    .bg(rgba((color << 8) | 28))
                                    .text_color(rgb(color))
                                    .text_size(px(11. * scale))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgba(0xffffff20)))
                                    .child(icon(
                                        if active { Glyph::Stop } else { Glyph::Check },
                                        color,
                                        12. * scale,
                                    ))
                                    .child(if active { "停止" } else { "收起" })
                                    .on_click(cx.listener(|this, _, _, _| {
                                        if this.runtime.active.lock().unwrap().is_some() {
                                            this.runtime.stop(false);
                                        } else {
                                            this.runtime.dismiss_overlay();
                                        }
                                    })),
                            )
                            .child(
                                icon_button("overlay-close", Glyph::Close, MUTED, 28. * scale)
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.runtime.stop(true);
                                        this.runtime.dismiss_overlay();
                                    })),
                            ),
                    )
                    .child(
                        div()
                            .id("caption")
                            .flex_1()
                            .min_w_0()
                            .min_h_0()
                            .w_full()
                            .overflow_y_scroll()
                            .text_size(px(15. * scale))
                            .line_height(px(24. * scale))
                            .when(text.is_empty(), |s| s.text_color(rgb(MUTED)))
                            .child(if text.is_empty() {
                                "把想说的话，变成眼前的文字。".to_string()
                            } else {
                                text
                            }),
                    )
                    .child(
                        self.probe("footer", row())
                            .h(px(30. * scale))
                            .flex_shrink_0()
                            .gap(px(8. * scale))
                            .child(
                                div()
                                    .id("overlay-status")
                                    .flex_1()
                                    .min_w_0()
                                    .max_h(px(30. * scale))
                                    .overflow_y_scroll()
                                    .text_size(px(10. * scale))
                                    .line_height(px(15. * scale))
                                    .text_color(rgb(MUTED))
                                    .child(if preview_only {
                                        "10 秒后自动收起 · 不会启动录音".to_string()
                                    } else {
                                        status.message
                                    }),
                            )
                            .child(
                                div()
                                    .id("overlay-copy")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink_0()
                                    .h(px(28. * scale))
                                    .gap(px(5. * scale))
                                    .px(px(9. * scale))
                                    .rounded(px(8. * scale))
                                    .text_size(px(11. * scale))
                                    .cursor_pointer()
                                    .bg(rgba(0xffffff0c))
                                    .hover(|s| s.bg(rgba(0xffffff1c)))
                                    .child(icon(Glyph::Copy, TEXT, 13. * scale))
                                    .child("复制")
                                    .on_click(cx.listener(|this, _, _, cx| {
                                        if let Err(error) = this.runtime.copy_transcript() {
                                            this.runtime.set_status("error", error);
                                        }
                                        cx.notify();
                                    })),
                            )
                            .child(
                                div()
                                    .id("overlay-settings")
                                    .flex()
                                    .items_center()
                                    .justify_center()
                                    .flex_shrink_0()
                                    .h(px(28. * scale))
                                    .gap(px(5. * scale))
                                    .px(px(9. * scale))
                                    .rounded(px(8. * scale))
                                    .text_size(px(11. * scale))
                                    .cursor_pointer()
                                    .hover(|s| s.bg(rgba(0xffffff1c)))
                                    .child(icon(Glyph::Settings, MUTED, 13. * scale))
                                    .child("设置")
                                    .on_click(cx.listener(|this, _, _, _| {
                                        this.runtime
                                            .settings_requested
                                            .store(true, Ordering::SeqCst);
                                    })),
                            ),
                    ),
            )
    }
}

fn open_overlay(runtime: Arc<Runtime>, cx: &mut App) -> anyhow::Result<WindowHandle<Overlay>> {
    cx.open_window(
        WindowOptions {
            window_bounds: Some(WindowBounds::Windowed(Bounds {
                origin: point(px(100.), px(40.)),
                size: size(px(560.), px(210.)),
            })),
            titlebar: Some(TitlebarOptions {
                title: Some("VoiceBar · 语音条".into()),
                appears_transparent: true,
                ..Default::default()
            }),
            kind: WindowKind::PopUp,
            focus: false,
            show: false,
            is_resizable: false,
            is_minimizable: false,
            is_movable: false,
            window_background: WindowBackgroundAppearance::Transparent,
            app_id: Some("studio.voicebar.desktop".into()),
            ..Default::default()
        },
        move |_, cx| {
            cx.new(|_| Overlay {
                runtime,
                voice: VoiceMotion::default(),
                listening_since: None,
                last_snapshot: String::new(),
                #[cfg(test)]
                layout: Default::default(),
            })
        },
    )
}

#[cfg(all(test, windows))]
#[path = "ui/native_tests.rs"]
mod native_overlay_tests;

#[cfg(all(test, windows))]
#[path = "ui/shortcut_tests.rs"]
mod native_shortcut_tests;

/// Both native windows share the same state. Hidden windows never request
/// frames; microphone events are sampled only while the overlay is visible.
pub fn run(runtime: Arc<Runtime>, desktop: Desktop, hidden: bool) {
    Application::new().run(move |cx: &mut App| {
        init_theme(cx);
        let desktop = Rc::new(desktop);
        let result = (|| -> anyhow::Result<()> {
            let settings_slot = Rc::new(std::cell::RefCell::new(None));
            let slot = settings_slot.clone();
            let state = runtime.clone();
            let system = desktop.clone();
            let settings = cx.open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::centered(size(px(1160.), px(840.)), cx)),
                    titlebar: Some(TitlebarOptions {
                        title: Some("VoiceBar · 让思绪自然成文".into()),
                        appears_transparent: true,
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(880.), px(650.))),
                    app_id: Some("studio.voicebar.desktop".into()),
                    focus: !hidden,
                    show: !hidden,
                    ..Default::default()
                },
                move |window, cx| {
                    let view = cx.new(|cx| Settings::new(state, system, window, cx));
                    *slot.borrow_mut() = Some(view.clone());
                    let weak = view.downgrade();
                    window.on_window_should_close(cx, move |window, cx| {
                        let _ = weak.update(cx, |view, cx| view.cancel_shortcut(cx));
                        desktop::hide_window(native_handle(window));
                        false
                    });
                    cx.new(|cx| Root::new(view, window, cx))
                },
            )?;
            let settings_view = settings_slot.borrow_mut().take().unwrap();
            let overlay = open_overlay(runtime.clone(), cx)?;
            let settings_hwnd = settings.update(cx, |_, window, _| native_handle(window))?;
            let overlay_hwnd = overlay.update(cx, |_, window, _| native_handle(window))?;
            desktop::set_window_icon(settings_hwnd);
            if !hidden {
                desktop::show_window(settings_hwnd, true);
            }
            if runtime.config.lock().unwrap().update_check == "auto" && updater::can_self_update() {
                let state = runtime.clone();
                runtime.executor.spawn(async move {
                    if let Ok(info) = updater::check_for_update().await {
                        if info.available {
                            state.set_status(
                                "idle",
                                format!(
                                    "发现新版本 {}，可在概览中检查并安装。",
                                    info.version.as_deref().unwrap_or("")
                                ),
                            );
                        }
                    }
                });
            }
            let state = runtime.clone();
            cx.spawn(async move |cx| {
                let mut overlay_shown = false;
                let mut applied_appearance = None;
                let mut applied_preview = None;
                loop {
                    let preview = state.overlay_preview();
                    let visible = state.overlay_visible.load(Ordering::SeqCst) || preview.is_some();
                    let mut config = state.config.lock().unwrap().clone();
                    if let Some(preview) = &preview {
                        config.appearance = preview.appearance.clone();
                    }
                    let fps = if visible || desktop::is_visible(settings_hwnd) {
                        config.appearance.fps.clamp(30, 60)
                    } else {
                        20
                    };
                    cx.background_executor()
                        .timer(Duration::from_millis(1000 / fps as u64))
                        .await;
                    if state.quit_requested.load(Ordering::SeqCst) {
                        state.stop(true);
                        let _ = cx.update(|cx| cx.quit());
                        break;
                    }
                    // These Win32 calls synchronously dispatch resize/paint callbacks.
                    // Keep them outside App updates so GPUI can borrow the app and
                    // update the overlay's viewport and compositor during those callbacks.
                    if state.settings_requested.swap(false, Ordering::SeqCst) {
                        desktop::show_window(settings_hwnd, true);
                    }
                    let preview_id = preview.as_ref().map(|p| p.started_at);
                    let reconfigure = visible
                        && (!overlay_shown
                            || applied_appearance.as_ref() != Some(&config.appearance)
                            || applied_preview != preview_id);
                    let mut display_error = None;
                    if reconfigure {
                        match desktop::configure_overlay(overlay_hwnd, &config) {
                            Ok(()) => {
                                applied_appearance = Some(config.appearance);
                                applied_preview = preview_id;
                            }
                            Err(error) => display_error = Some(error),
                        }
                    }
                    if visible && !overlay_shown && display_error.is_none() {
                        desktop::show_window(overlay_hwnd, false);
                        overlay_shown = desktop::is_visible(overlay_hwnd);
                    } else if !visible && overlay_shown {
                        desktop::hide_window(overlay_hwnd);
                        overlay_shown = false;
                        applied_appearance = None;
                    }
                    let result = cx.update(|cx| {
                        if let Some(error) = display_error {
                            settings_view.update(cx, |view, cx| {
                                view.message = format!("语音条显示失败：{error}");
                                cx.notify();
                            });
                        }
                        settings_view.update(cx, |view, cx| view.tick(cx));
                        if overlay_shown {
                            let _ = overlay.update(cx, |view, window, cx| {
                                view.tick(cx);
                                if reconfigure {
                                    window.refresh();
                                }
                            });
                        }
                    });
                    if result.is_err() {
                        break;
                    }
                }
                drop(desktop);
            })
            .detach();
            Ok(())
        })();
        if let Err(error) = result {
            crate::startup_error(&format!("原生界面初始化失败：{error}"));
            cx.quit();
        }
    });
}
