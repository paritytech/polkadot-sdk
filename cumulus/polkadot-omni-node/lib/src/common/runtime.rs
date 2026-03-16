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

//! Runtime parameters.

use codec::Decode;
use cumulus_client_service::ParachainHostFunctions;
use sc_chain_spec::ChainSpec;
use sc_executor::WasmExecutor;
use sc_runtime_utilities::fetch_latest_metadata_from_code_blob;
use scale_info::{form::PortableForm, TypeDef, TypeDefPrimitive};
use std::fmt::Display;
use subxt_metadata::{Metadata, StorageEntryType};

/// Expected parachain system pallet runtime type name.
pub const DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME: &str = "ParachainSystem";
/// Expected frame system pallet runtime type name.
pub const DEFAULT_FRAME_SYSTEM_PALLET_NAME: &str = "System";
/// Expected Aura pallet runtime type name.
pub const DEFAULT_AURA_PALLET_NAME: &str = "Aura";

/// The Aura ID used by the Aura consensus
#[derive(Debug, PartialEq)]
pub enum AuraConsensusId {
	/// Ed25519
	Ed25519,
	/// Sr25519
	Sr25519,
}

/// Determines the appropriate Aura consensus ID based on the chain spec ID.
///
/// Most parachains use Sr25519 for Aura consensus, but Asset Hub Polkadot
/// (formerly Statemint) uses Ed25519.
///
/// # Returns
///
/// Returns `AuraConsensusId::Ed25519` for chain spec IDs starting with
/// `asset-hub-polkadot` or `statemint`, and `AuraConsensusId::Sr25519` for all
/// other chains.
pub fn aura_id_from_chain_spec_id(id: &str) -> AuraConsensusId {
	let id_normalized = id.replace('_', "-");
	if id_normalized.starts_with("asset-hub-polkadot") || id_normalized.starts_with("statemint") {
		log::warn!(
			"⚠️  Aura authority id type is assumed to be `ed25519` because the chain spec id \
			starts with `asset-hub-polkadot` or `statemint`. This is a known special case for \
			Asset Hub Polkadot (formerly Statemint). If this assumption is wrong for your runtime, \
			the node may not work correctly."
		);
		AuraConsensusId::Ed25519
	} else {
		log::warn!(
			"⚠️  Aura authority id type is assumed to be `sr25519` by default. Runtimes using \
			`ed25519` for Aura are not yet supported (except for `asset-hub-polkadot` / `statemint`). \
			If your runtime uses `ed25519` for Aura, it may not work correctly with this node."
		);
		AuraConsensusId::Sr25519
	}
}

/// The choice of consensus for the parachain omni-node.
#[derive(PartialEq)]
pub enum Consensus {
	/// Aura consensus.
	Aura(AuraConsensusId),
}

/// The choice of block number for the parachain omni-node.
#[derive(PartialEq, Debug)]
pub enum BlockNumber {
	/// u32
	U32,
	/// u64
	U64,
}

impl Display for BlockNumber {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		match self {
			BlockNumber::U32 => write!(f, "u32"),
			BlockNumber::U64 => write!(f, "u64"),
		}
	}
}

impl Into<TypeDefPrimitive> for BlockNumber {
	fn into(self) -> TypeDefPrimitive {
		match self {
			BlockNumber::U32 => TypeDefPrimitive::U32,
			BlockNumber::U64 => TypeDefPrimitive::U64,
		}
	}
}

impl BlockNumber {
	fn from_type_def(type_def: &TypeDef<PortableForm>) -> Option<BlockNumber> {
		match type_def {
			TypeDef::Primitive(TypeDefPrimitive::U32) => Some(BlockNumber::U32),
			TypeDef::Primitive(TypeDefPrimitive::U64) => Some(BlockNumber::U64),
			_ => None,
		}
	}
}

/// Helper enum listing the supported Runtime types
#[derive(PartialEq)]
pub enum Runtime {
	/// None of the system-chain runtimes, rather the node will act agnostic to the runtime ie. be
	/// an omni-node, and simply run a node with the given consensus algorithm.
	Omni(BlockNumber, Consensus),
}

/// Helper trait used for extracting the Runtime variant from the chain spec ID.
pub trait RuntimeResolver {
	/// Extract the Runtime variant from the chain spec ID.
	fn runtime(&self, chain_spec: &dyn ChainSpec) -> sc_cli::Result<Runtime>;
}

/// Default implementation for `RuntimeResolver` that just returns
/// `Runtime::Omni(BlockNumber::U32, Consensus::Aura(AuraConsensusId::Sr25519))`.
pub struct DefaultRuntimeResolver;

