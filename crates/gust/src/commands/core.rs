//! Core CLI command implementations.

use crate::commands::ui::{self, dim, green, pkg, separator};
use crate::commands::version::{check_all_for_updates, filter_breaking};
use crate::install::{InstallOptions, Installer};
use console::style;
use gust_build::{BuildOptions, Builder};
use gust_cache::GlobalCache;
use gust_manifest::{find_manifest, generate_gust_toml, write_package_swift, ManifestType};
use gust_types::{BuildConfiguration, Manifest, Package, Target, TargetType, Version};
use miette::{IntoDiagnostic, Result};
use std::env;
use std::fs;
use std::path::Path;
use std::process::Command;

/// Well-known Swift package GitHub organizations for auto-discovery
const KNOWN_ORGS: &[(&str, &str)] = &[
    ("vapor", "vapor"),
    ("swift-log", "apple"),
    ("swift-nio", "apple"),
    ("swift-collections", "apple"),
    ("swift-algorithms", "apple"),
    ("swift-crypto", "apple"),
    ("swift-system", "apple"),
    ("swift-argument-parser", "apple"),
    ("swift-atomics", "apple"),
    ("swift-numerics", "apple"),
    ("swift-async-algorithms", "apple"),
    ("async-http-client", "swift-server"),
    ("async-kit", "vapor"),
    ("fluent", "vapor"),
    ("leaf", "vapor"),
    ("alamofire", "Alamofire"),
    ("kingfisher", "onevcat"),
    ("snapkit", "SnapKit"),
    ("realm-swift", "realm"),
    ("rxswift", "ReactiveX"),
    ("moya", "Moya"),
    ("swiftyjson", "SwiftyJSON"),
    ("hero", "HeroTransitions"),
];

/// Create a new package.
pub async fn new_package(name: &str, pkg_type: &str, no_git: bool) -> Result<()> {
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
        TargetType::Executable => sources_dir.join("main.swift"),
        _ => sources_dir.join(format!("{}.swift", name)),
    };

    let content = match target_type {
        TargetType::Executable => "print(\"Hello, world!\")\n",
        _ => &format!("public struct {} {{\n    public init() {{}}\n}}\n", name),
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

    // Create .gitignore
    let gitignore = r#".build/
.swiftpm/
*.xcodeproj
*.xcworkspace
DerivedData/
Package.swift
"#;
    fs::write(path.join(".gitignore"), gitignore).into_diagnostic()?;

    // Initialize git repository
    if !no_git {
        let git_result = Command::new("git")
            .args(["init", "-q"])
            .current_dir(&path)
            .status();

        if let Ok(status) = git_result {
            if status.success() {
                println!("{} Initialized git repository", style("✓").green());
            }
        }
    }

    println!(
        "{} Created package {} at {}",
        style("✓").green().bold(),
        style(name).cyan(),
        path.display()
    );

    println!("\n{}", style("Next steps:").bold());
    println!("  cd {}", name);
    println!("  gust build");
    if target_type == TargetType::Executable {
        println!("  gust run");
    }

    Ok(())
}

