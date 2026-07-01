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

//! Utility module to use a custom random state for HashMap and friends
//! in a no_std environment.

use core::{
	cell::UnsafeCell,
	hash::Hasher as CoreHasher,
	sync::atomic::{AtomicU8, Ordering},
};

use core::hash::BuildHasher;
use foldhash::quality::RandomState as FoldHashBuilder;

// Constants to represent the state of the global extra randomness.
// UNINITIALIZED: The extra randomness has not been set yet.
const UNINITIALIZED: u8 = 0;
// LOCKED: The extra randomness is being set.
const LOCKED: u8 = 1;
// INITIALIZED: The extra randomness has been set and is ready to use.
const INITIALIZED: u8 = 2;

// SAFETY: we only mutate the UnsafeCells when state is in the thread-exclusive
// LOCKED state, and only read when state is in the INITIALIZED state.
unsafe impl Sync for GlobalExtraRandomnesss {}
struct GlobalExtraRandomnesss {
	initialized: AtomicU8,
	randomness: UnsafeCell<[u8; 16]>,
}

// Extra randomness to be used besides the one provided by the `FoldHashBuilder`.
static EXTRA_RANDOMNESS: GlobalExtraRandomnesss = GlobalExtraRandomnesss {
	initialized: AtomicU8::new(UNINITIALIZED),
	randomness: UnsafeCell::new([0u8; 16]),
};

/// Adds extra randomness to be used by all new instances of RandomState.
pub fn add_extra_randomness(extra_randomness: [u8; 16]) {
	match EXTRA_RANDOMNESS.initialized.compare_exchange(
		UNINITIALIZED,
		LOCKED,
		Ordering::Acquire,
		Ordering::Acquire,
	) {
		Ok(_) => {
			// SAFETY: We are the only ones writing exclusively to this memory.
			unsafe { *EXTRA_RANDOMNESS.randomness.get() = extra_randomness };
			EXTRA_RANDOMNESS.initialized.store(INITIALIZED, Ordering::Release);
		},
		Err(_) => {
			panic!("Extra randomness has already been set, cannot set it again.");
		},
	}
}

// Returns the extra randomness if it has been set, otherwise returns None.
fn extra_randomness() -> Option<&'static [u8; 16]> {
	// SAFETY: We are reading from a static memory location that is initialized
	// only once, so it is safe to read from it.
	if EXTRA_RANDOMNESS.initialized.load(Ordering::Acquire) == INITIALIZED {
		Some(unsafe { &*EXTRA_RANDOMNESS.randomness.get() })
	} else {
		None
	}
}

/// A wrapper around `FoldHashBuilder` that adds extra randomness to the hashers it creates.
#[derive(Copy, Clone, Debug)]
pub struct RandomState {
	default: FoldHashBuilder,
	extra_randomness: Option<&'static [u8; 16]>,
}

impl Default for RandomState {
	#[inline(always)]
	fn default() -> Self {
		RandomState {
			// FoldHashBuilder already uses a random seed, so we use that as the base.
			default: FoldHashBuilder::default(),
			extra_randomness: extra_randomness(),
		}
	}
}

impl BuildHasher for RandomState {
	type Hasher = <FoldHashBuilder as BuildHasher>::Hasher;

	#[inline(always)]
	fn build_hasher(&self) -> Self::Hasher {
		let mut hasher = self.default.build_hasher();
		if let Some(extra) = self.extra_randomness {
			// If extra randomness is set, we write it into the hasher.
			hasher.write(extra);
		}

		hasher
	}
}

#[cfg(test)]
mod tests {
	use core::hash::{BuildHasher, Hasher};

	#[test]
	fn hashbuilder_produces_same_result() {
		let haser_builder = super::RandomState::default();
		let mut hasher_1 = haser_builder.build_hasher();
		let mut hasher_2 = haser_builder.build_hasher();

		hasher_1.write_u32(8128);
		hasher_2.write_u32(8128);

		assert_eq!(hasher_1.finish(), hasher_2.finish());
	}

	#[test]
	fn adding_randomness_does_not_affect_already_instantiated_builders() {
		let hasher_builder = super::RandomState::default();
		let mut hasher_1 = hasher_builder.build_hasher();

		let randomness = [0xde; 16];
		super::add_extra_randomness(randomness);
		let builder_after_randomness_added = super::RandomState::default();
		assert_eq!(builder_after_randomness_added.extra_randomness, Some(&randomness));

		let mut hasher_2 = hasher_builder.build_hasher();

		hasher_1.write_u32(8128);
		hasher_2.write_u32(8128);

		assert_eq!(hasher_1.finish(), hasher_2.finish());
	}

	#[test]
	fn sanity_check() {
		let haser_builder = super::RandomState::default();
		let mut hasher_create_manually =
			hashbrown::HashMap::<u32, u32, _>::with_hasher(haser_builder);
		let mut default_built = hashbrown::HashMap::<u32, u32, super::RandomState>::default();

		for x in 0..100 {
			default_built.insert(x, x * 2);
			hasher_create_manually.insert(x, x * 2);
		}

		for x in 0..100 {
			assert_eq!(default_built.get(&x), Some(&(x * 2)));
			assert_eq!(hasher_create_manually.get(&x), Some(&(x * 2)));
		}

		for x in 100..200 {
			assert_eq!(default_built.get(&x), None);
			assert_eq!(hasher_create_manually.get(&x), None);
		}
	}
}
