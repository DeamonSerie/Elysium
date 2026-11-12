# Production Roadmap: Full NPoS, zkVM/STF Proving, Aggregation, zk DA, Performance, Genesis Economics

> **Status:** Draft — not implementation-ready (blocking decisions in §7). Execute milestones in order; do not enable strict ZK or remove PoA/Sudo safeguards until each milestone’s exit criteria pass.  
> **Requested location:** user asked to place in `docs/`; plan agent cannot edit non-plan docs. Save/copy this content to `docs/npos-zkvm-production-roadmap.md` via an implementation-capable agent or manual move when approved.

## 1. Goal and scope
- **Goal:** Production-grade Elysium on Substrate/Polkadot SDK with (1) full NPoS elections/rewards/slashing economics, (2) production zkVM guest/STF proving, (3) prover aggregation service, (4) zk DA, (5) performance/benchmarking/weights audit, (6) production chain-spec and genesis economics.
- **Assumed target (pending §7.1):** sovereign Aura+GRANDPA solochain on existing `elysium-node`/`elysium-runtime`. Parachain/collator migration out of scope unless selected.
- **Out of scope (unless added):** bridges/exchange integrations, governance UI, formal verification beyond stated audits/tests, parachain migration details if solochain chosen.

## 2. Repo baseline (source of truth)
- **Runtime** `runtime/elysium-runtime/src/lib.rs`, `runtime/elysium-runtime/Cargo.toml`: Aura/Grandpa/Timestamp/Session (PoA `PoaSessionManager`, periodic sessions), ECC (`Instance1`, native/system) & HNN (`Instance2`, fees/treasury) balances, custom pallets (`elysium-assets-registry`, `liquidity-mining`, `beta`, `zk-verifier`), contracts/treasury/sudo/utility; `spec_version=3`; post-exec ZK hook `runtime/elysium-runtime/src/zk_hook.rs`.
- **ZK** `pallets/pallet-zk-verifier/src/lib.rs`, `primitives/src/lib.rs` (`ZkPublicInputsV1`, limits), `zk/guest`, `zk/prover`: mock/placeholder verification, root-or-unsigned proof submission + `ValidateUnsigned`, strict scaffold.
- **Node** `node/src/service.rs`, `node/src/chain_spec.rs`: Aura import/authoring, Grandpa finality/warp sync, session-key genesis.
- **Pinned deps available** (workspace `Cargo.toml`/`Cargo.lock`): `pallet-staking`, `pallet-session`, `pallet-offences`, `pallet-bags-list`, `pallet-election-provider-multi-phase`, `frame-election-provider-support`, `sp-npos-elections` — implementation must conform to locked API shapes.

## 3. Global constraints and invariants
- Deterministic, pinned Rust/SDK/zkVM toolchains and reproducible builds; bump `spec_version` and add storage migrations for every breaking change.
- Never enable production `zkVerifier.strict_mode` without sound verifier, verified proof carriage, and header/post-state binding; fail-closed in strict, dev/mock isolated.
- Make staking currency/locks and ECC/HNN fee-treasury economics explicit and consistent before genesis.
- Bound worst-case block weight/length/PoV and proof/VK size/verification cost; no unbenchmarked production dispatch paths.
- Every milestone delivers: Rust changes + tests + benchmarks/weights (if applicable) + docs + migration/rollback notes.

## 4. Ordered milestones (Rust/Substrate/Polkadot)

### M1 — NPoS runtime foundation (staking, session, authorship)
**Objective:** Replace PoA session manager with staking-backed validator selection and Aura/Grandpa-compatible session rotation.
- **M1.1 Pallets/config** (`runtime/elysium-runtime/Cargo.toml`, `src/lib.rs`): wire `pallet-staking`, `pallet-authorship`, session historical support as needed, `pallet-bags-list`, election provider per §7.2, `pallet-offences`; configure currency/locks (stake asset explicit), bonding/era/session params, nomination bounds. Update `construct_runtime!`, std/runtime-benchmarks/try-runtime features.
- **M1.2 Session & consensus:** replace `PoaSessionManager` with staking session pipeline; set `SessionHandler` keys (Aura/Grandpa), Aura disabled-validator hooks, Grandpa `EquivocationReportSystem`/`KeyOwnerProof` for reportable offences; ensure `AuraApi`/`GrandpaApi` return staking/session-derived sets.
- **M1.3 Genesis & migrations:** extend `node/src/chain_spec.rs` and runtime genesis (`staking`, `session`, `bags-list`, election provider storages, initial validators/nominators/keys/stakes/invulnerables); implement PoA→NPoS storage migrations and `spec_version` bump; plan Sudo posture per §7.7.
- **Exit:** era/session rotation on multi-node devnet with staking authorities; runtime/node compile; migration dry-run documented.

