//! Manifest caching for fast repeated parsing.
//!
//! Caches parsed Package.swift results to avoid slow `swift package dump-package` calls.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Manifest cache for storing parsed Package.swift results.
pub struct ManifestCache {
    cache_dir: PathBuf,
}

impl ManifestCache {
    /// Open or create the manifest cache.
    pub fn open() -> io::Result<Self> {
        let cache_dir = directories::ProjectDirs::from("dev", "gust", "gust")
            .map(|d| d.cache_dir().join("manifests"))
            .unwrap_or_else(|| PathBuf::from("/tmp/gust-manifest-cache"));

        fs::create_dir_all(&cache_dir)?;
        Ok(Self { cache_dir })
    }

    /// Get the cache key for a Package.swift file.
    pub fn cache_key(path: &Path) -> io::Result<String> {
        let content = fs::read(path)?;
        let hash = blake3::hash(&content);
        Ok(hash.to_hex().to_string())
    }

    /// Get cached JSON for a Package.swift file.
    pub fn get(&self, key: &str) -> Option<String> {
        let cache_path = self.cache_dir.join(format!("{}.json", key));
        fs::read_to_string(cache_path).ok()
    }

    /// Store parsed JSON in the cache.
    pub fn put(&self, key: &str, json: &str) -> io::Result<()> {
        let cache_path = self.cache_dir.join(format!("{}.json", key));
        fs::write(cache_path, json)
    }

    /// Check if a cache entry exists.
    pub fn contains(&self, key: &str) -> bool {
        self.cache_dir.join(format!("{}.json", key)).exists()
    }

    /// Clear all cached manifests.
    pub fn clear(&self) -> io::Result<()> {
        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                fs::remove_file(entry.path())?;
            }
        }
        Ok(())
    }

    /// Get cache statistics.
    pub fn stats(&self) -> io::Result<CacheStats> {
        let mut count = 0;
        let mut size = 0;

        for entry in fs::read_dir(&self.cache_dir)? {
            let entry = entry?;
            if entry.path().extension().map(|e| e == "json").unwrap_or(false) {
                count += 1;
                size += entry.metadata()?.len();
            }
        }

        Ok(CacheStats { count, size })
    }
}

/// Cache statistics.
#[derive(Debug)]
pub struct CacheStats {
    pub count: usize,
    pub size: u64,
}
