//! Package installation orchestration.
//!
//! Coordinates: manifest → resolve → fetch → cache → link

use console::style;
use gust_cache::GlobalCache;
use gust_fetch::{FetchResult, FetchStatus, Fetcher};
use gust_lockfile::{LockedPackage, Lockfile, LockfileDiff};
use gust_manifest::{find_manifest, parse_transitive_deps};
use gust_resolver::{Resolution, ResolvedDep};
use gust_types::{Dependency, DependencySource, Manifest, Version};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};
use miette::{IntoDiagnostic, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// Installation options.
#[derive(Debug, Clone, Default)]
pub struct InstallOptions {
    /// Error if lockfile is out of date
    pub frozen: bool,
    /// Number of parallel downloads
    pub concurrency: usize,
}

/// The package installer.
pub struct Installer {
    /// Project root directory
    project_dir: PathBuf,
    /// Global package cache
    cache: GlobalCache,
    /// Package fetcher
    fetcher: Fetcher,
    /// Installation options
    options: InstallOptions,
}

impl Installer {
    /// Create a new installer for the given project.
    pub fn new(project_dir: PathBuf, options: InstallOptions) -> Result<Self> {
        let cache = GlobalCache::open().into_diagnostic()?;
        let fetcher = Fetcher::new().with_concurrency(options.concurrency);

        Ok(Self {
            project_dir,
            cache,
            fetcher,
            options,
        })
    }

    /// Run the full installation flow.
    pub async fn install(&self) -> Result<InstallResult> {
        let mp = MultiProgress::new();

        // Step 1: Parse manifest
        let spinner = mp.add(ProgressBar::new_spinner());
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        spinner.set_message("Reading manifest...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let (manifest, _manifest_type) = find_manifest(&self.project_dir).into_diagnostic()?;
        spinner.finish_with_message(format!(
            "{} Read manifest for {}",
            style("✓").green(),
            style(&manifest.package.name).cyan()
        ));

        // Step 2: Check lockfile
        let lockfile_path = self.project_dir.join("Gust.lock");
        let existing_lockfile = if lockfile_path.exists() {
            Some(Lockfile::load(&lockfile_path).into_diagnostic()?)
        } else {
            None
        };

        // Step 3: Resolve dependencies (with parallel transitive parsing)
        let resolution = self
            .resolve(&manifest, existing_lockfile.as_ref(), &mp)
            .await?;
        let pkg_count = resolution.packages.len();

        println!(
            "{} Resolved {} total packages",
            style("✓").green(),
            style(pkg_count).cyan()
        );

        if pkg_count == 0 {
            println!("{} No dependencies to install", style("✓").green().bold());
            return Ok(InstallResult { installed: 0 });
        }

        // Step 4: Fetch packages
        let fetch_results = self.fetch_packages(&mp, &resolution).await?;

        // Step 5: Link packages to project
        let spinner = mp.add(ProgressBar::new_spinner());
        spinner.set_style(
            ProgressStyle::default_spinner()
                .template("{spinner:.blue} {msg}")
                .unwrap(),
        );
        spinner.set_message("Linking packages...");
        spinner.enable_steady_tick(std::time::Duration::from_millis(100));

        let linked = self.link_packages(&resolution, &fetch_results)?;

        spinner.finish_with_message(format!(
            "{} Linked {} packages",
            style("✓").green(),
            style(linked).cyan()
        ));

        // Step 6: Update lockfile (incremental, async)
        match self
            .update_lockfile(
                &lockfile_path,
                &resolution,
                &fetch_results,
                existing_lockfile.as_ref(),
            )
            .await?
        {
            Some(diff) if diff.has_changes() => {
                let summary = diff.summary();
                println!(
                    "{} Updated lockfile ({})",
                    style("✓").green(),
                    style(summary).dim()
                );
            }
            Some(_) => {
                println!("{} Lockfile unchanged", style("✓").green());
            }
            None => {
                tracing::debug!("Lockfile already up to date");
            }
        }

        Ok(InstallResult {
            installed: fetch_results.len(),
        })
    }

    /// Resolve dependencies including transitive ones.
    ///
    /// This performs iterative resolution:
    /// 1. Fetch direct dependencies
    /// 2. Parse their manifests in parallel to discover transitive deps
    /// 3. Repeat until all dependencies are resolved
    async fn resolve(
        &self,
        manifest: &Manifest,
        existing_lockfile: Option<&Lockfile>,
        mp: &MultiProgress,
    ) -> Result<Resolution> {
        // If we have a lockfile and frozen mode, use it directly
        if self.options.frozen {
            if let Some(lockfile) = existing_lockfile {
                return self.resolution_from_lockfile(lockfile);
            } else {
                return Err(miette::miette!(
                    "No lockfile found but --frozen was specified"
                ));
            }
        }

        let mut packages: HashMap<String, ResolvedDep> = HashMap::new();
        let mut pending_deps: Vec<(String, Dependency)> = manifest
            .dependencies
            .iter()
            .map(|(name, dep)| (name.clone(), dep.clone()))
            .collect();

        let mut iteration = 0;
        const MAX_ITERATIONS: usize = 20; // Prevent infinite loops

        while !pending_deps.is_empty() && iteration < MAX_ITERATIONS {
            iteration += 1;

            // Filter out already resolved deps
            pending_deps.retain(|(name, _)| !packages.contains_key(name));

            if pending_deps.is_empty() {
                break;
            }

            let count = pending_deps.len();
            let depth_msg = if iteration == 1 {
                format!("Resolving {} direct dependencies", count)
            } else {
                format!(
                    "Resolving {} transitive dependencies (depth {})",
                    count, iteration
                )
            };

            let spinner = mp.add(ProgressBar::new_spinner());
            spinner.set_style(
                ProgressStyle::default_spinner()
                    .template("{spinner:.blue} {msg}")
                    .unwrap(),
            );
            spinner.set_message(depth_msg.clone());
            spinner.enable_steady_tick(std::time::Duration::from_millis(100));

            // Build list of packages to fetch
            let to_fetch: Vec<(Dependency, PathBuf)> = pending_deps
                .iter()
                .filter_map(|(name, dep)| {
                    let dest = self.cache.git_dir().join(sanitize_name(name));
                    if dest.exists() {
                        None // Already cached
                    } else {
                        Some((dep.clone(), dest))
                    }
                })
                .collect();

            // Fetch packages in parallel
            if !to_fetch.is_empty() {
                let _results = self.fetcher.fetch_many(to_fetch, |_name, _status| {}).await;
            }

            // Collect paths for parsing
            let parse_dirs: Vec<(String, PathBuf)> = pending_deps
                .iter()
                .map(|(name, _)| {
                    let path = self.cache.git_dir().join(sanitize_name(name));
                    (name.clone(), path)
                })
                .collect();

            // Parse all fetched manifests in parallel
            let (parsed, discovered) =
                parse_transitive_deps(parse_dirs, self.options.concurrency).await;

            // Add resolved packages
            for parsed_dep in &parsed {
                let dep = pending_deps
                    .iter()
                    .find(|(n, _)| n == &parsed_dep.name)
                    .map(|(_, d)| d.clone());

                let source = if let Some(ref d) = dep {
                    match d.source_kind() {
                        DependencySource::Git => gust_resolver::ResolvedSource::Git {
                            url: d.git.clone().unwrap_or_default(),
                            revision: d.revision.clone().unwrap_or_else(|| "HEAD".to_string()),
                            tag: d.tag.clone(),
                        },
                        DependencySource::Path => gust_resolver::ResolvedSource::Path {
                            path: d.path.clone().unwrap_or_default(),
                        },
                        DependencySource::Registry => gust_resolver::ResolvedSource::Registry,
                    }
                } else {
                    // For discovered transitive deps, try to find git URL from their manifest
                    gust_resolver::ResolvedSource::Git {
                        url: String::new(),
                        revision: "HEAD".to_string(),
                        tag: None,
                    }
                };

                packages.insert(
                    parsed_dep.name.clone(),
                    ResolvedDep {
                        name: parsed_dep.name.clone(),
                        version: parsed_dep.manifest.package.version.clone(),
                        source,
                        dependencies: parsed_dep.dependency_names.clone(),
                    },
                );
            }

            // Queue up transitive dependencies with proper URLs from parent manifests
            pending_deps.clear();
            for dep_name in discovered {
                if packages.contains_key(&dep_name) {
                    continue;
                }

                // Find the dependency details from the parsed manifests
                let mut found_dep: Option<Dependency> = None;
                for parsed in &parsed {
                    if let Some(dep) = parsed.manifest.dependencies.get(&dep_name) {
                        found_dep = Some(dep.clone());
                        break;
                    }
                }

                if let Some(dep) = found_dep {
                    if dep.git.is_some() || dep.path.is_some() {
                        pending_deps.push((dep_name, dep));
                    }
                } else {
                    tracing::debug!("Could not find dependency info for {}", dep_name);
                }
            }

            spinner.finish_with_message(format!("{} {}", style("✓").green(), depth_msg));
        }

        if iteration >= MAX_ITERATIONS {
            tracing::warn!("Reached maximum resolution depth, some transitive deps may be missing");
        }

        Ok(Resolution { packages })
    }

    /// Create resolution from existing lockfile.
    fn resolution_from_lockfile(&self, lockfile: &Lockfile) -> Result<Resolution> {
        let mut packages = HashMap::new();

        for pkg in &lockfile.packages {
            // For locked packages, derive tag from version
            let version_tag = if pkg.version != Version::new(0, 0, 0) {
                Some(pkg.version.to_string())
            } else {
                None
            };

            let source = match pkg.source {
                DependencySource::Git => gust_resolver::ResolvedSource::Git {
                    url: pkg.git.clone().unwrap_or_default(),
                    revision: pkg.revision.clone().unwrap_or_default(),
                    tag: version_tag,
                },
                DependencySource::Path => gust_resolver::ResolvedSource::Path {
                    path: PathBuf::new(), // Would need to store path in lockfile
                },
                DependencySource::Registry => gust_resolver::ResolvedSource::Registry,
            };

            packages.insert(
                pkg.name.clone(),
                ResolvedDep {
                    name: pkg.name.clone(),
                    version: pkg.version.clone(),
                    source,
                    dependencies: pkg.dependencies.clone(),
                },
            );
        }

        Ok(Resolution { packages })
    }

    /// Fetch all packages in parallel.
    async fn fetch_packages(
        &self,
        mp: &MultiProgress,
        resolution: &Resolution,
    ) -> Result<HashMap<String, FetchResult>> {
        use std::sync::atomic::{AtomicUsize, Ordering};

        // Separate packages into cached and need-to-fetch
        let mut already_cached: HashMap<String, FetchResult> = HashMap::new();
        let mut to_fetch: Vec<(Dependency, PathBuf)> = Vec::new();

        for (name, resolved) in &resolution.packages {
            let (dep, tag) = match &resolved.source {
                gust_resolver::ResolvedSource::Git { url, revision, tag } => {
                    let mut d = Dependency::git(name, url);
                    d.revision = Some(revision.clone());
                    d.tag = tag.clone();
                    (d, tag.clone())
                }
                gust_resolver::ResolvedSource::Path { path } => {
                    (Dependency::path(name, path), None)
                }
                gust_resolver::ResolvedSource::Registry => {
                    // Skip registry deps for now
                    continue;
                }
            };

            let dest = self.cache.git_dir().join(sanitize_name(name));

            // Check if already in cache
            if dest.exists() {
                already_cached.insert(
                    name.clone(),
                    FetchResult {
                        name: name.clone(),
                        path: dest,
                        checksum: String::new(),
                        revision: None,
                        tag,
                    },
                );
            } else {
                to_fetch.push((dep, dest));
            }
        }

        let cached_count = already_cached.len();
        let fetch_count = to_fetch.len();

        if cached_count > 0 {
            println!(
                "{} {} packages already cached",
                style("✓").green(),
                cached_count
            );
        }

        if fetch_count == 0 {
            return Ok(already_cached);
        }

        // Create progress bar for fetching
        let pb = mp.add(ProgressBar::new(fetch_count as u64));
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{spinner:.green} [{bar:40.cyan/blue}] {pos}/{len} {wide_msg}")
                .unwrap()
                .progress_chars("█▓░"),
        );

        // Track active fetches for display
        let active_fetches: Arc<std::sync::Mutex<Vec<String>>> =
            Arc::new(std::sync::Mutex::new(Vec::new()));
        let completed = Arc::new(AtomicUsize::new(0));

        let active_clone = Arc::clone(&active_fetches);
        let completed_clone = Arc::clone(&completed);
        let pb_clone = pb.clone();

        // Progress callback
        let on_progress = move |name: &str, status: FetchStatus| {
            let mut active = active_clone.lock().unwrap();
            match status {
                FetchStatus::Started => {
                    active.push(name.to_string());
                    let msg = if active.len() <= 3 {
                        active.join(", ")
                    } else {
                        format!("{} and {} more", active[..3].join(", "), active.len() - 3)
                    };
                    pb_clone.set_message(format!("Fetching: {}", msg));
                }
                FetchStatus::Completed => {
                    active.retain(|n| n != name);
                    completed_clone.fetch_add(1, Ordering::SeqCst);
                    pb_clone.inc(1);
                }
                FetchStatus::Failed(_) => {
                    active.retain(|n| n != name);
                    pb_clone.inc(1);
                }
            }
        };

        // Fetch all packages in parallel!
        let fetch_results = self.fetcher.fetch_many(to_fetch, on_progress).await;

        // Collect results
        let mut results = already_cached;
        let mut errors = Vec::new();

        for result in fetch_results {
            match result {
                Ok(fetch_result) => {
                    results.insert(fetch_result.name.clone(), fetch_result);
                }
                Err(e) => {
                    errors.push(e.to_string());
                }
            }
        }

        if !errors.is_empty() {
            pb.abandon_with_message(format!("{} errors during fetch", errors.len()));
            return Err(miette::miette!("Fetch errors: {}", errors.join(", ")));
        }

        pb.finish_with_message(format!(
            "{} Fetched {} packages in parallel",
            style("✓").green(),
            fetch_count
        ));

        Ok(results)
    }

    /// Link packages from cache to project.
    fn link_packages(
        &self,
        _resolution: &Resolution,
        fetch_results: &HashMap<String, FetchResult>,
    ) -> Result<usize> {
        let checkouts_dir = self.project_dir.join(".build").join("checkouts");
        std::fs::create_dir_all(&checkouts_dir).into_diagnostic()?;

        let mut linked = 0;

        for (name, result) in fetch_results {
            let link_path = checkouts_dir.join(name);

            // Remove existing link/dir
            if link_path.exists() {
                if link_path.is_symlink() || link_path.is_file() {
                    std::fs::remove_file(&link_path).into_diagnostic()?;
                } else {
                    std::fs::remove_dir_all(&link_path).into_diagnostic()?;
                }
            }

            // Create symlink to cached package
            #[cfg(unix)]
            std::os::unix::fs::symlink(&result.path, &link_path).into_diagnostic()?;

            #[cfg(windows)]
            std::os::windows::fs::symlink_dir(&result.path, &link_path).into_diagnostic()?;

            linked += 1;
        }

        Ok(linked)
    }

    /// Update the lockfile incrementally.
    ///
    /// Only writes if there are actual changes, and shows a diff summary.
    async fn update_lockfile(
        &self,
        lockfile_path: &Path,
        resolution: &Resolution,
        fetch_results: &HashMap<String, FetchResult>,
        existing_lockfile: Option<&Lockfile>,
    ) -> Result<Option<LockfileDiff>> {
        // Build the new package list
        let mut new_packages: Vec<LockedPackage> = Vec::new();

        for (name, resolved) in &resolution.packages {
            let fetch_result = fetch_results.get(name);

            let locked = match &resolved.source {
                gust_resolver::ResolvedSource::Git { url, revision, tag } => {
                    // Try to get version from tag, fallback to manifest version
                    let version = tag
                        .as_ref()
                        .or_else(|| fetch_result.and_then(|r| r.tag.as_ref()))
                        .and_then(|t| parse_version_from_tag(t))
                        .unwrap_or_else(|| resolved.version.clone());

                    let mut pkg = LockedPackage::git(
                        name,
                        version,
                        url,
                        fetch_result
                            .and_then(|r| r.revision.clone())
                            .unwrap_or_else(|| revision.clone()),
                    );
                    pkg.dependencies = resolved.dependencies.clone();
                    pkg
                }
                gust_resolver::ResolvedSource::Registry => {
                    let mut pkg = LockedPackage::registry(
                        name,
                        resolved.version.clone(),
                        fetch_result
                            .map(|r| format!("blake3:{}", r.checksum))
                            .unwrap_or_default(),
                    );
                    pkg.dependencies = resolved.dependencies.clone();
                    pkg
                }
                gust_resolver::ResolvedSource::Path { .. } => {
                    // Don't lock path dependencies
                    continue;
                }
            };

            new_packages.push(locked);
        }

        // Sort for deterministic output
        new_packages.sort_by(|a, b| a.name.cmp(&b.name));

        // Check if we need to update
        if let Some(existing) = existing_lockfile {
            if !existing.needs_update(&new_packages) {
                tracing::debug!("Lockfile is up to date, skipping write");
                return Ok(None);
            }

            // Compute and apply incremental diff
            let (diff, merged) = existing.merge(new_packages);

            if diff.has_changes() {
                // Write asynchronously
                let path = lockfile_path.to_path_buf();
                merged.save_async(path).await.into_diagnostic()?;
                return Ok(Some(diff));
            } else {
                return Ok(None);
            }
        }

        // No existing lockfile, create new one
        let lockfile = Lockfile {
            packages: new_packages,
            ..Default::default()
        };

        let diff = LockfileDiff {
            added: lockfile.packages.clone(),
            removed: Vec::new(),
            updated: Vec::new(),
            unchanged: Vec::new(),
        };

        let path = lockfile_path.to_path_buf();
        lockfile.save_async(path).await.into_diagnostic()?;

        Ok(Some(diff))
    }
}

/// Sanitize a package name for use as a directory name.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// Parse a version from a git tag.
/// Handles formats like "1.5.0", "v1.5.0", "1.5.0-beta.1"
fn parse_version_from_tag(tag: &str) -> Option<Version> {
    let tag = tag.trim();

    // Try parsing directly
    if let Ok(v) = Version::parse(tag) {
        return Some(v);
    }

    // Try stripping leading 'v'
    if let Some(stripped) = tag.strip_prefix('v').or_else(|| tag.strip_prefix('V')) {
        if let Ok(v) = Version::parse(stripped) {
            return Some(v);
        }
    }

    None
}

/// Result of an installation.
#[derive(Debug)]
pub struct InstallResult {
    /// Number of packages installed
    pub installed: usize,
}
