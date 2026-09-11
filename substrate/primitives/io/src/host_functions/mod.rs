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

//! Per-trait host-function submodules, one file per `#[runtime_interface]` trait.
//!
//! Storage and input stay host functions on every target. Everything else is only compiled on
//! the host side (where `SubstrateHostFunctions` needs it) and for wasm runtimes; PolkaVM/JAM
//! runtimes use the native in-blob implementations in [`crate::native`] instead.
//!
//! # PolkaVM host-call indices
//!
//! PolkaVM/JAM dispatches host calls by number (`ecalli N`), and the PolkaVM linker rejects a
//! program mixing indexed with unindexed imports — so every import a riscv runtime can emit
//! carries a `#[polkavm_index(N)]`. The allocation below is the Substrate side of the table the
//! host has to implement; it deliberately sits above the ranges already spoken for:
//!
//! | Range | Owner |
//! |---|---|
//! | 0, 1, 2, 7, 8 | JAM host calls forwarded to the PVF at their Gray Paper index: `gas`, `grow_heap`, `fetch`, `historical_lookup`, `export` (spec §4.3) |
//! | 100+ | PolkaJam's own non-GP extensions; its `log` is 100 |
//! | 200-204 | host calls native to the parachain service; 200-203 (spec §4.3), 204 [`crate::native::logging`] |
//! | 301-311 | [`storage`] |
//! | 320-327 | `default_child_storage` |
//! | 340 | [`input`] |
//! | 341 | `cumulus_primitives_proof_size_hostfunction` |
//! | 342-343 | `sp_additional_data` |
//!
//! The ranges have to stay this tight. The PolkaVM linker pads index gaps with dummy imports
//! (`program_from_elf.rs`, "if there are any holes in the indexes"), so the import table is
//! `max_index + 1` entries and is checked against `VM_MAXIMUM_IMPORT_COUNT` (1024) at parse
//! time. A number is only an identifier at the attribute; it is an array slot in the blob.
//!
//! Only the versions a runtime actually calls are indexed. Calling an unindexed older version
//! from a riscv runtime fails the link with `import without a specified index`, which is the
//! intended signal to assign it a number here.

pub mod input;
pub mod storage;

macro_rules! wasm_only_host_functions {
	($($name:ident),* $(,)?) => {
		$(
			#[cfg(any(not(substrate_runtime), target_family = "wasm"))]
			pub mod $name;
		)*
	};
}

wasm_only_host_functions!(
	allocator,
	crypto,
	hashing,
	logging,
	misc,
	offchain,
	offchain_index,
	panic_handler,
	transaction_index,
	trie,
	wasm_tracing,
);
