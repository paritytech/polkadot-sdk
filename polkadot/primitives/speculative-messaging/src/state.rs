// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! State types for speculative cross-chain messaging.
//!
//! These types represent a parachain runtime's internal bookkeeping for
//! speculative messaging. They are **not** sent to the relay chain — they
//! are used only within parachain runtimes to track incoming and outgoing
//! message progress.

extern crate alloc;

use alloc::{collections::BTreeMap, vec::Vec};
use codec::{Decode, DecodeWithMemTracking, Encode};
use polkadot_parachain_primitives::primitives::Id as ParaId;
use scale_info::TypeInfo;
use sp_core::H256;

use crate::{
	commitments::ProvidesCommitment,
	merkle_tree::{DestinationMerkleTree, StoredMerkleTree},
};

/// Per-source tracking on the receiver side.
///
/// Tracks how many messages have been processed from a particular source
/// parachain, and the last provides root we built against.
///
/// `last_processed` is the **number** of messages processed (0 means none),
/// so the next position to process is equal to `last_processed`.
#[derive(
	Debug, Clone, Copy, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Default,
)]
pub struct SourceState {
	/// Number of messages processed from this source (0-indexed count).
	/// The next expected position equals this value.
	pub last_processed: u64,
	/// The source's provides root we last built against.
	pub last_seen_root: H256,
}

impl SourceState {
	/// Create a new [`SourceState`] with default values (0, zero hash).
	pub fn new() -> Self {
		Self::default()
	}

	/// Returns the next expected message position (0-indexed).
	///
	/// Since `last_processed` stores the count of processed messages,
	/// the next position is simply `last_processed`.
	pub fn next_expected_position(&self) -> u64 {
		self.last_processed
	}

	/// Advance the processed count by `count` messages and update the
	/// last seen root.
	pub fn advance(&mut self, count: u64, new_root: H256) {
		self.last_processed += count;
		self.last_seen_root = new_root;
	}
}

/// Receiver-side state tracking all incoming message sources.
///
/// Maintains a map from source [`ParaId`] to its [`SourceState`],
/// allowing the runtime to know how far along it is in processing
/// messages from each source.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Default)]
pub struct IncomingMessageState {
	/// Per-source tracking.
	pub per_source: BTreeMap<ParaId, SourceState>,
}

impl IncomingMessageState {
	/// Create a new empty [`IncomingMessageState`].
	pub fn new() -> Self {
		Self::default()
	}

	/// Returns the [`SourceState`] for the given source, or the default
	/// if no state has been recorded yet.
	pub fn get_source(&self, source: ParaId) -> SourceState {
		self.per_source.get(&source).copied().unwrap_or_default()
	}

	/// Returns a mutable reference to the [`SourceState`] for the given
	/// source, inserting a default entry if one does not already exist.
	pub fn get_source_mut(&mut self, source: ParaId) -> &mut SourceState {
		self.per_source.entry(source).or_insert_with(SourceState::default)
	}

	/// Convenience method: advance the given source's state by `count`
	/// messages and update its last seen root.
	pub fn advance_source(&mut self, source: ParaId, count: u64, new_root: H256) {
		self.get_source_mut(source).advance(count, new_root);
	}

	/// Returns a sorted list of all tracked source [`ParaId`]s.
	pub fn tracked_sources(&self) -> Vec<ParaId> {
		self.per_source.keys().copied().collect()
	}
}

/// Sender-side state tracking outgoing messages.
///
/// Maintains per-destination MMR roots and a top-level Merkle root
/// computed over all of them.
#[derive(Debug, Clone, PartialEq, Eq, Encode, Decode, DecodeWithMemTracking, TypeInfo, Default)]
pub struct OutgoingMessageState {
	/// Per-destination MMR roots.
	pub destination_roots: BTreeMap<ParaId, H256>,
	/// Current top-level Merkle root over all destination MMR roots.
	pub current_root: H256,
}

impl OutgoingMessageState {
	/// Create a new empty [`OutgoingMessageState`].
	pub fn new() -> Self {
		Self::default()
	}

	/// Update the MMR root for a particular destination.
	pub fn update_destination(&mut self, dest: ParaId, new_mmr_root: H256) {
		self.destination_roots.insert(dest, new_mmr_root);
	}

	/// Recompute `current_root` from `destination_roots` using
	/// [`DestinationMerkleTree::compute_root`].
	pub fn recompute_root(&mut self) {
		let entries: Vec<(ParaId, H256)> =
			self.destination_roots.iter().map(|(&k, &v)| (k, v)).collect();
		self.current_root = DestinationMerkleTree::compute_root(&entries);
	}

	/// Returns a [`ProvidesCommitment`] reflecting the current root.
	pub fn provides_commitment(&self) -> ProvidesCommitment {
		ProvidesCommitment { root: self.current_root }
	}

	/// Returns the number of tracked destinations.
	pub fn destination_count(&self) -> usize {
		self.destination_roots.len()
	}

	/// Returns the MMR root for a particular destination, if tracked.
	pub fn get_destination_root(&self, dest: ParaId) -> Option<H256> {
		self.destination_roots.get(&dest).copied()
	}

