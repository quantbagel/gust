//! Platform and Swift toolchain detection for Gust.

use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::process::Command;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum PlatformError {
    #[error("Swift toolchain not found")]
    SwiftNotFound,
    #[error("Failed to execute swift: {0}")]
    ExecutionError(#[from] std::io::Error),
    #[error("Failed to parse Swift version: {0}")]
    VersionParseError(String),
}

/// Information about the current platform.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlatformInfo {
    /// Operating system (macos, linux, windows)
    pub os: String,
    /// CPU architecture (arm64, x86_64)
    pub arch: String,
    /// Platform triple (e.g., "arm64-apple-macosx")
    pub triple: String,
}

impl PlatformInfo {
    /// Detect the current platform.
    pub fn detect() -> Self {
        let os = std::env::consts::OS.to_string();
        let arch = std::env::consts::ARCH.to_string();

        let triple = match (os.as_str(), arch.as_str()) {
            ("macos", "aarch64") => "arm64-apple-macosx",
            ("macos", "x86_64") => "x86_64-apple-macosx",
            ("linux", "aarch64") => "aarch64-unknown-linux-gnu",
            ("linux", "x86_64") => "x86_64-unknown-linux-gnu",
            _ => "unknown",
        };

        Self {
            os,
            arch,
            triple: triple.to_string(),
        }
    }

    /// Get a cache-friendly identifier for this platform.
    pub fn cache_key(&self) -> String {
        format!("{}-{}", self.os, self.arch)
    }
}

/// Information about the Swift toolchain.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SwiftToolchain {
    /// Path to the swift binary
    pub swift_path: PathBuf,
    /// Swift version (e.g., "5.9.2")
    pub version: String,
    /// Major version number
    pub major_version: u32,
    /// Minor version number
    pub minor_version: u32,
}

impl SwiftToolchain {
    /// Find and detect the Swift toolchain.
    pub fn detect() -> Result<Self, PlatformError> {
        let swift_path = which::which("swift").map_err(|_| PlatformError::SwiftNotFound)?;

        let output = Command::new(&swift_path).arg("--version").output()?;

        let version_str = String::from_utf8_lossy(&output.stdout);
        let version = Self::parse_version(&version_str)?;

        let parts: Vec<u32> = version.split('.').filter_map(|s| s.parse().ok()).collect();

        Ok(Self {
            swift_path,
            version,
            major_version: parts.first().copied().unwrap_or(0),
            minor_version: parts.get(1).copied().unwrap_or(0),
        })
    }

    fn parse_version(output: &str) -> Result<String, PlatformError> {
        // Parse "Swift version 5.9.2 (swift-5.9.2-RELEASE)" or similar
        for line in output.lines() {
            if line.contains("Swift version") {
                if let Some(version_part) = line.split_whitespace().nth(2) {
                    return Ok(version_part.to_string());
                }
            }
        }
        Err(PlatformError::VersionParseError(output.to_string()))
    }

    /// Check if this toolchain meets the minimum version requirement.
    pub fn meets_requirement(&self, tools_version: &str) -> bool {
        let parts: Vec<u32> = tools_version
            .split('.')
            .filter_map(|s| s.parse().ok())
            .collect();

        let required_major = parts.first().copied().unwrap_or(0);
        let required_minor = parts.get(1).copied().unwrap_or(0);

        (self.major_version, self.minor_version) >= (required_major, required_minor)
    }

    /// Get the path to the PackageDescription library for manifest parsing.
    pub fn package_description_lib(&self) -> Option<PathBuf> {
        // On macOS with Xcode, it's typically at:
        // /Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/pm/
        if cfg!(target_os = "macos") {
            let xcode_path = PathBuf::from("/Applications/Xcode.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain/usr/lib/swift/pm");
            if xcode_path.exists() {
                return Some(xcode_path);
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_platform_detection() {
        let platform = PlatformInfo::detect();
        assert!(!platform.os.is_empty());
        assert!(!platform.arch.is_empty());
    }

    #[test]
    fn test_version_requirement() {
        let toolchain = SwiftToolchain {
            swift_path: PathBuf::from("/usr/bin/swift"),
            version: "5.9.2".to_string(),
            major_version: 5,
            minor_version: 9,
        };

        assert!(toolchain.meets_requirement("5.9"));
        assert!(toolchain.meets_requirement("5.8"));
        assert!(!toolchain.meets_requirement("5.10"));
        assert!(!toolchain.meets_requirement("6.0"));
    }
}
