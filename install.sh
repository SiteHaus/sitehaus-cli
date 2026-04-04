#!/usr/bin/env sh
set -e

REPO="https://github.com/SiteHaus/sitehaus-cli"
CLONE_DIR="$HOME/.sitehaus/src"

echo ""
echo "  sitehaus CLI installer"
echo ""

# Check for rustup/cargo
if ! command -v cargo >/dev/null 2>&1; then
  echo "  cargo not found — installing rustup..."
  if command -v pacman >/dev/null 2>&1; then
    sudo pacman -S --noconfirm rustup
    rustup default stable
  else
    echo "  pacman not found. Install Rust manually: https://rustup.rs"
    exit 1
  fi
fi

# Clone or update the repo
if [ -d "$CLONE_DIR/.git" ]; then
  echo "  Updating sitehaus-cli..."
  git -C "$CLONE_DIR" pull --ff-only
else
  echo "  Cloning sitehaus-cli..."
  mkdir -p "$(dirname "$CLONE_DIR")"
  git clone "$REPO" "$CLONE_DIR"
fi

# Build and install
echo "  Installing..."
cargo install --path "$CLONE_DIR" --quiet

echo ""
echo "  sitehaus installed successfully!"
echo ""

# Run setup wizard if no config exists
if [ ! -f "$HOME/.sitehaus/config.yml" ]; then
  sitehaus setup
fi
