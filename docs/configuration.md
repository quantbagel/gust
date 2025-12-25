# Configuration

Gust can be configured through the `Gust.toml` manifest file.

## Gust.toml

See [gust-toml.md](gust-toml.md) for the full format specification.

## Environment Variables

### `GUST_INSTALL_DIR`

Override the installation directory for the install script.

```sh
GUST_INSTALL_DIR=/usr/local/bin curl -fsSL quantbagel.vercel.app/gust/install.sh | sh
```

### `GUST_CACHE_DIR`

Override the cache directory location.

```sh
export GUST_CACHE_DIR=/path/to/cache
```

Default: `~/.gust/cache`

### `GUST_JOBS`

Default number of parallel jobs.

```sh
export GUST_JOBS=8
```

### `NO_COLOR`

Disable colored output.

```sh
export NO_COLOR=1
```

## Cache Location

By default, Gust stores its cache at:

- **macOS**: `~/Library/Caches/gust`
- **Linux**: `~/.cache/gust`

The cache contains:

- `git/` - Cloned git repositories
- `artifacts/` - Compiled binary artifacts
- `checksums/` - Content hashes for deduplication
