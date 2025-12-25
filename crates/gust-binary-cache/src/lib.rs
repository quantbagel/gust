//! Binary artifact caching for Gust.
//!
//! Caches compiled Swift modules and object files to skip redundant builds.
//! Supports both local disk cache and remote artifact servers.

use blake3::Hasher;
use gust_types::BuildConfiguration;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum BinaryCacheError {
    #[error("Cache miss for {0}")]
    CacheMiss(String),
    #[error("Network error: {0}")]
    NetworkError(#[from] reqwest::Error),
    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Invalid signature")]
    InvalidSignature,
    #[error("Decompression error: {0}")]
    DecompressionError(String),
}

/// Build fingerprint for cache lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildFingerprint {
    /// Hash of all source files
    pub source_hash: String,
    /// Hash of the resolved manifest
    pub manifest_hash: String,
    /// Hash of dependency versions
    pub deps_hash: String,
    /// Swift version
    pub swift_version: String,
    /// Platform (e.g., "macos-arm64")
    pub platform: String,
    /// Build configuration
    pub build_config: BuildConfiguration,
    /// Compiler flags
    pub swift_flags: Vec<String>,
    /// Combined fingerprint
    pub fingerprint: String,
}

impl BuildFingerprint {
    /// Compute a build fingerprint.
    pub fn compute(
        source_hash: String,
        manifest_hash: String,
        deps_hash: String,
        swift_version: String,
        platform: String,
        build_config: BuildConfiguration,
        swift_flags: Vec<String>,
    ) -> Self {
        let mut hasher = Hasher::new();
        hasher.update(source_hash.as_bytes());
        hasher.update(manifest_hash.as_bytes());
        hasher.update(deps_hash.as_bytes());
        hasher.update(swift_version.as_bytes());
        hasher.update(platform.as_bytes());
        hasher.update(build_config.to_string().as_bytes());
        for flag in &swift_flags {
            hasher.update(flag.as_bytes());
        }

        let fingerprint = hasher.finalize().to_hex().to_string();

        Self {
            source_hash,
            manifest_hash,
            deps_hash,
            swift_version,
            platform,
            build_config,
            swift_flags,
            fingerprint,
        }
    }
}

/// Artifact metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ArtifactInfo {
    pub fingerprint: String,
    pub package: String,
    pub version: String,
    pub platform: String,
    pub swift_version: String,
    pub file_size: u64,
    pub compression: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

/// Binary cache client.
pub struct BinaryCacheClient {
    /// Remote cache URL
    base_url: String,
    /// HTTP client
    client: reqwest::Client,
    /// Authentication token
    auth_token: Option<String>,
}

impl BinaryCacheClient {
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            base_url: base_url.into(),
            client: reqwest::Client::new(),
            auth_token: None,
        }
    }

    pub fn with_auth(mut self, token: impl Into<String>) -> Self {
        self.auth_token = Some(token.into());
        self
    }

    /// Check if an artifact exists in the cache.
    pub async fn exists(&self, fingerprint: &str) -> Result<bool, BinaryCacheError> {
        let url = format!("{}/artifacts/{}", self.base_url, fingerprint);
        let resp = self.client.head(&url).send().await?;
        Ok(resp.status().is_success())
    }

    /// Get artifact metadata.
    pub async fn get_info(&self, fingerprint: &str) -> Result<ArtifactInfo, BinaryCacheError> {
        let url = format!("{}/artifacts/{}.info", self.base_url, fingerprint);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(BinaryCacheError::CacheMiss(fingerprint.to_string()));
        }

        let info: ArtifactInfo = resp.json().await?;
        Ok(info)
    }

    /// Download and extract an artifact.
    pub async fn pull(&self, fingerprint: &str, dest: &Path) -> Result<(), BinaryCacheError> {
        let url = format!("{}/artifacts/{}", self.base_url, fingerprint);
        let resp = self.client.get(&url).send().await?;

        if !resp.status().is_success() {
            return Err(BinaryCacheError::CacheMiss(fingerprint.to_string()));
        }

        let bytes = resp.bytes().await?;

        // Decompress zstd
        let decompressed = zstd::decode_all(bytes.as_ref())
            .map_err(|e| BinaryCacheError::DecompressionError(e.to_string()))?;

        // Extract tar
        let mut archive = tar::Archive::new(decompressed.as_slice());
        archive.unpack(dest)?;

        tracing::info!("Pulled artifact {} to {}", fingerprint, dest.display());
        Ok(())
    }

    /// Push an artifact to the cache.
    pub async fn push(
        &self,
        fingerprint: &str,
        source: &Path,
        info: &ArtifactInfo,
    ) -> Result<(), BinaryCacheError> {
        // Create tar archive
        let mut tar_data = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_data);
            builder.append_dir_all(".", source)?;
            builder.finish()?;
        }

        // Compress with zstd level 1 (fast compression, ARM decompresses quickly regardless)
        let compressed = zstd::encode_all(tar_data.as_slice(), 1)
            .map_err(|e| BinaryCacheError::DecompressionError(e.to_string()))?;

        // Upload
        let url = format!("{}/artifacts/{}", self.base_url, fingerprint);
        let mut req = self.client.put(&url).body(compressed);

        if let Some(token) = &self.auth_token {
            req = req.bearer_auth(token);
        }

        let resp = req.send().await?;

        if !resp.status().is_success() {
            return Err(BinaryCacheError::NetworkError(
                resp.error_for_status().unwrap_err(),
            ));
        }

        // Upload metadata
        let info_url = format!("{}/artifacts/{}.info", self.base_url, fingerprint);
        let mut info_req = self.client.put(&info_url).json(info);

        if let Some(token) = &self.auth_token {
            info_req = info_req.bearer_auth(token);
        }

        info_req.send().await?;

        tracing::info!("Pushed artifact {}", fingerprint);
        Ok(())
    }
}

