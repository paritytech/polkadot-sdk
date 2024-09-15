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
//! applied **once** at code upload time. Since this action cannot be performened by contracts we
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
	use crate::{Config, Error};
	use frame_support::ensure;
	use sp_runtime::DispatchResult;

	/// The maximum length of a code blob in bytes.
	pub const BLOB_BYTES: u32 = CODE_BYTES + STATIC_DATA_BYTES;

	/// Maximum size of the code section in bytes.
	pub const CODE_BYTES: u32 = 50 * 1024;

	/// Maximum size of the other sections combined in bytes.
	///
	/// We limit data seperately from code because after compilation
	/// code consumes more memory per source byte than data. This allows
	/// us to allow larger programs as long as they don't contain too much
	/// code.
	pub const STATIC_DATA_BYTES: u32 = 100 * 1024;

	/// Make sure that the various program parts are within the defined limits.
	pub fn enforce<T: Config>(program: &polkavm::ProgramParts) -> DispatchResult {
		fn round_page(n: u32) -> u32 {
			debug_assert!(
				PAGE_SIZE != 0 && (PAGE_SIZE & (PAGE_SIZE - 1)) == 0,
				"Page size must be power of 2"
			);
			(n + PAGE_SIZE - 1) & !(PAGE_SIZE - 1)
		}

		ensure!(program.code_and_jump_table.len() as u32 <= CODE_BYTES, <Error<T>>::CodeTooLarge);

		let data_size = round_page(program.ro_data_size)
			.saturating_add(round_page(program.rw_data_size))
			.saturating_add(round_page(program.stack_size))
			.saturating_add(program.import_offsets.len() as u32)
			.saturating_add(program.import_symbols.len() as u32)
			.saturating_add(program.exports.len() as u32)
			.saturating_add(program.debug_strings.len() as u32)
			.saturating_add(program.debug_line_program_ranges.len() as u32)
			.saturating_add(program.debug_line_programs.len() as u32);

		ensure!(data_size <= STATIC_DATA_BYTES, <Error<T>>::StaticDataTooLarge);

		Ok(())
	}
}
