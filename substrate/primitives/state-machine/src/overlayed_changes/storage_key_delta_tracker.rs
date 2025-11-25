use crate::trace;
use alloc::vec::Vec;
use core::{
	hash::{BuildHasher, Hash},
	mem,
};
use indexmap::IndexMap as HashMap;
#[cfg(not(feature = "std"))]
use indexmap::IndexSet as HashSet;
use nohash_hasher::BuildNoHashHasher;
#[cfg(feature = "std")]
use std::collections::HashSet;

const LOG_TARGET: &str = "storage_key_delta_tracker";

/// Operation type for a key
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeyOp {
	/// Key was updated/inserted
	Updated,
	/// Key was deleted
	Deleted,
}

type KeyMap<K> = HashMap<u64, (K, KeyOp), BuildNoHashHasher<u64>>;
type CapturedSet = HashSet<u64, BuildNoHashHasher<u64>>;

/// Incremental snapshot result: map of base hash -> key for all new keys since last snapshot.
///
/// Base hash value can be ignored.
pub type DeltaKeys<K> = HashMap<u64, K, BuildNoHashHasher<u64>>;

pub type DefaultHashBuilder = foldhash::fast::FixedState;

/// Tracks storage key modifications with transaction support and incremental delta extraction.
///
/// Provides nested transactions and efficient incremental deltas that avoid duplicating
/// previously captured keys. Keys can be tracked as `Updated` or `Deleted` operations.
///
/// # Operation Semantics
///
/// - **Updated**: Standard key insertion/update
/// - **Deleted**: Key deletion that prevents future `Updated` operations from appearing
/// - Updated → Deleted: Both operations appear in separate deltas
/// - Deleted → Updated: Only `Deleted` appears, `Updated` is filtered
///
/// # Note on performance profile
///
/// Optimized for 1k-5k transactions with ~10 nesting levels. Each transaction typically
/// has 10-20 keys, with 50% duplication across transactions. Keys are 64-162 byte arrays
/// (concatenated cryptographic hashes). Deduplication is highly optimized.
#[derive(Debug, Clone)]
pub struct StorageKeyDeltaTracker<K, HB = DefaultHashBuilder> {
	/// Stack of parent transaction layers (outermost first).
	layers: Vec<TransactionLayer<K>>,
	/// Current (innermost) transaction layer being built.
	current: TransactionLayer<K>,
	/// Hash builder for computing key hashes.
	hasher: HB,
}

#[derive(Debug, Clone)]
struct TransactionLayer<K> {
	/// base hash -> key map of all new keys that were not yet included into any delta.
	dirty_keys: KeyMap<K>,
	/// Base hashes of keys that have been already reported in incremental delta.
	snapshot: Option<CapturedSet>,
	/// Base hashes of keys that have been deleted.
	/// Used to prevent `Updated` operations on deleted keys from appearing in future snapshots.
	deleted_keys: CapturedSet,
}

impl<K> Default for TransactionLayer<K> {
	fn default() -> Self {
		Self {
			dirty_keys: KeyMap::with_capacity_and_hasher(16, BuildNoHashHasher::<u64>::default()),
			snapshot: None,
			deleted_keys: CapturedSet::default(),
		}
	}
}

impl<K, H: Default> Default for StorageKeyDeltaTracker<K, H> {
	fn default() -> Self {
		Self { layers: Vec::new(), current: TransactionLayer::default(), hasher: H::default() }
	}
}

impl<K: core::fmt::Debug, H> StorageKeyDeltaTracker<K, H> {
	/// Adds a `key` to the tracker with an operation `op` type.
	///
	/// A key added as `Deleted` prevents future `Updated` operations on the same key
	/// from appearing in snapshots. If a key is captured as `Updated` in one snapshot,
	/// it can still appear as `Deleted` in a subsequent snapshot.
	pub fn add_key(&mut self, key: K, op: KeyOp)
	where
		K: Hash,
		H: BuildHasher,
	{
		trace!(target:LOG_TARGET, "add_key: {:?}/{:?}", key, op);
		let hash = self.hasher.hash_one(&key);
		// Insert into current dirty keys (HashMap dedup happens automatically)
		self.current.dirty_keys.insert(hash, (key, op));
	}

