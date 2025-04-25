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

use cumulus_primitives_core::ParaId;
use polkadot_omni_node_lib::{
	chain_spec::{GenericChainSpec, LoadSpec},
	runtime::{
		AuraConsensusId, BlockNumber, Consensus, Runtime, RuntimeResolver as RuntimeResolverT,
	},
};
use sc_chain_spec::{ChainSpec, ChainType};
use yet_another_parachain::yet_another_parachain_config;

pub mod asset_hubs;
pub mod bridge_hubs;
pub mod collectives;
pub mod coretime;
pub mod glutton;
pub mod penpal;
pub mod people;
pub mod rococo_parachain;
pub mod yet_another_parachain;

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

/// Extracts the normalized chain id and parachain id from the input chain id.
/// (H/T to Phala for the idea)
/// E.g. "penpal-kusama-2004" yields ("penpal-kusama", Some(2004))
fn extract_parachain_id<'a>(
	id: &'a str,
	para_prefixes: &[&str],
) -> (&'a str, &'a str, Option<ParaId>) {
	for para_prefix in para_prefixes {
		if let Some(suffix) = id.strip_prefix(para_prefix) {
			let para_id: u32 = suffix.parse().expect("Invalid parachain-id suffix");
			return (&id[..para_prefix.len() - 1], id, Some(para_id.into()));
		}
	}

	(id, id, None)
}

#[derive(Debug)]
pub(crate) struct ChainSpecLoader;

