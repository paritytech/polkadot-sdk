// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

pub mod cmd;
pub mod read;
pub mod template;
pub mod write;

pub use cmd::StorageCmd;

/// Empirically, the maximum batch size for block validation should be no more than 10,000.
/// Bigger sizes may cause problems with runtime memory allocation.
pub(crate) const MAX_BATCH_SIZE_FOR_BLOCK_VALIDATION: usize = 10_000;

pub(crate) fn get_wasm_module() -> Box<dyn sc_executor_common::wasm_runtime::WasmModule> {
	let blob = sc_executor_common::runtime_blob::RuntimeBlob::uncompress_if_needed(
		frame_storage_access_test_runtime::WASM_BINARY
			.expect("You need to build the WASM binaries to run the benchmark!"),
	)
	.expect("Failed to create runtime blob");
	let config = sc_executor_wasmtime::Config {
		allow_missing_func_imports: true,
		cache_path: None,
		semantics: sc_executor_wasmtime::Semantics {
			heap_alloc_strategy: sc_executor_common::wasm_runtime::HeapAllocStrategy::Dynamic {
				maximum_pages: Some(4096),
			},
			instantiation_strategy: sc_executor::WasmtimeInstantiationStrategy::PoolingCopyOnWrite,
			deterministic_stack_limit: None,
			canonicalize_nans: false,
			parallel_compilation: false,
			wasm_multi_value: false,
			wasm_bulk_memory: false,
			wasm_reference_types: false,
			wasm_simd: false,
		},
	};

	Box::new(
		sc_executor_wasmtime::create_runtime::<sp_io::SubstrateHostFunctions>(blob, config)
			.expect("Unable to create wasm module."),
	)
}
