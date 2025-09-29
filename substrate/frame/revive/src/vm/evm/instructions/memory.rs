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
	vm::{
		evm::{interpreter::Halt, util::as_usize_or_halt, EVMGas, Interpreter},
		Ext,
	},
	Error, U256,
};
use core::{cmp::max, ops::ControlFlow};
use revm::interpreter::gas::{copy_cost_verylow, BASE, VERYLOW};

/// Implements the MLOAD instruction.
///
/// Loads a 32-byte word from memory.
pub fn mload<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([], top) = interpreter.stack.popn_top()?;
	let offset = as_usize_or_halt::<E::T>(*top)?;
	interpreter.memory.resize(offset, 32)?;
	*top = U256::from_big_endian(interpreter.memory.slice_len(offset, 32));
	ControlFlow::Continue(())
}

/// Implements the MSTORE instruction.
///
/// Stores a 32-byte word to memory.
pub fn mstore<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let [offset, value] = interpreter.stack.popn()?;
	let offset = as_usize_or_halt::<E::T>(offset)?;
	interpreter.memory.resize(offset, 32)?;
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
	interpreter.memory.resize(offset, 1)?;
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
	interpreter.memory.resize(max(dst, src), len)?;
	// Copy memory in place
	interpreter.memory.copy(dst, src, len);
	ControlFlow::Continue(())
}
