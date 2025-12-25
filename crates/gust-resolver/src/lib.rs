//! PubGrub-based dependency resolution for Gust.
//!
//! This crate provides a SAT-based dependency resolver using the PubGrub algorithm.
//! It supports:
//! - Proper backtracking and conflict detection
//! - Version overrides and constraints
//! - Lockfile hints for fast re-resolution
//! - Rich error messages with derivation trees

pub mod conflict;
pub mod error;
pub mod hints;
pub mod package;
pub mod provider;

pub use error::ResolveError;
pub use hints::{ChoiceReason, LockfileHints, ResolutionTrace};
pub use package::GustPackage;
pub use provider::{
    GustDependencyProvider, GustVersion, GustVersionSet, MemoryProvider, PackageProvider,
};

use gust_types::{Dependency, Manifest, ResolutionStrategy, Version};
use pubgrub::resolve as pubgrub_resolve;
use pubgrub::{DefaultStringReporter, PubGrubError, Reporter};
use std::collections::HashMap;
use std::sync::Arc;

/// A resolved dependency graph.
#[derive(Debug, Clone, Default)]
pub struct Resolution {
    /// Map of package name to resolved dependency info
    pub packages: HashMap<String, ResolvedDep>,
    /// Resolution metadata (why each version was chosen)
    pub metadata: HashMap<String, gust_types::ResolutionMetadata>,
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
    Git {
        url: String,
        revision: String,
        tag: Option<String>,
    },
    Path {
        path: std::path::PathBuf,
    },
}

/// The main dependency resolver.
///
/// # Example
///
/// ```ignore
/// use gust_resolver::{Resolver, MemoryProvider};
///
/// let provider = MemoryProvider::new();
/// let resolver = Resolver::new(provider);
/// let resolution = resolver.resolve(&manifest)?;
/// ```
pub struct Resolver<P: PackageProvider> {
    provider: P,
    hints: LockfileHints,
    strategy: ResolutionStrategy,
}

impl<P: PackageProvider> Resolver<P> {
    /// Create a new resolver with the given provider.
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

    /// Resolve dependencies for a manifest.
    ///
    /// This uses the PubGrub algorithm for SAT-based resolution with
    /// proper backtracking and conflict detection.
    pub fn resolve(&self, manifest: &Manifest) -> Result<Resolution, ResolveError> {
        // Create the dependency provider
        let dp = GustDependencyProvider::new(&self.provider, Arc::new(manifest.clone()))
            .with_hints(self.hints.clone())
            .with_strategy(self.strategy);

        // Run PubGrub resolution
        let root = GustPackage::Root;

        match pubgrub_resolve(&dp, root, GustVersion(Version::new(0, 0, 0))) {
            Ok(solution) => {
                // Convert solution to Resolution
                let mut resolution = Resolution::default();

                #[cfg(test)]
                eprintln!(
                    "PubGrub solution: {:?}",
                    solution.keys().collect::<Vec<_>>()
                );

                for (package, version) in solution {
                    #[cfg(test)]
                    eprintln!("Processing package: {:?} version: {:?}", package, version);

                    if let GustPackage::Named(name) = package {
                        // Get dependencies for this package
                        let deps = self
                            .provider
                            .dependencies(&name, &version.0)
                            .unwrap_or_default();
                        let dep_names: Vec<String> = deps.iter().map(|d| d.name.clone()).collect();

                        // Get resolution metadata from trace
                        let metadata = dp.trace().to_metadata(&name);
                        resolution.metadata.insert(name.clone(), metadata);

                        resolution.packages.insert(
                            name.clone(),
                            ResolvedDep {
                                name,
                                version: version.0,
                                source: ResolvedSource::Registry, // TODO: Determine actual source
                                dependencies: dep_names,
                            },
                        );
                    }
                }

                Ok(resolution)
            }
            Err(PubGrubError::NoSolution(derivation_tree)) => {
                // Format the derivation tree into a user-friendly error
                let formatted = DefaultStringReporter::report(&derivation_tree);
                let derivation = error::ConflictDerivation::new(formatted);

                Err(ResolveError::NoSolution {
                    message: "No solution found for dependency constraints".to_string(),
                    derivation,
                    suggestions: vec![],
                })
            }
            Err(PubGrubError::ErrorRetrievingDependencies {
                package,
                version,
                source,
            }) => Err(ResolveError::ProviderError(format!(
                "Failed to get dependencies for {} {}: {}",
                package, version, source
            ))),
            Err(PubGrubError::ErrorChoosingVersion { package, source }) => {
                Err(ResolveError::ProviderError(format!(
                    "Failed to choose version for {}: {}",
                    package, source
                )))
            }
            Err(PubGrubError::ErrorInShouldCancel(e)) => Err(e),
        }
    }
}

// Implement PackageProvider for references to providers
impl<P: PackageProvider> PackageProvider for &P {
    fn available_versions(&self, package: &str) -> Result<Vec<Version>, ResolveError> {
        (*self).available_versions(package)
    }

    fn dependencies(
        &self,
        package: &str,
        version: &Version,
    ) -> Result<Vec<Dependency>, ResolveError> {
        (*self).dependencies(package, version)
    }
}

/// A package version provider for the resolver (legacy trait).
///
/// This trait is kept for backward compatibility with existing code.
/// New code should use `PackageProvider` instead.
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

#[cfg(test)]
mod tests {
    use super::*;
    use gust_types::VersionReq;

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
        eprintln!(
            "Resolution packages: {:?}",
            resolution.packages.keys().collect::<Vec<_>>()
        );
        let resolved = resolution
            .packages
            .get("swift-log")
            .expect("swift-log should be in resolution");
        assert_eq!(resolved.version, Version::new(1, 5, 4));
    }

    #[test]
    fn test_resolution_with_hints() {
        let mut provider = MemoryProvider::new();
        provider.add_package("swift-log", Version::new(1, 5, 4), vec![]);
        provider.add_package("swift-log", Version::new(1, 4, 0), vec![]);

        let mut hints = LockfileHints::new();
        hints.add_preferred_version("swift-log", Version::new(1, 4, 0));

        let resolver = Resolver::new(provider).with_hints(hints);

        let mut manifest = Manifest::default();
        manifest.dependencies.insert(
            "swift-log".to_string(),
            Dependency::registry("swift-log", VersionReq::parse("^1.4").unwrap()),
        );

        let resolution = resolver.resolve(&manifest).unwrap();
        let resolved = resolution.packages.get("swift-log").unwrap();
        // With hints, should prefer the locked version
        assert_eq!(resolved.version, Version::new(1, 4, 0));
    }

    #[test]
    fn test_transitive_resolution() {
        let mut provider = MemoryProvider::new();

        // swift-log has no dependencies
        provider.add_package("swift-log", Version::new(1, 5, 4), vec![]);

        // swift-nio depends on swift-log
        provider.add_package(
            "swift-nio",
            Version::new(2, 58, 0),
            vec![Dependency::registry(
                "swift-log",
                VersionReq::parse("^1.5").unwrap(),
            )],
        );

        let resolver = Resolver::new(provider);

        let mut manifest = Manifest::default();
        manifest.dependencies.insert(
            "swift-nio".to_string(),
            Dependency::registry("swift-nio", VersionReq::parse("^2.50").unwrap()),
        );

        let resolution = resolver.resolve(&manifest).unwrap();

        assert!(resolution.packages.contains_key("swift-nio"));
        assert!(resolution.packages.contains_key("swift-log"));
    }
}
