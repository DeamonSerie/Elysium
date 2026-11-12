#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
    use elysium_primitives::{BlockProof, ProofPublicInputs};
use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;
use sp_runtime::traits::Hash;
use sp_runtime::transaction_validity::{
    InvalidTransaction, TransactionPriority, TransactionSource, TransactionValidity, ValidTransaction,
};
use sp_std::vec::Vec;

    #[pallet::pallet]
    pub struct Pallet<T>(_);

    #[pallet::config]
    pub trait Config: frame_system::Config {
        type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;
        type AdminOrigin: EnsureOrigin<Self::RuntimeOrigin>;
        /// If true, accept proofs in mock/devnet mode when verification key is unset or mock flag enabled.
        #[pallet::constant]
        type DevnetMockVerification: Get<bool>;
        /// Maximum proof size in bytes accepted.
        #[pallet::constant]
        type MaxProofSize: Get<u32>;
        /// Maximum verifying key size in bytes.
        #[pallet::constant]
        type MaxVkSize: Get<u32>;
    }

    /// Active verifying key bytes for current proof system/version.
    #[pallet::storage]
    #[pallet::getter(fn verifying_key)]
    pub type VerifyingKey<T: Config> = StorageValue<_, BoundedVec<u8, T::MaxVkSize>, OptionQuery>;

    /// Current VK/proof system version id.
    #[pallet::storage]
    pub type VkVersion<T: Config> = StorageValue<_, u32, ValueQuery>;

    /// Whether strict (production) verification is enforced.
    #[pallet::storage]
    #[pallet::getter(fn strict_mode)]
    pub type StrictMode<T: Config> = StorageValue<_, bool, ValueQuery>;

    /// Last verified block number and proof commitment hash (for observability).
    #[pallet::storage]
    pub type LastVerified<T: Config> = StorageValue<_, (u64, T::Hash), OptionQuery>;

    /// Optional per-block submitted proof commitment (can be set during block authoring/import pipeline).
    #[pallet::storage]
    pub type PendingBlockProof<T: Config> = StorageValue<_, (ProofPublicInputs<T::Hash>, BoundedVec<u8, T::MaxProofSize>, u32), OptionQuery>;

    #[pallet::event]
    #[pallet::generate_deposit(pub(super) fn deposit_event)]
    pub enum Event<T: Config> {
        VerifyingKeyUpdated { version: u32 },
        StrictModeSet { strict: bool },
        ProofVerified { block_number: u64 },
        ProofRejected { block_number: u64 },
        MockVerificationAccepted { block_number: u64 },
    }

    #[pallet::error]
    pub enum Error<T> {
        ProofTooLarge,
        VkTooLarge,
        MissingProof,
        MissingVerifyingKey,
        VerificationFailed,
        PublicInputMismatch,
        VkVersionMismatch,
    }

    #[pallet::genesis_config]
    #[derive(frame_support::DefaultNoBound)]
    pub struct GenesisConfig<T: Config> {
        pub strict_mode: bool,
        #[serde(skip)]
        pub _phantom: PhantomData<T>,
    }

    #[pallet::genesis_build]
    impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
        fn build(&self) {
            StrictMode::<T>::put(self.strict_mode);
        }
    }

    #[pallet::call]
    impl<T: Config> Pallet<T> {
        /// Set/upgrade verifying key (governed).
        #[pallet::call_index(0)]
        #[pallet::weight(Weight::from_parts(20_000, 0))]
        pub fn set_verifying_key(origin: OriginFor<T>, vk: Vec<u8>, version: u32) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            let bounded: BoundedVec<u8, T::MaxVkSize> =
                vk.try_into().map_err(|_| Error::<T>::VkTooLarge)?;
            VerifyingKey::<T>::put(bounded);
            VkVersion::<T>::put(version);
            Self::deposit_event(Event::VerifyingKeyUpdated { version });
            Ok(())
        }

        #[pallet::call_index(1)]
        #[pallet::weight(Weight::from_parts(10_000, 0))]
        pub fn set_strict_mode(origin: OriginFor<T>, strict: bool) -> DispatchResult {
            T::AdminOrigin::ensure_origin(origin)?;
            StrictMode::<T>::put(strict);
            Self::deposit_event(Event::StrictModeSet { strict });
            Ok(())
        }

        /// Submit proof for the current block context.
        /// MVP-ASSUMED: unsigned submission allowed for devnet/block-author injection; production
        /// should gate via digest/inherent/proof DA and stronger validation.
        #[pallet::call_index(2)]
        #[pallet::weight(Weight::from_parts(50_000, 0))]
        pub fn submit_block_proof(
            origin: OriginFor<T>,
            public_inputs: ProofPublicInputs<T::Hash>,
            proof: Vec<u8>,
            vk_id: u32,
        ) -> DispatchResult {
            if ensure_none(origin.clone()).is_err() {
                T::AdminOrigin::ensure_origin(origin)?;
            }
            let bounded: BoundedVec<u8, T::MaxProofSize> =
                proof.try_into().map_err(|_| Error::<T>::ProofTooLarge)?;
            PendingBlockProof::<T>::put((public_inputs, bounded, vk_id));
            Ok(())
        }
    }

    #[pallet::validate_unsigned]
    impl<T: Config> ValidateUnsigned for Pallet<T> {
        type Call = Call<T>;

        fn validate_unsigned(_source: TransactionSource, call: &Self::Call) -> TransactionValidity {
            if let Call::submit_block_proof { .. } = call {
                // MVP-ASSUMED: accept unsigned block-proof submissions; runtime execution enforces
                // correctness against header/expected inputs and strict mode policy.
                return ValidTransaction::with_tag_prefix("ZkVerifierBlockProof")
                    .priority(TransactionPriority::max_value())
                    .longevity(64_u64)
                    .propagate(true)
                    .build();
            }
            InvalidTransaction::Call.into()
        }
    }

    impl<T: Config> Pallet<T> {
        /// Core verification routine. Binds public inputs to expected roots/commitment supplied by caller.
        pub fn verify_block_proof(
            public_inputs: &ProofPublicInputs<T::Hash>,
            proof: &[u8],
            vk_id: u32,
        ) -> DispatchResult {
            let strict = StrictMode::<T>::get();
            let vk = VerifyingKey::<T>::get();

            if strict || vk.is_some() {
                ensure!(!proof.is_empty(), Error::<T>::MissingProof);
            }

            // DEVNET/MOCK path: accept if mock allowed and (non-strict or no vk configured).
            // MVP-ASSUMED: mock verifier for deterministic offline/dev builds (Section 5.5).
            if T::DevnetMockVerification::get() && (!strict || vk.is_none()) {
                Self::deposit_event(Event::MockVerificationAccepted { block_number: public_inputs.block_number });
                return Ok(());
            }

            // Production hook: real zk proof system verification must be integrated here.
            ensure!(vk.is_some(), Error::<T>::MissingVerifyingKey);
            ensure!(vk_id == VkVersion::<T>::get(), Error::<T>::VkVersionMismatch);
            ensure!(public_inputs.vk_version == VkVersion::<T>::get(), Error::<T>::VkVersionMismatch);
            ensure!(!proof.is_empty(), Error::<T>::MissingProof);

            // Placeholder deterministic "verification" over commitment: hash(vk || inputs || vk_id || proof)
            // must have leading zero nibble. Replace with actual verifier in production integration.
            let mut buf = Vec::with_capacity(vk.as_ref().map(|v| v.len()).unwrap_or(0) + proof.len() + 256);
            if let Some(v) = vk { buf.extend_from_slice(&v); }
            buf.extend_from_slice(&public_inputs.encode());
            buf.extend_from_slice(&vk_id.encode());
            buf.extend_from_slice(proof);
            let h = T::Hashing::hash(&buf);
            let bytes = h.as_ref();
            ensure!(!bytes.is_empty() && (bytes[0] & 0xF0) == 0, Error::<T>::VerificationFailed);
            Ok(())
        }

        /// Called from runtime executive/block import integration to enforce validity.
        pub fn require_valid_block_proof(
            expected_pre: T::Hash,
            expected_post: T::Hash,
            expected_extrinsics_root: T::Hash,
            block_number: u64,
        ) -> DispatchResult {
            let parent_hash = <frame_system::Pallet<T>>::parent_hash();
            let spec_version = <frame_system::Pallet<T>>::runtime_version().spec_version;
            let expected_vk = VkVersion::<T>::get();
            if let Some((inputs, proof, vk_id)) = PendingBlockProof::<T>::get() {
                let res = (|| -> DispatchResult {
                    ensure!(inputs.pre_state_root == expected_pre, Error::<T>::PublicInputMismatch);
                    ensure!(inputs.post_state_root == expected_post, Error::<T>::PublicInputMismatch);
                    ensure!(inputs.extrinsics_root == expected_extrinsics_root, Error::<T>::PublicInputMismatch);
                    ensure!(inputs.block_number == block_number, Error::<T>::PublicInputMismatch);
                    ensure!(inputs.parent_hash == parent_hash, Error::<T>::PublicInputMismatch);
                    ensure!(inputs.spec_version == spec_version, Error::<T>::PublicInputMismatch);
                    ensure!(inputs.vk_version == expected_vk, Error::<T>::VkVersionMismatch);
                    Self::verify_block_proof(&inputs, &proof, vk_id)
                })();
                match res {
                    Ok(()) => {
                        PendingBlockProof::<T>::kill();
                        let commitment = T::Hashing::hash(&proof[..]);
                        LastVerified::<T>::put((block_number, commitment));
                        Self::deposit_event(Event::ProofVerified { block_number });
                        Ok(())
                    }
                    Err(e) => {
                        Self::deposit_event(Event::ProofRejected { block_number });
                        Err(e)
                    }
                }
            } else if T::DevnetMockVerification::get() && !StrictMode::<T>::get() {
                Self::deposit_event(Event::MockVerificationAccepted { block_number });
                Ok(())
            } else {
                Err(Error::<T>::MissingProof.into())
            }
        }

        /// Convenience for callers constructing expected inputs (e.g. import/runtime hooks).
        pub fn expected_inputs(
            pre_state_root: T::Hash,
            post_state_root: T::Hash,
            extrinsics_root: T::Hash,
            block_number: u64,
        ) -> ProofPublicInputs<T::Hash> {
            ProofPublicInputs {
                vk_version: VkVersion::<T>::get(),
                spec_version: <frame_system::Pallet<T>>::runtime_version().spec_version,
                block_number,
                parent_hash: <frame_system::Pallet<T>>::parent_hash(),
                pre_state_root,
                post_state_root,
                extrinsics_root,
            }
        }
    }
}

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;
