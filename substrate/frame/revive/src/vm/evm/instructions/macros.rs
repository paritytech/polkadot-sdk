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

//! Utility macros to help implementing opcode instruction functions.

/// `const` Option `?`.
#[macro_export]
macro_rules! tri {
	($e:expr) => {
		match $e {
			Some(v) => v,
			None => return None,
		}
	};
}

/// Fails the instruction if the current call is static.
#[macro_export]
macro_rules! require_non_staticcall {
	($interpreter:expr) => {
		if $interpreter.runtime_flag.is_static() {
			$interpreter.halt(revm::interpreter::InstructionResult::StateChangeDuringStaticCall);
			return;
		}
	};
}

/// Macro for optional try - returns early if the expression evaluates to None.
/// Similar to the `?` operator but for use in instruction implementations.
#[macro_export]
macro_rules! otry {
	($expression: expr) => {{
		let Some(value) = $expression else {
			return;
		};
		value
	}};
}

/// Error if the current call is executing EOF.
#[macro_export]
macro_rules! require_eof {
	($interpreter:expr) => {
		if !$interpreter.runtime_flag.is_eof() {
			$interpreter.halt(revm::interpreter::InstructionResult::EOFOpcodeDisabledInLegacy);
			return;
		}
	};
}

/// Check if the `SPEC` is enabled, and fail the instruction if it is not.
#[macro_export]
macro_rules! check {
	($interpreter:expr, $min:ident) => {
		if !$interpreter
			.runtime_flag
			.spec_id()
			.is_enabled_in(revm::primitives::hardfork::SpecId::$min)
		{
			$interpreter.halt(revm::interpreter::InstructionResult::NotActivated);
			return;
		}
	};
}

/// Records a `gas` cost and fails the instruction if it would exceed the available gas.
#[macro_export]
macro_rules! gas_legacy {
	($interpreter:expr, $gas:expr) => {
		gas_legacy!($interpreter, $gas, ())
	};
	($interpreter:expr, $gas:expr, $ret:expr) => {
		if $interpreter.extend.gas_meter_mut().charge_evm_gas($gas).is_err() {
			$interpreter.halt(revm::interpreter::InstructionResult::OutOfGas);
			return $ret;
		}
	};
}

#[macro_export]
macro_rules! gas {
	($interpreter:expr, $gas:expr) => {
		gas!($interpreter, $gas, ())
	};
	($interpreter:expr, $gas:expr, $ret:expr) => {
		let meter = $interpreter.extend.gas_meter_mut();
		if meter.charge_evm_gas(1).is_err() || meter.charge($gas).is_err() {
			$interpreter.halt(revm::interpreter::InstructionResult::OutOfGas);
			return $ret;
		}
	};
}

/// Same as [`gas_legacy!`], but with `gas` as an option.
#[macro_export]
macro_rules! gas_or_fail_legacy {
	($interpreter:expr, $gas:expr) => {
		gas_or_fail_legacy!($interpreter, $gas, ())
	};
	($interpreter:expr, $gas:expr, $ret:expr) => {
		match $gas {
			Some(gas_used) => gas_legacy!($interpreter, gas_used, $ret),
			None => {
				$interpreter.halt(revm::interpreter::InstructionResult::OutOfGas);
				return $ret;
			},
		}
	};
}

use crate::vm::Ext;
use revm::interpreter::gas::{MemoryExtensionResult, MemoryGas};

/// Adapted from
/// https://docs.rs/revm/latest/revm/interpreter/struct.Gas.html#method.record_memory_expansion
pub fn record_memory_expansion<E: Ext>(
	memory: &mut MemoryGas,
	ext: &mut E,
	new_len: usize,
) -> MemoryExtensionResult {
	let Some(additional_cost) = memory.record_new_len(new_len) else {
		return MemoryExtensionResult::Same;
	};

	if ext.gas_meter_mut().charge_evm_gas(additional_cost).is_err() {
		return MemoryExtensionResult::OutOfGas;
	}

	MemoryExtensionResult::Extended
}

