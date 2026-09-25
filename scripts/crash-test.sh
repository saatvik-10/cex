#!/usr/bin/env bash
# Engine robustness gate (requires backend + engine already running, fresh data):
#   A. same idempotency_key sent twice -> single credit
#   B. deposits fired while the engine is killed -> exactly-once after restart
set -euo pipefail

cd "$(dirname "$0")/.."

BASE=http://127.0.0.1:8000
ENGINE_LOG=/tmp/opencode/cex-engine.log

USR="u$(date +%s)"
SIGNUP=$(curl -s -X POST "$BASE/auth/signup" -H 'content-type: application/json' \
  -d "{\"username\":\"$USR\",\"password\":\"password123\"}")
TOK=$(echo "$SIGNUP" | bun -e 'const d = JSON.parse(await Bun.stdin.text()); console.log(d.access_token)')
UID1=$(echo "$SIGNUP" | bun -e 'const d = JSON.parse(await Bun.stdin.text()); console.log(d.user.id)')
AUTH="Authorization: Bearer $TOK"

usd() {
  echo "$1" | bun -e 'const d = JSON.parse(await Bun.stdin.text()); console.log(d.balances.find(b => b.asset === "USD")?.amount ?? "?")'
}

# --- A. same-key dedupe ------------------------------------------------------------
echo "==> A. same-key idempotency"
curl -s -X POST "$BASE/deposit/USD" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"amount":"10","idempotency_key":"alpha"}' >/dev/null
B1=$(usd "$(curl -s "$BASE/wallet/balance" -H "$AUTH")")
curl -s -X POST "$BASE/deposit/USD" -H "$AUTH" -H 'content-type: application/json' \
  -d '{"amount":"10","idempotency_key":"alpha"}' >/dev/null
B2=$(usd "$(curl -s "$BASE/wallet/balance" -H "$AUTH")")
# Demo seed (10k USD) + one credit of 10 -> 10010; the second same-key deposit
# must add nothing.
[ "$B1" = "$B2" ] && [ "$B1" = "10010.000000000000000000" ] \
  || { echo "FAIL same-key dedupe: b1=$B1 b2=$B2"; exit 1; }
echo "  ok (credited once, balance=$B2)"

# --- B. kill engine mid-flight, deposits must land exactly once ---------------------
echo "==> B. crash recovery (kill -9 during deposits)"
# Writes go STRAIGHT to the stream so the deposits are genuinely in flight when
# the engine dies. (Sending them via the backend would be a different contract:
# its cap-check get_balances fails once the engine stops answering, so the
# deposit is never even XADDed and the client must retry.)
for i in 1 2 3 4 5; do
  docker exec cex-redis redis-cli XADD stream:commands '*' payload \
    "{\"kind\":\"deposit\",\"reply_key\":\"b-$i\",\"user_id\":\"$UID1\",\"asset\":\"USD\",\"amount\":\"1\",\"idempotency_key\":\"k$i\"}" >/dev/null
  sleep 0.05
  if [ "$i" = "2" ]; then
    pkill -9 -f 'target/debug/engine' 2>/dev/null || true
  fi
done

for _ in $(seq 1 10); do pgrep -f 'target/debug/engine' >/dev/null && sleep 0.2 || break; done
pgrep -f 'target/debug/engine' >/dev/null && { echo "FAIL engine still alive"; exit 1; }

echo "==> restarting engine"
(cd engine && setsid ./target/debug/engine > "$ENGINE_LOG" 2>&1 &)
for _ in $(seq 1 30); do
  grep -q 'ready' "$ENGINE_LOG" && break
  sleep 0.2
done
grep -q 'ready' "$ENGINE_LOG" || { echo "ERROR: engine did not restart"; tail -5 "$ENGINE_LOG"; exit 1; }

# The freshly booted engine takes a moment to drain the pending list; poll
# until the balance endpoint answers again.
B3=""
for _ in $(seq 1 30); do
  RESP=$(curl -s "$BASE/wallet/balance" -H "$AUTH")
  case "$RESP" in
    *'balances'*) B3=$(usd "$RESP"); break ;;
    *) sleep 0.5 ;;
  esac
done
[ -n "$B3" ] || { echo "FAIL crash recovery: balance endpoint never answered"; exit 1; }
# 10010 (after A) + 5 deposits of 1 = 10015. A kill -9 during the burst must
# leave every deposit credited exactly once.
[ "$B3" = "10015.000000000000000000" ] \
  || { echo "FAIL crash recovery: want 10015.000000000000000000 got $B3"; exit 1; }
echo "  ok (5 deposits, one credit each, balance=$B3)"

# --- C. idempotency keys stay isolated per user ----------------------------------
# Two users issue an IDENTICAL idempotency_key in the same engine batch by
# writing both commands straight to the stream. Each user must be credited once.
echo "==> C. cross-user idempotency keys stay isolated"
USR2="v$(date +%s)"
SIGNUP2=$(curl -s -X POST "$BASE/auth/signup" -H 'content-type: application/json' \
  -d "{\"username\":\"$USR2\",\"password\":\"password123\"}")
TOK2=$(echo "$SIGNUP2" | bun -e 'const d = JSON.parse(await Bun.stdin.text()); console.log(d.access_token)')
UID2=$(echo "$SIGNUP2" | bun -e 'const d = JSON.parse(await Bun.stdin.text()); console.log(d.user.id)')
AUTH2="Authorization: Bearer $TOK2"

docker exec cex-redis redis-cli XADD stream:commands '*' payload \
  "{\"kind\":\"deposit\",\"reply_key\":\"c-1\",\"user_id\":\"$UID1\",\"asset\":\"USD\",\"amount\":\"10\",\"idempotency_key\":\"shared\"}" >/dev/null
sleep 0.2
docker exec cex-redis redis-cli XADD stream:commands '*' payload \
  "{\"kind\":\"deposit\",\"reply_key\":\"c-2\",\"user_id\":\"$UID2\",\"asset\":\"USD\",\"amount\":\"10\",\"idempotency_key\":\"shared\"}" >/dev/null
sleep 1

C1=$(usd "$(curl -s "$BASE/wallet/balance" -H "$AUTH")")
C2=$(usd "$(curl -s "$BASE/wallet/balance" -H "$AUTH2")")
# u1: 10015 (after B) + 10 = 10025; u2: 10000 seed + 10 = 10010.
[ "$C1" = "10025.000000000000000000" ] && [ "$C2" = "10010.000000000000000000" ] \
  || { echo "FAIL cross-user keys: u1=$C1 (want 10025) u2=$C2 (want 10010)"; exit 1; }
echo "  ok (user1=$C1, user2=$C2 — same key, credited independently)"