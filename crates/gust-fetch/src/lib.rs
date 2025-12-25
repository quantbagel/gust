//! Parallel package fetching for Gust.

#![allow(clippy::ptr_arg)]

use gust_types::Dependency;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Arc;
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
        // Use available parallelism (optimized for Apple Silicon's many cores)
        // Falls back to 8 if detection fails
        let concurrency = std::thread::available_parallelism()
            .map(|n| n.get())
            .unwrap_or(8);
        Self { concurrency }
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
    /// Uses git command for reliability with annotated tags.
    async fn fetch_git_static(dep: &Dependency, dest: &PathBuf) -> Result<FetchResult, FetchError> {
        let url = dep.git.as_ref().ok_or_else(|| FetchError::FetchFailed {
            package: dep.name.clone(),
            message: "No git URL".to_string(),
        })?;

        tracing::info!("Fetching {} from {}", dep.name, url);

        let url = url.clone();
        let dest_clone = dest.clone();
        let dest_result = dest.clone();
        let branch = dep.branch.clone();
        let tag = dep.tag.clone();
        let name = dep.name.clone();

        // Use git command for better compatibility with annotated tags
        let (revision, checksum) =
            tokio::task::spawn_blocking(move || clone_with_git(&url, &dest_clone, branch, tag))
                .await
                .map_err(|e| FetchError::GitError(format!("Task join error: {}", e)))??;

        Ok(FetchResult {
            name,
            path: dest_result,
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
    async fn fetch_path_static(
        dep: &Dependency,
        dest: &PathBuf,
    ) -> Result<FetchResult, FetchError> {
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
        // Delegate to static version which uses native gix
        Self::fetch_git_static(dep, dest).await
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

    async fn fetch_path(
        &self,
        dep: &Dependency,
        dest: &PathBuf,
    ) -> Result<FetchResult, FetchError> {
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

fn compute_dir_hash(path: &Path) -> Result<String, FetchError> {
    use rayon::prelude::*;
    use std::collections::BTreeMap;
    use std::fs;
    use std::path::PathBuf;

    // First, collect all file paths (single-threaded, fast)
    let mut files: Vec<(String, PathBuf)> = Vec::new();

    fn collect_files(
        dir: &std::path::Path,
        prefix: &str,
        files: &mut Vec<(String, PathBuf)>,
    ) -> Result<(), FetchError> {
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
                    collect_files(&path, &key, files)?;
                } else {
                    files.push((key, path));
                }
            }
        }
        Ok(())
    }

    collect_files(path, "", &mut files)?;

    // Parallel hash all files using rayon + mmap (Apple Silicon optimization)
    // mmap provides zero-copy reads directly from the kernel page cache
    let file_hashes: Result<BTreeMap<String, String>, FetchError> = files
        .par_iter()
        .map(|(key, path)| {
            let file = fs::File::open(path)?;
            let metadata = file.metadata()?;

            // Use mmap for files > 4KB, regular read for small files
            let hash = if metadata.len() > 4096 {
                // SAFETY: We only read the file, and it's not modified during hashing
                let mmap = unsafe { memmap2::Mmap::map(&file)? };
                blake3::hash(&mmap).to_hex().to_string()
            } else {
                let content = fs::read(path)?;
                blake3::hash(&content).to_hex().to_string()
            };

            Ok((key.clone(), hash))
        })
        .collect();

    let file_hashes = file_hashes?;

    // Hash all the file hashes together
    let combined: String = file_hashes
        .iter()
        .map(|(k, v)| format!("{}:{}", k, v))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(blake3::hash(combined.as_bytes()).to_hex().to_string())
}

/// Clone a git repository using the git command.
/// More reliable for annotated tags and complex scenarios.
/// Returns (revision, checksum) on success.
fn clone_with_git(
    url: &str,
    dest: &std::path::Path,
    branch: Option<String>,
    tag: Option<String>,
) -> Result<(String, String), FetchError> {
    let mut args = vec!["clone", "--depth", "1"];

    // Add branch or tag
    let ref_arg: String;
    if let Some(ref t) = tag {
        ref_arg = t.clone();
        args.push("--branch");
        args.push(&ref_arg);
    } else if let Some(ref b) = branch {
        ref_arg = b.clone();
        args.push("--branch");
        args.push(&ref_arg);
    }

    args.push(url);
    let dest_str = dest.to_string_lossy();
    args.push(&dest_str);

    let output = Command::new("git")
        .args(&args)
        .output()
        .map_err(|e| FetchError::GitError(format!("Failed to run git: {}", e)))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(FetchError::GitError(format!(
            "git clone failed: {}",
            stderr
        )));
    }

    // Get the HEAD revision
    let rev_output = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .current_dir(dest)
        .output()
        .map_err(|e| FetchError::GitError(format!("Failed to get revision: {}", e)))?;

    let revision = String::from_utf8_lossy(&rev_output.stdout)
        .trim()
        .to_string();

    // Compute checksum
    let checksum = compute_dir_hash(dest)?;

    Ok((revision, checksum))
}

/// Clone a git repository using native gix library.
/// Returns (revision, checksum) on success.
#[allow(dead_code)]
fn clone_with_gix(
    url: &str,
    dest: &std::path::Path,
    branch: Option<String>,
    tag: Option<String>,
) -> Result<(String, String), FetchError> {
    use gix::remote::fetch::Shallow;
    use std::sync::atomic::AtomicBool;

    // Prepare the clone with shallow fetch (depth=1)
    let mut prepare = gix::clone::PrepareFetch::new(
        url,
        dest,
        gix::create::Kind::WithWorktree,
        gix::create::Options::default(),
        gix::open::Options::isolated(),
    )
    .map_err(|e| FetchError::GitError(format!("Failed to prepare clone: {}", e)))?
    .with_shallow(Shallow::DepthAtRemote(1.try_into().unwrap()));

    // Configure remote to fetch specific branch or tag
    // Tags use refs/tags/, branches use refs/heads/
    if let Some(ref tag_name) = tag {
        let refspec_str = format!("+refs/tags/{0}:refs/tags/{0}", tag_name);
        prepare = prepare.configure_remote(move |remote| {
            Ok(remote.with_refspecs(Some(refspec_str.as_str()), gix::remote::Direction::Fetch)?)
        });
    } else if let Some(ref branch_name) = branch {
        let refspec_str = format!("+refs/heads/{0}:refs/remotes/origin/{0}", branch_name);
        prepare = prepare.configure_remote(move |remote| {
            Ok(remote.with_refspecs(Some(refspec_str.as_str()), gix::remote::Direction::Fetch)?)
        });
    }

    // Perform the fetch
    let should_interrupt = AtomicBool::new(false);
    let (mut checkout, _outcome) = prepare
        .fetch_then_checkout(gix::progress::Discard, &should_interrupt)
        .map_err(|e| FetchError::GitError(format!("Failed to fetch: {}", e)))?;

    // Checkout the worktree
    let (_repo, _outcome) = checkout
        .main_worktree(gix::progress::Discard, &should_interrupt)
        .map_err(|e| FetchError::GitError(format!("Failed to checkout: {}", e)))?;

    // Open the repo to get HEAD
    let repo =
        gix::open(dest).map_err(|e| FetchError::GitError(format!("Failed to open repo: {}", e)))?;

    let head = repo
        .head_id()
        .map_err(|e| FetchError::GitError(format!("Failed to get HEAD: {}", e)))?;

    let revision = head.to_string();

    // Compute checksum
    let checksum = compute_dir_hash(dest)?;

    Ok((revision, checksum))
}

/// A remote git tag with its version.
#[derive(Debug, Clone)]
pub struct GitTag {
    /// Tag name (e.g., "1.5.0" or "v1.5.0")
    pub name: String,
    /// Parsed semver version (if valid)
    pub version: Option<semver::Version>,
    /// Commit SHA
    pub sha: String,
}

/// Fetch available tags from a remote git repository.
/// Uses `git ls-remote --tags` for efficiency (no clone needed).
pub async fn list_remote_tags(url: &str) -> Result<Vec<GitTag>, FetchError> {
    let url = url.to_string();

    tokio::task::spawn_blocking(move || {
        let output = Command::new("git")
            .args(["ls-remote", "--tags", "--refs", &url])
            .output()
            .map_err(|e| FetchError::GitError(format!("Failed to run git ls-remote: {}", e)))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            return Err(FetchError::GitError(format!(
                "git ls-remote failed: {}",
                stderr
            )));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let mut tags = Vec::new();

        for line in stdout.lines() {
            // Format: "<sha>\trefs/tags/<tag>"
            let parts: Vec<&str> = line.split('\t').collect();
            if parts.len() != 2 {
                continue;
            }

            let sha = parts[0].to_string();
            let ref_name = parts[1];

            // Extract tag name from refs/tags/<name>
            let tag_name = ref_name
                .strip_prefix("refs/tags/")
                .unwrap_or(ref_name)
                .to_string();

            // Try to parse as semver (strip 'v' prefix if present)
            let version_str = tag_name.strip_prefix('v').unwrap_or(&tag_name);
            let version = semver::Version::parse(version_str).ok();

            tags.push(GitTag {
                name: tag_name,
                version,
                sha,
            });
        }

        // Sort by version (newest first), putting non-semver tags at the end
        tags.sort_by(|a, b| match (&b.version, &a.version) {
            (Some(v1), Some(v2)) => v1.cmp(v2),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => b.name.cmp(&a.name),
        });

        Ok(tags)
    })
    .await
    .map_err(|e| FetchError::GitError(format!("Task join error: {}", e)))?
}
