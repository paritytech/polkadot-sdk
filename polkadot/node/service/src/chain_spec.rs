// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Polkadot chain configurations.

#[cfg(feature = "westend-native")]
use pallet_staking::Forcing;
use polkadot_primitives::{AccountId, AccountPublic, AssignmentId, ValidatorId};
use sc_consensus_grandpa::AuthorityId as GrandpaId;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_beefy::ecdsa_crypto::AuthorityId as BeefyId;

#[cfg(feature = "westend-native")]
use polkadot_primitives::vstaging::SchedulerParams;
#[cfg(feature = "rococo-native")]
use rococo_runtime as rococo;
use sc_chain_spec::ChainSpecExtension;
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
use sc_chain_spec::ChainType;
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
use sc_telemetry::TelemetryEndpoints;
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::IdentifyAccount;
#[cfg(feature = "westend-native")]
use sp_runtime::Perbill;
#[cfg(feature = "westend-native")]
use westend_runtime as westend;
#[cfg(feature = "westend-native")]
use westend_runtime_constants::currency::UNITS as WND;

#[cfg(feature = "westend-native")]
const WESTEND_STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
#[cfg(feature = "rococo-native")]
const ROCOCO_STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
#[cfg(feature = "rococo-native")]
const VERSI_STAGING_TELEMETRY_URL: &str = "wss://telemetry.polkadot.io/submit/";
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
const DEFAULT_PROTOCOL_ID: &str = "dot";

/// Node `ChainSpec` extensions.
///
/// Additional parameters for some Substrate core modules,
/// customizable from the chain spec.
#[derive(Default, Clone, Serialize, Deserialize, ChainSpecExtension)]
#[serde(rename_all = "camelCase")]
pub struct Extensions {
	/// Block numbers with known hashes.
	pub fork_blocks: sc_client_api::ForkBlocks<polkadot_primitives::Block>,
	/// Known bad block hashes.
	pub bad_blocks: sc_client_api::BadBlocks<polkadot_primitives::Block>,
	/// The light sync state.
	///
	/// This value will be set by the `sync-state rpc` implementation.
	pub light_sync_state: sc_sync_state_rpc::LightSyncStateExtension,
}

// Generic chain spec, in case when we don't have the native runtime.
pub type GenericChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The `ChainSpec` parameterized for the westend runtime.
#[cfg(feature = "westend-native")]
pub type WestendChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The `ChainSpec` parameterized for the westend runtime.
// Dummy chain spec, but that is fine when we don't have the native runtime.
#[cfg(not(feature = "westend-native"))]
pub type WestendChainSpec = GenericChainSpec;

/// The `ChainSpec` parameterized for the rococo runtime.
#[cfg(feature = "rococo-native")]
pub type RococoChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The `ChainSpec` parameterized for the rococo runtime.
// Dummy chain spec, but that is fine when we don't have the native runtime.
#[cfg(not(feature = "rococo-native"))]
pub type RococoChainSpec = GenericChainSpec;

pub fn polkadot_config() -> Result<GenericChainSpec, String> {
	GenericChainSpec::from_json_bytes(&include_bytes!("../chain-specs/polkadot.json")[..])
}

pub fn kusama_config() -> Result<GenericChainSpec, String> {
	GenericChainSpec::from_json_bytes(&include_bytes!("../chain-specs/kusama.json")[..])
}

pub fn westend_config() -> Result<WestendChainSpec, String> {
	WestendChainSpec::from_json_bytes(&include_bytes!("../chain-specs/westend.json")[..])
}

pub fn paseo_config() -> Result<GenericChainSpec, String> {
	GenericChainSpec::from_json_bytes(&include_bytes!("../chain-specs/paseo.json")[..])
}

pub fn rococo_config() -> Result<RococoChainSpec, String> {
	RococoChainSpec::from_json_bytes(&include_bytes!("../chain-specs/rococo.json")[..])
}

