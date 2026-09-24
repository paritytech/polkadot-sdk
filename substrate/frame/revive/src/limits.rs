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

/// Maximum size of of the transaction payload
///
/// Maximum code size during instantiation taken into account plus some overhead.
pub const MAX_TRANSACTION_PAYLOAD_SIZE: u32 = code::BLOB_BYTES + CALLDATA_BYTES;

/// Maximum size of storage items.
pub const STORAGE_BYTES: u32 = 416;

/// Maximum payload size of events.
pub const EVENT_BYTES: u32 = 64 * 1024;

/// The extra ref time charge of deposit event per byte.
///
/// This ensure the block builder has enough memory and pallet storage
/// to operate under worst case scenarios.
pub const EXTRA_EVENT_CHARGE_PER_BYTE: u64 = 256 * 1024;

/// The maximum size for calldata and return data.
///
/// Please note that the calldata is limited to 128KB on geth anyways.
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

/// upperbound of memory that can be used by the EVM interpreter.
pub const EVM_MEMORY_BYTES: u32 = 1024 * 1024;

/// EVM interpreter stack limit.
pub const EVM_STACK_LIMIT: u32 = 1024;

/// Limits that are only enforced on code upload.
///
/// # Note
///
/// This limit can be increased later without breaking existing contracts
/// as it is only enforced at code upload time. Code already uploaded
/// will not be affected by those limits.
pub mod code {
	use super::PAGE_SIZE;
	use crate::{Config, Error, LOG_TARGET};
	use alloc::vec::Vec;
	use sp_runtime::{DispatchError, SaturatedConversion};

	/// The maximum length of a code blob in bytes.
	///
	/// This mostly exist to prevent parsing too big blobs and to
	/// have a maximum encoded length.
	pub const BLOB_BYTES: u32 = 1024 * 1024;

	/// The maximum amount of memory the interpreter is allowed to use for compilation artifacts.
	pub const INTERPRETER_CACHE_BYTES: u32 = 1024 * 1024;

	/// The maximum size of a basic block in number of instructions.
	///
	/// We need to limit the size of basic blocks because the interpreters lazy compilation
	/// compiles one basic block at a time. A malicious program could trigger the compilation
	/// of the whole program by creating one giant basic block otherwise.
	pub const BASIC_BLOCK_SIZE: u32 = 1000;

	/// The limit for memory that can be purged on demand.
	///
	/// We purge this memory every time we call into another contract.
	/// Hence we effectively only need to hold it once in RAM.
	pub const PURGABLE_MEMORY_LIMIT: u32 = INTERPRETER_CACHE_BYTES + 2 * 1024 * 1024;

	/// The limit for memory that needs to be kept alive for a contracts whole life time.
	///
	/// This means tuning this number affects the call stack depth.
	pub const BASELINE_MEMORY_LIMIT: u32 = BLOB_BYTES + 512 * 1024;

	/// Why [`check_pvm_code`] rejected a code blob.
	///
	/// The limits that were exceeded are reported alongside the measured value. They are not
	/// meant to be hard-coded by tooling since they may be raised in future releases.
	#[derive(Debug, PartialEq)]
	#[non_exhaustive]
	pub enum CodeRejection {
		/// The blob is larger than allowed. Sizes are in bytes.
		BlobTooLarge { size: u32, limit: u32 },
		/// A basic block is larger than allowed. Sizes are in number of instructions.
		BasicBlockTooLarge { size: u32, limit: u32 },
		/// The program needs more purgeable memory than allowed. Sizes are in bytes.
		PurgeableMemoryTooLarge { size: u32, limit: u32 },
		/// The program needs more baseline memory than allowed. Sizes are in bytes.
		BaselineMemoryTooLarge { size: u32, limit: u32 },
		/// The program contains an instruction that contracts are not allowed to use.
		InvalidInstruction,
		/// The blob failed to parse, is a 32 bit program, uses the wrong instruction set, or
		/// contains a malformed or unknown import.
		Malformed,
	}

	impl CodeRejection {
		/// The error the pallet returns on code upload for this rejection.
		fn into_error<T: Config>(self) -> Error<T> {
			match self {
				Self::BlobTooLarge { .. } => Error::<T>::BlobTooLarge,
				Self::BasicBlockTooLarge { .. } => Error::<T>::BasicBlockTooLarge,
				Self::PurgeableMemoryTooLarge { .. } | Self::BaselineMemoryTooLarge { .. } => {
					Error::<T>::StaticMemoryTooLarge
				},
				Self::InvalidInstruction => Error::<T>::InvalidInstruction,
				Self::Malformed => Error::<T>::CodeRejected,
			}
		}
	}

