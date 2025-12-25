//! Swift Package Index integration.
//!
//! Fetches and caches the package list from Swift Package Index for search.

use miette::{IntoDiagnostic, Result};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;
use std::time::{Duration, SystemTime};

const PACKAGE_LIST_URL: &str =
    "https://raw.githubusercontent.com/SwiftPackageIndex/PackageList/main/packages.json";

/// Cache duration: 24 hours
const CACHE_DURATION: Duration = Duration::from_secs(24 * 60 * 60);

/// A parsed package from the Swift Package Index.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct IndexedPackage {
    /// Package name (extracted from URL)
    pub name: String,
    /// GitHub organization/owner
    pub owner: String,
    /// Full git URL
    pub url: String,
}

/// Cached package list.
#[derive(Debug, Serialize, Deserialize)]
struct PackageCache {
    /// When the cache was last updated
    updated_at: u64,
    /// List of package URLs
    packages: Vec<String>,
}

impl PackageCache {
    fn is_stale(&self) -> bool {
        let now = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
        now - self.updated_at > CACHE_DURATION.as_secs()
    }
}

/// Get the cache file path.
fn cache_path() -> Option<PathBuf> {
    dirs::cache_dir().map(|d| d.join("gust").join("package-index.json"))
}

/// Load cached package list.
fn load_cache() -> Option<PackageCache> {
    let path = cache_path()?;
    let content = fs::read_to_string(&path).ok()?;
    serde_json::from_str(&content).ok()
}

/// Save package list to cache.
fn save_cache(packages: &[String]) -> Result<()> {
    let Some(path) = cache_path() else {
        return Ok(());
    };

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).into_diagnostic()?;
    }

    let cache = PackageCache {
        updated_at: SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs(),
        packages: packages.to_vec(),
    };

    let content = serde_json::to_string(&cache).into_diagnostic()?;
    fs::write(&path, content).into_diagnostic()?;
    Ok(())
}

/// Parse a GitHub URL into owner and name.
fn parse_github_url(url: &str) -> Option<(String, String)> {
    // Format: https://github.com/owner/repo.git
    let url = url.trim_end_matches(".git");
    let parts: Vec<&str> = url.split('/').collect();

    if parts.len() >= 2 {
        let name = parts.last()?.to_string();
        let owner = parts.get(parts.len() - 2)?.to_string();
        Some((owner, name))
    } else {
        None
    }
}

/// Fetch the package list from Swift Package Index.
pub async fn fetch_package_list() -> Result<Vec<String>> {
    // Check cache first
    if let Some(cache) = load_cache() {
        if !cache.is_stale() {
            return Ok(cache.packages);
        }
    }

    // Fetch fresh list
    let client = reqwest::Client::new();
    let resp = client
        .get(PACKAGE_LIST_URL)
        .send()
        .await
        .into_diagnostic()?;

    if !resp.status().is_success() {
        return Err(miette::miette!(
            "Failed to fetch package list: HTTP {}",
            resp.status()
        ));
    }

    let packages: Vec<String> = resp.json().await.into_diagnostic()?;

    // Cache the result
    let _ = save_cache(&packages);

    Ok(packages)
}

/// Search for packages matching a query.
pub async fn search_packages(query: &str, limit: usize) -> Result<Vec<IndexedPackage>> {
    let packages = fetch_package_list().await?;
    let query_lower = query.to_lowercase();

    let mut matches: Vec<IndexedPackage> = packages
        .iter()
        .filter_map(|url| {
            let (owner, name) = parse_github_url(url)?;
            // Match against package name or owner
            if name.to_lowercase().contains(&query_lower)
                || owner.to_lowercase().contains(&query_lower)
            {
                Some(IndexedPackage {
                    name,
                    owner,
                    url: url.clone(),
                })
            } else {
                None
            }
        })
        .collect();

    // Sort by relevance: exact name matches first, then name starts with, then contains
    matches.sort_by(|a, b| {
        let a_name_lower = a.name.to_lowercase();
        let b_name_lower = b.name.to_lowercase();

        // Exact match
        let a_exact = a_name_lower == query_lower;
        let b_exact = b_name_lower == query_lower;
        if a_exact != b_exact {
            return b_exact.cmp(&a_exact);
        }

        // Starts with
        let a_starts = a_name_lower.starts_with(&query_lower);
        let b_starts = b_name_lower.starts_with(&query_lower);
        if a_starts != b_starts {
            return b_starts.cmp(&a_starts);
        }

        // Alphabetical
        a_name_lower.cmp(&b_name_lower)
    });

    Ok(matches.into_iter().take(limit).collect())
}

/// Get the total number of indexed packages.
pub async fn package_count() -> Result<usize> {
    let packages = fetch_package_list().await?;
    Ok(packages.len())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_github_url() {
        let (owner, name) =
            parse_github_url("https://github.com/apple/swift-log.git").unwrap();
        assert_eq!(owner, "apple");
        assert_eq!(name, "swift-log");

        let (owner, name) =
            parse_github_url("https://github.com/vapor/vapor").unwrap();
        assert_eq!(owner, "vapor");
        assert_eq!(name, "vapor");
    }
}