/// This is a temporary testnet that uses the same runtime as rococo.
pub fn wococo_config() -> Result<RococoChainSpec, String> {
	RococoChainSpec::from_json_bytes(&include_bytes!("../chain-specs/wococo.json")[..])
}

/// The default parachains host configuration.
#[cfg(feature = "westend-native")]
fn default_parachains_host_configuration(
) -> polkadot_runtime_parachains::configuration::HostConfiguration<polkadot_primitives::BlockNumber>
{
	use polkadot_primitives::{
		node_features::FeatureIndex, ApprovalVotingParams, AsyncBackingParams, MAX_CODE_SIZE,
		MAX_POV_SIZE,
	};

	polkadot_runtime_parachains::configuration::HostConfiguration {
		validation_upgrade_cooldown: 2u32,
		validation_upgrade_delay: 2,
		code_retention_period: 1200,
		max_code_size: MAX_CODE_SIZE,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 32 * 1024,
		max_upward_queue_count: 8,
		max_upward_queue_size: 1024 * 1024,
		max_downward_message_size: 1024 * 1024,
		max_upward_message_size: 50 * 1024,
		max_upward_message_num_per_candidate: 5,
		hrmp_sender_deposit: 0,
		hrmp_recipient_deposit: 0,
		hrmp_channel_max_capacity: 8,
		hrmp_channel_max_total_size: 8 * 1024,
		hrmp_max_parachain_inbound_channels: 4,
		hrmp_channel_max_message_size: 1024 * 1024,
		hrmp_max_parachain_outbound_channels: 4,
		hrmp_max_message_num_per_candidate: 5,
		dispute_period: 6,
		no_show_slots: 2,
		n_delay_tranches: 25,
		needed_approvals: 2,
		relay_vrf_modulo_samples: 2,
		zeroth_delay_tranche_width: 0,
		minimum_validation_upgrade_delay: 5,
		async_backing_params: AsyncBackingParams {
			max_candidate_depth: 3,
			allowed_ancestry_len: 2,
		},
		node_features: bitvec::vec::BitVec::from_element(
			1u8 << (FeatureIndex::ElasticScalingMVP as usize) |
				1u8 << (FeatureIndex::EnableAssignmentsV2 as usize),
		),
		scheduler_params: SchedulerParams {
			lookahead: 2,
			group_rotation_frequency: 20,
			paras_availability_period: 4,
			..Default::default()
		},
		approval_voting_params: ApprovalVotingParams { max_approval_coalesce_count: 5 },
		..Default::default()
	}
}

#[cfg(feature = "westend-native")]
#[test]
fn default_parachains_host_configuration_is_consistent() {
	default_parachains_host_configuration().panic_if_not_consistent();
}

#[cfg(feature = "westend-native")]
fn westend_session_keys(
	babe: BabeId,
	grandpa: GrandpaId,
	para_validator: ValidatorId,
	para_assignment: AssignmentId,
	authority_discovery: AuthorityDiscoveryId,
	beefy: BeefyId,
) -> westend::SessionKeys {
	westend::SessionKeys {
		babe,
		grandpa,
		para_validator,
		para_assignment,
		authority_discovery,
		beefy,
	}
}

