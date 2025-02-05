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

use cumulus_primitives_core::ParaId;
use polkadot_omni_node_lib::chain_spec::GenericChainSpec;
use sc_chain_spec::{ChainType, GenericChainSpec};

pub const CORETIME_PARA_ID: ParaId = ParaId::new(1005);

pub fn coretime_westend_development_config() -> GenericChainSpec {
	GenericChainSpec::builder(
		coretime_westend_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend".into(), para_id: CORETIME_PARA_ID },
	)
		.with_name("Westend Coretime Development")
		.with_id("coretime-westend-dev")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
		.with_properties(westend_properties())
		.build()
}

pub fn coretime_westend_local_config() -> GenericChainSpec {
	GenericChainSpec::builder(
		coretime_westend_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend-local".into(), para_id: CORETIME_PARA_ID },
	)
		.with_name("Westend Coretime Local")
		.with_id("coretime-westend-local")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
		.with_properties(westend_properties())
		.build()
}

pub fn coretime_rococo_development_config() -> GenericChainSpec {
	GenericChainSpec::builder(
		coretime_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo-dev".into(), CORETIME_PARA_ID },
	)
		.with_name("Rococo Coretime Development")
		.with_id("coretime-rococo-dev")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_preset_name(sp_genesis_builder::DEV_RUNTIME_PRESET)
		.with_properties(rococo_properties())
		.build()
}

pub fn coretime_rococo_local_config() -> GenericChainSpec {
	GenericChainSpec::builder(
		coretime_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "rococo-local".into(), CORETIME_PARA_ID },
	)
		.with_name("Rococo Coretime Local")
		.with_id("coretime-rococo-local")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_preset_name(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
		.with_properties(rococo_properties())
		.build()
}

pub fn westend_properties() -> sc_chain_spec::Properties {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	properties
}

pub fn rococo_properties() -> sc_chain_spec::Properties {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 42.into());
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());

	properties
}