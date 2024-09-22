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

<<<<<<< HEAD
use parachains_common::{AccountId, Signature};
use sc_chain_spec::{ChainSpecExtension, ChainSpecGroup};
use serde::{Deserialize, Serialize};
use sp_core::{Pair, Public};
use sp_runtime::traits::{IdentifyAccount, Verify};
=======
use cumulus_primitives_core::ParaId;
pub(crate) use parachains_common::genesis_config_helpers::{
	get_account_id_from_seed, get_collator_keys_from_seed, get_from_seed,
};
use polkadot_parachain_lib::{
	chain_spec::{GenericChainSpec, LoadSpec},
	runtime::{
		AuraConsensusId, BlockNumber, Consensus, Runtime, RuntimeResolver as RuntimeResolverT,
	},
};
use sc_chain_spec::ChainSpec;
>>>>>>> 8735c66 (Moved presets to the testnet runtimes (#5327))

pub mod asset_hubs;
pub mod bridge_hubs;
pub mod collectives;
pub mod contracts;
pub mod coretime;
pub mod glutton;
pub mod penpal;
pub mod people;
pub mod rococo_parachain;
pub mod seedling;
pub mod shell;

/// The default XCM version to set in genesis config.
const SAFE_XCM_VERSION: u32 = xcm::prelude::XCM_VERSION;

<<<<<<< HEAD
/// Generic extensions for Parachain ChainSpecs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, ChainSpecGroup, ChainSpecExtension)]
pub struct Extensions {
	/// The relay chain of the Parachain.
	#[serde(alias = "relayChain", alias = "RelayChain")]
	pub relay_chain: String,
	/// The id of the Parachain.
	#[serde(alias = "paraId", alias = "ParaId")]
	pub para_id: u32,
}

impl Extensions {
	/// Try to get the extension from the given `ChainSpec`.
	pub fn try_get(chain_spec: &dyn sc_service::ChainSpec) -> Option<&Self> {
		sc_chain_spec::get_extension(chain_spec.extensions())
	}
}

/// Generic chain spec for all polkadot-parachain runtimes
pub type GenericChainSpec = sc_service::GenericChainSpec<Extensions>;

/// Helper function to generate a crypto pair from seed
pub fn get_from_seed<TPublic: Public>(seed: &str) -> <TPublic::Pair as Pair>::Public {
	TPublic::Pair::from_string(&format!("//{}", seed), None)
		.expect("static values are valid; qed")
		.public()
}

type AccountPublic = <Signature as Verify>::Signer;

/// Helper function to generate an account ID from seed
pub fn get_account_id_from_seed<TPublic: Public>(seed: &str) -> AccountId
where
	AccountPublic: From<<TPublic::Pair as Pair>::Public>,
{
	AccountPublic::from(get_from_seed::<TPublic>(seed)).into_account()
}

/// Generate collator keys from seed.
///
/// This function's return type must always match the session keys of the chain in tuple format.
pub fn get_collator_keys_from_seed<AuraId: Public>(seed: &str) -> <AuraId::Pair as Pair>::Public {
	get_from_seed::<AuraId>(seed)
=======
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

			// -- Starters
			"shell" => Box::new(shell::get_shell_chain_spec()),
			"seedling" => Box::new(seedling::get_seedling_chain_spec()),

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
				Box::new(glutton::glutton_westend_development_config(
					para_id.expect("Must specify parachain id"),
				))
			},
			id if id.starts_with("glutton-westend-local") => {
				let (_, _, para_id) = extract_parachain_id(&id, &["glutton-westend-local-"]);
				Box::new(glutton::glutton_westend_local_config(
					para_id.expect("Must specify parachain id"),
				))
			},
			// the chain spec as used for generating the upgrade genesis values
			id if id.starts_with("glutton-westend-genesis") => {
				let (_, _, para_id) = extract_parachain_id(&id, &["glutton-westend-genesis-"]);
				Box::new(glutton::glutton_westend_config(
					para_id.expect("Must specify parachain id"),
				))
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
	Shell,
	Seedling,
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

		if id.starts_with("shell") {
			LegacyRuntime::Shell
		} else if id.starts_with("seedling") {
			LegacyRuntime::Seedling
		} else if id.starts_with("asset-hub-polkadot") | id.starts_with("statemint") {
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
			LegacyRuntime::Shell | LegacyRuntime::Seedling => Runtime::Shell,
		})
	}
>>>>>>> 8735c66 (Moved presets to the testnet runtimes (#5327))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn can_decode_extension_camel_and_snake_case() {
		let camel_case = r#"{"relayChain":"relay","paraId":1}"#;
		let snake_case = r#"{"relay_chain":"relay","para_id":1}"#;
		let pascal_case = r#"{"RelayChain":"relay","ParaId":1}"#;

		let camel_case_extension: Extensions = serde_json::from_str(camel_case).unwrap();
		let snake_case_extension: Extensions = serde_json::from_str(snake_case).unwrap();
		let pascal_case_extension: Extensions = serde_json::from_str(pascal_case).unwrap();

		assert_eq!(camel_case_extension, snake_case_extension);
		assert_eq!(snake_case_extension, pascal_case_extension);
	}
}