/// Initialize a package in the current directory.
pub async fn init(name: Option<&str>, pkg_type: &str) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let pkg_name = name
        .map(String::from)
        .or_else(|| cwd.file_name().map(|n| n.to_string_lossy().to_string()))
        .unwrap_or_else(|| "MyPackage".to_string());

    let manifest_path = cwd.join("Gust.toml");
    if manifest_path.exists() {
        return Err(miette::miette!("Gust.toml already exists"));
    }

    let target_type = match pkg_type {
        "executable" | "exe" => TargetType::Executable,
        "library" | "lib" => TargetType::Library,
        _ => return Err(miette::miette!("Unknown package type: {}", pkg_type)),
    };

    let manifest = create_manifest(&pkg_name, target_type);
    let toml = generate_gust_toml(&manifest);
    fs::write(&manifest_path, toml).into_diagnostic()?;

    // Create source directory if it doesn't exist
    let sources_dir = cwd.join("Sources").join(&pkg_name);
    if !sources_dir.exists() {
        fs::create_dir_all(&sources_dir).into_diagnostic()?;

        let main_file = match target_type {
            TargetType::Executable => sources_dir.join("main.swift"),
            _ => sources_dir.join(format!("{}.swift", pkg_name)),
        };

        let content = match target_type {
            TargetType::Executable => "print(\"Hello, world!\")\n".to_string(),
            _ => format!(
                "public struct {} {{\n    public init() {{}}\n}}\n",
                pkg_name
            ),
        };

        fs::write(main_file, content).into_diagnostic()?;
        println!("{} Created source files", style("✓").green());
    }

    // Create .gitignore if it doesn't exist
    let gitignore_path = cwd.join(".gitignore");
    if !gitignore_path.exists() {
        let gitignore = r#".build/
.swiftpm/
*.xcodeproj
*.xcworkspace
DerivedData/
"#;
        fs::write(&gitignore_path, gitignore).into_diagnostic()?;
    }

    println!(
        "{} Initialized package {}",
        style("✓").green().bold(),
        style(&pkg_name).cyan()
    );

    println!("\n{}", style("Next steps:").bold());
    println!("  gust add <package>  # Add dependencies");
    println!("  gust install        # Install dependencies");
    println!("  gust build          # Build the package");

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
    let (manifest, manifest_type) = find_manifest(&cwd).into_diagnostic()?;

    // Auto-generate Package.swift from Gust.toml if needed
    if manifest_type == ManifestType::GustToml {
        write_package_swift(&manifest, &cwd).into_diagnostic()?;
    }

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

/// Try to resolve a package name to a GitHub URL.
fn resolve_package_url(package: &str) -> Option<String> {
    // Check if it's org/repo format
    if package.contains('/') {
        let parts: Vec<&str> = package.split('/').collect();
        if parts.len() == 2 {
            return Some(format!("https://github.com/{}/{}.git", parts[0], parts[1]));
        }
    }

    // Check known packages
    let lower = package.to_lowercase();
    for (name, org) in KNOWN_ORGS {
        if lower == *name {
            return Some(format!("https://github.com/{}/{}.git", org, name));
        }
    }

    None
}

