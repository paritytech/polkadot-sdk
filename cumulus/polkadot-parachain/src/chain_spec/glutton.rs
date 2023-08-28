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

use crate::chain_spec::{get_account_id_from_seed, Extensions};
use cumulus_primitives_core::ParaId;
use sc_service::ChainType;
use sp_core::sr25519;

/// Specialized `ChainSpec` for the Glutton parachain runtime.
pub type GluttonChainSpec = sc_service::GenericChainSpec<(), Extensions>;

pub fn glutton_development_config(para_id: ParaId) -> GluttonChainSpec {
	#[allow(deprecated)]
	GluttonChainSpec::builder()
		.with_name("Glutton Development")
		.with_id("glutton_dev")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(glutton_genesis(para_id))
		.with_extensions(Extensions { relay_chain: "kusama-dev".into(), para_id: para_id.into() })
		.with_code(
			glutton_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
		)
		.build()
}

pub fn glutton_local_config(para_id: ParaId) -> GluttonChainSpec {
	#[allow(deprecated)]
	GluttonChainSpec::builder()
		.with_name("Glutton Local")
		.with_id("glutton_local")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(glutton_genesis(para_id))
		.with_extensions(Extensions { relay_chain: "kusama-local".into(), para_id: para_id.into() })
		.with_code(
			glutton_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
		)
		.build()
}

pub fn glutton_config(para_id: ParaId) -> GluttonChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());

	GluttonChainSpec::builder()
		.with_name(format!("Glutton {}", para_id).as_str())
		.with_id(format!("glutton-kusama-{}", para_id).as_str())
		.with_chain_type(ChainType::Live)
		.with_genesis_config_patch(glutton_genesis(para_id))
		.with_protocol_id(format!("glutton-kusama-{}", para_id).as_str())
		.with_properties(properties)
		.with_extensions(Extensions { relay_chain: "kusama".into(), para_id: para_id.into() })
		.with_code(
			glutton_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
		)
		.build()
}

fn glutton_genesis(parachain_id: ParaId) -> serde_json::Value {
	serde_json::json!( {
		"parachainInfo": {
			"parachainId": parachain_id
		},
		"sudo": {
			"key": Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
		}
	})
}
