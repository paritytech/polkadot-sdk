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

//! Utils for keeping track of the heap memory allocated for decoding structures within nested
//! contexts during runtime code execution.
//!
//! For example an object that is double-encoded within an extrinsic would trigger such a scenario.
//! In this case the object will be decoded once together with the extrinsic and at that point its
//! heap memory use will be accounted for by the extrinsic decoding logic. Then it will be decoded
//! once more while executing the extrinsic, and we will need to separately keep track of the heap
//! memory used during this step as well.
//!
//! Another example would be a double-encoded object that we read from the storage while executing
//! a hook (e.g. `on_idle()`).
//!
//! There are also cases where there can be multiple nested double-encoded layers.

use crate::generic::DEFAULT_CALL_SIZE_LIMIT;
use codec::{Decode, Error as CodecError, Input, MemTrackingInput};

// Global variable for keeping track of the heap memory allocated for decoding objects within
// nested contexts.
environmental::environmental!(limiter: usize);

/// Get the current value stored in the `limiter` global variable.
pub fn get_current_limit() -> Option<usize> {
	limiter::with(|mem_limit| *mem_limit)
}

/// Initialize the `limiter` global variable.
///
/// Any runtime logic that may lead to decoding double encoded objects while executing runtime code
/// should be called within this context.
pub fn using_limiter_once<F, R>(f: F) -> R
where
	F: FnOnce() -> R,
{
	let mut mem_limit = DEFAULT_CALL_SIZE_LIMIT;
	limiter::using_once(&mut mem_limit, f)
}

/// Helper struct used to update the `limiter` when some of the tracked memory is deallocated.
pub struct DeallocationReminder {
	mem_size: usize,
}

impl DeallocationReminder {
	/// Create a new instance of `DeallocationReminder`.
	pub fn new(mem_size: usize) -> Self {
		Self { mem_size }
	}
}

impl Drop for DeallocationReminder {
	fn drop(&mut self) {
		let _ = limiter::with(|mem_limit| {
			*mem_limit = mem_limit.saturating_add(self.mem_size);
			*mem_limit = core::cmp::min(*mem_limit, DEFAULT_CALL_SIZE_LIMIT);
		});
	}
}

/// Helper function used to decode an object within a nested context.
///
/// Apart from decoding the object, this method also returns a `DeallocationReminder` in order to
/// keep track of the heap memory that was allocated in the nested context.
pub fn decode_with_limiter<
	I: Input,
	T: Decode,
	F: FnOnce(&mut MemTrackingInput<I>) -> Result<T, CodecError> + Clone,
>(
	input: &mut I,
	decode_fn: F,
) -> Result<(T, Option<DeallocationReminder>), CodecError> {
	limiter::with(|mem_limit| {
		let mut mem_tracking_input = MemTrackingInput::new(input, *mem_limit);
		let decoded = decode_fn.clone()(&mut mem_tracking_input)?;
		let used_mem = mem_tracking_input.used_mem();
		*mem_limit = mem_limit.saturating_sub(used_mem);
		Ok((decoded, Some(DeallocationReminder::new(used_mem))))
	})
	.unwrap_or_else(|| {
		let mut mem_tracking_input = MemTrackingInput::new(input, DEFAULT_CALL_SIZE_LIMIT);
		Ok((decode_fn(&mut mem_tracking_input)?, None))
	})
}
