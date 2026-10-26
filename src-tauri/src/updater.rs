// In-app update support built on tauri-plugin-updater.
//
// Release builds carry the minisign public key below; the release workflow
// signs the NSIS bundle with the matching private key and publishes
// `latest.json` next to it. Nothing here runs unless the build carries a key
// and the app runs as a normal installer (see `bundled()`), because the
// updater needs a registered install location to replace files safely.
//
// The endpoint is compiled in and passed explicitly, so the feed cannot be
// changed from the settings file or by the webview.

use serde::Serialize;
use tauri_plugin_updater::UpdaterExt;

/// Updater feed. Must match where the release workflow uploads `latest.json`
/// (GitHub's fixed "latest release" asset path).
const ENDPOINT: &str = "https://github.com/MLGBJDLW/VoiceBar/releases/latest/download/latest.json";

/// Public half of the release signing key (`tauri signer generate`). Rotating
/// it means every future release must be signed with the new private key; apps
/// already installed keep verifying with the key they shipped with.
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDcwMUJBN0Y1MkU2MTgzQ0EKUldUS2cyRXU5YWNiY0hySklGZER3RENwMWZzVG0rem5WYnl5S0Z3SjNaOWt6SmlJRnZUNXViaEQK";

/// How long a manifest check may hang before we tell the user to retry.
const CHECK_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(20);
/// Downloads are far bigger than the manifest; keep the budget generous.
const DOWNLOAD_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(600);

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

fn endpoint() -> String {
    option_env!("VOICEBAR_UPDATER_ENDPOINT")
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .unwrap_or(ENDPOINT)
        .to_string()
}

/// A build can update itself only when it can verify the artifact and talks to
/// the feed over TLS.
pub fn enabled() -> bool {
    !PUBKEY.trim().is_empty() && endpoint().starts_with("https://")
}

/// Both halves of the question, for the background check and the UI.
pub fn can_self_update() -> bool {
    bundled() && enabled()
}

/// Custom builds (`-Mode portable`, `cargo run`) have nothing to update.
/// The Tauri NSIS installer ships the exe inside `%LOCALAPPDATA%\VoiceBar`;
/// the MSI installs under `Program Files`, where a non-elevated update cannot
/// write, so only the NSIS layout is offered in-app updates.
pub fn bundled() -> bool {
    if cfg!(debug_assertions) || !cfg!(windows) {
        return false;
    }
    let Ok(exe) = std::env::current_exe() else {
        return false;
    };
    let Some(dir) = exe.parent() else {
        return false;
    };
    let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    root.is_some_and(|root| dir.starts_with(root.join("VoiceBar")))
}

fn parse_endpoint(text: &str) -> Result<url::Url, String> {
    let text = text.trim();
    if !(text.starts_with("https://") || text.starts_with("http://")) {
        return Err("更新地址必须以 https:// 开头".into());
    }
    url::Url::parse(text).map_err(|e| format!("更新地址无效：{e}"))
}

fn build_updater(
    app: &tauri::AppHandle,
    timeout: std::time::Duration,
) -> Result<tauri_plugin_updater::Updater, String> {
    app.updater_builder()
        .pubkey(PUBKEY)
        .endpoints(vec![parse_endpoint(&endpoint())?])
        .map_err(|e| format!("更新配置无效：{e}"))?
        .timeout(timeout)
        .build()
        .map_err(|e| format!("更新器初始化失败：{e}"))
}

/// What an update can do in *this* build, so the UI can explain itself instead
/// of offering a button that always fails.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateSupport {
    pub self_update: bool,
    pub current_version: String,
}

#[tauri::command]
pub fn update_support(app: tauri::AppHandle) -> UpdateSupport {
    UpdateSupport {
        self_update: bundled() && enabled(),
        current_version: app.package_info().version.to_string(),
    }
}

/// Background variant used at launch: never takes the manual gate (a user
/// click must win) and returns `None` when there is nothing to install.
pub async fn check_quietly(app: &tauri::AppHandle) -> Result<Option<UpdateInfo>, String> {
    if !can_self_update() {
        return Ok(None);
    }
    let current = app.package_info().version.to_string();
    let release = build_updater(app, CHECK_TIMEOUT)?
        .check()
        .await
        .map_err(describe)?;
    Ok(release.map(|release| UpdateInfo {
        current_version: current,
        available: true,
        version: Some(release.version.clone()),
        date: release.date.map(|date| date.to_string()),
        notes: release.body.clone(),
    }))
}

