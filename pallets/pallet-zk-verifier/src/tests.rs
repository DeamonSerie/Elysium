use crate::{mock::*, Error, Pallet as Zk, PendingBlockProof};
use elysium_primitives::ProofPublicInputs;
use frame_support::{assert_noop, assert_ok};
use sp_runtime::traits::Hash;

fn inputs(n: u64) -> ProofPublicInputs<<Test as frame_system::Config>::Hash> {
    ProofPublicInputs {
        vk_version: 0,
        spec_version: 0,
        block_number: n,
        parent_hash: System::parent_hash(),
        pre_state_root: <Test as frame_system::Config>::Hashing::hash(&[1]),
        post_state_root: <Test as frame_system::Config>::Hashing::hash(&[2]),
        extrinsics_root: <Test as frame_system::Config>::Hashing::hash(&[3]),
    }
}

#[test]
fn mock_verification_accepts_without_vk_in_non_strict_mode() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let pi = inputs(1);
        PendingBlockProof::<Test>::put((pi.clone(), sp_runtime::BoundedVec::try_from(vec![0u8]).unwrap(), 0u32));
        assert_ok!(Zk::<Test>::require_valid_block_proof(pi.pre_state_root, pi.post_state_root, pi.extrinsics_root, 1));
    });
}

#[test]
fn strict_mode_requires_proof() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let pi = inputs(1);
        assert_ok!(Zk::<Test>::set_strict_mode(RuntimeOrigin::root(), true));
        assert_noop!(Zk::<Test>::require_valid_block_proof(pi.pre_state_root, pi.post_state_root, pi.extrinsics_root, 1), Error::<Test>::MissingProof);
    });
}

#[test]
fn mismatch_public_inputs_rejected() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let pi = inputs(1);
        PendingBlockProof::<Test>::put((pi.clone(), sp_runtime::BoundedVec::try_from(vec![0u8]).unwrap(), 0u32));
        let wrong_post = <Test as frame_system::Config>::Hashing::hash(&[99]);
        let res = Zk::<Test>::require_valid_block_proof(pi.pre_state_root, wrong_post, pi.extrinsics_root, 1);
        assert_noop!(res.map_err(|e| e), Error::<Test>::PublicInputMismatch);
        // Pending proof must remain for retry/correction on failure.
        assert!(PendingBlockProof::<Test>::get().is_some());
    });
}
