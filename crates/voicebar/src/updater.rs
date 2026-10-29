//! Native updater: only an explicit install action downloads and launches code.
//! The feed is untrusted metadata; minisign authenticates both the installer
//! bytes and the version in its trusted comment before anything is executed.

use base64::{engine::general_purpose::STANDARD, Engine as _};
use minisign_verify::{PublicKey, Signature};
use semver::Version;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    io::Write,
    path::{Path, PathBuf},
    time::Duration,
};

const ENDPOINT: &str = "https://github.com/MLGBJDLW/VoiceBar/releases/latest/download/latest.json";
// Keep this key stable: existing installations trust this exact key.
const PUBKEY: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDYzMDc0M0UxMUMyQUQ1NUYKUldSZjFTb2M0VU1IWS93cVVibHlQOWtHdEJUN1hsTXpIc1dOTXA4RGFXN3htMVlCUWk3L2N3SmIK";
const PLATFORM: &str = "windows-x86_64";
const MAX_MANIFEST_BYTES: usize = 256 * 1024;
const MAX_INSTALLER_BYTES: usize = 256 * 1024 * 1024;
const CHECK_TIMEOUT: Duration = Duration::from_secs(20);
const DOWNLOAD_TIMEOUT: Duration = Duration::from_secs(600);
static UPDATE_GATE: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct UpdateInfo {
    pub current_version: String,
    pub available: bool,
    pub version: Option<String>,
    pub notes: Option<String>,
    pub date: Option<String>,
}

#[derive(Deserialize)]
struct Manifest {
    version: String,
    notes: Option<String>,
    pub_date: Option<String>,
    platforms: HashMap<String, Artifact>,
}

#[derive(Deserialize)]
struct Artifact {
    url: String,
    signature: String,
    sha256: Option<String>,
}

struct Release {
    version: Version,
    notes: Option<String>,
    date: Option<String>,
    artifact: Artifact,
}

/// Development and portable builds may check for releases, but only the
/// per-user installed layout may replace itself. Require the exact directory
/// (not a child such as logs/updates) and its uninstaller, including legacy
/// installations that predate the native UI.
pub fn can_self_update() -> bool {
    if cfg!(debug_assertions) || !cfg!(all(windows, target_arch = "x86_64")) {
        return false;
    }
    installed_directory().is_some()
}

fn installed_directory() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let dir = exe.parent()?.canonicalize().ok()?;
    let expected = PathBuf::from(std::env::var_os("LOCALAPPDATA")?)
        .join("VoiceBar")
        .canonicalize()
        .ok()?;
    (dir == expected && dir.join("uninstall.exe").is_file()).then_some(dir)
}

fn client(timeout: Duration) -> Result<reqwest::Client, String> {
    reqwest::Client::builder()
        .https_only(true)
        .redirect(reqwest::redirect::Policy::limited(10))
        .connect_timeout(CHECK_TIMEOUT)
        .timeout(timeout)
        .user_agent(concat!("VoiceBar/", env!("CARGO_PKG_VERSION")))
        .build()
        .map_err(|_| "无法初始化更新连接".into())
}

fn https_url(text: &str) -> Result<url::Url, String> {
    let url = url::Url::parse(text).map_err(|_| "更新下载地址无效")?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
        || url.fragment().is_some()
    {
        return Err("更新地址必须使用无凭据的 HTTPS".into());
    }
    Ok(url)
}

// Do not expose reqwest's URL-bearing diagnostics in the UI or logs.
fn network_error(error: reqwest::Error) -> String {
    if error.is_timeout() {
        "更新请求超时，请稍后重试；当前网络可能无法连接 GitHub".into()
    } else if error.is_connect() {
        "无法连接更新服务，请检查网络或代理".into()
    } else {
        "更新请求失败，请稍后重试".into()
    }
}

async fn read_limited(mut response: reqwest::Response, limit: usize) -> Result<Vec<u8>, String> {
    if response
        .content_length()
        .is_some_and(|size| size > limit as u64)
    {
        return Err("更新文件超过允许的大小，已取消下载".into());
    }
    let mut bytes = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(network_error)? {
        if chunk.len() > limit.saturating_sub(bytes.len()) {
            return Err("更新文件超过允许的大小，已取消下载".into());
        }
        bytes.extend_from_slice(&chunk);
    }
    Ok(bytes)
}

