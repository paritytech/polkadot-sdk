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

//! Custom EVM stack implementation using sp_core::U256

use crate::vm::evm::interpreter::Halt;
use alloc::vec::Vec;
use core::ops::ControlFlow;
use sp_core::{H160, H256, U256};

/// EVM stack implementation using sp_core types
#[derive(Debug, Clone)]
pub struct Stack(Vec<U256>);

/// A trait for converting types into an unsigned 256-bit integer (`U256`).
pub trait ToU256 {
	/// Converts `self` into a `U256`.
	fn to_u256(self) -> U256;
}

impl ToU256 for U256 {
	fn to_u256(self) -> U256 {
		self
	}
}

impl ToU256 for u64 {
	fn to_u256(self) -> U256 {
		self.into()
	}
}

impl ToU256 for H160 {
	fn to_u256(self) -> U256 {
		U256::from_big_endian(H256::from(self).as_ref())
	}
}

impl Stack {
	/// Create a new empty stack
	pub fn new() -> Self {
		Self(Vec::with_capacity(32))
	}

	/// Push a value onto the stack
	/// Returns Continue(()) if successful, Break(Halt::StackOverflow) if stack would overflow
	pub fn push(&mut self, value: impl ToU256) -> ControlFlow<Halt> {
		if self.0.len() >= 1024 {
			ControlFlow::Break(Halt::StackOverflow)
		} else {
			self.0.push(value.to_u256());
			ControlFlow::Continue(())
		}
	}

	/// Get a reference to the top stack item without removing it
	pub fn top(&self) -> Option<&U256> {
		self.0.last()
	}

	/// Get the current stack size
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// Check if stack is empty
	#[cfg(test)]
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Pop multiple values from the stack
	/// Returns Continue(array) if successful, Break(Halt::StackUnderflow) if not enough values on
	/// stack
	pub fn popn<const N: usize>(&mut self) -> ControlFlow<Halt, [U256; N]> {
		if self.0.len() < N {
			return ControlFlow::Break(Halt::StackUnderflow);
		}

		let mut result: [U256; N] = [U256::zero(); N];
		for i in 0..N {
			match self.0.pop() {
				Some(value) => result[i] = value,
				None => return ControlFlow::Break(Halt::StackUnderflow),
			}
		}
		ControlFlow::Continue(result)
	}

	/// Pop multiple values and return them along with a mutable reference to the new top
	/// This is used for operations that pop some values and modify the top of the stack
	pub fn popn_top<const N: usize>(&mut self) -> ControlFlow<Halt, ([U256; N], &mut U256)> {
		if self.0.len() < N + 1 {
			return ControlFlow::Break(Halt::StackUnderflow);
		}

		let mut popped: [U256; N] = [U256::zero(); N];
		for i in 0..N {
			match self.0.pop() {
				Some(value) => popped[i] = value,
				None => return ControlFlow::Break(Halt::StackUnderflow),
			}
		}

		// Get mutable reference to the new top
		match self.0.last_mut() {
			Some(top) => ControlFlow::Continue((popped, top)),
			None => ControlFlow::Break(Halt::StackUnderflow),
		}
	}

	/// Duplicate the Nth item from the top and push it onto the stack
	/// Returns Continue(()) if successful, Break(Halt) if stack would overflow or index is invalid
	pub fn dup(&mut self, n: usize) -> ControlFlow<Halt> {
		if n == 0 || n > self.0.len() {
			return ControlFlow::Break(Halt::StackUnderflow);
		}
		if self.0.len() >= 1024 {
			return ControlFlow::Break(Halt::StackOverflow);
		}

		let idx = self.0.len() - n;
		let value = self.0[idx];
		self.0.push(value);
		ControlFlow::Continue(())
	}

	/// Swap the top stack item with the Nth item from the top
	/// Returns Continue(()) if successful, Break(Halt::StackUnderflow) if indices are invalid
	pub fn exchange(&mut self, i: usize, j: usize) -> ControlFlow<Halt> {
		let len = self.0.len();
		if i >= len || j >= len {
			return ControlFlow::Break(Halt::StackUnderflow);
		}

		let i_idx = len - 1 - i;
		let j_idx = len - 1 - j;
		self.0.swap(i_idx, j_idx);
		ControlFlow::Continue(())
	}
}

