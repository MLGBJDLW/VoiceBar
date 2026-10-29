use crate::{audio, config::AppConfig, platform, providers, runtime::Runtime};
use std::{sync::Arc, time::Duration};
use tokio::sync::mpsc;

struct InputGuard(platform::InputTarget);
impl Drop for InputGuard {
    fn drop(&mut self) {
        self.0.release();
    }
}

struct Delivery {
    target: platform::InputTarget,
    delivered: String,
    paused: bool,
    internal: bool,
}
impl Delivery {
    fn new(target: platform::InputTarget) -> Self {
        let internal = target.process_id() == std::process::id();
        Self {
            target,
            delivered: String::new(),
            paused: false,
            internal,
        }
    }
    async fn append(&mut self, text: &str, method: &str) -> Result<(), String> {
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
        if !self.target.valid() {
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
        let target = self.target.window();
        let fragment = suffix.to_string();
        let clipboard = method == "clipboard";
        let result = tokio::task::spawn_blocking(move || {
            if clipboard {
                platform::paste_text(target, &fragment)
            } else {
                platform::type_text(target, &fragment)
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
        let target = self.target;
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
    mut stop: mpsc::UnboundedReceiver<bool>,
) {
    let input_guard = InputGuard(target);
    let result = execute(&runtime, &config, target, &mut stop).await;
    drop(input_guard);
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
    target: platform::InputTarget,
    stop: &mut mpsc::UnboundedReceiver<bool>,
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
            stop_result=stop.recv()=>{
                let cancel=stop_result.unwrap_or(true);
                capture.stop();
                if cancel{provider.abort();return Ok(Outcome::Cancelled);}
                break;
            }
            _=&mut max=>{capture.stop();break;}
            result=&mut silence_rx,if silence_stop_secs>0=>{
                // Closing capture drains the provider's audio queue and follows
                // exactly the same finalization path as the Stop button.
                if result.is_ok(){capture.stop();break;}
                return Err("麦克风停顿检测已断开，请重新开始录音。".into());
            }
            Some(e)=errors_rx.recv()=>{capture.stop();provider.abort();return Err(e);}
            _=focus.tick()=>{
                if !delivery.internal&&!delivery.paused&&!delivery.target.valid(){
                    delivery.paused=true;
                    runtime.set_status("listening","输入位置已变化或检测到手动编辑，自动写入暂停；识别继续，可复制结果");
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
                command=stop.recv()=>{if command.unwrap_or(true){provider.abort();return Ok(Outcome::Cancelled);}}
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
            _=async {loop{if stop.recv().await.unwrap_or(true){break;}}}=>return Ok(Outcome::Cancelled),
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
            message = "已写入原输入位置".into();
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
    if let Some(e) = correction_warning {
        message = format!("{message}；校对失败，保留原文：{e}");
    }
    // Keep un-injected results/errors visible for manual recovery.
    let keep = delivery.paused
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
    if config.injection.mode == "realtime" {
        let text = format!("{}{}", update.stable, update.partial);
        if let Err(error) = delivery.replace(&text, &config.injection.method).await {
            runtime.set_status("listening", error);
        }
    } else if config.injection.mode == "stable" {
        if let Err(e) = delivery
            .append(&update.stable, &config.injection.method)
            .await
        {
            runtime.set_status("listening", e);
        }
    }
    *current = update;
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
