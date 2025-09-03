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

use super::{
	utility::{IntoAddress, IntoU256},
	Context,
};
use crate::vm::Ext;
use core::cmp::min;
use revm::{
	interpreter::{
		gas::{self, warm_cold_cost, CALL_STIPEND},
		host::Host,
		interpreter_types::{InputsTr, RuntimeFlag, StackTr},
		InstructionResult,
	},
	primitives::{hardfork::SpecId::*, Bytes, Log, LogData, B256, BLOCK_HASH_HISTORY, U256},
};

/// Implements the BALANCE instruction.
///
/// Gets the balance of the given account.
pub fn balance<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], top, context.interpreter);
	let address = top.into_address();
	let Some(balance) = context.host.balance(address) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};
	let spec_id = context.interpreter.runtime_flag.spec_id();
	gas_legacy!(
		context.interpreter,
		if spec_id.is_enabled_in(BERLIN) {
			warm_cold_cost(balance.is_cold)
		} else if spec_id.is_enabled_in(ISTANBUL) {
			// EIP-1884: Repricing for trie-size-dependent opcodes
			700
		} else if spec_id.is_enabled_in(TANGERINE) {
			400
		} else {
			20
		}
	);
	*top = balance.data;
}

/// EIP-1884: Repricing for trie-size-dependent opcodes
pub fn selfbalance<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, ISTANBUL);
	gas_legacy!(context.interpreter, gas::LOW);

	let Some(balance) = context.host.balance(context.interpreter.input.target_address()) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};
	push!(context.interpreter, balance.data);
}

/// Implements the EXTCODESIZE instruction.
///
/// Gets the size of an account's code.
pub fn extcodesize<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], top, context.interpreter);
	let address = top.into_address();
	let Some(code) = context.host.load_account_code(address) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};
	let spec_id = context.interpreter.runtime_flag.spec_id();
	if spec_id.is_enabled_in(BERLIN) {
		gas_legacy!(context.interpreter, warm_cold_cost(code.is_cold));
	} else if spec_id.is_enabled_in(TANGERINE) {
		gas_legacy!(context.interpreter, 700);
	} else {
		gas_legacy!(context.interpreter, 20);
	}

	*top = U256::from(code.len());
}

/// EIP-1052: EXTCODEHASH opcode
pub fn extcodehash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, CONSTANTINOPLE);
	popn_top!([], top, context.interpreter);
	let address = top.into_address();
	let Some(code_hash) = context.host.load_account_code_hash(address) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};
	let spec_id = context.interpreter.runtime_flag.spec_id();
	if spec_id.is_enabled_in(BERLIN) {
		gas_legacy!(context.interpreter, warm_cold_cost(code_hash.is_cold));
	} else if spec_id.is_enabled_in(ISTANBUL) {
		gas_legacy!(context.interpreter, 700);
	} else {
		gas_legacy!(context.interpreter, 400);
	}
	*top = code_hash.into_u256();
}

/// Implements the EXTCODECOPY instruction.
///
/// Copies a portion of an account's code to memory.
pub fn extcodecopy<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([address, memory_offset, code_offset, len_u256], context.interpreter);
	let address = address.into_address();
	let Some(code) = context.host.load_account_code(address) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};

	let len = as_usize_or_fail!(context.interpreter, len_u256);
	gas_or_fail_legacy!(
		context.interpreter,
		gas::extcodecopy_cost(context.interpreter.runtime_flag.spec_id(), len, code.is_cold)
	);
	if len == 0 {
		return;
	}
	let memory_offset = as_usize_or_fail!(context.interpreter, memory_offset);
	let code_offset = min(as_usize_saturated!(code_offset), code.len());
	resize_memory!(context.interpreter, memory_offset, len);

	// Note: This can't panic because we resized memory to fit.
	context.interpreter.memory.set_data(memory_offset, code_offset, len, &code);
}

/// Implements the BLOCKHASH instruction.
///
/// Gets the hash of one of the 256 most recent complete blocks.
pub fn blockhash<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, gas::BLOCKHASH);
	popn_top!([], number, context.interpreter);

	let requested_number = *number;
	let block_number = context.host.block_number();

	let Some(diff) = block_number.checked_sub(requested_number) else {
		*number = U256::ZERO;
		return;
	};

	let diff = as_u64_saturated!(diff);

	// blockhash should push zero if number is same as current block number.
	if diff == 0 {
		*number = U256::ZERO;
		return;
	}

	*number = if diff <= BLOCK_HASH_HISTORY {
		let Some(hash) = context.host.block_hash(as_u64_saturated!(requested_number)) else {
			context.interpreter.halt(InstructionResult::FatalExternalError);
			return;
		};
		U256::from_be_bytes(hash.0)
	} else {
		U256::ZERO
	}
}

