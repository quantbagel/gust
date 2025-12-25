//! Manifest parsing for Gust.
//!
//! Supports both Gust.toml and Package.swift formats,
//! and can generate Package.swift from Gust.toml.

mod cache;
mod generate;

pub use cache::{CacheStats, ManifestCache};
pub use generate::{generate_package_swift, write_package_swift};
use gust_types::{
    BinaryCacheConfig, BuildSettings, Dependency, Manifest, Package, Target, TargetType, Version,
    VersionReq, WorkspaceConfig, WorkspacePackageDefaults,
};
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ManifestError {
    #[error("Manifest not found in {0}")]
    NotFound(PathBuf),
    #[error("Failed to read manifest: {0}")]
    ReadError(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    TomlError(#[from] toml::de::Error),
    #[error("Failed to parse Package.swift: {0}")]
    SwiftParseError(String),
    #[error("Invalid manifest: {0}")]
    ValidationError(String),
}

/// The manifest file type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManifestType {
    /// Gust.toml
    GustToml,
    /// Package.swift
    PackageSwift,
}

/// Find and load a manifest from a directory.
pub fn find_manifest(dir: &Path) -> Result<(Manifest, ManifestType), ManifestError> {
    let gust_toml = dir.join("Gust.toml");
    let package_swift = dir.join("Package.swift");

    if gust_toml.exists() {
        let manifest = parse_gust_toml(&gust_toml)?;
        Ok((manifest, ManifestType::GustToml))
    } else if package_swift.exists() {
        let manifest = parse_package_swift(&package_swift)?;
        Ok((manifest, ManifestType::PackageSwift))
    } else {
        Err(ManifestError::NotFound(dir.to_path_buf()))
    }
}

/// Raw TOML structure for Gust.toml
#[derive(Debug, Deserialize)]
struct RawGustToml {
    package: RawPackage,
    #[serde(default)]
    dependencies: HashMap<String, RawDependency>,
    #[serde(default, rename = "dev-dependencies")]
    dev_dependencies: HashMap<String, RawDependency>,
    #[serde(default)]
    target: Vec<RawTarget>,
    #[serde(default, rename = "binary-cache")]
    binary_cache: Option<BinaryCacheConfig>,
    #[serde(default)]
    build: Option<BuildSettings>,
    /// Version overrides - force specific versions regardless of constraints
    #[serde(default)]
    overrides: HashMap<String, String>,
    /// Additional version constraints without adding as dependencies
    #[serde(default)]
    constraints: HashMap<String, String>,
    /// Workspace configuration (if this is a workspace root)
    #[serde(default)]
    workspace: Option<RawWorkspace>,
}

#[derive(Debug, Deserialize)]
struct RawPackage {
    name: String,
    version: String,
    #[serde(
        default = "default_swift_tools_version",
        rename = "swift-tools-version"
    )]
    swift_tools_version: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    authors: Vec<String>,
    #[serde(default)]
    repository: Option<String>,
}

