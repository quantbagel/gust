# Installation

## Quick Install (macOS/Linux)

```sh
curl -fsSL quantbagel.vercel.app/gust/install.sh | sh
```

This installs to `~/.gust/bin`. Add it to your PATH:

```sh
export PATH="$HOME/.gust/bin:$PATH"
```

## Homebrew

```sh
brew tap quantbagel/tap
brew install gust
```

## Cargo

```sh
cargo install gust
```

## From Source

```sh
git clone https://github.com/quantbagel/gust
cd gust
cargo install --path crates/gust
```

## GitHub Releases

Download pre-built binaries from [GitHub Releases](https://github.com/quantbagel/gust/releases):

- `gust-x86_64-apple-darwin.tar.gz` - macOS Intel
- `gust-aarch64-apple-darwin.tar.gz` - macOS Apple Silicon
- `gust-x86_64-unknown-linux-gnu.tar.gz` - Linux x64
- `gust-aarch64-unknown-linux-gnu.tar.gz` - Linux ARM64

## GitHub Actions

```yaml
- uses: quantbagel/gust/action@v0.2
  with:
    version: latest
```

## Requirements

- macOS 12+ or Linux
- Swift 5.9+ (for building Swift packages)

## Verify Installation

```sh
gust --version
```
