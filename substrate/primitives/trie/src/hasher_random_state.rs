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
	hash::Hasher as CoreHasher,
	sync::atomic::{AtomicUsize, Ordering},
};

use core::hash::BuildHasher;
use hashbrown::DefaultHashBuilder;

// Extra randomness to be used besides the one provided by the `DefaultHashBuilder`.
static EXTRA_RANDOMNESS: AtomicUsize = AtomicUsize::new(0x082efa98);

/// Adds extra randomness to be used by all new instances of RandomState.
pub fn add_extra_randomness(extra_randomness: usize) {
	EXTRA_RANDOMNESS.store(extra_randomness, Ordering::Relaxed);
}

/// A wrapper around `DefaultHashBuilder` that adds extra randomness to the hashers it creates.
#[derive(Copy, Clone, Debug)]
pub struct RandomState {
	default: DefaultHashBuilder,
	extra_randomness: usize,
}

impl Default for RandomState {
	#[inline(always)]
	fn default() -> Self {
		RandomState {
			// DefaultHashBuild already uses a random seed, so we use that as the base.
			default: DefaultHashBuilder::default(),
			extra_randomness: EXTRA_RANDOMNESS.load(Ordering::Relaxed),
		}
	}
}

impl BuildHasher for RandomState {
	type Hasher = <DefaultHashBuilder as BuildHasher>::Hasher;

	#[inline(always)]
	fn build_hasher(&self) -> Self::Hasher {
		let mut hasher = self.default.build_hasher();
		hasher.write_usize(self.extra_randomness);

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

		super::add_extra_randomness(12345678);
		let builder_after_randomness_added = super::RandomState::default();
		assert_eq!(builder_after_randomness_added.extra_randomness, 12345678);

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
		super::add_extra_randomness(12345678);

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
