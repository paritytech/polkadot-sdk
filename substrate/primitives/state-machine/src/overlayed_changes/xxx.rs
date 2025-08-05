use alloc::{vec, vec::Vec};
use core::{cmp::Eq, hash::Hash};

#[cfg(not(feature = "std"))]
use alloc::collections::btree_set::BTreeSet as Set;
#[cfg(feature = "std")]
use std::collections::HashSet as Set;

#[derive(Debug, Clone)]
enum TransactionKeys<K> {
	Dirty(Set<K>),
	Snapshot(Set<K>),
}

impl<K> TransactionKeys<K> {
	fn is_empty(&self) -> bool {
		match self {
			TransactionKeys::Dirty(k) | TransactionKeys::Snapshot(k) => k.is_empty(),
		}
	}
	fn keys(&self) -> &Set<K> {
		match self {
			TransactionKeys::Dirty(k) | TransactionKeys::Snapshot(k) => k,
		}
	}
}

impl<K> Default for TransactionKeys<K> {
	fn default() -> Self {
		TransactionKeys::Dirty(Default::default())
	}
}

type Transaction<K> = Vec<TransactionKeys<K>>;

#[derive(Debug, Clone)]
pub struct Changeset<K> {
	// Stack of transactions.
	transactions: Vec<Transaction<K>>,
}

impl<K> Default for Changeset<K> {
	fn default() -> Self {
		Self { transactions: Default::default() }
	}
}

impl<K: Clone + Hash + Eq + Ord> Changeset<K> {
	pub fn new() -> Self {
		// Initialize with one transaction having a single empty dirty set
		Changeset { transactions: vec![vec![Default::default()]] }
	}

	pub fn add_key(&mut self, key: K) {
		if let Some(transaction) = self.transactions.last_mut() {
			match transaction.last_mut() {
				Some(TransactionKeys::Dirty(dirty_set)) => {
					dirty_set.insert(key);
				},
				Some(TransactionKeys::Snapshot(_)) | None => {
					transaction.push(TransactionKeys::Dirty(Set::from([key])));
				},
			}
		}
	}

	pub fn start_transaction(&mut self) {
		self.transactions.push(vec![TransactionKeys::Dirty(Set::new())]);
	}

