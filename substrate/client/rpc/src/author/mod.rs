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

//! Substrate block-author/full-node API.

//! Substrate block-author/full-node API.

#[cfg(test)]
mod tests;

use self::error::{Error, Result};
use crate::{
	utils::{spawn_subscription_task, BoundedVecDeque, PendingSubscription},
	SubscriptionTaskExecutor,
};
use codec::{Decode, Encode};
use jsonrpsee::{core::async_trait, types::ErrorObject, Extensions, PendingSubscriptionSink};
use sc_rpc_api::check_if_safe;
use sc_transaction_pool_api::{
	error::IntoPoolError, BlockHash, InPoolTransaction, TransactionFor, TransactionPool,
	TransactionReceipt, TransactionSource, TransactionStatus, TxHash, TxInvalidityReportMap,
};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::Bytes;
use sp_keystore::{KeystoreExt, KeystorePtr};
use sp_runtime::traits::Block as BlockT;
use sp_session::SessionKeys;
use std::sync::Arc;

use sc_transaction_pool::TransactionReceiptDb;

/// Re-export the API for backward compatibility.
pub use sc_rpc_api::author::*;

/// Authoring API
pub struct Author<P, Client> {
	/// Substrate client
	client: Arc<Client>,
	/// Transactions pool
	pool: Arc<P>,
	/// The key store.
	keystore: KeystorePtr,
	/// Executor to spawn subscriptions.
	executor: SubscriptionTaskExecutor,
	/// Transaction receipt database
	receipt_db: Option<Arc<TransactionReceiptDb>>,
}

impl<P, Client> Author<P, Client> {
	/// Create new instance of Authoring API.
	pub fn new(
		client: Arc<Client>,
		pool: Arc<P>,
		keystore: KeystorePtr,
		executor: SubscriptionTaskExecutor,
		receipt_db: Option<Arc<TransactionReceiptDb>>,
	) -> Self {
		Author { client, pool, keystore, executor, receipt_db }
	}
}

/// Currently we treat all RPC transactions as externals.
///
/// Possibly in the future we could allow opt-in for special treatment
/// of such transactions, so that the block authors can inject
/// some unique transactions via RPC and have them included in the pool.
const TX_SOURCE: TransactionSource = TransactionSource::External;

/// Helper function to convert string-based events to typed events
fn convert_string_event_to_typed<TxHash, BlockHash>(
	event: &TransactionStatus<String, String>,
) -> Option<TransactionStatus<TxHash, BlockHash>>
where
	TxHash: Decode,
	BlockHash: Decode,
{
	match event {
		TransactionStatus::Ready => Some(TransactionStatus::Ready),
		TransactionStatus::Future => Some(TransactionStatus::Future),
		TransactionStatus::Broadcast(peers) => Some(TransactionStatus::Broadcast(peers.clone())),
		TransactionStatus::InBlock((hash_str, idx)) => {
			let clean_str = hash_str.trim_start_matches("0x");
			hex::decode(clean_str)
				.ok()
				.and_then(|bytes| BlockHash::decode(&mut &bytes[..]).ok())
				.map(|hash| TransactionStatus::InBlock((hash, *idx)))
		},
		TransactionStatus::Retracted(hash_str) => {
			let clean_str = hash_str.trim_start_matches("0x");
			hex::decode(clean_str)
				.ok()
				.and_then(|bytes| BlockHash::decode(&mut &bytes[..]).ok())
				.map(TransactionStatus::Retracted)
		},
		TransactionStatus::FinalityTimeout(hash_str) => {
			let clean_str = hash_str.trim_start_matches("0x");
			hex::decode(clean_str)
				.ok()
				.and_then(|bytes| BlockHash::decode(&mut &bytes[..]).ok())
				.map(TransactionStatus::FinalityTimeout)
		},
		TransactionStatus::Finalized((hash_str, idx)) => {
			let clean_str = hash_str.trim_start_matches("0x");
			hex::decode(clean_str)
				.ok()
				.and_then(|bytes| BlockHash::decode(&mut &bytes[..]).ok())
				.map(|hash| TransactionStatus::Finalized((hash, *idx)))
		},
		TransactionStatus::Usurped(hash_str) => {
			let clean_str = hash_str.trim_start_matches("0x");
			hex::decode(clean_str)
				.ok()
				.and_then(|bytes| TxHash::decode(&mut &bytes[..]).ok())
				.map(TransactionStatus::Usurped)
		},
		TransactionStatus::Dropped => Some(TransactionStatus::Dropped),
		TransactionStatus::Invalid => Some(TransactionStatus::Invalid),
	}
}

