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

//! Extension for the default recorder.

use crate::RawStorageProof;
use alloc::{collections::BTreeSet, vec::Vec};
use trie_db::{Recorder, TrieLayout};

/// Convenience extension for the `Recorder` struct.
///
/// Used to deduplicate some logic.
pub trait RecorderExt<L: TrieLayout>
where
	Self: Sized,
{
	/// Convert the recorder into a `BTreeSet`.
	fn into_set(self) -> BTreeSet<Vec<u8>>;

	/// Convert the recorder into a `RawStorageProof`, avoiding duplicate nodes.
	fn into_raw_storage_proof(self) -> RawStorageProof {
		// The recorder may record the same trie node multiple times,
		// and we don't want duplicate nodes in our proofs
		// => let's deduplicate it by collecting to a BTreeSet first
		self.into_set().into_iter().collect()
	}
}

impl<L: TrieLayout> RecorderExt<L> for Recorder<L> {
	fn into_set(mut self) -> BTreeSet<Vec<u8>> {
		self.drain().into_iter().map(|record| record.data).collect::<BTreeSet<_>>()
	}
}
