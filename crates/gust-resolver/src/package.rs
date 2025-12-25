//! Package types for PubGrub resolution.

use std::fmt;
use std::hash::Hash;

/// Package identifier for PubGrub resolution.
///
/// This enum represents different kinds of packages that can appear in
/// the dependency resolution graph.
#[derive(Debug, Clone, Eq, PartialEq, Hash)]
pub enum GustPackage {
    /// The virtual root package representing the project being resolved.
    /// This is the starting point for resolution.
    Root,

    /// A named package from a registry or git source.
    Named(String),
}

impl GustPackage {
    /// Create a new named package.
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }

    /// Get the package name, if this is a named package.
    pub fn name(&self) -> Option<&str> {
        match self {
            Self::Root => None,
            Self::Named(name) => Some(name),
        }
    }

    /// Returns true if this is the root package.
    pub fn is_root(&self) -> bool {
        matches!(self, Self::Root)
    }
}

impl fmt::Display for GustPackage {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Root => write!(f, "<root>"),
            Self::Named(name) => write!(f, "{}", name),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_display() {
        assert_eq!(GustPackage::Root.to_string(), "<root>");
        assert_eq!(GustPackage::named("swift-log").to_string(), "swift-log");
    }

    #[test]
    fn test_package_equality() {
        assert_eq!(GustPackage::Root, GustPackage::Root);
        assert_eq!(
            GustPackage::named("swift-log"),
            GustPackage::named("swift-log")
        );
        assert_ne!(GustPackage::Root, GustPackage::named("swift-log"));
    }
}
