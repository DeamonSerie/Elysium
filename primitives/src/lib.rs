#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use codec::{Decode, DecodeWithMemTracking, Encode, MaxEncodedLen};
use core::fmt::Debug;
use scale_info::TypeInfo;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

pub type AssetId = u32;
pub type PoolId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;
pub type AccountId32 = sp_runtime::AccountId32;

/// Native currency identifiers.
pub const ECC_ASSET_ID: AssetId = 0;
pub const HNN_ASSET_ID: AssetId = 1;

/// Pragmatic decimals (12) for ECC/HNN in devnet.
pub const NATIVE_DECIMALS: u32 = 12;
pub const UNIT: Balance = 1_000_000_000_000;
pub const MILLIUNIT: Balance = 1_000_000_000;
pub const MICROUNIT: Balance = 1_000_000;

/// Fixed-point precision for rate/bonus math (1e12).
pub const RATE_PRECISION: u128 = 1_000_000_000_000;

// --- MVP-ASSUMED centralized parameters (Section 5.6) ---
pub const DEFAULT_K_CURVE: u128 = RATE_PRECISION; // 1.0
pub const DEFAULT_MIN_MULTIPLIER: u128 = RATE_PRECISION / 100; // 1%
pub const DEFAULT_PERIOD_BLOCKS: u32 = 10;
pub const DEFAULT_B_MAX_RATE: u128 = RATE_PRECISION / 10; // 10%
pub const MAX_PROOF_SIZE: u32 = 1024 * 1024;
pub const MAX_VK_SIZE: u32 = 512 * 1024;
pub const SESSION_LENGTH_BLOCKS: u32 = 10;
pub const SESSIONS_PER_ERA: u32 = 6;
pub const BONDING_DURATION_ERAS: u32 = 28;
pub const SLASH_DEFER_DURATION_ERAS: u32 = 27;

/// Per-asset economic and issuance metadata.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct AssetMetadata<Balance, AccountId> {
    pub name: BoundedName,
    pub symbol: BoundedSymbol,
    pub decimals: u8,
    pub total_supply_declared: Balance,
    pub genesis_issued: Balance,
    pub mined_to_date: Balance,
    pub mineable: bool,
    /// Eligible to be chosen as 1:1 β redemption target when sufficiently backed.
    pub redeemable_eligible: bool,
    /// Account holding protocol reserve/backing for this asset (if applicable).
    pub reserve_account: Option<AccountId>,
}

pub type BoundedName = sp_runtime::BoundedVec<u8, sp_runtime::traits::ConstU32<32>>;
pub type BoundedSymbol = sp_runtime::BoundedVec<u8, sp_runtime::traits::ConstU32<16>>;

/// Mining lifecycle for a mineable asset.
#[derive(Clone, Copy, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub enum MiningStatus {
    #[default]
    Open,
    Mining,
    Complete,
}

/// Parameters controlling mining emission for an asset.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct MiningParams<Balance> {
    /// Base rate in target-asset units per accrual period (scaled by RATE_PRECISION).
    pub r_base_per_period: Balance,
    /// Curve parameter k for f(d)=1/(1+k*d), scaled by RATE_PRECISION.
    pub k_curve: u128,
    /// Minimum multiplier for f(d), scaled by RATE_PRECISION (floor).
    pub min_multiplier: u128,
    /// Blocks per accrual period.
    pub period_blocks: u32,
    /// Maximum bonus budget (β credits/target units) allocatable for pool uniformity bonus.
    pub bonus_budget: Balance,
    /// Bonus budget already consumed.
    pub bonus_consumed: Balance,
    /// B_max as share of pool-accrued amount, scaled by RATE_PRECISION.
    pub b_max_rate: u128,
}

impl<Balance: Default> Default for MiningParams<Balance> {
    fn default() -> Self {
        Self {
            r_base_per_period: Balance::default(),
            k_curve: DEFAULT_K_CURVE, // MVP-ASSUMED k=1.0
            min_multiplier: DEFAULT_MIN_MULTIPLIER, // MVP-ASSUMED 1% floor
            period_blocks: DEFAULT_PERIOD_BLOCKS,
            bonus_budget: Balance::default(),
            bonus_consumed: Balance::default(),
            b_max_rate: DEFAULT_B_MAX_RATE, // MVP-ASSUMED 10%
        }
    }
}

/// Public inputs committed to a block validity proof (V1, Section 3.1/5.5).
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct ZkPublicInputsV1<Hash> {
    pub vk_version: u32,
    pub spec_version: u32,
    pub block_number: u64,
    pub parent_hash: Hash,
    pub pre_state_root: Hash,
    pub post_state_root: Hash,
    pub extrinsics_root: Hash,
}

/// Backwards-compatible alias for scaffold code.
pub type ProofPublicInputs<Hash> = ZkPublicInputsV1<Hash>;

