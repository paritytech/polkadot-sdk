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
	BabeConfig, Balance, BalancesConfig, ElectionsConfig, NominationPoolsConfig,
	RuntimeGenesisConfig, SessionConfig, SessionKeys, SocietyConfig, StakerStatus, StakingConfig,
	SudoConfig, TechnicalCommitteeConfig, BABE_GENESIS_EPOCH_CONFIG,
};
use alloc::{vec, vec::Vec};
use pallet_im_online::sr25519::AuthorityId as ImOnlineId;
use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;
use sp_consensus_babe::AuthorityId as BabeId;
use sp_consensus_beefy::ecdsa_crypto::AuthorityId as BeefyId;
use sp_consensus_grandpa::AuthorityId as GrandpaId;
use sp_keyring::Sr25519Keyring;
use sp_mixnet::types::AuthorityId as MixnetId;
use sp_runtime::Perbill;

const ENDOWMENT: Balance = 10_000_000 * DOLLARS;
const STASH: Balance = ENDOWMENT / 1000;

pub struct StakingPlaygroundConfig {
	/// (Validators, Nominators)
	pub dev_stakers: (u32, u32),
	pub validator_count: u32,
	pub minimum_validator_count: u32,
}

pub type Staker = (AccountId, AccountId, Balance, StakerStatus<AccountId>);

/// Helper function to create RuntimeGenesisConfig json patch for testing.
pub fn kitchen_sink_genesis(
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
	root_key: AccountId,
	endowed_accounts: Vec<AccountId>,
	stakers: Vec<Staker>,
	staking_playground_config: Option<StakingPlaygroundConfig>,
) -> serde_json::Value {
	let (validator_count, min_validator_count, dev_stakers) = match staking_playground_config {
		Some(c) => (c.validator_count, c.minimum_validator_count, Some(c.dev_stakers)),
		None => {
			let authorities_count = initial_authorities.len() as u32;
			(authorities_count, authorities_count, None)
		},
	};

	let collective = collective(&endowed_accounts);

	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|x| (x, ENDOWMENT)).collect(),
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
				.collect(),
		},
		staking: StakingConfig {
			validator_count,
			minimum_validator_count: min_validator_count,
			invulnerables: initial_authorities
				.iter()
				.map(|x| x.0.clone())
				.collect::<Vec<_>>()
				.try_into()
				.expect("too many authorities"),
			slash_reward_fraction: Perbill::from_percent(10),
			stakers,
			dev_stakers
		},
		elections: ElectionsConfig {
			members: collective.iter().cloned().map(|member| (member, STASH)).collect(),
		},
		technical_committee: TechnicalCommitteeConfig { members: collective },
		sudo: SudoConfig { key: Some(root_key) },
		babe: BabeConfig { epoch_config: BABE_GENESIS_EPOCH_CONFIG },
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

/// Extract some accounts from endowed to be put into the collective.
fn collective(endowed: &[AccountId]) -> Vec<AccountId> {
	const MAX_COLLECTIVE_SIZE: usize = 50;
	let endowed_accounts_count = endowed.len();
	endowed
		.iter()
		.take(((endowed_accounts_count + 1) / 2).min(MAX_COLLECTIVE_SIZE))
		.cloned()
		.collect()
}

fn session_keys(
	grandpa: GrandpaId,
	babe: BabeId,
	im_online: ImOnlineId,
	authority_discovery: AuthorityDiscoveryId,
	mixnet: MixnetId,
	beefy: BeefyId,
) -> SessionKeys {
	SessionKeys { grandpa, babe, im_online, authority_discovery, mixnet, beefy }
}
