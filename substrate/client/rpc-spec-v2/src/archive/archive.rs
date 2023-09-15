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

use super::ArchiveApiServer;
use crate::chain_head::hex_string;
use codec::Encode;
use jsonrpsee::core::{async_trait, RpcResult};
use sc_client_api::{Backend, BlockBackend, BlockchainEvents, ExecutorProvider, StorageProvider};
use sp_api::CallApiAt;
use sp_blockchain::{Error as BlockChainError, HeaderBackend, HeaderMetadata};
use sp_runtime::traits::Block as BlockT;
use std::{marker::PhantomData, sync::Arc};

/// An API for archive RPC calls.
pub struct Archive<BE: Backend<Block>, Block: BlockT, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// The hexadecimal encoded hash of the genesis block.
	genesis_hash: String,
	/// Phantom member to pin the block type.
	_phantom: PhantomData<(Block, BE)>,
}

impl<BE: Backend<Block>, Block: BlockT, Client> Archive<BE, Block, Client> {
	/// Create a new [`Archive`].
	pub fn new<GenesisHash: AsRef<[u8]>>(client: Arc<Client>, genesis_hash: GenesisHash) -> Self {
		let genesis_hash = hex_string(&genesis_hash.as_ref());
		Self { client, genesis_hash, _phantom: PhantomData }
	}
}

#[async_trait]
impl<BE, Block, Client> ArchiveApiServer<Block::Hash> for Archive<BE, Block, Client>
where
	Block: BlockT + 'static,
	Block::Header: Unpin,
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
}
