#!/usr/bin/env bash
# Full hybrid-CEX gate:
#   1. redis + postgres are running
#   2. schema applied (Prisma owns it), database and streams reset
#   3. engine + backend booted
#   4. behavioural parity against the legacy monolith (bun test: 20/20)
#   5. engine robustness: idempotency + crash recovery (crash-test.sh)
set -euo pipefail

cd "$(dirname "$0")/.."

ENGINE_LOG=/tmp/opencode/cex-engine.log
BACKEND_LOG=/tmp/opencode/cex-backend.log
BASE=http://127.0.0.1:8000

# Kill whatever currently owns a TCP port. Used instead of `pkill -f`, which
# can miss relative-path launches (`bun src/index.ts`) and self-match.
kill_listener() {
  local port=$1
  for pid in $(lsof -ti tcp:"$port" 2>/dev/null || true); do
    kill "$pid" 2>/dev/null || true
  done
  sleep 0.3
}

STOP_PROCESSES=0

cleanup() {
  trap - EXIT
  if [ "$STOP_PROCESSES" = 1 ]; then
    pkill -f 'target/debug/engine' 2>/dev/null || true
    kill_listener 8000
  fi
}
trap cleanup EXIT

# --- 1. infra ---------------------------------------------------------------
echo "==> ensuring infra (redis + postgres)"
if ! docker ps --format '{{.Names}}' | grep -qx cex-pg || ! docker ps --format '{{.Names}}' | grep -qx cex-redis; then
  docker rm -f cex-pg cex-redis 2>/dev/null || true
  docker compose up -d
fi
for _ in $(seq 1 60); do
  if docker exec cex-pg pg_isready -U cex -d cex >/dev/null 2>&1 && docker exec cex-redis redis-cli ping >/dev/null 2>&1; then
    break
  fi
  sleep 1
done
docker exec cex-pg pg_isready -U cex -d cex >/dev/null || { echo "ERROR: postgres not ready"; exit 1; }

# --- 2. schema + data reset ---------------------------------------------------
echo "==> applying schema (prisma migrate deploy)"
(cd backend && bunx prisma migrate deploy >/dev/null)

echo "==> resetting data"
docker exec cex-pg psql -U cex -d cex -c "TRUNCATE idempotency_keys, balances, refresh_tokens, users CASCADE;" >/dev/null
docker exec cex-redis redis-cli del stream:commands stream:results >/dev/null
rm -f engine/wal.log

# --- 3. build engine -----------------------------------------------------------
echo "==> building engine"
cargo build --manifest-path engine/Cargo.toml

# --- 4. boot engine + backend ---------------------------------------------------
echo "==> booting engine"
pkill -f 'target/debug/engine' 2>/dev/null || true
sleep 0.3
(cd engine && setsid ./target/debug/engine > "$ENGINE_LOG" 2>&1 &)
echo "==> booting backend"
kill_listener 8000
(cd backend && setsid bun src/index.ts > "$BACKEND_LOG" 2>&1 &)
STOP_PROCESSES=1

echo "==> waiting for backend"
for i in $(seq 1 30); do
  if curl -s -o /dev/null "$BASE/auth/signin"; then
    break
  fi
  sleep 0.5
done
curl -s -o /dev/null "$BASE/auth/signin" || { echo "ERROR: backend not ready"; exit 1; }
grep -q 'ready' "$ENGINE_LOG" || { echo "ERROR: engine not ready"; tail -5 "$ENGINE_LOG"; exit 1; }

# --- 5. behavioural parity -------------------------------------------------------
echo "==> running parity tests (expect 20 pass / 0 fail)"
(cd tests && bun test)

# --- 6. robustness gate -----------------------------------------------------------
echo "==> running crash + idempotency gate"
bash scripts/crash-test.sh

echo "==> all green"