async fn latest_release() -> Result<Option<Release>, String> {
    let response = client(CHECK_TIMEOUT)?
        .get(https_url(ENDPOINT)?)
        .send()
        .await
        .map_err(network_error)?;
    match response.status() {
        reqwest::StatusCode::NO_CONTENT => return Ok(None),
        reqwest::StatusCode::NOT_FOUND => {
            return Err("还没有可用的更新包（latest.json 尚未发布）".into());
        }
        status if !status.is_success() => {
            return Err(format!(
                "更新服务返回错误（HTTP {}），请稍后重试",
                status.as_u16()
            ));
        }
        _ => {}
    }
    let manifest: Manifest =
        serde_json::from_slice(&read_limited(response, MAX_MANIFEST_BYTES).await?)
            .map_err(|_| "更新清单格式不正确，请稍后重试")?;
    let version = parse_version(&manifest.version)?;
    let current = parse_version(env!("CARGO_PKG_VERSION"))?;
    // cmp_precedence deliberately ignores build metadata for update ordering.
    if !version.cmp_precedence(&current).is_gt() {
        return Ok(None);
    }
    let artifact = manifest
        .platforms
        .into_iter()
        .find_map(|(key, artifact)| (key == PLATFORM).then_some(artifact))
        .ok_or("更新清单缺少 Windows x64 安装包")?;
    let url = https_url(&artifact.url)?;
    if !url
        .path()
        .ends_with(&format!("/VoiceBar_{version}_x64-setup.exe"))
    {
        return Err("更新清单中的安装包名称与版本不一致".into());
    }
    // Decode now to reject malformed input, but trust nothing in the comment
    // until verify_installer has verified minisign's global signature.
    decode_signature(&artifact.signature)?;
    Ok(Some(Release {
        version,
        notes: manifest.notes,
        date: manifest.pub_date,
        artifact,
    }))
}

fn parse_version(text: &str) -> Result<Version, String> {
    if text.len() > 160 {
        return Err("更新版本号无效".into());
    }
    Version::parse(text.trim().trim_start_matches('v')).map_err(|_| "更新版本号无效".into())
}

/// Read-only. Safe to call at launch when the user's check preference allows it.
pub async fn check_for_update() -> Result<UpdateInfo, String> {
    let _gate = UPDATE_GATE.try_lock().map_err(|_| "正在处理更新，请稍候")?;
    let release = latest_release().await?;
    Ok(UpdateInfo {
        current_version: env!("CARGO_PKG_VERSION").into(),
        available: release.is_some(),
        version: release.as_ref().map(|release| release.version.to_string()),
        notes: release.as_ref().and_then(|release| release.notes.clone()),
        date: release.and_then(|release| release.date),
    })
}

fn decode_signature(text: &str) -> Result<Signature, String> {
    if text.len() > 8192 {
        return Err("更新包签名格式无效".into());
    }
    let decoded = STANDARD
        .decode(text.trim())
        .map_err(|_| "更新包签名格式无效")?;
    let decoded = std::str::from_utf8(&decoded).map_err(|_| "更新包签名格式无效")?;
    Signature::decode(decoded).map_err(|_| "更新包签名格式无效".into())
}

fn verify_installer(bytes: &[u8], release: &Release) -> Result<(), String> {
    verify_signed_installer(bytes, release, PUBKEY)
}

fn verify_signed_installer(
    bytes: &[u8],
    release: &Release,
    public_key: &str,
) -> Result<(), String> {
    let key = STANDARD
        .decode(public_key)
        .map_err(|_| "内置更新公钥无效")?;
    let key = std::str::from_utf8(&key).map_err(|_| "内置更新公钥无效")?;
    let key = PublicKey::decode(key).map_err(|_| "内置更新公钥无效")?;
    let signature = decode_signature(&release.artifact.signature)?;
    // true keeps compatibility with older, non-prehashed minisign signatures.
    // Both forms authenticate the global signature over the trusted comment.
    key.verify(bytes, &signature, true)
        .map_err(|_| "更新包签名校验失败，已取消安装")?;
    let versions: Vec<_> = signature
        .trusted_comment()
        .split('\t')
        .filter_map(|field| field.strip_prefix("version:"))
        .collect();
    if versions.len() != 1 || parse_version(versions[0])? != release.version {
        return Err("更新包签名中的版本与清单不一致，已取消安装".into());
    }
    if let Some(expected) = &release.artifact.sha256 {
        let actual = Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if !actual.eq_ignore_ascii_case(expected) {
            return Err("更新包 SHA-256 校验失败，已取消安装".into());
        }
    }
    Ok(())
}

