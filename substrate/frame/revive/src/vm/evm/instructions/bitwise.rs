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

mod bits;

use super::{arithmetic::i256::i256_cmp, utility::as_usize_saturated};
use crate::{
	vm::{
		evm::{interpreter::Halt, EVMGas, Interpreter},
		Ext,
	},
	U256,
};
use bits::Bits;
use core::{cmp::Ordering, ops::ControlFlow};
use revm::interpreter::gas::VERYLOW;

/// Implements the LT instruction - less than comparison.
pub fn lt<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;
	*op2 = if op1 < *op2 { U256::one() } else { U256::zero() };
	ControlFlow::Continue(())
}

/// Implements the GT instruction - greater than comparison.
pub fn gt<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	*op2 = if op1 > *op2 { U256::one() } else { U256::zero() };
	ControlFlow::Continue(())
}

/// Implements the CLZ instruction - count leading zeros.
pub fn clz<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([], op1) = interpreter.stack.popn_top()?;

	let leading_zeros = op1.leading_zeros();
	*op1 = U256::from(leading_zeros);
	ControlFlow::Continue(())
}

/// Implements the SLT instruction.
///
/// Signed less than comparison of two values from stack.
pub fn slt<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	*op2 = if i256_cmp(&op1, op2) == Ordering::Less { U256::one() } else { U256::zero() };
	ControlFlow::Continue(())
}

/// Implements the SGT instruction.
///
/// Signed greater than comparison of two values from stack.
pub fn sgt<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	*op2 = if i256_cmp(&op1, op2) == Ordering::Greater { U256::one() } else { U256::zero() };
	ControlFlow::Continue(())
}

/// Implements the EQ instruction.
///
/// Equality comparison of two values from stack.
pub fn eq<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	*op2 = if op1 == *op2 { U256::one() } else { U256::zero() };
	ControlFlow::Continue(())
}

/// Implements the ISZERO instruction.
///
/// Checks if the top stack value is zero.
pub fn iszero<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([], op1) = interpreter.stack.popn_top()?;

	*op1 = if op1.is_zero() { U256::one() } else { U256::zero() };
	ControlFlow::Continue(())
}

/// Implements the AND instruction.
///
/// Bitwise AND of two values from stack.
pub fn bitand<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;
	*op2 = op1 & *op2;
	ControlFlow::Continue(())
}

/// Implements the OR instruction.
///
/// Bitwise OR of two values from stack.
pub fn bitor<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;
	*op2 = op1 | *op2;
	ControlFlow::Continue(())
}

/// Implements the XOR instruction.
///
/// Bitwise XOR of two values from stack.
pub fn bitxor<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;
	*op2 = op1 ^ *op2;
	ControlFlow::Continue(())
}

/// Implements the NOT instruction.
///
/// Bitwise NOT (negation) of the top stack value.
pub fn not<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([], op1) = interpreter.stack.popn_top()?;
	*op1 = !*op1;
	ControlFlow::Continue(())
}

/// Implements the BYTE instruction.
///
/// Extracts a single byte from a word at a given index.
pub fn byte<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	let o1 = as_usize_saturated(op1);
	*op2 = if o1 < 32 {
		// `31 - o1` because `byte` returns LE, while we want BE
		U256::from(op2.byte(31 - o1))
	} else {
		U256::zero()
	};
	ControlFlow::Continue(())
}

/// EIP-145: Bitwise shifting instructions in EVM
pub fn shl<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	let shift = as_usize_saturated(op1);
	*op2 = if shift < 256 { *op2 << shift } else { U256::zero() };
	ControlFlow::Continue(())
}

/// EIP-145: Bitwise shifting instructions in EVM
pub fn shr<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	let shift = as_usize_saturated(op1);
	*op2 = if shift < 256 { *op2 >> shift } else { U256::zero() };
	ControlFlow::Continue(())
}

/// EIP-145: Bitwise shifting instructions in EVM
pub fn sar<E: Ext>(interpreter: &mut Interpreter<E>) -> ControlFlow<Halt> {
	interpreter.ext.charge_or_halt(EVMGas(VERYLOW))?;
	let ([op1], op2) = interpreter.stack.popn_top()?;

	let shift = as_usize_saturated(op1);
	*op2 = if shift < 256 {
		op2.arithmetic_shr(shift)
	} else if op2.bit(255) {
		U256::MAX
	} else {
		U256::zero()
	};
	ControlFlow::Continue(())
}

#[cfg(test)]
mod tests {
	use super::{byte, clz, sar, shl, shr};
	use crate::{tests::Test, vm::evm::Interpreter};
	use alloy_core::hex;
	use core::ops::ControlFlow;
	use sp_core::U256;

	macro_rules! test_interpreter {
		($interpreter: ident) => {
			let mut mock_ext = crate::exec::mock_ext::MockExt::<Test>::new();
			let mut $interpreter = Interpreter::new(Default::default(), vec![], &mut mock_ext);
		};
	}

