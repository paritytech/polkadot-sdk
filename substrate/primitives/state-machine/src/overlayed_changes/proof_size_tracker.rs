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
// limitations under the License

//! Tracks modified storage keys to optimize PoV size estimation for storage root computation.

use crate::{
	backend::{EstimationTrie, EstimationTrieSnapshot},
	trace,
};
use alloc::vec::Vec;
use core::{
	fmt::{self, Debug},
	hash::{BuildHasherDefault, Hasher as CoreHasher},
	mem,
};
use hash_db::Hasher;
use indexmap::IndexMap as HashMap;
use sp_crypto_hashing::blake2_128;

#[cfg(feature = "std")]
const LOG_TARGET: &str = "proof_size_tracker";

/// Operation type for a key in the delta tracker.
///
/// Carried through the pipeline from the delta tracker to the trie layer,
/// determining whether a key should be read (Updated) or removed (Deleted)
/// during PoV size estimation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum DeltaKeyOp {
	/// Key was updated/inserted
	Updated,
	/// Key was deleted
	Deleted,
}

/// Identity of a storage key inside the tracker: the Blake2 hash of its bytes.
pub type KeyHash = u128;

/// A key the tracker can identify, see [`KeyHash`].
pub trait TrackedKey {
	/// The identity of the key.
	fn key_hash(&self) -> KeyHash;
}

impl TrackedKey for Vec<u8> {
	fn key_hash(&self) -> KeyHash {
		KeyHash::from_le_bytes(blake2_128(self))
	}
}

impl TrackedKey for (Vec<u8>, Vec<u8>) {
	fn key_hash(&self) -> KeyHash {
		let (prefix, key) = self;
		let prefix_len = (prefix.len() as u32).to_le_bytes();
		KeyHash::from_le_bytes(blake2_128(&[&prefix_len[..], prefix, key].concat()))
	}
}

/// No hashing: the key is a hash already, its low 64 bits are the bucket index.
#[derive(Default)]
pub struct KeyHashHasher(u64);

impl CoreHasher for KeyHashHasher {
	fn finish(&self) -> u64 {
		self.0
	}

	fn write(&mut self, _: &[u8]) {
		unreachable!("only `KeyHash` is hashed, which uses `write_u128`; qed")
	}

	fn write_u128(&mut self, key_hash: u128) {
		self.0 = key_hash as u64;
	}
}

/// Builder of [`KeyHashHasher`], the map hasher of [`DeltaKeys`].
pub type KeyHashBuilder = BuildHasherDefault<KeyHashHasher>;

/// Maps key hash -> (key, last op) for keys staged in the current transaction layer.
type KeyMap<K> = HashMap<KeyHash, (K, DeltaKeyOp), KeyHashBuilder>;

/// Maps key hash -> the latest op `take_delta` saw for the key, reported or not. Lets later
/// `take_delta` calls skip ops they already reported.
type SnapshotMap = HashMap<KeyHash, DeltaKeyOp, KeyHashBuilder>;

/// Key operations returned by [`ProofSizeTracker::take_delta`].
///
/// Keyed by [`KeyHash`], which callers ignore; the values carry the key and its op.
pub type DeltaKeys<K> = HashMap<KeyHash, (K, DeltaKeyOp), KeyHashBuilder>;

/// Tracks storage key modifications with transaction support and incremental delta extraction.
///
/// Supports nested transactions and incremental deltas. Keys are tracked as `Updated` or
/// `Deleted`. Repeated operations are omitted from later deltas.
///
/// # Reporting
///
/// [`Self::take_delta`] reports pending key operations. A key is included if its latest operation
/// differs from the operation previously reported for it in a transaction that has not been rolled
/// back. Multiple changes between calls are collapsed into one operation.
///
/// A key can therefore be reported as `Updated`, then `Deleted`, then `Updated` across separate
/// calls. The second `Updated` report adds no nodes to the proof: the deletion already loaded the
/// key's path in the block-start trie.
///
/// # Estimation trie
///
/// The estimation trie is an in-memory trie used to estimate the proof size needed for storage
/// root computation.
///
/// Only removals are applied to the estimation trie. An update is estimated by reading the key's
/// path in the block-start trie, while a removal restructures the trie and may load sibling nodes
/// that later removals depend on. The trie therefore accumulates removals across size estimation
/// calls.
///
/// # Key identity
///
/// Keys are 64 to 162 byte arrays, and a block writes thousands of them. The tracker looks
/// each key up many times: on every write, on every size estimation call, and once per open
/// layer when it checks whether the key was already reported. Hashing the full key bytes on
/// each of those lookups was measured at 6-7% of block production time.
///
/// So the tracker hashes a key once, when it is written, and uses the hash as the key's
/// identity from then on. See [`KeyHash`] and [`TrackedKey`]. All maps are keyed by that
/// hash. Their own hasher, [`KeyHashHasher`], does no hashing at all: the key is already a
/// hash, so it just returns the low 64 bits as the bucket index.
///
/// The hash is Blake2-128. It is deterministic, so the collator and the PVF derive the same
/// identities. Collisions are assumed to be computationally infeasible; a collision would merge
/// two keys and could drop a report.
pub struct ProofSizeTracker<K, H: Hasher> {
	/// Stack of parent transaction layers (outermost first).
	layers: Vec<TransactionLayer<K, H>>,
	/// Current (innermost) transaction layer being built.
	current: TransactionLayer<K, H>,
	/// In-memory trie kept across PoV size estimation calls, so removals accumulate as they do in
	/// the real storage root computation. Created by the first size estimation call that applies a
	/// removal.
	estimation_trie: Option<EstimationTrie<H>>,
}

