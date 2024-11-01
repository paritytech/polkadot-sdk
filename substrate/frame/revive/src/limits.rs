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
pub const CALL_STACK_DEPTH: u32 = 5;

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

/// The maximum amount of immutable bytes a single contract can store.
///
/// The current limit of 4kb allows storing up 16 U256 immutable variables.
/// Which should always be enough because Solidity allows for 16 local (stack) variables.
pub const IMMUTABLE_BYTES: u32 = 4 * 1024;

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
	pub const STATIC_MEMORY_BYTES: u32 = 2 * 1024 * 1024;

	/// How much memory each instruction will take in-memory after compilation.
	///
	/// This is `size_of<usize>() + 16`. But we don't use `usize` here so it isn't
	/// different on the native runtime (used for testing).
	const BYTES_PER_INSTRUCTION: u32 = 20;

	/// The code is stored multiple times as part of the compiled program.
	const EXTRA_OVERHEAD_PER_CODE_BYTE: u32 = 4;

	/// The maximum size of a basic block in number of instructions.
	///
	/// We need to limit the size of basic blocks because the interpreters lazy compilation
	/// compiles one basic block at a time. A malicious program could trigger the compilation
	/// of the whole program by creating one giant basic block otherwise.
	const BASIC_BLOCK_SIZE: u32 = 1000;

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

		// This scans the whole program but we only do it once on code deployment.
		// It is safe to do unchecked math in u32 because the size of the program
		// was already checked above.
		use polkavm_common::program::ISA32_V1_NoSbrk as ISA;
		let mut num_instructions: u32 = 0;
		let mut max_basic_block_size: u32 = 0;
		let mut basic_block_size: u32 = 0;
		for inst in program.instructions(ISA) {
			num_instructions += 1;
			basic_block_size += 1;
			if inst.kind.opcode().starts_new_basic_block() {
				max_basic_block_size = max_basic_block_size.max(basic_block_size);
				basic_block_size = 0;
			}
			if matches!(inst.kind, polkavm::program::Instruction::invalid) {
				log::debug!(target: LOG_TARGET, "invalid instruction at offset {}", inst.offset);
				return Err(<Error<T>>::InvalidInstruction.into())
			}
		}

		if max_basic_block_size > BASIC_BLOCK_SIZE {
			log::debug!(target: LOG_TARGET, "basic block too large: {max_basic_block_size} limit: {BASIC_BLOCK_SIZE}");
			return Err(Error::<T>::BasicBlockTooLarge.into())
		}

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
			.saturating_add(
				u64::from(num_instructions).saturating_mul(BYTES_PER_INSTRUCTION.into()),
			)
			.saturating_add(
				(program.code().len() as u64).saturating_mul(EXTRA_OVERHEAD_PER_CODE_BYTE.into()),
			);

		if memory_size > STATIC_MEMORY_BYTES.into() {
			log::debug!(target: LOG_TARGET, "static memory too large: {memory_size} limit: {STATIC_MEMORY_BYTES}");
			return Err(Error::<T>::StaticMemoryTooLarge.into())
		}

		Ok(blob)
	}
}
