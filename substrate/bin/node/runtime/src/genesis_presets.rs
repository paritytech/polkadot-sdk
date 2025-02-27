// This file is part of Substrate.

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

//! Genesis Presets for the Kitchensink Runtime

use polkadot_sdk::*;

use crate::{
	constants::currency::*, frame_support::build_struct_json_patch, AccountId, AssetsConfig,
	Balance, BalancesConfig, IndicesConfig, NominationPoolsConfig, RuntimeGenesisConfig,
	SessionConfig, SocietyConfig, StakerStatus, StakingConfig, BABE_GENESIS_EPOCH_CONFIG,
};
use alloc::{vec, vec::Vec};
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_beefy::ecdsa_crypto::AuthorityId as BeefyId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};
use sp_runtime::{BoundedVec, Perbill};

fn kitchensink_genesis(
	invulnerables: Vec<(AccountId, BabeId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
) -> serde_json::Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		indices: IndicesConfig { indices: vec![] },
		balances: BalancesConfig { balances: endowed, ..Default::default() },
		session: SessionConfig {
			keys: vec![
				(alice(), dave(), session_keys_from_seed(Ed25519Keyring::Alice.into())),
				(bob(), eve(), session_keys_from_seed(Ed25519Keyring::Bob.into())),
				(charlie(), ferdie(), session_keys_from_seed(Ed25519Keyring::Charlie.into())),
			],
			..Default::default()
		},
		staking: StakingConfig {
			stakers: vec![
				(dave(), dave(), 111 * DOLLARS, StakerStatus::Validator),
				(eve(), eve(), 100 * DOLLARS, StakerStatus::Validator),
				(ferdie(), ferdie(), 100 * DOLLARS, StakerStatus::Validator),
			],
			validator_count: 3,
			minimum_validator_count: 0,
			slash_reward_fraction: Perbill::from_percent(10),
			invulnerables: BoundedVec::try_from(vec![alice(), bob(), charlie()])
				.expect("Too many invulnerable validators: upper limit is MaxInvulnerables from pallet staking config"),
			..Default::default()
		},
		society: SocietyConfig { pot: 0 },
		assets: AssetsConfig { assets: vec![(9, alice(), true, 1)], ..Default::default() },
		..Default::default()
	})
}

/// Helper function to create RuntimeGenesisConfig json patch for testing.
pub fn testnet_genesis(
	initial_authorities: Vec<(
		AccountId,
		AccountId,
		GrandpaId,
		BabeId,
		ImOnlineId,
		AuthorityDiscoveryId,
		MixnetId,
		BeefyId,
	)>,
	initial_nominators: Vec<AccountId>,
	root_key: AccountId,
	endowed_accounts: Option<Vec<AccountId>>,
	dev_stakers: Option<(u32, u32)>,
	minimum_validator_count: Option<u32>,
) -> serde_json::Value {
	let (initial_authorities, endowed_accounts, num_endowed_accounts, stakers) =
		configure_accounts(initial_authorities, initial_nominators, endowed_accounts, STASH);
	const MAX_COLLECTIVE_SIZE: usize = 50;

	let min_validator_count =
		minimum_validator_count.unwrap_or_else(|| initial_authorities.len() as u32);

	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|x| (x, ENDOWMENT)).collect::<Vec<_>>(),
			..Default::default()
		},
		session: SessionConfig {
			keys: initial_authorities
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
		staking: StakingConfig {
			validator_count,
			minimum_validator_count: min_validator_count,
			invulnerables: initial_authorities.iter().map(|x| x.0.clone()).collect::<Vec<_>>(),
			slash_reward_fraction: Perbill::from_percent(10),
			stakers: stakers.clone(),
			dev_stakers
		},
		elections: ElectionsConfig {
			members: endowed_accounts
				.iter()
				.take(((num_endowed_accounts + 1) / 2).min(MAX_COLLECTIVE_SIZE))
				.cloned()
				.map(|member| (member, STASH))
				.collect::<Vec<_>>(),
		},
		technical_committee: TechnicalCommitteeConfig {
			members: endowed_accounts
				.iter()
				.take(((num_endowed_accounts + 1) / 2).min(MAX_COLLECTIVE_SIZE))
				.cloned()
				.collect::<Vec<_>>(),
		},
		sudo: SudoConfig { key: Some(root_key.clone()) },
		babe: BabeConfig { epochConfig: Some(BABE_GENESIS_EPOCH_CONFIG) },
		society: SocietyConfig { pot: 0 },
		assets: AssetsConfig {
			// This asset is used by the NIS pallet as counterpart currency.
			assets: vec![(9, Sr25519Keyring::Alice.to_account_id(), true, 1)],
			..Default::default()
		},
		nomination_pools: NominationPoolsConfig {
			min_create_bond: 10 * DOLLARS,
			min_join_bond: 1 * DOLLARS,
		},
	})
}
