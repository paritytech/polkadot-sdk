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

use crate::chain_spec::{get_account_id_from_seed, Extensions, GenericChainSpec};
use cumulus_primitives_core::ParaId;
use parachains_common::AuraId;
use sc_service::ChainType;
use sp_core::sr25519;

use super::get_collator_keys_from_seed;

fn glutton_genesis(parachain_id: ParaId, collators: Vec<AuraId>) -> serde_json::Value {
	serde_json::json!( {
		"parachainInfo": {
			"parachainId": parachain_id
		},
		"sudo": {
			"key": Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
		},
		"aura": { "authorities": collators },
	})
}

pub fn glutton_westend_development_config(para_id: ParaId) -> GenericChainSpec {
	GenericChainSpec::builder(
		glutton_westend_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend-dev".into(), para_id: para_id.into() },
	)
	.with_name("Glutton Development")
	.with_id("glutton_westend_dev")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(glutton_genesis(
		para_id,
		vec![get_collator_keys_from_seed::<AuraId>("Alice")],
	))
	.build()
}

pub fn glutton_westend_local_config(para_id: ParaId) -> GenericChainSpec {
	GenericChainSpec::builder(
		glutton_westend_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend-local".into(), para_id: para_id.into() },
	)
	.with_name("Glutton Local")
	.with_id("glutton_westend_local")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(glutton_genesis(
		para_id,
		vec![
			get_collator_keys_from_seed::<AuraId>("Alice"),
			get_collator_keys_from_seed::<AuraId>("Bob"),
		],
	))
	.build()
}

pub fn glutton_westend_config(para_id: ParaId) -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 42.into());

	GenericChainSpec::builder(
		glutton_westend_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend".into(), para_id: para_id.into() },
	)
	.with_name(format!("Glutton {}", para_id).as_str())
	.with_id(format!("glutton-westend-{}", para_id).as_str())
	.with_chain_type(ChainType::Live)
	.with_genesis_config_patch(glutton_westend_genesis(
		para_id,
		vec![
			get_collator_keys_from_seed::<AuraId>("Alice"),
			get_collator_keys_from_seed::<AuraId>("Bob"),
		],
	))
	.with_protocol_id(format!("glutton-westend-{}", para_id).as_str())
	.with_properties(properties)
	.build()
}

fn glutton_westend_genesis(parachain_id: ParaId, collators: Vec<AuraId>) -> serde_json::Value {
	serde_json::json!( {
		"parachainInfo": {
			"parachainId": parachain_id
		},
		"sudo": {
			"key": Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
		},
		"aura": { "authorities": collators },
	})
}
