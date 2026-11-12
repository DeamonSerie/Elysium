use codec::Encode;
use frame_support::traits::Currency;
use pallet_contracts::chain_extension::{
    ChainExtension, Environment, Ext, InitState, RetVal,
};
use sp_runtime::DispatchError;

use crate::{AccountId, Balance, Runtime};

/// Elysium chain extension (Section 2.3, MVP-ASSUMED read-focused, stable func IDs v1).
/// ext_id 0, func_id:
/// 1 = ecc_free_balance(account: [u8;32]) -> u128
/// 2 = hnn_free_balance(account: [u8;32]) -> u128
/// 3 = total_deposited_by_asset(asset_id: le u32) -> u128
/// 4 = pool_deposit(pool_id: le u64) -> u128
/// 5 = beta_credits(account: [u8;32]) -> u128
/// 6 = pool_participant_count(pool_id: le u64) -> u32
/// 7 = mining_status(asset_id: le u32) -> u8 (MiningStatus discriminant)
/// 8 = mineable_remainder(asset_id: le u32) -> u128 (0 if unknown)
/// 9 = redemption_backing(asset_id: le u32) -> u128
#[derive(Default)]
pub struct ElysiumChainExtension;

impl ChainExtension<Runtime> for ElysiumChainExtension {
    fn call<E: Ext<T = Runtime>>(
        &mut self,
        env: Environment<E, InitState>,
    ) -> pallet_contracts::chain_extension::Result<RetVal> {
        let func_id = env.func_id();
        // Charge a conservative fixed weight for simple reads (MVP-ASSUMED).
        let mut env = env.buf_in_buf_out();
        env.charge_weight(frame_support::weights::Weight::from_parts(10_000, 0))?;

        let read_account = |env: &Environment<E, pallet_contracts::chain_extension::BufInBufOutState>| -> Result<AccountId, DispatchError> {
            let mut buf = [0u8; 32];
            let mut slice: &mut [u8] = &mut buf;
            env.read_into(&mut slice)?;
            Ok(AccountId::from(buf))
        };

        let result_bytes = match func_id {
            1 => {
                let who = read_account(&env)?;
                let v: Balance = <pallet_balances::Pallet<Runtime, crate::EccInstance> as Currency<AccountId>>::free_balance(&who);
                v.encode()
            }
            2 => {
                let who = read_account(&env)?;
                let v: Balance = <pallet_balances::Pallet<Runtime, crate::HnnInstance> as Currency<AccountId>>::free_balance(&who);
                v.encode()
            }
            3 => {
                let mut b = [0u8; 4];
                let mut slice: &mut [u8] = &mut b;
                env.read_into(&mut slice)?;
                let asset_id = u32::from_le_bytes(b);
                let v: Balance = pallet_liquidity_mining::TotalDepositedByAsset::<Runtime>::get(asset_id);
                v.encode()
            }
            4 => {
                let mut b = [0u8; 8];
                let mut slice: &mut [u8] = &mut b;
                env.read_into(&mut slice)?;
                let pool_id = u64::from_le_bytes(b);
                let v: Balance = pallet_liquidity_mining::Pallet::<Runtime>::pools(pool_id).map(|p| p.total_deposit).unwrap_or(0);
                v.encode()
            }
            5 => {
                let who = read_account(&env)?;
                let v: Balance = pallet_beta::Pallet::<Runtime>::beta_credits(&who);
                v.encode()
            }
            6 => {
                let mut b = [0u8; 8];
                let mut slice: &mut [u8] = &mut b;
                env.read_into(&mut slice)?;
                let pool_id = u64::from_le_bytes(b);
                let v: u32 = pallet_liquidity_mining::Pallet::<Runtime>::pools(pool_id).map(|p| p.participant_count).unwrap_or(0);
                v.encode()
            }
            7 => {
                let mut b = [0u8; 4];
                let mut slice: &mut [u8] = &mut b;
                env.read_into(&mut slice)?;
                let asset_id = u32::from_le_bytes(b);
                let s = pallet_elysium_assets_registry::Pallet::<Runtime>::mining_status(asset_id);
                let v: u8 = match s {
                    elysium_primitives::MiningStatus::Open => 0,
                    elysium_primitives::MiningStatus::Mining => 1,
                    elysium_primitives::MiningStatus::Complete => 2,
                };
                v.encode()
            }
            8 => {
                let mut b = [0u8; 4];
                let mut slice: &mut [u8] = &mut b;
                env.read_into(&mut slice)?;
                let asset_id = u32::from_le_bytes(b);
                let v: Balance = pallet_elysium_assets_registry::Pallet::<Runtime>::mineable_remainder(asset_id).unwrap_or(0);
                v.encode()
            }
            9 => {
                let mut b = [0u8; 4];
                let mut slice: &mut [u8] = &mut b;
                env.read_into(&mut slice)?;
                let asset_id = u32::from_le_bytes(b);
                let v: Balance = pallet_beta::RedemptionBacking::<Runtime>::get(asset_id);
                v.encode()
            }
            _ => return Err(DispatchError::Other("Unknown Elysium chain extension func_id")),
        };

        env.write(&result_bytes, false, None)?;
        Ok(RetVal::Converging(0))
    }
}
