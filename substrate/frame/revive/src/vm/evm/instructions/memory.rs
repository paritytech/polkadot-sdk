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

use crate::{
	Error, U256,
	vm::{
		Ext,
		evm::{EVMGas, Interpreter, interpreter::Halt, util::as_usize_or_halt},
	},
};
use core::{cmp::max, ops::ControlFlow};
use revm::interpreter::gas::{BASE, VERYLOW, copy_cost_verylow};

/// Implements the MLOAD instruction.
///
/// Loads a 32-byte word from memory.
pub fn mload<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let [offset] = interpreter.stack.popn()?;
	let offset = as_usize_or_halt::<E::T>(offset)?;
	interpreter.resize_memory(offset, 32)?;
	let value = U256::from_big_endian(interpreter.memory.slice_len(offset, 32));
	interpreter.stack.push(value)
}

/// Implements the MSTORE instruction.
///
/// Stores a 32-byte word to memory.
pub fn mstore<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let [offset, value] = interpreter.stack.popn()?;
	let offset = as_usize_or_halt::<E::T>(offset)?;
	interpreter.resize_memory(offset, 32)?;
	interpreter.memory.set(offset, &value.to_big_endian());
	ControlFlow::Continue(())
}

/// Implements the MSTORE8 instruction.
///
/// Stores a single byte to memory.
pub fn mstore8<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let [offset, value] = interpreter.stack.popn()?;
	let offset = as_usize_or_halt::<E::T>(offset)?;
	interpreter.resize_memory(offset, 1)?;
	interpreter.memory.set(offset, &[value.byte(0)]);
	ControlFlow::Continue(())
}

/// Implements the MSIZE instruction.
///
/// Gets the size of active memory in bytes.
pub fn msize<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	interpreter.stack.push(U256::from(interpreter.memory.size()))
}

/// Implements the MCOPY instruction.
///
/// EIP-5656: Memory copying instruction that copies memory from one location to another.
pub fn mcopy<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	let [dst, src, len] = interpreter.stack.popn()?;

	// Into usize or fail
	let len = as_usize_or_halt::<E::T>(len)?;
	// Deduce gas
	let Some(gas_cost) = copy_cost_verylow(len) else {
		return ControlFlow::Break(Error::<E::T>::OutOfGas.into());
	};
	interpreter.ext.charge_or_halt(EVMGas(gas_cost))?;
	if len == 0 {
		return ControlFlow::Continue(());
	}

	let dst = as_usize_or_halt::<E::T>(dst)?;
	let src = as_usize_or_halt::<E::T>(src)?;
	// Resize memory
	interpreter.resize_memory(max(dst, src), len)?;
	// Copy memory in place
	interpreter.memory.copy(dst, src, len);
	ControlFlow::Continue(())
}

#[cfg(test)]
mod tests {
	use crate::{
		exec::{PrecompileExt, mock_ext::MockExt},
		precompiles::Token,
		tests::Test,
		vm::evm::{EVMGas, Interpreter},
	};
	use frame_support::weights::Weight;
	use revm::interpreter::{gas::memory_gas, num_words};

	/// Weight of `gas` EVM gas units under the `Test` runtime.
	fn evm_gas_weight(gas: u64) -> Weight {
		<EVMGas as Token<Test>>::weight(&EVMGas(gas))
	}

	// Growing memory must charge the quadratic `memory_gas` cost, not be free.
	#[test]
	fn memory_expansion_gas_is_charged() {
		let mut ext = MockExt::<Test>::new();
		let mut interpreter = Interpreter::new(Default::default(), vec![], &mut ext);

		// 64 KiB == 2048 words == 14_336 gas, matching Ethereum.
		let len = 64 * 1024;
		let words = num_words(len);
		assert_eq!(memory_gas(words), 14_336);

		let before = interpreter.ext.frame_meter().weight_consumed();
		assert!(interpreter.resize_memory(0, len).is_continue());
		let charged = interpreter.ext.frame_meter().weight_consumed().saturating_sub(before);

		assert_eq!(charged, evm_gas_weight(memory_gas(words)));
		assert!(charged.ref_time() > 0);

		// Re-touching allocated memory is free.
		let before = interpreter.ext.frame_meter().weight_consumed();
		assert!(interpreter.resize_memory(0, len).is_continue());
		assert_eq!(interpreter.ext.frame_meter().weight_consumed(), before);

		// Super-linear: doubling the size costs more than the first half.
		let before = interpreter.ext.frame_meter().weight_consumed();
		assert!(interpreter.resize_memory(0, 2 * len).is_continue());
		let second_half = interpreter.ext.frame_meter().weight_consumed().saturating_sub(before);
		assert!(second_half.ref_time() > charged.ref_time());
	}
}
