//! Lockfile hints for faster resolution.
//!
//! When re-resolving dependencies, we can use the existing lockfile as hints
//! to prefer previously selected versions, leading to faster resolution and
//! more stable updates.

use gust_types::Version;
use std::collections::HashMap;

/// Provides hints from an existing lockfile for faster resolution.
///
/// The resolver can use these hints to prefer previously locked versions
/// when they're still compatible with the current constraints.
#[derive(Debug, Clone, Default)]
pub struct LockfileHints {
    /// Preferred versions from the existing lockfile
    preferred_versions: HashMap<String, Version>,

    /// Git revisions from the existing lockfile
    preferred_revisions: HashMap<String, String>,
}

impl LockfileHints {
    /// Create a new empty set of hints.
    pub fn new() -> Self {
        Self::default()
    }

    /// Add a preferred version for a package.
    pub fn add_preferred_version(&mut self, package: impl Into<String>, version: Version) {
        self.preferred_versions.insert(package.into(), version);
    }

    /// Add a preferred git revision for a package.
    pub fn add_preferred_revision(
        &mut self,
        package: impl Into<String>,
        revision: impl Into<String>,
    ) {
        self.preferred_revisions
            .insert(package.into(), revision.into());
    }

    /// Get the preferred version for a package, if any.
    pub fn preferred_version(&self, package: &str) -> Option<&Version> {
        self.preferred_versions.get(package)
    }

    /// Get the preferred revision for a package, if any.
    pub fn preferred_revision(&self, package: &str) -> Option<&str> {
        self.preferred_revisions.get(package).map(String::as_str)
    }

    /// Check if a version matches the preferred version.
    pub fn matches_preferred(&self, package: &str, version: &Version) -> bool {
        self.preferred_versions
            .get(package)
            .map(|v| v == version)
            .unwrap_or(false)
    }

    /// Returns true if there are no hints.
    pub fn is_empty(&self) -> bool {
        self.preferred_versions.is_empty() && self.preferred_revisions.is_empty()
    }

    /// Get the number of hints.
    pub fn len(&self) -> usize {
        self.preferred_versions.len() + self.preferred_revisions.len()
    }

    /// Merge another set of hints into this one.
    /// Existing hints take precedence.
    pub fn merge(&mut self, other: LockfileHints) {
        for (pkg, ver) in other.preferred_versions {
            self.preferred_versions.entry(pkg).or_insert(ver);
        }
        for (pkg, rev) in other.preferred_revisions {
            self.preferred_revisions.entry(pkg).or_insert(rev);
        }
    }
}

/// The reason a version was chosen during resolution.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChoiceReason {
    /// Version was chosen because it was in the lockfile
    LockedHint,
    /// Version was chosen because it's the highest compatible
    HighestCompatible,
    /// Version was chosen because it's the lowest compatible
    LowestCompatible,
    /// Version was forced by an override
    Override,
    /// Version was the only one available
    OnlyOption,
}

impl std::fmt::Display for ChoiceReason {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::LockedHint => write!(f, "locked"),
            Self::HighestCompatible => write!(f, "highest compatible"),
            Self::LowestCompatible => write!(f, "lowest compatible"),
            Self::Override => write!(f, "override"),
            Self::OnlyOption => write!(f, "only option"),
        }
    }
}

/// Tracks why each version was selected during resolution.
#[derive(Debug, Clone, Default)]
pub struct ResolutionTrace {
    /// Map of package name to (version, reason) tuples
    choices: HashMap<String, (Version, ChoiceReason)>,
    /// Map of package name to the packages that required it
    required_by: HashMap<String, Vec<String>>,
}

impl ResolutionTrace {
    /// Create a new resolution trace.
    pub fn new() -> Self {
        Self::default()
    }

    /// Record a version choice.
    pub fn record_choice(&mut self, package: &str, version: &Version, reason: ChoiceReason) {
        self.choices
            .insert(package.to_string(), (version.clone(), reason));
    }

    /// Record that a package was required by another.
    pub fn record_requirement(&mut self, package: &str, required_by: &str) {
        self.required_by
            .entry(package.to_string())
            .or_default()
            .push(required_by.to_string());
    }

    /// Get the choice for a package.
    pub fn get_choice(&self, package: &str) -> Option<(&Version, ChoiceReason)> {
        self.choices.get(package).map(|(v, r)| (v, *r))
    }

    /// Get the packages that required a package.
    pub fn get_required_by(&self, package: &str) -> &[String] {
        self.required_by
            .get(package)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Convert to resolution metadata for the lockfile.
    pub fn to_metadata(&self, package: &str) -> gust_types::ResolutionMetadata {
        let required_by = self.required_by.get(package).cloned().unwrap_or_default();

        gust_types::ResolutionMetadata {
            required_by,
            constraints: Vec::new(), // TODO: track constraints
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lockfile_hints() {
        let mut hints = LockfileHints::new();
        hints.add_preferred_version("swift-log", Version::new(1, 5, 4));

        assert!(hints.matches_preferred("swift-log", &Version::new(1, 5, 4)));
        assert!(!hints.matches_preferred("swift-log", &Version::new(1, 4, 0)));
        assert!(!hints.matches_preferred("swift-nio", &Version::new(2, 0, 0)));
    }

    #[test]
    fn test_resolution_trace() {
        let mut trace = ResolutionTrace::new();
        trace.record_choice(
            "swift-log",
            &Version::new(1, 5, 4),
            ChoiceReason::LockedHint,
        );
        trace.record_requirement("swift-log", "my-app");
        trace.record_requirement("swift-log", "swift-nio");

        let (version, reason) = trace.get_choice("swift-log").unwrap();
        assert_eq!(*version, Version::new(1, 5, 4));
        assert_eq!(reason, ChoiceReason::LockedHint);

        let required_by = trace.get_required_by("swift-log");
        assert!(required_by.contains(&"my-app".to_string()));
        assert!(required_by.contains(&"swift-nio".to_string()));
    }
}
