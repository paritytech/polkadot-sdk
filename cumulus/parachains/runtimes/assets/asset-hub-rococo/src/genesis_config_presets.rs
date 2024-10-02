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

//! # Asset Hub Rococo Runtime genesis config presets

use crate::*;
use alloc::{vec, vec::Vec};
use cumulus_primitives_core::ParaId;
use hex_literal::hex;
use parachains_common::{genesis_config_helpers::*, AccountId, AuraId};
use sp_core::{crypto::UncheckedInto, sr25519};
use sp_genesis_builder::PresetId;
use testnet_parachains_constants::rococo::{currency::UNITS as ROC, xcm_version::SAFE_XCM_VERSION};

const ASSET_HUB_ROCOCO_ED: Balance = ExistentialDeposit::get();

fn asset_hub_rococo_genesis(
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
			candidacy_bond: ASSET_HUB_ROCOCO_ED * 16,
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
		Ok(PRESET_GENESIS) => asset_hub_rococo_genesis(
			// initial collators.
			vec![
				// E8XC6rTJRsioKCp6KMy6zd24ykj4gWsusZ3AkSeyavpVBAG
				(
					hex!("44cb62d1d6cdd2fff2a5ef3bb7ef827be5b3e117a394ecaa634d8dd9809d5608").into(),
					hex!("44cb62d1d6cdd2fff2a5ef3bb7ef827be5b3e117a394ecaa634d8dd9809d5608")
						.unchecked_into(),
				),
				// G28iWEybndgGRbhfx83t7Q42YhMPByHpyqWDUgeyoGF94ri
				(
					hex!("9864b85e23aa4506643db9879c3dbbeabaa94d269693a4447f537dd6b5893944").into(),
					hex!("9864b85e23aa4506643db9879c3dbbeabaa94d269693a4447f537dd6b5893944")
						.unchecked_into(),
				),
				// G839e2eMiq7UXbConsY6DS1XDAYG2XnQxAmLuRLGGQ3Px9c
				(
					hex!("9ce5741ee2f1ac3bdedbde9f3339048f4da2cb88ddf33a0977fa0b4cf86e2948").into(),
					hex!("9ce5741ee2f1ac3bdedbde9f3339048f4da2cb88ddf33a0977fa0b4cf86e2948")
						.unchecked_into(),
				),
				// GLao4ukFUW6qhexuZowdFrKa2NLCfnEjZMftSXXfvGv1vvt
				(
					hex!("a676ed15f5a325eab49ed8d5f8c00f3f814b19bb58cda14ad10894c078dd337f").into(),
					hex!("a676ed15f5a325eab49ed8d5f8c00f3f814b19bb58cda14ad10894c078dd337f")
						.unchecked_into(),
				),
			],
			Vec::new(),
			ASSET_HUB_ROCOCO_ED * 524_288,
			1000.into(),
		),
		Ok(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET) => asset_hub_rococo_genesis(
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
			ROC * 1_000_000,
			1000.into(),
		),
		Ok(sp_genesis_builder::DEV_RUNTIME_PRESET) => asset_hub_rococo_genesis(
			// initial collators.
			vec![(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_collator_keys_from_seed::<AuraId>("Alice"),
			)],
			vec![
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
				get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			],
			ROC * 1_000_000,
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