	/// Starts a new transaction layer for atomic operations.
	pub fn start_transaction(&mut self) {
		trace!(target:LOG_TARGET, "start_transaction {}", self.layers.len());
		// Push current layer onto stack
		let old_current = mem::replace(
			&mut self.current,
			TransactionLayer {
				dirty_keys: KeyMap::with_capacity_and_hasher(
					16,
					BuildNoHashHasher::<u64>::default(),
				),
				snapshot: None,
				deleted_keys: CapturedSet::default(),
			},
		);
		self.layers.push(old_current);
	}

	/// Commits the current transaction, merging dirty keys, updated and deleted key sets
	/// into parent.
	pub fn commit_transaction(&mut self) {
		trace!(target:LOG_TARGET, "commit_transaction empty:{}", self.layers.len());
		if let Some(mut parent) = self.layers.pop() {
			if let Some(current_snapshot) = self.current.snapshot.take() {
				match &mut parent.snapshot {
					Some(parent_snapshot) => parent_snapshot.extend(current_snapshot),
					None => parent.snapshot = Some(current_snapshot),
				}
			}

			parent.dirty_keys.extend(self.current.dirty_keys.drain(..));
			parent.deleted_keys.extend(&self.current.deleted_keys);

			self.current = parent;
		}
	}

	/// Rolls back the current transaction, discarding dirty keys, updated and deleted key sets.
	pub fn rollback_transaction(&mut self) {
		trace!(target:LOG_TARGET, "rollback_transaction empty:{}", self.layers.len());
		if let Some(layer) = self.layers.pop() {
			self.current = layer;
		}
	}

	/// Returns keys not in any previous delta, respecting Updated/Deleted semantics.
	///
	/// Dirty keys are collected from current layer and parent layers up to the first captured
	/// delta. After taking a delta:
	/// - On commit: captured keys are merged into parent, dirty keys become unavailable in parent
	/// - On rollback: captured keys are discarded, dirty keys become available again in parent
	pub fn take_delta(&mut self) -> DeltaKeys<K>
	where
		K: Clone,
	{
		let mut delta: DeltaKeys<K> =
			HashMap::with_capacity_and_hasher(16, BuildNoHashHasher::<u64>::default());
		let mut new_deleted_keys = CapturedSet::default();

		let mut process_key = |hash: u64, key: K, op: KeyOp| {
			let is_deleted = self.current.deleted_keys.contains(&hash) ||
				self.layers.iter().any(|layer| layer.deleted_keys.contains(&hash));

			if is_deleted {
				return
			}

			if op == KeyOp::Deleted {
				new_deleted_keys.insert(hash);
				delta.insert(hash, key);
			} else {
				let is_captured = self
					.layers
					.iter()
					.any(|layer| layer.snapshot.as_ref().is_some_and(|s| s.contains(&hash))) ||
					self.current.snapshot.as_ref().is_some_and(|s| s.contains(&hash));

				if !is_captured {
					delta.insert(hash, key);
				}
			}
		};

		for (hash, (key, op)) in self.current.dirty_keys.drain(..) {
			process_key(hash, key, op);
		}

		for layer in self.layers.iter().rev() {
			for (&hash, (key, op)) in &layer.dirty_keys {
				process_key(hash, key.clone(), *op);
			}

			// Stop after processing the first layer with a snapshot
			if layer.snapshot.is_some() {
				break;
			}
		}

		self.current.deleted_keys.extend(&new_deleted_keys);

		trace!(target:LOG_TARGET, "get_delta: {:?}", delta.values().collect::<Vec<_>>());

		// Merge delta into current layer's snapshot
		if !delta.is_empty() {
			match &mut self.current.snapshot {
				Some(snapshot) => {
					snapshot.extend(delta.keys().copied());
				},
				None => {
					self.current.snapshot = Some(delta.keys().copied().collect());
				},
			}
		}

		delta
	}
}

