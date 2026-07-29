use crate::output::{OutputData, print_output};
use chrono::{DateTime, Duration, Utc};
use clap::Subcommand;
use orquestra_core::config::OutputFormat;
use orquestra_core::error::OrquestraError;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::time::{Duration as StdDuration, Instant};

const PACKAGE_NAME: &str = "@jonathlima/orquestra";
const CACHE_TTL_HOURS: i64 = 24;
const NPM_TIMEOUT: StdDuration = StdDuration::from_secs(3);
const MAX_VERSION_OUTPUT_BYTES: usize = 1024;

#[derive(Debug, Subcommand)]
pub enum UpdateAction {
    /// Check npm for a newer Orquestra version
    Check,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCheckOutput {
    status: UpdateStatus,
    current_version: String,
    latest_version: Option<String>,
    source: UpdateSource,
    checked_at: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "kebab-case")]
enum UpdateStatus {
    Available,
    UpToDate,
    Unknown,
    Disabled,
}

#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum UpdateSource {
    Registry,
    Cache,
    None,
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
struct UpdateCache {
    latest_version: String,
    checked_at: String,
}

impl OutputData for UpdateCheckOutput {
    fn render_human(&self) -> String {
        match (self.status, self.latest_version.as_deref()) {
            (UpdateStatus::Available, Some(latest)) => format!(
                "Orquestra update available: {} -> {}\nRun: npm install --global {PACKAGE_NAME}@latest",
                self.current_version, latest
            ),
            (UpdateStatus::UpToDate, Some(latest)) => {
                format!("Orquestra is up to date ({latest}).")
            }
            (UpdateStatus::Disabled, _) => "Orquestra update check is disabled.".to_string(),
            _ => "Could not check for Orquestra updates. Continuing offline.".to_string(),
        }
    }
}

pub fn run(action: &UpdateAction, output: &OutputFormat) -> Result<(), OrquestraError> {
    match action {
        UpdateAction::Check => {
            print_output(&check(), output);
            Ok(())
        }
    }
}

fn check() -> UpdateCheckOutput {
    let current_version = env!("CARGO_PKG_VERSION").to_string();
    if update_check_disabled() {
        return UpdateCheckOutput {
            status: UpdateStatus::Disabled,
            current_version,
            latest_version: None,
            source: UpdateSource::None,
            checked_at: None,
        };
    }

    let cache_path = cache_file();
    if let Some(cache) = read_fresh_cache(&cache_path) {
        return output_for_version(
            current_version,
            cache.latest_version,
            UpdateSource::Cache,
            Some(cache.checked_at),
        );
    }

    let Some(latest_version) = query_npm_version() else {
        return UpdateCheckOutput {
            status: UpdateStatus::Unknown,
            current_version,
            latest_version: None,
            source: UpdateSource::None,
            checked_at: None,
        };
    };

    let checked_at = Utc::now().to_rfc3339();
    let cache = UpdateCache {
        latest_version: latest_version.clone(),
        checked_at: checked_at.clone(),
    };
    write_cache(&cache_path, &cache);
    output_for_version(
        current_version,
        latest_version,
        UpdateSource::Registry,
        Some(checked_at),
    )
}

fn output_for_version(
    current_version: String,
    latest_version: String,
    source: UpdateSource,
    checked_at: Option<String>,
) -> UpdateCheckOutput {
    let status = match (
        parse_stable_version(&current_version),
        parse_stable_version(&latest_version),
    ) {
        (Some(current), Some(latest)) if latest > current => UpdateStatus::Available,
        (Some(_), Some(_)) => UpdateStatus::UpToDate,
        _ => UpdateStatus::Unknown,
    };
    UpdateCheckOutput {
        status,
        current_version,
        latest_version: Some(latest_version),
        source,
        checked_at,
    }
}

fn parse_stable_version(version: &str) -> Option<(u64, u64, u64)> {
    let mut parts = version.split('.');
    let parsed = (
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
        parts.next()?.parse().ok()?,
    );
    parts.next().is_none().then_some(parsed)
}

fn update_check_disabled() -> bool {
    matches!(
        std::env::var("ORQUESTRA_DISABLE_UPDATE_CHECK")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

fn cache_file() -> PathBuf {
    if let Some(path) = std::env::var_os("ORQUESTRA_UPDATE_CACHE_DIR") {
        return PathBuf::from(path).join("update-check.json");
    }
    #[cfg(windows)]
    let root = std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            orquestra_core::config::home_dir()
                .join("AppData")
                .join("Local")
        });
    #[cfg(target_os = "macos")]
    let root = orquestra_core::config::home_dir()
        .join("Library")
        .join("Caches");
    #[cfg(all(unix, not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CACHE_HOME")
        .map(PathBuf::from)
        .unwrap_or_else(|| orquestra_core::config::home_dir().join(".cache"));
    root.join("orquestra").join("update-check.json")
}

fn read_fresh_cache(path: &PathBuf) -> Option<UpdateCache> {
    let bytes = std::fs::read(path).ok()?;
    let cache: UpdateCache = serde_json::from_slice(&bytes).ok()?;
    parse_stable_version(&cache.latest_version)?;
    let checked_at = DateTime::parse_from_rfc3339(&cache.checked_at)
        .ok()?
        .with_timezone(&Utc);
    let age = Utc::now().signed_duration_since(checked_at);
    (age >= Duration::zero() && age < Duration::hours(CACHE_TTL_HOURS)).then_some(cache)
}

fn write_cache(path: &PathBuf, cache: &UpdateCache) {
    let Some(parent) = path.parent() else {
        return;
    };
    if std::fs::create_dir_all(parent).is_err() {
        return;
    }
    let Ok(bytes) = serde_json::to_vec(cache) else {
        return;
    };
    let _ = std::fs::write(path, bytes);
}

fn query_npm_version() -> Option<String> {
    let npm = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let mut child = Command::new(npm)
        .args(["--silent", "view", PACKAGE_NAME, "version", "--json"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let deadline = Instant::now() + NPM_TIMEOUT;
    loop {
        match child.try_wait() {
            Ok(Some(status)) if status.success() => break,
            Ok(Some(_)) | Err(_) => return None,
            Ok(None) if Instant::now() < deadline => {
                std::thread::sleep(StdDuration::from_millis(10));
            }
            Ok(None) => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    }
    let output = child.wait_with_output().ok()?;
    if output.stdout.len() > MAX_VERSION_OUTPUT_BYTES {
        return None;
    }
    let version = serde_json::from_slice::<String>(&output.stdout).ok()?;
    parse_stable_version(&version)?;
    Some(version)
}
