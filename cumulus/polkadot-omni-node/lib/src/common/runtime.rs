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

use codec::{Decode, Encode};
use cumulus_client_service::ParachainHostFunctions;
use frame_metadata::RuntimeMetadataPrefixed;
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
		log::info!(
			"Omni Node strategy: BlockNumber={}, Consensus=Aura({:?})",
			block_number,
			aura_id
		);
		Ok(Runtime::Omni(block_number, Consensus::Aura(aura_id)))
	}
}

struct MetadataInspector {
	metadata: Metadata,
	#[allow(dead_code)]
	version: u32,
}

impl MetadataInspector {
	fn new(chain_spec: &dyn ChainSpec) -> Result<MetadataInspector, sc_cli::Error> {
		let (metadata, version) = MetadataInspector::fetch_metadata(chain_spec)?;
		log::info!("Detected runtime metadata version: V{}", version);
		Ok(MetadataInspector { metadata, version })
	}

	#[cfg(test)]
	fn version(&self) -> u32 {
		self.version
	}

	fn pallet_exists(&self, name: &str) -> bool {
		self.metadata.pallet_by_name(name).is_some()
	}

	fn block_number(&self) -> Option<BlockNumber> {
		let pallet_metadata = self.metadata.pallet_by_name(DEFAULT_FRAME_SYSTEM_PALLET_NAME);
		pallet_metadata
			.and_then(|inner| inner.storage())
			.and_then(|inner| inner.entry_by_name("Number"))
			.and_then(|number_ty| match number_ty.entry_type() {
				StorageEntryType::Plain(ty_id) => Some(ty_id),
				_ => None,
			})
			.and_then(|ty_id| self.metadata.types().resolve(*ty_id))
			.and_then(|portable_type| BlockNumber::from_type_def(&portable_type.type_def))
	}

	fn aura_consensus_id(&self) -> Option<AuraConsensusId> {
		let pallet = self.metadata.pallet_by_name(DEFAULT_AURA_PALLET_NAME)?;

		// 1. (Recommended) Try to find AuthorityId in the pallet's associated types.
		if let Some(ty_id) = pallet.associated_type_id("AuthorityId") {
			if let Some(id) = self.resolve_aura_id_from_type_id(ty_id) {
				return Some(id);
			}
		}

		// 2. (Robust Fallback) Check the "Authorities" storage item in the Aura pallet.
		// Some chain specs might not expose all associated types clearly, but storage is usually
		// present.
		if let Some(storage) = pallet.storage() {
			if let Some(entry) = storage.entry_by_name("Authorities") {
				if let StorageEntryType::Plain(ty_id) = entry.entry_type() {
					if let Some(id) = self.resolve_aura_id_from_type_id(*ty_id) {
						return Some(id);
					}
				}
			}
		}

		None
	}

	/// Resolves whether a given type ID represents an Sr25519 or Ed25519 Aura ID.
	fn resolve_aura_id_from_type_id(&self, type_id: u32) -> Option<AuraConsensusId> {
		let portable_type = self.metadata.types().resolve(type_id)?;
		let segments = &portable_type.path.segments;

		// Check if the type path contains sr25519 or ed25519.
		if segments.iter().any(|s| s.to_lowercase().contains("sr25519")) {
			return Some(AuraConsensusId::Sr25519);
		}
		if segments.iter().any(|s| s.to_lowercase().contains("ed25519")) {
			return Some(AuraConsensusId::Ed25519);
		}

		// If it's a wrapper, we traverse it without complex recursion for now to keep it clean.
		match &portable_type.type_def {
			TypeDef::Composite(composite) => {
				for field in &composite.fields {
					// We use format and parse here as a final fallback only if we really can't get
					// the ID. But we should be able to get it if we use the right types.
					// For now, let's just use the name if possible.
					if let Some(resolved) = self.resolve_aura_id_from_type_id_opaque(&field.ty) {
						return Some(resolved);
					}
				}
			},
			TypeDef::Sequence(seq) => {
				if let Some(resolved) = self.resolve_aura_id_from_type_id_opaque(&seq.type_param) {
					return Some(resolved);
				}
			},
			_ => {},
		}

		None
	}

	/// Fallback helper to resolve from an opaque symbol.
	fn resolve_aura_id_from_type_id_opaque<T: std::fmt::Debug>(
		&self,
		symbol: &T,
	) -> Option<AuraConsensusId> {
		let debug_str = format!("{:?}", symbol);
		// Parse the ID out of "UntrackedSymbol { id: 183, ... }" or similar
		let id = debug_str
			.split("id: ")
			.nth(1)
			.and_then(|s| s.split(',').next())
			.and_then(|s| s.trim().parse::<u32>().ok())
			.or_else(|| {
				debug_str
					.split('(')
					.nth(1)
					.and_then(|s| s.split(',').next())
					.and_then(|s| s.trim().parse::<u32>().ok())
			});

		if let Some(id) = id {
			self.resolve_aura_id_from_type_id(id)
		} else {
			None
		}
	}

