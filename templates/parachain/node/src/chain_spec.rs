use cumulus_primitives_core::ParaId;
use runtime::{AccountId, AuraId, Signature, EXISTENTIAL_DEPOSIT};
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};
use staking_runtime as runtime;

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type ChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

/// The extensions for the [`ChainSpec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
pub struct Extensions {
	/// The relay chain of the Parachain.
	#[serde(alias = "relayChain", alias = "RelayChain")]
	pub relay_chain: String,
	/// The id of the Parachain.
	#[serde(alias = "paraId", alias = "ParaId")]
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn template_session_keys(keys: AuraId) -> runtime::SessionKeys {
	runtime::SessionKeys { aura: keys }
}

pub fn development_config() -> ChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "UNIT".into());
	properties.insert("tokenDecimals".into(), 12.into());
	properties.insert("ss58Format".into(), 42.into());

	ChainSpec::builder(
		runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions {
			relay_chain: "rococo_local_testnet".into(),
			// You MUST set this to the correct network!
			para_id: 2000,
		},
	)
	.with_name("Development")
	.with_id("dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_patch(testnet_genesis(
		// initial collators.
		vec![
			(
				utils::get_account_id_from_seed::<sr25519::Public>("Alice"),
				utils::get_collator_keys_from_seed("Alice"),
			),
			(
				utils::get_account_id_from_seed::<sr25519::Public>("Bob"),
				utils::get_collator_keys_from_seed("Bob"),
			),
		],
		vec![
			utils::get_account_id_from_seed::<sr25519::Public>("Alice"),
			utils::get_account_id_from_seed::<sr25519::Public>("Bob"),
			utils::get_account_id_from_seed::<sr25519::Public>("Charlie"),
			utils::get_account_id_from_seed::<sr25519::Public>("Dave"),
			utils::get_account_id_from_seed::<sr25519::Public>("Eve"),
			utils::get_account_id_from_seed::<sr25519::Public>("Ferdie"),
			utils::get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
		],
		utils::get_account_id_from_seed::<sr25519::Public>("Alice"),
		2000.into(),
	))
	.build()
}

pub fn local_testnet_config() -> ChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "UNIT".into());
	properties.insert("tokenDecimals".into(), 12.into());
	properties.insert("ss58Format".into(), 42.into());

	#[allow(deprecated)]
	ChainSpec::builder(
		runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo_local_testnet".into(), para_id: 2000 },
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(testnet_genesis(
		// initial collators.
		vec![(
			utils::get_account_id_from_seed::<sr25519::Public>("Charlie"),
			utils::get_collator_keys_from_seed("Charlie"),
		)],
		vec![
			utils::get_account_id_from_seed::<sr25519::Public>("Alice"),
			utils::get_account_id_from_seed::<sr25519::Public>("Bob"),
			utils::get_account_id_from_seed::<sr25519::Public>("Charlie"),
			utils::get_account_id_from_seed::<sr25519::Public>("Dave"),
			utils::get_account_id_from_seed::<sr25519::Public>("Eve"),
			utils::get_account_id_from_seed::<sr25519::Public>("Ferdie"),
			utils::get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
			utils::get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
		],
		utils::get_account_id_from_seed::<sr25519::Public>("Alice"),
		2000.into(),
	))
	.with_protocol_id("template-local")
	.with_properties(properties)
	.build()
}

fn testnet_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	mut endowed_accounts: Vec<AccountId>,
	root: AccountId,
	id: ParaId,
) -> serde_json::Value {
	let validators = 4_000;
	let nominators = 50_000;
	let edges = 24;
	//let validators_count = 1_000;
	let validators_count = 4_000;
	let max_validators_count = 4_000;

	let staking_gen = staking_genesis::generate(
		validators,
		nominators,
		edges,
		validators_count,
		max_validators_count,
	);

	endowed_accounts.append(
		&mut staking_gen
			.stakers
			.iter()
			.cloned()
			.map(|(k, _, _, _)| k)
			.rev()
			.collect::<Vec<_>>(),
	);

	serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().cloned().map(|k| (k, 1u64 << 60)).collect::<Vec<_>>(),
		},
		"parachainInfo": {
			"parachainId": id,
		},
		"collatorSelection": {
			"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
			"candidacyBond": EXISTENTIAL_DEPOSIT * 16,
		},
		"session": {
			"keys": invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                 // account id
						acc,                         // validator id
						template_session_keys(aura), // session keys
					)
				})
			.collect::<Vec<_>>(),
		},
		"polkadotXcm": {
			"safeXcmVersion": Some(SAFE_XCM_VERSION),
		},
		"sudo": { "key": Some(root) },
		"staking": staking_gen
	})
}

mod staking_genesis {
	use super::*;
	use pallet_staking::StakerStatus;

	use rand::Rng;

	pub(crate) fn generate(
		validators: u32,
		nominators: u32,
		edges: usize,
		validator_count: u32,
		max_validator_count: u32,
	) -> staking_runtime::StakingConfig {
		let mut targets = vec![];
		let mut stakers = vec![];

		for i in 0..validators {
			let stash =
				utils::get_account_id_from_seed::<sr25519::Public>(&utils::as_seed(i, "validator"));
			let stake = 1u128 << 50;
			targets.push(stash.clone());

			stakers.push((stash.clone(), stash, stake, StakerStatus::Validator));
		}

		for i in 0..nominators {
			let stash =
				utils::get_account_id_from_seed::<sr25519::Public>(&utils::as_seed(i, "nominator"));
			let stake = rand::thread_rng().gen_range((1u128 << 50)..(2u128 << 50));
			let nominations = utils::select_targets(edges, targets.clone());

			stakers.push((stash.clone(), stash, stake, StakerStatus::Nominator(nominations)));
		}

		staking_runtime::StakingConfig {
			stakers,
			validator_count,
			max_validator_count: Some(max_validator_count),
			..Default::default()
		}
	}
}

mod utils {
	use super::*;
	use rand::prelude::*;

	type AccountPublic = <Signature as Verify>::Signer;

	/// Helper function to generate a crypto pair from seed
	pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
		TPublic::Pair::from_string(&format!("//{}", seed), None)
			.expect("static values are valid; qed")
			.public()
	}

	/// Generate collator keys from seed.
	///
	/// This function's return type must always match the session keys of the chain in tuple format.
	pub fn get_collator_keys_from_seed(seed: &str) -> AuraId {
		get_from_seed::<AuraId>(seed)
	}

	/// Helper function to generate an account ID from seed
	pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
	where
		AccountPublic: From<<TPublic::Pair as Pair>::Public>,
	{
		AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
	}

	/// Seed generator from ID and domain.
	pub(crate) fn as_seed(id: u32, domain: &str) -> String {
		let seed = format!("{}-{}", domain, id);
		seed
	}

	/// Selects `n` random targets from a list of accounts.
	pub(crate) fn select_targets(n: usize, validators: Vec<AccountId>) -> Vec<AccountId> {
		validators
			.choose_multiple(&mut rand::thread_rng(), n)
			.cloned()
			.collect::<Vec<_>>()
	}
}
