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

use alloc::{vec, vec::Vec};

#[cfg(not(substrate_runtime))]
use sp_core::hexdisplay::HexDisplay;

use sp_runtime_interface::{
	pass_by::{
		AllocateAndReturnByCodec, ConvertAndReturnAs, PassFatPointerAndRead, PassFatPointerAndWrite,
	},
	runtime_interface,
};

#[cfg(not(substrate_runtime))]
use sp_externalities::ExternalitiesExt;

use crate::*;

/// Interface that provides miscellaneous functions for communicating between the runtime and the
/// node.
#[runtime_interface]
pub trait Misc {
	// NOTE: We use the target 'runtime' for messages produced by general printing functions,
	// instead of LOG_TARGET.

	/// Print a number.
	fn print_num(val: u64) {
		log::debug!(target: "runtime", "{}", val);
	}

	/// Print any valid `utf8` buffer.
	fn print_utf8(utf8: PassFatPointerAndRead<&[u8]>) {
		if let Ok(data) = core::str::from_utf8(utf8) {
			log::debug!(target: "runtime", "{}", data)
		}
	}

	/// Print any `u8` slice as hex.
	fn print_hex(data: PassFatPointerAndRead<&[u8]>) {
		log::debug!(target: "runtime", "{}", HexDisplay::from(&data));
	}

	/// Extract the runtime version of the given wasm blob by calling `Core_version`.
	///
	/// Returns `None` if calling the function failed for any reason or `Some(Vec<u8>)` where
	/// the `Vec<u8>` holds the SCALE encoded runtime version.
	///
	/// # Performance
	///
	/// This function may be very expensive to call depending on the wasm binary. It may be
	/// relatively cheap if the wasm binary contains version information. In that case,
	/// uncompression of the wasm blob is the dominating factor.
	///
	/// If the wasm binary does not have the version information attached, then a legacy mechanism
	/// may be involved. This means that a runtime call will be performed to query the version.
	///
	/// Calling into the runtime may be incredible expensive and should be approached with care.
	fn runtime_version(
		&mut self,
		wasm: PassFatPointerAndRead<&[u8]>,
	) -> AllocateAndReturnByCodec<Option<Vec<u8>>> {
		use sp_core::traits::ReadRuntimeVersionExt;

		let mut ext = sp_state_machine::BasicExternalities::default();

		match self
			.extension::<ReadRuntimeVersionExt>()
			.expect("No `ReadRuntimeVersionExt` associated for the current context!")
			.read_runtime_version(wasm, &mut ext)
		{
			Ok(v) => Some(v),
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"cannot read version from the given runtime: {}",
					err,
				);
				None
			},
		}
	}

	/// Extract the runtime version of the given wasm blob by calling `Core_version`.
	///
	/// Returns `None` if calling the function failed for any reason. Otherwise, write the
	/// SCALE-encoded version information to the provided output buffer if it's large enough.
	/// Returns the full length of the encoded version information regardless of whether the
	/// buffer was written or not.
	///
	/// # Performance
	///
	/// This function may be very expensive to call depending on the wasm binary. It may be
	/// relatively cheap if the wasm binary contains version information. In that case,
	/// uncompression of the wasm blob is the dominating factor.
	///
	/// If the wasm binary does not have the version information attached, then a legacy mechanism
	/// may be involved. This means that a runtime call will be performed to query the version.
	///
	/// Calling into the runtime may be incredible expensive and should be approached with care.
	#[version(2)]
	#[raw_api]
	fn runtime_version(
		&mut self,
		wasm: PassFatPointerAndRead<&[u8]>,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		use sp_core::traits::ReadRuntimeVersionExt;

		let mut ext = sp_state_machine::BasicExternalities::default();

		match self
			.extension::<ReadRuntimeVersionExt>()
			.expect("No `ReadRuntimeVersionExt` associated for the current context!")
			.read_runtime_version(wasm, &mut ext)
		{
			Ok(v) => {
				if out.len() >= v.len() {
					out[..v.len()].copy_from_slice(v.as_slice());
				}
				Some(v.len() as u32)
			},
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"cannot read version from the given runtime: {}",
					err,
				);
				None
			},
		}
	}

	/// A convenience wrapper providing a developer-friendly interface for the `runtime_version`
	/// host function.
	#[wrapper]
	fn runtime_version(code: impl AsRef<[u8]>) -> Option<Vec<u8>> {
		let mut version = vec![0u8; 1024];
		let len = runtime_version__raw(code.as_ref(), &mut version[..])?;
		if len as usize > version.len() {
			version.resize(len as usize, 0);
			runtime_version__raw(code.as_ref(), &mut version[..])?;
		}
		version.truncate(len as usize);
		Some(version)
	}

	/// Get the last storage cursor stored by `storage::clear_prefix`,
	/// `default_child_storage::clear_prefix` and `default_child_storage::storage_kill`. Returns
	/// `None` if there is no stored cursor, otherwise returns the length of the cursor in bytes.
	/// The cursor is written to `out` and consumed only if `out` is large enough to hold it;
	/// otherwise, the cursor is retained and can be requested again with a larger buffer.
	// ERRATA: The RFC requires passing a raw pointer without a length, which is not safe.
	// Currently, we accept a fat pointer.
	fn last_cursor(
		&mut self,
		out: PassFatPointerAndWrite<&mut [u8]>,
	) -> ConvertAndReturnAs<Option<u32>, RIIntOption<u32>, i64> {
		let cursor = self.take_last_cursor()?;

		if out.len() >= cursor.len() {
			out[..cursor.len()].copy_from_slice(&cursor[..]);
		} else {
			self.store_last_cursor(&cursor[..]);
		}

		Some(cursor.len() as u32)
	}
}
