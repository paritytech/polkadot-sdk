// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.
//! The client connects to the source substrate chain
//! and is used by the rpc server to query and send transactions to the substrate chain.

#[cfg(feature = "subxt")]
pub(crate) mod runtime_api;

use crate::{
	BlockId, BlockInfoProvider, BlockNumberOrTag, FeeHistoryProvider, FeeHistoryResult, Filter,
	Log, ReceiptInfo, ReceiptProvider, SyncingProgress, SyncingStatus, TraceV1, TransactionTrace,
	block_info_provider::BlockInfo,
	block_sync::{SyncCheckpoint, SyncLabel},
	substrate_client::{NodeHealth, SubmitResult, SubstrateClient},
};
use jsonrpsee::types::{ErrorObjectOwned, error::CALL_EXECUTION_FAILED_CODE};
use pallet_revive::{
	EthTransactError,
	evm::{H256, TransactionSigned, U256, decode_revert_reason},
};
use pallet_revive_types::runtime_api::{
	BlockV1 as Block, GenericTransactionV1 as GenericTransaction,
	HashesOrTransactionInfosV1 as HashesOrTransactionInfos, StateOverrideSetV1 as StateOverrideSet,
	TracerTypeV1,
};
use sp_weights::Weight;
use std::{
	ops::Range,
	sync::{
		Arc,
		atomic::{AtomicBool, AtomicUsize, Ordering},
	},
};
use thiserror::Error;
use tokio::sync::{Mutex, mpsc};

/// The substrate block number type.
pub type SubstrateBlockNumber = u32;

/// The substrate block hash type.
pub type SubstrateBlockHash = sp_core::H256;

/// The runtime balance type.
pub type Balance = u128;

/// The subscription type used to listen to new blocks.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum SubscriptionType {
	/// Subscribe to best blocks.
	BestBlocks,
	/// Subscribe to finalized blocks.
	FinalizedBlocks,
}

/// Submit Error reason.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Error)]
pub enum SubmitError {
	/// Transaction was usurped by another with the same nonce.
	#[error("Transaction was usurped by another with the same nonce")]
	Usurped,
	/// Transaction was dropped from the pool.
	#[error("Transaction was dropped")]
	Dropped,
	/// Transaction is invalid (e.g. bad nonce, signature, etc).
	#[error("Transaction is invalid (e.g. bad nonce, signature, etc)")]
	Invalid,
	/// Transaction stream ended without a terminal status.
	#[error("Transaction stream ended without status")]
	StreamEnded,
	/// Unknown transaction status.
	#[error("Unknown transaction status")]
	Unknown,
}

impl From<sc_transaction_pool_api::TransactionStatus<SubstrateBlockHash, SubstrateBlockHash>>
	for SubmitError
{
	fn from(
		status: sc_transaction_pool_api::TransactionStatus<SubstrateBlockHash, SubstrateBlockHash>,
	) -> Self {
		use sc_transaction_pool_api::TransactionStatus;
		match status {
			TransactionStatus::Usurped(_) => SubmitError::Usurped,
			TransactionStatus::Dropped => SubmitError::Dropped,
			TransactionStatus::Invalid => SubmitError::Invalid,
			_ => SubmitError::Unknown,
		}
	}
}

