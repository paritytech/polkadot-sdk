use alloc::vec::Vec;
use core::{
	hash::{BuildHasher, Hash},
	mem,
};
use foldhash::fast::FixedState;
use indexmap::IndexMap as HashMap;
#[cfg(not(feature = "std"))]
use indexmap::IndexSet as HashSet;
use nohash_hasher::BuildNoHashHasher;
#[cfg(feature = "std")]
use std::collections::HashSet;
use tracing::trace;

const LOG_TARGET: &str = "changesetfilter";

type KeyMap<K> = HashMap<u64, K, BuildNoHashHasher<u64>>;
type CapturedSet = HashSet<u64, BuildNoHashHasher<u64>>;
pub type DeltaKeys<K> = KeyMap<K>;

pub type DefaultHashBuilder = foldhash::fast::FixedState;

/// A changeset that tracks keys with transaction support and incremental snapshots.
///
/// `Changeset` provides a way to track and manage collections of keys with support
/// for nested transactions and efficient incremental snapshots. It allows you to
/// build up sets of changes, organize them into atomic transaction boundaries,
/// and capture incremental snapshots that avoid duplicating previously captured keys.
///
/// Typically there will 1k-5k transactions, the level of nesting is not expected to be higher then
/// 10. Each transaction is expected to have 10-20 keys, half of them is expected to be duplicated
/// across transactions, the other half unique for every transaction. Snapshot typically will be
/// queried before start_transaction and commit_transaction, but it is not a rule (implementation
/// hint).
///
/// Keys deduplication must be very well optimized, this strcuture is crucial for performance.
///
/// # Key Features
///
/// - **Transaction Support**: Nested transactions with commit/rollback semantics
/// - **Incremental Snapshots**: Efficient snapshots that only return new keys
/// - **Automatic Deduplication**: Keys are never returned in multiple snapshots
/// - **Atomic Operations**: Transaction boundaries ensure consistency
///
/// # Use Cases
///
/// This structure is particularly useful for:
/// - Change tracking systems that need incremental updates
/// - Building efficient data synchronization protocols
/// - Implementing undo/redo functionality with snapshots
/// - Managing state changes with atomic transaction semantics
///
/// # Basic Workflow
///
/// 1. Add keys to track changes
/// 2. Optionally use transactions for atomic operations
/// 3. Take snapshots to capture incremental changes
/// 4. Repeat the cycle for ongoing change tracking
///
/// # Examples
///
/// ```rust
/// use poc::Changeset;
///
/// let mut changeset: Changeset<&[u8]> = Changeset::new();
///
/// // Basic usage
/// changeset.add_key(b"key1");
/// changeset.add_key(b"key2");
/// let snapshot1 = changeset.create_snapshot_and_get_delta2();
/// assert_eq!(snapshot1.values().collect::<Vec<_>>().len(), 2);
///
/// // With transactions
/// changeset.start_transaction();
/// changeset.add_key(b"key3");
/// changeset.commit_transaction();
///
/// let snapshot2 = changeset.create_snapshot_and_get_delta2();
/// assert_eq!(snapshot2.values().collect::<Vec<_>>().len(), 1);
/// ```
#[derive(Debug, Clone)]
pub struct Changeset<K, HB = DefaultHashBuilder> {
	layers: Vec<TransactionLayer<K>>,
	current: TransactionLayer<K>,
	hasher: HB,
}

#[derive(Debug, Clone)]
struct TransactionLayer<K> {
	dirty_keys: KeyMap<K>,
	snapshot: Option<CapturedSet>,
}

impl<K> Default for TransactionLayer<K> {
	fn default() -> Self {
		Self {
			dirty_keys: KeyMap::with_capacity_and_hasher(16, BuildNoHashHasher::<u64>::default()),
			snapshot: None,
		}
	}
}

impl<K, H: Default> Default for Changeset<K, H> {
	fn default() -> Self {
		Self { layers: Vec::new(), current: TransactionLayer::default(), hasher: H::default() }
	}
}

impl<K, H: Default> Changeset<K, H> {
	/// Creates a new, empty instance of Changeset
	pub fn new() -> Self {
		Self::default()
	}
}

