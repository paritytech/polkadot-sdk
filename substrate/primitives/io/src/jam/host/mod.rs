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

//! The real JAM host calls.
//!
//! Every module here declares bare PolkaVM imports with
//! [`sp_jam_runtime_interface::host_calls!`] and marshals the arguments itself. The encodings
//! shared by several host calls are:
//!
//! - `&[u8]`: a pointer and a `u64` length.
//! - `Option<&[u8]>`: a pointer and a `u64` length, where a null pointer is `None`.
//! - `Option<u32>`: an `i64`, where `-1` is `None`.
//! - `bool`: a `u32`, where `0` is `false`.
//! - `Result<(), ()>`: an `i32`, where `0` is `Ok` and `-1` is `Err`.

pub mod crypto;
pub mod default_child_storage;
pub mod input;
pub mod jam;
pub mod offchain;
pub mod service;
pub mod storage;
pub mod transaction_index;

pub use offchain::offchain_index;

/// Runs `f` with the current externalities, panicking if there are none.
#[cfg(not(substrate_runtime))]
fn with_externalities<R>(f: impl FnOnce(&mut dyn sp_externalities::Externalities) -> R) -> R {
	sp_externalities::with_externalities(f)
		.expect("host call executed outside of an Externalities-provided environment.")
}

/// Encodes an `Option<u32>` as `i64`.
fn option_u32_into_ffi(value: Option<u32>) -> i64 {
	value.map_or(-1, i64::from)
}

/// Decodes an `Option<u32>` encoded as `i64`; `None` if `value` is invalid.
fn option_u32_from_ffi(value: i64) -> Option<Option<u32>> {
	match value {
		-1 => Some(None),
		value => u32::try_from(value).ok().map(Some),
	}
}

/// Decodes an `Option<u32>` returned by a host call.
#[cfg(substrate_runtime)]
fn option_u32_from_host(value: i64) -> Option<u32> {
	option_u32_from_ffi(value).expect("the host returns a valid `Option<u32>`; qed")
}

/// Decodes an `Option<u32>` argument of a host call.
#[cfg(not(substrate_runtime))]
fn option_u32_arg(value: i64) -> Result<Option<u32>, alloc::string::String> {
	option_u32_from_ffi(value).ok_or_else(|| alloc::format!("invalid `Option<u32>`: {value}"))
}

/// Encodes a `Result<(), ()>` as `i32`.
#[cfg(not(substrate_runtime))]
fn void_result_into_ffi(value: Result<(), ()>) -> i32 {
	if value.is_ok() {
		0
	} else {
		-1
	}
}

/// Decodes a `Result<(), ()>` returned by a host call.
#[cfg(substrate_runtime)]
fn void_result_from_host(value: i32) -> Result<(), ()> {
	match value {
		0 => Ok(()),
		-1 => Err(()),
		_ => panic!("the host returns a valid `Result<(), ()>`; qed"),
	}
}

/// Returns the pointer and length passing `value` as `Option<&[u8]>`.
#[cfg(substrate_runtime)]
fn optional_slice(value: Option<&[u8]>) -> (*const u8, u64) {
	value.map_or((core::ptr::null(), 0), |value| (value.as_ptr(), value.len() as u64))
}

/// Reads an `Option<&[u8]>` argument of a host call.
#[cfg(not(substrate_runtime))]
fn read_optional_slice(
	ctx: &mut dyn sp_jam_runtime_interface::FunctionContext,
	ptr: *const u8,
	len: u64,
) -> Result<Option<alloc::vec::Vec<u8>>, alloc::string::String> {
	if ptr.is_null() {
		Ok(None)
	} else {
		ctx.read_memory(ptr, len).map(Some)
	}
}
