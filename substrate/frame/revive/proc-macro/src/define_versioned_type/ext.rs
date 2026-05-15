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

use core::hash::Hash;

use indexmap::{map::Entry, IndexMap};

pub trait IndexMapExt<K, V> {
	/// Attempts to insert. Fails if a value is already at the provided key. Doesn't perform the
	/// insertion on failure.
	fn bounce_insert(&mut self, key: K, value: V) -> Result<(), BounceOutput<'_, V>>;

	/// Attempts to insert. Fails if there's no value at the provided key. Doesn't perform the
	/// insertion on failure.
	fn override_insert(&mut self, key: K, value: V) -> Result<(), V>;
}

impl<K: Hash + Eq, V> IndexMapExt<K, V> for IndexMap<K, V> {
	fn bounce_insert(&mut self, key: K, value: V) -> Result<(), BounceOutput<'_, V>> {
		match self.entry(key) {
			Entry::Occupied(entry) => Err(BounceOutput {
				existing_value: entry.into_mut(),
				attempted_insert_value: value,
			}),
			Entry::Vacant(entry) => {
				entry.insert(value);
				Ok(())
			},
		}
	}

	fn override_insert(&mut self, key: K, value: V) -> Result<(), V> {
		match self.entry(key) {
			Entry::Occupied(mut entry) => {
				entry.insert(value);
				Ok(())
			},
			Entry::Vacant(..) => Err(value),
		}
	}
}

pub struct BounceOutput<'a, V> {
	pub existing_value: &'a V,
	pub attempted_insert_value: V,
}
