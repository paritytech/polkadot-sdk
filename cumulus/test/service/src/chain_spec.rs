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
use cumulus_test_runtime::{
	genesis_config_presets::{
		ASYNC_BACKING_PRESET, ASYNC_BACKING_V3_PRESET, ASYNC_BACKING_V3_RPO_PRESET,
		BLOCK_BUNDLING_PRESET, ELASTIC_SCALING_500MS_PRESET, ELASTIC_SCALING_PRESET,
		ELASTIC_SCALING_V3_PRESET, RELAY_PARENT_OFFSET_PRESET, SYNC_BACKING_PRESET,
		WITH_AUTHORITY_DISCOVERY_PRESET,
	},
	AccountId,
};
use sc_chain_spec::GenesisConfigBuilderRuntimeCaller;
use sc_service::{ChainType, GenericChainSpec};
use serde_json::json;

/// Get a chain spec for a specific parachain ID using a named GenesisBuilder preset.
///
/// `preset_id` selects which preset to instantiate (controls consensus parameters such as
/// slot duration, block-processing velocity, relay-parent offset, etc. — see
/// [`cumulus_test_runtime::genesis_config_presets`] for the full set). `extra_endowed_accounts`
/// are added on top of the preset's default endowments.
pub fn get_chain_spec_with_extra_endowed(
	id: Option<ParaId>,
	extra_endowed_accounts: Vec<AccountId>,
	code: &[u8],
	preset_id: &str,
) -> GenericChainSpec {
	let runtime_caller = GenesisConfigBuilderRuntimeCaller::<ParachainHostFunctions>::new(code);
	let mut development_preset = runtime_caller
		.get_named_preset(Some(&preset_id.to_string()))
		.unwrap_or_else(|_| panic!("preset `{preset_id}` is available on test runtime; qed"));

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
		},
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

	GenericChainSpec::builder(code, None)
		.with_name("Local Testnet")
		.with_id(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET)
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(development_preset)
		.build()
}

/// Resolve the default WASM blob, panicking with a helpful message if it was not built.
fn default_wasm() -> &'static [u8] {
	cumulus_test_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!")
}

/// Default chain spec — async-backing parachain with no consensus-parameter overrides.
pub fn get_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(id, Default::default(), default_wasm(), ASYNC_BACKING_PRESET)
}

/// Elastic-scaling chain spec (velocity = 3).
pub fn get_elastic_scaling_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		default_wasm(),
		ELASTIC_SCALING_PRESET,
	)
}

/// Chain spec with `relay_parent_offset = 2`.
pub fn get_relay_parent_offset_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		default_wasm(),
		RELAY_PARENT_OFFSET_PRESET,
	)
}

/// Elastic-scaling 500ms chain spec (velocity = 12).
pub fn get_elastic_scaling_500ms_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		default_wasm(),
		ELASTIC_SCALING_500MS_PRESET,
	)
}

/// Block-bundling chain spec (velocity = 12, same throughput as elastic-scaling 500ms).
pub fn get_block_bundling_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(id, Default::default(), default_wasm(), BLOCK_BUNDLING_PRESET)
}

/// Sync-backing chain spec (slot = 12s, `AllowMultipleBlocksPerSlot` = false).
pub fn get_sync_backing_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(id, Default::default(), default_wasm(), SYNC_BACKING_PRESET)
}

/// Async-backing chain spec with scheduling V3 enabled.
pub fn get_async_backing_v3_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		default_wasm(),
		ASYNC_BACKING_V3_PRESET,
	)
}

/// Async-backing chain spec with scheduling V3 and relay-parent offset = 2.
pub fn get_async_backing_v3_rpo_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		default_wasm(),
		ASYNC_BACKING_V3_RPO_PRESET,
	)
}

/// Elastic-scaling chain spec with scheduling V3 enabled (velocity = 3).
pub fn get_elastic_scaling_v3_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		default_wasm(),
		ELASTIC_SCALING_V3_PRESET,
	)
}

/// Async-backing chain spec — alias for the default `get_chain_spec`.
pub fn get_async_backing_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(id, Default::default(), default_wasm(), ASYNC_BACKING_PRESET)
}

/// Chain spec for the authority-discovery / collator-discovery test.
///
/// Uses the structural `with-authority-discovery` WASM variant which includes
/// `pallet_session` + `pallet_authority_discovery` and carries a higher `spec_version` (4 vs
/// 2) so a `set_code` upgrade from the default WASM triggers the `EnableAuthorityDiscovery`
/// migration. The matching preset additionally pins `relay_parent_offset = 2`, mirroring the
/// pre-refactor behaviour where the `with-authority-discovery` cargo feature implied RPO=2.
pub fn get_with_authority_discovery_chain_spec(id: Option<ParaId>) -> GenericChainSpec {
	get_chain_spec_with_extra_endowed(
		id,
		Default::default(),
		cumulus_test_runtime::with_authority_discovery::WASM_BINARY
			.expect("WASM binary was not built, please build it!"),
		WITH_AUTHORITY_DISCOVERY_PRESET,
	)
}