	/// Make sure that the various program parts are within the defined limits.
	///
	/// This is the check the pallet runs on code upload. It is not generic over the runtime so
	/// that tooling can find out ahead of deployment whether a blob would be accepted.
	pub fn check_pvm_code(pvm_blob: &[u8]) -> Result<(), CodeRejection> {
		use polkavm_common::program::{
			EstimateInterpreterMemoryUsageArgs, ISA_ReviveV1, InstructionSetKind,
		};

		check_blob_size(pvm_blob)?;

		let available_syscalls = crate::vm::pvm::env::list_syscalls();

		let program = polkavm::ProgramBlob::parse(pvm_blob.into()).map_err(|err| {
			log::debug!(target: LOG_TARGET, "failed to parse polkavm blob: {err:?}");
			CodeRejection::Malformed
		})?;

		if !program.is_64_bit() {
			log::debug!(target: LOG_TARGET, "32bit programs are not supported.");
			Err(CodeRejection::Malformed)?;
		}

		if program.isa() != InstructionSetKind::ReviveV1 {
			log::debug!(target: LOG_TARGET, "Program instruction set '{}' is not '{}'", program.isa().name(), InstructionSetKind::ReviveV1.name());
			Err(CodeRejection::Malformed)?;
		}

		// Need to check that no non-existent syscalls are used. This allows us to add
		// new syscalls later without affecting already deployed code.
		for (idx, import) in program.imports().iter().enumerate() {
			// We are being defensive in case an attacker is able to somehow include
			// a lot of imports. This is important because we search the array of host
			// functions for every import.
			if idx == available_syscalls.len() {
				log::debug!(target: LOG_TARGET, "Program contains too many imports.");
				Err(CodeRejection::Malformed)?;
			}
			let Some(import) = import else {
				log::debug!(target: LOG_TARGET, "Program contains malformed import.");
				return Err(CodeRejection::Malformed);
			};
			if !available_syscalls.contains(&import.as_bytes()) {
				log::debug!(target: LOG_TARGET, "Program references unknown syscall: {}", import);
				Err(CodeRejection::Malformed)?;
			}
		}

		// This scans the whole program but we only do it once on code deployment.
		// It is safe to do unchecked math in u32 because the size of the program
		// was already checked above.
		let mut max_block_size: u32 = 0;
		let mut block_size: u32 = 0;
		let mut basic_block_count: u32 = 0;
		let mut instruction_count: u32 = 0;
		for inst in program.instructions_with_isa(ISA_ReviveV1) {
			use polkavm::program::Instruction;
			block_size += 1;
			instruction_count += 1;
			if inst.kind.opcode().starts_new_basic_block() {
				max_block_size = max_block_size.max(block_size);
				block_size = 0;
				basic_block_count += 1;
			}
			match inst.kind {
				Instruction::invalid => {
					log::debug!(target: LOG_TARGET, "invalid instruction at offset {}", inst.offset);
					return Err(CodeRejection::InvalidInstruction);
				},
				// Since polkavm `0.30.0` linker will fail if it detects sbrk instruction.
				// So this branch is never reached for programs built with polkavm >= 0.30.0.
				//
				// # Note
				//
				// The decoder might decode sbrk to `invalid` under our Revive isa. In this case,
				// this branch is _not_ hit for sbrk but the one above instead.
				Instruction::sbrk(_, _) => {
					log::debug!(target: LOG_TARGET, "sbrk instruction is not allowed. offset {}", inst.offset);
					return Err(CodeRejection::InvalidInstruction);
				},
				// Only benchmarking code is allowed to circumvent the import table. We might want
				// to remove this magic syscall number later. Hence we need to prevent contracts
				// from using it.
				//
				// The `as` conversion is fine here because we are just testing equality.
				#[cfg(not(feature = "runtime-benchmarks"))]
				Instruction::ecalli(idx) if idx as u32 == crate::SENTINEL => {
					log::debug!(target: LOG_TARGET, "reserved syscall idx {idx}. offset {}", inst.offset);
					return Err(CodeRejection::InvalidInstruction);
				},
				_ => (),
			}
		}
		max_block_size = max_block_size.max(block_size);

		if max_block_size > BASIC_BLOCK_SIZE {
			log::debug!(target: LOG_TARGET, "basic block too large: {max_block_size} limit: {BASIC_BLOCK_SIZE}");
			return Err(CodeRejection::BasicBlockTooLarge {
				size: max_block_size,
				limit: BASIC_BLOCK_SIZE,
			});
		}

		let usage_args = EstimateInterpreterMemoryUsageArgs::BoundedCache {
			max_cache_size_bytes: INTERPRETER_CACHE_BYTES,
			instruction_count,
			max_block_size,
			basic_block_count,
			page_size: PAGE_SIZE,
		};

		let program_info =
			program.estimate_interpreter_memory_usage(usage_args).map_err(|err| {
				log::debug!(target: LOG_TARGET, "failed to estimate memory usage of program: {err:?}");
				CodeRejection::Malformed
			})?;

		log::trace!(
			target: LOG_TARGET, "Contract memory usage: purgable={}/{} KB baseline={}/{}",
			program_info.purgeable_ram_consumption, PURGABLE_MEMORY_LIMIT,
			program_info.baseline_ram_consumption, BASELINE_MEMORY_LIMIT,
		);

		if program_info.purgeable_ram_consumption > PURGABLE_MEMORY_LIMIT {
			log::debug!(target: LOG_TARGET, "contract uses too much purgeable memory: {} limit: {}",
				program_info.purgeable_ram_consumption,
				PURGABLE_MEMORY_LIMIT,
			);
			return Err(CodeRejection::PurgeableMemoryTooLarge {
				size: program_info.purgeable_ram_consumption,
				limit: PURGABLE_MEMORY_LIMIT,
			});
		}

		if program_info.baseline_ram_consumption > BASELINE_MEMORY_LIMIT {
			log::debug!(target: LOG_TARGET, "contract uses too much baseline memory: {} limit: {}",
				program_info.baseline_ram_consumption,
				BASELINE_MEMORY_LIMIT,
			);
			return Err(CodeRejection::BaselineMemoryTooLarge {
				size: program_info.baseline_ram_consumption,
				limit: BASELINE_MEMORY_LIMIT,
			});
		}

		Ok(())
	}

