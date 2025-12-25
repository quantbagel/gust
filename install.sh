#!/bin/bash
# Gust installer script
# Usage: curl -fsSL https://quantbagel.vercel.app/gust/install.sh | sh
#
# Environment variables:
#   GUST_INSTALL_DIR    - Installation directory (default: ~/.gust/bin)
#   GUST_VERSION        - Specific version to install (default: latest)
#   GUST_NO_MODIFY_PATH - Set to 1 to skip modifying shell config

set -euo pipefail

REPO="quantbagel/gust"
INSTALL_DIR="${GUST_INSTALL_DIR:-$HOME/.gust/bin}"
NO_MODIFY_PATH="${GUST_NO_MODIFY_PATH:-0}"

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
                aarch64) echo "aarch64-unknown-linux-gnu" ;;
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

# Add PATH to shell config file
add_to_path() {
    local config_file="$1"
    local path_line="export PATH=\"$INSTALL_DIR:\$PATH\""

    # Check if already added
    if [ -f "$config_file" ] && grep -q "$INSTALL_DIR" "$config_file" 2>/dev/null; then
        return 0
    fi

    # Add to config
    echo "" >> "$config_file"
    echo "# Added by Gust installer" >> "$config_file"
    echo "$path_line" >> "$config_file"

    return 1
}

# Add PATH for fish shell
add_to_fish_path() {
    local config_file="$1"
    local path_line="fish_add_path $INSTALL_DIR"

    # Check if already added
    if [ -f "$config_file" ] && grep -q "$INSTALL_DIR" "$config_file" 2>/dev/null; then
        return 0
    fi

    mkdir -p "$(dirname "$config_file")"
    echo "" >> "$config_file"
    echo "# Added by Gust installer" >> "$config_file"
    echo "$path_line" >> "$config_file"

    return 1
}

# Update shell configurations
update_shell_configs() {
    local modified=0

    # Bash
    if [ -f "$HOME/.bashrc" ]; then
        if ! add_to_path "$HOME/.bashrc"; then
            modified=1
        fi
    fi

    if [ -f "$HOME/.bash_profile" ]; then
        if ! add_to_path "$HOME/.bash_profile"; then
            modified=1
        fi
    elif [ -f "$HOME/.profile" ]; then
        if ! add_to_path "$HOME/.profile"; then
            modified=1
        fi
    fi

    # Zsh
    if [ -f "$HOME/.zshrc" ] || [ "$SHELL" = */zsh ]; then
        if ! add_to_path "$HOME/.zshrc"; then
            modified=1
        fi
    fi

    # Also update .zshenv for non-interactive shells
    if [ -f "$HOME/.zshenv" ]; then
        if ! add_to_path "$HOME/.zshenv"; then
            modified=1
        fi
    fi

    # Fish
    if [ -d "$HOME/.config/fish" ] || [ "$SHELL" = */fish ]; then
        if ! add_to_fish_path "$HOME/.config/fish/config.fish"; then
            modified=1
        fi
    fi

    return $modified
}

# Remove old installations
cleanup_old_installations() {
    # Remove from cargo bin if exists
    if [ -f "$HOME/.cargo/bin/gust" ]; then
        info "Removing old installation from ~/.cargo/bin/gust"
        rm -f "$HOME/.cargo/bin/gust"
    fi
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

    # Clean up old installations
    cleanup_old_installations

    # Create install directory
    mkdir -p "$INSTALL_DIR"

    # Download and extract
    local download_url="https://github.com/$REPO/releases/download/v${version}/gust-${platform}.tar.gz"
    info "Downloading from: $download_url"

    if ! curl -fsSL "$download_url" | tar xz -C "$INSTALL_DIR"; then
        error "Failed to download or extract Gust"
    fi

    chmod +x "$INSTALL_DIR/gust"

    # Install completions if they exist in the tarball
    if [ -d "$INSTALL_DIR/completions" ]; then
        info "Installing shell completions..."

        # Bash completions
        if [ -f "$INSTALL_DIR/completions/gust.bash" ]; then
            local bash_comp_dir="${BASH_COMPLETION_DIR:-$HOME/.local/share/bash-completion/completions}"
            mkdir -p "$bash_comp_dir"
            cp "$INSTALL_DIR/completions/gust.bash" "$bash_comp_dir/gust"
        fi

        # Zsh completions
        if [ -f "$INSTALL_DIR/completions/_gust" ]; then
            local zsh_comp_dir="${ZSH_COMPLETION_DIR:-$HOME/.local/share/zsh/site-functions}"
            mkdir -p "$zsh_comp_dir"
            cp "$INSTALL_DIR/completions/_gust" "$zsh_comp_dir/_gust"
        fi

        # Fish completions
        if [ -f "$INSTALL_DIR/completions/gust.fish" ]; then
            local fish_comp_dir="${FISH_COMPLETION_DIR:-$HOME/.config/fish/completions}"
            mkdir -p "$fish_comp_dir"
            cp "$INSTALL_DIR/completions/gust.fish" "$fish_comp_dir/gust.fish"
        fi

        rm -rf "$INSTALL_DIR/completions"
    fi

    # Install man page if it exists in the tarball
    if [ -d "$INSTALL_DIR/man" ]; then
        info "Installing man page..."
        local man_dir="${MAN_DIR:-$HOME/.local/share/man/man1}"
        mkdir -p "$man_dir"
        cp "$INSTALL_DIR/man/man1/gust.1" "$man_dir/"
        rm -rf "$INSTALL_DIR/man"
    fi

    # Verify installation
    if ! "$INSTALL_DIR/gust" --version > /dev/null 2>&1; then
        error "Installation verification failed"
    fi

    local installed_version
    installed_version=$("$INSTALL_DIR/gust" --version | awk '{print $2}')

    echo ""
    success "Gust $installed_version installed successfully!"

    # Update PATH in shell configs (unless disabled)
    if [ "$NO_MODIFY_PATH" != "1" ]; then
        if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
            info "Updating shell configuration..."
            if update_shell_configs; then
                info "PATH already configured"
            else
                success "Added Gust to PATH in shell config"
                echo ""
                info "Restart your shell or run:"
                echo ""
                echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
                echo ""
            fi
        else
            info "Gust is already in your PATH"
        fi
    else
        # Manual instructions if auto-update disabled
        if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
            echo ""
            warn "Add Gust to your PATH by adding this to your shell config:"
            echo ""
            echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
            echo ""
        fi
    fi

    echo ""
    info "Run 'gust --help' to get started"
}

main "$@"
