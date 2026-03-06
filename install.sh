#!/bin/bash

set -e

# Colors for output
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"
BINARY_NAME="linear-claude"
REPO_URL="https://raw.githubusercontent.com/Blightwidow/linear-claude/main"

echo "🔂 Installing Linear Claude..."

# Create install directory if it doesn't exist
mkdir -p "$INSTALL_DIR"

# Download the script
echo "📥 Downloading $BINARY_NAME..."
if ! curl -fsSL "$REPO_URL/linear_claude.sh" -o "$INSTALL_DIR/$BINARY_NAME"; then
    echo -e "${RED}❌ Failed to download $BINARY_NAME${NC}" >&2
    exit 1
fi

# Make it executable
chmod +x "$INSTALL_DIR/$BINARY_NAME"

echo -e "${GREEN}✅ $BINARY_NAME installed to $INSTALL_DIR/$BINARY_NAME${NC}"

# Check if install directory is in PATH
if [[ ":$PATH:" != *":$INSTALL_DIR:"* ]]; then
    echo -e "${YELLOW}⚠️  Warning: $INSTALL_DIR is not in your PATH${NC}"
    echo ""
    echo "To add it to your PATH, add this line to your shell profile:"
    echo ""
    
    # Detect shell
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

# Check for dependencies
echo ""
echo "🔍 Checking dependencies..."

missing_deps=()

if ! command -v claude &> /dev/null; then
    missing_deps+=("Claude Code CLI")
fi

if ! command -v gh &> /dev/null; then
    missing_deps+=("GitHub CLI")
fi

if ! command -v jq &> /dev/null; then
    missing_deps+=("jq")
fi

if ! command -v linear &> /dev/null; then
    missing_deps+=("Linear CLI")
fi

if [ ${#missing_deps[@]} -eq 0 ]; then
    echo -e "${GREEN}✅ All dependencies installed${NC}"
else
    echo -e "${YELLOW}⚠️  Missing dependencies:${NC}"
    for dep in "${missing_deps[@]}"; do
        echo "   - $dep"
    done
    echo ""
    echo "Install them with:"
    if [[ "$OSTYPE" == "darwin"* ]]; then
        echo "  brew install gh jq schpet/tap/linear"
        echo "  brew install --cask claude-code"
    elif [[ "$OSTYPE" == "linux-gnu"* ]]; then
        echo "  # Install GitHub CLI: https://github.com/cli/cli#installation"
        echo "  sudo apt-get install jq  # or equivalent for your distro"
        echo "  # Install Linear CLI: https://github.com/schpet/linear"
        echo "  # Install Claude Code CLI: https://code.claude.com"
    fi
fi

echo ""
echo -e "${GREEN}🎉 Installation complete!${NC}"
echo ""
echo "Get started with:"
echo "  $BINARY_NAME view \"https://linear.app/team/view/your-view-id\""
echo ""
echo "For more information, visit: https://github.com/Blightwidow/linear-claude"