#[tauri::command]
pub async fn check_for_update(app: tauri::AppHandle) -> Result<UpdateInfo, String> {
    let current = app.package_info().version.to_string();
    let none = || UpdateInfo {
        current_version: current.clone(),
        available: false,
        version: None,
        notes: None,
        date: None,
    };
    if !can_self_update() {
        return Ok(none());
    }
    let _gate = crate::Gate::acquire().ok_or("正在检查更新，请稍候")?;
    // `None` means "no newer version" (the plugin also maps a server-side 204
    // to this); either way there is nothing to install.
    Ok(check_quietly(&app).await?.unwrap_or_else(none))
}

#[tauri::command]
pub async fn install_update(app: tauri::AppHandle) -> Result<(), String> {
    if !bundled() || !enabled() {
        return Err(crate::UPDATE_UNAVAILABLE.into());
    }
    let _gate = crate::Gate::acquire().ok_or("正在下载更新，请稍候")?;
    let updater = build_updater(&app, DOWNLOAD_TIMEOUT)?;
    let Some(release) = updater.check().await.map_err(describe)? else {
        return Err("已经是最新版本".into());
    };
    // Download verifies the minisign signature before returning bytes, so a
    // tampered or unsigned artifact never reaches the installer. Abort any live
    // dictation first: the app restarts and nothing should be injected.
    crate::signal_stop_current(&app, true);
    release
        .download_and_install(|_chunk, _total| {}, || {})
        .await
        .map_err(describe)?;
    app.restart();
}

/// Turns updater errors into an actionable sentence; never echoes URLs with
/// query strings or key material.
fn describe(error: tauri_plugin_updater::Error) -> String {
    use tauri_plugin_updater::Error as E;
    match error {
        E::EmptyEndpoints => "更新地址未配置".into(),
        E::Reqwest(e) if e.is_timeout() => {
            "检查更新超时，请稍后重试；企业网络可能拦截了 GitHub".into()
        }
        E::Reqwest(e) if e.is_connect() => "无法连接更新服务，请检查网络或代理".into(),
        E::Reqwest(_) => "更新请求失败，请稍后重试".into(),
        E::Network(message) => format!("更新网络错误：{}", trim(message)),
        E::ReleaseNotFound | E::Serialization(_) => {
            "还没有可用的更新包（latest.json 缺失或格式不正确）".into()
        }
        E::InsecureTransportProtocol => "更新地址必须使用 HTTPS".into(),
        E::TargetNotFound(target) => {
            format!("更新包缺少目标 {target}，请检查发布流程生成的 latest.json")
        }
        E::TargetsNotFound(targets) => format!(
            "更新包缺少目标 {}，请检查发布流程生成的 latest.json",
            targets.join(" / ")
        ),
        E::Minisign(_) => "更新包签名校验失败，已取消安装".into(),
        E::Io(_) | E::TempDirNotOnSameMountPoint | E::TempDirNotFound => {
            "无法写入临时文件，请检查磁盘空间和权限".into()
        }
        other => trim(other.to_string()),
    }
}

fn trim(text: String) -> String {
    let compact = text.split_whitespace().collect::<Vec<_>>().join(" ");
    if compact.chars().count() > 240 {
        compact.chars().take(240).collect::<String>() + "…"
    } else {
        compact
    }
}

#[cfg(test)]
mod release_metadata_tests {
    // The repository config is checked from here because these are the checks
    // that historically broke CI: `release-plz.toml` once held a JSON object,
    // so every TOML parser rejected it, and the Rust sources had never been
    // through `cargo fmt`. Keep both guarded.
    use std::path::PathBuf;

    fn repo_file(relative: &str) -> String {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .expect("src-tauri sits inside the repository")
            .to_path_buf();
        let path = root.join(relative);
        std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
    }

    fn schema_properties(schema: &serde_json::Value, definition: &str) -> Vec<String> {
        schema["$defs"][definition]["properties"]
            .as_object()
            .expect("schema properties")
            .keys()
            .cloned()
            .collect()
    }

