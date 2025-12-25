#!/bin/bash
# Gust installer script
# Usage: curl -fsSL https://raw.githubusercontent.com/quantbagel/gust/main/install.sh | bash

set -euo pipefail

REPO="quantbagel/gust"
INSTALL_DIR="${GUST_INSTALL_DIR:-$HOME/.gust/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${BLUE}==>${NC} $1"
}

success() {
    echo -e "${GREEN}==>${NC} $1"
}

warn() {
    echo -e "${YELLOW}==>${NC} $1"
}

error() {
    echo -e "${RED}==>${NC} $1" >&2
    exit 1
}

detect_platform() {
    local os arch

    os="$(uname -s)"
    arch="$(uname -m)"

    case "$os" in
        Linux)
            case "$arch" in
                x86_64) echo "x86_64-unknown-linux-gnu" ;;
                *) error "Unsupported Linux architecture: $arch" ;;
            esac
            ;;
        Darwin)
            case "$arch" in
                x86_64) echo "x86_64-apple-darwin" ;;
                arm64) echo "aarch64-apple-darwin" ;;
                *) error "Unsupported macOS architecture: $arch" ;;
            esac
            ;;
        *)
            error "Unsupported operating system: $os"
            ;;
    esac
}

get_latest_version() {
    local version
    version=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" | grep '"tag_name"' | sed -E 's/.*"v([^"]+)".*/\1/')

    if [ -z "$version" ]; then
        error "Failed to determine latest version"
    fi

    echo "$version"
}

main() {
    info "Installing Gust - A blazing fast Swift package manager"
    echo ""

    # Detect platform
    local platform
    platform=$(detect_platform)
    info "Detected platform: $platform"

    # Get version
    local version="${GUST_VERSION:-}"
    if [ -z "$version" ]; then
        info "Fetching latest version..."
        version=$(get_latest_version)
    fi
    info "Installing version: $version"

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Download and extract
    local download_url="https://github.com/$REPO/releases/download/v${version}/gust-${platform}.tar.gz"
    info "Downloading from: $download_url"

    if ! curl -fsSL "$download_url" | tar xz -C "$INSTALL_DIR"; then
        error "Failed to download or extract Gust"
    fi

    chmod +x "$INSTALL_DIR/gust"

    # Verify installation
    if ! "$INSTALL_DIR/gust" --version > /dev/null 2>&1; then
        error "Installation verification failed"
    fi

    local installed_version
    installed_version=$("$INSTALL_DIR/gust" --version | awk '{print $2}')

    echo ""
    success "Gust $installed_version installed successfully!"
    echo ""

    # Check if in PATH
    if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
        warn "Add Gust to your PATH by adding this to your shell config:"
        echo ""
        echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
        echo ""

        # Detect shell and suggest file
        local shell_config=""
        case "$SHELL" in
            */zsh) shell_config="~/.zshrc" ;;
            */bash)
                if [ -f "$HOME/.bash_profile" ]; then
                    shell_config="~/.bash_profile"
                else
                    shell_config="~/.bashrc"
                fi
                ;;
            */fish)
                echo "  Or for fish shell:"
                echo "  set -gx PATH $INSTALL_DIR \$PATH"
                shell_config="~/.config/fish/config.fish"
                ;;
        esac

        if [ -n "$shell_config" ]; then
            echo "  Add it to: $shell_config"
        fi
        echo ""
    else
        info "Gust is already in your PATH"
    fi

    info "Run 'gust --help' to get started"
}

main "$@"
