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

use crate::vm::evm::interpreter::HaltReason;
use alloc::vec::Vec;
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
	/// Returns Ok(()) if successful, Err(HaltReason::StackOverflow) if stack would overflow
	pub fn push(&mut self, value: impl ToU256) -> Result<(), HaltReason> {
		if self.0.len() >= 1024 {
			Err(HaltReason::StackOverflow)
		} else {
			self.0.push(value.to_u256());
			Ok(())
		}
	}

	/// Pop a value from the stack
	/// Returns Ok(value) if successful, Err(HaltReason::StackUnderflow) if stack is empty
	pub fn pop(&mut self) -> Result<U256, HaltReason> {
		self.0.pop().ok_or(HaltReason::StackUnderflow)
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
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}

	/// Pop multiple values from the stack
	/// Returns Ok(array) if successful, Err(HaltReason::StackUnderflow) if not enough values on
	/// stack
	pub fn popn<const N: usize>(&mut self) -> Result<[U256; N], HaltReason> {
		if self.0.len() < N {
			return Err(HaltReason::StackUnderflow);
		}

		let mut result: [U256; N] = [U256::zero(); N];
		for i in 0..N {
			result[i] = self.0.pop().ok_or(HaltReason::StackUnderflow)?;
		}
		Ok(result)
	}

	/// Pop multiple values and return them along with a mutable reference to the new top
	/// This is used for operations that pop some values and modify the top of the stack
	pub fn popn_top<const N: usize>(&mut self) -> Result<([U256; N], &mut U256), HaltReason> {
		if self.0.len() < N + 1 {
			return Err(HaltReason::StackUnderflow);
		}

		let mut popped: [U256; N] = [U256::zero(); N];
		for i in 0..N {
			popped[i] = self.0.pop().ok_or(HaltReason::StackUnderflow)?;
		}

		// Get mutable reference to the new top
		let top = self.0.last_mut().ok_or(HaltReason::StackUnderflow)?;
		Ok((popped, top))
	}

	/// Duplicate the Nth item from the top and push it onto the stack
	/// Returns Ok(()) if successful, Err(HaltReason) if stack would overflow or index is invalid
	pub fn dup(&mut self, n: usize) -> Result<(), HaltReason> {
		if n == 0 || n > self.0.len() {
			return Err(HaltReason::StackUnderflow);
		}
		if self.0.len() >= 1024 {
			return Err(HaltReason::StackOverflow);
		}

		let idx = self.0.len() - n;
		let value = self.0[idx];
		self.0.push(value);
		Ok(())
	}

	/// Swap the top stack item with the Nth item from the top
	/// Returns Ok(()) if successful, Err(HaltReason::StackUnderflow) if indices are invalid
	pub fn exchange(&mut self, i: usize, j: usize) -> Result<(), HaltReason> {
		let len = self.0.len();
		if i >= len || j >= len {
			return Err(HaltReason::StackUnderflow);
		}

		let i_idx = len - 1 - i;
		let j_idx = len - 1 - j;
		self.0.swap(i_idx, j_idx);
		Ok(())
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
		assert!(stack.push(U256::from(42)).is_ok());
		assert_eq!(stack.len(), 1);

		// Test pop
		assert_eq!(stack.pop(), Ok(U256::from(42)));
		assert_eq!(stack.len(), 0);
		assert_eq!(stack.pop(), Err(HaltReason::StackUnderflow));
	}

	#[test]
	fn test_popn() {
		let mut stack = Stack::new();

		// Push some values
		stack.push(U256::from(1));
		stack.push(U256::from(2));
		stack.push(U256::from(3));

		// Pop multiple values
		let result: Result<[U256; 2], _> = stack.popn();
		assert_eq!(result, Ok([U256::from(3), U256::from(2)]));
		assert_eq!(stack.len(), 1);

		// Try to pop more than available
		let result: Result<[U256; 2], _> = stack.popn();
		assert_eq!(result, Err(HaltReason::StackUnderflow));
	}

	#[test]
	fn test_popn_top() {
		let mut stack = Stack::new();

		// Push some values
		stack.push(U256::from(1));
		stack.push(U256::from(2));
		stack.push(U256::from(3));
		stack.push(U256::from(4));

		// Pop 2 values and get mutable reference to new top
		let result = stack.popn_top::<2>();
		assert!(result.is_ok());
		let (popped, top_ref) = result.unwrap();
		assert_eq!(popped, [U256::from(4), U256::from(3)]);
		assert_eq!(*top_ref, U256::from(2));

		// Modify the top
		*top_ref = U256::from(99);
		assert_eq!(stack.top(), Some(&U256::from(99)));
	}

	#[test]
	fn test_dup() {
		let mut stack = Stack::new();

		stack.push(U256::from(1)).unwrap();
		stack.push(U256::from(2)).unwrap();

		// Duplicate the top item (index 1)
		assert!(stack.dup(1).is_ok());
		assert_eq!(stack.0, vec![U256::from(1), U256::from(2), U256::from(2)]);

		// Duplicate the second item (index 2)
		assert!(stack.dup(2).is_ok());
		assert_eq!(stack.0, vec![U256::from(1), U256::from(2), U256::from(2), U256::from(2)]);
	}

	#[test]
	fn test_exchange() {
		let mut stack = Stack::new();

		stack.push(U256::from(1)).unwrap();
		stack.push(U256::from(2)).unwrap();
		stack.push(U256::from(3)).unwrap();

		// Swap top (index 0) with second (index 1)
		assert!(stack.exchange(0, 1).is_ok());
		assert_eq!(stack.0, vec![U256::from(1), U256::from(3), U256::from(2)]);
	}

	#[test]
	fn test_stack_limit() {
		let mut stack = Stack::new();

		// Fill stack to limit
		for i in 0..1024 {
			assert!(stack.push(U256::from(i)).is_ok());
		}

		// Should fail to push one more
		assert_eq!(stack.push(U256::from(9999)), Err(HaltReason::StackOverflow));
		assert_eq!(stack.len(), 1024);
	}

	#[test]
	fn test_top() {
		let mut stack = Stack::new();
		assert_eq!(stack.top(), None);

		stack.push(U256::from(42)).unwrap();
		assert_eq!(stack.top(), Some(&U256::from(42)));

		stack.push(U256::from(100)).unwrap();
		assert_eq!(stack.top(), Some(&U256::from(100)));
	}

	#[test]
	fn test_is_empty() {
		let mut stack = Stack::new();
		assert!(stack.is_empty());

		stack.push(U256::from(1)).unwrap();
		assert!(!stack.is_empty());

		stack.pop().unwrap();
		assert!(stack.is_empty());
	}
}
