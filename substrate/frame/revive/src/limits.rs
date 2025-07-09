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

/// The amount of total memory we require to safely operate.
///
/// This is not a config knob but derived from the limits in this file.
pub const MEMORY_REQUIRED: u32 = memory_required();

/// The maximum depth of the call stack.
///
/// A 0 means that no callings of other contracts are possible. In other words only the origin
/// called "root contract" is allowed to execute then.
pub const CALL_STACK_DEPTH: u32 = 25;

/// The maximum number of topics a call to [`crate::SyscallDoc::deposit_event`] can emit.
///
/// We set it to the same limit that ethereum has. It is unlikely to change.
pub const NUM_EVENT_TOPICS: u32 = 4;

/// Maximum size of events (including topics) and storage values.
pub const PAYLOAD_BYTES: u32 = 416;

/// The maximum size for calldata and return data.
pub const CALLDATA_BYTES: u32 = 128 * 1024;

/// The maximum size of the transient storage in bytes.
///
/// This includes keys, values, and previous entries used for storage rollback.
pub const TRANSIENT_STORAGE_BYTES: u32 = 4 * 1024;

/// The maximum allowable length in bytes for (transient) storage keys.
pub const STORAGE_KEY_BYTES: u32 = 128;

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
	pub const BLOB_BYTES: u32 = 1024 * 1024;

	/// The maximum amount of memory the interpreter is allowed to use for compilation artifacts.
	pub const INTERPRETER_CACHE_BYTES: u32 = 1024 * 1024;

	/// The maximum size of a basic block in number of instructions.
	///
	/// We need to limit the size of basic blocks because the interpreters lazy compilation
	/// compiles one basic block at a time. A malicious program could trigger the compilation
	/// of the whole program by creating one giant basic block otherwise.
	pub const BASIC_BLOCK_SIZE: u32 = 1000;

	/// The combined amount of rw/ro/stack memory a contract is allowed to specify.
	///
	/// Please not that this is the minimum available memory. The overall memory
	/// is shared between code and data. A contract that uses less code can use
	/// more memory.
	pub const DATA_BYTES: u32 = 512 * 1024;

	/// A flatmap of 4xblob_len is created as part of the program.
	///
	/// This is an implementation detail of PolkaVM.
	pub const EXTRA_OVERHEAD_PER_CODE_BYTE: u32 = 4;

	/// Make sure that the various program parts are within the defined limits.
	pub fn enforce<T: Config>(
		blob: Vec<u8>,
		available_syscalls: &[&[u8]],
	) -> Result<CodeVec, DispatchError> {
		fn round_page(n: u32) -> u64 {
			// performing the rounding in u64 in order to prevent overflow
			u64::from(n).next_multiple_of(PAGE_SIZE.into())
		}

		let len: u64 = blob.len() as u64;
		let blob: CodeVec = blob.try_into().map_err(|_| {
			log::debug!(target: LOG_TARGET, "contract blob too large: {len} limit: {}", BLOB_BYTES);
			<Error<T>>::BlobTooLarge
		})?;

		#[cfg(feature = "std")]
		if std::env::var_os("REVIVE_SKIP_VALIDATION").is_some() {
			log::warn!(target: LOG_TARGET, "Skipping validation because env var REVIVE_SKIP_VALIDATION is set");
			return Ok(blob)
		}

		let program = polkavm::ProgramBlob::parse(blob.as_slice().into()).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to parse polkavm blob: {err:?}");
			Error::<T>::CodeRejected
		})?;

		if !program.is_64_bit() {
			log::debug!(target: LOG_TARGET, "32bit programs are not supported.");
			Err(Error::<T>::CodeRejected)?;
		}

		// Need to check that no non-existent syscalls are used. This allows us to add
		// new syscalls later without affecting already deployed code.
		for (idx, import) in program.imports().iter().enumerate() {
			// We are being defensive in case an attacker is able to somehow include
			// a lot of imports. This is important because we search the array of host
			// functions for every import.
			if idx == available_syscalls.len() {
				log::debug!(target: LOG_TARGET, "Program contains too many imports.");
				Err(Error::<T>::CodeRejected)?;
			}
			let Some(import) = import else {
				log::debug!(target: LOG_TARGET, "Program contains malformed import.");
				return Err(Error::<T>::CodeRejected.into());
			};
			if !available_syscalls.contains(&import.as_bytes()) {
				log::debug!(target: LOG_TARGET, "Program references unknown syscall: {}", import);
				Err(Error::<T>::CodeRejected)?;
			}
		}

		// This scans the whole program but we only do it once on code deployment.
		// It is safe to do unchecked math in u32 because the size of the program
		// was already checked above.
		use polkavm::program::ISA64_V1 as ISA;
		let mut max_basic_block_size: u32 = 0;
		let mut basic_block_size: u32 = 0;
		for inst in program.instructions(ISA) {
			use polkavm::program::Instruction;
			basic_block_size += 1;
			if inst.kind.opcode().starts_new_basic_block() {
				max_basic_block_size = max_basic_block_size.max(basic_block_size);
				basic_block_size = 0;
			}
			match inst.kind {
				Instruction::invalid => {
					log::debug!(target: LOG_TARGET, "invalid instruction at offset {}", inst.offset);
					return Err(<Error<T>>::InvalidInstruction.into())
				},
				Instruction::sbrk(_, _) => {
					log::debug!(target: LOG_TARGET, "sbrk instruction is not allowed. offset {}", inst.offset);
					return Err(<Error<T>>::InvalidInstruction.into())
				},
				// Only benchmarking code is allowed to circumvent the import table. We might want
				// to remove this magic syscall number later. Hence we need to prevent contracts
				// from using it.
				#[cfg(not(feature = "runtime-benchmarks"))]
				Instruction::ecalli(idx) if idx == crate::SENTINEL => {
					log::debug!(target: LOG_TARGET, "reserved syscall idx {idx}. offset {}", inst.offset);
					return Err(<Error<T>>::InvalidInstruction.into())
				},
				_ => (),
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
		// plus the stack size
		// plus each frame can hold both its input data and the return data of the last frame
		let data_size = round_page(program.ro_data_size())
			.saturating_sub(program.ro_data().len() as u64)
			.saturating_add(round_page(program.rw_data_size()))
			.saturating_sub(program.rw_data().len() as u64)
			.saturating_add(round_page(program.stack_size()));
		let per_frame_size = len
			.saturating_add(data_size)
			.saturating_add(2 * u64::from(super::CALLDATA_BYTES));

		if per_frame_size > super::memory_required_per_frame().into() {
			log::debug!(target: LOG_TARGET, "contract uses too much memory: {per_frame_size} limit: {} data_size={data_size} code_size={}",
				super::memory_required_per_frame(),
				program.code().len(),
			);
			return Err(Error::<T>::StaticMemoryTooLarge.into())
		}

		Ok(blob)
	}
}

