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

use super::utility::as_usize_saturated;
use crate::{
	address::AddressMapper,
	vm::{
		evm::{interpreter::Halt, util::as_usize_or_halt, EVMGas, Interpreter},
		Ext, RuntimeCosts,
	},
	Config, Error, U256,
};
use core::ops::ControlFlow;
use revm::interpreter::gas::{BASE, VERYLOW};
use sp_core::H256;
use sp_io::hashing::keccak_256;
// TODO: Fix the gas handling for the memory operations

/// The Keccak-256 hash of the empty string `""`.
pub const KECCAK_EMPTY: [u8; 32] =
	alloy_core::hex!("c5d2460186f7233c927e7db2dcc703c0e500b653ca82273b7bfad8045d85a470");

/// Implements the KECCAK256 instruction.
///
/// Computes Keccak-256 hash of memory data.
pub fn keccak256<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let ([offset], top) = interpreter.stack.popn_top()?;
	let len = as_usize_or_halt::<E::T>(*top)?;
	interpreter
		.ext
		.gas_meter_mut()
		.charge_or_halt(RuntimeCosts::HashKeccak256(len as u32))?;

	let hash = if len == 0 {
		H256::from(KECCAK_EMPTY)
	} else {
		let from = as_usize_or_halt::<E::T>(offset)?;
		interpreter.memory.resize(from, len)?;
		H256::from(keccak_256(interpreter.memory.slice_len(from, len)))
	};
	*top = U256::from_big_endian(hash.as_ref());
	ControlFlow::Continue(())
}

/// Implements the ADDRESS instruction.
///
/// Pushes the current contract's address onto the stack.
pub fn address<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::Address)?;
	let address = interpreter.ext.address();
	interpreter.stack.push(address)
}

/// Implements the CALLER instruction.
///
/// Pushes the caller's address onto the stack.
pub fn caller<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::Caller)?;
	match interpreter.ext.caller().account_id() {
		Ok(account_id) => {
			let address = <E::T as Config>::AddressMapper::to_address(account_id);
			interpreter.stack.push(address)
		},
		Err(_) => ControlFlow::Break(Error::<E::T>::ContractTrapped.into()),
	}
}

/// Implements the CODESIZE instruction.
///
/// Pushes the size of running contract's bytecode onto the stack.
pub fn codesize<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	let len = in_data_size(interpreter, true);
	interpreter.stack.push(len)
}

/// Implements the CODECOPY instruction.
///
/// Copies running contract's bytecode to memory.
pub fn codecopy<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [memory_offset, code_offset, len] = interpreter.stack.popn()?;
	in_data_copy(interpreter, memory_offset, code_offset, len, true)
}

/// Implements the CALLDATALOAD instruction.
///
/// Loads 32 bytes of input data from the specified offset.
pub fn calldataload<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([], offset_ptr) = interpreter.stack.popn_top()?;
	let mut word = [0u8; 32];
	let offset = as_usize_saturated(*offset_ptr);
	let is_constructor = interpreter.ext.entry_point().is_evm_constructor();
	let input = interpreter.input.as_slice();

	let input = if is_constructor { &[] } else { input };

	let input_len = input.len();
	if offset < input_len {
		let count = 32.min(input_len - offset);
		word[..count].copy_from_slice(&input[offset..offset + count]);
	}
	*offset_ptr = U256::from_big_endian(&word);
	ControlFlow::Continue(())
}

/// Implements the CALLDATASIZE instruction.
///
/// Pushes the size of input data onto the stack.
pub fn calldatasize<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	let len = in_data_size(interpreter, false);
	interpreter.stack.push(len)
}

/// Implements the CALLVALUE instruction.
///
/// Pushes the value sent with the current call onto the stack.
pub fn callvalue<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::ValueTransferred)?;
	let value = interpreter.ext.value_transferred();
	interpreter.stack.push(value)
}

/// Implements the CALLDATACOPY instruction.
///
/// Copies input data to memory.
pub fn calldatacopy<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [memory_offset, data_offset, len] = interpreter.stack.popn()?;
	in_data_copy(interpreter, memory_offset, data_offset, len, false)
}

/// EIP-211: New opcodes: RETURNDATASIZE and RETURNDATACOPY
pub fn returndatasize<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	let return_data_len = interpreter.ext.last_frame_output().data.len();
	interpreter.stack.push(U256::from(return_data_len))
}

