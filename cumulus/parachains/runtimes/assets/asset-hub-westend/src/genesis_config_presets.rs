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

//! # Asset Hub Westend Runtime genesis config presets

use crate::*;
use alloc::{vec, vec::Vec};
use cumulus_primitives_core::ParaId;
use hex_literal::hex;
use parachains_common::{AccountId, AuraId};
use sp_core::crypto::UncheckedInto;
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;
use testnet_parachains_constants::westend::{
	currency::UNITS as WND, xcm_version::SAFE_XCM_VERSION,
};

const ASSET_HUB_WESTEND_ED: Balance = ExistentialDeposit::get();

fn asset_hub_westend_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
	id: ParaId,
) -> serde_json::Value {
	let config = RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, endowment)).collect(),
		},
		parachain_info: ParachainInfoConfig { parachain_id: id, ..Default::default() },
		collator_selection: CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: ASSET_HUB_WESTEND_ED * 16,
			..Default::default()
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
			..Default::default()
		},
		polkadot_xcm: PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			..Default::default()
		},
		..Default::default()
	};

	serde_json::to_value(config).expect("Could not build genesis config.")
}

/// Encapsulates names of predefined presets.
mod preset_names {
	pub const PRESET_GENESIS: &str = "genesis";
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	use preset_names::*;
	let patch = match id.try_into() {
		Ok(PRESET_GENESIS) => asset_hub_westend_genesis(
			// initial collators.
			vec![
				(
					hex!("9cfd429fa002114f33c1d3e211501d62830c9868228eb3b4b8ae15a83de04325").into(),
					hex!("9cfd429fa002114f33c1d3e211501d62830c9868228eb3b4b8ae15a83de04325")
						.unchecked_into(),
				),
				(
					hex!("12a03fb4e7bda6c9a07ec0a11d03c24746943e054ff0bb04938970104c783876").into(),
					hex!("12a03fb4e7bda6c9a07ec0a11d03c24746943e054ff0bb04938970104c783876")
						.unchecked_into(),
				),
				(
					hex!("1256436307dfde969324e95b8c62cb9101f520a39435e6af0f7ac07b34e1931f").into(),
					hex!("1256436307dfde969324e95b8c62cb9101f520a39435e6af0f7ac07b34e1931f")
						.unchecked_into(),
				),
				(
					hex!("98102b7bca3f070f9aa19f58feed2c0a4e107d203396028ec17a47e1ed80e322").into(),
					hex!("98102b7bca3f070f9aa19f58feed2c0a4e107d203396028ec17a47e1ed80e322")
						.unchecked_into(),
				),
			],
			Vec::new(),
			ASSET_HUB_WESTEND_ED * 4096,
			1000.into(),
		),
		Ok(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET) => asset_hub_westend_genesis(
			// initial collators.
			vec![
				(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into()),
				(Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Bob.public().into()),
			],
			Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
			WND * 1_000_000,
			1000.into(),
		),
		Ok(sp_genesis_builder::DEV_RUNTIME_PRESET) => asset_hub_westend_genesis(
			// initial collators.
			vec![(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into())],
			vec![
				Sr25519Keyring::Alice.to_account_id(),
				Sr25519Keyring::Bob.to_account_id(),
				Sr25519Keyring::AliceStash.to_account_id(),
				Sr25519Keyring::BobStash.to_account_id(),
			],
			WND * 1_000_000,
			1000.into(),
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
