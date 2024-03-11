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

use codec::Decode;
use frame_metadata::RuntimeMetadataPrefixed;
use merkleized_metadata::{generate_metadata_digest, ExtraInfo};
use sc_executor_wasmtime::{
	create_runtime, Config, HeapAllocStrategy, InstantiationStrategy, RuntimeBlob, WasmModule,
};
use std::path::Path;

pub fn generate_hash(wasm: &Path) -> [u8; 32] {
	sp_tracing::try_init_simple();

	let wasm = std::fs::read(wasm).expect("Reads wasm");

	let runtime_blob = RuntimeBlob::new(&wasm).unwrap();
	let metadata =
		create_runtime::<(sp_io::allocator::HostFunctions, sp_io::logging::HostFunctions)>(
			runtime_blob,
			Config {
				allow_missing_func_imports: true,
				cache_path: None,
				semantics: sc_executor_wasmtime::Semantics {
					heap_alloc_strategy: HeapAllocStrategy::Dynamic { maximum_pages: None },
					instantiation_strategy: InstantiationStrategy::PoolingCopyOnWrite,
					deterministic_stack_limit: None,
					canonicalize_nans: false,
					parallel_compilation: true,
					wasm_multi_value: false,
					wasm_bulk_memory: false,
					wasm_reference_types: false,
					wasm_simd: false,
				},
			},
		)
		.expect("Creates a runtime")
		.new_instance()
		.unwrap()
		.call_export("Metadata_metadata", &[])
		.expect("Calls `Metadata_metadata`");

	let metadata = Vec::<u8>::decode(&mut &metadata[..]).unwrap();

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
