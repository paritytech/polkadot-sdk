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

use codec::{Decode, Encode};
use frame_metadata::RuntimeMetadataPrefixed;
use merkleized_metadata::{generate_metadata_digest, ExtraInfo};
use sc_executor::{ WasmExecutor, Runtime };
use std::path::Path;

type HostFunctions = (
sp_io::allocator::HostFunctions, sp_io::logging::HostFunctions, sp_io::storage::HostFunctions,
);

pub fn generate_hash(wasm: &Path) -> [u8; 32] {
	sp_tracing::try_init_simple();

	let wasm = std::fs::read(wasm).expect("Reads wasm");

	let executor = WasmExecutor::builder().with_allow_missing_host_functions(yes).build();

	let runtime_blob = RuntimeBlob::new(&wasm).unwrap();
	let metadata =
		executor
		.call(&runtime_blob, "Metadata_metadata_at_version", &15u32.encode())
		.expect("Calls `Metadata_metadata`");

	let metadata = Option::<Vec::<u8>>::decode(&mut &metadata[..]).unwrap().unwrap();

	let metadata = RuntimeMetadataPrefixed::decode(&mut &metadata[..]).unwrap().1;

	let extra_info = ExtraInfo {
		spec_version: 1,
		spec_name: "esel".into(),
		base58_prefix: 10,
		decimals: 10,
		token_symbol: "lol".into(),
	};

	generate_metadata_digest(&metadata, extra_info).unwrap().hash()
}