impl LoadSpec for ChainSpecLoader {
	fn load_spec(&self, id: &str) -> Result<Box<dyn ChainSpec>, String> {
		Ok(match id {
			// - Default-like
			"staging" => Box::new(rococo_parachain::staging_rococo_parachain_local_config()),
			"tick" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/tick.json")[..],
			)?),
			"trick" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/trick.json")[..],
			)?),
			"track" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/track.json")[..],
			)?),

			// -- Asset Hub Polkadot
			"asset-hub-polkadot" | "statemint" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/asset-hub-polkadot.json")[..],
			)?),

			// -- Asset Hub Kusama
			"asset-hub-kusama" | "statemine" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/asset-hub-kusama.json")[..],
			)?),

			// -- Asset Hub Rococo
			"asset-hub-rococo-dev" => Box::new(asset_hubs::asset_hub_rococo_development_config()),
			"asset-hub-rococo-local" => Box::new(asset_hubs::asset_hub_rococo_local_config()),
			// the chain spec as used for generating the upgrade genesis values
			"asset-hub-rococo-genesis" => Box::new(asset_hubs::asset_hub_rococo_genesis_config()),
			"asset-hub-rococo" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/asset-hub-rococo.json")[..],
			)?),

			// -- Asset Hub Westend
			"asset-hub-westend-dev" | "westmint-dev" =>
				Box::new(asset_hubs::asset_hub_westend_development_config()),
			"asset-hub-westend-local" | "westmint-local" =>
				Box::new(asset_hubs::asset_hub_westend_local_config()),
			// the chain spec as used for generating the upgrade genesis values
			"asset-hub-westend-genesis" | "westmint-genesis" =>
				Box::new(asset_hubs::asset_hub_westend_config()),
			// the shell-based chain spec as used for syncing
			"asset-hub-westend" | "westmint" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/asset-hub-westend.json")[..],
			)?),

			// -- Polkadot Collectives
			"collectives-polkadot" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/collectives-polkadot.json")[..],
			)?),

			// -- Westend Collectives
			"collectives-westend-dev" =>
				Box::new(collectives::collectives_westend_development_config()),
			"collectives-westend-local" =>
				Box::new(collectives::collectives_westend_local_config()),
			"collectives-westend" => Box::new(GenericChainSpec::from_json_bytes(
				&include_bytes!("../../chain-specs/collectives-westend.json")[..],
			)?),

			// -- BridgeHub
			bridge_like_id
				if bridge_like_id.starts_with(bridge_hubs::BridgeHubRuntimeType::ID_PREFIX) =>
				bridge_like_id
					.parse::<bridge_hubs::BridgeHubRuntimeType>()
					.expect("invalid value")
					.load_config()?,

			// -- Coretime
			coretime_like_id
				if coretime_like_id.starts_with(coretime::CoretimeRuntimeType::ID_PREFIX) =>
				coretime_like_id
					.parse::<coretime::CoretimeRuntimeType>()
					.expect("invalid value")
					.load_config()?,

			// -- Penpal
			id if id.starts_with("penpal-rococo") => {
				let (_, _, para_id) = extract_parachain_id(&id, &["penpal-rococo-"]);
				Box::new(penpal::get_penpal_chain_spec(
					para_id.expect("Must specify parachain id"),
					"rococo-local",
				))
			},
			id if id.starts_with("penpal-westend") => {
				let (_, _, para_id) = extract_parachain_id(&id, &["penpal-westend-"]);
				Box::new(penpal::get_penpal_chain_spec(
					para_id.expect("Must specify parachain id"),
					"westend-local",
				))
			},

			// -- Glutton Westend
			id if id.starts_with("glutton-westend-dev") => {
				let (_, _, para_id) = extract_parachain_id(&id, &["glutton-westend-dev-"]);
				Box::new(glutton::glutton_westend_config(
					para_id.expect("Must specify parachain id"),
					ChainType::Development,
					"westend-dev",
				))
			},
			id if id.starts_with("glutton-westend-local") => {
				let (_, _, para_id) = extract_parachain_id(&id, &["glutton-westend-local-"]);
				Box::new(glutton::glutton_westend_config(
					para_id.expect("Must specify parachain id"),
					ChainType::Local,
					"westend-local",
				))
			},
			// the chain spec as used for generating the upgrade genesis values
			id if id.starts_with("glutton-westend-genesis") => {
				let (_, _, para_id) = extract_parachain_id(&id, &["glutton-westend-genesis-"]);
				Box::new(glutton::glutton_westend_config(
					para_id.expect("Must specify parachain id"),
					ChainType::Live,
					"westend",
				))
			},

			id if id.starts_with("yap-") => {
				let tok: Vec<String> = id.split('-').map(|s| s.to_owned()).collect();
				assert!(
					tok.len() == 4,
					"Invalid YAP chain id, should be 'yap-<relay>-<chaintype>-<para-id>'"
				);
				let relay = if &tok[2] == "live" { tok[1].clone() } else { tok[1..=2].join("-") };
				let chain_type = match tok[2].as_str() {
					"local" => ChainType::Local,
					"dev" => ChainType::Development,
					"live" => ChainType::Live,
					_ => unimplemented!("Unknown chain type {}", tok[2]),
				};
				let para_id: u32 =
					tok[3].parse().expect(&format!("Illegal para id '{}' provided", tok[3]));

				Box::new(yet_another_parachain_config(relay, chain_type, para_id))
			},

			// -- People
			people_like_id if people_like_id.starts_with(people::PeopleRuntimeType::ID_PREFIX) =>
				people_like_id
					.parse::<people::PeopleRuntimeType>()
					.expect("invalid value")
					.load_config()?,

			// -- Fallback (generic chainspec)
			"" => {
				log::warn!("No ChainSpec.id specified, so using default one, based on rococo-parachain runtime");
				Box::new(rococo_parachain::rococo_parachain_local_config())
			},

			// -- Loading a specific spec from disk
			path => Box::new(GenericChainSpec::from_json_file(path.into())?),
		})
	}
}

/// Helper enum that is used for better distinction of different parachain/runtime configuration
/// (it is based/calculated on ChainSpec's ID attribute)
#[derive(Debug, PartialEq)]
enum LegacyRuntime {
	Omni,
	AssetHubPolkadot,
	AssetHub,
	Penpal,
	Collectives,
	Glutton,
	BridgeHub(bridge_hubs::BridgeHubRuntimeType),
	Coretime(coretime::CoretimeRuntimeType),
	People(people::PeopleRuntimeType),
}

