//! Partial state keys tracking.

use codec::{Decode, Encode};
use hash_db::Hasher;
use parking_lot::Mutex;
use sc_state_db::CommitSet;
use sc_state_db::MetaDb;
use sp_runtime::traits::HashingFor;
use sp_trie::partial_state::PartialState;
use std::collections::HashMap;
use std::collections::HashSet;

type Key = Vec<u8>;
type Keys<H> = HashMap<<H as Hasher>::Out, HashSet<Key>>;

/// List of partial state keys already imported per block.
/// By default ParityDB doesn't support iterating db keys unless `btree_index` is set for column,
/// so storing list of partial state keys under single meta key.
const PARTIAL_STATE_KEYS: &[u8] = b"partial_state_keys";

pub struct PartialStateTracking<H: Hasher>
where
  H::Out: Decode + Encode,
{
	/// List of partial state keys already imported per block.
	partial_state_keys: Mutex<Keys<H>>,
}

pub type PartialStateTrackingFor<B> = PartialStateTracking<HashingFor<B>>;

impl<H: Hasher> PartialStateTracking<H>
where
  H::Out: Decode + Encode,
{
	pub fn new<D: MetaDb>(db: D) -> sp_blockchain::Result<Self> {
		let partial_state_keys = if let Some(encoded) = db.get_meta(&PARTIAL_STATE_KEYS).map_err(|e| sp_blockchain::Error::Storage(format!("{e:?}")))? {
			<Vec<(H::Out, Vec<Key>)> as Decode>::decode(&mut encoded.as_slice())
        .map_err(|e| sp_blockchain::Error::Storage(format!("{e:?}")))?
				.into_iter()
				.map(|(block_hash, keys)| (block_hash, keys.into_iter().collect()))
				.collect()
		} else {
			Default::default()
		};
		Ok(Self { partial_state_keys: Mutex::new(partial_state_keys) })
	}

	fn write_partial_state_keys(partial_state_keys: &mut Keys<H>, commit: &mut CommitSet<Key>) {
		commit.meta.inserted.push((
			PARTIAL_STATE_KEYS.to_vec(),
			partial_state_keys
				.iter()
				.map(|(block_hash, keys)| (block_hash, keys.iter().collect::<Vec<_>>()))
				.collect::<Vec<_>>()
				.encode(),
		));
	}

	/// Inject partial state into the database.
	/// State sync receives subset of trie nodes and uses `import_partial_state` to write them to database.
	/// After downloading all trie nodes it calls `set_partial_state_completed` to mark completely donwloaded state.
	/// Block hash is passed to remember partial state belonging to that block,
	/// to avoid inserting node second time (may break reference counting),
	/// and to allow cleaning up incomplete partial state for that block.
	pub fn import_partial_state<BlockNumber>(&self, mut partial_state: PartialState<H, BlockNumber>) -> CommitSet<Key> {
    let mut partial_state_keys = self.partial_state_keys.lock();
		let mut commit = CommitSet::default();
		let keys = partial_state_keys.entry(partial_state.block_hash.clone()).or_default();
		for (key, (value, _)) in partial_state.nodes.drain() {
			if keys.contains(&key) {
				continue;
			}
			keys.insert(key.clone());
			commit.data.inserted.push((key, value));
		}
		if !commit.data.inserted.is_empty() {
			Self::write_partial_state_keys(&mut partial_state_keys, &mut commit);
		}
		commit
	}

	/// Remove partial state keys used for deduplication after completing state sync.
	pub fn remove_completed_partial_state(&self, block_hash: &H::Out) -> CommitSet<Key> {
    let mut partial_state_keys = self.partial_state_keys.lock();
		let mut commit = CommitSet::default();
		if partial_state_keys.remove(block_hash).is_some() {
			Self::write_partial_state_keys(&mut partial_state_keys, &mut commit);
		}
		commit
	}
}
