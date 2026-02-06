#!/bin/bash
# Copyright(c) The Maintainers of Nanvix.
# Licensed under the MIT License.
#
# Script to copy the minimal set of Nanvix build artifacts needed for Python
# execution into the hyperlight-nanvix dist folder.
#
# Files referenced by runtime.rs when nanvix_registry is set:
#   {registry}/bin/kernel.elf          — microkernel binary
#   {registry}/bin/python3             — Python interpreter (symlink → python3.12)
#   {registry}/lib/python3.12.fat      — FAT image with Python stdlib
#
# Usage:
#   ./dist/copy-nanvix.sh [NANVIX_DIR]
#
# Arguments:
#   NANVIX_DIR  Path to the nanvix source/build tree (default: ../nanvix)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
DIST_DIR="$SCRIPT_DIR"
NANVIX_DIR="${1:-$(realpath "$SCRIPT_DIR/../../nanvix")}"

if [[ ! -d "$NANVIX_DIR" ]]; then
    echo "Error: Nanvix directory not found at $NANVIX_DIR"
    echo "Usage: $0 [NANVIX_DIR]"
    exit 1
fi

echo "Nanvix source : $NANVIX_DIR"
echo "Dist target   : $DIST_DIR"
echo ""

# ---------------------------------------------------------------------------
# 0. Build nanvix and create FAT image so artifacts are up to date
# ---------------------------------------------------------------------------
echo "==> Building nanvix..."
pushd "$NANVIX_DIR" > /dev/null
./z build -- BUILD_OPT=yes LOG_LEVEL=info MACHINE=hyperlight all
echo ""
echo "==> Creating FAT image for Python stdlib..."
./scripts/create-fat.sh sysroot/lib/python3.12
popd > /dev/null
echo ""

ERRORS=0

# ---------------------------------------------------------------------------
# 1. bin/kernel.elf — microkernel
# ---------------------------------------------------------------------------
echo "==> Copying kernel.elf..."
mkdir -p "$DIST_DIR/bin"

if [[ -f "$NANVIX_DIR/sysroot/bin/kernel.elf" ]]; then
    cp -v "$NANVIX_DIR/sysroot/bin/kernel.elf" "$DIST_DIR/bin/"
elif [[ -f "$NANVIX_DIR/bin/kernel.elf" ]]; then
    cp -v "$NANVIX_DIR/bin/kernel.elf" "$DIST_DIR/bin/"
else
    echo "  ERROR: kernel.elf not found"
    ERRORS=$((ERRORS + 1))
fi

# ---------------------------------------------------------------------------
# 2. bin/python3 — Python interpreter (+ python3.12 actual binary)
# ---------------------------------------------------------------------------
echo ""
echo "==> Copying python3 interpreter..."

if [[ -f "$NANVIX_DIR/sysroot/bin/python3.12" ]]; then
    cp -v "$NANVIX_DIR/sysroot/bin/python3.12" "$DIST_DIR/bin/"
    ln -sfv python3.12 "$DIST_DIR/bin/python3"
elif [[ -f "$NANVIX_DIR/sysroot/bin/python3" ]]; then
    cp -v "$NANVIX_DIR/sysroot/bin/python3" "$DIST_DIR/bin/"
else
    echo "  ERROR: python3 / python3.12 not found"
    ERRORS=$((ERRORS + 1))
fi

# ---------------------------------------------------------------------------
# 3. lib/python3.12.fat — FAT image with Python standard library
# ---------------------------------------------------------------------------
echo ""
echo "==> Copying python3.12.fat..."
mkdir -p "$DIST_DIR/lib"

if [[ -f "$NANVIX_DIR/lib/fat/python3.12.fat" ]]; then
    cp -v "$NANVIX_DIR/lib/fat/python3.12.fat" "$DIST_DIR/lib/"
else
    echo "  ERROR: lib/fat/python3.12.fat not found"
    echo "  Hint: run  ./scripts/create-fat.sh sysroot/lib/python3.12  in the nanvix tree first (output goes to lib/fat/)"
    ERRORS=$((ERRORS + 1))
fi

# ---------------------------------------------------------------------------
# 4. Summary
# ---------------------------------------------------------------------------
echo ""
echo "============================================"
echo "Done! Dist directory contents:"
echo "============================================"
echo ""
find "$DIST_DIR" -type f -o -type l | sort | while read -r f; do
    size=$(stat --printf="%s" "$f" 2>/dev/null || stat -f%z "$f" 2>/dev/null || echo "?")
    printf "  %-60s %s\n" "${f#$DIST_DIR/}" "$(numfmt --to=iec "$size" 2>/dev/null || echo "${size}B")"
done

if [[ $ERRORS -gt 0 ]]; then
    echo ""
    echo "WARNING: $ERRORS required file(s) were missing. Python execution may not work."
    exit 1
fi

echo ""
echo "Usage with hyperlight-nanvix:"
echo "  cargo run -- guest-examples/hello.py"
echo "  # (dist/ is the default --nanvix-registry path)"
