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

//! Native in-process [`SubstrateClient`] implementation.
//!
//! [`NativeSubstrateClient`] drives the ETH-RPC server directly through the
//! Substrate node's in-memory client APIs (`sc_client_api`, `sp_api`, etc.),
//! removing the need for a separate `subxt`/WebSocket connection when the RPC
//! server is embedded inside the node binary.
use crate::{
	BlockId, ClientError, SubstrateClient,
	client::{Balance, SubscriptionType, SubstrateBlockHash, SubstrateBlockNumber},
	native_block_info_provider::NativeNormalizedBlock,
	substrate_client::{NodeHealth, RawExtrinsic, SubmitResult},
};
use codec::{Decode, Encode};
use futures::StreamExt;
use jsonrpsee::core::async_trait;
use pallet_revive::{ReviveApi, evm::U256};
use pallet_revive_types::runtime_api::{
	BlockV1 as EthBlock, EthTransactInfoV1 as EthTransactInfo,
	GenericTransactionV1 as GenericTransaction, StateOverrideSetV1 as StateOverrideSet, *,
};
use sc_client_api::{BlockBackend, BlockchainEvents, HeaderBackend};
use sc_network::NetworkStatusProvider;
use sc_transaction_pool_api::TransactionPool;
use sp_api::{ApiExt, Metadata, ProvideRuntimeApi};
use sp_consensus::SyncOracle;
use sp_core::{H160, H256};
use sp_runtime::{
	OpaqueExtrinsic,
	traits::{Block as BlockT, Header as HeaderT},
};
use sp_weights::Weight;
use std::{marker::PhantomData, sync::Arc};

/// Convenience bound that bundles all runtime-API traits required by the native client.
pub trait ReviveRuntimeApiT<Block: BlockT, Moment: codec::Codec>:
	pallet_revive::ReviveApi<Block, sp_core::H160, Balance, u32, u128, Moment>
	+ sp_api::Core<Block>
	+ sp_api::Metadata<Block>
{
}

impl<T, Block: BlockT, Moment: codec::Codec> ReviveRuntimeApiT<Block, Moment> for T where
	T: pallet_revive::ReviveApi<Block, sp_core::H160, Balance, u32, u128, Moment>
		+ sp_api::Core<Block>
		+ sp_api::Metadata<Block>
{
}

pub use crate::substrate_client::OpaqueBlock;

#[inline]
fn native_err(e: impl std::fmt::Display) -> ClientError {
	ClientError::NativeClientError(e.to_string())
}

/// A closure that, given a block hash and extrinsic index, returns the post-dispatch
/// `Weight` for that extrinsic by scanning block events.
type FetchExtrinsicWeightFn =
	Arc<dyn Fn(SubstrateBlockHash, usize) -> Option<Weight> + Send + Sync>;

/// A [`SubstrateClient`] backed by the node's native in-process Substrate client.
pub struct NativeSubstrateClient<Client, Pool, Block = OpaqueBlock, Moment = u64>
where
	Block: BlockT,
{
	client: Arc<Client>,
	pool: Arc<Pool>,
	chain_id: u64,
	max_block_weight: Weight,
	automine: bool,
	/// The runtime index of `pallet-revive`, discovered from metadata at construction time.
	pallet_revive_index: u8,
	/// Optional network-status provider for real peer count in `system_health`.
	network: Option<Arc<dyn NetworkStatusProvider + Send + Sync>>,
	/// Optional sync oracle for real `is_syncing` in `system_health`.
	sync_oracle: Option<Arc<dyn SyncOracle + Send + Sync>>,
	/// Optional hook that scans block events to return post-dispatch weight.
	fetch_extrinsic_weight: Option<FetchExtrinsicWeightFn>,
	_block: PhantomData<Block>,
	_moment: PhantomData<Moment>,
}

impl<Client, Pool, Block, Moment> Clone for NativeSubstrateClient<Client, Pool, Block, Moment>
where
	Block: BlockT,
{
	fn clone(&self) -> Self {
		Self {
			client: self.client.clone(),
			pool: self.pool.clone(),
			chain_id: self.chain_id,
			max_block_weight: self.max_block_weight,
			automine: self.automine,
			pallet_revive_index: self.pallet_revive_index,
			network: self.network.clone(),
			sync_oracle: self.sync_oracle.clone(),
			fetch_extrinsic_weight: self.fetch_extrinsic_weight.clone(),
			_block: PhantomData,
			_moment: PhantomData,
		}
	}
}