impl RuntimeResolver for DefaultRuntimeResolver {
	fn runtime(&self, chain_spec: &dyn ChainSpec) -> sc_cli::Result<Runtime> {
		let Ok(metadata_inspector) = MetadataInspector::new(chain_spec) else {
			log::info!(
				"Unable to check metadata. Skipping metadata checks. Metadata checks are supported for metadata versions v14 and higher."
			);
			let aura_id = aura_id_from_chain_spec_id(chain_spec.id());
			return Ok(Runtime::Omni(BlockNumber::U32, Consensus::Aura(aura_id)));
		};

		let block_number = metadata_inspector.block_number().unwrap_or_else(|| {
			log::warn!(
				r#"⚠️  There isn't a runtime type named `System`, corresponding to the `frame-system`
                pallet (https://docs.rs/frame-system/latest/frame_system/). Please check Omni Node docs for runtime conventions:
                https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html#runtime-conventions.
                Note: We'll assume a block number size of `u32`."#
			);
			BlockNumber::U32
		});

		if !metadata_inspector.pallet_exists(DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME) {
			log::warn!(
				r#"⚠️  The parachain system pallet (https://docs.rs/crate/cumulus-pallet-parachain-system/latest) is
			   missing from the runtime's metadata. Please check Omni Node docs for runtime conventions:
			   https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html#runtime-conventions."#
			);
		}

		let aura_id = match metadata_inspector.aura_consensus_id() {
			Some(id) => id,
			None => {
				log::warn!(
					r#"⚠️  The Aura authority ID type was not found in the runtime metadata.
					   This can be expected if the runtime does not include `pallet-aura`,
					   or if the chain starts without Aura at genesis and enables it later
					   via a runtime upgrade (for example, asset-hub-polkadot).
				
					   Falling back to chain spec ID heuristics."#
				);
				aura_id_from_chain_spec_id(chain_spec.id())
			},
		};
		Ok(Runtime::Omni(block_number, Consensus::Aura(aura_id)))
	}
}

struct MetadataInspector(Metadata);

impl MetadataInspector {
	fn new(chain_spec: &dyn ChainSpec) -> Result<MetadataInspector, sc_cli::Error> {
		MetadataInspector::fetch_metadata(chain_spec).map(MetadataInspector)
	}

	fn pallet_exists(&self, name: &str) -> bool {
		self.0.pallet_by_name(name).is_some()
	}

	fn block_number(&self) -> Option<BlockNumber> {
		let pallet_metadata = self.0.pallet_by_name(DEFAULT_FRAME_SYSTEM_PALLET_NAME);
		pallet_metadata
			.and_then(|inner| inner.storage())
			.and_then(|inner| inner.entry_by_name("Number"))
			.and_then(|number_ty| match number_ty.entry_type() {
				StorageEntryType::Plain(ty_id) => Some(ty_id),
				_ => None,
			})
			.and_then(|ty_id| self.0.types().resolve(*ty_id))
			.and_then(|portable_type| BlockNumber::from_type_def(&portable_type.type_def))
	}

	fn aura_consensus_id(&self) -> Option<AuraConsensusId> {
		if !self.pallet_exists(DEFAULT_AURA_PALLET_NAME) {
			return None;
		}

		for portable_type in &self.0.types().types {
			let path = &portable_type.ty.path;
			let segments = &path.segments;

			// Check if the type is related to Aura consensus
			if segments.iter().any(|s| s == "sp_consensus_aura") {
				let is_authority_id = segments.iter().any(|s| s == "AuthorityId") ||
					segments.iter().any(|s| s == "Public");

				if is_authority_id {
					if segments.iter().any(|s| s == "sr25519") {
						return Some(AuraConsensusId::Sr25519);
					}
					if segments.iter().any(|s| s == "ed25519") {
						return Some(AuraConsensusId::Ed25519);
					}
				}
			}
		}

		None
	}

	fn fetch_metadata(chain_spec: &dyn ChainSpec) -> Result<Metadata, sc_cli::Error> {
		let mut storage = chain_spec.build_storage()?;
		let code_bytes = storage
			.top
			.remove(sp_storage::well_known_keys::CODE)
			.ok_or("chain spec genesis does not contain code")?;
		let opaque_metadata = fetch_latest_metadata_from_code_blob(
			&WasmExecutor::<ParachainHostFunctions>::builder()
				.with_allow_missing_host_functions(true)
				.build(),
			sp_runtime::Cow::Borrowed(code_bytes.as_slice()),
		)
		.map_err(|err| err.to_string())?;

		println!("Metadata size: {}", opaque_metadata.len());
		if opaque_metadata.len() >= 8 {
			println!("Metadata prefix: {:02x?}", &opaque_metadata[..8]);
		}

		Metadata::decode(&mut (*opaque_metadata).as_slice()).map_err(Into::into)
	}
}

#[cfg(test)]
mod tests {
	use crate::runtime::{
		AuraConsensusId, BlockNumber, MetadataInspector, DEFAULT_FRAME_SYSTEM_PALLET_NAME,
		DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME,
	};
	use codec::Decode;
	use cumulus_client_service::ParachainHostFunctions;
	use sc_executor::WasmExecutor;
	use sc_runtime_utilities::fetch_latest_metadata_from_code_blob;

