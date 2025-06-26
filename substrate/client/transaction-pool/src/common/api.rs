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

//! Chain api required for the transaction pool.

use crate::{
	common::{sliding_stat::DurationSlidingStats, STAT_SLIDING_WINDOW},
	graph::ValidateTransactionPriority,
	insert_and_log_throttled, LOG_TARGET, LOG_TARGET_STAT,
};
use codec::Encode;
use futures::future::{ready, Future, FutureExt, Ready};
use prometheus_endpoint::Registry as PrometheusRegistry;
use sc_client_api::{blockchain::HeaderBackend, BlockBackend};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::{HeaderMetadata, TreeRoute};
use sp_core::traits::SpawnEssentialNamed;
use sp_runtime::{
	generic::BlockId,
	traits::{self, Block as BlockT, BlockIdTo},
	transaction_validity::{TransactionSource, TransactionValidity},
};
use sp_transaction_pool::runtime_api::TaggedTransactionQueue;
use std::{
	marker::PhantomData,
	pin::Pin,
	sync::Arc,
	time::{Duration, Instant},
};
use tokio::sync::{mpsc, oneshot, Mutex};

use super::{
	error::{self, Error},
	metrics::{ApiMetrics, ApiMetricsExt},
};
use crate::graph;
use tracing::{trace, warn, Level};

/// The transaction pool logic for full client.
pub struct FullChainApi<Client, Block> {
	client: Arc<Client>,
	_marker: PhantomData<Block>,
	metrics: Option<Arc<ApiMetrics>>,
	validation_pool_normal: mpsc::Sender<Pin<Box<dyn Future<Output = ()> + Send>>>,
	validation_pool_maintained: mpsc::Sender<Pin<Box<dyn Future<Output = ()> + Send>>>,
	validate_transaction_normal_stats: DurationSlidingStats,
	validate_transaction_maintained_stats: DurationSlidingStats,
}

/// Spawn a validation task that will be used by the transaction pool to validate transactions.
fn spawn_validation_pool_task(
	name: &'static str,
	receiver_normal: Arc<Mutex<mpsc::Receiver<Pin<Box<dyn Future<Output = ()> + Send>>>>>,
	receiver_maintained: Arc<Mutex<mpsc::Receiver<Pin<Box<dyn Future<Output = ()> + Send>>>>>,
	spawner: &impl SpawnEssentialNamed,
	stats: DurationSlidingStats,
	blocking_stats: DurationSlidingStats,
) {
	spawner.spawn_essential_blocking(
		name,
		Some("transaction-pool"),
		async move {
			loop {
				let start = Instant::now();

				let task = {
					let receiver_maintained = receiver_maintained.clone();
					let receiver_normal = receiver_normal.clone();
					tokio::select! {
						Some(task) = async {
							receiver_maintained.lock().await.recv().await
						} => { task }
						Some(task) = async {
							receiver_normal.lock().await.recv().await
						} => { task }
						else => {
							return
						}
					}
				};

				let blocking_duration = {
					let start = Instant::now();
					task.await;
					start.elapsed()
				};

				insert_and_log_throttled!(
					Level::DEBUG,
					target:LOG_TARGET_STAT,
					prefix:format!("validate_transaction_inner_stats"),
					stats,
					start.elapsed().into()
				);
				insert_and_log_throttled!(
					Level::DEBUG,
					target:LOG_TARGET_STAT,
					prefix:format!("validate_transaction_blocking_stats"),
					blocking_stats,
					blocking_duration.into()
				);
				trace!(target:LOG_TARGET, duration=?start.elapsed(), "spawn_validation_pool_task");
			}
		}
		.boxed(),
	);
}

