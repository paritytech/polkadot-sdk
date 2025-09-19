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
		evm::{
			interpreter::{HaltReason, OutOfGasError},
			Interpreter,
		},
		Ext,
	},
	RuntimeCosts,
};
use revm::{
	interpreter::{
		gas::{BASE, HIGH, JUMPDEST, MID},
		interpreter_action::InterpreterAction,
		interpreter_types::{Jumps, LoopControl, StackTr},
		InstructionResult,
	},
	primitives::Bytes,
};
use sp_core::U256;
use sp_runtime::DispatchResult;

/// Implements the JUMP instruction.
///
/// Unconditional jump to a valid destination.
pub fn jump<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	interpreter.ext.gas_meter_mut().charge_evm_gas(MID)?;
	let [target] = interpreter.stack.popn()?;
	jump_inner(interpreter, target)?;
	Ok(())
}

/// Implements the JUMPI instruction.
///
/// Conditional jump to a valid destination if condition is true.
pub fn jumpi<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	interpreter.ext.gas_meter_mut().charge_evm_gas(HIGH)?;
	let [target, cond] = interpreter.stack.popn()?;

	if !cond.is_zero() {
		jump_inner(interpreter, target)?;
	}
	Ok(())
}

#[inline(always)]
/// Internal helper function for jump operations.
///
/// Validates jump target and performs the actual jump.
fn jump_inner<E: Ext>(interpreter: &mut Interpreter<'_, E>, target: U256) -> DispatchResult {
	let target = as_usize_checked(target).ok_or(HaltReason::InvalidJump)?;
	if !interpreter.bytecode.is_valid_legacy_jump(target) {
		return Err(HaltReason::InvalidJump.into());
	}
	// SAFETY: `is_valid_jump` ensures that `dest` is in bounds.
	interpreter.bytecode.absolute_jump(target);
	Ok(())
}

/// Implements the JUMPDEST instruction.
///
/// Marks a valid destination for jump operations.
pub fn jumpdest<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	interpreter.ext.gas_meter_mut().charge_evm_gas(JUMPDEST)?;
	Ok(())
}

/// Implements the PC instruction.
///
/// Pushes the current program counter onto the stack.
pub fn pc<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	interpreter.ext.gas_meter_mut().charge_evm_gas(BASE)?;
	// - 1 because we have already advanced the instruction pointer in `Interpreter::step`
	interpreter.stack.push(U256::from(interpreter.bytecode.pc() - 1))?;
	Ok(())
}

#[inline]
/// Internal helper function for return operations.
///
/// Handles memory data retrieval and sets the return action.
fn return_inner<E: Ext>(
	interpreter: &mut Interpreter<'_, E>,
	instruction_result: InstructionResult,
) -> DispatchResult {
	// Zero gas cost
	let [offset, len] = interpreter.stack.popn()?;
	let len = as_usize_checked(len).ok_or(HaltReason::OutOfGas(OutOfGasError::InvalidOperand))?;
	// Important: Offset must be ignored if len is zeros
	let mut output = Bytes::default();
	if len != 0 {
		let offset =
			as_usize_checked(offset).ok_or(HaltReason::OutOfGas(OutOfGasError::InvalidOperand))?;
		resize_memory_checked(interpreter, offset, len)?;
		output = interpreter.memory.slice_len(offset, len).to_vec().into()
	}

	todo!();
	// interpreter.bytecode.set_action(InterpreterAction::new_return(
	// 	instruction_result,
	// 	output,
	// 	interpreter.gas,
	// ));
	// Ok(())
}

/// Helper function to resize memory with proper error handling
#[inline]
fn resize_memory_checked<E: Ext>(
	interpreter: &mut Interpreter<'_, E>,
	offset: usize,
	len: usize,
) -> DispatchResult {
	let current_len = interpreter.memory.size();
	let target_len = revm::interpreter::num_words(offset.saturating_add(len)) * 32;
	if target_len as u32 > crate::limits::code::BASELINE_MEMORY_LIMIT {
		log::debug!(target: crate::LOG_TARGET, "check memory bounds failed: offset={} target_len={target_len} current_len={current_len}", offset);
		return Err(HaltReason::OutOfGas(OutOfGasError::Memory).into());
	}

	if target_len > current_len {
		interpreter.memory.resize(target_len);
	}
	Ok(())
}

/// Implements the RETURN instruction.
///
/// Halts execution and returns data from memory.
pub fn ret<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	return_inner(interpreter, InstructionResult::Return)
}

/// EIP-140: REVERT instruction
pub fn revert<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	return_inner(interpreter, InstructionResult::Revert)
}

/// Stop opcode. This opcode halts the execution.
pub fn stop<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	return_inner(interpreter, InstructionResult::Stop)
}

/// Invalid opcode. This opcode halts the execution.
pub fn invalid<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	interpreter.ext.gas_meter_mut().consume_all();
	Err(HaltReason::InvalidFEOpcode.into())
}

/// Unknown opcode. This opcode halts the execution.
pub fn unknown<'ext, E: Ext>(interpreter: &mut Interpreter<'ext, E>) -> DispatchResult {
	Err(HaltReason::OpcodeNotFound.into())
}

/// Helper function to convert U256 to usize, checking for overflow
fn as_usize_checked(value: U256) -> Option<usize> {
	let limbs = value.0;
	if (limbs[0] > usize::MAX as u64) | (limbs[1] != 0) | (limbs[2] != 0) | (limbs[3] != 0) {
		None
	} else {
		Some(limbs[0] as usize)
	}
}
