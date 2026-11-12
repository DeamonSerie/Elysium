#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use elysium_primitives::{
        normalized_distance, rate_multiplier, solo_rate_factor_scaled, AssetId, Balance, MiningStatus,
        PoolId, RATE_PRECISION,
    };
    use frame_support::{
        pallet_prelude::*,
        traits::{LockIdentifier, LockableCurrency, WithdrawReasons},
    };
    use frame_system::pallet_prelude::*;
    use pallet_elysium_assets_registry as assets_registry;
    use sp_runtime::traits::{CheckedAdd, Saturating, Zero};
    use sp_std::vec::Vec;

    pub const MINING_LOCK_ID: LockIdentifier = *b"elymine!";

    #[derive(Clone, Encode, Decode, Eq, PartialEq, core::fmt::Debug, TypeInfo, MaxEncodedLen)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct PoolInfo<AccountId> {
        pub asset_id: AssetId,
        pub total_deposit: Balance,
        pub participant_count: u32,
        /// Block when mining started / snapshot taken.
        pub start_block: Option<u64>,
        pub bonus_snapshot_hash: Option<[u8; 32]>,
        pub uniformity_scaled: u128,
        pub complete: bool,
        pub creator: Option<AccountId>,
    }

    impl<AccountId> Default for PoolInfo<AccountId> {
        fn default() -> Self {
            Self {
                asset_id: 0,
                total_deposit: 0,
                participant_count: 0,
                start_block: None,
                bonus_snapshot_hash: None,
                uniformity_scaled: 0,
                complete: false,
                creator: None,
            }
        }
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, core::fmt::Debug, TypeInfo, MaxEncodedLen, Default)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct PoolPosition {
        pub deposit: Balance,
        pub accrued_mined: Balance,
        pub bonus_beta: Balance,
        /// Share basis at last settlement.
        pub reward_debt_scaled: u128,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, core::fmt::Debug, TypeInfo, MaxEncodedLen, Default)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct SoloPosition {
        pub deposit: Balance,
        pub accrued_mined: Balance,
        pub last_settle_block: u64,
    }

    #[derive(Clone, Encode, Decode, Eq, PartialEq, core::fmt::Debug, TypeInfo, MaxEncodedLen, Default)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct DepositSnapshot {
        /// Bucket counts/magnitudes (8 buckets) captured at pool start.
        pub buckets: [Balance; 8],
        pub total: Balance,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + assets_registry::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// ECC currency used for deposits/collateral (lockable).
        type EccCurrency: LockableCurrency<Self::AccountId, Balance = Balance>;
        /// Hook to mint/credit mined target asset to an account (implemented in runtime via beta/asset logic).
        type MinedAssetMinter: MinedAssetMinter<Self::AccountId>;
        /// Hook to mint bonus β credits to pool miners (runtime/beta pallet).
        type BonusBetaMinter: BonusBetaMinter<Self::AccountId>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        #[pallet::constant]
        type MaxPoolsPerAsset: Get<u32>;
        #[pallet::constant]
        type MaxDeposit: Get<Balance>;
    }

    /// Mint/credit mined asset units to miner (e.g. record claimable or mint registry-tracked units).
    pub trait MinedAssetMinter<AccountId> {
        fn mint_mined(asset_id: AssetId, to: &AccountId, amount: Balance) -> DispatchResult;
    }
    impl<AccountId> MinedAssetMinter<AccountId> for () {
        fn mint_mined(_asset_id: AssetId, _to: &AccountId, _amount: Balance) -> DispatchResult { Ok(()) }
    }

    /// Mint generic β credits as bonus to pool miner.
    pub trait BonusBetaMinter<AccountId> {
        fn mint_bonus_beta(to: &AccountId, amount: Balance) -> DispatchResult;
    }
    impl<AccountId> BonusBetaMinter<AccountId> for () {
        fn mint_bonus_beta(_to: &AccountId, _amount: Balance) -> DispatchResult { Ok(()) }
    }

    #[pallet::storage]
    pub type NextPoolId<T: Config> = StorageValue<_, PoolId, ValueQuery>;

    #[pallet::storage]
    #[pallet::getter(fn pools)]
    pub type Pools<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, PoolInfo<T::AccountId>, OptionQuery>;

    /// Pools registered per asset.
    #[pallet::storage]
    #[pallet::getter(fn pools_of)]
    pub type PoolsOfAsset<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, BoundedVec<PoolId, T::MaxPoolsPerAsset>, ValueQuery>;

    /// Member position in a pool.
    #[pallet::storage]
    pub type PoolPositions<T: Config> = StorageDoubleMap<
        _, Blake2_128Concat, PoolId, Blake2_128Concat, T::AccountId, PoolPosition, OptionQuery
    >;

    /// Snapshot per pool at mining start.
    #[pallet::storage]
    pub type Snapshots<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, DepositSnapshot, OptionQuery>;

    /// Solo position per (account, asset).
    #[pallet::storage]
    pub type SoloPositions<T: Config> = StorageDoubleMap<
        _, Blake2_128Concat, T::AccountId, Blake2_128Concat, AssetId, SoloPosition, OptionQuery
    >;

    /// Total ECC deposited for asset across all pools + solo (for rate d).
    #[pallet::storage]
    pub type TotalDepositedByAsset<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, Balance, ValueQuery>;

    /// Solo participant count per asset.
    #[pallet::storage]
    pub type SoloCountByAsset<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, u32, ValueQuery>;

    /// Last block accrual ran for asset.
    #[pallet::storage]
    pub type LastAccrualBlock<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, u64, ValueQuery>;

    /// Accumulated per-share reward index (scaled) for pool distribution.
    #[pallet::storage]
    pub type PoolRewardPerShare<T: Config> = StorageMap<_, Blake2_128Concat, PoolId, u128, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        PoolCreated { asset_id: AssetId, pool_id: PoolId },
        Deposited { who: T::AccountId, asset_id: AssetId, pool_id: Option<PoolId>, amount: Balance, placed_pool: Option<PoolId> },
        Withdrawn { who: T::AccountId, asset_id: AssetId, pool_id: Option<PoolId>, amount: Balance },
        MiningStarted { asset_id: AssetId, pool_id: PoolId, uniformity_scaled: u128 },
        Accrued { asset_id: AssetId, amount: Balance },
        MiningCompleted { asset_id: AssetId },
        BonusDistributed { pool_id: PoolId, who: T::AccountId, amount: Balance },
        MinedClaimed { who: T::AccountId, asset_id: AssetId, amount: Balance },
    }

    #[pallet::error]
    pub enum Error<T> {
        AssetNotFound,
        NotMineable,
        PoolNotFound,
        PoolFull,
        TooManyPools,
        NoPools,
        ZeroAmount,
        Overflow,
        InsufficientDeposit,
        NothingToClaim,
        MiningNotStarted,
        AlreadyComplete,
        NotPoolMember,
        TransferFailed,
        BonusSourceExhausted,
    }

    #[pallet::hooks]
    impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
        fn on_initialize(n: BlockNumberFor<T>) -> Weight {
            // Best-effort accrual for all known assets with pools/solo deposits.
            let mut weight = Weight::from_parts(10_000, 0);
            if let Ok(block) = TryInto::<u64>::try_into(n) {
                for asset_id in assets_registry::Pallet::<T>::asset_ids().into_inner() {
                    let _ = Self::accrue_asset(asset_id, block);
                    weight = weight.saturating_add(Weight::from_parts(25_000, 0));
                }
            }
            weight
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(50_000, 0))]
        pub fn create_pool(origin: OriginFor<T>, asset_id: AssetId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::ensure_asset_mineable(asset_id)?;
            let pool_id = NextPoolId::<T>::get();
            PoolsOfAsset::<T>::try_mutate(asset_id, |v| v.try_push(pool_id).map_err(|_| Error::<T>::TooManyPools))?;
            Pools::<T>::insert(pool_id, PoolInfo::<T::AccountId> { asset_id, creator: Some(who), ..Default::default() });
            NextPoolId::<T>::put(pool_id.saturating_add(1));
            Self::deposit_event(Event::PoolCreated { asset_id, pool_id });
            Ok(())
        }

        /// Join/deposit into a pool for an asset. Auto-places into eligible pool minimizing |D_pool - S_target_for_pool|.
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(80_000, 0))]
        pub fn join_pool(origin: OriginFor<T>, asset_id: AssetId, preferred_pool: PoolId, amount: Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            Self::ensure_asset_mineable(asset_id)?;
            ensure!(Pools::<T>::contains_key(preferred_pool), Error::<T>::PoolNotFound);
            let placed = Self::choose_pool_for_join(asset_id, preferred_pool, amount)?;
            Self::do_deposit_pool(&who, asset_id, placed, amount)?;
            Self::maybe_start_mining(asset_id, placed)?;
            Self::deposit_event(Event::Deposited { who, asset_id, pool_id: Some(preferred_pool), amount, placed_pool: Some(placed) });
            Ok(())
        }

        /// Solo deposit for an asset (never auto-transferred, no pool bonus).
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(60_000, 0))]
        pub fn deposit_solo(origin: OriginFor<T>, asset_id: AssetId, amount: Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            Self::ensure_asset_mineable(asset_id)?;
            Self::lock_ecc(&who, amount)?;
            let now = Self::current_block();
            SoloPositions::<T>::try_mutate(&who, asset_id, |pos| -> DispatchResult {
                let p = pos.get_or_insert_with(|| { SoloCountByAsset::<T>::mutate(asset_id, |c| *c = c.saturating_add(1)); SoloPosition { last_settle_block: now, ..Default::default() }});
                p.deposit = p.deposit.checked_add(amount).ok_or(Error::<T>::Overflow)?;
                Ok(())
            })?;
            TotalDepositedByAsset::<T>::mutate(asset_id, |d| *d = d.saturating_add(amount));
            Self::deposit_event(Event::Deposited { who, asset_id, pool_id: None, amount, placed_pool: None });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(Weight::from_parts(70_000, 0))]
        pub fn withdraw_pool(origin: OriginFor<T>, pool_id: PoolId, amount: Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            let pool = Pools::<T>::get(pool_id).ok_or(Error::<T>::PoolNotFound)?;
            Self::accrue_asset(pool.asset_id, Self::current_block())?;
            PoolPositions::<T>::try_mutate(pool_id, &who, |pos| -> DispatchResult {
                let p = pos.as_mut().ok_or(Error::<T>::NotPoolMember)?;
                ensure!(p.deposit >= amount, Error::<T>::InsufficientDeposit);
                p.deposit = p.deposit.saturating_sub(amount);
                Ok(())
            })?;
            Pools::<T>::mutate(pool_id, |pi| if let Some(p) = pi { p.total_deposit = p.total_deposit.saturating_sub(amount); if PoolPositions::<T>::get(pool_id, &who).map(|x| x.deposit.is_zero()).unwrap_or(true) { p.participant_count = p.participant_count.saturating_sub(1); PoolPositions::<T>::remove(pool_id, &who);} });
            TotalDepositedByAsset::<T>::mutate(pool.asset_id, |d| *d = d.saturating_sub(amount));
            Self::unlock_ecc(&who)?;
            Self::deposit_event(Event::Withdrawn { who, asset_id: pool.asset_id, pool_id: Some(pool_id), amount });
            Ok(())
        }

        #[pallet::call_index(4)]
        #[pallet::weight(Weight::from_parts(60_000, 0))]
        pub fn withdraw_solo(origin: OriginFor<T>, asset_id: AssetId, amount: Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            Self::accrue_asset(asset_id, Self::current_block())?;
            SoloPositions::<T>::try_mutate(&who, asset_id, |pos| -> DispatchResult {
                let p = pos.as_mut().ok_or(Error::<T>::InsufficientDeposit)?;
                ensure!(p.deposit >= amount, Error::<T>::InsufficientDeposit);
                p.deposit = p.deposit.saturating_sub(amount);
                if p.deposit.is_zero() && p.accrued_mined.is_zero() {
                    *pos = None;
                    SoloCountByAsset::<T>::mutate(asset_id, |c| *c = c.saturating_sub(1));
                }
                Ok(())
            })?;
            TotalDepositedByAsset::<T>::mutate(asset_id, |d| *d = d.saturating_sub(amount));
            Self::unlock_ecc(&who)?;
            Self::deposit_event(Event::Withdrawn { who, asset_id, pool_id: None, amount });
            Ok(())
        }

        /// Claim accrued mined amount for a pool position (mints/credits via hook and records against supply).
        #[pallet::call_index(5)]
        #[pallet::weight(Weight::from_parts(80_000, 0))]
        pub fn claim_pool(origin: OriginFor<T>, pool_id: PoolId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let pool = Pools::<T>::get(pool_id).ok_or(Error::<T>::PoolNotFound)?;
            Self::accrue_asset(pool.asset_id, Self::current_block())?;
            let amount = PoolPositions::<T>::try_mutate(pool_id, &who, |pos| -> Result<Balance, DispatchError> {
                let p = pos.as_mut().ok_or(Error::<T>::NotPoolMember)?;
                let amt = p.accrued_mined;
                ensure!(!amt.is_zero(), Error::<T>::NothingToClaim);
                p.accrued_mined = 0;
                Ok(amt)
            })?;
            // record & mint
            Self::mint_mined_checked(pool.asset_id, &who, amount)?;
            // also distribute any pending bonus beta (tracked separately)
            let bonus = PoolPositions::<T>::mutate(pool_id, &who, |pos| pos.as_mut().map(|p| { let b=p.bonus_beta; p.bonus_beta=0; b }).unwrap_or(0));
            if !bonus.is_zero() { T::BonusBetaMinter::mint_bonus_beta(&who, bonus)?; Self::deposit_event(Event::BonusDistributed { pool_id, who: who.clone(), amount: bonus }); }
            Self::deposit_event(Event::MinedClaimed { who, asset_id: pool.asset_id, amount });
            Ok(())
        }

        /// Claim accrued mined amount for a solo position.
        #[pallet::call_index(6)]
        #[pallet::weight(Weight::from_parts(70_000, 0))]
        pub fn claim_solo(origin: OriginFor<T>, asset_id: AssetId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Self::accrue_asset(asset_id, Self::current_block())?;
            let amount = SoloPositions::<T>::try_mutate(&who, asset_id, |pos| -> Result<Balance, DispatchError> {
                let p = pos.as_mut().ok_or(Error::<T>::NothingToClaim)?;
                let amt = p.accrued_mined;
                ensure!(!amt.is_zero(), Error::<T>::NothingToClaim);
                p.accrued_mined = 0;
                if p.deposit.is_zero() { *pos = None; SoloCountByAsset::<T>::mutate(asset_id, |c| *c = c.saturating_sub(1)); }
                Ok(amt)
            })?;
            Self::mint_mined_checked(asset_id, &who, amount)?;
            Self::deposit_event(Event::MinedClaimed { who, asset_id, amount });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn current_block() -> u64 {
            let n = <frame_system::Pallet<T>>::block_number();
            TryInto::<u64>::try_into(n).ok().unwrap_or(0)
        }

        fn ensure_asset_mineable(asset_id: AssetId) -> DispatchResult {
            let meta = assets_registry::Pallet::<T>::assets(asset_id).ok_or(Error::<T>::AssetNotFound)?;
            ensure!(meta.mineable, Error::<T>::NotMineable);
            Ok(())
        }

        fn total_locked_for(who: &T::AccountId) -> Balance {
            let mut t: Balance = 0;
            for (_pool, acc, pos) in PoolPositions::<T>::iter() {
                if &acc == who { t = t.saturating_add(pos.deposit); }
            }
            for (acc, _asset, pos) in SoloPositions::<T>::iter() {
                if &acc == who { t = t.saturating_add(pos.deposit); }
            }
            t
        }

        fn lock_ecc(who: &T::AccountId, additional: Balance) -> DispatchResult {
            let new_total = Self::total_locked_for(who).saturating_add(additional);
            T::EccCurrency::set_lock(MINING_LOCK_ID, who, new_total, WithdrawReasons::all());
            Ok(())
        }

        fn unlock_ecc(who: &T::AccountId) -> DispatchResult {
            let new_total = Self::total_locked_for(who);
            if new_total.is_zero() {
                T::EccCurrency::remove_lock(MINING_LOCK_ID, who);
            } else {
                T::EccCurrency::set_lock(MINING_LOCK_ID, who, new_total, WithdrawReasons::all());
            }
            Ok(())
        }

        fn choose_pool_for_join(asset_id: AssetId, preferred: PoolId, amount: Balance) -> Result<PoolId, DispatchError> {
            let pools = PoolsOfAsset::<T>::get(asset_id);
            ensure!(!pools.is_empty(), Error::<T>::NoPools);
            // target per pool = S_mineable_remainder_target / N_pools (simple equal split of declared total for balancing proxy)
            let meta = assets_registry::Pallet::<T>::assets(asset_id).ok_or(Error::<T>::AssetNotFound)?;
            let s_target_total = meta.total_supply_declared; // balancing toward S
            let n = pools.len() as u128;
            let s_target_pool = if n == 0 { s_target_total } else { s_target_total / n };
            let mut best = preferred;
            let mut best_dist: u128 = u128::MAX;
            for pid in pools.iter() {
                if let Some(p) = Pools::<T>::get(pid) {
                    if p.asset_id != asset_id || p.complete { continue; }
                    let new_d = p.total_deposit.saturating_add(amount);
                    let dist = new_d.abs_diff(s_target_pool);
                    if dist < best_dist { best_dist = dist; best = *pid; }
                }
            }
            ensure!(Pools::<T>::contains_key(best), Error::<T>::PoolNotFound);
            Ok(best)
        }

        fn do_deposit_pool(who: &T::AccountId, asset_id: AssetId, pool_id: PoolId, amount: Balance) -> DispatchResult {
            Self::lock_ecc(who, amount)?;
            Pools::<T>::try_mutate(pool_id, |p| -> DispatchResult {
                let pool = p.as_mut().ok_or(Error::<T>::PoolNotFound)?;
                ensure!(pool.asset_id == asset_id, Error::<T>::AssetNotFound);
                let is_new = PoolPositions::<T>::get(pool_id, who).is_none();
                PoolPositions::<T>::try_mutate(pool_id, who, |pos| -> DispatchResult {
                    let position = pos.get_or_insert_with(Default::default);
                    position.deposit = position.deposit.checked_add(amount).ok_or(Error::<T>::Overflow)?;
                    Ok(())
                })?;
                pool.total_deposit = pool.total_deposit.checked_add(amount).ok_or(Error::<T>::Overflow)?;
                if is_new { pool.participant_count = pool.participant_count.saturating_add(1); }
                Ok(())
            })?;
            TotalDepositedByAsset::<T>::mutate(asset_id, |d| *d = d.saturating_add(amount));
            Ok(())
        }

        fn maybe_start_mining(asset_id: AssetId, pool_id: PoolId) -> DispatchResult {
            Pools::<T>::try_mutate(pool_id, |p| -> DispatchResult {
                let pool = p.as_mut().ok_or(Error::<T>::PoolNotFound)?;
                if pool.start_block.is_some() { return Ok(()); }
                if pool.total_deposit.is_zero() { return Ok(()); }
                let now = Self::current_block();
                pool.start_block = Some(now);
                // snapshot & uniformity bonus
                let snapshot = Self::snapshot_pool(pool_id);
                let uniformity = Self::compute_uniformity(&snapshot);
                pool.uniformity_scaled = uniformity;
                Snapshots::<T>::insert(pool_id, snapshot);
                // allocate bonus beta credits pro-rata based on uniformity and params
                Self::allocate_bonus(pool_id, asset_id, uniformity)?;
                assets_registry::Pallet::<T>::set_status(asset_id, MiningStatus::Mining)?;
                Self::deposit_event(Event::MiningStarted { asset_id, pool_id, uniformity_scaled: uniformity });
                Ok(())
            })
        }

        fn snapshot_pool(pool_id: PoolId) -> DepositSnapshot {
            let mut buckets = [0u128; 8];
            let mut total: u128 = 0;
            // collect deposits to compute log-ish buckets by relative size
            let mut deposits: Vec<u128> = Vec::new();
            for (_acc, pos) in PoolPositions::<T>::iter_prefix(pool_id) {
                deposits.push(pos.deposit);
                total = total.saturating_add(pos.deposit);
            }
            if total == 0 { return DepositSnapshot { buckets, total }; }
            for d in deposits {
                // bucket by share: 8 bins over [0, total] quantiles by size -> simple log2-ish on ratio
                let ratio = d.saturating_mul(1_000_000) / total; // parts-per-million
                let idx = if ratio == 0 { 0 }
                    else if ratio < 1_000 { 0 }
                    else if ratio < 5_000 { 1 }
                    else if ratio < 20_000 { 2 }
                    else if ratio < 50_000 { 3 }
                    else if ratio < 100_000 { 4 }
                    else if ratio < 200_000 { 5 }
                    else if ratio < 500_000 { 6 }
                    else { 7 };
                buckets[idx] = buckets[idx].saturating_add(d);
            }
            DepositSnapshot { buckets, total }
        }

        /// Uniformity U in [0, RATE_PRECISION] based on 1 - normalized variance of non-zero bucket magnitudes.
        fn compute_uniformity(snap: &DepositSnapshot) -> u128 {
            if snap.total == 0 { return 0; }
            let mut vals: Vec<u128> = Vec::new();
            for b in snap.buckets.iter() { if *b > 0 { vals.push(*b); } }
            if vals.len() <= 1 { return RATE_PRECISION; } // single hump -> perfectly "uniform" by this metric
            let n = vals.len() as u128;
            let mean = snap.total / n;
            if mean == 0 { return 0; }
            // variance of bucket magnitudes
            let mut var_sum: u128 = 0;
            for v in vals {
                let diff = v.abs_diff(mean);
                // (diff^2) scaled down to avoid overflow
                let term = diff.saturating_mul(diff) / mean.max(1);
                var_sum = var_sum.saturating_add(term);
            }
            let variance = var_sum / n;
            // normalized variance in [0, ~mean]; map to U = 1 - min(1, variance/mean)
            let norm = variance.saturating_mul(RATE_PRECISION) / mean.max(1);
            if norm >= RATE_PRECISION { 0 } else { RATE_PRECISION - norm }
        }

        fn allocate_bonus(pool_id: PoolId, asset_id: AssetId, uniformity: u128) -> DispatchResult {
            let pool = Pools::<T>::get(pool_id).ok_or(Error::<T>::PoolNotFound)?;
            let params = assets_registry::Pallet::<T>::mining_params(asset_id).unwrap_or_default();
            if params.bonus_budget.is_zero() || params.b_max_rate == 0 { return Ok(()); }
            // bonus amount B = min(remaining_budget, B_max_rate * pool.total_deposit * U)
            let remaining = params.bonus_budget.saturating_sub(params.bonus_consumed);
            if remaining.is_zero() { return Ok(()); }
            let base = pool.total_deposit
                .saturating_mul(params.b_max_rate) / RATE_PRECISION;
            let bonus = base.saturating_mul(uniformity) / RATE_PRECISION;
            let bonus = bonus.min(remaining);
            if bonus.is_zero() { return Ok(()); }
            // update consumed in registry params
            assets_registry::Pallet::<T>::set_mining_params(
                frame_system::RawOrigin::Root.into(),
                asset_id,
                elysium_primitives::MiningParams { bonus_consumed: params.bonus_consumed.saturating_add(bonus), ..params }
            ).ok(); // best effort; admin origin required normally - so mutate storage directly via helper would be better
            // Since set_mining_params requires AdminOrigin, mutate storage directly:
            assets_registry::Params::<T>::mutate(asset_id, |p| if let Some(mp)=p { mp.bonus_consumed = mp.bonus_consumed.saturating_add(bonus); });
            // distribute pro-rata to pool members by deposit share at snapshot time
            let total = pool.total_deposit.max(1);
            for (acc, mut pos) in PoolPositions::<T>::iter_prefix(pool_id) {
                let share = bonus.saturating_mul(pos.deposit) / total;
                if !share.is_zero() {
                    pos.bonus_beta = pos.bonus_beta.saturating_add(share);
                    PoolPositions::<T>::insert(pool_id, &acc, pos);
                }
            }
            Ok(())
        }

        fn mint_mined_checked(asset_id: AssetId, who: &T::AccountId, amount: Balance) -> DispatchResult {
            if amount.is_zero() { return Ok(()); }
            // ensure not exceeding cap
            let rem = assets_registry::Pallet::<T>::mineable_remainder(asset_id).ok_or(Error::<T>::AssetNotFound)?;
            let amt = amount.min(rem);
            if amt.is_zero() {
                assets_registry::Pallet::<T>::set_status(asset_id, MiningStatus::Complete)?;
                Self::deposit_event(Event::MiningCompleted { asset_id });
                return Ok(());
            }
            assets_registry::Pallet::<T>::record_mint(asset_id, amt)?;
            T::MinedAssetMinter::mint_mined(asset_id, who, amt)?;
            // mark complete if exhausted
            if assets_registry::Pallet::<T>::mineable_remainder(asset_id).unwrap_or(0).is_zero() {
                assets_registry::Pallet::<T>::set_status(asset_id, MiningStatus::Complete)?;
                Self::deposit_event(Event::MiningCompleted { asset_id });
            }
            Ok(())
        }

        /// Accrue mined rewards for an asset for all pools and solo positions since last accrual.
        pub fn accrue_asset(asset_id: AssetId, now: u64) -> DispatchResult {
            let status = assets_registry::Pallet::<T>::mining_status(asset_id);
            if matches!(status, MiningStatus::Complete) { return Ok(()); }
            let meta = match assets_registry::Pallet::<T>::assets(asset_id) { Some(m) => m, None => return Ok(()) };
            if !meta.mineable { return Ok(()); }
            let params = assets_registry::Pallet::<T>::mining_params(asset_id).unwrap_or_default();
            if params.period_blocks == 0 { return Ok(()); }
            let last = LastAccrualBlock::<T>::get(asset_id);
            if now <= last { return Ok(()); }
            let elapsed = now.saturating_sub(if last == 0 { now.saturating_sub(params.period_blocks as u64) } else { last });
            let periods = elapsed / (params.period_blocks as u64);
            if periods == 0 { return Ok(()); }
            LastAccrualBlock::<T>::insert(asset_id, last.saturating_add(periods * params.period_blocks as u64));

            let d_total = TotalDepositedByAsset::<T>::get(asset_id);
            let s_target = meta.total_supply_declared.max(1);
            let d_scaled = normalized_distance(d_total, s_target);
            let mult = rate_multiplier(d_scaled, params.k_curve, params.min_multiplier);
            let rate_per_period = params.r_base_per_period.saturating_mul(mult) / RATE_PRECISION;
            let total_emission = rate_per_period.saturating_mul(periods as u128);
            let remainder = assets_registry::Pallet::<T>::mineable_remainder(asset_id).unwrap_or(0);
            let total_emission = total_emission.min(remainder);
            if total_emission.is_zero() { return Ok(()); }

            // Split emission between pools (aggregate) and solo aggregate by their deposit weights, then apply solo 1% rule.
            let mut pool_deposit_sum: u128 = 0;
            for pid in PoolsOfAsset::<T>::get(asset_id).iter() {
                if let Some(p) = Pools::<T>::get(*pid) { pool_deposit_sum = pool_deposit_sum.saturating_add(p.total_deposit); }
            }
            let solo_deposit_sum = d_total.saturating_sub(pool_deposit_sum);
            let total_dep = d_total.max(1);

            // baseline proportional shares
            let pool_share_base = total_emission.saturating_mul(pool_deposit_sum) / total_dep;
            let solo_share_base = total_emission.saturating_sub(pool_share_base);

            // apply solo factor: solo gets solo_share_base * factor, reduction stays unminted (conservative for supply) or could go to pools; keep unminted/simple.
            let solo_count = SoloCountByAsset::<T>::get(asset_id);
            // comparable pool participant count: sum over pools
            let mut pool_participants: u32 = 0;
            for pid in PoolsOfAsset::<T>::get(asset_id).iter() {
                if let Some(p) = Pools::<T>::get(*pid) { pool_participants = pool_participants.saturating_add(p.participant_count); }
            }
            let solo_factor = solo_rate_factor_scaled(solo_count, pool_participants);
            let solo_emission = solo_share_base.saturating_mul(solo_factor) / RATE_PRECISION;
            let pool_emission = pool_share_base; // pools keep their base share

            // distribute to pools pro-rata by pool deposit
            if pool_deposit_sum > 0 && !pool_emission.is_zero() {
                for pid in PoolsOfAsset::<T>::get(asset_id).iter() {
                    if let Some(pool) = Pools::<T>::get(*pid) {
                        if pool.total_deposit.is_zero() { continue; }
                        let pool_amt = pool_emission.saturating_mul(pool.total_deposit) / pool_deposit_sum;
                        Self::distribute_to_pool_members(*pid, pool_amt);
                    }
                }
            }
            // distribute to solo miners pro-rata by deposit
            if solo_deposit_sum > 0 && !solo_emission.is_zero() {
                // iterate solo positions for asset
                for (acc, _asset, mut pos) in SoloPositions::<T>::iter() {
                    if _asset != asset_id || pos.deposit.is_zero() { continue; }
                    let amt = solo_emission.saturating_mul(pos.deposit) / solo_deposit_sum;
                    if !amt.is_zero() {
                        pos.accrued_mined = pos.accrued_mined.saturating_add(amt);
                        pos.last_settle_block = now;
                        SoloPositions::<T>::insert(acc, asset_id, pos);
                    }
                }
            }
            Self::deposit_event(Event::Accrued { asset_id, amount: pool_emission.saturating_add(solo_emission) });
            Ok(())
        }

        fn distribute_to_pool_members(pool_id: PoolId, amount: Balance) {
            let pool = match Pools::<T>::get(pool_id) { Some(p) => p, None => return };
            if pool.total_deposit.is_zero() || amount.is_zero() { return; }
            for (acc, mut pos) in PoolPositions::<T>::iter_prefix(pool_id) {
                if pos.deposit.is_zero() { continue; }
                let share = amount.saturating_mul(pos.deposit) / pool.total_deposit;
                if !share.is_zero() {
                    pos.accrued_mined = pos.accrued_mined.saturating_add(share);
                    PoolPositions::<T>::insert(pool_id, &acc, pos);
                }
            }
        }

        /// Helpers for other pallets/beta integration.
        pub fn pool_asset(pool_id: PoolId) -> Option<AssetId> { Pools::<T>::get(pool_id).map(|p| p.asset_id) }
        pub fn is_pool_complete_for_asset(asset_id: AssetId) -> bool {
            matches!(assets_registry::Pallet::<T>::mining_status(asset_id), MiningStatus::Complete)
        }
        pub fn take_accrued_for_bundle(who: &T::AccountId, pool_id: PoolId) -> Result<(AssetId, Balance), DispatchError> {
            let pool = Pools::<T>::get(pool_id).ok_or(Error::<T>::PoolNotFound)?;
            Self::accrue_asset(pool.asset_id, Self::current_block())?;
            let amt = PoolPositions::<T>::try_mutate(pool_id, who, |pos| -> Result<Balance, DispatchError> {
                let p = pos.as_mut().ok_or(Error::<T>::NotPoolMember)?;
                let a = p.accrued_mined;
                ensure!(!a.is_zero(), Error::<T>::NothingToClaim);
                p.accrued_mined = 0;
                Ok(a)
            })?;
            Ok((pool.asset_id, amt))
        }
        pub fn take_accrued_solo_for_bundle(who: &T::AccountId, asset_id: AssetId) -> Result<Balance, DispatchError> {
            Self::accrue_asset(asset_id, Self::current_block())?;
            let amt = SoloPositions::<T>::try_mutate(who, asset_id, |pos| -> Result<Balance, DispatchError> {
                let p = pos.as_mut().ok_or(Error::<T>::NothingToClaim)?;
                let a = p.accrued_mined;
                ensure!(!a.is_zero(), Error::<T>::NothingToClaim);
                p.accrued_mined = 0;
                Ok(a)
            })?;
            Ok(amt)
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