impl<Client, Block> FullChainApi<Client, Block> {
	/// Create new transaction pool logic.
	pub fn new(
		client: Arc<Client>,
		prometheus: Option<&PrometheusRegistry>,
		spawner: &impl SpawnEssentialNamed,
	) -> Self {
		let stats = DurationSlidingStats::new(Duration::from_secs(STAT_SLIDING_WINDOW));
		let blocking_stats = DurationSlidingStats::new(Duration::from_secs(STAT_SLIDING_WINDOW));

		let metrics = prometheus.map(ApiMetrics::register).and_then(|r| match r {
			Err(error) => {
				warn!(
					target: LOG_TARGET,
					?error,
					"Failed to register transaction pool API Prometheus metrics"
				);
				None
			},
			Ok(api) => Some(Arc::new(api)),
		});

		let (sender, receiver) = mpsc::channel(1);
		let (sender_maintained, receiver_maintained) = mpsc::channel(1);

		let receiver = Arc::new(Mutex::new(receiver));
		let receiver_maintained = Arc::new(Mutex::new(receiver_maintained));
		spawn_validation_pool_task(
			"transaction-pool-task-0",
			receiver.clone(),
			receiver_maintained.clone(),
			spawner,
			stats.clone(),
			blocking_stats.clone(),
		);
		spawn_validation_pool_task(
			"transaction-pool-task-1",
			receiver,
			receiver_maintained,
			spawner,
			stats.clone(),
			blocking_stats.clone(),
		);

		FullChainApi {
			client,
			validation_pool_normal: sender,
			validation_pool_maintained: sender_maintained,
			_marker: Default::default(),
			metrics,
			validate_transaction_normal_stats: DurationSlidingStats::new(Duration::from_secs(
				STAT_SLIDING_WINDOW,
			)),
			validate_transaction_maintained_stats: DurationSlidingStats::new(Duration::from_secs(
				STAT_SLIDING_WINDOW,
			)),
		}
	}
}

impl<Client, Block> graph::ChainApi for FullChainApi<Client, Block>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ BlockIdTo<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>,
	Client: Send + Sync + 'static,
	Client::Api: TaggedTransactionQueue<Block>,
{
	type Block = Block;
	type Error = error::Error;
	type ValidationFuture =
		Pin<Box<dyn Future<Output = error::Result<TransactionValidity>> + Send>>;
	type BodyFuture = Ready<error::Result<Option<Vec<<Self::Block as BlockT>::Extrinsic>>>>;

	fn block_body(&self, hash: Block::Hash) -> Self::BodyFuture {
		ready(self.client.block_body(hash).map_err(error::Error::from))
	}

	fn validate_transaction(
		&self,
		at: <Self::Block as BlockT>::Hash,
		source: TransactionSource,
		uxt: graph::ExtrinsicFor<Self>,
		validation_priority: ValidateTransactionPriority,
	) -> Self::ValidationFuture {
		let start = Instant::now();
		let (tx, rx) = oneshot::channel();
		let client = self.client.clone();
		let (stats, validation_pool, prefix) =
			if validation_priority == ValidateTransactionPriority::Maintained {
				(
					self.validate_transaction_maintained_stats.clone(),
					self.validation_pool_maintained.clone(),
					"validate_transaction_maintained_stats",
				)
			} else {
				(
					self.validate_transaction_normal_stats.clone(),
					self.validation_pool_normal.clone(),
					"validate_transaction_stats",
				)
			};
		let metrics = self.metrics.clone();

		async move {
			metrics.report(|m| m.validations_scheduled.inc());

			{
				validation_pool
					.send(
						async move {
							let res = validate_transaction_blocking(&*client, at, source, uxt);
							let _ = tx.send(res);
							metrics.report(|m| m.validations_finished.inc());
						}
						.boxed(),
					)
					.await
					.map_err(|e| Error::RuntimeApi(format!("Validation pool down: {:?}", e)))?;
			}

			let validity = match rx.await {
				Ok(r) => r,
				Err(_) => Err(Error::RuntimeApi("Validation was canceled".into())),
			};

			insert_and_log_throttled!(
				Level::DEBUG,
				target:LOG_TARGET_STAT,
				prefix:prefix,
				stats,
				start.elapsed().into()
			);

			validity
		}
		.boxed()
	}

	/// Validates a transaction by calling into the runtime.
	///
	/// Same as `validate_transaction` but blocks the current thread when performing validation.
	fn validate_transaction_blocking(
		&self,
		at: Block::Hash,
		source: TransactionSource,
		uxt: graph::ExtrinsicFor<Self>,
	) -> error::Result<TransactionValidity> {
		validate_transaction_blocking(&*self.client, at, source, uxt)
	}

	fn block_id_to_number(
		&self,
		at: &BlockId<Self::Block>,
	) -> error::Result<Option<graph::NumberFor<Self>>> {
		self.client.to_number(at).map_err(|e| Error::BlockIdConversion(e.to_string()))
	}

	fn block_id_to_hash(
		&self,
		at: &BlockId<Self::Block>,
	) -> error::Result<Option<graph::BlockHash<Self>>> {
		self.client.to_hash(at).map_err(|e| Error::BlockIdConversion(e.to_string()))
	}

	fn hash_and_length(
		&self,
		ex: &graph::RawExtrinsicFor<Self>,
	) -> (graph::ExtrinsicHash<Self>, usize) {
		ex.using_encoded(|x| (<traits::HashingFor<Block> as traits::Hash>::hash(x), x.len()))
	}

	fn block_header(
		&self,
		hash: <Self::Block as BlockT>::Hash,
	) -> Result<Option<<Self::Block as BlockT>::Header>, Self::Error> {
		self.client.header(hash).map_err(Into::into)
	}

	fn tree_route(
		&self,
		from: <Self::Block as BlockT>::Hash,
		to: <Self::Block as BlockT>::Hash,
	) -> Result<TreeRoute<Self::Block>, Self::Error> {
		sp_blockchain::tree_route::<Block, Client>(&*self.client, from, to).map_err(Into::into)
	}
}

