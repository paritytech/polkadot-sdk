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

use runtime::{BalancesConfig, RuntimeGenesisConfig, SudoConfig, SystemConfig, WASM_BINARY};
use sc_service::{ChainType, Properties};
use sp_keyring::AccountKeyring;

/// This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<RuntimeGenesisConfig>;

fn props() -> Properties {
	let mut properties = Properties::new();
	properties.insert("tokenDecimals".to_string(), 0.into());
	properties.insert("tokenSymbol".to_string(), "MINI".into());
	properties
}

pub fn development_config() -> Result<ChainSpec, String> {
	let wasm_binary = WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?;
	Ok(ChainSpec::from_genesis(
		"Development",
		"dev",
		ChainType::Development,
		move || testnet_genesis(wasm_binary),
		vec![],
		None,
		None,
		None,
		Some(props()),
		None,
	))
}

/// Configure initial storage state for FRAME pallets.
fn testnet_genesis(wasm_binary: &[u8]) -> RuntimeGenesisConfig {
	use frame::traits::Get;
	use runtime::interface::{Balance, MinimumBalance};
	let endowment = <MinimumBalance as Get<Balance>>::get().max(1) * 1000;
	let balances = AccountKeyring::iter()
		.map(|a| (a.to_account_id(), endowment))
		.collect::<Vec<_>>();
	RuntimeGenesisConfig {
		system: SystemConfig {
			// Add Wasm runtime to storage.
			code: wasm_binary.to_vec(),
			_config: Default::default(),
		},
		balances: BalancesConfig { balances },
		sudo: SudoConfig { key: Some(AccountKeyring::Alice.to_account_id()) },
		..Default::default()
	}
}
