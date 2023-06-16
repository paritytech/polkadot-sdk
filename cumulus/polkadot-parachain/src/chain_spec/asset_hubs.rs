// Copyright 2019-2022 Parity Technologies (UK) Ltd.
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
use hex_literal::hex;
use parachains_common::{AccountId, AssetHubPolkadotAuraId, AuraId, Balance as AssetHubBalance};
use sc_service::ChainType;
use sp_core::{crypto::UncheckedInto, sr25519};

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type AssetHubPolkadotChainSpec =
	sc_service::GenericChainSpec<asset_hub_polkadot_runtime::RuntimeGenesisConfig, Extensions>;
pub type AssetHubKusamaChainSpec =
	sc_service::GenericChainSpec<asset_hub_kusama_runtime::RuntimeGenesisConfig, Extensions>;
pub type AssetHubWestendChainSpec =
	sc_service::GenericChainSpec<asset_hub_westend_runtime::RuntimeGenesisConfig, Extensions>;

const ASSET_HUB_POLKADOT_ED: AssetHubBalance =
	asset_hub_polkadot_runtime::constants::currency::EXISTENTIAL_DEPOSIT;
const ASSET_HUB_KUSAMA_ED: AssetHubBalance =
	asset_hub_kusama_runtime::constants::currency::EXISTENTIAL_DEPOSIT;
const ASSET_HUB_WESTEND_ED: AssetHubBalance =
	asset_hub_westend_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn asset_hub_polkadot_session_keys(
	keys: AssetHubPolkadotAuraId,
) -> asset_hub_polkadot_runtime::SessionKeys {
	asset_hub_polkadot_runtime::SessionKeys { aura: keys }
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn asset_hub_kusama_session_keys(keys: AuraId) -> asset_hub_kusama_runtime::SessionKeys {
	asset_hub_kusama_runtime::SessionKeys { aura: keys }
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn asset_hub_westend_session_keys(keys: AuraId) -> asset_hub_westend_runtime::SessionKeys {
	asset_hub_westend_runtime::SessionKeys { aura: keys }
}

pub fn asset_hub_polkadot_development_config() -> AssetHubPolkadotChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	AssetHubPolkadotChainSpec::from_genesis(
		// Name
		"Polkadot Asset Hub Development",
		// ID
		"asset-hub-polkadot-dev",
		ChainType::Local,
		move || {
			asset_hub_polkadot_genesis(
				// initial collators.
				vec![(
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_collator_keys_from_seed::<AssetHubPolkadotAuraId>("Alice"),
				)],
				vec![
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_account_id_from_seed::<sr25519::Public>("Bob"),
					get_account_id_from_seed::<sr25519::Public>("Alice//stash"),
					get_account_id_from_seed::<sr25519::Public>("Bob//stash"),
				],
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "polkadot-dev".into(), para_id: 1000 },
	)
}

pub fn asset_hub_polkadot_local_config() -> AssetHubPolkadotChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	AssetHubPolkadotChainSpec::from_genesis(
		// Name
		"Polkadot Asset Hub Local",
		// ID
		"asset-hub-polkadot-local",
		ChainType::Local,
		move || {
			asset_hub_polkadot_genesis(
				// initial collators.
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed::<AssetHubPolkadotAuraId>("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed::<AssetHubPolkadotAuraId>("Bob"),
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
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "polkadot-local".into(), para_id: 1000 },
	)
}

// Not used for syncing, but just to determine the genesis values set for the upgrade from shell.
pub fn asset_hub_polkadot_config() -> AssetHubPolkadotChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	AssetHubPolkadotChainSpec::from_genesis(
		// Name
		"Polkadot Asset Hub",
		// ID
		"asset-hub-polkadot",
		ChainType::Live,
		move || {
			asset_hub_polkadot_genesis(
				// initial collators.
				vec![
					(
						hex!("4c3d674d2a01060f0ded218e5dcc6f90c1726f43df79885eb3e22d97a20d5421")
							.into(),
						hex!("4c3d674d2a01060f0ded218e5dcc6f90c1726f43df79885eb3e22d97a20d5421")
							.unchecked_into(),
					),
					(
						hex!("c7d7d38d16bc23c6321152c50306212dc22c0efc04a2e52b5cccfc31ab3d7811")
							.into(),
						hex!("c7d7d38d16bc23c6321152c50306212dc22c0efc04a2e52b5cccfc31ab3d7811")
							.unchecked_into(),
					),
					(
						hex!("c5c07ba203d7375675f5c1ebe70f0a5bb729ae57b48bcc877fcc2ab21309b762")
							.into(),
						hex!("c5c07ba203d7375675f5c1ebe70f0a5bb729ae57b48bcc877fcc2ab21309b762")
							.unchecked_into(),
					),
					(
						hex!("0b2d0013fb974794bd7aa452465b567d48ef70373fe231a637c1fb7c547e85b3")
							.into(),
						hex!("0b2d0013fb974794bd7aa452465b567d48ef70373fe231a637c1fb7c547e85b3")
							.unchecked_into(),
					),
				],
				vec![],
				1000u32.into(),
			)
		},
		vec![
			"/ip4/34.65.251.121/tcp/30334/p2p/12D3KooWG3GrM6XKMM4gp3cvemdwUvu96ziYoJmqmetLZBXE8bSa".parse().unwrap(),
			"/ip4/34.65.35.228/tcp/30334/p2p/12D3KooWMRyTLrCEPcAQD6c4EnudL3vVzg9zji3whvsMYPUYevpq".parse().unwrap(),
			"/ip4/34.83.247.146/tcp/30334/p2p/12D3KooWE4jFh5FpJDkWVZhnWtFnbSqRhdjvC7Dp9b8b3FTuubQC".parse().unwrap(),
			"/ip4/104.199.117.230/tcp/30334/p2p/12D3KooWG9R8pVXKumVo2rdkeVD4j5PVhRTqmYgLHY3a4yPYgLqM".parse().unwrap(),
		],
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "polkadot".into(), para_id: 1000 },
	)
}

fn asset_hub_polkadot_genesis(
	invulnerables: Vec<(AccountId, AssetHubPolkadotAuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> asset_hub_polkadot_runtime::RuntimeGenesisConfig {
	asset_hub_polkadot_runtime::RuntimeGenesisConfig {
		system: asset_hub_polkadot_runtime::SystemConfig {
			code: asset_hub_polkadot_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: asset_hub_polkadot_runtime::BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, ASSET_HUB_POLKADOT_ED * 4096))
				.collect(),
		},
		parachain_info: asset_hub_polkadot_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: asset_hub_polkadot_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: ASSET_HUB_POLKADOT_ED * 16,
			..Default::default()
		},
		session: asset_hub_polkadot_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                           // account id
						acc,                                   // validator id
						asset_hub_polkadot_session_keys(aura), // session keys
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: asset_hub_polkadot_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
	}
}

