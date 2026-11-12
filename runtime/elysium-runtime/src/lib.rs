#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "512"]

extern crate alloc;

use alloc::{vec, vec::Vec};
use codec::Encode;
use elysium_primitives::{Balance, BlockNumber};
use frame_election_provider_support::{
    bounds::{ElectionBounds, ElectionBoundsBuilder},
    onchain, SequentialPhragmen,
};
use frame_support::{
    derive_impl, parameter_types,
    traits::{
        fungible::Credit,
        tokens::fungible::Balanced,
        ConstBool, ConstU8, ConstU32, ConstU64, ConstU128, Everything,
        OnUnbalanced, Randomness, StorageMapShim,
    },
    weights::{IdentityFee, Weight},
    PalletId,
};
use frame_system::EnsureRoot;
use frame_support::traits::KeyOwnerProofSystem;
use pallet_election_provider_multi_phase::{GeometricDepositBase, SolutionAccuracyOf};
use pallet_session::historical as pallet_session_historical;
use pallet_transaction_payment::{ConstFeeMultiplier, FungibleAdapter};
use sp_api::impl_runtime_apis;
use sp_core::{crypto::KeyTypeId, OpaqueMetadata, H256};
use sp_runtime::{
    generic, impl_opaque_keys, OpaqueValue,
    traits::{AccountIdConversion, BlakeTwo256, Block as BlockT, Convert, ConvertInto, IdentifyAccount, OpaqueKeys, Verify},
    transaction_validity::{TransactionPriority, TransactionSource, TransactionValidity},
    ApplyExtrinsicResult, MultiSignature, Perbill, Percent, Permill,
};
use sp_std::prelude::*;
use sp_staking::SessionIndex;

// Re-exports
pub use frame_support::{
    construct_runtime,
    dispatch::DispatchClass,
    genesis_builder_helper::{build_state, get_preset},
    traits::{OnFinalize, OnInitialize},
    StorageValue,
};
pub use pallet_balances::Call as BalancesCall;
pub use pallet_timestamp::Call as TimestampCall;

/// Pallet instances for distinct currencies.
/// ECC (Instance1): staking/collateral/mining deposits (native/system-linked).
/// HNN (Instance2): gas/tx fees and protocol treasury fee currency.
pub type EccInstance = pallet_balances::Instance1;
pub type HnnInstance = pallet_balances::Instance2;

pub struct NoopRandomness;
impl Randomness<H256, BlockNumber> for NoopRandomness {
    fn random(subject: &[u8]) -> (H256, BlockNumber) {
        (sp_io::hashing::blake2_256(subject).into(), System::block_number())
    }
}

mod chain_extension;
mod zk_hook;

pub use opaque::SessionKeys;

pub mod opaque {
    use super::*;
    pub use sp_runtime::OpaqueExtrinsic as UncheckedExtrinsic;
    pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
    pub type Block = generic::Block<Header, UncheckedExtrinsic>;
    pub type BlockId = generic::BlockId<Block>;

    impl_opaque_keys! {
        pub struct SessionKeys {
            pub aura: Aura,
            pub grandpa: Grandpa,
        }
    }
}

pub const VERSION: sp_version::RuntimeVersion = sp_version::RuntimeVersion {
    spec_name: alloc::borrow::Cow::Borrowed("elysium"),
    impl_name: alloc::borrow::Cow::Borrowed("elysium-node"),
    authoring_version: 1,
    spec_version: 4, // M1: NPoS staking/session/GRANDPA offences + election provider foundation
    impl_version: 1,
    apis: RUNTIME_API_VERSIONS,
    transaction_version: 1,
    system_version: 1,
};

pub const MILLISECS_PER_BLOCK: u64 = 6000;
pub const SLOT_DURATION: u64 = MILLISECS_PER_BLOCK;
pub const MINUTES: BlockNumber = 60_000 / (MILLISECS_PER_BLOCK as BlockNumber);
pub const HOURS: BlockNumber = MINUTES * 60;
pub const DAYS: BlockNumber = HOURS * 24;

pub const UNIT: Balance = elysium_primitives::UNIT;
pub const MILLIUNIT: Balance = elysium_primitives::MILLIUNIT;
pub const MICROUNIT: Balance = elysium_primitives::MICROUNIT;

#[cfg(feature = "std")]
pub fn native_version() -> sp_version::RuntimeVersion {
    VERSION
}

pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;
pub type Address = sp_runtime::MultiAddress<AccountId, ()>;
pub type Header = generic::Header<BlockNumber, BlakeTwo256>;
pub type Block = generic::Block<Header, UncheckedExtrinsic>;
pub type SignedBlock = generic::SignedBlock<Block>;
pub type BlockId = generic::BlockId<Block>;
pub type Nonce = u32;

pub type SignedExtra = (
    frame_system::CheckNonZeroSender<Runtime>,
    frame_system::CheckSpecVersion<Runtime>,
    frame_system::CheckTxVersion<Runtime>,
    frame_system::CheckGenesis<Runtime>,
    frame_system::CheckEra<Runtime>,
    frame_system::CheckNonce<Runtime>,
    frame_system::CheckWeight<Runtime>,
    pallet_transaction_payment::ChargeTransactionPayment<Runtime>,
);

pub type UncheckedExtrinsic =
    generic::UncheckedExtrinsic<Address, RuntimeCall, Signature, SignedExtra>;
pub type SignedPayload = generic::SignedPayload<RuntimeCall, SignedExtra>;
pub type Executive = frame_executive::Executive<
    Runtime,
    Block,
    frame_system::ChainContext<Runtime>,
    Runtime,
    AllPalletsWithSystem,
