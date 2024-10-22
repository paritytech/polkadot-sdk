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

#![allow(missing_docs)]

use cumulus_primitives_core::ParaId;
use cumulus_test_runtime::AccountId;
use parachains_common::AuraId;
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use sp_keyring::Sr25519Keyring;

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type ChainSpec = sc_service::GenericChainSpec<Extensions>;

/// The extensions for the [`ChainSpec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
#[serde(deny_unknown_fields)]
pub struct Extensions {
	/// The id of the Parachain.
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

/// Get the chain spec for a specific parachain ID.
/// The given accounts are initialized with funds in addition
/// to the default known accounts.
pub fn get_chain_spec_with_extra_endowed(
	id: Option<ParaId>,
	extra_endowed_accounts: Vec<AccountId>,
	code: &[u8],
) -> ChainSpec {
	ChainSpec::builder(
		code,
		Extensions { para_id: id.unwrap_or(cumulus_test_runtime::PARACHAIN_ID.into()).into() },
	)
	.with_name("Local Testnet")
	.with_id("local_testnet")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(testnet_genesis_with_default_endowed(
		extra_endowed_accounts.clone(),
		id,
	))
	.build()
}

/// Get the chain spec for a specific parachain ID.
pub fn get_chain_spec(id: Option<ParaId>) -> ChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
	)
}

/// Get the chain spec for a specific parachain ID.
pub fn get_elastic_scaling_chain_spec(id: Option<ParaId>) -> ChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::elastic_scaling::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
	)
}

/// Local testnet genesis for testing.
pub fn testnet_genesis_with_default_endowed(
	mut extra_endowed_accounts: Vec<AccountId>,
	self_para_id: Option<ParaId>,
) -> serde_json::Value {
	let mut endowed = Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect::<Vec<_>>();
	endowed.append(&mut extra_endowed_accounts);
	let invulnerables =
		Sr25519Keyring::invulnerable().map(|k| k.public().into()).collect::<Vec<_>>();
	testnet_genesis(Sr25519Keyring::Alice.to_account_id(), invulnerables, endowed, self_para_id)
}

/// Creates a local testnet genesis with endowed accounts.
pub fn testnet_genesis(
	root_key: AccountId,
	invulnerables: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	self_para_id: Option<ParaId>,
) -> serde_json::Value {
	let self_para_id = self_para_id.unwrap_or(cumulus_test_runtime::PARACHAIN_ID.into());
	serde_json::json!({
		"balances": cumulus_test_runtime::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		"sudo": cumulus_test_runtime::SudoConfig { key: Some(root_key) },
		"parachainInfo": {
			"parachainId": self_para_id,
		},
		"aura": cumulus_test_runtime::AuraConfig { authorities: invulnerables }
	})
}
