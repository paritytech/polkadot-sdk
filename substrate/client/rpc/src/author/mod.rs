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
	TransactionSource, TxHash, TxInvalidityReportMap,
};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_core::Bytes;
use sp_keystore::{KeystoreExt, KeystorePtr};
use sp_runtime::traits::Block as BlockT;
use sp_session::SessionKeys;
use std::sync::Arc;

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
}

impl<P, Client> Author<P, Client> {
	/// Create new instance of Authoring API.
	pub fn new(
		client: Arc<Client>,
		pool: Arc<P>,
		keystore: KeystorePtr,
		executor: SubscriptionTaskExecutor,
	) -> Self {
		Author { client, pool, keystore, executor }
	}
}

/// Currently we treat all RPC transactions as externals.
///
/// Possibly in the future we could allow opt-in for special treatment
/// of such transactions, so that the block authors can inject
/// some unique transactions via RPC and have them included in the pool.
const TX_SOURCE: TransactionSource = TransactionSource::External;

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
}
