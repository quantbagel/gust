# Gust

An extremely fast Swift package manager, written in Rust.

## Highlights

- **10-100x faster** than SwiftPM for common operations
- **Parallel dependency fetching** with concurrent git clones
- **Content-addressable cache** with hard links (pnpm-style)
- **Binary artifact caching** for near-instant rebuilds
- Works with existing `Package.swift` files or simpler `Gust.toml`

## Installation

```sh
curl -fsSL quantbagel.vercel.app/gust/install.sh | sh
```

See [docs/installation.md](docs/installation.md) for Homebrew, Cargo, and other methods.

## Quick Start

```sh
gust new myapp        # Create a new package
cd myapp
gust add swift-log --git https://github.com/apple/swift-log.git --tag 1.5.0
gust install          # Install dependencies
gust build            # Build the package
gust run              # Run the executable
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
| `gust build` | Build the package |
| `gust run` | Run the executable |
| `gust test` | Run tests |
| `gust tree` | Show dependency tree |
| `gust search <query>` | Search Swift packages |

## Documentation

- [Installation](docs/installation.md)
- [Configuration](docs/configuration.md)
- [Commands](docs/commands.md)
- [Gust.toml Format](docs/gust-toml.md)
- [Workspaces](docs/workspaces.md)
- [Caching](docs/caching.md)

## License

MIT
