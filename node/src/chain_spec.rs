use elysium_runtime::{AccountId, SessionKeys, Signature, UNIT, WASM_BINARY};
use sc_service::ChainType;
use sp_consensus_aura::sr25519::AuthorityId as AuraId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::{traits::{IdentifyAccount, Verify}, Perbill};

pub type ChainSpec = sc_service::GenericChainSpec;

fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
    TPublic::Pair::from_string(&format!("//{seed}"), None)
        .expect("static values are valid; qed")
        .public()
}

type AccountPublic = <Signature as Verify>::Signer;

fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
    AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
    AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

fn authority_keys_from_seed(s: &str) -> (AccountId, AccountId, AuraId, GrandpaId) {
    (
        get_account_id_from_seed::<sr25519::Public>(&format!("{s}//stash")),
        get_account_id_from_seed::<sr25519::Public>(s),
        get_from_seed::<AuraId>(s),
        get_from_seed::<GrandpaId>(s),
    )
}

fn session_keys(aura: AuraId, grandpa: GrandpaId) -> SessionKeys {
    SessionKeys { aura, grandpa }
}

pub fn development_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Elysium Development")
    .with_id("elysium_dev")
    .with_chain_type(ChainType::Development)
    .with_genesis_config_patch(testnet_genesis(
        vec![authority_keys_from_seed("Alice")],
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
            get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
        ],
    ))
    .build())
}

pub fn local_testnet_chain_spec() -> Result<ChainSpec, String> {
    Ok(ChainSpec::builder(
        WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
        None,
    )
    .with_name("Elysium Local Testnet")
    .with_id("elysium_local")
    .with_chain_type(ChainType::Local)
    .with_genesis_config_patch(testnet_genesis(
        vec![
            authority_keys_from_seed("Alice"),
            authority_keys_from_seed("Bob"),
        ],
        get_account_id_from_seed::<sr25519::Public>("Alice"),
        vec![
            get_account_id_from_seed::<sr25519::Public>("Alice"),
            get_account_id_from_seed::<sr25519::Public>("Bob"),
            get_account_id_from_seed::<sr25519::Public>("Charlie"),
            get_account_id_from_seed::<sr25519::Public>("Dave"),
            get_account_id_from_seed::<sr25519::Public>("Eve"),
            get_account_id_from_seed::<sr25519::Public>("Ferdie"),
            get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
            get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
        ],
    ))
    .build())
}

fn testnet_genesis(
    initial_authorities: Vec<(AccountId, AccountId, AuraId, GrandpaId)>,
    root_key: AccountId,
    endowed_accounts: Vec<AccountId>,
) -> serde_json::Value {
    // Bootstrap genesis for NPoS devnet (M1): ECC is staking/native currency (Instance1),
    // HNN remains fee/treasury currency (Instance2). Each authority is configured as a
    // stash/controller self-bonded validator with session keys; suitable for multi-node devnets.
    let endowment = 1_000_000 * UNIT;
    let stake: u128 = 100_000 * UNIT;

    let session_keys: Vec<_> = initial_authorities
        .iter()
        .map(|(stash, controller, aura, grandpa)| {
            (
                stash.clone(),
                controller.clone(),
                session_keys(aura.clone(), grandpa.clone()),
            )
        })
        .collect();

    let stakers: Vec<_> = initial_authorities
        .iter()
        .map(|(stash, _controller, _aura, _grandpa)| {
            serde_json::json!([
                stash.clone(),                 // stash
                stash.clone(),                 // controller (same account for dev)
                stake,                         // stake (ECC)
                "Validator"                    // status
            ])
        })
        .collect();

    serde_json::json!({
        // ECC (Instance1) - native/system and staking balance
        "balances": {
            "balances": endowed_accounts.iter().cloned().map(|k| (k, endowment)).collect::<Vec<_>>(),
        },
        // HNN (Instance2) - fee currency
        "hnnBalances": {
            "balances": endowed_accounts.iter().cloned().map(|k| (k, endowment)).collect::<Vec<_>>(),
        },
        "session": {
            "keys": session_keys,
            "nonAuthorityKeys": []
        },
        "staking": {
            "validatorCount": initial_authorities.len() as u32,
            "minimumValidatorCount": 1u32,
            "invulnerables": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
            "forceEra": "NotForcing",
            "slashRewardFraction": Perbill::from_percent(10),
            "canceledPayout": 0u128,
            "stakers": stakers,
            "minNominatorBond": 1u128 * UNIT,
            "minValidatorBond": 1u128 * UNIT,
            "maxValidatorCount": null,
            "maxNominatorCount": null
        },
        "sudo": {
            "key": Some(root_key),
        },
        "elysiumAssets": {
            "assets": [
                [
                    0u32, // ECC
                    {
                        "name": [69,108,121,115,105,117,109],
                        "symbol": [69,67,67],
                        "decimals": 12u8,
                        "total_supply_declared": "1000000000000000000000",
                        "genesis_issued": "10000000000000000000",
                        "mined_to_date": "0",
                        "mineable": true,
                        "redeemable_eligible": true,
                        "reserve_account": null
                    },
                    {
                        "r_base_per_period": "1000000000000",
                        "k_curve": 1000000000000u128,
                        "min_multiplier": 10000000000u128,
                        "period_blocks": 10u32,
                        "bonus_budget": "100000000000000",
                        "bonus_consumed": "0",
                        "b_max_rate": 100000000000u128
                    },
                    "Open"
                ],
                [
                    1u32, // HNN
                    {
                        "name": [72,97,110,97],
                        "symbol": [72,78,78],
                        "decimals": 12u8,
                        "total_supply_declared": "1000000000000000000000",
                        "genesis_issued": "10000000000000000000",
                        "mined_to_date": "0",
                        "mineable": true,
                        "redeemable_eligible": true,
                        "reserve_account": null
                    },
                    {
                        "r_base_per_period": "1000000000000",
                        "k_curve": 1000000000000u128,
                        "min_multiplier": 10000000000u128,
                        "period_blocks": 10u32,
                        "bonus_budget": "100000000000000",
                        "bonus_consumed": "0",
                        "b_max_rate": 100000000000u128
                    },
                    "Open"
                ]
            ],
            "next_asset_id": 2u32
        },
        "beta": {
            "hnn_per_ecc_scaled": 1000000000000u128
        },
        "zkVerifier": {
            "strict_mode": false
        }
    })
}
