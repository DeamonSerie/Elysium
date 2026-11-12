#!/usr/bin/env bash
set -euo pipefail
# MVP/devnet helper. Requires a built node and protoc for full node builds.
# PoA authorities/keys are session-managed in genesis (Alice dev / Alice+Bob local).
# ZK strict mode is disabled in dev genesis by default; enable only with a valid proof path.
# Usage: ./scripts/run-devnet.sh [--build]
ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"
if [[ "${1:-}" == "--build" ]]; then
  echo "[devnet] attempting build (offline if deps cached; protoc may be required)"
  cargo build -p elysium-node --offline || cargo build -p elysium-node
fi
if [[ ! -x ./target/debug/elysium-node && ! -x ./target/release/elysium-node ]]; then
  echo "[devnet] node binary not found. Build may be blocked by SDK dep/protoc alignment (see README/docs)."
  exit 1
fi
BIN=./target/debug/elysium-node
[[ -x ./target/release/elysium-node ]] && BIN=./target/release/elysium-node
exec "$BIN" --dev --tmp