impl<Client, Pool, Block, Moment> NativeSubstrateClient<Client, Pool, Block, Moment>
where
	Block: BlockT<Hash = H256>,
	Block::Header: HeaderT<Number = u32>,
	Moment: codec::Codec + From<u64> + Send + Sync + 'static,
	Client: HeaderBackend<Block>
		+ ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: ReviveRuntimeApiT<Block, Moment>,
	Pool: TransactionPool<Block = Block, Hash = H256> + Send + Sync + 'static,
{
	/// Create a new native client.
	#[allow(deprecated)]
	pub fn new(
		client: Arc<Client>,
		pool: Arc<Pool>,
		chain_id: u64,
		automine: bool,
	) -> Result<Self, ClientError> {
		let best_hash = client.info().best_hash;
		let runtime_api = client.runtime_api();

		let block_gas_limit: u64 = if Self::versioned_api_available(&client, best_hash) {
			runtime_api
				.block_gas_limit_versioned(
					best_hash,
					BlockGasLimitVersionedInputPayload::from(BlockGasLimitInputPayloadV1),
				)
				.ok()
				.and_then(|output| BlockGasLimitOutputPayloadV1::try_from(output).ok())
				.map(|output| output.block_gas_limit.min(U256::from(u64::MAX)).as_u64())
				.unwrap_or(u64::MAX)
		} else {
			runtime_api
				.block_gas_limit(best_hash)
				.map(|v: U256| v.min(U256::from(u64::MAX)).as_u64())
				.unwrap_or(u64::MAX)
		};

		let pallet_revive_index = Self::discover_pallet_index(&client, best_hash)?;

		Ok(Self {
			client,
			pool,
			chain_id,
			max_block_weight: Weight::from_parts(block_gas_limit, u64::MAX),
			automine,
			pallet_revive_index,
			network: None,
			sync_oracle: None,
			fetch_extrinsic_weight: None,
			_block: PhantomData,
			_moment: PhantomData,
		})
	}

	pub fn with_network(
		mut self,
		network: Arc<dyn NetworkStatusProvider + Send + Sync>,
		sync_oracle: Arc<dyn SyncOracle + Send + Sync>,
	) -> Self {
		self.network = Some(network);
		self.sync_oracle = Some(sync_oracle);
		self
	}

	pub fn with_event_reader(
		mut self,
		f: impl Fn(SubstrateBlockHash, usize) -> Option<Weight> + Send + Sync + 'static,
	) -> Self {
		self.fetch_extrinsic_weight = Some(Arc::new(f));
		self
	}

	/// Whether the runtime at `at` exposes the versioned revive runtime API.
	fn versioned_api_available(client: &Arc<Client>, at: Block::Hash) -> bool {
		client
			.runtime_api()
			.api_version::<dyn ReviveApi<Block, H160, Balance, u32, u128, Moment>>(at)
			.ok()
			.flatten()
			.is_some_and(|version| version >= 2)
	}

	/// Read the pallet index of `pallet-revive` from the runtime metadata.
	fn discover_pallet_index(
		client: &Arc<Client>,
		best_hash: Block::Hash,
	) -> Result<u8, ClientError> {
		let opaque = client.runtime_api().metadata(best_hash).map_err(native_err)?;
		let meta = subxt_metadata::Metadata::decode(&mut &opaque[..])?;
		meta.pallet_by_name("Revive")
			.map(|p| p.call_index())
			.ok_or_else(|| native_err("`pallet-revive` not found in the runtime metadata"))
	}

	/// Build a [`NativeNormalizedBlock`] from a block hash by querying the in-process client.
	fn block_info_from_hash(
		&self,
		block_hash: H256,
	) -> Result<Option<NativeNormalizedBlock>, ClientError> {
		let Some(signed) = self.client.block(block_hash).map_err(native_err)? else {
			return Ok(None);
		};

		let encoded = signed.block.encode();
		let opaque = OpaqueBlock::decode(&mut &encoded[..])?;

		let number: u32 = (*signed.block.header().number()).into();
		Ok(Some(NativeNormalizedBlock {
			hash: block_hash,
			number,
			parent_hash: *signed.block.header().parent_hash(),
			opaque,
		}))
	}
}

