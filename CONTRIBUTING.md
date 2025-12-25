# Contributing to Gust

Thanks for your interest in contributing to Gust!

## Development Setup

### Prerequisites

- Rust 1.75+ (install via [rustup](https://rustup.rs))
- Swift 5.9+
- Git

### Building

```console
$ git clone https://github.com/user/gust.git
$ cd gust
$ cargo build
```

### Running

```console
$ cargo run -- --help
$ cargo run -- new testpkg
$ cargo run -- build
```

### Testing

```console
$ cargo test
```

### Release build

```console
$ cargo build --release
$ ./target/release/gust --help
```

## Project Structure

```
crates/
├── gust/              # CLI and main binary
├── gust-types/        # Core data types (Package, Dependency, etc.)
├── gust-manifest/     # Gust.toml and Package.swift parsing
├── gust-resolver/     # Dependency resolution (PubGrub-based)
├── gust-fetch/        # Parallel package fetching
├── gust-build/        # Swift compilation orchestration
├── gust-cache/        # Content-addressable store
├── gust-binary-cache/ # Compiled artifact caching
├── gust-lockfile/     # Gust.lock handling
├── gust-registry/     # Swift Package Registry client
├── gust-platform/     # Platform/toolchain detection
└── gust-diagnostics/  # Error formatting
```

## Making Changes

1. **Fork and clone** the repository
2. **Create a branch** for your changes: `git checkout -b my-feature`
3. **Make your changes** with clear, focused commits
4. **Add tests** for new functionality
5. **Run tests**: `cargo test`
6. **Run clippy**: `cargo clippy`
7. **Format code**: `cargo fmt`
8. **Submit a PR** with a clear description

## Code Style

- Run `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Keep functions focused and small
- Add doc comments for public APIs
- Use meaningful variable names

## Commit Messages

Use clear, concise commit messages:

```
Add parallel dependency fetching

- Use tokio::spawn for concurrent git clones
- Add semaphore to limit concurrency
- Show progress bar during fetch
```

## Pull Requests

- Keep PRs focused on a single change
- Update documentation if needed
- Add tests for bug fixes and new features
- Ensure CI passes

## Areas for Contribution

### Good First Issues

- Improve error messages
- Add more tests
- Documentation improvements
- CLI help text enhancements

### Larger Projects

- Swift Package Registry support
- Workspace/monorepo support
- Plugin system
- Performance optimizations
- Linux packaging

## Running Benchmarks

```console
$ cargo build --release
$ ./scripts/benchmark.sh
```

## Questions?

Open an issue for questions, bug reports, or feature requests.

## License

By contributing, you agree that your contributions will be licensed under the MIT License.
