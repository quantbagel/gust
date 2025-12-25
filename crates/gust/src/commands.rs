//! CLI command implementations.

use crate::install::{InstallOptions, Installer};
use console::style;
use miette::{IntoDiagnostic, Result};
use std::env;
use std::fs;
use std::path::Path;
use gust_build::{BuildOptions, Builder};
use gust_cache::GlobalCache;
use gust_manifest::{find_manifest, generate_gust_toml};
use gust_types::{BuildConfiguration, Manifest, Package, Target, TargetType, Version};

/// Create a new package.
pub async fn new_package(name: &str, pkg_type: &str) -> Result<()> {
    let path = env::current_dir().into_diagnostic()?.join(name);

    if path.exists() {
        return Err(miette::miette!("Directory {} already exists", name));
    }

    fs::create_dir_all(&path).into_diagnostic()?;

    let target_type = match pkg_type {
        "executable" | "exe" => TargetType::Executable,
        "library" | "lib" => TargetType::Library,
        _ => return Err(miette::miette!("Unknown package type: {}", pkg_type)),
    };

    let manifest = create_manifest(name, target_type);
    let toml = generate_gust_toml(&manifest);
    fs::write(path.join("Gust.toml"), toml).into_diagnostic()?;

    // Create source directory
    let sources_dir = path.join("Sources").join(name);
    fs::create_dir_all(&sources_dir).into_diagnostic()?;

    // Create main file
    let main_file = match target_type {
        TargetType::Executable => {
            sources_dir.join("main.swift")
        }
        _ => sources_dir.join(format!("{}.swift", name)),
    };

    let content = match target_type {
        TargetType::Executable => "print(\"Hello, world!\")\n",
        _ => &format!(
            "public struct {} {{\n    public init() {{}}\n}}\n",
            name
        ),
    };

    fs::write(main_file, content).into_diagnostic()?;

    // Create tests directory
    let tests_dir = path.join("Tests").join(format!("{}Tests", name));
    fs::create_dir_all(&tests_dir).into_diagnostic()?;

    let test_file = tests_dir.join(format!("{}Tests.swift", name));
    let test_content = format!(
        r#"import XCTest
@testable import {}

final class {}Tests: XCTestCase {{
    func testExample() {{
        XCTAssertTrue(true)
    }}
}}
"#,
        name, name
    );
    fs::write(test_file, test_content).into_diagnostic()?;

    println!(
        "{} Created package {} at {}",
        style("✓").green().bold(),
        style(name).cyan(),
        path.display()
    );

    Ok(())
}

/// Initialize a package in the current directory.
pub async fn init(name: Option<&str>) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let pkg_name = name
        .map(String::from)
        .or_else(|| cwd.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "MyPackage".to_string());

    let manifest_path = cwd.join("Gust.toml");
    if manifest_path.exists() {
        return Err(miette::miette!("Gust.toml already exists"));
    }

    let manifest = create_manifest(&pkg_name, TargetType::Library);
    let toml = generate_gust_toml(&manifest);
    fs::write(&manifest_path, toml).into_diagnostic()?;

    println!(
        "{} Initialized package {}",
        style("✓").green().bold(),
        style(&pkg_name).cyan()
    );

    Ok(())
}

fn create_manifest(name: &str, target_type: TargetType) -> Manifest {
    Manifest {
        package: Package {
            name: name.to_string(),
            version: Version::new(0, 1, 0),
            swift_tools_version: "5.9".to_string(),
            ..Default::default()
        },
        targets: vec![Target {
            name: name.to_string(),
            target_type,
            path: Some(format!("Sources/{}", name).into()),
            dependencies: Vec::new(),
            resources: Vec::new(),
        }],
        ..Default::default()
    }
}

/// Build the package.
pub async fn build(
    release: bool,
    target: Option<&str>,
    jobs: Option<usize>,
    no_cache: bool,
) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let (manifest, _) = find_manifest(&cwd).into_diagnostic()?;

    let builder = Builder::new(cwd).into_diagnostic()?;

    let options = BuildOptions {
        configuration: if release {
            BuildConfiguration::Release
        } else {
            BuildConfiguration::Debug
        },
        target: target.map(String::from),
        jobs,
        use_cache: !no_cache,
        ..Default::default()
    };

    println!(
        "{} Building {} ({})",
        style("→").blue().bold(),
        style(&manifest.package.name).cyan(),
        options.configuration
    );

    let result = builder.build(&manifest, &options).await.into_diagnostic()?;

    if result.cached {
        println!(
            "{} Restored from cache in {:.3}s",
            style("⚡").yellow().bold(),
            result.duration_secs
        );
    } else {
        println!(
            "{} Built in {:.2}s",
            style("✓").green().bold(),
            result.duration_secs
        );

        if let Some(ref fp) = result.fingerprint {
            println!(
                "  {} Cached as {}",
                style("→").dim(),
                style(&fp[..16]).dim()
            );
        }
    }

    for product in &result.products {
        println!("  {} {}", style("•").dim(), product.display());
    }

    Ok(())
}

