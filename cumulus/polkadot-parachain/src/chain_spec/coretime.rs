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
use sc_chain_spec::ChainSpec;
use std::str::FromStr;

/// Collects all supported Coretime configurations.
#[derive(Debug, PartialEq)]
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

impl CoretimeRuntimeType {
	pub const ID_PREFIX: &'static str = "coretime";

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Rococo => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../../parachains/chain-specs/coretime-rococo.json")[..],
			)?)),
			CoretimeRuntimeType::RococoLocal => Ok(Box::new(rococo::local_config(
				rococo::CORETIME_ROCOCO_DEVELOPMENT,
				"Rococo Coretime Local",
				"rococo-local",
				ParaId::new(1005),
			))),
			CoretimeRuntimeType::RococoDevelopment => Ok(Box::new(rococo::local_config(
				rococo::CORETIME_ROCOCO_DEVELOPMENT,
				"Rococo Coretime Development",
				"rococo-dev",
				ParaId::new(1005),
			))),
			CoretimeRuntimeType::WestendLocal => Ok(Box::new(westend::local_config(
				westend::CORETIME_WESTEND_DEVELOPMENT,
				"Westend Coretime Local",
				"westend-local",
				ParaId::new(1005),
			))),
			CoretimeRuntimeType::WestendDevelopment => Ok(Box::new(westend::local_config(
				westend::CORETIME_WESTEND_DEVELOPMENT,
				"Westend Coretime Development",
				"westend-dev",
				ParaId::new(1005),
			))),
		}
	}
}

/// Check if `id` satisfies Coretime-like format.
fn ensure_id(id: &str) -> Result<&str, String> {
	if id.starts_with(CoretimeRuntimeType::ID_PREFIX) {
		Ok(id)
	} else {
		Err(format!(
			"Invalid 'id' attribute ({}), should start with prefix: {}",
			id,
			CoretimeRuntimeType::ID_PREFIX
		))
	}
}

/// Sub-module for Rococo setup.
pub mod rococo {
	use super::{GenericChainSpec, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{AccountId, AuraId, Balance};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const CORETIME_ROCOCO: &str = "coretime-rococo";
	pub(crate) const CORETIME_ROCOCO_LOCAL: &str = "coretime-rococo-local";
	pub(crate) const CORETIME_ROCOCO_DEVELOPMENT: &str = "coretime-rococo-dev";
	const CORETIME_ROCOCO_ED: Balance = parachains_common::rococo::currency::EXISTENTIAL_DEPOSIT;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> GenericChainSpec {
		// Rococo defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());

		GenericChainSpec::builder(
			coretime_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
		.with_name(chain_name)
		.with_id(super::ensure_id(id).expect("invalid id"))
		.with_chain_type(ChainType::Local)
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
	use super::{GenericChainSpec, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{AccountId, AuraId, Balance};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const CORETIME_WESTEND_LOCAL: &str = "coretime-westend-local";
	pub(crate) const CORETIME_WESTEND_DEVELOPMENT: &str = "coretime-westend-dev";
	const CORETIME_WESTEND_ED: Balance = parachains_common::westend::currency::EXISTENTIAL_DEPOSIT;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> GenericChainSpec {
		// westend defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "WND".into());
		properties.insert("tokenDecimals".into(), 12.into());

		GenericChainSpec::builder(
			coretime_westend_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
		.with_name(chain_name)
		.with_id(super::ensure_id(id).expect("invalid id"))
		.with_chain_type(ChainType::Local)
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