fn default_swift_tools_version() -> String {
    "5.9".to_string()
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum RawDependency {
    /// Simple version string: "1.0"
    Version(String),
    /// Full dependency specification
    Full {
        #[serde(default)]
        version: Option<String>,
        #[serde(default)]
        git: Option<String>,
        #[serde(default)]
        branch: Option<String>,
        #[serde(default)]
        tag: Option<String>,
        #[serde(default)]
        revision: Option<String>,
        #[serde(default)]
        path: Option<PathBuf>,
        #[serde(default)]
        features: Vec<String>,
        #[serde(default)]
        optional: bool,
    },
}

#[derive(Debug, Deserialize)]
struct RawTarget {
    name: String,
    #[serde(rename = "type")]
    target_type: String,
    #[serde(default)]
    path: Option<PathBuf>,
    #[serde(default)]
    dependencies: Vec<String>,
}

/// Raw workspace configuration
#[derive(Debug, Deserialize)]
struct RawWorkspace {
    /// Glob patterns for workspace members
    #[serde(default)]
    members: Vec<String>,
    /// Glob patterns to exclude from workspace
    #[serde(default)]
    exclude: Vec<String>,
    /// Shared dependencies for workspace inheritance
    #[serde(default)]
    dependencies: HashMap<String, RawDependency>,
    /// Default package metadata for workspace members
    #[serde(default, rename = "package")]
    package_defaults: Option<RawWorkspacePackageDefaults>,
}

/// Default package values that can be inherited by workspace members
#[derive(Debug, Deserialize, Default)]
struct RawWorkspacePackageDefaults {
    #[serde(default, rename = "swift-tools-version")]
    swift_tools_version: Option<String>,
    #[serde(default)]
    authors: Option<Vec<String>>,
    #[serde(default)]
    license: Option<String>,
    #[serde(default)]
    repository: Option<String>,
}

/// Parse a Gust.toml file.
pub fn parse_gust_toml(path: &Path) -> Result<Manifest, ManifestError> {
    let content = std::fs::read_to_string(path)?;
    let raw: RawGustToml = toml::from_str(&content)?;

    let version = Version::parse(&raw.package.version)
        .map_err(|e| ManifestError::ValidationError(format!("Invalid version: {}", e)))?;

    let package = Package {
        name: raw.package.name,
        version,
        swift_tools_version: raw.package.swift_tools_version,
        description: raw.package.description,
        license: raw.package.license,
        authors: raw.package.authors,
        repository: raw.package.repository,
    };

    let dependencies = raw
        .dependencies
        .into_iter()
        .map(|(name, raw)| {
            let dep = parse_raw_dependency(&name, raw)?;
            Ok((name, dep))
        })
        .collect::<Result<HashMap<_, _>, ManifestError>>()?;

    let dev_dependencies = raw
        .dev_dependencies
        .into_iter()
        .map(|(name, raw)| {
            let dep = parse_raw_dependency(&name, raw)?;
            Ok((name, dep))
        })
        .collect::<Result<HashMap<_, _>, ManifestError>>()?;

    let targets = raw
        .target
        .into_iter()
        .map(|t| {
            let target_type = match t.target_type.as_str() {
                "executable" => TargetType::Executable,
                "library" => TargetType::Library,
                "test" => TargetType::Test,
                "plugin" => TargetType::Plugin,
                other => {
                    return Err(ManifestError::ValidationError(format!(
                        "Unknown target type: {}",
                        other
                    )))
                }
            };
            Ok(Target {
                name: t.name,
                target_type,
                path: t.path,
                dependencies: t.dependencies,
                resources: Vec::new(),
            })
        })
        .collect::<Result<Vec<_>, ManifestError>>()?;

    // Convert workspace config if present
    let workspace = raw.workspace.map(|ws| {
        // Parse workspace shared dependencies
        let deps = ws
            .dependencies
            .into_iter()
            .filter_map(|(name, raw_dep)| {
                parse_raw_dependency(&name, raw_dep)
                    .ok()
                    .map(|dep| (name, dep))
            })
            .collect();

        WorkspaceConfig {
            members: ws.members,
            exclude: ws.exclude,
            dependencies: deps,
            dev_dependencies: HashMap::new(), // Could add [workspace.dev-dependencies] in future
            package: ws.package_defaults.map(|p| WorkspacePackageDefaults {
                swift_tools_version: p.swift_tools_version,
                authors: p.authors,
                license: p.license,
                repository: p.repository,
            }),
        }
    });

    Ok(Manifest {
        package,
        dependencies,
        dev_dependencies,
        targets,
        binary_cache: raw.binary_cache,
        build: raw.build,
        overrides: raw.overrides,
        constraints: raw.constraints,
        workspace,
    })
}

fn parse_raw_dependency(name: &str, raw: RawDependency) -> Result<Dependency, ManifestError> {
    match raw {
        RawDependency::Version(v) => {
            let version_req = VersionReq::parse(&v).map_err(|e| {
                ManifestError::ValidationError(format!("Invalid version for {}: {}", name, e))
            })?;
            Ok(Dependency::registry(name, version_req))
        }
        RawDependency::Full {
            version,
            git,
            branch,
            tag,
            revision,
            path,
            features,
            optional,
        } => {
            let mut dep = if let Some(path) = path {
                Dependency::path(name, path)
            } else if let Some(git_url) = git {
                let mut d = Dependency::git(name, git_url);
                if let Some(b) = branch {
                    d = d.with_branch(b);
                }
                if let Some(t) = tag {
                    d = d.with_tag(t);
                }
                d.revision = revision;
                d
            } else if let Some(v) = version {
                let version_req = VersionReq::parse(&v).map_err(|e| {
                    ManifestError::ValidationError(format!("Invalid version for {}: {}", name, e))
                })?;
                Dependency::registry(name, version_req)
            } else {
                return Err(ManifestError::ValidationError(format!(
                    "Dependency {} must have version, git, or path",
                    name
                )));
            };

            dep.features = features;
            dep.optional = optional;
            Ok(dep)
        }
    }
}

/// Parse a Package.swift file by executing `swift package dump-package`.
///
/// Results are cached based on the BLAKE3 hash of the Package.swift content,
/// so repeated parsing of unchanged manifests is instant.
pub fn parse_package_swift(path: &Path) -> Result<Manifest, ManifestError> {
    // Try to use cache for fast path
    let cache = ManifestCache::open().ok();
    let cache_key = ManifestCache::cache_key(path).ok();

    // Check cache first
    if let (Some(ref cache), Some(ref key)) = (&cache, &cache_key) {
        if let Some(cached_json) = cache.get(key) {
            tracing::debug!("Cache hit for {}", path.display());
            let json: serde_json::Value = serde_json::from_str(&cached_json)
                .map_err(|e| ManifestError::SwiftParseError(e.to_string()))?;
            return convert_spm_json(json);
        }
    }

    // Cache miss - run swift package dump-package (slow path)
    tracing::debug!(
        "Cache miss for {}, running swift package dump-package",
        path.display()
    );
    let dir = path.parent().unwrap_or(Path::new("."));

    let output = Command::new("swift")
        .arg("package")
        .arg("dump-package")
        .current_dir(dir)
        .output()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(ManifestError::SwiftParseError(stderr.to_string()));
    }

    let json_str = String::from_utf8_lossy(&output.stdout);

    // Store in cache for next time
    if let (Some(cache), Some(key)) = (&cache, &cache_key) {
        if let Err(e) = cache.put(key, &json_str) {
            tracing::warn!("Failed to cache manifest: {}", e);
        }
    }

    let json: serde_json::Value = serde_json::from_str(&json_str)
        .map_err(|e| ManifestError::SwiftParseError(e.to_string()))?;

    convert_spm_json(json)
}

