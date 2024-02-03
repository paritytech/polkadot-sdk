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

use beefy_primitives::ecdsa_crypto::AuthorityId as BeefyId;
use grandpa::AuthorityId as GrandpaId;
#[cfg(feature = "westend-native")]
use pallet_staking::Forcing;
use polkadot_primitives::{AccountId, AccountPublic, AssignmentId, ValidatorId};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;

#[cfg(feature = "rococo-native")]
use rococo_runtime as rococo;
#[cfg(feature = "rococo-native")]
use rococo_runtime_constants::currency::UNITS as ROC;
use sc_chain_spec::ChainSpecExtension;
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
use sc_chain_spec::ChainType;
use serde::{Deserialize, Serialize};
use sp_core::{sr25519, Pair, Public};
use sp_runtime::traits::IdentifyAccount;
#[cfg(feature = "westend-native")]
use sp_runtime::Perbill;
#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
use telemetry::TelemetryEndpoints;
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
pub type GenericChainSpec = service::GenericChainSpec<(), Extensions>;

/// The `ChainSpec` parameterized for the westend runtime.
#[cfg(feature = "westend-native")]
pub type WestendChainSpec = service::GenericChainSpec<(), Extensions>;

/// The `ChainSpec` parameterized for the westend runtime.
// Dummy chain spec, but that is fine when we don't have the native runtime.
#[cfg(not(feature = "westend-native"))]
pub type WestendChainSpec = GenericChainSpec;

/// The `ChainSpec` parameterized for the rococo runtime.
#[cfg(feature = "rococo-native")]
pub type RococoChainSpec = service::GenericChainSpec<(), Extensions>;

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
#[cfg(any(feature = "rococo-native", feature = "westend-native",))]
fn default_parachains_host_configuration(
) -> polkadot_runtime_parachains::configuration::HostConfiguration<polkadot_primitives::BlockNumber>
{
	use polkadot_primitives::{AsyncBackingParams, MAX_CODE_SIZE, MAX_POV_SIZE};

	polkadot_runtime_parachains::configuration::HostConfiguration {
		validation_upgrade_cooldown: 2u32,
		validation_upgrade_delay: 2,
		code_retention_period: 1200,
		max_code_size: MAX_CODE_SIZE,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 32 * 1024,
		group_rotation_frequency: 20,
		paras_availability_period: 4,
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
		scheduling_lookahead: 2,
		async_backing_params: AsyncBackingParams {
			max_candidate_depth: 3,
			allowed_ancestry_len: 2,
		},
		..Default::default()
	}
}

#[cfg(any(feature = "rococo-native", feature = "westend-native",))]
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