pub fn asset_hub_kusama_development_config() -> AssetHubKusamaChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());
	properties.insert("tokenSymbol".into(), "KSM".into());
	properties.insert("tokenDecimals".into(), 12.into());

	AssetHubKusamaChainSpec::from_genesis(
		// Name
		"Kusama Asset Hub Development",
		// ID
		"asset-hub-kusama-dev",
		ChainType::Local,
		move || {
			asset_hub_kusama_genesis(
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
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "kusama-dev".into(), para_id: 1000 },
	)
}

pub fn asset_hub_kusama_local_config() -> AssetHubKusamaChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());
	properties.insert("tokenSymbol".into(), "KSM".into());
	properties.insert("tokenDecimals".into(), 12.into());

	AssetHubKusamaChainSpec::from_genesis(
		// Name
		"Kusama Asset Hub Local",
		// ID
		"asset-hub-kusama-local",
		ChainType::Local,
		move || {
			asset_hub_kusama_genesis(
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
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "kusama-local".into(), para_id: 1000 },
	)
}

pub fn asset_hub_kusama_config() -> AssetHubKusamaChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());
	properties.insert("tokenSymbol".into(), "KSM".into());
	properties.insert("tokenDecimals".into(), 12.into());

	AssetHubKusamaChainSpec::from_genesis(
		// Name
		"Kusama Asset Hub",
		// ID
		"asset-hub-kusama",
		ChainType::Live,
		move || {
			asset_hub_kusama_genesis(
				// initial collators.
				vec![
					(
						hex!("50673d59020488a4ffc9d8c6de3062a65977046e6990915617f85fef6d349730")
							.into(),
						hex!("50673d59020488a4ffc9d8c6de3062a65977046e6990915617f85fef6d349730")
							.unchecked_into(),
					),
					(
						hex!("fe8102dbc244e7ea2babd9f53236d67403b046154370da5c3ea99def0bd0747a")
							.into(),
						hex!("fe8102dbc244e7ea2babd9f53236d67403b046154370da5c3ea99def0bd0747a")
							.unchecked_into(),
					),
					(
						hex!("38144b5398e5d0da5ec936a3af23f5a96e782f676ab19d45f29075ee92eca76a")
							.into(),
						hex!("38144b5398e5d0da5ec936a3af23f5a96e782f676ab19d45f29075ee92eca76a")
							.unchecked_into(),
					),
					(
						hex!("3253947640e309120ae70fa458dcacb915e2ddd78f930f52bd3679ec63fc4415")
							.into(),
						hex!("3253947640e309120ae70fa458dcacb915e2ddd78f930f52bd3679ec63fc4415")
							.unchecked_into(),
					),
				],
				Vec::new(),
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "kusama".into(), para_id: 1000 },
	)
}

