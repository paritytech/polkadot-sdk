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

//! Rococo Parachain Runtime genesis config presets

use crate::*;
use alloc::{vec, vec::Vec};
use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use parachains_common::{AccountId, AuraId};
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

const DEFAULT_PARA_ID: ParaId = ParaId::new(1000);
const ENDOWMENT: u128 = 1 << 60;

fn rococo_parachain_genesis(
	root_key: AccountId,
	initial_authorities: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
	id: ParaId,
) -> serde_json::Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		aura: AuraConfig { authorities: initial_authorities },
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, endowment)).collect(),
		},
		parachain_info: ParachainInfoConfig { parachain_id: id },
		polkadot_xcm: PolkadotXcmConfig { safe_xcm_version: Some(SAFE_XCM_VERSION) },
		sudo: SudoConfig { key: Some(root_key) }
	})
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let genesis_fn = |authorities| {
		rococo_parachain_genesis(
			Sr25519Keyring::Alice.to_account_id(),
			authorities,
			Sr25519Keyring::well_known().map(|x| x.to_account_id()).collect(),
			ENDOWMENT,
			DEFAULT_PARA_ID,
		)
	};

	let patch = match id.as_ref() {
		sp_genesis_builder::DEV_RUNTIME_PRESET =>
			genesis_fn(vec![Sr25519Keyring::Alice.public().into()]),
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => genesis_fn(vec![
			Sr25519Keyring::Alice.public().into(),
			Sr25519Keyring::Bob.public().into(),
		]),
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
