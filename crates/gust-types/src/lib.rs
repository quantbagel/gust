//! Core types for the Gust package manager.
//!
//! This crate defines the fundamental data structures used throughout Gust,
//! including packages, dependencies, versions, and targets.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;

pub use semver::{Version, VersionReq};

/// A Swift package manifest.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Package {
    /// Package name
    pub name: String,
    /// Package version
    pub version: Version,
    /// Minimum Swift tools version required
    pub swift_tools_version: String,
    /// Package description
    #[serde(default)]
    pub description: Option<String>,
    /// License identifier
    #[serde(default)]
    pub license: Option<String>,
    /// Package authors
    #[serde(default)]
    pub authors: Vec<String>,
    /// Repository URL
    #[serde(default)]
    pub repository: Option<String>,
}

impl Default for Package {
    fn default() -> Self {
        Self {
            name: String::new(),
            version: Version::new(0, 1, 0),
            swift_tools_version: "5.9".to_string(),
            description: None,
            license: None,
            authors: Vec::new(),
            repository: None,
        }
    }
}

/// A dependency specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dependency {
    /// Dependency name
    pub name: String,
    /// Version requirement
    pub version: Option<VersionReq>,
    /// Git repository URL
    pub git: Option<String>,
    /// Git branch
    pub branch: Option<String>,
    /// Git tag
    pub tag: Option<String>,
    /// Git revision
    pub revision: Option<String>,
    /// Local path
    pub path: Option<PathBuf>,
    /// Optional features to enable
    #[serde(default)]
    pub features: Vec<String>,
    /// Is this an optional dependency?
    #[serde(default)]
    pub optional: bool,
}

impl Dependency {
    /// Create a new registry dependency with a version requirement.
    pub fn registry(name: impl Into<String>, version: VersionReq) -> Self {
        Self {
            name: name.into(),
            version: Some(version),
            git: None,
            branch: None,
            tag: None,
            revision: None,
            path: None,
            features: Vec::new(),
            optional: false,
        }
    }

    /// Create a new git dependency.
    pub fn git(name: impl Into<String>, url: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            version: None,
            git: Some(url.into()),
            branch: None,
            tag: None,
            revision: None,
            path: None,
            features: Vec::new(),
            optional: false,
        }
    }

    /// Create a new path dependency.
    pub fn path(name: impl Into<String>, path: impl Into<PathBuf>) -> Self {
        Self {
            name: name.into(),
            version: None,
            git: None,
            branch: None,
            tag: None,
            revision: None,
            path: Some(path.into()),
            features: Vec::new(),
            optional: false,
        }
    }

    /// Set the git branch.
    pub fn with_branch(mut self, branch: impl Into<String>) -> Self {
        self.branch = Some(branch.into());
        self
    }

    /// Set the git tag.
    pub fn with_tag(mut self, tag: impl Into<String>) -> Self {
        self.tag = Some(tag.into());
        self
    }

    /// Returns the source kind of this dependency.
    pub fn source_kind(&self) -> DependencySource {
        if self.path.is_some() {
            DependencySource::Path
        } else if self.git.is_some() {
            DependencySource::Git
        } else {
            DependencySource::Registry
        }
    }
}

/// The source type of a dependency.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencySource {
    /// From a package registry
    Registry,
    /// From a git repository
    Git,
    /// From a local path
    Path,
}

/// A build target (executable, library, test, etc).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Target {
    /// Target name
    pub name: String,
    /// Target type
    #[serde(rename = "type")]
    pub target_type: TargetType,
    /// Source path (relative to package root)
    #[serde(default)]
    pub path: Option<PathBuf>,
    /// Dependencies specific to this target
    #[serde(default)]
    pub dependencies: Vec<String>,
    /// Resources to include
    #[serde(default)]
    pub resources: Vec<PathBuf>,
}

impl Target {
    /// Create a new executable target.
    pub fn executable(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            target_type: TargetType::Executable,
            path: None,
            dependencies: Vec::new(),
            resources: Vec::new(),
        }
    }

    /// Create a new library target.
    pub fn library(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            target_type: TargetType::Library,
            path: None,
            dependencies: Vec::new(),
            resources: Vec::new(),
        }
    }

    /// Create a new test target.
    pub fn test(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            target_type: TargetType::Test,
            path: None,
            dependencies: Vec::new(),
            resources: Vec::new(),
        }
    }
}

/// The type of a build target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TargetType {
    /// An executable binary
    Executable,
    /// A library
    Library,
    /// A test target
    Test,
    /// A plugin
    Plugin,
    /// A system library
    SystemLibrary,
    /// A binary target (pre-built)
    Binary,
}

