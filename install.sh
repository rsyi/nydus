#!/usr/bin/env bash
set -e

# Nydus installation script
# Usage: curl -fsSL https://raw.githubusercontent.com/oxy-hq/nydus/main/install.sh | bash

REPO="oxy-hq/nydus"
INSTALL_DIR="${INSTALL_DIR:-$HOME/.local/bin}"

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

echo -e "${BLUE}Installing nydus...${NC}"
echo ""

# Detect OS
OS="$(uname -s)"
case "${OS}" in
    Linux*)     PLATFORM=linux;;
    Darwin*)    PLATFORM=macos;;
    *)
        echo -e "${RED}Unsupported OS: ${OS}${NC}"
        exit 1
        ;;
esac

# Detect architecture
ARCH="$(uname -m)"
case "${ARCH}" in
    x86_64)     ARCH_NAME=x86_64;;
    aarch64)    ARCH_NAME=aarch64;;
    arm64)      ARCH_NAME=aarch64;;
    *)
        echo -e "${RED}Unsupported architecture: ${ARCH}${NC}"
        exit 1
        ;;
esac

BINARY_NAME="nydus-${PLATFORM}-${ARCH_NAME}"
DOWNLOAD_URL="https://github.com/${REPO}/releases/latest/download/${BINARY_NAME}"

echo -e "Platform: ${GREEN}${PLATFORM}${NC}"
echo -e "Architecture: ${GREEN}${ARCH_NAME}${NC}"
echo -e "Binary: ${GREEN}${BINARY_NAME}${NC}"
echo ""

# Create temp directory
TMP_DIR="$(mktemp -d)"
trap "rm -rf ${TMP_DIR}" EXIT

# Download binary
echo -e "${BLUE}Downloading from ${DOWNLOAD_URL}...${NC}"
if command -v curl >/dev/null 2>&1; then
    curl -fsSL "${DOWNLOAD_URL}" -o "${TMP_DIR}/nydus"
elif command -v wget >/dev/null 2>&1; then
    wget -q "${DOWNLOAD_URL}" -O "${TMP_DIR}/nydus"
else
    echo -e "${RED}Error: curl or wget is required${NC}"
    exit 1
fi

# Make executable
chmod +x "${TMP_DIR}/nydus"

# Determine install location
if [[ -w "/usr/local/bin" ]]; then
    # Can write to /usr/local/bin without sudo
    INSTALL_DIR="/usr/local/bin"
    echo -e "${BLUE}Installing to /usr/local/bin...${NC}"
    mv "${TMP_DIR}/nydus" "/usr/local/bin/nydus"
elif [[ "${EUID}" -eq 0 ]]; then
    # Running as root
    INSTALL_DIR="/usr/local/bin"
    echo -e "${BLUE}Installing to /usr/local/bin...${NC}"
    mv "${TMP_DIR}/nydus" "/usr/local/bin/nydus"
else
    # Install to user directory
    mkdir -p "${INSTALL_DIR}"
    echo -e "${BLUE}Installing to ${INSTALL_DIR}...${NC}"
    mv "${TMP_DIR}/nydus" "${INSTALL_DIR}/nydus"

    # Check if directory is in PATH
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        echo ""
        echo -e "${YELLOW}Warning: ${INSTALL_DIR} is not in your PATH${NC}"
        echo -e "Add this line to your shell config (~/.bashrc, ~/.zshrc, etc.):"
        echo -e "${GREEN}export PATH=\"\$PATH:${INSTALL_DIR}\"${NC}"
        echo ""
    fi
fi

# Verify installation
if command -v nydus >/dev/null 2>&1; then
    VERSION="$(nydus --version 2>&1 || echo 'unknown')"
    echo ""
    echo -e "${GREEN}✓ nydus installed successfully!${NC}"
    echo -e "Version: ${VERSION}"
    echo ""
    echo -e "Get started:"
    echo -e "  ${BLUE}nydus init${NC}     # Initialize configuration"
    echo -e "  ${BLUE}nydus up <name>${NC} # Create a new instance"
    echo -e "  ${BLUE}nydus --help${NC}   # See all commands"
    echo ""
else
    echo ""
    echo -e "${GREEN}✓ nydus installed to ${INSTALL_DIR}/nydus${NC}"
    echo ""
    if [[ ":$PATH:" != *":${INSTALL_DIR}:"* ]]; then
        echo -e "${YELLOW}To use nydus, either:${NC}"
        echo -e "1. Add to PATH: ${GREEN}export PATH=\"\$PATH:${INSTALL_DIR}\"${NC}"
        echo -e "2. Or run: ${GREEN}${INSTALL_DIR}/nydus${NC}"
    else
        echo -e "Run ${BLUE}nydus init${NC} to get started!"
    fi
    echo ""
fi