/// Call ONLY from the user's explicit install button. Returns Ok after
/// launching the verified installer. The caller must then cancel recording
/// and request GPUI shutdown; the installer waits for this process to exit.
pub async fn install_update() -> Result<(), String> {
    if !can_self_update() {
        return Err("当前构建不支持原地更新，请下载安装版".into());
    }
    let _gate = UPDATE_GATE.try_lock().map_err(|_| "正在处理更新，请稍候")?;
    let release = latest_release().await?.ok_or("已经是最新版本")?;
    let response = client(DOWNLOAD_TIMEOUT)?
        .get(https_url(&release.artifact.url)?)
        .send()
        .await
        .map_err(network_error)?;
    if !response.status().is_success() {
        return Err("无法下载更新安装包，请稍后重试".into());
    }
    let bytes = read_limited(response, MAX_INSTALLER_BYTES).await?;
    verify_installer(&bytes, &release)?;
    // Verification is bounded but CPU-heavy: downloads are never run on the
    // GPUI foreground thread (call this future on the application's runtime).
    let directory = installed_directory().ok_or("安装位置已变化，请重新启动后再试")?;
    let stage = directory.join(format!("update-{}", uuid::Uuid::new_v4()));
    std::fs::create_dir(&stage).map_err(|_| "无法创建更新目录，请检查磁盘空间和权限")?;
    let path = stage.join("VoiceBar-update.exe");
    let result = write_and_launch(&path, &bytes);
    if result.is_err() {
        let _ = std::fs::remove_file(&path);
        let _ = std::fs::remove_dir(&stage);
    }
    result
}

fn write_and_launch(path: &Path, bytes: &[u8]) -> Result<(), String> {
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(path)
        .map_err(|_| "无法保存更新安装包，请检查磁盘空间和权限")?;
    file.write_all(bytes)
        .and_then(|_| file.sync_all())
        .map_err(|_| "无法保存更新安装包，请检查磁盘空间和权限")?;
    drop(file);
    std::process::Command::new(path)
        .arg("/S")
        .arg("/UPDATE")
        .arg(format!("/VOICEBAR_PID={}", std::process::id()))
        .spawn()
        .map_err(|_| "无法启动更新安装程序，请手动下载安装包")?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn signed_fixture(comment: &str) -> (Vec<u8>, Release, String) {
        let pair = minisign::KeyPair::generate_unencrypted_keypair().unwrap();
        let bytes = b"owned offline installer fixture".to_vec();
        let signature = minisign::sign(
            Some(&pair.pk),
            &pair.sk,
            std::io::Cursor::new(&bytes),
            Some(comment),
            None,
        )
        .unwrap();
        let key = STANDARD.encode(pair.pk.to_box().unwrap().to_string());
        let release = Release {
            version: Version::new(1, 2, 3),
            notes: None,
            date: None,
            artifact: Artifact {
                url: "https://example.com/VoiceBar.exe".into(),
                signature: STANDARD.encode(signature.to_string()),
                sha256: None,
            },
        };
        (bytes, release, key)
    }
    #[test]
    fn accepts_authentic_installer_and_its_signed_version() {
        let (bytes, release, key) = signed_fixture("version:1.2.3");
        verify_signed_installer(&bytes, &release, &key).unwrap();
    }
    #[test]
    fn rejects_modified_installer_and_manifest_version_substitution() {
        let (mut bytes, mut release, key) = signed_fixture("timestamp:1\tversion:1.2.3");
        bytes[0] ^= 1;
        assert!(verify_signed_installer(&bytes, &release, &key).is_err());
        bytes[0] ^= 1;
        release.version = Version::new(2, 0, 0);
        assert!(verify_signed_installer(&bytes, &release, &key)
            .unwrap_err()
            .contains("版本"));
    }
    #[test]
    fn rejects_unsigned_and_ambiguous_version_comments() {
        for comment in ["timestamp:1", "version:1.2.3\tversion:1.2.3"] {
            let (bytes, release, key) = signed_fixture(comment);
            assert!(verify_signed_installer(&bytes, &release, &key).is_err());
        }
    }
    #[test]
    fn keeps_builtin_public_key_and_https_feed_readable() {
        let key = STANDARD.decode(PUBKEY).unwrap();
        PublicKey::decode(std::str::from_utf8(&key).unwrap()).unwrap();
        assert!(https_url(ENDPOINT).is_ok());
        assert!(https_url("http://example.com/update").is_err());
    }
}
