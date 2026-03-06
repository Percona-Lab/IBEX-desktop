#!/usr/bin/env bash
#
# IBEX Prototype Build Script
#
# Produces a macOS DMG installer with:
# - Bundled Node.js binary (ARM64)
# - Bundled MCP server scripts + dependencies
#
# Prerequisites: pnpm, npm, Rust toolchain, Tauri CLI
#
# Usage: ./scripts/build-prototype.sh
#
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
TAURI_DIR="$PROJECT_DIR/src-tauri"

NODE_VERSION="22.14.0"
NODE_ARCH="darwin-arm64"
NODE_TARBALL="node-v${NODE_VERSION}-${NODE_ARCH}.tar.gz"
NODE_URL="https://nodejs.org/dist/v${NODE_VERSION}/${NODE_TARBALL}"
CACHE_DIR="$PROJECT_DIR/.cache"

IBEX_SOURCE="${HOME}/IBEX"
RESOURCES_DIR="$TAURI_DIR/resources/ibex-servers"
BINARIES_DIR="$TAURI_DIR/binaries"

echo "╔══════════════════════════════════════════╗"
echo "║       IBEX Prototype Build               ║"
echo "╚══════════════════════════════════════════╝"
echo ""
echo "Project:  $PROJECT_DIR"
echo "Node.js:  v${NODE_VERSION} (${NODE_ARCH})"
echo "Servers:  $IBEX_SOURCE"
echo ""

# ── Pre-flight checks ──

if [ ! -d "$IBEX_SOURCE/servers" ] || [ ! -d "$IBEX_SOURCE/connectors" ]; then
    echo "ERROR: ~/IBEX directory not found or incomplete."
    echo "Expected: ~/IBEX/servers/ and ~/IBEX/connectors/"
    exit 1
fi

if ! command -v pnpm &>/dev/null; then
    echo "ERROR: pnpm not found. Install with: npm install -g pnpm"
    exit 1
fi

if ! command -v npm &>/dev/null; then
    echo "ERROR: npm not found. Install Node.js first."
    exit 1
fi

if ! command -v cargo &>/dev/null; then
    echo "ERROR: cargo not found. Install Rust: https://rustup.rs"
    exit 1
fi

# ── Step 1: Download and extract Node.js binary ──

echo "▶ Step 1/5: Preparing Node.js binary..."
mkdir -p "$CACHE_DIR" "$BINARIES_DIR"

if [ ! -f "$CACHE_DIR/$NODE_TARBALL" ]; then
    echo "  Downloading Node.js v${NODE_VERSION}..."
    curl -L --progress-bar -o "$CACHE_DIR/$NODE_TARBALL" "$NODE_URL"
else
    echo "  Using cached Node.js download."
fi

echo "  Extracting node binary..."
tar -xzf "$CACHE_DIR/$NODE_TARBALL" \
    -C "$CACHE_DIR" \
    "node-v${NODE_VERSION}-${NODE_ARCH}/bin/node"

# Tauri externalBin expects: node-<target_triple>
# Remove any existing file/symlink first (dev builds may have a symlink)
rm -f "$BINARIES_DIR/node-aarch64-apple-darwin"
cp "$CACHE_DIR/node-v${NODE_VERSION}-${NODE_ARCH}/bin/node" "$BINARIES_DIR/node-aarch64-apple-darwin"
chmod +x "$BINARIES_DIR/node-aarch64-apple-darwin"

NODE_SIZE=$(du -sh "$BINARIES_DIR/node-aarch64-apple-darwin" | cut -f1)
echo "  ✓ Node.js binary ready ($NODE_SIZE)"

# ── Step 2: Copy MCP server scripts ──

echo ""
echo "▶ Step 2/5: Bundling MCP server scripts..."

rm -rf "$RESOURCES_DIR"
mkdir -p "$RESOURCES_DIR"

# Copy server scripts
cp -R "$IBEX_SOURCE/servers" "$RESOURCES_DIR/servers"
cp -R "$IBEX_SOURCE/connectors" "$RESOURCES_DIR/connectors"

# Remove google-docs connector (googleapis dependency is ~80MB — excluded from prototype)
rm -f "$RESOURCES_DIR/connectors/google-docs.js"
echo "  Excluded: google-docs.js (googleapis dependency too large for prototype)"

# Copy optional read-only files
[ -f "$IBEX_SOURCE/notion_index.json" ] && cp "$IBEX_SOURCE/notion_index.json" "$RESOURCES_DIR/"

echo "  ✓ Copied servers/ and connectors/"

# ── Step 3: Install production dependencies (without googleapis) ──

echo ""
echo "▶ Step 3/5: Installing Node.js dependencies..."

# Create a modified package.json without googleapis
IBEX_PKG="$IBEX_SOURCE/package.json" OUT_PKG="$RESOURCES_DIR/package.json" \
    node --input-type=module -e '
import { readFileSync, writeFileSync } from "fs";
const pkg = JSON.parse(readFileSync(process.env.IBEX_PKG, "utf8"));
delete pkg.dependencies["googleapis"];
writeFileSync(process.env.OUT_PKG, JSON.stringify(pkg, null, 2));
console.log("  Dependencies:", Object.keys(pkg.dependencies).join(", "));
'

# Do NOT copy lockfile — we modified dependencies, fresh install is cleaner

cd "$RESOURCES_DIR"
npm install --production --no-optional 2>&1 | tail -3
cd "$PROJECT_DIR"

DEPS_SIZE=$(du -sh "$RESOURCES_DIR/node_modules" 2>/dev/null | cut -f1 || echo "0")
echo "  ✓ Dependencies installed ($DEPS_SIZE)"

# ── Step 4: Install frontend dependencies ──

echo ""
echo "▶ Step 4/5: Preparing frontend..."

cd "$PROJECT_DIR"
if [ ! -d "$PROJECT_DIR/node_modules" ]; then
    echo "  Installing frontend dependencies..."
    pnpm install
else
    echo "  Frontend dependencies already installed."
fi

# ── Step 5: Build the Tauri app ──

echo ""
echo "▶ Step 5/5: Building Tauri application..."
echo "  This may take several minutes on first build..."
echo ""

cd "$PROJECT_DIR"
pnpm tauri build 2>&1

# ── Report ──

echo ""
echo "╔══════════════════════════════════════════╗"
echo "║       Build Complete                     ║"
echo "╚══════════════════════════════════════════╝"
echo ""

DMG_PATH=$(find "$TAURI_DIR/target/release/bundle/dmg" -name "*.dmg" 2>/dev/null | head -1)
APP_PATH=$(find "$TAURI_DIR/target/release/bundle/macos" -name "*.app" -maxdepth 1 2>/dev/null | head -1)

if [ -n "$DMG_PATH" ]; then
    DMG_SIZE=$(du -sh "$DMG_PATH" | cut -f1)
    echo "  DMG:  $DMG_PATH"
    echo "  Size: $DMG_SIZE"
fi

if [ -n "$APP_PATH" ]; then
    APP_SIZE=$(du -sh "$APP_PATH" | cut -f1)
    echo "  App:  $APP_PATH"
    echo "  Size: $APP_SIZE"
fi

if [ -z "$DMG_PATH" ] && [ -z "$APP_PATH" ]; then
    echo "  WARNING: No build output found. Check the build log above for errors."
    exit 1
fi

echo ""
echo "Next steps:"
echo "  1. Test the DMG: open \"$DMG_PATH\""
echo "  2. Drag IBEX to Applications"
echo "  3. Right-click → Open (bypass Gatekeeper)"
echo "  4. Tag and release: git tag v0.1.0-prototype.1"
echo ""