/// The amount of total memory we require.
///
/// Unchecked math is okay since we evaluate at compile time.
const fn memory_required() -> u32 {
	// 1 ) We delete the flatmap and compiler artifacts when we call into
	// another contract. They will be recreated on return.
	// Hence we only need to hold them in memory for one contract at a time.
	// 2) Transient storage uses a BTreeMap, which has overhead compared to the raw size of
	// key-value data. To ensure safety, a margin of 2x the raw key-value size is used.
	let memory_per_stack = code::EXTRA_OVERHEAD_PER_CODE_BYTE * code::BLOB_BYTES +
		code::INTERPRETER_CACHE_BYTES +
		TRANSIENT_STORAGE_BYTES * 2;

	// The root frame is not accounted for in CALL_STACK_DEPTH
	let max_call_depth = CALL_STACK_DEPTH + 1;

	memory_per_stack + max_call_depth * memory_required_per_frame()
}

/// The amount of memory we need for each call frame on the stack.
///
/// Unchecked math is okay since we evaluate at compile time.
const fn memory_required_per_frame() -> u32 {
	// 1) The blob itself is not dropped when calling into another contract
	// 2) The data memory regions cannot be dropped.
	// 3) Each frame can hold calldata and return data.
	code::BLOB_BYTES + code::DATA_BYTES + CALLDATA_BYTES * 2
}
