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

use crate::chain_spec::get_account_id_from_seed;
use cumulus_primitives_core::ParaId;
use parachains_common::{AccountId, AuraId};
use polkadot_parachain_lib::chain_spec::{Extensions, GenericChainSpec};
use sc_service::ChainType;
use sp_core::sr25519;

use super::get_collator_keys_from_seed;

pub fn get_seedling_chain_spec() -> GenericChainSpec {
	GenericChainSpec::builder(
		seedling_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions { relay_chain: "westend".into(), para_id: 2000 },
	)
	.with_name("Seedling Local Testnet")
	.with_id("seedling_local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(seedling_testnet_genesis(
		get_account_id_from_seed::<sr25519::Public>("Alice"),
		2000.into(),
		vec![get_collator_keys_from_seed::<AuraId>("Alice")],
	))
	.with_boot_nodes(Vec::new())
	.build()
}

fn seedling_testnet_genesis(
	root_key: AccountId,
	parachain_id: ParaId,
	collators: Vec<AuraId>,
) -> serde_json::Value {
	serde_json::json!({
		"sudo": { "key": Some(root_key) },
		"parachainInfo":  {
			"parachainId": parachain_id,
		},
		"aura": { "authorities": collators },
	})
}