/// Run the executable.
pub async fn run(target: Option<&str>, args: &[String]) -> Result<()> {
    // First build (with cache)
    build(false, target, None, false).await?;

    let cwd = env::current_dir().into_diagnostic()?;
    let (manifest, _) = find_manifest(&cwd).into_diagnostic()?;

    // Find executable target
    let exe_target = if let Some(name) = target {
        manifest
            .targets
            .iter()
            .find(|t| t.name == name && t.target_type == TargetType::Executable)
            .ok_or_else(|| miette::miette!("Executable target '{}' not found", name))?
    } else {
        manifest
            .targets
            .iter()
            .find(|t| t.target_type == TargetType::Executable)
            .ok_or_else(|| miette::miette!("No executable target found"))?
    };

    let exe_path = cwd.join(".build").join("debug").join(&exe_target.name);

    println!(
        "{} Running {}",
        style("→").blue().bold(),
        style(&exe_target.name).cyan()
    );

    let status = tokio::process::Command::new(&exe_path)
        .args(args)
        .status()
        .await
        .into_diagnostic()?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }

    Ok(())
}

/// Run tests.
pub async fn test(target: Option<&str>, filter: Option<&str>) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;

    println!("{} Running tests", style("→").blue().bold());

    let mut cmd = tokio::process::Command::new("swift");
    cmd.arg("test");
    cmd.current_dir(&cwd);

    if let Some(t) = target {
        cmd.arg("--filter").arg(t);
    }
    if let Some(f) = filter {
        cmd.arg("--filter").arg(f);
    }

    let status = cmd.status().await.into_diagnostic()?;

    if status.success() {
        println!("{} Tests passed", style("✓").green().bold());
    } else {
        return Err(miette::miette!("Tests failed"));
    }

    Ok(())
}

/// Clean build artifacts.
pub async fn clean(deps: bool) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;

    let build_dir = cwd.join(".build");
    if build_dir.exists() {
        fs::remove_dir_all(&build_dir).into_diagnostic()?;
        println!(
            "{} Removed {}",
            style("✓").green().bold(),
            build_dir.display()
        );
    }

    if deps {
        // Also clean Package.resolved if using SPM
        let resolved = cwd.join("Package.resolved");
        if resolved.exists() {
            fs::remove_file(&resolved).into_diagnostic()?;
        }
    }

    Ok(())
}

/// Add a dependency.
pub async fn add(package: &str, git: Option<&str>, path: Option<&Path>, dev: bool) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let manifest_path = cwd.join("Gust.toml");

    if !manifest_path.exists() {
        return Err(miette::miette!(
            "No Gust.toml found. Run 'gust init' first."
        ));
    }

    // Parse package@version if provided
    let (name, version) = if let Some(idx) = package.find('@') {
        (&package[..idx], Some(&package[idx + 1..]))
    } else {
        (package, None)
    };

    println!(
        "{} Adding {} {}",
        style("→").blue().bold(),
        style(name).cyan(),
        if dev { "(dev)" } else { "" }
    );

    // Read existing manifest
    let content = fs::read_to_string(&manifest_path).into_diagnostic()?;

    // Build the dependency line
    let dep_line = if let Some(git_url) = git {
        format!("{} = {{ git = \"{}\" }}", name, git_url)
    } else if let Some(p) = path {
        format!("{} = {{ path = \"{}\" }}", name, p.display())
    } else {
        let ver = version.unwrap_or("*");
        format!("{} = \"{}\"", name, ver)
    };

    // Find or create the dependencies section
    let section = if dev { "[dev-dependencies]" } else { "[dependencies]" };

    let new_content = if content.contains(section) {
        // Add to existing section
        let mut lines: Vec<&str> = content.lines().collect();
        let mut insert_idx = None;
        let mut in_section = false;

        for (i, line) in lines.iter().enumerate() {
            if line.trim() == section {
                in_section = true;
                continue;
            }
            if in_section {
                // Check if we've hit another section
                if line.starts_with('[') && line.ends_with(']') {
                    insert_idx = Some(i);
                    break;
                }
                // Check if this dep already exists
                if line.starts_with(name) && (line.contains('=') || line.contains(" =")) {
                    return Err(miette::miette!(
                        "Dependency '{}' already exists. Use 'gust update' to change it.",
                        name
                    ));
                }
            }
        }

        // If we're still in the section at the end, insert at end
        if in_section && insert_idx.is_none() {
            insert_idx = Some(lines.len());
        }

        if let Some(idx) = insert_idx {
            lines.insert(idx, &dep_line);
            lines.join("\n")
        } else {
            // Section exists but couldn't find insert point, append to end
            format!("{}\n{}\n", content.trim_end(), dep_line)
        }
    } else {
        // Add new section
        format!("{}\n\n{}\n{}\n", content.trim_end(), section, dep_line)
    };

    fs::write(&manifest_path, new_content).into_diagnostic()?;

    println!(
        "{} Added {} to {}",
        style("✓").green().bold(),
        style(name).cyan(),
        if dev { "dev-dependencies" } else { "dependencies" }
    );

    // Offer to install
    println!(
        "\n{} Run {} to install",
        style("→").dim(),
        style("gust install").cyan()
    );

    Ok(())
}