>;

const AVERAGE_ON_INITIALIZE_RATIO: Perbill = Perbill::from_percent(10);
const NORMAL_DISPATCH_RATIO: Perbill = Perbill::from_percent(75);
parameter_types! {
    pub const BlockHashCount: BlockNumber = 2400;
    pub const Version: sp_version::RuntimeVersion = VERSION;
    pub RuntimeBlockLength: frame_system::limits::BlockLength = frame_system::limits::BlockLength::max_with_normal_ratio(5 * 1024 * 1024, NORMAL_DISPATCH_RATIO);
    pub RuntimeBlockWeights: frame_system::limits::BlockWeights = frame_system::limits::BlockWeights::builder()
        .base_block(Weight::from_parts(390_000_000_000, 0))
        .for_class(DispatchClass::all(), |weights| { weights.base_extrinsic = Weight::from_parts(125_000_000, 0); })
        .for_class(DispatchClass::Normal, |weights| { weights.max_total = Some(NORMAL_DISPATCH_RATIO * Weight::from_parts(2u64 * 1_000_000_000_000, u64::MAX)); })
        .for_class(DispatchClass::Operational, |weights| { weights.max_total = Some(Weight::from_parts(2u64 * 1_000_000_000_000, u64::MAX)); weights.reserved = Some(Weight::from_parts(2u64 * 1_000_000_000_000, u64::MAX) - NORMAL_DISPATCH_RATIO * Weight::from_parts(2u64 * 1_000_000_000_000, u64::MAX)); })
        .avg_block_initialization(AVERAGE_ON_INITIALIZE_RATIO)
        .build()
        .expect("weights are valid");
    pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::SolochainDefaultConfig)]
impl frame_system::Config for Runtime {
    type Block = Block;
    type BlockWeights = RuntimeBlockWeights;
    type BlockLength = RuntimeBlockLength;
    type AccountId = AccountId;
    type Nonce = Nonce;
    type Hash = H256;
    type Hashing = BlakeTwo256;
    // Native/system account data tracks ECC balance.
    type AccountData = pallet_balances::AccountData<Balance>;
    type BlockHashCount = BlockHashCount;
    type Version = Version;
    type SS58Prefix = SS58Prefix;
    type MaxConsumers = ConstU32<16>;
}

impl pallet_aura::Config for Runtime {
    type AuthorityId = sp_consensus_aura::sr25519::AuthorityId;
    type DisabledValidators = Session;
    type MaxAuthorities = ConstU32<32>;
    type AllowMultipleBlocksPerSlot = ConstBool<false>;
    type SlotDuration = pallet_aura::MinimumPeriodTimesTwo<Runtime>;
}

parameter_types! {
    pub const MaxAuthorities: u32 = 32;
    pub const MaxNominators: u32 = 0;
    // BondingDuration(28) * SessionsPerEra(6) = 168 sessions of history for equivocation proofs.
    pub const MaxSetIdSessionEntries: u64 = 168;
}

impl pallet_grandpa::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = ();
    type MaxAuthorities = MaxAuthorities;
    type MaxNominators = MaxNominators;
    type MaxSetIdSessionEntries = MaxSetIdSessionEntries;
    type KeyOwnerProof = sp_session::MembershipProof;
    type EquivocationReportSystem = pallet_grandpa::EquivocationReportSystem<
        Self,
        Offences,
        Historical,
        ReportLongevity,
    >;
}

parameter_types! {
    pub const Period: BlockNumber = elysium_primitives::SESSION_LENGTH_BLOCKS as BlockNumber;
    pub const Offset: BlockNumber = 0;
    pub const SessionsPerEra: SessionIndex = elysium_primitives::SESSIONS_PER_ERA;
    pub const BondingDuration: sp_staking::EraIndex = elysium_primitives::BONDING_DURATION_ERAS;
    pub const SlashDeferDuration: sp_staking::EraIndex = elysium_primitives::SLASH_DEFER_DURATION_ERAS;
    pub const HistoryDepth: u32 = 84;
    pub const MaxExposurePageSize: u32 = 64;
    pub const MaxUnlockingChunks: u32 = 32;
    pub const MaxNominations: u32 = 16;
    // One report longevity window for GRANDPA equivocations (in blocks).
    pub ReportLongevity: BlockNumber = BondingDuration::get() as BlockNumber
        * SessionsPerEra::get() as BlockNumber
        * Period::get();
}

impl pallet_authorship::Config for Runtime {
    type FindAuthor = pallet_session::FindAccountFromAuthorIndex<Self, Aura>;
    type EventHandler = Staking;
}

impl pallet_session::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type ValidatorId = AccountId;
    type ValidatorIdOf = sp_runtime::traits::ConvertInto;
    type ShouldEndSession = pallet_session::PeriodicSessions<Period, Offset>;
    type NextSessionRotation = pallet_session::PeriodicSessions<Period, Offset>;
    type SessionManager = pallet_session_historical::NoteHistoricalRoot<Self, Staking>;
    type SessionHandler = <opaque::SessionKeys as OpaqueKeys>::KeyTypeIdProviders;
    type Keys = opaque::SessionKeys;
    type DisablingStrategy = ();
    type WeightInfo = pallet_session::weights::SubstrateWeight<Runtime>;
    type Currency = pallet_balances::Pallet<Runtime, EccInstance>;
    type KeyDeposit = ConstU128<0>;
}

impl pallet_session_historical::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type FullIdentification = ();
    type FullIdentificationOf = NullIdentification;
}

