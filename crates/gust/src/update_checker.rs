//! Self-update checker for Gust.
//!
//! Checks for new versions in the background and notifies the user.
//! Inspired by uv's update mechanism.

use console::style;
use semver::Version;
use serde::{Deserialize, Serialize};
use std::env;
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

const GITHUB_REPO: &str = "quantbagel/gust";
const CHECK_INTERVAL: Duration = Duration::from_secs(24 * 60 * 60); // 24 hours

/// Cached update check result.
#[derive(Debug, Serialize, Deserialize)]
struct UpdateCache {
    /// Last check timestamp (seconds since UNIX epoch)
    last_check: u64,
    /// Latest version found
    latest_version: Option<String>,
}

/// Get the cache file path.
fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|p| p.join("gust").join("update_check.json"))
}

/// Read the cached update check.
fn read_cache() -> Option<UpdateCache> {
    let path = cache_path()?;
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Write the update check cache.
fn write_cache(cache: &UpdateCache) {
    if let Some(path) = cache_path() {
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&path, serde_json::to_string(cache).unwrap_or_default());
    }
}

/// Get current timestamp in seconds.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}

/// Check if we should perform an update check.
fn should_check() -> bool {
    // Don't check in CI environments
    if env::var("CI").is_ok() || env::var("GUST_NO_UPDATE_CHECK").is_ok() {
        return false;
    }

    match read_cache() {
        Some(cache) => {
            let elapsed = now_secs().saturating_sub(cache.last_check);
            elapsed >= CHECK_INTERVAL.as_secs()
        }
        None => true,
    }
}

/// GitHub release response (minimal).
#[derive(Debug, Deserialize)]
struct GitHubRelease {
    tag_name: String,
}

/// Fetch the latest version from GitHub.
async fn fetch_latest_version() -> Option<String> {
    let url = format!(
        "https://api.github.com/repos/{}/releases/latest",
        GITHUB_REPO
    );

    let client = reqwest::Client::builder()
        .user_agent("gust-update-checker")
        .timeout(Duration::from_secs(5))
        .build()
        .ok()?;

    let response = client.get(&url).send().await.ok()?;

    if !response.status().is_success() {
        return None;
    }

    let release: GitHubRelease = response.json().await.ok()?;
    let version = release.tag_name.trim_start_matches('v').to_string();

    Some(version)
}

/// Get the current version of gust.
pub fn current_version() -> &'static str {
    env!("CARGO_PKG_VERSION")
}

/// Check for updates and print a notification if available.
/// This is designed to be called after the main command completes.
pub async fn check_and_notify() {
    if !should_check() {
        // Check cache for previously found update
        if let Some(cache) = read_cache() {
            if let Some(latest) = &cache.latest_version {
                if is_newer(latest) {
                    print_update_notification(latest);
                }
            }
        }
        return;
    }

    // Perform the check
    if let Some(latest) = fetch_latest_version().await {
        let cache = UpdateCache {
            last_check: now_secs(),
            latest_version: Some(latest.clone()),
        };
        write_cache(&cache);

        if is_newer(&latest) {
            print_update_notification(&latest);
        }
    } else {
        // Update cache timestamp even on failure to avoid hammering
        let cache = UpdateCache {
            last_check: now_secs(),
            latest_version: read_cache().and_then(|c| c.latest_version),
        };
        write_cache(&cache);
    }
}

/// Check if the given version is newer than current.
fn is_newer(latest: &str) -> bool {
    let current = Version::parse(current_version()).ok();
    let latest = Version::parse(latest).ok();

    match (current, latest) {
        (Some(c), Some(l)) => l > c,
        _ => false,
    }
}

/// Print the update notification.
fn print_update_notification(latest: &str) {
    eprintln!();
    eprintln!(
        "{} {} → {}",
        style("A new version of gust is available:").dim(),
        style(current_version()).yellow(),
        style(latest).green().bold()
    );
    eprintln!("{}", style("Run `gust self update` to update").dim());
}

