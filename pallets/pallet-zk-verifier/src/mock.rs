use crate as pallet_zk_verifier;
use frame_support::{derive_impl, parameter_types};
use sp_runtime::{traits::IdentityLookup, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
    pub enum Test {
        System: frame_system,
        ZkVerifier: pallet_zk_verifier,
    }
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
    type Block = Block;
    type AccountId = u64;
    type Lookup = IdentityLookup<Self::AccountId>;
}

parameter_types! {
    pub const DevnetMockVerification: bool = true;
    pub const MaxProofSize: u32 = 1024 * 1024;
    pub const MaxVkSize: u32 = 512 * 1024;
}

impl pallet_zk_verifier::Config for Test {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = frame_system::EnsureRoot<u64>;
    type DevnetMockVerification = DevnetMockVerification;
    type MaxProofSize = MaxProofSize;
    type MaxVkSize = MaxVkSize;
}

pub fn new_test_ext() -> sp_io::TestExternalities {
    let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
    pallet_zk_verifier::GenesisConfig::<Test> { strict_mode: false, _phantom: Default::default() }
        .assimilate_storage(&mut t).unwrap();
    sp_io::TestExternalities::new(t)
}