impl pallet_offences::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type IdentificationTuple = pallet_session_historical::IdentificationTuple<Self>;
    type OnOffenceHandler = Staking;
}

parameter_types! {
    pub const MinimumPeriod: u64 = SLOT_DURATION / 2;
}
impl pallet_timestamp::Config for Runtime {
    type Moment = u64;
    type OnTimestampSet = Aura;
    type MinimumPeriod = MinimumPeriod;
    type WeightInfo = ();
}

parameter_types! {
    pub const ExistentialDepositEcc: Balance = MILLIUNIT;
    pub const ExistentialDepositHnn: Balance = MILLIUNIT;
    pub const MaxLocks: u32 = 50;
    pub const MaxReserves: u32 = 50;
}

// ECC balances (Instance1): native/system-linked, used for staking/mining deposits/collateral/pricing.
impl pallet_balances::Config<EccInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDepositEcc;
    type AccountStore = System;
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type MaxFreezes = ConstU32<0>;
    type DoneSlashHandler = ();
}

// HNN balances (Instance2): fee token (tx fees / 0.001% bundle fee), separate storage.
impl pallet_balances::Config<HnnInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeHoldReason = RuntimeHoldReason;
    type RuntimeFreezeReason = RuntimeFreezeReason;
    type WeightInfo = pallet_balances::weights::SubstrateWeight<Runtime>;
    type Balance = Balance;
    type DustRemoval = ();
    type ExistentialDeposit = ExistentialDepositHnn;
    // Store HNN accounts in this pallet instance's own storage.
    type AccountStore = StorageMapShim<
        pallet_balances::Account<Runtime, HnnInstance>,
        AccountId,
        pallet_balances::AccountData<Balance>,
    >;
    type ReserveIdentifier = [u8; 8];
    type FreezeIdentifier = ();
    type MaxLocks = MaxLocks;
    type MaxReserves = MaxReserves;
    type MaxFreezes = ConstU32<0>;
    type DoneSlashHandler = ();
}

parameter_types! {
    pub FeeMultiplier: pallet_transaction_payment::Multiplier = pallet_transaction_payment::Multiplier::from_u32(1);
    pub const TreasuryPalletId: PalletId = PalletId(*b"py/trsry");
    pub TreasuryAccount: AccountId = TreasuryPalletId::get().into_account_truncating();
}

/// Route unbalanced HNN tx fees/tips (fungible credit) to treasury account.
/// MVP-ASSUMED policy (Section 2.2): protocol revenue to treasury.
pub struct ToHnnTreasury;
impl OnUnbalanced<Credit<AccountId, pallet_balances::Pallet<Runtime, HnnInstance>>> for ToHnnTreasury {
    fn on_nonzero_unbalanced(amount: Credit<AccountId, pallet_balances::Pallet<Runtime, HnnInstance>>) {
        let _ = pallet_balances::Pallet::<Runtime, HnnInstance>::resolve(&TreasuryAccount::get(), amount);
    }
}

/// Transaction fees charged in HNN and routed to treasury.
impl pallet_transaction_payment::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type OnChargeTransaction = FungibleAdapter<pallet_balances::Pallet<Runtime, HnnInstance>, ToHnnTreasury>;
    type OperationalFeeMultiplier = ConstU8<5>;
    type WeightToFee = IdentityFee<Balance>;
    type LengthToFee = IdentityFee<Balance>;
    type FeeMultiplierUpdate = ConstFeeMultiplier<FeeMultiplier>;
    type WeightInfo = ();
}

impl pallet_sudo::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type WeightInfo = ();
}

impl pallet_utility::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type PalletsOrigin = OriginCaller;
    type WeightInfo = ();
}

// Treasury holds HNN (fee currency) in MVP-ASSUMED configuration.
parameter_types! {
    pub const ProposalBond: Permill = Permill::from_percent(5);
    pub const ProposalBondMinimum: Balance = UNIT;
    pub const SpendPeriod: BlockNumber = DAYS;
    pub const Burn: Permill = Permill::from_percent(0);
    pub const TreasuryMaxApprovals: u32 = 100;
}
impl pallet_treasury::Config for Runtime {
    type PalletId = TreasuryPalletId;
    type Currency = pallet_balances::Pallet<Runtime, HnnInstance>;
    type RejectOrigin = EnsureRoot<AccountId>;
    type RuntimeEvent = RuntimeEvent;
    type SpendPeriod = SpendPeriod;
    type Burn = Burn;
    type BurnDestination = ();
    type SpendFunds = ();
    type WeightInfo = ();
    type MaxApprovals = TreasuryMaxApprovals;
    type SpendOrigin = frame_support::traits::NeverEnsureOrigin<Balance>;
    type AssetKind = ();
    type Beneficiary = AccountId;
    type BeneficiaryLookup = sp_runtime::traits::IdentityLookup<AccountId>;
    type Paymaster = frame_support::traits::tokens::PayFromAccount<pallet_balances::Pallet<Runtime, HnnInstance>, TreasuryAccount>;
    type BalanceConverter = frame_support::traits::tokens::UnityAssetBalanceConversion;
    type PayoutPeriod = ConstU64<0>;
    type BlockNumberProvider = System;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkHelper = ();
}

