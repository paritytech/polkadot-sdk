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

use crate::chain_spec::{
	get_account_id_from_seed, get_collator_keys_from_seed, Extensions, SAFE_XCM_VERSION,
};
use cumulus_primitives_core::ParaId;
use hex_literal::hex;
use parachains_common::{AccountId, AuraId};
use sc_service::ChainType;
use sp_core::{crypto::UncheckedInto, sr25519};

pub type ContractsRococoChainSpec =
	sc_service::GenericChainSpec<contracts_rococo_runtime::RuntimeGenesisConfig, Extensions>;

/// No relay chain suffix because the id is the same over all relay chains.
const CONTRACTS_PARACHAIN_ID: u32 = 1002;

/// The existential deposit is determined by the runtime "contracts-rococo".
const CONTRACTS_ROCOCO_ED: contracts_rococo_runtime::Balance =
	contracts_rococo_runtime::constants::currency::EXISTENTIAL_DEPOSIT;

pub fn contracts_rococo_development_config() -> ContractsRococoChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());

	ContractsRococoChainSpec::from_genesis(
		// Name
		"Contracts on Rococo Development",
		// ID
		"contracts-rococo-dev",
		ChainType::Development,
		move || {
			contracts_rococo_genesis(
				// initial collators.
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed::<contracts_rococo_runtime::AuraId>("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed::<contracts_rococo_runtime::AuraId>("Bob"),
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
				CONTRACTS_PARACHAIN_ID.into(),
			)
		},
		Vec::new(),
		None,
		None,
		None,
		None,
		Extensions {
			relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
			para_id: CONTRACTS_PARACHAIN_ID,
		},
	)
}

