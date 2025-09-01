// This file is part of Cumulus.
// SPDX-License-Identifier: Unlicense

// This is free and unencumbered software released into the public domain.

// Anyone is free to copy, modify, publish, use, compile, sell, or
// distribute this software, either in source code form or as a compiled
// binary, for any purpose, commercial or non-commercial, and by any
// means.

// In jurisdictions that recognize copyright laws, the author or authors
// of this software dedicate any and all copyright interest in the
// software to the public domain. We make this dedication for the benefit
// of the public at large and to the detriment of our heirs and
// successors. We intend this dedication to be an overt act of
// relinquishment in perpetuity of all present and future rights to this
// software under copyright law.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND,
// EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF
// MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT.
// IN NO EVENT SHALL THE AUTHORS BE LIABLE FOR ANY CLAIM, DAMAGES OR
// OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE,
// ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR
// OTHER DEALINGS IN THE SOFTWARE.

// For more information, please refer to <http://unlicense.org/>

//! Penpal Parachain Runtime genesis config presets

use crate::*;
use alloc::{vec, vec::Vec};
use cumulus_primitives_core::ParaId;
use frame_support::build_struct_json_patch;
use parachains_common::{AccountId, AuraId};
use sp_genesis_builder::PresetId;
use sp_keyring::Sr25519Keyring;

const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

const DEFAULT_PARA_ID: ParaId = ParaId::new(2000);
const ENDOWMENT: u128 = 1 << 60;

fn penpal_parachain_genesis(
	sudo: AccountId,
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	endowment: Balance,
	id: ParaId,
) -> serde_json::Value {
	build_struct_json_patch!(RuntimeGenesisConfig {
		balances: BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, endowment)).collect(),
		},
		parachain_info: ParachainInfoConfig { parachain_id: id },
		collator_selection: CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: crate::EXISTENTIAL_DEPOSIT * 16,
		},
		session: SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),               // account id
						acc,                       // validator id
						penpal_session_keys(aura), // session keys
					)
				})
				.collect(),
		},
		polkadot_xcm: PolkadotXcmConfig { safe_xcm_version: Some(SAFE_XCM_VERSION) },
		sudo: SudoConfig { key: Some(sudo.clone()) },
		assets: AssetsConfig {
			assets: vec![(
				crate::xcm_config::TELEPORTABLE_ASSET_ID,
				sudo.clone(), // owner
				false,        // is_sufficient
				crate::EXISTENTIAL_DEPOSIT,
			)],
			metadata: vec![(
				crate::xcm_config::TELEPORTABLE_ASSET_ID,
				"pal-2".as_bytes().to_vec(),
				"pal-2".as_bytes().to_vec(),
				12,
			)],
			accounts: vec![(
				crate::xcm_config::TELEPORTABLE_ASSET_ID,
				sudo.clone(),
				crate::EXISTENTIAL_DEPOSIT * 4096,
			)]
		},
		foreign_assets: ForeignAssetsConfig {
			assets: vec![(
				crate::xcm_config::RelayLocation::get(),
				sudo.clone(),
				true,
				crate::EXISTENTIAL_DEPOSIT
			)],
			metadata: vec![(
				crate::xcm_config::RelayLocation::get(),
				"relay".as_bytes().to_vec(),
				"relay".as_bytes().to_vec(),
				12
			)],
			accounts: vec![(
				crate::xcm_config::RelayLocation::get(),
				sudo,
				crate::EXISTENTIAL_DEPOSIT * 4096,
			)]
		}
	})
}

/// Provides the JSON representation of predefined genesis config for given `id`.
pub fn get_preset(id: &PresetId) -> Option<Vec<u8>> {
	let genesis_fn = |authorities| {
		penpal_parachain_genesis(
			Sr25519Keyring::Alice.to_account_id(),
			authorities,
			Sr25519Keyring::well_known().map(|x| x.to_account_id()).collect(),
			ENDOWMENT,
			DEFAULT_PARA_ID,
		)
	};

	let patch = match id.as_ref() {
		sp_genesis_builder::DEV_RUNTIME_PRESET => genesis_fn(vec![(
			Sr25519Keyring::Alice.to_account_id(),
			Sr25519Keyring::Alice.public().into(),
		)]),
		sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET => genesis_fn(vec![
			(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into()),
			(Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Bob.public().into()),
		]),
		_ => return None,
	};

	Some(
		serde_json::to_string(&patch)
			.expect("serialization to json is expected to work. qed.")
			.into_bytes(),
	)
}

/// List of supported presets.
pub fn preset_names() -> Vec<PresetId> {
	vec![
		PresetId::from(sp_genesis_builder::DEV_RUNTIME_PRESET),
		PresetId::from(sp_genesis_builder::LOCAL_TESTNET_RUNTIME_PRESET),
	]
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn penpal_session_keys(keys: AuraId) -> crate::SessionKeys {
	crate::SessionKeys { aura: keys }
}
