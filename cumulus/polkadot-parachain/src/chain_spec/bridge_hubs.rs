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

use cumulus_primitives_core::ParaId;
use polkadot_omni_node_lib::chain_spec::GenericChainSpec;
use sc_chain_spec::{ChainSpec, ChainType};
use std::str::FromStr;

/// Collects all supported BridgeHub configurations
#[derive(Debug, PartialEq)]
pub enum BridgeHubRuntimeType {
	Kusama,
	KusamaLocal,

	Polkadot,
	PolkadotLocal,

	Rococo,
	RococoLocal,
	// used by benchmarks
	RococoDevelopment,

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
			kusama::BRIDGE_HUB_KUSAMA => Ok(BridgeHubRuntimeType::Kusama),
			kusama::BRIDGE_HUB_KUSAMA_LOCAL => Ok(BridgeHubRuntimeType::KusamaLocal),
			westend::BRIDGE_HUB_WESTEND => Ok(BridgeHubRuntimeType::Westend),
			westend::BRIDGE_HUB_WESTEND_LOCAL => Ok(BridgeHubRuntimeType::WestendLocal),
			westend::BRIDGE_HUB_WESTEND_DEVELOPMENT => Ok(BridgeHubRuntimeType::WestendDevelopment),
			rococo::BRIDGE_HUB_ROCOCO => Ok(BridgeHubRuntimeType::Rococo),
			rococo::BRIDGE_HUB_ROCOCO_LOCAL => Ok(BridgeHubRuntimeType::RococoLocal),
			rococo::BRIDGE_HUB_ROCOCO_DEVELOPMENT => Ok(BridgeHubRuntimeType::RococoDevelopment),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl BridgeHubRuntimeType {
	pub const ID_PREFIX: &'static str = "bridge-hub";

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			BridgeHubRuntimeType::Polkadot => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/bridge-hub-polkadot.json")[..],
			)?)),
			BridgeHubRuntimeType::Kusama => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/bridge-hub-kusama.json")[..],
			)?)),
			BridgeHubRuntimeType::Westend => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/bridge-hub-westend.json")[..],
			)?)),
			BridgeHubRuntimeType::WestendLocal => Ok(Box::new(westend::local_config(
				westend::BRIDGE_HUB_WESTEND_LOCAL,
				"Westend BridgeHub Local",
				"westend-local",
				ParaId::new(1002),
				ChainType::Local,
			))),
			BridgeHubRuntimeType::WestendDevelopment => Ok(Box::new(westend::local_config(
				westend::BRIDGE_HUB_WESTEND_DEVELOPMENT,
				"Westend BridgeHub Development",
				"westend-dev",
				ParaId::new(1002),
				ChainType::Development,
			))),
			BridgeHubRuntimeType::Rococo => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/bridge-hub-rococo.json")[..],
			)?)),
			BridgeHubRuntimeType::RococoLocal => Ok(Box::new(rococo::local_config(
				rococo::BRIDGE_HUB_ROCOCO_LOCAL,
				"Rococo BridgeHub Local",
				"rococo-local",
				ParaId::new(1013),
				|_| (),
				ChainType::Local,
			))),
			BridgeHubRuntimeType::RococoDevelopment => Ok(Box::new(rococo::local_config(
				rococo::BRIDGE_HUB_ROCOCO_DEVELOPMENT,
				"Rococo BridgeHub Development",
				"rococo-dev",
				ParaId::new(1013),
				|_| (),
				ChainType::Development,
			))),
			other => Err(std::format!("No default config present for {:?}", other)),
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
	use super::{ChainType, ParaId};
	use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};

	pub(crate) const BRIDGE_HUB_ROCOCO: &str = "bridge-hub-rococo";
	pub(crate) const BRIDGE_HUB_ROCOCO_LOCAL: &str = "bridge-hub-rococo-local";
	pub(crate) const BRIDGE_HUB_ROCOCO_DEVELOPMENT: &str = "bridge-hub-rococo-dev";

	pub fn local_config<ModifyProperties: Fn(&mut sc_chain_spec::Properties)>(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
		modify_props: ModifyProperties,
		chain_type: ChainType,
	) -> GenericChainSpec {
		// Rococo defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());
		modify_props(&mut properties);

		GenericChainSpec::builder(
			bridge_hub_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
		.with_name(chain_name)
		.with_id(super::ensure_id(id).expect("invalid id"))
		.with_chain_type(chain_type.clone())
		.with_genesis_config_preset_name(match chain_type {
			ChainType::Development => sp_genesis_builder::DEV_RUNTIME_PRESET,
			ChainType::Local => sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET,
			_ => panic!("chain_type: {chain_type:?} not supported here!"),
		})
		.with_properties(properties)
		.build()
	}
}

/// Sub-module for Kusama setup
pub mod kusama {
	pub(crate) const BRIDGE_HUB_KUSAMA: &str = "bridge-hub-kusama";
	pub(crate) const BRIDGE_HUB_KUSAMA_LOCAL: &str = "bridge-hub-kusama-local";
}

/// Sub-module for Westend setup.
pub mod westend {
	use super::{ChainType, ParaId};
	use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};

	pub(crate) const BRIDGE_HUB_WESTEND: &str = "bridge-hub-westend";
	pub(crate) const BRIDGE_HUB_WESTEND_LOCAL: &str = "bridge-hub-westend-local";
	pub(crate) const BRIDGE_HUB_WESTEND_DEVELOPMENT: &str = "bridge-hub-westend-dev";

	pub fn local_config(
		id: &str,
		chain_name: &str,
		relay_chain: &str,
		para_id: ParaId,
		chain_type: ChainType,
	) -> GenericChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("tokenSymbol".into(), "WND".into());
		properties.insert("tokenDecimals".into(), 12.into());

		GenericChainSpec::builder(
			bridge_hub_westend_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
			Extensions { relay_chain: relay_chain.to_string(), para_id: para_id.into() },
		)
		.with_name(chain_name)
		.with_id(super::ensure_id(id).expect("invalid id"))
		.with_chain_type(chain_type.clone())
		.with_genesis_config_preset_name(match chain_type {
			ChainType::Development => sp_genesis_builder::DEV_RUNTIME_PRESET,
			ChainType::Local => sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET,
			_ => panic!("chain_type: {chain_type:?} not supported here!"),
		})
		.with_properties(properties)
		.build()
	}
}

/// Sub-module for Polkadot setup
pub mod polkadot {
	pub(crate) const BRIDGE_HUB_POLKADOT: &str = "bridge-hub-polkadot";
	pub(crate) const BRIDGE_HUB_POLKADOT_LOCAL: &str = "bridge-hub-polkadot-local";
}
