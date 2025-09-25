use crate::{vm::evm::{interpreter::Halt, HaltReason}, U256};
use core::ops::ControlFlow;

/// Helper function to convert U256 to usize, checking for overflow
pub fn as_usize_or_halt_with(value: U256, halt: impl Fn() -> Halt) -> ControlFlow<Halt, usize> {
	let limbs = value.0;
	if (limbs[0] > usize::MAX as u64) | (limbs[1] != 0) | (limbs[2] != 0) | (limbs[3] != 0) {
		ControlFlow::Break(halt())
	} else {
		ControlFlow::Continue(limbs[0] as usize)
	}
}

/// Helper function to convert U256 to usize, checking for overflow, with default InvalidOperandOOG error
pub fn as_usize_or_halt(value: U256) -> ControlFlow<Halt, usize> {
	as_usize_or_halt_with(value, || HaltReason::InvalidOperandOOG.into())
}
