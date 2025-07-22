// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

use polkadot_omni_node_lib::chain_spec::GenericChainSpec;
use sc_chain_spec::{ChainSpec, ChainType};
use std::{borrow::Cow, str::FromStr};

/// Collects all supported Coretime configurations.
#[derive(Debug, PartialEq, Clone, Copy)]
pub enum CoretimeRuntimeType {
	Kusama,
	KusamaLocal,

	Polkadot,
	PolkadotLocal,

	// Live
	Rococo,
	// Local
	RococoLocal,
	// Benchmarks
	RococoDevelopment,

	// Live
	Westend,
	// Local
	WestendLocal,
	// Benchmarks
	WestendDevelopment,
}

impl FromStr for CoretimeRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			kusama::CORETIME_KUSAMA => Ok(CoretimeRuntimeType::Kusama),
			kusama::CORETIME_KUSAMA_LOCAL => Ok(CoretimeRuntimeType::KusamaLocal),
			polkadot::CORETIME_POLKADOT => Ok(CoretimeRuntimeType::Polkadot),
			polkadot::CORETIME_POLKADOT_LOCAL => Ok(CoretimeRuntimeType::PolkadotLocal),
			rococo::CORETIME_ROCOCO => Ok(CoretimeRuntimeType::Rococo),
			rococo::CORETIME_ROCOCO_LOCAL => Ok(CoretimeRuntimeType::RococoLocal),
			rococo::CORETIME_ROCOCO_DEVELOPMENT => Ok(CoretimeRuntimeType::RococoDevelopment),
			westend::CORETIME_WESTEND => Ok(CoretimeRuntimeType::Westend),
			westend::CORETIME_WESTEND_LOCAL => Ok(CoretimeRuntimeType::WestendLocal),
			westend::CORETIME_WESTEND_DEVELOPMENT => Ok(CoretimeRuntimeType::WestendDevelopment),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl From<CoretimeRuntimeType> for &str {
	fn from(runtime_type: CoretimeRuntimeType) -> Self {
		match runtime_type {
			CoretimeRuntimeType::Kusama => kusama::CORETIME_KUSAMA,
			CoretimeRuntimeType::KusamaLocal => kusama::CORETIME_KUSAMA_LOCAL,
			CoretimeRuntimeType::Polkadot => polkadot::CORETIME_POLKADOT,
			CoretimeRuntimeType::PolkadotLocal => polkadot::CORETIME_POLKADOT_LOCAL,
			CoretimeRuntimeType::Rococo => rococo::CORETIME_ROCOCO,
			CoretimeRuntimeType::RococoLocal => rococo::CORETIME_ROCOCO_LOCAL,
			CoretimeRuntimeType::RococoDevelopment => rococo::CORETIME_ROCOCO_DEVELOPMENT,
			CoretimeRuntimeType::Westend => westend::CORETIME_WESTEND,
			CoretimeRuntimeType::WestendLocal => westend::CORETIME_WESTEND_LOCAL,
			CoretimeRuntimeType::WestendDevelopment => westend::CORETIME_WESTEND_DEVELOPMENT,
		}
	}
}

impl From<CoretimeRuntimeType> for ChainType {
	fn from(runtime_type: CoretimeRuntimeType) -> Self {
		match runtime_type {
			CoretimeRuntimeType::Kusama |
			CoretimeRuntimeType::Polkadot |
			CoretimeRuntimeType::Rococo |
			CoretimeRuntimeType::Westend => ChainType::Live,
			CoretimeRuntimeType::KusamaLocal |
			CoretimeRuntimeType::PolkadotLocal |
			CoretimeRuntimeType::RococoLocal |
			CoretimeRuntimeType::WestendLocal => ChainType::Local,
			CoretimeRuntimeType::RococoDevelopment | CoretimeRuntimeType::WestendDevelopment =>
				ChainType::Development,
		}
	}
}

