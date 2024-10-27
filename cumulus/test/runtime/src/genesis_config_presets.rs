// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use super::{
	AccountId, AuraConfig, AuraId, BalancesConfig, ParachainInfoConfig, RuntimeGenesisConfig,
	SudoConfig,
};
use alloc::{vec, vec::Vec};

use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

fn cumulus_test_runtime(
	invulnerables: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> serde_json::Value {
	let config = build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		sudo: SudoConfig { key: Some(Sr25519Keyring::Alice.public().into()) },
		parachain_info: ParachainInfoConfig { parachain_id: id },
		aura: AuraConfig { authorities: invulnerables },
	});
	serde_json::to_value(config).expect("Could not build genesis config.")
}

fn testnet_genesis_with_default_endowed(self_para_id: ParaId) -> serde_json::Value {
	let endowed = Sr25519Keyring::well_known().map(|x| x.to_account_id()).collect::<Vec<_>>();

	let invulnerables =
		Sr25519Keyring::invulnerable().map(|x| x.public().into()).collect::<Vec<_>>();
	cumulus_test_runtime(invulnerables, endowed, self_para_id)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
	]
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let patch = match id.try_into() {
		Ok(sp_genesis_builder::DEV_RUNTIME_PRESET) |
		Ok(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET) =>
			testnet_genesis_with_default_endowed(100.into()),
		_ => return None,
	};
	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}
