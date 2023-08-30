// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use beefy_primitives::ecdsa_crypto::AuthorityId as BeefyId;
use grandpa::AuthorityId as GrandpaId;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use parachains_common::{AccountId, AssetHubPolkadotAuraId, AuraId, Balance, BlockNumber};
use polkadot_parachain::primitives::{HeadData, ValidationCode};
use polkadot_primitives::{AssignmentId, ValidatorId};
use polkadot_runtime_parachains::{
	configuration::HostConfiguration,
	paras::{ParaGenesisArgs, ParaKind},
};
use polkadot_service::chain_spec::get_authority_keys_from_seed_no_beefy;
use sc_chain_spec::GenesisConfigBuilderRuntimeCaller;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::{sr25519, storage::Storage, Pair, Public};
#[cfg(test)]
use sp_runtime::BuildStorage;
use sp_runtime::{
	traits::{IdentifyAccount, Verify},
	MultiSignature, Perbill,
};
use xcm;

pub const XCM_V2: u32 = 3;
pub const XCM_V3: u32 = 2;
pub const REF_TIME_THRESHOLD: u64 = 33;
pub const PROOF_SIZE_THRESHOLD: u64 = 33;

type AccountPublic = <MultiSignature as Verify>::Signer;

/// Helper function to generate a crypto pair from seed
fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

/// Helper function to generate an account ID from seed.
fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Helper function to build the genesis storage using given json patch and code
fn build_genesis_storage(patch: serde_json::Value, code: &[u8]) -> Storage {
	let mut storage = GenesisConfigBuilderRuntimeCaller::new(code)
		.get_storage_for_patch(patch)
		.unwrap();
	storage
		.top
		.insert(sp_core::storage::well_known_keys::CODE.to_vec(), code.into());
	storage
}

#[cfg(test)]
/// Helper function used in tests to build the genesis storage using given RuntimeGenesisConfig and
/// code Used in `legacy_vs_json_check` submods to verify storage building with JSON patch against
/// building with RuntimeGenesisConfig struct.
fn build_genesis_storage_legacy(builder: &dyn BuildStorage, code: &[u8]) -> Storage {
	let mut storage = builder.build_storage().unwrap();
	storage
		.top
		.insert(sp_core::storage::well_known_keys::CODE.to_vec(), code.into());
	storage
}

pub mod accounts {
	use super::*;
	pub const ALICE: &str = "Alice";
	pub const BOB: &str = "Bob";
	pub const CHARLIE: &str = "Charlie";
	pub const DAVE: &str = "Dave";
	pub const EVE: &str = "Eve";
	pub const FERDIE: &str = "Ferdei";
	pub const ALICE_STASH: &str = "Alice//stash";
	pub const BOB_STASH: &str = "Bob//stash";
	pub const CHARLIE_STASH: &str = "Charlie//stash";
	pub const DAVE_STASH: &str = "Dave//stash";
	pub const EVE_STASH: &str = "Eve//stash";
	pub const FERDIE_STASH: &str = "Ferdie//stash";
	pub const FERDIE_BEEFY: &str = "Ferdie//stash";

	pub fn init_balances() -> Vec<AccountId> {
		vec![
			get_account_id_from_seed::<sr25519::Public>(ALICE),
			get_account_id_from_seed::<sr25519::Public>(BOB),
			get_account_id_from_seed::<sr25519::Public>(CHARLIE),
			get_account_id_from_seed::<sr25519::Public>(DAVE),
			get_account_id_from_seed::<sr25519::Public>(EVE),
			get_account_id_from_seed::<sr25519::Public>(FERDIE),
			get_account_id_from_seed::<sr25519::Public>(ALICE_STASH),
			get_account_id_from_seed::<sr25519::Public>(BOB_STASH),
			get_account_id_from_seed::<sr25519::Public>(CHARLIE_STASH),
			get_account_id_from_seed::<sr25519::Public>(DAVE_STASH),
			get_account_id_from_seed::<sr25519::Public>(EVE_STASH),
			get_account_id_from_seed::<sr25519::Public>(FERDIE_STASH),
		]
	}
}

pub mod collators {
	use super::*;

	pub fn invulnerables_asset_hub_polkadot() -> Vec<(AccountId, AssetHubPolkadotAuraId)> {
		vec![
			(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_from_seed::<AssetHubPolkadotAuraId>("Alice"),
			),
			(
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_from_seed::<AssetHubPolkadotAuraId>("Bob"),
			),
		]
	}

	pub fn invulnerables() -> Vec<(AccountId, AuraId)> {
		vec![
			(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_from_seed::<AuraId>("Alice"),
			),
			(get_account_id_from_seed::<sr25519::Public>("Bob"), get_from_seed::<AuraId>("Bob")),
		]
	}
}

