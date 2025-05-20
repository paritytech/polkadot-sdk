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

//! Produce opaque sequential IDs.

/// A Sequence of IDs.
#[derive(Debug, Default)]
// The `Clone` trait is intentionally not defined on this type.
pub struct IDSequence {
	next_id: u64,
}

/// A Sequential ID.
///
/// Its integer value is intentionally not public: it is supposed to be instantiated from within
/// this module only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct SeqID(u64);

impl std::fmt::Display for SeqID {
	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
		write!(f, "{}", self.0)
	}
}

impl IDSequence {
	/// Create a new ID-sequence.
	pub fn new() -> Self {
		Default::default()
	}

	/// Obtain another ID from this sequence.
	pub fn next_id(&mut self) -> SeqID {
		let id = SeqID(self.next_id);
		self.next_id += 1;

		id
	}
}