fn convert_spm_json(json: serde_json::Value) -> Result<Manifest, ManifestError> {
    let name = json["name"]
        .as_str()
        .ok_or_else(|| ManifestError::SwiftParseError("Missing package name".to_string()))?;

    let package = Package {
        name: name.to_string(),
        version: Version::new(0, 0, 0), // SPM doesn't include version in manifest
        swift_tools_version: json["toolsVersion"]["_version"]
            .as_str()
            .unwrap_or("5.9")
            .to_string(),
        ..Default::default()
    };

    let mut dependencies = HashMap::new();
    if let Some(deps) = json["dependencies"].as_array() {
        for dep in deps {
            if let Some(scm) = dep["sourceControl"].as_array().and_then(|a| a.first()) {
                let dep_name = scm["identity"].as_str().unwrap_or("unknown");

                // URL is at location.remote[0].urlString
                let url = scm["location"]["remote"]
                    .as_array()
                    .and_then(|a| a.first())
                    .and_then(|v| v["urlString"].as_str())
                    .unwrap_or("");

                dependencies.insert(dep_name.to_string(), Dependency::git(dep_name, url));
            }
        }
    }

    let mut targets = Vec::new();
    if let Some(tgts) = json["targets"].as_array() {
        for tgt in tgts {
            let tgt_name = tgt["name"].as_str().unwrap_or("unknown");
            let tgt_type = match tgt["type"].as_str() {
                Some("executable") => TargetType::Executable,
                Some("test") => TargetType::Test,
                _ => TargetType::Library,
            };

            let tgt_deps: Vec<String> = tgt["dependencies"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .filter_map(|d| {
                            d["byName"]
                                .as_array()
                                .and_then(|a| a.first())
                                .and_then(|v| v.as_str())
                                .map(String::from)
                        })
                        .collect()
                })
                .unwrap_or_default();

            targets.push(Target {
                name: tgt_name.to_string(),
                target_type: tgt_type,
                path: tgt["path"].as_str().map(PathBuf::from),
                dependencies: tgt_deps,
                resources: Vec::new(),
            });
        }
    }

    Ok(Manifest {
        package,
        dependencies,
        dev_dependencies: HashMap::new(),
        targets,
        binary_cache: None,
        build: None,
        overrides: HashMap::new(),
        constraints: HashMap::new(),
        workspace: None,
    })
}

