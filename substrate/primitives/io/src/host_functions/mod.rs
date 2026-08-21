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
