//! Workspace discovery from filesystem.
//!
//! Finds workspace roots by walking up directory tree looking for
//! Gust.toml files with [workspace] sections.

use crate::WorkspaceError;
use gust_manifest::{find_manifest, ManifestType};
use std::path::{Path, PathBuf};

/// Workspace discovery utilities.
pub struct WorkspaceDiscovery;

impl WorkspaceDiscovery {
    /// Find all workspace members by expanding glob patterns.
    pub fn expand_members(
        root: &Path,
        patterns: &[String],
        exclude: &[String],
    ) -> Result<Vec<PathBuf>, WorkspaceError> {
        let mut members = Vec::new();

        for pattern in patterns {
            let full_pattern = root.join(pattern);
            let full_pattern_str = full_pattern.to_string_lossy();

            for entry in glob::glob(&full_pattern_str)? {
                match entry {
                    Ok(path) => {
                        // Check if path should be excluded
                        if !Self::is_excluded(&path, root, exclude) {
                            // Only include directories with a manifest
                            if path.is_dir() && Self::has_manifest(&path) {
                                members.push(path);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::warn!("Failed to process glob entry: {}", e);
                    }
                }
            }
        }

        // Sort for deterministic ordering
        members.sort();
        Ok(members)
    }

    /// Check if a path should be excluded.
    fn is_excluded(path: &Path, root: &Path, exclude: &[String]) -> bool {
        let relative = path.strip_prefix(root).unwrap_or(path);

        for pattern in exclude {
            // Simple wildcard matching
            if pattern.contains('*') {
                let pattern_parts: Vec<&str> = pattern.split('*').collect();
                let path_str = relative.to_string_lossy();

                // Handle prefix/*suffix patterns
                if pattern_parts.len() == 2 {
                    let (prefix, suffix) = (pattern_parts[0], pattern_parts[1]);
                    if path_str.starts_with(prefix) && path_str.ends_with(suffix) {
                        return true;
                    }
                }
            } else if relative.to_string_lossy() == pattern.as_str() {
                return true;
            }
        }

        false
    }

    /// Check if a directory contains a manifest file.
    fn has_manifest(path: &Path) -> bool {
        path.join("Gust.toml").exists() || path.join("Package.swift").exists()
    }
}

/// Find the workspace root directory.
///
/// Walks up from the given path looking for a Gust.toml with a [workspace] section.
/// Returns the workspace root or an error if not found.
pub fn find_workspace_root(start: &Path) -> Result<PathBuf, WorkspaceError> {
    let mut current = start.to_path_buf();

    // Make absolute
    if !current.is_absolute() {
        current = std::env::current_dir()?.join(current);
    }

    // Walk up directory tree
    loop {
        let manifest_path = current.join("Gust.toml");

        if manifest_path.exists() {
            // Check if this manifest has a [workspace] section
            if let Ok((manifest, ManifestType::GustToml)) = find_manifest(&current) {
                if manifest.workspace.is_some() {
                    return Ok(current);
                }
            }
        }

        // Move to parent directory
        if let Some(parent) = current.parent() {
            if parent == current {
                // Reached root
                break;
            }
            current = parent.to_path_buf();
        } else {
            break;
        }
    }

    Err(WorkspaceError::NotFound(start.to_path_buf()))
}

/// Check if a directory is the root of a workspace.
#[allow(dead_code)]
pub fn is_workspace_root(path: &Path) -> bool {
    let manifest_path = path.join("Gust.toml");

    if manifest_path.exists() {
        if let Ok((manifest, ManifestType::GustToml)) = find_manifest(path) {
            return manifest.workspace.is_some();
        }
    }

    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_excluded() {
        let root = Path::new("/workspace");

        // Exact match
        assert!(WorkspaceDiscovery::is_excluded(
            Path::new("/workspace/deprecated"),
            root,
            &["deprecated".to_string()]
        ));

        // Wildcard suffix
        assert!(WorkspaceDiscovery::is_excluded(
            Path::new("/workspace/packages/deprecated-old"),
            root,
            &["packages/deprecated-*".to_string()]
        ));

        // Not excluded
        assert!(!WorkspaceDiscovery::is_excluded(
            Path::new("/workspace/packages/core"),
            root,
            &["deprecated".to_string()]
        ));
    }
}