/// Implements the SLOAD instruction.
///
/// Loads a word from storage.
pub fn sload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([], index, context.interpreter);

	let Some(value) = context.host.sload(context.interpreter.input.target_address(), *index) else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};

	gas_legacy!(
		context.interpreter,
		gas::sload_cost(context.interpreter.runtime_flag.spec_id(), value.is_cold)
	);
	*index = value.data;
}

/// Implements the SSTORE instruction.
///
/// Stores a word to storage.
pub fn sstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([index, value], context.interpreter);

	let Some(state_load) =
		context.host.sstore(context.interpreter.input.target_address(), index, value)
	else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};

	// EIP-1706 Disable SSTORE with gasleft lower than call stipend
	if context.interpreter.runtime_flag.spec_id().is_enabled_in(ISTANBUL) &&
		context.interpreter.gas.remaining() <= CALL_STIPEND
	{
		context.interpreter.halt(InstructionResult::ReentrancySentryOOG);
		return;
	}
	gas_legacy!(
		context.interpreter,
		gas::sstore_cost(
			context.interpreter.runtime_flag.spec_id(),
			&state_load.data,
			state_load.is_cold
		)
	);

	context.interpreter.gas.record_refund(gas::sstore_refund(
		context.interpreter.runtime_flag.spec_id(),
		&state_load.data,
	));
}

/// EIP-1153: Transient storage opcodes
/// Store value to transient storage
pub fn tstore<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, CANCUN);
	require_non_staticcall!(context.interpreter);
	gas_legacy!(context.interpreter, gas::WARM_STORAGE_READ_COST);

	popn!([index, value], context.interpreter);

	context.host.tstore(context.interpreter.input.target_address(), index, value);
}

/// EIP-1153: Transient storage opcodes
/// Load value from transient storage
pub fn tload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, CANCUN);
	gas_legacy!(context.interpreter, gas::WARM_STORAGE_READ_COST);

	popn_top!([], index, context.interpreter);

	*index = context.host.tload(context.interpreter.input.target_address(), *index);
}

/// Implements the LOG0-LOG4 instructions.
///
/// Appends log record with N topics.
pub fn log<'ext, const N: usize, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);

	popn!([offset, len], context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, len);
	gas_or_fail_legacy!(context.interpreter, gas::log_cost(N as u8, len as u64));
	let data = if len == 0 {
		Bytes::new()
	} else {
		let offset = as_usize_or_fail!(context.interpreter, offset);
		resize_memory!(context.interpreter, offset, len);
		Bytes::copy_from_slice(context.interpreter.memory.slice_len(offset, len).as_ref())
	};
	if context.interpreter.stack.len() < N {
		context.interpreter.halt(InstructionResult::StackUnderflow);
		return;
	}
	let Some(topics) = <_ as StackTr>::popn::<N>(&mut context.interpreter.stack) else {
		context.interpreter.halt(InstructionResult::StackUnderflow);
		return;
	};

	let log = Log {
		address: context.interpreter.input.target_address(),
		data: LogData::new(topics.into_iter().map(B256::from).collect(), data)
			.expect("LogData should have <=4 topics"),
	};

	context.host.log(log);
}

/// Implements the SELFDESTRUCT instruction.
///
/// Halt execution and register account for later deletion.
pub fn selfdestruct<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	require_non_staticcall!(context.interpreter);
	popn!([target], context.interpreter);
	let target = target.into_address();

	let Some(res) = context.host.selfdestruct(context.interpreter.input.target_address(), target)
	else {
		context.interpreter.halt(InstructionResult::FatalExternalError);
		return;
	};

	// EIP-3529: Reduction in refunds
	if !context.interpreter.runtime_flag.spec_id().is_enabled_in(LONDON) &&
		!res.previously_destroyed
	{
		context.interpreter.gas.record_refund(gas::SELFDESTRUCT)
	}

	gas_legacy!(
		context.interpreter,
		gas::selfdestruct_cost(context.interpreter.runtime_flag.spec_id(), res)
	);

	context.interpreter.halt(InstructionResult::SelfDestruct);
}