struct TransactionLayer<K, H: Hasher> {
	/// Pending operations stored in this layer, keyed by key hash.
	///
	/// Each entry holds the key and its latest operation. An entry leaves this map when a delta is
	/// taken for this layer, or when a child transaction whose snapshot holds the key commits.
	dirty_keys: KeyMap<K>,
	/// Latest operation `take_delta` saw for each key, reported or not. Snapshots of child
	/// transactions are merged into it on commit.
	snapshot: Option<SnapshotMap>,
	/// The trie as it was when this transaction started, `None` if the trie did not exist when the
	/// transaction started.
	trie_snapshot: Option<EstimationTrieSnapshot<H>>,
}

impl<K, H: Hasher> Default for TransactionLayer<K, H> {
	fn default() -> Self {
		Self {
			dirty_keys: KeyMap::with_hasher(KeyHashBuilder::default()),
			snapshot: None,
			trie_snapshot: None,
		}
	}
}

impl<K: Debug, H: Hasher> Debug for TransactionLayer<K, H> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("TransactionLayer")
			.field("dirty_keys", &self.dirty_keys)
			.field("snapshot", &self.snapshot)
			.finish_non_exhaustive()
	}
}

impl<K, H: Hasher> Default for ProofSizeTracker<K, H> {
	fn default() -> Self {
		Self { layers: Vec::new(), current: TransactionLayer::default(), estimation_trie: None }
	}
}

impl<K: Debug, H: Hasher> Debug for ProofSizeTracker<K, H> {
	fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
		f.debug_struct("ProofSizeTracker")
			.field("layers", &self.layers)
			.field("current", &self.current)
			.finish_non_exhaustive()
	}
}

impl<K, H: Hasher> ProofSizeTracker<K, H> {
	/// Creates a tracker pre-initialized with `depth` empty transaction layers.
	///
	/// Used by `spawn_child` to match the parent's transaction depth so that
	/// commit/rollback operations on pre-existing levels work correctly
	/// instead of being silent no-ops. The tracker starts without an estimation trie, so no layer
	/// holds a trie snapshot.
	pub fn with_transaction_depth(depth: usize) -> Self {
		Self {
			layers: (0..depth).map(|_| TransactionLayer::default()).collect(),
			current: TransactionLayer::default(),
			estimation_trie: None,
		}
	}
}

impl<K: core::fmt::Debug, H: Hasher> ProofSizeTracker<K, H> {
	/// Adds a `key` to the tracker with an operation `op` type.
	///
	/// A later op on the same key replaces this one; the next delta reports the key only if its
	/// newest op differs from the one in the snapshot.
	pub fn add_key(&mut self, key: K, op: DeltaKeyOp)
	where
		K: TrackedKey,
	{
		trace!(target:LOG_TARGET, "add_key: {:?}/{:?}", key, op);
		let hash = key.key_hash();
		self.current.dirty_keys.insert(hash, (key, op));
	}

	/// Starts a new transaction layer for atomic operations.
	///
	/// The new layer holds a snapshot of the estimation trie taken at this point, if the trie
	/// exists.
	pub fn start_transaction(&mut self) {
		trace!(target:LOG_TARGET, "start_transaction {}", self.layers.len());
		let trie_snapshot = self.estimation_trie.as_mut().map(|trie| trie.snapshot());
		// Push current layer onto stack
		let old_current = mem::replace(
			&mut self.current,
			TransactionLayer { trie_snapshot, ..Default::default() },
		);
		self.layers.push(old_current);
	}

