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
	AccountId, AuraConfig, AuraId, BalancesConfig, ParachainInfoConfig, RuntimeGenesisConfig,
	SudoConfig, TestPalletConfig,
};
use alloc::{vec, vec::Vec};

use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

const DEV_RELAY_PARENT_OFFSET: &'static str = "relay_parent_offset_dev";

fn cumulus_test_runtime(
	invulnerables: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
	relay_parent_offset: u32,
) -> serde_json::Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		sudo: SudoConfig { key: Some(Sr25519Keyring::Alice.public().into()) },
		parachain_info: ParachainInfoConfig { parachain_id: id },
		aura: AuraConfig { authorities: invulnerables },
		test_pallet: TestPalletConfig { relay_parent_offset }
	})
}

fn testnet_genesis_with_default_endowed(
	self_para_id: ParaId,
	relay_parent_offset: u32,
) -> serde_json::Value {
	let endowed = Sr25519Keyring::well_known().map(|x| x.to_account_id()).collect::<Vec<_>>();

	let invulnerables =
		Sr25519Keyring::invulnerable().map(|x| x.public().into()).collect::<Vec<_>>();
	cumulus_test_runtime(invulnerables, endowed, self_para_id, relay_parent_offset)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
		PresetId::from(DEV_RELAY_PARENT_OFFSET),
	]
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let patch = match id.as_ref() {
		sp_genesis_builder::DEV_RUNTIME_PRESET |
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET =>
			testnet_genesis_with_default_endowed(100.into(), 0),
		DEV_RELAY_PARENT_OFFSET => testnet_genesis_with_default_endowed(100.into(), 2),
		_ => return None,
	};
	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}