impl LegacyRuntime {
	fn from_id(id: &str) -> LegacyRuntime {
		let id = id.replace('_', "-");

		if id.starts_with("asset-hub-polkadot") | id.starts_with("statemint") {
			LegacyRuntime::AssetHubPolkadot
		} else if id.starts_with("asset-hub-kusama") |
			id.starts_with("statemine") |
			id.starts_with("asset-hub-rococo") |
			id.starts_with("rockmine") |
			id.starts_with("asset-hub-westend") |
			id.starts_with("westmint")
		{
			LegacyRuntime::AssetHub
		} else if id.starts_with("penpal") {
			LegacyRuntime::Penpal
		} else if id.starts_with("collectives-polkadot") || id.starts_with("collectives-westend") {
			LegacyRuntime::Collectives
		} else if id.starts_with(bridge_hubs::BridgeHubRuntimeType::ID_PREFIX) {
			LegacyRuntime::BridgeHub(
				id.parse::<bridge_hubs::BridgeHubRuntimeType>().expect("Invalid value"),
			)
		} else if id.starts_with(coretime::CoretimeRuntimeType::ID_PREFIX) {
			LegacyRuntime::Coretime(
				id.parse::<coretime::CoretimeRuntimeType>().expect("Invalid value"),
			)
		} else if id.starts_with("glutton") {
			LegacyRuntime::Glutton
		} else if id.starts_with(people::PeopleRuntimeType::ID_PREFIX) {
			LegacyRuntime::People(id.parse::<people::PeopleRuntimeType>().expect("Invalid value"))
		} else {
			log::warn!(
				"No specific runtime was recognized for ChainSpec's id: '{}', \
				so Runtime::Omni(Consensus::Aura) will be used",
				id
			);
			LegacyRuntime::Omni
		}
	}
}

#[derive(Debug)]
pub(crate) struct RuntimeResolver;

impl RuntimeResolverT for RuntimeResolver {
	fn runtime(&self, chain_spec: &dyn ChainSpec) -> sc_cli::Result<Runtime> {
		let legacy_runtime = LegacyRuntime::from_id(chain_spec.id());
		Ok(match legacy_runtime {
			LegacyRuntime::AssetHubPolkadot =>
				Runtime::Omni(BlockNumber::U32, Consensus::Aura(AuraConsensusId::Ed25519)),
			LegacyRuntime::AssetHub |
			LegacyRuntime::BridgeHub(_) |
			LegacyRuntime::Collectives |
			LegacyRuntime::Coretime(_) |
			LegacyRuntime::People(_) |
			LegacyRuntime::Glutton |
			LegacyRuntime::Penpal |
			LegacyRuntime::Omni =>
				Runtime::Omni(BlockNumber::U32, Consensus::Aura(AuraConsensusId::Sr25519)),
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup, ChainType, Extension};
	use serde::{Deserialize, Serialize};
	use sp_keyring::Sr25519Keyring;

	#[derive(
		Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension, Default,
	)]
	#[serde(deny_unknown_fields)]
	pub struct Extensions1 {
		pub attribute1: String,
		pub attribute2: u32,
	}

	#[derive(
		Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension, Default,
	)]
	#[serde(deny_unknown_fields)]
	pub struct Extensions2 {
		pub attribute_x: String,
		pub attribute_y: String,
		pub attribute_z: u32,
	}

	pub type DummyChainSpec<E> = sc_service::GenericChainSpec<E>;

	pub fn create_default_with_extensions<E: Extension>(
		id: &str,
		extension: E,
	) -> DummyChainSpec<E> {
		DummyChainSpec::builder(
			rococo_parachain_runtime::WASM_BINARY
				.expect("WASM binary was not built, please build it!"),
			extension,
		)
		.with_name("Dummy local testnet")
		.with_id(id)
		.with_chain_type(ChainType::Local)
		.with_genesis_config_patch(crate::chain_spec::rococo_parachain::testnet_genesis(
			Sr25519Keyring::Alice.to_account_id(),
			vec![Sr25519Keyring::Alice.public().into(), Sr25519Keyring::Bob.public().into()],
			vec![Sr25519Keyring::Bob.to_account_id()],
			1000.into(),
		))
		.build()
	}

	#[test]
	fn test_legacy_runtime_for_different_chain_specs() {
		let chain_spec =
			create_default_with_extensions("penpal-rococo-1000", Extensions2::default());
		assert_eq!(LegacyRuntime::Penpal, LegacyRuntime::from_id(chain_spec.id()));

		let chain_spec = crate::chain_spec::rococo_parachain::rococo_parachain_local_config();
		assert_eq!(LegacyRuntime::Omni, LegacyRuntime::from_id(chain_spec.id()));
	}
}