#[async_trait]
impl<P, Client> AuthorApiServer<TxHash<P>, BlockHash<P>> for Author<P, Client>
where
	P: TransactionPool + Sync + Send + 'static,
	Client: HeaderBackend<P::Block> + ProvideRuntimeApi<P::Block> + Send + Sync + 'static,
	Client::Api: SessionKeys<P::Block>,
	P::Hash: Unpin,
	<P::Block as BlockT>::Hash: Unpin,
{
	async fn submit_extrinsic(&self, ext: Bytes) -> Result<TxHash<P>> {
		let xt = match Decode::decode(&mut &ext[..]) {
			Ok(xt) => xt,
			Err(err) => return Err(Error::Client(Box::new(err)).into()),
		};
		let best_block_hash = self.client.info().best_hash;
		self.pool.submit_one(best_block_hash, TX_SOURCE, xt).await.map_err(|e| {
			e.into_pool_error()
				.map(|e| Error::Pool(e))
				.unwrap_or_else(|e| Error::Verification(Box::new(e)))
				.into()
		})
	}

	fn insert_key(
		&self,
		ext: &Extensions,
		key_type: String,
		suri: String,
		public: Bytes,
	) -> Result<()> {
		check_if_safe(ext)?;

		let key_type = key_type.as_str().try_into().map_err(|_| Error::BadKeyType)?;
		self.keystore
			.insert(key_type, &suri, &public[..])
			.map_err(|_| Error::KeystoreUnavailable)?;
		Ok(())
	}

	fn rotate_keys(&self, ext: &Extensions) -> Result<Bytes> {
		check_if_safe(ext)?;

		let best_block_hash = self.client.info().best_hash;
		let mut runtime_api = self.client.runtime_api();

		runtime_api.register_extension(KeystoreExt::from(self.keystore.clone()));

		runtime_api
			.generate_session_keys(best_block_hash, None)
			.map(Into::into)
			.map_err(|api_err| Error::Client(Box::new(api_err)).into())
	}

	fn has_session_keys(&self, ext: &Extensions, session_keys: Bytes) -> Result<bool> {
		check_if_safe(ext)?;

		let best_block_hash = self.client.info().best_hash;
		let keys = self
			.client
			.runtime_api()
			.decode_session_keys(best_block_hash, session_keys.to_vec())
			.map_err(|e| Error::Client(Box::new(e)))?
			.ok_or(Error::InvalidSessionKeys)?;

		Ok(self.keystore.has_keys(&keys))
	}

	fn has_key(&self, ext: &Extensions, public_key: Bytes, key_type: String) -> Result<bool> {
		check_if_safe(ext)?;

		let key_type = key_type.as_str().try_into().map_err(|_| Error::BadKeyType)?;
		Ok(self.keystore.has_keys(&[(public_key.to_vec(), key_type)]))
	}

	fn pending_extrinsics(&self) -> Result<Vec<Bytes>> {
		Ok(self.pool.ready().map(|tx| tx.data().encode().into()).collect())
	}

	async fn remove_extrinsic(
		&self,
		ext: &Extensions,
		bytes_or_hash: Vec<hash::ExtrinsicOrHash<TxHash<P>>>,
	) -> Result<Vec<TxHash<P>>> {
		check_if_safe(ext)?;
		let hashes = bytes_or_hash
			.into_iter()
			.map(|x| match x {
				hash::ExtrinsicOrHash::Hash(h) => Ok((h, None)),
				hash::ExtrinsicOrHash::Extrinsic(bytes) => {
					let xt = Decode::decode(&mut &bytes[..])?;
					Ok((self.pool.hash_of(&xt), None))
				},
			})
			.collect::<Result<TxInvalidityReportMap<TxHash<P>>>>()?;

		Ok(self
			.pool
			.report_invalid(None, hashes)
			.await
			.into_iter()
			.map(|tx| tx.hash().clone())
			.collect())
	}

	fn watch_extrinsic(&self, pending: PendingSubscriptionSink, xt: Bytes) {
		let best_block_hash = self.client.info().best_hash;
		let dxt = match TransactionFor::<P>::decode(&mut &xt[..]).map_err(|e| Error::from(e)) {
			Ok(dxt) => dxt,
			Err(e) => {
				spawn_subscription_task(&self.executor, pending.reject(e));
				return
			},
		};

		let pool = self.pool.clone();
		let fut = async move {
			let submit =
				pool.submit_and_watch(best_block_hash, TX_SOURCE, dxt).await.map_err(|e| {
					e.into_pool_error()
						.map(error::Error::from)
						.unwrap_or_else(|e| error::Error::Verification(Box::new(e)))
				});

			let stream = match submit {
				Ok(stream) => stream,
				Err(err) => {
					let _ = pending.reject(ErrorObject::from(err)).await;
					return
				},
			};

			PendingSubscription::from(pending)
				.pipe_from_stream(stream, BoundedVecDeque::default())
				.await;
		};

		spawn_subscription_task(&self.executor, fut);
	}

	/// Get transaction receipt by hash
	async fn transaction_receipt(
		&self,
		hash: TxHash<P>,
	) -> std::result::Result<Option<TransactionReceipt<BlockHash<P>, TxHash<P>>>, Error> {
		// First try to get from the pool's in-memory tracker
		if let Some(receipt) = self.pool.get_transaction_receipt(&hash).await {
			return Ok(Some(receipt));
		}

		// Fallback to database if available
		if let Some(ref db) = self.receipt_db {
			// Use hex encoding of the hash for database lookup
			let hash_bytes = hash.encode();
			let hash_str = format!("0x{}", hex::encode(&hash_bytes));

			match db.get_transaction_receipt(&hash_str).await {
				Ok(Some(receipt)) => {
					// Convert string-based receipt to proper types
					let block_hash = receipt.block_hash.and_then(|bh_str| {
						let clean_str = bh_str.trim_start_matches("0x");
						hex::decode(clean_str)
							.ok()
							.and_then(|bytes| BlockHash::<P>::decode(&mut &bytes[..]).ok())
					});

					// Convert string events back to proper types
					let events: Vec<TransactionStatus<TxHash<P>, BlockHash<P>>> = receipt
						.events
						.iter()
						.filter_map(|event| {
							convert_string_event_to_typed::<TxHash<P>, BlockHash<P>>(event)
						})
						.collect();

					let converted_receipt = TransactionReceipt {
						status: receipt.status,
						block_hash,
						block_number: receipt.block_number,
						transaction_index: receipt.transaction_index,
						events,
						transaction_hash: hash,
						submitted_at: receipt.submitted_at,
					};
					Ok(Some(converted_receipt))
				},
				Ok(None) => Ok(None),
				Err(e) => {
					log::warn!("Failed to get transaction receipt from database: {}", e);
					Ok(None)
				},
			}
		} else {
			Ok(None)
		}
	}
}