/// Per-block proof payload (opaque bytes bounded in runtime config in practice).
#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BlockProof {
    pub proof: alloc::vec::Vec<u8>,
    pub vk_id: u32,
}

/// Extended proof record bound to public inputs.
#[derive(Clone, Encode, Decode, Eq, PartialEq, Debug, TypeInfo, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BlockProofRecord<Hash> {
    pub inputs: ZkPublicInputsV1<Hash>,
    pub proof: alloc::vec::Vec<u8>,
    pub vk_id: u32,
}

/// Outcome of bundle sale splits (amounts in respective currencies).
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct SaleSplit<Balance> {
    pub seller_ecc: Balance,
    pub treasury_hnn: Balance,
    pub reserve_ecc: Balance,
}

/// Parameters for beta redemption eligibility/backing.
#[derive(Clone, Encode, Decode, DecodeWithMemTracking, Eq, PartialEq, Debug, TypeInfo, MaxEncodedLen, Default)]
#[cfg_attr(feature = "std", derive(Serialize, Deserialize))]
pub struct BackingInfo<Balance> {
    pub available_backing: Balance,
}

/// Compute 49.999%/0.001%/50% split of X (ECC) with explicit rounding.
/// - seller = 49.999%
/// - reserve = 50%
/// - treasury fee expressed as 0.001% of X in HNN-equivalent units (caller supplies conversion)
/// Rounding: floor components, remainder assigned to reserve to preserve conservation.
pub fn split_sale_x(x: u128) -> (u128, u128) {
    // seller_ecc = x * 49999 / 100000
    let seller = x.saturating_mul(49_999) / 100_000;
    let reserve = x.saturating_sub(seller);
    (seller, reserve)
}

/// Treasury fee in HNN units as 0.001% of X using a fixed conversion rate:
/// hnn_fee = x * rate_hnn_per_ecc / 100000, where rate is scaled by RATE_PRECISION.
pub fn treasury_hnn_fee(x_ecc: u128, hnn_per_ecc_scaled: u128) -> u128 {
    // 0.001% = 1/100000
    x_ecc
        .saturating_mul(hnn_per_ecc_scaled)
        .saturating_div(RATE_PRECISION)
        .saturating_div(100_000)
}

/// f(d) = max(min_multiplier, RATE_PRECISION / (1 + k*d/RATE_PRECISION))
/// where d_scaled = |D-S|/S scaled by RATE_PRECISION.
pub fn rate_multiplier(d_scaled: u128, k_curve: u128, min_multiplier: u128) -> u128 {
    let one = RATE_PRECISION;
    let kd = k_curve.saturating_mul(d_scaled) / one;
    let denom = one.saturating_add(kd);
    if denom == 0 {
        return min_multiplier;
    }
    let m = one.saturating_mul(one) / denom;
    if m < min_multiplier { min_multiplier } else { m }
}

/// Normalized distance d_scaled = |D-S|/S * RATE_PRECISION. Returns RATE_PRECISION on S==0.
pub fn normalized_distance(d_deposit: u128, s_target: u128) -> u128 {
    if s_target == 0 {
        return RATE_PRECISION;
    }
    let diff = d_deposit.abs_diff(s_target);
    diff.saturating_mul(RATE_PRECISION) / s_target
}

/// Solo effective rate factor relative to comparable pool total rate.
/// Equals 1% when participant counts equal; scales by min(1, solo_count/pool_count) otherwise.
pub fn solo_rate_factor_scaled(solo_count: u32, pool_participant_count: u32) -> u128 {
    if solo_count == 0 {
        return 0;
    }
    let base = RATE_PRECISION / 100; // 1%
    if pool_participant_count == 0 {
        return 0;
    }
    if solo_count == pool_participant_count {
        base
    } else {
        // scale: base * solo_count / pool_count, capped at base
        let scaled = base.saturating_mul(solo_count as u128) / (pool_participant_count as u128);
        if scaled > base { base } else { scaled }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn split_conserves() {
        let x = 1_000_000u128;
        let (s, r) = split_sale_x(x);
        assert_eq!(s + r, x);
        assert_eq!(s, 499_990);
        assert_eq!(r, 500_010);
    }

    #[test]
    fn rate_multiplier_peaks_at_zero_distance() {
        let m0 = rate_multiplier(0, RATE_PRECISION, RATE_PRECISION / 100);
        let m1 = rate_multiplier(RATE_PRECISION, RATE_PRECISION, RATE_PRECISION / 100);
        assert!(m0 > m1);
        assert_eq!(m0, RATE_PRECISION);
    }

    #[test]
    fn solo_factor_equal_counts() {
        assert_eq!(solo_rate_factor_scaled(5, 5), RATE_PRECISION / 100);
    }
}
