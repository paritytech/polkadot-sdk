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

// Substrate
use beefy_primitives::ecdsa_crypto::AuthorityId as BeefyId;
use grandpa::AuthorityId as GrandpaId;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_core::storage::Storage;
use sp_runtime::Perbill;

// Polkadot
use polkadot_primitives::{AssignmentId, ValidatorId};

// Cumulus
use emulated_integration_tests_common::{
	accounts, build_genesis_storage, get_host_config, validators,
};
use parachains_common::Balance;
use westend_runtime_constants::currency::UNITS as WND;

pub const ED: Balance = westend_runtime_constants::currency::EXISTENTIAL_DEPOSIT;
const ENDOWMENT: u128 = 1_000_000 * WND;
const STASH: u128 = 100 * WND;

fn session_keys(
	babe: BabeId,
	grandpa: GrandpaId,
	para_validator: ValidatorId,
	para_assignment: AssignmentId,
	authority_discovery: AuthorityDiscoveryId,
	beefy: BeefyId,
) -> westend_runtime::SessionKeys {
	westend_runtime::SessionKeys {
		babe,
		grandpa,
		para_validator,
		para_assignment,
		authority_discovery,
		beefy,
	}
}

pub fn genesis() -> Storage {
	let genesis_config = westend_runtime::RuntimeGenesisConfig {
		system: westend_runtime::SystemConfig::default(),
		balances: westend_runtime::BalancesConfig {
			balances: accounts::init_balances().iter().cloned().map(|k| (k, ENDOWMENT)).collect(),
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
			invulnerables: validators::initial_authorities().iter().map(|x| x.0.clone()).collect(),
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

	build_genesis_storage(&genesis_config, westend_runtime::WASM_BINARY.unwrap())
}
