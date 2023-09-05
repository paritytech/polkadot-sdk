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

use super::trie_cache;

use codec::Encode;

use core::borrow::BorrowMut;
use frame_support::traits::Get;
use sp_externalities::{set_and_run_with_externalities, Externalities};
use sp_io::KillStorageResult;
use sp_runtime::traits::{Block as BlockT, Extrinsic, HashingFor, Header as HeaderT};
use sp_std::{
	boxed::Box,
	cell::{RefCell, RefMut},
	collections::btree_set::BTreeSet,
	prelude::*,
};
use sp_trie::{MemoryDB, NodeCodec, StorageProof};
use trie_db::{Hasher, RecordedForKey, TrieAccess};

/// A trie recorder that only keeps track of the proof size.
pub(crate) struct SizeRecorder<'a, H: Hasher> {
	seen_nodes: RefMut<'a, BTreeSet<H::Out>>,
	encoded_size: RefMut<'a, usize>,
}

impl<'a, H: trie_db::Hasher> trie_db::TrieRecorder<H::Out> for SizeRecorder<'a, H> {
	fn record<'b>(&mut self, access: TrieAccess<'b, H::Out>) {
		let mut encoded_size_update = 0;
		match access {
			TrieAccess::NodeOwned { hash, node_owned } =>
				if !self.seen_nodes.get(&hash).is_some() {
					let node = node_owned.to_encoded::<NodeCodec<H>>();
					encoded_size_update += node.encoded_size();
					self.seen_nodes.insert(hash);
				},
			TrieAccess::EncodedNode { hash, encoded_node } => {
				if !self.seen_nodes.get(&hash).is_some() {
					let node = encoded_node.into_owned();
					encoded_size_update += node.encoded_size();
					self.seen_nodes.insert(hash);
				}
			},
			TrieAccess::Value { hash, value, .. } =>
				if !self.seen_nodes.get(&hash).is_some() {
					let value = value.into_owned();

					encoded_size_update += value.encoded_size();
					self.seen_nodes.insert(hash);
				},
			TrieAccess::Hash { .. } | TrieAccess::NonExisting { .. } => {},
		};

		*self.encoded_size += encoded_size_update;
	}

	fn trie_nodes_recorded_for_key(&self, key: &[u8]) -> RecordedForKey {
		RecordedForKey::None
	}
}

pub(crate) struct RecorderProvider<H: Hasher> {
	seen_nodes: RefCell<BTreeSet<H::Out>>,
	encoded_size: RefCell<usize>,
}

impl<H: Hasher> RecorderProvider<H> {
	pub fn new() -> Self {
		Self { seen_nodes: Default::default(), encoded_size: Default::default() }
	}
}

impl<H: trie_db::Hasher> sp_trie::TrieRecorderProvider<H> for RecorderProvider<H> {
	type Recorder<'a> = SizeRecorder<'a, H> where H: 'a;

	fn drain_storage_proof(self) -> StorageProof {
		panic!("Tried to drain storage proof")
	}

	fn as_trie_recorder(&self, storage_root: H::Out) -> Self::Recorder<'_> {
		log::info!(target: "skunert", "validate_block: as_trie_recorder");
		SizeRecorder {
			encoded_size: self.encoded_size.borrow_mut(),
			seen_nodes: self.seen_nodes.borrow_mut(),
		}
	}

	fn estimate_encoded_size(&self) -> usize {
		log::info!(target: "skunert", "validate_block: estimate_encoded_size");
		*self.encoded_size.borrow()
	}
}

// This is safe here since we are single-threaded in WASM
unsafe impl<H: Hasher> Send for RecorderProvider<H> {}
unsafe impl<H: Hasher> Sync for RecorderProvider<H> {}