pub mod validators {
	use super::*;

	pub fn initial_authorities() -> Vec<(
		AccountId,
		AccountId,
		BabeId,
		GrandpaId,
		ImOnlineId,
		ValidatorId,
		AssignmentId,
		AuthorityDiscoveryId,
	)> {
		vec![get_authority_keys_from_seed_no_beefy("Alice")]
	}
}

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;
// Polkadot
pub mod polkadot {
	use super::*;
	pub const ED: Balance = polkadot_runtime_constants::currency::EXISTENTIAL_DEPOSIT;
	const STASH: u128 = 100 * polkadot_runtime_constants::currency::UNITS;

	pub fn get_host_config() -> HostConfiguration<BlockNumber> {
		HostConfiguration {
			max_upward_queue_count: 10,
			max_upward_queue_size: 51200,
			max_upward_message_size: 51200,
			max_upward_message_num_per_candidate: 10,
			max_downward_message_size: 51200,
			hrmp_sender_deposit: 100_000_000_000,
			hrmp_recipient_deposit: 100_000_000_000,
			hrmp_channel_max_capacity: 1000,
			hrmp_channel_max_message_size: 102400,
			hrmp_channel_max_total_size: 102400,
			hrmp_max_parachain_outbound_channels: 30,
			hrmp_max_parachain_inbound_channels: 30,
			..Default::default()
		}
	}

