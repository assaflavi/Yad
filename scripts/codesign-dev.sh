#!/bin/bash
# Re-sign the dev binary with a stable identifier so macOS TCC
# (Accessibility, etc.) permissions persist across rebuilds.
#
# Usage: Run after `cargo build` or add to your dev workflow.
#   ./scripts/codesign-dev.sh

BINARY="src-tauri/target/debug/yad"

if [ -f "$BINARY" ]; then
  codesign --force --sign - --identifier "com.lavi.yad" "$BINARY" 2>/dev/null
  echo "✓ Signed $BINARY as com.lavi.yad"
else
  echo "⚠ Binary not found: $BINARY"
fi