impl<K:core::fmt::Debug, H> Changeset<K, H> {
	/// Adds a key to the current changeset.
	///
	/// Registers a key as part of the current changes being tracked. The key will be
	/// included in future snapshots until it has been captured.
	///
	/// # Parameters
	///
	/// * `key` - The key to add to the changeset
	///
	/// # Snapshot Impact
	///
	/// Keys added via this method will be included in the next snapshot returned by
	/// `create_snapshot_and_get_delta2()`, unless they were already captured in a previous
	/// snapshot.
	///
	/// The key typically is already cryptographically hashed. It is array of bytes, lenght greater
	/// then 64.
	///
	/// # Examples
	///
	/// ```rust
	/// use poc::Changeset;
	///
	/// let mut changeset: Changeset<&[u8]> = Changeset::new();
	/// changeset.add_key(b"my_key");
	/// changeset.add_key(b"another_key");
	///
	/// let snapshot = changeset.create_snapshot_and_get_delta2();
	/// assert!(snapshot.values().any(|&k| k == b"my_key"));
	/// assert!(snapshot.values().any(|&k| k == b"another_key"));
	/// ```
	pub fn add_key(&mut self, key: K)
	where
		K: Hash,
		H: BuildHasher,
	{
		// Insert into current dirty keys (HashMap dedup happens automatically)
		trace!(target:LOG_TARGET, "add_key: {:?}", key);
		self.current.dirty_keys.insert(self.hasher.hash_one(&key), key);
	}

	/// Starts a new transaction layer.
	///
	/// Creates a new transaction scope that allows for atomic operations. Changes made
	/// within this transaction can later be committed or rolled back.
	///
	/// # Snapshot Impact
	///
	/// Starting a new transaction does not immediately affect snapshots. Keys added
	/// after starting a transaction will be included in future snapshots until the
	/// transaction is committed or rolled back.
	///
	/// # Examples
	///
	/// ```rust
	/// use poc::Changeset;
	///
	/// let mut changeset: Changeset<&[u8]> = Changeset::new();
	/// changeset.add_key(b"base_key");
	///
	/// changeset.start_transaction();
	/// changeset.add_key(b"tx_key");
	/// let snapshot = changeset.create_snapshot_and_get_delta2();
	/// assert!(snapshot.values().any(|&k| k == b"base_key"));
	/// assert!(snapshot.values().any(|&k| k == b"tx_key"));
	/// ```
	pub fn start_transaction(&mut self) {
		trace!(target:LOG_TARGET, "start_transaction");
		// Push current layer onto stack
		let old_current = mem::replace(
			&mut self.current,
			TransactionLayer {
				dirty_keys: KeyMap::with_capacity_and_hasher(
					16,
					BuildNoHashHasher::<u64>::default(),
				),
				snapshot: None,
			},
		);
		self.layers.push(old_current);
	}

	/// Commits the current transaction, making its changes permanent.
	///
	/// Makes all changes in the current transaction permanent and merges them with
	/// the parent scope. After commit, the changes cannot be rolled back.
	///
	/// # Snapshot Impact
	///
	/// Committing has sophisticated effects on snapshots:
	/// - **Snapshot Consolidation** - if the transaction contains multiple snapshots, they are
	///   merged into a single consolidated snapshot to optimize storage
	/// - **Parent Integration** - snapshots from the committed transaction are integrated into the
	///   parent transaction's snapshot structure
	/// - **Key Deduplication** - redundant dirty keys in the parent that are already captured in
	///   committed snapshots are automatically removed
	/// - **Structural Optimization** - the transaction structure is flattened and optimized for
	///   better performance
	///
	/// The commit process ensures that snapshot boundaries remain consistent while
	/// optimizing the internal representation for efficiency.
	///
	/// # Examples
	///
	/// ```rust
	/// use poc::Changeset;
	///
	/// let mut changeset: Changeset<&[u8]> = Changeset::new();
	/// changeset.add_key(b"base");
	///
	/// changeset.start_transaction();
	/// changeset.add_key(b"tx1");
	/// let snap1 = changeset.create_snapshot_and_get_delta2();
	/// assert!(snap1.values().any(|&k| k == b"base"));
	///
	/// changeset.add_key(b"tx2");
	/// let snap2 = changeset.create_snapshot_and_get_delta2();
	/// assert!(snap2.values().any(|&k| k == b"tx2"));
	///
	/// changeset.commit_transaction();
	/// ```
	pub fn commit_transaction(&mut self) {
		trace!(target:LOG_TARGET, "commit_transaction empty:{}", self.layers.is_empty());
		if self.layers.is_empty() {
			return;
		}

		// Pop parent layer
		let mut parent = self.layers.pop().expect("No transaction to commit");

		// Step 1: Merge ALL current dirty keys into parent (no filtering needed)
		parent.dirty_keys.extend(self.current.dirty_keys.drain(..));

		// Step 2: Merge current snapshot into parent snapshot
		if let Some(current_snapshot) = self.current.snapshot.take() {
			match &mut parent.snapshot {
				Some(parent_snapshot) => {
					// Merge current snapshot into existing parent snapshot
					parent_snapshot.extend(current_snapshot);
				},
				None => {
					// Parent has no snapshot, just move current snapshot
					parent.snapshot = Some(current_snapshot);
				},
			}
		}

		// Step 3: Make parent the new current
		self.current = parent;
	}

