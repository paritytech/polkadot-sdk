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

use cumulus_primitives_core::ParaId;
use hex_literal::hex;
use rococo_parachain_runtime::{AccountId, AuraId, Signature};
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use sc_service::ChainType;
use serde::{Deserialize, Serialize};
use sp_core::{crypto::UncheckedInto, sr25519, Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type ChainSpec =
	sc_service::GenericChainSpec<rococo_parachain_runtime::GenesisConfig, Extensions>;

/// Specialized `ChainSpec` for the shell parachain runtime.
pub type ShellChainSpec = sc_service::GenericChainSpec<shell_runtime::GenesisConfig, Extensions>;

/// Specialized `ChainSpec` for the seedling parachain runtime.
pub type SeedlingChainSpec =
	sc_service::GenericChainSpec<seedling_runtime::GenesisConfig, Extensions>;

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

/// The extensions for the [`ChainSpec`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
#[serde(deny_unknown_fields)]
pub struct Extensions {
	/// The relay chain of the Parachain.
	pub relay_chain: String,
	/// The id of the Parachain.
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

type AccountPublic = <Signature as Verify>::Signer;

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

pub fn get_chain_spec() -> ChainSpec {
	ChainSpec::from_genesis(
		"Local Testnet",
		"local_testnet",
		ChainType::Local,
		move || {
			testnet_genesis(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				vec![get_from_seed::<AuraId>("Alice"), get_from_seed::<AuraId>("Bob")],
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
		None,
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
}

pub fn get_shell_chain_spec() -> ShellChainSpec {
	ShellChainSpec::from_genesis(
		"Shell Local Testnet",
		"shell_local_testnet",
		ChainType::Local,
		move || shell_testnet_genesis(1000.into()),
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
}

pub fn get_seedling_chain_spec() -> SeedlingChainSpec {
	SeedlingChainSpec::from_genesis(
		"Seedling Local Testnet",
		"seedling_local_testnet",
		ChainType::Local,
		move || {
			seedling_testnet_genesis(
				get_account_id_from_seed::<sr25519::Public>("Alice"),
				2000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "westend".into(), para_id: 2000 },
	)
}

pub fn staging_test_net() -> ChainSpec {
	ChainSpec::from_genesis(
		"Staging Testnet",
		"staging_testnet",
		ChainType::Live,
		move || {
			testnet_genesis(
				hex!["9ed7705e3c7da027ba0583a22a3212042f7e715d3c168ba14f1424e2bc111d00"].into(),
				vec![
					// $secret//one
					hex!["aad9fa2249f87a210a0f93400b7f90e47b810c6d65caa0ca3f5af982904c2a33"]
						.unchecked_into(),
					// $secret//two
					hex!["d47753f0cca9dd8da00c70e82ec4fc5501a69c49a5952a643d18802837c88212"]
						.unchecked_into(),
				],
				vec![
					hex!["9ed7705e3c7da027ba0583a22a3212042f7e715d3c168ba14f1424e2bc111d00"].into()
				],
				1000.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions { relay_chain: "westend".into(), para_id: 1000 },
	)
}

fn testnet_genesis(
	root_key: AccountId,
	initial_authorities: Vec<AuraId>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> rococo_parachain_runtime::GenesisConfig {
	rococo_parachain_runtime::GenesisConfig {
		system: rococo_parachain_runtime::SystemConfig {
			code: rococo_parachain_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: rococo_parachain_runtime::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		sudo: rococo_parachain_runtime::SudoConfig { key: Some(root_key) },
		parachain_info: rococo_parachain_runtime::ParachainInfoConfig { parachain_id: id },
		aura: rococo_parachain_runtime::AuraConfig { authorities: initial_authorities },
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: rococo_parachain_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
	}
}

fn shell_testnet_genesis(parachain_id: ParaId) -> shell_runtime::GenesisConfig {
	shell_runtime::GenesisConfig {
		system: shell_runtime::SystemConfig {
			code: shell_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		parachain_info: shell_runtime::ParachainInfoConfig { parachain_id },
		parachain_system: Default::default(),
	}
}

fn seedling_testnet_genesis(
	root_key: AccountId,
	parachain_id: ParaId,
) -> seedling_runtime::GenesisConfig {
	seedling_runtime::GenesisConfig {
		system: seedling_runtime::SystemConfig {
			code: seedling_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		sudo: seedling_runtime::SudoConfig { key: Some(root_key) },
		parachain_info: seedling_runtime::ParachainInfoConfig { parachain_id },
		parachain_system: Default::default(),
	}
}

use parachains_common::{Balance as StatemintBalance, StatemintAuraId};

/// Specialized `ChainSpec` for the normal parachain runtime.
pub type StatemintChainSpec =
	sc_service::GenericChainSpec<statemint_runtime::GenesisConfig, Extensions>;
pub type StatemineChainSpec =
	sc_service::GenericChainSpec<statemine_runtime::GenesisConfig, Extensions>;
pub type WestmintChainSpec =
	sc_service::GenericChainSpec<westmint_runtime::GenesisConfig, Extensions>;

const STATEMINT_ED: StatemintBalance = statemint_runtime::constants::currency::EXISTENTIAL_DEPOSIT;
const STATEMINE_ED: StatemintBalance = statemine_runtime::constants::currency::EXISTENTIAL_DEPOSIT;
const WESTMINT_ED: StatemintBalance = westmint_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

/// Helper function to generate a crypto pair from seed
pub fn get_public_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

/// Generate collator keys from seed.
///
/// This function's return type must always match the session keys of the chain in tuple format.
pub fn get_collator_keys_from_seed<AuraId: Public>(seed: &str) -> <AuraId::Pair as Pair>::Public {
	get_public_from_seed::<AuraId>(seed)
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn statemint_session_keys(keys: StatemintAuraId) -> statemint_runtime::SessionKeys {
	statemint_runtime::SessionKeys { aura: keys }
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn statemine_session_keys(keys: AuraId) -> statemine_runtime::SessionKeys {
	statemine_runtime::SessionKeys { aura: keys }
}

/// Generate the session keys from individual elements.
///
/// The input must be a tuple of individual keys (a single arg for now since we have just one key).
pub fn westmint_session_keys(keys: AuraId) -> westmint_runtime::SessionKeys {
	westmint_runtime::SessionKeys { aura: keys }
}

pub fn statemint_development_config() -> StatemintChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	StatemintChainSpec::from_genesis(
		// Name
		"Statemint Development",
		// ID
		"statemint_dev",
		ChainType::Local,
		move || {
			statemint_genesis(
				// initial collators.
				vec![(
					get_account_id_from_seed::<sr25519::Public>("Alice"),
					get_collator_keys_from_seed::<StatemintAuraId>("Alice"),
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

pub fn statemint_local_config() -> StatemintChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	StatemintChainSpec::from_genesis(
		// Name
		"Statemint Local",
		// ID
		"statemint_local",
		ChainType::Local,
		move || {
			statemint_genesis(
				// initial collators.
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed::<StatemintAuraId>("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed::<StatemintAuraId>("Bob"),
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
pub fn statemint_config() -> StatemintChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 0.into());
	properties.insert("tokenSymbol".into(), "DOT".into());
	properties.insert("tokenDecimals".into(), 10.into());

	StatemintChainSpec::from_genesis(
		// Name
		"Statemint",
		// ID
		"statemint",
		ChainType::Live,
		move || {
			statemint_genesis(
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

fn statemint_genesis(
	invulnerables: Vec<(AccountId, StatemintAuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> statemint_runtime::GenesisConfig {
	statemint_runtime::GenesisConfig {
		system: statemint_runtime::SystemConfig {
			code: statemint_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: statemint_runtime::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, STATEMINT_ED * 4096)).collect(),
		},
		parachain_info: statemint_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: statemint_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: STATEMINT_ED * 16,
			..Default::default()
		},
		session: statemint_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                  // account id
						acc,                          // validator id
						statemint_session_keys(aura), // session keys
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: statemint_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
	}
}

pub fn statemine_development_config() -> StatemineChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());
	properties.insert("tokenSymbol".into(), "KSM".into());
	properties.insert("tokenDecimals".into(), 12.into());

	StatemineChainSpec::from_genesis(
		// Name
		"Statemine Development",
		// ID
		"statemine_dev",
		ChainType::Local,
		move || {
			statemine_genesis(
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

pub fn statemine_local_config() -> StatemineChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());
	properties.insert("tokenSymbol".into(), "KSM".into());
	properties.insert("tokenDecimals".into(), 12.into());

	StatemineChainSpec::from_genesis(
		// Name
		"Statemine Local",
		// ID
		"statemine_local",
		ChainType::Local,
		move || {
			statemine_genesis(
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

pub fn statemine_config() -> StatemineChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("ss58Format".into(), 2.into());
	properties.insert("tokenSymbol".into(), "KSM".into());
	properties.insert("tokenDecimals".into(), 12.into());

	StatemineChainSpec::from_genesis(
		// Name
		"Statemine",
		// ID
		"statemine",
		ChainType::Live,
		move || {
			statemine_genesis(
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

fn statemine_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> statemine_runtime::GenesisConfig {
	statemine_runtime::GenesisConfig {
		system: statemine_runtime::SystemConfig {
			code: statemine_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: statemine_runtime::BalancesConfig {
			balances: endowed_accounts
				.iter()
				.cloned()
				.map(|k| (k, STATEMINE_ED * 524_288))
				.collect(),
		},
		parachain_info: statemine_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: statemine_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: STATEMINE_ED * 16,
			..Default::default()
		},
		session: statemine_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                  // account id
						acc,                          // validator id
						statemine_session_keys(aura), // session keys
					)
				})
				.collect(),
		},
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: statemine_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
	}
}

pub fn westmint_development_config() -> WestmintChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	WestmintChainSpec::from_genesis(
		// Name
		"Westmint Development",
		// ID
		"westmint_dev",
		ChainType::Local,
		move || {
			westmint_genesis(
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

pub fn westmint_local_config() -> WestmintChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	WestmintChainSpec::from_genesis(
		// Name
		"Westmint Local",
		// ID
		"westmint_local",
		ChainType::Local,
		move || {
			westmint_genesis(
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

pub fn westmint_config() -> WestmintChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "WND".into());
	properties.insert("tokenDecimals".into(), 12.into());

	WestmintChainSpec::from_genesis(
		// Name
		"Westmint",
		// ID
		"westmint",
		ChainType::Live,
		move || {
			westmint_genesis(
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

fn westmint_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> westmint_runtime::GenesisConfig {
	westmint_runtime::GenesisConfig {
		system: westmint_runtime::SystemConfig {
			code: westmint_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: westmint_runtime::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, WESTMINT_ED * 4096)).collect(),
		},
		parachain_info: westmint_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: westmint_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: WESTMINT_ED * 16,
			..Default::default()
		},
		session: westmint_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                 // account id
						acc,                         // validator id
						westmint_session_keys(aura), // session keys
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: westmint_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
	}
}
