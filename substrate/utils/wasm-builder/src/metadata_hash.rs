// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

use crate::builder::MetadataExtraInfo;
use codec::{Decode, Encode};
use frame_metadata::{RuntimeMetadata, RuntimeMetadataPrefixed};
use merkleized_metadata::{generate_metadata_digest, ExtraInfo};
use sc_executor::WasmExecutor;
use sp_core::traits::{CallContext, CodeExecutor, RuntimeCode, WrappedRuntimeCode};
use std::path::Path;

/// The host functions that we provide when calling into the wasm file.
///
/// Any other host function will return an error.
type HostFunctions = (
	// The allocator functions.
	sp_io::allocator::HostFunctions,
	// Logging is good to have for debugging issues.
	sp_io::logging::HostFunctions,
	// Give access to the "state", actually the state will be empty, but some chains put constants
	// into the state and this would panic at metadata generation. Thus, we give them an empty
	// state to not panic.
	sp_io::storage::HostFunctions,
	// The hashing functions.
	sp_io::hashing::HostFunctions,
);

/// Generate the metadata hash.
///
/// The metadata hash is generated as specced in
/// [RFC78](https://polkadot-fellows.github.io/RFCs/approved/0078-merkleized-metadata.html).
///
/// Returns the metadata hash.
pub fn generate_metadata_hash(wasm: &Path, extra_info: MetadataExtraInfo) -> [u8; 32] {
	sp_tracing::try_init_simple();

	let wasm = std::fs::read(wasm).expect("Wasm file was just created and should be readable.");

	let executor = WasmExecutor::<HostFunctions>::builder()
		.with_allow_missing_host_functions(true)
		.build();

	let runtime_code = RuntimeCode {
		code_fetcher: &WrappedRuntimeCode(wasm.into()),
		heap_pages: None,
		// The hash is only used for caching and thus, not that important for our use case here.
		hash: vec![1, 2, 3],
	};

	let metadata = executor
		.call(
			&mut sp_io::TestExternalities::default().ext(),
			&runtime_code,
			"Metadata_metadata_at_version",
			&15u32.encode(),
			CallContext::Offchain,
		)
		.0
		.expect("`Metadata::metadata_at_version` should exist.");

	let metadata = Option::<Vec<u8>>::decode(&mut &metadata[..])
		.ok()
		.flatten()
		.expect("Metadata V15 support is required.");

	let metadata = RuntimeMetadataPrefixed::decode(&mut &metadata[..])
		.expect("Invalid encoded metadata?")
		.1;

	let runtime_version = executor
		.call(
			&mut sp_io::TestExternalities::default().ext(),
			&runtime_code,
			"Core_version",
			&[],
			CallContext::Offchain,
		)
		.0
		.expect("`Core_version` should exist.");
	let runtime_version = sp_version::RuntimeVersion::decode(&mut &runtime_version[..])
		.expect("Invalid `RuntimeVersion` encoding");

	let base58_prefix = extract_ss58_prefix(&metadata);

	let extra_info = ExtraInfo {
		spec_version: runtime_version.spec_version,
		spec_name: runtime_version.spec_name.into(),
		base58_prefix,
		decimals: extra_info.decimals,
		token_symbol: extra_info.token_symbol,
	};

	generate_metadata_digest(&metadata, extra_info)
		.expect("Failed to generate the metadata digest")
		.hash()
}

/// Extract the `SS58` from the constants in the given `metadata`.
fn extract_ss58_prefix(metadata: &RuntimeMetadata) -> u16 {
	let RuntimeMetadata::V15(ref metadata) = metadata else {
		panic!("Metadata version 15 required")
	};

	let system = metadata
		.pallets
		.iter()
		.find(|p| p.name == "System")
		.expect("Each FRAME runtime has the `System` pallet; qed");

	system
		.constants
		.iter()
		.find_map(|c| {
			(c.name == "SS58Prefix")
				.then(|| u16::decode(&mut &c.value[..]).expect("SS58 is an `u16`; qed"))
		})
		.expect("`SS58PREFIX` exists in the `System` constants; qed")
}
