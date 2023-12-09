// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

//! API implementation for `archive`.

use crate::{
	archive::{error::Error as ArchiveError, ArchiveApiServer},
	chain_head::hex_string,
	MethodResult,
};

use codec::Encode;
use jsonrpsee::core::{async_trait, RpcResult};
use sc_client_api::{
	Backend, BlockBackend, BlockchainEvents, CallExecutor, ExecutorProvider, StorageProvider,
};
use sp_api::{CallApiAt, CallContext};
use sp_blockchain::{
	Backend as BlockChainBackend, Error as BlockChainError, HeaderBackend, HeaderMetadata,
};
use sp_core::Bytes;
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, NumberFor},
	SaturatedConversion,
};
use std::{collections::HashSet, marker::PhantomData, sync::Arc};

/// An API for archive RPC calls.
pub struct Archive<BE: Backend<Block>, Block: BlockT, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Backend of the chain.
	backend: Arc<BE>,
	/// The hexadecimal encoded hash of the genesis block.
	genesis_hash: String,
	/// Phantom member to pin the block type.
	_phantom: PhantomData<(Block, BE)>,
}

impl<BE: Backend<Block>, Block: BlockT, Client> Archive<BE, Block, Client> {
	/// Create a new [`Archive`].
	pub fn new<GenesisHash: AsRef<[u8]>>(
		client: Arc<Client>,
		backend: Arc<BE>,
		genesis_hash: GenesisHash,
	) -> Self {
		let genesis_hash = hex_string(&genesis_hash.as_ref());
		Self { client, backend, genesis_hash, _phantom: PhantomData }
	}
}

/// Parse hex-encoded string parameter as raw bytes.
///
/// If the parsing fails, returns an error propagated to the RPC method.
fn parse_hex_param(param: String) -> Result<Vec<u8>, ArchiveError> {
	// Methods can accept empty parameters.
	if param.is_empty() {
		return Ok(Default::default())
	}

	array_bytes::hex2bytes(&param).map_err(|_| ArchiveError::InvalidParam(param))
}

#[async_trait]
impl<BE, Block, Client> ArchiveApiServer<Block::Hash> for Archive<BE, Block, Client>
where
	Block: BlockT + 'static,
	Block::Header: Unpin,
	<<Block as BlockT>::Header as HeaderT>::Number: From<u64>,
	BE: Backend<Block> + 'static,
	Client: BlockBackend<Block>
		+ ExecutorProvider<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = BlockChainError>
		+ BlockchainEvents<Block>
		+ CallApiAt<Block>
		+ StorageProvider<Block, BE>
		+ 'static,
{
	fn archive_unstable_body(&self, hash: Block::Hash) -> RpcResult<Option<Vec<String>>> {
		let Ok(Some(signed_block)) = self.client.block(hash) else { return Ok(None) };

		let extrinsics = signed_block
			.block
			.extrinsics()
			.iter()
			.map(|extrinsic| hex_string(&extrinsic.encode()))
			.collect();

		Ok(Some(extrinsics))
	}

	fn archive_unstable_genesis_hash(&self) -> RpcResult<String> {
		Ok(self.genesis_hash.clone())
	}

	fn archive_unstable_header(&self, hash: Block::Hash) -> RpcResult<Option<String>> {
		let Ok(Some(header)) = self.client.header(hash) else { return Ok(None) };

		Ok(Some(hex_string(&header.encode())))
	}

	fn archive_unstable_finalized_height(&self) -> RpcResult<u64> {
		Ok(self.client.info().finalized_number.saturated_into())
	}

	fn archive_unstable_hash_by_height(&self, height: u64) -> RpcResult<Vec<String>> {
		let height: NumberFor<Block> = height.into();
		let finalized_num = self.client.info().finalized_number;

		if finalized_num >= height {
			let Ok(Some(hash)) = self.client.block_hash(height.into()) else { return Ok(vec![]) };
			return Ok(vec![hex_string(&hash.as_ref())])
		}

		let blockchain = self.backend.blockchain();
		// Fetch all the leaves of the blockchain that are on a higher or equal height.
		let mut headers: Vec<_> = blockchain
			.leaves()
			.map_err(|error| ArchiveError::FetchLeaves(error.to_string()))?
			.into_iter()
			.filter_map(|hash| {
				let Ok(Some(header)) = self.client.header(hash) else { return None };

				if header.number() < &height {
					return None
				}

				Some(header)
			})
			.collect();

		let mut result = Vec::new();
		let mut visited = HashSet::new();

		while let Some(header) = headers.pop() {
			if header.number() == &height {
				result.push(hex_string(&header.hash().as_ref()));
				continue
			}

			let parent_hash = *header.parent_hash();

			// Continue the iteration for unique hashes.
			// Forks might intersect on a common chain that is not yet finalized.
			if visited.insert(parent_hash) {
				let Ok(Some(next_header)) = self.client.header(parent_hash) else { continue };
				headers.push(next_header);
			}
		}

		Ok(result)
	}

	fn archive_unstable_call(
		&self,
		hash: Block::Hash,
		function: String,
		call_parameters: String,
	) -> RpcResult<MethodResult> {
		let call_parameters = Bytes::from(parse_hex_param(call_parameters)?);

		let result =
			self.client
				.executor()
				.call(hash, &function, &call_parameters, CallContext::Offchain);

		Ok(match result {
			Ok(result) => MethodResult::ok(hex_string(&result)),
			Err(error) => MethodResult::err(error.to_string()),
		})
	}
}
