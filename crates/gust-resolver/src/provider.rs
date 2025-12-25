//! PubGrub DependencyProvider implementation for Gust.

use crate::error::ResolveError;
use crate::hints::{ChoiceReason, LockfileHints, ResolutionTrace};
use crate::package::GustPackage;
use gust_types::{Dependency, Manifest, ResolutionStrategy, Version, VersionReq};
use pubgrub::{Dependencies, DependencyProvider, Map, PackageResolutionStatistics, VersionSet};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fmt::{self, Display};
use std::sync::Arc;

/// A provider that supplies package information to PubGrub.
///
/// This trait is implemented by different backends (registry, git, memory)
/// to provide version and dependency information.
pub trait PackageProvider: Send + Sync {
    /// Get all available versions for a package.
    fn available_versions(&self, package: &str) -> Result<Vec<Version>, ResolveError>;

    /// Get the dependencies of a specific package version.
    fn dependencies(
        &self,
        package: &str,
        version: &Version,
    ) -> Result<Vec<Dependency>, ResolveError>;
}

/// Wrapper around semver::Version that implements pubgrub traits.
#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct GustVersion(pub Version);

impl From<Version> for GustVersion {
    fn from(v: Version) -> Self {
        Self(v)
    }
}

impl Display for GustVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Version set for Gust packages using semver requirements.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GustVersionSet {
    /// The version requirement
    req: Option<VersionReq>,
    /// Explicit versions that are included (for overrides)
    included: Vec<Version>,
    /// Negated version sets
    negated: bool,
}

impl GustVersionSet {
    /// Create a set that matches any version.
    pub fn any() -> Self {
        Self {
            req: None,
            included: vec![],
            negated: false,
        }
    }

    /// Create a set from a version requirement.
    pub fn from_req(req: VersionReq) -> Self {
        Self {
            req: Some(req),
            included: vec![],
            negated: false,
        }
    }

    /// Create a set that matches exactly one version.
    pub fn exact(version: Version) -> Self {
        Self {
            req: Some(VersionReq::parse(&format!("={}", version)).unwrap()),
            included: vec![version],
            negated: false,
        }
    }

    /// Create an empty set.
    pub fn empty() -> Self {
        Self {
            req: None,
            included: vec![],
            negated: true,
        }
    }
}

impl VersionSet for GustVersionSet {
    type V = GustVersion;

    fn empty() -> Self {
        GustVersionSet::empty()
    }

    fn singleton(v: Self::V) -> Self {
        GustVersionSet::exact(v.0)
    }

    fn complement(&self) -> Self {
        Self {
            req: self.req.clone(),
            included: self.included.clone(),
            negated: !self.negated,
        }
    }

    fn intersection(&self, other: &Self) -> Self {
        // Handle empty set cases
        if self.negated && self.req.is_none() && self.included.is_empty() {
            return self.clone(); // self is empty
        }
        if other.negated && other.req.is_none() && other.included.is_empty() {
            return other.clone(); // other is empty
        }

        // Handle full set cases
        if !self.negated && self.req.is_none() {
            return other.clone(); // self is full
        }
        if !other.negated && other.req.is_none() {
            return self.clone(); // other is full
        }

        // Both positive with requirements - intersect
        if !self.negated && !other.negated {
            // For simplicity, keep self's requirement (proper impl would combine)
            self.clone()
        } else if self.negated {
            other.clone()
        } else {
            self.clone()
        }
    }

    fn contains(&self, v: &Self::V) -> bool {
        let matches = match &self.req {
            Some(req) => req.matches(&v.0),
            None => true,
        };
        if self.negated {
            !matches
        } else {
            matches
        }
    }

    fn full() -> Self {
        GustVersionSet::any()
    }

    fn union(&self, other: &Self) -> Self {
        // Handle full set cases
        if !self.negated && self.req.is_none() {
            return self.clone(); // self is full
        }
        if !other.negated && other.req.is_none() {
            return other.clone(); // other is full
        }

        // Handle empty set cases
        if self.negated && self.req.is_none() && self.included.is_empty() {
            return other.clone(); // self is empty
        }
        if other.negated && other.req.is_none() && other.included.is_empty() {
            return self.clone(); // other is empty
        }

        // For other cases, return full (conservative)
        GustVersionSet::any()
    }

    fn is_disjoint(&self, other: &Self) -> bool {
        // Empty set is disjoint with everything
        if (self.negated && self.req.is_none() && self.included.is_empty())
            || (other.negated && other.req.is_none() && other.included.is_empty())
        {
            return true;
        }
        false // Conservative
    }

