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

use super::{i256::i256_cmp, Context};
use crate::vm::Ext;
use core::cmp::Ordering;
use revm::{
	interpreter::{gas as revm_gas, interpreter_types::StackTr},
	primitives::U256,
};

/// Implements the LT instruction - less than comparison.
pub fn lt<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);
	*op2 = U256::from(op1 < *op2);
}

/// Implements the GT instruction - greater than comparison.
pub fn gt<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	*op2 = U256::from(op1 > *op2);
}

/// Implements the CLZ instruction - count leading zeros.
pub fn clz<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([], op1, context.interpreter);

	let leading_zeros = op1.leading_zeros();
	*op1 = U256::from(leading_zeros);
}

/// Implements the SLT instruction.
///
/// Signed less than comparison of two values from stack.
pub fn slt<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	*op2 = U256::from(i256_cmp(&op1, op2) == Ordering::Less);
}

/// Implements the SGT instruction.
///
/// Signed greater than comparison of two values from stack.
pub fn sgt<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	*op2 = U256::from(i256_cmp(&op1, op2) == Ordering::Greater);
}

/// Implements the EQ instruction.
///
/// Equality comparison of two values from stack.
pub fn eq<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	*op2 = U256::from(op1 == *op2);
}

/// Implements the ISZERO instruction.
///
/// Checks if the top stack value is zero.
pub fn iszero<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([], op1, context.interpreter);
	*op1 = U256::from(op1.is_zero());
}

/// Implements the AND instruction.
///
/// Bitwise AND of two values from stack.
pub fn bitand<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);
	*op2 = op1 & *op2;
}

/// Implements the OR instruction.
///
/// Bitwise OR of two values from stack.
pub fn bitor<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	*op2 = op1 | *op2;
}

/// Implements the XOR instruction.
///
/// Bitwise XOR of two values from stack.
pub fn bitxor<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	*op2 = op1 ^ *op2;
}

/// Implements the NOT instruction.
///
/// Bitwise NOT (negation) of the top stack value.
pub fn not<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([], op1, context.interpreter);

	*op1 = !*op1;
}

/// Implements the BYTE instruction.
///
/// Extracts a single byte from a word at a given index.
pub fn byte<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	let o1 = as_usize_saturated!(op1);
	*op2 = if o1 < 32 {
		// `31 - o1` because `byte` returns LE, while we want BE
		U256::from(op2.byte(31 - o1))
	} else {
		U256::ZERO
	};
}

/// EIP-145: Bitwise shifting instructions in EVM
pub fn shl<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	let shift = as_usize_saturated!(op1);
	*op2 = if shift < 256 { *op2 << shift } else { U256::ZERO }
}

/// EIP-145: Bitwise shifting instructions in EVM
pub fn shr<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	let shift = as_usize_saturated!(op1);
	*op2 = if shift < 256 { *op2 >> shift } else { U256::ZERO }
}

/// EIP-145: Bitwise shifting instructions in EVM
pub fn sar<'ext, E: Ext>(context: Context<'_, 'ext, E>) {
	gas_legacy!(context.interpreter, revm_gas::VERYLOW);
	popn_top!([op1], op2, context.interpreter);

	let shift = as_usize_saturated!(op1);
	*op2 = if shift < 256 {
		op2.arithmetic_shr(shift)
	} else if op2.bit(255) {
		U256::MAX
	} else {
		U256::ZERO
	};
}

#[cfg(test)]
mod tests {
	use super::{byte, clz, sar, shl, shr};
	use revm::{
		interpreter::{host::DummyHost, InstructionContext},
		primitives::{hardfork::SpecId, uint, U256},
	};

