#!/usr/bin/env bash
# Launch claude with the claudtributter plugin loaded.
# Run from tests/interactive/ (or any test repo).

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

exec claude --plugin-dir "$PROJECT_DIR" "$@"
