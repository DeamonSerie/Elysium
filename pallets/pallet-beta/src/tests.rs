use crate::{mock::*, Error, Pallet as Beta, BetaCredits, RedemptionBacking};
use elysium_primitives::{AssetMetadata, BoundedName, BoundedSymbol, MiningStatus};
use frame_support::{assert_noop, assert_ok};

fn register_asset(id: u32, eligible: bool) {
    let meta = AssetMetadata {
        name: BoundedName::try_from(b"Asset".to_vec()).unwrap(),
        symbol: BoundedSymbol::try_from(b"AST".to_vec()).unwrap(),
        decimals: 12,
        total_supply_declared: 1_000_000,
        genesis_issued: 100_000,
        mined_to_date: 0,
        mineable: true,
        redeemable_eligible: eligible,
        reserve_account: None,
    };
    assert_ok!(AssetsReg::register_asset(RuntimeOrigin::root(), id, meta, None, MiningStatus::Complete));
}

#[test]
fn beta_mint_burn_and_redeem_backed() {
    new_test_ext().execute_with(|| {
        register_asset(7, true);
        assert_ok!(Beta::<Test>::deposit_backing(RuntimeOrigin::root(), 7, 500));
        assert_ok!(Beta::<Test>::mint_beta(&1u64, 300));
        assert_eq!(BetaCredits::<Test>::get(1), 300);
        assert_ok!(Beta::<Test>::redeem(RuntimeOrigin::signed(1), 7, 200));
        assert_eq!(BetaCredits::<Test>::get(1), 100);
        assert_eq!(RedemptionBacking::<Test>::get(7), 300);
        assert_noop!(Beta::<Test>::redeem(RuntimeOrigin::signed(1), 7, 400), Error::<Test>::InsufficientBacking);
    });
}

#[test]
fn redeem_requires_eligible_asset() {
    new_test_ext().execute_with(|| {
        register_asset(8, false);
        assert_ok!(Beta::<Test>::deposit_backing(RuntimeOrigin::root(), 8, 500));
        assert_ok!(Beta::<Test>::mint_beta(&1u64, 100));
        assert_noop!(Beta::<Test>::redeem(RuntimeOrigin::signed(1), 8, 50), Error::<Test>::AssetNotEligible);
    });
}
