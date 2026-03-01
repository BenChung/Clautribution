#!/usr/bin/env bash
# Reset the tests/interactive directory to a fresh git repository and
# build the binary.  The directory inode is preserved so shells already
# cd'd into it keep working.
#
# Usage:
#   ./tests/reset-interactive.sh
#   cd tests/interactive
#   ../run-claude.sh

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
INTERACTIVE="$SCRIPT_DIR/interactive"

# --- Build the binary ---
echo "Building claudtributter ..."
cargo build --release --manifest-path "$PROJECT_DIR/Cargo.toml"

# --- Reset the interactive repo (preserve directory inode) ---
echo "Resetting $INTERACTIVE ..."

mkdir -p "$INTERACTIVE"
# Remove contents but keep the directory itself.
find "$INTERACTIVE" -mindepth 1 -maxdepth 1 -exec rm -rf {} +

git -C "$INTERACTIVE" init
git -C "$INTERACTIVE" config user.name "Test"
git -C "$INTERACTIVE" config user.email "test@test.com"

cat > "$INTERACTIVE/README" <<'EOF'
Interactive test directory for claudtributter
EOF

cat > "$INTERACTIVE/.gitignore" <<'EOF'
.claudetributer
.claude
EOF

git -C "$INTERACTIVE" add README .gitignore
git -C "$INTERACTIVE" commit -m "initial"

echo ""
echo "Done. Fresh repo at $INTERACTIVE"
