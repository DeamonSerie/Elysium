use crate::{mock::*, Error, Pallet as Mining, TotalDepositedByAsset};
use elysium_primitives::{AssetMetadata, BoundedName, BoundedSymbol, MiningParams, MiningStatus, RATE_PRECISION};
use frame_support::{assert_noop, assert_ok};

fn register_mineable_asset(id: u32) {
    let meta = AssetMetadata {
        name: BoundedName::try_from(b"Token".to_vec()).unwrap(),
        symbol: BoundedSymbol::try_from(b"TKN".to_vec()).unwrap(),
        decimals: 12,
        total_supply_declared: 1_000_000,
        genesis_issued: 0,
        mined_to_date: 0,
        mineable: true,
        redeemable_eligible: true,
        reserve_account: None,
    };
    let params = MiningParams {
        r_base_per_period: 1000,
        k_curve: RATE_PRECISION,
        min_multiplier: RATE_PRECISION / 100,
        period_blocks: 1,
        bonus_budget: 10_000,
        bonus_consumed: 0,
        b_max_rate: RATE_PRECISION / 10,
    };
    assert_ok!(AssetsReg::register_asset(RuntimeOrigin::root(), id, meta, Some(params), MiningStatus::Open));
}

#[test]
fn create_pool_and_join_auto_places() {
    new_test_ext().execute_with(|| {
        register_mineable_asset(10);
        assert_ok!(Mining::<Test>::create_pool(RuntimeOrigin::signed(1), 10));
        assert_ok!(Mining::<Test>::create_pool(RuntimeOrigin::signed(1), 10));
        // join preferred pool 0, amount 100
        assert_ok!(Mining::<Test>::join_pool(RuntimeOrigin::signed(2), 10, 0, 100));
        assert!(Mining::<Test>::pools(0).unwrap().total_deposit >= 100 || Mining::<Test>::pools(1).unwrap().total_deposit >= 100);
        assert_eq!(TotalDepositedByAsset::<Test>::get(10), 100);
    });
}

#[test]
fn solo_deposit_tracks_totals() {
    new_test_ext().execute_with(|| {
        register_mineable_asset(11);
        assert_ok!(Mining::<Test>::deposit_solo(RuntimeOrigin::signed(1), 11, 500));
        assert_eq!(TotalDepositedByAsset::<Test>::get(11), 500);
        assert_noop!(Mining::<Test>::withdraw_solo(RuntimeOrigin::signed(1), 11, 600), Error::<Test>::InsufficientDeposit);
        assert_ok!(Mining::<Test>::withdraw_solo(RuntimeOrigin::signed(1), 11, 200));
        assert_eq!(TotalDepositedByAsset::<Test>::get(11), 300);
    });
}