	/// Rolls back the current transaction, discarding all its changes.
	///
	/// Completely discards all changes made within the current transaction scope.
	/// This operation is irreversible and reverts the changeset to the exact state
	/// before the transaction was started.
	///
	/// # Snapshot Impact
	///
	/// Rolling back has these effects on snapshots:
	/// - **Discards snapshots taken within the transaction** - any snapshots created after
	///   `start_transaction()` are removed from history
	/// - **Restores dirty key visibility** - dirty keys from parent layers that were captured by
	///   discarded snapshots become available for future snapshots again
	/// - **Removes transaction-local keys** - keys added only in this transaction are discarded
	///   entirely
	/// - **Preserves prior state** - snapshots taken before the transaction remain intact
	///
	/// # Examples
	///
	/// ```rust
	/// use poc::Changeset;
	///
	/// let mut changeset: Changeset<&[u8]> = Changeset::new();
	/// changeset.add_key(b"before");
	/// let snap1 = changeset.create_snapshot_and_get_delta2();
	/// assert!(snap1.values().any(|&k| k == b"before"));
	///
	/// changeset.add_key(b"after");
	/// changeset.start_transaction();
	/// changeset.add_key(b"in_tx");
	/// let snap2 = changeset.create_snapshot_and_get_delta2();
	/// assert!(snap2.values().any(|&k| k == b"after"));
	///
	/// changeset.rollback_transaction();
	/// // snap2 discarded, "after" available again, "in_tx" gone
	///
	/// let snap3 = changeset.create_snapshot_and_get_delta2();
	/// assert!(snap3.values().any(|&k| k == b"after"));
	/// assert!(!snap3.values().any(|&k| k == b"before")); // filtered
	/// ```
	pub fn rollback_transaction(&mut self) {
		trace!(target:LOG_TARGET, "rollback_transaction empty:{}", self.layers.is_empty());
		// Simply discard current layer and restore parent
		if self.layers.is_empty() {
			return;
		}

		self.current = self.layers.pop().expect("No transaction to rollback");
	}

