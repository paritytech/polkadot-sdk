//! Partial state keys tracking.

use codec::{Decode, Encode};
use hash_db::Hasher;
use parking_lot::Mutex;
use sc_state_db::CommitSet;
use sc_state_db::MetaDb;
use sp_runtime::traits::HashingFor;
use sp_trie::partial_state::Error as PartialStateError;
use sp_trie::partial_state::PartialState;
use sp_trie::partial_state::PartialStatePrefixerPerBlock;

type DbCommitSet = CommitSet<Vec<u8>>;

const PARTIAL_STATE_KEYS: &[u8] = b"partial_state_keys";

/// Tracks partial state keys written to database.
/// Tracking makes `import_partial_state` idempotent, preventing double increment if reference counting database is used.
///
/// In the beginnning, state root hash is only piece of information known.
/// When node with that hash is received, this node can be decoded to learn it's children node hashes.
/// This class stores list of not yet received nodes, starting from root node.
/// When node is received, it is removed from that list, and it's children are added to that list.
/// `import_partial_state` behaves like recursive coroutine, traversing trie, and waiting for nodes to be received.
pub struct PartialStateTracking<H: Hasher>
where
	H::Out: Decode + Encode,
{
	/// List of partial state keys not yet imported per block.
	prefixer_per_block: Mutex<PartialStatePrefixerPerBlock<H>>,
}

pub type PartialStateTrackingFor<B> = PartialStateTracking<HashingFor<B>>;

impl<H: Hasher> PartialStateTracking<H>
where
	H::Out: Decode + Encode,
{
	pub fn new<D: MetaDb>(db: D) -> sp_blockchain::Result<Self> {
		let prefixer_per_block = if let Some(encoded) = db.get_meta(&PARTIAL_STATE_KEYS).map_err(|e| sp_blockchain::Error::Storage(format!("{e:?}")))? {
			PartialStatePrefixerPerBlock::decode(&mut encoded.as_slice())
				.map_err(|e| sp_blockchain::Error::Storage(format!("{e:?}")))?
		} else {
			Default::default()
		};
		Ok(Self { prefixer_per_block: Mutex::new(prefixer_per_block) })
	}

	fn write_partial_state_keys(prefixer_per_block: &mut PartialStatePrefixerPerBlock<H>, commit: &mut DbCommitSet) {
		commit.meta.inserted.push((
			PARTIAL_STATE_KEYS.to_vec(),
			prefixer_per_block.encode(),
		));
	}

	/// Inject partial state into the database.
	/// State sync receives subset of trie nodes and uses `import_partial_state` to write them to database.
	/// After downloading all trie nodes it calls `set_partial_state_completed` to mark completely donwloaded state.
	/// Block hash is passed to remember partial state belonging to that block,
	/// to avoid inserting node second time (may break reference counting),
	/// and to allow cleaning up incomplete partial state for that block.
	pub fn import_partial_state<BlockNumber: Default>(&self, partial_state: PartialState<H, BlockNumber>) -> Result<DbCommitSet, PartialStateError> {
		let mut prefixer_per_block = self.prefixer_per_block.lock();
		let mut prefixed = prefixer_per_block.import(partial_state)?;
		let mut commit = CommitSet::default();
		for (key, (value, _)) in prefixed.drain() {
			commit.data.inserted.push((key, value));
		}
		if !commit.data.inserted.is_empty() {
			Self::write_partial_state_keys(&mut prefixer_per_block, &mut commit);
		}
		Ok(commit)
	}

	/// Remove partial state keys used for deduplication after completing state sync.
	pub fn remove_completed_partial_state(&self, block_hash: &H::Out) -> DbCommitSet {
		let mut prefixer_per_block = self.prefixer_per_block.lock();
		let mut commit = CommitSet::default();
		if prefixer_per_block.0.remove(block_hash).is_some() {
			Self::write_partial_state_keys(&mut prefixer_per_block, &mut commit);
		}
		commit
	}
}