/// Add a dependency.
pub async fn add(
    package: &str,
    git: Option<&str>,
    branch: Option<&str>,
    tag: Option<&str>,
    path: Option<&Path>,
    dev: bool,
) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let manifest_path = cwd.join("Gust.toml");

    if !manifest_path.exists() {
        return Err(miette::miette!(
            "No Gust.toml found. Run 'gust init' first."
        ));
    }

    // Parse package@version if provided
    let (pkg_spec, version) = if let Some(idx) = package.find('@') {
        (&package[..idx], Some(&package[idx + 1..]))
    } else {
        (package, None)
    };

    // Extract the actual package name (last component of org/repo)
    let name = if pkg_spec.contains('/') {
        pkg_spec.split('/').next_back().unwrap_or(pkg_spec)
    } else {
        pkg_spec
    };

    println!(
        "{} Adding {} {}",
        style("→").blue().bold(),
        style(name).cyan(),
        if dev { "(dev)" } else { "" }
    );

    // Resolve the git URL
    let resolved_git = if let Some(url) = git {
        Some(url.to_string())
    } else if path.is_none() {
        // Try to auto-discover the URL
        if let Some(url) = resolve_package_url(pkg_spec) {
            println!("  {} Resolved to {}", style("→").dim(), style(&url).dim());
            Some(url)
        } else {
            None
        }
    } else {
        None
    };

    // Read existing manifest
    let content = fs::read_to_string(&manifest_path).into_diagnostic()?;

    // Build the dependency line
    let dep_line = if let Some(ref git_url) = resolved_git {
        let mut parts = vec![format!("git = \"{}\"", git_url)];
        if let Some(b) = branch {
            parts.push(format!("branch = \"{}\"", b));
        }
        if let Some(t) = tag.or(version) {
            parts.push(format!("tag = \"{}\"", t));
        }
        format!("{} = {{ {} }}", name, parts.join(", "))
    } else if let Some(p) = path {
        format!("{} = {{ path = \"{}\" }}", name, p.display())
    } else {
        // No git URL found and no path - error with helpful message
        return Err(miette::miette!(
            "Could not resolve package '{}'. Try one of:\n  \
             gust add {} --git <url>\n  \
             gust add apple/{}\n  \
             gust add vapor/{}",
            name,
            name,
            name,
            name
        ));
    };

    // Find or create the dependencies section
    let section = if dev {
        "[dev-dependencies]"
    } else {
        "[dependencies]"
    };

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
        if dev {
            "dev-dependencies"
        } else {
            "dependencies"
        }
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
        if let Some(rest) = line.strip_prefix(package) {
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

    // Scale concurrency with CPU cores (optimized for Apple Silicon Pro/Max chips)
    let concurrency = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(8);

    let options = InstallOptions {
        frozen,
        concurrency,
    };

    let installer = Installer::new(cwd.clone(), options)?;
    let result = installer.install().await?;

    // Auto-generate Package.swift from Gust.toml
    let (manifest, manifest_type) = find_manifest(&cwd).into_diagnostic()?;
    if manifest_type == ManifestType::GustToml {
        write_package_swift(&manifest, &cwd).into_diagnostic()?;
    }

    println!(
        "\n{} Installed {} packages",
        style("✓").green().bold(),
        style(result.installed).cyan()
    );

    Ok(())
}

/// Update dependencies.
pub async fn update(package: Option<&str>, breaking: bool) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let manifest_path = cwd.join("Gust.toml");
    let lockfile_path = cwd.join("Gust.lock");

    if !manifest_path.exists() {
        return Err(miette::miette!(
            "No Gust.toml found. Run 'gust init' first."
        ));
    }

    if !lockfile_path.exists() {
        return Err(miette::miette!(
            "No Gust.lock found. Run 'gust install' first."
        ));
    }

    let lockfile = gust_lockfile::Lockfile::load(&lockfile_path).into_diagnostic()?;
    let mut manifest_content = fs::read_to_string(&manifest_path).into_diagnostic()?;

    // Filter packages to update
    let packages_to_check: Vec<_> = lockfile
        .packages
        .iter()
        .filter(|p| package.is_none_or(|name| p.name == name))
        .collect();

    if packages_to_check.is_empty() {
        if let Some(name) = package {
            return Err(miette::miette!("Package '{}' not found in lockfile", name));
        }
        ui::success("No dependencies to update");
        return Ok(());
    }

    ui::info(format!(
        "Checking {} package(s) for updates...",
        packages_to_check.len()
    ));

    // Check for updates using shared helper
    let all_updates = check_all_for_updates(&packages_to_check).await;

    if all_updates.is_empty() {
        ui::success("All dependencies are up to date");
        return Ok(());
    }

    // Filter by breaking changes
    let updates = filter_breaking(all_updates, breaking);

    if updates.is_empty() {
        ui::success("No non-breaking updates available");
        println!(
            "  Run {} to include breaking changes",
            pkg("gust update --breaking")
        );
        return Ok(());
    }

    println!();
    println!(
        "{:<30} {:<15} {:<15}",
        style("Package").bold(),
        style("Current").bold(),
        style("New").bold()
    );
    separator(60);

    for u in &updates {
        println!(
            "{:<30} {:<15} {}",
            pkg(&u.name),
            dim(&u.current),
            green(&u.latest_tag)
        );
        update_manifest_tag(&mut manifest_content, &u.name, &u.latest_tag);
    }

    // Write updated manifest
    fs::write(&manifest_path, &manifest_content).into_diagnostic()?;

    // Clear the cache for updated packages so they get re-fetched
    let cache = GlobalCache::open().into_diagnostic()?;
    for u in &updates {
        let cache_path = cache.git_dir().join(&u.name);
        if cache_path.exists() {
            let _ = fs::remove_dir_all(&cache_path);
        }
    }

    // Remove lockfile to force re-resolution
    let _ = fs::remove_file(&lockfile_path);

    println!();
    ui::success(format!("Updated {} package(s)", updates.len()));
    ui::hint(format!(
        "Run {} to install the updates",
        pkg("gust install")
    ));

    Ok(())
}

