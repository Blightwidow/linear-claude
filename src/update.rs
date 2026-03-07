use crate::version::{self, version_lt};
use anyhow::{bail, Context, Result};
use sha2::{Digest, Sha256};
use std::fs;
use std::io::{Read, Write};
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

const GITHUB_RELEASE_REPO: &str = "Blightwidow/linear-claude";
const CACHE_MAX_AGE_SECS: u64 = 86400; // 24 hours

fn cache_dir() -> PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| PathBuf::from("/tmp"))
        .join("linear-claude")
}

fn cache_file() -> PathBuf {
    cache_dir().join("latest-version")
}

fn now_epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

pub fn check_for_updates() {
    let _ = check_for_updates_inner();
}

fn check_for_updates_inner() -> Result<()> {
    let cache = cache_file();
    if cache.exists() {
        let content = fs::read_to_string(&cache).unwrap_or_default();
        let mut lines = content.lines();
        let cache_time: u64 = lines.next().and_then(|l| l.parse().ok()).unwrap_or(0);
        let cached_version = lines.next().unwrap_or("").to_string();

        let age = now_epoch().saturating_sub(cache_time);
        if age < CACHE_MAX_AGE_SECS {
            if !cached_version.is_empty() && version_lt(version::VERSION, &cached_version) {
                eprintln!(
                    "Warning: A newer version of linear-claude is available: {cached_version} (current: {})",
                    version::VERSION
                );
                eprintln!("   Run 'linear-claude update' to upgrade.");
                eprintln!();
            }
            return Ok(());
        }
    }

    let agent = ureq::Agent::new();
    let resp = agent
        .get(&format!(
            "https://api.github.com/repos/{GITHUB_RELEASE_REPO}/releases/latest"
        ))
        .set("User-Agent", "linear-claude")
        .call();

    let resp = match resp {
        Ok(r) => r,
        Err(_) => return Ok(()),
    };

    let body: serde_json::Value = resp.into_json()?;
    let latest_tag = body["tag_name"].as_str().unwrap_or("").to_string();

    if latest_tag.is_empty() {
        return Ok(());
    }

    let dir = cache_dir();
    fs::create_dir_all(&dir).ok();
    if let Ok(mut f) = fs::File::create(cache_file()) {
        let _ = writeln!(f, "{}", now_epoch());
        let _ = writeln!(f, "{latest_tag}");
    }

    if version_lt(version::VERSION, &latest_tag) {
        eprintln!(
            "Warning: A newer version of linear-claude is available: {latest_tag} (current: {})",
            version::VERSION
        );
        eprintln!("   Run 'linear-claude update' to upgrade.");
        eprintln!();
    }

    Ok(())
}

pub fn cmd_update() -> Result<()> {
    eprintln!("Checking for updates...");

    let agent = ureq::Agent::new();
    let resp = agent
        .get(&format!(
            "https://api.github.com/repos/{GITHUB_RELEASE_REPO}/releases/latest"
        ))
        .set("User-Agent", "linear-claude")
        .call()
        .context("Failed to fetch release information")?;

    let body: serde_json::Value = resp.into_json()?;
    let latest_tag = body["tag_name"]
        .as_str()
        .context("No releases found")?
        .to_string();

    if !version_lt(version::VERSION, &latest_tag) {
        eprintln!("Already up to date (version {}).", version::VERSION);
        return Ok(());
    }

    eprintln!("Updating from {} to {latest_tag}...", version::VERSION);

    let target = current_target();
    let assets = body["assets"].as_array().context("No assets in release")?;

    let binary_asset = assets
        .iter()
        .find(|a| {
            a["name"]
                .as_str()
                .map(|n| n.contains(&target))
                .unwrap_or(false)
        })
        .context(format!("No binary found for platform: {target}"))?;

    let binary_url = binary_asset["browser_download_url"]
        .as_str()
        .context("No download URL for binary")?;

    let checksum_asset = assets.iter().find(|a| {
        a["name"]
            .as_str()
            .map(|n| n.ends_with(".sha256"))
            .unwrap_or(false)
    });

    eprintln!("Downloading...");
    let resp = agent
        .get(binary_url)
        .call()
        .context("Failed to download binary")?;
    let mut binary_data = Vec::new();
    resp.into_reader()
        .read_to_end(&mut binary_data)
        .context("Failed to read binary data")?;

    if let Some(checksum_asset) = checksum_asset {
        if let Some(checksum_url) = checksum_asset["browser_download_url"].as_str() {
            eprintln!("Verifying checksum...");
            let checksum_text = agent
                .get(checksum_url)
                .call()
                .context("Failed to download checksum")?
                .into_string()
                .context("Failed to read checksum")?;

            let expected_sha = checksum_text
                .split_whitespace()
                .next()
                .unwrap_or("")
                .to_string();

            let actual_sha = compute_sha256(&binary_data);

            if expected_sha != actual_sha {
                bail!(
                    "Checksum verification failed!\n  Expected: {expected_sha}\n  Got:      {actual_sha}"
                );
            }
            eprintln!("Checksum verified.");
        }
    }

    let install_path = std::env::current_exe().context("Could not determine install location")?;
    eprintln!("Installing to: {}", install_path.display());

    if fs::write(&install_path, &binary_data).is_err() {
        #[cfg(unix)]
        {
            let tmp = tempfile(&binary_data)?;
            std::process::Command::new("sudo")
                .args(["cp", &tmp.to_string_lossy(), &install_path.to_string_lossy()])
                .status()
                .context("Failed to install with sudo")?;
            fs::remove_file(tmp).ok();
        }
        #[cfg(not(unix))]
        bail!("Cannot write to {}", install_path.display());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&install_path, fs::Permissions::from_mode(0o755)).ok();
    }

    let dir = cache_dir();
    fs::create_dir_all(&dir).ok();
    if let Ok(mut f) = fs::File::create(cache_file()) {
        let _ = writeln!(f, "{}", now_epoch());
        let _ = writeln!(f, "{latest_tag}");
    }

    eprintln!("Updated to {latest_tag} successfully!");
    Ok(())
}

fn current_target() -> String {
    let arch = std::env::consts::ARCH;
    let os = std::env::consts::OS;
    format!("{arch}-{os}")
}

fn compute_sha256(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

#[cfg(unix)]
fn tempfile(data: &[u8]) -> Result<PathBuf> {
    let path = std::env::temp_dir().join("linear-claude-update");
    fs::write(&path, data)?;
    Ok(path)
}
