use crate::{audio, config::AppConfig, platform, providers, runtime::Runtime};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

pub struct InputGuard(pub platform::InputTarget);

pub enum Command {
    Finish,
    Cancel,
    Resume(InputGuard, crate::runtime::Transcript),
}
impl Drop for InputGuard {
    fn drop(&mut self) {
        self.0.release();
    }
}

struct Delivery {
    target: InputGuard,
    protected: String,
    resumed: bool,
    delivered: String,
    paused: bool,
    internal: bool,
}
impl Delivery {
    fn new(target: InputGuard) -> Self {
        let internal = target.0.process_id() == std::process::id();
        Self {
            target,
            protected: String::new(),
            resumed: false,
            delivered: String::new(),
            paused: false,
            internal,
        }
    }
    fn resume(&mut self, target: InputGuard, observed: &crate::runtime::Transcript, mode: &str) {
        self.target = target;
        self.paused = false;
        self.resumed = true;
        self.delivered.clear();
        // Only text injected at the newly selected caret may be revised. A
        // final-only session has not yet injected anything.
        self.protected = match mode {
            "realtime" => format!("{}{}", observed.stable, observed.partial),
            "stable" => observed.stable.clone(),
            _ => String::new(),
        };
    }

    fn writable_suffix<'a>(&mut self, text: &'a str) -> Option<&'a str> {
        if let Some(suffix) = text.strip_prefix(&self.protected) {
            return Some(suffix);
        }
        // The recognizer rewrote text from before the resume boundary. Seal
        // this revision and continue on its next extension; the full revision
        // remains available in the overlay.
        if !self.protected.starts_with(text) {
            self.protected = text.into();
            self.delivered.clear();
        }
        None
    }

    async fn append(&mut self, text: &str, method: &str) -> Result<(), String> {
        let Some(text) = self.writable_suffix(text) else {
            return Ok(());
        };
        if self.internal || self.paused || text == self.delivered {
            return Ok(());
        }
        // Providers can revise a final segment. Never backspace into user work.
        let Some(suffix) = text.strip_prefix(&self.delivered) else {
            self.paused = true;
            return Err("识别引擎修订了已确认片段，自动写入暂停；请复制完整结果。".into());
        };
        if suffix.is_empty() {
            return Ok(());
        }
        if !self.target.0.valid() {
            self.paused = true;
            return Err("输入焦点已切换，自动写入暂停；文本保留在语音条中。".into());
        }
        // Hotkey modifiers may still be down when the first result arrives.
        for _ in 0..20 {
            if platform::modifiers_released() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let target = self.target.0;
        let fragment = suffix.to_string();
        let clipboard = method == "clipboard";
        let result = tokio::task::spawn_blocking(move || {
            if clipboard {
                platform::paste_text(&target, &fragment)
            } else {
                platform::type_text(&target, &fragment)
            }
        })
        .await
        .unwrap_or_else(|e| Err(format!("输入任务中断，自动写入暂停；请复制完整结果：{e}")));
        if let Err(e) = result {
            self.paused = true;
            return Err(e);
        }
        self.delivered = text.into();
        Ok(())
    }
    async fn replace(&mut self, text: &str, method: &str) -> Result<(), String> {
        let Some(text) = self.writable_suffix(text) else {
            return Ok(());
        };
        if self.internal || self.paused || text == self.delivered {
            return Ok(());
        }
        let edit = tail_edit(&self.delivered, text);
        let remove = edit.remove.to_string();
        let insert = edit.insert.to_string();
        // Wait for the invoking shortcut to be released before any key input.
        for _ in 0..20 {
            if platform::modifiers_released() {
                break;
            }
            tokio::time::sleep(Duration::from_millis(25)).await;
        }
        let target = self.target.0;
        let method = method.to_string();
        let result = tokio::task::spawn_blocking(move || {
            platform::realtime_edit(&target, &remove, &insert, &method)
        })
        .await
        .unwrap_or_else(|e| Err(format!("输入任务中断，自动写入暂停；请复制完整结果：{e}")));
        if let Err(error) = result {
            self.paused = true;
            return Err(error);
        }
        self.delivered = text.into();
        Ok(())
    }
}

