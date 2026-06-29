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

use super::{
	test_pallet::InitialConsensusParameters, AccountId, AuraId, BalancesConfig,
	ParachainInfoConfig, RuntimeGenesisConfig, SudoConfig, TestPalletConfig,
};
use alloc::{vec, vec::Vec};

use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

#[cfg(not(feature = "with-authority-discovery"))]
use super::AuraConfig;
#[cfg(feature = "with-authority-discovery")]
use super::SessionConfig;

// Each string corresponds to one of the WASM variants this runtime used to ship as a separate
// build artifact. They are stable strings exposed to clients via the GenesisBuilder API and
// referenced from `cumulus/test/service/src/chain_spec.rs` to pick a preset by name.

/// Default async-backing preset: slot=6s, velocity=1, no relay-parent offset.
pub const ASYNC_BACKING_PRESET: &str = "async-backing";

/// Elastic-scaling preset: velocity=3 (MVP elastic scaling configuration).
pub const ELASTIC_SCALING_PRESET: &str = "elastic-scaling";

/// Elastic-scaling-500ms preset: velocity=12, producing one block every 500ms.
pub const ELASTIC_SCALING_500MS_PRESET: &str = "elastic-scaling-500ms";

/// Block-bundling preset: same throughput as elastic-scaling-500ms (velocity=12),
/// used by tests focused on block-bundling collation behaviour.
pub const BLOCK_BUNDLING_PRESET: &str = "block-bundling";

/// Sync-backing preset: slot=12s, velocity=1, AllowMultipleBlocksPerSlot=false.
pub const SYNC_BACKING_PRESET: &str = "sync-backing";

/// Relay-parent-offset preset: velocity=1 with relay_parent_offset=2.
pub const RELAY_PARENT_OFFSET_PRESET: &str = "relay-parent-offset";

/// Elastic-scaling with 12s slot duration: slot=12s, velocity=3.
pub const ELASTIC_SCALING_12S_SLOT_PRESET: &str = "elastic-scaling-12s-slot";

/// Async-backing with scheduling V3 enabled.
pub const ASYNC_BACKING_V3_PRESET: &str = "async-backing-v3";

/// Async-backing with scheduling V3 and relay_parent_offset=2.
pub const ASYNC_BACKING_V3_RPO_PRESET: &str = "async-backing-v3-rpo";

/// Elastic-scaling (velocity=3) with scheduling V3 enabled.
pub const ELASTIC_SCALING_V3_PRESET: &str = "elastic-scaling-v3";

/// Authority-discovery preset: matches the structural `with-authority-discovery` WASM
/// variant, which historically also implied relay_parent_offset=2.
pub const WITH_AUTHORITY_DISCOVERY_PRESET: &str = "with-authority-discovery";

/// Slot duration of 18s — used by zombienet tests that change slot timing via parameter
/// updates / runtime upgrades.
pub const SLOT_DURATION_18S_PRESET: &str = "slot-duration-18s";

/// Map a preset id to the consensus parameter overrides it implies.
///
/// Returns `None` for unknown preset ids. Returns `Some(Default::default())` for presets
/// whose values match the runtime defaults (no overrides needed — but still a known preset).
fn initial_consensus_parameters_for(id: &str) -> Option<InitialConsensusParameters> {
	Some(match id {
		// Default async-backing — all `None`, runtime defaults apply (velocity=1, slot=6s).
		ASYNC_BACKING_PRESET => InitialConsensusParameters::default(),

		ELASTIC_SCALING_PRESET => InitialConsensusParameters {
			block_processing_velocity: Some(3),
			..Default::default()
		},

		ELASTIC_SCALING_500MS_PRESET | BLOCK_BUNDLING_PRESET => InitialConsensusParameters {
			block_processing_velocity: Some(12),
			..Default::default()
		},

		SYNC_BACKING_PRESET => InitialConsensusParameters {
			slot_duration_millis: Some(12_000),
			allow_multiple_blocks_per_slot: Some(false),
			..Default::default()
		},

		RELAY_PARENT_OFFSET_PRESET =>
			InitialConsensusParameters { relay_parent_offset: Some(2), ..Default::default() },

		ELASTIC_SCALING_12S_SLOT_PRESET => InitialConsensusParameters {
			slot_duration_millis: Some(12_000),
			block_processing_velocity: Some(3),
			..Default::default()
		},

		ASYNC_BACKING_V3_PRESET => InitialConsensusParameters {
			scheduling_v3_enabled: Some(true),
			..Default::default()
		},

		ASYNC_BACKING_V3_RPO_PRESET => InitialConsensusParameters {
			scheduling_v3_enabled: Some(true),
			relay_parent_offset: Some(2),
			..Default::default()
		},

		ELASTIC_SCALING_V3_PRESET => InitialConsensusParameters {
			scheduling_v3_enabled: Some(true),
			block_processing_velocity: Some(3),
			..Default::default()
		},

		// The structural `with-authority-discovery` WASM historically pinned RPO=2 via
		// `flavors::relay_parent_offset()`. Preserve that here so the preset stays
		// behaviour-equivalent to the pre-refactor build.
		WITH_AUTHORITY_DISCOVERY_PRESET =>
			InitialConsensusParameters { relay_parent_offset: Some(2), ..Default::default() },

		SLOT_DURATION_18S_PRESET => InitialConsensusParameters {
			slot_duration_millis: Some(18_000),
			..Default::default()
		},

		_ => return None,
	})
}

