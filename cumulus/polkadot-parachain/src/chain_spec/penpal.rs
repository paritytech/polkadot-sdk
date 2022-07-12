// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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

use crate::chain_spec::{get_account_id_from_seed, Extensions};
use cumulus_primitives_core::ParaId;
// use rococo_parachain_runtime::{AuraId};
use crate::chain_spec::{get_collator_keys_from_seed, SAFE_XCM_VERSION};
use sc_service::ChainType;
use sp_core::sr25519;
/// Specialized `ChainSpec` for the normal parachain runtime.
pub type PenpalChainSpec = sc_service::GenericChainSpec<penpal_runtime::GenesisConfig, Extensions>;

pub fn get_penpal_chain_spec(id: ParaId, relay_chain: &str) -> PenpalChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "UNIT".into());
	properties.insert("tokenDecimals".into(), 12u32.into());
	properties.insert("ss58Format".into(), 42u32.into());

	PenpalChainSpec::from_genesis(
		// Name
		"Penpal Parachain",
		// ID
		&format!("penpal-{}", relay_chain.replace("-local", "")),
		ChainType::Development,
		move || {
			penpal_testnet_genesis(
				// initial collators.
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed::<penpal_runtime::AuraId>("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed::<penpal_runtime::AuraId>("Bob"),
					),
				],
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Charlie"),
					get_account_id_from_seed::<sr25519::Public>("Dave"),
					get_account_id_from_seed::<sr25519::Public>("Eve"),
					get_account_id_from_seed::<sr25519::Public>("Ferdie"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
					get_account_id_from_seed::<sr25519::Public>("Charlie//stash"),
					get_account_id_from_seed::<sr25519::Public>("Dave//stash"),
					get_account_id_from_seed::<sr25519::Public>("Eve//stash"),
					get_account_id_from_seed::<sr25519::Public>("Ferdie//stash"),
				],
				id,
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions {
			relay_chain: relay_chain.into(), // You MUST set this to the correct network!
			para_id: id.into(),
		},
	)
}

fn penpal_testnet_genesis(
	invulnerables: Vec<(penpal_runtime::AccountId, penpal_runtime::AuraId)>,
	endowed_accounts: Vec<penpal_runtime::AccountId>,
	id: ParaId,
) -> penpal_runtime::GenesisConfig {
	penpal_runtime::GenesisConfig {
		system: penpal_runtime::SystemConfig {
			code: penpal_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: penpal_runtime::BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, penpal_runtime::EXISTENTIAL_DEPOSIT * 4096))
				.collect(),
		},
		parachain_info: penpal_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: penpal_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: penpal_runtime::EXISTENTIAL_DEPOSIT * 16,
			..Default::default()
		},
		session: penpal_runtime::SessionConfig {
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
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: penpal_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
		sudo: penpal_runtime::SudoConfig {
			key: Some(get_account_id_from_seed::<sr25519::Public>("Alice")),
		},
	}
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn penpal_session_keys(keys: penpal_runtime::AuraId) -> penpal_runtime::SessionKeys {
	penpal_runtime::SessionKeys { aura: keys }
}