	#[test]
	fn test_shift_left() {
		struct TestCase {
			value: U256,
			shift: U256,
			expected: U256,
		}

		let test_cases = [
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("00")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000002"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("ff")),
				expected: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("0100")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("0101")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("00")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("ff")),
				expected: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("0100")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe"
				)),
			},
		];

		test_interpreter!(interpreter);
		for test in test_cases {
			assert!((|| {
				interpreter.stack.push(test.value)?;
				interpreter.stack.push(test.shift)?;
				shl(&mut interpreter)?;
				let [res] = interpreter.stack.popn::<1>()?;
				assert_eq!(res, test.expected);
				ControlFlow::Continue(())
			})()
			.is_continue());
		}
	}

	#[test]
	fn test_logical_shift_right() {
		struct TestCase {
			value: U256,
			shift: U256,
			expected: U256,
		}

		let test_cases = [
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("00")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"4000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("ff")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("0100")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("0101")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("00")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("ff")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("0100")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
		];

		test_interpreter!(interpreter);
		for test in test_cases {
			assert!((|| {
				interpreter.stack.push(test.value)?;
				interpreter.stack.push(test.shift)?;
				shr(&mut interpreter)?;
				let [res] = interpreter.stack.popn::<1>()?;
				assert_eq!(res, test.expected);
				ControlFlow::Continue(())
			})()
			.is_continue());
		}
	}

	#[test]
	fn test_arithmetic_shift_right() {
		struct TestCase {
			value: U256,
			shift: U256,
			expected: U256,
		}

		let test_cases = [
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("00")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"c000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("ff")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("0100")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("0101")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("00")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("ff")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("0100")),
				expected: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("01")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"4000000000000000000000000000000000000000000000000000000000000000"
				)),
				shift: U256::from_big_endian(&hex!("fe")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("f8")),
				expected: U256::from_big_endian(&hex!(
					"000000000000000000000000000000000000000000000000000000000000007f"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("fe")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000001"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("ff")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				shift: U256::from_big_endian(&hex!("0100")),
				expected: U256::from_big_endian(&hex!(
					"0000000000000000000000000000000000000000000000000000000000000000"
				)),
			},
		];

		test_interpreter!(interpreter);
		for test in test_cases {
			assert!((|| {
				interpreter.stack.push(test.value)?;
				interpreter.stack.push(test.shift)?;
				sar(&mut interpreter)?;
				let [res] = interpreter.stack.popn::<1>()?;
				assert_eq!(res, test.expected);
				ControlFlow::Continue(())
			})()
			.is_continue());
		}
	}

	#[test]
	fn test_byte() {
		struct TestCase {
			input: U256,
			index: usize,
			expected: U256,
		}

		let input_value = U256::from_big_endian(&hex!("1234567890abcdef1234567890abcdef"));
		let test_cases = (0..32)
			.map(|i| {
				let byte_pos = 31 - i;
				let shift_amount = U256::from(byte_pos * 8);
				let byte_value = (input_value >> shift_amount) & U256::from(0xFF);
				TestCase { input: input_value, index: i, expected: byte_value }
			})
			.collect::<Vec<_>>();

		test_interpreter!(interpreter);
		for test in test_cases.iter() {
			assert!((|| {
				interpreter.stack.push(test.input)?;
				interpreter.stack.push(U256::from(test.index))?;
				byte(&mut interpreter)?;
				let [res] = interpreter.stack.popn::<1>()?;
				assert_eq!(res, test.expected, "Failed at index: {}", test.index);
				ControlFlow::Continue(())
			})()
			.is_continue());
		}
	}

	#[test]
	fn test_clz() {
		struct TestCase {
			value: U256,
			expected: U256,
		}

		let test_cases = [
			TestCase { value: U256::from_big_endian(&hex!("00")), expected: U256::from(256) },
			TestCase { value: U256::from_big_endian(&hex!("01")), expected: U256::from(255) },
			TestCase { value: U256::from_big_endian(&hex!("02")), expected: U256::from(254) },
			TestCase { value: U256::from_big_endian(&hex!("03")), expected: U256::from(254) },
			TestCase { value: U256::from_big_endian(&hex!("04")), expected: U256::from(253) },
			TestCase { value: U256::from_big_endian(&hex!("07")), expected: U256::from(253) },
			TestCase { value: U256::from_big_endian(&hex!("08")), expected: U256::from(252) },
			TestCase { value: U256::from_big_endian(&hex!("ff")), expected: U256::from(248) },
			TestCase { value: U256::from_big_endian(&hex!("0100")), expected: U256::from(247) },
			TestCase { value: U256::from_big_endian(&hex!("ffff")), expected: U256::from(240) },
			TestCase {
				value: U256::from_big_endian(&hex!(
					"ffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				expected: U256::from(0),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"8000000000000000000000000000000000000000000000000000000000000000"
				)),
				expected: U256::from(0),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"4000000000000000000000000000000000000000000000000000000000000000"
				)),
				expected: U256::from(1),
			},
			TestCase {
				value: U256::from_big_endian(&hex!(
					"7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff"
				)),
				expected: U256::from(1),
			},
		];

		test_interpreter!(interpreter);
		for test in test_cases.iter() {
			assert!((|| {
				interpreter.stack.push(test.value)?;
				clz(&mut interpreter)?;
				let [res] = interpreter.stack.popn::<1>()?;
				assert_eq!(
					res, test.expected,
					"CLZ for value {:#x} failed. Expected: {}, Got: {}",
					test.value, test.expected, res
				);
				ControlFlow::Continue(())
			})()
			.is_continue());
		}
	}
}
