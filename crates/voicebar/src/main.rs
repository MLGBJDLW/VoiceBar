#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod audio;
mod config;
mod desktop;
mod palette;
mod platform;
mod providers;
mod runtime;
mod service_error;
mod session;
mod ui;
mod updater;
mod voice_shape;
mod voice_visual;

use std::sync::Arc;

fn main() {
    if let Err(error) = launch() {
        startup_error(&error);
    }
}

fn launch() -> Result<(), String> {
    platform::set_app_id("studio.voicebar.desktop");
    let explicit_hidden = std::env::args().any(|arg| arg == "--hidden");
    let path = config::config_path()?;
    let (mut config, load_error) = match config::load(&path) {
        Ok(config) => (config, None),
        Err(error) => (config::AppConfig::default(), Some(error)),
    };
    if let Ok(enabled) = platform::autostart_enabled() {
        config.autostart = enabled;
    }
    let hidden = config.starts_hidden(explicit_hidden);
    let runtime = Arc::new(runtime::Runtime::new(config, path));
    if let Some(error) = load_error {
        runtime.set_status("error", format!("配置读取失败，请检查设置：{error}"));
    }
    let Some(desktop) = desktop::Desktop::new(runtime.clone(), explicit_hidden)? else {
        return Ok(());
    };
    startup_log("starting native GPUI event loop");
    ui::run(runtime, desktop, hidden);
    Ok(())
}

fn startup_log(message: &str) {
    use std::io::Write;
    let base = std::env::var_os("LOCALAPPDATA")
        .map(std::path::PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("VoiceBar")
        .join("logs");
    if std::fs::create_dir_all(&base).is_ok() {
        if let Ok(mut file) = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(base.join("startup.log"))
        {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let _ = writeln!(file, "{now} pid={} {message}", std::process::id());
        }
    }
}

fn startup_error(message: &str) {
    startup_log(message);
    #[cfg(windows)]
    unsafe {
        use windows::{
            core::PCWSTR,
            Win32::UI::WindowsAndMessaging::{MessageBoxW, MB_ICONERROR, MB_OK},
        };
        let message: Vec<u16> =
            format!("{message}\n\n启动日志：%LOCALAPPDATA%\\VoiceBar\\logs\\startup.log")
                .encode_utf16()
                .chain(Some(0))
                .collect();
        let title: Vec<u16> = "VoiceBar 启动失败".encode_utf16().chain(Some(0)).collect();
        MessageBoxW(
            None,
            PCWSTR(message.as_ptr()),
            PCWSTR(title.as_ptr()),
            MB_OK | MB_ICONERROR,
        );
    }
}
