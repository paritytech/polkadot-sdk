// Copyright 2022 Parity Technologies (UK) Ltd.
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

use crate::chain_spec::{
	get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
};
use cumulus_primitives_core::ParaId;
use parachains_common::{AccountId, AuraId, Balance as CollectivesBalance};
use sc_service::ChainType;
use sp_core::sr25519;

pub type CollectivesPolkadotChainSpec = sc_service::GenericChainSpec<(), Extensions>;

const COLLECTIVES_POLKADOT_ED: CollectivesBalance =
	collectives_polkadot_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn collectives_polkadot_session_keys(
	keys: AuraId,
) -> collectives_polkadot_runtime::SessionKeys {
	collectives_polkadot_runtime::SessionKeys { aura: keys }
}

pub fn collectives_polkadot_development_config() -> CollectivesPolkadotChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	CollectivesPolkadotChainSpec::builder()
		.with_name("Polkadot Collectives Development")
		.with_id("collectives_polkadot_dev")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(collectives_polkadot_genesis(
			// initial collators.
			vec![(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_collator_keys_from_seed::<AuraId>("Alice"),
			)],
			vec![
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				get_account_id_from_seed::<sr25519::Public>("Bob"),
				get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
				get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
			],
			// 1002 avoids a potential collision with Kusama-1001 (Encointer) should there ever
			// be a collective para on Kusama.
			1002.into(),
		))
		.with_boot_nodes(Vec::new())
		.with_properties(properties)
		.with_extensions(Extensions { relay_chain: "polkadot-dev".into(), para_id: 1002 })
		.with_code(
			collectives_polkadot_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
		.build()
}

/// Collectives Polkadot Local Config.
pub fn collectives_polkadot_local_config() -> CollectivesPolkadotChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	CollectivesPolkadotChainSpec::builder()
		.with_name("Polkadot Collectives Local")
		.with_id("collectives_polkadot_local")
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(collectives_polkadot_genesis(
			// initial collators.
			vec![
				(
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_collator_keys_from_seed::<AuraId>("Alice"),
				),
				(
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_collator_keys_from_seed::<AuraId>("Bob"),
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
			1002.into(),
		))
		.with_boot_nodes(Vec::new())
		.with_properties(properties)
		.with_extensions(Extensions { relay_chain: "polkadot-local".into(), para_id: 1002 })
		.with_code(
			collectives_polkadot_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!"),
		)
		.build()
}

fn collectives_polkadot_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> serde_json::Value {
	serde_json::json!( {
		"balances": {
			"balances": endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, COLLECTIVES_POLKADOT_ED * 4096))
				.collect::<Vec<_>>(),
		},
		"parachainInfo": {
			"parachainId": id,
		},
		"collatorSelection": {
			"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
			"candidacyBond": COLLECTIVES_POLKADOT_ED * 16,
		},
		"session": {
			"keys": invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                             // account id
						acc,                                     // validator id
						collectives_polkadot_session_keys(aura), // session keys
					)
				})
				.collect::<Vec<_>>(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		"polkadotXcm": {
			"safeXcmVersion": Some(SAFE_XCM_VERSION),
		},
	})
}
