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
//! ## Host function sets
//!
//! The crate provides two mutually exclusive sets of host functions, selected at compile time:
//! the Polkadot set (`polkadot_io.rs`, the default) and the JAM set (`jam_io.rs`, compiled with
//! `--cfg jam`). The JAM set consists of the host functions specified by RFC-145, which never
//! make the host allocate runtime memory; the runtime manages its own heap instead. The Rust
//! API of both sets is the same, so runtime code compiles unchanged against either of them.
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

// The host function set: the Polkadot one by default, the JAM one with `--cfg jam`.
#[cfg_attr(not(jam), path = "polkadot_io.rs")]
#[cfg_attr(jam, path = "jam_io.rs")]
mod io;
pub use io::*;

// The allocator of the runtime: the Polkadot set relies on the host-side allocator, the JAM set
// manages the heap of the runtime itself.
#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, not(jam)))]
mod global_alloc;
#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, jam, target_arch = "riscv64"))]
mod global_alloc_riscv;
#[cfg(all(not(feature = "disable_allocator"), substrate_runtime, jam, target_family = "wasm"))]
mod global_alloc_wasm;