	fn fetch_metadata(chain_spec: &dyn ChainSpec) -> Result<(Metadata, u32), sc_cli::Error> {
		let mut storage = chain_spec.build_storage()?;
		let code_bytes = storage
			.top
			.remove(sp_storage::well_known_keys::CODE)
			.ok_or("chain spec genesis does not contain code")?;
		let executor = WasmExecutor::<ParachainHostFunctions>::builder()
			.with_allow_missing_host_functions(true)
			.build();
		let opaque_metadata = fetch_latest_metadata_from_code_blob(
			&executor,
			sp_runtime::Cow::Borrowed(code_bytes.as_slice()),
		)
		.map_err(|err| err.to_string())?;

		let mut encoded = (*opaque_metadata).as_slice();
		MetadataInspector::fetch_metadata_from_bytes(&mut encoded)
	}

	fn fetch_metadata_from_bytes(mut encoded: &[u8]) -> Result<(Metadata, u32), sc_cli::Error> {
		let prefixed = RuntimeMetadataPrefixed::decode(&mut encoded).map_err(|e| {
			sc_cli::Error::Input(format!("failed to decode prefixed metadata: {e}").into())
		})?;

		let version = prefixed.1.version();

		// Transform into subxt-metadata.
		// subxt-metadata doesn't directly implement TryFrom<RuntimeMetadata>, so we decode it again
		// as subxt-metadata. This is "cleaner" because we use a robust metadata versioning check
		// first. We encode the full `RuntimeMetadataPrefixed` to include the magic number.
		let encoded = prefixed.encode();
		let metadata = Metadata::decode(&mut &encoded[..]).map_err(|e| {
			sc_cli::Error::Input(format!("failed to decode subxt metadata: {e}").into())
		})?;

		Ok((metadata, version))
	}
}

#[cfg(test)]
mod tests {
	use crate::runtime::{
		AuraConsensusId, BlockNumber, MetadataInspector, DEFAULT_FRAME_SYSTEM_PALLET_NAME,
		DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME,
	};
	use cumulus_client_service::ParachainHostFunctions;
	use sc_executor::WasmExecutor;
	use sc_runtime_utilities::fetch_latest_metadata_from_code_blob;

	fn test_inspector_for_runtime(wasm_binary: &[u8]) -> MetadataInspector {
		let opaque_metadata = fetch_latest_metadata_from_code_blob(
			&WasmExecutor::<ParachainHostFunctions>::builder()
				.with_allow_missing_host_functions(true)
				.build(),
			sp_runtime::Cow::Borrowed(wasm_binary),
		)
		.unwrap();
		let mut encoded = (*opaque_metadata).as_slice();
		let (metadata, version) =
			MetadataInspector::fetch_metadata_from_bytes(&mut encoded).unwrap();
		MetadataInspector { metadata, version }
	}

	fn cumulus_test_runtime_inspector() -> MetadataInspector {
		test_inspector_for_runtime(cumulus_test_runtime::WASM_BINARY.unwrap())
	}

	#[test]
	fn test_pallet_exists() {
		let inspector = cumulus_test_runtime_inspector();
		assert!(inspector.version() >= 14);
		assert!(inspector.pallet_exists(DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME));
		assert!(inspector.pallet_exists(DEFAULT_FRAME_SYSTEM_PALLET_NAME));
	}

	#[test]
	fn test_runtime_block_number() {
		let inspector = cumulus_test_runtime_inspector();
		assert!(inspector.version() >= 14);
		assert_eq!(inspector.block_number().unwrap(), BlockNumber::U32);
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
		assert_eq!(inspector.version(), 15);
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
		assert_eq!(inspector.version(), 14);
		let aura_id = inspector.aura_consensus_id();
		assert_eq!(aura_id, Some(AuraConsensusId::Sr25519));
	}

	#[test]
	fn test_aura_consensus_id_asset_hub_polkadot() {
		// Test with Asset Hub Polkadot metadata
		let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
			.join("tests/chain-specs/asset-hub-polkadot.json");
		let chain_spec = sc_chain_spec::GenericChainSpec::<Option<()>>::from_json_file(path)
			.expect("invalid chain spec");

		let inspector = MetadataInspector::new(&chain_spec).expect("failed to inspect metadata");
		assert!(inspector.version() >= 14);
		let aura_id = inspector.aura_consensus_id();

		// Asset Hub Polkadot uses Ed25519 for Aura.
		assert_eq!(aura_id, Some(AuraConsensusId::Ed25519));
	}
}
