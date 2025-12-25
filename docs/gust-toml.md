# Gust.toml Format

Gust.toml is a simpler alternative to Package.swift for defining Swift packages.

## Basic Example

```toml
[package]
name = "myapp"
version = "1.0.0"

[dependencies]
swift-log = { git = "https://github.com/apple/swift-log.git", tag = "1.5.0" }
swift-nio = { git = "https://github.com/apple/swift-nio.git", tag = "2.60.0" }
```

## Package Section

```toml
[package]
name = "myapp"                    # Required
version = "1.0.0"                 # Required
swift-tools-version = "5.9"       # Optional, defaults to 5.9

# Optional metadata
authors = ["Your Name"]
license = "MIT"
description = "My awesome app"
repository = "https://github.com/you/myapp"
```

## Dependencies

### Git Dependencies

```toml
[dependencies]
# With tag (recommended)
swift-log = { git = "https://github.com/apple/swift-log.git", tag = "1.5.0" }

# With branch
vapor = { git = "https://github.com/vapor/vapor.git", branch = "main" }

# With specific commit
my-fork = { git = "https://github.com/me/fork.git", rev = "abc123" }
```

### Path Dependencies

```toml
[dependencies]
my-local-lib = { path = "../my-local-lib" }
```

### Version Constraints

```toml
[constraints]
swift-log = ">=1.4, <2.0"
swift-nio = "^2.50"
```

### Overrides

Force specific versions, ignoring other constraints:

```toml
[overrides]
swift-log = "1.5.4"
```

## Targets

```toml
[[target]]
name = "myapp"
type = "executable"
path = "Sources/myapp"
dependencies = ["swift-log", "swift-nio"]

[[target]]
name = "mylib"
type = "library"
path = "Sources/mylib"
```

**Target types:**
- `executable` - Builds an executable binary
- `library` - Builds a library
- `test` - Test target

## Dev Dependencies

Dependencies only needed for development/testing:

```toml
[dev-dependencies]
swift-testing = { git = "https://github.com/apple/swift-testing.git", tag = "0.1.0" }
```

## Platforms

```toml
[platforms]
macOS = "12.0"
iOS = "15.0"
```

## Complete Example

```toml
[package]
name = "vapor-app"
version = "1.0.0"
swift-tools-version = "5.9"
license = "MIT"

[dependencies]
vapor = { git = "https://github.com/vapor/vapor.git", tag = "4.90.0" }
fluent = { git = "https://github.com/vapor/fluent.git", tag = "4.9.0" }
fluent-postgres-driver = { git = "https://github.com/vapor/fluent-postgres-driver.git", tag = "2.8.0" }

[dev-dependencies]
vapor-testing = { git = "https://github.com/vapor/vapor.git", tag = "4.90.0" }

[[target]]
name = "App"
type = "executable"
dependencies = ["vapor", "fluent", "fluent-postgres-driver"]

[[target]]
name = "AppTests"
type = "test"
dependencies = ["App", "vapor-testing"]

[platforms]
macOS = "12.0"
```
