//! Workspace loading and dependency inheritance.

use crate::discovery::WorkspaceDiscovery;
use crate::{Workspace, WorkspaceError, WorkspaceMember};
use gust_manifest::{find_manifest, ManifestType};
use gust_types::Dependency;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Result type for workspace loading.
pub type LoadedWorkspace = Result<Workspace, WorkspaceError>;

/// Workspace loader with dependency inheritance.
pub struct WorkspaceLoader {
    /// Whether to resolve inherited dependencies
    resolve_inheritance: bool,
}

impl Default for WorkspaceLoader {
    fn default() -> Self {
        Self::new()
    }
}

impl WorkspaceLoader {
    /// Create a new workspace loader.
    pub fn new() -> Self {
        Self {
            resolve_inheritance: true,
        }
    }

    /// Disable dependency inheritance resolution.
    pub fn without_inheritance(mut self) -> Self {
        self.resolve_inheritance = false;
        self
    }

    /// Load a workspace from a root directory.
    pub fn load(&self, root: &Path) -> LoadedWorkspace {
        // Load root manifest
        let (root_manifest, manifest_type) = find_manifest(root)?;

        if manifest_type != ManifestType::GustToml {
            return Err(WorkspaceError::InvalidConfig(
                "Workspace root must be a Gust.toml file".to_string(),
            ));
        }

        let config = root_manifest.workspace.clone().ok_or_else(|| {
            WorkspaceError::InvalidConfig("No [workspace] section found in root manifest".to_string())
        })?;

        // Build shared dependencies map
        let shared_dependencies = config.dependencies.clone();

        // Find and load members
        let member_paths =
            WorkspaceDiscovery::expand_members(root, &config.members, &config.exclude)?;

        let mut members = Vec::new();
        let member_names: Vec<String> = member_paths
            .iter()
            .filter_map(|p| {
                find_manifest(p)
                    .ok()
                    .map(|(m, _)| m.package.name.clone())
            })
            .collect();

        for member_path in member_paths {
            let member = self.load_member(&member_path, &shared_dependencies, &member_names)?;
            members.push(member);
        }

        Ok(Workspace {
            root: root.to_path_buf(),
            root_manifest,
            config,
            members,
            shared_dependencies,
        })
    }

    /// Load a single workspace member.
    fn load_member(
        &self,
        path: &Path,
        shared_deps: &HashMap<String, Dependency>,
        all_member_names: &[String],
    ) -> Result<WorkspaceMember, WorkspaceError> {
        let (mut manifest, _) = find_manifest(path)?;
        let name = manifest.package.name.clone();

        // Track which dependencies are on other workspace members
        let mut workspace_deps = Vec::new();

        // Process dependencies with workspace inheritance
        if self.resolve_inheritance {
            let mut resolved_deps = HashMap::new();

            for (dep_name, dep) in manifest.dependencies.drain() {
                // Check if this dependency is on another workspace member
                if all_member_names.contains(&dep_name) {
                    workspace_deps.push(dep_name.clone());
                    // Create a path dependency for workspace members
                    resolved_deps.insert(dep_name, dep);
                } else if dep.is_workspace_inherited() {
                    // Inherit from workspace shared dependencies
                    if let Some(shared_dep) = shared_deps.get(&dep_name) {
                        resolved_deps.insert(dep_name, shared_dep.clone());
                    } else {
                        return Err(WorkspaceError::InvalidConfig(format!(
                            "Dependency '{}' marked as workspace but not found in [workspace.dependencies]",
                            dep_name
                        )));
                    }
                } else {
                    resolved_deps.insert(dep_name, dep);
                }
            }

            manifest.dependencies = resolved_deps;
        }

        Ok(WorkspaceMember {
            path: path.to_path_buf(),
            name,
            manifest,
            workspace_deps,
        })
    }

    /// Load a workspace asynchronously.
    pub async fn load_async(&self, root: PathBuf) -> LoadedWorkspace {
        let loader = self.clone();
        tokio::task::spawn_blocking(move || loader.load(&root))
            .await
            .map_err(|e| WorkspaceError::IoError(std::io::Error::other(e.to_string())))?
    }
}

impl Clone for WorkspaceLoader {
    fn clone(&self) -> Self {
        Self {
            resolve_inheritance: self.resolve_inheritance,
        }
    }
}

/// Extension trait for Dependency to check workspace inheritance.
trait DependencyExt {
    fn is_workspace_inherited(&self) -> bool;
}

impl DependencyExt for Dependency {
    fn is_workspace_inherited(&self) -> bool {
        // A dependency is workspace-inherited if it has no version/git/path
        // In TOML this would be: `my-dep = { workspace = true }`
        // For now we detect this by checking if all source fields are None
        self.version.is_none() && self.git.is_none() && self.path.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_inheritance_check() {
        // Regular dependency with version
        let versioned = Dependency::registry("test", gust_types::VersionReq::parse("^1.0").unwrap());
        assert!(!versioned.is_workspace_inherited());

        // Bare dependency (workspace inherited)
        let bare = Dependency {
            name: "test".to_string(),
            version: None,
            git: None,
            branch: None,
            tag: None,
            revision: None,
            path: None,
            features: vec![],
            optional: false,
        };
        assert!(bare.is_workspace_inherited());
    }
}