/// Local binary cache for offline access.
pub struct LocalBinaryCache {
    cache_dir: PathBuf,
}

impl LocalBinaryCache {
    /// Create a new local binary cache.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self { cache_dir }
    }

    /// Open the default local binary cache.
    pub fn open() -> Result<Self, BinaryCacheError> {
        let cache_dir = directories::ProjectDirs::from("dev", "gust", "gust")
            .map(|d| d.cache_dir().join("binary-cache"))
            .ok_or_else(|| {
                BinaryCacheError::IoError(std::io::Error::new(
                    std::io::ErrorKind::NotFound,
                    "Could not determine cache directory",
                ))
            })?;

        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Check if an artifact exists in the cache.
    pub fn contains(&self, fingerprint: &str) -> bool {
        self.cache_dir
            .join(format!("{}.tar.zst", fingerprint))
            .exists()
    }

    /// Get the path to a cached artifact.
    pub fn get(&self, fingerprint: &str) -> Option<PathBuf> {
        let path = self.cache_dir.join(format!("{}.tar.zst", fingerprint));
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }

    /// Restore cached artifacts to a destination directory.
    pub fn restore(&self, fingerprint: &str, dest: &Path) -> Result<(), BinaryCacheError> {
        let archive_path = self
            .get(fingerprint)
            .ok_or_else(|| BinaryCacheError::CacheMiss(fingerprint.to_string()))?;

        let compressed = fs::read(&archive_path)?;
        let decompressed = zstd::decode_all(compressed.as_slice())
            .map_err(|e| BinaryCacheError::DecompressionError(e.to_string()))?;

        fs::create_dir_all(dest)?;

        let mut archive = tar::Archive::new(decompressed.as_slice());
        archive.unpack(dest)?;

        tracing::info!(
            "Restored cached artifacts {} to {}",
            fingerprint,
            dest.display()
        );
        Ok(())
    }

    /// Store build artifacts in the cache.
    pub fn store(&self, fingerprint: &str, source: &Path) -> Result<(), BinaryCacheError> {
        fs::create_dir_all(&self.cache_dir)?;

        let dest = self.cache_dir.join(format!("{}.tar.zst", fingerprint));

        // Skip if already cached
        if dest.exists() {
            return Ok(());
        }

        // Create tar and compress
        let mut tar_data = Vec::new();
        {
            let mut builder = tar::Builder::new(&mut tar_data);
            builder.append_dir_all(".", source)?;
            builder.finish()?;
        }

        // Use zstd level 1 for faster compression on Apple Silicon
        // ARM CPUs decompress fast regardless of level, so optimize for write speed
        let compressed = zstd::encode_all(tar_data.as_slice(), 1)
            .map_err(|e| BinaryCacheError::DecompressionError(e.to_string()))?;

        fs::write(&dest, &compressed)?;

        let size_mb = compressed.len() as f64 / 1024.0 / 1024.0;
        tracing::info!(
            "Stored artifacts {} ({:.2} MB compressed)",
            fingerprint,
            size_mb
        );
        Ok(())
    }

    /// Get cache statistics.
    pub fn stats(&self) -> Result<CacheStats, BinaryCacheError> {
        let mut count = 0;
        let mut total_size = 0;

        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "zst")
                    .unwrap_or(false)
                {
                    count += 1;
                    total_size += entry.metadata()?.len();
                }
            }
        }

        Ok(CacheStats { count, total_size })
    }

    /// Clear all cached artifacts.
    pub fn clear(&self) -> Result<usize, BinaryCacheError> {
        let mut cleared = 0;

        if self.cache_dir.exists() {
            for entry in fs::read_dir(&self.cache_dir)? {
                let entry = entry?;
                if entry
                    .path()
                    .extension()
                    .map(|e| e == "zst")
                    .unwrap_or(false)
                {
                    fs::remove_file(entry.path())?;
                    cleared += 1;
                }
            }
        }

        Ok(cleared)
    }
}