	/// Commits the current transaction, merging its pending operations and snapshot into the
	/// parent. Parent operations are removed for keys in the child's snapshot.
	///
	/// The estimation trie keeps its changes and the layer's trie snapshot is discarded.
	pub fn commit_transaction(&mut self) {
		trace!(target:LOG_TARGET, "commit_transaction empty:{}", self.layers.len());
		if let Some(mut parent) = self.layers.pop() {
			if let Some(current_snapshot) = self.current.snapshot.take() {
				// The child's snapshot supersedes the parent's pending operations for the same
				// keys. Remove those operations so they cannot be reported later.
				for hash in current_snapshot.keys() {
					parent.dirty_keys.swap_remove(hash);
				}
				match &mut parent.snapshot {
					// Child ops overwrite parent ops for the same key (child is more recent).
					Some(parent_snapshot) => parent_snapshot.extend(current_snapshot),
					None => parent.snapshot = Some(current_snapshot),
				}
			}

			parent.dirty_keys.extend(self.current.dirty_keys.drain(..));

			self.current = parent;
		}
	}

	/// Rolls back the current transaction, discarding its pending operations and snapshot.
	///
	/// The estimation trie is rolled back to the trie snapshot of the discarded layer, or dropped
	/// if it did not exist when that layer started.
	pub fn rollback_transaction(&mut self) {
		trace!(target:LOG_TARGET, "rollback_transaction empty:{}", self.layers.len());
		if let Some(layer) = self.layers.pop() {
			let discarded = mem::replace(&mut self.current, layer);
			match (self.estimation_trie.as_mut(), discarded.trie_snapshot) {
				(Some(trie), Some(snapshot)) => trie.rollback_to(&snapshot).expect(
					"layers are a stack, so a layer's snapshot is never rolled back past; qed",
				),
				(_, None) => self.estimation_trie = None,
				(None, Some(_)) => unreachable!(
					"a layer only holds a snapshot of a trie that existed when it started; qed"
				),
			}
		}
	}

	/// The trie kept for PoV size estimation, see [`EstimationTrie`].
	pub fn estimation_trie_mut(&mut self) -> &mut Option<EstimationTrie<H>> {
		&mut self.estimation_trie
	}

	/// Returns pending key operations. Only the latest operation for each key is considered. It is
	/// returned if it differs from the operation in the snapshot, or if the key is not in the
	/// snapshot yet. Either way the snapshot records it.
	///
	/// Snapshots follow transaction commits and rollbacks. Rolling back discards the snapshot of
	/// that transaction, so pending operations of outer layers that it covered can be returned
	/// again.
	///
	/// Does not touch the estimation trie.
	#[must_use]
	pub fn take_delta(&mut self) -> DeltaKeys<K>
	where
		K: Clone,
	{
		let TransactionLayer { dirty_keys, snapshot, .. } = &mut self.current;

		// Collect the newest pending op for each key. Start with the current layer, then walk
		// outwards. Stop after processing the first layer with a snapshot. Skip keys already
		// collected or already in the current layer's snapshot.
		let mut newest = mem::take(dirty_keys);
		for layer in self.layers.iter().rev() {
			for (&hash, entry) in &layer.dirty_keys {
				let in_current_snapshot = snapshot.as_ref().is_some_and(|s| s.contains_key(&hash));
				if !newest.contains_key(&hash) && !in_current_snapshot {
					newest.insert(hash, entry.clone());
				}
			}
			if layer.snapshot.is_some() {
				break;
			}
		}

		// Report a key when its newest op differs from the op in the snapshots. Record every
		// key's newest op in the current snapshot, reported or not.
		let mut delta: DeltaKeys<K> = HashMap::with_hasher(KeyHashBuilder::default());
		for (hash, (key, op)) in newest {
			let standing_op = snapshot.as_ref().and_then(|s| s.get(&hash).copied()).or_else(|| {
				self.layers.iter().rev().find_map(|l| l.snapshot.as_ref()?.get(&hash).copied())
			});
			snapshot.get_or_insert_with(Default::default).insert(hash, op);
			if standing_op != Some(op) {
				delta.insert(hash, (key, op));
			}
		}

		trace!(target:LOG_TARGET, "get_delta: {:?}", delta.values().collect::<Vec<_>>());
		delta
	}
}