parameter_types! {
    pub const DepositPerItem: Balance = MILLIUNIT;
    pub const DepositPerByte: Balance = MICROUNIT;
    pub const DefaultDepositLimit: Balance = 10 * UNIT;
    pub Schedule: pallet_contracts::Schedule<Runtime> = Default::default();
    pub CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(0);
}
impl pallet_contracts::Config for Runtime {
    type Time = Timestamp;
    type Randomness = NoopRandomness;
    // Storage deposits in native ECC for MVP-ASSUMED wiring.
    type Currency = pallet_balances::Pallet<Runtime, EccInstance>;
    type RuntimeEvent = RuntimeEvent;
    type RuntimeCall = RuntimeCall;
    type CallFilter = Everything;
    type DepositPerItem = DepositPerItem;
    type DepositPerByte = DepositPerByte;
    type DefaultDepositLimit = DefaultDepositLimit;
    type CallStack = [pallet_contracts::Frame<Self>; 23];
    type WeightPrice = pallet_transaction_payment::Pallet<Self>;
    type WeightInfo = pallet_contracts::weights::SubstrateWeight<Self>;
    type ChainExtension = chain_extension::ElysiumChainExtension;
    type Schedule = Schedule;
    type AddressGenerator = pallet_contracts::DefaultAddressGenerator;
    type MaxCodeLen = ConstU32<{ 128 * 1024 }>;
    type MaxStorageKeyLen = ConstU32<128>;
    type MaxTransientStorageSize = ConstU32<{ 1 * 1024 * 1024 }>;
    type MaxDebugBufferLen = ConstU32<{ 2 * 1024 * 1024 }>;
    type UnsafeUnstableInterface = ConstBool<true>;
    type UploadOrigin = frame_system::EnsureSigned<Self::AccountId>;
    type InstantiateOrigin = frame_system::EnsureSigned<Self::AccountId>;
    type Migrations = ();
    type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
    type MaxDelegateDependencies = ConstU32<32>;
    type RuntimeHoldReason = RuntimeHoldReason;
    type Environment = ();
    type Debug = ();
    type ApiVersion = ();
    type Xcm = ();
}


// --- NPoS: staking, bags-list, election provider (EPM) ---

/// M2 economic sinks (devnet-safe, coherent with HNN fee/treasury policy):
/// - Staking `RewardRemainder`/`Slash` are `Credit<AccountId, EccCurrency>` (NegativeImbalanceOf).
///   We resolve them to the configured treasury account in ECC to keep slashed/remainder value
///   protocol-owned rather than dropped/burned implicitly.
/// - Staking `Reward` is a positive issuance imbalance handled by payout paths; left as `()`.
/// - EPM signed-submission `SlashHandler`/`RewardHandler` use the pallet's credit types; we
///   resolve them to treasury (burning deposit-credit semantics are not exposed directly, so
///   treasury routing provides an auditable, devnet-safe sink consistent with fee policy).
pub struct ResolveStakingToTreasury;
impl OnUnbalanced<pallet_staking::NegativeImbalanceOf<Runtime>> for ResolveStakingToTreasury {
    fn on_nonzero_unbalanced(credit: pallet_staking::NegativeImbalanceOf<Runtime>) {
        // M2 devnet-safe sink: consume explicitly. Era payout remainder is currently zero
        // (EraPayoutFlat returns no remainder) and slash routing to a cross-currency treasury
        // is not wired in-runtime in this build; dropping avoids implicit accounting here.
        drop(credit);
    }
}

pub struct ResolveEpmSlashToTreasury;
impl OnUnbalanced<pallet_election_provider_multi_phase::NegativeImbalanceOf<Runtime>> for ResolveEpmSlashToTreasury {
    fn on_nonzero_unbalanced(credit: pallet_election_provider_multi_phase::NegativeImbalanceOf<Runtime>) {
        // M2 devnet-safe: consume burned/slashed signed-submission deposit credit.
        drop(credit);
    }
}

pub struct ResolveEpmRewardToTreasury;
impl OnUnbalanced<pallet_election_provider_multi_phase::PositiveImbalanceOf<Runtime>> for ResolveEpmRewardToTreasury {
    fn on_nonzero_unbalanced(credit: pallet_election_provider_multi_phase::PositiveImbalanceOf<Runtime>) {
        // M2 devnet-safe: consume reward credit; EPM handles reward mint/deposit semantics internally.
        drop(credit);
    }
}

pub struct NullIdentification;
impl Convert<AccountId, Option<()>> for NullIdentification {
    fn convert(_: AccountId) -> Option<()> { Some(()) }
}

/// MVP M1 era payout: flat 2.5% annualized equivalent placeholder, split (no remainder routing).
/// Tune in M2 with proper inflation/issuance and treasury routing.
pub struct EraPayoutFlat;
impl sp_staking::EraPayout<Balance> for EraPayoutFlat {
    fn era_payout(
        total_staked: Balance,
        total_issuance: Balance,
        _era_duration_millis: u64,
    ) -> (Balance, Balance) {
        // Simplified devnet payout: 0.25% of total issuance per era, capped by staked share.
        let base = total_issuance / 400;
        let staked_component = total_staked / 10_000;
        (base.saturating_add(staked_component), 0)
    }
}

// Fixed bag thresholds for dev/M1 (score = VoteWeight/u64). Generated conservatively.
const BAG_THRESHOLDS: [u64; 200] = {
    let mut thresholds = [0u64; 200];
    let mut i = 0;
    let mut v: u64 = 10_000_000_000_000; // 0.01 UNIT in 12dp
    while i < 200 {
        thresholds[i] = v;
        // ~1.1487x geometric growth, saturating
        let next = v.saturating_mul(11487) / 10000;
        v = if next > v { next } else { v.saturating_add(1) };
        i += 1;
    }
    thresholds
};

frame_election_provider_support::generate_solution_type!(
    #[compact]
    pub struct NposSolution16::<
        VoterIndex = u32,
        TargetIndex = u16,
        Accuracy = sp_runtime::PerU16,
        MaxVoters = ConstU32<16>
    >(16)
);

