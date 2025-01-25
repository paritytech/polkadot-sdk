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

/// The Aura ID used by the Aura consensus
#[derive(PartialEq)]
pub enum AuraConsensusId {
	/// Ed25519
	Ed25519,
	/// Sr25519
	Sr25519,
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
			log::info!("Unable to check metadata. Skipping metadata checks. Metadata checks are supported for metadata versions v14 and higher.");
			return Ok(Runtime::Omni(BlockNumber::U32, Consensus::Aura(AuraConsensusId::Sr25519)))
		};

		let block_number = match metadata_inspector.block_number() {
			Some(inner) => inner,
			None => {
				log::warn!(
					r#"⚠️  There isn't a runtime type named `System`, corresponding to the `frame-system`
                pallet (https://docs.rs/frame-system/latest/frame_system/). Please check Omni Node docs for runtime conventions:
                https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html#runtime-conventions.
                Note: We'll assume a block number size of `u32`."#
				);
				BlockNumber::U32
			},
		};

		if !metadata_inspector.pallet_exists(DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME) {
			log::warn!(
				r#"⚠️  The parachain system pallet (https://docs.rs/crate/cumulus-pallet-parachain-system/latest) is
			   missing from the runtime’s metadata. Please check Omni Node docs for runtime conventions:
			   https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html#runtime-conventions."#
			);
		}

		Ok(Runtime::Omni(block_number, Consensus::Aura(AuraConsensusId::Sr25519)))
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

		Metadata::decode(&mut (*opaque_metadata).as_slice()).map_err(Into::into)
	}
}

#[cfg(test)]
mod tests {
	use crate::runtime::{
		BlockNumber, MetadataInspector, DEFAULT_FRAME_SYSTEM_PALLET_NAME,
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
}