/// Cache statistics.
#[derive(Debug, Clone)]
pub struct CacheStats {
    /// Number of cached artifacts
    pub count: usize,
    /// Total size in bytes
    pub total_size: u64,
}

impl CacheStats {
    /// Format size as human-readable string.
    pub fn size_human(&self) -> String {
        let size = self.total_size as f64;
        if size < 1024.0 {
            format!("{} B", self.total_size)
        } else if size < 1024.0 * 1024.0 {
            format!("{:.1} KB", size / 1024.0)
        } else if size < 1024.0 * 1024.0 * 1024.0 {
            format!("{:.1} MB", size / 1024.0 / 1024.0)
        } else {
            format!("{:.2} GB", size / 1024.0 / 1024.0 / 1024.0)
        }
    }
}

/// Hash all Swift source files in a directory.
/// Uses rayon for parallel file hashing - optimized for Apple Silicon's many cores.
pub fn hash_sources(dir: &Path) -> Result<String, BinaryCacheError> {
    use rayon::prelude::*;

    // First, collect all source file paths (single-threaded, fast)
    let mut source_files: Vec<PathBuf> = Vec::new();

    fn collect_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), BinaryCacheError> {
        if !dir.is_dir() {
            return Ok(());
        }

        for entry in fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = path.file_name().unwrap().to_string_lossy();

            // Skip hidden files and build directories
            if name.starts_with('.') || name == "Package.resolved" {
                continue;
            }

            if path.is_dir() {
                collect_files(&path, files)?;
            } else {
                // Only hash Swift source files and important config
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
                if matches!(ext, "swift" | "h" | "c" | "cpp" | "m" | "mm") {
                    files.push(path);
                }
            }
        }
        Ok(())
    }

    collect_files(dir, &mut source_files)?;

    // Parallel hash all files using rayon + mmap (Apple Silicon optimization)
    // mmap provides zero-copy reads directly from the kernel page cache
    let file_hashes: Result<BTreeMap<String, String>, BinaryCacheError> = source_files
        .par_iter()
        .map(|path| {
            let file = fs::File::open(path)?;
            let metadata = file.metadata()?;

            // Use mmap for files > 4KB, regular read for small files
            // (mmap overhead not worth it for tiny files)
            let hash = if metadata.len() > 4096 {
                // SAFETY: We only read the file, and it's not modified during hashing
                let mmap = unsafe { memmap2::Mmap::map(&file)? };
                blake3::hash(&mmap).to_hex().to_string()
            } else {
                let content = fs::read(path)?;
                blake3::hash(&content).to_hex().to_string()
            };

            let rel_path = path.strip_prefix(dir).unwrap_or(path);
            Ok((rel_path.to_string_lossy().to_string(), hash))
        })
        .collect();

    let file_hashes = file_hashes?;

    // Combine all hashes deterministically
    let mut hasher = Hasher::new();
    for (path, hash) in &file_hashes {
        hasher.update(path.as_bytes());
        hasher.update(b":");
        hasher.update(hash.as_bytes());
        hasher.update(b"\n");
    }

    Ok(hasher.finalize().to_hex().to_string())
}

/// Hash a target's source files specifically.
pub fn hash_target_sources(
    project_dir: &Path,
    target_name: &str,
) -> Result<String, BinaryCacheError> {
    // Try common source directory patterns
    let possible_dirs = [
        project_dir.join("Sources").join(target_name),
        project_dir.join("Source").join(target_name),
        project_dir.join("src").join(target_name),
        project_dir.join(target_name),
    ];

    for dir in &possible_dirs {
        if dir.exists() && dir.is_dir() {
            return hash_sources(dir);
        }
    }

    // Fallback: hash all sources
    hash_sources(&project_dir.join("Sources"))
}

/// Create a complete build fingerprint for a target.
pub fn compute_target_fingerprint(
    project_dir: &Path,
    target_name: &str,
    deps_hash: &str,
    swift_version: &str,
    platform: &str,
    config: BuildConfiguration,
    flags: &[String],
) -> Result<BuildFingerprint, BinaryCacheError> {
    let source_hash = hash_target_sources(project_dir, target_name)?;

    // Hash the manifest too
    let manifest_path = project_dir.join("Package.swift");
    let manifest_hash = if manifest_path.exists() {
        let content = fs::read(&manifest_path)?;
        blake3::hash(&content).to_hex().to_string()
    } else {
        let gust_toml = project_dir.join("Gust.toml");
        if gust_toml.exists() {
            let content = fs::read(&gust_toml)?;
            blake3::hash(&content).to_hex().to_string()
        } else {
            String::new()
        }
    };

    Ok(BuildFingerprint::compute(
        source_hash,
        manifest_hash,
        deps_hash.to_string(),
        swift_version.to_string(),
        platform.to_string(),
        config,
        flags.to_vec(),
    ))
}
