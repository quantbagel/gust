//! Plugin communication protocol.
//!
//! Defines the JSON-based protocol for communicating with plugins via stdin/stdout.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Input sent to plugin via stdin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginInput {
    /// Working directory for the plugin
    #[serde(rename = "workingDirectory")]
    pub working_directory: PathBuf,

    /// Command line arguments
    pub arguments: Vec<String>,
}

/// Output received from plugin via stdout.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginOutput {
    /// Generated source files
    #[serde(rename = "generatedFiles", default)]
    pub generated_files: Vec<PathBuf>,

    /// Diagnostic messages
    #[serde(default)]
    pub diagnostics: Vec<PluginDiagnostic>,

    /// Commands for the build system
    #[serde(rename = "buildCommands", default)]
    pub build_commands: Vec<BuildCommand>,
}

/// A diagnostic message from the plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginDiagnostic {
    /// Severity level
    pub severity: DiagnosticSeverity,

    /// Diagnostic message
    pub message: String,

    /// File location (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub file: Option<PathBuf>,

    /// Line number (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub line: Option<u32>,

    /// Column number (optional)
    #[serde(skip_serializing_if = "Option::is_none")]
    pub column: Option<u32>,
}

/// Diagnostic severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
    Remark,
}

/// A build command from a build tool plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildCommand {
    /// Display name for the command
    #[serde(rename = "displayName")]
    pub display_name: String,

    /// Executable to run
    pub executable: PathBuf,

    /// Arguments to the executable
    pub arguments: Vec<String>,

    /// Environment variables
    #[serde(default)]
    pub environment: std::collections::HashMap<String, String>,

    /// Input files (for dependency tracking)
    #[serde(rename = "inputFiles", default)]
    pub input_files: Vec<PathBuf>,

    /// Output files (for dependency tracking)
    #[serde(rename = "outputFiles", default)]
    pub output_files: Vec<PathBuf>,
}

/// Generic message in the plugin protocol.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum PluginMessage {
    /// Request from host to plugin
    #[serde(rename = "request")]
    Request(PluginRequest),

    /// Response from plugin to host
    #[serde(rename = "response")]
    Response(PluginResponse),

    /// Log message from plugin
    #[serde(rename = "log")]
    Log(LogMessage),
}

/// Request from host to plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginRequest {
    /// Request ID for correlation
    pub id: u64,

    /// Request payload
    pub payload: RequestPayload,
}

/// Request payload variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum RequestPayload {
    /// Request build commands
    #[serde(rename = "getBuildCommands")]
    GetBuildCommands {
        target: String,
        sources: Vec<PathBuf>,
    },

    /// Run command plugin
    #[serde(rename = "runCommand")]
    RunCommand { arguments: Vec<String> },
}

/// Response from plugin to host.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PluginResponse {
    /// Request ID this responds to
    pub id: u64,

    /// Response payload
    pub payload: ResponsePayload,
}

/// Response payload variants.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "kind")]
pub enum ResponsePayload {
    /// Build commands response
    #[serde(rename = "buildCommands")]
    BuildCommands { commands: Vec<BuildCommand> },

    /// Command result
    #[serde(rename = "commandResult")]
    CommandResult { success: bool, message: String },

    /// Error response
    #[serde(rename = "error")]
    Error { message: String },
}

/// Log message from plugin.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LogMessage {
    /// Log level
    pub level: LogLevel,

    /// Log message
    pub message: String,
}

/// Log level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LogLevel {
    Debug,
    Info,
    Warning,
    Error,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_input_serialization() {
        let input = PluginInput {
            working_directory: PathBuf::from("/workspace"),
            arguments: vec!["--format".to_string(), "json".to_string()],
        };

        let json = serde_json::to_string(&input).unwrap();
        assert!(json.contains("workingDirectory"));
        assert!(json.contains("/workspace"));

        let parsed: PluginInput = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.working_directory, input.working_directory);
    }

    #[test]
    fn test_plugin_output_deserialization() {
        let json = r#"{
            "generatedFiles": ["Generated/File.swift"],
            "diagnostics": [
                {
                    "severity": "warning",
                    "message": "Deprecated API usage"
                }
            ],
            "buildCommands": []
        }"#;

        let output: PluginOutput = serde_json::from_str(json).unwrap();
        assert_eq!(output.generated_files.len(), 1);
        assert_eq!(output.diagnostics.len(), 1);
        assert_eq!(output.diagnostics[0].severity, DiagnosticSeverity::Warning);
    }

    #[test]
    fn test_diagnostic_severity() {
        assert_eq!(
            serde_json::to_string(&DiagnosticSeverity::Error).unwrap(),
            "\"error\""
        );
        assert_eq!(
            serde_json::to_string(&DiagnosticSeverity::Warning).unwrap(),
            "\"warning\""
        );
    }
}