#[async_trait]
#[allow(deprecated)]
impl<Client, Pool, Block, Moment> SubstrateClient
	for NativeSubstrateClient<Client, Pool, Block, Moment>
where
	Block: BlockT<Hash = H256, Extrinsic = OpaqueExtrinsic> + Send + Sync + 'static,
	Block::Header: HeaderT<Number = u32, Hash = H256> + Send + Sync,
	Moment: codec::Codec + Clone + From<u64> + Send + Sync + 'static,
	Client: HeaderBackend<Block>
		+ ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ BlockchainEvents<Block>
		+ Send
		+ Sync
		+ 'static,
	Client::Api: ReviveRuntimeApiT<Block, Moment>,
	Pool: TransactionPool<Block = Block, Hash = H256> + Send + Sync + 'static,
{
	type BlockInfo = NativeNormalizedBlock;

	fn chain_id(&self) -> u64 {
		self.chain_id
	}

	fn max_block_weight(&self) -> Weight {
		self.max_block_weight
	}

	async fn block_by_hash(
		&self,
		hash: &SubstrateBlockHash,
	) -> Result<Option<NativeNormalizedBlock>, ClientError> {
		self.block_info_from_hash(*hash)
	}

	async fn block_by_number(
		&self,
		number: SubstrateBlockNumber,
	) -> Result<Option<NativeNormalizedBlock>, ClientError> {
		let hash = self.client.block_hash(number.into()).map_err(native_err)?;
		match hash {
			Some(h) => self.block_info_from_hash(h),
			None => Ok(None),
		}
	}

	async fn latest_block(&self) -> Result<NativeNormalizedBlock, ClientError> {
		let info = self.client.info();
		self.block_info_from_hash(info.best_hash)?.ok_or(ClientError::BlockNotFound)
	}

	async fn latest_finalized_block(&self) -> Result<NativeNormalizedBlock, ClientError> {
		let info = self.client.info();
		self.block_info_from_hash(info.finalized_hash)?
			.ok_or(ClientError::BlockNotFound)
	}

	async fn dry_run(
		&self,
		block_hash: SubstrateBlockHash,
		tx: GenericTransaction,
		block: BlockId,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<EthTransactInfo<Balance>, ClientError> {
		let timestamp_override: Option<Moment> = block
			.is_pending()
			.then(|| Moment::from(sp_timestamp::Timestamp::current().as_millis()));

		if Self::versioned_api_available(&self.client, block_hash) {
			let input = TransactVersionedInputPayload::from(TransactInputPayloadV1 {
				tx,
				timestamp_override,
				perform_balance_checks: false,
				state_overrides,
			});
			let output = self
				.client
				.runtime_api()
				.eth_transact_versioned(block_hash, input)
				.map_err(native_err)?
				.map_err(ClientError::TransactError)?;
			Ok(TransactOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.transact_info)
		} else {
			let config =
				DryRunConfigV1 { timestamp_override, state_overrides, ..Default::default() };
			self.client
				.runtime_api()
				.eth_transact_with_config(block_hash, tx, config)
				.map_err(native_err)?
				.map_err(ClientError::TransactError)
		}
	}

	async fn estimate_gas(
		&self,
		block_hash: SubstrateBlockHash,
		tx: GenericTransaction,
		block: BlockId,
	) -> Result<U256, ClientError> {
		let timestamp_override: Option<Moment> = block
			.is_pending()
			.then(|| Moment::from(sp_timestamp::Timestamp::current().as_millis()));

		if Self::versioned_api_available(&self.client, block_hash) {
			let input = EstimateGasVersionedInputPayload::from(EstimateGasInputPayloadV1 {
				tx,
				timestamp_override,
				state_overrides: None,
			});
			let output = self
				.client
				.runtime_api()
				.eth_estimate_gas_versioned(block_hash, input)
				.map_err(native_err)?
				.map_err(ClientError::TransactError)?;
			Ok(EstimateGasOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.gas_estimate)
		} else {
			let config = DryRunConfigV1 { timestamp_override, ..Default::default() };
			self.client
				.runtime_api()
				.eth_estimate_gas(block_hash, tx, config)
				.map_err(native_err)?
				.map_err(ClientError::TransactError)
		}
	}

	async fn gas_price(&self, block_hash: SubstrateBlockHash) -> Result<U256, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = GasPriceVersionedInputPayload::from(GasPriceInputPayloadV1);
			let output = self
				.client
				.runtime_api()
				.gas_price_versioned(block_hash, input)
				.map_err(native_err)?;
			Ok(GasPriceOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.gas_price)
		} else {
			self.client.runtime_api().gas_price(block_hash).map_err(native_err)
		}
	}

	async fn balance(
		&self,
		block_hash: SubstrateBlockHash,
		address: H160,
	) -> Result<U256, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = BalanceVersionedInputPayload::from(BalanceInputPayloadV1 { address });
			let output = self
				.client
				.runtime_api()
				.balance_versioned(block_hash, input)
				.map_err(native_err)?;
			Ok(BalanceOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.balance)
		} else {
			self.client.runtime_api().balance(block_hash, address).map_err(native_err)
		}
	}

	async fn nonce(
		&self,
		block_hash: SubstrateBlockHash,
		address: H160,
	) -> Result<U256, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = NonceVersionedInputPayload::from(NonceInputPayloadV1 { address });
			let output = self
				.client
				.runtime_api()
				.nonce_versioned(block_hash, input)
				.map_err(native_err)?;
			Ok(U256::from(
				NonceOutputPayloadV1::try_from(output)
					.map_err(|_| native_err("v1 input must produce v1 output"))?
					.nonce,
			))
		} else {
			self.client
				.runtime_api()
				.nonce(block_hash, address)
				.map(|n: u32| U256::from(n))
				.map_err(native_err)
		}
	}

	async fn code(
		&self,
		block_hash: SubstrateBlockHash,
		address: H160,
	) -> Result<Vec<u8>, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = CodeVersionedInputPayload::from(CodeInputPayloadV1 { address });
			let output = self
				.client
				.runtime_api()
				.code_versioned(block_hash, input)
				.map_err(native_err)?;
			Ok(CodeOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.code)
		} else {
			self.client.runtime_api().code(block_hash, address).map_err(native_err)
		}
	}

	async fn get_storage(
		&self,
		block_hash: SubstrateBlockHash,
		address: H160,
		key: [u8; 32],
	) -> Result<Option<Vec<u8>>, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = GetStorageVersionedInputPayload::from(GetStorageInputPayloadV1 {
				address,
				key: StorageKeyV1::Fixed(key),
			});
			let output = self
				.client
				.runtime_api()
				.get_storage_versioned(block_hash, input)
				.map_err(native_err)?
				.map_err(|_| ClientError::ContractNotFound)?;
			Ok(GetStorageOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.storage)
		} else {
			self.client
				.runtime_api()
				.get_storage(block_hash, address, key)
				.map_err(native_err)?
				.map_err(|_| ClientError::ContractNotFound)
		}
	}

	async fn eth_block(&self, block_hash: SubstrateBlockHash) -> Result<EthBlock, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = BlockVersionedInputPayload::from(BlockInputPayloadV1);
			let output = self
				.client
				.runtime_api()
				.eth_block_versioned(block_hash, input)
				.map_err(native_err)?;
			Ok(BlockOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.block)
		} else {
			self.client.runtime_api().eth_block(block_hash).map_err(native_err)
		}
	}

	async fn eth_block_hash(
		&self,
		block_hash: SubstrateBlockHash,
		number: U256,
	) -> Result<Option<H256>, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = BlockHashVersionedInputPayload::from(BlockHashInputPayloadV1 {
				block_number: number,
			});
			let output = self
				.client
				.runtime_api()
				.eth_block_hash_versioned(block_hash, input)
				.map_err(native_err)?;
			Ok(BlockHashOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.block_hash)
		} else {
			self.client.runtime_api().eth_block_hash(block_hash, number).map_err(native_err)
		}
	}

	async fn eth_receipt_data(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<Vec<ReceiptGasInfoV1>, ClientError> {
		if Self::versioned_api_available(&self.client, block_hash) {
			let input = ReceiptDataVersionedInputPayload::from(ReceiptDataInputPayloadV1);
			let output = self
				.client
				.runtime_api()
				.eth_receipt_data_versioned(block_hash, input)
				.map_err(native_err)?;
			Ok(ReceiptDataOutputPayloadV1::try_from(output)
				.map_err(|_| native_err("v1 input must produce v1 output"))?
				.receipt_data)
		} else {
			self.client.runtime_api().eth_receipt_data(block_hash).map_err(native_err)
		}
	}

	async fn trace_block(
		&self,
		_block_hash: SubstrateBlockHash,
		block: OpaqueBlock,
		config: TracerTypeV1,
	) -> Result<Vec<(u32, TraceV1)>, ClientError> {
		let parent = *block.header().parent_hash();
		let block_generic: Block = Block::decode(&mut &block.encode()[..])?;
		self.client
			.runtime_api()
			.trace_block(parent, block_generic, config)
			.map_err(native_err)
	}

	async fn trace_tx(
		&self,
		_block_hash: SubstrateBlockHash,
		block: OpaqueBlock,
		transaction_index: u32,
		config: TracerTypeV1,
	) -> Result<TraceV1, ClientError> {
		let parent = *block.header().parent_hash();
		let block_generic: Block = Block::decode(&mut &block.encode()[..])?;
		self.client
			.runtime_api()
			.trace_tx(parent, block_generic, transaction_index, config)
			.map_err(native_err)?
			.ok_or(ClientError::EthExtrinsicNotFound)
	}

	async fn trace_call(
		&self,
		block_hash: SubstrateBlockHash,
		transaction: GenericTransaction,
		config: TracerTypeV1,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<TraceV1, ClientError> {
		if let Some(overrides) = state_overrides {
			let tracing_config = TracingConfigV1 { state_overrides: Some(overrides) };
			self.client
				.runtime_api()
				.trace_call_with_config(block_hash, transaction, config, tracing_config)
				.map_err(native_err)?
				.map_err(ClientError::TransactError)
		} else {
			self.client
				.runtime_api()
				.trace_call(block_hash, transaction, config)
				.map_err(native_err)?
				.map_err(ClientError::TransactError)
		}
	}

	async fn submit_extrinsic(&self, payload: Vec<u8>) -> Result<SubmitResult, ClientError> {
		let opaque_xt = OpaqueExtrinsic::try_from_encoded_extrinsic(&payload)
			.map_err(|_| ClientError::TxDecodingFailed)?;

		let at = self.client.info().best_hash;

		let mut status_stream = self
			.pool
			.submit_and_watch(at, sc_transaction_pool_api::TransactionSource::External, opaque_xt)
			.await
			.map_err(native_err)?;

		// Mirrors the subxt client: first pool status wins, failed statuses reject.
		tokio::time::timeout(std::time::Duration::from_secs(5), async {
			match status_stream.next().await {
				Some(
					status @ (SubmitResult::Usurped(_) |
					SubmitResult::Dropped |
					SubmitResult::Invalid),
				) => Err(ClientError::SubmitError(crate::client::SubmitError::from(status))),
				Some(status) => Ok(status),
				None => Err(ClientError::SubmitError(crate::client::SubmitError::StreamEnded)),
			}
		})
		.await
		.map_err(|_| ClientError::Timeout)?
	}

	async fn sync_state(
		&self,
	) -> Result<sc_rpc::system::SyncState<SubstrateBlockNumber>, ClientError> {
		let info = self.client.info();
		Ok(sc_rpc::system::SyncState {
			starting_block: 0,
			current_block: info.best_number,
			highest_block: info.best_number,
		})
	}

	async fn system_health(&self) -> Result<NodeHealth, ClientError> {
		match &self.network {
			Some(net) => {
				let status = net.status().await.map_err(|_| {
					ClientError::NativeClientError(
						"network status provider returned an error".to_string(),
					)
				})?;
				let is_syncing =
					self.sync_oracle.as_ref().map(|o| o.is_major_syncing()).unwrap_or(false);
				Ok(NodeHealth {
					peers: status.num_connected_peers,
					is_syncing,
					should_have_peers: true,
				})
			},
			None => Ok(NodeHealth { peers: 0, is_syncing: false, should_have_peers: false }),
		}
	}

	fn pallet_revive_index(&self) -> u8 {
		self.pallet_revive_index
	}

	async fn get_automine(&self) -> bool {
		self.automine
	}

	async fn subscribe_blocks(
		&self,
		subscription_type: SubscriptionType,
	) -> Result<tokio::sync::mpsc::Receiver<NativeNormalizedBlock>, ClientError> {
		log::debug!(
			target: crate::LOG_TARGET,
			"NativeSubstrateClient::subscribe_blocks ({subscription_type:?}): \
			 native notification stream active"
		);

		let (tx, rx) = tokio::sync::mpsc::channel(crate::client::NOTIFIER_CAPACITY);
		let this = self.clone();
		tokio::spawn(async move {
			match subscription_type {
				SubscriptionType::BestBlocks => {
					let mut stream = this.client.import_notification_stream();
					while let Some(notification) = stream.next().await {
						if !notification.is_new_best {
							continue;
						}

						let hash = notification.hash;
						let block_info = match this.block_info_from_hash(hash) {
							Ok(Some(b)) => b,
							Ok(None) => {
								log::warn!(
									target: crate::LOG_TARGET,
									"NativeSubstrateClient: best-block notification for unknown hash {:?}",
									hash
								);
								continue;
							},
							Err(err) => {
								log::error!(
									target: crate::LOG_TARGET,
									"NativeSubstrateClient: failed to fetch best-block info: {err:?}"
								);
								continue;
							},
						};

						log::trace!(
							target: crate::LOG_TARGET,
							"NativeSubstrateClient: new best block #{} ({:?})",
							block_info.number,
							block_info.hash
						);

						if tx.send(block_info).await.is_err() {
							break;
						}
					}
				},

				SubscriptionType::FinalizedBlocks => {
					let mut stream = this.client.finality_notification_stream();
					while let Some(notification) = stream.next().await {
						let hash = notification.hash;
						let block_info = match this.block_info_from_hash(hash) {
							Ok(Some(b)) => b,
							Ok(None) => {
								log::warn!(
									target: crate::LOG_TARGET,
									"NativeSubstrateClient: finalized-block notification for unknown hash {:?}",
									hash
								);
								continue;
							},
							Err(err) => {
								log::error!(
									target: crate::LOG_TARGET,
									"NativeSubstrateClient: failed to fetch finalized-block info: {err:?}"
								);
								continue;
							},
						};

						log::trace!(
							target: crate::LOG_TARGET,
							"NativeSubstrateClient: finalized block #{} ({:?})",
							block_info.number,
							block_info.hash
						);

						if tx.send(block_info).await.is_err() {
							break;
						}
					}
				},
			}
		});

		Ok(rx)
	}

	async fn signed_block(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<OpaqueBlock, ClientError> {
		let signed = self
			.client
			.block(block_hash)
			.map_err(native_err)?
			.ok_or(ClientError::BlockNotFound)?;

		let encoded = signed.block.encode();
		OpaqueBlock::decode(&mut &encoded[..]).map_err(ClientError::CodecError)
	}

	async fn extrinsic_post_dispatch_weight(
		&self,
		block_hash: SubstrateBlockHash,
		extrinsic_index: usize,
	) -> Option<Weight> {
		self.fetch_extrinsic_weight
			.as_ref()
			.and_then(|f| f(block_hash, extrinsic_index))
	}

	async fn block_extrinsics(
		&self,
		block_hash: SubstrateBlockHash,
	) -> Result<Vec<RawExtrinsic>, ClientError> {
		let body = self
			.client
			.block_body(block_hash)
			.map_err(native_err)?
			.ok_or(ClientError::BlockNotFound)?;

		Ok(body
			.into_iter()
			.enumerate()
			.map(|(index, ext)| RawExtrinsic { payload: ext.encode(), index })
			.collect())
	}

	async fn block_events(
		&self,
		_block_hash: SubstrateBlockHash,
	) -> Result<Option<crate::receipt_extractor::BlockEvents>, ClientError> {
		// In-process event scanning is a follow up; receipts carry no logs until then.
		Ok(None)
	}
}