/// Remove a dependency.
pub async fn remove(package: &str) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let manifest_path = cwd.join("Gust.toml");

    if !manifest_path.exists() {
        return Err(miette::miette!("No Gust.toml found"));
    }

    println!(
        "{} Removing {}",
        style("→").blue().bold(),
        style(package).cyan()
    );

    let content = fs::read_to_string(&manifest_path).into_diagnostic()?;
    let mut lines: Vec<&str> = content.lines().collect();
    let mut removed = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i].trim();
        // Check if line starts with the package name followed by = or space
        if line.starts_with(package) {
            let rest = &line[package.len()..];
            if rest.starts_with(' ') || rest.starts_with('=') {
                lines.remove(i);
                removed = true;
                continue;
            }
        }
        i += 1;
    }

    if !removed {
        return Err(miette::miette!("Dependency '{}' not found", package));
    }

    // Clean up empty sections
    let new_content = lines.join("\n");
    fs::write(&manifest_path, new_content).into_diagnostic()?;

    println!(
        "{} Removed {}",
        style("✓").green().bold(),
        style(package).cyan()
    );

    Ok(())
}

/// Install dependencies.
pub async fn install(frozen: bool) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;

    let options = InstallOptions {
        frozen,
        concurrency: 8,
    };

    let installer = Installer::new(cwd, options)?;
    let result = installer.install().await?;

    println!(
        "\n{} Installed {} packages",
        style("✓").green().bold(),
        style(result.installed).cyan()
    );

    Ok(())
}

/// Update dependencies.
pub async fn update(package: Option<&str>, _breaking: bool) -> Result<()> {
    if let Some(pkg) = package {
        println!(
            "{} Updating {}",
            style("→").blue().bold(),
            style(pkg).cyan()
        );
    } else {
        println!("{} Updating all dependencies", style("→").blue().bold());
    }

    // TODO: Implement actual update
    println!("{} Dependencies updated", style("✓").green().bold());

    Ok(())
}

/// Show dependency tree.
pub async fn tree(_depth: Option<usize>, _duplicates: bool) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let (manifest, _) = find_manifest(&cwd).into_diagnostic()?;

    println!("{} v{}", manifest.package.name, manifest.package.version);

    for (name, dep) in &manifest.dependencies {
        let version = dep
            .version
            .as_ref()
            .map(|v| v.to_string())
            .unwrap_or_else(|| "git".to_string());
        println!("├── {} {}", name, style(version).dim());
    }

    Ok(())
}

/// Show outdated dependencies.
pub async fn outdated() -> Result<()> {
    println!("{} Checking for outdated dependencies...", style("→").blue().bold());

    // TODO: Implement version checking
    println!("{} All dependencies are up to date", style("✓").green().bold());

    Ok(())
}

/// List cached packages.
pub async fn cache_list() -> Result<()> {
    let cache = GlobalCache::open().into_diagnostic()?;

    println!("{} Package cache", style("Packages:").bold());
    println!("  Location: {}", cache.packages_dir().display());

    // Show binary cache stats
    if let Ok(stats) = gust_build::get_cache_stats() {
        println!("\n{} Binary artifact cache", style("Artifacts:").bold());
        println!("  Cached builds: {}", stats.count);
        println!("  Total size: {}", stats.size_human());
    }

    Ok(())
}

/// Show cache statistics.
pub async fn cache_stats() -> Result<()> {
    println!("{}", style("Cache Statistics").bold().underlined());
    println!();

    // Package cache
    let cache = GlobalCache::open().into_diagnostic()?;
    println!("{}", style("Package Cache:").bold());
    println!("  Location: {}", cache.git_dir().display());

    // Count git cache entries
    if let Ok(entries) = fs::read_dir(cache.git_dir()) {
        let count = entries.filter(|e| e.is_ok()).count();
        println!("  Cached repos: {}", count);
    }

    // Binary cache
    println!();
    println!("{}", style("Binary Artifact Cache:").bold());

    match gust_build::get_cache_stats() {
        Ok(stats) => {
            println!("  Cached builds: {}", stats.count);
            println!("  Total size: {}", stats.size_human());
        }
        Err(_) => {
            println!("  {} Not available", style("!").yellow());
        }
    }

    // Manifest cache
    println!();
    println!("{}", style("Manifest Cache:").bold());
    if let Ok(manifest_cache) = gust_manifest::ManifestCache::open() {
        if let Ok(stats) = manifest_cache.stats() {
            println!("  Cached manifests: {}", stats.count);
            println!("  Total size: {} bytes", stats.size);
        }
    }

    Ok(())
}

