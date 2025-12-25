//! Plugin discovery from package dependencies.
//!
//! Scans packages for plugin targets and extracts capability information.

use crate::PluginError;
use gust_manifest::find_manifest;
use gust_types::{PluginCapability, PluginPermission, TargetType};
use std::path::{Path, PathBuf};

/// Kind of plugin discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PluginKind {
    /// Build tool plugin (runs during build)
    BuildTool,
    /// Command plugin (CLI extension)
    Command,
}

/// Information about a discovered plugin.
#[derive(Debug, Clone)]
pub struct DiscoveredPlugin {
    /// Plugin name (target name)
    pub name: String,

    /// Kind of plugin
    pub kind: PluginKind,

    /// Plugin capability details
    pub capability: PluginCapability,

    /// Required permissions
    pub permissions: Vec<PluginPermission>,

    /// Path to plugin source
    pub source_path: PathBuf,

    /// Package that provides this plugin
    pub source_package: String,
}

/// Discover plugins in a package directory.
///
/// Looks for targets with type "plugin" in the manifest and
/// extracts their capabilities.
pub fn discover_plugins(package_dir: &Path) -> Result<Vec<DiscoveredPlugin>, PluginError> {
    let (manifest, _) = find_manifest(package_dir)?;
    let mut plugins = Vec::new();

    for target in &manifest.targets {
        if target.target_type == TargetType::Plugin {
            // Determine plugin kind from target configuration
            // For now, assume all plugin targets are build tools
            // Real implementation would parse Package.swift for @main attribute
            let capability = PluginCapability::BuildTool;
            let kind = PluginKind::BuildTool;

            let source_path = target
                .path
                .clone()
                .unwrap_or_else(|| package_dir.join("Plugins").join(&target.name));

            plugins.push(DiscoveredPlugin {
                name: target.name.clone(),
                kind,
                capability,
                permissions: Vec::new(),
                source_path,
                source_package: manifest.package.name.clone(),
            });
        }
    }

    Ok(plugins)
}

/// Discover all plugins from multiple package directories.
#[allow(dead_code)]
pub fn discover_all_plugins(
    package_dirs: &[PathBuf],
) -> Result<Vec<DiscoveredPlugin>, PluginError> {
    let mut all_plugins = Vec::new();

    for dir in package_dirs {
        match discover_plugins(dir) {
            Ok(plugins) => all_plugins.extend(plugins),
            Err(PluginError::ManifestError(_)) => {
                // Skip packages without manifests
                continue;
            }
            Err(e) => return Err(e),
        }
    }

    Ok(all_plugins)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_kind_equality() {
        assert_eq!(PluginKind::BuildTool, PluginKind::BuildTool);
        assert_ne!(PluginKind::BuildTool, PluginKind::Command);
    }
}