	/// Creates a snapshot of the current state and returns new keys.
	///
	/// Captures the current state and returns only the keys that have been added
	/// since any previous snapshot was taken. Each key is only returned once across
	/// all snapshot operations.
	///
	/// # Returns
	///
	/// A collection containing only the keys that are new since the last snapshot.
	/// Returns empty if no new keys have been added.
	///
	/// # Snapshot Impact
	///
	/// After calling this method:
	/// - All returned keys become part of the snapshot history
	/// - Future snapshots will only include keys added after this call
	/// - Duplicate keys are automatically filtered out
	///
	/// # Examples
	///
	/// ```rust
	/// use poc::Changeset;
	///
	/// let mut changeset: Changeset<&[u8]> = Changeset::new();
	///
	/// changeset.add_key(b"key1");
	/// changeset.add_key(b"key2");
	///
	/// let snapshot1 = changeset.create_snapshot_and_get_delta2();
	/// assert!(snapshot1.values().any(|&k| k == b"key1"));
	///
	/// changeset.add_key(b"key1"); // duplicate
	/// changeset.add_key(b"key3"); // new
	///
	/// let snapshot2 = changeset.create_snapshot_and_get_delta2();
	/// assert!(snapshot2.values().any(|&k| k == b"key3"));
	/// assert!(!snapshot2.values().any(|&k| k == b"key1")); // filtered
	/// ```
	pub fn create_snapshot_and_get_delta2(&mut self) -> DeltaKeys<K>
	where
		K: Clone,
	{
		let mut delta: DeltaKeys<K> =
			KeyMap::with_capacity_and_hasher(16, BuildNoHashHasher::<u64>::default());

		let is_captured = |hash: u64| {
			self.layers
				.iter()
				.any(|layer| layer.snapshot.as_ref().is_some_and(|s| s.contains(&hash))) ||
				self.current.snapshot.as_ref().is_some_and(|s| s.contains(&hash))
		};

		// Process current layer dirty keys
		for (hash, key) in self.current.dirty_keys.drain(..) {
			if !is_captured(hash) {
				delta.insert(hash, key);
			}
		}

		// Process parent layers dirty keys until we hit first snapshot
		for layer in self.layers.iter().rev() {
			for (&hash, key) in &layer.dirty_keys {
				if !is_captured(hash) {
					delta.insert(hash, key.clone());
				}
			}

			// Stop after processing the first layer with a snapshot
			if layer.snapshot.is_some() {
				break;
			}
		}
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
	use super::LOG_TARGET;
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

	type Changeset = super::Changeset<String>;

	#[test]
	fn test_empty_snapshot() {
		let mut changeset = Changeset::new();
		let delta = changeset.create_snapshot_and_get_delta2();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta2();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta2();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta2();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta2();
		assert!(delta.is_empty());
	}

	#[test]
	fn test_simple_snapshot() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b"]);
		changeset.add_key("c".to_string());
		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_nested_tx_and_rollback() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let d1 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(d1, ["a", "b"]);
		changeset.add_key("c".to_string());
		changeset.start_transaction();
		changeset.add_key("e".to_string());
		changeset.start_transaction();
		changeset.add_key("f".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["c", "e", "f"]);
		changeset.rollback_transaction();
		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["c", "e"]);
	}

	#[test]
	fn test_nested_tx_and_commit() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		changeset.start_transaction();
		changeset.add_key("d".to_string());
		changeset.add_key("e".to_string());
		changeset.start_transaction();
		changeset.add_key("f".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b", "c", "d", "e", "f"]);
		changeset.start_transaction();
		changeset.add_key("g".to_string());
		changeset.commit_transaction();
		changeset.add_key("h".to_string());
		changeset.commit_transaction();
		changeset.add_key("i".to_string());
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["g", "h", "i",]);
	}

	#[test]
	fn test_commit_merges_dirty_keys() {
		let mut changeset = Changeset::new();
		changeset.add_key("x".to_string());
		changeset.start_transaction();
		changeset.add_key("y".to_string());
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["x", "y"]);
	}

	#[test]
	fn test_commit_merges_dirty_keys2() {
		let mut changeset = Changeset::new();
		changeset.add_key("x".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["x"]);
		changeset.start_transaction();
		changeset.add_key("y".to_string());
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["y"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.start_transaction();
		changeset.add_key("b".to_string());
		changeset.start_transaction();
		changeset.add_key("c".to_string());
		changeset.rollback_transaction();
		changeset.add_key("d".to_string());
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b", "d"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested00() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.start_transaction();
		{
			changeset.add_key("b".to_string());
			changeset.start_transaction();
			{
				changeset.start_transaction();
				{
					changeset.add_key("c".to_string());
					let delta = changeset.create_snapshot_and_get_delta2();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				changeset.rollback_transaction();
				changeset.add_key("d".to_string());
				let delta = changeset.create_snapshot_and_get_delta2();
				delta_assert_eq!(delta, ["a", "b", "d"]);
				changeset.add_key("d0".to_string());
			}
			changeset.rollback_transaction();
			changeset.add_key("e".to_string());
		}
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested01() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.start_transaction();
		{
			changeset.add_key("b".to_string());
			changeset.start_transaction();
			{
				changeset.start_transaction();
				{
					changeset.add_key("c".to_string());
					let delta = changeset.create_snapshot_and_get_delta2();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				changeset.commit_transaction();
				changeset.add_key("d".to_string());
				let delta = changeset.create_snapshot_and_get_delta2();
				delta_assert_eq!(delta, ["d"]);
				changeset.add_key("d0".to_string());
			}
			changeset.rollback_transaction();
			changeset.add_key("e".to_string());
		}
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_open_commit_and_rollback_combined_nested02() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.start_transaction();
		{
			changeset.add_key("b".to_string());
			changeset.start_transaction();
			{
				changeset.start_transaction();
				{
					changeset.add_key("c".to_string());
					let delta = changeset.create_snapshot_and_get_delta2();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				changeset.commit_transaction();
				changeset.add_key("d".to_string());
				let delta = changeset.create_snapshot_and_get_delta2();
				delta_assert_eq!(delta, ["d"]);
				changeset.add_key("d0".to_string());
			}
			changeset.rollback_transaction();
			changeset.add_key("e".to_string());
		}
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b", "e"]);
	}

	#[test]
	fn test_simple_snapshot_uniq() {
		// Initialize tracing with RUST_LOG support
		// tracing_subscriber::fmt()
		// 	.with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
		// 	.try_init()
		// 	.ok(); // Ignore error if already initialized
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b"]);
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq2() {
		let mut changeset = Changeset::new();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b"]);
		changeset.commit_transaction();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		changeset.commit_transaction();
		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq3() {
		let mut changeset = Changeset::new();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b"]);
		changeset.rollback_transaction();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		changeset.commit_transaction();
		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["a", "b", "c"]);
	}

	#[test]
	fn test_simple_snapshot_uniq4() {
		let mut changeset = Changeset::new();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b", "c"]);
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		changeset.commit_transaction();
		changeset.commit_transaction();
		let delta2 = changeset.create_snapshot_and_get_delta2();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_simple_snapshot_uniq5() {
		let mut changeset = Changeset::new();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a", "b", "c"]);
		changeset.start_transaction();
		changeset.add_key("d".to_string());
		changeset.add_key("e".to_string());
		changeset.add_key("f".to_string());
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["d", "e", "f"]);
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let delta2 = changeset.create_snapshot_and_get_delta2();
		assert!(delta2.is_empty());
	}

	#[test]
	fn test_rollback_without_snapshot() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.start_transaction();
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		changeset.rollback_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_empty_transaction_commit() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.start_transaction();
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_empty_transaction_rollback() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.start_transaction();
		changeset.rollback_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["a"]);
	}

	#[test]
	fn test_transaction_snapshot_rollback_root_visibility() {
		let mut changeset = Changeset::new();
		changeset.add_key("root1".to_string());
		changeset.add_key("root2".to_string());
		changeset.start_transaction();
		changeset.add_key("tx1".to_string());
		let snap1 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(snap1, ["root1", "root2", "tx1"]);
		changeset.rollback_transaction();
		// After rollback, root1 and root2 should be available again
		let snap2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(snap2, ["root1", "root2"]);
	}

	#[test]
	fn test_deep_nesting_snapshots_at_every_level() {
		let mut changeset = Changeset::new();
		changeset.add_key("l0".to_string());
		let s0 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(s0, ["l0"]);

		changeset.start_transaction();
		changeset.add_key("l1".to_string());
		let s1 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(s1, ["l1"]);

		changeset.start_transaction();
		changeset.add_key("l2".to_string());
		let s2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(s2, ["l2"]);

		changeset.start_transaction();
		changeset.add_key("l3".to_string());
		let s3 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(s3, ["l3"]);

		changeset.commit_transaction();
		changeset.commit_transaction();
		changeset.commit_transaction();

		let final_snap = changeset.create_snapshot_and_get_delta2();
		assert!(final_snap.is_empty());
	}

	#[test]
	fn test_duplicate_keys_in_same_transaction() {
		let mut changeset = Changeset::new();
		changeset.start_transaction();
		changeset.add_key("dup".to_string());
		changeset.add_key("dup".to_string());
		changeset.add_key("dup".to_string());
		changeset.add_key("unique".to_string());
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta, ["dup", "unique"]);
	}

	#[test]
	fn xxxx() {
		// use tracing_subscriber::EnvFilter;
		// // Initialize tracing with RUST_LOG support
		// tracing_subscriber::fmt()
		// 	.with_env_filter(EnvFilter::from_default_env())
		// 	.try_init()
		// 	.ok(); // Ignore error if already initialized
		//
		let mut changeset = Changeset::new();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.commit_transaction();
		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["a", "b"]);

		changeset.start_transaction();
		changeset.add_key("c".to_string());
		changeset.add_key("d".to_string());
		changeset.commit_transaction();
		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["c", "d"]);

		changeset.start_transaction();
		changeset.add_key("e".to_string());
		changeset.add_key("f".to_string());
		debug!(target:LOG_TARGET, ">> before commit {:?}", changeset);
		changeset.commit_transaction();
		debug!(target:LOG_TARGET, ">> after commit {:?}", changeset);

		let delta2 = changeset.create_snapshot_and_get_delta2();
		delta_assert_eq!(delta2, ["e", "f"]);
		debug!(target:LOG_TARGET, ">> after snap {:?}", changeset);
		changeset.start_transaction();
		changeset.commit_transaction();
		debug!(target:LOG_TARGET, ">> after final commit {:?}", changeset);
		let delta2 = changeset.create_snapshot_and_get_delta2();
		assert!(delta2.is_empty());
	}
}