impl Default for Stack {
	fn default() -> Self {
		Self::new()
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn test_push_pop() {
		let mut stack = Stack::new();

		// Test push
		assert!(matches!(stack.push(U256::from(42)), ControlFlow::Continue(())));
		assert_eq!(stack.len(), 1);

		// Test pop
		assert_eq!(stack.popn::<1>(), ControlFlow::Continue([U256::from(42)]));
		assert_eq!(stack.len(), 0);
		assert_eq!(stack.popn::<1>(), ControlFlow::Break(Halt::StackUnderflow));
	}

	#[test]
	fn test_popn() {
		let mut stack = Stack::new();

		// Push some values
		for i in 1..=3 {
			assert!(stack.push(U256::from(i)).is_continue());
		}

		// Pop multiple values
		let result: ControlFlow<_, [U256; 2]> = stack.popn();
		assert_eq!(result, ControlFlow::Continue([U256::from(3), U256::from(2)]));
		assert_eq!(stack.len(), 1);

		// Try to pop more than available
		let result: ControlFlow<_, [U256; 2]> = stack.popn();
		assert_eq!(result, ControlFlow::Break(Halt::StackUnderflow));
	}

	#[test]
	fn test_popn_top() {
		let mut stack = Stack::new();

		// Push some values
		for i in 1..=4 {
			assert!(stack.push(U256::from(i)).is_continue());
		}

		// Pop 2 values and get mutable reference to new top
		let result = stack.popn_top::<2>();
		assert!(matches!(result, ControlFlow::Continue(_)));
		let (popped, top_ref) = match result {
			ControlFlow::Continue(val) => val,
			ControlFlow::Break(_) => panic!("Expected Continue"),
		};
		assert_eq!(popped, [U256::from(4), U256::from(3)]);
		assert_eq!(*top_ref, U256::from(2));

		// Modify the top
		*top_ref = U256::from(99);
		assert_eq!(stack.top(), Some(&U256::from(99)));
	}

	#[test]
	fn test_dup() {
		let mut stack = Stack::new();

		let _ = stack.push(U256::from(1));
		let _ = stack.push(U256::from(2));

		// Duplicate the top item (index 1)
		assert!(matches!(stack.dup(1), ControlFlow::Continue(())));
		assert_eq!(stack.0, vec![U256::from(1), U256::from(2), U256::from(2)]);

		// Duplicate the second item (index 2)
		assert!(matches!(stack.dup(2), ControlFlow::Continue(())));
		assert_eq!(stack.0, vec![U256::from(1), U256::from(2), U256::from(2), U256::from(2)]);
	}

	#[test]
	fn test_exchange() {
		let mut stack = Stack::new();

		let _ = stack.push(U256::from(1));
		let _ = stack.push(U256::from(2));
		let _ = stack.push(U256::from(3));

		// Swap top (index 0) with second (index 1)
		assert!(matches!(stack.exchange(0, 1), ControlFlow::Continue(())));
		assert_eq!(stack.0, vec![U256::from(1), U256::from(3), U256::from(2)]);
	}

	#[test]
	fn test_stack_limit() {
		let mut stack = Stack::new();

		// Fill stack to limit
		for i in 0..1024 {
			assert!(matches!(stack.push(U256::from(i)), ControlFlow::Continue(())));
		}

		// Should fail to push one more
		assert_eq!(stack.push(U256::from(9999)), ControlFlow::Break(Halt::StackOverflow));
		assert_eq!(stack.len(), 1024);
	}

	#[test]
	fn test_top() {
		let mut stack = Stack::new();
		assert_eq!(stack.top(), None);

		let _ = stack.push(U256::from(42));
		assert_eq!(stack.top(), Some(&U256::from(42)));

		let _ = stack.push(U256::from(100));
		assert_eq!(stack.top(), Some(&U256::from(100)));
	}

	#[test]
	fn test_is_empty() {
		let mut stack = Stack::new();
		assert!(stack.is_empty());

		assert!(stack.push(U256::from(1)).is_continue());
		assert!(!stack.is_empty());

		assert!(stack.popn::<1>().is_continue());
		assert!(stack.is_empty());
	}
}
