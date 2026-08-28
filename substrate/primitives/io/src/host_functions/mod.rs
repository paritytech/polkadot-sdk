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
//! | 0-29 | parachain-service `HostCall` (spec §4.3) |
//! | 1 | `grow_heap`, fixed by the Gray Paper (`Ω_Gemini`) |
//! | 100 | `log` (`jam-pvm-common`, not part of the GP) |
//! | 200-216 | [`storage`] |
//! | 220-227 | `default_child_storage` |
//! | 240 | [`input`] |
//! | 241 | `cumulus_primitives_proof_size_hostfunction` |
//! | 242-243 | `sp_additional_data` |
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
