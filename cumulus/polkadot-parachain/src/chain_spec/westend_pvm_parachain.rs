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

//! ChainSpecs dedicated to Rococo parachain setups (for testing and example purposes)

use cumulus_primitives_core::ParaId;
use parachains_common::AccountId;
use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
use rococo_parachain_runtime::AuraId;
use sc_chain_spec::ChainType;
use sp_keyring::Sr25519Keyring;

pub fn westend_pvm_parachain_local_config() -> GenericChainSpec {
	GenericChainSpec::builder(
		westend_pvm_parachain_runtime::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		Extensions::new_with_relay_chain("westend-local".into()),
	)
	.with_name("Westend PVM Parachain Local")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(testnet_genesis(
		Sr25519Keyring::Alice.to_account_id(),
		vec![
			AuraId::from(Sr25519Keyring::Alice.public()),
			AuraId::from(Sr25519Keyring::Bob.public()),
		],
		Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
		1000.into(),
	))
	.build()
}

pub(crate) fn testnet_genesis(
	root_key: AccountId,
	initial_authorities: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> serde_json::Value {
	serde_json::json!({
		"balances": {
			"balances": endowed_accounts.iter().cloned().map(|k| (k, 1u64 << 60)).collect::<Vec<_>>(),
		},
		"sudo": { "key": Some(root_key) },
		"parachainInfo": {
			"parachainId": id,
		},
		"aura": { "authorities": initial_authorities },
	})
}
