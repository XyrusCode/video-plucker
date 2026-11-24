//! One-click updater, replacing `tauri-plugin-updater`.
//!
//! Reads the same `latest.json` the existing release workflow publishes: a
//! Tauri-style manifest with a minisign signature per platform. The asset is
//! downloaded, its signature verified against the key that was in
//! `tauri.conf.json`, and the platform installer is launched.

use std::collections::HashMap;
use std::path::PathBuf;

use base64::Engine as _;
use minisign_verify::{PublicKey, Signature};
use serde::Deserialize;

/// The public key previously shipped in `tauri.conf.json`. It is base64 of a
/// two-line minisign public-key file; `PublicKey::from_base64` wants the raw
/// key line, so decode once and keep the line that starts with "RW".
const PUBKEY_B64: &str = "dW50cnVzdGVkIGNvbW1lbnQ6IG1pbmlzaWduIHB1YmxpYyBrZXk6IDRCRURGOEU2MjFDMEI4RQpSV1NPQ3h4aWp0KytCTkNOS0NIVFJQU3lmbWVJTkVCMExRVWhYb2hrbXlyVElzR3ZFcDJlNmR2Qgo=";

#[derive(Clone, Debug, Deserialize)]
pub struct PlatformAsset {
    #[serde(default)]
    pub signature: String,
    #[serde(default)]
    pub checksum: Option<String>,
    pub url: String,
}

#[derive(Clone, Debug, Deserialize)]
pub struct LatestJson {
    pub version: String,
    #[serde(default)]
    pub notes: Option<String>,
    pub platforms: HashMap<String, PlatformAsset>,
}

#[derive(Clone, Debug)]
pub struct UpdateInfo {
    pub version: String,
    pub notes: String,
    pub asset: PlatformAsset,
}

fn platform_key() -> &'static str {
    #[cfg(all(windows, target_arch = "x86_64"))]
    {
        "windows-x86_64"
    }
    #[cfg(all(windows, target_arch = "aarch64"))]
    {
        "windows-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    {
        "darwin-aarch64"
    }
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    {
        "darwin-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    {
        "linux-x86_64"
    }
    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    {
        "linux-aarch64"
    }
    #[cfg(not(any(
        all(windows, target_arch = "x86_64"),
        all(windows, target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64")
    )))]
    {
        ""
    }
}

/// Numeric dotted-version comparison. Returns true when `latest` is newer.
pub fn is_newer(latest: &str, current: &str) -> bool {
    fn parts(v: &str) -> Vec<u64> {
        v.trim()
            .trim_start_matches('v')
            .split(|c: char| !c.is_ascii_digit())
            .filter(|s| !s.is_empty())
            .filter_map(|s| s.parse().ok())
            .collect()
    }
    let (a, b) = (parts(latest), parts(current));
    for i in 0..a.len().max(b.len()) {
        let x = *a.get(i).unwrap_or(&0);
        let y = *b.get(i).unwrap_or(&0);
        if x != y {
            return x > y;
        }
    }
    false
}

pub async fn check(force: bool) -> Result<Option<UpdateInfo>, String> {
    let url = "https://github.com/XyrusCode/video-plucker/releases/latest/download/latest.json";
    let latest: LatestJson = crate::extractors::client()
        .get(url)
        .send()
        .await
        .map_err(|e| format!("update check failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("update check failed: {e}"))?
        .json()
        .await
        .map_err(|e| format!("bad update manifest: {e}"))?;

    let current = env!("CARGO_PKG_VERSION");
    if !force && !is_newer(&latest.version, current) {
        return Ok(None);
    }
    let asset = latest
        .platforms
        .get(platform_key())
        .cloned()
        .ok_or_else(|| "no update asset for this platform".to_string())?;
    Ok(Some(UpdateInfo {
        version: latest.version,
        notes: latest.notes.unwrap_or_default(),
        asset,
    }))
}

/// Download the installer and verify its detached minisign signature.
pub async fn download(info: &UpdateInfo) -> Result<PathBuf, String> {
    let bytes = crate::extractors::client()
        .get(&info.asset.url)
        .send()
        .await
        .map_err(|e| format!("download failed: {e}"))?
        .error_for_status()
        .map_err(|e| format!("download failed: {e}"))?
        .bytes()
        .await
        .map_err(|e| format!("download failed: {e}"))?;

    if info.asset.signature.trim().is_empty() {
        // Releases without a minisign signature carry a SHA-256 checksum in
        // the same manifest. Both are published by CI over HTTPS.
        use sha2::Digest as _;
        let expected = info
            .asset
            .checksum
            .as_deref()
            .ok_or("update manifest has no signature or checksum")?;
        let digest = sha2::Sha256::digest(&bytes);
        let actual = digest.iter().map(|b| format!("{b:02x}")).collect::<String>();
        if actual != expected.trim().to_ascii_lowercase() {
            return Err("update checksum did not verify".into());
        }
    } else {
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(PUBKEY_B64)
            .map_err(|e| format!("bad update key: {e}"))?;
        let text = String::from_utf8_lossy(&decoded);
        let raw_key = text
            .lines()
            .find(|l| !l.starts_with("untrusted") && !l.trim().is_empty())
            .ok_or("bad update key")?;
        let key = PublicKey::from_base64(raw_key).map_err(|e| format!("bad update key: {e}"))?;
        let sig = Signature::decode(&info.asset.signature).map_err(|e| format!("bad signature: {e}"))?;
        key.verify(&bytes, &sig, false)
            .map_err(|e| format!("update signature did not verify: {e}"))?;
    }

    let name = info
        .asset
        .url
        .rsplit('/')
        .next()
        .unwrap_or("update")
        .to_string();
    let safe: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .collect();
    let path = std::env::temp_dir().join(format!("yt-plucker-{}-{}", info.version, safe));
    std::fs::write(&path, &bytes).map_err(|e| format!("could not save update: {e}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755));
    }
    Ok(path)
}

/// Launch the platform installer. Returns after spawning; the app should quit
/// so the installer can replace its files.
pub fn install(path: &PathBuf) -> Result<(), String> {
    #[cfg(windows)]
    {
        let mut cmd = std::process::Command::new(path);
        cmd.arg("/S");
        cmd.spawn().map_err(|e| format!("could not start installer: {e}"))?;
    }
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg(path)
            .spawn()
            .map_err(|e| format!("could not open disk image: {e}"))?;
    }
    #[cfg(target_os = "linux")]
    {
        let mut cmd = std::process::Command::new("pkexec");
        cmd.args(["dpkg", "-i"]).arg(path);
        cmd.spawn().map_err(|e| format!("could not start installer: {e}"))?;
    }
    Ok(())
}
