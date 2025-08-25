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

use crate::vm::Ext;
use core::{cmp::min, ops::Range};
use revm::{
	context_interface::{context::StateLoad, journaled_state::AccountLoad},
	interpreter::{
		gas as revm_gas,
		interpreter_types::{MemoryTr, RuntimeFlag, StackTr},
		Interpreter,
	},
	primitives::{hardfork::SpecId::*, U256},
};

/// Gets memory input and output ranges for call instructions.
#[inline]
pub fn get_memory_input_and_out_ranges<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
) -> Option<(Range<usize>, Range<usize>)> {
	popn!([in_offset, in_len, out_offset, out_len], interpreter, None);

	let mut in_range = resize_memory(interpreter, in_offset, in_len)?;

	if !in_range.is_empty() {
		let offset = <_ as MemoryTr>::local_memory_offset(&interpreter.memory);
		in_range = in_range.start.saturating_add(offset)..in_range.end.saturating_add(offset);
	}

	let ret_range = resize_memory(interpreter, out_offset, out_len)?;
	Some((in_range, ret_range))
}

/// Resize memory and return range of memory.
/// If `len` is 0 dont touch memory and return `usize::MAX` as offset and 0 as length.
#[inline]
pub fn resize_memory<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
	offset: U256,
	len: U256,
) -> Option<Range<usize>> {
	let len = as_usize_or_fail_ret!(interpreter, len, None);
	let offset = if len != 0 {
		let offset = as_usize_or_fail_ret!(interpreter, offset, None);
		resize_memory!(interpreter, offset, len, None);
		offset
	} else {
		usize::MAX //unrealistic value so we are sure it is not used
	};
	Some(offset..offset + len)
}

/// Calculates gas cost and limit for call instructions.
#[inline]
pub fn calc_call_gas<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
	account_load: StateLoad<AccountLoad>,
	has_transfer: bool,
	local_gas_limit: u64,
) -> Option<u64> {
	let call_cost =
		revm_gas::call_cost(interpreter.runtime_flag.spec_id(), has_transfer, account_load);
	gas_legacy!(interpreter, call_cost, None);

	// EIP-150: Gas cost changes for IO-heavy operations
	let gas_limit = if interpreter.runtime_flag.spec_id().is_enabled_in(TANGERINE) {
		// Take l64 part of gas_limit
		min(interpreter.gas.remaining_63_of_64_parts(), local_gas_limit)
	} else {
		local_gas_limit
	};

	Some(gas_limit)
}
