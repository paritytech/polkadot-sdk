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

use super::{utility::cast_slice_to_u256, Context};
use crate::vm::Ext;
use revm::{
	interpreter::{
		gas as revm_gas,
		interpreter_types::{Immediates, Jumps, StackTr},
		InstructionResult,
	},
	primitives::U256,
};

/// Implements the POP instruction.
///
/// Removes the top item from the stack.
pub fn pop<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	// Can ignore return. as relative N jump is safe operation.
	popn!([_i], context.interpreter);
}

/// EIP-3855: PUSH0 instruction
///
/// Introduce a new instruction which pushes the constant value 0 onto the stack.
pub fn push0<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	push!(context.interpreter, U256::ZERO);
}

/// Implements the PUSH1-PUSH32 instructions.
///
/// Pushes N bytes from bytecode onto the stack as a 32-byte value.
pub fn push<'ext, const N: usize, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	push!(context.interpreter, U256::ZERO);
	popn_top!([], top, context.interpreter);

	let imm = context.interpreter.bytecode.read_slice(N);
	cast_slice_to_u256(imm, top);

	// Can ignore return. as relative N jump is safe operation
	context.interpreter.bytecode.relative_jump(N as isize);
}

/// Implements the DUP1-DUP16 instructions.
///
/// Duplicates the Nth stack item to the top of the stack.
pub fn dup<'ext, const N: usize, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	if !context.interpreter.stack.dup(N) {
		context.interpreter.halt(InstructionResult::StackOverflow);
	}
}

/// Implements the SWAP1-SWAP16 instructions.
///
/// Swaps the top stack item with the Nth stack item.
pub fn swap<'ext, const N: usize, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	assert!(N != 0);
	if !context.interpreter.stack.exchange(0, N) {
		context.interpreter.halt(InstructionResult::StackOverflow);
	}
}