	fn session_keys(
		babe: BabeId,
		grandpa: GrandpaId,
		im_online: ImOnlineId,
		para_validator: ValidatorId,
		para_assignment: AssignmentId,
		authority_discovery: AuthorityDiscoveryId,
	) -> polkadot_runtime::SessionKeys {
		polkadot_runtime::SessionKeys {
			babe,
			grandpa,
			im_online,
			para_validator,
			para_assignment,
			authority_discovery,
		}
	}

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"session": {
				"keys": validators::initial_authorities()
					.iter()
					.map(|x| {
						(
							x.0.clone(),
							x.0.clone(),
							polkadot::session_keys(
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
				"validatorCount": validators::initial_authorities().len() as u32,
				"minimumValidatorCount": 1,
				"stakers": validators::initial_authorities()
					.iter()
					.map(|x| {
						(x.0.clone(), x.1.clone(), STASH, polkadot_runtime::StakerStatus::<AccountId>::Validator)
					})
					.collect::<Vec<_>>(),
				"invulnerables": validators::initial_authorities()
					.iter()
					.map(|x| x.0.clone())
					.collect::<Vec<_>>(),
				"forceEra": pallet_staking::Forcing::ForceNone,
				"slashRewardFraction": Perbill::from_percent(10),
			},
			"babe": {
				"epochConfig": Some(polkadot_runtime::BABE_GENESIS_EPOCH_CONFIG),
			},
			"configuration": { "config": get_host_config() },
			"paras": {
				"paras": vec![
					(
						cumulus_primitives_core::ParaId::from(asset_hub_polkadot::PARA_ID),
						ParaGenesisArgs {
							genesis_head: HeadData::default(),
							validation_code: ValidationCode(
								asset_hub_polkadot_runtime::WASM_BINARY.unwrap().to_vec(),
							),
							para_kind: ParaKind::Parachain,
						},
					),
					(
						cumulus_primitives_core::ParaId::from(penpal::PARA_ID_A),
						ParaGenesisArgs {
							genesis_head: HeadData::default(),
							validation_code: ValidationCode(
								penpal_runtime::WASM_BINARY.unwrap().to_vec(),
							),
							para_kind: ParaKind::Parachain,
						},
					),
					(
						cumulus_primitives_core::ParaId::from(penpal::PARA_ID_B),
						ParaGenesisArgs {
							genesis_head: HeadData::default(),
							validation_code: ValidationCode(
								penpal_runtime::WASM_BINARY.unwrap().to_vec(),
							),
							para_kind: ParaKind::Parachain,
						},
					),
				],
			},
		});

		build_genesis_storage(genesis_config, polkadot_runtime::WASM_BINARY.unwrap())
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = polkadot_runtime::RuntimeGenesisConfig {
				system: polkadot_runtime::SystemConfig::default(),
				balances: polkadot_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				session: polkadot_runtime::SessionConfig {
					keys: validators::initial_authorities()
						.iter()
						.map(|x| {
							(
								x.0.clone(),
								x.0.clone(),
								polkadot::session_keys(
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
				staking: polkadot_runtime::StakingConfig {
					validator_count: validators::initial_authorities().len() as u32,
					minimum_validator_count: 1,
					stakers: validators::initial_authorities()
						.iter()
						.map(|x| {
							(
								x.0.clone(),
								x.1.clone(),
								STASH,
								polkadot_runtime::StakerStatus::Validator,
							)
						})
						.collect(),
					invulnerables: validators::initial_authorities()
						.iter()
						.map(|x| x.0.clone())
						.collect(),
					force_era: pallet_staking::Forcing::ForceNone,
					slash_reward_fraction: Perbill::from_percent(10),
					..Default::default()
				},
				babe: polkadot_runtime::BabeConfig {
					authorities: Default::default(),
					epoch_config: Some(polkadot_runtime::BABE_GENESIS_EPOCH_CONFIG),
					..Default::default()
				},
				configuration: polkadot_runtime::ConfigurationConfig { config: get_host_config() },
				paras: polkadot_runtime::ParasConfig {
					paras: vec![
						(
							asset_hub_polkadot::PARA_ID.into(),
							ParaGenesisArgs {
								genesis_head: HeadData::default(),
								validation_code: ValidationCode(
									asset_hub_polkadot_runtime::WASM_BINARY.unwrap().to_vec(),
								),
								para_kind: ParaKind::Parachain,
							},
						),
						(
							penpal::PARA_ID_A.into(),
							ParaGenesisArgs {
								genesis_head: HeadData::default(),
								validation_code: ValidationCode(
									penpal_runtime::WASM_BINARY.unwrap().to_vec(),
								),
								para_kind: ParaKind::Parachain,
							},
						),
						(
							penpal::PARA_ID_B.into(),
							ParaGenesisArgs {
								genesis_head: HeadData::default(),
								validation_code: ValidationCode(
									penpal_runtime::WASM_BINARY.unwrap().to_vec(),
								),
								para_kind: ParaKind::Parachain,
							},
						),
					],
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(&genesis_config, polkadot_runtime::WASM_BINARY.unwrap())
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Westend
pub mod westend {
	use super::*;
	use westend_runtime_constants::currency::UNITS as WND;
	pub const ED: Balance = westend_runtime_constants::currency::EXISTENTIAL_DEPOSIT;
	const ENDOWMENT: u128 = 1_000_000 * WND;
	const STASH: u128 = 100 * WND;

	pub fn get_host_config() -> HostConfiguration<BlockNumber> {
		HostConfiguration {
			max_upward_queue_count: 10,
			max_upward_queue_size: 51200,
			max_upward_message_size: 51200,
			max_upward_message_num_per_candidate: 10,
			max_downward_message_size: 51200,
			hrmp_sender_deposit: 100_000_000_000,
			hrmp_recipient_deposit: 100_000_000_000,
			hrmp_channel_max_capacity: 1000,
			hrmp_channel_max_message_size: 102400,
			hrmp_channel_max_total_size: 102400,
			hrmp_max_parachain_outbound_channels: 30,
			hrmp_max_parachain_inbound_channels: 30,
			..Default::default()
		}
	}

	fn session_keys(
		babe: BabeId,
		grandpa: GrandpaId,
		im_online: ImOnlineId,
		para_validator: ValidatorId,
		para_assignment: AssignmentId,
		authority_discovery: AuthorityDiscoveryId,
		beefy: BeefyId,
	) -> westend_runtime::SessionKeys {
		westend_runtime::SessionKeys {
			babe,
			grandpa,
			im_online,
			para_validator,
			para_assignment,
			authority_discovery,
			beefy,
		}
	}

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ENDOWMENT))
					.collect::<Vec<_>>(),
			},
			"session": {
				"keys": validators::initial_authorities()
					.iter()
					.map(|x| {
						(
							x.0.clone(),
							x.0.clone(),
							westend::session_keys(
								x.2.clone(),
								x.3.clone(),
								x.4.clone(),
								x.5.clone(),
								x.6.clone(),
								x.7.clone(),
								get_from_seed::<BeefyId>("Alice"),
							),
						)
					})
					.collect::<Vec<_>>(),
			},
			"staking": {
				"validatorCount": validators::initial_authorities().len() as u32,
				"minimumValidatorCount": 1,
				"stakers": validators::initial_authorities()
					.iter()
					.map(|x| {
						(x.0.clone(), x.1.clone(), STASH, westend_runtime::StakerStatus::<AccountId>::Validator)
					})
					.collect::<Vec<_>>(),
				"invulnerables": validators::initial_authorities()
					.iter()
					.map(|x| x.0.clone())
					.collect::<Vec<_>>(),
				"forceEra": pallet_staking::Forcing::ForceNone,
				"slashRewardFraction": Perbill::from_percent(10),
			},
			"babe": {
				"epochConfig": Some(westend_runtime::BABE_GENESIS_EPOCH_CONFIG),
			},
			"configuration": { "config": get_host_config() },
		});

		build_genesis_storage(genesis_config, westend_runtime::WASM_BINARY.unwrap())
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = westend_runtime::RuntimeGenesisConfig {
				system: westend_runtime::SystemConfig::default(),
				balances: westend_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ENDOWMENT))
						.collect(),
				},
				session: westend_runtime::SessionConfig {
					keys: validators::initial_authorities()
						.iter()
						.map(|x| {
							(
								x.0.clone(),
								x.0.clone(),
								westend::session_keys(
									x.2.clone(),
									x.3.clone(),
									x.4.clone(),
									x.5.clone(),
									x.6.clone(),
									x.7.clone(),
									get_from_seed::<BeefyId>("Alice"),
								),
							)
						})
						.collect::<Vec<_>>(),
				},
				staking: westend_runtime::StakingConfig {
					validator_count: validators::initial_authorities().len() as u32,
					minimum_validator_count: 1,
					stakers: validators::initial_authorities()
						.iter()
						.map(|x| {
							(
								x.0.clone(),
								x.1.clone(),
								STASH,
								westend_runtime::StakerStatus::Validator,
							)
						})
						.collect(),
					invulnerables: validators::initial_authorities()
						.iter()
						.map(|x| x.0.clone())
						.collect(),
					force_era: pallet_staking::Forcing::ForceNone,
					slash_reward_fraction: Perbill::from_percent(10),
					..Default::default()
				},
				babe: westend_runtime::BabeConfig {
					authorities: Default::default(),
					epoch_config: Some(westend_runtime::BABE_GENESIS_EPOCH_CONFIG),
					..Default::default()
				},
				configuration: westend_runtime::ConfigurationConfig { config: get_host_config() },
				..Default::default()
			};

			build_genesis_storage_legacy(&genesis_config, westend_runtime::WASM_BINARY.unwrap())
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Kusama
pub mod kusama {
	use super::*;
	pub const ED: Balance = kusama_runtime_constants::currency::EXISTENTIAL_DEPOSIT;
	use kusama_runtime_constants::currency::UNITS as KSM;
	const ENDOWMENT: u128 = 1_000_000 * KSM;
	const STASH: u128 = 100 * KSM;

	pub fn get_host_config() -> HostConfiguration<BlockNumber> {
		HostConfiguration {
			max_upward_queue_count: 10,
			max_upward_queue_size: 51200,
			max_upward_message_size: 51200,
			max_upward_message_num_per_candidate: 10,
			max_downward_message_size: 51200,
			hrmp_sender_deposit: 5_000_000_000_000,
			hrmp_recipient_deposit: 5_000_000_000_000,
			hrmp_channel_max_capacity: 1000,
			hrmp_channel_max_message_size: 102400,
			hrmp_channel_max_total_size: 102400,
			hrmp_max_parachain_outbound_channels: 30,
			hrmp_max_parachain_inbound_channels: 30,
			..Default::default()
		}
	}

	fn session_keys(
		babe: BabeId,
		grandpa: GrandpaId,
		im_online: ImOnlineId,
		para_validator: ValidatorId,
		para_assignment: AssignmentId,
		authority_discovery: AuthorityDiscoveryId,
		beefy: BeefyId,
	) -> kusama_runtime::SessionKeys {
		kusama_runtime::SessionKeys {
			babe,
			grandpa,
			im_online,
			para_validator,
			para_assignment,
			authority_discovery,
			beefy,
		}
	}

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.map(|k: &AccountId| (k.clone(), ENDOWMENT))
					.collect::<Vec<_>>(),
			},
			"session": {
				"keys": validators::initial_authorities()
					.iter()
					.map(|x| {
						(
							x.0.clone(),
							x.0.clone(),
							kusama::session_keys(
								x.2.clone(),
								x.3.clone(),
								x.4.clone(),
								x.5.clone(),
								x.6.clone(),
								x.7.clone(),
								get_from_seed::<BeefyId>("Alice"),
							),
						)
					})
					.collect::<Vec<_>>(),
			},
			"staking": {
				"validatorCount": validators::initial_authorities().len() as u32,
				"minimumValidatorCount": 1,
				"stakers": validators::initial_authorities()
					.iter()
					.map(|x| {
						(
							x.0.clone(),
							x.1.clone(),
							STASH,
							kusama_runtime::StakerStatus::<AccountId>::Validator,
						)
					})
					.collect::<Vec<_>>(),
				"invulnerables": validators::initial_authorities()
					.iter()
					.map(|x| x.0.clone())
					.collect::<Vec<_>>(),
				"forceEra": pallet_staking::Forcing::NotForcing,
				"slashRewardFraction": Perbill::from_percent(10),
			},
			"babe": {
				"epochConfig": Some(kusama_runtime::BABE_GENESIS_EPOCH_CONFIG),
			},
			"configuration": { "config": get_host_config() },
			"paras":  {
				"paras": vec![
					(
						cumulus_primitives_core::ParaId::from(asset_hub_kusama::PARA_ID),
						ParaGenesisArgs {
							genesis_head: HeadData::default(),
							validation_code: ValidationCode(
								asset_hub_kusama_runtime::WASM_BINARY.unwrap().to_vec(),
							),
							para_kind: ParaKind::Parachain,
						},
					),
					(
						cumulus_primitives_core::ParaId::from(penpal::PARA_ID_A),
						ParaGenesisArgs {
							genesis_head: HeadData::default(),
							validation_code: ValidationCode(
								penpal_runtime::WASM_BINARY.unwrap().to_vec(),
							),
							para_kind: ParaKind::Parachain,
						},
					),
					(
						cumulus_primitives_core::ParaId::from(penpal::PARA_ID_B),
						ParaGenesisArgs {
							genesis_head: HeadData::default(),
							validation_code: ValidationCode(
								penpal_runtime::WASM_BINARY.unwrap().to_vec(),
							),
							para_kind: ParaKind::Parachain,
						},
					),
				],
			},
		});