	pub fn commit_transaction(&mut self) {
		if self.transactions.len() <= 1 {
			return;
		}

		let mut commited = self.transactions.pop().expect("there is at leas one transactions. qed");
		debug_assert!(
			commited.iter().filter(|k| matches!(k, TransactionKeys::Dirty(_))).count() <= 1
		);

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

	pub fn create_snapshot_and_get_delta(&mut self) -> Set<K> {
		// Gather dirty keys from all transactions from newest to oldest, stopping at last snapshot
		// found
		let mut delta = Set::new();

		'outer: for transaction in self.transactions.iter().rev() {
			// Process keys in reverse to find last snapshot and collect dirty keys
			for keys in transaction.iter().rev() {
				match keys {
					TransactionKeys::Dirty(set) => {
						// Accumulate live dirty keys
						delta.extend(set.clone());
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
			top_transaction
				.last_mut()
				.map(|stage| *stage = TransactionKeys::Snapshot(delta.clone()));
			top_transaction.push(TransactionKeys::Dirty(Set::new()));
		}
		delta
	}
}

// aka: Snapshot?
pub trait BackendTransaction {
	fn consolidate(&mut self, other: Self);
}

#[derive(Debug, Clone)]
pub struct BackendSnapshots<T: BackendTransaction> {
	// Stack of BackendTransactions.
	//
	// For every single storage transcation, multiple backend transactions are consolidated.
	// When storage transaction is rolled back, snapshto
	transactions: Vec<Option<T>>,
}

impl<T: BackendTransaction> Default for BackendSnapshots<T> {
	fn default() -> Self {
		// Initialize with root storage transaction having no backend transaction
		Self { transactions: vec![None] }
	}
}

impl<T: BackendTransaction> BackendSnapshots<T> {
	pub fn new() -> Self {
		Self::default()
	}

	pub fn start_transaction(&mut self) {
		self.transactions.push(None);
	}

	pub fn commit_transaction(&mut self) {
		if self.transactions.len() <= 1 {
			return;
		}

		let commited = self.transactions.pop().expect("there is at least one item, qed.");

		if let Some(commited) = commited {
			let recent = self.transactions.last_mut().expect("there is at least one item, qed.");
			if recent.is_some() {
				recent.as_mut().map(|r| r.consolidate(commited));
			} else {
				*recent = Some(commited);
			}
		}
	}

	pub fn rollback_transaction(&mut self) {
		if self.transactions.len() <= 1 {
			return;
		}
		self.transactions.pop();
	}

	//better name: maybe consolidate?
	pub fn push(&mut self, pushed: T) {
		if let Some(recent) = self.transactions.last_mut() {
			if recent.is_some() {
				recent.as_mut().map(|r| r.consolidate(pushed));
			} else {
				*recent = Some(pushed);
			}
		}
	}
}

impl<T: BackendTransaction + Clone> BackendSnapshots<T> {
	//better name: take last snapshot ?
	pub fn pop(&mut self) -> Option<T> {
		self.transactions
			.iter()
			.cloned()
			.reduce(|a, i| match (a, i) {
				(Some(a), None) => Some(a),
				(Some(mut a), Some(i)) => {
					a.consolidate(i);
					Some(a)
				},
				(None, Some(i)) => Some(i),
				(None, None) => None,
			})
			.expect("there is at least one root element")
	}
}

#[cfg(test)]
mod tests {
	use super::BackendTransaction;

	macro_rules! delta_assert_eq {
        ($delta:expr, [$($val:expr),* $(,)?]) => {
            {
                let expected: ::std::collections::Set<String> =
                    [$($val),*].iter().cloned().map(String::from).collect();
                assert_eq!($delta, expected);
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
}

#[cfg(test)]
mod tests2 {
	use super::{BackendSnapshots, BackendTransaction};

	type Snapshots = BackendSnapshots<String>;
	impl BackendTransaction for String {
		fn consolidate(&mut self, other: Self) {
			*self += &other
		}
	}

	#[test]
	fn test_basic() {
		let mut snapshots = Snapshots::new();
		assert_eq!(snapshots.pop(), None);
		snapshots.push("x".to_string());
		assert_eq!(snapshots.pop(), Some("x".to_string()));
	}

	#[test]
	fn test_consolidate() {
		let mut snapshots = Snapshots::new();
		snapshots.push("x".to_string());
		snapshots.push("y".to_string());
		assert_eq!(snapshots.pop(), Some("xy".to_string()));
	}

	#[test]
	fn test_commit() {
		let mut snapshots = Snapshots::new();
		snapshots.pop();
		snapshots.push("x".to_string());
		snapshots.push("y".to_string());
		snapshots.start_transaction();
		snapshots.push("z".to_string());
		snapshots.commit_transaction();
		assert_eq!(snapshots.pop(), Some("xyz".to_string()));
	}

	#[test]
	fn test_rollback() {
		let mut snapshots = Snapshots::new();
		snapshots.pop();
		snapshots.push("x".to_string());
		snapshots.push("y".to_string());
		snapshots.start_transaction();
		snapshots.push("z".to_string());
		snapshots.rollback_transaction();
		assert_eq!(snapshots.pop(), Some("xy".to_string()));
	}
	#[test]
	fn test_nested01() {
		let mut snapshots = Snapshots::new();
		snapshots.pop();
		snapshots.push("a".to_string());
		snapshots.push("b".to_string());
		snapshots.start_transaction();
		{
			snapshots.start_transaction();
			{
				snapshots.push("c".to_string());
				snapshots.start_transaction();
				{
					snapshots.push("d".to_string());
				}
				snapshots.rollback_transaction();
			}
			snapshots.rollback_transaction();
			snapshots.push("e".to_string());
		}
		snapshots.rollback_transaction();
		assert_eq!(snapshots.pop(), Some("ab".to_string()));
	}

	#[test]
	fn test_nested02() {
		let mut snapshots = Snapshots::new();
		snapshots.pop();
		snapshots.push("a".to_string());
		snapshots.push("b".to_string());
		snapshots.start_transaction();
		{
			snapshots.start_transaction();
			{
				snapshots.push("c".to_string());
				snapshots.start_transaction();
				{
					snapshots.push("d".to_string());
				}
				snapshots.commit_transaction();
				assert_eq!(snapshots.pop(), Some("abcd".to_string()));
			}
			snapshots.rollback_transaction();
			snapshots.push("e".to_string());
		}
		snapshots.rollback_transaction();
		assert_eq!(snapshots.pop(), Some("ab".to_string()));
	}

	#[test]
	fn test_nested03() {
		let mut snapshots = Snapshots::new();
		snapshots.pop();
		snapshots.push("a".to_string());
		snapshots.push("b".to_string());
		snapshots.start_transaction();
		{
			snapshots.start_transaction();
			{
				snapshots.push("c".to_string());
				snapshots.start_transaction();
				{
					snapshots.push("d".to_string());
				}
				snapshots.commit_transaction();
				assert_eq!(snapshots.pop(), Some("abcd".to_string()));
			}
			snapshots.commit_transaction();
			snapshots.push("e".to_string());
		}
		snapshots.commit_transaction();
		assert_eq!(snapshots.pop(), Some("abcde".to_string()));
	}
}
