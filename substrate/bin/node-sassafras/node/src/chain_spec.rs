#[cfg(feature = "use-session-pallet")]
use node_sassafras_runtime::SessionKeys;
use node_sassafras_runtime::{AccountId, RuntimeGenesisConfig, Signature, WASM_BINARY};
use sc_service::ChainType;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_consensus_sassafras::AuthorityId as SassafrasId;
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};

// Genesis constants for Sassafras parameters configuration.
const SASSAFRAS_TICKETS_MAX_ATTEMPTS_NUMBER: u32 = 8;
const SASSAFRAS_TICKETS_REDUNDANCY_FACTOR: u32 = 1;

/// Specialized `ChainSpec`. This is a specialization of the general Substrate ChainSpec type.
/// Ec-utils host functions required to construct the test `RingContext` instance.
pub type ChainSpec = sc_service::GenericChainSpec<
	RuntimeGenesisConfig,
	Option<()>,
	(
		sp_crypto_ec_utils::bls12_381::host_calls::HostFunctions,
		sp_crypto_ec_utils::ed_on_bls12_381_bandersnatch::host_calls::HostFunctions,
	),
>;

/// Generate a crypto pair from seed.
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Generate an account id from seed.
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Generate authority account id and keys from seed.
pub fn authority_keys_from_seed(seed: &str) -> (AccountId, SassafrasId, GrandpaId) {
	(
		get_account_id_from_seed::<sr25519::Public>(seed),
		get_from_seed::<SassafrasId>(seed),
		get_from_seed::<GrandpaId>(seed),
	)
}

pub fn development_config() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Development")
	.with_id("dev")
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

pub fn local_testnet_config() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(
		WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
		None,
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(testnet_genesis(
		// Initial PoA authorities
		vec![authority_keys_from_seed("Alice"), authority_keys_from_seed("Bob")],
		// Sudo account
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		// Pre-funded accounts
		vec![
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			get_account_id_from_seed::<sr25519::Public>("Bob"),
			get_account_id_from_seed::<sr25519::Public>("Charlie"),
			get_account_id_from_seed::<sr25519::Public>("Dave"),
			get_account_id_from_seed::<sr25519::Public>("Eve"),
			get_account_id_from_seed::<sr25519::Public>("Ferdie"),
			get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
			get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
			get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
			get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
			get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
		],
	))
	.build())
}

#[cfg(feature = "use-session-pallet")]
fn testnet_genesis(
	initial_authorities: Vec<(AccountId, SassafrasId, GrandpaId)>,
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
) -> serde_json::Value {
	serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().cloned().map(|k| (k, 1u64 << 60)).collect::<Vec<_>>(),
		},
		"sassafras": {
			"epochConfig": sp_consensus_sassafras::EpochConfiguration {
				attempts_number: SASSAFRAS_TICKETS_MAX_ATTEMPTS_NUMBER,
				redundancy_factor: SASSAFRAS_TICKETS_REDUNDANCY_FACTOR,
			},
		},
		"session": {
			"keys": initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						SessionKeys { sassafras: x.1.clone(), grandpa: x.2.clone() },
					)
				})
				.collect::<Vec<_>>(),
		},
		"sudo": {
			"key": Some(root_key),
		},
	})
}

#[cfg(not(feature = "use-session-pallet"))]
fn testnet_genesis(
	initial_authorities: Vec<(AccountId, SassafrasId, GrandpaId)>,
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
) -> serde_json::Value {
	serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().cloned().map(|k| (k, 1u64 << 60)).collect::<Vec<_>>(),
		},
		"sassafras": {
			"authorities": initial_authorities.iter().map(|x| x.1.clone()).collect::<Vec<_>>(),
			"epochConfig": sp_consensus_sassafras::EpochConfiguration {
				attempts_number: SASSAFRAS_TICKETS_MAX_ATTEMPTS_NUMBER,
				redundancy_factor: SASSAFRAS_TICKETS_REDUNDANCY_FACTOR,
			},
		},
		"grandpa": {
			"authorities": initial_authorities.iter().map(|x| (x.2.clone(), 1)).collect::<Vec<_>>(),
		},
		"sudo": {
			"key": Some(root_key),
		},
	})
}
