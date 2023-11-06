// Copyright (C) Parity Technologies (UK) Ltd.
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
use parachains_common::Balance as BridgeHubBalance;
use sc_chain_spec::ChainSpec;
use sp_core::sr25519;
use std::{path::PathBuf, str::FromStr};

/// Collects all supported BridgeHub configurations
#[derive(Debug, PartialEq)]
pub enum BridgeHubRuntimeType {
	Rococo,
	RococoLocal,
	// used by benchmarks
	RococoDevelopment,

	Wococo,
	WococoLocal,

	Kusama,
	KusamaLocal,
	// used by benchmarks
	KusamaDevelopment,

	Polkadot,
	PolkadotLocal,
	// used by benchmarks
	PolkadotDevelopment,

	Westend,
	WestendLocal,
	// used by benchmarks
	WestendDevelopment,
}

impl FromStr for BridgeHubRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			polkadot::BRIDGE_HUB_POLKADOT => Ok(BridgeHubRuntimeType::Polkadot),
			polkadot::BRIDGE_HUB_POLKADOT_LOCAL => Ok(BridgeHubRuntimeType::PolkadotLocal),
			polkadot::BRIDGE_HUB_POLKADOT_DEVELOPMENT =>
				Ok(BridgeHubRuntimeType::PolkadotDevelopment),
			kusama::BRIDGE_HUB_KUSAMA => Ok(BridgeHubRuntimeType::Kusama),
			kusama::BRIDGE_HUB_KUSAMA_LOCAL => Ok(BridgeHubRuntimeType::KusamaLocal),
			kusama::BRIDGE_HUB_KUSAMA_DEVELOPMENT => Ok(BridgeHubRuntimeType::KusamaDevelopment),
			westend::BRIDGE_HUB_WESTEND => Ok(BridgeHubRuntimeType::Westend),
			westend::BRIDGE_HUB_WESTEND_LOCAL => Ok(BridgeHubRuntimeType::WestendLocal),
			westend::BRIDGE_HUB_WESTEND_DEVELOPMENT => Ok(BridgeHubRuntimeType::WestendDevelopment),
			rococo::BRIDGE_HUB_ROCOCO => Ok(BridgeHubRuntimeType::Rococo),
			rococo::BRIDGE_HUB_ROCOCO_LOCAL => Ok(BridgeHubRuntimeType::RococoLocal),
			rococo::BRIDGE_HUB_ROCOCO_DEVELOPMENT => Ok(BridgeHubRuntimeType::RococoDevelopment),
			wococo::BRIDGE_HUB_WOCOCO => Ok(BridgeHubRuntimeType::Wococo),
			wococo::BRIDGE_HUB_WOCOCO_LOCAL => Ok(BridgeHubRuntimeType::WococoLocal),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl BridgeHubRuntimeType {
	pub const ID_PREFIX: &'static str = "bridge-hub";

	pub fn chain_spec_from_json_file(&self, path: PathBuf) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			BridgeHubRuntimeType::Polkadot |
			BridgeHubRuntimeType::PolkadotLocal |
			BridgeHubRuntimeType::PolkadotDevelopment =>
				Ok(Box::new(polkadot::BridgeHubChainSpec::from_json_file(path)?)),
			BridgeHubRuntimeType::Kusama |
			BridgeHubRuntimeType::KusamaLocal |
			BridgeHubRuntimeType::KusamaDevelopment =>
				Ok(Box::new(kusama::BridgeHubChainSpec::from_json_file(path)?)),
			BridgeHubRuntimeType::Westend |
			BridgeHubRuntimeType::WestendLocal |
			BridgeHubRuntimeType::WestendDevelopment =>
				Ok(Box::new(westend::BridgeHubChainSpec::from_json_file(path)?)),
			BridgeHubRuntimeType::Rococo |
			BridgeHubRuntimeType::RococoLocal |
			BridgeHubRuntimeType::RococoDevelopment =>
				Ok(Box::new(rococo::BridgeHubChainSpec::from_json_file(path)?)),
			BridgeHubRuntimeType::Wococo | BridgeHubRuntimeType::WococoLocal =>
				Ok(Box::new(wococo::BridgeHubChainSpec::from_json_file(path)?)),
		}
	}

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			BridgeHubRuntimeType::Polkadot =>
				Ok(Box::new(polkadot::BridgeHubChainSpec::from_json_bytes(
					&include_bytes!("../../chain-specs/bridge-hub-polkadot.json")[..],
				)?)),
			BridgeHubRuntimeType::PolkadotLocal => Ok(Box::new(polkadot::local_config(
				polkadot::BRIDGE_HUB_POLKADOT_LOCAL,
				"Polkadot BridgeHub Local",
				"polkadot-local",
				ParaId::new(1002),
			))),
			BridgeHubRuntimeType::PolkadotDevelopment => Ok(Box::new(polkadot::local_config(
				polkadot::BRIDGE_HUB_POLKADOT_DEVELOPMENT,
				"Polkadot BridgeHub Development",
				"polkadot-dev",
				ParaId::new(1002),
			))),
			BridgeHubRuntimeType::Kusama =>
				Ok(Box::new(kusama::BridgeHubChainSpec::from_json_bytes(
					&include_bytes!("../../chain-specs/bridge-hub-kusama.json")[..],
				)?)),
			BridgeHubRuntimeType::KusamaLocal => Ok(Box::new(kusama::local_config(
				kusama::BRIDGE_HUB_KUSAMA_LOCAL,
				"Kusama BridgeHub Local",
				"kusama-local",
				ParaId::new(1003),
			))),
			BridgeHubRuntimeType::KusamaDevelopment => Ok(Box::new(kusama::local_config(
				kusama::BRIDGE_HUB_KUSAMA_DEVELOPMENT,
				"Kusama BridgeHub Development",
				"kusama-dev",
				ParaId::new(1003),
			))),
			BridgeHubRuntimeType::Westend =>
				Ok(Box::new(westend::BridgeHubChainSpec::from_json_bytes(
					&include_bytes!("../../chain-specs/bridge-hub-westend.json")[..],
				)?)),
			BridgeHubRuntimeType::WestendLocal => Ok(Box::new(westend::local_config(
				westend::BRIDGE_HUB_WESTEND_LOCAL,
				"Westend BridgeHub Local",
				"westend-local",
				ParaId::new(1002),
				Some("Bob".to_string()),
			))),
			BridgeHubRuntimeType::WestendDevelopment => Ok(Box::new(westend::local_config(
				westend::BRIDGE_HUB_WESTEND_DEVELOPMENT,
				"Westend BridgeHub Development",
				"westend-dev",
				ParaId::new(1002),
				Some("Bob".to_string()),
			))),
			BridgeHubRuntimeType::Rococo =>
				Ok(Box::new(rococo::BridgeHubChainSpec::from_json_bytes(
					&include_bytes!("../../chain-specs/bridge-hub-rococo.json")[..],
				)?)),
			BridgeHubRuntimeType::RococoLocal => Ok(Box::new(rococo::local_config(
				rococo::BRIDGE_HUB_ROCOCO_LOCAL,
				"Rococo BridgeHub Local",
				"rococo-local",
				ParaId::new(1013),
				Some("Bob".to_string()),
				|_| (),
			))),
			BridgeHubRuntimeType::RococoDevelopment => Ok(Box::new(rococo::local_config(
				rococo::BRIDGE_HUB_ROCOCO_DEVELOPMENT,
				"Rococo BridgeHub Development",
				"rococo-dev",
				ParaId::new(1013),
				Some("Bob".to_string()),
				|_| (),
			))),
			BridgeHubRuntimeType::Wococo =>
				Ok(Box::new(wococo::BridgeHubChainSpec::from_json_bytes(
					&include_bytes!("../../chain-specs/bridge-hub-wococo.json")[..],
				)?)),
			BridgeHubRuntimeType::WococoLocal => Ok(Box::new(wococo::local_config(
				wococo::BRIDGE_HUB_WOCOCO_LOCAL,
				"Wococo BridgeHub Local",
				"wococo-local",
				ParaId::new(1014),
				Some("Bob".to_string()),
			))),
		}
	}
}