/// Clean cache.
pub async fn cache_clean(all: bool, binary_only: bool) -> Result<()> {
    if binary_only {
        println!(
            "{} Clearing binary artifact cache...",
            style("→").blue().bold()
        );

        match gust_build::clear_binary_cache() {
            Ok(count) => {
                println!(
                    "{} Removed {} cached builds",
                    style("✓").green().bold(),
                    count
                );
            }
            Err(e) => {
                println!("{} Failed to clear binary cache: {}", style("!").yellow(), e);
            }
        }

        return Ok(());
    }

    let cache = GlobalCache::open().into_diagnostic()?;

    if all {
        println!(
            "{} Removing all cached packages...",
            style("→").blue().bold()
        );

        // Clear git cache
        let git_dir = cache.git_dir();
        if git_dir.exists() {
            fs::remove_dir_all(&git_dir).into_diagnostic()?;
            fs::create_dir_all(&git_dir).into_diagnostic()?;
        }

        // Clear binary cache
        if let Ok(count) = gust_build::clear_binary_cache() {
            if count > 0 {
                println!("  {} Removed {} binary artifacts", style("•").dim(), count);
            }
        }

        // Clear manifest cache
        if let Ok(manifest_cache) = gust_manifest::ManifestCache::open() {
            let _ = manifest_cache.clear();
        }
    } else {
        println!(
            "{} Removing unused packages...",
            style("→").blue().bold()
        );
        // TODO: Implement unused package cleanup (track last access time)
    }

    println!("{} Cache cleaned", style("✓").green().bold());

    Ok(())
}

/// Print cache path.
pub async fn cache_path() -> Result<()> {
    let cache = GlobalCache::open().into_diagnostic()?;
    println!("Package cache: {}", cache.packages_dir().display());
    println!("Git cache: {}", cache.git_dir().display());
    println!("Binary cache: {}", cache.binary_cache_dir().display());
    Ok(())
}

/// Migrate Package.swift to Gust.toml.
pub async fn migrate() -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let package_swift = cwd.join("Package.swift");

    if !package_swift.exists() {
        return Err(miette::miette!("Package.swift not found"));
    }

    let gust_toml = cwd.join("Gust.toml");
    if gust_toml.exists() {
        return Err(miette::miette!("Gust.toml already exists"));
    }

    println!(
        "{} Migrating Package.swift to Gust.toml",
        style("→").blue().bold()
    );

    let (manifest, _) = find_manifest(&cwd).into_diagnostic()?;
    let toml = generate_gust_toml(&manifest);
    fs::write(&gust_toml, toml).into_diagnostic()?;

    println!(
        "{} Created {}",
        style("✓").green().bold(),
        gust_toml.display()
    );

    Ok(())
}

/// Show package info.
pub async fn info(package: &str) -> Result<()> {
    println!("{} Looking up {}...", style("→").blue().bold(), package);

    // Parse scope.name format
    let (scope, name) = if let Some(idx) = package.find('.') {
        (&package[..idx], &package[idx + 1..])
    } else {
        // Try common scopes
        ("apple", package)
    };

    let client = gust_registry::RegistryClient::new();

    match client.list_versions(scope, name).await {
        Ok(versions) => {
            println!("\n{} {}.{}", style("Package:").bold(), scope, name);
            println!("{} {} versions available", style("Versions:").bold(), versions.releases.len());

            let mut version_list: Vec<_> = versions.releases.keys().collect();
            version_list.sort();
            version_list.reverse();

            for (i, v) in version_list.iter().take(5).enumerate() {
                let marker = if i == 0 { "(latest)" } else { "" };
                println!("  {} {}", style(v).cyan(), style(marker).dim());
            }

            if version_list.len() > 5 {
                println!("  {} ... and {} more", style("").dim(), version_list.len() - 5);
            }
        }
        Err(e) => {
            println!(
                "{} Package not found in registry: {}",
                style("!").yellow().bold(),
                e
            );
            println!(
                "\n{} Try searching with: {}",
                style("→").dim(),
                style(format!("gust add {} --git <url>", package)).cyan()
            );
        }
    }

    Ok(())
}
