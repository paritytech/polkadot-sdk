// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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

//! Substrate light client interfaces

use std::sync::Arc;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;

use sp_runtime::{
	traits::{
		Block as BlockT, Header as HeaderT, NumberFor,
	},
	generic::BlockId
};
use sp_core::ChangesTrieConfigurationRange;
use sp_state_machine::StorageProof;
use sp_blockchain::{
	HeaderMetadata, well_known_cache_keys, HeaderBackend, Cache as BlockchainCache,
	Error as ClientError, Result as ClientResult,
};
use crate::{backend::{AuxStore, NewBlockState}, UsageInfo};

/// Remote call request.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RemoteCallRequest<Header: HeaderT> {
	/// Call at state of given block.
	pub block: Header::Hash,
	/// Header of block at which call is performed.
	pub header: Header,
	/// Method to call.
	pub method: String,
	/// Call data.
	pub call_data: Vec<u8>,
	/// Number of times to retry request. None means that default RETRY_COUNT is used.
	pub retry_count: Option<usize>,
}

/// Remote canonical header request.
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct RemoteHeaderRequest<Header: HeaderT> {
	/// The root of CHT this block is included in.
	pub cht_root: Header::Hash,
	/// Number of the header to query.
	pub block: Header::Number,
	/// Number of times to retry request. None means that default RETRY_COUNT is used.
	pub retry_count: Option<usize>,
}

/// Remote storage read request.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RemoteReadRequest<Header: HeaderT> {
	/// Read at state of given block.
	pub block: Header::Hash,
	/// Header of block at which read is performed.
	pub header: Header,
	/// Storage key to read.
	pub keys: Vec<Vec<u8>>,
	/// Number of times to retry request. None means that default RETRY_COUNT is used.
	pub retry_count: Option<usize>,
}

/// Remote storage read child request.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RemoteReadChildRequest<Header: HeaderT> {
	/// Read at state of given block.
	pub block: Header::Hash,
	/// Header of block at which read is performed.
	pub header: Header,
	/// Storage key for child.
	pub storage_key: Vec<u8>,
	/// Child trie source information.
	pub child_info: Vec<u8>,
	/// Child type, its required to resolve `child_info`
	/// content and choose child implementation.
	pub child_type: u32,
	/// Child storage key to read.
	pub keys: Vec<Vec<u8>>,
	/// Number of times to retry request. None means that default RETRY_COUNT is used.
	pub retry_count: Option<usize>,
}

/// Remote key changes read request.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RemoteChangesRequest<Header: HeaderT> {
	/// All changes trie configurations that are valid within [first_block; last_block].
	pub changes_trie_configs: Vec<ChangesTrieConfigurationRange<Header::Number, Header::Hash>>,
	/// Query changes from range of blocks, starting (and including) with this hash...
	pub first_block: (Header::Number, Header::Hash),
	/// ...ending (and including) with this hash. Should come after first_block and
	/// be the part of the same fork.
	pub last_block: (Header::Number, Header::Hash),
	/// Only use digests from blocks up to this hash. Should be last_block OR come
	/// after this block and be the part of the same fork.
	pub max_block: (Header::Number, Header::Hash),
	/// Known changes trie roots for the range of blocks [tries_roots.0..max_block].
	/// Proofs for roots of ascendants of tries_roots.0 are provided by the remote node.
	pub tries_roots: (Header::Number, Header::Hash, Vec<Header::Hash>),
	/// Optional Child Storage key to read.
	pub storage_key: Option<Vec<u8>>,
	/// Storage key to read.
	pub key: Vec<u8>,
	/// Number of times to retry request. None means that default RETRY_COUNT is used.
	pub retry_count: Option<usize>,
}

/// Key changes read proof.
#[derive(Debug, PartialEq, Eq)]
pub struct ChangesProof<Header: HeaderT> {
	/// Max block that has been used in changes query.
	pub max_block: Header::Number,
	/// All touched nodes of all changes tries.
	pub proof: Vec<Vec<u8>>,
	/// All changes tries roots that have been touched AND are missing from
	/// the requester' node. It is a map of block number => changes trie root.
	pub roots: BTreeMap<Header::Number, Header::Hash>,
	/// The proofs for all changes tries roots that have been touched AND are
	/// missing from the requester' node. It is a map of CHT number => proof.
	pub roots_proof: StorageProof,
}