/// Download and install the latest version.
pub async fn self_update() -> miette::Result<()> {
    use miette::IntoDiagnostic;

    println!("{} Checking for updates...", style("→").blue().bold());

    let latest = fetch_latest_version()
        .await
        .ok_or_else(|| miette::miette!("Failed to fetch latest version from GitHub"))?;

    let current = current_version();

    if !is_newer(&latest) {
        println!(
            "{} Already up to date ({})",
            style("✓").green().bold(),
            current
        );
        return Ok(());
    }

    println!(
        "{} Updating {} → {}",
        style("→").blue().bold(),
        style(current).yellow(),
        style(&latest).green()
    );

    // Detect platform
    let target = detect_target()?;

    // Download URL
    let download_url = format!(
        "https://github.com/{}/releases/download/v{}/gust-{}.tar.gz",
        GITHUB_REPO, latest, target
    );

    println!(
        "{} Downloading from {}",
        style("→").blue().bold(),
        style(&download_url).dim()
    );

    // Download the tarball
    let client = reqwest::Client::builder()
        .user_agent("gust-self-update")
        .timeout(Duration::from_secs(300))
        .build()
        .into_diagnostic()?;

    let response = client.get(&download_url).send().await.into_diagnostic()?;

    if !response.status().is_success() {
        return Err(miette::miette!(
            "Failed to download: HTTP {}",
            response.status()
        ));
    }

    let bytes = response.bytes().await.into_diagnostic()?;

    // Get current executable path
    let current_exe = env::current_exe().into_diagnostic()?;

    // Extract to temp location
    let temp_dir = tempfile::tempdir().into_diagnostic()?;
    let tar_gz = flate2::read::GzDecoder::new(&bytes[..]);
    let mut archive = tar::Archive::new(tar_gz);
    archive.unpack(temp_dir.path()).into_diagnostic()?;

    // Find the gust binary in extracted files
    let new_binary = temp_dir.path().join("gust");
    if !new_binary.exists() {
        return Err(miette::miette!(
            "Could not find gust binary in downloaded archive"
        ));
    }

    // Replace the current binary
    let backup_path = current_exe.with_extension("old");

    // On Unix, we can replace the running binary
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        // Make new binary executable
        let mut perms = fs::metadata(&new_binary).into_diagnostic()?.permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&new_binary, perms).into_diagnostic()?;

        // Backup current binary
        if current_exe.exists() {
            let _ = fs::remove_file(&backup_path);
            fs::rename(&current_exe, &backup_path).into_diagnostic()?;
        }

        // Move new binary into place
        fs::copy(&new_binary, &current_exe).into_diagnostic()?;

        // Clean up backup
        let _ = fs::remove_file(&backup_path);
    }

    // Clear the update cache
    if let Some(path) = cache_path() {
        let _ = fs::remove_file(&path);
    }

    println!(
        "{} Updated to gust {}",
        style("✓").green().bold(),
        style(&latest).cyan()
    );

    Ok(())
}

/// Detect the current platform target triple.
fn detect_target() -> miette::Result<&'static str> {
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    return Ok("x86_64-apple-darwin");

    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    return Ok("aarch64-apple-darwin");

    #[cfg(all(target_os = "linux", target_arch = "x86_64"))]
    return Ok("x86_64-unknown-linux-gnu");

    #[cfg(all(target_os = "linux", target_arch = "aarch64"))]
    return Ok("aarch64-unknown-linux-gnu");

    #[cfg(not(any(
        all(target_os = "macos", target_arch = "x86_64"),
        all(target_os = "macos", target_arch = "aarch64"),
        all(target_os = "linux", target_arch = "x86_64"),
        all(target_os = "linux", target_arch = "aarch64"),
    )))]
    return Err(miette::miette!("Unsupported platform for self-update"));
}
