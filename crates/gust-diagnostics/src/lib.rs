//! Beautiful error formatting for Gust.
//!
//! Uses miette for rich error diagnostics with suggestions.

pub use miette::{Diagnostic, Report, Result};
use thiserror::Error;

/// A Gust error with rich diagnostics.
#[derive(Error, Diagnostic, Debug)]
pub enum GustError {
    #[error("Manifest not found")]
    #[diagnostic(
        code(gust::manifest::not_found),
        help("Create a Gust.toml or Package.swift in your project root")
    )]
    ManifestNotFound,

    #[error("Failed to parse manifest: {message}")]
    #[diagnostic(code(gust::manifest::parse_error))]
    ManifestParseError {
        message: String,
        #[source_code]
        src: Option<String>,
        #[label("error here")]
        span: Option<miette::SourceSpan>,
    },

    #[error("Package not found: {name}")]
    #[diagnostic(
        code(gust::resolve::package_not_found),
        help("Did you mean '{suggestion}'?")
    )]
    PackageNotFound { name: String, suggestion: String },

    #[error("Version conflict for {package}")]
    #[diagnostic(
        code(gust::resolve::version_conflict),
        help("{help}")
    )]
    VersionConflict {
        package: String,
        required: Vec<String>,
        help: String,
    },

    #[error("Build failed for target '{target}'")]
    #[diagnostic(code(gust::build::failed))]
    BuildFailed {
        target: String,
        #[source_code]
        src: Option<String>,
        #[label("error here")]
        span: Option<miette::SourceSpan>,
    },

    #[error("Swift toolchain not found")]
    #[diagnostic(
        code(gust::platform::no_toolchain),
        help("Install Swift from https://swift.org/download or via Xcode")
    )]
    SwiftNotFound,

    #[error("Cache error: {message}")]
    #[diagnostic(code(gust::cache::error))]
    CacheError { message: String },

    #[error("Network error: {message}")]
    #[diagnostic(
        code(gust::network::error),
        help("Check your internet connection and try again")
    )]
    NetworkError { message: String },

    #[error("{0}")]
    #[diagnostic(code(gust::generic))]
    Generic(String),
}

impl GustError {
    pub fn manifest_parse(message: impl Into<String>) -> Self {
        Self::ManifestParseError {
            message: message.into(),
            src: None,
            span: None,
        }
    }

    pub fn package_not_found(name: impl Into<String>, suggestion: impl Into<String>) -> Self {
        Self::PackageNotFound {
            name: name.into(),
            suggestion: suggestion.into(),
        }
    }

    pub fn cache(message: impl Into<String>) -> Self {
        Self::CacheError {
            message: message.into(),
        }
    }

    pub fn network(message: impl Into<String>) -> Self {
        Self::NetworkError {
            message: message.into(),
        }
    }
}

/// Setup miette for pretty error output.
pub fn setup() {
    miette::set_hook(Box::new(|_| {
        Box::new(
            miette::MietteHandlerOpts::new()
                .terminal_links(true)
                .unicode(true)
                .context_lines(2)
                .tab_width(4)
                .build(),
        )
    }))
    .ok();
}