/// EIP-211: New opcodes: RETURNDATASIZE and RETURNDATACOPY
pub fn returndatacopy<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [memory_offset, offset, len] = interpreter.stack.popn()?;

	let len = as_usize_or_halt::<E::T>(len)?;
	let data_offset = as_usize_saturated(offset);

	// Old legacy behavior is to panic if data_end is out of scope of return buffer.
	let data_end = data_offset.saturating_add(len);
	if data_end > interpreter.ext.last_frame_output().data.len() {
		return ControlFlow::Break(Error::<E::T>::OutOfBounds.into());
	}

	let Some(memory_offset) = memory_resize(interpreter, memory_offset, len)? else {
		return ControlFlow::Continue(())
	};

	// Note: This can't panic because we resized memory.
	interpreter.memory.set_data(
		memory_offset,
		data_offset,
		len,
		&interpreter.ext.last_frame_output().data,
	);
	ControlFlow::Continue(())
}

/// Implements the GAS instruction.
///
/// Pushes the amount of remaining gas onto the stack.
pub fn gas<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(RuntimeCosts::RefTimeLeft)?;
	let gas = interpreter.ext.gas_left();
	interpreter.stack.push(U256::from(gas))
}

/// Common logic for copying data from a source buffer to the EVM's memory.
///
/// Handles memory expansion and gas calculation for data copy operations.
fn memory_resize<'a, E: Ext>(
	interpreter: &mut Interpreter<'a, E>,
	memory_offset: U256,
	len: usize,
) -> ControlFlow<Halt, Option<usize>> {
	if len == 0 {
		return ControlFlow::Continue(None)
	}

	interpreter.ext.charge_or_halt(RuntimeCosts::CopyToContract(len as u32))?;
	let memory_offset = as_usize_or_halt::<E::T>(memory_offset)?;
	interpreter.memory.resize(memory_offset, len)?;
	ControlFlow::Continue(Some(memory_offset))
}

/// The shared logic for CODECOPY and CALLDATACOPY.
fn in_data_copy<E: Ext>(
	interpreter: &mut Interpreter<E>,
	memory_offset: U256,
	data_offset: U256,
	len: U256,
	is_codecopy: bool,
) -> ControlFlow<Halt> {
	let len = as_usize_or_halt::<E::T>(len)?;
	let Some(memory_offset) = memory_resize(interpreter, memory_offset, len)? else {
		return ControlFlow::Continue(())
	};
	let data_offset = as_usize_saturated(data_offset);
	let is_constructor = interpreter.ext.entry_point().is_evm_constructor();
	let bytecode = interpreter.bytecode.bytecode_slice();
	let input = interpreter.input.as_slice();

	struct Write<'a> {
		memory_offset: usize,
		data_offset: usize,
		len: usize,
		slice: &'a [u8],
	}

	let writes = match (is_constructor, is_codecopy) {
		(true, true) => {
			let bytes_written = bytecode.len().saturating_sub(data_offset).min(len);
			&[
				Write { memory_offset, data_offset, len: bytes_written, slice: bytecode },
				Write {
					memory_offset: memory_offset.saturating_add(bytes_written),
					data_offset: data_offset.saturating_sub(bytecode.len()),
					len: len.saturating_sub(bytes_written),
					slice: input,
				},
			][..]
		},
		(true, false) => &[Write { memory_offset, data_offset, len, slice: &[] }],
		(false, true) => &[Write { memory_offset, data_offset, len, slice: bytecode }],
		(false, false) => &[Write { memory_offset, data_offset, len, slice: input }],
	};

	for write in writes.into_iter().filter(|w| w.len != 0) {
		// Note: This can't panic because we resized memory.
		interpreter
			.memory
			.set_data(write.memory_offset, write.data_offset, write.len, write.slice);
	}

	ControlFlow::Continue(())
}

/// The shared logic for CODESIZE and CALLDATASIZE
fn in_data_size<E: Ext>(interpreter: &mut Interpreter<E>, is_codecopy: bool) -> U256 {
	let bytecode_len = interpreter.bytecode.len();
	let input_len = interpreter.input.len();
	let is_constructor = interpreter.ext.entry_point().is_evm_constructor();

	match (is_constructor, is_codecopy) {
		(true, true) => bytecode_len.saturating_add(input_len),
		(true, false) => 0,
		(false, true) => bytecode_len,
		(false, false) => input_len,
	}
	.into()
}
