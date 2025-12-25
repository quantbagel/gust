//! Conflict formatting and suggestion generation.

use crate::error::{ConflictDerivation, DerivationStep, ResolutionSuggestion};
use gust_types::Version;

/// Formats PubGrub derivation trees into human-readable conflict messages.
pub struct ConflictFormatter;

impl ConflictFormatter {
    /// Format a conflict message from PubGrub's incompatibility.
    pub fn format_incompatibility(
        package: &str,
        requirements: &[(String, String)], // (requirer, requirement)
        _root_name: &str,
    ) -> ConflictDerivation {
        let mut derivation = ConflictDerivation::new(format!(
            "Incompatible version requirements for {}",
            package
        ));

        // Add steps for each requirement
        for (from, requirement) in requirements {
            let step = DerivationStep::new(format!(
                "{} requires {} {}",
                from, package, requirement
            ))
            .with_package(from.clone());
            derivation.add_step(step);
        }

        // Format the derivation
        derivation.format();
        derivation
    }

    /// Generate suggestions for resolving a version conflict.
    pub fn suggest_fixes(
        package: &str,
        requirements: &[(String, String)],
        available_versions: &[Version],
    ) -> Vec<ResolutionSuggestion> {
        let mut suggestions = Vec::new();

        // Check if there's a version that satisfies all requirements
        // (This is a simplified heuristic - real implementation would be more sophisticated)
        if let Some(latest) = available_versions.iter().max() {
            suggestions.push(ResolutionSuggestion::AddOverride {
                package: package.to_string(),
                version: latest.to_string(),
            });
        }

        // Suggest upgrading if one requirement is older
        for (from, _requirement) in requirements {
            suggestions.push(ResolutionSuggestion::RemoveConstraint {
                package: package.to_string(),
                from: from.clone(),
            });
        }

        suggestions
    }

    /// Format a "no matching version" error.
    pub fn format_no_matching_version(
        package: &str,
        requirement: &str,
        available: &[Version],
    ) -> String {
        let mut output = format!(
            "No version of '{}' satisfies the requirement '{}'.\n\n",
            package, requirement
        );

        if available.is_empty() {
            output.push_str("No versions are available for this package.");
        } else {
            output.push_str("Available versions:\n");
            let mut versions: Vec<_> = available.iter().collect();
            versions.sort();
            versions.reverse();
            for (i, version) in versions.iter().take(10).enumerate() {
                output.push_str(&format!("  {}. {}\n", i + 1, version));
            }
            if versions.len() > 10 {
                output.push_str(&format!("  ... and {} more\n", versions.len() - 10));
            }
        }

        output
    }

    /// Format a dependency cycle error.
    pub fn format_cycle(cycle: &[String]) -> String {
        let mut output = String::from("Dependency cycle detected:\n\n");

        for (i, pkg) in cycle.iter().enumerate() {
            if i > 0 {
                output.push_str("  ↓\n");
            }
            output.push_str(&format!("  {}\n", pkg));
        }

        if !cycle.is_empty() {
            output.push_str("  ↓\n");
            output.push_str(&format!("  {} (cycle)\n", cycle[0]));
        }

        output
    }
}

/// Helper for building resolution error messages.
pub struct ErrorMessageBuilder {
    sections: Vec<String>,
}

impl ErrorMessageBuilder {
    pub fn new() -> Self {
        Self {
            sections: Vec::new(),
        }
    }

    pub fn add_header(mut self, header: &str) -> Self {
        self.sections.push(format!("{}\n", header));
        self
    }

    pub fn add_section(mut self, title: &str, content: &str) -> Self {
        self.sections.push(format!("{}:\n{}\n", title, content));
        self
    }

    pub fn add_list(mut self, title: &str, items: &[String]) -> Self {
        let mut section = format!("{}:\n", title);
        for item in items {
            section.push_str(&format!("  • {}\n", item));
        }
        self.sections.push(section);
        self
    }

    pub fn add_suggestions(mut self, suggestions: &[ResolutionSuggestion]) -> Self {
        if suggestions.is_empty() {
            return self;
        }

        let mut section = String::from("Suggestions:\n");
        for (i, suggestion) in suggestions.iter().enumerate() {
            section.push_str(&format!("  {}. {}\n", i + 1, suggestion));
        }
        self.sections.push(section);
        self
    }

    pub fn build(self) -> String {
        self.sections.join("\n")
    }
}

impl Default for ErrorMessageBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_incompatibility() {
        let requirements = vec![
            ("my-app".to_string(), "^1.4".to_string()),
            ("swift-nio".to_string(), ">=1.5.0".to_string()),
        ];

        let derivation =
            ConflictFormatter::format_incompatibility("swift-log", &requirements, "my-app");

        assert!(derivation.formatted.contains("swift-log"));
        assert!(derivation.formatted.contains("my-app"));
    }

    #[test]
    fn test_format_cycle() {
        let cycle = vec![
            "package-a".to_string(),
            "package-b".to_string(),
            "package-c".to_string(),
        ];

        let output = ConflictFormatter::format_cycle(&cycle);
        assert!(output.contains("package-a"));
        assert!(output.contains("cycle"));
    }

    #[test]
    fn test_error_message_builder() {
        let message = ErrorMessageBuilder::new()
            .add_header("Resolution failed")
            .add_section("Problem", "Version conflict for swift-log")
            .add_list(
                "Conflicting requirements",
                &["my-app requires ^1.4".to_string()],
            )
            .build();

        assert!(message.contains("Resolution failed"));
        assert!(message.contains("Version conflict"));
    }
}
