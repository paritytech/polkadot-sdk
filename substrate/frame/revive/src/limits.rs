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

//! Limits that are observeable by contract code.
//!
//! It is important to never change this limits without supporting the old limits
//! for already deployed contracts. This is what the [`crate::Contract::behaviour_version`]
//! is meant for. This is true for either increasing or decreasing the limit.
//!
//! Limits in this file are different from the limits configured on the [`Config`] trait which are
//! generally only affect actions that cannot be performed by a contract: For example things related
//! to deposits and weights are allowed to be changed as they are paid by root callers which
//! are not contracts.
//!
//! Exceptions to this rule apply: Limits in the [`code`] module can be increased
//! without emulating the old values for existing contracts. Reason is that those limits are only
//! applied **once** at code upload time. Since this action cannot be performed by contracts we
//! can change those limits without breaking existing contracts. Please keep in mind that we should
//! only ever **increase** those values but never decrease.

/// The maximum depth of the call stack.
///
/// A 0 means that no callings of other contracts are possible. In other words only the origin
/// called "root contract" is allowed to execute then.
pub const CALL_STACK_DEPTH: u32 = 10;

/// The maximum number of topics a call to [`crate::SyscallDoc::deposit_event`] can emit.
///
/// We set it to the same limit that ethereum has. It is unlikely to change.
pub const NUM_EVENT_TOPICS: u32 = 4;

/// The maximum number of code hashes a contract can lock.
pub const DELEGATE_DEPENDENCIES: u32 = 32;

/// Maximum size of events (including topics) and storage values.
pub const PAYLOAD_BYTES: u32 = 512;

/// The maximum size of the transient storage in bytes.
///
/// This includes keys, values, and previous entries used for storage rollback.
pub const TRANSIENT_STORAGE_BYTES: u32 = 4 * 1024;

/// The maximum allowable length in bytes for (transient) storage keys.
pub const STORAGE_KEY_BYTES: u32 = 128;

/// The maximum size of the debug buffer contracts can write messages to.
///
/// The buffer will always be disabled for on-chain execution.
pub const DEBUG_BUFFER_BYTES: u32 = 2 * 1024 * 1024;

/// The page size in which PolkaVM should allocate memory chunks.
pub const PAGE_SIZE: u32 = 4 * 1024;

/// Limits that are only enforced on code upload.
///
/// # Note
///
/// This limit can be increased later without breaking existing contracts
/// as it is only enforced at code upload time. Code already uploaded
/// will not be affected by those limits.
pub mod code {
	use super::PAGE_SIZE;
	use crate::{CodeVec, Config, Error, LOG_TARGET};
	use alloc::vec::Vec;
	use frame_support::ensure;
	use sp_runtime::DispatchError;

	/// The maximum length of a code blob in bytes.
	///
	/// This mostly exist to prevent parsing too big blobs and to
	/// have a maximum encoded length. The actual memory calculation
	/// is purely based off [`STATIC_MEMORY_BYTES`].
	pub const BLOB_BYTES: u32 = 256 * 1024;

	/// Maximum size the program is allowed to take in memory.
	///
	/// This includes data and code. Increasing this limit will allow
	/// for more code or more data. However, since code will decompress
	/// into a bigger representation on compilation it will only increase
	/// the allowed code size by [`BYTE_PER_INSTRUCTION`].
	pub const STATIC_MEMORY_BYTES: u32 = 1024 * 1024;

	/// How much memory each instruction will take in-memory after compilation.
	///
	/// This is `size_of<usize>() + 16`. But we don't use `usize` here so it isn't
	/// different on the native runtime (used for testing).
	const BYTES_PER_INSTRUCTION: u32 = 20;

	/// The code is stored multiple times as part of the compiled program.
	const EXTRA_OVERHEAD_PER_CODE_BYTE: u32 = 4;

	/// Make sure that the various program parts are within the defined limits.
	pub fn enforce<T: Config>(blob: Vec<u8>) -> Result<CodeVec, DispatchError> {
		fn round_page(n: u32) -> u64 {
			// performing the rounding in u64 in order to prevent overflow
			u64::from(n).next_multiple_of(PAGE_SIZE.into())
		}

		let blob: CodeVec = blob.try_into().map_err(|_| <Error<T>>::BlobTooLarge)?;

		let program = polkavm::ProgramBlob::parse(blob.as_slice().into()).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to parse polkavm blob: {err:?}");
			Error::<T>::CodeRejected
		})?;

		// this is O(n) but it allows us to be more precise
		let num_instructions = program.instructions().count() as u64;

		// The memory consumptions is the byte size of the whole blob,
		// minus the RO data payload in the blob,
		// minus the RW data payload in the blob,
		// plus the RO data in memory (which is always equal or bigger than the RO payload),
		// plus RW data in memory, plus stack size in memory.
		// plus the overhead of instructions in memory which is derived from the code
		// size itself and the number of instruction
		let memory_size = (blob.len() as u64)
			.saturating_add(round_page(program.ro_data_size()))
			.saturating_sub(program.ro_data().len() as u64)
			.saturating_add(round_page(program.rw_data_size()))
			.saturating_sub(program.rw_data().len() as u64)
			.saturating_add(round_page(program.stack_size()))
			.saturating_add((num_instructions).saturating_mul(BYTES_PER_INSTRUCTION.into()))
			.saturating_add(
				(program.code().len() as u64).saturating_mul(EXTRA_OVERHEAD_PER_CODE_BYTE.into()),
			);

		ensure!(memory_size <= STATIC_MEMORY_BYTES as u64, <Error<T>>::StaticMemoryTooLarge);

		Ok(blob)
	}
}
