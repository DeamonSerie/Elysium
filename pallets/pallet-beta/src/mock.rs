use crate as pallet_beta;
use frame_support::{derive_impl, parameter_types, traits::ConstU32, PalletId};
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
        Beta: pallet_beta,
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
    type MaxLocks = ConstU32<50>;
    type MaxReserves = ConstU32<50>;
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
    type BonusBetaMinter = pallet_beta::Pallet<Test>;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type MaxPoolsPerAsset = MaxPoolsPerAsset;
    type MaxDeposit = MaxDeposit;
}

parameter_types! {
    pub const BetaPalletId: PalletId = PalletId(*b"ely/beta");
    pub const TreasuryAccount: AccountId = 99;
    pub const ReserveAccount: AccountId = 98;
    pub const HnnPerEccScaled: u128 = elysium_primitives::RATE_PRECISION;
}
impl pallet_beta::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type EccCurrency = Balances;
    type HnnCurrency = Balances;
    type TreasuryAccount = TreasuryAccount;
    type ReserveAccount = ReserveAccount;
    type PalletId = BetaPalletId;
    type AdminOrigin = frame_system::EnsureRoot<AccountId>;
    type HnnPerEccScaled = HnnPerEccScaled;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
    pallet_balances::GenesisConfig::<Test> {
        balances: vec![(1, 1_000_000), (2, 1_000_000), (98, 1_000_000), (99, 1_000_000)],
        dev_accounts: None,
    }.assimilate_storage(&mut t).unwrap();
    let mut ext = sp_io::TestExternalities::new(t);
    ext.execute_with(|| System::set_block_number(1));
    ext
}
