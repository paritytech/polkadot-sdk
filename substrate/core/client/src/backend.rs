// Copyright 2017 Parity Technologies (UK) Ltd.
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

//! Polkadot Client data backend

use error;
use primitives::AuthorityId;
use runtime_primitives::bft::Justification;
use runtime_primitives::generic::BlockId;
use runtime_primitives::traits::{Block as BlockT, NumberFor};
use state_machine::backend::Backend as StateBackend;
use state_machine::ChangesTrieStorage as StateChangesTrieStorage;
use patricia_trie::NodeCodec;
use hashdb::Hasher;
use memorydb::MemoryDB;

/// Block insertion operation. Keeps hold if the inserted block state and data.
pub trait BlockImportOperation<Block, H, C>
where
	Block: BlockT,
	H: Hasher,
	C: NodeCodec<H>,
{
	/// Associated state backend type.
	type State: StateBackend<H, C>;

	/// Returns pending state. Returns None for backends with locally-unavailable state data.
	fn state(&self) -> error::Result<Option<&Self::State>>;
	/// Append block data to the transaction.
	fn set_block_data(
		&mut self,
		header: Block::Header,
		body: Option<Vec<Block::Extrinsic>>,
		justification: Option<Justification<Block::Hash>>,
		is_new_best: bool
	) -> error::Result<()>;

	/// Append authorities set to the transaction. This is a set of parent block (set which
	/// has been used to check justification of this block).
	fn update_authorities(&mut self, authorities: Vec<AuthorityId>);
	/// Inject storage data into the database.
	fn update_storage(&mut self, update: <Self::State as StateBackend<H, C>>::Transaction) -> error::Result<()>;
	/// Inject storage data into the database replacing any existing data.
	fn reset_storage<I: Iterator<Item=(Vec<u8>, Vec<u8>)>>(&mut self, iter: I) -> error::Result<()>;
	/// Inject changes trie data into the database.
	fn update_changes_trie(&mut self, update: MemoryDB<H>) -> error::Result<()>;
}

/// Client backend. Manages the data layer.
///
/// Note on state pruning: while an object from `state_at` is alive, the state
/// should not be pruned. The backend should internally reference-count
/// its state objects.
///
/// The same applies for live `BlockImportOperation`s: while an import operation building on a parent `P`
/// is alive, the state for `P` should not be pruned.
pub trait Backend<Block, H, C>: Send + Sync
where
	Block: BlockT,
	H: Hasher,
	C: NodeCodec<H>,
{
	/// Associated block insertion operation type.
	type BlockImportOperation: BlockImportOperation<Block, H, C>;
	/// Associated blockchain backend type.
	type Blockchain: ::blockchain::Backend<Block>;
	/// Associated state backend type.
	type State: StateBackend<H, C>;
	/// Changes trie storage.
	type ChangesTrieStorage: StateChangesTrieStorage<H>;

	/// Begin a new block insertion transaction with given parent block id.
	/// When constructing the genesis, this is called with all-zero hash.
	fn begin_operation(&self, block: BlockId<Block>) -> error::Result<Self::BlockImportOperation>;
	/// Commit block insertion.
	fn commit_operation(&self, transaction: Self::BlockImportOperation) -> error::Result<()>;
	/// Returns reference to blockchain backend.
	fn blockchain(&self) -> &Self::Blockchain;
	/// Returns reference to changes trie storage.
	fn changes_trie_storage(&self) -> Option<&Self::ChangesTrieStorage>;
	/// Returns state backend with post-state of given block.
	fn state_at(&self, block: BlockId<Block>) -> error::Result<Self::State>;
	/// Attempts to revert the chain by `n` blocks. Returns the number of blocks that were
	/// successfully reverted.
	fn revert(&self, n: NumberFor<Block>) -> error::Result<NumberFor<Block>>;
}

/// Mark for all Backend implementations, that are making use of state data, stored locally.
pub trait LocalBackend<Block, H, C>: Backend<Block, H, C>
where
	Block: BlockT,
	H: Hasher,
	C: NodeCodec<H>,
{}

/// Mark for all Backend implementations, that are fetching required state data from remote nodes.
pub trait RemoteBackend<Block, H, C>: Backend<Block, H, C>
where
	Block: BlockT,
	H: Hasher,
	C: NodeCodec<H>,
{}
