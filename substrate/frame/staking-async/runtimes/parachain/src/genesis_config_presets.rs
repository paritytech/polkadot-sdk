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

//! # Staking Async Runtime genesis config presets

use crate::*;
use alloc::{vec, vec::Vec};
use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use parachains_common::{AccountId, AuraId};
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;
use testnet_parachains_constants::westend::{
	currency::UNITS as WND, xcm_version::SAFE_XCM_VERSION,
};

const STAKING_ASYNC_PARA_ED: Balance = ExistentialDeposit::get();

struct GenesisParams {
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
	dev_stakers: Option<(u32, u32)>,
	pages: u32,
	max_electing_voters: u32,
	validator_count: u32,
	root: AccountId,
	id: ParaId,
}

fn staking_async_parachain_genesis(params: GenesisParams) -> serde_json::Value {
	let GenesisParams {
		invulnerables,
		endowed_accounts,
		endowment,
		dev_stakers,
		validator_count,
		root,
		// TODO: find a way to set these here, but for now we will set them directly in the runtime.
		pages: _pages,
		max_electing_voters: _max_electing_voters,
		id,
	} = params;
	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, endowment)).collect(),
		},
		parachain_info: ParachainInfoConfig { parachain_id: id },
		collator_selection: CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: STAKING_ASYNC_PARA_ED * 16,
		},
		session: SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),          // account id
						acc,                  // validator id
						SessionKeys { aura }, // session keys
					)
				})
				.collect(),
		},
		polkadot_xcm: PolkadotXcmConfig { safe_xcm_version: Some(SAFE_XCM_VERSION) },
		sudo: SudoConfig { key: Some(root) },
		staking: StakingConfig { validator_count, dev_stakers, ..Default::default() }
	})
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let mut dev_and_testnet_params = GenesisParams {
		invulnerables: vec![
			(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into()),
			(Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Bob.public().into()),
		],
		endowed_accounts: Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
		endowment: WND * 1_000_000,
		dev_stakers: Some((100, 2000)),
		validator_count: 10,
		root: Sr25519Keyring::Alice.to_account_id(),
		id: 1100.into(),
		max_electing_voters: 2000,
		pages: 4,
	};
	let patch = match id.as_ref() {
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET =>
			staking_async_parachain_genesis(dev_and_testnet_params),
		sp_genesis_builder::DEV_RUNTIME_PRESET =>
			staking_async_parachain_genesis(dev_and_testnet_params),
		"ksm_size" => {
			dev_and_testnet_params.validator_count = 1_000;
			dev_and_testnet_params.dev_stakers = Some((4_000, 20_000));
			staking_async_parachain_genesis(dev_and_testnet_params)
		},
		"dot_size" => {
			dev_and_testnet_params.validator_count = 500;
			dev_and_testnet_params.dev_stakers = Some((2_000, 25_000));
			staking_async_parachain_genesis(dev_and_testnet_params)
		},
		_ => panic!("unrecognized genesis preset!"),
	};

	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
		PresetId::from("ksm_size"),
		PresetId::from("dot_size"),
	]
}