#[cfg(feature = "westend-native")]
fn westend_staging_testnet_config_genesis() -> serde_json::Value {
	use hex_literal::hex;
	use sp_core::crypto::UncheckedInto;

	// Following keys are used in genesis config for development chains.
	// DO NOT use them in production chains as the secret seed is public.
	//
	// SECRET_SEED="slow awkward present example safe bundle science ocean cradle word tennis earn"
	// subkey inspect -n polkadot "$SECRET_SEED"
	let endowed_accounts = vec![
		// 15S75FkhCWEowEGfxWwVfrW3LQuy8w8PNhVmrzfsVhCMjUh1
		hex!["c416837e232d9603e83162ef4bda08e61580eeefe60fe92fc044aa508559ae42"].into(),
	];
	// SECRET=$SECRET_SEED ./scripts/prepare-test-net.sh 4
	let initial_authorities: Vec<(
		AccountId,
		AccountId,
		BabeId,
		GrandpaId,
		ValidatorId,
		AssignmentId,
		AuthorityDiscoveryId,
		BeefyId,
	)> = vec![
		(
			//5EvydUTtHvt39Khac3mMxNPgzcfu49uPDzUs3TL7KEzyrwbw
			hex!["7ecfd50629cdd246649959d88d490b31508db511487e111a52a392e6e458f518"].into(),
			//5HQyX5gyy77m9QLXguAhiwjTArHYjYspeY98dYDu1JDetfZg
			hex!["eca2cca09bdc66a7e6d8c3d9499a0be2ad4690061be8a9834972e17d13d2fe7e"].into(),
			//5G13qYRudTyttwTJvHvnwp8StFtcfigyPnwfD4v7LNopsnX4
			hex!["ae27367cb77850fb195fe1f9c60b73210409e68c5ad953088070f7d8513d464c"]
				.unchecked_into(),
			//5Eb7wM65PNgtY6e33FEAzYtU5cRTXt6WQvZTnzaKQwkVcABk
			hex!["6faae44b21c6f2681a7f60df708e9f79d340f7d441d28bd987fab8d05c6487e8"]
				.unchecked_into(),
			//5FqMLAgygdX9UqzukDp15Uid9PAKdFAR621U7xtp5ut2NfrW
			hex!["a6c1a5b501985a83cb1c37630c5b41e6b0a15b3675b2fd94694758e6cfa6794d"]
				.unchecked_into(),
			//5DhXAV75BKvF9o447ikWqLttyL2wHtLMFSX7GrsKF9Ny61Ta
			hex!["485051748ab9c15732f19f3fbcf1fd00a6d9709635f084505107fbb059c33d2f"]
				.unchecked_into(),
			//5GNHfmrtWLTawnGCmc39rjAEiW97vKvE7DGePYe4am5JtE4i
			hex!["be59ed75a72f7b47221ce081ba4262cf2e1ea7867e30e0b3781822f942b97677"]
				.unchecked_into(),
			//5DA6Z8RUF626stn94aTRBCeobDCYcFbU7Pdk4Tz1R9vA8B8F
			hex!["0207e43990799e1d02b0507451e342a1240ff836ea769c57297589a5fd072ad8f4"]
				.unchecked_into(),
		),
		(
			//5DFpvDUdCgw54E3E357GR1PyJe3Ft9s7Qyp7wbELAoJH9RQa
			hex!["34b7b3efd35fcc3c1926ca065381682b1af29b57dabbcd091042c6de1d541b7d"].into(),
			//5DZSSsND5wCjngvyXv27qvF3yPzt3MCU8rWnqNy4imqZmjT8
			hex!["4226796fa792ac78875e023ff2e30e3c2cf79f0b7b3431254cd0f14a3007bc0e"].into(),
			//5CPrgfRNDQvQSnLRdeCphP3ibj5PJW9ESbqj2fw29vBMNQNn
			hex!["0e9b60f04be3bffe362eb2212ea99d2b909b052f4bff7c714e13c2416a797f5d"]
				.unchecked_into(),
			//5FXFsPReTUEYPRNKhbTdUathcWBsxTNsLbk2mTpYdKCJewjA
			hex!["98f4d81cb383898c2c3d54dab28698c0f717c81b509cb32dc6905af3cc697b18"]
				.unchecked_into(),
			//5CZjurB78XbSHf6SLkLhCdkqw52Zm7aBYUDdfkLqEDWJ9Zhj
			hex!["162508accd470e379b04cb0c7c60b35a7d5357e84407a89ed2dd48db4b726960"]
				.unchecked_into(),
			//5DkAqCtSjUMVoJFauuGoAbSEgn2aFCRGziKJiLGpPwYgE1pS
			hex!["4a559c028b69a7f784ce553393e547bec0aa530352157603396d515f9c83463b"]
				.unchecked_into(),
			//5GsBt9MhGwkg8Jfb1F9LAy2kcr88WNyNy4L5ezwbCr8NWKQU
			hex!["d464908266c878acbf181bf8fda398b3aa3fd2d05508013e414aaece4cf0d702"]
				.unchecked_into(),
			//5DtJVkz8AHevEnpszy3X4dUcPvACW6x1qBMQZtFxjexLr5bq
			hex!["02fdf30222d2cb88f2376d558d3de9cb83f9fde3aa4b2dd40c93e3104e3488bcd2"]
				.unchecked_into(),
		),
		(
			//5E2cob2jrXsBkTih56pizwSqENjE4siaVdXhaD6akLdDyVq7
			hex!["56e0f73c563d49ee4a3971c393e17c44eaa313dabad7fcf297dc3271d803f303"].into(),
			//5D4rNYgP9uFNi5GMyDEXTfiaFLjXyDEEX2VvuqBVi3f1qgCh
			hex!["2c58e5e1d5aef77774480cead4f6876b1a1a6261170166995184d7f86140572b"].into(),
			//5Ea2D65KXqe625sz4uV1jjhSfuigVnkezC8VgEj9LXN7ERAk
			hex!["6ed45cb7af613be5d88a2622921e18d147225165f24538af03b93f2a03ce6e13"]
				.unchecked_into(),
			//5G4kCbgqUhEyrRHCyFwFEkgBZXoYA8sbgsRxT9rY8Tp5Jj5F
			hex!["b0f8d2b9e4e1eafd4dab6358e0b9d5380d78af27c094e69ae9d6d30ca300fd86"]
				.unchecked_into(),
			//5CS7thd2n54WfqeKU3cjvZzK4z5p7zku1Zw97mSzXgPioAAs
			hex!["1055100a283968271a0781450b389b9093231be809be1e48a305ebad2a90497e"]
				.unchecked_into(),
			//5DSaL4ZmSYarZSazhL5NQh7LT6pWhNRDcefk2QS9RxEXfsJe
			hex!["3cea4ab74bab4adf176cf05a6e18c1599a7bc217d4c6c217275bfbe3b037a527"]
				.unchecked_into(),
			//5CaNLkYEbFYXZodXhd3UjV6RNLjFGNLiYafc8X5NooMkZiAq
			hex!["169faa81aebfe74533518bda28567f2e2664014c8905aa07ea003336afda5a58"]
				.unchecked_into(),
			//5ERwhKiePayukzZStMuzGzRJGxGRFpwxYUXVarQpMSMrXzDS
			hex!["03429d0d20f6ac5ca8b349f04d014f7b5b864acf382a744104d5d9a51108156c0f"]
				.unchecked_into(),
		),
		(
			//5H6j9ovzYk9opckVjvM9SvVfaK37ASTtPTzWeRfqk1tgLJUN
			hex!["deb804ed2ed2bb696a3dd4ed7de4cd5c496528a2b204051c6ace385bacd66a3a"].into(),
			//5DJ51tMW916mGwjMpfS1o9skcNt6Sb28YnZQXaKVg4h89agE
			hex!["366da6a748afedb31f07902f2de36ab265beccee37762d3ae1f237de234d9c36"].into(),
			//5CSPYDYoCDGSoSLgSp4EHkJ52YasZLHG2woqhPZkdbtNQpke
			hex!["1089bc0cd60237d061872925e81d36c9d9205d250d5d8b542c8e08a8ecf1b911"]
				.unchecked_into(),
			//5ChfdrAqmLjCeDJvynbMjcxYLHYzPe8UWXd3HnX9JDThUMbn
			hex!["1c309a70b4e274314b84c9a0a1f973c9c4fc084df5479ef686c54b1ae4950424"]
				.unchecked_into(),
			//5D8C3HHEp5E8fJsXRD56494F413CdRSR9QKGXe7v5ZEfymdj
			hex!["2ee4d78f328db178c54f205ac809da12e291a33bcbd4f29f081ce7e74bdc5044"]
				.unchecked_into(),
			//5GxeTYCGmp1C3ZRLDkRWqJc6gB2GYmuqnygweuH3vsivMQq6
			hex!["d88e40e3c2c7a7c5abf96ffdd8f7b7bec8798cc277bc97e255881871ab73b529"]
				.unchecked_into(),
			//5DoGpsgSLcJsHa9B8V4PKjxegWAqDZttWfxicAd68prUX654
			hex!["4cb3863271b70daa38612acd5dae4f5afcb7c165fa277629e5150d2214df322a"]
				.unchecked_into(),
			//5G1KLjqFyMsPAodnjSRkwRFJztTTEzmZWxow2Q3ZSRCPdthM
			hex!["03be5ec86d10a94db89c9b7a396d3c7742e3bec5f85159d4cf308cef505966ddf5"]
				.unchecked_into(),
		),
	];

	const ENDOWMENT: u128 = 1_000_000 * WND;
	const STASH: u128 = 100 * WND;

	serde_json::json!({
		"balances": {
			"balances": endowed_accounts
				.iter()
				.map(|k: &AccountId| (k.clone(), ENDOWMENT))
				.chain(initial_authorities.iter().map(|x| (x.0.clone(), STASH)))
				.collect::<Vec<_>>(),
		},
		"session": {
			"keys": initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						westend_session_keys(
							x.2.clone(),
							x.3.clone(),
							x.4.clone(),
							x.5.clone(),
							x.6.clone(),
							x.7.clone(),
						),
					)
				})
				.collect::<Vec<_>>(),
		},
		"staking": {
			"validatorCount": 50,
			"minimumValidatorCount": 4,
			"stakers": initial_authorities
				.iter()
				.map(|x| (x.0.clone(), x.0.clone(), STASH, westend::StakerStatus::<AccountId>::Validator))
				.collect::<Vec<_>>(),
			"invulnerables": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
			"forceEra": Forcing::ForceNone,
			"slashRewardFraction": Perbill::from_percent(10),
		},
		"babe": {
			"epochConfig": Some(westend::BABE_GENESIS_EPOCH_CONFIG),
		},
		"sudo": { "key": Some(endowed_accounts[0].clone()) },
		"configuration": {
			"config": default_parachains_host_configuration(),
		},
		"registrar": {
			"nextFreeParaId": polkadot_primitives::LOWEST_PUBLIC_ID,
		},
	})
}

