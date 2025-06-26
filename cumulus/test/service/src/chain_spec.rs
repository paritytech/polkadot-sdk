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

use cumulus_client_service::ParachainHostFunctions;
use cumulus_primitives_core::ParaId;
use cumulus_test_runtime::AccountId;
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup, GenesisConfigBuilderRuntimeCaller};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use serde_json::json;

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
	let runtime_caller = GenesisConfigBuilderRuntimeCaller::<ParachainHostFunctions>::new(code);
	let mut development_preset = runtime_caller
		.get_named_preset(Some(&sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET.to_string()))
		.expect("development preset is available on test runtime; qed");

	// Extract existing balances
	let existing_balances = development_preset
		.get("balances")
		.and_then(|b| b.get("balances"))
		.and_then(|b| b.as_array())
		.cloned()
		.unwrap_or_default();

	// Create new balances by combining existing and extra accounts
	let mut all_balances = existing_balances;
	all_balances.extend(extra_endowed_accounts.into_iter().map(|a| json!([a, 1u64 << 60])));

	let mut patch_json = json!({
		"balances": {
			"balances": all_balances,
		}
	});

	if let Some(id) = id {
		// Merge parachain ID if given, otherwise use the one from the preset.
		sc_chain_spec::json_merge(
			&mut patch_json,
			json!({
				"parachainInfo": {
					"parachainId": id,
				},
			}),
		);
	};

	sc_chain_spec::json_merge(&mut development_preset, patch_json.into());

	ChainSpec::builder(
		code,
		Extensions { para_id: id.unwrap_or(cumulus_test_runtime::PARACHAIN_ID.into()).into() },
	)
	.with_name("Local Testnet")
	.with_id(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(development_preset)
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

/// Get the chain spec for a specific parachain ID.
pub fn get_relay_parent_offset_chain_spec(id: Option<ParaId>) -> ChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::relay_parent_offset::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
	)
}
/// Get the chain spec for a specific parachain ID.
pub fn get_elastic_scaling_500ms_chain_spec(id: Option<ParaId>) -> ChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::elastic_scaling_500ms::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
	)
}

/// Get the chain spec for a specific parachain ID.
pub fn get_elastic_scaling_mvp_chain_spec(id: Option<ParaId>) -> ChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::elastic_scaling_mvp::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
	)
}

pub fn get_elastic_scaling_multi_block_slot_chain_spec(id: Option<ParaId>) -> ChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::elastic_scaling_multi_block_slot::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
	)
}

pub fn get_sync_backing_chain_spec(id: Option<ParaId>) -> ChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::sync_backing::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
	)
}