	/// Build a [`StoredMerkleTree`] from the current destination roots.
	///
	/// The returned tree supports O(log D) incremental updates via
	/// [`StoredMerkleTree::update`] and O(log D) proof generation via
	/// [`StoredMerkleTree::generate_proof`]. Use this when you expect
	/// multiple operations in the same block — mutate the tree, then call
	/// [`Self::sync_from_tree`] once at the end to persist.
	pub fn build_tree(&self) -> StoredMerkleTree {
		let entries: Vec<(ParaId, H256)> =
			self.destination_roots.iter().map(|(&k, &v)| (k, v)).collect();
		StoredMerkleTree::from_destinations(&entries)
	}

	/// Synchronise this state from a [`StoredMerkleTree`] that was mutated
	/// externally (e.g. via [`StoredMerkleTree::update`]).
	///
	/// Replaces `destination_roots` and `current_root` with the tree's
	/// contents.
	pub fn sync_from_tree(&mut self, tree: &StoredMerkleTree) {
		self.destination_roots.clear();
		for &(id, root) in tree.destinations() {
			self.destination_roots.insert(id, root);
		}
		self.current_root = tree.root();
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	fn make_hash(byte: u8) -> H256 {
		H256::from([byte; 32])
	}

	#[test]
	fn source_state_new_defaults() {
		let state = SourceState::new();
		assert_eq!(state.last_processed, 0);
		assert_eq!(state.last_seen_root, H256::zero());
	}

	#[test]
	fn source_state_next_expected_position() {
		let mut state = SourceState::new();
		assert_eq!(state.next_expected_position(), 0);

		state.advance(3, make_hash(1));
		assert_eq!(state.next_expected_position(), 3);
	}

	#[test]
	fn source_state_advance() {
		let mut state = SourceState::new();
		let root_a = make_hash(10);
		let root_b = make_hash(20);

		state.advance(5, root_a);
		assert_eq!(state.last_processed, 5);
		assert_eq!(state.last_seen_root, root_a);

		state.advance(3, root_b);
		assert_eq!(state.last_processed, 8);
		assert_eq!(state.last_seen_root, root_b);
	}

	#[test]
	fn incoming_state_get_source_default() {
		let state = IncomingMessageState::new();
		let source = ParaId::from(1000);
		let source_state = state.get_source(source);
		assert_eq!(source_state, SourceState::default());
	}

	#[test]
	fn incoming_state_advance_source() {
		let mut state = IncomingMessageState::new();
		let source = ParaId::from(2000);
		let root = make_hash(42);

		state.advance_source(source, 7, root);

		let source_state = state.get_source(source);
		assert_eq!(source_state.last_processed, 7);
		assert_eq!(source_state.last_seen_root, root);
	}

	#[test]
	fn incoming_state_tracked_sources() {
		let mut state = IncomingMessageState::new();

		state.advance_source(ParaId::from(300), 1, make_hash(1));
		state.advance_source(ParaId::from(100), 1, make_hash(2));
		state.advance_source(ParaId::from(200), 1, make_hash(3));

		let sources = state.tracked_sources();
		assert_eq!(sources, alloc::vec![ParaId::from(100), ParaId::from(200), ParaId::from(300),]);
	}

	#[test]
	fn outgoing_state_update_and_recompute() {
		let mut state = OutgoingMessageState::new();

		let root_a = make_hash(1);
		let root_b = make_hash(2);
		state.update_destination(ParaId::from(100), root_a);
		state.update_destination(ParaId::from(200), root_b);
		state.recompute_root();

		let entries = alloc::vec![(ParaId::from(100), root_a), (ParaId::from(200), root_b),];
		let expected = DestinationMerkleTree::compute_root(&entries);
		assert_eq!(state.current_root, expected);
	}

	#[test]
	fn outgoing_state_provides_commitment() {
		let mut state = OutgoingMessageState::new();
		state.update_destination(ParaId::from(500), make_hash(55));
		state.recompute_root();

		let commitment = state.provides_commitment();
		assert_eq!(commitment.root, state.current_root);
	}

	#[test]
	fn outgoing_state_update_destination_replaces() {
		let mut state = OutgoingMessageState::new();
		let dest = ParaId::from(100);

		state.update_destination(dest, make_hash(1));
		assert_eq!(state.get_destination_root(dest), Some(make_hash(1)));

		state.update_destination(dest, make_hash(2));
		assert_eq!(state.get_destination_root(dest), Some(make_hash(2)));

		assert_eq!(state.destination_count(), 1);
	}

	#[test]
	fn outgoing_state_empty_root() {
		let state = OutgoingMessageState::new();
		assert_eq!(state.current_root, H256::zero());
	}

	#[test]
	fn encode_decode_roundtrip() {
		// SourceState roundtrip
		let mut source = SourceState::new();
		source.advance(10, make_hash(99));
		let encoded = source.encode();
		let decoded = SourceState::decode(&mut &encoded[..]).expect("SourceState should decode");
		assert_eq!(source, decoded);

		// IncomingMessageState roundtrip
		let mut incoming = IncomingMessageState::new();
		incoming.advance_source(ParaId::from(100), 5, make_hash(1));
		incoming.advance_source(ParaId::from(200), 3, make_hash(2));
		let encoded = incoming.encode();
		let decoded = IncomingMessageState::decode(&mut &encoded[..])
			.expect("IncomingMessageState should decode");
		assert_eq!(incoming, decoded);

		// OutgoingMessageState roundtrip
		let mut outgoing = OutgoingMessageState::new();
		outgoing.update_destination(ParaId::from(300), make_hash(30));
		outgoing.recompute_root();
		let encoded = outgoing.encode();
		let decoded = OutgoingMessageState::decode(&mut &encoded[..])
			.expect("OutgoingMessageState should decode");
		assert_eq!(outgoing, decoded);
	}

	// ---------------------------------------------------------------
	// Tests for build_tree() and sync_from_tree()
	// ---------------------------------------------------------------

	#[test]
	fn outgoing_state_build_tree_empty() {
		let state = OutgoingMessageState::new();
		let tree = state.build_tree();
		assert_eq!(tree.root(), H256::zero());
		assert!(tree.is_empty());
		assert_eq!(tree.len(), 0);
	}

	#[test]
	fn outgoing_state_build_tree_matches_recompute() {
		let mut state = OutgoingMessageState::new();
		state.update_destination(ParaId::from(100), make_hash(1));
		state.update_destination(ParaId::from(200), make_hash(2));
		state.update_destination(ParaId::from(300), make_hash(3));
		state.recompute_root();

		let tree = state.build_tree();
		assert_eq!(tree.root(), state.current_root);
	}

	#[test]
	fn outgoing_state_build_tree_and_update() {
		let mut state = OutgoingMessageState::new();
		state.update_destination(ParaId::from(100), make_hash(1));
		state.update_destination(ParaId::from(200), make_hash(2));
		state.update_destination(ParaId::from(300), make_hash(3));

		let mut tree = state.build_tree();
		let new_root_200 = make_hash(22);
		tree.update(ParaId::from(200), new_root_200)
			.expect("update existing dest should succeed");

		// A fresh compute_root with the updated value must match.
		let expected = DestinationMerkleTree::compute_root(&[
			(ParaId::from(100), make_hash(1)),
			(ParaId::from(200), new_root_200),
			(ParaId::from(300), make_hash(3)),
		]);
		assert_eq!(tree.root(), expected);
	}

	#[test]
	fn outgoing_state_sync_from_tree() {
		let mut state = OutgoingMessageState::new();
		state.update_destination(ParaId::from(100), make_hash(1));
		state.update_destination(ParaId::from(200), make_hash(2));

		let mut tree = state.build_tree();
		let new_root_200 = make_hash(22);
		tree.update(ParaId::from(200), new_root_200)
			.expect("update existing dest should succeed");

		state.sync_from_tree(&tree);

		assert_eq!(state.current_root, tree.root());
		assert_eq!(
			state.get_destination_root(ParaId::from(200)),
			Some(new_root_200),
		);
		assert_eq!(state.destination_count(), 2);
	}

	#[test]
	fn outgoing_state_sync_from_tree_with_new_dest() {
		let mut state = OutgoingMessageState::new();
		state.update_destination(ParaId::from(100), make_hash(1));
		state.update_destination(ParaId::from(200), make_hash(2));

		let mut tree = state.build_tree();
		let new_dest_root = make_hash(33);
		tree.upsert(ParaId::from(300), new_dest_root);

		state.sync_from_tree(&tree);

		assert_eq!(state.current_root, tree.root());
		assert_eq!(
			state.get_destination_root(ParaId::from(300)),
			Some(new_dest_root),
		);
		assert_eq!(state.destination_count(), 3);
	}

	#[test]
	fn outgoing_state_sync_from_tree_with_removal() {
		let mut state = OutgoingMessageState::new();
		state.update_destination(ParaId::from(100), make_hash(1));
		state.update_destination(ParaId::from(200), make_hash(2));
		state.update_destination(ParaId::from(300), make_hash(3));

		let mut tree = state.build_tree();
		tree.remove(ParaId::from(200))
			.expect("remove existing dest should succeed");

		state.sync_from_tree(&tree);

		assert_eq!(state.current_root, tree.root());
		assert_eq!(state.get_destination_root(ParaId::from(200)), None);
		assert_eq!(state.destination_count(), 2);
		// Remaining destinations are intact.
		assert_eq!(
			state.get_destination_root(ParaId::from(100)),
			Some(make_hash(1)),
		);
		assert_eq!(
			state.get_destination_root(ParaId::from(300)),
			Some(make_hash(3)),
		);
	}

	#[test]
	fn outgoing_state_roundtrip_build_sync() {
		let mut state = OutgoingMessageState::new();
		state.update_destination(ParaId::from(100), make_hash(1));
		state.update_destination(ParaId::from(200), make_hash(2));
		state.update_destination(ParaId::from(300), make_hash(3));
		state.recompute_root();

		let original = state.clone();
		let tree = state.build_tree();
		state.sync_from_tree(&tree);

		assert_eq!(state, original);
	}
}