/// Resizes the interpreterreter memory if necessary. Fails the instruction if the memory or gas
/// limit is exceeded.
#[macro_export]
macro_rules! resize_memory {
	($interpreter:expr, $offset:expr, $len:expr) => {
		resize_memory!($interpreter, $offset, $len, ())
	};
	($interpreter:expr, $offset:expr, $len:expr, $ret:expr) => {
		let words_num = revm::interpreter::num_words($offset.saturating_add($len));
		match crate::vm::evm::instructions::macros::record_memory_expansion(
			$interpreter.gas.memory_mut(),
			$interpreter.extend,
			words_num,
		) {
			revm::interpreter::gas::MemoryExtensionResult::Extended => {
				$interpreter.memory.resize(words_num * 32);
			},
			revm::interpreter::gas::MemoryExtensionResult::OutOfGas => {
				$interpreter.halt(revm::interpreter::InstructionResult::MemoryOOG);
				return $ret;
			},
			revm::interpreter::gas::MemoryExtensionResult::Same => (), // no action
		};
	};
}

/// Pops n values from the stack. Fails the instruction if n values can't be popped.
#[macro_export]
macro_rules! popn {
    ([ $($x:ident),* ],$interpreterreter:expr $(,$ret:expr)? ) => {
        let Some([$( $x ),*]) = <_ as StackTr>::popn(&mut $interpreterreter.stack) else {
            $interpreterreter.halt(revm::interpreter::InstructionResult::StackUnderflow);
            return $($ret)?;
        };
    };
}

/// Pops n values from the stack and returns the top value. Fails the instruction if n values can't
/// be popped.
#[macro_export]
macro_rules! popn_top {
    ([ $($x:ident),* ], $top:ident, $interpreter:expr $(,$ret:expr)? ) => {
		let Some(([$($x),*], $top)) = <_ as StackTr>::popn_top(&mut $interpreter.stack) else {
            $interpreter.halt(revm::interpreter::InstructionResult::StackUnderflow);
            return $($ret)?;
        };
    };
}

/// Pushes a `B256` value onto the stack. Fails the instruction if the stack is full.
#[macro_export]
macro_rules! push {
    ($interpreter:expr, $x:expr $(,$ret:item)?) => (
        if !($interpreter.stack.push($x)) {
            $interpreter.halt(revm::interpreter::InstructionResult::StackOverflow);
            return $($ret)?;
        }
    )
}

/// Converts a `U256` value to a `u64`, saturating to `MAX` if the value is too large.
#[macro_export]
macro_rules! as_u64_saturated {
	($v:expr) => {
		match $v.as_limbs() {
			x =>
				if (x[1] == 0) & (x[2] == 0) & (x[3] == 0) {
					x[0]
				} else {
					u64::MAX
				},
		}
	};
}

/// Converts a `U256` value to a `usize`, saturating to `MAX` if the value is too large.
#[macro_export]
macro_rules! as_usize_saturated {
	($v:expr) => {
		usize::try_from(as_u64_saturated!($v)).unwrap_or(usize::MAX)
	};
}

/// Converts a `U256` value to a `isize`, saturating to `isize::MAX` if the value is too large.
#[macro_export]
macro_rules! as_isize_saturated {
	($v:expr) => {
		// `isize_try_from(u64::MAX)`` will fail and return isize::MAX
		// This is expected behavior as we are saturating the value.
		isize::try_from(as_u64_saturated!($v)).unwrap_or(isize::MAX)
	};
}

/// Converts a `U256` value to a `usize`, failing the instruction if the value is too large.
#[macro_export]
macro_rules! as_usize_or_fail {
	($interpreter:expr, $v:expr) => {
		as_usize_or_fail_ret!($interpreter, $v, ())
	};
	($interpreter:expr, $v:expr, $reason:expr) => {
		as_usize_or_fail_ret!($interpreter, $v, $reason, ())
	};
}

/// Converts a `U256` value to a `usize` and returns `ret`,
/// failing the instruction if the value is too large.
#[macro_export]
macro_rules! as_usize_or_fail_ret {
	($interpreter:expr, $v:expr, $ret:expr) => {
		as_usize_or_fail_ret!(
			$interpreter,
			$v,
			revm::interpreter::InstructionResult::InvalidOperandOOG,
			$ret
		)
	};

	($interpreter:expr, $v:expr, $reason:expr, $ret:expr) => {
		match $v.as_limbs() {
			x => {
				if (x[0] > usize::MAX as u64) | (x[1] != 0) | (x[2] != 0) | (x[3] != 0) {
					$interpreter.halt($reason);
					return $ret;
				}
				x[0] as usize
			},
		}
	};
}
