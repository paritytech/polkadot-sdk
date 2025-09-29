use alloc::{vec, vec::Vec};
use core::{
	cmp::Eq,
	hash::{BuildHasher, Hash},
};
use nohash_hasher::BuildNoHashHasher;

#[cfg(not(feature = "std"))]
pub type DefaultHashBuilder = sp_trie::RandomState;

// #[cfg(feature = "std")]
// pub type DefaultHashBuilder = foldhash::quality::RandomState;
#[cfg(feature = "std")]
// pub type DefaultHashBuilder = foldhash::quality::RandomState;
pub type DefaultHashBuilder = foldhash::fast::FixedState;

#[cfg(not(feature = "std"))]
use hashbrown::{
	hash_map::{IntoValues, Values},
	HashMap as Map,
};
// #[cfg(feature = "std")]
// use std::collections::{
// 	hash_map::{IntoValues, Values},
// 	HashMap as Map,
// };

#[cfg(feature = "std")]
use indexmap::{map as hash_map, map::IntoValues, map::Values, IndexMap as Map};

pub type XxxKey = u64;
const LOG_TARGET: &str = "xxx";

fn xxx_key<K: Hash, HB: BuildHasher>(key: &K, hash_builder: &HB) -> XxxKey {
	use core::hash::Hasher;
	let mut hasher = hash_builder.build_hasher();
	key.hash(&mut hasher);
	hasher.finish()
}

#[derive(Debug, Clone)]
pub struct InternalSet<K, HB = DefaultHashBuilder> {
	inner: Map<XxxKey, K, BuildNoHashHasher<XxxKey>>,
	hash_builder: HB,
}

pub type DeltaKeys<K> = Map<XxxKey, K, BuildNoHashHasher<XxxKey>>;

impl<K, HB: BuildHasher + Default> InternalSet<K, HB> {
	pub fn new() -> Self {
		Self {
			inner: Map::with_capacity_and_hasher(1024, BuildNoHashHasher::<XxxKey>::default()),
			hash_builder: HB::default(),
		}
	}

	pub fn new_with_hash_builder(hash_builder: HB) -> Self {
		Self {
			inner: Map::with_capacity_and_hasher(1024, BuildNoHashHasher::<XxxKey>::default()),
			hash_builder,
		}
	}
}
impl<K: Hash + Eq, HB: BuildHasher + Default> InternalSet<K, HB> {
	pub fn with_capacity(capacity: usize) -> Self {
		Self {
			inner: Map::with_capacity_and_hasher(capacity, BuildNoHashHasher::<XxxKey>::default()),
			hash_builder: HB::default(),
		}
	}

	pub fn insert(&mut self, value: K) -> bool {
		let key = xxx_key(&value, &self.hash_builder);
		self.inner.insert(key, value).is_none()
	}

	pub fn contains(&self, value: &K) -> bool {
		let key = xxx_key(value, &self.hash_builder);
		self.inner.contains_key(&key)
	}

	pub fn contains_hash(&self, hash_key: &XxxKey) -> bool {
		self.inner.contains_key(hash_key)
	}

	pub fn is_empty(&self) -> bool {
		self.inner.is_empty()
	}

	pub fn extend(&mut self, other: InternalSet<K, HB>) {
		self.inner.extend(other.inner);
	}

	pub fn iter(&self) -> Values<'_, XxxKey, K> {
		self.inner.values()
	}
}

impl<K, HB: BuildHasher + Default> Default for InternalSet<K, HB> {
	fn default() -> Self {
		Self::new()
	}
}

// impl<K: Hash + Eq + AsRef<[u8]>> std::fmt::Debug for InternalSet<K> {
// 	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
// 		f.debug_set()
// 			.entries(self.inner.values().map(|k| hex::encode(k.as_ref())))
// 			.finish()
// 	}
// }

impl<K: Hash + Eq, HB: BuildHasher> IntoIterator for InternalSet<K, HB> {
	type Item = K;
	type IntoIter = IntoValues<XxxKey, K>;

	fn into_iter(self) -> Self::IntoIter {
		self.inner.into_values()
	}
}

impl<'a, K: Hash + Eq, HB: BuildHasher> IntoIterator for &'a InternalSet<K, HB> {
	type Item = &'a K;
	type IntoIter = Values<'a, XxxKey, K>;

	fn into_iter(self) -> Self::IntoIter {
		self.inner.values()
	}
}

#[derive(Debug, Clone)]
enum TransactionKeys<K, HB = DefaultHashBuilder> {
	Dirty(InternalSet<K, HB>),
	Snapshot(InternalSet<K, HB>),
}

