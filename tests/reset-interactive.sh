#!/usr/bin/env bash
# Reset the tests/interactive directory to a fresh git repository and
# register the claudtributter hooks into its .claude/settings.json.
#
# Mirrors the setup in temp_git_repo() in tests/cli.rs:
#   - git init
#   - user.name = Test, user.email = test@test.com
#   - initial commit with a README and .gitignore
#
# Then builds the binary and writes .claude/settings.json with hooks for
# SessionStart, UserPromptSubmit, Stop, and SessionEnd (matching main.rs
# dispatch).

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
INTERACTIVE="$SCRIPT_DIR/interactive"

# --- Build the binary ---
echo "Building claudtributter ..."
cargo build --manifest-path "$PROJECT_DIR/Cargo.toml"
BINARY="$PROJECT_DIR/target/debug/claudtributter"

# --- Reset the interactive repo ---
echo "Resetting $INTERACTIVE ..."

rm -rf "$INTERACTIVE"
mkdir "$INTERACTIVE"

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

# --- Register hooks ---
mkdir -p "$INTERACTIVE/.claude"

cat > "$INTERACTIVE/.claude/settings.json" <<EOF
{
  "hooks": {
    "SessionStart": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$BINARY"
          }
        ]
      }
    ],
    "UserPromptSubmit": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$BINARY"
          }
        ]
      }
    ],
    "Stop": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$BINARY"
          }
        ]
      }
    ],
    "SessionEnd": [
      {
        "hooks": [
          {
            "type": "command",
            "command": "$BINARY"
          }
        ]
      }
    ]
  }
}
EOF

echo "Done. Fresh repo at $INTERACTIVE"
echo "Binary: $BINARY"
echo "Hooks registered in $INTERACTIVE/.claude/settings.json"
