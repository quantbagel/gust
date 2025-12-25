# Workspaces

Workspaces allow you to manage multiple related packages in a single repository (monorepo).

## Setup

Create a `Gust.toml` at the repository root with a `[workspace]` section:

```toml
[workspace]
members = [
    "packages/*",
    "apps/cli",
    "apps/server"
]
exclude = ["packages/deprecated-*"]

[workspace.dependencies]
swift-log = { git = "https://github.com/apple/swift-log.git", tag = "1.5.0" }
swift-nio = { git = "https://github.com/apple/swift-nio.git", tag = "2.60.0" }

[workspace.package-defaults]
swift-tools-version = "5.9"
license = "MIT"
```

## Directory Structure

```
my-monorepo/
├── Gust.toml              # Workspace root
├── packages/
│   ├── core/
│   │   ├── Gust.toml      # Member package
│   │   └── Sources/
│   └── utils/
│       ├── Gust.toml
│       └── Sources/
└── apps/
    ├── cli/
    │   ├── Gust.toml
    │   └── Sources/
    └── server/
        ├── Gust.toml
        └── Sources/
```

## Member Packages

Member packages can inherit dependencies from the workspace:

```toml
# packages/core/Gust.toml
[package]
name = "core"
version = "1.0.0"

[dependencies]
swift-log = { workspace = true }  # Inherits from workspace
```

## Shared Dependencies

Define dependencies once in the workspace root:

```toml
# Root Gust.toml
[workspace.dependencies]
swift-log = { git = "https://github.com/apple/swift-log.git", tag = "1.5.0" }
vapor = { git = "https://github.com/vapor/vapor.git", tag = "4.90.0" }
```

Members reference them with `workspace = true`:

```toml
# packages/server/Gust.toml
[dependencies]
swift-log = { workspace = true }
vapor = { workspace = true }
```

## Package Defaults

Set default values for all member packages:

```toml
[workspace.package-defaults]
swift-tools-version = "5.9"
license = "MIT"
authors = ["Team Name"]
```

## Commands

Workspace-aware commands:

```sh
# Install dependencies for all members
gust install

# Build all members
gust build

# Build specific member
gust build -p core

# Run tests for all members
gust test

# Show combined dependency tree
gust tree
```

## Member Filtering

```sh
# Only specific members
gust build -p core -p utils

# Exclude members
gust build --exclude deprecated-pkg
```

## Inter-package Dependencies

Members can depend on each other:

```toml
# apps/cli/Gust.toml
[dependencies]
core = { path = "../../packages/core" }
utils = { path = "../../packages/utils" }
```

Gust automatically detects and builds dependencies in the correct order.