		build_genesis_storage(genesis_config, kusama_runtime::WASM_BINARY.unwrap())
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = kusama_runtime::RuntimeGenesisConfig {
				system: kusama_runtime::SystemConfig::default(),
				balances: kusama_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.map(|k: &AccountId| (k.clone(), ENDOWMENT))
						.collect(),
				},
				session: kusama_runtime::SessionConfig {
					keys: validators::initial_authorities()
						.iter()
						.map(|x| {
							(
								x.0.clone(),
								x.0.clone(),
								kusama::session_keys(
									x.2.clone(),
									x.3.clone(),
									x.4.clone(),
									x.5.clone(),
									x.6.clone(),
									x.7.clone(),
									get_from_seed::<BeefyId>("Alice"),
								),
							)
						})
						.collect::<Vec<_>>(),
				},
				staking: kusama_runtime::StakingConfig {
					validator_count: validators::initial_authorities().len() as u32,
					minimum_validator_count: 1,
					stakers: validators::initial_authorities()
						.iter()
						.map(|x| {
							(
								x.0.clone(),
								x.1.clone(),
								STASH,
								kusama_runtime::StakerStatus::Validator,
							)
						})
						.collect(),
					invulnerables: validators::initial_authorities()
						.iter()
						.map(|x| x.0.clone())
						.collect(),
					force_era: pallet_staking::Forcing::NotForcing,
					slash_reward_fraction: Perbill::from_percent(10),
					..Default::default()
				},
				babe: kusama_runtime::BabeConfig {
					authorities: Default::default(),
					epoch_config: Some(kusama_runtime::BABE_GENESIS_EPOCH_CONFIG),
					..Default::default()
				},
				configuration: kusama_runtime::ConfigurationConfig { config: get_host_config() },
				paras: kusama_runtime::ParasConfig {
					paras: vec![
						(
							asset_hub_kusama::PARA_ID.into(),
							ParaGenesisArgs {
								genesis_head: HeadData::default(),
								validation_code: ValidationCode(
									asset_hub_kusama_runtime::WASM_BINARY.unwrap().to_vec(),
								),
								para_kind: ParaKind::Parachain,
							},
						),
						(
							penpal::PARA_ID_A.into(),
							ParaGenesisArgs {
								genesis_head: HeadData::default(),
								validation_code: ValidationCode(
									penpal_runtime::WASM_BINARY.unwrap().to_vec(),
								),
								para_kind: ParaKind::Parachain,
							},
						),
						(
							penpal::PARA_ID_B.into(),
							ParaGenesisArgs {
								genesis_head: HeadData::default(),
								validation_code: ValidationCode(
									penpal_runtime::WASM_BINARY.unwrap().to_vec(),
								),
								para_kind: ParaKind::Parachain,
							},
						),
					],
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(&genesis_config, kusama_runtime::WASM_BINARY.unwrap())
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Rococo
pub mod rococo {
	use super::*;
	pub const ED: Balance = rococo_runtime_constants::currency::EXISTENTIAL_DEPOSIT;
	use rococo_runtime_constants::currency::UNITS as ROC;
	const ENDOWMENT: u128 = 1_000_000 * ROC;

	pub fn get_host_config() -> HostConfiguration<BlockNumber> {
		HostConfiguration {
			max_upward_queue_count: 10,
			max_upward_queue_size: 51200,
			max_upward_message_size: 51200,
			max_upward_message_num_per_candidate: 10,
			max_downward_message_size: 51200,
			hrmp_sender_deposit: 0,
			hrmp_recipient_deposit: 0,
			hrmp_channel_max_capacity: 1000,
			hrmp_channel_max_message_size: 102400,
			hrmp_channel_max_total_size: 102400,
			hrmp_max_parachain_outbound_channels: 30,
			hrmp_max_parachain_inbound_channels: 30,
			..Default::default()
		}
	}

	fn session_keys(
		babe: BabeId,
		grandpa: GrandpaId,
		im_online: ImOnlineId,
		para_validator: ValidatorId,
		para_assignment: AssignmentId,
		authority_discovery: AuthorityDiscoveryId,
		beefy: BeefyId,
	) -> rococo_runtime::SessionKeys {
		rococo_runtime::SessionKeys {
			babe,
			grandpa,
			im_online,
			para_validator,
			para_assignment,
			authority_discovery,
			beefy,
		}
	}

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.map(|k| (k.clone(), ENDOWMENT))
					.collect::<Vec<_>>(),
			},
			// indices: rococo_runtime::IndicesConfig { indices: vec![] },
			"session": {
				"keys": validators::initial_authorities()
					.iter()
					.map(|x| {
						(
							x.0.clone(),
							x.0.clone(),
							session_keys(
								x.2.clone(),
								x.3.clone(),
								x.4.clone(),
								x.5.clone(),
								x.6.clone(),
								x.7.clone(),
								get_from_seed::<BeefyId>("Alice"),
							),
						)
					})
					.collect::<Vec<_>>(),
			},
			"babe": {
				"epochConfig": Some(rococo_runtime::BABE_GENESIS_EPOCH_CONFIG),
			},
			"sudo": {
				"key": Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
			},
			"configuration": { "config": get_host_config() },
			"registrar": {
				"nextFreeParaId": polkadot_primitives::LOWEST_PUBLIC_ID,
			},
		});

		build_genesis_storage(genesis_config, rococo_runtime::WASM_BINARY.unwrap())
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = rococo_runtime::RuntimeGenesisConfig {
				system: rococo_runtime::SystemConfig::default(),
				balances: rococo_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.map(|k| (k.clone(), ENDOWMENT))
						.collect(),
				},
				// indices: rococo_runtime::IndicesConfig { indices: vec![] },
				session: rococo_runtime::SessionConfig {
					keys: validators::initial_authorities()
						.iter()
						.map(|x| {
							(
								x.0.clone(),
								x.0.clone(),
								session_keys(
									x.2.clone(),
									x.3.clone(),
									x.4.clone(),
									x.5.clone(),
									x.6.clone(),
									x.7.clone(),
									get_from_seed::<BeefyId>("Alice"),
								),
							)
						})
						.collect::<Vec<_>>(),
				},
				babe: rococo_runtime::BabeConfig {
					authorities: Default::default(),
					epoch_config: Some(rococo_runtime::BABE_GENESIS_EPOCH_CONFIG),
					..Default::default()
				},
				sudo: rococo_runtime::SudoConfig {
					key: Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
				},
				configuration: rococo_runtime::ConfigurationConfig { config: get_host_config() },
				registrar: rococo_runtime::RegistrarConfig {
					next_free_para_id: polkadot_primitives::LOWEST_PUBLIC_ID,
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(&genesis_config, rococo_runtime::WASM_BINARY.unwrap())
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Asset Hub Polkadot
pub mod asset_hub_polkadot {
	use super::*;
	pub const PARA_ID: u32 = 1000;
	pub const ED: Balance = asset_hub_polkadot_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(PARA_ID),
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables_asset_hub_polkadot()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables_asset_hub_polkadot()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                      // account id
							acc,                                              // validator id
							asset_hub_polkadot_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
		});

		build_genesis_storage(
			genesis_config,
			asset_hub_polkadot_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = asset_hub_polkadot_runtime::RuntimeGenesisConfig {
				system: asset_hub_polkadot_runtime::SystemConfig::default(),
				balances: asset_hub_polkadot_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: asset_hub_polkadot_runtime::ParachainInfoConfig {
					parachain_id: PARA_ID.into(),
					..Default::default()
				},
				collator_selection: asset_hub_polkadot_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables_asset_hub_polkadot()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: asset_hub_polkadot_runtime::SessionConfig {
					keys: collators::invulnerables_asset_hub_polkadot()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                                      // account id
								acc,                                              // validator id
								asset_hub_polkadot_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: asset_hub_polkadot_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				asset_hub_polkadot_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Asset Hub Westend
pub mod asset_hub_westend {
	use super::*;
	pub const PARA_ID: u32 = 1000;
	pub const ED: Balance = asset_hub_westend_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(PARA_ID),
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                     // account id
							acc,                                             // validator id
							asset_hub_westend_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
		});

		build_genesis_storage(
			genesis_config,
			asset_hub_westend_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = asset_hub_westend_runtime::RuntimeGenesisConfig {
				system: asset_hub_westend_runtime::SystemConfig::default(),
				balances: asset_hub_westend_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: asset_hub_westend_runtime::ParachainInfoConfig {
					parachain_id: PARA_ID.into(),
					..Default::default()
				},
				collator_selection: asset_hub_westend_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: asset_hub_westend_runtime::SessionConfig {
					keys: collators::invulnerables()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                                     // account id
								acc,                                             // validator id
								asset_hub_westend_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: asset_hub_westend_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				asset_hub_westend_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Asset Hub Kusama
pub mod asset_hub_kusama {
	use super::*;
	pub const PARA_ID: u32 = 1000;
	pub const ED: Balance = asset_hub_kusama_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(PARA_ID)
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                    // account id
							acc,                                            // validator id
							asset_hub_kusama_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
		});

		build_genesis_storage(
			genesis_config,
			asset_hub_kusama_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = asset_hub_kusama_runtime::RuntimeGenesisConfig {
				system: asset_hub_kusama_runtime::SystemConfig::default(),
				balances: asset_hub_kusama_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: asset_hub_kusama_runtime::ParachainInfoConfig {
					parachain_id: PARA_ID.into(),
					..Default::default()
				},
				collator_selection: asset_hub_kusama_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: asset_hub_kusama_runtime::SessionConfig {
					keys: collators::invulnerables()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                                    // account id
								acc,                                            // validator id
								asset_hub_kusama_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: asset_hub_kusama_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				asset_hub_kusama_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Penpal
pub mod penpal {
	use super::*;
	pub const PARA_ID_A: u32 = 2000;
	pub const PARA_ID_B: u32 = 2001;
	pub const ED: Balance = penpal_runtime::EXISTENTIAL_DEPOSIT;

	pub fn genesis(para_id: u32) -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(para_id),
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                          // account id
							acc,                                  // validator id
							penpal_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
			"sudo": {
				"key": Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
			},
		});

		build_genesis_storage(
			genesis_config,
			penpal_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis(para_id: u32) -> Storage {
			let genesis_config = penpal_runtime::RuntimeGenesisConfig {
				system: penpal_runtime::SystemConfig::default(),
				balances: penpal_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: penpal_runtime::ParachainInfoConfig {
					parachain_id: para_id.into(),
					..Default::default()
				},
				collator_selection: penpal_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: penpal_runtime::SessionConfig {
					keys: collators::invulnerables()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                          // account id
								acc,                                  // validator id
								penpal_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: penpal_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				sudo: penpal_runtime::SudoConfig {
					key: Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				penpal_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis(101);
			let j2 = genesis(101);

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Collectives
pub mod collectives {
	use super::*;
	pub const PARA_ID: u32 = 1001;
	pub const ED: Balance = collectives_polkadot_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(PARA_ID),
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                        // account id
							acc,                                                // validator id
							collectives_polkadot_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
		});

		build_genesis_storage(
			genesis_config,
			collectives_polkadot_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = collectives_polkadot_runtime::RuntimeGenesisConfig {
				system: collectives_polkadot_runtime::SystemConfig::default(),
				balances: collectives_polkadot_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: collectives_polkadot_runtime::ParachainInfoConfig {
					parachain_id: PARA_ID.into(),
					..Default::default()
				},
				collator_selection: collectives_polkadot_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: collectives_polkadot_runtime::SessionConfig {
					keys: collators::invulnerables()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                                        // account id
								acc,                                                // validator id
								collectives_polkadot_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: collectives_polkadot_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				collectives_polkadot_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Bridge Hub Kusama
pub mod bridge_hub_kusama {
	use super::*;
	pub const PARA_ID: u32 = 1002;
	pub const ED: Balance = bridge_hub_kusama_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(PARA_ID),
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                     // account id
							acc,                                             // validator id
							bridge_hub_kusama_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
		});

		build_genesis_storage(
			genesis_config,
			bridge_hub_kusama_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = bridge_hub_kusama_runtime::RuntimeGenesisConfig {
				system: bridge_hub_kusama_runtime::SystemConfig::default(),
				balances: bridge_hub_kusama_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: bridge_hub_kusama_runtime::ParachainInfoConfig {
					parachain_id: PARA_ID.into(),
					..Default::default()
				},
				collator_selection: bridge_hub_kusama_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: bridge_hub_kusama_runtime::SessionConfig {
					keys: collators::invulnerables()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                                     // account id
								acc,                                             // validator id
								bridge_hub_kusama_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: bridge_hub_kusama_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				bridge_hub_kusama_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Bridge Hub Polkadot
pub mod bridge_hub_polkadot {
	use super::*;
	pub const PARA_ID: u32 = 1002;
	pub const ED: Balance = bridge_hub_polkadot_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(PARA_ID),
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                       // account id
							acc,                                               // validator id
							bridge_hub_polkadot_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
		});

		build_genesis_storage(
			genesis_config,
			bridge_hub_polkadot_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = bridge_hub_polkadot_runtime::RuntimeGenesisConfig {
				system: bridge_hub_polkadot_runtime::SystemConfig::default(),
				balances: bridge_hub_polkadot_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: bridge_hub_polkadot_runtime::ParachainInfoConfig {
					parachain_id: PARA_ID.into(),
					..Default::default()
				},
				collator_selection: bridge_hub_polkadot_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: bridge_hub_polkadot_runtime::SessionConfig {
					keys: collators::invulnerables()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                                       // account id
								acc,                                               // validator id
								bridge_hub_polkadot_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: bridge_hub_polkadot_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				bridge_hub_polkadot_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}

// Bridge Hub Rococo & Bridge Hub Wococo
pub mod bridge_hub_rococo {
	use super::*;
	pub const PARA_ID: u32 = 1013;
	pub const ED: Balance = bridge_hub_rococo_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

	pub fn genesis() -> Storage {
		let genesis_config = serde_json::json!({
			"balances": {
				"balances": accounts::init_balances()
					.iter()
					.cloned()
					.map(|k| (k, ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": cumulus_primitives_core::ParaId::from(PARA_ID),
			},
			"collatorSelection": {
				"invulnerables": collators::invulnerables()
					.iter()
					.cloned()
					.map(|(acc, _)| acc)
					.collect::<Vec<_>>(),
				"candidacyBond": ED * 16,
			},
			"session": {
				"keys": collators::invulnerables()
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                     // account id
							acc,                                             // validator id
							bridge_hub_rococo_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
			"bridgeWococoGrandpa": {
				"owner": Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
			},
			"bridgeRococoGrandpa": {
				"owner": Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
			},
			"bridgeRococoMessages": {
				"owner": Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
			},
			"bridgeWococoMessages": {
				"owner": Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
			},
		});

		build_genesis_storage(
			genesis_config,
			bridge_hub_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
	}

	#[cfg(test)]
	mod legacy_vs_json_check {
		use super::*;
		fn genesis() -> Storage {
			let genesis_config = bridge_hub_rococo_runtime::RuntimeGenesisConfig {
				system: bridge_hub_rococo_runtime::SystemConfig::default(),
				balances: bridge_hub_rococo_runtime::BalancesConfig {
					balances: accounts::init_balances()
						.iter()
						.cloned()
						.map(|k| (k, ED * 4096))
						.collect(),
				},
				parachain_info: bridge_hub_rococo_runtime::ParachainInfoConfig {
					parachain_id: PARA_ID.into(),
					..Default::default()
				},
				collator_selection: bridge_hub_rococo_runtime::CollatorSelectionConfig {
					invulnerables: collators::invulnerables()
						.iter()
						.cloned()
						.map(|(acc, _)| acc)
						.collect(),
					candidacy_bond: ED * 16,
					..Default::default()
				},
				session: bridge_hub_rococo_runtime::SessionConfig {
					keys: collators::invulnerables()
						.into_iter()
						.map(|(acc, aura)| {
							(
								acc.clone(),                                     // account id
								acc,                                             // validator id
								bridge_hub_rococo_runtime::SessionKeys { aura }, // session keys
							)
						})
						.collect(),
				},
				polkadot_xcm: bridge_hub_rococo_runtime::PolkadotXcmConfig {
					safe_xcm_version: Some(SAFE_XCM_VERSION),
					..Default::default()
				},
				bridge_wococo_grandpa: bridge_hub_rococo_runtime::BridgeWococoGrandpaConfig {
					owner: Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
					..Default::default()
				},
				bridge_rococo_grandpa: bridge_hub_rococo_runtime::BridgeRococoGrandpaConfig {
					owner: Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
					..Default::default()
				},
				bridge_rococo_messages: bridge_hub_rococo_runtime::BridgeRococoMessagesConfig {
					owner: Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
					..Default::default()
				},
				bridge_wococo_messages: bridge_hub_rococo_runtime::BridgeWococoMessagesConfig {
					owner: Some(get_account_id_from_seed::<sr25519::Public>(accounts::BOB)),
					..Default::default()
				},
				..Default::default()
			};

			build_genesis_storage_legacy(
				&genesis_config,
				bridge_hub_rococo_runtime::WASM_BINARY
					.expect("WASM binary was not build, please build it!"),
			)
		}

		#[test]
		fn test_genesis() {
			let j1 = super::genesis();
			let j2 = genesis();

			assert_eq!(j1.top, j2.top);
			assert_eq!(j1.children_default, j2.children_default);
		}
	}
}
