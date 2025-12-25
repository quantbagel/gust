//! Swift Package Registry API client.
//!
//! Implements the Swift Package Registry Service API (SE-0292, SE-0321).
//! https://github.com/apple/swift-package-manager/blob/main/Documentation/PackageRegistry/Registry.md

use reqwest::Client;
use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum RegistryError {
    #[error("Package not found: {0}")]
    NotFound(String),
    #[error("Version not found: {0}@{1}")]
    VersionNotFound(String, String),
    #[error("Network error: {0}")]
    Network(#[from] reqwest::Error),
    #[error("Invalid response: {0}")]
    InvalidResponse(String),
}

/// A Swift Package Registry client.
pub struct RegistryClient {
    base_url: String,
    client: Client,
}

impl RegistryClient {
    /// Create a client for the default registry.
    pub fn new() -> Self {
        Self::with_url("https://packages.swift.org")
    }

    /// Create a client for a custom registry.
    pub fn with_url(url: impl Into<String>) -> Self {
        Self {
            base_url: url.into(),
            client: Client::new(),
        }
    }

    /// List all versions of a package.
    pub async fn list_versions(
        &self,
        scope: &str,
        name: &str,
    ) -> Result<PackageVersions, RegistryError> {
        let url = format!("{}/{}/{}", self.base_url, scope, name);

        let resp = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.swift.registry.v1+json")
            .send()
            .await?;

        if resp.status() == 404 {
            return Err(RegistryError::NotFound(format!("{}/{}", scope, name)));
        }

        let body: PackageVersions = resp.json().await?;
        Ok(body)
    }

    /// Get metadata for a specific version.
    pub async fn get_version(
        &self,
        scope: &str,
        name: &str,
        version: &str,
    ) -> Result<PackageRelease, RegistryError> {
        let url = format!("{}/{}/{}/{}", self.base_url, scope, name, version);

        let resp = self
            .client
            .get(&url)
            .header("Accept", "application/vnd.swift.registry.v1+json")
            .send()
            .await?;

        if resp.status() == 404 {
            return Err(RegistryError::VersionNotFound(
                format!("{}/{}", scope, name),
                version.to_string(),
            ));
        }

        let body: PackageRelease = resp.json().await?;
        Ok(body)
    }

    /// Get the manifest for a specific version.
    pub async fn get_manifest(
        &self,
        scope: &str,
        name: &str,
        version: &str,
    ) -> Result<String, RegistryError> {
        let url = format!(
            "{}/{}/{}/{}/Package.swift",
            self.base_url, scope, name, version
        );

        let resp = self
            .client
            .get(&url)
            .header("Accept", "text/x-swift")
            .send()
            .await?;

        if resp.status() == 404 {
            return Err(RegistryError::VersionNotFound(
                format!("{}/{}", scope, name),
                version.to_string(),
            ));
        }

        let body = resp.text().await?;
        Ok(body)
    }

    /// Download the source archive for a version.
    pub async fn download_source(
        &self,
        scope: &str,
        name: &str,
        version: &str,
    ) -> Result<Vec<u8>, RegistryError> {
        let url = format!("{}/{}/{}/{}.zip", self.base_url, scope, name, version);

        let resp = self
            .client
            .get(&url)
            .header("Accept", "application/zip")
            .send()
            .await?;

        if resp.status() == 404 {
            return Err(RegistryError::VersionNotFound(
                format!("{}/{}", scope, name),
                version.to_string(),
            ));
        }

        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    /// Lookup package identifiers from a repository URL.
    pub async fn lookup_by_url(&self, url: &str) -> Result<Vec<PackageIdentifier>, RegistryError> {
        let lookup_url = format!(
            "{}/identifiers?url={}",
            self.base_url,
            urlencoding::encode(url)
        );

        let resp = self
            .client
            .get(&lookup_url)
            .header("Accept", "application/vnd.swift.registry.v1+json")
            .send()
            .await?;

        if resp.status() == 404 {
            return Ok(vec![]);
        }

        let body: IdentifiersResponse = resp.json().await?;
        Ok(body.identifiers)
    }
}

impl Default for RegistryClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Package version listing response.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageVersions {
    pub releases: std::collections::HashMap<String, ReleaseInfo>,
}

/// Information about a single release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseInfo {
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub problem: Option<ReleaseProblem>,
}

/// Problem with a release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseProblem {
    pub status: String,
    pub title: String,
    #[serde(default)]
    pub detail: Option<String>,
}

/// Detailed release metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageRelease {
    pub id: String,
    pub version: String,
    pub resources: Vec<ReleaseResource>,
    #[serde(default)]
    pub metadata: Option<ReleaseMetadata>,
}

/// A resource in a release.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseResource {
    pub name: String,
    #[serde(rename = "type")]
    pub resource_type: String,
    #[serde(default)]
    pub checksum: Option<String>,
}

/// Release metadata.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReleaseMetadata {
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub license: Option<LicenseInfo>,
    #[serde(default)]
    pub author: Option<AuthorInfo>,
    #[serde(rename = "repositoryURLs", default)]
    pub repository_urls: Vec<String>,
}

/// License information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LicenseInfo {
    pub name: String,
    #[serde(default)]
    pub url: Option<String>,
}

/// Author information.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorInfo {
    pub name: String,
    #[serde(default)]
    pub email: Option<String>,
    #[serde(default)]
    pub url: Option<String>,
}

/// Package identifier.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PackageIdentifier {
    pub scope: String,
    pub name: String,
}

impl std::fmt::Display for PackageIdentifier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.scope, self.name)
    }
}

/// Response from identifiers lookup.
#[derive(Debug, Clone, Serialize, Deserialize)]
struct IdentifiersResponse {
    identifiers: Vec<PackageIdentifier>,
}

// URL encoding helper
mod urlencoding {
    pub fn encode(s: &str) -> String {
        let mut result = String::new();
        for c in s.chars() {
            match c {
                'a'..='z' | 'A'..='Z' | '0'..='9' | '-' | '_' | '.' | '~' => result.push(c),
                _ => {
                    for b in c.to_string().bytes() {
                        result.push_str(&format!("%{:02X}", b));
                    }
                }
            }
        }
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_package_identifier_display() {
        let id = PackageIdentifier {
            scope: "apple".to_string(),
            name: "swift-argument-parser".to_string(),
        };
        assert_eq!(id.to_string(), "apple.swift-argument-parser");
    }

    #[test]
    fn test_url_encoding() {
        assert_eq!(
            urlencoding::encode("https://github.com/apple/swift-log.git"),
            "https%3A%2F%2Fgithub.com%2Fapple%2Fswift-log.git"
        );
    }
}
