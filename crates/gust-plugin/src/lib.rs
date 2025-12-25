//! Plugin system for Gust (SwiftPM compatible).
//!
//! This crate provides support for Swift package plugins:
//! - Build tool plugins (code generation during build)
//! - Command plugins (CLI extensions)
//! - Sandboxed execution with permissions
//! - Plugin discovery from dependencies

mod discovery;
mod executor;
mod protocol;

pub use discovery::{discover_plugins, DiscoveredPlugin, PluginKind};
pub use executor::{PluginExecutor, PluginContext, PluginResult};
pub use protocol::{PluginInput, PluginOutput, PluginMessage};

use gust_types::{PluginCapability, PluginPermission};
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PluginError {
    #[error("Plugin not found: {0}")]
    NotFound(String),

    #[error("Plugin compilation failed: {0}")]
    CompilationFailed(String),

    #[error("Plugin execution failed: {0}")]
    ExecutionFailed(String),

    #[error("Plugin protocol error: {0}")]
    ProtocolError(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),

    #[error("Sandbox error: {0}")]
    SandboxError(String),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("JSON error: {0}")]
    JsonError(#[from] serde_json::Error),

    #[error("Manifest error: {0}")]
    ManifestError(#[from] gust_manifest::ManifestError),
}

/// A loaded plugin ready for execution.
#[derive(Debug, Clone)]
pub struct Plugin {
    /// Plugin name
    pub name: String,

    /// Plugin capability (build tool or command)
    pub capability: PluginCapability,

    /// Path to the compiled plugin executable
    pub executable: PathBuf,

    /// Required permissions
    pub permissions: Vec<PluginPermission>,

    /// Source package that provides this plugin
    pub source_package: String,
}

impl Plugin {
    /// Check if this is a build tool plugin.
    pub fn is_build_tool(&self) -> bool {
        matches!(self.capability, PluginCapability::BuildTool)
    }

    /// Check if this is a command plugin.
    pub fn is_command(&self) -> bool {
        matches!(self.capability, PluginCapability::Command(_))
    }

    /// Get the command intent if this is a command plugin.
    pub fn command_intent(&self) -> Option<&gust_types::CommandPluginCapability> {
        match &self.capability {
            PluginCapability::Command(cap) => Some(cap),
            _ => None,
        }
    }
}

/// Plugin manager for discovering, compiling, and running plugins.
pub struct PluginManager {
    /// Cache directory for compiled plugins
    cache_dir: PathBuf,

    /// Discovered plugins
    plugins: Vec<Plugin>,

    /// Whether to use sandbox for execution
    use_sandbox: bool,
}

impl PluginManager {
    /// Create a new plugin manager.
    pub fn new(cache_dir: PathBuf) -> Self {
        Self {
            cache_dir,
            plugins: Vec::new(),
            use_sandbox: true,
        }
    }

    /// Disable sandboxing (for testing).
    pub fn without_sandbox(mut self) -> Self {
        self.use_sandbox = false;
        self
    }

    /// Discover plugins from a list of package directories.
    pub fn discover(&mut self, package_dirs: &[PathBuf]) -> Result<(), PluginError> {
        for dir in package_dirs {
            let discovered = discover_plugins(dir)?;
            for plugin_info in discovered {
                let plugin = self.load_plugin(&plugin_info)?;
                self.plugins.push(plugin);
            }
        }
        Ok(())
    }

    /// Load and compile a plugin.
    fn load_plugin(&self, info: &DiscoveredPlugin) -> Result<Plugin, PluginError> {
        // For now, return a placeholder. Real implementation would compile.
        Ok(Plugin {
            name: info.name.clone(),
            capability: info.capability.clone(),
            executable: self.cache_dir.join(&info.name),
            permissions: info.permissions.clone(),
            source_package: info.source_package.clone(),
        })
    }

    /// Get all discovered plugins.
    pub fn plugins(&self) -> &[Plugin] {
        &self.plugins
    }

    /// Get all command plugins.
    pub fn command_plugins(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.is_command()).collect()
    }

    /// Get all build tool plugins.
    pub fn build_tool_plugins(&self) -> Vec<&Plugin> {
        self.plugins.iter().filter(|p| p.is_build_tool()).collect()
    }

    /// Find a plugin by name.
    pub fn find(&self, name: &str) -> Option<&Plugin> {
        self.plugins.iter().find(|p| p.name == name)
    }

    /// Execute a command plugin.
    pub async fn execute_command(
        &self,
        plugin: &Plugin,
        args: &[String],
        working_dir: &Path,
        permissions: &[PluginPermission],
    ) -> Result<PluginResult, PluginError> {
        if !plugin.is_command() {
            return Err(PluginError::ExecutionFailed(
                "Not a command plugin".to_string(),
            ));
        }

        // Verify permissions
        for required in &plugin.permissions {
            if !permissions.contains(required) {
                return Err(PluginError::PermissionDenied(format!(
                    "Plugin requires permission: {:?}",
                    required
                )));
            }
        }

        let executor = PluginExecutor::new(self.use_sandbox);
        let context = PluginContext {
            working_directory: working_dir.to_path_buf(),
            arguments: args.to_vec(),
            permissions: permissions.to_vec(),
        };

        executor.execute(plugin, context).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_manager_creation() {
        let manager = PluginManager::new(PathBuf::from("/tmp/gust-plugins"));
        assert!(manager.plugins().is_empty());
    }

    #[test]
    fn test_plugin_capability_check() {
        let build_tool = Plugin {
            name: "SwiftFormat".to_string(),
            capability: PluginCapability::BuildTool,
            executable: PathBuf::from("/bin/swiftformat"),
            permissions: vec![],
            source_package: "swift-format".to_string(),
        };

        assert!(build_tool.is_build_tool());
        assert!(!build_tool.is_command());

        let command = Plugin {
            name: "DocsPreview".to_string(),
            capability: PluginCapability::Command(gust_types::CommandPluginCapability {
                intent: gust_types::CommandIntent::DocumentationGeneration,
                permissions: vec![],
            }),
            executable: PathBuf::from("/bin/docs-preview"),
            permissions: vec![],
            source_package: "swift-docc".to_string(),
        };

        assert!(command.is_command());
        assert!(!command.is_build_tool());
    }
}
