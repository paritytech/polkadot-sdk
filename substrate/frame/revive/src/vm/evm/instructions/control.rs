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
		evm::{interpreter::Halt, util::as_usize_or_halt_with, Interpreter},
		Ext,
	},
	RuntimeCosts, U256,
};
use core::ops::ControlFlow;
use revm::{
	interpreter::{
		gas::{BASE, HIGH, JUMPDEST, MID},
		interpreter_types::{Immediates, Jumps},
	},
	primitives::Bytes,
};

/// Implements the JUMP instruction.
///
/// Unconditional jump to a valid destination.
pub fn jump<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	interpreter.ext.gas_meter_mut().charge_evm_gas(MID)?;
	let [target] = interpreter.stack.popn()?;
	jump_inner(interpreter, target)?;
	ControlFlow::Continue(())
}

/// Implements the JUMPI instruction.
///
/// Conditional jump to a valid destination if condition is true.
pub fn jumpi<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	interpreter.ext.gas_meter_mut().charge_evm_gas(HIGH)?;
	let [target, cond] = interpreter.stack.popn()?;

	if !cond.is_zero() {
		jump_inner(interpreter, target)?;
	}
	ControlFlow::Continue(())
}

#[inline(always)]
/// Internal helper function for jump operations.
///
/// Validates jump target and performs the actual jump.
fn jump_inner<E: Ext>(interpreter: &mut Interpreter<'_, E>, target: U256) -> ControlFlow<Halt> {
	let target = as_usize_or_halt_with(target, || Halt::InvalidJump)?;

	if !interpreter.bytecode.is_valid_legacy_jump(target) {
		return ControlFlow::Break(Halt::InvalidJump);
	}
	// SAFETY: `is_valid_jump` ensures that `dest` is in bounds.
	interpreter.bytecode.absolute_jump(target);
	ControlFlow::Continue(())
}

/// Implements the JUMPDEST instruction.
///
/// Marks a valid destination for jump operations.
pub fn jumpdest<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	interpreter.ext.gas_meter_mut().charge_evm_gas(JUMPDEST)?;
	ControlFlow::Continue(())
}

/// Implements the PC instruction.
///
/// Pushes the current program counter onto the stack.
pub fn pc<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	interpreter.ext.gas_meter_mut().charge_evm_gas(BASE)?;
	// - 1 because we have already advanced the instruction pointer in `Interpreter::step`
	interpreter.stack.push(U256::from(interpreter.bytecode.pc() - 1))?;
	ControlFlow::Continue(())
}

#[inline]
/// Internal helper function for return operations.
///
/// Handles memory data retrieval and sets the return action.
fn return_inner<E: Ext>(
	interpreter: &mut Interpreter<'_, E>,
	halt: impl Fn(Vec<u8>) -> Halt,
) -> ControlFlow<Halt> {
	// Zero gas cost
	let [offset, len] = interpreter.stack.popn()?;
	let len = as_usize_or_halt_with(len, || Halt::InvalidOperandOOG)?;

	// Important: Offset must be ignored if len is zeros
	let mut output = Default::default();
	if len != 0 {
		let offset = as_usize_or_halt_with(offset, || Halt::InvalidOperandOOG)?;
		interpreter.memory.resize(offset, len)?;
		output = interpreter.memory.slice_len(offset, len).to_vec()
	}

	ControlFlow::Break(halt(output))
}

/// Implements the RETURN instruction.
///
/// Halts execution and returns data from memory.
pub fn ret<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	return_inner(interpreter, Halt::Return)
}

/// EIP-140: REVERT instruction
pub fn revert<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	return_inner(interpreter, Halt::Revert)
}

/// Stop opcode. This opcode halts the execution.
pub fn stop<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	return_inner(interpreter, |_| Halt::Stop)
}

/// Invalid opcode. This opcode halts the execution.
pub fn invalid<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	interpreter.ext.gas_meter_mut().consume_all();
	ControlFlow::Break(Halt::InvalidFEOpcode)
}

/// Unknown opcode. This opcode halts the execution.
pub fn unknown<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> ControlFlow<Halt> {
	ControlFlow::Break(Halt::OpcodeNotFound)
}
