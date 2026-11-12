//! zkVM guest scaffold for Elysium STF validity proving (Section 3.3).
//!
//! MVP-ASSUMED (Section 5.5): a zkVM guest proves block STF execution and commits
//! `ZkPublicInputsV1` as public journal. Feature-gated real verifier/prover integration
//! is planned; default builds remain offline-deterministic with mock verification.
//!
//! Deterministic I/O contract (versioned):
//! - Input: pre-state root/witness, parent/block context, ordered extrinsics/block payload.
//! - Output/public journal: `vk_version, spec_version, block_number, parent_hash,
//!   pre_state_root, post_state_root, extrinsics_root`.
#![cfg_attr(not(feature = "std"), no_std)]

#[cfg(feature = "std")]
extern crate std;

extern crate alloc;

/// Canonical guest input envelope (STF boundary placeholder).
#[derive(Clone, Debug)]
pub struct StfInput<Hash, Block, Witness> {
    pub pre_state_root: Hash,
    pub parent_hash: Hash,
    pub block_number: u64,
    pub spec_version: u32,
    pub vk_version: u32,
    pub block: Block,
    pub witnesses: Witness,
}

/// Canonical public outputs committed by the proof.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StfOutput<Hash> {
    pub vk_version: u32,
    pub spec_version: u32,
    pub block_number: u64,
    pub parent_hash: Hash,
    pub pre_state_root: Hash,
    pub post_state_root: Hash,
    pub extrinsics_root: Hash,
}

/// Placeholder deterministic STF entrypoint.
///
/// Production implementation must execute runtime STF against provided witness/state
/// inside the selected zkVM (e.g. SP1/Risc0), preserving deterministic ordering and
/// hashing compatible with the runtime.
pub fn execute<Hash, Block, Witness>(_input: StfInput<Hash, Block, Witness>) -> Option<StfOutput<Hash>> {
    None
}
