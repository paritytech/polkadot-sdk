// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <http://www.gnu.org/licenses/>.

//! The actual implementation of the validate block functionality.
#![cfg_attr(not(feature = "std"), no_std)]

use codec::Encode;

use sp_std::{
	cell::{RefCell, RefMut},
	collections::{btree_map::BTreeMap, btree_set::BTreeSet},
	sync::Arc,
};
use sp_trie::{NodeCodec, StorageProof};
use trie_db::{Hasher, RecordedForKey, TrieAccess};

/// A trie recorder that only keeps track of the proof size.
pub(crate) struct SizeRecorder<'a, H: Hasher> {
	seen_nodes: RefMut<'a, BTreeSet<H::Out>>,
	encoded_size: RefMut<'a, usize>,
	recorded_keys: RefMut<'a, BTreeMap<Arc<[u8]>, RecordedForKey>>,
}

impl<'a, H: trie_db::Hasher> trie_db::TrieRecorder<H::Out> for SizeRecorder<'a, H> {
	fn record<'b>(&mut self, access: TrieAccess<'b, H::Out>) {
		let mut encoded_size_update = 0;
		match access {
			TrieAccess::NodeOwned { hash, node_owned } =>
				if !self.seen_nodes.get(&hash).is_some() {
					let node = node_owned.to_encoded::<NodeCodec<H>>();
					log::info!(target: "skunert", "TrieAccess::NodeOwned");
					encoded_size_update += node.encoded_size();
					self.seen_nodes.insert(hash);
				},
			TrieAccess::EncodedNode { hash, encoded_node } => {
				if !self.seen_nodes.get(&hash).is_some() {
					let node = encoded_node.into_owned();

					log::info!(target: "skunert", "TrieAccess::EncodedNode");
					encoded_size_update += node.encoded_size();
					self.seen_nodes.insert(hash);
				}
			},
			TrieAccess::Value { hash, value, full_key } => {
				if !self.seen_nodes.get(&hash).is_some() {
					let value = value.into_owned();
					log::info!(target: "skunert", "TrieAccess::Value");
					encoded_size_update += value.encoded_size();
					self.seen_nodes.insert(hash);
				}
				self.recorded_keys
					.entry(full_key.into())
					.and_modify(|e| *e = RecordedForKey::Value)
					.or_insert_with(|| RecordedForKey::Value);
			},
			TrieAccess::Hash { full_key } => {
				self.recorded_keys
					.entry(full_key.into())
					.or_insert_with(|| RecordedForKey::Hash);
			},
			TrieAccess::NonExisting { full_key } => {
				self.recorded_keys
					.entry(full_key.into())
					.and_modify(|e| *e = RecordedForKey::Value)
					.or_insert_with(|| RecordedForKey::Value);
			},
		};

		*self.encoded_size += encoded_size_update;
	}

	fn trie_nodes_recorded_for_key(&self, key: &[u8]) -> RecordedForKey {
		self.recorded_keys.get(key).copied().unwrap_or(RecordedForKey::None)
	}
}

pub(crate) struct RecorderProvider<H: Hasher> {
	seen_nodes: RefCell<BTreeSet<H::Out>>,
	encoded_size: RefCell<usize>,
	recorded_keys: RefCell<BTreeMap<Arc<[u8]>, RecordedForKey>>,
}

impl<H: Hasher> RecorderProvider<H> {
	pub fn new() -> Self {
		Self {
			seen_nodes: Default::default(),
			encoded_size: Default::default(),
			recorded_keys: Default::default(),
		}
	}
}

impl<H: trie_db::Hasher> sp_trie::TrieRecorderProvider<H> for RecorderProvider<H> {
	type Recorder<'a> = SizeRecorder<'a, H> where H: 'a;

	fn drain_storage_proof(self) -> StorageProof {
		unimplemented!("Draining storage proof not supported!")
	}

	fn as_trie_recorder(&self, _storage_root: H::Out) -> Self::Recorder<'_> {
		SizeRecorder {
			encoded_size: self.encoded_size.borrow_mut(),
			seen_nodes: self.seen_nodes.borrow_mut(),
			recorded_keys: self.recorded_keys.borrow_mut(),
		}
	}

	fn estimate_encoded_size(&self) -> usize {
		*self.encoded_size.borrow()
	}
}

// This is safe here since we are single-threaded in WASM
unsafe impl<H: Hasher> Send for RecorderProvider<H> {}
unsafe impl<H: Hasher> Sync for RecorderProvider<H> {}
