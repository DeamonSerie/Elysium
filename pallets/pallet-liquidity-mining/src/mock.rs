use crate as pallet_liquidity_mining;
use frame_support::{derive_impl, parameter_types, traits::ConstU32};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;
pub type Balance = u128;
pub type AccountId = u64;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        Balances: pallet_balances,
        AssetsReg: pallet_elysium_assets_registry,
        Mining: pallet_liquidity_mining,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = AccountId;
    type Lookup = IdentityLookup<Self::AccountId>;
    type AccountData = pallet_balances::AccountData<Balance>;
}

parameter_types! {
    pub const ExistentialDeposit: Balance = 1;
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}
impl pallet_balances::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type WeightInfo = ();
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDeposit;
    type AccountStore = System;
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type MaxFreezes = ConstU32<0>;
    type DoneSlashHandler = ();
}

parameter_types! { pub const MaxAssets: u32 = 32; }
impl pallet_elysium_assets_registry::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxAssets = MaxAssets;
}

parameter_types! {
    pub const MaxPoolsPerAsset: u32 = 16;
    pub const MaxDeposit: Balance = 1_000_000_000_000_000;
}
impl pallet_liquidity_mining::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type EccCurrency = Balances;
    type MinedAssetMinter = ();
    type BonusBetaMinter = ();
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxPoolsPerAsset = MaxPoolsPerAsset;
    type MaxDeposit = MaxDeposit;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 10_000), (2, 10_000), (3, 10_000)],
        dev_accounts: None,
    }.assimilate_storage(&mut t).unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
