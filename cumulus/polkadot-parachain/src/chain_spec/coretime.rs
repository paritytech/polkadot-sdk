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

use crate::chain_spec::{get_account_id_from_seed, get_collator_keys_from_seed};
use cumulus_primitives_core::ParaId;
use parachains_common::Balance as CoretimeBalance;
use sc_chain_spec::ChainSpec;
use sp_core::sr25519;
use std::{path::PathBuf, str::FromStr};

/// Collects all supported Coretime configurations.
#[derive(Debug, PartialEq)]
pub enum CoretimeRuntimeType {
	Rococo,
	RococoLocal,
	Westend,
}

impl FromStr for CoretimeRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			rococo::CORETIME_ROCOCO => Ok(CoretimeRuntimeType::Rococo),
			rococo::CORETIME_ROCOCO_LOCAL => Ok(CoretimeRuntimeType::RococoLocal),
			westend::CORETIME_WESTEND => Ok(CoretimeRuntimeType::Westend),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl CoretimeRuntimeType {
	pub const ID_PREFIX: &'static str = "coretime";

	pub fn chain_spec_from_json_file(&self, path: PathBuf) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Rococo =>
				Ok(Box::new(rococo::CoretimeChainSpec::from_json_file(path)?)),
			CoretimeRuntimeType::Westend =>
				Ok(Box::new(westend::CoretimeChainSpec::from_json_file(path)?)),
			_ =>
				Err("Chain spec from json file is not supported for this runtime type".to_string()),
		}
	}

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Rococo =>
				Ok(Box::new(rococo::CoretimeChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/coretime-rococo.json")[..],
				)?)),
			CoretimeRuntimeType::RococoLocal => Ok(Box::new(rococo::local_config(
				rococo::CORETIME_ROCOCO_LOCAL,
				"Coretime Rococo Local",
				"rococo-local",
				ParaId::new(1004),
				Some("Bob".to_string()),
				|_| (),
			))),
			CoretimeRuntimeType::Westend =>
				Ok(Box::new(westend::CoretimeChainSpec::from_json_bytes(
					&include_bytes!("../../../parachains/chain-specs/coretime-westend.json")[..],
				)?)),
		}
	}
}

/// Check if 'id' satisfy Coretime-like format
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
	use super::{
		get_account_id_from_seed, get_collator_keys_from_seed, sr25519, CoretimeBalance, ParaId,
	};
	use crate::chain_spec::{Extensions, SAFE_XCM_VERSION};
	use parachains_common::{AccountId, AuraId};
	use sc_chain_spec::ChainType;

	pub(crate) const CORETIME_ROCOCO: &str = "coretime-rococo";
	pub(crate) const CORETIME_ROCOCO_LOCAL: &str = "coretime-rococo-local";
	const CORETIME_ROCOCO_ED: CoretimeBalance =
		parachains_common::rococo::currency::EXISTENTIAL_DEPOSIT;
	pub type CoretimeChainSpec =
		sc_service::GenericChainSpec<coretime_rococo_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = coretime_rococo_runtime::RuntimeApi;

	pub fn local_config<ModifyProperties: Fn(&mut sc_chain_spec::Properties)>(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
		bridges_pallet_owner_seed: Option<String>,
		modify_props: ModifyProperties,
	) -> CoretimeChainSpec {
		// Rococo defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());
		modify_props(&mut properties);

		CoretimeChainSpec::builder(
			bridge_hub_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
		.with_name(chain_name)
		.with_id(super::ensure_id(id).expect("invalid id"))
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(genesis(
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
			para_id,
			bridges_pallet_owner_seed
				.as_ref()
				.map(|seed| get_account_id_from_seed::<sr25519::Public>(seed)),
		))
		.with_properties(properties)
		.build()
	}

	fn genesis(
		invulnerables: Vec<(AccountId, AuraId)>,
		endowed_accounts: Vec<AccountId>,
		id: ParaId,
		bridges_pallet_owner: Option<AccountId>,
	) -> serde_json::Value {
		serde_json::json!({
			"balances": {
				"balances": endowed_accounts.iter().cloned().map(|k| (k, 1u64 << 60)).collect::<Vec<_>>(),
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
							acc.clone(),                                     // account id
							acc,                                             // validator id
							bridge_hub_rococo_runtime::SessionKeys { aura }, // session keys
						)
					})
					.collect::<Vec<_>>(),
			},
			"polkadotXcm": {
				"safeXcmVersion": Some(SAFE_XCM_VERSION),
			},
			"bridgeRococoGrandpa": {
				"owner": bridges_pallet_owner.clone(),
			},
			"bridgeRococoMessages": {
				"owner": bridges_pallet_owner.clone(),
			},
		})
	}
}

/// Sub-module for Westend setup.
pub mod westend {
	use crate::chain_spec::Extensions;

	pub(crate) const CORETIME_WESTEND: &str = "coretime-westend";
	pub type CoretimeChainSpec =
		sc_service::GenericChainSpec<coretime_westend_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = coretime_westend_runtime::RuntimeApi;
}