/// Remote block body request
#[derive(Clone, Default, Debug, PartialEq, Eq, Hash)]
pub struct RemoteBodyRequest<Header: HeaderT> {
	/// Header of the requested block body
	pub header: Header,
	/// Number of times to retry request. None means that default RETRY_COUNT is used.
	pub retry_count: Option<usize>,
}

/// Light client data fetcher. Implementations of this trait must check if remote data
/// is correct (see FetchedDataChecker) and return already checked data.
pub trait Fetcher<Block: BlockT>: Send + Sync {
	/// Remote header future.
	type RemoteHeaderResult: Future<Output = Result<
		Block::Header,
		ClientError,
	>> + Unpin + Send + 'static;
	/// Remote storage read future.
	type RemoteReadResult: Future<Output = Result<
		HashMap<Vec<u8>, Option<Vec<u8>>>,
		ClientError,
	>> + Unpin + Send + 'static;
	/// Remote call result future.
	type RemoteCallResult: Future<Output = Result<
		Vec<u8>,
		ClientError,
	>> + Unpin + Send + 'static;
	/// Remote changes result future.
	type RemoteChangesResult: Future<Output = Result<
		Vec<(NumberFor<Block>, u32)>,
		ClientError,
	>> + Unpin + Send + 'static;
	/// Remote block body result future.
	type RemoteBodyResult: Future<Output = Result<
		Vec<Block::Extrinsic>,
		ClientError,
	>> + Unpin + Send + 'static;

	/// Fetch remote header.
	fn remote_header(&self, request: RemoteHeaderRequest<Block::Header>) -> Self::RemoteHeaderResult;
	/// Fetch remote storage value.
	fn remote_read(
		&self,
		request: RemoteReadRequest<Block::Header>
	) -> Self::RemoteReadResult;
	/// Fetch remote storage child value.
	fn remote_read_child(
		&self,
		request: RemoteReadChildRequest<Block::Header>
	) -> Self::RemoteReadResult;
	/// Fetch remote call result.
	fn remote_call(&self, request: RemoteCallRequest<Block::Header>) -> Self::RemoteCallResult;
	/// Fetch remote changes ((block number, extrinsic index)) where given key has been changed
	/// at a given blocks range.
	fn remote_changes(&self, request: RemoteChangesRequest<Block::Header>) -> Self::RemoteChangesResult;
	/// Fetch remote block body
	fn remote_body(&self, request: RemoteBodyRequest<Block::Header>) -> Self::RemoteBodyResult;
}

/// Light client remote data checker.
///
/// Implementations of this trait should not use any prunable blockchain data
/// except that is passed to its methods.
pub trait FetchChecker<Block: BlockT>: Send + Sync {
	/// Check remote header proof.
	fn check_header_proof(
		&self,
		request: &RemoteHeaderRequest<Block::Header>,
		header: Option<Block::Header>,
		remote_proof: StorageProof,
	) -> ClientResult<Block::Header>;
	/// Check remote storage read proof.
	fn check_read_proof(
		&self,
		request: &RemoteReadRequest<Block::Header>,
		remote_proof: StorageProof,
	) -> ClientResult<HashMap<Vec<u8>, Option<Vec<u8>>>>;
	/// Check remote storage read proof.
	fn check_read_child_proof(
		&self,
		request: &RemoteReadChildRequest<Block::Header>,
		remote_proof: StorageProof,
	) -> ClientResult<HashMap<Vec<u8>, Option<Vec<u8>>>>;
	/// Check remote method execution proof.
	fn check_execution_proof(
		&self,
		request: &RemoteCallRequest<Block::Header>,
		remote_proof: StorageProof,
	) -> ClientResult<Vec<u8>>;
	/// Check remote changes query proof.
	fn check_changes_proof(
		&self,
		request: &RemoteChangesRequest<Block::Header>,
		proof: ChangesProof<Block::Header>
	) -> ClientResult<Vec<(NumberFor<Block>, u32)>>;
	/// Check remote body proof.
	fn check_body_proof(
		&self,
		request: &RemoteBodyRequest<Block::Header>,
		body: Vec<Block::Extrinsic>
	) -> ClientResult<Vec<Block::Extrinsic>>;
}