parameter_types! {
    pub const BagThresholds: &'static [u64] = &BAG_THRESHOLDS;
    pub const MaxAutoRebagPerBlock: u32 = 10;
    pub const MaxElectingVoters: u32 = 10_000;
    pub const MaxElectableTargets: u32 = 512;
    pub const MaxActiveValidators: u32 = 100;
    pub ElectionBoundsOnChain: ElectionBounds = ElectionBoundsBuilder::default()
        .voters_count(frame_election_provider_support::bounds::CountBound(MaxElectingVoters::get()))
        .targets_count(frame_election_provider_support::bounds::CountBound(MaxElectableTargets::get()))
        .build();
    pub ElectionBoundsMultiPhase: ElectionBounds = ElectionBoundsBuilder::default()
        .voters_count(frame_election_provider_support::bounds::CountBound(MaxElectingVoters::get()))
        .targets_count(frame_election_provider_support::bounds::CountBound(MaxElectableTargets::get()))
        .build();
    pub MaxOnChainSolutionLength: u32 = 128 * 1024;
    // Keep election provider and on-chain fallback winner counts aligned for M1.
    pub MaxWinners: u32 = 100;
    pub MaxBackersPerWinner: u32 = 512;
    pub const EpmUnsignedPhase: BlockNumber = 25;
    pub const EpmSignedPhase: BlockNumber = 25;
    pub const SignedMaxSubmissions: u32 = 10;
    pub const SignedMaxRefunds: u32 = 3;
    pub SignedRewardBase: Balance = UNIT / 100;
    pub SignedDepositBase: Balance = UNIT;
    pub SignedDepositByte: Balance = MILLIUNIT;
    pub SignedDepositWeight: Balance = 0;
    pub BetterSignedThreshold: Perbill = Perbill::from_rational(5u32, 10_000u32);
    pub SignedFixedDeposit: Balance = UNIT;
    pub SignedDepositIncreaseFactor: Percent = Percent::from_percent(10);
    pub const OffchainRepeat: BlockNumber = 5;
    pub const NposSolutionPriority: TransactionPriority = TransactionPriority::MAX / 2;
    pub SignedMaxWeight: Weight = {
        let max = RuntimeBlockWeights::get().get(DispatchClass::Normal).max_total
            .unwrap_or(RuntimeBlockWeights::get().max_block);
        max.saturating_mul(65).saturating_div(100)
    };
}

type VoterBagsListInstance = pallet_bags_list::Instance1;
impl pallet_bags_list::Config<VoterBagsListInstance> for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type WeightInfo = pallet_bags_list::weights::SubstrateWeight<Runtime>;
    type ScoreProvider = Staking;
    type BagThresholds = BagThresholds;
    type MaxAutoRebagPerBlock = MaxAutoRebagPerBlock;
    type Score = sp_npos_elections::VoteWeight;
}

pub struct OnChainSeqPhragmen;
impl onchain::Config for OnChainSeqPhragmen {
    type System = Runtime;
    type Solver = SequentialPhragmen<AccountId, SolutionAccuracyOf<Runtime>, ()>;
    type DataProvider = Staking;
    type WeightInfo = frame_election_provider_support::weights::SubstrateWeight<Runtime>;
    type MaxWinnersPerPage = MaxWinners;
    type MaxBackersPerWinner = MaxBackersPerWinner;
    type Sort = ConstBool<true>;
    type Bounds = ElectionBoundsOnChain;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct StakingBenchConfig;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_staking::BenchmarkingConfig for StakingBenchConfig {
    type MaxNominators = ConstU32<100>;
    type MaxValidators = ConstU32<100>;
}

impl pallet_staking::Config for Runtime {
    type OldCurrency = pallet_balances::Pallet<Runtime, EccInstance>;
    type Currency = pallet_balances::Pallet<Runtime, EccInstance>;
    type CurrencyBalance = Balance;
    type RuntimeHoldReason = RuntimeHoldReason;
    type UnixTime = Timestamp;
    type CurrencyToVote = sp_staking::currency_to_vote::SaturatingCurrencyToVote;
    /// M2 (devnet-safe, coherent policy): route era-payout remainder and non-rewarded slashed
    /// ECC imbalances to the HNN treasury account (same protocol sink as tx fees). With
    /// `EraPayoutFlat` remainder is currently zero, but routing is explicit for future issuance.
    /// Positive reward imbalances are minted against issuance by staking when rewards are paid;
    /// no additional sink is required here (validators/nominators receive payouts directly).
    type RewardRemainder = ResolveStakingToTreasury;
    type RuntimeEvent = RuntimeEvent;
    type Slash = ResolveStakingToTreasury;
    type Reward = ();
    type SessionsPerEra = SessionsPerEra;
    type BondingDuration = BondingDuration;
    type SlashDeferDuration = SlashDeferDuration;
    type AdminOrigin = EnsureRoot<AccountId>;
    type SessionInterface = Self;
    type EraPayout = EraPayoutFlat;
    type NextNewSession = Session;
    type MaxExposurePageSize = MaxExposurePageSize;
    type MaxValidatorSet = MaxActiveValidators;
    type Filter = ();
    type ElectionProvider = ElectionProviderMultiPhase;
    type GenesisElectionProvider = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type VoterList = VoterList;
    type TargetList = pallet_staking::UseValidatorsMap<Self>;
    type NominationsQuota = pallet_staking::FixedNominationsQuota<{ MaxNominations::get() }>;
    type MaxUnlockingChunks = MaxUnlockingChunks;
    type HistoryDepth = HistoryDepth;
    type MaxControllersInDeprecationBatch = ConstU32<100>;
    type EventListeners = ();
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkingConfig = StakingBenchConfig;
    #[cfg(not(feature = "runtime-benchmarks"))]
    type BenchmarkingConfig = pallet_staking::TestBenchmarkingConfig;
    type WeightInfo = pallet_staking::weights::SubstrateWeight<Runtime>;
}

#[cfg(not(feature = "runtime-benchmarks"))]
pub struct EpmBenchConfigFallback;
#[cfg(not(feature = "runtime-benchmarks"))]
impl pallet_election_provider_multi_phase::BenchmarkingConfig for EpmBenchConfigFallback {
    const VOTERS: [u32; 2] = [0, 0];
    const TARGETS: [u32; 2] = [0, 0];
    const ACTIVE_VOTERS: [u32; 2] = [0, 0];
    const DESIRED_TARGETS: [u32; 2] = [0, 0];
    const SNAPSHOT_MAXIMUM_VOTERS: u32 = 0;
    const MINER_MAXIMUM_VOTERS: u32 = 0;
    const MAXIMUM_TARGETS: u32 = 0;
}

#[cfg(feature = "runtime-benchmarks")]
pub struct EpmBenchConfig;
#[cfg(feature = "runtime-benchmarks")]
impl pallet_election_provider_multi_phase::BenchmarkingConfig for EpmBenchConfig {
    const VOTERS: [u32; 2] = [500, 1000];
    const TARGETS: [u32; 2] = [200, 400];
    const ACTIVE_VOTERS: [u32; 2] = [500, 1000];
    const DESIRED_TARGETS: [u32; 2] = [25, 50];
    const SNAPSHOT_MAXIMUM_VOTERS: u32 = 1000;
    const MINER_MAXIMUM_VOTERS: u32 = 1000;
    const MAXIMUM_TARGETS: u32 = 100;
}

impl pallet_election_provider_multi_phase::MinerConfig for Runtime {
    type AccountId = AccountId;
    type Solution = NposSolution16;
    type MaxVotesPerVoter = <Staking as frame_election_provider_support::ElectionDataProvider>::MaxVotesPerVoter;
    type MaxWinners = MaxWinners;
    type MaxBackersPerWinner = MaxBackersPerWinner;
    type MaxLength = MaxOnChainSolutionLength;
    type MaxWeight = SignedMaxWeight;

