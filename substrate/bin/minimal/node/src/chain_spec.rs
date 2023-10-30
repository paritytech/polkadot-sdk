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

use runtime::{BalancesConfig, SudoConfig, WASM_BINARY};
use sc_service::{ChainType, Properties};
use serde_json::{json, Value};
use sp_keyring::AccountKeyring;

/// This is a specialization of the general Substrate ChainSpec type.
pub type ChainSpec = sc_service::GenericChainSpec<()>;

fn props() -> Properties {
	let mut properties = Properties::new();
	properties.insert("tokenDecimals".to_string(), 0.into());
	properties.insert("tokenSymbol".to_string(), "MINI".into());
	properties
}

pub fn development_config() -> Result<ChainSpec, String> {
	Ok(ChainSpec::builder(WASM_BINARY.expect("Development wasm not available"), Default::default())
		.with_name("Development")
		.with_id("dev")
		.with_chain_type(ChainType::Development)
		.with_genesis_config_patch(testnet_genesis())
		.with_properties(props())
		.build())
}

/// Configure initial storage state for FRAME pallets.
fn testnet_genesis() -> Value {
	use frame::traits::Get;
	use runtime::interface::{Balance, MinimumBalance};
	let endowment = <MinimumBalance as Get<Balance>>::get().max(1) * 1000;
	let balances = AccountKeyring::iter()
		.map(|a| (a.to_account_id(), endowment))
		.collect::<Vec<_>>();
	json!({
		"balances": BalancesConfig { balances },
		"sudo": SudoConfig { key: Some(AccountKeyring::Alice.to_account_id()) },
	})
}

#[cfg(test)]
mod test {
	use super::*;
	use runtime::RuntimeGenesisConfig;
	pub type LegacyChainSpec = sc_service::GenericChainSpec<RuntimeGenesisConfig>;

	fn development_config_legacy() -> Result<LegacyChainSpec, String> {
		Ok(
			#[allow(deprecated)]
			LegacyChainSpec::from_genesis(
				"Development",
				"dev",
				ChainType::Development,
				move || testnet_genesis_legacy(),
				vec![],
				None,
				None,
				None,
				Some(props()),
				None,
				WASM_BINARY.ok_or_else(|| "Development wasm not available".to_string())?,
			),
		)
	}

	/// Configure initial storage state for FRAME pallets (legacy version).
	fn testnet_genesis_legacy() -> RuntimeGenesisConfig {
		use frame::traits::Get;
		use runtime::interface::{Balance, MinimumBalance};
		let endowment = <MinimumBalance as Get<Balance>>::get().max(1) * 1000;
		let balances = AccountKeyring::iter()
			.map(|a| (a.to_account_id(), endowment))
			.collect::<Vec<_>>();
		RuntimeGenesisConfig {
			balances: BalancesConfig { balances },
			sudo: SudoConfig { key: Some(AccountKeyring::Alice.to_account_id()) },
			..Default::default()
		}
	}

	#[test]
	fn legacy_vs_json_based_chainspec_check() {
		let j1 = development_config().unwrap().as_json(true).unwrap();
		let j2 = development_config_legacy().unwrap().as_json(true).unwrap();
		assert_eq!(j1, j2);
	}
}
