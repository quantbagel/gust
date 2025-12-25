//! PubGrub-based dependency resolution for Gust.

use std::collections::HashMap;
use gust_types::{Dependency, Manifest, Version, VersionReq};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ResolveError {
    #[error("No version of {package} satisfies {requirement}")]
    NoMatchingVersion { package: String, requirement: String },

    #[error("Version conflict for {package}: {conflicts:?}")]
    VersionConflict {
        package: String,
        conflicts: Vec<String>,
    },

    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Dependency cycle detected: {0:?}")]
    CycleDetected(Vec<String>),
}

/// A resolved dependency graph.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    /// Map of package name to resolved version
    pub packages: HashMap<String, ResolvedDep>,
}

/// A single resolved dependency.
#[derive(Debug, Clone)]
pub struct ResolvedDep {
    pub name: String,
    pub version: Version,
    pub source: ResolvedSource,
    pub dependencies: Vec<String>,
}

/// The resolved source of a package.
#[derive(Debug, Clone)]
pub enum ResolvedSource {
    Registry,
    Git { url: String, revision: String },
    Path { path: std::path::PathBuf },
}

/// A package version provider for the resolver.
pub trait VersionProvider {
    /// Get all available versions for a package.
    fn available_versions(&self, package: &str) -> Result<Vec<Version>, ResolveError>;

    /// Get the dependencies of a specific package version.
    fn dependencies(
        &self,
        package: &str,
        version: &Version,
    ) -> Result<Vec<Dependency>, ResolveError>;
}

/// The dependency resolver.
pub struct Resolver<P: VersionProvider> {
    provider: P,
}

impl<P: VersionProvider> Resolver<P> {
    pub fn new(provider: P) -> Self {
        Self { provider }
    }

    /// Resolve dependencies for a manifest.
    pub fn resolve(&self, manifest: &Manifest) -> Result<Resolution, ResolveError> {
        let mut resolution = Resolution::default();
        let mut queue: Vec<(String, VersionReq)> = manifest
            .dependencies
            .iter()
            .filter_map(|(name, dep)| {
                dep.version.as_ref().map(|v| (name.clone(), v.clone()))
            })
            .collect();

        let mut visited = std::collections::HashSet::new();

        while let Some((name, requirement)) = queue.pop() {
            if visited.contains(&name) {
                continue;
            }
            visited.insert(name.clone());

            // Find the best matching version
            let versions = self.provider.available_versions(&name)?;
            let matching = versions
                .into_iter()
                .filter(|v| requirement.matches(v))
                .max();

            let version = matching.ok_or_else(|| ResolveError::NoMatchingVersion {
                package: name.clone(),
                requirement: requirement.to_string(),
            })?;

            // Get transitive dependencies
            let deps = self.provider.dependencies(&name, &version)?;
            let dep_names: Vec<String> = deps.iter().map(|d| d.name.clone()).collect();

            // Add transitive deps to queue
            for dep in deps {
                if let Some(req) = dep.version {
                    queue.push((dep.name, req));
                }
            }

            resolution.packages.insert(
                name.clone(),
                ResolvedDep {
                    name,
                    version,
                    source: ResolvedSource::Registry,
                    dependencies: dep_names,
                },
            );
        }

        Ok(resolution)
    }
}

/// A simple in-memory version provider for testing.
#[derive(Default)]
pub struct MemoryProvider {
    packages: HashMap<String, Vec<(Version, Vec<Dependency>)>>,
}

impl MemoryProvider {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add_package(&mut self, name: &str, version: Version, deps: Vec<Dependency>) {
        self.packages
            .entry(name.to_string())
            .or_default()
            .push((version, deps));
    }
}

impl VersionProvider for MemoryProvider {
    fn available_versions(&self, package: &str) -> Result<Vec<Version>, ResolveError> {
        self.packages
            .get(package)
            .map(|versions| versions.iter().map(|(v, _)| v.clone()).collect())
            .ok_or_else(|| ResolveError::PackageNotFound(package.to_string()))
    }

    fn dependencies(
        &self,
        package: &str,
        version: &Version,
    ) -> Result<Vec<Dependency>, ResolveError> {
        self.packages
            .get(package)
            .and_then(|versions| {
                versions
                    .iter()
                    .find(|(v, _)| v == version)
                    .map(|(_, deps)| deps.clone())
            })
            .ok_or_else(|| ResolveError::PackageNotFound(package.to_string()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_resolution() {
        let mut provider = MemoryProvider::new();
        provider.add_package("swift-log", Version::new(1, 5, 4), vec![]);
        provider.add_package("swift-log", Version::new(1, 4, 0), vec![]);

        let resolver = Resolver::new(provider);

        let mut manifest = Manifest::default();
        manifest.dependencies.insert(
            "swift-log".to_string(),
            Dependency::registry("swift-log", VersionReq::parse("^1.4").unwrap()),
        );

        let resolution = resolver.resolve(&manifest).unwrap();
        let resolved = resolution.packages.get("swift-log").unwrap();
        assert_eq!(resolved.version, Version::new(1, 5, 4));
    }
}
