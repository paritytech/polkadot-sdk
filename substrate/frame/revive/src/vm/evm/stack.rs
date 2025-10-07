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

use crate::{limits::EVM_STACK_LIMIT, vm::evm::interpreter::Halt, Config, Error};
use alloc::vec::Vec;
use core::ops::ControlFlow;
use sp_core::{H160, H256, U256};

/// EVM stack implementation using sp_core types
#[derive(Debug, Clone)]
pub struct Stack<T: Config> {
	stack: Vec<U256>,
	_phantom: core::marker::PhantomData<T>,
}

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

impl<T: Config> Stack<T> {
	/// Create a new empty stack
	pub fn new() -> Self {
		Self { stack: Vec::with_capacity(32), _phantom: core::marker::PhantomData }
	}

	/// Push a value onto the stack
	pub fn push(&mut self, value: impl ToU256) -> ControlFlow<Halt> {
		if self.stack.len() >= (EVM_STACK_LIMIT as usize) {
			ControlFlow::Break(Error::<T>::StackOverflow.into())
		} else {
			self.stack.push(value.to_u256());
			ControlFlow::Continue(())
		}
	}

	/// Get a reference to the top stack item without removing it
	#[cfg(test)]
	pub fn top(&self) -> Option<&U256> {
		self.stack.last()
	}

	/// Get the current stack size
	pub fn len(&self) -> usize {
		self.stack.len()
	}

	/// Check if stack is empty
	#[cfg(test)]
	pub fn is_empty(&self) -> bool {
		self.stack.is_empty()
	}

	/// Pop multiple values from the stack
	pub fn popn<const N: usize>(&mut self) -> ControlFlow<Halt, [U256; N]> {
		if self.stack.len() < N {
			return ControlFlow::Break(Error::<T>::StackUnderflow.into());
		}

		let mut result: [U256; N] = [U256::zero(); N];
		for i in 0..N {
			match self.stack.pop() {
				Some(value) => result[i] = value,
				None => return ControlFlow::Break(Error::<T>::StackUnderflow.into()),
			}
		}
		ControlFlow::Continue(result)
	}

	/// Pop multiple values and return them along with a mutable reference to the new top
	/// This is used for operations that pop some values and modify the top of the stack
	pub fn popn_top<const N: usize>(&mut self) -> ControlFlow<Halt, ([U256; N], &mut U256)> {
		if self.stack.len() < N + 1 {
			return ControlFlow::Break(Error::<T>::StackUnderflow.into());
		}

		let mut popped: [U256; N] = [U256::zero(); N];
		for i in 0..N {
			match self.stack.pop() {
				Some(value) => popped[i] = value,
				None => return ControlFlow::Break(Error::<T>::StackUnderflow.into()),
			}
		}

		// Get mutable reference to the new top
		match self.stack.last_mut() {
			Some(top) => ControlFlow::Continue((popped, top)),
			None => ControlFlow::Break(Error::<T>::StackUnderflow.into()),
		}
	}

	/// Duplicate the Nth item from the top and push it onto the stack
	pub fn dup(&mut self, n: usize) -> ControlFlow<Halt> {
		if n == 0 || n > self.stack.len() {
			return ControlFlow::Break(Error::<T>::StackUnderflow.into());
		}
		if self.stack.len() >= (EVM_STACK_LIMIT as usize) {
			return ControlFlow::Break(Error::<T>::StackOverflow.into());
		}

		let idx = self.stack.len() - n;
		let value = self.stack[idx];
		self.stack.push(value);
		ControlFlow::Continue(())
	}

	/// Swap the top stack item with the Nth item from the top
	pub fn exchange(&mut self, i: usize, j: usize) -> ControlFlow<Halt> {
		let len = self.stack.len();
		if i >= len || j >= len {
			return ControlFlow::Break(Error::<T>::StackUnderflow.into());
		}

		let i_idx = len - 1 - i;
		let j_idx = len - 1 - j;
		self.stack.swap(i_idx, j_idx);
		ControlFlow::Continue(())
	}

