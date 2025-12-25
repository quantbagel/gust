//! Workspace management for Gust.
//!
//! This crate provides multi-package monorepo support including:
//! - Workspace discovery from Gust.toml
//! - Dependency inheritance from workspace to members
//! - Unified resolution across all workspace members
//! - Filter-based operations on subset of members

mod discovery;
mod loader;
mod resolver;

pub use discovery::{find_workspace_root, WorkspaceDiscovery};
pub use loader::{LoadedWorkspace, WorkspaceLoader};
pub use resolver::WorkspaceResolver;

use gust_types::{Dependency, Manifest, WorkspaceConfig};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum WorkspaceError {
    #[error("No workspace found in {0} or any parent directory")]
    NotFound(PathBuf),

    #[error("Failed to read manifest: {0}")]
    ManifestError(#[from] gust_manifest::ManifestError),

    #[error("Invalid workspace configuration: {0}")]
    InvalidConfig(String),

    #[error("Member not found: {0}")]
    MemberNotFound(String),

    #[error("Glob pattern error: {0}")]
    GlobError(#[from] glob::PatternError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Cycle detected in workspace dependencies")]
    CycleDetected { cycle: Vec<String> },

    #[error("Resolver error: {0}")]
    ResolverError(#[from] Box<gust_resolver::ResolveError>),
}

/// Represents a loaded workspace.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Path to the workspace root (contains Gust.toml with [workspace])
    pub root: PathBuf,

    /// The root manifest with workspace configuration
    pub root_manifest: Manifest,

    /// Workspace configuration
    pub config: WorkspaceConfig,

    /// Loaded workspace members
    pub members: Vec<WorkspaceMember>,

    /// Shared dependencies that can be inherited
    pub shared_dependencies: HashMap<String, Dependency>,
}

/// A single workspace member.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    /// Path to the member directory
    pub path: PathBuf,

    /// Member name (from manifest or directory name)
    pub name: String,

    /// Resolved manifest with inherited dependencies
    pub manifest: Manifest,

    /// Dependencies on other workspace members
    pub workspace_deps: Vec<String>,
}

impl Workspace {
    /// Check if a path is a workspace member.
    pub fn is_member(&self, path: &Path) -> bool {
        self.members.iter().any(|m| m.path == path)
    }

    /// Get a member by name.
    pub fn get_member(&self, name: &str) -> Option<&WorkspaceMember> {
        self.members.iter().find(|m| m.name == name)
    }

    /// Get all member names.
    pub fn member_names(&self) -> Vec<&str> {
        self.members.iter().map(|m| m.name.as_str()).collect()
    }

    /// Get members matching a filter pattern.
    pub fn filter_members(&self, pattern: &str) -> Vec<&WorkspaceMember> {
        if pattern == "*" {
            return self.members.iter().collect();
        }

        // Simple glob-like matching
        let pattern = pattern.replace('*', "");
        self.members
            .iter()
            .filter(|m| m.name.contains(&pattern) || m.path.to_string_lossy().contains(&pattern))
            .collect()
    }

    /// Get the topological order for building members.
    ///
    /// Members with no dependencies on other workspace members come first.
    pub fn build_order(&self) -> Result<Vec<&WorkspaceMember>, WorkspaceError> {
        let mut order = Vec::new();
        let mut visited = HashMap::new();
        let mut in_stack = HashMap::new();

        for member in &self.members {
            if !visited.contains_key(&member.name) {
                self.topo_visit(member, &mut visited, &mut in_stack, &mut order)?;
            }
        }

        Ok(order)
    }

    fn topo_visit<'a>(
        &'a self,
        member: &'a WorkspaceMember,
        visited: &mut HashMap<String, bool>,
        in_stack: &mut HashMap<String, bool>,
        order: &mut Vec<&'a WorkspaceMember>,
    ) -> Result<(), WorkspaceError> {
        visited.insert(member.name.clone(), true);
        in_stack.insert(member.name.clone(), true);

        for dep_name in &member.workspace_deps {
            if let Some(dep_member) = self.get_member(dep_name) {
                if in_stack.get(dep_name).copied().unwrap_or(false) {
                    // Cycle detected
                    return Err(WorkspaceError::CycleDetected {
                        cycle: vec![member.name.clone(), dep_name.clone()],
                    });
                }

                if !visited.contains_key(dep_name) {
                    self.topo_visit(dep_member, visited, in_stack, order)?;
                }
            }
        }

        in_stack.insert(member.name.clone(), false);
        order.push(member);
        Ok(())
    }

    /// Collect all external dependencies across all workspace members.
    pub fn all_external_dependencies(&self) -> HashMap<String, Dependency> {
        let mut all_deps = HashMap::new();

        for member in &self.members {
            for (name, dep) in &member.manifest.dependencies {
                // Skip workspace members
                if !self.members.iter().any(|m| m.name == *name) {
                    all_deps.insert(name.clone(), dep.clone());
                }
            }
        }

        all_deps
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_filter_members() {
        let ws = Workspace {
            root: PathBuf::from("/workspace"),
            root_manifest: Manifest::default(),
            config: WorkspaceConfig::default(),
            members: vec![
                WorkspaceMember {
                    path: PathBuf::from("/workspace/packages/core"),
                    name: "core".to_string(),
                    manifest: Manifest::default(),
                    workspace_deps: vec![],
                },
                WorkspaceMember {
                    path: PathBuf::from("/workspace/packages/cli"),
                    name: "cli".to_string(),
                    manifest: Manifest::default(),
                    workspace_deps: vec!["core".to_string()],
                },
            ],
            shared_dependencies: HashMap::new(),
        };

        assert_eq!(ws.filter_members("*").len(), 2);
        assert_eq!(ws.filter_members("core").len(), 1);
        assert_eq!(ws.filter_members("cli").len(), 1);
    }
}
