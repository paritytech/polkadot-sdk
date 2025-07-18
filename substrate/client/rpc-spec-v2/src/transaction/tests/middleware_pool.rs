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

use async_trait::async_trait;
use codec::Encode;
use sc_transaction_pool::BasicPool;
use sc_transaction_pool_api::{
	ImportNotificationStream, PoolStatus, ReadyTransactions, TransactionFor, TransactionPool,
	TransactionSource, TransactionStatusStreamFor, TxHash, TxInvalidityReportMap,
};

use crate::hex_string;
use futures::StreamExt;

use sp_runtime::traits::Block as BlockT;
use std::{collections::HashMap, pin::Pin, sync::Arc};
use substrate_test_runtime_transaction_pool::TestApi;
use tokio::sync::mpsc;

pub type Block = substrate_test_runtime_client::runtime::Block;

pub type TxTestPool = MiddlewarePool;
pub type TxStatusType<Pool> = sc_transaction_pool_api::TransactionStatus<
	sc_transaction_pool_api::TxHash<Pool>,
	sc_transaction_pool_api::BlockHash<Pool>,
>;
pub type TxStatusTypeTest = TxStatusType<TxTestPool>;

/// The type of the event that the middleware captures.
#[derive(Debug, PartialEq)]
pub enum MiddlewarePoolEvent {
	TransactionStatus {
		transaction: String,
		status: sc_transaction_pool_api::TransactionStatus<
			<Block as BlockT>::Hash,
			<Block as BlockT>::Hash,
		>,
	},
	PoolError {
		transaction: String,
		err: String,
	},
}

/// The channel that receives events when the broadcast futures are dropped.
pub type MiddlewarePoolRecv = mpsc::UnboundedReceiver<MiddlewarePoolEvent>;

/// Add a middleware to the transaction pool.
///
/// This wraps the `submit_and_watch` to gain access to the events.
pub struct MiddlewarePool {
	pub inner_pool: Arc<BasicPool<TestApi, Block>>,
	/// Send the middleware events to the test.
	sender: mpsc::UnboundedSender<MiddlewarePoolEvent>,
}

impl MiddlewarePool {
	/// Construct a new [`MiddlewarePool`].
	pub fn new(pool: Arc<BasicPool<TestApi, Block>>) -> (Self, MiddlewarePoolRecv) {
		let (sender, recv) = mpsc::unbounded_channel();
		(MiddlewarePool { inner_pool: pool, sender }, recv)
	}
}

#[async_trait]
impl TransactionPool for MiddlewarePool {
	type Block = <BasicPool<TestApi, Block> as TransactionPool>::Block;
	type Hash = <BasicPool<TestApi, Block> as TransactionPool>::Hash;
	type InPoolTransaction = <BasicPool<TestApi, Block> as TransactionPool>::InPoolTransaction;
	type Error = <BasicPool<TestApi, Block> as TransactionPool>::Error;

	async fn submit_at(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xts: Vec<TransactionFor<Self>>,
	) -> Result<Vec<Result<TxHash<Self>, Self::Error>>, Self::Error> {
		self.inner_pool.submit_at(at, source, xts).await
	}

	async fn submit_one(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> Result<TxHash<Self>, Self::Error> {
		self.inner_pool.submit_one(at, source, xt).await
	}

	async fn submit_and_watch(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		xt: TransactionFor<Self>,
	) -> Result<Pin<Box<TransactionStatusStreamFor<Self>>>, Self::Error> {
		let transaction = hex_string(&xt.encode());
		let sender = self.sender.clone();

		let watcher = match self.inner_pool.submit_and_watch(at, source, xt).await {
			Ok(watcher) => watcher,
			Err(err) => {
				let _ = sender.send(MiddlewarePoolEvent::PoolError {
					transaction: transaction.clone(),
					err: err.to_string(),
				});
				return Err(err);
			},
		};

		let watcher = watcher.map(move |status| {
			let sender = sender.clone();
			let transaction = transaction.clone();

			let _ = sender.send(MiddlewarePoolEvent::TransactionStatus {
				transaction,
				status: status.clone(),
			});

			status
		});

		Ok(watcher.boxed())
	}

	async fn report_invalid(
		&self,
		at: Option<<Self::Block as BlockT>::Hash>,
		invalid_tx_errors: TxInvalidityReportMap<TxHash<Self>>,
	) -> Vec<Arc<Self::InPoolTransaction>> {
		self.inner_pool.report_invalid(at, invalid_tx_errors).await
	}

	fn status(&self) -> PoolStatus {
		self.inner_pool.status()
	}

	fn import_notification_stream(&self) -> ImportNotificationStream<TxHash<Self>> {
		self.inner_pool.import_notification_stream()
	}

	fn hash_of(&self, xt: &TransactionFor<Self>) -> TxHash<Self> {
		self.inner_pool.hash_of(xt)
	}

	fn on_broadcasted(&self, propagations: HashMap<TxHash<Self>, Vec<String>>) {
		self.inner_pool.on_broadcasted(propagations)
	}

	fn ready_transaction(&self, hash: &TxHash<Self>) -> Option<Arc<Self::InPoolTransaction>> {
		self.inner_pool.ready_transaction(hash)
	}

	async fn ready_at(
		&self,
		at: <Self::Block as BlockT>::Hash,
	) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		self.inner_pool.ready_at(at).await
	}

	fn ready(&self) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		self.inner_pool.ready()
	}

	fn futures(&self) -> Vec<Self::InPoolTransaction> {
		self.inner_pool.futures()
	}

	async fn ready_at_with_timeout(
		&self,
		at: <Self::Block as BlockT>::Hash,
		_timeout: std::time::Duration,
	) -> Box<dyn ReadyTransactions<Item = Arc<Self::InPoolTransaction>> + Send> {
		self.inner_pool.ready_at(at).await
	}
}