    fn subset_of(&self, other: &Self) -> bool {
        // Empty set is subset of everything
        if self.negated && self.req.is_none() && self.included.is_empty() {
            return true;
        }
        // Everything is subset of full set
        if !other.negated && other.req.is_none() {
            return true;
        }
        // A set is subset of itself
        if self == other {
            return true;
        }
        false // Conservative
    }
}

impl Display for GustVersionSet {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.negated {
            write!(f, "not ")?;
        }
        match &self.req {
            Some(req) => write!(f, "{}", req),
            None => write!(f, "*"),
        }
    }
}

/// Gust's DependencyProvider implementation for PubGrub.
pub struct GustDependencyProvider<'a, P: PackageProvider> {
    /// The underlying package provider
    provider: &'a P,

    /// The root manifest being resolved
    manifest: Arc<Manifest>,

    /// Version overrides (force specific versions)
    overrides: HashMap<String, VersionReq>,

    /// Additional constraints
    constraints: HashMap<String, VersionReq>,

    /// Lockfile hints for preferring locked versions
    hints: LockfileHints,

    /// Resolution strategy
    strategy: ResolutionStrategy,

    /// Track why each version was selected
    trace: RefCell<ResolutionTrace>,

    /// Cache of available versions
    version_cache: RefCell<HashMap<String, Vec<Version>>>,
}

impl<'a, P: PackageProvider> GustDependencyProvider<'a, P> {
    /// Create a new dependency provider.
    pub fn new(provider: &'a P, manifest: Arc<Manifest>) -> Self {
        // Extract overrides from manifest
        let overrides: HashMap<String, VersionReq> = manifest
            .overrides
            .iter()
            .filter_map(|(name, version)| {
                VersionReq::parse(version).ok().map(|v| (name.clone(), v))
            })
            .collect();

        // Extract constraints from manifest
        let constraints: HashMap<String, VersionReq> = manifest
            .constraints
            .iter()
            .filter_map(|(name, version)| {
                VersionReq::parse(version).ok().map(|v| (name.clone(), v))
            })
            .collect();

        Self {
            provider,
            manifest,
            overrides,
            constraints,
            hints: LockfileHints::new(),
            strategy: ResolutionStrategy::Highest,
            trace: RefCell::new(ResolutionTrace::new()),
            version_cache: RefCell::new(HashMap::new()),
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

    /// Get the resolution trace.
    pub fn trace(&self) -> std::cell::Ref<'_, ResolutionTrace> {
        self.trace.borrow()
    }

    /// Get available versions for a package (cached).
    fn get_versions(&self, package: &str) -> Result<Vec<Version>, ResolveError> {
        {
            let cache = self.version_cache.borrow();
            if let Some(versions) = cache.get(package) {
                return Ok(versions.clone());
            }
        }

        let versions = self.provider.available_versions(package)?;
        self.version_cache
            .borrow_mut()
            .insert(package.to_string(), versions.clone());
        Ok(versions)
    }
}

impl<'a, P: PackageProvider> DependencyProvider for GustDependencyProvider<'a, P> {
    type P = GustPackage;
    type V = GustVersion;
    type VS = GustVersionSet;
    type M = String;
    type Err = ResolveError;
    type Priority = u32;

    fn prioritize(
        &self,
        package: &Self::P,
        _range: &Self::VS,
        _stats: &PackageResolutionStatistics,
    ) -> Self::Priority {
        // Lower priority = resolve first
        // Prioritize packages with fewer versions (finds conflicts faster)
        match package {
            GustPackage::Root => 0, // Always resolve root first
            GustPackage::Named(name) => {
                if self.overrides.contains_key(name) {
                    1 // Overrides second
                } else {
                    match self.get_versions(name) {
                        Ok(versions) => (100 + versions.len()) as u32,
                        Err(_) => 1000,
                    }
                }
            }
        }
    }