/// Westend staging testnet config.
#[cfg(feature = "westend-native")]
pub fn westend_staging_testnet_config() -> Result<WestendChainSpec, String> {
	Ok(WestendChainSpec::builder(
		westend::WASM_BINARY.ok_or("Westend development wasm not available")?,
		Default::default(),
	)
	.with_name("Westend Staging Testnet")
	.with_id("westend_staging_testnet")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_patch(westend_staging_testnet_config_genesis())
	.with_telemetry_endpoints(
		TelemetryEndpoints::new(vec![(WESTEND_STAGING_TELEMETRY_URL.to_string(), 0)])
			.expect("Westend Staging telemetry url is valid; qed"),
	)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Rococo staging testnet config.
#[cfg(feature = "rococo-native")]
pub fn rococo_staging_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Rococo development wasm not available")?,
		Default::default(),
	)
	.with_name("Rococo Staging Testnet")
	.with_id("rococo_staging_testnet")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name("staging_testnet")
	.with_telemetry_endpoints(
		TelemetryEndpoints::new(vec![(ROCOCO_STAGING_TELEMETRY_URL.to_string(), 0)])
			.expect("Rococo Staging telemetry url is valid; qed"),
	)
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

pub fn versi_chain_spec_properties() -> serde_json::map::Map<String, serde_json::Value> {
	serde_json::json!({
		"ss58Format": 42,
		"tokenDecimals": 12,
		"tokenSymbol": "VRS",
	})
	.as_object()
	.expect("Map given; qed")
	.clone()
}

/// Versi staging testnet config.
#[cfg(feature = "rococo-native")]
pub fn versi_staging_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Versi development wasm not available")?,
		Default::default(),
	)
	.with_name("Versi Staging Testnet")
	.with_id("versi_staging_testnet")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name("staging_testnet")
	.with_telemetry_endpoints(
		TelemetryEndpoints::new(vec![(VERSI_STAGING_TELEMETRY_URL.to_string(), 0)])
			.expect("Versi Staging telemetry url is valid; qed"),
	)
	.with_protocol_id("versi")
	.with_properties(versi_chain_spec_properties())
	.build())
}

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Helper function to generate stash, controller and session key from seed
pub fn get_authority_keys_from_seed(
	seed: &str,
) -> (
	AccountId,
	AccountId,
	BabeId,
	GrandpaId,
	ValidatorId,
	AssignmentId,
	AuthorityDiscoveryId,
	BeefyId,
) {
	let keys = get_authority_keys_from_seed_no_beefy(seed);
	(keys.0, keys.1, keys.2, keys.3, keys.4, keys.5, keys.6, get_from_seed::<BeefyId>(seed))
}

