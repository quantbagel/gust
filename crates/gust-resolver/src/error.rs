//! Error types for dependency resolution.

use gust_types::Version;
use std::fmt;
use thiserror::Error;

/// Errors that can occur during dependency resolution.
#[derive(Error, Debug)]
pub enum ResolveError {
    /// No version of a package satisfies the given requirement.
    #[error("no version of {package} satisfies {requirement}")]
    NoMatchingVersion {
        package: String,
        requirement: String,
        available: Vec<Version>,
    },

    /// Version conflict between dependencies.
    #[error("version conflict for {package}")]
    VersionConflict {
        package: String,
        /// The conflicting requirements
        conflicts: Vec<ConflictingRequirement>,
        /// Derivation tree explaining the conflict
        derivation: Option<ConflictDerivation>,
    },

    /// Package not found in any source.
    #[error("package not found: {name}")]
    PackageNotFound {
        name: String,
        /// Similar package names for suggestions
        suggestions: Vec<String>,
    },

    /// Dependency cycle detected.
    #[error("dependency cycle detected: {}", format_cycle(.cycle))]
    CycleDetected { cycle: Vec<String> },

    /// Resolution was cancelled (e.g., timeout or user interrupt).
    #[error("resolution cancelled")]
    Cancelled,

    /// No solution exists for the given constraints.
    #[error("no solution found: {message}")]
    NoSolution {
        message: String,
        derivation: ConflictDerivation,
        suggestions: Vec<ResolutionSuggestion>,
    },

    /// Provider error (e.g., network failure, invalid manifest).
    #[error("failed to fetch package info: {0}")]
    ProviderError(String),
}

fn format_cycle(cycle: &[String]) -> String {
    cycle.join(" -> ")
}

/// A conflicting requirement in the dependency graph.
#[derive(Debug, Clone)]
pub struct ConflictingRequirement {
    /// The package that imposed this requirement
    pub from: String,
    /// The version requirement string
    pub requirement: String,
    /// Path from root to this requirement
    pub dependency_chain: Vec<String>,
}

impl fmt::Display for ConflictingRequirement {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{} requires {}", self.from, self.requirement)?;
        if !self.dependency_chain.is_empty() {
            write!(f, " (via {})", self.dependency_chain.join(" -> "))?;
        }
        Ok(())
    }
}

/// Derivation tree explaining how a conflict arose.
#[derive(Debug, Clone, Default)]
pub struct ConflictDerivation {
    /// Root cause description
    pub root_cause: String,
    /// Steps showing how the conflict was derived
    pub steps: Vec<DerivationStep>,
    /// Formatted tree for display
    pub formatted: String,
}

impl ConflictDerivation {
    /// Create a new conflict derivation.
    pub fn new(root_cause: impl Into<String>) -> Self {
        Self {
            root_cause: root_cause.into(),
            steps: Vec::new(),
            formatted: String::new(),
        }
    }

    /// Add a derivation step.
    pub fn add_step(&mut self, step: DerivationStep) {
        self.steps.push(step);
    }

    /// Format the derivation for display.
    pub fn format(&mut self) {
        let mut output = String::new();
        output.push_str(&format!("Root cause: {}\n", self.root_cause));
        output.push('\n');

        for (i, step) in self.steps.iter().enumerate() {
            output.push_str(&format!("{}. {}\n", i + 1, step.description));
            for pkg in &step.packages_involved {
                output.push_str(&format!("   - {}\n", pkg));
            }
        }

        self.formatted = output;
    }
}

impl fmt::Display for ConflictDerivation {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.formatted.is_empty() {
            write!(f, "{}", self.root_cause)
        } else {
            write!(f, "{}", self.formatted)
        }
    }
}

/// A single step in the derivation of a conflict.
#[derive(Debug, Clone)]
pub struct DerivationStep {
    /// Description of this step
    pub description: String,
    /// Packages involved in this step
    pub packages_involved: Vec<String>,
}

impl DerivationStep {
    /// Create a new derivation step.
    pub fn new(description: impl Into<String>) -> Self {
        Self {
            description: description.into(),
            packages_involved: Vec::new(),
        }
    }

    /// Add a package to this step.
    pub fn with_package(mut self, package: impl Into<String>) -> Self {
        self.packages_involved.push(package.into());
        self
    }
}

/// Suggested fixes for resolution failures.
#[derive(Debug, Clone)]
pub enum ResolutionSuggestion {
    /// Upgrade a dependency to resolve conflict.
    Upgrade {
        package: String,
        from: Version,
        to: Version,
    },

    /// Downgrade a dependency to resolve conflict.
    Downgrade {
        package: String,
        from: Version,
        to: Version,
    },

    /// Add an override to force a version.
    AddOverride { package: String, version: String },

    /// Remove a conflicting constraint.
    RemoveConstraint { package: String, from: String },

    /// Use a different branch/tag.
    ChangeBranch {
        package: String,
        current: String,
        suggested: String,
    },
}

impl fmt::Display for ResolutionSuggestion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Upgrade { package, from, to } => {
                write!(f, "Upgrade {} from {} to {}", package, from, to)
            }
            Self::Downgrade { package, from, to } => {
                write!(f, "Downgrade {} from {} to {}", package, from, to)
            }
            Self::AddOverride { package, version } => {
                write!(
                    f,
                    "Add override: [overrides]\n{} = \"{}\"",
                    package, version
                )
            }
            Self::RemoveConstraint { package, from } => {
                write!(f, "Remove {} constraint from {}", package, from)
            }
            Self::ChangeBranch {
                package,
                current,
                suggested,
            } => {
                write!(
                    f,
                    "Change {} branch from '{}' to '{}'",
                    package, current, suggested
                )
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflicting_requirement_display() {
        let req = ConflictingRequirement {
            from: "swift-nio".to_string(),
            requirement: ">=1.5.0".to_string(),
            dependency_chain: vec!["my-app".to_string()],
        };
        assert!(req.to_string().contains("swift-nio requires >=1.5.0"));
    }

    #[test]
    fn test_derivation_formatting() {
        let mut derivation = ConflictDerivation::new("Incompatible version requirements");
        derivation.add_step(
            DerivationStep::new("swift-nio requires swift-log >=1.5").with_package("swift-log"),
        );
        derivation.format();
        assert!(derivation.formatted.contains("swift-nio requires"));
    }
}