/// Update a tag in the manifest content for a given package.
fn update_manifest_tag(content: &mut String, name: &str, new_tag: &str) {
    let patterns = [
        format!("{} = {{ git =", name),
        format!("{} = {{git =", name),
    ];

    for pattern in &patterns {
        if let Some(start_idx) = content.find(pattern) {
            let line_start = content[..start_idx].rfind('\n').map_or(0, |i| i + 1);
            let line_end = content[start_idx..]
                .find('\n')
                .map_or(content.len(), |i| start_idx + i);
            let line = &content[line_start..line_end];

            if let Some(tag_start) = line.find("tag = \"") {
                let tag_value_start = tag_start + 7;
                if let Some(tag_end) = line[tag_value_start..].find('"') {
                    let old_line = line.to_string();
                    let new_line = format!(
                        "{}{}{}",
                        &line[..tag_value_start],
                        new_tag,
                        &line[tag_value_start + tag_end..]
                    );
                    *content = content.replace(&old_line, &new_line);
                }
            }
        }
    }
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
    let cwd = env::current_dir().into_diagnostic()?;
    let lockfile_path = cwd.join("Gust.lock");

    if !lockfile_path.exists() {
        return Err(miette::miette!(
            "No Gust.lock found. Run 'gust install' first."
        ));
    }

    ui::info("Checking for outdated dependencies...");

    let lockfile = gust_lockfile::Lockfile::load(&lockfile_path).into_diagnostic()?;

    if lockfile.packages.is_empty() {
        ui::success("No dependencies to check");
        return Ok(());
    }

    let packages: Vec<_> = lockfile.packages.iter().collect();
    let outdated_deps = check_all_for_updates(&packages).await;

    if outdated_deps.is_empty() {
        ui::success("All dependencies are up to date");
    } else {
        println!();
        println!(
            "{:<30} {:<15} {:<15}",
            style("Package").bold(),
            style("Current").bold(),
            style("Latest").bold()
        );
        separator(60);

        for dep in &outdated_deps {
            println!(
                "{:<30} {:<15} {}",
                pkg(&dep.name),
                dim(&dep.current),
                green(&dep.latest)
            );
        }

        println!();
        ui::warn(format!("{} package(s) can be updated", outdated_deps.len()));
        println!("  Run {} to update all", pkg("gust update"));
    }

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
                println!(
                    "{} Failed to clear binary cache: {}",
                    style("!").yellow(),
                    e
                );
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
        println!("{} Removing unused packages...", style("→").blue().bold());

        // Remove packages not accessed in last 30 days
        let git_dir = cache.git_dir();
        let cutoff =
            std::time::SystemTime::now() - std::time::Duration::from_secs(30 * 24 * 60 * 60);
        let mut removed = 0;

        if git_dir.exists() {
            if let Ok(entries) = fs::read_dir(&git_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        // Check last access time
                        if let Ok(metadata) = fs::metadata(&path) {
                            let accessed = metadata
                                .accessed()
                                .or_else(|_| metadata.modified())
                                .unwrap_or(std::time::SystemTime::UNIX_EPOCH);

                            if accessed < cutoff && fs::remove_dir_all(&path).is_ok() {
                                removed += 1;
                                if let Some(name) = path.file_name() {
                                    println!(
                                        "  {} Removed {}",
                                        style("•").dim(),
                                        name.to_string_lossy()
                                    );
                                }
                            }
                        }
                    }
                }
            }
        }

        if removed == 0 {
            println!("  {} No unused packages found", style("•").dim());
        } else {
            println!("  {} Removed {} unused packages", style("•").dim(), removed);
        }
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

