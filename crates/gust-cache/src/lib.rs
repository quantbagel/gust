//! Content-addressable cache with hard links for Gust.
//!
//! Implements a pnpm-style global store that saves disk space
//! by storing each unique file only once.

use blake3::Hasher;
use directories::ProjectDirs;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};
use thiserror::Error;
use tracing::{debug, info};

#[derive(Error, Debug)]
pub enum CacheError {
    #[error("Failed to create cache directory: {0}")]
    CreateDirError(#[source] io::Error),
    #[error("Failed to read file: {0}")]
    ReadError(#[source] io::Error),
    #[error("Failed to write file: {0}")]
    WriteError(#[source] io::Error),
    #[error("Failed to create hard link: {0}")]
    LinkError(#[source] io::Error),
    #[error("Cache directory not found")]
    NoCacheDir,
    #[error("Package not in cache: {0}")]
    PackageNotFound(String),
}

/// The global package cache.
pub struct GlobalCache {
    /// Root cache directory (~/.gust)
    root: PathBuf,
    /// Store version for migrations
    version: u32,
}

impl GlobalCache {
    /// Create or open the global cache.
    pub fn open() -> Result<Self, CacheError> {
        let root = Self::default_cache_dir()?;
        Self::open_at(root)
    }

    /// Open a cache at a specific location.
    pub fn open_at(root: PathBuf) -> Result<Self, CacheError> {
        let cache = Self { root, version: 1 };
        cache.ensure_dirs()?;
        Ok(cache)
    }

    /// Get the default cache directory.
    pub fn default_cache_dir() -> Result<PathBuf, CacheError> {
        ProjectDirs::from("dev", "gust", "gust")
            .map(|dirs| dirs.cache_dir().to_path_buf())
            .ok_or(CacheError::NoCacheDir)
    }

    /// Ensure all cache directories exist.
    fn ensure_dirs(&self) -> Result<(), CacheError> {
        fs::create_dir_all(self.files_dir()).map_err(CacheError::CreateDirError)?;
        fs::create_dir_all(self.packages_dir()).map_err(CacheError::CreateDirError)?;
        fs::create_dir_all(self.git_dir()).map_err(CacheError::CreateDirError)?;
        Ok(())
    }

    /// Get the content-addressed files directory.
    pub fn files_dir(&self) -> PathBuf {
        self.root
            .join("store")
            .join(format!("v{}", self.version))
            .join("files")
    }

    /// Get the packages metadata directory.
    pub fn packages_dir(&self) -> PathBuf {
        self.root
            .join("store")
            .join(format!("v{}", self.version))
            .join("packages")
    }

    /// Get the git repositories directory.
    pub fn git_dir(&self) -> PathBuf {
        self.root.join("git")
    }

    /// Get the binary cache directory.
    pub fn binary_cache_dir(&self) -> PathBuf {
        self.root.join("binary-cache")
    }

    /// Compute the BLAKE3 hash of a file.
    pub fn hash_file(path: &Path) -> Result<String, CacheError> {
        let mut file = File::open(path).map_err(CacheError::ReadError)?;
        let mut hasher = Hasher::new();
        let mut buffer = [0u8; 65536];

        loop {
            let bytes_read = file.read(&mut buffer).map_err(CacheError::ReadError)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        Ok(hasher.finalize().to_hex().to_string())
    }

    /// Compute the BLAKE3 hash of bytes.
    pub fn hash_bytes(data: &[u8]) -> String {
        blake3::hash(data).to_hex().to_string()
    }

    /// Get the path for a content-addressed file.
    fn content_path(&self, hash: &str) -> PathBuf {
        let prefix = &hash[..2];
        self.files_dir().join(prefix).join(hash)
    }

    /// Store a file in the content-addressed store.
    pub fn store_file(&self, path: &Path) -> Result<String, CacheError> {
        let hash = Self::hash_file(path)?;
        let dest = self.content_path(&hash);

        if !dest.exists() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(CacheError::CreateDirError)?;
            }
            fs::copy(path, &dest).map_err(CacheError::WriteError)?;
            debug!("Stored file {} -> {}", path.display(), hash);
        }

        Ok(hash)
    }

    /// Store bytes in the content-addressed store.
    pub fn store_bytes(&self, data: &[u8]) -> Result<String, CacheError> {
        let hash = Self::hash_bytes(data);
        let dest = self.content_path(&hash);

        if !dest.exists() {
            if let Some(parent) = dest.parent() {
                fs::create_dir_all(parent).map_err(CacheError::CreateDirError)?;
            }
            let mut file = File::create(&dest).map_err(CacheError::WriteError)?;
            file.write_all(data).map_err(CacheError::WriteError)?;
            debug!("Stored {} bytes -> {}", data.len(), hash);
        }

        Ok(hash)
    }

    /// Link a cached file to a destination path.
    pub fn link_file(&self, hash: &str, dest: &Path) -> Result<(), CacheError> {
        let src = self.content_path(hash);

        if let Some(parent) = dest.parent() {
            fs::create_dir_all(parent).map_err(CacheError::CreateDirError)?;
        }

        // Remove existing file if present
        if dest.exists() {
            fs::remove_file(dest).map_err(CacheError::WriteError)?;
        }

        // Try hard link first, fall back to copy
        if fs::hard_link(&src, dest).is_err() {
            debug!("Hard link failed, falling back to copy");
            fs::copy(&src, dest).map_err(CacheError::WriteError)?;
        }

        Ok(())
    }

    /// Check if a hash exists in the cache.
    pub fn contains(&self, hash: &str) -> bool {
        self.content_path(hash).exists()
    }

    /// Get the path to a cached file.
    pub fn get_path(&self, hash: &str) -> Option<PathBuf> {
        let path = self.content_path(hash);
        if path.exists() {
            Some(path)
        } else {
            None
        }
    }
}

/// Metadata for a cached package.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageMetadata {
    /// Package name
    pub name: String,
    /// Package version
    pub version: String,
    /// Map of file paths to content hashes
    pub files: HashMap<String, String>,
    /// Total size in bytes
    pub total_size: u64,
}

impl PackageMetadata {
    /// Save metadata to the cache.
    pub fn save(&self, cache: &GlobalCache) -> Result<(), CacheError> {
        let dir = cache
            .packages_dir()
            .join(format!("{}@{}", self.name, self.version));
        fs::create_dir_all(&dir).map_err(CacheError::CreateDirError)?;

        let path = dir.join("metadata.json");
        let content = serde_json::to_string_pretty(self)
            .map_err(|e| CacheError::WriteError(io::Error::new(io::ErrorKind::InvalidData, e)))?;
        fs::write(path, content).map_err(CacheError::WriteError)?;

        info!("Saved metadata for {}@{}", self.name, self.version);
        Ok(())
    }

    /// Load metadata from the cache.
    pub fn load(cache: &GlobalCache, name: &str, version: &str) -> Result<Self, CacheError> {
        let path = cache
            .packages_dir()
            .join(format!("{}@{}", name, version))
            .join("metadata.json");

        let content = fs::read_to_string(&path).map_err(CacheError::ReadError)?;
        serde_json::from_str(&content)
            .map_err(|e| CacheError::ReadError(io::Error::new(io::ErrorKind::InvalidData, e)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_hash_bytes() {
        let hash = GlobalCache::hash_bytes(b"hello world");
        assert_eq!(hash.len(), 64); // BLAKE3 produces 256-bit (64 hex char) hashes
    }

    #[test]
    fn test_store_and_retrieve() {
        let tmp = TempDir::new().unwrap();
        let cache = GlobalCache::open_at(tmp.path().to_path_buf()).unwrap();

        let hash = cache.store_bytes(b"test content").unwrap();
        assert!(cache.contains(&hash));

        let path = cache.get_path(&hash).unwrap();
        let content = fs::read_to_string(path).unwrap();
        assert_eq!(content, "test content");
    }

    #[test]
    fn test_link_file() {
        let tmp = TempDir::new().unwrap();
        let cache = GlobalCache::open_at(tmp.path().to_path_buf()).unwrap();

        let hash = cache.store_bytes(b"linked content").unwrap();

        let dest = tmp.path().join("linked_file.txt");
        cache.link_file(&hash, &dest).unwrap();

        let content = fs::read_to_string(dest).unwrap();
        assert_eq!(content, "linked content");
    }
}