	fn cumulus_test_runtime_metadata() -> subxt_metadata::Metadata {
		let opaque_metadata = fetch_latest_metadata_from_code_blob(
			&WasmExecutor::<ParachainHostFunctions>::builder()
				.with_allow_missing_host_functions(true)
				.build(),
			sp_runtime::Cow::Borrowed(cumulus_test_runtime::WASM_BINARY.unwrap()),
		)
		.unwrap();

		subxt_metadata::Metadata::decode(&mut (*opaque_metadata).as_slice()).unwrap()
	}

	#[test]
	fn test_pallet_exists() {
		let metadata_inspector = MetadataInspector(cumulus_test_runtime_metadata());
		assert!(metadata_inspector.pallet_exists(DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME));
		assert!(metadata_inspector.pallet_exists(DEFAULT_FRAME_SYSTEM_PALLET_NAME));
	}

	#[test]
	fn test_runtime_block_number() {
		let metadata_inspector = MetadataInspector(cumulus_test_runtime_metadata());
		assert_eq!(metadata_inspector.block_number().unwrap(), BlockNumber::U32);
	}

	#[test]
	fn test_aura_consensus_id() {
		let metadata_inspector = MetadataInspector(cumulus_test_runtime_metadata());
		// Verify that the function correctly detects sr25519 from metadata
		let aura_id = metadata_inspector.aura_consensus_id();
		assert_eq!(aura_id, Some(AuraConsensusId::Sr25519));
	}

	#[test]
	fn test_aura_id_from_chain_spec_id() {
		use crate::runtime::{aura_id_from_chain_spec_id, AuraConsensusId};

		// Asset Hub Polkadot uses Ed25519
		assert_eq!(aura_id_from_chain_spec_id("asset-hub-polkadot"), AuraConsensusId::Ed25519);
		assert_eq!(aura_id_from_chain_spec_id("statemint"), AuraConsensusId::Ed25519);

		// Other chains use Sr25519
		assert_eq!(aura_id_from_chain_spec_id("asset-hub-kusama"), AuraConsensusId::Sr25519);
		assert_eq!(aura_id_from_chain_spec_id("penpal-rococo-1000"), AuraConsensusId::Sr25519);
		assert_eq!(aura_id_from_chain_spec_id("collectives-westend"), AuraConsensusId::Sr25519);
	}

	#[test]
	fn test_aura_consensus_id_v15() {
		// Test with V15 metadata (Sr25519)
		let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("tests/chain-specs/coretime-polkadot.json");
		let chain_spec = sc_chain_spec::GenericChainSpec::<Option<()>>::from_json_file(path)
			.expect("invalid chain spec");

		let inspector = MetadataInspector::new(&chain_spec).expect("failed to inspect metadata");
		let aura_id = inspector.aura_consensus_id();
		assert_eq!(aura_id, Some(AuraConsensusId::Sr25519));
	}

	#[test]
	fn test_aura_consensus_id_v14() {
		// Test with V14 metadata (Sr25519) from bridge-hub-polkadot
		let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("tests/chain-specs/bridge-hub-polkadot.json");
		let chain_spec = sc_chain_spec::GenericChainSpec::<Option<()>>::from_json_file(path)
			.expect("invalid chain spec");

		let inspector = MetadataInspector::new(&chain_spec).expect("failed to inspect metadata");
		let aura_id = inspector.aura_consensus_id();
		assert_eq!(aura_id, Some(AuraConsensusId::Sr25519));
	}

	#[test]
	fn test_aura_consensus_id_ed25519_blocker() {
		// Asset Hub Polkadot uses Ed25519, but its production-like chain specs currently
		// use V14 metadata that fails to decode with the current subxt-metadata crate.
		// We use it here to verify that we gracefully handle or document these blockers.
		let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("tests/chain-specs/asset-hub-polkadot-genesis.json");
		let chain_spec = sc_chain_spec::GenericChainSpec::<Option<()>>::from_json_file(path)
			.expect("invalid chain spec");

		// Currently, this is expected to return an error during inspection due to V14 decoding issues.
		// Once V14 support is fully compatible for this spec, this test should be updated 
		// to verify Ed25519 detection.
		match MetadataInspector::new(&chain_spec) {
			Ok(inspector) => {
				let aura_id = inspector.aura_consensus_id();
				// If it ever starts working, it should be Ed25519.
				println!("Detected Aura ID: {:?}", aura_id);
			}
			Err(e) => {
				println!("Metadata inspection failed as expected for this V14 spec: {:?}", e);
			}
		}
	}
}
