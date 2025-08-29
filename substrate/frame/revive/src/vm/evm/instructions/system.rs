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

use super::Context;
use crate::{
	address::AddressMapper,
	vm::{evm::U256Converter, Ext, RuntimeCosts},
	Config,
};
use core::ptr;
use revm::{
	interpreter::{
		gas as revm_gas,
		interpreter_types::{InputsTr, LegacyBytecode, MemoryTr, ReturnData, StackTr},
		CallInput, InstructionResult, Interpreter,
	},
	primitives::{Address, B256, KECCAK_EMPTY, U256},
};
use sp_io::hashing::keccak_256;

// TODO: Fix the gas handling for the memory operations

/// Implements the KECCAK256 instruction.
///
/// Computes Keccak-256 hash of memory data.
pub fn keccak256<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn_top!([offset], top, context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, top);
	gas!(context.interpreter, RuntimeCosts::HashKeccak256(len as u32));
	let hash = if len == 0 {
		KECCAK_EMPTY
	} else {
		let from = as_usize_or_fail!(context.interpreter, offset);
		resize_memory!(context.interpreter, from, len);
		keccak_256(context.interpreter.memory.slice_len(from, len).as_ref()).into()
	};
	*top = hash.into();
}

/// Implements the ADDRESS instruction.
///
/// Pushes the current contract's address onto the stack.
pub fn address<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::Address);
	let address: Address = context.interpreter.extend.address().0.into();
	push!(context.interpreter, address.into_word().into());
}

/// Implements the CALLER instruction.
///
/// Pushes the caller's address onto the stack.
pub fn caller<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::Caller);
	match context.interpreter.extend.caller().account_id() {
		Ok(account_id) => {
			let address: Address = <E::T as Config>::AddressMapper::to_address(account_id).0.into();
			push!(context.interpreter, address.into_word().into());
		},
		Err(_) => {
			context
				.interpreter
				.halt(revm::interpreter::InstructionResult::FatalExternalError);
		},
	}
}

/// Implements the CODESIZE instruction.
///
/// Pushes the size of running contract's bytecode onto the stack.
pub fn codesize<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(context.interpreter.bytecode.bytecode_len()));
}

/// Implements the CODECOPY instruction.
///
/// Copies running contract's bytecode to memory.
pub fn codecopy<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([memory_offset, code_offset, len], context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, len);
	let Some(memory_offset) = memory_resize(context.interpreter, memory_offset, len) else {
		return;
	};
	let code_offset = as_usize_saturated!(code_offset);

	// Note: This can't panic because we resized memory to fit.
	context.interpreter.memory.set_data(
		memory_offset,
		code_offset,
		len,
		context.interpreter.bytecode.bytecode_slice(),
	);
}

/// Implements the CALLDATALOAD instruction.
///
/// Loads 32 bytes of input data from the specified offset.
pub fn calldataload<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	//pop_top!(interpreter, offset_ptr);
	popn_top!([], offset_ptr, context.interpreter);
	let mut word = B256::ZERO;
	let offset = as_usize_saturated!(offset_ptr);
	let input = context.interpreter.input.input();
	let input_len = input.len();
	if offset < input_len {
		let count = 32.min(input_len - offset);

		// SAFETY: `count` is bounded by the calldata length.
		// This is `word[..count].copy_from_slice(input[offset..offset + count])`, written using
		// raw pointers as apparently the compiler cannot optimize the slice version, and using
		// `get_unchecked` twice is uglier.
		match context.interpreter.input.input() {
			CallInput::Bytes(bytes) => {
				unsafe {
					ptr::copy_nonoverlapping(bytes.as_ptr().add(offset), word.as_mut_ptr(), count)
				};
			},
			CallInput::SharedBuffer(range) => {
				let input_slice = context.interpreter.memory.global_slice(range.clone());
				unsafe {
					ptr::copy_nonoverlapping(
						input_slice.as_ptr().add(offset),
						word.as_mut_ptr(),
						count,
					)
				};
			},
		}
	}
	*offset_ptr = word.into();
}

/// Implements the CALLDATASIZE instruction.
///
/// Pushes the size of input data onto the stack.
pub fn calldatasize<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(context.interpreter.input.input().len()));
}

/// Implements the CALLVALUE instruction.
///
/// Pushes the value sent with the current call onto the stack.
pub fn callvalue<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::ValueTransferred);
	let call_value = context.interpreter.extend.value_transferred();
	push!(context.interpreter, call_value.into_revm_u256());
}

/// Implements the CALLDATACOPY instruction.
///
/// Copies input data to memory.
pub fn calldatacopy<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([memory_offset, data_offset, len], context.interpreter);
	let len = as_usize_or_fail!(context.interpreter, len);
	let Some(memory_offset) = memory_resize(context.interpreter, memory_offset, len) else {
		return;
	};

	let data_offset = as_usize_saturated!(data_offset);
	match context.interpreter.input.input() {
		CallInput::Bytes(bytes) => {
			context
				.interpreter
				.memory
				.set_data(memory_offset, data_offset, len, bytes.as_ref());
		},
		CallInput::SharedBuffer(range) => {
			context.interpreter.memory.set_data_from_global(
				memory_offset,
				data_offset,
				len,
				range.clone(),
			);
		},
	}
}

/// EIP-211: New opcodes: RETURNDATASIZE and RETURNDATACOPY
pub fn returndatasize<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::from(context.interpreter.return_data.buffer().len()));
}

/// EIP-211: New opcodes: RETURNDATASIZE and RETURNDATACOPY
pub fn returndatacopy<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	popn!([memory_offset, offset, len], context.interpreter);

	let len = as_usize_or_fail!(context.interpreter, len);
	let data_offset = as_usize_saturated!(offset);

	// Old legacy behavior is to panic if data_end is out of scope of return buffer.
	let data_end = data_offset.saturating_add(len);
	if data_end > context.interpreter.return_data.buffer().len() {
		context.interpreter.halt(InstructionResult::OutOfOffset);
		return;
	}

	let Some(memory_offset) = memory_resize(context.interpreter, memory_offset, len) else {
		return;
	};

	// Note: This can't panic because we resized memory to fit.
	context.interpreter.memory.set_data(
		memory_offset,
		data_offset,
		len,
		context.interpreter.return_data.buffer(),
	);
}

/// Implements the GAS instruction.
///
/// Pushes the amount of remaining gas onto the stack.
pub fn gas<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas!(context.interpreter, RuntimeCosts::RefTimeLeft);
	// TODO: This accounts only for 'ref_time' now. It should be fixed to also account for other
	// costs. See #9577 for more context.
	let gas = context.interpreter.extend.gas_meter().gas_left().ref_time();
	push!(context.interpreter, U256::from(gas));
}

/// Common logic for copying data from a source buffer to the EVM's memory.
///
/// Handles memory expansion and gas calculation for data copy operations.
pub fn memory_resize<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
	memory_offset: U256,
	len: usize,
) -> Option<usize> {
	gas!(interpreter, RuntimeCosts::CopyToContract(len as u32), None);
	if len == 0 {
		return None;
	}
	let memory_offset = as_usize_or_fail_ret!(interpreter, memory_offset, None);
	resize_memory!(interpreter, memory_offset, len, None);

	Some(memory_offset)
}