impl CoretimeRuntimeType {
	pub const ID_PREFIX: &'static str = "coretime";

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			CoretimeRuntimeType::Kusama => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/coretime-kusama.json")[..],
			)?)),
			CoretimeRuntimeType::Polkadot => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/coretime-polkadot.json")[..],
			)?)),
			CoretimeRuntimeType::Rococo => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/coretime-rococo.json")[..],
			)?)),
			CoretimeRuntimeType::RococoLocal =>
				Ok(Box::new(rococo::local_config(*self, "rococo-local"))),
			CoretimeRuntimeType::RococoDevelopment =>
				Ok(Box::new(rococo::local_config(*self, "rococo-dev"))),
			CoretimeRuntimeType::Westend => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../../parachains/chain-specs/coretime-westend.json")[..],
			)?)),
			CoretimeRuntimeType::WestendLocal =>
				Ok(Box::new(westend::local_config(*self, "westend-local"))),
			CoretimeRuntimeType::WestendDevelopment =>
				Ok(Box::new(westend::local_config(*self, "westend-dev"))),
			other => Err(std::format!(
				"No default config present for {:?}, you should provide a chain-spec as json file!",
				other
			)),
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
	use super::{chain_type_name, CoretimeRuntimeType};
	use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
	use sc_chain_spec::ChainType;

	pub(crate) const CORETIME_ROCOCO: &str = "coretime-rococo";
	pub(crate) const CORETIME_ROCOCO_LOCAL: &str = "coretime-rococo-local";
	pub(crate) const CORETIME_ROCOCO_DEVELOPMENT: &str = "coretime-rococo-dev";

	pub fn local_config(runtime_type: CoretimeRuntimeType, relay_chain: &str) -> GenericChainSpec {
		// Rococo defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());

		let chain_type = runtime_type.into();
		let chain_name = format!("Coretime Rococo {}", chain_type_name(&chain_type));

		let wasm_binary = if matches!(chain_type, ChainType::Local | ChainType::Development) {
			coretime_rococo_runtime::fast_runtime_binary::WASM_BINARY
				.expect("WASM binary was not built, please build it!")
		} else {
			coretime_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!")
		};

		GenericChainSpec::builder(
			wasm_binary,
			Extensions::new_with_relay_chain(relay_chain.to_string()),
		)
		.with_name(&chain_name)
		.with_id(runtime_type.into())
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

/// Sub-module for Westend setup.
pub mod westend {
	use super::{chain_type_name, CoretimeRuntimeType, GenericChainSpec};
	use polkadot_omni_node_lib::chain_spec::Extensions;
	use sc_chain_spec::ChainType;

	pub(crate) const CORETIME_WESTEND: &str = "coretime-westend";
	pub(crate) const CORETIME_WESTEND_LOCAL: &str = "coretime-westend-local";
	pub(crate) const CORETIME_WESTEND_DEVELOPMENT: &str = "coretime-westend-dev";

	pub fn local_config(runtime_type: CoretimeRuntimeType, relay_chain: &str) -> GenericChainSpec {
		// westend defaults
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "WND".into());
		properties.insert("tokenDecimals".into(), 12.into());

		let chain_type = runtime_type.into();
		let chain_name = format!("Coretime Westend {}", chain_type_name(&chain_type));

		GenericChainSpec::builder(
			coretime_westend_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions::new_with_relay_chain(relay_chain.to_string()),
		)
		.with_name(&chain_name)
		.with_id(runtime_type.into())
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

pub mod kusama {
	pub(crate) const CORETIME_KUSAMA: &str = "coretime-kusama";
	pub(crate) const CORETIME_KUSAMA_LOCAL: &str = "coretime-kusama-local";
}

pub mod polkadot {
	pub(crate) const CORETIME_POLKADOT: &str = "coretime-polkadot";
	pub(crate) const CORETIME_POLKADOT_LOCAL: &str = "coretime-polkadot-local";
}
