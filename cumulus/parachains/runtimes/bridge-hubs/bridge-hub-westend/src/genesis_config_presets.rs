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
use parachains_common::{AccountId, AuraId};
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;
use testnet_parachains_constants::westend::xcm_version::SAFE_XCM_VERSION;

const BRIDGE_HUB_WESTEND_ED: Balance = ExistentialDeposit::get();

fn bridge_hub_westend_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
	bridges_pallet_owner: Option<AccountId>,
	asset_hub_para_id: ParaId,
	opened_bridges: Vec<(Location, InteriorLocation, Option<bp_messages::LegacyLaneId>)>,
) -> serde_json::Value {
	let config = RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, 1u128 << 60))
				.collect::<Vec<_>>(),
		},
		parachain_info: ParachainInfoConfig { parachain_id: id, ..Default::default() },
		collator_selection: CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: BRIDGE_HUB_WESTEND_ED * 16,
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
		bridge_rococo_grandpa: BridgeRococoGrandpaConfig {
			owner: bridges_pallet_owner.clone(),
			..Default::default()
		},
		bridge_rococo_messages: BridgeRococoMessagesConfig {
			owner: bridges_pallet_owner.clone(),
			..Default::default()
		},
		xcm_over_bridge_hub_rococo: XcmOverBridgeHubRococoConfig {
			opened_bridges,
			..Default::default()
		},
		ethereum_system: EthereumSystemConfig {
			para_id: id,
			asset_hub_para_id,
			..Default::default()
		},
		..Default::default()
	};

	serde_json::to_value(config).expect("Could not build genesis config.")
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &sp_genesis_builder::PresetId) -> Option<sp_std::vec::Vec<u8>> {
	let patch = match id.try_into() {
		Ok(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET) => bridge_hub_westend_genesis(
			// initial collators.
			vec![
				(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into()),
				(Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Bob.public().into()),
			],
			Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
			1002.into(),
			Some(Sr25519Keyring::Bob.to_account_id()),
			westend_runtime_constants::system_parachain::ASSET_HUB_ID.into(),
			vec![(
				Location::new(1, [Parachain(1000)]),
				Junctions::from([Rococo.into(), Parachain(1000)]),
				Some(bp_messages::LegacyLaneId([0, 0, 0, 2])),
			)],
		),
		Ok(sp_genesis_builder::DEV_RUNTIME_PRESET) => bridge_hub_westend_genesis(
			// initial collators.
			vec![
				(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into()),
				(Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Bob.public().into()),
			],
			Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
			1002.into(),
			Some(Sr25519Keyring::Bob.to_account_id()),
			westend_runtime_constants::system_parachain::ASSET_HUB_ID.into(),
			vec![],
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
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
	]
}
