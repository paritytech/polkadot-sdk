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
use parachains_common::AccountId;
use sc_service::ChainType;
use sp_core::sr25519;

/// Specialized `ChainSpec` for the seedling parachain runtime.
pub type SeedlingChainSpec = sc_service::GenericChainSpec<(), Extensions>;

pub fn get_seedling_chain_spec() -> SeedlingChainSpec {
	SeedlingChainSpec::builder()
		.with_name("Seedling Local Testnet")
		.with_id("seedling_local_testnet")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(seedling_testnet_genesis(
			get_account_id_from_seed::<sr25519::Public>("Alice"),
			2000.into(),
		))
		.with_boot_nodes(Vec::new())
		.with_extensions(Extensions { relay_chain: "westend".into(), para_id: 2000 })
		.with_code(
			seedling_runtime::WASM_BINARY.expect("WASM binary was not build, please build it!"),
		)
		.build()
}

fn seedling_testnet_genesis(root_key: AccountId, parachain_id: ParaId) -> serde_json::Value {
	serde_json::json!({
		"sudo": { "key": Some(root_key) },
		"parachainInfo":  {
			"parachainId": parachain_id,
		},
	})
}