/// Helper function to generate stash, controller and session key from seed
pub fn get_authority_keys_from_seed_no_beefy(
	seed: &str,
) -> (AccountId, AccountId, BabeId, GrandpaId, ValidatorId, AssignmentId, AuthorityDiscoveryId) {
	(
		get_account_id_from_seed::<sr25519::Public>(&format!("{}//stash", seed)),
		get_account_id_from_seed::<sr25519::Public>(seed),
		get_from_seed::<BabeId>(seed),
		get_from_seed::<GrandpaId>(seed),
		get_from_seed::<ValidatorId>(seed),
		get_from_seed::<AssignmentId>(seed),
		get_from_seed::<AuthorityDiscoveryId>(seed),
	)
}

#[cfg(feature = "westend-native")]
fn testnet_accounts() -> Vec<AccountId> {
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
	]
}

/// Helper function to create westend runtime `GenesisConfig` patch for testing
#[cfg(feature = "westend-native")]
pub fn westend_testnet_genesis(
	initial_authorities: Vec<(
		AccountId,
		AccountId,
		BabeId,
		GrandpaId,
		ValidatorId,
		AssignmentId,
		AuthorityDiscoveryId,
		BeefyId,
	)>,
	root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
) -> serde_json::Value {
	let endowed_accounts: Vec<AccountId> = endowed_accounts.unwrap_or_else(testnet_accounts);

	const ENDOWMENT: u128 = 1_000_000 * WND;
	const STASH: u128 = 100 * WND;

	serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().map(|k| (k.clone(), ENDOWMENT)).collect::<Vec<_>>(),
		},
		"session": {
			"keys": initial_authorities
				.iter()
				.map(|x| {
					(
						x.0.clone(),
						x.0.clone(),
						westend_session_keys(
							x.2.clone(),
							x.3.clone(),
							x.4.clone(),
							x.5.clone(),
							x.6.clone(),
							x.7.clone(),
						),
					)
				})
				.collect::<Vec<_>>(),
		},
		"staking": {
			"minimumValidatorCount": 1,
			"validatorCount": initial_authorities.len() as u32,
			"stakers": initial_authorities
				.iter()
				.map(|x| (x.0.clone(), x.0.clone(), STASH, westend::StakerStatus::<AccountId>::Validator))
				.collect::<Vec<_>>(),
			"invulnerables": initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
			"forceEra": Forcing::NotForcing,
			"slashRewardFraction": Perbill::from_percent(10),
		},
		"babe": {
			"epochConfig": Some(westend::BABE_GENESIS_EPOCH_CONFIG),
		},
		"sudo": { "key": Some(root_key) },
		"configuration": {
			"config": default_parachains_host_configuration(),
		},
		"registrar": {
			"nextFreeParaId": polkadot_primitives::LOWEST_PUBLIC_ID,
		},
	})
}