/// Helper function to validate a transaction using a full chain API.
/// This method will call into the runtime to perform the validation.
fn validate_transaction_blocking<Client, Block>(
	client: &Client,
	at: Block::Hash,
	source: TransactionSource,
	uxt: graph::ExtrinsicFor<FullChainApi<Client, Block>>,
) -> error::Result<TransactionValidity>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ BlockIdTo<Block>
		+ HeaderBackend<Block>
		+ HeaderMetadata<Block, Error = sp_blockchain::Error>,
	Client: Send + Sync + 'static,
	Client::Api: TaggedTransactionQueue<Block>,
{
	let s = std::time::Instant::now();
	let tx_hash = uxt.using_encoded(|x| <traits::HashingFor<Block> as traits::Hash>::hash(x));

	let result = sp_tracing::within_span!(sp_tracing::Level::TRACE, "validate_transaction";
	{
		let runtime_api = client.runtime_api();
		let api_version = sp_tracing::within_span! { sp_tracing::Level::TRACE, "check_version";
			runtime_api
				.api_version::<dyn TaggedTransactionQueue<Block>>(at)
				.map_err(|e| Error::RuntimeApi(e.to_string()))?
				.ok_or_else(|| Error::RuntimeApi(
					format!("Could not find `TaggedTransactionQueue` api for block `{:?}`.", at)
				))
		}?;

		use sp_api::Core;

		sp_tracing::within_span!(
			sp_tracing::Level::TRACE, "runtime::validate_transaction";
		{
			if api_version >= 3 {
				runtime_api.validate_transaction(at, source, (*uxt).clone(), at)
					.map_err(|e| Error::RuntimeApi(e.to_string()))
			} else {
				let block_number = client.to_number(&BlockId::Hash(at))
					.map_err(|e| Error::RuntimeApi(e.to_string()))?
					.ok_or_else(||
						Error::RuntimeApi(format!("Could not get number for block `{:?}`.", at))
					)?;

				// The old versions require us to call `initialize_block` before.
				runtime_api.initialize_block(at, &sp_runtime::traits::Header::new(
					block_number + sp_runtime::traits::One::one(),
					Default::default(),
					Default::default(),
					at,
					Default::default()),
				).map_err(|e| Error::RuntimeApi(e.to_string()))?;

				if api_version == 2 {
					#[allow(deprecated)] // old validate_transaction
					runtime_api.validate_transaction_before_version_3(at, source, (*uxt).clone())
						.map_err(|e| Error::RuntimeApi(e.to_string()))
				} else {
					#[allow(deprecated)] // old validate_transaction
					runtime_api.validate_transaction_before_version_2(at, (*uxt).clone())
						.map_err(|e| Error::RuntimeApi(e.to_string()))
				}
			}
		})
	});
	trace!(
		target: LOG_TARGET,
		?tx_hash,
		?at,
		duration = ?s.elapsed(),
		"validate_transaction_blocking"
	);
	result
}
