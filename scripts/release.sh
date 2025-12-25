#!/bin/bash
# Release script for Gust
# Usage: ./scripts/release.sh <version>
# Example: ./scripts/release.sh 0.2.5

set -euo pipefail

VERSION="${1:-}"

if [ -z "$VERSION" ]; then
    echo "Usage: $0 <version>"
    echo "Example: $0 0.2.5"
    exit 1
fi

# Validate version format
if ! [[ "$VERSION" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
    echo "Error: Version must be in format X.Y.Z (e.g., 0.2.5)"
    exit 1
fi

# Check for uncommitted changes
if ! git diff --quiet || ! git diff --staged --quiet; then
    echo "Error: You have uncommitted changes. Commit or stash them first."
    exit 1
fi

# Check we're on main branch
BRANCH=$(git rev-parse --abbrev-ref HEAD)
if [ "$BRANCH" != "main" ]; then
    echo "Error: Must be on main branch (currently on $BRANCH)"
    exit 1
fi

# Pull latest
echo "Pulling latest changes..."
git pull origin main

# Update version in Cargo.toml
echo "Updating version to $VERSION..."
sed -i '' "s/^version = \".*\"/version = \"$VERSION\"/" Cargo.toml

# Verify the change
if ! grep -q "version = \"$VERSION\"" Cargo.toml; then
    echo "Error: Failed to update version in Cargo.toml"
    exit 1
fi

# Commit
git add Cargo.toml
git commit -m "Release v$VERSION"

# Create and push tag
echo "Creating tag v$VERSION..."
git tag "v$VERSION"

# Push commit and tag
echo "Pushing to origin..."
git push origin main
git push origin "v$VERSION"

echo ""
echo "✓ Released v$VERSION"
echo "  View release: https://github.com/quantbagel/gust/releases/tag/v$VERSION"
echo "  View workflow: https://github.com/quantbagel/gust/actions"