/// Check if 'id' satisfy BridgeHub-like format
fn ensure_id(id: &str) -> Result<&str, String> {
	if id.starts_with(BridgeHubRuntimeType::ID_PREFIX) {
		Ok(id)
	} else {
		Err(format!(
			"Invalid 'id' attribute ({}), should start with prefix: {}",
			id,
			BridgeHubRuntimeType::ID_PREFIX
		))
	}
}

/// Sub-module for Rococo setup
pub mod rococo {
	use super::{get_account_id_from_seed, get_collator_keys_from_seed, sr25519, ParaId};
	use crate::chain_spec::{Extensions, SAFE_XCM_VERSION};
	use parachains_common::{AccountId, AuraId};
	use sc_chain_spec::ChainType;

	use super::BridgeHubBalance;

	pub(crate) const BRIDGE_HUB_ROCOCO: &str = "bridge-hub-rococo";
	pub(crate) const BRIDGE_HUB_ROCOCO_LOCAL: &str = "bridge-hub-rococo-local";
	pub(crate) const BRIDGE_HUB_ROCOCO_DEVELOPMENT: &str = "bridge-hub-rococo-dev";
	const BRIDGE_HUB_ROCOCO_ED: BridgeHubBalance =
		parachains_common::rococo::currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type BridgeHubChainSpec = sc_service::GenericChainSpec<(), Extensions>;