fn asset_hub_kusama_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> asset_hub_kusama_runtime::RuntimeGenesisConfig {
	asset_hub_kusama_runtime::RuntimeGenesisConfig {
		system: asset_hub_kusama_runtime::SystemConfig {
			code: asset_hub_kusama_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: asset_hub_kusama_runtime::BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, ASSET_HUB_KUSAMA_ED * 524_288))
				.collect(),
		},
		parachain_info: asset_hub_kusama_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: asset_hub_kusama_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: ASSET_HUB_KUSAMA_ED * 16,
			..Default::default()
		},
		session: asset_hub_kusama_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                         // account id
						acc,                                 // validator id
						asset_hub_kusama_session_keys(aura), // session keys
					)
				})
				.collect(),
		},
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: asset_hub_kusama_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
	}
}

pub fn asset_hub_westend_development_config() -> AssetHubWestendChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	AssetHubWestendChainSpec::from_genesis(
		// Name
		"Westend Asset Hub Development",
		// ID
		"asset-hub-westend-dev",
		ChainType::Local,
		move || {
			asset_hub_westend_genesis(
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
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
}

pub fn asset_hub_westend_local_config() -> AssetHubWestendChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	AssetHubWestendChainSpec::from_genesis(
		// Name
		"Westend Asset Hub Local",
		// ID
		"asset-hub-westend-local",
		ChainType::Local,
		move || {
			asset_hub_westend_genesis(
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
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "westend-local".into(), para_id: 1000 },
	)
}

pub fn asset_hub_westend_config() -> AssetHubWestendChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	AssetHubWestendChainSpec::from_genesis(
		// Name
		"Westend Asset Hub",
		// ID
		"asset-hub-westend",
		ChainType::Live,
		move || {
			asset_hub_westend_genesis(
				// initial collators.
				vec![
					(
						hex!("9cfd429fa002114f33c1d3e211501d62830c9868228eb3b4b8ae15a83de04325")
							.into(),
						hex!("9cfd429fa002114f33c1d3e211501d62830c9868228eb3b4b8ae15a83de04325")
							.unchecked_into(),
					),
					(
						hex!("12a03fb4e7bda6c9a07ec0a11d03c24746943e054ff0bb04938970104c783876")
							.into(),
						hex!("12a03fb4e7bda6c9a07ec0a11d03c24746943e054ff0bb04938970104c783876")
							.unchecked_into(),
					),
					(
						hex!("1256436307dfde969324e95b8c62cb9101f520a39435e6af0f7ac07b34e1931f")
							.into(),
						hex!("1256436307dfde969324e95b8c62cb9101f520a39435e6af0f7ac07b34e1931f")
							.unchecked_into(),
					),
					(
						hex!("98102b7bca3f070f9aa19f58feed2c0a4e107d203396028ec17a47e1ed80e322")
							.into(),
						hex!("98102b7bca3f070f9aa19f58feed2c0a4e107d203396028ec17a47e1ed80e322")
							.unchecked_into(),
					),
				],
				Vec::new(),
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		Some(properties),
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
}

fn asset_hub_westend_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> asset_hub_westend_runtime::RuntimeGenesisConfig {
	asset_hub_westend_runtime::RuntimeGenesisConfig {
		system: asset_hub_westend_runtime::SystemConfig {
			code: asset_hub_westend_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: asset_hub_westend_runtime::BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, ASSET_HUB_WESTEND_ED * 4096))
				.collect(),
		},
		parachain_info: asset_hub_westend_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: asset_hub_westend_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: ASSET_HUB_WESTEND_ED * 16,
			..Default::default()
		},
		session: asset_hub_westend_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                          // account id
						acc,                                  // validator id
						asset_hub_westend_session_keys(aura), // session keys
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: asset_hub_westend_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
	}
}
