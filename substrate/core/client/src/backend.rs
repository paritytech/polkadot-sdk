// Copyright 2017-2018 Parity Technologies (UK) Ltd.
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
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Substrate Client data backend

use crate::error;
use primitives::ChangesTrieConfiguration;
use runtime_primitives::{generic::BlockId, Justification, StorageMap, ChildrenStorageMap};
use runtime_primitives::traits::{AuthorityIdFor, Block as BlockT, NumberFor};
use state_machine::backend::Backend as StateBackend;
use state_machine::ChangesTrieStorage as StateChangesTrieStorage;
use hash_db::Hasher;
use trie::MemoryDB;

/// State of a new block.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NewBlockState {
	/// Normal block.
	Normal,
	/// New best block.
	Best,
	/// Newly finalized block (implicitly best).
	Final,
}

impl NewBlockState {
	/// Whether this block is the new best block.
	pub fn is_best(self) -> bool {
		match self {
			NewBlockState::Best | NewBlockState::Final => true,
			NewBlockState::Normal => false,
		}
	}
}

/// Block insertion operation. Keeps hold if the inserted block state and data.
pub trait BlockImportOperation<Block, H> where
	Block: BlockT,
	H: Hasher<Out=Block::Hash>,
{
	/// Associated state backend type.
	type State: StateBackend<H>;

	/// Returns pending state. Returns None for backends with locally-unavailable state data.
	fn state(&self) -> error::Result<Option<&Self::State>>;
	/// Append block data to the transaction.
	fn set_block_data(
		&mut self,
		header: Block::Header,
		body: Option<Vec<Block::Extrinsic>>,
		justification: Option<Justification>,
		state: NewBlockState,
	) -> error::Result<()>;

	/// Append authorities set to the transaction. This is a set of parent block (set which
	/// has been used to check justification of this block).
	fn update_authorities(&mut self, authorities: Vec<AuthorityIdFor<Block>>);
	/// Inject storage data into the database.
	fn update_db_storage(&mut self, update: <Self::State as StateBackend<H>>::Transaction) -> error::Result<()>;
	/// Inject storage data into the database replacing any existing data.
	fn reset_storage(&mut self, top: StorageMap, children: ChildrenStorageMap) -> error::Result<H::Out>;
	/// Set top level storage changes.
	fn update_storage(&mut self, update: Vec<(Vec<u8>, Option<Vec<u8>>)>) -> error::Result<()>;
	/// Inject changes trie data into the database.
	fn update_changes_trie(&mut self, update: MemoryDB<H>) -> error::Result<()>;
	/// Insert auxiliary keys. Values are `None` if should be deleted.
	fn insert_aux<I>(&mut self, ops: I) -> error::Result<()>
		where I: IntoIterator<Item=(Vec<u8>, Option<Vec<u8>>)>;
	/// Mark a block as finalized.
	fn mark_finalized(&mut self, id: BlockId<Block>, justification: Option<Justification>) -> error::Result<()>;
}

/// Provides access to an auxiliary database.
pub trait AuxStore {
	/// Insert auxiliary data into key-value store. Deletions occur after insertions.
	fn insert_aux<
		'a,
		'b: 'a,
		'c: 'a,
		I: IntoIterator<Item=&'a(&'c [u8], &'c [u8])>,
		D: IntoIterator<Item=&'a &'b [u8]>,
	>(&self, insert: I, delete: D) -> error::Result<()>;
	/// Query auxiliary data from key-value store.
	fn get_aux(&self, key: &[u8]) -> error::Result<Option<Vec<u8>>>;
}

/// Client backend. Manages the data layer.
///
/// Note on state pruning: while an object from `state_at` is alive, the state
/// should not be pruned. The backend should internally reference-count
/// its state objects.
///
/// The same applies for live `BlockImportOperation`s: while an import operation building on a parent `P`
/// is alive, the state for `P` should not be pruned.
pub trait Backend<Block, H>: AuxStore + Send + Sync where
	Block: BlockT,
	H: Hasher<Out=Block::Hash>,
{
	/// Associated block insertion operation type.
	type BlockImportOperation: BlockImportOperation<Block, H, State=Self::State>;
	/// Associated blockchain backend type.
	type Blockchain: crate::blockchain::Backend<Block>;
	/// Associated state backend type.
	type State: StateBackend<H>;
	/// Changes trie storage.
	type ChangesTrieStorage: PrunableStateChangesTrieStorage<H>;

	/// Begin a new block insertion transaction with given parent block id.
	/// When constructing the genesis, this is called with all-zero hash.
	fn begin_operation(&self) -> error::Result<Self::BlockImportOperation>;
	/// Note an operation to contain state transition.
	fn begin_state_operation(&self, operation: &mut Self::BlockImportOperation, block: BlockId<Block>) -> error::Result<()>;
	/// Commit block insertion.
	fn commit_operation(&self, transaction: Self::BlockImportOperation) -> error::Result<()>;
	/// Finalize block with given Id. This should only be called if the parent of the given
	/// block has been finalized.
	fn finalize_block(&self, block: BlockId<Block>, justification: Option<Justification>) -> error::Result<()>;
	/// Returns reference to blockchain backend.
	fn blockchain(&self) -> &Self::Blockchain;
	/// Returns reference to changes trie storage.
	fn changes_trie_storage(&self) -> Option<&Self::ChangesTrieStorage>;
	/// Returns state backend with post-state of given block.
	fn state_at(&self, block: BlockId<Block>) -> error::Result<Self::State>;
	/// Destroy state and save any useful data, such as cache.
	fn destroy_state(&self, _state: Self::State) -> error::Result<()> {
		Ok(())
	}
	/// Attempts to revert the chain by `n` blocks. Returns the number of blocks that were
	/// successfully reverted.
	fn revert(&self, n: NumberFor<Block>) -> error::Result<NumberFor<Block>>;

	/// Insert auxiliary data into key-value store.
	fn insert_aux<
		'a,
		'b: 'a,
		'c: 'a,
		I: IntoIterator<Item=&'a(&'c [u8], &'c [u8])>,
		D: IntoIterator<Item=&'a &'b [u8]>,
	>(&self, insert: I, delete: D) -> error::Result<()>
	{
		AuxStore::insert_aux(self, insert, delete)
	}
	/// Query auxiliary data from key-value store.
	fn get_aux(&self, key: &[u8]) -> error::Result<Option<Vec<u8>>> {
		AuxStore::get_aux(self, key)
	}
}

/// Changes trie storage that supports pruning.
pub trait PrunableStateChangesTrieStorage<H: Hasher>: StateChangesTrieStorage<H> {
	/// Get number block of oldest, non-pruned changes trie.
	fn oldest_changes_trie_block(&self, config: &ChangesTrieConfiguration, best_finalized: u64) -> u64;
}

/// Mark for all Backend implementations, that are making use of state data, stored locally.
pub trait LocalBackend<Block, H>: Backend<Block, H>
where
	Block: BlockT,
	H: Hasher<Out=Block::Hash>,
{}

/// Mark for all Backend implementations, that are fetching required state data from remote nodes.
pub trait RemoteBackend<Block, H>: Backend<Block, H>
where
	Block: BlockT,
	H: Hasher<Out=Block::Hash>,
{}
