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

use serde_json::Value;
use sp_genesis_builder::{self, PresetId};

use crate::{AccountId, Balances, BalancesConfig, RuntimeGenesisConfig, Sudo, SudoConfig};

fn testnet_genesis(endowed_accounts: Vec<AccountId>, root: AccountId) -> Value {
	let config = RuntimeGenesisConfig {
		//balances: Balances(vec![]),
		balances: BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, 1u128 << 60))
				.collect::<Vec<_>>(),
		},
		sudo: SudoConfig { key: Some(root) },
		..Default::default()
	};

	serde_json::to_value(config).expect("Could not build genesis config.")
}

fn development_config_genesis() -> Value {
	testnet_genesis(
		vec![
			sp_keyring::AccountKeyring::Alice.to_account_id(),
			sp_keyring::AccountKeyring::Bob.to_account_id(),
			sp_keyring::AccountKeyring::Bob.to_account_id(),
			sp_keyring::AccountKeyring::Dave.to_account_id(),
			sp_keyring::AccountKeyring::Ferdie.to_account_id(),
			sp_keyring::AccountKeyring::AliceStash.to_account_id(),
			sp_keyring::AccountKeyring::BobStash.to_account_id(),
			sp_keyring::AccountKeyring::CharlieStash.to_account_id(),
			sp_keyring::AccountKeyring::DaveStash.to_account_id(),
			sp_keyring::AccountKeyring::EveStash.to_account_id(),
			sp_keyring::AccountKeyring::FerdieStash.to_account_id(),
		],
		sp_keyring::AccountKeyring::Alice.to_account_id(),
	)
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let patch = match id.try_into() {
		Ok(sp_genesis_builder::DEV_RUNTIME_PRESET) => development_config_genesis(),
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
	vec![PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET)]
}