pub fn contracts_rococo_local_config() -> ContractsRococoChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());

	ContractsRococoChainSpec::from_genesis(
		// Name
		"Contracts on Rococo",
		// ID
		"contracts-rococo-local",
		ChainType::Local,
		move || {
			contracts_rococo_genesis(
				// initial collators.
				vec![
					(
						get_account_id_from_seed::<sr25519::Public>("Alice"),
						get_collator_keys_from_seed::<contracts_rococo_runtime::AuraId>("Alice"),
					),
					(
						get_account_id_from_seed::<sr25519::Public>("Bob"),
						get_collator_keys_from_seed::<contracts_rococo_runtime::AuraId>("Bob"),
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
				CONTRACTS_PARACHAIN_ID.into(),
			)
		},
		// Bootnodes
		Vec::new(),
		// Telemetry
		None,
		// Protocol ID
		None,
		// Fork ID
		None,
		// Properties
		Some(properties),
		// Extensions
		Extensions {
			relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
			para_id: CONTRACTS_PARACHAIN_ID,
		},
	)
}

pub fn contracts_rococo_config() -> ContractsRococoChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());

	ContractsRococoChainSpec::from_genesis(
		// Name
		"Contracts on Rococo",
		// ID
		"contracts-rococo",
		ChainType::Live,
		move || {
			contracts_rococo_genesis(
				vec![
					// 5GKFbTTgrVS4Vz1UWWHPqMZQNFWZtqo7H2KpCDyYhEL3aS26
					(
						hex!["bc09354c12c054c8f6b3da208485eacec4ac648bad348895273b37bab5a0937c"]
							.into(),
						hex!["bc09354c12c054c8f6b3da208485eacec4ac648bad348895273b37bab5a0937c"]
							.unchecked_into(),
					),
					// 5EPRJHm2GpABVWcwnAujcrhnrjFZyDGd5TwKFzkBoGgdRyv2
					(
						hex!["66be63b7bcbfb91040e5248e2d1ceb822cf219c57848c5924ffa3a1f8e67ba72"]
							.into(),
						hex!["66be63b7bcbfb91040e5248e2d1ceb822cf219c57848c5924ffa3a1f8e67ba72"]
							.unchecked_into(),
					),
					// 5GH62vrJrVZxLREcHzm2PR5uTLAT5RQMJitoztCGyaP4o3uM
					(
						hex!["ba62886472a0a9f66b5e39f1469ce1c5b3d8cad6be39078daf16f111e89d1e44"]
							.into(),
						hex!["ba62886472a0a9f66b5e39f1469ce1c5b3d8cad6be39078daf16f111e89d1e44"]
							.unchecked_into(),
					),
					// 5FHfoJDLdjRYX5KXLRqMDYBbWrwHLMtti21uK4QByUoUAbJF
					(
						hex!["8e97f65cda001976311df9bed39e8d0c956089093e94a75ef76fe9347a0eda7b"]
							.into(),
						hex!["8e97f65cda001976311df9bed39e8d0c956089093e94a75ef76fe9347a0eda7b"]
							.unchecked_into(),
					),
				],
				// Warning: The configuration for a production chain should not contain
				// any endowed accounts here, otherwise it'll be minting extra native tokens
				// from the relay chain on the parachain.
				vec![
					// NOTE: Remove endowed accounts if deployed on other relay chains.
					// Endowed accounts
					hex!["baa78c7154c7f82d6d377177e20bcab65d327eca0086513f9964f5a0f6bdad56"].into(),
					// AccountId of an account which `ink-waterfall` uses for automated testing
					hex!["0e47e2344d523c3cc5c34394b0d58b9a4200e813a038e6c5a6163cc07d70b069"].into(),
				],
				CONTRACTS_PARACHAIN_ID.into(),
			)
		},
		// Bootnodes
		vec![
			"/dns/contracts-collator-0.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWKg3Rpxcr9oJ8n6khoxpGKWztCZydtUZk2cojHqnfLrpj"
				.parse()
				.expect("MultiaddrWithPeerId"),
			"/dns/contracts-collator-1.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWPEXYrz8tHU3nDtPoPw4V7ou5dzMEWSTuUj7vaWiYVAVh"
				.parse()
				.expect("MultiaddrWithPeerId"),
			"/dns/contracts-collator-2.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWEVU8AFNary4nP4qEnEcwJaRuy59Wefekzdu9pKbnVEhk"
				.parse()
				.expect("MultiaddrWithPeerId"),
			"/dns/contracts-collator-3.parity-testnet.parity.io/tcp/30333/p2p/12D3KooWP6pV3ZmcXzGDjv8ZMgA6nZxfAKDxSz4VNiLx6vVCQgJX"
				.parse()
				.expect("MultiaddrWithPeerId"),
		],
		// Telemetry
		None,
		// Protocol ID
		None,
		// Fork ID
		None,
		// Properties
		Some(properties),
		// Extensions
		Extensions { relay_chain: "rococo".into(), para_id: CONTRACTS_PARACHAIN_ID },
	)
}

fn contracts_rococo_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> contracts_rococo_runtime::RuntimeGenesisConfig {
	contracts_rococo_runtime::RuntimeGenesisConfig {
		system: contracts_rococo_runtime::SystemConfig {
			code: contracts_rococo_runtime::WASM_BINARY
				.expect("WASM binary was not build, please build it!")
				.to_vec(),
		},
		balances: contracts_rococo_runtime::BalancesConfig {
			balances: endowed_accounts.iter().cloned().map(|k| (k, 1 << 60)).collect(),
		},
		parachain_info: contracts_rococo_runtime::ParachainInfoConfig { parachain_id: id },
		collator_selection: contracts_rococo_runtime::CollatorSelectionConfig {
			invulnerables: invulnerables.iter().cloned().map(|(acc, _)| acc).collect(),
			candidacy_bond: CONTRACTS_ROCOCO_ED * 16,
			..Default::default()
		},
		session: contracts_rococo_runtime::SessionConfig {
			keys: invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                                    // account id
						acc,                                            // validator id
						contracts_rococo_runtime::SessionKeys { aura }, // session keys
					)
				})
				.collect(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		aura: Default::default(),
		aura_ext: Default::default(),
		parachain_system: Default::default(),
		polkadot_xcm: contracts_rococo_runtime::PolkadotXcmConfig {
			safe_xcm_version: Some(SAFE_XCM_VERSION),
		},
		sudo: contracts_rococo_runtime::SudoConfig {
			key: Some(
				hex!["2681a28014e7d3a5bfb32a003b3571f53c408acbc28d351d6bf58f5028c4ef14"].into(),
			),
		},
	}
}