/// A complete package manifest with all dependencies and targets.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Manifest {
    /// Package metadata
    pub package: Package,
    /// Regular dependencies
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
    /// Development-only dependencies
    #[serde(default)]
    pub dev_dependencies: HashMap<String, Dependency>,
    /// Build targets
    #[serde(default)]
    pub targets: Vec<Target>,
    /// Binary cache configuration
    #[serde(default)]
    pub binary_cache: Option<BinaryCacheConfig>,
    /// Build settings
    #[serde(default)]
    pub build: Option<BuildSettings>,
    /// Version overrides (force specific versions)
    #[serde(default)]
    pub overrides: HashMap<String, String>,
    /// Additional version constraints
    #[serde(default)]
    pub constraints: HashMap<String, String>,
    /// Workspace configuration (only present at workspace root)
    #[serde(default)]
    pub workspace: Option<WorkspaceConfig>,
}

/// Binary cache configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BinaryCacheConfig {
    /// Remote cache URL
    pub url: String,
    /// Allow reading from cache
    #[serde(default = "default_true")]
    pub read: bool,
    /// Allow writing to cache
    #[serde(default)]
    pub write: bool,
}

fn default_true() -> bool {
    true
}

/// Build settings.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BuildSettings {
    /// Number of parallel jobs
    #[serde(default)]
    pub parallel_jobs: Option<usize>,
    /// Enable incremental builds
    #[serde(default)]
    pub incremental: Option<bool>,
    /// Extra Swift compiler flags
    #[serde(default)]
    pub swift_flags: Vec<String>,
    /// Extra C compiler flags
    #[serde(default)]
    pub c_flags: Vec<String>,
    /// Extra linker flags
    #[serde(default)]
    pub link_flags: Vec<String>,
}

/// A resolved package in the dependency graph.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResolvedPackage {
    /// Package name
    pub name: String,
    /// Resolved version
    pub version: Version,
    /// Source of the package
    pub source: DependencySource,
    /// Content hash (BLAKE3)
    pub checksum: Option<String>,
    /// Git URL (for git deps)
    pub git: Option<String>,
    /// Git revision (for git deps)
    pub revision: Option<String>,
    /// Resolved dependencies
    pub dependencies: Vec<String>,
}

/// Build configuration (debug/release).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum BuildConfiguration {
    /// Debug build with no optimizations
    #[default]
    Debug,
    /// Release build with optimizations
    Release,
}

impl std::fmt::Display for BuildConfiguration {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BuildConfiguration::Debug => write!(f, "debug"),
            BuildConfiguration::Release => write!(f, "release"),
        }
    }
}

/// Platform specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Platform {
    /// Platform name (macOS, iOS, tvOS, watchOS, linux)
    pub name: String,
    /// Minimum version
    pub version: Option<String>,
}

// ============================================================================
// Resolution Configuration Types
// ============================================================================

/// Version override configuration.
///
/// Overrides force a specific version to be used regardless of what other
/// packages request. This is useful for resolving conflicts or pinning
/// specific versions for security reasons.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionOverride {
    /// The package name to override
    pub package: String,
    /// The version to force
    pub version: VersionReq,
}

/// Version constraint configuration.
///
/// Constraints add additional version requirements without adding the package
/// as a direct dependency. This is useful for enforcing minimum versions
/// across transitive dependencies.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionConstraint {
    /// The package name to constrain
    pub package: String,
    /// The version constraint to apply
    pub version: VersionReq,
}

/// Resolution options that affect how dependencies are resolved.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolutionOptions {
    /// Version overrides (force specific versions)
    #[serde(default)]
    pub overrides: Vec<VersionOverride>,
    /// Additional version constraints
    #[serde(default)]
    pub constraints: Vec<VersionConstraint>,
    /// Resolution strategy
    #[serde(default)]
    pub strategy: ResolutionStrategy,
    /// Whether to prefer pre-release versions
    #[serde(default)]
    pub prefer_prerelease: bool,
}

/// The strategy used to select versions during resolution.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ResolutionStrategy {
    /// Always pick the highest compatible version (default)
    #[default]
    Highest,
    /// Pick the lowest compatible version (useful for testing compatibility)
    Lowest,
    /// Prefer versions from the existing lockfile
    Locked,
}

/// Metadata about why a version was selected during resolution.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ResolutionMetadata {
    /// Packages that required this dependency
    #[serde(default, rename = "required-by")]
    pub required_by: Vec<String>,
    /// The version constraints that led to this choice
    #[serde(default)]
    pub constraints: Vec<ConstraintInfo>,
}