// The common prefix stays in place. Only the tail this session inserted is
// eligible for replacement; byte offsets must always be UTF-8 boundaries.
struct TailEdit<'a> {
    remove: &'a str,
    insert: &'a str,
}
fn tail_edit<'a>(old: &'a str, new: &'a str) -> TailEdit<'a> {
    let prefix = old
        .chars()
        .zip(new.chars())
        .take_while(|(a, b)| a == b)
        .map(|(ch, _)| ch.len_utf8())
        .sum::<usize>();
    TailEdit {
        remove: &old[prefix..],
        insert: &new[prefix..],
    }
}

pub async fn run(
    runtime: Arc<Runtime>,
    config: AppConfig,
    target: platform::InputTarget,
    id: u64,
    mut stop: mpsc::UnboundedReceiver<Command>,
) {
    let result = execute(&runtime, &config, InputGuard(target), &mut stop).await;
    let auto_hide = matches!(
        &result,
        Ok(Outcome::Cancelled) | Ok(Outcome::Done { keep: false, .. })
    );
    {
        // Publish completion before another start can claim the active slot.
        let mut active = runtime.active.lock().unwrap();
        if active.as_ref().map(|v| v.id) != Some(id) {
            return;
        }
        match result {
            Ok(Outcome::Cancelled) => {
                runtime.set_status("idle", "已取消；已经写入的文字会保留");
                runtime.set_transcript("", "", false);
            }
            Ok(Outcome::Done { message, .. }) => runtime.set_status("done", message),
            Err(e) => runtime.set_status("error", e),
        }
        // Preserve session identity for the renderer while removing stale audio.
        let mut level = runtime.level.lock().unwrap();
        level.audio = Default::default();
        level.updated_at = None;
        active.take();
    }
    // Successful sessions disappear after a short acknowledgement; errors stay
    // visible so the user can recover the transcript without losing work.
    if auto_hide {
        tokio::time::sleep(Duration::from_millis(2200)).await;
        // Holding active through the visibility update prevents an older
        // session's timer from hiding a newly started recording.
        let active = runtime.active.lock().unwrap();
        let state = runtime.status.lock().unwrap().state.clone();
        if active.is_none()
            && runtime.counter.load(std::sync::atomic::Ordering::SeqCst) == id
            && ["done", "idle"].contains(&state.as_str())
        {
            runtime
                .overlay_visible
                .store(false, std::sync::atomic::Ordering::SeqCst);
            let hotkey = runtime.config.lock().unwrap().hotkey.clone();
            runtime.set_status("idle", format!("就绪 · {hotkey} 开始"));
        }
    }
}
enum Outcome {
    Cancelled,
    Done { message: String, keep: bool },
}