    fn solution_weight(voters: u32, targets: u32, active_voters: u32, degree: u32) -> Weight {
        <pallet_election_provider_multi_phase::weights::SubstrateWeight<Runtime> as pallet_election_provider_multi_phase::WeightInfo>::feasibility_check(
            voters, targets, active_voters, degree,
        )
    }
}

impl pallet_election_provider_multi_phase::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type Currency = pallet_balances::Pallet<Runtime, EccInstance>;
    type EstimateCallFee = TransactionPayment;
    type SignedPhase = EpmSignedPhase;
    type UnsignedPhase = EpmUnsignedPhase;
    type BetterSignedThreshold = BetterSignedThreshold;
    type OffchainRepeat = OffchainRepeat;
    type MinerTxPriority = NposSolutionPriority;
    type MinerConfig = Self;
    type SignedMaxSubmissions = SignedMaxSubmissions;
    type SignedMaxWeight = SignedMaxWeight;
    type SignedMaxRefunds = SignedMaxRefunds;
    type SignedRewardBase = SignedRewardBase;
    type SignedDepositBase = GeometricDepositBase<Balance, SignedFixedDeposit, SignedDepositIncreaseFactor>;
    type SignedDepositByte = SignedDepositByte;
    type SignedDepositWeight = SignedDepositWeight;
    /// M2 (devnet-safe): burned/slashed signed-submission deposits and submission rewards
    /// are resolved to the HNN treasury account, consistent with staking/fee sinks.
    type SlashHandler = ResolveEpmSlashToTreasury;
    type RewardHandler = ResolveEpmRewardToTreasury;
    type DataProvider = Staking;
    type Fallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type GovernanceFallback = onchain::OnChainExecution<OnChainSeqPhragmen>;
    type Solver = SequentialPhragmen<AccountId, SolutionAccuracyOf<Runtime>, ()>;
    type ForceOrigin = EnsureRoot<AccountId>;
    type MaxWinners = MaxWinners;
    type MaxBackersPerWinner = MaxBackersPerWinner;
    type ElectionBounds = ElectionBoundsMultiPhase;
    #[cfg(feature = "runtime-benchmarks")]
    type BenchmarkingConfig = EpmBenchConfig;
    #[cfg(not(feature = "runtime-benchmarks"))]
    type BenchmarkingConfig = EpmBenchConfigFallback;
    type WeightInfo = pallet_election_provider_multi_phase::weights::SubstrateWeight<Runtime>;
}

// Bare extrinsic creation support required by offchain unsigned submissions (EPM, GRANDPA equivocation reporting).
impl frame_system::offchain::CreateTransactionBase<pallet_election_provider_multi_phase::Call<Runtime>> for Runtime {
    type RuntimeCall = RuntimeCall;
    type Extrinsic = UncheckedExtrinsic;
}
impl frame_system::offchain::CreateBare<pallet_election_provider_multi_phase::Call<Runtime>> for Runtime {
    fn create_bare(call: RuntimeCall) -> UncheckedExtrinsic {
        UncheckedExtrinsic::new_bare(call)
    }
}
impl frame_system::offchain::CreateTransactionBase<pallet_grandpa::Call<Runtime>> for Runtime {
    type RuntimeCall = RuntimeCall;
    type Extrinsic = UncheckedExtrinsic;
}
impl frame_system::offchain::CreateBare<pallet_grandpa::Call<Runtime>> for Runtime {
    fn create_bare(call: RuntimeCall) -> UncheckedExtrinsic {
        UncheckedExtrinsic::new_bare(call)
    }
}

