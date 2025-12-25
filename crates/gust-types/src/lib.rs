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
#[derive(Debug, Clone, Serialize, Deserialize)]
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
}

impl Default for Manifest {
    fn default() -> Self {
        Self {
            package: Package::default(),
            dependencies: HashMap::new(),
            dev_dependencies: HashMap::new(),
            targets: Vec::new(),
            binary_cache: None,
            build: None,
        }
    }
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