	/// Runs [`check_pvm_code`] and turns a rejection into the pallet's dispatch error.
	pub fn enforce<T: Config>(pvm_blob: Vec<u8>) -> Result<Vec<u8>, DispatchError> {
		check_blob_size(&pvm_blob).map_err(CodeRejection::into_error::<T>)?;

		#[cfg(feature = "std")]
		if std::env::var_os("REVIVE_SKIP_VALIDATION").is_some() {
			log::warn!(target: LOG_TARGET, "Skipping validation because env var REVIVE_SKIP_VALIDATION is set");
			return Ok(pvm_blob);
		}

		check_pvm_code(&pvm_blob).map_err(CodeRejection::into_error::<T>)?;
		Ok(pvm_blob)
	}

	fn check_blob_size(pvm_blob: &[u8]) -> Result<(), CodeRejection> {
		let len: u64 = pvm_blob.len() as u64;
		if len > BLOB_BYTES.into() {
			log::debug!(target: LOG_TARGET, "contract blob too large: {len} limit: {BLOB_BYTES}");
			return Err(CodeRejection::BlobTooLarge {
				size: len.saturated_into(),
				limit: BLOB_BYTES,
			});
		}
		Ok(())
	}

	#[cfg(test)]
	mod tests {
		use super::CodeRejection;
		use crate::{Error, tests::Test};
		use sp_runtime::DispatchError;

		fn to_error(rejection: CodeRejection) -> DispatchError {
			rejection.into_error::<Test>().into()
		}

		#[test]
		fn blob_too_large_maps_to_blob_too_large() {
			let rejection = CodeRejection::BlobTooLarge { size: 2, limit: 1 };
			assert_eq!(to_error(rejection), Error::<Test>::BlobTooLarge.into());
		}

		#[test]
		fn basic_block_too_large_maps_to_basic_block_too_large() {
			let rejection = CodeRejection::BasicBlockTooLarge { size: 2, limit: 1 };
			assert_eq!(to_error(rejection), Error::<Test>::BasicBlockTooLarge.into());
		}

		#[test]
		fn purgeable_memory_too_large_maps_to_static_memory_too_large() {
			let rejection = CodeRejection::PurgeableMemoryTooLarge { size: 2, limit: 1 };
			assert_eq!(to_error(rejection), Error::<Test>::StaticMemoryTooLarge.into());
		}

		#[test]
		fn baseline_memory_too_large_maps_to_static_memory_too_large() {
			let rejection = CodeRejection::BaselineMemoryTooLarge { size: 2, limit: 1 };
			assert_eq!(to_error(rejection), Error::<Test>::StaticMemoryTooLarge.into());
		}

		#[test]
		fn invalid_instruction_maps_to_invalid_instruction() {
			assert_eq!(
				to_error(CodeRejection::InvalidInstruction),
				Error::<Test>::InvalidInstruction.into()
			);
		}

		#[test]
		fn malformed_maps_to_code_rejected() {
			assert_eq!(to_error(CodeRejection::Malformed), Error::<Test>::CodeRejected.into());
		}
	}
}

/// The amount of total memory we require.
///
/// Unchecked math is okay since we evaluate at compile time.
const fn memory_required() -> u32 {
	// The root frame is not accounted for in CALL_STACK_DEPTH
	let max_call_depth = CALL_STACK_DEPTH + 1;

	let per_stack_memory = code::PURGABLE_MEMORY_LIMIT +
		TRANSIENT_STORAGE_BYTES * 2 +
		crate::access_list::MAX_ACCESS_LIST_BYTES;

	let evm_max_initcode_size = revm::primitives::eip3860::MAX_INITCODE_SIZE as u32;
	let evm_overhead = EVM_MEMORY_BYTES + evm_max_initcode_size + EVM_STACK_LIMIT * 32;
	let per_frame_memory = if code::BASELINE_MEMORY_LIMIT > evm_overhead {
		code::BASELINE_MEMORY_LIMIT
	} else {
		evm_overhead
	} + CALLDATA_BYTES * 2;

	per_stack_memory + max_call_depth * per_frame_memory
}
