use crate::{mock::*, Error, Event, Pallet as Assets};
use elysium_primitives::{AssetMetadata, BoundedName, BoundedSymbol, MiningParams, MiningStatus};
use frame_support::{assert_noop, assert_ok};

fn meta(total: u128, genesis: u128, mineable: bool) -> AssetMetadata<u128, u64> {
    AssetMetadata {
        name: BoundedName::try_from(b"Elysium".to_vec()).unwrap(),
        symbol: BoundedSymbol::try_from(b"ECC".to_vec()).unwrap(),
        decimals: 12,
        total_supply_declared: total,
        genesis_issued: genesis,
        mined_to_date: 0,
        mineable,
        redeemable_eligible: true,
        reserve_account: None,
    }
}

#[test]
fn register_and_mint_respects_cap() {
    new_test_ext().execute_with(|| {
        System::set_block_number(1);
        let id = 1u32;
        assert_ok!(Assets::<Test>::register_asset(
            RuntimeOrigin::root(),
            id,
            meta(1000, 100, true),
            Some(MiningParams::<u128>::default()),
            MiningStatus::Open,
        ));
        assert_eq!(Assets::<Test>::mineable_remainder(id), Some(900));
        assert_ok!(Assets::<Test>::record_mint(id, 400));
        assert_eq!(Assets::<Test>::mineable_remainder(id), Some(500));
        assert_noop!(Assets::<Test>::record_mint(id, 600), Error::<Test>::SupplyCapExceeded);
        assert_ok!(Assets::<Test>::record_mint(id, 500));
        assert_eq!(Assets::<Test>::mineable_remainder(id), Some(0));
    });
}

#[test]
fn cannot_register_duplicate() {
    new_test_ext().execute_with(|| {
        let id = 2u32;
        assert_ok!(Assets::<Test>::register_asset(
            RuntimeOrigin::root(), id, meta(100, 10, true), None, MiningStatus::Open
        ));
        assert_noop!(Assets::<Test>::register_asset(
            RuntimeOrigin::root(), id, meta(100, 10, true), None, MiningStatus::Open
        ), Error::<Test>::AssetExists);
    });
}
