//! Parallel package fetching for Gust.

use std::path::PathBuf;
use std::sync::Arc;
use gust_types::Dependency;
use thiserror::Error;
use tokio::sync::Semaphore;

#[derive(Error, Debug)]
pub enum FetchError {
    #[error("Failed to fetch {package}: {message}")]
    FetchFailed { package: String, message: String },
    #[error("Git error: {0}")]
    GitError(String),
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
}

/// Result of fetching a package.
#[derive(Debug)]
pub struct FetchResult {
    /// Package name
    pub name: String,
    /// Path where package was fetched
    pub path: PathBuf,
    /// Content hash
    pub checksum: String,
    /// Git revision (if git dependency)
    pub revision: Option<String>,
}

/// Status updates during fetch operations.
#[derive(Debug, Clone)]
pub enum FetchStatus {
    /// Fetch has started
    Started,
    /// Fetch completed successfully
    Completed,
    /// Fetch failed with error message
    Failed(String),
}

/// Fetch packages in parallel.
pub struct Fetcher {
    /// Number of concurrent downloads
    concurrency: usize,
}

impl Default for Fetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl Fetcher {
    pub fn new() -> Self {
        Self { concurrency: 8 }
    }

    pub fn with_concurrency(mut self, n: usize) -> Self {
        self.concurrency = n;
        self
    }

    /// Fetch a single dependency.
    pub async fn fetch(&self, dep: &Dependency, dest: &PathBuf) -> Result<FetchResult, FetchError> {
        match dep.source_kind() {
            gust_types::DependencySource::Git => self.fetch_git(dep, dest).await,
            gust_types::DependencySource::Registry => self.fetch_registry(dep, dest).await,
            gust_types::DependencySource::Path => self.fetch_path(dep, dest).await,
        }
    }

    /// Fetch multiple dependencies in parallel.
    ///
    /// Uses a semaphore to limit concurrent fetches to `self.concurrency`.
    /// Returns results as they complete, with package name as key.
    pub async fn fetch_many<F>(
        &self,
        deps: Vec<(Dependency, PathBuf)>,
        on_progress: F,
    ) -> Vec<Result<FetchResult, FetchError>>
    where
        F: FnMut(&str, FetchStatus) + Send + 'static,
    {
        use futures::future::join_all;
        use std::sync::Mutex;

        let semaphore = Arc::new(Semaphore::new(self.concurrency));
        let on_progress = Arc::new(Mutex::new(on_progress));

        let tasks: Vec<_> = deps
            .into_iter()
            .map(|(dep, dest)| {
                let sem = Arc::clone(&semaphore);
                let progress = Arc::clone(&on_progress);
                let name = dep.name.clone();

                async move {
                    // Acquire permit (limits concurrency)
                    let _permit = sem.acquire().await.unwrap();

                    // Notify start
                    if let Ok(mut cb) = progress.lock() {
                        cb(&name, FetchStatus::Started);
                    }

                    // Perform fetch
                    let result = match dep.source_kind() {
                        gust_types::DependencySource::Git => {
                            Self::fetch_git_static(&dep, &dest).await
                        }
                        gust_types::DependencySource::Registry => {
                            Self::fetch_registry_static(&dep, &dest).await
                        }
                        gust_types::DependencySource::Path => {
                            Self::fetch_path_static(&dep, &dest).await
                        }
                    };

                    // Notify completion
                    if let Ok(mut cb) = progress.lock() {
                        match &result {
                            Ok(_) => cb(&name, FetchStatus::Completed),
                            Err(e) => cb(&name, FetchStatus::Failed(e.to_string())),
                        }
                    }

                    result
                }
            })
            .collect();

        // Run all tasks concurrently (semaphore limits actual parallelism)
        join_all(tasks).await
    }