    #[test]
    fn release_plz_config_matches_its_schema() {
        // Three separate release-job failures lived here: the file once held a
        // JSON object (no TOML parser could read it), then put global keys at
        // the top level, then carried a `"$schema"` key. release-plz accepts
        // only `workspace`, `package` and `changelog` at the top level and
        // aborts the run on anything else, metadata keys included.
        let text = repo_file("release-plz.toml");
        assert!(
            !text.trim_start().starts_with('{'),
            "release-plz.toml must be TOML, not a JSON object"
        );

        let schema: serde_json::Value =
            serde_json::from_str(&repo_file("docs/release-plz.schema.json")).expect("schema");
        let workspace_keys = schema_properties(&schema, "Workspace");
        let package_keys = schema_properties(&schema, "PackageSpecificConfigWithName");

        let mut section = "";
        let mut seen_workspace = false;
        let mut seen_package = false;
        for line in text.lines().map(str::trim) {
            if line.starts_with('#') || line.is_empty() {
                continue;
            }
            if line == "[workspace]" {
                section = "workspace";
                seen_workspace = true;
                continue;
            }
            if line == "[[package]]" {
                section = "package";
                seen_package = true;
                continue;
            }
            let Some((key, _)) = line.split_once('=') else {
                continue;
            };
            let key = key.trim().trim_matches('"');
            match section {
                "" => panic!("top-level key {key:?} must live under [workspace]"),
                "workspace" => assert!(
                    workspace_keys.iter().any(|k| k == key),
                    "[workspace] has no key {key:?}"
                ),
                _ => assert!(
                    package_keys.iter().any(|k| k == key),
                    "[[package]] has no key {key:?}"
                ),
            }
        }
        assert!(seen_workspace && seen_package, "expected both tables");
        assert!(text.contains("publish = false"), "crates.io stays off");
        assert!(text.contains("version_group = \"voicebar\""));
        assert!(repo_file(".release-plz/changelog.toml").contains("commit_parsers"));
    }

    #[test]
    fn release_version_matches_every_manifest() {
        let cargo = repo_file("src-tauri/Cargo.toml");
        let version = cargo
            .split("[package]")
            .nth(1)
            .expect("[package] table")
            .lines()
            .find_map(|line| line.trim().strip_prefix("version"))
            .map(|rest| {
                rest.trim_start()
                    .trim_start_matches('=')
                    .trim()
                    .trim_matches('"')
                    .to_string()
            })
            .expect("package.version");
        let tauri: serde_json::Value =
            serde_json::from_str(&repo_file("src-tauri/tauri.conf.json")).expect("tauri.conf.json");
        assert_eq!(tauri["version"].as_str(), Some(version.as_str()));
        let package = repo_file("package.json");
        assert!(
            package.contains(&format!("\"version\": \"{version}\"")),
            "package.json must carry version {version}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn feed_is_https_and_points_at_the_latest_asset() {
        let parsed = parse_endpoint(&endpoint()).unwrap();
        assert_eq!(parsed.scheme(), "https");
        assert!(parsed
            .path()
            .ends_with("/releases/latest/download/latest.json"));
        assert!(parse_endpoint("file:///C:/latest.json").is_err());
        assert!(parse_endpoint("not a url").is_err());
    }

    #[test]
    fn checked_in_build_carries_a_verifiable_key() {
        // Embedded on purpose: without it no installed build could ever accept
        // an update and the settings page would silently do nothing.
        let decoded = base64_decode(PUBKEY);
        assert!(decoded.contains("minisign public key"), "{decoded}");
        assert!(enabled());
    }

    #[test]
    fn unbundled_builds_never_pretend_to_update() {
        // Tests run from target/debug, outside the NSIS install directory.
        assert!(!bundled());
    }

    #[test]
    fn errors_are_short_and_actionable() {
        assert_eq!(trim("  a \n b  ".to_string()), "a b");
        assert!(trim("x ".repeat(400)).chars().count() <= 241);
        assert!(describe(tauri_plugin_updater::Error::EmptyEndpoints).contains("更新地址"));
    }

    /// Minimal base64 decoder so the test needs no extra dependency.
    fn base64_decode(input: &str) -> String {
        const TABLE: &[u8; 64] =
            b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
        let mut out = Vec::new();
        let mut buffer: u32 = 0;
        let mut bits = 0;
        for byte in input
            .bytes()
            .filter(|b| !b.is_ascii_whitespace() && *b != b'=')
        {
            let value = TABLE
                .iter()
                .position(|c| *c == byte)
                .expect("base64 alphabet") as u32;
            buffer = (buffer << 6) | value;
            bits += 6;
            if bits >= 8 {
                bits -= 8;
                out.push((buffer >> bits) as u8);
            }
        }
        String::from_utf8(out).unwrap()
    }
}