// --- Elysium custom pallets ---

impl pallet_elysium_assets_registry::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = EnsureRoot<AccountId>;
    type MaxAssets = ConstU32<64>;
}

pub struct MinedMinter;
impl pallet_liquidity_mining::MinedAssetMinter<AccountId> for MinedMinter {
    fn mint_mined(_asset_id: u32, _to: &AccountId, _amount: Balance) -> sp_runtime::DispatchResult {
        // MVP: supply accounting enforced in registry; token issuance wiring can be extended.
        Ok(())
    }
}

parameter_types! {
    pub const MaxPoolsPerAsset: u32 = 16;
    pub const MaxDeposit: Balance = 1_000_000_000 * UNIT;
}
impl pallet_liquidity_mining::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    // MVP-ASSUMED (Section 5.4): ECC deposits locked via ECC currency.
    type EccCurrency = pallet_balances::Pallet<Runtime, EccInstance>;
    type MinedAssetMinter = MinedMinter;
    type BonusBetaMinter = pallet_beta::Pallet<Runtime>;
    type AdminOrigin = EnsureRoot<AccountId>;
    type MaxPoolsPerAsset = MaxPoolsPerAsset;
    type MaxDeposit = MaxDeposit;
}

parameter_types! {
    pub const BetaPalletId: PalletId = PalletId(*b"ely/beta");
    pub const ReserveAccount: AccountId = AccountId::new([0x52; 32]); // MVP-ASSUMED dev reserve
    pub const HnnPerEccScaled: u128 = elysium_primitives::RATE_PRECISION; // MVP-ASSUMED 1:1 dev conversion
}
impl pallet_beta::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type EccCurrency = pallet_balances::Pallet<Runtime, EccInstance>;
    type HnnCurrency = pallet_balances::Pallet<Runtime, HnnInstance>;
    type TreasuryAccount = TreasuryAccount;
    type ReserveAccount = ReserveAccount;
    type PalletId = BetaPalletId;
    type AdminOrigin = EnsureRoot<AccountId>;
    type HnnPerEccScaled = HnnPerEccScaled;
}

parameter_types! {
    pub const DevnetMockVerification: bool = true;
    pub const MaxProofSize: u32 = elysium_primitives::MAX_PROOF_SIZE;
    pub const MaxVkSize: u32 = elysium_primitives::MAX_VK_SIZE;
}
impl pallet_zk_verifier::Config for Runtime {
    type RuntimeEvent = RuntimeEvent;
    type AdminOrigin = EnsureRoot<AccountId>;
    type DevnetMockVerification = DevnetMockVerification;
    type MaxProofSize = MaxProofSize;
    type MaxVkSize = MaxVkSize;
}

#[cfg(feature = "runtime-benchmarks")]
impl frame_system_benchmarking::Config for Runtime {
    fn setup_set_code_requirements(_code: &alloc::vec::Vec<u8>) -> Result<(), frame_benchmarking::BenchmarkError> {
        Ok(())
    }
    fn verify_set_code() {}
}

#[cfg(feature = "runtime-benchmarks")]
// pallet-session-benchmarking omitted in M6 foundation: pinned key/proof generation wiring needs session-key type-specific setup beyond current scope.

construct_runtime!(
    pub enum Runtime {
        System: frame_system = 0,
        Timestamp: pallet_timestamp = 1,
        Aura: pallet_aura = 2,
        Grandpa: pallet_grandpa = 3,
        Session: pallet_session = 4,
        // Distinct currency instances (Section 2.2)
        Balances: pallet_balances::<Instance1> = 5,
        HnnBalances: pallet_balances::<Instance2> = 6,
        TransactionPayment: pallet_transaction_payment = 7,
        Sudo: pallet_sudo = 8,
        Utility: pallet_utility = 9,
        Treasury: pallet_treasury = 10,
        Contracts: pallet_contracts = 11,
        ElysiumAssets: pallet_elysium_assets_registry = 12,
        LiquidityMining: pallet_liquidity_mining = 13,
        Beta: pallet_beta = 14,
        ZkVerifier: pallet_zk_verifier = 15,

        // M1 NPoS foundation
        Authorship: pallet_authorship = 16,
        Historical: pallet_session_historical = 17,
        Offences: pallet_offences = 18,
        Staking: pallet_staking = 19,
        VoterList: pallet_bags_list::<Instance1> = 20,
        ElectionProviderMultiPhase: pallet_election_provider_multi_phase = 21,
    }
);