### M2 — Elections, rewards, slashing/offences economics
**Objective:** Production-correct selection, payouts, and punishment.
- **M2.1 Elections:** configure chosen provider (e.g. EPM phases/bounds/solution verification, signed/unsigned submission, fallback). Add node offchain election/miner support in `node/src/service.rs` if required.
- **M2.2 Rewards:** implement issuance/inflation or fixed payout model (§7.6), era points via `pallet-authorship`, validator/nominator split & commission, treasury/remainder routing consistent with ECC/HNN; document formulas/invariants.
- **M2.3 Slashing/offences:** map Grandpa equivocation (and Aura unresponsiveness if enabled) to offence classes/severity; configure slash fractions, defer/cancel, disabling, reporting; validate key-ownership proofs and economics.
- **Exit:** tests for election, payout accounting, equivocation/unresponsiveness slashing; invariants and parameter sheet documented.

### M3 — Production zkVM guest and STF proving
**Objective:** Deterministic, verifiable block proofs replacing mock/placeholder verification.
- **M3.1 Spec:** versioned public-input schema (evolve `ZkPublicInputsV1` in `primitives`) binding parent hash, pre/post state roots, extrinsics/state commitments, block number, spec/VK versions; define canonical STF and witness format.
- **M3.2 Guest** (`zk/guest`): deterministic STF execution committing public inputs; pinned/reproducible guest artifact build; witness/state IO per zkVM (§7.3).
- **M3.3 Prover host** (`zk/prover`): witness extraction, proof generation, artifact/VK/version management, CLI/API for single-block proving.
- **M3.4 Verifier & enforcement:** replace placeholder in `pallet-zk-verifier` with no_std/WASM-feasible production verifier, VK registry/rotation/governance, size/weight bounds; update `zk_hook`/`execute_block` and/or import path to bind proof to final header commitments and fail-closed strict mode; remove/isolate mock path for production.
- **M3.5 Node integration:** `node/src/service.rs`/RPC authoring attaches/requests proofs, import rejects invalid/missing proofs per mode, proof status/metrics.
- **Exit:** end-to-end prove+verify on devnet; tampered block/witness/proof rejected; header/post-state binding resolved.

### M4 — Prover aggregation service
**Objective:** Scalable batched/recursive proving.
- **M4.1 Design:** aggregation topology (block → batch → recursive/settlement), range commitments, SLOs, queue/idempotency, artifact store.
- **M4.2 Implementation:** Rust service (new crate e.g. `zk/aggregator` or extend `zk/prover`) with workers, recursion, retries, VK/proof cache, auth, metrics/tracing, health APIs.
- **M4.3 Acceptance path:** runtime verifier/pallet and node submission/import support for aggregated/range proofs mapped to block commitments/finality conditions.
- **Exit:** meets SLOs on staging; restart-safe recovery; e2e aggregated proof accepted.

### M5 — zk DA
**Objective:** Data availability for block/proof/witness data per selected model (§7.4).
- **M5.1 Spec:** payloads, encoding/erasure (if any), commitments bound to headers/proofs, and availability↔validity/finality coupling policy.
- **M5.2 Implementation:** publication/retrieval, node/runtime verification hooks, networking/storage, retention/pruning.
- **M5.3 Adversarial handling:** withholding/corruption detection, sampling/attestation (if applicable), explicit halt/degrade policy and runbook.
- **Exit:** commitments verified in pipeline; withholding/adversarial tests pass per policy.

