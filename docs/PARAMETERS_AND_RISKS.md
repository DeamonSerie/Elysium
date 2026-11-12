# Parameters, Invariants, and Open Risks

This document captures implementation choices made for the current scaffold and the plan items that must be finalized before production. Items marked **MVP-ASSUMED** are parameterized/configurable where possible but require product/governance/security finalization.

## Implemented defaults (MVP/devnet)

- **Decimals/units:** 12 decimals, `UNIT=10^12` (`elysium-primitives`).
- **Dual currencies (Section 2.2, MVP-ASSUMED):** ECC = `pallet_balances::Instance1` (native/system account data; staking/collateral/mining deposits/pricing). HNN = `pallet_balances::Instance2` (separate storage; tx fees via `FungibleAdapter` routed to treasury; bundle sale 0.001% fee). Chain spec endows both for dev accounts.
- **Assets registry:** ECC (`asset_id=0`) and HNN (`asset_id=1`) registered in genesis with declared totals, genesis-issued bootstrap amounts, and mineable remainder. Supply-cap accounting enforced on mint recording.
- **Mining:** `d = |D - S| / S * RATE_PRECISION`, `f(d)=max(min_multiplier, 1/(1+k*d))`, `rate_per_period = r_base_per_period * f(d)`, accrued per `period_blocks`, capped by remaining mineable supply. Solo emission factor applied per comparable participant counts (conservative). Auto-placement into eligible pool minimizing `|D_pool - S_target_for_pool|` with `S_target_pool = total_supply_declared / N_pools` (MVP proxy). Pool-only uniformity bonus from bounded budget, distributed pro-rata at start snapshot.
- **Bundle sale / β:** `seller_ecc = X*49999/100000`, `reserve_ecc = X - seller_ecc` (conservation). Treasury HNN fee `hnn_fee = X * hnn_per_ecc_scaled / RATE_PRECISION / 100000` (0.001% of X). `hnn_per_ecc_scaled` is governance-settable storage (`Beta::set_hnn_per_ecc_rate`) with config/genesis default (**MVP-ASSUMED 1:1**). Buyer pays ECC to seller+reserve and HNN fee to treasury; seller receives generic β credits = sold amount. Redemption 1:1 only if asset eligible and sufficient backing.
- **ZK verifier (Section 3.1/3.2):** `ZkPublicInputsV1 { vk_version, spec_version, block_number, parent_hash, pre_state_root, post_state_root, extrinsics_root }` bound in `require_valid_block_proof`. VK/versioning, strict/mock modes, non-mutating failure on invalid proof, dev/mock acceptance when non-strict/unkeyed (**MVP-ASSUMED**). Real verifier hook/feature-gating and full import/finality gating remain to be completed.
- **Contracts (Section 2.3):** `pallet-contracts` wired with `ElysiumChainExtension` (ext_id 0, read-only funcs v1): ECC/HNN free balance, total deposited by asset, pool deposit/participants, beta credits, mining status, mineable remainder, redemption backing. **MVP-ASSUMED**: conservative fixed weights, reads only.
- **Consensus baseline:** Aura + Grandpa retained. Full NPoS/session/offences/election integration, session key rotation, rewards/slashing wiring, and mining-vs-staking lock policy are **not fully implemented** and remain required (Section 2.1).

## Invariants enforced (by design/code paths)

- No mint recording beyond `total_supply_declared - genesis_issued`.
- ECC mining deposits locked per account aggregate across pool/solo positions.
- Sale split conservation in ECC: `seller_ecc + reserve_ecc == X`.
- β redemption requires eligibility and sufficient backing; burns before payout accounting.
- Accrual capped to mineable remainder; completion on exhaustion.
- Pending ZK proof not consumed on validation failure (supports retry/correction).

## MVP-ASSUMED / required specs to finalize (plan Section 5)

1. **ZK (5.5):** proving system (SP1/Risc0/Groth16/etc.), real verifier feature/build, exact witness/tx commitment format, guest STF execution against runtime, proof carriage in block/digest/inherent, node import + Grandpa/finality gating, prover parallelization/aggregation, VK artifacts/hash/procedure, latency.
2. **Mining economics (5.1):** production `R_base`, `f(d)` constants, cadence/normalization, pool target policy, bonus budget source/backing and supply safety.
3. **HNN sourcing (5.2):** authoritative `hnn_per_ecc` oracle/governance process, rounding, failure UX.
4. **β backing/eligibility (5.3):** per-asset backing sources, solvency invariants, governance controls, redemption payout for non-native assets.
5. **Deposit-vs-staking locks (5.4):** ECC locked for mining must not double-count for consensus security; enforce via lock/hold semantics or adapter and test invariants.
6. **NPoS:** staking currency/holds, session rotation, validator/nominator sets, election provider, rewards/slashes/offences, genesis keys/stakers.
7. **Node/build:** coherent sc-* dependency set in cache, `protoc`, RPC/API compatibility, devnet E2E.
8. **Security:** chain extension auth/weights/reentrancy, weight/benchmark accuracy, migrations.

## Operational notes

- Prefer `cargo ... --offline` with cached deps. `protoc` may be required for node/network crates.
- Runtime spec version bumped for currency/config evolution; regenerate chain specs for new genesis layout.