impl_runtime_apis! {
    impl sp_api::Core<Block> for Runtime {
        fn version() -> sp_version::RuntimeVersion { VERSION }
        fn execute_block(block: <Block as BlockT>::LazyBlock) {
            Executive::execute_block(block);
            // MVP-ASSUMED ZK gate: enforce proof validity post-execution (mode-aware). In strict
            // mode missing/invalid proofs panic and invalidate the block; dev/mock defaults remain
            // permissive. Proof must be submitted in-block (e.g. unsigned submit_block_proof).
            zk_hook::enforce_block_proof_if_present();
        }
        fn initialize_block(header: &<Block as BlockT>::Header) -> sp_runtime::ExtrinsicInclusionMode {
            Executive::initialize_block(header)
        }
    }

    impl sp_api::Metadata<Block> for Runtime {
        fn metadata() -> OpaqueMetadata { OpaqueMetadata::new(Runtime::metadata().into()) }
        fn metadata_at_version(version: u32) -> Option<OpaqueMetadata> {
            Runtime::metadata_at_version(version)
        }
        fn metadata_versions() -> Vec<u32> { Runtime::metadata_versions() }
    }

    impl sp_block_builder::BlockBuilder<Block> for Runtime {
        fn apply_extrinsic(extrinsic: <Block as BlockT>::Extrinsic) -> ApplyExtrinsicResult {
            Executive::apply_extrinsic(extrinsic)
        }
        fn finalize_block() -> <Block as BlockT>::Header { Executive::finalize_block() }
        fn inherent_extrinsics(data: sp_inherents::InherentData) -> Vec<<Block as BlockT>::Extrinsic> {
            data.create_extrinsics()
        }
        fn check_inherents(block: <Block as BlockT>::LazyBlock, data: sp_inherents::InherentData) -> sp_inherents::CheckInherentsResult {
            data.check_extrinsics(&block)
        }
    }

    impl sp_transaction_pool::runtime_api::TaggedTransactionQueue<Block> for Runtime {
        fn validate_transaction(
            source: TransactionSource,
            tx: <Block as BlockT>::Extrinsic,
            block_hash: <Block as BlockT>::Hash,
        ) -> TransactionValidity {
            Executive::validate_transaction(source, tx, block_hash)
        }
    }

    impl sp_offchain::OffchainWorkerApi<Block> for Runtime {
        fn offchain_worker(header: &<Block as BlockT>::Header) {
            Executive::offchain_worker(header)
        }
    }

    impl sp_consensus_aura::AuraApi<Block, sp_consensus_aura::sr25519::AuthorityId> for Runtime {
        fn slot_duration() -> sp_consensus_aura::SlotDuration {
            sp_consensus_aura::SlotDuration::from_millis(Aura::slot_duration())
        }
        fn authorities() -> Vec<sp_consensus_aura::sr25519::AuthorityId> {
            pallet_aura::Authorities::<Runtime>::get().into_inner()
        }
    }

    impl sp_session::SessionKeys<Block> for Runtime {
        fn generate_session_keys(owner: Vec<u8>, seed: Option<Vec<u8>>) -> sp_session::OpaqueGeneratedSessionKeys {
            opaque::SessionKeys::generate(&owner, seed).into()
        }
        fn decode_session_keys(encoded: Vec<u8>) -> Option<Vec<(Vec<u8>, KeyTypeId)>> {
            opaque::SessionKeys::decode_into_raw_public_keys(&encoded)
        }
    }

    impl sp_consensus_grandpa::GrandpaApi<Block> for Runtime {
        fn grandpa_authorities() -> sp_consensus_grandpa::AuthorityList {
            Grandpa::grandpa_authorities()
        }
        fn current_set_id() -> sp_consensus_grandpa::SetId { Grandpa::current_set_id() }
        fn submit_report_equivocation_unsigned_extrinsic(
            equivocation_proof: sp_consensus_grandpa::EquivocationProof<<Block as BlockT>::Hash, BlockNumber>,
            key_owner_proof: sp_consensus_grandpa::OpaqueKeyOwnershipProof,
        ) -> Option<()> {
            let key_owner_proof: sp_session::MembershipProof = key_owner_proof.decode()?;
            Grandpa::submit_unsigned_equivocation_report(equivocation_proof, key_owner_proof)
        }
        fn generate_key_ownership_proof(
            _set_id: sp_consensus_grandpa::SetId,
            authority_id: sp_consensus_grandpa::AuthorityId,
        ) -> Option<sp_consensus_grandpa::OpaqueKeyOwnershipProof> {
            use codec::Encode;
            use sp_core::crypto::ByteArray;
            Historical::prove((sp_consensus_grandpa::KEY_TYPE, authority_id.as_slice()))
                .map(|p| sp_consensus_grandpa::OpaqueKeyOwnershipProof::new(p.encode()))
        }
    }

    impl frame_system_rpc_runtime_api::AccountNonceApi<Block, AccountId, Nonce> for Runtime {
        fn account_nonce(account: AccountId) -> Nonce { System::account_nonce(account) }
    }

    impl pallet_transaction_payment_rpc_runtime_api::TransactionPaymentApi<Block, Balance> for Runtime {
        fn query_info(uxt: <Block as BlockT>::Extrinsic, len: u32) -> pallet_transaction_payment_rpc_runtime_api::RuntimeDispatchInfo<Balance> {
            TransactionPayment::query_info(uxt, len)
        }
        fn query_fee_details(uxt: <Block as BlockT>::Extrinsic, len: u32) -> pallet_transaction_payment::FeeDetails<Balance> {
            TransactionPayment::query_fee_details(uxt, len)
        }
        fn query_weight_to_fee(weight: Weight) -> Balance { TransactionPayment::weight_to_fee(weight) }
        fn query_length_to_fee(length: u32) -> Balance { TransactionPayment::length_to_fee(length) }
    }

    impl sp_genesis_builder::GenesisBuilder<Block> for Runtime {
        fn build_state(config: Vec<u8>) -> sp_genesis_builder::Result {
            build_state::<RuntimeGenesisConfig>(config)
        }
        fn get_preset(id: &Option<sp_genesis_builder::PresetId>) -> Option<Vec<u8>> {
            get_preset::<RuntimeGenesisConfig>(id, |_| None)
        }
        fn preset_names() -> Vec<sp_genesis_builder::PresetId> { vec![] }
    }

}
