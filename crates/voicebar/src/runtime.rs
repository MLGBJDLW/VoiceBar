use crate::{
    config::{self, AppConfig, Appearance},
    platform, session,
};
use serde::Serialize;
use std::{
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, AtomicU64, Ordering},
        Arc, Mutex,
    },
    time::{Duration, Instant},
};
use tokio::sync::mpsc;

#[derive(Clone, Debug, Serialize)]
pub struct Status {
    pub state: String,
    pub message: String,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct Transcript {
    pub stable: String,
    pub partial: String,
    pub corrected: bool,
    pub utterance: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Level {
    pub audio: crate::voice_visual::AudioFrame,
    pub updated_at: Option<Instant>,
    pub started_at: Option<Instant>,
    pub session_id: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct OverlayPreview {
    pub appearance: Appearance,
    pub started_at: Instant,
}

#[derive(Clone, Copy)]
pub enum InputState {
    Preview,
    Writing(platform::InputTarget),
    Paused,
    Finishing,
}

pub struct Active {
    pub stop: mpsc::UnboundedSender<session::Command>,
    pub input: InputState,
    pub id: u64,
}

/// Shared state for the native desktop and background audio/session tasks.
/// Window operations and global shortcut registration belong to the desktop.
pub struct Runtime {
    pub config: Mutex<AppConfig>,
    pub status: Mutex<Status>,
    pub transcript: Mutex<Transcript>,
    pub level: Mutex<Level>,
    pub active: Mutex<Option<Active>>,
    pub overlay_visible: AtomicBool,
    overlay_preview: Mutex<Option<OverlayPreview>>,
    pub settings_requested: AtomicBool,
    pub quit_requested: AtomicBool,
    pub executor: tokio::runtime::Runtime,
    pub(crate) counter: AtomicU64,
    config_path: PathBuf,
}

impl Runtime {
    pub fn new(config: AppConfig, path: PathBuf) -> Self {
        Self {
            config: Mutex::new(config),
            status: Mutex::new(Status {
                state: "idle".into(),
                message: "就绪".into(),
            }),
            transcript: Mutex::new(Transcript::default()),
            level: Mutex::new(Level::default()),
            active: Mutex::new(None),
            overlay_visible: AtomicBool::new(false),
            overlay_preview: Mutex::new(None),
            settings_requested: AtomicBool::new(false),
            quit_requested: AtomicBool::new(false),
            executor: tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .thread_name("voicebar")
                .build()
                .expect("无法初始化 VoiceBar 异步运行时"),
            counter: AtomicU64::new(0),
            config_path: path,
        }
    }

    /// Captures the foreground insertion point synchronously, before the
    /// desktop can observe the overlay request and create/show its window.
    pub fn start(self: &Arc<Self>) -> Result<(), String> {
        // Keep the same lock order as save_config. The active lock serializes
        // starts with saves and protects an existing input guard from recapture.
        let mut active = self.active.lock().unwrap();
        if active.is_some() {
            return Err("录音或转写正在进行".into());
        }
        // A real recording request takes over the preview even if validation fails.
        *self.overlay_preview.lock().unwrap() = None;
        let config = self.config.lock().unwrap().clone();
        if let Err(error) = config.validate_for_recording() {
            self.set_status("error", &error);
            self.overlay_visible.store(true, Ordering::SeqCst);
            return Err(error);
        }
        let id = self.counter.fetch_add(1, Ordering::SeqCst) + 1;
        let (stop, receiver) = mpsc::unbounded_channel();
        let target = platform::InputTarget::capture(&config.hotkey);
        let input =
            if config.injection.mode == "preview" || target.process_id() == std::process::id() {
                InputState::Preview
            } else {
                InputState::Writing(target)
            };
        *active = Some(Active { stop, id, input });
        self.set_transcript("", "", false);
        *self.level.lock().unwrap() = Level {
            started_at: Some(Instant::now()),
            session_id: id,
            ..Level::default()
        };
        self.set_status("connecting", "正在启动麦克风并连接识别引擎…");
        self.overlay_visible.store(true, Ordering::SeqCst);
        let runtime = Arc::clone(self);
        self.executor.spawn(async move {
            session::run(runtime, config, target, id, receiver).await;
        });
        Ok(())
    }

    pub fn toggle(self: &Arc<Self>) -> Result<(), String> {
        if self.toggle_active() {
            Ok(())
        } else {
            self.start()
        }
    }

    pub fn toggle_active(&self) -> bool {
        let mut slot = self.active.lock().unwrap();
        let Some(active) = slot.as_mut() else {
            return false;
        };
        // A physical edit can reach the input hook just before the shortcut,
        // between two session focus ticks. Check the live guard here as well.
        let resume = match active.input {
            InputState::Paused => true,
            InputState::Writing(target) => !target.valid(),
            InputState::Preview | InputState::Finishing => false,
        };
        if resume {
            self.resume_locked(active);
        } else {
            active.input = InputState::Finishing;
            let _ = active.stop.send(session::Command::Finish);
        }
        true
    }

    pub fn writing_paused(&self) -> bool {
        self.active
            .lock()
            .unwrap()
            .as_ref()
            .is_some_and(|active| matches!(active.input, InputState::Paused))
    }

    pub fn resume(&self) {
        if let Some(active) = self.active.lock().unwrap().as_mut() {
            if matches!(active.input, InputState::Paused) {
                self.resume_locked(active);
            }
        }
    }

    fn resume_locked(&self, active: &mut Active) {
        let hotkey = self.config.lock().unwrap().hotkey.clone();
        let target = session::InputGuard(platform::InputTarget::capture(&hotkey));
        if target.0.process_id() == std::process::id() || !target.0.valid() {
            active.input = InputState::Paused;
            self.set_status(
                "paused",
                format!("请先点选要继续写入的输入框，再按 {hotkey}。识别仍在继续。"),
            );
            return;
        }
        active.input = InputState::Writing(target.0);
        let observed = self.transcript.lock().unwrap().clone();
        let _ = active.stop.send(session::Command::Resume(target, observed));
        self.set_status("listening", "已继续写入当前光标");
    }

    pub fn pause_writing(&self, target: platform::InputTarget, reason: &str) {
        if let Some(active) = self.active.lock().unwrap().as_mut() {
            // An old, in-flight edit may fail just after the user resumed on a
            // fresh generation. It must not invalidate the new insertion point.
            if matches!(active.input, InputState::Writing(current) if current == target) {
                active.input = InputState::Paused;
                self.set_status(
                    "paused",
                    format!("{reason} 识别继续，按快捷键或点「继续」可从当前光标接着写。"),
                );
            }
        }
    }

    pub fn stop(&self, cancel: bool) {
        if let Some(active) = self.active.lock().unwrap().as_mut() {
            active.input = InputState::Finishing;
            let _ = active.stop.send(if cancel {
                session::Command::Cancel
            } else {
                session::Command::Finish
            });
        }
    }

    /// The desktop registers the new shortcuts before calling this method and
    /// restores the old registrations if it returns an error.
    pub fn save_config(&self, mut config: AppConfig) -> Result<(), String> {
        let active = self.active.lock().unwrap();
        let mut current = self.config.lock().unwrap();
        config.merge_secrets(&current);
        config.normalize();
        config.validate()?;
        if active.is_some() && !config.only_appearance_changed(&current) {
            return Err(
                "录音期间可保存外观与位置；更改引擎、快捷键或输入设置请先结束录音。".into(),
            );
        }

        let previous_autostart = if config.autostart != current.autostart {
            Some(platform::autostart_enabled()?)
        } else {
            None
        };
        if let Some(previous) = previous_autostart {
            if let Err(error) = platform::set_autostart(config.autostart) {
                // A platform call can fail after writing but before verifying.
                return Err(rollback_autostart(previous, error));
            }
        }
        if let Err(error) = config::save(&self.config_path, &config) {
            return Err(match previous_autostart {
                Some(previous) => rollback_autostart(previous, error),
                None => error,
            });
        }
        // The atomic disk replacement and OS setting both succeeded. Only now
        // publish the new in-memory config; failures keep the previous values.
        *current = config;
        Ok(())
    }

    /// A preview never starts capture, touches credentials, or commits settings.
    pub fn preview_overlay(&self, appearance: Appearance) -> Result<(), String> {
        appearance.validate()?;
        *self.overlay_preview.lock().unwrap() = Some(OverlayPreview {
            appearance,
            started_at: Instant::now(),
        });
        Ok(())
    }

    pub fn overlay_preview(&self) -> Option<OverlayPreview> {
        self.overlay_preview_at(Instant::now())
    }

    fn overlay_preview_at(&self, now: Instant) -> Option<OverlayPreview> {
        let mut preview = self.overlay_preview.lock().unwrap();
        if preview
            .as_ref()
            .is_some_and(|p| now.saturating_duration_since(p.started_at) >= Duration::from_secs(10))
        {
            *preview = None;
        }
        preview.clone()
    }

    pub fn dismiss_overlay(&self) {
        *self.overlay_preview.lock().unwrap() = None;
        self.overlay_visible.store(false, Ordering::SeqCst);
    }

    pub fn copy_transcript(&self) -> Result<(), String> {
        let text = {
            let transcript = self.transcript.lock().unwrap();
            format!("{}{}", transcript.stable, transcript.partial)
        };
        platform::copy_text(&text)
    }

    pub fn set_status(&self, state: &str, message: impl Into<String>) {
        *self.status.lock().unwrap() = Status {
            state: state.into(),
            message: message.into(),
        };
    }

    pub fn set_transcript(&self, stable: &str, partial: &str, corrected: bool) {
        let mut transcript = self.transcript.lock().unwrap();
        // Confirmed extensions produce one pulse; tentative revisions and
        // correction rewrites do not manufacture a new sentence boundary.
        let utterance = if stable.is_empty() && partial.is_empty() {
            0
        } else if !corrected
            && stable.len() > transcript.stable.len()
            && stable.starts_with(&transcript.stable)
        {
            transcript.utterance.wrapping_add(1)
        } else {
            transcript.utterance
        };
        *transcript = Transcript {
            stable: stable.into(),
            partial: partial.into(),
            corrected,
            utterance,
        };
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn runtime() -> Runtime {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../.nexa/tmp/overlay-position-tests")
            .join(uuid::Uuid::new_v4().to_string())
            .join("settings.json");
        Runtime::new(AppConfig::default(), path)
    }

    #[test]
    fn paused_shortcut_without_a_writable_target_keeps_the_session_and_transcript() {
        let runtime = Arc::new(runtime());
        let (stop, mut commands) = mpsc::unbounded_channel();
        *runtime.active.lock().unwrap() = Some(Active {
            stop,
            id: 23,
            input: InputState::Paused,
        });
        // An unsupported chord deliberately prevents the test from acquiring
        // hooks or writing into the foreground application.
        runtime.config.lock().unwrap().hotkey = "invalid test shortcut".into();
        runtime.overlay_visible.store(true, Ordering::SeqCst);
        runtime.set_transcript("甲", "乙", false);
        runtime.toggle().unwrap();
        assert!(commands.try_recv().is_err());
        assert!(runtime.writing_paused());
        assert_eq!(runtime.status.lock().unwrap().state, "paused");
        assert!(runtime.overlay_visible.load(Ordering::SeqCst));
        assert_eq!(runtime.transcript.lock().unwrap().partial, "乙");
        runtime.stop(false);
        assert!(matches!(commands.try_recv(), Ok(session::Command::Finish)));
        assert!(!runtime.writing_paused());
    }

    #[test]
    fn appearance_saves_and_reloads_during_an_active_session() {
        let runtime = runtime();
        let (stop, _receive) = mpsc::unbounded_channel();
        *runtime.active.lock().unwrap() = Some(Active {
            stop,
            id: 17,
            input: InputState::Preview,
        });
        let mut config = runtime.config.lock().unwrap().clone();
        config.appearance.anchor = "bottom-right".into();
        config.appearance.offset_x = -37;
        config.appearance.offset_y = -19;
        runtime.save_config(config.clone()).unwrap();
        assert_eq!(config::load(&runtime.config_path).unwrap(), config);
        assert_eq!(runtime.config.lock().unwrap().appearance, config.appearance);
        assert_eq!(runtime.active.lock().unwrap().as_ref().unwrap().id, 17);
        std::fs::remove_dir_all(runtime.config_path.parent().unwrap()).unwrap();
    }

    #[test]
    fn active_session_still_rejects_other_setting_changes_without_writing() {
        let runtime = runtime();
        let (stop, _receive) = mpsc::unbounded_channel();
        *runtime.active.lock().unwrap() = Some(Active {
            stop,
            id: 18,
            input: InputState::Preview,
        });
        let previous = runtime.config.lock().unwrap().clone();
        let mut draft = previous.clone();
        draft.appearance.anchor = "center".into();
        draft.provider.model = "another-model".into();
        assert!(runtime
            .save_config(draft)
            .unwrap_err()
            .contains("请先结束录音"));
        assert!(!runtime.config_path.exists());
        assert_eq!(*runtime.config.lock().unwrap(), previous);
        assert!(runtime.active.lock().unwrap().is_some());
    }

    #[test]
    fn preview_uses_unsaved_position_without_starting_or_persisting_a_session() {
        let runtime = runtime();
        let original = runtime.config.lock().unwrap().clone();
        let mut appearance = original.appearance.clone();
        appearance.anchor = "bottom-left".into();
        appearance.offset_x = 72;
        appearance.offset_y = -40;
        runtime.preview_overlay(appearance.clone()).unwrap();
        assert_eq!(runtime.overlay_preview().unwrap().appearance, appearance);
        assert_eq!(*runtime.config.lock().unwrap(), original);
        assert!(!runtime.config_path.exists());
        assert!(runtime.active.lock().unwrap().is_none());
        assert!(!runtime.overlay_visible.load(Ordering::SeqCst));

        let started = runtime.overlay_preview().unwrap().started_at;
        assert!(runtime
            .overlay_preview_at(started + Duration::from_secs(9))
            .is_some());
        assert!(runtime
            .overlay_preview_at(started + Duration::from_secs(10))
            .is_none());
    }

    #[test]
    fn recording_validation_error_takes_over_an_existing_preview() {
        let runtime = Arc::new(runtime());
        {
            let mut config = runtime.config.lock().unwrap();
            config.provider.kind = "openai-realtime".into();
            config.provider.endpoint = "wss://api.openai.com/v1/realtime".into();
        }
        runtime.preview_overlay(Appearance::default()).unwrap();
        assert!(runtime.start().unwrap_err().contains("API Key"));
        assert!(runtime.overlay_preview().is_none());
        assert!(runtime.overlay_visible.load(Ordering::SeqCst));
        assert_eq!(runtime.status.lock().unwrap().state, "error");
        assert!(runtime.active.lock().unwrap().is_none());
    }

    #[test]
    fn preview_expiration_does_not_hide_recording_and_invalid_preview_keeps_previous() {
        let runtime = runtime();
        runtime.overlay_visible.store(true, Ordering::SeqCst);
        runtime.preview_overlay(Appearance::default()).unwrap();
        let preview = runtime.overlay_preview().unwrap();
        let mut invalid = Appearance::default();
        invalid.scale = f64::NAN;
        assert!(runtime.preview_overlay(invalid).is_err());
        assert_eq!(runtime.overlay_preview(), Some(preview.clone()));
        assert!(runtime
            .overlay_preview_at(preview.started_at + Duration::from_secs(11))
            .is_none());
        assert!(runtime.overlay_visible.load(Ordering::SeqCst));
        runtime.dismiss_overlay();
        assert!(!runtime.overlay_visible.load(Ordering::SeqCst));
    }
}

fn rollback_autostart(previous: bool, error: String) -> String {
    match platform::set_autostart(previous) {
        Ok(()) => error,
        Err(rollback) => format!("{error}；恢复开机自启设置失败：{rollback}"),
    }
}
