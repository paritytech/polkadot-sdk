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
	archive::{
		archive_storage::ArchiveStorageDiff,
		error::{Error as ArchiveError, Infallible},
		types::MethodResult,
		ArchiveApiServer,
	},
	common::{
		events::{
			ArchiveStorageDiffEvent, ArchiveStorageDiffItem, ArchiveStorageEvent, StorageQuery,
		},
		storage::{QueryResult, StorageSubscriptionClient},
	},
	hex_string, SubscriptionTaskExecutor,
};

use codec::Encode;
use futures::FutureExt;
use jsonrpsee::{core::async_trait, PendingSubscriptionSink};
use sc_client_api::{
	Backend, BlockBackend, BlockchainEvents, CallExecutor, ChildInfo, ExecutorProvider, StorageKey,
	StorageProvider,
};
use sc_rpc::utils::Subscription;
use sp_api::{CallApiAt, CallContext};
use sp_blockchain::{
	Backend as BlockChainBackend, Error as BlockChainError, HeaderBackend, HeaderMetadata,
};
use sp_core::{Bytes, U256};
use sp_runtime::{
	traits::{Block as BlockT, Header as HeaderT, NumberFor},
	SaturatedConversion,
};
use std::{collections::HashSet, marker::PhantomData, sync::Arc};

use tokio::sync::mpsc;

pub(crate) const LOG_TARGET: &str = "rpc-spec-v2::archive";

/// The buffer capacity for each storage query.
///
/// This is small because the underlying JSON-RPC server has
/// its down buffer capacity per connection as well.
const STORAGE_QUERY_BUF: usize = 16;

/// An API for archive RPC calls.
pub struct Archive<BE: Backend<Block>, Block: BlockT, Client> {
	/// Substrate client.
	client: Arc<Client>,
	/// Backend of the chain.
	backend: Arc<BE>,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// The hexadecimal encoded hash of the genesis block.
	genesis_hash: String,
	/// Phantom member to pin the block type.
	_phantom: PhantomData<Block>,
}

