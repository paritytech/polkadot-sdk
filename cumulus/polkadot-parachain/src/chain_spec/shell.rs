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

use crate::chain_spec::{Extensions, GenericChainSpec};
use cumulus_primitives_core::ParaId;
use parachains_common::AuraId;
use sc_service::ChainType;

use super::get_collator_keys_from_seed;

pub fn get_shell_chain_spec() -> GenericChainSpec {
	GenericChainSpec::builder(
		shell_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
	.with_name("Shell Local Testnet")
	.with_id("shell_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(shell_testnet_genesis(
		1000.into(),
		vec![get_collator_keys_from_seed::<AuraId>("Alice")],
	))
	.with_boot_nodes(Vec::new())
	.build()
}

fn shell_testnet_genesis(parachain_id: ParaId, collators: Vec<AuraId>) -> serde_json::Value {
	serde_json::json!({
		"parachainInfo": { "parachainId": parachain_id},
		"aura": { "authorities": collators },
	})
}
