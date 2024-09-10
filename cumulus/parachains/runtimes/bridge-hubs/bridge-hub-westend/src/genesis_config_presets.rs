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

//! # Bridge Hub Westend Runtime genesis config presets

use crate::*;
use alloc::{vec, vec::Vec};
use cumulus_primitives_core::ParaId;
use parachains_common::{genesis_config_helpers::*, AccountId, AuraId};
use sp_core::sr25519;
use sp_genesis_builder::PresetId;
use testnet_parachains_constants::westend::xcm_version::SAFE_XCM_VERSION;

const BRIDGE_HUB_WESTEND_ED: Balance = ExistentialDeposit::get();

fn bridge_hub_westend_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
	bridges_pallet_owner: Option<AccountId>,
	asset_hub_para_id: ParaId,
) -> serde_json::Value {
	serde_json::json!({
		"balances": BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1u128 << 60)).collect::<Vec<_>>(),
		},
		"parachainInfo": ParachainInfoConfig {
			parachain_id: id,
			..Default::default()
		},
		"collatorSelection": CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: BRIDGE_HUB_WESTEND_ED * 16,
			..Default::default()
		},
		"session": SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                         // account id
						acc,                                 // validator id
						SessionKeys { aura }, 		     // session keys
					)
				})
				.collect(),
			..Default::default()
		},
		"polkadotXcm": PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			..Default::default()
		},
		"bridgeRococoGrandpa": BridgeRococoGrandpaConfig {
			owner: bridges_pallet_owner.clone(),
			..Default::default()
		},
		"bridgeRococoMessages": BridgeRococoMessagesConfig {
			owner: bridges_pallet_owner.clone(),
			..Default::default()
		},
		"ethereumSystem": EthereumSystemConfig {
			para_id: id,
			asset_hub_para_id,
			..Default::default()
		}
	})
}

/// Encapsulates names of predefined presets.
mod preset_names {
	pub const PRESET_LOCAL: &str = "local";
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &sp_genesis_builder::PresetId) -> Option<sp_std::vec::Vec<u8>> {
	use preset_names::*;
	let patch = match id.try_into() {
		Ok(PRESET_LOCAL) => bridge_hub_westend_genesis(
			// initial collators.
			vec![
				(
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_collator_keys_from_seed::<AuraId>("Alice"),
				),
				(
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_collator_keys_from_seed::<AuraId>("Bob"),
				),
			],
			vec![
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_account_id_from_seed::<sr25519::Public>("Charlie"),
				get_account_id_from_seed::<sr25519::Public>("Dave"),
				get_account_id_from_seed::<sr25519::Public>("Eve"),
				get_account_id_from_seed::<sr25519::Public>("Ferdie"),
				get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
				get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
				get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
				get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
				get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
			],
			1002.into(),
			Some(get_account_id_from_seed::<sr25519::Public>("Bob")),
			westend_runtime_constants::system_parachain::ASSET_HUB_ID.into(),
		),
		Ok(sp_genesis_builder::DEV_RUNTIME_PRESET) => bridge_hub_westend_genesis(
			// initial collators.
			vec![
				(
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_collator_keys_from_seed::<AuraId>("Alice"),
				),
				(
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_collator_keys_from_seed::<AuraId>("Bob"),
				),
			],
			vec![
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_account_id_from_seed::<sr25519::Public>("Charlie"),
				get_account_id_from_seed::<sr25519::Public>("Dave"),
				get_account_id_from_seed::<sr25519::Public>("Eve"),
				get_account_id_from_seed::<sr25519::Public>("Ferdie"),
				get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
				get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
				get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
				get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
				get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
			],
			1002.into(),
			Some(get_account_id_from_seed::<sr25519::Public>("Bob")),
			westend_runtime_constants::system_parachain::ASSET_HUB_ID.into(),
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
	vec![PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET), PresetId::from(PRESET_LOCAL)]
}
