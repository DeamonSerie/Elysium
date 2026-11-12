# ZK Validity Integration (Design Target A)

## Goal
Each block carries a zkVM validity proof attesting that executing the block’s transactions (runtime STF) against prior state yields the posted post-state root. In strict/production mode, proof verification is required for block acceptance/finality. Proving should support parallel proving of independent transaction traces/batches with deterministic ordering and aggregation into a single per-block proof.

## On-chain components (implemented scaffold)
- `pallet-zk-verifier`:
  - Stores verifying key bytes and `vk_id/version`, and `strict_mode`.
  - Accepts submitted proof payload `(ZkPublicInputsV1, proof_bytes, vk_id)` and enforces `require_valid_block_proof(expected_pre, expected_post, expected_extrinsics_root, block_number)` bound to parent hash/spec/VK.
  - **Devnet/mock (MVP-ASSUMED):** accepts when mock verification allowed and non-strict/unkeyed; placeholder deterministic check when keyed/strict without real verifier.
  - **Production hook:** replace placeholder verification with real verifier (SP1/Risc0/Groth16/etc.) over committed public inputs/journal and active VK; feature-gate `real-zk` as artifacts become available.
  - Provides `expected_inputs(...)` helper; pending proof is not consumed on validation failure.

## Public inputs (V1, implemented)
`ZkPublicInputsV1 { vk_version, spec_version, block_number, parent_hash, pre_state_root, post_state_root, extrinsics_root }` (SCALE/TypeInfo). Alias `ProofPublicInputs = ZkPublicInputsV1`. **MVP-ASSUMED:** extend with chain/genesis binding and explicit tx commitments/witness commitments per selected proving system.

## Integration points (to complete for production)
1. **Block/header carriage:** include proof or proof commitment in block digest/extension/inherent and propagate through node import pipeline.
2. **Import/runtime gating:** reject blocks with missing/invalid/mismatched proofs before import and before Grandpa finality in strict mode; bind to header/state/extrinsics roots derived by executor.
3. **Consensus coupling:** Aura authoring packages/waits for proof as required; verified-only blocks are gossiped/finalized.
4. **Weight/limits:** bound proof size/VK and meter verification cost (benchmarked weights).
5. **Governance upgrades:** VK rotation/versioning with delay/fork coordination; record artifact hashes/IDs.

## Prover pipeline (scaffold)
- `zk/guest`: deterministic STF boundary and I/O envelope committing `ZkPublicInputsV1`; production guest must execute runtime STF with witnesses inside chosen zkVM.
- `zk/prover`: `ProverService` scaffold with mock proof generation; production path must fetch block/witness, partition dependency-safe tx traces, prove in parallel, recursively aggregate/continuations, and submit proof to node/runtime.
- Deterministic tx ordering and state access must be committed in the block.

## Devnet workflow
- Run with `strict_mode=false` and mock verification enabled to develop economics/logic without proving latency.
- Submit proofs via root `ZkVerifier::submit_block_proof` for exercising enforcement hooks; strict mode requires valid, bound proof.
- Node import/finality gating and real prover integration are pending (see plan Section 3.5, 4.1, 4.4).
