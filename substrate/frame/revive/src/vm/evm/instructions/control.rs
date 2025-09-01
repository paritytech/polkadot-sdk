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
use crate::vm::Ext;
use revm::{
	interpreter::{
		gas as revm_gas,
		interpreter_action::InterpreterAction,
		interpreter_types::{Jumps, LoopControl, RuntimeFlag, StackTr},
		InstructionResult, Interpreter,
	},
	primitives::{Bytes, U256},
};

/// Implements the JUMP instruction.
///
/// Unconditional jump to a valid destination.
pub fn jump<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::MID);
	let Some([target]) = <_ as StackTr>::popn(&mut context.interpreter.stack) else {
		context.interpreter.halt(InstructionResult::StackUnderflow);
		return;
	};
	jump_inner(context.interpreter, target);
}

/// Implements the JUMPI instruction.
///
/// Conditional jump to a valid destination if condition is true.
pub fn jumpi<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::HIGH);
	let Some([target, cond]) = <_ as StackTr>::popn(&mut context.interpreter.stack) else {
		context.interpreter.halt(InstructionResult::StackUnderflow);
		return;
	};

	if !cond.is_zero() {
		jump_inner(context.interpreter, target);
	}
}

#[inline(always)]
/// Internal helper function for jump operations.
///
/// Validates jump target and performs the actual jump.
fn jump_inner(
	interpreter: &mut Interpreter<impl revm::interpreter::interpreter_types::InterpreterTypes>,
	target: U256,
) {
	let target = as_usize_or_fail!(interpreter, target, InstructionResult::InvalidJump);
	if !interpreter.bytecode.is_valid_legacy_jump(target) {
		interpreter.halt(InstructionResult::InvalidJump);
		return;
	}
	// SAFETY: `is_valid_jump` ensures that `dest` is in bounds.
	interpreter.bytecode.absolute_jump(target);
}

/// Implements the JUMPDEST instruction.
///
/// Marks a valid destination for jump operations.
pub fn jumpdest<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::JUMPDEST);
}

/// Implements the PC instruction.
///
/// Pushes the current program counter onto the stack.
pub fn pc<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::BASE);
	// - 1 because we have already advanced the instruction pointer in `Interpreter::step`
	push!(context.interpreter, U256::from(context.interpreter.bytecode.pc() - 1));
}

#[inline]
/// Internal helper function for return operations.
///
/// Handles memory data retrieval and sets the return action.
fn return_inner<'a, E: Ext>(
	interpreter: &mut Interpreter<crate::vm::evm::EVMInterpreter<'a, E>>,
	instruction_result: InstructionResult,
) {
	// Zero gas cost
	// gas_legacy!(interpreter, revm_gas::ZERO)
	let Some([offset, len]) = <_ as StackTr>::popn(&mut interpreter.stack) else {
		interpreter.halt(InstructionResult::StackUnderflow);
		return;
	};
	let len = as_usize_or_fail!(interpreter, len);
	// Important: Offset must be ignored if len is zeros
	let mut output = Bytes::default();
	if len != 0 {
		let offset = as_usize_or_fail!(interpreter, offset);
		resize_memory!(interpreter, offset, len);
		output = interpreter.memory.slice_len(offset, len).to_vec().into()
	}

	interpreter.bytecode.set_action(InterpreterAction::new_return(
		instruction_result,
		output,
		interpreter.gas,
	));
}

/// Implements the RETURN instruction.
///
/// Halts execution and returns data from memory.
pub fn ret<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	return_inner(context.interpreter, InstructionResult::Return);
}

/// EIP-140: REVERT instruction
pub fn revert<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	check!(context.interpreter, BYZANTIUM);
	return_inner(context.interpreter, InstructionResult::Revert);
}

/// Stop opcode. This opcode halts the execution.
pub fn stop<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	context.interpreter.halt(InstructionResult::Stop);
}

/// Invalid opcode. This opcode halts the execution.
pub fn invalid<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	context.interpreter.halt(InstructionResult::InvalidFEOpcode);
}

/// Unknown opcode. This opcode halts the execution.
pub fn unknown<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	context.interpreter.halt(InstructionResult::OpcodeNotFound);
}
