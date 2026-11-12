# ZK Block Validity Integration (MVP Scaffold)

## Components
- **Primitive types:** `elysium_primitives::{ZkPublicInputsV1, ProofPublicInputs, BlockProof}` define proof payloads and public inputs (`vk_version`, `spec_version`, `block_number`, `parent_hash`, `pre_state_root`, `post_state_root`, `extrinsics_root`).
- **Pallet:** `pallet-zk-verifier`
  - Storage: `VerifyingKey`, `VkVersion`, `StrictMode`, `PendingBlockProof`, `LastVerified`.
  - Calls: `set_verifying_key` (root), `set_strict_mode` (root), `submit_block_proof` (root or unsigned, MVP-ASSUMED devnet carriage).
  - Validation: `verify_block_proof` and `require_valid_block_proof` bind submitted inputs to expected values and enforce mode-aware acceptance (mock/devnet vs strict).
- **Runtime hook:** `runtime/elysium-runtime/src/lib.rs` (`Core::execute_block`) + `zk_hook.rs` invoke post-execution enforcement; strict mode rejects missing/invalid proofs.

## Proof carriage and enforcement
- **Carriage (MVP):** proofs/public inputs are submitted in-block via `submit_block_proof` extrinsic (unsigned allowed by pallet validation for devnet authoring/injection convenience). Production should move to digest/DA/inherent-backed proof channels.
- **Execution gate:** after `Executive::execute_block`, runtime enforces proof validity when strict mode is enabled or a pending proof exists.
  - Strict: missing proof or invalid/mismatched inputs/proof => block invalid.
  - Non-strict/dev mock: missing proof may be accepted per pallet config; mock verification paths are deterministic scaffolds.
- **Input binding:** checks include block number, parent hash, extrinsics root, spec/VK versions, and submitted state roots as required by pallet logic. Post-state-root commitment relative to the produced header is a known scaffold limitation and must be hardened in production proving/import/finality flow.

## Modes and governance
- `StrictMode` and verifying key/version are root-governed.
- Chain spec defaults `zkVerifier.strict_mode=false` for devnet.
- Enabling strict mode without a reliable proof generation/submission path will halt valid block production/import.

## Assumptions / limitations
- Mock/placeholder verification is **not** cryptographically sound; for development only.
- No real zkVM guest/STF proving, aggregation, or proof DA/finality integration yet.
- Unsigned proof submission is minimally validated at pool/runtime-entry level (MVP); production requires stronger author/inherent/digest binding.
