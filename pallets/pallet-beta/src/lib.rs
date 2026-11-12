#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use elysium_primitives::{split_sale_x, treasury_hnn_fee, AssetId, Balance};
    use frame_support::{
        pallet_prelude::*,
        traits::{Currency, ExistenceRequirement, ReservableCurrency},
        PalletId,
    };
    use frame_system::pallet_prelude::*;
    use pallet_elysium_assets_registry as assets_registry;
    use pallet_liquidity_mining as mining;
    use sp_runtime::traits::{Saturating, Zero};


    #[derive(Clone, Encode, Decode, Eq, PartialEq, core::fmt::Debug, TypeInfo, MaxEncodedLen, Default)]
    #[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
    pub struct BundleListing<AccountId> {
        pub seller: AccountId,
        pub asset_id: AssetId,
        /// Amount of mined asset bundled (and β credits to be minted on sale).
        pub amount: Balance,
        /// Price in ECC.
        pub price_ecc: Balance,
        pub active: bool,
    }

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config + assets_registry::Config + mining::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        /// ECC currency (bundle sale payment, reserve, seller proceeds).
        type EccCurrency: Currency<Self::AccountId, Balance = Balance> + ReservableCurrency<Self::AccountId>;
        /// HNN currency (treasury fee).
        type HnnCurrency: Currency<Self::AccountId, Balance = Balance> + ReservableCurrency<Self::AccountId>;
        /// Treasury account to receive 0.001% HNN fee.
        type TreasuryAccount: Get<Self::AccountId>;
        /// Protocol reserve account to receive 50% ECC.
        type ReserveAccount: Get<Self::AccountId>;
        /// Pallet id for derived accounts if needed.
        type PalletId: Get<PalletId>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        /// Fixed conversion: HNN per 1 ECC scaled by RATE_PRECISION (e.g. 1:1 => RATE_PRECISION).
        /// Used to compute 0.001% of X(ECC) denominated in HNN; buyer must provide/supply this fee.
        #[pallet::constant]
        type HnnPerEccScaled: Get<u128>;
    }

    /// Generic β credit balance per account (fungible protocol credit).
    #[pallet::storage]
    #[pallet::getter(fn beta_credits)]
    pub type BetaCredits<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Balance, ValueQuery>;

    /// Total outstanding β credits (liability) for invariant tracking.
    #[pallet::storage]
    pub type TotalBetaOutstanding<T: Config> = StorageValue<_, Balance, ValueQuery>;

    /// Bundle listings by id.
    #[pallet::storage]
    pub type Listings<T: Config> = StorageMap<_, Blake2_128Concat, u64, BundleListing<T::AccountId>, OptionQuery>;

    #[pallet::storage]
    pub type NextListingId<T: Config> = StorageValue<_, u64, ValueQuery>;

    /// Bundled (escrowed) mined amounts ready for sale: (seller, asset_id) -> amount.
    #[pallet::storage]
    pub type BundledEscrow<T: Config> = StorageDoubleMap<
        _, Blake2_128Concat, T::AccountId, Blake2_128Concat, AssetId, Balance, ValueQuery
    >;

    /// Backing available per asset for β redemption (protocol reserve/backing pool accounting).
    #[pallet::storage]
    pub type RedemptionBacking<T: Config> = StorageMap<_, Blake2_128Concat, AssetId, Balance, ValueQuery>;

    /// MVP-ASSUMED (Section 5.2): governed/configured conversion rate (HNN per 1 ECC, scaled by RATE_PRECISION).
    /// Used to compute 0.001% of X(ECC) denominated in HNN.
    #[pallet::storage]
    #[pallet::getter(fn hnn_per_ecc_scaled)]
    pub type HnnPerEccRate<T: Config> = StorageValue<_, u128, ValueQuery>;

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub hnn_per_ecc_scaled: u128,
        #[serde(skip)]
        pub _phantom: core::marker::PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            let rate = if self.hnn_per_ecc_scaled == 0 { T::HnnPerEccScaled::get() } else { self.hnn_per_ecc_scaled };
            HnnPerEccRate::<T>::put(rate);
        }
    }

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        Bundled { who: T::AccountId, asset_id: AssetId, amount: Balance },
        ListingCreated { listing_id: u64, seller: T::AccountId, asset_id: AssetId, amount: Balance, price_ecc: Balance },
        ListingCancelled { listing_id: u64 },
        BundleSold { listing_id: u64, buyer: T::AccountId, seller: T::AccountId, asset_id: AssetId, amount: Balance, price_ecc: Balance, seller_ecc: Balance, treasury_hnn: Balance, reserve_ecc: Balance },
        BetaMinted { to: T::AccountId, amount: Balance },
        BetaBurned { from: T::AccountId, amount: Balance },
        Redeemed { who: T::AccountId, target_asset: AssetId, amount: Balance },
        BackingDeposited { asset_id: AssetId, amount: Balance },
        HnnPerEccRateUpdated { rate_scaled: u128 },
    }

    #[pallet::error]
    pub enum Error<T> {
        ZeroAmount,
        Overflow,
        Underflow,
        NotPoolComplete,
        NothingBundled,
        InsufficientBundled,
        ListingNotFound,
        NotSeller,
        ListingInactive,
        InsufficientBalance,
        InsufficientHnnForFee,
        InsufficientBeta,
        AssetNotEligible,
        InsufficientBacking,
        SelfPurchase,
        TransferFailed,
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Bundle accrued mined share from a pool position into escrow (requires asset mining complete for that pool's asset).
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(80_000, 0))]
        pub fn bundle_from_pool(origin: OriginFor<T>, pool_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let (asset_id, amount) = mining::Pallet::<T>::take_accrued_for_bundle(&who, pool_id)?;
            Self::do_bundle(&who, asset_id, amount)
        }

        /// Bundle accrued mined share from solo position (requires mining complete).
        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(70_000, 0))]
        pub fn bundle_from_solo(origin: OriginFor<T>, asset_id: AssetId) -> DispatchResult {
            let who = ensure_signed(origin)?;
            let status = assets_registry::Pallet::<T>::mining_status(asset_id);
            ensure!(matches!(status, elysium_primitives::MiningStatus::Complete), Error::<T>::NotPoolComplete);
            let amount = mining::Pallet::<T>::take_accrued_solo_for_bundle(&who, asset_id)?;
            Self::do_bundle(&who, asset_id, amount)
        }

        /// Create an on-chain listing to sell a bundled amount for X ECC.
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(50_000, 0))]
        pub fn create_listing(origin: OriginFor<T>, asset_id: AssetId, amount: Balance, price_ecc: Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero() && !price_ecc.is_zero(), Error::<T>::ZeroAmount);
            BundledEscrow::<T>::try_mutate(&who, asset_id, |b| -> DispatchResult {
                ensure!(*b >= amount, Error::<T>::InsufficientBundled);
                *b = b.saturating_sub(amount);
                Ok(())
            })?;
            let id = NextListingId::<T>::get();
            Listings::<T>::insert(id, BundleListing { seller: who.clone(), asset_id, amount, price_ecc, active: true });
            NextListingId::<T>::put(id.saturating_add(1));
            Self::deposit_event(Event::ListingCreated { listing_id: id, seller: who, asset_id, amount, price_ecc });
            Ok(())
        }

        #[pallet::call_index(3)]
        #[pallet::weight(Weight::from_parts(40_000, 0))]
        pub fn cancel_listing(origin: OriginFor<T>, listing_id: u64) -> DispatchResult {
            let who = ensure_signed(origin)?;
            Listings::<T>::try_mutate(listing_id, |l| -> DispatchResult {
                let listing = l.as_mut().ok_or(Error::<T>::ListingNotFound)?;
                ensure!(listing.seller == who, Error::<T>::NotSeller);
                ensure!(listing.active, Error::<T>::ListingInactive);
                listing.active = false;
                BundledEscrow::<T>::mutate(&who, listing.asset_id, |b| *b = b.saturating_add(listing.amount));
                Ok(())
            })?;
            Self::deposit_event(Event::ListingCancelled { listing_id });
            Ok(())
        }

        /// Buy a listing: buyer pays X ECC + 0.001% HNN fee. Splits applied; seller receives generic β credits = amount.
        #[pallet::call_index(4)]
        #[pallet::weight(Weight::from_parts(120_000, 0))]
        pub fn buy_listing(origin: OriginFor<T>, listing_id: u64) -> DispatchResult {
            let buyer = ensure_signed(origin)?;
            let listing = Listings::<T>::get(listing_id).ok_or(Error::<T>::ListingNotFound)?;
            ensure!(listing.active, Error::<T>::ListingInactive);
            ensure!(listing.seller != buyer, Error::<T>::SelfPurchase);

            let x = listing.price_ecc;
            let (seller_ecc, reserve_ecc) = split_sale_x(x);
            let rate = {
                let r = HnnPerEccRate::<T>::get();
                if r == 0 { T::HnnPerEccScaled::get() } else { r }
            };
            let hnn_fee = treasury_hnn_fee(x, rate);

            // collect ECC from buyer
            <T as Config>::EccCurrency::transfer(&buyer, &listing.seller, seller_ecc, ExistenceRequirement::KeepAlive)
                .map_err(|_| Error::<T>::TransferFailed)?;
            <T as Config>::EccCurrency::transfer(&buyer, &<T as Config>::ReserveAccount::get(), reserve_ecc, ExistenceRequirement::KeepAlive)
                .map_err(|_| Error::<T>::TransferFailed)?;
            // collect HNN fee from buyer (0.001% of X, sourced from buyer as separate HNN payment)
            if !hnn_fee.is_zero() {
                <T as Config>::HnnCurrency::transfer(&buyer, &<T as Config>::TreasuryAccount::get(), hnn_fee, ExistenceRequirement::KeepAlive)
                    .map_err(|_| Error::<T>::InsufficientHnnForFee)?;
            }

            // mark inactive
            Listings::<T>::mutate(listing_id, |l| if let Some(x)=l { x.active = false; });

            // mint generic β credits to seller equal to sold mined amount (1:1 units)
            Self::mint_beta(&listing.seller, listing.amount)?;

            // record backing contribution: reserve ECC received counts as backing for ECC (and protocol policy may allocate);
            // credit backing for the sold asset's redemption pool notionally via reserve — for MVP credit reserve_ecc as ECC backing.
            RedemptionBacking::<T>::mutate(elysium_primitives::ECC_ASSET_ID, |b| *b = b.saturating_add(reserve_ecc));

            Self::deposit_event(Event::BundleSold {
                listing_id, buyer, seller: listing.seller, asset_id: listing.asset_id, amount: listing.amount,
                price_ecc: x, seller_ecc, treasury_hnn: hnn_fee, reserve_ecc,
            });
            Ok(())
        }

        /// Redeem generic β credits 1:1 into an eligible, sufficiently reserve-backed asset.
        #[pallet::call_index(5)]
        #[pallet::weight(Weight::from_parts(90_000, 0))]
        pub fn redeem(origin: OriginFor<T>, target_asset: AssetId, amount: Balance) -> DispatchResult {
            let who = ensure_signed(origin)?;
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            ensure!(assets_registry::Pallet::<T>::is_redeemable_eligible(target_asset), Error::<T>::AssetNotEligible);
            let backing = RedemptionBacking::<T>::get(target_asset);
            ensure!(backing >= amount, Error::<T>::InsufficientBacking);
            // burn β
            Self::burn_beta(&who, amount)?;
            // reduce backing and transfer/mint target asset from backing source
            RedemptionBacking::<T>::mutate(target_asset, |b| *b = b.saturating_sub(amount));
            Self::payout_redeemed_asset(&who, target_asset, amount)?;
            Self::deposit_event(Event::Redeemed { who, target_asset, amount });
            Ok(())
        }

        /// Admin: deposit/declare backing for an asset (e.g. reserve funds, unminted supply allocated for redemption).
        #[pallet::call_index(6)]
        #[pallet::weight(Weight::from_parts(30_000, 0))]
        pub fn deposit_backing(origin: OriginFor<T>, asset_id: AssetId, amount: Balance) -> DispatchResult {
            <T as Config>::AdminOrigin::ensure_origin(origin)?;
            RedemptionBacking::<T>::mutate(asset_id, |b| *b = b.saturating_add(amount));
            Self::deposit_event(Event::BackingDeposited { asset_id, amount });
            Ok(())
        }

        /// MVP-ASSUMED (Section 5.2): set governed HNN/ECC conversion rate for 0.001% fee computation.
        #[pallet::call_index(7)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn set_hnn_per_ecc_rate(origin: OriginFor<T>, rate_scaled: u128) -> DispatchResult {
            <T as Config>::AdminOrigin::ensure_origin(origin)?;
            HnnPerEccRate::<T>::put(rate_scaled);
            Self::deposit_event(Event::HnnPerEccRateUpdated { rate_scaled });
            Ok(())
        }
    }

    impl<T: Config> Pallet<T> {
        fn do_bundle(who: &T::AccountId, asset_id: AssetId, amount: Balance) -> DispatchResult {
            ensure!(!amount.is_zero(), Error::<T>::ZeroAmount);
            let status = assets_registry::Pallet::<T>::mining_status(asset_id);
            ensure!(matches!(status, elysium_primitives::MiningStatus::Complete), Error::<T>::NotPoolComplete);
            BundledEscrow::<T>::mutate(who, asset_id, |b| *b = b.saturating_add(amount));
            Self::deposit_event(Event::Bundled { who: who.clone(), asset_id, amount });
            Ok(())
        }

        pub fn mint_beta(to: &T::AccountId, amount: Balance) -> DispatchResult {
            if amount.is_zero() { return Ok(()); }
            BetaCredits::<T>::mutate(to, |b| *b = b.saturating_add(amount));
            TotalBetaOutstanding::<T>::mutate(|t| *t = t.saturating_add(amount));
            Self::deposit_event(Event::BetaMinted { to: to.clone(), amount });
            Ok(())
        }

        pub fn burn_beta(from: &T::AccountId, amount: Balance) -> DispatchResult {
            if amount.is_zero() { return Ok(()); }
            BetaCredits::<T>::try_mutate(from, |b| -> DispatchResult {
                ensure!(*b >= amount, Error::<T>::InsufficientBeta);
                *b = b.saturating_sub(amount);
                Ok(())
            })?;
            TotalBetaOutstanding::<T>::mutate(|t| *t = t.saturating_sub(amount));
            Self::deposit_event(Event::BetaBurned { from: from.clone(), amount });
            Ok(())
        }

        fn payout_redeemed_asset(to: &T::AccountId, asset_id: AssetId, amount: Balance) -> DispatchResult {
            // For native ECC/HNN redemption, transfer from reserve/treasury holding if possible; else for mineable assets,
            // mint via assets-registry supply accounting if under cap (redemption backed by allocated unminted/reserve).
            if asset_id == elysium_primitives::ECC_ASSET_ID {
                // pay from protocol reserve ECC account
                <T as Config>::EccCurrency::transfer(&<T as Config>::ReserveAccount::get(), to, amount, ExistenceRequirement::KeepAlive)
                    .map_err(|_| Error::<T>::TransferFailed)?;
            } else if asset_id == elysium_primitives::HNN_ASSET_ID {
                <T as Config>::HnnCurrency::transfer(&<T as Config>::TreasuryAccount::get(), to, amount, ExistenceRequirement::AllowDeath)
                    .map_err(|_| Error::<T>::TransferFailed)?;
            } else {
                // For other mineable assets: if mintable within remaining cap treat backing as allocated supply and mint-record.
                if assets_registry::Pallet::<T>::can_mint(asset_id, amount) {
                    assets_registry::Pallet::<T>::record_mint(asset_id, amount)?;
                    // actual token crediting for generic assets would go through pallet-assets in full impl;
                    // MVP: record only (event evidences entitlement) — runtime should wire a real minter for production.
                }
                // if not mintable, backing must be actual reserve balance held externally; MVP assumes backing accounting sufficient.
            }
            Ok(())
        }
    }

    /// Hook impl to mint bonus β credits (called by liquidity mining).
    impl<T: Config> mining::BonusBetaMinter<T::AccountId> for Pallet<T> {
        fn mint_bonus_beta(to: &T::AccountId, amount: Balance) -> DispatchResult {
            Self::mint_beta(to, amount)
        }
    }
}
#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