/// Generate Package.swift from Gust.toml.
pub async fn generate() -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let gust_toml = cwd.join("Gust.toml");

    if !gust_toml.exists() {
        return Err(miette::miette!(
            "Gust.toml not found. Run 'gust init' or 'gust migrate' first."
        ));
    }

    println!(
        "{} Generating Package.swift from Gust.toml",
        style("→").blue().bold()
    );

    let (manifest, _) = find_manifest(&cwd).into_diagnostic()?;
    write_package_swift(&manifest, &cwd).into_diagnostic()?;

    println!("{} Generated Package.swift", style("✓").green().bold());

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
            println!(
                "{} {} versions available",
                style("Versions:").bold(),
                versions.releases.len()
            );

            let mut version_list: Vec<_> = versions.releases.keys().collect();
            version_list.sort();
            version_list.reverse();

            for (i, v) in version_list.iter().take(5).enumerate() {
                let marker = if i == 0 { "(latest)" } else { "" };
                println!("  {} {}", style(v).cyan(), style(marker).dim());
            }

            if version_list.len() > 5 {
                println!(
                    "  {} ... and {} more",
                    style("").dim(),
                    version_list.len() - 5
                );
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

/// Search for packages.
pub async fn search(query: &str, limit: usize) -> Result<()> {
    use crate::package_index;

    println!(
        "{} Searching Swift Package Index for '{}'...",
        style("→").blue().bold(),
        style(query).cyan()
    );

    // Try to get package count for context
    let count_info = match package_index::package_count().await {
        Ok(count) => format!(" ({} packages indexed)", count),
        Err(_) => String::new(),
    };

    // Search in Swift Package Index
    let matches = match package_index::search_packages(query, limit * 2).await {
        Ok(m) => m,
        Err(e) => {
            // Fall back to known packages if index fetch fails
            println!(
                "{} Could not fetch package index: {}",
                style("!").yellow(),
                e
            );
            println!("{} Searching local cache...\n", style("→").blue().bold());

            let query_lower = query.to_lowercase();
            let local_matches: Vec<_> = KNOWN_ORGS
                .iter()
                .filter(|(name, _)| name.to_lowercase().contains(&query_lower))
                .map(|(name, org)| crate::package_index::IndexedPackage {
                    name: name.to_string(),
                    owner: org.to_string(),
                    url: format!("https://github.com/{}/{}.git", org, name),
                })
                .collect();

            if local_matches.is_empty() {
                println!(
                    "{} No packages found matching '{}'",
                    style("!").yellow(),
                    query
                );
                return Ok(());
            }
            local_matches
        }
    };

    if matches.is_empty() {
        println!(
            "\n{} No packages found matching '{}'{}",
            style("!").yellow(),
            query,
            count_info
        );
        println!("\n{}", style("Try:").bold());
        println!("  gust add <org>/<repo>           # Add from GitHub");
        println!("  gust add <name> --git <url>     # Add with explicit URL");
        return Ok(());
    }

    println!(
        "\n{} Found {} matching packages{}:\n",
        style("✓").green(),
        matches.len().min(limit),
        count_info
    );

    for pkg in matches.iter().take(limit) {
        println!(
            "  {} {}/{}",
            style("•").dim(),
            style(&pkg.owner).dim(),
            style(&pkg.name).cyan().bold()
        );
        println!(
            "    {}",
            style(format!("gust add {}/{}", pkg.owner, pkg.name)).dim()
        );
    }

    if matches.len() > limit {
        println!(
            "\n  {} ... and {} more (use --limit to see more)",
            style("").dim(),
            matches.len() - limit
        );
    }

    Ok(())
}

/// List installed Swift versions.
pub async fn swift_list() -> Result<()> {
    println!("{}", style("Installed Swift versions:").bold());

    // Check Xcode toolchains
    let toolchain_dir = dirs::home_dir().map(|h| h.join("Library/Developer/Toolchains"));

    if let Some(dir) = toolchain_dir {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        if name.ends_with(".xctoolchain") {
                            let version = name.trim_end_matches(".xctoolchain");
                            println!("  {} {}", style("•").dim(), version);
                        }
                    }
                }
            }
        }
    }

    // Check swiftenv
    let swiftenv_dir = dirs::home_dir().map(|h| h.join(".swiftenv/versions"));

    if let Some(dir) = swiftenv_dir {
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    if let Some(name) = entry.file_name().to_str() {
                        println!("  {} {} (swiftenv)", style("•").dim(), name);
                    }
                }
            }
        }
    }

    // Show current
    swift_current().await?;

    Ok(())
}

