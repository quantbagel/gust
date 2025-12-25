//! Unified resolution across workspace members.
//!
//! Resolves all dependencies across the workspace at once,
//! ensuring consistent versions for shared dependencies.

use crate::{Workspace, WorkspaceError};
use gust_resolver::{LockfileHints, PackageProvider, Resolution, Resolver};
use gust_types::{Manifest, ResolutionStrategy};
use std::collections::HashMap;

/// Workspace-aware dependency resolver.
pub struct WorkspaceResolver<P: PackageProvider> {
    provider: P,
    hints: LockfileHints,
    strategy: ResolutionStrategy,
}

impl<P: PackageProvider> WorkspaceResolver<P> {
    /// Create a new workspace resolver.
    pub fn new(provider: P) -> Self {
        Self {
            provider,
            hints: LockfileHints::new(),
            strategy: ResolutionStrategy::Highest,
        }
    }

    /// Set lockfile hints for preferring locked versions.
    pub fn with_hints(mut self, hints: LockfileHints) -> Self {
        self.hints = hints;
        self
    }

    /// Set the resolution strategy.
    pub fn with_strategy(mut self, strategy: ResolutionStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Resolve all dependencies across the workspace.
    ///
    /// Creates a unified resolution that includes all external dependencies
    /// from all workspace members.
    pub fn resolve(&self, workspace: &Workspace) -> Result<WorkspaceResolution, WorkspaceError> {
        // Collect all external dependencies from all members
        let all_deps = workspace.all_external_dependencies();

        // Create a synthetic root manifest that depends on everything
        let mut root_manifest = Manifest::default();
        root_manifest.package.name = format!("{}-workspace", workspace.root_manifest.package.name);
        root_manifest.package.version = workspace.root_manifest.package.version.clone();
        root_manifest.dependencies = all_deps;

        // Apply workspace overrides
        root_manifest.overrides = workspace.root_manifest.overrides.clone();
        root_manifest.constraints = workspace.root_manifest.constraints.clone();

        // Resolve using the unified manifest
        let resolver = Resolver::new(&self.provider)
            .with_hints(self.hints.clone())
            .with_strategy(self.strategy);

        let resolution = resolver.resolve(&root_manifest).map_err(Box::new)?;

        // Build member-specific resolutions
        let mut member_resolutions = HashMap::new();

        for member in &workspace.members {
            let member_deps: Vec<String> = member
                .manifest
                .dependencies
                .keys()
                .filter(|name| {
                    // Only include external dependencies (not other workspace members)
                    !workspace.members.iter().any(|m| &m.name == *name)
                })
                .cloned()
                .collect();

            member_resolutions.insert(member.name.clone(), member_deps);
        }

        Ok(WorkspaceResolution {
            resolution,
            member_deps: member_resolutions,
            workspace_root: workspace.root.clone(),
        })
    }
}

/// Resolution result for a workspace.
#[derive(Debug, Clone)]
pub struct WorkspaceResolution {
    /// The unified resolution across all members
    pub resolution: Resolution,

    /// Map of member name to its external dependency names
    pub member_deps: HashMap<String, Vec<String>>,

    /// Path to workspace root
    pub workspace_root: std::path::PathBuf,
}

impl WorkspaceResolution {
    /// Get the resolved packages for a specific member.
    pub fn packages_for_member(&self, member_name: &str) -> Vec<&gust_resolver::ResolvedDep> {
        if let Some(dep_names) = self.member_deps.get(member_name) {
            dep_names
                .iter()
                .filter_map(|name| self.resolution.packages.get(name))
                .collect()
        } else {
            vec![]
        }
    }

    /// Get all resolved packages.
    pub fn all_packages(&self) -> impl Iterator<Item = &gust_resolver::ResolvedDep> {
        self.resolution.packages.values()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use gust_resolver::MemoryProvider;
    use gust_types::{Dependency, Package, Version, VersionReq, WorkspaceConfig};

    fn create_test_workspace() -> Workspace {
        let mut core_manifest = Manifest::default();
        core_manifest.package = Package {
            name: "core".to_string(),
            version: Version::new(0, 1, 0),
            ..Default::default()
        };
        core_manifest.dependencies.insert(
            "swift-log".to_string(),
            Dependency::registry("swift-log", VersionReq::parse("^1.4").unwrap()),
        );

        let mut cli_manifest = Manifest::default();
        cli_manifest.package = Package {
            name: "cli".to_string(),
            version: Version::new(0, 1, 0),
            ..Default::default()
        };
        cli_manifest.dependencies.insert(
            "swift-log".to_string(),
            Dependency::registry("swift-log", VersionReq::parse("^1.4").unwrap()),
        );
        cli_manifest.dependencies.insert(
            "swift-argument-parser".to_string(),
            Dependency::registry("swift-argument-parser", VersionReq::parse("^1.2").unwrap()),
        );

        Workspace {
            root: std::path::PathBuf::from("/workspace"),
            root_manifest: Manifest::default(),
            config: WorkspaceConfig::default(),
            members: vec![
                crate::WorkspaceMember {
                    path: std::path::PathBuf::from("/workspace/packages/core"),
                    name: "core".to_string(),
                    manifest: core_manifest,
                    workspace_deps: vec![],
                },
                crate::WorkspaceMember {
                    path: std::path::PathBuf::from("/workspace/packages/cli"),
                    name: "cli".to_string(),
                    manifest: cli_manifest,
                    workspace_deps: vec!["core".to_string()],
                },
            ],
            shared_dependencies: HashMap::new(),
        }
    }

    #[test]
    fn test_workspace_collects_all_deps() {
        let workspace = create_test_workspace();
        let all_deps = workspace.all_external_dependencies();

        assert!(all_deps.contains_key("swift-log"));
        assert!(all_deps.contains_key("swift-argument-parser"));
        // core is a workspace member, should not be in external deps
        assert!(!all_deps.contains_key("core"));
    }

    #[test]
    fn test_workspace_resolution() {
        let workspace = create_test_workspace();

        // Create a test provider
        let mut provider = MemoryProvider::new();
        provider.add_package("swift-log", Version::new(1, 5, 4), vec![]);
        provider.add_package("swift-argument-parser", Version::new(1, 3, 0), vec![]);

        let resolver = WorkspaceResolver::new(provider);
        let resolution = resolver.resolve(&workspace).unwrap();

        // Both deps should be resolved
        assert!(resolution.resolution.packages.contains_key("swift-log"));
        assert!(resolution
            .resolution
            .packages
            .contains_key("swift-argument-parser"));
    }
}