async fn execute(
    runtime: &Arc<Runtime>,
    config: &AppConfig,
    target: InputGuard,
    stop: &mut mpsc::UnboundedReceiver<Command>,
) -> Result<Outcome, String> {
    let (audio_tx, audio_rx) = mpsc::channel(256);
    let (output_tx, mut output_rx) = mpsc::unbounded_channel();
    let (errors_tx, mut errors_rx) = mpsc::unbounded_channel();
    let device = config.mic_device.clone();
    let capture_runtime = runtime.clone();
    let silence_stop_secs = config.silence_stop_secs;
    let (silence_end, mut silence_rx) = tokio::sync::oneshot::channel();
    let mut capture = tokio::task::spawn_blocking(move || {
        audio::Capture::start(
            device,
            audio_tx,
            errors_tx,
            capture_runtime,
            silence_stop_secs,
            silence_end,
        )
    })
    .await
    .map_err(|e| e.to_string())??;
    let provider_config = config.provider.clone();
    let hotwords = config.hotwords.clone();
    let mut provider = tokio::spawn(providers::run(
        provider_config,
        hotwords,
        audio_rx,
        output_tx,
    ));
    // Aborting this task also closes provider resources on cancellation.
    struct AbortOnDrop(tokio::task::AbortHandle);
    impl Drop for AbortOnDrop {
        fn drop(&mut self) {
            self.0.abort();
        }
    }
    let _provider_guard = AbortOnDrop(provider.abort_handle());
    let mut delivery = Delivery::new(target);
    let mut current = providers::Transcript {
        stable: String::new(),
        partial: String::new(),
    };
    runtime.set_status(
        "listening",
        if delivery.internal {
            "正在录音 · 设置窗口内仅预览，不向配置表单输入"
        } else {
            "正在录音 · 等待识别服务响应"
        },
    );
    let limit = if config.provider.kind == "xfyun" {
        config.max_recording_secs.min(58)
    } else {
        config.max_recording_secs
    };
    let max = tokio::time::sleep(Duration::from_secs(limit));
    tokio::pin!(max);
    let mut focus = tokio::time::interval(Duration::from_millis(100));
    let mut finished = false;
    loop {
        tokio::select! {
            command=stop.recv()=>match command {
                Some(Command::Resume(target, observed)) => {
                    delivery.resume(target, &observed, &config.injection.mode);
                    // A transcript may have arrived after the button/shortcut
                    // but before this command was consumed by the session.
                    deliver_update(runtime, config, &mut delivery, &current).await;
                }
                Some(Command::Finish) => break,
                Some(Command::Cancel) | None => return Ok(Outcome::Cancelled),
            },
            _=&mut max=>{capture.stop();break;}
            result=&mut silence_rx,if silence_stop_secs>0=>{
                // Closing capture drains the provider's audio queue and follows
                // exactly the same finalization path as the Stop button.
                if result.is_ok(){capture.stop();break;}
                return Err("麦克风停顿检测已断开，请重新开始录音。".into());
            }
            Some(e)=errors_rx.recv()=>{capture.stop();provider.abort();return Err(e);}
            _=focus.tick()=>{
                if config.injection.mode != "preview" && !delivery.internal && !delivery.paused && !delivery.target.0.valid() {
                    delivery.paused=true;
                    runtime.pause_writing(delivery.target.0, "输入位置已变化或检测到手动编辑。");
                }
            }
            Some(update)=output_rx.recv()=>{
                update_transcript(runtime,config,&mut delivery,&mut current,update).await;
            }
            result=&mut provider=>{
                capture.stop();
                result.map_err(|e|e.to_string())?.map_err(|e|format!("识别服务错误：{e}"))?;
                finished=true;break;
            }
        }
    }
    capture.stop();
    if let Some(active) = runtime.active.lock().unwrap().as_mut() {
        active.input = crate::runtime::InputState::Finishing;
    }
    runtime.set_status("processing", "正在收尾并确认最后的文字…");
    if !finished {
        let wait = if ["openai-compatible", "whispercpp"].contains(&config.provider.kind.as_str()) {
            125
        } else {
            25
        };
        let deadline = tokio::time::sleep(Duration::from_secs(wait));
        tokio::pin!(deadline);
        loop {
            tokio::select! {
                command=stop.recv()=>{if matches!(command, Some(Command::Cancel) | None) {provider.abort();return Ok(Outcome::Cancelled);}}
                _=&mut deadline=>{provider.abort();return Err("识别收尾超时，已保留当前文本，请复制后重试。".into());}
                Some(update)=output_rx.recv()=>{update_transcript(runtime,config,&mut delivery,&mut current,update).await;}
                result=&mut provider=>{result.map_err(|e|e.to_string())?.map_err(|e|format!("识别服务错误：{e}"))?;break;}
            }
        }
    }
    while let Ok(update) = output_rx.try_recv() {
        update_transcript(runtime, config, &mut delivery, &mut current, update).await;
    }
    let original = format!("{}{}", current.stable, current.partial);
    if original.trim().is_empty() {
        return Ok(Outcome::Done {
            message: "未识别到文字，请检查麦克风或识别服务".into(),
            keep: true,
        });
    }
    // Deliver the last ASR revision before a potentially slow correction call.
    // LLM corrections remain copyable and never erase live text already typed.
    if config.injection.mode == "realtime" {
        if let Err(error) = delivery.replace(&original, &config.injection.method).await {
            runtime.set_status("processing", error);
        }
    }
    let mut final_text = original.clone();
    let mut correction_warning = None;
    if config.correction.enabled {
        runtime.set_status("processing", "正在校对文字…");
        let corrected = tokio::select! {
            result=correct(config,&original)=>result,
            _=async {loop{if matches!(stop.recv().await, Some(Command::Cancel) | None){break;}}}=>return Ok(Outcome::Cancelled),
        };
        match corrected {
            Ok(text) => {
                final_text = text;
                runtime.set_transcript(&final_text, "", true);
            }
            Err(e) => {
                correction_warning = Some(e);
                runtime.set_transcript(&original, "", false);
            }
        }
    } else {
        runtime.set_transcript(&original, "", false);
    }
    let mut message: String;
    if config.injection.mode == "final" {
        if let Err(e) = delivery.append(&final_text, &config.injection.method).await {
            message = e;
        } else if delivery.internal {
            message = "识别完成 · 设置窗口内仅预览，可复制结果".into();
        } else if delivery.paused {
            message = "识别完成 · 焦点已变化，请复制结果".into();
        } else {
            message = "已写入选定的输入位置".into();
        }
    } else if config.injection.mode == "realtime" {
        message = if delivery.internal {
            "识别完成 · 设置窗口内仅预览，请在目标输入框按快捷键开始".into()
        } else if delivery.paused {
            "识别完成 · 自动写入已暂停，请复制完整结果".into()
        } else if config.correction.enabled {
            "实时文本已写入 · 校对结果可单独复制".into()
        } else {
            "实时文本已写入原输入位置".into()
        };
    } else if config.injection.mode == "stable" {
        // Corrections are shown for copying; existing text is never erased.
        if let Err(e) = delivery.append(&original, &config.injection.method).await {
            message = e;
        } else if delivery.internal {
            message = "识别完成 · 设置窗口内仅预览，请在目标输入框按快捷键开始".into();
        } else if delivery.paused {
            message = "识别完成 · 自动写入已暂停，请复制完整结果".into();
        } else if config.correction.enabled {
            message = "确认片段已写入 · 校对结果可单独复制".into();
        } else {
            message = "确认片段已写入".into();
        }
    } else {
        message = "识别完成 · 仅预览，可复制文字".into();
    }
    let resumed_stream =
        delivery.resumed && matches!(config.injection.mode.as_str(), "stable" | "realtime");
    if resumed_stream && !delivery.paused {
        message = "识别完成 · 已接续写入，完整结果可复制".into();
    }
    if let Some(e) = correction_warning {
        message = format!("{message}；校对失败，保留原文：{e}");
    }
    // Keep un-injected results/errors visible for manual recovery.
    let keep = resumed_stream
        || delivery.paused
        || delivery.internal
        || config.injection.mode == "preview"
        || config.correction.enabled
            && ["stable", "realtime"].contains(&config.injection.mode.as_str());
    Ok(Outcome::Done { message, keep })
}
async fn update_transcript(
    runtime: &Arc<Runtime>,
    config: &AppConfig,
    delivery: &mut Delivery,
    current: &mut providers::Transcript,
    update: providers::Transcript,
) {
    runtime.set_transcript(&update.stable, &update.partial, false);
    deliver_update(runtime, config, delivery, &update).await;
    *current = update;
}