### M6 — Performance tuning, benchmarks, weights audit
**Objective:** Safe limits and reproducible performance.
- **M6.1 Benchmarks/weights:** add FRAME benchmarks and generated weights for staking/election/offences, `pallet-zk-verifier` verify/submit/VK ops, and affected custom pallets; commit weights via `runtime-benchmarks`.
- **M6.2 Limits & profiling:** profile execution/import/verification/proving; set `RuntimeBlockWeights/Length`, proof/VK caps, verifier compute bounds; optimize hot paths.
- **M6.3 Audit/regression:** reproducible bench process, CI/regression thresholds, capacity & SLO report (block time, proof/aggregation latency, hardware).
- **Exit:** all production paths benchmarked with margin; no unbounded verification.

### M7 — Production chain-spec and genesis economics
**Objective:** Auditable, reproducible launch configuration.
- **M7.1 Economics:** total/split allocations ECC/HNN, issuance/inflation, vesting/locks, treasury/endowments, bootstrap params, fee/slash/reward routing consistency (§7.6).
- **M7.2 Genesis:** validators/nominators/keys/stakes, session/era params, governance/Sudo posture, VK/strict-mode/DA params per environment; update `node/src/chain_spec.rs` builders/presets.
- **M7.3 Reproducibility & launch:** deterministic spec artifacts/checksums, PoA→NPoS migration rehearsal (if needed), strict-mode enablement runbook, rollback criteria.
- **Exit:** deterministic specs verified, economic review sign-off, staged launch rehearsal passed.

## 5. Cross-cutting risks, failure modes, validation
- **Risks:** (R1) proof-to-header/post-state binding gap in current hook; (R2) no_std/WASM verifier feasibility/cost; (R3) DA liveness coupling; (R4) election/solver liveness; (R5) economic misconfiguration/slash cascades; (R6) unsafe PoA→NPoS migration. Mitigate with fail-closed staging, caps/fallbacks, dry-runs, adversarial tests.
- **Failure modes:** missing/invalid/late proofs, DA withholding, equivocation, finality stall, prover backlog, VK/spec mismatch.
- **Validation:** per-crate/runtime `cargo test` (+ `runtime-benchmarks`), weight generation, multi-node devnet/zombienet (eras, slashing, finality), e2e prove→aggregate→DA→import/finality, upgrade/migration rehearsals, negative/adversarial cases.
- **Rollout:** devnet (mock/non-strict) → staging (real proofs/agg/DA, shadow where possible) → canary strict with kill-switch → production.

## 6. Affected boundaries / data flow (summary)
- **Authoring/import:** inherent/timestamp/slot (+ proof/DA artifacts per §7.4–§7.5) → runtime STF/`zk_hook` or import verifier → Aura/Grandpa finality.
- **Staking loop:** staking/election/session → validator set → Aura authorities & Grandpa voters → offences/slashing → stake/era updates.
- **Proving pipeline:** node/witness → `zk/prover` (guest/STF) → block proof → aggregator → runtime/node verification + DA commitments.

## 7. Blocking decisions (must resolve)
1. **Target (most important):** sovereign solochain (**recommended**, preserves current Aura+Grandpa/node) vs Polkadot parachain/collator (shared security/interop but requires cumulus/collation, relay-DA, redesign). 
2. **Election provider:** EPM+bags-list (+ node miner/offchain) vs SDK-supported alternative.
3. **zkVM/proof system:** stack with proven no_std/WASM verifier, recursion/aggregation, reproducible toolchain.
4. **zk DA model:** sovereign DA vs relay/external DA; availability↔finality coupling.
5. **Proof carriage/binding:** extrinsic/inherent vs digest+DA; authoritative header post-state commitment and enforcement point.
6. **Economics:** stake currency/locks, issuance/reward curve, commission/slash params, allocations/vesting, ECC/HNN roles, treasury routing.
7. **Governance/migration:** Sudo removal/upgrade authority, VK governance, PoA→NPoS migration path.
8. **SLOs/env:** block/finality/proof latency, throughput, hardware, CI/release, audit requirements.

**Most important unresolved decision:** §7.1 deployment target. **Recommendation:** assume **sovereign solochain** unless Polkadot shared security/interop is a near-term production requirement. Answering this unblocks runtime/node/DA/proving architecture choices in M1–M7.