	pub type RuntimeApi = bridge_hub_rococo_runtime::RuntimeApi;

	pub fn local_config<ModifyProperties: Fn(&mut sc_chain_spec::Properties)>(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
		bridges_pallet_owner_seed: Option<String>,
		modify_props: ModifyProperties,
	) -> BridgeHubChainSpec {
		// Rococo defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());
		modify_props(&mut properties);

		BridgeHubChainSpec::builder(
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
				"candidacyBond": BRIDGE_HUB_ROCOCO_ED * 16,
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

			"bridgeWococoGrandpa":  {
				"owner": bridges_pallet_owner.clone(),
			},
			"bridgeWestendGrandpa": {
				"owner": bridges_pallet_owner.clone(),
			},
			"bridgeRococoGrandpa": {
				"owner": bridges_pallet_owner.clone(),
			},
			"bridgeRococoMessages": {
				"owner": bridges_pallet_owner.clone(),
			},
			"bridgeWococoMessages": {
				"owner": bridges_pallet_owner.clone(),
			},
			"bridgeWestendMessages": {
				"owner": bridges_pallet_owner.clone(),
			},
		})
	}
}

/// Sub-module for Wococo setup (reuses stuff from Rococo)
pub mod wococo {
	use super::ParaId;
	use crate::chain_spec::bridge_hubs::rococo;

	pub(crate) const BRIDGE_HUB_WOCOCO: &str = "bridge-hub-wococo";
	pub(crate) const BRIDGE_HUB_WOCOCO_LOCAL: &str = "bridge-hub-wococo-local";

	pub type BridgeHubChainSpec = rococo::BridgeHubChainSpec;
	pub type RuntimeApi = rococo::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
		bridges_pallet_owner_seed: Option<String>,
	) -> BridgeHubChainSpec {
		rococo::local_config(
			id,
			chain_name,
			relay_chain,
			para_id,
			bridges_pallet_owner_seed,
			|properties| {
				properties.insert("tokenSymbol".into(), "WOOK".into());
			},
		)
	}
}

/// Sub-module for Kusama setup
pub mod kusama {
	use super::{BridgeHubBalance, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{AccountId, AuraId};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const BRIDGE_HUB_KUSAMA: &str = "bridge-hub-kusama";
	pub(crate) const BRIDGE_HUB_KUSAMA_LOCAL: &str = "bridge-hub-kusama-local";
	pub(crate) const BRIDGE_HUB_KUSAMA_DEVELOPMENT: &str = "bridge-hub-kusama-dev";
	const BRIDGE_HUB_KUSAMA_ED: BridgeHubBalance =
		parachains_common::kusama::currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type BridgeHubChainSpec = sc_service::GenericChainSpec<(), Extensions>;
	pub type RuntimeApi = bridge_hub_kusama_runtime::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> BridgeHubChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 2.into());
		properties.insert("tokenSymbol".into(), "KSM".into());
		properties.insert("tokenDecimals".into(), 12.into());