// impl<K: Hash + Eq + AsRef<[u8]>> std::fmt::Debug for TransactionKeys<K> {
// 	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
// 		match self {
// 			TransactionKeys::Dirty(set) => f.debug_tuple("Dirty").field(set).finish(),
// 			TransactionKeys::Snapshot(set) => f.debug_tuple("Snapshot").field(set).finish(),
// 		}
// 	}
// }

impl<K: Hash + Eq, HB: BuildHasher + Default> TransactionKeys<K, HB> {
	fn is_empty(&self) -> bool {
		match self {
			TransactionKeys::Dirty(k) | TransactionKeys::Snapshot(k) => k.is_empty(),
		}
	}
	fn keys(&self) -> &InternalSet<K, HB> {
		match self {
			TransactionKeys::Dirty(k) | TransactionKeys::Snapshot(k) => k,
		}
	}
}

impl<K, HB: BuildHasher + Default> Default for TransactionKeys<K, HB> {
	fn default() -> Self {
		TransactionKeys::Dirty(Default::default())
	}
}

type Transaction<K, HB = DefaultHashBuilder> = Vec<TransactionKeys<K, HB>>;

#[derive(Debug, Clone)]
pub struct Changeset<K, HB = DefaultHashBuilder> {
	// Stack of transactions.
	transactions: Vec<Transaction<K, HB>>,
	// Shared hasher for all InternalSet instances
	hash_builder: HB,
}

// impl<K: Hash + Eq + AsRef<[u8]>> std::fmt::Debug for Changeset<K> {
// 	fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
// 		f.debug_struct("Changeset").field("transactions", &self.transactions).finish()
// 	}
// }

impl<K, HB: BuildHasher + Default + Clone> Default for Changeset<K, HB> {
	fn default() -> Self {
		Self::new()
	}
}

impl<K, HB: BuildHasher + Default + Clone> Changeset<K, HB> {
	pub fn new() -> Self {
		let hash_builder = HB::default();
		// Initialize with one transaction having a single empty dirty set
		let initial_set = InternalSet::new_with_hash_builder(hash_builder.clone());
		Changeset { transactions: vec![vec![TransactionKeys::Dirty(initial_set)]], hash_builder }
	}
}

// impl<K: Ord + Hash + Clone, V> OverlayedMap<K, V> {
impl<K: Clone + Hash + Ord + core::fmt::Debug, HB: BuildHasher + Default + Clone> Changeset<K, HB> {
	pub fn add_key(&mut self, key: K) {
		if let Some(transaction) = self.transactions.last_mut() {
			match transaction.last_mut() {
				Some(TransactionKeys::Dirty(dirty_set)) => {
					dirty_set.insert(key);
				},
				Some(TransactionKeys::Snapshot(_)) | None => {
					let mut set = InternalSet::new_with_hash_builder(self.hash_builder.clone());
					set.insert(key);
					transaction.push(TransactionKeys::Dirty(set));
				},
			}
		}
	}

	pub fn start_transaction(&mut self) {
		self.transactions
			.push(vec![TransactionKeys::Dirty(InternalSet::new_with_hash_builder(
				self.hash_builder.clone(),
			))]);
	}

	pub fn commit_transaction(&mut self) {
		if self.transactions.len() <= 1 {
			return;
		}

		let mut commited = self.transactions.pop().expect("there is at leas one transactions. qed");
		debug_assert!(
			commited.iter().filter(|k| matches!(k, TransactionKeys::Dirty(_))).count() <= 1
		);

		// commited: |Snapshot| |Dirty|
		//      tx5: |Snapshot| |Dirty|
		//      tx4: |Snapshot| |Dirty|
		//      tx3: |Snapshot| |Dirty|
		//      tx2: |Snapshot| |Dirty|

		// add key: a
		// add key: b
		//   snapshot
		// add key: c
		//
		fn fix<K: Clone + Hash + Ord + core::fmt::Debug, HB: BuildHasher + Default>(
			top_transaction: &mut Transaction<K, HB>,
		) {
			if top_transaction.len() > 1 {
				let dirty = top_transaction.pop().expect("there is always dirty at the end");

				if let TransactionKeys::Snapshot(mut base) = top_transaction.swap_remove(0) {
					while let Some(TransactionKeys::Snapshot(keys)) = top_transaction.pop() {
						//debug!(target:LOG_TARGET, "base: {}/{}, new keys len: {}/{}",
						// base.inner.len(), base.inner.capacity(), keys.inner.len(),
						// keys.inner.capacity());
						base.extend(keys);
					}
					*top_transaction = vec![TransactionKeys::Snapshot(base), dirty];
				} else {
					unreachable!("xxx");
				}
			}
		}
		if let Some(top_transaction) = self.transactions.last_mut() {
			if matches!(commited.first(), Some(TransactionKeys::Snapshot(_))) {
				if matches!(top_transaction.last(), Some(TransactionKeys::Dirty(_))) {
					//all dirty keys of top transaction must be contained in first snapshot of
					// commited transaction. So we can drop them as we will be appending
					// commited keys.
					let dirty = top_transaction.pop().expect("there is at least one item.qed");
					debug_assert!(dirty.keys().iter().all(|k| commited
						.first()
						.expect("there is at least one item.qed")
						.keys()
						.contains(k)));
				}
				top_transaction.extend(commited);
				fix(top_transaction);
			} else if commited.len() == 1 {
				// commited transaction does not contain any snapshots. We need to merge keys with
				// preivous trasnsaction.
				match commited.remove(0) {
					TransactionKeys::Dirty(commited_dirty_keys) => {
						if let Some(TransactionKeys::Dirty(ref mut dirty)) =
							top_transaction.last_mut()
						{
							dirty.extend(commited_dirty_keys);
						} else {
							// top_transaction.push(TransactionKeys::Dirty(commited_dirty_keys))
							// transaction always have Dirty.
							unreachable!();
						}
					},
					snapshot => top_transaction.push(snapshot),
				}
				fix(top_transaction);
			} else {
				top_transaction.extend(commited);
			}
		}
	}

