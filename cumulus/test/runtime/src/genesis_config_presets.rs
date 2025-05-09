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
	AuraConfig, BalancesConfig, ParachainInfoConfig, RuntimeGenesisConfig, SudoConfig,
	TestPalletConfig,
};
use alloc::{vec, vec::Vec};

use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

const DEV_RELAY_PARENT_OFFSET: &'static str = "relay_parent_offset_dev";
const DEV_ELASTIC_SCALING: &'static str = "dev_elastic_scaling";
const DEV_ELASTIC_SCALING_500MS: &'static str = "dev_elastic_scaling_500ms";
const DEV_ELASTIC_SCALING_MULTI_BLOCK_SLOT: &'static str = "dev_elastic_scaling_multi_block_slot";
const DEV_SYNC_BACKING: &'static str = "dev_sync_backing";

fn testnet_genesis_with_default_endowed(
	self_para_id: ParaId,
	relay_parent_offset: u32,
	velocity: u32,
	unincluded_segment: u32,
) -> serde_json::Value {
	let endowed = Sr25519Keyring::well_known().map(|x| x.to_account_id()).collect::<Vec<_>>();

	let invulnerables =
		Sr25519Keyring::invulnerable().map(|x| x.public().into()).collect::<Vec<_>>();

	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		sudo: SudoConfig { key: Some(Sr25519Keyring::Alice.public().into()) },
		parachain_info: ParachainInfoConfig { parachain_id: self_para_id },
		aura: AuraConfig { authorities: invulnerables },
		test_pallet: TestPalletConfig { relay_parent_offset, velocity, unincluded_segment }
	})
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
		PresetId::from(DEV_RELAY_PARENT_OFFSET),
		PresetId::from(DEV_ELASTIC_SCALING),
		PresetId::from(DEV_ELASTIC_SCALING_500MS),
		PresetId::from(DEV_ELASTIC_SCALING_MULTI_BLOCK_SLOT),
		PresetId::from(DEV_SYNC_BACKING),
	]
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let patch = match id.as_ref() {
		sp_genesis_builder::DEV_RUNTIME_PRESET |
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET =>
			testnet_genesis_with_default_endowed(100.into(), 0, 1, 3),
		DEV_RELAY_PARENT_OFFSET => testnet_genesis_with_default_endowed(100.into(), 2, 3, 14),
		DEV_ELASTIC_SCALING => testnet_genesis_with_default_endowed(100.into(), 0, 3, 8),
		DEV_ELASTIC_SCALING_500MS => testnet_genesis_with_default_endowed(100.into(), 0, 12, 26),
		DEV_ELASTIC_SCALING_MULTI_BLOCK_SLOT =>
			testnet_genesis_with_default_endowed(100.into(), 0, 6, 12),
		DEV_SYNC_BACKING => testnet_genesis_with_default_endowed(100.into(), 0, 1, 1),
		_ => return None,
	};
	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}
