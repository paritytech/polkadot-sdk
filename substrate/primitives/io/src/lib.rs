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

//! # Substrate Primitives: IO
//!
//! This crate contains interfaces for the runtime to communicate with the outside world, ergo `io`.
//! In other context, such interfaces are referred to as "**host functions**".
//!
//! Each set of host functions are defined with an instance of the
//! [`sp_runtime_interface::runtime_interface`] macro.
//!
//! Most notably, this crate contains host functions for:
//!
//! - [`hashing`]
//! - [`crypto`]
//! - [`trie`]
//! - [`offchain`]
//! - [`storage`]
//! - [`allocator`]
//! - [`logging`]
//!
//! All of the default host functions provided by this crate, and by default contained in all
//! substrate-based clients are amalgamated in [`SubstrateHostFunctions`].
//!
//! ## Externalities
//!
//! Host functions go hand in hand with the concept of externalities. Externalities are an
//! environment in which host functions are provided, and thus can be accessed. Some host functions
//! are only accessible in an externality environment that provides it.
//!
//! A typical error for substrate developers is the following:
//!
//! ```should_panic
//! use sp_io::storage::get;
//! # fn main() {
//! let data = get(b"hello world");
//! # }
//! ```
//!
//! This code will panic with the following error:
//!
//! ```no_compile
//! thread 'main' panicked at '`get_version_1` called outside of an Externalities-provided environment.'
//! ```
//!
//! Such error messages should always be interpreted as "code accessing host functions accessed
//! outside of externalities".
//!
//! An externality is any type that implements [`sp_externalities::Externalities`]. A simple example
//! of which is [`TestExternalities`], which is commonly used in tests and is exported from this
//! crate.
//!
//! ```
//! use sp_io::{storage::get, TestExternalities};
//! # fn main() {
//! TestExternalities::default().execute_with(|| {
//! 	let data = get(b"hello world");
//! });
//! # }
//! ```

#![warn(missing_docs)]
#![cfg_attr(not(feature = "std"), no_std)]
#![cfg_attr(enable_alloc_error_handler, feature(alloc_error_handler))]

extern crate alloc;

#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, not(jam)))]
mod global_alloc;

#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, jam, target_arch = "riscv64"))]
mod global_alloc_riscv;

/// Host functions for the relay chain runtime.
#[cfg(not(jam))]
pub mod relay_chain;
#[cfg(not(jam))]
pub use relay_chain::*;

/// Host functions for a JAM runtime.
///
/// Compiled for a JAM runtime blob (`--cfg jam`) and on the node side, where the PolkaVM
/// executor registers these host calls for a JAM blob. The glob re-export stays `--cfg jam`
/// only: on the node side the relay-chain items keep their `sp_io` paths.
#[cfg(any(jam, not(substrate_runtime)))]
pub mod jam;
#[cfg(jam)]
pub use jam::*;

#[cfg(all(jam, substrate_runtime, target_family = "wasm"))]
compile_error!(
	"`--cfg jam` runtimes must be built for PolkaVM (riscv64); the JAM host-call set has no wasm ABI."
);

/// A default panic handler for the JAM runtime environment.
///
/// The JAM host-call set has no `abort_on_panic` host call: the handler logs through the
/// (no-op in-runtime) logging bridge and then traps with the RISC-V `unimp` instruction.
#[cfg(all(not(feature = "disable_panic_handler"), substrate_runtime, jam))]
#[panic_handler]
pub fn panic(info: &core::panic::PanicInfo) -> ! {
	let message = alloc::format!("{}", info);
	logging::log(sp_core::RuntimeInterfaceLogLevel::Error, "runtime", message.as_bytes());
	unreachable();
}

/// A default OOM handler for the JAM runtime environment.
///
/// Like [`panic`], there is no `abort_on_panic` host call: the handler logs and then traps.
#[cfg(all(not(feature = "disable_oom"), enable_alloc_error_handler, jam))]
#[alloc_error_handler]
pub fn oom(_: core::alloc::Layout) -> ! {
	logging::log(
		sp_core::RuntimeInterfaceLogLevel::Error,
		"runtime",
		b"Runtime memory exhausted. Aborting",
	);
	unreachable();
}
