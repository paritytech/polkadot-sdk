//! Partial state keys tracking.

use codec::{Decode, Encode};
use hash_db::{Hasher, Prefix};
use parking_lot::Mutex;
use sc_state_db::{CommitSet, MetaDb};
use sp_runtime::traits::HashingFor;
use sp_trie::partial_state::{PartialState, PartialStatePrefixerPerBlock};

type DbCommitSet = CommitSet<Vec<u8>>;

const PARTIAL_STATE_KEYS: &[u8] = b"partial_state_keys";

/// Tracks partial state keys written to database.
/// Tracking makes `import_partial_state` idempotent, preventing double increment if reference
/// counting database is used.
///
/// In the beginnning, state root hash is only piece of information known.
/// When node with that hash is received, this node can be decoded to learn it's children node
/// hashes. This class stores list of not yet received nodes, starting from root node.
/// When node is received, it is removed from that list, and it's children are added to that list.
/// `import_partial_state` behaves like recursive coroutine, traversing trie, and waiting for nodes
/// to be received.
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
		let prefixer_per_block = if let Some(encoded) = db
			.get_meta(&PARTIAL_STATE_KEYS)
			.map_err(|e| sp_blockchain::Error::Storage(format!("{e:?}")))?
		{
			PartialStatePrefixerPerBlock::decode(&mut encoded.as_slice())
				.map_err(|e| sp_blockchain::Error::Storage(format!("{e:?}")))?
		} else {
			Default::default()
		};
		Ok(Self { prefixer_per_block: Mutex::new(prefixer_per_block) })
	}

	fn write_partial_state_keys(
		prefixer_per_block: &mut PartialStatePrefixerPerBlock<H>,
		commit: &mut DbCommitSet,
	) {
		commit
			.meta
			.inserted
			.push((PARTIAL_STATE_KEYS.to_vec(), prefixer_per_block.encode()));
	}

	/// Inject partial state into the database.
	/// State sync receives subset of trie nodes and uses `import_partial_state` to write them to
	/// database. After downloading all trie nodes it calls `set_partial_state_completed` to mark
	/// completely downloaded state. Block hash is passed to remember partial state belonging to
	/// that block, to avoid inserting node second time (may break reference counting),
	/// and to allow cleaning up incomplete partial state for that block.
	///
	/// `state_key` turns a node's hash and database prefix into its key in the `STATE` column; the
	/// backend passes the same rule it uses for reads. `commit` persists the resulting changes. It
	/// runs under the tracker lock, and if the import or the commit fails the block's walker is
	/// rolled back to what it was before this call, so the same chunk can be submitted again and
	/// its nodes are written then.
	pub fn import_partial_state<BlockNumber: Default>(
		&self,
		partial_state: PartialState<H, BlockNumber>,
		state_key: impl FnMut(&H::Out, Prefix) -> Vec<u8>,
		commit: impl FnOnce(DbCommitSet) -> sp_blockchain::Result<()>,
	) -> sp_blockchain::Result<()> {
		let mut prefixer_per_block = self.prefixer_per_block.lock();

		// Snapshot the block's walker, and roll it back if anything fails along the way.
		let snapshot = prefixer_per_block.snapshot(&partial_state.block_hash);
		let result =
			Self::import_and_commit(&mut prefixer_per_block, partial_state, state_key, commit);
		if result.is_err() {
			prefixer_per_block.rollback(snapshot);
		}
		result
	}

	/// Advance the block's walker with `partial_state` and persist the nodes it released.
	fn import_and_commit<BlockNumber: Default>(
		prefixer_per_block: &mut PartialStatePrefixerPerBlock<H>,
		partial_state: PartialState<H, BlockNumber>,
		mut state_key: impl FnMut(&H::Out, Prefix) -> Vec<u8>,
		commit: impl FnOnce(DbCommitSet) -> sp_blockchain::Result<()>,
	) -> sp_blockchain::Result<()> {
		let nodes = prefixer_per_block
			.import(partial_state)
			.map_err(|e| sp_blockchain::Error::Storage(format!("{e:?}")))?;
		if nodes.is_empty() {
			return Ok(());
		}
		let mut commit_set = CommitSet::default();
		commit_set.data.inserted.reserve(nodes.len());
		for node in nodes {
			commit_set
				.data
				.inserted
				.push((state_key(&node.hash, node.prefix.as_prefix()), node.encoded));
		}
		Self::write_partial_state_keys(prefixer_per_block, &mut commit_set);
		commit(commit_set)
	}

	/// Remove partial state keys used for deduplication after completing state sync.
	pub fn remove_completed_partial_state(&self, block_hash: &H::Out) -> DbCommitSet {
		let mut prefixer_per_block = self.prefixer_per_block.lock();
		let mut commit = CommitSet::default();
		if prefixer_per_block.remove(block_hash) {
			Self::write_partial_state_keys(&mut prefixer_per_block, &mut commit);
		}
		commit
	}
}
