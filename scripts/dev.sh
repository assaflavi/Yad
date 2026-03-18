#!/bin/bash
# Wrapper for tauri dev that re-signs the binary after each rebuild.
# This ensures macOS TCC permissions (Accessibility, etc.) persist
# across rebuilds by maintaining a stable code signature identity.

BINARY="src-tauri/target/debug/yad"

# Start watching for binary changes and re-sign in background
(
  while true; do
    if [ -f "$BINARY" ]; then
      CURRENT_ID=$(codesign -dv "$BINARY" 2>&1 | grep "^Identifier=" | cut -d= -f2)
      if [ "$CURRENT_ID" != "com.lavi.yad" ]; then
        codesign --force --sign - --identifier "com.lavi.yad" "$BINARY" 2>/dev/null
        echo "[codesign] Re-signed $BINARY as com.lavi.yad"
      fi
    fi
    sleep 2
  done
) &
WATCHER_PID=$!

# Clean up watcher on exit
trap "kill $WATCHER_PID 2>/dev/null" EXIT

# Start vite dev server (this is what beforeDevCommand normally runs)
bun run dev
