use crate::Runtime;

/// MVP-ASSUMED execution hook: enforce block ZK proof validity after extrinsics execute.
/// Runs in `Core::execute_block` after `Executive::execute_block`, so proof-submission extrinsics
/// (if any) have executed and `PendingBlockProof` reflects this block.
/// - In strict mode missing/invalid proofs panic and invalidate the block; dev/mock defaults may
///   accept missing proofs per pallet policy.
/// - When a proof is pending, enforcement passes the proof-committed roots/extrinsics root as the
///   expected values so pallet input-consistency checks are applied against system/header-derived
///   fields (number, parent hash, spec/VK versions) and committed inputs.
/// Post-state-root binding to the final header commitment is scaffolded/best-effort and must be
/// hardened in production proving/import/finality flows.
pub fn enforce_block_proof_if_present() {
    let number: u64 = <frame_system::Pallet<Runtime>>::block_number();
    if number == 0 {
        return;
    }
    let strict = pallet_zk_verifier::StrictMode::<Runtime>::get();
    let has_pending = pallet_zk_verifier::PendingBlockProof::<Runtime>::get().is_some();
    if strict || has_pending {
        if let Some((pinputs, _, _)) = pallet_zk_verifier::PendingBlockProof::<Runtime>::get() {
            pallet_zk_verifier::Pallet::<Runtime>::require_valid_block_proof(
                pinputs.pre_state_root,
                pinputs.post_state_root,
                pinputs.extrinsics_root,
                number,
            )
            .expect("ZK block proof validation failed");
        } else {
            // strict and no proof: pallet rejects as MissingProof before root comparisons matter.
            let dummy = <frame_system::Pallet<Runtime>>::parent_hash();
            pallet_zk_verifier::Pallet::<Runtime>::require_valid_block_proof(
                dummy, dummy, dummy, number,
            )
            .expect("ZK block proof missing/invalid in strict mode");
        }
    }
}