#[cfg(test)]
mod tests {
	use super::{KeyOp, LOG_TARGET};
	use tracing::debug;

	macro_rules! delta_assert_eq {
        ($delta:expr, [$($val:expr),* $(,)?]) => {
            {
                let expected: ::std::collections::HashSet<String> =
                    [$($val),*].iter().cloned().map(String::from).collect();
                let actual: ::std::collections::HashSet<String> =
                    $delta.values().cloned().collect();
                assert_eq!(actual, expected);
            }
        };
    }

	type Tracker = super::StorageKeyDeltaTracker<String>;

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
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_nested_tx_and_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		let d1 = tracker.take_delta();
		delta_assert_eq!(d1, ["a", "b"]);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("e".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("f".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["c", "e", "f"]);
		tracker.rollback_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c", "e"]);
	}

	#[test]
	fn test_nested_tx_and_commit() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("d".to_string(), KeyOp::Updated);
		tracker.add_key("e".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("f".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "c", "d", "e", "f"]);
		tracker.start_transaction();
		tracker.add_key("g".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		tracker.add_key("h".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		tracker.add_key("i".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["g", "h", "i",]);
	}

	#[test]
	fn test_commit_merges_dirty_keys() {
		let mut tracker = Tracker::default();
		tracker.add_key("x".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("y".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["x", "y"]);
	}

	#[test]
	fn test_commit_merges_dirty_keys2() {
		let mut tracker = Tracker::default();
		tracker.add_key("x".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["x"]);
		tracker.start_transaction();
		tracker.add_key("y".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["y"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.rollback_transaction();
		tracker.add_key("d".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "d"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested00() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		{
			tracker.add_key("b".to_string(), KeyOp::Updated);
			tracker.start_transaction();
			{
				tracker.start_transaction();
				{
					tracker.add_key("c".to_string(), KeyOp::Updated);
					let delta = tracker.take_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				tracker.rollback_transaction();
				tracker.add_key("d".to_string(), KeyOp::Updated);
				let delta = tracker.take_delta();
				delta_assert_eq!(delta, ["a", "b", "d"]);
				tracker.add_key("d0".to_string(), KeyOp::Updated);
			}
			tracker.rollback_transaction();
			tracker.add_key("e".to_string(), KeyOp::Updated);
		}
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested01() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		{
			tracker.add_key("b".to_string(), KeyOp::Updated);
			tracker.start_transaction();
			{
				tracker.start_transaction();
				{
					tracker.add_key("c".to_string(), KeyOp::Updated);
					let delta = tracker.take_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				tracker.commit_transaction();
				tracker.add_key("d".to_string(), KeyOp::Updated);
				let delta = tracker.take_delta();
				delta_assert_eq!(delta, ["d"]);
				tracker.add_key("d0".to_string(), KeyOp::Updated);
			}
			tracker.rollback_transaction();
			tracker.add_key("e".to_string(), KeyOp::Updated);
		}
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested02() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		{
			tracker.add_key("b".to_string(), KeyOp::Updated);
			tracker.start_transaction();
			{
				tracker.start_transaction();
				{
					tracker.add_key("c".to_string(), KeyOp::Updated);
					let delta = tracker.take_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				tracker.commit_transaction();
				tracker.add_key("d".to_string(), KeyOp::Updated);
				let delta = tracker.take_delta();
				delta_assert_eq!(delta, ["d"]);
				tracker.add_key("d0".to_string(), KeyOp::Updated);
			}
			tracker.rollback_transaction();
			tracker.add_key("e".to_string(), KeyOp::Updated);
		}
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_simple_snapshot_uniq() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq2() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.commit_transaction();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq3() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		tracker.rollback_transaction();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a", "b", "c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq4() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "c"]);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_simple_snapshot_uniq5() {
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a", "b", "c"]);
		tracker.start_transaction();
		tracker.add_key("d".to_string(), KeyOp::Updated);
		tracker.add_key("e".to_string(), KeyOp::Updated);
		tracker.add_key("f".to_string(), KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["d", "e", "f"]);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_rollback_without_snapshot() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.rollback_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_empty_transaction_commit() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_empty_transaction_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.rollback_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_transaction_snapshot_rollback_root_visibility() {
		let mut tracker = Tracker::default();
		tracker.add_key("root1".to_string(), KeyOp::Updated);
		tracker.add_key("root2".to_string(), KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("tx1".to_string(), KeyOp::Updated);
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
		tracker.add_key("l0".to_string(), KeyOp::Updated);
		let s0 = tracker.take_delta();
		delta_assert_eq!(s0, ["l0"]);

		tracker.start_transaction();
		tracker.add_key("l1".to_string(), KeyOp::Updated);
		let s1 = tracker.take_delta();
		delta_assert_eq!(s1, ["l1"]);

		tracker.start_transaction();
		tracker.add_key("l2".to_string(), KeyOp::Updated);
		let s2 = tracker.take_delta();
		delta_assert_eq!(s2, ["l2"]);

		tracker.start_transaction();
		tracker.add_key("l3".to_string(), KeyOp::Updated);
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
		tracker.add_key("dup".to_string(), KeyOp::Updated);
		tracker.add_key("dup".to_string(), KeyOp::Updated);
		tracker.add_key("dup".to_string(), KeyOp::Updated);
		tracker.add_key("unique".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["dup", "unique"]);
	}

	#[test]
	fn test_updated_then_deleted_same_transaction() {
		// Updated then Deleted in same transaction - both should appear
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]);

		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a"]);
	}

	#[test]
	fn test_updated_then_deleted_across_transactions() {
		// Updated then Deleted across transactions
		let mut tracker = Tracker::default();
		tracker.start_transaction();
		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		tracker.commit_transaction();
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]);

		tracker.start_transaction();
		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a"]);
	}

	#[test]
	fn test_deleted_then_updated_filters_updated() {
		// Deleted then Updated - Updated should be filtered
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]);

		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_deleted_then_updated_no_snapshot_between() {
		// Deleted then Updated before snapshot - only Deleted appears
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_updated_in_parent_deleted_in_child() {
		// Updated in parent, Deleted in child transaction
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		tracker.start_transaction();
		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]); // Only one "a" - the Deleted one wins
		tracker.commit_transaction();
	}

	#[test]
	fn test_deleted_in_parent_updated_in_child_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]);

		tracker.start_transaction();
		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		tracker.rollback_transaction();

		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_multiple_updated_then_deleted() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]);

		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		let delta2 = tracker.take_delta();
		assert!(delta2.is_empty());

		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		let delta3 = tracker.take_delta();
		delta_assert_eq!(delta3, ["a"]);
	}

	#[test]
	fn test_updated_then_deleted_in_child_snapshot_then_rollback() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Updated);

		tracker.start_transaction();
		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
		let delta1 = tracker.take_delta();
		delta_assert_eq!(delta1, ["a"]); // Both Updated and Deleted captured

		tracker.rollback_transaction();

		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a"]); // Updated key appears again
	}

	#[test]
	fn test_updated_then_deleted_in_child_snapshot_then_rollback_2() {
		let mut tracker = Tracker::default();
		tracker.add_key("a".to_string(), super::KeyOp::Updated);
		let delta = tracker.take_delta();
		delta_assert_eq!(delta, ["a"]); // Updated key appears again

		tracker.start_transaction();
		tracker.add_key("a".to_string(), super::KeyOp::Deleted);
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
		tracker.add_key("a".to_string(), KeyOp::Updated);
		tracker.add_key("b".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["a", "b"]);

		tracker.start_transaction();
		tracker.add_key("c".to_string(), KeyOp::Updated);
		tracker.add_key("d".to_string(), KeyOp::Updated);
		tracker.commit_transaction();
		let delta2 = tracker.take_delta();
		delta_assert_eq!(delta2, ["c", "d"]);

		tracker.start_transaction();
		tracker.add_key("e".to_string(), KeyOp::Updated);
		tracker.add_key("f".to_string(), KeyOp::Updated);
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
}