		BridgeHubChainSpec::builder(
			bridge_hub_kusama_runtime::WASM_BINARY
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
				"balances": endowed_accounts
					.iter()
					.cloned()
					.map(|k| (k, BRIDGE_HUB_KUSAMA_ED * 524_288))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": id,
			},
			"collatorSelection": {
				"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
				"candidacyBond": BRIDGE_HUB_KUSAMA_ED * 16,
			},
			"session": {
				"keys": invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                     // account id
							acc,                                             // validator id
							bridge_hub_kusama_runtime::SessionKeys { aura }, // session keys
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

/// Sub-module for Westend setup.
pub mod westend {
	use super::{get_account_id_from_seed, get_collator_keys_from_seed, sr25519, ParaId};
	use crate::chain_spec::{Extensions, SAFE_XCM_VERSION};
	use parachains_common::{AccountId, AuraId};
	use sc_chain_spec::ChainType;

	use super::BridgeHubBalance;

	pub(crate) const BRIDGE_HUB_WESTEND: &str = "bridge-hub-westend";
	pub(crate) const BRIDGE_HUB_WESTEND_LOCAL: &str = "bridge-hub-westend-local";
	pub(crate) const BRIDGE_HUB_WESTEND_DEVELOPMENT: &str = "bridge-hub-westend-dev";
	const BRIDGE_HUB_WESTEND_ED: BridgeHubBalance =
		parachains_common::westend::currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type BridgeHubChainSpec =
		sc_service::GenericChainSpec<bridge_hub_westend_runtime::RuntimeGenesisConfig, Extensions>;
	pub type RuntimeApi = bridge_hub_westend_runtime::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
		bridges_pallet_owner_seed: Option<String>,
	) -> BridgeHubChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("tokenSymbol".into(), "WND".into());
		properties.insert("tokenDecimals".into(), 12.into());

		BridgeHubChainSpec::builder(
			bridge_hub_westend_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
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
				"candidacyBond": BRIDGE_HUB_WESTEND_ED * 16,
			},
			"session": {
				"keys": invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                      // account id
							acc,                                              // validator id
							bridge_hub_westend_runtime::SessionKeys { aura }, // session keys
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
			"bridgeRococoMessages":  {
				"owner": bridges_pallet_owner.clone(),
			}
		})
	}
}

/// Sub-module for Polkadot setup
pub mod polkadot {
	use super::{BridgeHubBalance, ParaId};
	use crate::chain_spec::{
		get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
	};
	use parachains_common::{AccountId, AuraId};
	use sc_chain_spec::ChainType;
	use sp_core::sr25519;

	pub(crate) const BRIDGE_HUB_POLKADOT: &str = "bridge-hub-polkadot";
	pub(crate) const BRIDGE_HUB_POLKADOT_LOCAL: &str = "bridge-hub-polkadot-local";
	pub(crate) const BRIDGE_HUB_POLKADOT_DEVELOPMENT: &str = "bridge-hub-polkadot-dev";
	const BRIDGE_HUB_POLKADOT_ED: BridgeHubBalance =
		parachains_common::polkadot::currency::EXISTENTIAL_DEPOSIT;

	/// Specialized `ChainSpec` for the normal parachain runtime.
	pub type BridgeHubChainSpec = sc_service::GenericChainSpec<(), Extensions>;
	pub type RuntimeApi = bridge_hub_polkadot_runtime::RuntimeApi;

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
	) -> BridgeHubChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 0.into());
		properties.insert("tokenSymbol".into(), "DOT".into());
		properties.insert("tokenDecimals".into(), 10.into());

		BridgeHubChainSpec::builder(
			bridge_hub_polkadot_runtime::WASM_BINARY
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
				"balances": endowed_accounts
					.iter()
					.cloned()
					.map(|k| (k, BRIDGE_HUB_POLKADOT_ED * 4096))
					.collect::<Vec<_>>(),
			},
			"parachainInfo": {
				"parachainId": id,
			},
			"collatorSelection": {
				"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
				"candidacyBond": BRIDGE_HUB_POLKADOT_ED * 16,
			},
			"session": {
				"keys": invulnerables
					.into_iter()
					.map(|(acc, aura)| {
						(
							acc.clone(),                                       // account id
							acc,                                               // validator id
							bridge_hub_polkadot_runtime::SessionKeys { aura }, // session keys
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
