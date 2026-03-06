#!/bin/bash

set -e

GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m'

INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="linear-claude"
REPO="Blightwidow/linear-claude"

# Detect platform
detect_target() {
    local arch os target

    arch=$(uname -m)
    os=$(uname -s)

    case "$os" in
        Darwin) os="apple-darwin" ;;
        Linux)  os="unknown-linux-gnu" ;;
        *)
            echo -e "${RED}Unsupported OS: $os${NC}" >&2
            exit 1
            ;;
    esac

    case "$arch" in
        x86_64|amd64) arch="x86_64" ;;
        arm64|aarch64) arch="aarch64" ;;
        *)
            echo -e "${RED}Unsupported architecture: $arch${NC}" >&2
            exit 1
            ;;
    esac

    echo "${arch}-${os}"
}

echo "Installing Linear Claude..."

TARGET=$(detect_target)
echo "Detected platform: $TARGET"

# Fetch latest release info
echo "Fetching latest release..."
RELEASE_JSON=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" 2>/dev/null) || {
    echo -e "${RED}Failed to fetch release information${NC}" >&2
    exit 1
}

DOWNLOAD_URL=$(echo "$RELEASE_JSON" | grep -o '"browser_download_url":[^,]*' | grep "$TARGET\"" | head -1 | cut -d'"' -f4)
CHECKSUM_URL=$(echo "$RELEASE_JSON" | grep -o '"browser_download_url":[^,]*' | grep "${TARGET}.sha256\"" | head -1 | cut -d'"' -f4)

if [ -z "$DOWNLOAD_URL" ]; then
    echo -e "${RED}No binary found for platform: $TARGET${NC}" >&2
    echo "Available assets:" >&2
    echo "$RELEASE_JSON" | grep '"name"' | head -10 >&2
    exit 1
fi

mkdir -p "$INSTALL_DIR"

# Download binary
echo "Downloading $BINARY_NAME for $TARGET..."
if ! curl -fsSL "$DOWNLOAD_URL" -o "$INSTALL_DIR/$BINARY_NAME"; then
    echo -e "${RED}Failed to download binary${NC}" >&2
    exit 1
fi

# Verify checksum if available
if [ -n "$CHECKSUM_URL" ]; then
    echo "Verifying checksum..."
    EXPECTED_SHA=$(curl -fsSL "$CHECKSUM_URL" 2>/dev/null | awk '{print $1}')
    if command -v shasum >/dev/null 2>&1; then
        ACTUAL_SHA=$(shasum -a 256 "$INSTALL_DIR/$BINARY_NAME" | awk '{print $1}')
    elif command -v sha256sum >/dev/null 2>&1; then
        ACTUAL_SHA=$(sha256sum "$INSTALL_DIR/$BINARY_NAME" | awk '{print $1}')
    fi

    if [ -n "$ACTUAL_SHA" ] && [ "$EXPECTED_SHA" != "$ACTUAL_SHA" ]; then
        echo -e "${RED}Checksum verification failed!${NC}" >&2
        echo "  Expected: $EXPECTED_SHA" >&2
        echo "  Got:      $ACTUAL_SHA" >&2
        rm -f "$INSTALL_DIR/$BINARY_NAME"
        exit 1
    fi
    echo -e "${GREEN}Checksum verified${NC}"
fi

chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo -e "${GREEN}$BINARY_NAME installed to $INSTALL_DIR/$BINARY_NAME${NC}"

# Check if install directory is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${YELLOW}Warning: $INSTALL_DIR is not in your PATH${NC}"
    echo ""
    echo "Add it to your PATH by adding this line to your shell profile:"
    echo ""
    if [[ "$SHELL" == *"zsh"* ]]; then
        echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.zshrc"
        echo "  source ~/.zshrc"
    elif [[ "$SHELL" == *"bash"* ]]; then
        echo "  echo 'export PATH=\"\$HOME/.local/bin:\$PATH\"' >> ~/.bashrc"
        echo "  source ~/.bashrc"
    else
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
    fi
    echo ""
fi

# Check for required dependency
echo ""
echo "Checking dependencies..."

if command -v claude &> /dev/null; then
    echo -e "${GREEN}Claude Code CLI found${NC}"
else
    echo -e "${YELLOW}Warning: Claude Code CLI not found${NC}"
    echo "  Install from: https://claude.ai/code"
fi

echo ""
echo -e "${GREEN}Installation complete!${NC}"
echo ""
echo "Get started with:"
echo "  $BINARY_NAME view \"https://linear.app/team/view/your-view-id\""
echo ""
echo "For more information, visit: https://github.com/$REPO"