#[cfg(feature = "westend-native")]
fn westend_development_config_genesis() -> serde_json::Value {
	westend_testnet_genesis(
		vec![get_authority_keys_from_seed("Alice")],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
	)
}

/// Westend development config (single validator Alice)
#[cfg(feature = "westend-native")]
pub fn westend_development_config() -> Result<WestendChainSpec, String> {
	Ok(WestendChainSpec::builder(
		westend::WASM_BINARY.ok_or("Westend development wasm not available")?,
		Default::default(),
	)
	.with_name("Development")
	.with_id("westend_dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_patch(westend_development_config_genesis())
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Rococo development config (single validator Alice)
#[cfg(feature = "rococo-native")]
pub fn rococo_development_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Rococo development wasm not available")?,
		Default::default(),
	)
	.with_name("Development")
	.with_id("rococo_dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name("development")
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// `Versi` development config (single validator Alice)
#[cfg(feature = "rococo-native")]
pub fn versi_development_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Versi development wasm not available")?,
		Default::default(),
	)
	.with_name("Development")
	.with_id("versi_dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name("development")
	.with_protocol_id("versi")
	.build())
}

/// Wococo development config (single validator Alice)
#[cfg(feature = "rococo-native")]
pub fn wococo_development_config() -> Result<RococoChainSpec, String> {
	const WOCOCO_DEV_PROTOCOL_ID: &str = "woco";
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Wococo development wasm not available")?,
		Default::default(),
	)
	.with_name("Development")
	.with_id("wococo_dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_preset_name("development")
	.with_protocol_id(WOCOCO_DEV_PROTOCOL_ID)
	.build())
}