async fn deliver_update(
    runtime: &Runtime,
    config: &AppConfig,
    delivery: &mut Delivery,
    update: &providers::Transcript,
) {
    if config.injection.mode == "realtime" {
        let text = format!("{}{}", update.stable, update.partial);
        if let Err(error) = delivery.replace(&text, &config.injection.method).await {
            runtime.pause_writing(delivery.target.0, &error);
        }
    } else if config.injection.mode == "stable" {
        if let Err(e) = delivery
            .append(&update.stable, &config.injection.method)
            .await
        {
            runtime.pause_writing(delivery.target.0, &e);
        }
    }
}
async fn correct(config: &AppConfig, text: &str) -> Result<String, String> {
    let mut builder = reqwest::Client::builder()
        .timeout(Duration::from_secs(20))
        .redirect(reqwest::redirect::Policy::none());
    // Local correction servers must not be routed through a system proxy either.
    if crate::config::is_loopback_endpoint(&config.correction.endpoint) {
        builder = builder.no_proxy();
    }
    let client = builder.build().map_err(|e| e.to_string())?;
    let mut request = client.post(&config.correction.endpoint).json(
        &serde_json::json!({"model":config.correction.model,"temperature":0,"messages":[
            {"role":"system","content":config.correction.prompt},{"role":"user","content":text}]}),
    );
    if !config.correction.api_key.is_empty() {
        request = request.bearer_auth(&config.correction.api_key);
    }
    let response = request.send().await.map_err(|e| {
        crate::service_error::http_request("校对服务", &config.correction.endpoint, &e)
    })?;
    if !response.status().is_success() {
        return Err(crate::service_error::http_status(
            "校对服务",
            response.status().as_u16(),
        ));
    }
    let data: serde_json::Value = response
        .json()
        .await
        .map_err(|_| "校对服务返回无效 JSON".to_string())?;
    let out = data["choices"][0]["message"]["content"]
        .as_str()
        .ok_or("校对 API 没有返回文字")?
        .trim()
        .to_string();
    if out.is_empty() || out.chars().count() > text.chars().count().saturating_mul(3) + 100 {
        return Err("校对结果异常，已保留原文".into());
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resume_only_revises_text_owned_at_the_new_caret() {
        let target = || InputGuard(platform::InputTarget::capture("invalid test shortcut"));
        let mut delivery = Delivery::new(target());
        delivery.delivered = "甲".into();
        delivery.paused = true;
        let observed = crate::runtime::Transcript {
            stable: "甲".into(),
            partial: "乙".into(),
            ..Default::default()
        };
        delivery.resume(target(), &observed, "realtime");
        assert!(!delivery.paused);
        assert!(delivery.delivered.is_empty());
        assert_eq!(delivery.writable_suffix("甲乙"), Some(""));
        assert_eq!(delivery.writable_suffix("甲乙丙"), Some("丙"));
        delivery.delivered = "丙".into();
        let suffix = delivery.writable_suffix("甲乙丁").unwrap();
        let edit = tail_edit(&delivery.delivered, suffix);
        assert_eq!((edit.remove, edit.insert), ("丙", "丁"));

        // A temporary retraction must not replay the frozen prefix. A new
        // hypothesis crossing the boundary cannot authorize any backspaces.
        assert_eq!(delivery.writable_suffix("甲"), None);
        assert_eq!(delivery.writable_suffix("甲乙丁"), Some("丁"));
        assert_eq!(delivery.writable_suffix("甲已丁"), None);
        assert!(delivery.delivered.is_empty());
        assert_eq!(delivery.writable_suffix("甲已丁戊"), Some("戊"));

        delivery.resume(target(), &observed, "stable");
        assert_eq!(delivery.writable_suffix("甲乙"), Some("乙"));
        delivery.resume(target(), &observed, "final");
        assert_eq!(delivery.writable_suffix("甲乙"), Some("甲乙"));
    }

    #[test]
    fn realtime_diff_handles_append_correction_retraction_and_unicode() {
        for (old, new, remove, insert) in [
            ("", "实时", "", "实时"),
            ("实时", "实时输入", "", "输入"),
            ("科研科盐", "科研科研。", "盐", "研。"),
            ("你好世界", "你好", "世界", ""),
            ("甲😀乙", "甲😀丙", "乙", "丙"),
            ("hello", "hello", "", ""),
            ("错字", "", "错字", ""),
        ] {
            let edit = tail_edit(old, new);
            assert_eq!((edit.remove, edit.insert), (remove, insert));
            let prefix = old.strip_suffix(edit.remove).unwrap();
            assert_eq!(format!("{prefix}{}", edit.insert), new);
        }
    }
}
