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

use crate::chain_spec::SAFE_XCM_VERSION;
use cumulus_primitives_core::ParaId;
use parachains_common::{AccountId, AuraId};
use polkadot_omni_node_lib::chain_spec::{Extensions, GenericChainSpec};
use sc_service::ChainType;
use sp_keyring::Sr25519Keyring;

pub fn get_penpal_chain_spec(id: ParaId, relay_chain: &str) -> GenericChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "UNIT".into());
	properties.insert("tokenDecimals".into(), 12u32.into());
	properties.insert("ss58Format".into(), 42u32.into());

	GenericChainSpec::builder(
		penpal_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions {
			relay_chain: relay_chain.into(), // You MUST set this to the correct network!
			para_id: id.into(),
		},
	)
	.with_name("Penpal Parachain")
	.with_id(&format!("penpal-{}", relay_chain.replace("-local", "")))
	.with_chain_type(ChainType::Development)
	.with_genesis_config_patch(penpal_testnet_genesis(
		// initial collators.
		vec![
			(Sr25519Keyring::Alice.to_account_id(), Sr25519Keyring::Alice.public().into()),
			(Sr25519Keyring::Bob.to_account_id(), Sr25519Keyring::Bob.public().into()),
		],
		Sr25519Keyring::well_known().map(|k| k.to_account_id()).collect(),
		id,
		Sr25519Keyring::Alice.to_account_id(),
	))
	.build()
}

fn penpal_testnet_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
	sudo_account: AccountId,
) -> serde_json::Value {
	serde_json::json!({
		"balances": {
			"balances": endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, penpal_runtime::EXISTENTIAL_DEPOSIT * 4096))
				.collect::<Vec<_>>(),
		},
		"parachainInfo": {
			"parachainId": id,
		},
		"collatorSelection": {
			"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
			"candidacyBond": penpal_runtime::EXISTENTIAL_DEPOSIT * 16,
		},
		"session": {
			"keys": invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),               // account id
						acc,                       // validator id
						penpal_session_keys(aura), // session keys
					)
				})
				.collect::<Vec<_>>(),
		},
		"polkadotXcm": {
			"safeXcmVersion": Some(SAFE_XCM_VERSION),
		},
		"sudo": {
			"key": Some(sudo_account.clone()),
		},
		"assets": {
			"assets": vec![(
				penpal_runtime::xcm_config::TELEPORTABLE_ASSET_ID,
				sudo_account.clone(), // owner
				false, // is_sufficient
				penpal_runtime::EXISTENTIAL_DEPOSIT,
			)],
			"metadata": vec![(
				penpal_runtime::xcm_config::TELEPORTABLE_ASSET_ID,
				"pal-2".as_bytes().to_vec(),
				"pal-2".as_bytes().to_vec(),
				12,
			)],
			"accounts": vec![(
				penpal_runtime::xcm_config::TELEPORTABLE_ASSET_ID,
				sudo_account.clone(),
				penpal_runtime::EXISTENTIAL_DEPOSIT * 4096,
			)]
		},
		"foreignAssets": {
			"assets": vec![(
				penpal_runtime::xcm_config::RelayLocation::get(),
				sudo_account.clone(),
				true,
				penpal_runtime::EXISTENTIAL_DEPOSIT
			)],
			"metadata": vec![(
				penpal_runtime::xcm_config::RelayLocation::get(),
				"relay".as_bytes().to_vec(),
				"relay".as_bytes().to_vec(),
				12
			)],
			"accounts": vec![(
				penpal_runtime::xcm_config::RelayLocation::get(),
				sudo_account.clone(),
				penpal_runtime::EXISTENTIAL_DEPOSIT * 4096,
			)]
		},
	})
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn penpal_session_keys(keys: AuraId) -> penpal_runtime::SessionKeys {
	penpal_runtime::SessionKeys { aura: keys }
}
