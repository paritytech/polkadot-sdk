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

//! # Asset Hub Next Westend Runtime genesis config presets

use crate::*;
use alloc::{vec, vec::Vec};
use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use hex_literal::hex;
use parachains_common::{AccountId, AuraId};
use sp_core::crypto::UncheckedInto;
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;
use testnet_parachains_constants::westend::{
	currency::UNITS as WND, xcm_version::SAFE_XCM_VERSION,
};

const ASSET_HUB_NEXT_WESTEND_ED: Balance = ExistentialDeposit::get();

fn asset_hub_next_westend_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
	dev_stakers: Option<(u32, u32)>,
	root: AccountId,
	id: ParaId,
) -> serde_json::Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, endowment)).collect(),
		},
		parachain_info: ParachainInfoConfig { parachain_id: id },
		collator_selection: CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: ASSET_HUB_NEXT_WESTEND_ED * 16,
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
		staking: StakingConfig {
			// we wish to elect 500 validators, maximum is set to 1000 in the runtime configs.
			validator_count: 500,
			// smallest validator set we accept, 50 for now?
			minimum_validator_count: 50,
			// initial stakers
			dev_stakers,
			..Default::default()
		}
	})
}

/// Encapsulates names of predefined presets.
mod preset_names {
	pub const PRESET_GENESIS: &str = "genesis";
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	use preset_names::*;
	let dev_stakers = Some((1_000, 25_000));
	let patch = match id.as_ref() {
		PRESET_GENESIS => asset_hub_next_westend_genesis(
			// initial collators.
			vec![
				(
					hex!("d2a4117d7f47cce89f84230a8d4003f42d615261a0854d1bb69da410b4561c07").into(),
					hex!("d2a4117d7f47cce89f84230a8d4003f42d615261a0854d1bb69da410b4561c07")
						.unchecked_into(),
				),
				(
					hex!("0ca6cf3fb25cc2dfe510eb1b55c36839ba9cb80dee22de1278f9b48898919563").into(),
					hex!("0ca6cf3fb25cc2dfe510eb1b55c36839ba9cb80dee22de1278f9b48898919563")
						.unchecked_into(),
				),
			],
			Vec::new(),
			ASSET_HUB_NEXT_WESTEND_ED * 4096,
			dev_stakers,
			// Ask Dónal for access, or overwrite from Relay via XCM.
			hex!("b662f28c3beb0d03e6f4cc9a5d6eb158c609d4ac909a746a2a8dc6b40634be65").into(),
			1100.into(),
		),
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => asset_hub_next_westend_genesis(
			// initial collators.
			vec![
				(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into()),
				(Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Bob.public().into()),
			],
			Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
			WND * 1_000_000,
			dev_stakers,
			Sr25519Keyring::Alice.to_account_id(),
			1100.into(),
		),
		sp_genesis_builder::DEV_RUNTIME_PRESET => asset_hub_next_westend_genesis(
			// initial collators.
			vec![(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into())],
			vec![
				Sr25519Keyring::Alice.to_account_id(),
				Sr25519Keyring::Bob.to_account_id(),
				Sr25519Keyring::AliceStash.to_account_id(),
				Sr25519Keyring::BobStash.to_account_id(),
			],
			WND * 1_000_000,
			dev_stakers,
			Sr25519Keyring::Alice.to_account_id(),
			1100.into(),
		),
		_ => return None,
	};

	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	use preset_names::*;
	vec![
		PresetId::from(PRESET_GENESIS),
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
	]
}
