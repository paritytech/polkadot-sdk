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

use crate::chain_spec::{
	get_account_id_from_seed, get_collator_keys_from_seed, Extensions, GenericChainSpec,
	SAFE_XCM_VERSION,
};
use cumulus_primitives_core::ParaId;
use hex_literal::hex;
use parachains_common::{AccountId, AuraId};
use sc_service::ChainType;
use sp_core::{crypto::UncheckedInto, sr25519};

/// No relay chain suffix because the id is the same over all relay chains.
const CONTRACTS_PARACHAIN_ID: u32 = 1002;

/// The existential deposit is determined by the runtime "contracts-rococo".
const CONTRACTS_ROCOCO_ED: contracts_rococo_runtime::Balance =
	testnet_parachains_constants::rococo::currency::EXISTENTIAL_DEPOSIT;

pub fn contracts_rococo_development_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());

	GenericChainSpec::builder(
		contracts_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions {
			relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
			para_id: CONTRACTS_PARACHAIN_ID,
		},
	)
	.with_name("Contracts on Rococo Development")
	.with_id("contracts-rococo-dev")
	.with_chain_type(ChainType::Development)
	.with_genesis_config_patch(contracts_rococo_genesis(
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
	))
	.with_boot_nodes(Vec::new())
	.build()
}

pub fn contracts_rococo_local_config() -> GenericChainSpec {
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());

	GenericChainSpec::builder(
		contracts_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
		Extensions {
			relay_chain: "rococo-local".into(), // You MUST set this to the correct network!
			para_id: CONTRACTS_PARACHAIN_ID,
		},
	)
	.with_name("Contracts on Rococo")
	.with_id("contracts-rococo-local")
	.with_chain_type(ChainType::Local)
	.with_genesis_config_patch(contracts_rococo_genesis(
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
	))
	.with_properties(properties)
	.build()
}

pub fn contracts_rococo_config() -> GenericChainSpec {
	// Give your base currency a unit name and decimal places
	let mut properties = sc_chain_spec::Properties::new();
	properties.insert("tokenSymbol".into(), "ROC".into());
	properties.insert("tokenDecimals".into(), 12.into());

	GenericChainSpec::builder(
	 		contracts_rococo_runtime::WASM_BINARY.expect("WASM binary was not built, please build it!"),
			Extensions { relay_chain: "rococo".into(), para_id: CONTRACTS_PARACHAIN_ID }
		)
		.with_name("Contracts on Rococo")
		.with_id("contracts-rococo")
		.with_chain_type(ChainType::Live)
		.with_genesis_config_patch(contracts_rococo_genesis(
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
		))
		.with_boot_nodes(vec![
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
		])
		.with_properties(properties)
		.build()
}

fn contracts_rococo_genesis(
	invulnerables: Vec<(AccountId, AuraId)>,
	endowed_accounts: Vec<AccountId>,
	id: ParaId,
) -> serde_json::Value {
	serde_json::json!( {
		"balances": {
			"balances": endowed_accounts.iter().cloned().map(|k| (k, 1u64 << 60)).collect::<Vec<_>>(),
		},
		"parachainInfo": {
			"parachainId": id,
		},
		"collatorSelection": {
			"invulnerables": invulnerables.iter().cloned().map(|(acc, _)| acc).collect::<Vec<_>>(),
			"candidacyBond": CONTRACTS_ROCOCO_ED * 16,
		},
		"session": {
			"keys": invulnerables
				.into_iter()
				.map(|(acc, aura)| {
					(
						acc.clone(),                                    // account id
						acc,                                            // validator id
						contracts_rococo_runtime::SessionKeys { aura }, // session keys
					)
				})
				.collect::<Vec<_>>(),
		},
		// no need to pass anything to aura, in fact it will panic if we do. Session will take care
		// of this.
		"polkadotXcm": {
			"safeXcmVersion": Some(SAFE_XCM_VERSION),
		},
		"sudo": {
			"key": Some(sp_runtime::AccountId32::from(hex![
				"2681a28014e7d3a5bfb32a003b3571f53c408acbc28d351d6bf58f5028c4ef14"
			])),
		},
	})
}