#[cfg(test)]
mod tests {
	use super::{blake2_128, DeltaKeyOp, KeyHash, KeyHashBuilder, TrackedKey, LOG_TARGET};
	use sp_core::Blake2Hasher;

	impl TrackedKey for String {
		fn key_hash(&self) -> KeyHash {
			KeyHash::from_le_bytes(blake2_128(self.as_bytes()))
		}
	}
	use tracing::debug;

	macro_rules! delta_assert_eq {
        ($delta:expr, [$($val:expr),* $(,)?]) => {
            {
                let expected: ::std::collections::HashSet<String> =
                    [$($val),*].iter().cloned().map(String::from).collect();
                let actual: ::std::collections::HashSet<String> =
                    $delta.values().map(|(k, _)| k.clone()).collect();
                assert_eq!(actual, expected);
            }
        };
    }

	macro_rules! delta_assert_eq_with_ops {
        ($delta:expr, [$(($key:expr, $op:expr)),* $(,)?]) => {
            {
                let expected: ::std::collections::HashSet<(String, DeltaKeyOp)> =
                    [$(($key.to_string(), $op)),*].into_iter().collect();
                let actual: ::std::collections::HashSet<(String, DeltaKeyOp)> =
                    $delta.values().map(|(k, op)| (k.clone(), *op)).collect();
                assert_eq!(actual, expected);
            }
        };
    }

	type Tracker = super::ProofSizeTracker<String, Blake2Hasher>;

	#[test]
	fn test_empty_snapshot() {
		let mut tracker = Tracker::default();
		let delta = tracker.take_delta();
		assert!(delta.is_empty());
		let delta = tracker.take_delta();
		assert!(delta.is_empty());
		let delta = tracker.take_delta();
		assert!(delta.is_empty());
		let delta = tracker.take_delta();
		assert!(delta.is_empty());
		let delta = tracker.take_delta();
		assert!(delta.is_empty());
	}

	#[test]
	fn test_simple_snapshot() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_nested_tx_and_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		let d1 = tracker.take_delta();
		delta_assert_eq!(d1, ["a", "b"]);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("e".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("f".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["c", "e", "f"]);
		tracker.rollback_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c", "e"]);
	}

