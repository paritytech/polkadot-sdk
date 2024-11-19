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

use std::fmt::Display;

use sc_chain_spec::ChainSpec;
use scale_info::{form::PortableForm, TypeDef, TypeDefPrimitive};

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
	fn runtime(&self, _chain_spec: &dyn ChainSpec) -> sc_cli::Result<Runtime> {
		Ok(Runtime::Omni(BlockNumber::U32, Consensus::Aura(AuraConsensusId::Sr25519)))
	}
}

/// Logic that inspects runtime's metadata for Omni Node compatibility.
pub mod metadata {
	use super::BlockNumber;
	use frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed};

	// Checks if pallet exists in runtime's metadata based on pallet name.
	fn pallet_exists(
		metadata: &RuntimeMetadataPrefixed,
		name: &str,
	) -> Result<bool, sc_service::error::Error> {
		match &metadata.1 {
			RuntimeMetadata::V14(inner) => Ok(inner.pallets.iter().any(|p| p.name == name)),
			RuntimeMetadata::V15(inner) => Ok(inner.pallets.iter().any(|p| p.name == name)),
			_ => Err(sc_service::error::Error::Application(
				anyhow::anyhow!(format!(
					"Metadata version {} not supported for checking against pallet existence.",
					metadata.0
				))
				.into(),
			)),
		}
	}

	// Get the configured runtime's block number type from `frame-system` pallet storage.
	fn block_number(
		metadata: &RuntimeMetadataPrefixed,
	) -> Result<BlockNumber, sc_service::error::Error> {
		// Macro to define reusable logic for processing metadata.
		macro_rules! process_metadata {
			($metadata:expr) => {{
				$metadata
					.pallets
					.iter()
					.find(|p| p.name == "System")
					.and_then(|system| system.storage.as_ref())
					.and_then(|storage| storage.entries.iter().find(|entry| entry.name == "Number"))
					.and_then(|number_ty| match number_ty.ty {
						frame_metadata::v14::StorageEntryType::Plain(ty) => Some(ty.id),
						_ => None,
					})
					.and_then(|number_id| $metadata.types.resolve(number_id))
					.and_then(|portable_type| BlockNumber::from_type_def(&portable_type.type_def))
			}};
		}

		let err_msg = "Can not get block number type from `frame-system-pallet` storage.";
		match &metadata.1 {
			RuntimeMetadata::V14(meta) => process_metadata!(meta).ok_or(sc_service::error::Error::Application(
					anyhow::anyhow!(err_msg).into())),
			RuntimeMetadata::V15(meta) => process_metadata!(meta).ok_or(sc_service::error::Error::Application(
					anyhow::anyhow!(err_msg).into())),
			_ =>
				Err(sc_service::error::Error::Application(
					anyhow::anyhow!(format!(
						"Metadata version {} not supported for checking block number type stored in `frame-system-pallet` storage.",
						metadata.0
					))
					.into(),
				)),
		}
	}

	/// Execute a set of checks to ensure runtime/parachain compatibility.
	///
	/// The checks will emit warning level logs in case the runtime doesn't comply with the
	/// conventions at https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html#runtime-conventions.
	pub fn verify_parachain_compatibility(
		metadata: &RuntimeMetadataPrefixed,
	) -> Result<(), sc_service::error::Error> {
		if !pallet_exists(&metadata, DEFAULT_PARACHAIN_SYSTEM_PALLET_NAME)? {
			log::warn!(
				r#"⚠️  The parachain system pallet (https://docs.rs/crate/cumulus-pallet-parachain-system/latest) is
			missing from the runtime’s metadata. Please check Omni Node docs for runtime conventions:
			https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html#runtime-conventions."#,
			);
		}

		let runtime_block_number = block_number(&metadata)?;
		if runtime_block_number != BlockNumber::U32 {
			log::warn!(
				r#"⚠️  Configured `frame-system` pallet and Omni Node block numbers mismatch, or there isn't a runtime
				type named `System`, corresponding to the `frame-system` pallet (https://docs.rs/frame-system/38.0.0/frame_system/).
				Please check Omni Node docs for runtime conventions:
				https://paritytech.github.io/polkadot-sdk/master/polkadot_sdk_docs/reference_docs/omni_node/index.html#runtime-conventions."#,
			);
		}

		Ok(())
	}

	#[cfg(test)]
	mod tests {
		use crate::runtime::BlockNumber;
		use codec::{Decode, Encode};
		use frame_metadata::RuntimeMetadataPrefixed;
		use sc_executor::WasmExecutor;
		use sp_core::traits::{CallContext, CodeExecutor, RuntimeCode, WrappedRuntimeCode};

		fn cumulus_test_runtime_metadata() -> RuntimeMetadataPrefixed {
			type HostFunctions = (
				// The allocator functions.
				sp_io::allocator::HostFunctions,
				// Logging is good to have for debugging issues.
				sp_io::logging::HostFunctions,
				// Give access to the "state", actually the state will be empty, but some chains
				// put constants into the state and this would panic at metadata generation. Thus,
				// we give them an empty state to not panic.
				sp_io::storage::HostFunctions,
				// The hashing functions.
				sp_io::hashing::HostFunctions,
			);

			let executor = WasmExecutor::<HostFunctions>::builder()
				.with_allow_missing_host_functions(true)
				.build();

			let wasm = cumulus_test_runtime::WASM_BINARY.expect("to get wasm blob. qed");
			let runtime_code = RuntimeCode {
				code_fetcher: &WrappedRuntimeCode(wasm.into()),
				heap_pages: None,
				// The hash is only used for caching and thus, not that important for our use case
				// here.
				hash: vec![1, 2, 3],
			};

			let metadata = executor
				.call(
					&mut sp_io::TestExternalities::default().ext(),
					&runtime_code,
					"Metadata_metadata_at_version",
					&14u32.encode(),
					CallContext::Offchain,
				)
				.0
				.expect("`Metadata::metadata_at_version` should exist. qed.");

			let metadata = Option::<Vec<u8>>::decode(&mut &metadata[..])
				.ok()
				.flatten()
				.expect("Metadata stable version support is required. qed.");

			RuntimeMetadataPrefixed::decode(&mut &metadata[..])
				.expect("Invalid encoded metadata. qed")
		}

		#[test]
		fn test_pallet_exist() {
			let metadata = cumulus_test_runtime_metadata();
			assert!(super::pallet_exists(&metadata, "ParachainSystem").unwrap());
			assert!(super::pallet_exists(&metadata, "System").unwrap());
		}

		#[test]
		fn test_runtime_block_number() {
			let metadata = cumulus_test_runtime_metadata();
			assert_eq!(super::block_number(&metadata).unwrap(), BlockNumber::U32);
		}
	}
}
