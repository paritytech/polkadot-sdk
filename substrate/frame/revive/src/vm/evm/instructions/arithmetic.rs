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
	i256::{i256_div, i256_mod},
	Context,
};
use crate::vm::Ext;
use revm::{
	interpreter::{
		gas as revm_gas,
		interpreter_types::{RuntimeFlag, StackTr},
	},
	primitives::U256,
};

/// Implements the ADD instruction - adds two values from stack.
pub fn add<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);
	*op2 = op1.wrapping_add(*op2);
}

/// Implements the MUL instruction - multiplies two values from stack.
pub fn mul<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::LOW);
	popn_top!([op1], op2, context.interpreter);
	*op2 = op1.wrapping_mul(*op2);
}

/// Implements the SUB instruction - subtracts two values from stack.
pub fn sub<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);
	*op2 = op1.wrapping_sub(*op2);
}

/// Implements the DIV instruction - divides two values from stack.
pub fn div<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::LOW);
	popn_top!([op1], op2, context.interpreter);
	if !op2.is_zero() {
		*op2 = op1.wrapping_div(*op2);
	}
}

/// Implements the SDIV instruction.
///
/// Performs signed division of two values from stack.
pub fn sdiv<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::LOW);
	popn_top!([op1], op2, context.interpreter);
	*op2 = i256_div(op1, *op2);
}

/// Implements the MOD instruction.
///
/// Pops two values from stack and pushes the remainder of their division.
pub fn rem<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::LOW);
	popn_top!([op1], op2, context.interpreter);
	if !op2.is_zero() {
		*op2 = op1.wrapping_rem(*op2);
	}
}

/// Implements the SMOD instruction.
///
/// Performs signed modulo of two values from stack.
pub fn smod<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::LOW);
	popn_top!([op1], op2, context.interpreter);
	*op2 = i256_mod(op1, *op2)
}

/// Implements the ADDMOD instruction.
///
/// Pops three values from stack and pushes (a + b) % n.
pub fn addmod<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::MID);
	popn_top!([op1, op2], op3, context.interpreter);
	*op3 = op1.add_mod(op2, *op3)
}

/// Implements the MULMOD instruction.
///
/// Pops three values from stack and pushes (a * b) % n.
pub fn mulmod<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::MID);
	popn_top!([op1, op2], op3, context.interpreter);
	*op3 = op1.mul_mod(op2, *op3)
}

/// Implements the EXP instruction - exponentiates two values from stack.
pub fn exp<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	let spec_id = context.interpreter.runtime_flag.spec_id();
	popn_top!([op1], op2, context.interpreter);
	gas_or_fail!(context.interpreter, revm_gas::exp_cost(spec_id, *op2));
	*op2 = op1.pow(*op2);
}

/// Implements the `SIGNEXTEND` opcode as defined in the Ethereum Yellow Paper.
///
/// In the yellow paper `SIGNEXTEND` is defined to take two inputs, we will call them
/// `x` and `y`, and produce one output.
///
/// The first `t` bits of the output (numbering from the left, starting from 0) are
/// equal to the `t`-th bit of `y`, where `t` is equal to `256 - 8(x + 1)`.
///
/// The remaining bits of the output are equal to the corresponding bits of `y`.
///
/// **Note**: If `x >= 32` then the output is equal to `y` since `t <= 0`.
///
/// To efficiently implement this algorithm in the case `x < 32` we do the following.
///
/// Let `b` be equal to the `t`-th bit of `y` and let `s = 255 - t = 8x + 7`
/// (this is effectively the same index as `t`, but numbering the bits from the
/// right instead of the left).
///
/// We can create a bit mask which is all zeros up to and including the `t`-th bit,
/// and all ones afterwards by computing the quantity `2^s - 1`.
///
/// We can use this mask to compute the output depending on the value of `b`.
///
/// If `b == 1` then the yellow paper says the output should be all ones up to
/// and including the `t`-th bit, followed by the remaining bits of `y`; this is equal to
/// `y | !mask` where `|` is the bitwise `OR` and `!` is bitwise negation.
///
/// Similarly, if `b == 0` then the yellow paper says the output should start with all zeros,
/// then end with bits from `b`; this is equal to `y & mask` where `&` is bitwise `AND`.
pub fn signextend<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::LOW);
	popn_top!([ext], x, context.interpreter);
	// For 31 we also don't need to do anything.
	if ext < U256::from(31) {
		let ext = ext.as_limbs()[0];
		let bit_index = (8 * ext + 7) as usize;
		let bit = x.bit(bit_index);
		let mask = (U256::from(1) << bit_index) - U256::from(1);
		*x = if bit { *x | !mask } else { *x & mask };
	}
}
