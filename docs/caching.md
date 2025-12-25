# Caching

Gust uses aggressive caching to achieve its speed improvements over SwiftPM.

## Cache Types

### Git Repository Cache

Cloned repositories are stored globally and shared across all projects.

**Location:** `~/.gust/cache/git/`

When you add a dependency, Gust:
1. Checks if the repository is already cached
2. Fetches only new commits if cached
3. Creates hard links to the project's dependency folder

### Content-Addressable Store

Files are deduplicated using BLAKE3 content hashing.

**Location:** `~/.gust/cache/cas/`

If two packages contain identical files, they're stored only once on disk.

### Binary Artifact Cache

Compiled Swift modules are cached to skip rebuilding unchanged dependencies.

**Location:** `~/.gust/cache/artifacts/`

Cache keys include:
- Swift version
- Build configuration (debug/release)
- Platform and architecture
- Source file hashes
- Compiler flags

## How It Works

### Hard Links

Instead of copying files, Gust creates hard links from the cache to your project:

```
~/.gust/cache/cas/abc123... (actual file, stored once)
    ↓ hard link
~/myproject/.gust/deps/swift-log/Sources/Logging.swift
    ↓ hard link
~/otherproject/.gust/deps/swift-log/Sources/Logging.swift
```

Benefits:
- No duplicate disk usage
- Instant "copies"
- Changes in one project don't affect others (copy-on-write)

### Parallel Fetching

Gust clones multiple repositories simultaneously:

```
Fetching swift-log... ━━━━━━━━━━━━━━━━━━━━━━ 100%
Fetching swift-nio... ━━━━━━━━━━━━━━ 70%
Fetching vapor...     ━━━━━━━ 35%
```

Default concurrency: 8 parallel clones (configurable with `--jobs`).

## Cache Management

### View Cache Stats

```sh
gust cache stats
```

Output:
```
Cache Statistics
────────────────
Git repositories: 47 (1.2 GB)
Content store:    2,341 files (450 MB deduplicated)
Binary artifacts: 128 (890 MB)
Total disk usage: 2.5 GB
```

### Clean Cache

```sh
# Remove unused cached items (older than 30 days)
gust cache clean

# Remove all cached items
gust cache clean --all

# Remove only binary artifacts
gust cache clean --artifacts
```

### Cache Location

Override the default cache location:

```sh
export GUST_CACHE_DIR=/path/to/cache
```

## Lockfile

The `Gust.lock` file ensures reproducible builds:

```toml
version = 2

[[package]]
name = "swift-log"
version = "1.5.4"
source = "git"
git = "https://github.com/apple/swift-log.git"
revision = "e9d49cbf6b5f691e0072eb89a63de9f7d0a1cbb2"
content-hash = "blake3:abc123..."
```

### Frozen Installs

For CI/CD, use `--frozen` to ensure exact versions:

```sh
gust install --frozen
```

This fails if `Gust.lock` is out of sync with `Gust.toml`.

## Performance Tips

1. **Keep the cache warm** - Don't clean unnecessarily
2. **Use `--frozen` in CI** - Avoids resolution overhead
3. **Share cache in CI** - Cache `~/.gust/cache` between runs
4. **Use binary caching** - Enabled by default for release builds

### GitHub Actions Example

```yaml
- name: Cache Gust
  uses: actions/cache@v4
  with:
    path: ~/.gust/cache
    key: gust-${{ hashFiles('Gust.lock') }}
    restore-keys: gust-

- name: Install dependencies
  run: gust install --frozen
```
