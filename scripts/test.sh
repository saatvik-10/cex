#!/usr/bin/env bash
# Build, boot the server, run the Bun integration tests, then tear down.
set -euo pipefail

cd "$(dirname "$0")/.."

echo "==> building server"
cargo build

pkill -f 'target/debug/rs' 2>/dev/null || true
sleep 0.5

echo "==> starting server"
setsid ./target/debug/rs > /tmp/opencode/cex-server.log 2>&1 &
SRV=$!
trap 'kill $SRV 2>/dev/null; pkill -f "target/debug/rs" 2>/dev/null || true' EXIT

# Wait for the server to accept connections.
for i in $(seq 1 30); do
  if curl -s -o /dev/null http://127.0.0.1:8000/auth/signin; then
    break
  fi
  sleep 0.5
done

echo "==> running tests"
bun test

echo "==> done"
