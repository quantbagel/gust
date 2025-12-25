//! Swift build orchestration for Gust.
//!
//! Supports binary artifact caching for near-instant rebuilds.

use std::path::{Path, PathBuf};
use std::process::Stdio;
use gust_binary_cache::{hash_sources, LocalBinaryCache, BuildFingerprint};
use gust_platform::SwiftToolchain;
use gust_types::{BuildConfiguration, Manifest};
use thiserror::Error;
use tokio::io::{AsyncBufReadExt, BufReader};
use tokio::process::Command;

#[derive(Error, Debug)]
pub enum BuildError {
    #[error("Swift toolchain not found: {0}")]
    ToolchainError(#[from] gust_platform::PlatformError),
    #[error("Build failed: {0}")]
    BuildFailed(String),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Target not found: {0}")]
    TargetNotFound(String),
    #[error("Cache error: {0}")]
    CacheError(#[from] gust_binary_cache::BinaryCacheError),
}

/// Build options.
#[derive(Debug, Clone)]
pub struct BuildOptions {
    /// Build configuration (debug/release)
    pub configuration: BuildConfiguration,
    /// Number of parallel jobs
    pub jobs: Option<usize>,
    /// Specific target to build
    pub target: Option<String>,
    /// Extra Swift flags
    pub swift_flags: Vec<String>,
    /// Show verbose output
    pub verbose: bool,
    /// Enable binary artifact caching
    pub use_cache: bool,
    /// Skip cache lookup (always rebuild)
    pub force_rebuild: bool,
}

impl Default for BuildOptions {
    fn default() -> Self {
        Self {
            configuration: BuildConfiguration::Debug,
            jobs: None,
            target: None,
            swift_flags: Vec::new(),
            verbose: false,
            use_cache: true,
            force_rebuild: false,
        }
    }
}

/// Build result.
#[derive(Debug)]
pub struct BuildResult {
    /// Path to built products
    pub products: Vec<PathBuf>,
    /// Build duration in seconds
    pub duration_secs: f64,
    /// Whether the build was restored from cache
    pub cached: bool,
    /// Build fingerprint (for cache key)
    pub fingerprint: Option<String>,
}

/// The build orchestrator.
pub struct Builder {
    /// Project root directory
    project_dir: PathBuf,
    /// Swift toolchain
    toolchain: SwiftToolchain,
    /// Binary artifact cache
    binary_cache: Option<LocalBinaryCache>,
}

impl Builder {
    /// Create a new builder for the given project.
    pub fn new(project_dir: PathBuf) -> Result<Self, BuildError> {
        let toolchain = SwiftToolchain::detect()?;
        let binary_cache = LocalBinaryCache::open().ok();

        Ok(Self {
            project_dir,
            toolchain,
            binary_cache,
        })
    }

    /// Get the current platform identifier.
    fn platform_id(&self) -> String {
        #[cfg(target_os = "macos")]
        let os = "macos";
        #[cfg(target_os = "linux")]
        let os = "linux";
        #[cfg(target_os = "windows")]
        let os = "windows";
        #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
        let os = "unknown";

        #[cfg(target_arch = "aarch64")]
        let arch = "arm64";
        #[cfg(target_arch = "x86_64")]
        let arch = "x86_64";
        #[cfg(not(any(target_arch = "aarch64", target_arch = "x86_64")))]
        let arch = "unknown";

        format!("{}-{}", os, arch)
    }

    /// Compute build fingerprint for cache lookup.
    fn compute_fingerprint(
        &self,
        manifest: &Manifest,
        options: &BuildOptions,
    ) -> Result<BuildFingerprint, BuildError> {
        // Hash all sources in the project
        let source_hash = hash_sources(&self.project_dir.join("Sources"))
            .or_else(|_| hash_sources(&self.project_dir))
            .unwrap_or_default();

        // Hash manifest
        let manifest_path = self.project_dir.join("Package.swift");
        let manifest_hash = if manifest_path.exists() {
            let content = std::fs::read(&manifest_path)?;
            blake3::hash(&content).to_hex().to_string()
        } else {
            let gust_path = self.project_dir.join("Gust.toml");
            if gust_path.exists() {
                let content = std::fs::read(&gust_path)?;
                blake3::hash(&content).to_hex().to_string()
            } else {
                String::new()
            }
        };

        // Hash dependencies from lockfile if present
        let lockfile_path = self.project_dir.join("Gust.lock");
        let deps_hash = if lockfile_path.exists() {
            let content = std::fs::read(&lockfile_path)?;
            blake3::hash(&content).to_hex().to_string()
        } else {
            // Hash dependency names as fallback
            let deps: Vec<_> = manifest.dependencies.keys().collect();
            let deps_str = format!("{:?}", deps);
            blake3::hash(deps_str.as_bytes()).to_hex().to_string()
        };

        Ok(BuildFingerprint::compute(
            source_hash,
            manifest_hash,
            deps_hash,
            self.toolchain.version.clone(),
            self.platform_id(),
            options.configuration.clone(),
            options.swift_flags.clone(),
        ))
    }

    /// Build the project.
    pub async fn build(
        &self,
        manifest: &Manifest,
        options: &BuildOptions,
    ) -> Result<BuildResult, BuildError> {
        let start = std::time::Instant::now();

        // Verify target exists if specified
        if let Some(target_name) = &options.target {
            if !manifest.targets.iter().any(|t| &t.name == target_name) {
                return Err(BuildError::TargetNotFound(target_name.clone()));
            }
        }

        // Compute build fingerprint for cache
        let fingerprint = if options.use_cache {
            Some(self.compute_fingerprint(manifest, options)?)
        } else {
            None
        };

        let build_dir = self.project_dir.join(".build").join(match options.configuration {
            BuildConfiguration::Debug => "debug",
            BuildConfiguration::Release => "release",
        });

        // Check cache for existing build
        if options.use_cache && !options.force_rebuild {
            if let (Some(ref fp), Some(ref cache)) = (&fingerprint, &self.binary_cache) {
                if cache.contains(&fp.fingerprint) {
                    tracing::info!("Cache hit for fingerprint {}", &fp.fingerprint[..16]);

                    // Restore from cache
                    std::fs::create_dir_all(&build_dir)?;
                    cache.restore(&fp.fingerprint, &build_dir)?;

                    let duration = start.elapsed().as_secs_f64();
                    let products = find_products(&build_dir, manifest)?;

                    return Ok(BuildResult {
                        products,
                        duration_secs: duration,
                        cached: true,
                        fingerprint: Some(fp.fingerprint.clone()),
                    });
                } else {
                    tracing::debug!("Cache miss for fingerprint {}", &fp.fingerprint[..16]);
                }
            }
        }

        // Build command
        let mut cmd = Command::new(&self.toolchain.swift_path);
        cmd.arg("build");
        cmd.current_dir(&self.project_dir);

        // Configuration
        match options.configuration {
            BuildConfiguration::Release => {
                cmd.arg("-c").arg("release");
            }
            BuildConfiguration::Debug => {}
        }

        // Parallel jobs
        if let Some(jobs) = options.jobs {
            cmd.arg("-j").arg(jobs.to_string());
        }

        // Specific target
        if let Some(target) = &options.target {
            cmd.arg("--target").arg(target);
        }

        // Extra flags
        for flag in &options.swift_flags {
            cmd.arg(flag);
        }

        // Stream output
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());

        tracing::info!("Running: swift build");

        let mut child = cmd.spawn()?;

        // Stream stderr (where swift build outputs progress)
        if let Some(stderr) = child.stderr.take() {
            let reader = BufReader::new(stderr);
            let mut lines = reader.lines();

            while let Some(line) = lines.next_line().await? {
                if options.verbose {
                    println!("{}", line);
                } else {
                    // Parse and display progress
                    if line.contains("Compiling") || line.contains("Linking") {
                        println!("{}", line);
                    }
                }
            }
        }

        let status = child.wait().await?;

        if !status.success() {
            return Err(BuildError::BuildFailed(
                "swift build failed".to_string(),
            ));
        }

        let duration = start.elapsed().as_secs_f64();

        // Find built products
        let products = find_products(&build_dir, manifest)?;

        // Store in cache for next time
        if options.use_cache {
            if let (Some(ref fp), Some(ref cache)) = (&fingerprint, &self.binary_cache) {
                if let Err(e) = cache.store(&fp.fingerprint, &build_dir) {
                    tracing::warn!("Failed to cache build artifacts: {}", e);
                } else {
                    tracing::info!("Cached build artifacts as {}", &fp.fingerprint[..16]);
                }
            }
        }

        Ok(BuildResult {
            products,
            duration_secs: duration,
            cached: false,
            fingerprint: fingerprint.map(|f| f.fingerprint),
        })
    }

    /// Clean build artifacts.
    pub async fn clean(&self) -> Result<(), BuildError> {
        let mut cmd = Command::new(&self.toolchain.swift_path);
        cmd.arg("package").arg("clean");
        cmd.current_dir(&self.project_dir);

        let status = cmd.status().await?;

        if !status.success() {
            return Err(BuildError::BuildFailed("swift package clean failed".to_string()));
        }

        Ok(())
    }

    /// Get the build directory for a configuration.
    pub fn build_dir(&self, config: BuildConfiguration) -> PathBuf {
        self.project_dir.join(".build").join(config.to_string())
    }

    /// Get binary cache statistics.
    pub fn cache_stats(&self) -> Option<gust_binary_cache::CacheStats> {
        self.binary_cache.as_ref().and_then(|c| c.stats().ok())
    }

    /// Clear the binary cache.
    pub fn clear_cache(&self) -> Result<usize, BuildError> {
        if let Some(cache) = &self.binary_cache {
            Ok(cache.clear()?)
        } else {
            Ok(0)
        }
    }
}

/// Get binary cache statistics (standalone function for CLI use).
pub fn get_cache_stats() -> Result<gust_binary_cache::CacheStats, BuildError> {
    let cache = LocalBinaryCache::open()?;
    Ok(cache.stats()?)
}

/// Clear the binary cache (standalone function for CLI use).
pub fn clear_binary_cache() -> Result<usize, BuildError> {
    let cache = LocalBinaryCache::open()?;
    Ok(cache.clear()?)
}

fn find_products(build_dir: &Path, manifest: &Manifest) -> Result<Vec<PathBuf>, BuildError> {
    let mut products = Vec::new();

    for target in &manifest.targets {
        let path = match target.target_type {
            gust_types::TargetType::Executable => build_dir.join(&target.name),
            gust_types::TargetType::Library => {
                // Try both static and dynamic lib names
                let static_lib = build_dir.join(format!("lib{}.a", target.name));
                let dylib = build_dir.join(format!("lib{}.dylib", target.name));
                if static_lib.exists() {
                    static_lib
                } else {
                    dylib
                }
            }
            _ => continue,
        };

        if path.exists() {
            products.push(path);
        }
    }

    Ok(products)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_build_options_default() {
        let opts = BuildOptions::default();
        assert_eq!(opts.configuration, BuildConfiguration::Debug);
        assert!(opts.target.is_none());
    }
}