	pub fn rollback_transaction(&mut self) {
		if self.transactions.len() <= 1 {
			return;
		}
		let t = self.transactions.pop();
		debug_assert!(
			t.unwrap().iter().filter(|k| matches!(k, TransactionKeys::Dirty(_))).count() <= 1
		);
	}

	pub fn create_snapshot_and_get_delta(&mut self) -> DeltaKeys<K> {
		// Gather dirty keys from all transactions from newest to oldest, stopping at last snapshot
		// found
		let mut delta = DeltaKeys::<K>::default();

		'outer: for transaction in self.transactions.iter().rev() {
			// Process keys in reverse to find last snapshot and collect dirty keys
			for keys in transaction.iter().rev() {
				match keys {
					TransactionKeys::Dirty(set) => {
						// Accumulate live dirty keys
						for (hash_key, key) in &set.inner {
							delta.insert(*hash_key, key.clone());
						}
					},
					TransactionKeys::Snapshot(_) => {
						// Found snapshot: stop collecting keys from older transactions / snapshots
						break 'outer;
					},
				}
			}
		}

		// if cumulated dirty keys are empty and most recent transaction contains snapshot, return
		// early - prevent pushing multiple dirty keys
		if delta.is_empty() {
			return delta;
		}

		// Append snapshot keys and new dirty keys to the most recent transaction:
		if let Some(top_transaction) = self.transactions.last_mut() {
			top_transaction.last_mut().map(|stage| {
				let mut internal_set =
					InternalSet::new_with_hash_builder(self.hash_builder.clone());
				internal_set.inner.extend(delta.iter().map(|(k, v)| (*k, v.clone())));
				*stage = TransactionKeys::Snapshot(internal_set);
			});
			top_transaction.push(TransactionKeys::Dirty(InternalSet::new_with_hash_builder(
				self.hash_builder.clone(),
			)));
		}
		delta
	}

	pub fn create_snapshot_and_get_delta2(&mut self) -> DeltaKeys<K> {
		let mut snapshot_keys = Vec::new();
		for transaction in self.transactions.iter() {
			for keys in transaction.iter() {
				if let TransactionKeys::Snapshot(set) = keys {
					snapshot_keys.push(set);
				}
			}
		}

		// Second pass: collect filtered dirty keys using single contains() check
		let mut delta = DeltaKeys::<K>::default();
		for transaction in self.transactions.iter().rev() {
			for keys in transaction.iter().rev() {
				if let TransactionKeys::Dirty(set) = keys {
					for (hash_key, key) in &set.inner {
						if !snapshot_keys.iter().any(|snapshot| snapshot.contains_hash(hash_key)) {
							delta.insert(*hash_key, key.clone());
						}
					}
				}
			}
		}

		// Drop all_snapshot_keys here to release immutable borrows
		drop(snapshot_keys);

		if delta.is_empty() {
			return delta;
		}

		// Now we can safely modify self
		if let Some(top_transaction) = self.transactions.last_mut() {
			top_transaction.last_mut().map(|stage| {
				let mut internal_set =
					InternalSet::new_with_hash_builder(self.hash_builder.clone());
				internal_set.inner.extend(delta.iter().map(|(k, v)| (*k, v.clone())));
				*stage = TransactionKeys::Snapshot(internal_set);
			});
			top_transaction.push(TransactionKeys::Dirty(InternalSet::new_with_hash_builder(
				self.hash_builder.clone(),
			)));
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
		let delta = changeset.create_snapshot_and_get_delta();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta();
		assert!(delta.is_empty());
		let delta = changeset.create_snapshot_and_get_delta();
		assert!(delta.is_empty());
	}

	#[test]
	fn test_simple_snapshot() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let delta = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		changeset.add_key("c".to_string());
		let delta2 = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta2, ["c"]);
	}

	#[test]
	fn test_nested_tx_and_rollback() {
		let mut changeset = Changeset::new();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let d1 = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(d1, ["a", "b"]);
		changeset.add_key("c".to_string());
		changeset.start_transaction();
		changeset.add_key("e".to_string());
		changeset.start_transaction();
		changeset.add_key("f".to_string());
		let delta = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta, ["c", "e", "f"]);
		changeset.rollback_transaction();
		let delta2 = changeset.create_snapshot_and_get_delta();
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
		let delta = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta, ["a", "b", "c", "d", "e", "f"]);
		changeset.start_transaction();
		changeset.add_key("g".to_string());
		changeset.commit_transaction();
		changeset.add_key("h".to_string());
		changeset.commit_transaction();
		changeset.add_key("i".to_string());
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta, ["g", "h", "i",]);
	}

	#[test]
	fn test_commit_merges_dirty_keys() {
		let mut changeset = Changeset::new();
		changeset.add_key("x".to_string());
		changeset.start_transaction();
		changeset.add_key("y".to_string());
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta, ["x", "y"]);
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
		let delta = changeset.create_snapshot_and_get_delta();
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
					let delta = changeset.create_snapshot_and_get_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				changeset.rollback_transaction();
				changeset.add_key("d".to_string());
				let delta = changeset.create_snapshot_and_get_delta();
				delta_assert_eq!(delta, ["a", "b", "d"]);
				changeset.add_key("d0".to_string());
			}
			changeset.rollback_transaction();
			changeset.add_key("e".to_string());
		}
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta();
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
					let delta = changeset.create_snapshot_and_get_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				changeset.commit_transaction();
				changeset.add_key("d".to_string());
				let delta = changeset.create_snapshot_and_get_delta();
				delta_assert_eq!(delta, ["d"]);
				changeset.add_key("d0".to_string());
			}
			changeset.rollback_transaction();
			changeset.add_key("e".to_string());
		}
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta();
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
					let delta = changeset.create_snapshot_and_get_delta();
					delta_assert_eq!(delta, ["a", "b", "c"]);
				}
				changeset.commit_transaction();
				changeset.add_key("d".to_string());
				let delta = changeset.create_snapshot_and_get_delta();
				delta_assert_eq!(delta, ["d"]);
				changeset.add_key("d0".to_string());
			}
			changeset.rollback_transaction();
			changeset.add_key("e".to_string());
		}
		changeset.commit_transaction();
		let delta = changeset.create_snapshot_and_get_delta();
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
		let delta = changeset.create_snapshot_and_get_delta();
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
		let delta = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta, ["a", "b"]);
		changeset.commit_transaction();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		changeset.add_key("c".to_string());
		// let delta2 = changeset.create_snapshot_and_get_delta();
		// delta_assert_eq!(delta2, ["a", "b", "c"]);
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
		let delta = changeset.create_snapshot_and_get_delta();
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
		let delta = changeset.create_snapshot_and_get_delta();
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
		let delta = changeset.create_snapshot_and_get_delta();
		delta_assert_eq!(delta, ["a", "b", "c"]);
		changeset.start_transaction();
		changeset.add_key("d".to_string());
		changeset.add_key("e".to_string());
		changeset.add_key("f".to_string());
		let delta = changeset.create_snapshot_and_get_delta();
		changeset.start_transaction();
		changeset.add_key("a".to_string());
		changeset.add_key("b".to_string());
		let delta2 = changeset.create_snapshot_and_get_delta2();
		assert!(delta2.is_empty());
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

		changeset.start_transaction();
		changeset.add_key("e".to_string());
		changeset.add_key("f".to_string());
		debug!(target:LOG_TARGET, ">> before commit {:?}", changeset);
		changeset.commit_transaction();
		debug!(target:LOG_TARGET, ">> after commit {:?}", changeset);
		let delta2 = changeset.create_snapshot_and_get_delta2();
		debug!(target:LOG_TARGET, ">> after snap {:?}", changeset);
		changeset.start_transaction();
		changeset.commit_transaction();
		debug!(target:LOG_TARGET, ">> after final commit {:?}", changeset);

		// debug!(target:LOG_TARGET, "pre {:?}", changeset);
		//
		// debug!(target:LOG_TARGET, "final {:?}", changeset);
	}
}
