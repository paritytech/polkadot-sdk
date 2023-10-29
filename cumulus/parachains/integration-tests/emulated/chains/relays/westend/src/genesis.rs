// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use integration_tests_common::constants::{
    accounts,
    validators,
    get_from_seed,
};

use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use grandpa::AuthorityId as GrandpaId;
use parachains_common::{AccountId, Balance, BlockNumber};
use polkadot_runtime_parachains::{
	configuration::HostConfiguration,
};
use polkadot_primitives::{AssignmentId, ValidatorId};
use beefy_primitives::ecdsa_crypto::AuthorityId as BeefyId;
use sp_runtime::{
	BuildStorage, Perbill,
};
use sp_core::storage::Storage;

// Westend
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
	let genesis_config = westend_runtime::RuntimeGenesisConfig {
		system: westend_runtime::SystemConfig {
			code: westend_runtime::WASM_BINARY.unwrap().to_vec(),
			..Default::default()
		},
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
		staking: westend_runtime::StakingConfig {
			validator_count: validators::initial_authorities().len() as u32,
			minimum_validator_count: 1,
			stakers: validators::initial_authorities()
				.iter()
				.map(|x| {
					(x.0.clone(), x.1.clone(), STASH, westend_runtime::StakerStatus::Validator)
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

	genesis_config.build_storage().unwrap()
}
