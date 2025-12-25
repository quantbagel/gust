//! Version checking utilities for update and outdated commands.

use gust_lockfile::LockedPackage;
use semver::Version;

/// Information about an outdated package.
#[derive(Debug, Clone)]
pub struct OutdatedPackage {
    pub name: String,
    pub current: String,
    pub latest: String,
    pub latest_tag: String,
}

/// Check a locked package for available updates.
/// Returns None if up-to-date or no valid tags found.
pub async fn check_for_update(pkg: &LockedPackage) -> Option<OutdatedPackage> {
    let git_url = pkg.git.as_ref()?;
    let name = pkg.name.clone();
    let current_version = pkg.version.to_string();

    match gust_fetch::list_remote_tags(git_url).await {
        Ok(tags) => {
            // Find the latest semver tag
            let latest = tags.iter().find(|t| t.version.is_some())?;
            let latest_version = latest.version.as_ref()?;
            let current = Version::parse(&current_version).ok();

            if let Some(ref curr) = current {
                if latest_version > curr {
                    return Some(OutdatedPackage {
                        name,
                        current: current_version,
                        latest: latest_version.to_string(),
                        latest_tag: latest.name.clone(),
                    });
                }
            } else {
                // Current version isn't semver, show latest anyway
                return Some(OutdatedPackage {
                    name,
                    current: current_version,
                    latest: latest_version.to_string(),
                    latest_tag: latest.name.clone(),
                });
            }
            None
        }
        Err(e) => {
            tracing::warn!("Failed to check {} for updates: {}", name, e);
            None
        }
    }
}

/// Check multiple packages for updates in parallel.
pub async fn check_all_for_updates(packages: &[&LockedPackage]) -> Vec<OutdatedPackage> {
    let mut tasks = Vec::new();

    for pkg in packages {
        if pkg.git.is_some() {
            let pkg_clone = (*pkg).clone();
            tasks.push(tokio::spawn(async move {
                check_for_update(&pkg_clone).await
            }));
        }
    }

    let mut outdated = Vec::new();
    for task in tasks {
        if let Ok(Some(info)) = task.await {
            outdated.push(info);
        }
    }

    outdated
}

/// Filter updates by semver compatibility.
/// If `allow_breaking` is false, only returns updates within same major version.
pub fn filter_breaking(updates: Vec<OutdatedPackage>, allow_breaking: bool) -> Vec<OutdatedPackage> {
    if allow_breaking {
        return updates;
    }

    updates
        .into_iter()
        .filter(|u| {
            let Ok(curr) = Version::parse(&u.current) else {
                return true;
            };
            let Ok(latest) = Version::parse(&u.latest) else {
                return true;
            };

            // Only non-breaking updates (same major version, or 0.x.y -> 0.x.z)
            if curr.major == 0 && latest.major == 0 {
                curr.minor == latest.minor
            } else {
                curr.major == latest.major
            }
        })
        .collect()
}