/// Build the genesis config seeding `pallet_aura::authorities` directly.
///
/// This is the pre-upgrade path: no `pallet_session`, no `pallet_authority_discovery`.
#[cfg(not(feature = "with-authority-discovery"))]
fn cumulus_test_runtime(
	invulnerables: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
	initial_consensus_parameters: InitialConsensusParameters,
) -> serde_json::Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		sudo: SudoConfig { key: Some(Sr25519Keyring::Alice.public().into()) },
		parachain_info: ParachainInfoConfig { parachain_id: id },
		aura: AuraConfig { authorities: invulnerables },
		test_pallet: TestPalletConfig { initial_consensus_parameters },
	})
}

#[cfg(feature = "with-authority-discovery")]
fn cumulus_test_runtime(
	invulnerables: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
	initial_consensus_parameters: InitialConsensusParameters,
) -> serde_json::Value {
	use super::SessionKeys;
	use sp_authority_discovery::AuthorityId as AuthorityDiscoveryId;

	let session_keys: Vec<(AccountId, AccountId, SessionKeys)> = invulnerables
		.iter()
		.map(|aura| {
			// AuraId wraps sr25519::Public; convert to the inner type first.
			let inner: sp_core::sr25519::Public = aura.clone().into();
			let raw: [u8; 32] = inner.0;
			let account: AccountId = AccountId::from(raw);
			let aura_key: AuraId = aura.clone();
			let ad_key: AuthorityDiscoveryId = inner.into();
			(account.clone(), account, SessionKeys { aura: aura_key, authority_discovery: ad_key })
		})
		.collect();

	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		sudo: SudoConfig { key: Some(Sr25519Keyring::Alice.public().into()) },
		parachain_info: ParachainInfoConfig { parachain_id: id },
		session: SessionConfig { keys: session_keys, non_authority_keys: vec![] },
		test_pallet: TestPalletConfig { initial_consensus_parameters },
	})
}

fn testnet_genesis_with_default_endowed(
	self_para_id: ParaId,
	initial_consensus_parameters: InitialConsensusParameters,
) -> serde_json::Value {
	let endowed = Sr25519Keyring::well_known().map(|x| x.to_account_id()).collect::<Vec<_>>();

	let invulnerables =
		Sr25519Keyring::invulnerable().map(|x| x.public().into()).collect::<Vec<_>>();
	cumulus_test_runtime(invulnerables, endowed, self_para_id, initial_consensus_parameters)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
		PresetId::from(ASYNC_BACKING_PRESET),
		PresetId::from(ELASTIC_SCALING_PRESET),
		PresetId::from(ELASTIC_SCALING_500MS_PRESET),
		PresetId::from(BLOCK_BUNDLING_PRESET),
		PresetId::from(SYNC_BACKING_PRESET),
		PresetId::from(RELAY_PARENT_OFFSET_PRESET),
		PresetId::from(ELASTIC_SCALING_12S_SLOT_PRESET),
		PresetId::from(ASYNC_BACKING_V3_PRESET),
		PresetId::from(ASYNC_BACKING_V3_RPO_PRESET),
		PresetId::from(ELASTIC_SCALING_V3_PRESET),
		PresetId::from(WITH_AUTHORITY_DISCOVERY_PRESET),
		PresetId::from(SLOT_DURATION_18S_PRESET),
	]
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let id_str = id.as_ref();

	// The two well-known generic presets default to no consensus-parameter overrides
	// (runtime defaults apply: slot=6s, velocity=1, etc.).
	let initial_consensus_parameters = match id_str {
		sp_genesis_builder::DEV_RUNTIME_PRESET |
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => InitialConsensusParameters::default(),
		other => match initial_consensus_parameters_for(other) {
			Some(icp) => icp,
			None => return None,
		},
	};

	let patch = testnet_genesis_with_default_endowed(100.into(), initial_consensus_parameters);

	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}
