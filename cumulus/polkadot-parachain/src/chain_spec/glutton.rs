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
use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
use sc_service::ChainType;
use sp_keyring::Sr25519Keyring;

/// Generic Glutton Westend Config for all currently used setups.
pub fn glutton_westend_config(
	para_id: ParaId,
	chain_type: ChainType,
	relay_chain: &str,
) -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 42.into());

	let authorities = match chain_type {
		ChainType::Development => vec![Sr25519Keyring::Alice.public().into()],
		// Even the westend live setup does currently use Alice & Bob.
		_ => vec![Sr25519Keyring::Alice.public().into(), Sr25519Keyring::Bob.public().into()],
	};

	GenericChainSpec::builder(
		glutton_westend_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: relay_chain.into(), para_id: para_id.into() },
	)
	.with_name(&chain_type_name(para_id, &chain_type))
	.with_id(&chain_id(para_id, &chain_type))
	.with_chain_type(chain_type)
	.with_genesis_config_patch(
		glutton_westend_runtime::genesis_config_presets::glutton_westend_genesis(
			authorities,
			Some(Sr25519Keyring::Alice.to_account_id()),
			para_id,
		),
	)
	.build()
}

/// Generate the name directly from the ChainType
fn chain_type_name(para_id: ParaId, chain_type: &ChainType) -> String {
	match chain_type {
		ChainType::Development => format!("Glutton Development {}", para_id),
		ChainType::Local => format!("Glutton Local {}", para_id),
		ChainType::Live => format!("Glutton {}", para_id),
		ChainType::Custom(name) => name.clone(),
	}
}

/// Generate the name directly from the ChainType
pub fn chain_id(para_id: ParaId, chain_type: &ChainType) -> String {
	match chain_type {
		ChainType::Development => format!("glutton-westend-dev-{}", para_id),
		ChainType::Local => format!("glutton-westend-local-{}", para_id),
		ChainType::Live => format!("glutton-westend-{}", para_id),
		ChainType::Custom(_) => format!("glutton-westend-custom-{}", para_id),
	}
}