#[cfg(feature = "westend-native")]
fn westend_local_testnet_genesis() -> serde_json::Value {
	westend_testnet_genesis(
		vec![get_authority_keys_from_seed("Alice"), get_authority_keys_from_seed("Bob")],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
	)
}

/// Westend local testnet config (multivalidator Alice + Bob)
#[cfg(feature = "westend-native")]
pub fn westend_local_testnet_config() -> Result<WestendChainSpec, String> {
	Ok(WestendChainSpec::builder(
		westend::WASM_BINARY.ok_or("Westend development wasm not available")?,
		Default::default(),
	)
	.with_name("Westend Local Testnet")
	.with_id("westend_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(westend_local_testnet_genesis())
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Rococo local testnet config (multivalidator Alice + Bob)
#[cfg(feature = "rococo-native")]
pub fn rococo_local_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::fast_runtime_binary::WASM_BINARY.ok_or("Rococo development wasm not available")?,
		Default::default(),
	)
	.with_name("Rococo Local Testnet")
	.with_id("rococo_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name("local_testnet")
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Wococo local testnet config (multivalidator Alice + Bob + Charlie + Dave)
#[cfg(feature = "rococo-native")]
pub fn wococo_local_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Rococo development wasm (used for wococo) not available")?,
		Default::default(),
	)
	.with_name("Wococo Local Testnet")
	.with_id("wococo_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name("wococo_local_testnet")
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// `Versi` local testnet config (multivalidator Alice + Bob + Charlie + Dave)
#[cfg(feature = "rococo-native")]
pub fn versi_local_testnet_config() -> Result<RococoChainSpec, String> {
	Ok(RococoChainSpec::builder(
		rococo::WASM_BINARY.ok_or("Rococo development wasm (used for versi) not available")?,
		Default::default(),
	)
	.with_name("Versi Local Testnet")
	.with_id("versi_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name("versi_local_testnet")
	.with_protocol_id("versi")
	.build())
}