/// Generate a Gust.toml from a Manifest.
pub fn generate_gust_toml(manifest: &Manifest) -> String {
    let mut out = String::new();

    out.push_str("[package]\n");
    out.push_str(&format!("name = \"{}\"\n", manifest.package.name));
    out.push_str(&format!("version = \"{}\"\n", manifest.package.version));
    out.push_str(&format!(
        "swift-tools-version = \"{}\"\n",
        manifest.package.swift_tools_version
    ));

    if let Some(desc) = &manifest.package.description {
        out.push_str(&format!("description = \"{}\"\n", desc));
    }

    if !manifest.dependencies.is_empty() {
        out.push_str("\n[dependencies]\n");
        for (name, dep) in &manifest.dependencies {
            if let Some(v) = &dep.version {
                out.push_str(&format!("{} = \"{}\"\n", name, v));
            } else if let Some(git) = &dep.git {
                out.push_str(&format!("{} = {{ git = \"{}\" }}\n", name, git));
            }
        }
    }

    if !manifest.targets.is_empty() {
        for target in &manifest.targets {
            out.push_str("\n[[target]]\n");
            out.push_str(&format!("name = \"{}\"\n", target.name));
            out.push_str(&format!("type = \"{:?}\"\n", target.target_type).to_lowercase());
            if !target.dependencies.is_empty() {
                out.push_str(&format!("dependencies = {:?}\n", target.dependencies));
            }
        }
    }

    out
}

/// Async version of parse_package_swift using tokio.
pub async fn parse_package_swift_async(path: &Path) -> Result<Manifest, ManifestError> {
    let path = path.to_path_buf();

    // Run in blocking task since it involves file I/O and process spawning
    tokio::task::spawn_blocking(move || parse_package_swift(&path))
        .await
        .map_err(|e| ManifestError::SwiftParseError(format!("Task join error: {}", e)))?
}

/// Parse multiple Package.swift files in parallel.
///
/// Returns a map of directory path to parsed manifest.
/// Failed parses are logged but don't stop other parses.
pub async fn parse_manifests_parallel(
    paths: Vec<std::path::PathBuf>,
) -> Vec<(std::path::PathBuf, Result<Manifest, ManifestError>)> {
    use futures::future::join_all;

    let tasks: Vec<_> = paths
        .into_iter()
        .map(|dir| {
            let package_swift = dir.join("Package.swift");
            async move {
                let result = if package_swift.exists() {
                    parse_package_swift_async(&package_swift).await
                } else {
                    // Try Gust.toml
                    let gust_toml = dir.join("Gust.toml");
                    if gust_toml.exists() {
                        match tokio::task::spawn_blocking(move || parse_gust_toml(&gust_toml)).await
                        {
                            Ok(r) => r,
                            Err(e) => {
                                Err(ManifestError::SwiftParseError(format!("Task error: {}", e)))
                            }
                        }
                    } else {
                        Err(ManifestError::NotFound(dir.clone()))
                    }
                };
                (dir, result)
            }
        })
        .collect();

    join_all(tasks).await
}

/// Result of parsing a transitive dependency.
#[derive(Debug)]
pub struct ParsedDependency {
    /// Package name
    pub name: String,
    /// Directory where the package is located
    pub path: std::path::PathBuf,
    /// Parsed manifest
    pub manifest: Manifest,
    /// Names of this package's dependencies
    pub dependency_names: Vec<String>,
}