/// Light client blockchain storage.
pub trait Storage<Block: BlockT>: AuxStore + HeaderBackend<Block> + HeaderMetadata<Block, Error=ClientError> {
	/// Store new header. Should refuse to revert any finalized blocks.
	///
	/// Takes new authorities, the leaf state of the new block, and
	/// any auxiliary storage updates to place in the same operation.
	fn import_header(
		&self,
		header: Block::Header,
		cache: HashMap<well_known_cache_keys::Id, Vec<u8>>,
		state: NewBlockState,
		aux_ops: Vec<(Vec<u8>, Option<Vec<u8>>)>,
	) -> ClientResult<()>;

	/// Set an existing block as new best block.
	fn set_head(&self, block: BlockId<Block>) -> ClientResult<()>;

	/// Mark historic header as finalized.
	fn finalize_header(&self, block: BlockId<Block>) -> ClientResult<()>;

	/// Get last finalized header.
	fn last_finalized(&self) -> ClientResult<Block::Hash>;

	/// Get headers CHT root for given block. Returns None if the block is not pruned (not a part of any CHT).
	fn header_cht_root(
		&self,
		cht_size: NumberFor<Block>,
		block: NumberFor<Block>,
	) -> ClientResult<Option<Block::Hash>>;

	/// Get changes trie CHT root for given block. Returns None if the block is not pruned (not a part of any CHT).
	fn changes_trie_cht_root(
		&self,
		cht_size: NumberFor<Block>,
		block: NumberFor<Block>,
	) -> ClientResult<Option<Block::Hash>>;

	/// Get storage cache.
	fn cache(&self) -> Option<Arc<dyn BlockchainCache<Block>>>;

	/// Get storage usage statistics.
	fn usage_info(&self) -> Option<UsageInfo>;
}

/// Remote header.
#[derive(Debug)]
pub enum LocalOrRemote<Data, Request> {
	/// When data is available locally, it is returned.
	Local(Data),
	/// When data is unavailable locally, the request to fetch it from remote node is returned.
	Remote(Request),
	/// When data is unknown.
	Unknown,
}

/// Futures-based blockchain backend that either resolves blockchain data
/// locally, or fetches required data from remote node.
pub trait RemoteBlockchain<Block: BlockT>: Send + Sync {
	/// Get block header.
	fn header(&self, id: BlockId<Block>) -> ClientResult<LocalOrRemote<
		Block::Header,
		RemoteHeaderRequest<Block::Header>,
	>>;
}



#[cfg(test)]
pub mod tests {
	use futures::future::Ready;
	use parking_lot::Mutex;
	use sp_blockchain::Error as ClientError;
	use sp_test_primitives::{Block, Header, Extrinsic};
	use super::*;

	pub type OkCallFetcher = Mutex<Vec<u8>>;

	fn not_implemented_in_tests<T, E>() -> Ready<Result<T, E>>
	where
		E: std::convert::From<&'static str>,
	{
		futures::future::ready(Err("Not implemented on test node".into()))
	}

	impl Fetcher<Block> for OkCallFetcher {
		type RemoteHeaderResult = Ready<Result<Header, ClientError>>;
		type RemoteReadResult = Ready<Result<HashMap<Vec<u8>, Option<Vec<u8>>>, ClientError>>;
		type RemoteCallResult = Ready<Result<Vec<u8>, ClientError>>;
		type RemoteChangesResult = Ready<Result<Vec<(NumberFor<Block>, u32)>, ClientError>>;
		type RemoteBodyResult = Ready<Result<Vec<Extrinsic>, ClientError>>;

		fn remote_header(&self, _request: RemoteHeaderRequest<Header>) -> Self::RemoteHeaderResult {
			not_implemented_in_tests()
		}

		fn remote_read(&self, _request: RemoteReadRequest<Header>) -> Self::RemoteReadResult {
			not_implemented_in_tests()
		}

		fn remote_read_child(&self, _request: RemoteReadChildRequest<Header>) -> Self::RemoteReadResult {
			not_implemented_in_tests()
		}

		fn remote_call(&self, _request: RemoteCallRequest<Header>) -> Self::RemoteCallResult {
			futures::future::ready(Ok((*self.lock()).clone()))
		}

		fn remote_changes(&self, _request: RemoteChangesRequest<Header>) -> Self::RemoteChangesResult {
			not_implemented_in_tests()
		}

		fn remote_body(&self, _request: RemoteBodyRequest<Header>) -> Self::RemoteBodyResult {
			not_implemented_in_tests()
		}
	}
}
