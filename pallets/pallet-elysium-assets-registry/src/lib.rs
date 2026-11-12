#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use elysium_primitives::{AssetId, AssetMetadata, Balance, MiningParams, MiningStatus};
    use frame_support::pallet_prelude::*;
    use frame_system::pallet_prelude::*;
    use sp_runtime::traits::{CheckedAdd, CheckedSub, Saturating, Zero};
    use sp_std::vec::Vec;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// Origin allowed to register/update assets and governance-controlled flags.
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        /// Maximum length helpers are bounded in primitives; extra bounds if needed.
        #[pallet::constant]
        type MaxAssets: Get<u32>;
    }

    /// Registered assets and their metadata/accounting.
    #[pallet::storage]
    #[pallet::getter(fn assets)]
    pub type Assets<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, AssetMetadata<Balance, T::AccountId>, OptionQuery>;

    /// Mining status per mineable asset.
    #[pallet::storage]
    #[pallet::getter(fn mining_status)]
    pub type Status<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, MiningStatus, ValueQuery>;

    /// Mining/emission parameters per asset.
    #[pallet::storage]
    #[pallet::getter(fn mining_params)]
    pub type Params<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, MiningParams<Balance>, OptionQuery>;

    /// Known/registered asset ids.
    #[pallet::storage]
    #[pallet::getter(fn asset_ids)]
    pub type AssetIds<T: Config> = StorageValue<_, BoundedVec<AssetId, T::MaxAssets>, ValueQuery>;

    /// Next asset id helper for non-native registrations.
    #[pallet::storage]
    pub type NextAssetId<T: Config> = StorageValue<_, AssetId, ValueQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        AssetRegistered { asset_id: AssetId },
        AssetUpdated { asset_id: AssetId },
        MiningStatusChanged { asset_id: AssetId, status: MiningStatus },
        MiningParamsSet { asset_id: AssetId },
        MintRecorded { asset_id: AssetId, amount: Balance, mined_to_date: Balance },
    }

    #[pallet::error]
    pub enum Error<T> {
        AssetExists,
        AssetNotFound,
        TooManyAssets,
        InvalidSupply,
        Overflow,
        Underflow,
        SupplyCapExceeded,
        NotMineable,
        BadName,
        BadSymbol,
    }

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub assets: Vec<(AssetId, AssetMetadata<Balance, T::AccountId>, MiningParams<Balance>, MiningStatus)>,
        pub next_asset_id: AssetId,
        #[serde(skip)]
        pub _phantom: PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let mut ids: BoundedVec<AssetId, T::MaxAssets> = BoundedVec::default();
            for (id, meta, params, status) in &self.assets {
                assert!(meta.total_supply_declared >= meta.genesis_issued, "invalid genesis supply");
                Assets::<T>::insert(id, meta.clone());
                Params::<T>::insert(id, params.clone());
                Status::<T>::insert(id, *status);
                ids.try_push(*id).expect("too many genesis assets");
            }
            AssetIds::<T>::put(ids);
            NextAssetId::<T>::put(self.next_asset_id);
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Register a new asset. `asset_id` must be unused.
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn register_asset(
            origin: OriginFor<T>,
            asset_id: AssetId,
            metadata: AssetMetadata<Balance, T::AccountId>,
            mining_params: Option<MiningParams<Balance>>,
            status: MiningStatus,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(!Assets::<T>::contains_key(asset_id), Error::<T>::AssetExists);
            Self::validate_metadata(&metadata)?;
            AssetIds::<T>::try_mutate(|ids| ids.try_push(asset_id).map_err(|_| Error::<T>::TooManyAssets))?;
            Assets::<T>::insert(asset_id, metadata);
            Status::<T>::insert(asset_id, status);
            if let Some(p) = mining_params {
                Params::<T>::insert(asset_id, p);
                Self::deposit_event(Event::MiningParamsSet { asset_id });
            }
            Self::deposit_event(Event::AssetRegistered { asset_id });
            Ok(())
        }

        /// Update mutable metadata fields and flags (admin/governance).
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn update_asset(
            origin: OriginFor<T>,
            asset_id: AssetId,
            mineable: Option<bool>,
            redeemable_eligible: Option<bool>,
            reserve_account: Option<Option<T::AccountId>>,
            status: Option<MiningStatus>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            Assets::<T>::try_mutate(asset_id, |m| -> DispatchResult {
                let meta = m.as_mut().ok_or(Error::<T>::AssetNotFound)?;
                if let Some(v) = mineable { meta.mineable = v; }
                if let Some(v) = redeemable_eligible { meta.redeemable_eligible = v; }
                if let Some(v) = reserve_account { meta.reserve_account = v; }
                Ok(())
            })?;
            if let Some(s) = status {
                Status::<T>::insert(asset_id, s);
                Self::deposit_event(Event::MiningStatusChanged { asset_id, status: s });
            }
            Self::deposit_event(Event::AssetUpdated { asset_id });
            Ok(())
        }

        /// Set/replace mining parameters for an asset.
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn set_mining_params(
            origin: OriginFor<T>,
            asset_id: AssetId,
            params: MiningParams<Balance>,
        ) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            ensure!(Assets::<T>::contains_key(asset_id), Error::<T>::AssetNotFound);
            Params::<T>::insert(asset_id, params);
            Self::deposit_event(Event::MiningParamsSet { asset_id });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        pub fn validate_metadata(meta: &AssetMetadata<Balance, T::AccountId>) -> DispatchResult {
            ensure!(meta.total_supply_declared >= meta.genesis_issued, Error::<T>::InvalidSupply);
            ensure!(!meta.name.is_empty(), Error::<T>::BadName);
            ensure!(!meta.symbol.is_empty(), Error::<T>::BadSymbol);
            Ok(())
        }

        /// Remaining mineable amount = declared_total - genesis_issued - mined_to_date.
        pub fn mineable_remainder(asset_id: AssetId) -> Option<Balance> {
            let meta = Assets::<T>::get(asset_id)?;
            let issued = meta.genesis_issued.checked_add(meta.mined_to_date)?;
            meta.total_supply_declared.checked_sub(issued)
        }

        pub fn can_mint(asset_id: AssetId, amount: Balance) -> bool {
            if amount.is_zero() { return true; }
            Self::mineable_remainder(asset_id).map(|r| r >= amount).unwrap_or(false)
        }

        /// Record minted/mined amount against supply cap. Caller is responsible for actual token issuance.
        pub fn record_mint(asset_id: AssetId, amount: Balance) -> DispatchResult {
            if amount.is_zero() { return Ok(()); }
            Assets::<T>::try_mutate(asset_id, |m| -> DispatchResult {
                let meta = m.as_mut().ok_or(Error::<T>::AssetNotFound)?;
                ensure!(meta.mineable, Error::<T>::NotMineable);
                let new_mined = meta.mined_to_date.checked_add(amount).ok_or(Error::<T>::Overflow)?;
                let issued = meta.genesis_issued.checked_add(new_mined).ok_or(Error::<T>::Overflow)?;
                ensure!(issued <= meta.total_supply_declared, Error::<T>::SupplyCapExceeded);
                meta.mined_to_date = new_mined;
                Self::deposit_event(Event::MintRecorded { asset_id, amount, mined_to_date: new_mined });
                Ok(())
            })
        }

        pub fn set_status(asset_id: AssetId, status: MiningStatus) -> DispatchResult {
            ensure!(Assets::<T>::contains_key(asset_id), Error::<T>::AssetNotFound);
            Status::<T>::insert(asset_id, status);
            Self::deposit_event(Event::MiningStatusChanged { asset_id, status });
            Ok(())
        }

        pub fn is_redeemable_eligible(asset_id: AssetId) -> bool {
            Assets::<T>::get(asset_id).map(|m| m.redeemable_eligible).unwrap_or(false)
        }

        pub fn asset_account(asset_id: AssetId) -> Option<T::AccountId> {
            Assets::<T>::get(asset_id).and_then(|m| m.reserve_account)
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