#[cfg(feature = "rococo-native")]
fn rococo_session_keys(
	babe: BabeId,
	grandpa: GrandpaId,
	para_validator: ValidatorId,
	para_assignment: AssignmentId,
	authority_discovery: AuthorityDiscoveryId,
	beefy: BeefyId,
) -> rococo_runtime::SessionKeys {
	rococo_runtime::SessionKeys {
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

#[cfg(feature = "rococo-native")]
fn rococo_staging_testnet_config_genesis() -> serde_json::Value {
	use hex_literal::hex;
	use sp_core::crypto::UncheckedInto;

	// subkey inspect "$SECRET"
	let endowed_accounts = vec![
		// 5DwBmEFPXRESyEam5SsQF1zbWSCn2kCjyLW51hJHXe9vW4xs
		hex!["52bc71c1eca5353749542dfdf0af97bf764f9c2f44e860cd485f1cd86400f649"].into(),
	];

	// ./scripts/prepare-test-net.sh 8
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
			//5EHZkbp22djdbuMFH9qt1DVzSCvqi3zWpj6DAYfANa828oei
			hex!["62475fe5406a7cb6a64c51d0af9d3ab5c2151bcae982fb812f7a76b706914d6a"].into(),
			//5FeSEpi9UYYaWwXXb3tV88qtZkmSdB3mvgj3pXkxKyYLGhcd
			hex!["9e6e781a76810fe93187af44c79272c290c2b9e2b8b92ee11466cd79d8023f50"].into(),
			//5Fh6rDpMDhM363o1Z3Y9twtaCPfizGQWCi55BSykTQjGbP7H
			hex!["a076ef1280d768051f21d060623da3ab5b56944d681d303ed2d4bf658c5bed35"]
				.unchecked_into(),
			//5CPd3zoV9Aaah4xWucuDivMHJ2nEEmpdi864nPTiyRZp4t87
			hex!["0e6d7d1afbcc6547b92995a394ba0daed07a2420be08220a5a1336c6731f0bfa"]
				.unchecked_into(),
			//5CP6oGfwqbEfML8efqm1tCZsUgRsJztp9L8ZkEUxA16W8PPz
			hex!["0e07a51d3213842f8e9363ce8e444255990a225f87e80a3d651db7841e1a0205"]
				.unchecked_into(),
			//5HQdwiDh8Qtd5dSNWajNYpwDvoyNWWA16Y43aEkCNactFc2b
			hex!["ec60e71fe4a567ef9fef99d4bbf37ffae70564b41aa6f94ef0317c13e0a5477b"]
				.unchecked_into(),
			//5HbSgM72xVuscsopsdeG3sCSCYdAeM1Tay9p79N6ky6vwDGq
			hex!["f49eae66a0ac9f610316906ec8f1a0928e20d7059d76a5ca53cbcb5a9b50dd3c"]
				.unchecked_into(),
			//5DPSWdgw38Spu315r6LSvYCggeeieBAJtP5A1qzuzKhqmjVu
			hex!["034f68c5661a41930c82f26a662276bf89f33467e1c850f2fb8ef687fe43d62276"]
				.unchecked_into(),
		),
		(
			//5DvH8oEjQPYhzCoQVo7WDU91qmQfLZvxe9wJcrojmJKebCmG
			hex!["520b48452969f6ddf263b664de0adb0c729d0e0ad3b0e5f3cb636c541bc9022a"].into(),
			//5ENZvCRzyXJJYup8bM6yEzb2kQHEb1NDpY2ZEyVGBkCfRdj3
			hex!["6618289af7ae8621981ffab34591e7a6486e12745dfa3fd3b0f7e6a3994c7b5b"].into(),
			//5DLjSUfqZVNAADbwYLgRvHvdzXypiV1DAEaDMjcESKTcqMoM
			hex!["38757d0de00a0c739e7d7984ef4bc01161bd61e198b7c01b618425c16bb5bd5f"]
				.unchecked_into(),
			//5HnDVBN9mD6mXyx8oryhDbJtezwNSj1VRXgLoYCBA6uEkiao
			hex!["fcd5f87a6fd5707a25122a01b4dac0a8482259df7d42a9a096606df1320df08d"]
				.unchecked_into(),
			//5EPEWRecy2ApL5n18n3aHyU1956zXTRqaJpzDa9DoqiggNwF
			hex!["669a10892119453e9feb4e3f1ee8e028916cc3240022920ad643846fbdbee816"]
				.unchecked_into(),
			//5ES3fw5X4bndSgLNmtPfSbM2J1kLqApVB2CCLS4CBpM1UxUZ
			hex!["68bf52c482630a8d1511f2edd14f34127a7d7082219cccf7fd4c6ecdb535f80d"]
				.unchecked_into(),
			//5HeXbwb5PxtcRoopPZTp5CQun38atn2UudQ8p2AxR5BzoaXw
			hex!["f6f8fe475130d21165446a02fb1dbce3a7bf36412e5d98f4f0473aed9252f349"]
				.unchecked_into(),
			//5F7nTtN8MyJV4UsXpjg7tHSnfANXZ5KRPJmkASc1ZSH2Xoa5
			hex!["03a90c2bb6d3b7000020f6152fe2e5002fa970fd1f42aafb6c8edda8dacc2ea77e"]
				.unchecked_into(),
		),
		(
			//5FPMzsezo1PRxYbVpJMWK7HNbR2kUxidsAAxH4BosHa4wd6S
			hex!["92ef83665b39d7a565e11bf8d18d41d45a8011601c339e57a8ea88c8ff7bba6f"].into(),
			//5G6NQidFG7YiXsvV7hQTLGArir9tsYqD4JDxByhgxKvSKwRx
			hex!["b235f57244230589523271c27b8a490922ffd7dccc83b044feaf22273c1dc735"].into(),
			//5GpZhzAVg7SAtzLvaAC777pjquPEcNy1FbNUAG2nZvhmd6eY
			hex!["d2644c1ab2c63a3ad8d40ad70d4b260969e3abfe6d7e6665f50dc9f6365c9d2a"]
				.unchecked_into(),
			//5HAes2RQYPbYKbLBfKb88f4zoXv6pPA6Ke8CjN7dob3GpmSP
			hex!["e1b68fbd84333e31486c08e6153d9a1415b2e7e71b413702b7d64e9b631184a1"]
				.unchecked_into(),
			//5FtAGDZYJKXkhVhAxCQrXmaP7EE2mGbBMfmKDHjfYDgq2BiU
			hex!["a8e61ffacafaf546283dc92d14d7cc70ea0151a5dd81fdf73ff5a2951f2b6037"]
				.unchecked_into(),
			//5CtK7JHv3h6UQZ44y54skxdwSVBRtuxwPE1FYm7UZVhg8rJV
			hex!["244f3421b310c68646e99cdbf4963e02067601f57756b072a4b19431448c186e"]
				.unchecked_into(),
			//5D4r6YaB6F7A7nvMRHNFNF6zrR9g39bqDJFenrcaFmTCRwfa
			hex!["2c57f81fd311c1ab53813c6817fe67f8947f8d39258252663b3384ab4195494d"]
				.unchecked_into(),
			//5EPoHj8uV4fFKQHYThc6Z9fDkU7B6ih2ncVzQuDdNFb8UyhF
			hex!["039d065fe4f9234f0a4f13cc3ae585f2691e9c25afa469618abb6645111f607a53"]
				.unchecked_into(),
		),
		(
			//5DMNx7RoX6d7JQ38NEM7DWRcW2THu92LBYZEWvBRhJeqcWgR
			hex!["38f3c2f38f6d47f161e98c697bbe3ca0e47c033460afda0dda314ab4222a0404"].into(),
			//5GGdKNDr9P47dpVnmtq3m8Tvowwf1ot1abw6tPsTYYFoKm2v
			hex!["ba0898c1964196474c0be08d364cdf4e9e1d47088287f5235f70b0590dfe1704"].into(),
			//5EjkyPCzR2SjhDZq8f7ufsw6TfkvgNRepjCRQFc4TcdXdaB1
			hex!["764186bc30fd5a02477f19948dc723d6d57ab174debd4f80ed6038ec960bfe21"]
				.unchecked_into(),
			//5DJV3zCBTJBLGNDCcdWrYxWDacSz84goGTa4pFeKVvehEBte
			hex!["36be9069cdb4a8a07ecd51f257875150f0a8a1be44a10d9d98dabf10a030aef4"]
				.unchecked_into(),
			//5F9FsRjpecP9GonktmtFL3kjqNAMKjHVFjyjRdTPa4hbQRZA
			hex!["882d72965e642677583b333b2d173ac94b5fd6c405c76184bb14293be748a13b"]
				.unchecked_into(),
			//5F1FZWZSj3JyTLs8sRBxU6QWyGLSL9BMRtmSKDmVEoiKFxSP
			hex!["821271c99c958b9220f1771d9f5e29af969edfa865631dba31e1ab7bc0582b75"]
				.unchecked_into(),
			//5CtgRR74VypK4h154s369abs78hDUxZSJqcbWsfXvsjcHJNA
			hex!["2496f28d887d84705c6dae98aee8bf90fc5ad10bb5545eca1de6b68425b70f7c"]
				.unchecked_into(),
			//5CPx6dsr11SCJHKFkcAQ9jpparS7FwXQBrrMznRo4Hqv1PXz
			hex!["0307d29bbf6a5c4061c2157b44fda33b7bb4ec52a5a0305668c74688cedf288d58"]
				.unchecked_into(),
		),
		(
			//5C8AL1Zb4bVazgT3EgDxFgcow1L4SJjVu44XcLC9CrYqFN4N
			hex!["02a2d8cfcf75dda85fafc04ace3bcb73160034ed1964c43098fb1fe831de1b16"].into(),
			//5FLYy3YKsAnooqE4hCudttAsoGKbVG3hYYBtVzwMjJQrevPa
			hex!["90cab33f0bb501727faa8319f0845faef7d31008f178b65054b6629fe531b772"].into(),
			//5Et3tfbVf1ByFThNAuUq5pBssdaPPskip5yob5GNyUFojXC7
			hex!["7c94715e5dd8ab54221b1b6b2bfa5666f593f28a92a18e28052531de1bd80813"]
				.unchecked_into(),
			//5EX1JBghGbQqWohTPU6msR9qZ2nYPhK9r3RTQ2oD1K8TCxaG
			hex!["6c878e33b83c20324238d22240f735457b6fba544b383e70bb62a27b57380c81"]
				.unchecked_into(),
			//5EUNaBpX9mJgcmLQHyG5Pkms6tbDiKuLbeTEJS924Js9cA1N
			hex!["6a8570b9c6408e54bacf123cc2bb1b0f087f9c149147d0005badba63a5a4ac01"]
				.unchecked_into(),
			//5CaZuueRVpMATZG4hkcrgDoF4WGixuz7zu83jeBdY3bgWGaG
			hex!["16c69ea8d595e80b6736f44be1eaeeef2ac9c04a803cc4fd944364cb0d617a33"]
				.unchecked_into(),
			//5DABsdQCDUGuhzVGWe5xXzYQ9rtrVxRygW7RXf9Tsjsw1aGJ
			hex!["306ac5c772fe858942f92b6e28bd82fb7dd8cdd25f9a4626c1b0eee075fcb531"]
				.unchecked_into(),
			//5H91T5mHhoCw9JJG4NjghDdQyhC6L7XcSuBWKD3q3TAhEVvQ
			hex!["02fb0330356e63a35dd930bc74525edf28b3bf5eb44aab9e9e4962c8309aaba6a6"]
				.unchecked_into(),
		),
		(
			//5C8XbDXdMNKJrZSrQURwVCxdNdk8AzG6xgLggbzuA399bBBF
			hex!["02ea6bfa8b23b92fe4b5db1063a1f9475e3acd0ab61e6b4f454ed6ba00b5f864"].into(),
			//5GsyzFP8qtF8tXPSsjhjxAeU1v7D1PZofuQKN9TdCc7Dp1JM
			hex!["d4ffc4c05b47d1115ad200f7f86e307b20b46c50e1b72a912ec4f6f7db46b616"].into(),
			//5GHWB8ZDzegLcMW7Gdd1BS6WHVwDdStfkkE4G7KjPjZNJBtD
			hex!["bab3cccdcc34401e9b3971b96a662686cf755aa869a5c4b762199ce531b12c5b"]
				.unchecked_into(),
			//5GzDPGbUM9uH52ZEwydasTj8edokGUJ7vEpoFWp9FE1YNuFB
			hex!["d9c056c98ca0e6b4eb7f5c58c007c1db7be0fe1f3776108f797dd4990d1ccc33"]
				.unchecked_into(),
			//5CmLCFeSurRXXtwMmLcVo7sdJ9EqDguvJbuCYDcHkr3cpqyE
			hex!["1efc23c0b51ad609ab670ecf45807e31acbd8e7e5cb7c07cf49ee42992d2867c"]
				.unchecked_into(),
			//5DnsSy8a8pfE2aFjKBDtKw7WM1V4nfE5sLzP15MNTka53GqS
			hex!["4c64d3f06d28adeb36a892fdaccecace150bec891f04694448a60b74fa469c22"]
				.unchecked_into(),
			//5CZdFnyzZvKetZTeUwj5APAYskVJe4QFiTezo5dQNsrnehGd
			hex!["160ea09c5717270e958a3da42673fa011613a9539b2e4ebcad8626bc117ca04a"]
				.unchecked_into(),
			//5HgoR9JJkdBusxKrrs3zgd3ToppgNoGj1rDyAJp4e7eZiYyT
			hex!["020019a8bb188f8145d02fa855e9c36e9914457d37c500e03634b5223aa5702474"]
				.unchecked_into(),
		),
		(
			//5HinEonzr8MywkqedcpsmwpxKje2jqr9miEwuzyFXEBCvVXM
			hex!["fa373e25a1c4fe19c7148acde13bc3db1811cf656dc086820f3dda736b9c4a00"].into(),
			//5EHJbj6Td6ks5HDnyfN4ttTSi57osxcQsQexm7XpazdeqtV7
			hex!["62145d721967bd88622d08625f0f5681463c0f1b8bcd97eb3c2c53f7660fd513"].into(),
			//5EeCsC58XgJ1DFaoYA1WktEpP27jvwGpKdxPMFjicpLeYu96
			hex!["720537e2c1c554654d73b3889c3ef4c3c2f95a65dd3f7c185ebe4afebed78372"]
				.unchecked_into(),
			//5DnEySxbnppWEyN8cCLqvGjAorGdLRg2VmkY96dbJ1LHFK8N
			hex!["4bea0b37e0cce9bddd80835fa2bfd5606f5dcfb8388bbb10b10c483f0856cf14"]
				.unchecked_into(),
			//5CAC278tFCHAeHYqE51FTWYxHmeLcENSS1RG77EFRTvPZMJT
			hex!["042f07fc5268f13c026bbe199d63e6ac77a0c2a780f71cda05cee5a6f1b3f11f"]
				.unchecked_into(),
			//5HjRTLWcQjZzN3JDvaj1UzjNSayg5ZD9ZGWMstaL7Ab2jjAa
			hex!["fab485e87ed1537d089df521edf983a777c57065a702d7ed2b6a2926f31da74f"]
				.unchecked_into(),
			//5ELv74v7QcsS6FdzvG4vL2NnYDGWmRnJUSMKYwdyJD7Xcdi7
			hex!["64d59feddb3d00316a55906953fb3db8985797472bd2e6c7ea1ab730cc339d7f"]
				.unchecked_into(),
			//5FaUcPt4fPz93vBhcrCJqmDkjYZ7jCbzAF56QJoCmvPaKrmx
			hex!["033f1a6d47fe86f88934e4b83b9fae903b92b5dcf4fec97d5e3e8bf4f39df03685"]
				.unchecked_into(),
		),
		(
			//5Ey3NQ3dfabaDc16NUv7wRLsFCMDFJSqZFzKVycAsWuUC6Di
			hex!["8062e9c21f1d92926103119f7e8153cebdb1e5ab3e52d6f395be80bb193eab47"].into(),
			//5HiWsuSBqt8nS9pnggexXuHageUifVPKPHDE2arTKqhTp1dV
			hex!["fa0388fa88f3f0cb43d583e2571fbc0edad57dff3a6fd89775451dd2c2b8ea00"].into(),
			//5H168nKX2Yrfo3bxj7rkcg25326Uv3CCCnKUGK6uHdKMdPt8
			hex!["da6b2df18f0f9001a6dcf1d301b92534fe9b1f3ccfa10c49449fee93adaa8349"]
				.unchecked_into(),
			//5DrA2fZdzmNqT5j6DXNwVxPBjDV9jhkAqvjt6Us3bQHKy3cF
			hex!["4ee66173993dd0db5d628c4c9cb61a27b76611ad3c3925947f0d0011ee2c5dcc"]
				.unchecked_into(),
			//5Gx6YeNhynqn8qkda9QKpc9S7oDr4sBrfAu516d3sPpEt26F
			hex!["d822d4088b20dca29a580a577a97d6f024bb24c9550bebdfd7d2d18e946a1c7d"]
				.unchecked_into(),
			//5DhDcHqwxoes5s89AyudGMjtZXx1nEgrk5P45X88oSTR3iyx
			hex!["481538f8c2c011a76d7d57db11c2789a5e83b0f9680dc6d26211d2f9c021ae4c"]
				.unchecked_into(),
			//5DqAvikdpfRdk5rR35ZobZhqaC5bJXZcEuvzGtexAZP1hU3T
			hex!["4e262811acdfe94528bfc3c65036080426a0e1301b9ada8d687a70ffcae99c26"]
				.unchecked_into(),
			//5E41Znrr2YtZu8bZp3nvRuLVHg3jFksfQ3tXuviLku4wsao7
			hex!["025e84e95ed043e387ddb8668176b42f8e2773ddd84f7f58a6d9bf436a4b527986"]
				.unchecked_into(),
		),
	];

	const ENDOWMENT: u128 = 1_000_000 * ROC;
	const STASH: u128 = 100 * ROC;

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
						rococo_session_keys(
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
		"babe": {
			"epochConfig": Some(rococo_runtime::BABE_GENESIS_EPOCH_CONFIG),
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
	.with_genesis_config_patch(rococo_staging_testnet_config_genesis())
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
	.with_genesis_config_patch(rococo_staging_testnet_config_genesis())
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

#[cfg(any(feature = "westend-native", feature = "rococo-native"))]
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

/// Helper function to create rococo runtime `GenesisConfig` patch for testing
#[cfg(feature = "rococo-native")]
pub fn rococo_testnet_genesis(
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

	const ENDOWMENT: u128 = 1_000_000 * ROC;

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
						rococo_session_keys(
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
		"babe": {
			"epochConfig": Some(rococo_runtime::BABE_GENESIS_EPOCH_CONFIG),
		},
		"sudo": { "key": Some(root_key.clone()) },
		"configuration": {
			"config": polkadot_runtime_parachains::configuration::HostConfiguration {
				max_validators_per_core: Some(1),
				..default_parachains_host_configuration()
			},
		},
		"registrar": {
			"nextFreeParaId": polkadot_primitives::LOWEST_PUBLIC_ID,
		}
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

#[cfg(feature = "rococo-native")]
fn rococo_development_config_genesis() -> serde_json::Value {
	rococo_testnet_genesis(
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
	.with_genesis_config_patch(rococo_development_config_genesis())
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
	.with_genesis_config_patch(rococo_development_config_genesis())
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
	.with_genesis_config_patch(rococo_development_config_genesis())
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

#[cfg(feature = "rococo-native")]
fn rococo_local_testnet_genesis() -> serde_json::Value {
	rococo_testnet_genesis(
		vec![get_authority_keys_from_seed("Alice"), get_authority_keys_from_seed("Bob")],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
	)
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
	.with_genesis_config_patch(rococo_local_testnet_genesis())
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// Wococo is a temporary testnet that uses almost the same runtime as rococo.
#[cfg(feature = "rococo-native")]
fn wococo_local_testnet_genesis() -> serde_json::Value {
	rococo_testnet_genesis(
		vec![
			get_authority_keys_from_seed("Alice"),
			get_authority_keys_from_seed("Bob"),
			get_authority_keys_from_seed("Charlie"),
			get_authority_keys_from_seed("Dave"),
		],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
	)
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
	.with_genesis_config_patch(wococo_local_testnet_genesis())
	.with_protocol_id(DEFAULT_PROTOCOL_ID)
	.build())
}

/// `Versi` is a temporary testnet that uses the same runtime as rococo.
#[cfg(feature = "rococo-native")]
fn versi_local_testnet_genesis() -> serde_json::Value {
	rococo_testnet_genesis(
		vec![
			get_authority_keys_from_seed("Alice"),
			get_authority_keys_from_seed("Bob"),
			get_authority_keys_from_seed("Charlie"),
			get_authority_keys_from_seed("Dave"),
		],
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		None,
	)
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
	.with_genesis_config_patch(versi_local_testnet_genesis())
	.with_protocol_id("versi")
	.build())
}
