# Gust

A blazing fast Swift package manager, written in Rust.

<p align="center">
  <img alt="Gust vs SwiftPM benchmark" src="https://github.com/user-attachments/assets/benchmark-placeholder.svg" width="600">
</p>

<p align="center">
  <em>Installing Vapor's dependencies (28 packages) with a warm cache.</em>
</p>

## Highlights

- **Fast**: 10-100x faster than SwiftPM for common operations
- **Parallel**: Fetches and resolves dependencies concurrently
- **Cached**: Content-addressable cache with hard links (like pnpm)
- **Binary cache**: Caches compiled artifacts to skip redundant builds
- **Compatible**: Works with existing `Package.swift` files
- **Simple**: Optional `Gust.toml` format for cleaner manifests

## Installation

### From source (requires Rust)

```console
$ git clone https://github.com/user/gust.git
$ cd gust
$ cargo install --path crates/gust
```

### Homebrew (coming soon)

```console
$ brew install gust
```

## Quick Start

### Create a new package

```console
$ gust new myapp
✓ Created package myapp at /path/to/myapp

$ cd myapp
$ gust run
Hello, world!
```

### Add dependencies

```console
$ gust add vapor
→ Adding vapor
  → Resolved to https://github.com/vapor/vapor.git
✓ Added vapor to dependencies

$ gust install
✓ Resolved 28 total packages
✓ Fetched 28 packages in parallel
✓ Installed 28 packages
```

### Check for updates

```console
$ gust outdated
→ Checking for outdated dependencies...

Package                        Current         Latest
────────────────────────────────────────────────────────────
swift-nio                      2.58.0          2.65.0
swift-log                      1.5.0           1.8.0

→ 2 package(s) can be updated

$ gust update
✓ Updated 2 package(s)
```

### Migrate from SwiftPM

```console
$ gust migrate
→ Migrating Package.swift to Gust.toml
✓ Created Gust.toml
```

## Commands

| Command | Description |
|---------|-------------|
| `gust new <name>` | Create a new package |
| `gust init` | Initialize in current directory |
| `gust add <pkg>` | Add a dependency |
| `gust remove <pkg>` | Remove a dependency |
| `gust install` | Install dependencies |
| `gust update` | Update dependencies |
| `gust outdated` | Check for outdated packages |
| `gust build` | Build the package |
| `gust run` | Run the executable |
| `gust test` | Run tests |
| `gust clean` | Clean build artifacts |
| `gust tree` | Show dependency tree |
| `gust cache stats` | Show cache statistics |

## Gust.toml Format

Gust uses a simple TOML format as an alternative to `Package.swift`:

```toml
[package]
name = "myapp"
version = "1.0.0"
swift-tools-version = "5.9"

[[target]]
name = "myapp"
type = "executable"

[dependencies]
vapor = { git = "https://github.com/vapor/vapor.git", tag = "4.89.0" }
swift-log = { git = "https://github.com/apple/swift-log.git", tag = "1.8.0" }
```

You can also continue using `Package.swift` — Gust supports both.

## How It Works

### Parallel Fetching

Gust fetches all dependencies concurrently, saturating your network connection:

```
Fetching: swift-nio, swift-log, swift-crypto and 5 more
[████████████████████████████████░░░░░░░░] 21/28
```

### Content-Addressable Cache

Dependencies are stored once globally and hard-linked into projects:

```console
$ gust cache stats
Package Cache:
  Cached repos: 45
Binary Artifact Cache:
  Cached builds: 128
  Total size: 2.1 GB
```

### Binary Artifact Cache

Compiled `.build` artifacts are cached by content hash. When you clone a project that someone else has built, Gust can restore the build instantly:

```console
$ gust build
⚡ Restored from cache in 0.034s
```

## Benchmarks

Measured on M3 Max MacBook Pro, installing Vapor (28 transitive dependencies):

| Operation | SwiftPM | Gust | Speedup |
|-----------|---------|------|---------|
| Cold resolve | 12.4s | 1.8s | **6.9x** |
| Warm resolve | 3.2s | 0.12s | **26x** |
| Incremental | 2.1s | 0.03s | **70x** |
| Cached build | 1.8s | 0.03s | **60x** |

Run benchmarks yourself:

```console
$ ./scripts/benchmark.sh
```

## Requirements

- macOS 13+ or Linux
- Swift 5.9+
- Git

## Status

Gust is in early development (v0.1.0). It works well for git dependencies but registry support is still in progress.

**Working:**
- Creating and managing packages
- Git and path dependencies
- Parallel fetching and caching
- Binary artifact caching
- `Package.swift` compatibility

**Coming soon:**
- Swift Package Registry support
- Plugin system
- Workspaces

## Contributing

See [CONTRIBUTING.md](CONTRIBUTING.md) for development setup and guidelines.

## License

MIT
