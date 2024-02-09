// Copyright Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

use crate::chain_spec::GenericChainSpec;
use cumulus_primitives_core::ParaId;
use sc_chain_spec::{ChainSpec, ChainType};
use std::{borrow::Cow, str::FromStr};

/// Collects all supported Coretime configurations.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CoretimeRuntimeType {
	// Live
	Rococo,
	// Local
	RococoLocal,
	// Benchmarks
	RococoDevelopment,

	// Local
	WestendLocal,
	// Benchmarks
	WestendDevelopment,
}

impl FromStr for CoretimeRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			rococo::CORETIME_ROCOCO => Ok(CoretimeRuntimeType::Rococo),
			rococo::CORETIME_ROCOCO_LOCAL => Ok(CoretimeRuntimeType::RococoLocal),
			rococo::CORETIME_ROCOCO_DEVELOPMENT => Ok(CoretimeRuntimeType::RococoDevelopment),
			westend::CORETIME_WESTEND_LOCAL => Ok(CoretimeRuntimeType::WestendLocal),
			westend::CORETIME_WESTEND_DEVELOPMENT => Ok(CoretimeRuntimeType::WestendDevelopment),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl From<CoretimeRuntimeType> for &str {
	fn from(runtime_type: CoretimeRuntimeType) -> Self {
		match runtime_type {
			CoretimeRuntimeType::Rococo => rococo::CORETIME_ROCOCO,
			CoretimeRuntimeType::RococoLocal => rococo::CORETIME_ROCOCO_LOCAL,
			CoretimeRuntimeType::RococoDevelopment => rococo::CORETIME_ROCOCO_DEVELOPMENT,
			CoretimeRuntimeType::WestendLocal => westend::CORETIME_WESTEND_LOCAL,
			CoretimeRuntimeType::WestendDevelopment => westend::CORETIME_WESTEND_DEVELOPMENT,
		}
	}
}

impl From<CoretimeRuntimeType> for ChainType {
	fn from(runtime_type: CoretimeRuntimeType) -> Self {
		match runtime_type {
			CoretimeRuntimeType::Rococo => ChainType::Live,
			CoretimeRuntimeType::RococoLocal | CoretimeRuntimeType::WestendLocal =>
				ChainType::Local,
			CoretimeRuntimeType::RococoDevelopment | CoretimeRuntimeType::WestendDevelopment =>
				ChainType::Development,
		}
	}
}

pub const CORETIME_PARA_ID: ParaId = ParaId::new(1005);

impl CoretimeRuntimeType {
	pub const ID_PREFIX: &'static str = "coretime";

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Rococo => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../../parachains/chain-specs/coretime-rococo.json")[..],
			)?)),
			CoretimeRuntimeType::RococoLocal =>
				Ok(Box::new(rococo::local_config(*self, "rococo-local"))),
			CoretimeRuntimeType::RococoDevelopment =>
				Ok(Box::new(rococo::local_config(*self, "rococo-dev"))),
			CoretimeRuntimeType::WestendLocal =>
				Ok(Box::new(westend::local_config(*self, "westend-local"))),
			CoretimeRuntimeType::WestendDevelopment =>
				Ok(Box::new(westend::local_config(*self, "westend-dev"))),
		}
	}
}

/// Generate the name directly from the ChainType
pub fn chain_type_name(chain_type: &ChainType) -> Cow<str> {
	match chain_type {
		ChainType::Development => "Development",
		ChainType::Local => "Local",
		ChainType::Live => "Live",
		ChainType::Custom(name) => name,
	}
	.into()
}

/// Sub-module for Rococo setup.
pub mod rococo {
	use super::{chain_type_name, CoretimeRuntimeType, GenericChainSpec, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{AccountId, AuraId, Balance};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const CORETIME_ROCOCO: &str = "coretime-rococo";
	pub(crate) const CORETIME_ROCOCO_LOCAL: &str = "coretime-rococo-local";
	pub(crate) const CORETIME_ROCOCO_DEVELOPMENT: &str = "coretime-rococo-dev";
	const CORETIME_ROCOCO_ED: Balance = coretime_rococo_runtime::ExistentialDeposit::get();

	pub fn local_config(runtime_type: CoretimeRuntimeType, relay_chain: &str) -> GenericChainSpec {
		// Rococo defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());

		let chain_type = runtime_type.into();
		let chain_name = format!("Coretime Rococo {}", chain_type_name(&chain_type));
		let para_id = super::CORETIME_PARA_ID;

		let wasm_binary = if matches!(chain_type, ChainType::Local | ChainType::Development) {
			coretime_rococo_runtime::fast_runtime_binary::WASM_BINARY
				.expect("WASM binary was not built, please build it!")
		} else {
			coretime_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!")
		};

		GenericChainSpec::builder(
			wasm_binary,
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
		.with_name(&chain_name)
		.with_id(runtime_type.into())
		.with_chain_type(chain_type)
		.with_genesis_config_patch(genesis(
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
			para_id,
		))
		.with_properties(properties)
		.build()
	}

	fn genesis(
		invulnerables: Vec<(AccountId, AuraId)>,
		endowed_accounts: Vec<AccountId>,
		id: ParaId,
	) -> serde_json::Value {
		serde_json::json!({
			"balances": {
				"balances": endowed_accounts.iter().cloned().map(|k| (k, CORETIME_ROCOCO_ED * 4096)).collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": id,
			},
			"collatorSelection": {
				"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
				"candidacyBond": CORETIME_ROCOCO_ED * 16,
			},
			"session": {
				"keys": invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                   // account id
							acc,                                           // validator id
							coretime_rococo_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
			"sudo": {
				"key": Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
			},
		})
	}
}

/// Sub-module for Westend setup.
pub mod westend {
	use super::{chain_type_name, CoretimeRuntimeType, GenericChainSpec, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{AccountId, AuraId, Balance};
	use sp_core::sr25519;

	pub(crate) const CORETIME_WESTEND_LOCAL: &str = "coretime-westend-local";
	pub(crate) const CORETIME_WESTEND_DEVELOPMENT: &str = "coretime-westend-dev";
	const CORETIME_WESTEND_ED: Balance = coretime_westend_runtime::ExistentialDeposit::get();

	pub fn local_config(runtime_type: CoretimeRuntimeType, relay_chain: &str) -> GenericChainSpec {
		// westend defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "WND".into());
		properties.insert("tokenDecimals".into(), 12.into());

		let chain_type = runtime_type.into();
		let chain_name = format!("Coretime Westend {}", chain_type_name(&chain_type));
		let para_id = super::CORETIME_PARA_ID;

		GenericChainSpec::builder(
			coretime_westend_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
		.with_name(&chain_name)
		.with_id(runtime_type.into())
		.with_chain_type(chain_type)
		.with_genesis_config_patch(genesis(
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
			para_id,
		))
		.with_properties(properties)
		.build()
	}

	fn genesis(
		invulnerables: Vec<(AccountId, AuraId)>,
		endowed_accounts: Vec<AccountId>,
		id: ParaId,
	) -> serde_json::Value {
		serde_json::json!({
			"balances": {
				"balances": endowed_accounts.iter().cloned().map(|k| (k, CORETIME_WESTEND_ED * 4096)).collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": id,
			},
			"collatorSelection": {
				"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
				"candidacyBond": CORETIME_WESTEND_ED * 16,
			},
			"session": {
				"keys": invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                    // account id
							acc,                                            // validator id
							coretime_westend_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			}
		})
	}
}