    fn choose_version(
        &self,
        package: &Self::P,
        range: &Self::VS,
    ) -> Result<Option<Self::V>, Self::Err> {
        #[cfg(test)]
        eprintln!(
            "choose_version called for {:?} with range {:?}",
            package, range
        );

        match package {
            GustPackage::Root => {
                #[cfg(test)]
                eprintln!("  Returning Root version 0.0.0");
                Ok(Some(GustVersion(Version::new(0, 0, 0))))
            }
            GustPackage::Named(name) => {
                // Check for override
                if let Some(override_req) = self.overrides.get(name) {
                    let versions = self.get_versions(name)?;
                    let matching: Vec<_> = versions
                        .into_iter()
                        .filter(|v| override_req.matches(v))
                        .collect();

                    if let Some(version) = matching.into_iter().max() {
                        self.trace.borrow_mut().record_choice(
                            name,
                            &version,
                            ChoiceReason::Override,
                        );
                        return Ok(Some(GustVersion(version)));
                    }
                }

                // Get versions and filter by range
                let versions = self.get_versions(name)?;
                let matching: Vec<_> = versions
                    .into_iter()
                    .filter(|v| range.contains(&GustVersion(v.clone())))
                    .collect();

                // Check lockfile hints first
                if let Some(locked) = self.hints.preferred_version(name) {
                    if matching.iter().any(|v| v == locked) {
                        self.trace.borrow_mut().record_choice(
                            name,
                            locked,
                            ChoiceReason::LockedHint,
                        );
                        return Ok(Some(GustVersion(locked.clone())));
                    }
                }

                // Apply strategy
                let chosen = match self.strategy {
                    ResolutionStrategy::Highest => matching.into_iter().max(),
                    ResolutionStrategy::Lowest => matching.into_iter().min(),
                    ResolutionStrategy::Locked => matching.into_iter().max(),
                };

                if let Some(ref version) = chosen {
                    let reason = match self.strategy {
                        ResolutionStrategy::Highest => ChoiceReason::HighestCompatible,
                        ResolutionStrategy::Lowest => ChoiceReason::LowestCompatible,
                        ResolutionStrategy::Locked => ChoiceReason::HighestCompatible,
                    };
                    self.trace.borrow_mut().record_choice(name, version, reason);
                }

                Ok(chosen.map(GustVersion))
            }
        }
    }

    fn get_dependencies(
        &self,
        package: &Self::P,
        version: &Self::V,
    ) -> Result<Dependencies<Self::P, Self::VS, Self::M>, Self::Err> {
        match package {
            GustPackage::Root => {
                // Root package dependencies come from the manifest
                let mut deps = Map::default();

                #[cfg(test)]
                eprintln!(
                    "get_dependencies for Root, manifest has {} deps",
                    self.manifest.dependencies.len()
                );

                for (name, dep) in &self.manifest.dependencies {
                    let pkg = GustPackage::named(name);
                    let range = if let Some(version) = &dep.version {
                        #[cfg(test)]
                        eprintln!("  Adding dep {} with version {:?}", name, version);
                        GustVersionSet::from_req(version.clone())
                    } else {
                        #[cfg(test)]
                        eprintln!("  Adding dep {} with any version", name);
                        GustVersionSet::any()
                    };
                    deps.insert(pkg, range);
                }

                #[cfg(test)]
                eprintln!("Returning {} dependencies for Root", deps.len());

                Ok(Dependencies::Available(deps))
            }
            GustPackage::Named(name) => {
                // Get dependencies from the provider
                let deps = self.provider.dependencies(name, &version.0)?;

                // Record requirements for trace
                for dep in &deps {
                    self.trace.borrow_mut().record_requirement(&dep.name, name);
                }

                // Convert to PubGrub format
                let mut pubgrub_deps = Map::default();

                for dep in deps {
                    let pkg = GustPackage::named(&dep.name);
                    let mut range = if let Some(version_req) = &dep.version {
                        GustVersionSet::from_req(version_req.clone())
                    } else {
                        GustVersionSet::any()
                    };

                    // Apply additional constraints
                    if let Some(constraint) = self.constraints.get(&dep.name) {
                        let constraint_set = GustVersionSet::from_req(constraint.clone());
                        range = range.intersection(&constraint_set);
                    }

                    pubgrub_deps.insert(pkg, range);
                }

                Ok(Dependencies::Available(pubgrub_deps))
            }
        }
    }
}

/// A simple in-memory provider for testing.
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

impl PackageProvider for MemoryProvider {
    fn available_versions(&self, package: &str) -> Result<Vec<Version>, ResolveError> {
        self.packages
            .get(package)
            .map(|versions| versions.iter().map(|(v, _)| v.clone()).collect())
            .ok_or_else(|| ResolveError::PackageNotFound {
                name: package.to_string(),
                suggestions: vec![],
            })
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
            .ok_or_else(|| ResolveError::PackageNotFound {
                name: package.to_string(),
                suggestions: vec![],
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_provider() {
        let mut provider = MemoryProvider::new();
        provider.add_package("swift-log", Version::new(1, 5, 4), vec![]);
        provider.add_package("swift-log", Version::new(1, 4, 0), vec![]);

        let versions = provider.available_versions("swift-log").unwrap();
        assert_eq!(versions.len(), 2);
    }

    #[test]
    fn test_version_set() {
        let set = GustVersionSet::from_req(VersionReq::parse("^1.4").unwrap());
        assert!(set.contains(&GustVersion(Version::new(1, 5, 0))));
        assert!(set.contains(&GustVersion(Version::new(1, 4, 0))));
        assert!(!set.contains(&GustVersion(Version::new(2, 0, 0))));
    }
}
