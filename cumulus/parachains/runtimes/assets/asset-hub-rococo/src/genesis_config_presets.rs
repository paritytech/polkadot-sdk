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

//! Genesis configs presets for the AssetHubRococo runtime

use crate::*;
use sp_std::vec::Vec;
use testnet_parachains_constants::{genesis_presets::*, rococo::currency::UNITS as ROC};

const ASSET_HUB_ROCOCO_ED: Balance = ExistentialDeposit::get();

/// Default genesis pallet configurations for AssetHubRococo
pub fn asset_hub_rococo_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
	id: ParaId,
) -> serde_json::Value {
	serde_json::json!({
		"balances": BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, endowment))
				.collect(),
		},
		"parachainInfo": ParachainInfoConfig {
			parachain_id: id,
			..Default::default()
		},
		"collatorSelection": CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: ASSET_HUB_ROCOCO_ED * 16,
			..Default::default()
		},
		"session": SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                         // account id
						acc,                                 // validator id
						SessionKeys { aura }, 			// session keys
					)
				})
				.collect(),
		},
		"polkadotXcm": PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
			..Default::default()
		}
	})
}

/// Default genesis setup for `local_testnet` preset id.
pub fn asset_hub_rococo_local_testnet_genesis(para_id: ParaId) -> serde_json::Value {
	asset_hub_rococo_genesis(invulnerables(), testnet_accounts(), ROC * 1_000_000, para_id)
}

/// Default genesis setup for `development` preset id.
pub fn asset_hub_rococo_development_genesis(para_id: ParaId) -> serde_json::Value {
	asset_hub_rococo_local_testnet_genesis(para_id)
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &sp_genesis_builder::PresetId) -> Option<sp_std::vec::Vec<u8>> {
	let patch = match id.try_into() {
		Ok("development") => asset_hub_rococo_development_genesis(1000.into()),
		Ok("local_testnet") => asset_hub_rococo_local_testnet_genesis(1000.into()),
		_ => {
			log::error!(target: "bencher::FAIL-CI", "get_preset: None!?");
			return None
		},
	};
	log::error!(target: "bencher::FAIL-CI", "get_preset: {patch:?}");
	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}