/// Information about a constraint that was applied.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConstraintInfo {
    /// The package that imposed this constraint
    pub from: String,
    /// The version requirement string
    pub requirement: String,
}

// ============================================================================
// Workspace Types
// ============================================================================

/// Workspace configuration for multi-package monorepos.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspaceConfig {
    /// Glob patterns for member packages
    #[serde(default)]
    pub members: Vec<String>,
    /// Glob patterns to exclude from member discovery
    #[serde(default)]
    pub exclude: Vec<String>,
    /// Shared dependency versions (inherited by members)
    #[serde(default)]
    pub dependencies: HashMap<String, Dependency>,
    /// Shared dev dependencies
    #[serde(default, rename = "dev-dependencies")]
    pub dev_dependencies: HashMap<String, Dependency>,
    /// Default package metadata for workspace members
    #[serde(default)]
    pub package: Option<WorkspacePackageDefaults>,
}

/// Default package metadata that can be inherited by workspace members.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkspacePackageDefaults {
    /// Default Swift tools version
    #[serde(default, rename = "swift-tools-version")]
    pub swift_tools_version: Option<String>,
    /// Default authors
    #[serde(default)]
    pub authors: Option<Vec<String>>,
    /// Default license
    #[serde(default)]
    pub license: Option<String>,
    /// Default repository
    #[serde(default)]
    pub repository: Option<String>,
}

/// A workspace member package.
#[derive(Debug, Clone)]
pub struct WorkspaceMember {
    /// Path relative to workspace root
    pub path: PathBuf,
    /// Absolute path for file operations
    pub absolute_path: PathBuf,
    /// Package name
    pub name: String,
    /// Parsed manifest
    pub manifest: Manifest,
    /// Whether this member is also the workspace root
    pub is_root: bool,
}

/// A complete workspace.
#[derive(Debug, Clone)]
pub struct Workspace {
    /// Workspace root directory
    pub root: PathBuf,
    /// Workspace configuration
    pub config: WorkspaceConfig,
    /// All member packages
    pub members: Vec<WorkspaceMember>,
    /// Map of member name to index for quick lookup
    pub member_index: HashMap<String, usize>,
}

// ============================================================================
// Plugin Types
// ============================================================================

/// Plugin capability type.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PluginCapability {
    /// Build tool plugins run during build to generate sources/resources
    BuildTool,
    /// Command plugins provide custom CLI commands
    Command(CommandPluginCapability),
}

/// Command plugin configuration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandPluginCapability {
    /// The intent/purpose of this command plugin
    pub intent: CommandIntent,
    /// Permissions required by the plugin
    #[serde(default)]
    pub permissions: Vec<PluginPermission>,
}

/// Command plugin intent.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CommandIntent {
    /// Documentation generation
    DocumentationGeneration,
    /// Source code formatting
    SourceCodeFormatting,
    /// Custom intent with description
    Custom { verb: String, description: String },
}

/// Permissions that plugins can request.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PluginPermission {
    /// Write to package directory
    WriteToPackageDirectory { reason: String },
    /// Network access
    AllowNetworkConnections { scope: NetworkScope, reason: String },
}

/// Network access scope for plugins.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NetworkScope {
    /// No network access
    None,
    /// Local connections only (with port)
    Local(u16),
    /// All outgoing connections
    All,
    /// Docker connections
    Docker,
}

/// Information about a discovered plugin.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Plugin name
    pub name: String,
    /// Which package provides this plugin
    pub providing_package: String,
    /// Plugin capability
    pub capability: PluginCapability,
    /// Path to the compiled plugin executable
    pub executable_path: PathBuf,
    /// Path to the plugin source
    pub source_path: PathBuf,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dependency_creation() {
        let dep = Dependency::registry("swift-log", VersionReq::parse("^1.5").unwrap());
        assert_eq!(dep.name, "swift-log");
        assert_eq!(dep.source_kind(), DependencySource::Registry);

        let git_dep = Dependency::git("alamofire", "https://github.com/Alamofire/Alamofire.git")
            .with_tag("5.8.0");
        assert_eq!(git_dep.source_kind(), DependencySource::Git);
        assert_eq!(git_dep.tag, Some("5.8.0".to_string()));
    }

    #[test]
    fn test_target_creation() {
        let exe = Target::executable("MyApp");
        assert_eq!(exe.target_type, TargetType::Executable);

        let lib = Target::library("MyLib");
        assert_eq!(lib.target_type, TargetType::Library);
    }
}
