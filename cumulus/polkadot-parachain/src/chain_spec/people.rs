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
use std::str::FromStr;

/// Collects all supported People configurations.
#[derive(Debug, PartialEq)]
pub enum PeopleRuntimeType {
	Kusama,
	KusamaLocal,
	Polkadot,
	PolkadotLocal,
	Rococo,
	RococoLocal,
	RococoDevelopment,
	Westend,
	WestendLocal,
	WestendDevelopment,
}

impl FromStr for PeopleRuntimeType {
	type Err = String;

	fn from_str(value: &str) -> Result<Self, Self::Err> {
		match value {
			kusama::PEOPLE_KUSAMA => Ok(PeopleRuntimeType::Kusama),
			kusama::PEOPLE_KUSAMA_LOCAL => Ok(PeopleRuntimeType::KusamaLocal),
			polkadot::PEOPLE_POLKADOT => Ok(PeopleRuntimeType::Polkadot),
			polkadot::PEOPLE_POLKADOT_LOCAL => Ok(PeopleRuntimeType::PolkadotLocal),
			rococo::PEOPLE_ROCOCO => Ok(PeopleRuntimeType::Rococo),
			rococo::PEOPLE_ROCOCO_LOCAL => Ok(PeopleRuntimeType::RococoLocal),
			rococo::PEOPLE_ROCOCO_DEVELOPMENT => Ok(PeopleRuntimeType::RococoDevelopment),
			westend::PEOPLE_WESTEND => Ok(PeopleRuntimeType::Westend),
			westend::PEOPLE_WESTEND_LOCAL => Ok(PeopleRuntimeType::WestendLocal),
			westend::PEOPLE_WESTEND_DEVELOPMENT => Ok(PeopleRuntimeType::WestendDevelopment),
			_ => Err(format!("Value '{}' is not configured yet", value)),
		}
	}
}

impl PeopleRuntimeType {
	pub const ID_PREFIX: &'static str = "people";

	pub fn load_config(&self) -> Result<Box<dyn ChainSpec>, String> {
		match self {
			PeopleRuntimeType::Kusama => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/people-kusama.json")[..],
			)?)),
			PeopleRuntimeType::Polkadot => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/people-polkadot.json")[..],
			)?)),
			PeopleRuntimeType::Rococo => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/people-rococo.json")[..],
			)?)),
			PeopleRuntimeType::RococoLocal => Ok(Box::new(rococo::local_config(
				rococo::PEOPLE_ROCOCO_LOCAL,
				"Rococo People Local",
				"rococo-local",
				ChainType::Local,
			))),
			PeopleRuntimeType::RococoDevelopment => Ok(Box::new(rococo::local_config(
				rococo::PEOPLE_ROCOCO_DEVELOPMENT,
				"Rococo People Development",
				"rococo-development",
				ChainType::Development,
			))),
			PeopleRuntimeType::Westend => Ok(Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/people-westend.json")[..],
			)?)),
			PeopleRuntimeType::WestendLocal => Ok(Box::new(westend::local_config(
				westend::PEOPLE_WESTEND_LOCAL,
				"Westend People Local",
				"westend-local",
				ChainType::Local,
			))),
			PeopleRuntimeType::WestendDevelopment => Ok(Box::new(westend::local_config(
				westend::PEOPLE_WESTEND_DEVELOPMENT,
				"Westend People Development",
				"westend-development",
				ChainType::Development,
			))),
			other => Err(std::format!(
				"No default config present for {:?}, you should provide a chain-spec as json file!",
				other
			)),
		}
	}
}

/// Check if `id` satisfies People-like format.
fn ensure_id(id: &str) -> Result<&str, String> {
	if id.starts_with(PeopleRuntimeType::ID_PREFIX) {
		Ok(id)
	} else {
		Err(format!(
			"Invalid 'id' attribute ({}), should start with prefix: {}",
			id,
			PeopleRuntimeType::ID_PREFIX
		))
	}
}

/// Sub-module for Rococo setup.
pub mod rococo {
	use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
	use sc_chain_spec::ChainType;

	pub(crate) const PEOPLE_ROCOCO: &str = "people-rococo";
	pub(crate) const PEOPLE_ROCOCO_LOCAL: &str = "people-rococo-local";
	pub(crate) const PEOPLE_ROCOCO_DEVELOPMENT: &str = "people-rococo-dev";

	pub fn local_config(
		spec_id: &str,
		chain_name: &str,
		relay_chain: &str,
		chain_type: ChainType,
	) -> GenericChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "ROC".into());
		properties.insert("tokenDecimals".into(), 12.into());

		GenericChainSpec::builder(
			people_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions::new_with_relay_chain(relay_chain.to_string()),
		)
		.with_name(chain_name)
		.with_id(super::ensure_id(spec_id).expect("invalid id"))
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
	use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
	use sc_chain_spec::ChainType;

	pub(crate) const PEOPLE_WESTEND: &str = "people-westend";
	pub(crate) const PEOPLE_WESTEND_LOCAL: &str = "people-westend-local";
	pub(crate) const PEOPLE_WESTEND_DEVELOPMENT: &str = "people-westend-dev";

	pub fn local_config(
		spec_id: &str,
		chain_name: &str,
		relay_chain: &str,
		chain_type: ChainType,
	) -> GenericChainSpec {
		let mut properties = sc_chain_spec::Properties::new();
		properties.insert("ss58Format".into(), 42.into());
		properties.insert("tokenSymbol".into(), "WND".into());
		properties.insert("tokenDecimals".into(), 12.into());

		GenericChainSpec::builder(
			people_westend_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			Extensions::new_with_relay_chain(relay_chain.to_string()),
		)
		.with_name(chain_name)
		.with_id(super::ensure_id(spec_id).expect("invalid id"))
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
	pub(crate) const PEOPLE_KUSAMA: &str = "people-kusama";
	pub(crate) const PEOPLE_KUSAMA_LOCAL: &str = "people-kusama-local";
}

pub mod polkadot {
	pub(crate) const PEOPLE_POLKADOT: &str = "people-polkadot";
	pub(crate) const PEOPLE_POLKADOT_LOCAL: &str = "people-polkadot-local";
}