	#[test]
	fn test_nested_tx_and_commit() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("d".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("e".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("f".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "c", "d", "e", "f"]);
		tracker.start_transaction();
		tracker.add_key("g".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		tracker.add_key("h".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		tracker.add_key("i".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["g", "h", "i",]);
	}

	#[test]
	fn test_commit_merges_dirty_keys() {
		let mut tracker = Tracker::default();
		tracker.add_key("x".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("y".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["x", "y"]);
	}

	#[test]
	fn test_commit_merges_dirty_keys2() {
		let mut tracker = Tracker::default();
		tracker.add_key("x".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["x"]);
		tracker.start_transaction();
		tracker.add_key("y".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["y"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.rollback_transaction();
		tracker.add_key("d".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "d"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested00() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		{
			tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
			tracker.start_transaction();
			{
				tracker.start_transaction();
				{
					tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
					let delta = tracker.take_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				tracker.rollback_transaction();
				tracker.add_key("d".to_string(), DeltaKeyOp::Updated);
				let delta = tracker.take_delta();
				delta_assert_eq!(delta, ["a", "b", "d"]);
				tracker.add_key("d0".to_string(), DeltaKeyOp::Updated);
			}
			tracker.rollback_transaction();
			tracker.add_key("e".to_string(), DeltaKeyOp::Updated);
		}
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested01() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		{
			tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
			tracker.start_transaction();
			{
				tracker.start_transaction();
				{
					tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
					let delta = tracker.take_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				tracker.commit_transaction();
				tracker.add_key("d".to_string(), DeltaKeyOp::Updated);
				let delta = tracker.take_delta();
				delta_assert_eq!(delta, ["d"]);
				tracker.add_key("d0".to_string(), DeltaKeyOp::Updated);
			}
			tracker.rollback_transaction();
			tracker.add_key("e".to_string(), DeltaKeyOp::Updated);
		}
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested02() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		{
			tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
			tracker.start_transaction();
			{
				tracker.start_transaction();
				{
					tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
					let delta = tracker.take_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				tracker.commit_transaction();
				tracker.add_key("d".to_string(), DeltaKeyOp::Updated);
				let delta = tracker.take_delta();
				delta_assert_eq!(delta, ["d"]);
				tracker.add_key("d0".to_string(), DeltaKeyOp::Updated);
			}
			tracker.rollback_transaction();
			tracker.add_key("e".to_string(), DeltaKeyOp::Updated);
		}
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_simple_snapshot_uniq() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq2() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.commit_transaction();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq3() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.rollback_transaction();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a", "b", "c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq4() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "c"]);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_simple_snapshot_uniq5() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "c"]);
		tracker.start_transaction();
		tracker.add_key("d".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("e".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("f".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["d", "e", "f"]);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_rollback_without_snapshot() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.rollback_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_empty_transaction_commit() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_empty_transaction_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.rollback_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_transaction_snapshot_rollback_root_visibility() {
		let mut tracker = Tracker::default();
		tracker.add_key("root1".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("root2".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("tx1".to_string(), DeltaKeyOp::Updated);
		let snap1 = tracker.take_delta();
		delta_assert_eq!(snap1, ["root1", "root2", "tx1"]);
		tracker.rollback_transaction();
		// After rollback, root1 and root2 should be available again
		let snap2 = tracker.take_delta();
		delta_assert_eq!(snap2, ["root1", "root2"]);
	}

	#[test]
	fn test_deep_nesting_snapshots_at_every_level() {
		let mut tracker = Tracker::default();
		tracker.add_key("l0".to_string(), DeltaKeyOp::Updated);
		let s0 = tracker.take_delta();
		delta_assert_eq!(s0, ["l0"]);

		tracker.start_transaction();
		tracker.add_key("l1".to_string(), DeltaKeyOp::Updated);
		let s1 = tracker.take_delta();
		delta_assert_eq!(s1, ["l1"]);

		tracker.start_transaction();
		tracker.add_key("l2".to_string(), DeltaKeyOp::Updated);
		let s2 = tracker.take_delta();
		delta_assert_eq!(s2, ["l2"]);

		tracker.start_transaction();
		tracker.add_key("l3".to_string(), DeltaKeyOp::Updated);
		let s3 = tracker.take_delta();
		delta_assert_eq!(s3, ["l3"]);

		tracker.commit_transaction();
		tracker.commit_transaction();
		tracker.commit_transaction();

		let final_snap = tracker.take_delta();
		assert!(final_snap.is_empty());
	}

	#[test]
	fn test_duplicate_keys_in_same_transaction() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("dup".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("dup".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("dup".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("unique".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["dup", "unique"]);
	}

	#[test]
	fn test_updated_then_deleted_same_transaction() {
		// Updated then Deleted in same transaction - both should appear
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let delta1 = tracker.take_delta();
		delta_assert_eq_with_ops!(delta1, [("a", DeltaKeyOp::Updated)]);

		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		let delta2 = tracker.take_delta();
		delta_assert_eq_with_ops!(delta2, [("a", DeltaKeyOp::Deleted)]);
	}

	#[test]
	fn test_updated_then_deleted_across_transactions() {
		// Updated then Deleted across transactions
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta1 = tracker.take_delta();
		delta_assert_eq_with_ops!(delta1, [("a", DeltaKeyOp::Updated)]);

		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq_with_ops!(delta2, [("a", DeltaKeyOp::Deleted)]);
	}

	/// A key pending in an outer layer and written again in an inner one must be reported with
	/// the newer op, its final state: an older `Updated` would skip the removal the estimation
	/// has to model, an older `Deleted` would model a removal that never happens.
	#[test]
	fn newer_op_in_inner_layer_wins_over_older_op_in_outer_layer() {
		for (older, newer) in
			[(DeltaKeyOp::Updated, DeltaKeyOp::Deleted), (DeltaKeyOp::Deleted, DeltaKeyOp::Updated)]
		{
			let mut tracker = Tracker::default();
			tracker.start_transaction();
			tracker.add_key("a".to_string(), older);
			tracker.start_transaction();
			tracker.add_key("a".to_string(), newer);

			let delta = tracker.take_delta();
			delta_assert_eq_with_ops!(delta, [("a", newer)]);
		}
	}

	/// The collator and the PVF must identify keys the same way, otherwise they could disagree
	/// on which keys collide and estimate different sizes for the same block. Blake2 of the key
	/// bytes is the same on every build and needs no seed.
	#[test]
	fn key_identity_is_blake2_128_of_the_key_bytes() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);

		let delta = tracker.take_delta();
		let expected = KeyHash::from_le_bytes(blake2_128(b"a"));
		assert_eq!(delta.keys().copied().collect::<Vec<_>>(), vec![expected]);
	}

	/// An estimation that reports a key from the inner layer has accounted for the older write
	/// of that key still pending in the outer layer. The next estimation must not report that
	/// older write: it would remove a key that exists in the final state.
	#[test]
	fn outer_write_reported_by_the_inner_layer_is_not_reported_again() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);

		let first = tracker.take_delta();
		delta_assert_eq_with_ops!(first, [("a", DeltaKeyOp::Updated)]);

		let second = tracker.take_delta();
		assert!(second.is_empty(), "stale outer write reported again: {:?}", second.values());
	}

	/// Committing the inner layer merges its report into the outer one, next to the older
	/// pending write it covered; that write must not be reported afterwards either.
	#[test]
	fn outer_write_reported_by_the_inner_layer_is_not_reported_after_commit() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let first = tracker.take_delta();
		delta_assert_eq_with_ops!(first, [("a", DeltaKeyOp::Updated)]);
		tracker.commit_transaction();

		let second = tracker.take_delta();
		assert!(second.is_empty(), "stale outer write reported again: {:?}", second.values());
	}

	/// The newest op of a key enters the snapshot, even when it is not reported. The inner
	/// `Updated` matches the reported `Updated`, so nothing is reported; the older outer `Deleted`
	/// must not be reported in its place, the key exists in the final state.
	#[test]
	fn suppressed_newer_update_still_supersedes_older_pending_delete() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let _ = tracker.take_delta();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);

		let delta = tracker.take_delta();
		assert!(delta.is_empty(), "older delete reported: {:?}", delta.values());
	}

	/// Committing the inner layer must not revive the older outer `Deleted` either.
	#[test]
	fn suppressed_newer_update_still_supersedes_older_pending_delete_after_commit() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let _ = tracker.take_delta();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let inner = tracker.take_delta();
		assert!(inner.is_empty(), "older delete reported: {:?}", inner.values());
		tracker.commit_transaction();

		let delta = tracker.take_delta();
		assert!(delta.is_empty(), "older delete reported: {:?}", delta.values());
	}

	/// Rolling back the inner `Updated` leaves the outer `Deleted` as the key's final state, so
	/// the next estimation must report it.
	#[test]
	fn rolled_back_newer_update_lets_older_pending_delete_through() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let _ = tracker.take_delta();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let inner = tracker.take_delta();
		assert!(inner.is_empty(), "older delete reported: {:?}", inner.values());
		tracker.rollback_transaction();

		let delta = tracker.take_delta();
		delta_assert_eq_with_ops!(delta, [("a", DeltaKeyOp::Deleted)]);
	}

	/// The collator wraps every runtime call in a host transaction, the validator runs the
	/// block in one layer; both must report the same ops at every estimation or they disagree
	/// on the block weight. The script writes a key, then writes it again inside a layer and
	/// estimates while the layer is open and after it commits. It also deletes a reported key and
	/// writes it again inside a layer.
	#[test]
	fn nested_layers_report_the_same_ops_as_a_single_layer() {
		#[derive(Clone, Copy)]
		enum Step {
			Write(&'static str, DeltaKeyOp),
			Begin,
			Commit,
			Estimate,
		}
		use Step::*;
		let script = [
			Write("a", DeltaKeyOp::Deleted),
			Write("b", DeltaKeyOp::Updated),
			Begin,
			Write("a", DeltaKeyOp::Updated),
			Estimate,
			Estimate,
			Write("b", DeltaKeyOp::Deleted),
			Commit,
			Estimate,
			Begin,
			Write("c", DeltaKeyOp::Updated),
			Commit,
			Estimate,
			Write("d", DeltaKeyOp::Updated),
			Estimate,
			Write("d", DeltaKeyOp::Deleted),
			Begin,
			Write("d", DeltaKeyOp::Updated),
			Estimate,
			Commit,
			Estimate,
		];
		let reports = |with_layers: bool| {
			let mut tracker = Tracker::default();
			let mut reports = Vec::new();
			for step in script {
				match step {
					Write(key, op) => tracker.add_key(key.to_string(), op),
					Begin if with_layers => tracker.start_transaction(),
					Commit if with_layers => tracker.commit_transaction(),
					Begin | Commit => {},
					Estimate => {
						let mut delta: Vec<_> =
							tracker.take_delta().values().map(|(k, op)| (k.clone(), *op)).collect();
						delta.sort();
						reports.push(delta);
					},
				}
			}
			reports
		};
		assert_eq!(reports(true), reports(false));
	}

	#[test]
	fn test_deleted_then_updated_reports_updated() {
		// Deleted then Updated - Updated is reported, it differs from the reported Deleted
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		let delta1 = tracker.take_delta();
		delta_assert_eq_with_ops!(delta1, [("a", DeltaKeyOp::Deleted)]);

		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let delta2 = tracker.take_delta();
		delta_assert_eq_with_ops!(delta2, [("a", DeltaKeyOp::Updated)]);
	}

	#[test]
	fn test_deleted_then_updated_no_snapshot_between() {
		// Deleted then Updated before snapshot - only Deleted appears
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_updated_in_parent_deleted_in_child() {
		// Updated in parent, Deleted in child transaction
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]); // Only one "a" - the Deleted one wins
		tracker.commit_transaction();
	}

	#[test]
	fn test_deleted_in_parent_updated_in_child_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]);

		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.rollback_transaction();

		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_multiple_updated_then_deleted() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]);

		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());

		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		let delta3 = tracker.take_delta();
		delta_assert_eq!(delta3, ["a"]);
	}

	#[test]
	fn test_updated_then_deleted_in_child_snapshot_then_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);

		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]); // Both Updated and Deleted captured

		tracker.rollback_transaction();

		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a"]); // Updated key appears again
	}

	#[test]
	fn test_updated_then_deleted_in_child_snapshot_then_rollback_2() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]); // Updated key appears again

		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Deleted);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]); // Both Updated and Deleted captured

		tracker.rollback_transaction();
		let delta = tracker.take_delta();
		assert!(delta.is_empty()); // Filtered
	}

	#[test]
	fn delta_tracks_across_multiple_commit_cycles() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a", "b"]);

		tracker.start_transaction();
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("d".to_string(), DeltaKeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c", "d"]);

		tracker.start_transaction();
		tracker.add_key("e".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("f".to_string(), DeltaKeyOp::Updated);
		debug!(target:LOG_TARGET, ">> before commit {:?}", tracker);
		tracker.commit_transaction();
		debug!(target:LOG_TARGET, ">> after commit {:?}", tracker);

		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["e", "f"]);
		debug!(target:LOG_TARGET, ">> after snap {:?}", tracker);
		tracker.start_transaction();
		tracker.commit_transaction();
		debug!(target:LOG_TARGET, ">> after final commit {:?}", tracker);
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_with_transaction_depth_commit() {
		// Simulates spawn_child: tracker starts with 3 empty layers.
		let mut tracker = Tracker::with_transaction_depth(3);

		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);

		// Commit all 3 pre-existing layers — keys merge down.
		tracker.commit_transaction();
		tracker.commit_transaction();
		tracker.commit_transaction();

		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
	}

	#[test]
	fn test_with_transaction_depth_rollback_discards_keys() {
		let mut tracker = Tracker::with_transaction_depth(3);

		// Add keys at depth 3 (innermost).
		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);

		// Commit once: depth 3→2. "a" merges into depth 2.
		tracker.commit_transaction();

		// Add key at depth 2.
		tracker.add_key("b".to_string(), DeltaKeyOp::Updated);

		// Rollback depth 2 — discards "a" (merged in) and "b".
		tracker.rollback_transaction();

		// Add key at depth 1 after rollback.
		tracker.add_key("c".to_string(), DeltaKeyOp::Updated);

		// Commit depth 1.
		tracker.commit_transaction();

		// Only "c" survives.
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["c"]);
	}

	#[test]
	fn test_with_transaction_depth_rollback_all() {
		let mut tracker = Tracker::with_transaction_depth(2);

		tracker.add_key("a".to_string(), DeltaKeyOp::Updated);

		// Rollback both layers — discards everything.
		tracker.rollback_transaction();
		tracker.rollback_transaction();

		let delta = tracker.take_delta();
		assert!(delta.is_empty());
	}

	mod estimation_trie {
		use super::Tracker;
		use crate::backend::EstimationTrie;
		use sp_core::{storage::StateVersion, Blake2Hasher, H256};
		use sp_trie::{
			AppendOnlyTrieDBMutBuilder, DetachedTrieDBMut, LayoutV1, MemoryDB, TrieDBMutBuilder,
			TrieMut,
		};

		type Layout = LayoutV1<Blake2Hasher>;

		const FIRST: &[u8] = b"first";
		const SECOND: &[u8] = b"second";
		const THIRD: &[u8] = b"third";

		struct Fixture {
			tracker: Tracker,
			db: MemoryDB<Blake2Hasher>,
			root: H256,
		}

		impl Fixture {
			fn new() -> Self {
				let mut db = MemoryDB::default();
				let mut root = H256::default();
				{
					let mut trie = TrieDBMutBuilder::<Layout>::new(&mut db, &mut root).build();
					for key in [FIRST, SECOND, THIRD] {
						trie.insert(key, b"value").unwrap();
					}
				}
				Self { tracker: Tracker::default(), db, root }
			}

			fn remove(&mut self, key: &[u8]) {
				self.remove_all(&[key]);
			}

			fn remove_all(&mut self, keys: &[&[u8]]) {
				let trie = self.take_trie();
				let trie = sp_trie::remove_trie_keys_from_delta::<Layout, _, _, _>(
					&self.db,
					keys.iter().copied(),
					trie,
					None,
					None,
				)
				.unwrap();
				*self.tracker.estimation_trie_mut() = Some(EstimationTrie::V1(trie));
			}

			fn contains(&mut self, key: &[u8]) -> bool {
				let trie = self.take_trie();
				let trie =
					AppendOnlyTrieDBMutBuilder::<Layout>::from_detached(&self.db, trie).build();
				let contains = trie.get(key).unwrap().is_some();
				*self.tracker.estimation_trie_mut() = Some(EstimationTrie::V1(trie.detach()));
				contains
			}

			fn take_trie(&mut self) -> DetachedTrieDBMut<Layout> {
				let slot = self.tracker.estimation_trie_mut();
				match slot
					.take()
					.unwrap_or_else(|| EstimationTrie::new(StateVersion::V1, self.root))
				{
					EstimationTrie::V1(trie) => trie,
					EstimationTrie::V0(_) => unreachable!("the fixture only creates V1 tries"),
				}
			}
		}

		/// A rolled back extrinsic must not leave its removals in the trie, or later size
		/// estimation calls would estimate collapses the real storage root computation never does.
		#[test]
		fn rollback_undoes_a_removal() {
			let mut fixture = Fixture::new();
			fixture.remove(THIRD);

			fixture.tracker.start_transaction();
			fixture.remove(FIRST);
			fixture.tracker.rollback_transaction();

			assert!(fixture.contains(FIRST));
			assert!(!fixture.contains(THIRD));
		}

		/// A committed removal is part of the block, so later size estimation calls build on it.
		#[test]
		fn commit_keeps_a_removal() {
			let mut fixture = Fixture::new();

			fixture.tracker.start_transaction();
			fixture.remove(FIRST);
			fixture.tracker.commit_transaction();

			assert!(fixture.tracker.estimation_trie_mut().is_some());
			assert!(!fixture.contains(FIRST));
			assert!(fixture.contains(SECOND));
		}

		/// Each transaction layer must restore exactly the trie it started with, so an inner
		/// rollback keeps the outer layer's removals.
		#[test]
		fn nested_rollbacks_undo_their_own_removals() {
			let mut fixture = Fixture::new();

			fixture.tracker.start_transaction();
			fixture.remove(FIRST);
			fixture.tracker.start_transaction();
			fixture.remove(SECOND);

			fixture.tracker.rollback_transaction();
			assert!(!fixture.contains(FIRST));
			assert!(fixture.contains(SECOND));

			fixture.tracker.rollback_transaction();
			assert!(fixture.contains(FIRST));
			assert!(fixture.contains(SECOND));
		}

		/// Rolling back to before the first size estimation call drops the trie, so the next call
		/// creates it on the block-start root instead of continuing a state built on it.
		#[test]
		fn rollback_before_any_size_estimation_continues_from_the_given_root() {
			let mut fixture = Fixture::new();

			fixture.tracker.start_transaction();
			fixture.remove(FIRST);
			fixture.tracker.rollback_transaction();

			assert!(fixture.tracker.estimation_trie_mut().is_none());
			assert!(fixture.contains(FIRST));
		}
	}

	#[test]
	fn keys_sharing_low_identity_bits_spread_over_buckets() {
		use core::hash::BuildHasher;
		let builder = KeyHashBuilder::default();
		let shared_low_half = 0x1234u128;
		let buckets: std::collections::BTreeSet<u64> = (0..4096u128)
			.map(|high_half| builder.hash_one((high_half << 64) | shared_low_half) & 0xffff)
			.collect();
		assert!(
			buckets.len() > 1,
			"keys with equal low identity bits must still spread over buckets"
		);
	}
}