	/// Pushes a slice of bytes onto the stack, padding the last word with zeros
	/// if necessary.
	///
	/// # Panics
	///
	/// Panics if slice is longer than 32 bytes.
	pub fn push_slice(&mut self, slice: &[u8]) -> ControlFlow<Halt> {
		debug_assert!(slice.len() <= 32, "slice must be at most 32 bytes");
		if slice.is_empty() {
			return ControlFlow::Continue(());
		}

		if self.stack.len() >= (EVM_STACK_LIMIT as usize) {
			return ControlFlow::Break(Error::<T>::StackOverflow.into());
		}

		let mut word_bytes = [0u8; 32];
		let offset = 32 - slice.len();
		word_bytes[offset..].copy_from_slice(slice);

		self.stack.push(U256::from_big_endian(&word_bytes));
		return ControlFlow::Continue(());
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::tests::Test;

	#[test]
	fn test_push_slice() {
		// No-op
		let mut stack = Stack::<Test>::new();
		assert!(stack.push_slice(b"").is_continue());
		assert!(stack.is_empty());

		// Single byte
		let mut stack = Stack::<Test>::new();
		assert!(stack.push_slice(&[42]).is_continue());
		assert_eq!(stack.stack, vec![U256::from(42)]);

		// 16-byte value (128-bit)
		let n = 0x1111_2222_3333_4444_5555_6666_7777_8888_u128;
		let mut stack = Stack::<Test>::new();
		assert!(stack.push_slice(&n.to_be_bytes()).is_continue());
		assert_eq!(stack.stack, vec![U256::from(n)]);

		// Full 32-byte value
		let mut stack = Stack::<Test>::new();
		let bytes_32 = [42u8; 32];
		assert!(stack.push_slice(&bytes_32).is_continue());
		assert_eq!(stack.stack, vec![U256::from_big_endian(&bytes_32)]);
	}

	#[test]
	fn test_push_pop() {
		let mut stack = Stack::<Test>::new();

		// Test push
		assert!(matches!(stack.push(U256::from(42)), ControlFlow::Continue(())));
		assert_eq!(stack.len(), 1);

		// Test pop
		assert_eq!(stack.popn::<1>(), ControlFlow::Continue([U256::from(42)]));
		assert_eq!(stack.len(), 0);
		assert_eq!(stack.popn::<1>(), ControlFlow::Break(Error::<Test>::StackUnderflow.into()));
	}

	#[test]
	fn test_popn() {
		let mut stack = Stack::<Test>::new();

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
		assert_eq!(result, ControlFlow::Break(Error::<Test>::StackUnderflow.into()));
	}

	#[test]
	fn test_popn_top() {
		let mut stack = Stack::<Test>::new();

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
		let mut stack = Stack::<Test>::new();

		let _ = stack.push(U256::from(1));
		let _ = stack.push(U256::from(2));

		// Duplicate the top item (index 1)
		assert!(matches!(stack.dup(1), ControlFlow::Continue(())));
		assert_eq!(stack.stack, vec![U256::from(1), U256::from(2), U256::from(2)]);

		// Duplicate the second item (index 2)
		assert!(matches!(stack.dup(2), ControlFlow::Continue(())));
		assert_eq!(stack.stack, vec![U256::from(1), U256::from(2), U256::from(2), U256::from(2)]);
	}

	#[test]
	fn test_exchange() {
		let mut stack = Stack::<Test>::new();

		let _ = stack.push(U256::from(1));
		let _ = stack.push(U256::from(2));
		let _ = stack.push(U256::from(3));

		// Swap top (index 0) with second (index 1)
		assert!(matches!(stack.exchange(0, 1), ControlFlow::Continue(())));
		assert_eq!(stack.stack, vec![U256::from(1), U256::from(3), U256::from(2)]);
	}

	#[test]
	fn test_stack_limit() {
		let mut stack = Stack::<Test>::new();

		// Fill stack to limit
		for i in 0..EVM_STACK_LIMIT {
			assert!(matches!(stack.push(U256::from(i)), ControlFlow::Continue(())));
		}

		// Should fail to push one more
		assert_eq!(
			stack.push(U256::from(9999)),
			ControlFlow::Break(Error::<Test>::StackOverflow.into())
		);
		assert_eq!(stack.len(), EVM_STACK_LIMIT as usize);
	}

	#[test]
	fn test_top() {
		let mut stack = Stack::<Test>::new();
		assert_eq!(stack.top(), None);

		let _ = stack.push(U256::from(42));
		assert_eq!(stack.top(), Some(&U256::from(42)));

		let _ = stack.push(U256::from(100));
		assert_eq!(stack.top(), Some(&U256::from(100)));
	}

	#[test]
	fn test_is_empty() {
		let mut stack = Stack::<Test>::new();
		assert!(stack.is_empty());

		assert!(stack.push(U256::from(1)).is_continue());
		assert!(!stack.is_empty());

		assert!(stack.popn::<1>().is_continue());
		assert!(stack.is_empty());
	}
}
