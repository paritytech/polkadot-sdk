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
		evm::{interpreter::Halt, EVMGas, Interpreter},
		Ext,
	},
	U256,
};
use core::ops::ControlFlow;
use revm::interpreter::gas::{BASE, VERYLOW};

/// Implements the POP instruction.
///
/// Removes the top item from the stack.
pub fn pop<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	let [_] = interpreter.stack.popn()?;
	ControlFlow::Continue(())
}

/// EIP-3855: PUSH0 instruction
///
/// Introduce a new instruction which pushes the constant value 0 onto the stack.
pub fn push0<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(BASE))?;
	interpreter.stack.push(U256::zero())
}

/// Implements the PUSH1-PUSH32 instructions.
///
/// Pushes N bytes from bytecode onto the stack as a 32-byte value.
pub fn push<'ext, const N: usize, E: Ext>(
	interpreter: &mut Interpreter<'ext, E>,
) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;

	let slice = interpreter.bytecode.read_slice(N);
	interpreter.stack.push_slice(slice)?;

	// Can ignore return. as relative N jump is safe operation
	interpreter.bytecode.relative_jump(N as isize);
	ControlFlow::Continue(())
}

/// Implements the DUP1-DUP16 instructions.
///
/// Duplicates the Nth stack item to the top of the stack.
pub fn dup<'ext, const N: usize, E: Ext>(
	interpreter: &mut Interpreter<'ext, E>,
) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	interpreter.stack.dup(N)
}

/// Implements the SWAP1-SWAP16 instructions.
///
/// Swaps the top stack item with the Nth stack item.
pub fn swap<'ext, const N: usize, E: Ext>(
	interpreter: &mut Interpreter<'ext, E>,
) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	assert!(N != 0);
	interpreter.stack.exchange(0, N)
}