/// Show current Swift version.
pub async fn swift_current() -> Result<()> {
    let output = Command::new("swift")
        .arg("--version")
        .output()
        .into_diagnostic()?;

    if output.status.success() {
        let version = String::from_utf8_lossy(&output.stdout);
        let first_line = version.lines().next().unwrap_or("unknown");
        println!(
            "\n{} {}",
            style("Current:").bold(),
            style(first_line).cyan()
        );
    } else {
        println!("{} Swift not found in PATH", style("!").yellow().bold());
    }

    Ok(())
}

/// Install a Swift version.
pub async fn swift_install(version: &str) -> Result<()> {
    println!(
        "{} Installing Swift {}...",
        style("→").blue().bold(),
        style(version).cyan()
    );

    // Check if swiftenv is available
    if Command::new("swiftenv").arg("--version").output().is_ok() {
        let status = Command::new("swiftenv")
            .args(["install", version])
            .status()
            .into_diagnostic()?;

        if status.success() {
            println!("{} Installed Swift {}", style("✓").green().bold(), version);
        } else {
            return Err(miette::miette!("Failed to install Swift {}", version));
        }
    } else {
        println!("{}", style("Swift version management options:").bold());
        println!();
        println!("  {} Install swiftenv:", style("1.").cyan());
        println!("     brew install kylef/formulae/swiftenv");
        println!();
        println!("  {} Download from swift.org:", style("2.").cyan());
        println!("     https://swift.org/download/");
        println!();
        println!("  {} Use Xcode (macOS):", style("3.").cyan());
        println!("     xcode-select --install");
    }

    Ok(())
}

/// Use a specific Swift version.
pub async fn swift_use(version: &str, global: bool) -> Result<()> {
    // Check if swiftenv is available
    if Command::new("swiftenv").arg("--version").output().is_ok() {
        let args = if global {
            vec!["global", version]
        } else {
            vec!["local", version]
        };

        let status = Command::new("swiftenv")
            .args(&args)
            .status()
            .into_diagnostic()?;

        if status.success() {
            println!(
                "{} Now using Swift {} {}",
                style("✓").green().bold(),
                style(version).cyan(),
                if global { "(global)" } else { "(local)" }
            );
        } else {
            return Err(miette::miette!("Failed to set Swift version"));
        }
    } else {
        println!(
            "{} swiftenv not found. Install with:",
            style("!").yellow().bold()
        );
        println!("  brew install kylef/formulae/swiftenv");
    }

    Ok(())
}