	pub fn test_interpreter() -> revm::interpreter::Interpreter<
		crate::vm::evm::EVMInterpreter<'static, crate::exec::mock_ext::MockExt<crate::tests::Test>>,
	> {
		use crate::tests::Test;
		use revm::{
			interpreter::{
				interpreter::{RuntimeFlags, SharedMemory},
				Interpreter, Stack,
			},
			primitives::hardfork::SpecId,
		};

		let mock_ext = Box::leak(Box::new(crate::exec::mock_ext::MockExt::<Test>::new()));

		Interpreter {
			gas: revm::interpreter::Gas::new(0),
			bytecode: Default::default(),
			stack: Stack::new(),
			return_data: Default::default(),
			memory: SharedMemory::new(),
			input: crate::vm::evm::EVMInputs::default(),
			runtime_flag: RuntimeFlags { is_static: false, spec_id: SpecId::default() },
			extend: mock_ext,
		}
	}

	#[test]
	fn test_shift_left() {
		let mut interpreter = test_interpreter();

		struct TestCase {
			value: U256,
			shift: U256,
			expected: U256,
		}

		uint! {
			let test_cases = [
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
					shift: 0x00_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
				},
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
					shift: 0x01_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000002_U256,
				},
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
					shift: 0xff_U256,
					expected: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
					shift: 0x0100_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
					shift: 0x0101_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0x00_U256,
					expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0x01_U256,
					expected: 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0xff_U256,
					expected: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0x0100_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
					shift: 0x01_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0x01_U256,
					expected: 0xfffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffe_U256,
				},
			];
		}

		for test in test_cases {
			push!(interpreter, test.value);
			push!(interpreter, test.shift);
			let context =
				InstructionContext { host: &mut DummyHost, interpreter: &mut interpreter };
			shl(context);
			let res = interpreter.stack.pop().unwrap();
			assert_eq!(res, test.expected);
		}
	}

	#[test]
	fn test_logical_shift_right() {
		let mut interpreter = test_interpreter();

		struct TestCase {
			value: U256,
			shift: U256,
			expected: U256,
		}

		uint! {
			let test_cases = [
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
					shift: 0x00_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
				},
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
					shift: 0x01_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
					shift: 0x01_U256,
					expected: 0x4000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
					shift: 0xff_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
				},
				TestCase {
					value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
					shift: 0x0100_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
					shift: 0x0101_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0x00_U256,
					expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0x01_U256,
					expected: 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0xff_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
				},
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					shift: 0x0100_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
				TestCase {
					value: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
					shift: 0x01_U256,
					expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				},
			];
		}

		for test in test_cases {
			push!(interpreter, test.value);
			push!(interpreter, test.shift);
			let context =
				InstructionContext { host: &mut DummyHost, interpreter: &mut interpreter };
			shr(context);
			let res = interpreter.stack.pop().unwrap();
			assert_eq!(res, test.expected);
		}
	}

	#[test]
	fn test_arithmetic_shift_right() {
		let mut interpreter = test_interpreter();

		struct TestCase {
			value: U256,
			shift: U256,
			expected: U256,
		}

		uint! {
		let test_cases = [
			TestCase {
				value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
				shift: 0x00_U256,
				expected: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
			},
			TestCase {
				value: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
				shift: 0x01_U256,
				expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
			},
			TestCase {
				value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
				shift: 0x01_U256,
				expected: 0xc000000000000000000000000000000000000000000000000000000000000000_U256,
			},
			TestCase {
				value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
				shift: 0xff_U256,
				expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
			},
			TestCase {
				value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
				shift: 0x0100_U256,
				expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
			},
			TestCase {
				value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256,
				shift: 0x0101_U256,
				expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
			},
			TestCase {
				value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0x00_U256,
				expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
			},
			TestCase {
				value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0x01_U256,
				expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
			},
			TestCase {
				value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0xff_U256,
				expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
			},
			TestCase {
				value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0x0100_U256,
				expected: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
			},
			TestCase {
				value: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
				shift: 0x01_U256,
				expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
			},
			TestCase {
				value: 0x4000000000000000000000000000000000000000000000000000000000000000_U256,
				shift: 0xfe_U256,
				expected: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
			},
			TestCase {
				value: 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0xf8_U256,
				expected: 0x000000000000000000000000000000000000000000000000000000000000007f_U256,
			},
			TestCase {
				value: 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0xfe_U256,
				expected: 0x0000000000000000000000000000000000000000000000000000000000000001_U256,
			},
			TestCase {
				value: 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0xff_U256,
				expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
			},
			TestCase {
				value: 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
				shift: 0x0100_U256,
				expected: 0x0000000000000000000000000000000000000000000000000000000000000000_U256,
			},
		];
			}

		for test in test_cases {
			push!(interpreter, test.value);
			push!(interpreter, test.shift);
			let context =
				InstructionContext { host: &mut DummyHost, interpreter: &mut interpreter };
			sar(context);
			let res = interpreter.stack.pop().unwrap();
			assert_eq!(res, test.expected);
		}
	}

	#[test]
	fn test_byte() {
		struct TestCase {
			input: U256,
			index: usize,
			expected: U256,
		}

		let mut interpreter = test_interpreter();

		let input_value = U256::from(0x1234567890abcdef1234567890abcdef_u128);
		let test_cases = (0..32)
			.map(|i| {
				let byte_pos = 31 - i;

				let shift_amount = U256::from(byte_pos * 8);
				let byte_value = (input_value >> shift_amount) & U256::from(0xFF);
				TestCase { input: input_value, index: i, expected: byte_value }
			})
			.collect::<Vec<_>>();

		for test in test_cases.iter() {
			push!(interpreter, test.input);
			push!(interpreter, U256::from(test.index));
			let context =
				InstructionContext { host: &mut DummyHost, interpreter: &mut interpreter };
			byte(context);
			let res = interpreter.stack.pop().unwrap();
			assert_eq!(res, test.expected, "Failed at index: {}", test.index);
		}
	}

	#[test]
	fn test_clz() {
		let mut interpreter = test_interpreter();
		interpreter.runtime_flag.spec_id = SpecId::OSAKA;

		struct TestCase {
			value: U256,
			expected: U256,
		}

		uint! {
			let test_cases = [
				TestCase { value: 0x0_U256, expected: 256_U256 },
				TestCase { value: 0x1_U256, expected: 255_U256 },
				TestCase { value: 0x2_U256, expected: 254_U256 },
				TestCase { value: 0x3_U256, expected: 254_U256 },
				TestCase { value: 0x4_U256, expected: 253_U256 },
				TestCase { value: 0x7_U256, expected: 253_U256 },
				TestCase { value: 0x8_U256, expected: 252_U256 },
				TestCase { value: 0xff_U256, expected: 248_U256 },
				TestCase { value: 0x100_U256, expected: 247_U256 },
				TestCase { value: 0xffff_U256, expected: 240_U256 },
				TestCase {
					value: 0xffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256, // U256::MAX
					expected: 0_U256,
				},
				TestCase {
					value: 0x8000000000000000000000000000000000000000000000000000000000000000_U256, // 1 << 255
					expected: 0_U256,
				},
				TestCase { // Smallest value with 1 leading zero
					value: 0x4000000000000000000000000000000000000000000000000000000000000000_U256, // 1 << 254
					expected: 1_U256,
				},
				TestCase { // Value just below 1 << 255
					value: 0x7fffffffffffffffffffffffffffffffffffffffffffffffffffffffffffffff_U256,
					expected: 1_U256,
				},
			];
		}

		for test in test_cases {
			push!(interpreter, test.value);
			let context =
				InstructionContext { host: &mut DummyHost, interpreter: &mut interpreter };
			clz(context);
			let res = interpreter.stack.pop().unwrap();
			assert_eq!(
				res, test.expected,
				"CLZ for value {:#x} failed. Expected: {}, Got: {}",
				test.value, test.expected, res
			);
		}
	}
}
