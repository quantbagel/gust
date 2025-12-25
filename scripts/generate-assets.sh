#!/usr/bin/env bash
# Generate shell completions and man pages for gust
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"
OUTPUT_DIR="${1:-$PROJECT_ROOT/assets}"

# Build gust first
echo "Building gust..."
cargo build --release -p gust

GUST_BIN="$PROJECT_ROOT/target/release/gust"

# Create output directories
mkdir -p "$OUTPUT_DIR/completions/bash"
mkdir -p "$OUTPUT_DIR/completions/zsh"
mkdir -p "$OUTPUT_DIR/completions/fish"
mkdir -p "$OUTPUT_DIR/man/man1"

# Generate shell completions
echo "Generating shell completions..."
"$GUST_BIN" completions bash > "$OUTPUT_DIR/completions/bash/gust"
"$GUST_BIN" completions zsh > "$OUTPUT_DIR/completions/zsh/_gust"
"$GUST_BIN" completions fish > "$OUTPUT_DIR/completions/fish/gust.fish"

# Generate man page
echo "Generating man page..."
"$GUST_BIN" manpage > "$OUTPUT_DIR/man/man1/gust.1"

echo "Assets generated in $OUTPUT_DIR:"
find "$OUTPUT_DIR" -type f | sort
