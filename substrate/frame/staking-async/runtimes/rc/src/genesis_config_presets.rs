// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Genesis configs presets for the Westend runtime

use crate::{
	BabeConfig, BalancesConfig, ConfigurationConfig, RegistrarConfig, RuntimeGenesisConfig,
	SessionConfig, SessionKeys, StakingAhClientConfig, SudoConfig, BABE_GENESIS_EPOCH_CONFIG,
};
#[cfg(not(feature = "std"))]
use alloc::format;
use alloc::{string::ToString, vec, vec::Vec};
use core::panic;
use frame_support::build_struct_json_patch;
use pallet_staking_async_rc_runtime_constants::currency::UNITS as WND;
use polkadot_primitives::{AccountId, AssignmentId, SchedulerParams, ValidatorId};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_beefy::ecdsa_crypto::AuthorityId as BeefyId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_core::{crypto::get_public_from_string_or_panic, sr25519};
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

/// Helper function to generate stash, controller and session key from seed
fn get_authority_keys_from_seed(
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
	(
		keys.0,
		keys.1,
		keys.2,
		keys.3,
		keys.4,
		keys.5,
		keys.6,
		get_public_from_string_or_panic::<BeefyId>(seed),
	)
}

/// Helper function to generate stash, controller and session key from seed
fn get_authority_keys_from_seed_no_beefy(
	seed: &str,
) -> (AccountId, AccountId, BabeId, GrandpaId, ValidatorId, AssignmentId, AuthorityDiscoveryId) {
	(
		get_public_from_string_or_panic::<sr25519::Public>(&format!("{}//stash", seed)).into(),
		get_public_from_string_or_panic::<sr25519::Public>(seed).into(),
		get_public_from_string_or_panic::<BabeId>(seed),
		get_public_from_string_or_panic::<GrandpaId>(seed),
		get_public_from_string_or_panic::<ValidatorId>(seed),
		get_public_from_string_or_panic::<AssignmentId>(seed),
		get_public_from_string_or_panic::<AuthorityDiscoveryId>(seed),
	)
}

fn westend_session_keys(
	babe: BabeId,
	grandpa: GrandpaId,
	para_validator: ValidatorId,
	para_assignment: AssignmentId,
	authority_discovery: AuthorityDiscoveryId,
	beefy: BeefyId,
) -> SessionKeys {
	SessionKeys { babe, grandpa, para_validator, para_assignment, authority_discovery, beefy }
}

fn default_parachains_host_configuration(
) -> polkadot_runtime_parachains::configuration::HostConfiguration<polkadot_primitives::BlockNumber>
{
	use polkadot_primitives::{
		node_features::FeatureIndex, ApprovalVotingParams, AsyncBackingParams, MAX_CODE_SIZE,
		MAX_POV_SIZE,
	};

	polkadot_runtime_parachains::configuration::HostConfiguration {
		// Important configs are equal to what is on Polkadot. These configs can be tweaked to mimic
		// different VMP congestion scenarios.
		max_downward_message_size: 51200,
		max_upward_message_size: 65531,
		max_upward_message_num_per_candidate: 16,
		max_upward_queue_count: 174762,
		max_upward_queue_size: 1048576,

		validation_upgrade_cooldown: 2u32,
		validation_upgrade_delay: 2,
		code_retention_period: 1200,
		max_code_size: MAX_CODE_SIZE,
		max_pov_size: MAX_POV_SIZE,
		max_head_data_size: 32 * 1024,
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
			max_candidate_depth: 0,
			allowed_ancestry_len: 0,
		},
		node_features: bitvec::vec::BitVec::from_element(
			(1u8 << (FeatureIndex::ElasticScalingMVP as usize)) |
				(1u8 << (FeatureIndex::EnableAssignmentsV2 as usize)) |
				(1u8 << (FeatureIndex::CandidateReceiptV2 as usize)),
		),
		scheduler_params: SchedulerParams {
			lookahead: 3,
			group_rotation_frequency: 20,
			paras_availability_period: 4,
			..Default::default()
		},
		approval_voting_params: ApprovalVotingParams { max_approval_coalesce_count: 5 },
		..Default::default()
	}
}

#[test]
fn default_parachains_host_configuration_is_consistent() {
	default_parachains_host_configuration().panic_if_not_consistent();
}

/// Helper function to create westend runtime `GenesisConfig` patch for testing
fn westend_testnet_genesis(
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
	preset: alloc::string::String,
) -> serde_json::Value {
	let endowed_accounts =
		Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect::<Vec<_>>();

	const ENDOWMENT: u128 = 1_000_000 * WND;

	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().map(|k| (k.clone(), ENDOWMENT)).collect::<Vec<_>>(),
		},
		session: SessionConfig {
			keys: initial_authorities
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
		babe: BabeConfig { epoch_config: BABE_GENESIS_EPOCH_CONFIG },
		sudo: SudoConfig { key: Some(root_key) },
		configuration: ConfigurationConfig { config: default_parachains_host_configuration() },
		registrar: RegistrarConfig { next_free_para_id: polkadot_primitives::LOWEST_PUBLIC_ID },
		preset_store: crate::PresetStoreConfig { preset, ..Default::default() },
		staking_ah_client: StakingAhClientConfig {
			operating_mode: pallet_staking_async_ah_client::OperatingMode::Active,
			..Default::default()
		}
	})
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let patch = match id.as_ref() {
		"real-m" => westend_testnet_genesis(
			vec![
				get_authority_keys_from_seed("Alice"),
				get_authority_keys_from_seed("Bob"),
				get_authority_keys_from_seed("Eve"),
				get_authority_keys_from_seed("Dave"),
			],
			Sr25519Keyring::Alice.to_account_id(),
			id.to_string(),
		),
		"real-s" => westend_testnet_genesis(
			vec![get_authority_keys_from_seed("Alice"), get_authority_keys_from_seed("Bob")],
			Sr25519Keyring::Alice.to_account_id(),
			id.to_string(),
		),
		"fake-s" => westend_testnet_genesis(
			vec![get_authority_keys_from_seed("Alice"), get_authority_keys_from_seed("Bob")],
			Sr25519Keyring::Alice.to_account_id(),
			id.to_string(),
		),
		_ => panic!("Unknown preset ID: {}", id),
	};
	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![PresetId::from("real-m"), PresetId::from("real-s"), PresetId::from("fake-s")]
}