impl<BE: Backend<Block>, Block: BlockT, Client> Archive<BE, Block, Client> {
	/// Create a new [`Archive`].
	pub fn new<GenesisHash: AsRef<[u8]>>(
		client: Arc<Client>,
		backend: Arc<BE>,
		genesis_hash: GenesisHash,
		executor: SubscriptionTaskExecutor,
	) -> Self {
		let genesis_hash = hex_string(&genesis_hash.as_ref());
		Self { client, backend, executor, genesis_hash, _phantom: PhantomData }
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
	fn archive_v1_body(&self, hash: Block::Hash) -> Result<Option<Vec<String>>, Infallible> {
		let Ok(Some(signed_block)) = self.client.block(hash) else { return Ok(None) };

		let extrinsics = signed_block
			.block
			.extrinsics()
			.iter()
			.map(|extrinsic| hex_string(&extrinsic.encode()))
			.collect();

		Ok(Some(extrinsics))
	}

	fn archive_v1_genesis_hash(&self) -> Result<String, Infallible> {
		Ok(self.genesis_hash.clone())
	}

	fn archive_v1_header(&self, hash: Block::Hash) -> Result<Option<String>, Infallible> {
		let Ok(Some(header)) = self.client.header(hash) else { return Ok(None) };

		Ok(Some(hex_string(&header.encode())))
	}

	fn archive_v1_finalized_height(&self) -> Result<u64, Infallible> {
		Ok(self.client.info().finalized_number.saturated_into())
	}

	fn archive_v1_hash_by_height(&self, height: u64) -> Result<Vec<String>, ArchiveError> {
		let height: NumberFor<Block> = U256::from(height)
			.try_into()
			.map_err(|_| ArchiveError::InvalidParam(format!("Invalid block height: {}", height)))?;

		let finalized_num = self.client.info().finalized_number;

		if finalized_num >= height {
			let Ok(Some(hash)) = self.client.block_hash(height) else { return Ok(vec![]) };
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

	fn archive_v1_call(
		&self,
		hash: Block::Hash,
		function: String,
		call_parameters: String,
	) -> Result<MethodResult, ArchiveError> {
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

	fn archive_v1_storage(
		&self,
		pending: PendingSubscriptionSink,
		hash: Block::Hash,
		items: Vec<StorageQuery<String>>,
		child_trie: Option<String>,
	) {
		let mut storage_client =
			StorageSubscriptionClient::<Client, Block, BE>::new(self.client.clone());

		let fut = async move {
			let Ok(mut sink) = pending.accept().await.map(Subscription::from) else { return };

			let items = match items
				.into_iter()
				.map(|query| {
					let key = StorageKey(parse_hex_param(query.key)?);
					Ok(StorageQuery { key, query_type: query.query_type })
				})
				.collect::<Result<Vec<_>, ArchiveError>>()
			{
				Ok(items) => items,
				Err(error) => {
					let _ = sink.send(&ArchiveStorageEvent::err(error.to_string()));
					return
				},
			};

			let child_trie = child_trie.map(|child_trie| parse_hex_param(child_trie)).transpose();
			let child_trie = match child_trie {
				Ok(child_trie) => child_trie.map(ChildInfo::new_default_from_vec),
				Err(error) => {
					let _ = sink.send(&ArchiveStorageEvent::err(error.to_string()));
					return
				},
			};

			let (tx, mut rx) = tokio::sync::mpsc::channel(STORAGE_QUERY_BUF);
			let storage_fut = storage_client.generate_events(hash, items, child_trie, tx);

			// We don't care about the return value of this join:
			// - process_events might encounter an error (if the client disconnected)
			// - storage_fut might encounter an error while processing a trie queries and
			// the error is propagated via the sink.
			let _ = futures::future::join(storage_fut, process_storage_events(&mut rx, &mut sink))
				.await;
		};

		self.executor.spawn("substrate-rpc-subscription", Some("rpc"), fut.boxed());
	}

	fn archive_v1_storage_diff(
		&self,
		pending: PendingSubscriptionSink,
		hash: Block::Hash,
		items: Vec<ArchiveStorageDiffItem<String>>,
		previous_hash: Option<Block::Hash>,
	) {
		let storage_client = ArchiveStorageDiff::new(self.client.clone());
		let client = self.client.clone();

		log::trace!(target: LOG_TARGET, "Storage diff subscription started");

		let fut = async move {
			let Ok(mut sink) = pending.accept().await.map(Subscription::from) else { return };

			let previous_hash = if let Some(previous_hash) = previous_hash {
				previous_hash
			} else {
				let Ok(Some(current_header)) = client.header(hash) else {
					let message = format!("Block header is not present: {hash}");
					let _ = sink.send(&ArchiveStorageDiffEvent::err(message)).await;
					return
				};
				*current_header.parent_hash()
			};

			let (tx, mut rx) = tokio::sync::mpsc::channel(STORAGE_QUERY_BUF);
			let storage_fut =
				storage_client.handle_trie_queries(hash, items, previous_hash, tx.clone());

			// We don't care about the return value of this join:
			// - process_events might encounter an error (if the client disconnected)
			// - storage_fut might encounter an error while processing a trie queries and
			// the error is propagated via the sink.
			let _ =
				futures::future::join(storage_fut, process_storage_diff_events(&mut rx, &mut sink))
					.await;
		};

		self.executor.spawn("substrate-rpc-subscription", Some("rpc"), fut.boxed());
	}
}

/// Sends all the events of the storage_diff method to the sink.
async fn process_storage_diff_events(
	rx: &mut mpsc::Receiver<ArchiveStorageDiffEvent>,
	sink: &mut Subscription,
) {
	loop {
		tokio::select! {
			_ = sink.closed() => {
				return
			},

			maybe_event = rx.recv() => {
				let Some(event) = maybe_event else {
					break;
				};

				if event.is_done() {
					log::debug!(target: LOG_TARGET, "Finished processing partial trie query");
				} else if event.is_err() {
					log::debug!(target: LOG_TARGET, "Error encountered while processing partial trie query");
				}

				if sink.send(&event).await.is_err() {
					return
				}
			}
		}
	}
}

/// Sends all the events of the storage method to the sink.
async fn process_storage_events(rx: &mut mpsc::Receiver<QueryResult>, sink: &mut Subscription) {
	loop {
		tokio::select! {
			_ = sink.closed() => {
				break
			}

			maybe_storage = rx.recv() => {
				let Some(event) = maybe_storage else {
					break;
				};

				match event {
					Ok(None) => continue,

					Ok(Some(event)) =>
						if sink.send(&ArchiveStorageEvent::result(event)).await.is_err() {
							return
						},

					Err(error) => {
						let _ = sink.send(&ArchiveStorageEvent::err(error)).await;
						return
					}
				}
			}
		}
	}

	let _ = sink.send(&ArchiveStorageEvent::StorageDone).await;
}