/// Open package in Xcode.
///
/// Modern Xcode (11+) can open Package.swift files directly without generating
/// a .xcodeproj file. This is the recommended approach as generate-xcodeproj
/// was deprecated and removed in Swift 5.6+.
pub async fn xcode_generate(open: bool) -> Result<()> {
    let cwd = env::current_dir().into_diagnostic()?;
    let (manifest, manifest_type) = find_manifest(&cwd).into_diagnostic()?;

    // Ensure Package.swift exists
    let package_path = cwd.join("Package.swift");
    if manifest_type == ManifestType::GustToml {
        println!(
            "{} Generating Package.swift from Gust.toml",
            style("→").blue().bold()
        );
        write_package_swift(&manifest, &cwd).into_diagnostic()?;
    }

    if !package_path.exists() {
        return Err(miette::miette!(
            "No Package.swift found. Run 'gust generate' first."
        ));
    }

    println!(
        "{} Package {} is ready for Xcode",
        style("✓").green().bold(),
        style(&manifest.package.name).cyan()
    );

    println!(
        "  {} Xcode can open Package.swift directly (no .xcodeproj needed)",
        style("ℹ").blue()
    );

    if open {
        println!("{} Opening in Xcode...", style("→").blue());
        // Open the Package.swift directly - Xcode will handle it
        Command::new("open")
            .arg("-a")
            .arg("Xcode")
            .arg(&package_path)
            .status()
            .into_diagnostic()?;
    } else {
        println!(
            "  {} Run 'open -a Xcode {}' or use --open flag",
            style("→").dim(),
            package_path.display()
        );
    }

    Ok(())
}

/// Check environment and diagnose issues.
pub async fn doctor() -> Result<()> {
    println!("{}", style("Gust Doctor").bold().underlined());
    println!();

    let mut issues = 0;

    // Check Swift
    print!("{} Swift... ", style("Checking").dim());
    match Command::new("swift").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            let first_line = version.lines().next().unwrap_or("unknown");
            println!("{} {}", style("✓").green(), first_line);
        }
        _ => {
            println!("{} not found", style("✗").red());
            issues += 1;
        }
    }

    // Check git
    print!("{} Git... ", style("Checking").dim());
    match Command::new("git").arg("--version").output() {
        Ok(output) if output.status.success() => {
            let version = String::from_utf8_lossy(&output.stdout);
            println!("{} {}", style("✓").green(), version.trim());
        }
        _ => {
            println!("{} not found", style("✗").red());
            issues += 1;
        }
    }

    // Check Xcode (macOS)
    #[cfg(target_os = "macos")]
    {
        print!("{} Xcode... ", style("Checking").dim());
        match Command::new("xcode-select").arg("-p").output() {
            Ok(output) if output.status.success() => {
                let path = String::from_utf8_lossy(&output.stdout);
                println!("{} {}", style("✓").green(), path.trim());
            }
            _ => {
                println!(
                    "{} not found (run: xcode-select --install)",
                    style("✗").red()
                );
                issues += 1;
            }
        }
    }

    // Check cache directories
    print!("{} Cache directories... ", style("Checking").dim());
    match GlobalCache::open() {
        Ok(cache) => {
            let git_dir = cache.git_dir();
            let binary_dir = cache.binary_cache_dir();
            println!("{}", style("✓").green());
            println!("    Git cache: {}", git_dir.display());
            println!("    Binary cache: {}", binary_dir.display());
        }
        Err(e) => {
            println!("{} {}", style("✗").red(), e);
            issues += 1;
        }
    }

    // Check current project
    print!("{} Current project... ", style("Checking").dim());
    let cwd = env::current_dir().into_diagnostic()?;
    if cwd.join("Gust.toml").exists() {
        let (manifest, _) = find_manifest(&cwd).into_diagnostic()?;
        println!(
            "{} {} v{}",
            style("✓").green(),
            manifest.package.name,
            manifest.package.version
        );
    } else if cwd.join("Package.swift").exists() {
        println!(
            "{} Package.swift found (run {} to convert)",
            style("→").yellow(),
            style("gust migrate").cyan()
        );
    } else {
        println!("{} no package found", style("→").dim());
    }

    println!();
    if issues == 0 {
        println!("{} All checks passed!", style("✓").green().bold());
    } else {
        println!("{} {} issue(s) found", style("!").yellow().bold(), issues);
    }

    Ok(())
}
