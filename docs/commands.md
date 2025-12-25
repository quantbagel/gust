# Commands

## Package Management

### `gust new <name>`

Create a new Swift package.

```sh
gust new myapp              # Create executable
gust new mylib --type lib   # Create library
```

**Options:**
- `--type <exe|lib>` - Package type (default: exe)

### `gust init`

Initialize a package in the current directory.

```sh
gust init
gust init --type lib
```

### `gust add <package>`

Add a dependency.

```sh
# From git with tag
gust add swift-log --git https://github.com/apple/swift-log.git --tag 1.5.0

# From git with branch
gust add vapor --git https://github.com/vapor/vapor.git --branch main

# From local path
gust add my-lib --path ../my-lib
```

**Options:**
- `--git <url>` - Git repository URL
- `--tag <tag>` - Git tag
- `--branch <branch>` - Git branch
- `--rev <sha>` - Git commit SHA
- `--path <path>` - Local path

### `gust remove <package>`

Remove a dependency.

```sh
gust remove swift-log
```

## Dependency Resolution

### `gust install`

Install all dependencies.

```sh
gust install           # Normal install
gust install --frozen  # Use exact versions from lockfile
```

**Options:**
- `--frozen` - Don't update lockfile, fail if out of sync

### `gust update`

Update dependencies.

```sh
gust update              # Update all
gust update swift-log    # Update specific package
gust update --breaking   # Allow breaking version updates
```

**Options:**
- `--breaking` - Allow major version updates

### `gust outdated`

Check for outdated packages.

```sh
gust outdated
```

### `gust tree`

Show dependency tree.

```sh
gust tree              # Full tree
gust tree --depth 2    # Limit depth
gust tree --duplicates # Show only duplicates
```

**Options:**
- `--depth <n>` - Maximum depth to display
- `--duplicates` - Only show duplicate dependencies

## Building

### `gust build`

Build the package.

```sh
gust build             # Debug build
gust build --release   # Release build
```

**Options:**
- `--release` - Build in release mode
- `--jobs <n>` - Number of parallel jobs

### `gust run`

Run the executable.

```sh
gust run
gust run -- --arg1 --arg2   # Pass arguments
```

### `gust test`

Run tests.

```sh
gust test
gust test --filter MyTest   # Filter tests
```

**Options:**
- `--filter <pattern>` - Run matching tests only

### `gust clean`

Clean build artifacts.

```sh
gust clean             # Clean build artifacts
gust clean --deps      # Also clean dependencies
```

**Options:**
- `--deps` - Also remove fetched dependencies

## Utilities

### `gust search <query>`

Search for Swift packages.

```sh
gust search logging
gust search vapor --limit 20
```

**Options:**
- `--limit <n>` - Maximum results (default: 10)

### `gust cache stats`

Show cache statistics.

```sh
gust cache stats
```

### `gust migrate`

Convert Package.swift to Gust.toml.

```sh
gust migrate
```

### `gust generate`

Generate Package.swift from Gust.toml.

```sh
gust generate
```

Note: Package.swift is auto-generated when you run `gust build` or `gust install`, so you typically don't need to run this manually. It's useful for:
- Generating Package.swift for IDE support (Xcode, VSCode)
- Debugging the generated manifest
- Projects that need to commit Package.swift

### `gust self update`

Update gust to the latest version.

```sh
gust self update
```

Gust automatically checks for updates once per day and shows a notification if a new version is available. Use this command to install the update.

## Global Options

These options work with any command:

- `-v, --verbose` - Increase verbosity (use -vv or -vvv for more)
- `--quiet` - Suppress all output
- `--no-color` - Disable colored output
- `--manifest <path>` - Path to manifest file
- `--jobs <n>` - Number of parallel jobs
