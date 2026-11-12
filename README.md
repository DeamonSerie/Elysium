# Elysium (Substrate/FRAME, PoS, ZK Validity, Liquidity Mining)

Elysium is a standalone sovereign Substrate/FRAME blockchain scaffold implementing:
- **Consensus (PoA/session MVP):** Aura authoring/import + GRANDPA justification/finality with session-managed authorities/keys (`pallet_session`, session-derived consensus APIs). **MVP-ASSUMED:** static PoA session manager (no staking elections/rewards/slashing economics); full NPoS/offences/elections follow-up.
- **Native assets/currencies (Section 2.2):** ECC = `Balances` (Instance1, native/system) for staking/collateral/deposits/pricing; HNN = `HnnBalances` (Instance2, separate) for tx fees and 0.001% bundle-sale treasury fee. Tx fees charged in HNN and routed to treasury via fungible adapter.
- **Liquidity mining:** ECC-locked pool/solo deposits, auto-placement, normalized emission `rate = R_base * f(d)`, `f(d)=max(min, 1/(1+k*d))`, solo factor, pool-only uniformity bonus snapshot/budget, accrual/claims.
- **Beta bundling/sale/redemption:** bundling escrow, listings/sales with split (seller 49.999% ECC, 0.001% HNN fee to treasury using governed `hnn_per_ecc_scaled`, 50% ECC reserve), generic β credits to seller, 1:1 redemption into eligible sufficiently-backed assets.
- **Smart contracts:** `pallet-contracts` included with `ElysiumChainExtension` (read-only protocol queries v1: balances, mining/pool stats, β/backing/status).
- **ZK validity (MVP scaffold):** `pallet-zk-verifier` with `ZkPublicInputsV1` binding, VK/versioning, strict/mock modes, in-block proof submission path (root/unsigned MVP), and runtime execution-time enforcement hook (mode-aware). Guest/prover scaffolds provided; **real cryptographic verifier, STF guest execution, production proof carriage/import/finality binding, and aggregation are pending**.

> **Status:** active implementation against `~/.local/share/kilo/plans/1794435006735-elysium-full-implementation.md`. Economics/security-critical parameters and integration gaps are marked `MVP-ASSUMED` in code/docs and must be finalized before production.

## Workspace layout

- `primitives/` — shared types, IDs, params, fixed-point math, sale/rate helpers, `ZkPublicInputsV1`.
- `pallets/pallet-elysium-assets-registry/` — asset metadata, supply accounting, caps/status/eligibility.
- `pallets/pallet-liquidity-mining/` — pools/solo, ECC locking, placement, accrual, bonus, claims/bundle helpers.
- `pallets/pallet-beta/` — bundling, listings/sales/splits, β credits, backing/redemption, governed HNN/ECC rate.
- `pallets/pallet-zk-verifier/` — VK/versioning, strict/mock verification, public-input-bound enforcement hook.
- `runtime/elysium-runtime/` — FRAME runtime composing system/consensus/fees/treasury/contracts and Elysium pallets; dual balance instances and chain extension.
- `node/` — Substrate node service/CLI/RPC/chain-spec (restoration/build depends on coherent cached sc-* graph and `protoc`).
- `zk/guest/`, `zk/prover/` — zkVM STF boundary and prover/aggregation scaffolds.
- `docs/` — consensus (Aura/GRANDPA/session), ZK integration, parameters/risks.
- `scripts/run-devnet.sh` — devnet helper (session-managed PoA keys; ZK strict off by default).

## Toolchain

- Rust stable with `wasm32-unknown-unknown` (`rust-toolchain.toml`).
- `protoc` may be required for full node/network builds.
- Prefer `--offline` where dependencies are cached.

## Build & test

```bash
# primitives and pallets
cargo test -p elysium-primitives -p pallet-elysium-assets-registry -p pallet-liquidity-mining -p pallet-beta -p pallet-zk-verifier --offline

# runtime
cargo check -p elysium-runtime --offline

# node (may require dep alignment + protoc)
cargo check -p elysium-node --offline
```

## Plan traceability

Implementation follows the plan sequence: session-managed Aura/GRANDPA authorities and runtime APIs, ZK verifier proof carriage/enforcement hook, node chain-spec/genesis alignment, and updated docs/scripts. Pending major items: full NPoS/staking/offences/rewards and rotation economics, production-grade proof binding/carriage/import/finality, real ZK prover/guest/verifier, node dependency/API alignment, benchmarks/weights, and expanded integration/E2E validation.
