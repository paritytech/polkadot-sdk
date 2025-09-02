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
use alloc::{
	string::{String, ToString},
	vec,
	vec::Vec,
};
use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use parachains_common::{AccountId, AuraId};
use sp_core::{crypto::get_public_from_string_or_panic, sr25519};
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;
use sp_staking::StakerStatus;
use testnet_parachains_constants::westend::{
	currency::UNITS as WND, xcm_version::SAFE_XCM_VERSION,
};

const STAKING_ASYNC_PARA_ED: Balance = ExistentialDeposit::get();

struct GenesisParams {
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
	dev_stakers: Option<(u32, u32)>,
	validators: Vec<AccountId>,
	validator_count: u32,
	root: AccountId,
	id: ParaId,
}

fn staking_async_parachain_genesis(params: GenesisParams, preset: String) -> serde_json::Value {
	let GenesisParams {
		invulnerables,
		endowed_accounts,
		endowment,
		dev_stakers,
		validators,
		validator_count,
		root,
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
				.clone()
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
		preset_store: crate::PresetStoreConfig { preset, ..Default::default() },
		staking: StakingConfig {
			validator_count,
			dev_stakers,
			stakers: validators
				.into_iter()
				.map(|acc| (acc, endowment / 2, StakerStatus::Validator))
				.collect(),
			..Default::default()
		}
	})
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let mut params = GenesisParams {
		invulnerables: vec![
			// in all cases, our local collators is just charlie. ZN seems to override these
			// anyways.
			(
				get_public_from_string_or_panic::<sr25519::Public>("Charlie").into(),
				get_public_from_string_or_panic::<AuraId>("Charlie").into(),
			),
		],
		endowed_accounts: Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
		endowment: WND * 1_000_000,
		dev_stakers: Some((100, 2000)),
		validators: Default::default(),
		validator_count: 10,
		root: Sr25519Keyring::Alice.to_account_id(),
		id: 1100.into(),
	};
	let patch = match id.as_ref() {
		"real-s" => {
			params.validator_count = 2;
			// generate no new "fake" validators.
			params.dev_stakers = Some((0, 500));
			// set expected relay validators in genesis so they are elected
			params.validators = vec![
				Sr25519Keyring::AliceStash.to_account_id(),
				Sr25519Keyring::BobStash.to_account_id(),
			];
			staking_async_parachain_genesis(params, id.to_string())
		},
		"real-m" => {
			params.validator_count = 4;
			// generate no new "fake" validators.
			params.dev_stakers = Some((0, 2000));
			// set expected relay validators in genesis so they are elected
			params.validators = vec![
				Sr25519Keyring::AliceStash.to_account_id(),
				Sr25519Keyring::BobStash.to_account_id(),
				Sr25519Keyring::EveStash.to_account_id(),
				Sr25519Keyring::DaveStash.to_account_id(),
			];
			staking_async_parachain_genesis(params, id.to_string())
		},
		"fake-dev" => {
			// nada
			staking_async_parachain_genesis(params, id.to_string())
		},
		"fake-dot" => {
			params.validator_count = 500;
			params.dev_stakers = Some((2_500, 25_000));
			staking_async_parachain_genesis(params, id.to_string())
		},
		"fake-ksm" => {
			params.validator_count = 1_000;
			params.dev_stakers = Some((4_500, 15_000));
			staking_async_parachain_genesis(params, id.to_string())
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
		PresetId::from("real-s"),
		PresetId::from("real-m"),
		PresetId::from("fake-dev"),
		PresetId::from("fake-dot"),
		PresetId::from("fake-ksm"),
	]
}
