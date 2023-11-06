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

use crate::chain_spec::{get_account_id_from_seed, Extensions};
use cumulus_primitives_core::ParaId;
use parachains_common::AuraId;
use sc_service::ChainType;
use sp_core::sr25519;

use super::get_collator_keys_from_seed;

/// Specialized `ChainSpec` for the Glutton parachain runtime.
pub type GluttonChainSpec = sc_service::GenericChainSpec<(), Extensions>;
pub type GluttonWestendChainSpec = sc_service::GenericChainSpec<(), Extensions>;

pub fn glutton_development_config(para_id: ParaId) -> GluttonChainSpec {
	GluttonChainSpec::builder(
		glutton_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "kusama-dev".into(), para_id: para_id.into() },
	)
	.with_name("Glutton Development")
	.with_id("glutton_dev")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(glutton_genesis(
		para_id,
		vec![get_collator_keys_from_seed::<AuraId>("Alice")],
	))
	.build()
}

pub fn glutton_local_config(para_id: ParaId) -> GluttonChainSpec {
	GluttonChainSpec::builder(
		glutton_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "kusama-local".into(), para_id: para_id.into() },
	)
	.with_name("Glutton Local")
	.with_id("glutton_local")
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

pub fn glutton_config(para_id: ParaId) -> GluttonChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());

	GluttonChainSpec::builder(
		glutton_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "kusama".into(), para_id: para_id.into() },
	)
	.with_name(format!("Glutton {}", para_id).as_str())
	.with_id(format!("glutton-kusama-{}", para_id).as_str())
	.with_chain_type(ChainType::Live)
	.with_genesis_config_patch(glutton_genesis(
		para_id,
		vec![
			get_collator_keys_from_seed::<AuraId>("Alice"),
			get_collator_keys_from_seed::<AuraId>("Bob"),
		],
	))
	.with_protocol_id(format!("glutton-kusama-{}", para_id).as_str())
	.with_properties(properties)
	.build()
}

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

pub fn glutton_westend_development_config(para_id: ParaId) -> GluttonWestendChainSpec {
	GluttonWestendChainSpec::from_genesis(
		// Name
		"Glutton Development",
		// ID
		"glutton_westend_dev",
		ChainType::Local,
		move || {
			glutton_westend_genesis(para_id, vec![get_collator_keys_from_seed::<AuraId>("Alice")])
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "westend-dev".into(), para_id: para_id.into() },
	)
}

pub fn glutton_westend_local_config(para_id: ParaId) -> GluttonWestendChainSpec {
	GluttonWestendChainSpec::from_genesis(
		// Name
		"Glutton Westend Local",
		// ID
		"glutton_westend_local",
		ChainType::Local,
		move || {
			glutton_westend_genesis(
				para_id,
				vec![
					get_collator_keys_from_seed::<AuraId>("Alice"),
					get_collator_keys_from_seed::<AuraId>("Bob"),
				],
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "westend-local".into(), para_id: para_id.into() },
	)
}

pub fn glutton_westend_config(para_id: ParaId) -> GluttonWestendChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 42.into());

	GluttonWestendChainSpec::from_genesis(
		// Name
		format!("Glutton Westend {}", para_id).as_str(),
		// ID
		format!("glutton-westend-{}", para_id).as_str(),
		ChainType::Live,
		move || {
			glutton_westend_genesis(
				para_id,
				vec![
					get_collator_keys_from_seed::<AuraId>("Alice"),
					get_collator_keys_from_seed::<AuraId>("Bob"),
				],
			)
		},
		Vec::new(),
		None,
		// Protocol ID
		Some(format!("glutton-westend-{}", para_id).as_str()),
		None,
		Some(properties),
		Extensions { relay_chain: "westend".into(), para_id: para_id.into() },
	)
}

fn glutton_westend_genesis(
	parachain_id: ParaId,
	collators: Vec<AuraId>,
) -> glutton_westend_runtime::RuntimeGenesisConfig {
	glutton_westend_runtime::RuntimeGenesisConfig {
		system: glutton_westend_runtime::SystemConfig {
			code: glutton_westend_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
			..Default::default()
		},
		parachain_info: glutton_westend_runtime::ParachainInfoConfig {
			parachain_id,
			..Default::default()
		},
		parachain_system: Default::default(),
		glutton: glutton_westend_runtime::GluttonConfig {
			compute: Default::default(),
			storage: Default::default(),
			trash_data_count: Default::default(),
			..Default::default()
		},
		aura: glutton_westend_runtime::AuraConfig { authorities: collators },
		aura_ext: Default::default(),
		sudo: glutton_westend_runtime::SudoConfig {
			key: Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
		},
	}
}