/// The error type for the client.
#[derive(Error, Debug)]
pub enum ClientError {
	/// A [`jsonrpsee::core::ClientError`] wrapper error.
	#[error(transparent)]
	Jsonrpsee(#[from] jsonrpsee::core::ClientError),
	/// A [`subxt::Error`] wrapper error.
	#[cfg(feature = "subxt")]
	#[error(transparent)]
	SubxtError(#[from] subxt::Error),
	/// A [`subxt::rpcs::Error`] wrapper error.
	#[cfg(feature = "subxt")]
	#[error(transparent)]
	RpcError(#[from] subxt::rpcs::Error),
	/// A [`sqlx::Error`] wrapper error.
	#[error(transparent)]
	SqlxError(#[from] sqlx::Error),
	/// A [`codec::Error`] wrapper error.
	#[error(transparent)]
	CodecError(#[from] codec::Error),
	/// author_submitExtrinsic failed.
	#[error("Invalid transaction: {0}")]
	SubmitError(SubmitError),
	/// Transact call failed.
	#[error("contract reverted: {0:?}")]
	TransactError(EthTransactError),
	/// A decimal conversion failed.
	#[error("conversion failed")]
	ConversionFailed,
	/// The block hash was not found.
	#[error("hash not found")]
	BlockNotFound,
	/// The contract was not found.
	#[error("Contract not found")]
	ContractNotFound,
	#[error("No Ethereum extrinsic found")]
	EthExtrinsicNotFound,
	/// The transaction fee could not be found.
	#[error("transactionFeePaid event not found")]
	TxFeeNotFound,
	/// Failed to decode a raw payload into a signed transaction.
	#[error("Failed to decode a raw payload into a signed transaction")]
	TxDecodingFailed,
	/// Failed to recover eth address.
	#[error("failed to recover eth address")]
	RecoverEthAddressFailed,
	/// Failed to filter logs.
	#[error("Failed to filter logs")]
	LogFilterFailed(#[from] anyhow::Error),
	/// Receipt storage was not found.
	#[error("Receipt storage not found")]
	ReceiptDataNotFound,
	/// Ethereum block was not found.
	#[error("Ethereum block not found")]
	EthereumBlockNotFound,
	/// Receipt data length mismatch.
	#[error("Receipt data length mismatch")]
	ReceiptDataLengthMismatch,
	/// Transaction submission timeout.
	#[error("Transaction submission timeout")]
	Timeout,
	/// All of the estimation methods `eth_estimate`, `eth_transact_with_config`, and
	/// `eth_transact` were not found and therefore none of the estimation methods succeeded.
	#[error("None of the estimation methods were found")]
	NoEstimationMethodSucceeded,
	/// Chain identity mismatch between stored genesis and connected node.
	#[error("Genesis hash mismatch")]
	ChainMismatch,
	/// Stored sync boundary does not match the connected node.
	#[error("Sync boundary mismatch")]
	SyncBoundaryMismatch,
	/// An error originating from the native in-process Substrate client.
	#[error("Native client error: {0}")]
	NativeClientError(String),
}

impl ClientError {
	/// Errors that indicate a mismatch between the stored sync state and the connected node.
	pub(crate) fn is_chain_validation_error(&self) -> bool {
		matches!(self, Self::ChainMismatch | Self::SyncBoundaryMismatch)
	}
}

// Direct `From` impls so `?` can lift subxt 0.50 sub-error variants without an explicit
// `subxt::Error::from`.
#[cfg(feature = "subxt")]
macro_rules! impl_from_subxt_subtype {
	($($ty:ty),* $(,)?) => {
		$(
			impl From<$ty> for ClientError {
				fn from(err: $ty) -> Self {
					ClientError::SubxtError(err.into())
				}
			}
		)*
	};
}

#[cfg(feature = "subxt")]
impl_from_subxt_subtype!(
	subxt::error::OnlineClientAtBlockError,
	subxt::error::OnlineClientError,
	subxt::error::BackendError,
	subxt::error::BlockError,
	subxt::error::BlocksError,
	subxt::error::RuntimeApiError,
	subxt::error::EventsError,
	subxt::error::ExtrinsicError,
	subxt::error::ConstantError,
	subxt::error::StorageError,
	subxt::error::StorageValueError,
	subxt::error::ExtrinsicDecodeErrorAt,
);

const LOG_TARGET: &str = "eth-rpc::client";

const REVERT_CODE: i32 = 3;

pub(crate) const NOTIFIER_CAPACITY: usize = 16;

impl From<ClientError> for ErrorObjectOwned {
	fn from(err: ClientError) -> Self {
		match err {
			ClientError::TransactError(EthTransactError::Data(data)) => {
				let msg = match decode_revert_reason(&data) {
					Some(reason) => format!("execution reverted: {reason}"),
					None => "execution reverted".to_string(),
				};
				let data = format!("0x{}", hex::encode(data));
				ErrorObjectOwned::owned::<String>(REVERT_CODE, msg, Some(data))
			},
			ClientError::TransactError(EthTransactError::Message(msg)) => {
				ErrorObjectOwned::owned::<String>(CALL_EXECUTION_FAILED_CODE, msg, None)
			},
			_ => {
				ErrorObjectOwned::owned::<String>(CALL_EXECUTION_FAILED_CODE, err.to_string(), None)
			},
		}
	}
}

/// Returns the first EVM block number for main and test nets, `None` otherwise.
fn known_first_evm_block_for_chain(chain_id: u64) -> Option<u32> {
	match chain_id {
		420420417 => Some(4_367_914),  // Paseo Asset Hub
		420420418 => Some(12_234_156), // Kusama Asset Hub
		420420419 => Some(11_405_259), // Polkadot Asset Hub
		420420421 => Some(13_169_391), // Westend Asset Hub
		_ => None,
	}
}

/// A client that connects to a Substrate node and maintains a receipt/block cache.
#[derive(Clone)]
pub struct Client<C: SubstrateClient, BP: BlockInfoProvider> {
	/// The underlying substrate client backend.
	pub(crate) backend: C,
	/// Block info provider (caches latest best/finalized blocks).
	block_provider: BP,
	/// Receipt storage / cache.
	receipt_provider: ReceiptProvider<BP>,
	/// Fee history cache.
	fee_history_provider: FeeHistoryProvider,
	/// Whether the node has automine enabled.
	automine: bool,
	/// Block notifier for automine.
	block_notifier: Option<tokio::sync::broadcast::Sender<H256>>,
	/// Serialises write operations across concurrent subscriptions.
	subscription_lock: Arc<Mutex<()>>,

	/// Block subscription sender side.
	block_subscription_tx: tokio::sync::broadcast::Sender<Block>,
	/// Log subscription sender side.
	log_subscription_tx: tokio::sync::broadcast::Sender<Log>,
	/// Whether historic backfill has completed. `false` if not started or in progress.
	backfill_complete: Arc<AtomicBool>,
	/// Queue for backfilling blocks missed during subscription reconnects.
	subscription_gap_queue: SubscriptionGapQueue,
}

/// A request to backfill a range of missed blocks (both bounds inclusive).
pub struct GapFillRequest {
	pub from_inclusive: SubstrateBlockNumber,
	pub to_inclusive: SubstrateBlockNumber,
}

/// Queues gap-fill requests for blocks missed during subscription reconnects.
#[derive(Clone)]
pub struct SubscriptionGapQueue {
	/// Sender half of the gap-fill queue.
	tx: mpsc::Sender<GapFillRequest>,
	/// Queued + in-flight gap fills. Channel length alone is insufficient
	/// because it drops to zero as soon as the receiver dequeues the item.
	pending: Arc<AtomicUsize>,
}

impl SubscriptionGapQueue {
	pub fn new() -> (Self, mpsc::Receiver<GapFillRequest>) {
		// Each reconnect produces one gap-fill request for the entire missed range,
		// so 32 allows for 32 rapid disconnects before the consumer processes any.
		let (tx, rx) = mpsc::channel(32);
		(Self { tx, pending: Arc::new(AtomicUsize::new(0)) }, rx)
	}

	/// If `current` is not consecutive to `last`, queue a gap-fill for the missing range.
	pub fn detect_and_queue(&self, current: SubstrateBlockNumber, last: SubstrateBlockNumber) {
		if current.saturating_sub(last) <= 1 {
			return;
		}

		let from_inclusive = current.saturating_sub(1);
		let to_inclusive = last.saturating_add(1);
		let gap_len = from_inclusive.saturating_sub(to_inclusive) + 1;
		self.pending.fetch_add(1, Ordering::Release);
		match self.tx.try_send(GapFillRequest { from_inclusive, to_inclusive }) {
			Ok(_) => {
				log::info!(target: LOG_TARGET,
					"🔄 Subscription gap queue: queued #{from_inclusive} down to #{to_inclusive} ({gap_len} blocks)");
			},
			Err(err) => {
				self.pending.fetch_sub(1, Ordering::Release);
				log::warn!(target: LOG_TARGET,
					"🔄 Subscription gap queue error, dropping #{from_inclusive}..#{to_inclusive} ({gap_len} blocks): {err}");
			},
		}
	}

	/// Mark one request as processed.
	pub fn mark_done(&self) {
		let res = self
			.pending
			.fetch_update(Ordering::AcqRel, Ordering::Acquire, |v| v.checked_sub(1));
		if res.is_err() {
			debug_assert!(false, "subscription gap queue pending counter underflowed");
			log::error!(target: LOG_TARGET,
				"🔄 Subscription gap queue pending counter underflow, delete the database and restart with --eth-pruning=archive to resync");
		}
	}

	/// Returns `true` if there are pending gap-fill requests.
	pub fn has_pending(&self) -> bool {
		self.pending.load(Ordering::Acquire) > 0
	}
}

impl<C: SubstrateClient, BP: BlockInfoProvider> Client<C, BP> {
	/// Create a `Client` from an arbitrary backend and block-info-provider.
	pub fn from_backend(
		backend: C,
		block_provider: BP,
		receipt_provider: ReceiptProvider<BP>,
		automine: bool,
		subscription_gap_queue: SubscriptionGapQueue,
	) -> Result<Self, ClientError> {
		let block_notifier =
			automine.then(|| tokio::sync::broadcast::channel::<H256>(NOTIFIER_CAPACITY).0);

		let (block_subscription_tx, _) =
			tokio::sync::broadcast::channel::<Block>(NOTIFIER_CAPACITY);
		let (log_subscription_tx, _) = tokio::sync::broadcast::channel::<Log>(1000);

		Ok(Self {
			backend,
			block_provider,
			receipt_provider,
			fee_history_provider: FeeHistoryProvider::default(),
			automine,
			block_notifier,
			subscription_lock: Arc::new(Mutex::new(())),
			backfill_complete: Arc::new(AtomicBool::new(false)),
			block_subscription_tx,
			log_subscription_tx,
			subscription_gap_queue,
		})
	}

	pub fn from_native_backend(
		backend: C,
		block_provider: BP,
		receipt_provider: ReceiptProvider<BP>,
		automine: bool,
		subscription_gap_queue: SubscriptionGapQueue,
	) -> Result<Self, ClientError> {
		Self::from_backend(
			backend,
			block_provider,
			receipt_provider,
			automine,
			subscription_gap_queue,
		)
	}

	/// Mark historic backfill as complete.
	pub(crate) fn mark_backfill_complete(&self) {
		self.backfill_complete.store(true, Ordering::Release);
	}

	/// Expose the receipt provider (used by block_sync).
	pub(crate) fn receipt_provider(&self) -> &ReceiptProvider<BP> {
		&self.receipt_provider
	}

	/// Expose the block provider (used by block_sync).
	pub(crate) fn block_provider(&self) -> &BP {
		&self.block_provider
	}

	pub(crate) fn subscription_gap_queue(&self) -> &SubscriptionGapQueue {
		&self.subscription_gap_queue
	}

	/// Creates a block notifier instance.
	pub fn create_block_notifier(&mut self) {
		self.block_notifier = Some(tokio::sync::broadcast::channel::<H256>(NOTIFIER_CAPACITY).0);
	}

	/// Sets a block notifier.
	pub fn set_block_notifier(&mut self, notifier: Option<tokio::sync::broadcast::Sender<H256>>) {
		self.block_notifier = notifier;
	}

	/// Create a receiver for the block broadcast channel.
	pub fn get_block_subscription_rx(&self) -> tokio::sync::broadcast::Receiver<Block> {
		self.block_subscription_tx.subscribe()
	}

	/// Create a receiver for the log broadcast channel.
	pub fn get_log_subscription_rx(&self) -> tokio::sync::broadcast::Receiver<Log> {
		self.log_subscription_tx.subscribe()
	}

	/// Subscribe to past blocks, executing the callback for each block in `range`.
	async fn subscribe_past_blocks<F, Fut>(
		&self,
		range: Range<SubstrateBlockNumber>,
		callback: F,
	) -> Result<(), ClientError>
	where
		F: Fn(Arc<BP::Block>) -> Fut + Send + Sync,
		Fut: std::future::Future<Output = Result<(), ClientError>> + Send,
	{
		let mut block = self
			.block_provider
			.block_by_number(range.end)
			.await?
			.ok_or(ClientError::BlockNotFound)?;

		loop {
			let block_number = block.number();
			log::trace!(
				target: "eth-rpc::subscription",
				"Processing past block #{block_number}"
			);

			let parent_hash = block.parent_hash();
			callback(block.clone()).await.inspect_err(|err| {
				log::error!(
					target: "eth-rpc::subscription",
					"Failed to process past block #{block_number}: {err:?}"
				);
			})?;

			if range.start < block_number {
				block = self
					.block_provider
					.block_by_hash(&parent_hash)
					.await?
					.ok_or(ClientError::BlockNotFound)?;
			} else {
				return Ok(());
			}
		}
	}

	/// Subscribe to new blocks, and execute the async closure for each block.
	async fn subscribe_new_blocks<F, Fut>(
		&self,
		subscription_type: SubscriptionType,
		callback: F,
	) -> Result<(), ClientError>
	where
		F: Fn(Arc<BP::Block>) -> Fut + Send + Sync + 'static,
		Fut: std::future::Future<Output = Result<(), ClientError>> + Send,
	{
		let mut last_finalized_seen = u32::MAX;
		let mut block_rx = self.backend.subscribe_blocks(subscription_type).await?;

		while let Some(info) = block_rx.recv().await {
			let block = match self.block_provider.block_by_hash(&info.hash()).await {
				Ok(Some(block)) => block,
				Ok(None) => {
					log::warn!(
						target: LOG_TARGET,
						"Notified block {:?} not found ({subscription_type:?})",
						info.hash()
					);
					continue;
				},
				Err(err) => {
					log::error!(
						target: LOG_TARGET,
						"Failed to fetch notified block ({subscription_type:?}): {err:?}"
					);
					continue;
				},
			};

			let _guard = self.subscription_lock.lock().await;

			// Detect and queue gap-fills for missed finalized blocks.
			if subscription_type == SubscriptionType::FinalizedBlocks {
				let block_number = block.number();
				if last_finalized_seen != u32::MAX {
					self.subscription_gap_queue.detect_and_queue(block_number, last_finalized_seen);
				}
				last_finalized_seen = block_number;
			}

			if let Err(err) = callback(block).await {
				log::error!(
					target: LOG_TARGET,
					"block callback failed ({subscription_type:?}): {err:?}"
				);
			}
		}
		Ok(())
	}

	/// Start the block subscription, and populate the block cache.
	pub async fn subscribe_and_cache_new_blocks(
		&self,
		subscription_type: SubscriptionType,
	) -> Result<(), ClientError> {
		log::info!(
			target: LOG_TARGET,
			"🔌 Subscribing to new blocks ({subscription_type:?})"
		);

		let backend = self.backend.clone();
		let receipt_provider = self.receipt_provider.clone();
		let block_provider_update = self.block_provider.clone();
		let fee_history_provider = self.fee_history_provider.clone();
		let block_notifier = self.block_notifier.clone();
		// ↓ NEW: capture the broadcast senders
		let block_subscription_tx = self.block_subscription_tx.clone();
		let log_subscription_tx = self.log_subscription_tx.clone();
		let backfill_complete = self.backfill_complete.clone();
		let subscription_gap_queue = self.subscription_gap_queue.clone();

		self.subscribe_new_blocks(subscription_type, move |block: Arc<BP::Block>| {
			let backend = backend.clone();
			let receipt_provider = receipt_provider.clone();
			let block_provider_update = block_provider_update.clone();
			let fee_history_provider = fee_history_provider.clone();
			let block_notifier = block_notifier.clone();
			let block_subscription_tx = block_subscription_tx.clone();
			let log_subscription_tx = log_subscription_tx.clone();
			let backfill_complete = backfill_complete.clone();
			let subscription_gap_queue = subscription_gap_queue.clone();

			async move {
				let hash = block.hash();
				let number = block.number();
				let evm_block = backend.eth_block(hash).await?;

				// Advance `latest` before receipts land, so a visible receipt always implies
				// `latest` reflects its block. Otherwise `getBlock` and a following `eth_call`
				// at `latest` can straddle the update and disagree on the block number.
				block_provider_update.update_latest(Arc::clone(&block), subscription_type).await;

				let (_, receipts): (Vec<_>, Vec<_>) = receipt_provider
					.insert_block_receipts_by_hash(hash, number, &evm_block.hash)
					.await?
					.into_iter()
					.unzip();

				fee_history_provider.update_fee_history(&evm_block, &receipts).await;

				// Broadcast new block to eth_subscribe("newHeads") subscribers.
				if subscription_type == SubscriptionType::BestBlocks &&
					block_subscription_tx.receiver_count() > 0
				{
					let _ = block_subscription_tx.send(evm_block.clone());
				}

				if subscription_type == SubscriptionType::FinalizedBlocks &&
					log_subscription_tx.receiver_count() > 0
				{
					for receipt in &receipts {
						for log in &receipt.logs {
							let _ = log_subscription_tx.send(log.clone());
						}
					}
				}

				let should_advance_head = subscription_type == SubscriptionType::FinalizedBlocks &&
					backfill_complete.load(Ordering::Acquire) &&
					!subscription_gap_queue.has_pending();

				match (subscription_type, &block_notifier) {
					_ if should_advance_head => {
						// Advance the persistent sync head so a restart resumes from here.
						if let Err(err) = receipt_provider
							.advance_sync_label(SyncLabel::Head, SyncCheckpoint::new(number, hash))
							.await
						{
							log::warn!(target: LOG_TARGET,
								"Failed to update sync_label[{}]: {err:?}", SyncLabel::Head);
						}
					},
					(SubscriptionType::BestBlocks, Some(sender)) if sender.receiver_count() > 0 => {
						let _ = sender.send(hash);
					},
					_ => {},
				}
				Ok(())
			}
		})
		.await
	}

	/// Cache old blocks up to the given block number.
	pub async fn subscribe_and_cache_blocks(
		&self,
		index_last_n_blocks: SubstrateBlockNumber,
	) -> Result<(), ClientError> {
		let last = self.latest_block().await.number().saturating_sub(1);
		let range = last.saturating_sub(index_last_n_blocks)..last;
		log::info!(target: LOG_TARGET, "🗄️ Indexing past blocks in range {range:?}");

		self.subscribe_past_blocks(range, |block: Arc<BP::Block>| async move {
			let ethereum_hash = self
				.backend
				.eth_block_hash(block.hash(), pallet_revive::evm::U256::from(block.number()))
				.await?
				.ok_or(ClientError::EthereumBlockNotFound)?;
			self.receipt_provider
				.insert_block_receipts_by_hash(block.hash(), block.number(), &ethereum_hash)
				.await?;
			Ok(())
		})
		.await?;

		log::info!(target: LOG_TARGET, "🗄️ Finished indexing past blocks");
		Ok(())
	}

	/// Get the block hash for the given block number or tag.
	pub async fn block_hash_for_tag(&self, at: BlockId) -> Result<SubstrateBlockHash, ClientError> {
		match at {
			BlockId::Hash(hash) => self
				.resolve_substrate_hash(&H256::from(hash.block_hash.0))
				.await
				.ok_or(ClientError::EthereumBlockNotFound),
			BlockId::Number(tag) => self
				.block_by_number_or_tag(&tag)
				.await?
				.map(|block| block.hash())
				.ok_or(ClientError::BlockNotFound),
		}
	}

	/// Resolve a [`BlockNumberOrTag`] to a concrete block number.
	async fn resolve_tag_to_number(&self, tag: BlockNumberOrTag) -> Result<U256, ClientError> {
		match tag {
			BlockNumberOrTag::Number(n) => Ok(U256::from(n)),
			BlockNumberOrTag::Earliest => Ok(U256::from(self.earliest_block_number())),
			_ => Ok(self
				.block_by_number_or_tag(&tag)
				.await?
				.ok_or(ClientError::BlockNotFound)?
				.number()
				.into()),
		}
	}

	/// Get the latest finalized block.
	pub async fn latest_finalized_block(&self) -> Arc<BP::Block> {
		self.block_provider.latest_finalized_block().await
	}

	/// Get the latest best block.
	pub async fn latest_block(&self) -> Arc<BP::Block> {
		self.block_provider.latest_block().await
	}

	/// Get the block number of the latest block.
	pub async fn block_number(&self) -> Result<SubstrateBlockNumber, ClientError> {
		Ok(self.block_provider.latest_block().await.number())
	}

	/// Get a block hash for the given block number.
	pub async fn get_block_hash(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<SubstrateBlockHash>, ClientError> {
		Ok(self.block_provider.block_by_number(block_number).await?.map(|b| b.hash()))
	}

	/// Get a block for the specified hash or number.
	pub async fn block_by_number_or_tag(
		&self,
		block: &BlockNumberOrTag,
	) -> Result<Option<Arc<BP::Block>>, ClientError> {
		match block {
			BlockNumberOrTag::Number(n) => {
				let n = (*n).try_into().map_err(|_| ClientError::ConversionFailed)?;
				self.block_by_number(n).await
			},
			BlockNumberOrTag::Earliest => self.block_by_number(self.earliest_block_number()).await,
			BlockNumberOrTag::Finalized | BlockNumberOrTag::Safe => {
				Ok(Some(self.block_provider.latest_finalized_block().await))
			},
			_ => Ok(Some(self.block_provider.latest_block().await)),
		}
	}

	/// Get a block by hash.
	pub async fn block_by_hash(
		&self,
		hash: &SubstrateBlockHash,
	) -> Result<Option<Arc<BP::Block>>, ClientError> {
		self.block_provider.block_by_hash(hash).await
	}

	/// Resolve Ethereum block hash to Substrate block hash.
	pub async fn resolve_substrate_hash(&self, ethereum_hash: &H256) -> Option<H256> {
		self.receipt_provider.get_substrate_hash(ethereum_hash).await
	}

	/// Resolve Substrate block hash to Ethereum block hash.
	pub async fn resolve_ethereum_hash(&self, substrate_hash: &H256) -> Option<H256> {
		self.receipt_provider.get_ethereum_hash(substrate_hash).await
	}

	/// Get a block by Ethereum hash, falling back to treating it as a Substrate hash.
	pub async fn block_by_ethereum_hash(
		&self,
		ethereum_hash: &H256,
	) -> Result<Option<Arc<BP::Block>>, ClientError> {
		if let Some(substrate_hash) = self.resolve_substrate_hash(ethereum_hash).await {
			return self.block_by_hash(&substrate_hash).await;
		}
		self.block_by_hash(ethereum_hash).await
	}

	/// Get a block by number.
	pub async fn block_by_number(
		&self,
		block_number: SubstrateBlockNumber,
	) -> Result<Option<Arc<BP::Block>>, ClientError> {
		self.block_provider.block_by_number(block_number).await
	}

	/// Submit an ethereum transaction payload.
	pub async fn submit(&self, payload: Vec<u8>) -> Result<SubmitResult, ClientError> {
		self.backend.submit_extrinsic(payload).await
	}

	/// Get an EVM transaction receipt by hash.
	pub async fn receipt(&self, tx_hash: &H256) -> Option<ReceiptInfo> {
		self.receipt_provider.receipt_by_hash(tx_hash).await
	}

	/// Get an EVM transaction receipt by block hash and index.
	pub async fn receipt_by_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: usize,
	) -> Option<ReceiptInfo> {
		self.receipt_provider
			.receipt_by_block_hash_and_index(block_hash, transaction_index)
			.await
	}

	pub async fn signed_tx_by_hash(&self, tx_hash: &H256) -> Option<TransactionSigned> {
		self.receipt_provider.signed_tx_by_hash(tx_hash).await
	}

	/// Get receipts count per block.
	pub async fn receipts_count_per_block(&self, block_hash: &SubstrateBlockHash) -> Option<usize> {
		self.receipt_provider.receipts_count_per_block(block_hash).await
	}

	pub async fn sync_state(
		&self,
	) -> Result<sc_rpc::system::SyncState<SubstrateBlockNumber>, ClientError> {
		self.backend.sync_state().await
	}

	/// Get the syncing status of the chain.
	pub async fn syncing(&self) -> Result<SyncingStatus, ClientError> {
		let health = self.backend.system_health().await?;
		let status = if health.is_syncing {
			let sync_state = self.sync_state().await?;
			SyncingStatus::SyncingProgress(SyncingProgress {
				current_block: Some(sync_state.current_block.into()),
				highest_block: Some(sync_state.highest_block.into()),
				starting_block: Some(sync_state.starting_block.into()),
			})
		} else {
			SyncingStatus::Bool(false)
		};

		Ok(status)
	}

	/// Get the system health.
	pub async fn system_health(&self) -> Result<NodeHealth, ClientError> {
		self.backend.system_health().await
	}

	/// Get the chain ID.
	pub fn chain_id(&self) -> u64 {
		self.backend.chain_id()
	}

	fn earliest_block_number(&self) -> u32 {
		self.receipt_provider
			.first_evm_block()
			.or_else(|| known_first_evm_block_for_chain(self.backend.chain_id()))
			.unwrap_or(0)
	}

	/// Get the max block weight.
	pub fn max_block_weight(&self) -> Weight {
		self.backend.max_block_weight()
	}

	/// Get the block notifier.
	pub fn block_notifier(&self) -> Option<tokio::sync::broadcast::Sender<H256>> {
		self.block_notifier.clone()
	}

	/// Check if automine is enabled.
	pub fn is_automine(&self) -> bool {
		self.automine
	}

	/// Get the automine status from the node.
	pub async fn get_automine(&self) -> bool {
		self.backend.get_automine().await
	}

	/// Get the EVM block for the given block.
	pub async fn evm_block(
		&self,
		block: Arc<BP::Block>,
		hydrated_transactions: bool,
	) -> Option<Block> {
		log::trace!(
			target: LOG_TARGET,
			"Get Ethereum block for hash {:?}",
			block.hash()
		);
		match self.backend.eth_block(block.hash()).await {
			Ok(mut eth_block) => {
				log::trace!(
					target: LOG_TARGET,
					"Ethereum block from runtime API hash {:?}",
					eth_block.hash
				);

				if hydrated_transactions {
					// Hydrate the block.
					let tx_infos = self
						.receipt_provider
						.receipts_from_block_by_hash(block.hash(), block.number())
						.await
						.unwrap_or_default()
						.into_iter()
						.map(|(signed_tx, receipt)| receipt.transaction_info(signed_tx))
						.collect::<Vec<_>>();

					eth_block.transactions = HashesOrTransactionInfos::TransactionInfos(tx_infos);
				}

				Some(eth_block)
			},
			Err(err) => {
				log::error!(
					target: LOG_TARGET,
					"Failed to get Ethereum block for hash {:?}: {err:?}",
					block.hash()
				);
				None
			},
		}
	}

	/// Get the gas price at the given block.
	pub async fn gas_price_at(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<sp_core::U256, ClientError> {
		self.backend.gas_price(block_hash).await
	}

	/// Get the balance at the given block.
	pub async fn balance_at(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
	) -> Result<sp_core::U256, ClientError> {
		self.backend.balance(block_hash, address).await
	}

	/// Get the nonce at the given block.
	pub async fn nonce_at(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
	) -> Result<sp_core::U256, ClientError> {
		self.backend.nonce(block_hash, address).await
	}

	/// Get the code at the given block.
	pub async fn code_at(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
	) -> Result<Vec<u8>, ClientError> {
		self.backend.code(block_hash, address).await
	}

	/// Get the storage at the given block.
	pub async fn storage_at(
		&self,
		block_hash: SubstrateBlockHash,
		address: sp_core::H160,
		key: [u8; 32],
	) -> Result<Option<Vec<u8>>, ClientError> {
		self.backend.get_storage(block_hash, address, key).await
	}

	/// Estimate the gas for the given transaction using binary search.
	pub async fn estimate_gas(
		&self,
		block_hash: SubstrateBlockHash,
		tx: GenericTransaction,
		block: BlockId,
	) -> Result<pallet_revive::evm::U256, ClientError> {
		self.backend.estimate_gas(block_hash, tx, block).await
	}

	/// Dry-run a transaction and return the estimated gas as `U256`.
	pub async fn dry_run(
		&self,
		block_hash: SubstrateBlockHash,
		tx: GenericTransaction,
		block: BlockId,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<pallet_revive::evm::U256, ClientError> {
		self.backend
			.dry_run(block_hash, tx, block, state_overrides)
			.await
			.map(|info| info.eth_gas)
	}

	/// Fetch the raw signed block (used for tracing).
	async fn tracing_block(
		&self,
		block_hash: H256,
	) -> Result<
		sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		ClientError,
	> {
		self.backend.signed_block(block_hash).await
	}

	/// Get the transaction traces for the given block.
	pub async fn trace_block_by_number(
		&self,
		at: BlockNumberOrTag,
		config: TracerTypeV1,
	) -> Result<Vec<TransactionTrace>, ClientError> {
		if self.receipt_provider.is_before_earliest_block(&at) {
			return Ok(vec![]);
		}

		let block_hash = self.block_hash_for_tag(at.into()).await?;
		let block = self.tracing_block(block_hash).await?;
		if block.header.parent_hash == Default::default() {
			return Ok(vec![]);
		}
		let traces = self.backend.trace_block(block_hash, block, config).await?;

		let mut hashes = self
			.receipt_provider
			.block_transaction_hashes(&block_hash)
			.await
			.ok_or(ClientError::EthExtrinsicNotFound)?;

		let traces = traces.into_iter().filter_map(|(index, trace)| {
			Some(TransactionTrace { tx_hash: hashes.remove(&(index as usize))?, trace })
		});

		Ok(traces.collect())
	}

	/// Get the transaction traces for the given transaction.
	pub async fn trace_transaction(
		&self,
		transaction_hash: H256,
		config: TracerTypeV1,
	) -> Result<TraceV1, ClientError> {
		let (block_hash, transaction_index) = self
			.receipt_provider
			.find_transaction(&transaction_hash)
			.await
			.ok_or(ClientError::EthExtrinsicNotFound)?;

		let block = self.tracing_block(block_hash).await?;
		self.backend.trace_tx(block_hash, block, transaction_index as u32, config).await
	}

	/// Dry-run a call and return its trace.
	pub async fn trace_call(
		&self,
		transaction: GenericTransaction,
		block: BlockId,
		config: TracerTypeV1,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<TraceV1, ClientError> {
		let block_hash = self.block_hash_for_tag(block).await?;
		self.backend.trace_call(block_hash, transaction, config, state_overrides).await
	}

	/// Get the post-dispatch weight associated with this Ethereum transaction hash.
	pub async fn post_dispatch_weight(&self, tx_hash: &H256) -> Option<Weight> {
		let receipt = self.receipt(tx_hash).await?;
		let block_hash = self.resolve_substrate_hash(&receipt.block_hash).await?;
		self.backend
			.extrinsic_post_dispatch_weight(block_hash, receipt.transaction_index.as_usize())
			.await
	}

	/// Get the logs matching the given filter.
	pub async fn logs(&self, filter: Option<Filter>) -> Result<Vec<Log>, ClientError> {
		let logs =
			self.receipt_provider
				.logs(filter, |tag| async move {
					self.resolve_tag_to_number(tag).await.map_err(Into::into)
				})
				.await
				.map_err(ClientError::LogFilterFailed)?;

		Ok(logs)
	}

	pub async fn fee_history(
		&self,
		block_count: u32,
		latest_block: BlockNumberOrTag,
		reward_percentiles: Option<Vec<f64>>,
	) -> Result<FeeHistoryResult, ClientError> {
		let Some(latest_block) = self.block_by_number_or_tag(&latest_block).await? else {
			return Err(ClientError::BlockNotFound);
		};

		self.fee_history_provider
			.fee_history(block_count, latest_block.number(), reward_percentiles)
			.await
	}
}

#[cfg(feature = "subxt")]
impl Client<crate::subxt_client::SubxtClient, crate::subxt_client::SubxtBlockInfoProvider> {
	/// Expose the underlying subxt OnlineClient.
	pub fn api(&self) -> &subxt::OnlineClient<crate::subxt_client::SrcChainConfig> {
		&self.backend.api
	}
}
