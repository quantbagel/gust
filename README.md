# Gust

An extremely fast Swift package manager, written in Rust.

<p align="center">
  <img alt="Gust vs SwiftPM benchmark" src="https://github.com/user-attachments/assets/benchmark-placeholder.svg" width="600">
</p>

<p align="center">
  <em>Installing Vapor's dependencies with a warm cache.</em>
</p>

## Highlights

- 10-100x faster than SwiftPM
- Parallel dependency fetching
- Content-addressable cache with hard links
- Binary artifact caching
- Works with existing `Package.swift` files

## Installation

```sh
curl -fsSL https://gust.dev/install.sh | sh
```

See the [installation docs](https://gust.dev/docs/installation) for Homebrew, Cargo, and other methods.

## Usage

```sh
gust new myapp        # Create a new package
gust add vapor        # Add a dependency
gust install          # Install dependencies
gust build            # Build the package
gust run              # Run the executable
```

## Documentation

Full documentation is available at [gust.dev/docs](https://gust.dev/docs).

## License

MIT
