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

use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
use sc_service::ChainType;

pub fn asset_hub_westend_development_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	GenericChainSpec::builder(
		asset_hub_westend_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
	.with_name("Westend Asset Hub Development")
	.with_id("asset-hub-westend-dev")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.with_properties(properties)
	.build()
}

pub fn asset_hub_westend_local_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	GenericChainSpec::builder(
		asset_hub_westend_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend-local".into(), para_id: 1000 },
	)
	.with_name("Westend Asset Hub Local")
	.with_id("asset-hub-westend-local")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_properties(properties)
	.build()
}

pub fn asset_hub_westend_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	GenericChainSpec::builder(
		asset_hub_westend_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
	.with_name("Westend Asset Hub")
	.with_id("asset-hub-westend")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name("genesis")
	.with_properties(properties)
	.build()
}

pub fn asset_hub_rococo_development_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 42.into());
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());
	asset_hub_rococo_like_development_config(
		properties,
		"Rococo Asset Hub Development",
		"asset-hub-rococo-dev",
		1000,
	)
}

fn asset_hub_rococo_like_development_config(
	properties: sc_chain_spec::Properties,
	name: &str,
	chain_id: &str,
	para_id: u32,
) -> GenericChainSpec {
	GenericChainSpec::builder(
		asset_hub_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo-dev".into(), para_id },
	)
	.with_name(name)
	.with_id(chain_id)
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
	.with_properties(properties)
	.build()
}

pub fn asset_hub_rococo_local_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 42.into());
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());
	asset_hub_rococo_like_local_config(
		properties,
		"Rococo Asset Hub Local",
		"asset-hub-rococo-local",
		1000,
	)
}

fn asset_hub_rococo_like_local_config(
	properties: sc_chain_spec::Properties,
	name: &str,
	chain_id: &str,
	para_id: u32,
) -> GenericChainSpec {
	GenericChainSpec::builder(
		asset_hub_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo-local".into(), para_id },
	)
	.with_name(name)
	.with_id(chain_id)
	.with_chain_type(ChainType::Local)
	.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_properties(properties)
	.build()
}

pub fn asset_hub_rococo_genesis_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());
	let para_id = 1000;
	GenericChainSpec::builder(
		asset_hub_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo".into(), para_id },
	)
	.with_name("Rococo Asset Hub")
	.with_id("asset-hub-rococo")
	.with_chain_type(ChainType::Live)
	.with_genesis_config_preset_name("genesis")
	.with_properties(properties)
	.build()
}