    /// Static version of fetch_git for use in spawned tasks.
    async fn fetch_git_static(dep: &Dependency, dest: &PathBuf) -> Result<FetchResult, FetchError> {
        let url = dep.git.as_ref().ok_or_else(|| FetchError::FetchFailed {
            package: dep.name.clone(),
            message: "No git URL".to_string(),
        })?;

        tracing::info!("Fetching {} from {}", dep.name, url);

        // Build git clone command with appropriate options
        let mut cmd = tokio::process::Command::new("git");
        cmd.args(["clone", "--depth", "1", "--single-branch"]);

        // Add branch/tag if specified
        if let Some(ref branch) = dep.branch {
            cmd.args(["--branch", branch]);
        } else if let Some(ref tag) = dep.tag {
            cmd.args(["--branch", tag]);
        }

        cmd.arg(url).arg(dest);

        let output = cmd.output().await?;

        if !output.status.success() {
            return Err(FetchError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // Get the current revision
        let rev_output = tokio::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dest)
            .output()
            .await?;

        let revision = String::from_utf8_lossy(&rev_output.stdout)
            .trim()
            .to_string();

        // Compute checksum of the checkout
        let checksum = compute_dir_hash(dest)?;

        Ok(FetchResult {
            name: dep.name.clone(),
            path: dest.clone(),
            checksum,
            revision: Some(revision),
        })
    }

    /// Static version of fetch_registry for use in spawned tasks.
    async fn fetch_registry_static(
        dep: &Dependency,
        _dest: &PathBuf,
    ) -> Result<FetchResult, FetchError> {
        tracing::warn!("Registry fetching not yet implemented for {}", dep.name);
        Err(FetchError::FetchFailed {
            package: dep.name.clone(),
            message: "Registry fetching not implemented".to_string(),
        })
    }

    /// Static version of fetch_path for use in spawned tasks.
    async fn fetch_path_static(dep: &Dependency, dest: &PathBuf) -> Result<FetchResult, FetchError> {
        let src = dep.path.as_ref().ok_or_else(|| FetchError::FetchFailed {
            package: dep.name.clone(),
            message: "No path specified".to_string(),
        })?;

        // For path deps, we just symlink
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(src, dest)?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(src, dest)?;

        let checksum = compute_dir_hash(src)?;

        Ok(FetchResult {
            name: dep.name.clone(),
            path: dest.clone(),
            checksum,
            revision: None,
        })
    }

    async fn fetch_git(&self, dep: &Dependency, dest: &PathBuf) -> Result<FetchResult, FetchError> {
        let url = dep.git.as_ref().ok_or_else(|| FetchError::FetchFailed {
            package: dep.name.clone(),
            message: "No git URL".to_string(),
        })?;

        tracing::info!("Fetching {} from {}", dep.name, url);

        // For now, shell out to git (gix would be better for a real impl)
        let output = tokio::process::Command::new("git")
            .args(["clone", "--depth", "1"])
            .arg(url)
            .arg(dest)
            .output()
            .await?;

        if !output.status.success() {
            return Err(FetchError::GitError(
                String::from_utf8_lossy(&output.stderr).to_string(),
            ));
        }

        // Get the current revision
        let rev_output = tokio::process::Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(dest)
            .output()
            .await?;

        let revision = String::from_utf8_lossy(&rev_output.stdout)
            .trim()
            .to_string();

        // Compute checksum of the checkout
        let checksum = compute_dir_hash(dest)?;

        Ok(FetchResult {
            name: dep.name.clone(),
            path: dest.clone(),
            checksum,
            revision: Some(revision),
        })
    }

    async fn fetch_registry(
        &self,
        dep: &Dependency,
        _dest: &PathBuf,
    ) -> Result<FetchResult, FetchError> {
        // TODO: Implement registry fetching (Swift Package Registry API)
        tracing::warn!("Registry fetching not yet implemented for {}", dep.name);
        Err(FetchError::FetchFailed {
            package: dep.name.clone(),
            message: "Registry fetching not implemented".to_string(),
        })
    }

    async fn fetch_path(&self, dep: &Dependency, dest: &PathBuf) -> Result<FetchResult, FetchError> {
        let src = dep.path.as_ref().ok_or_else(|| FetchError::FetchFailed {
            package: dep.name.clone(),
            message: "No path specified".to_string(),
        })?;

        // For path deps, we just symlink
        if dest.exists() {
            std::fs::remove_file(dest)?;
        }

        #[cfg(unix)]
        std::os::unix::fs::symlink(src, dest)?;

        #[cfg(windows)]
        std::os::windows::fs::symlink_dir(src, dest)?;

        let checksum = compute_dir_hash(src)?;

        Ok(FetchResult {
            name: dep.name.clone(),
            path: dest.clone(),
            checksum,
            revision: None,
        })
    }
}

fn compute_dir_hash(path: &PathBuf) -> Result<String, FetchError> {
    // Simple implementation: hash all file contents
    use std::collections::BTreeMap;
    use std::fs;

    let mut file_hashes: BTreeMap<String, String> = BTreeMap::new();

    fn visit_dir(dir: &std::path::Path, prefix: &str, hashes: &mut BTreeMap<String, String>) -> Result<(), FetchError> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                let name = path.file_name().unwrap().to_string_lossy();

                // Skip .git directory
                if name == ".git" {
                    continue;
                }

                let key = if prefix.is_empty() {
                    name.to_string()
                } else {
                    format!("{}/{}", prefix, name)
                };

                if path.is_dir() {
                    visit_dir(&path, &key, hashes)?;
                } else {
                    let content = fs::read(&path)?;
                    let hash = blake3::hash(&content).to_hex().to_string();
                    hashes.insert(key, hash);
                }
            }
        }
        Ok(())
    }

    visit_dir(path.as_path(), "", &mut file_hashes)?;

    // Hash all the file hashes together
    let combined: String = file_hashes
        .iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(blake3::hash(combined.as_bytes()).to_hex().to_string())
}