/// Parse transitive dependencies in parallel.
///
/// Given a list of package directories, parses all their manifests in parallel
/// and returns the parsed results along with discovered transitive dependencies.
pub async fn parse_transitive_deps(
    package_dirs: Vec<(String, std::path::PathBuf)>,
    concurrency: usize,
) -> (Vec<ParsedDependency>, Vec<String>) {
    use futures::stream::{self, StreamExt};
    use std::sync::Arc;
    use tokio::sync::Semaphore;

    let semaphore = Arc::new(Semaphore::new(concurrency));
    let mut parsed = Vec::new();
    let mut discovered_deps = Vec::new();

    let results: Vec<_> = stream::iter(package_dirs)
        .map(|(name, dir)| {
            let sem = Arc::clone(&semaphore);
            async move {
                let _permit = sem.acquire().await.unwrap();

                let package_swift = dir.join("Package.swift");
                let start = std::time::Instant::now();

                let result = if package_swift.exists() {
                    parse_package_swift_async(&package_swift).await
                } else {
                    let gust_toml = dir.join("Gust.toml");
                    if gust_toml.exists() {
                        let toml_path = gust_toml.clone();
                        match tokio::task::spawn_blocking(move || parse_gust_toml(&toml_path)).await
                        {
                            Ok(r) => r,
                            Err(e) => {
                                Err(ManifestError::SwiftParseError(format!("Task error: {}", e)))
                            }
                        }
                    } else {
                        Err(ManifestError::NotFound(dir.clone()))
                    }
                };

                let elapsed = start.elapsed();
                tracing::debug!("Parsed {} in {:?}", name, elapsed);

                (name, dir, result)
            }
        })
        .buffer_unordered(concurrency)
        .collect()
        .await;

    for (name, dir, result) in results {
        match result {
            Ok(manifest) => {
                // Collect dependency names for transitive resolution
                let dep_names: Vec<String> = manifest.dependencies.keys().cloned().collect();

                for dep_name in &dep_names {
                    if !discovered_deps.contains(dep_name) {
                        discovered_deps.push(dep_name.clone());
                    }
                }

                parsed.push(ParsedDependency {
                    name,
                    path: dir,
                    manifest,
                    dependency_names: dep_names,
                });
            }
            Err(e) => {
                tracing::warn!("Failed to parse manifest for {}: {}", name, e);
            }
        }
    }

    (parsed, discovered_deps)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_toml() {
        let toml = r#"
[package]
name = "MyApp"
version = "1.0.0"
swift-tools-version = "5.9"

[dependencies]
swift-log = "1.5"
"#;
        let raw: RawGustToml = toml::from_str(toml).unwrap();
        assert_eq!(raw.package.name, "MyApp");
        assert!(raw.dependencies.contains_key("swift-log"));
    }

    #[test]
    fn test_parse_complex_dependency() {
        let toml = r#"
[package]
name = "Test"
version = "0.1.0"

[dependencies]
alamofire = { git = "https://github.com/Alamofire/Alamofire.git", tag = "5.8.0" }
"#;
        let raw: RawGustToml = toml::from_str(toml).unwrap();
        match &raw.dependencies["alamofire"] {
            RawDependency::Full { git, tag, .. } => {
                assert!(git.is_some());
                assert_eq!(tag.as_deref(), Some("5.8.0"));
            }
            _ => panic!("Expected full dependency"),
        }
    }

    #[test]
    fn test_parse_overrides_and_constraints() {
        let toml = r#"
[package]
name = "MyApp"
version = "1.0.0"

[dependencies]
swift-log = "^1.4"
swift-nio = { git = "https://github.com/apple/swift-nio.git", tag = "2.50.0" }

[overrides]
swift-log = "1.5.4"

[constraints]
swift-atomics = ">=1.1.0"
"#;
        let raw: RawGustToml = toml::from_str(toml).unwrap();
        assert_eq!(raw.overrides.get("swift-log"), Some(&"1.5.4".to_string()));
        assert_eq!(
            raw.constraints.get("swift-atomics"),
            Some(&">=1.1.0".to_string())
        );
    }

    #[test]
    fn test_parse_workspace() {
        let toml = r#"
[package]
name = "workspace-root"
version = "0.1.0"

[workspace]
members = ["packages/*", "apps/*"]
exclude = ["packages/deprecated-*"]

[workspace.dependencies]
swift-log = "1.5"
swift-nio = { git = "https://github.com/apple/swift-nio.git", tag = "2.60.0" }

[workspace.package]
swift-tools-version = "5.9"
license = "MIT"
"#;
        let raw: RawGustToml = toml::from_str(toml).unwrap();
        let ws = raw.workspace.expect("workspace should be present");
        assert_eq!(ws.members, vec!["packages/*", "apps/*"]);
        assert_eq!(ws.exclude, vec!["packages/deprecated-*"]);
        assert!(ws.dependencies.contains_key("swift-log"));
        assert!(ws.dependencies.contains_key("swift-nio"));

        let defaults = ws
            .package_defaults
            .expect("package defaults should be present");
        assert_eq!(defaults.swift_tools_version, Some("5.9".to_string()));
        assert_eq!(defaults.license, Some("MIT".to_string()));
    }
}
