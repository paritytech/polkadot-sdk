// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//  http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use futures::{FutureExt, TryFutureExt, future::BoxFuture};
use pallet_revive::evm::{H160, U256};
use pallet_revive_types::runtime_api::*;
use sp_core::H256;
use sp_timestamp::Timestamp;
use std::future::Future;
use subxt::{OnlineClient, ext::frame_metadata::v16::RuntimeMetadataV16, runtime_api::RuntimeApi};

use crate::{
	BlockId,
	client::{Balance, ClientError},
	subxt_client::{self, SrcChainConfig},
};

use MethodStatus::*;
use MethodVersioningStatus::*;

/// A version-aware runtime API wrapper for pallet-revive.
///
/// This is an abstraction for the runtime API of pallet-revive which allows us to call the methods
/// appropriate for each version without this leaking into the rest of the codebase.
///
/// All of the futures returned by this layer are [`Option<BoxFuture>`]. This is an intentional
/// decision so that versions which do not support the required runtime API functions return no
/// future which needs to be awaited nor resolved. For example, calling a runtime API function on
/// a version of asset-hub which doesn't have pallet-revive should yield [`None`] rather than us
/// attempting to call the method only for us to find that it's not available.
///
/// This layer doesn't have a 1:1 mapping of abstraction method to runtime API method. Rather, the
/// methods on this layer are grouped conceptually. For example, the [`estimate_gas`] method could
/// call any of the runtime API functions `estimate_gas`, `eth_transact_with_config`, or
/// `eth_transact` depending on their availability, which we do in this layer since it's the layer
/// that holds the information and the knowledge of what runtime API functions are available and
/// what isn't. Therefore it's not a 1:1 mapping.
///
/// [`estimate_gas`]: Self::estimate_gas
pub struct VersionAwareRuntimeApi {
	runtime_api: RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>,
	capabilities: ReviveRuntimeApiCapabilities,
}

impl VersionAwareRuntimeApi {
	/// Create a new instance.
	pub fn new(
		runtime_api: RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>,
		capabilities: ReviveRuntimeApiCapabilities,
	) -> Self {
		Self { runtime_api, capabilities }
	}

	/// Get the balance of the given address.
	pub fn balance(&self, address: H160) -> Option<BoxFuture<'_, Result<U256, ClientError>>> {
		self.capabilities.balance.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis().revive_api().balance(address).unvalidated();
					runtime_api.call(payload).await.map(|balance| balance.0).map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = BalanceInputPayloadV1 { address };
					let payload = subxt_client::apis()
						.revive_api()
						.balance_versioned(BalanceVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							BalanceOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.balance
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the contract storage for the given contract address and key.
	pub fn get_storage(
		&self,
		contract_address: H160,
		key: [u8; 32],
	) -> Option<BoxFuture<'_, Result<Option<Vec<u8>>, ClientError>>> {
		self.capabilities.get_storage.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis()
						.revive_api()
						.get_storage(contract_address, key)
						.unvalidated();
					runtime_api.call(payload).await?.map_err(|_| ClientError::ContractNotFound)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = GetStorageInputPayloadV1 {
						address: contract_address,
						key: StorageKeyV1::Fixed(key),
					};
					let payload = subxt_client::apis()
						.revive_api()
						.get_storage_versioned(GetStorageVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await?
						.map(|output| {
							GetStorageOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.storage
						})
						.map_err(|_| ClientError::ContractNotFound)
				}
			},
		)
	}

	/// Estimates the minimum gas limit required for the transaction execution. Returns a [`U256`]
	/// of the gas limit.
	///
	/// Falls back to a [`dry_run`] of the transaction on runtimes which do not have an estimation
	/// runtime API function available.
	///
	/// [`dry_run`]: Self::dry_run
	pub fn estimate_gas(
		&self,
		tx: GenericTransactionV1,
		block: BlockId,
	) -> Option<BoxFuture<'_, Result<U256, ClientError>>> {
		let timestamp_override = block.is_pending().then(|| Timestamp::current().as_millis());

		self.capabilities
			.eth_estimate_gas
			.handle(
				|| {
					let runtime_api = self.runtime_api.clone();
					let tx = tx.clone();
					async move {
						let config = DryRunConfigV1 { timestamp_override, ..Default::default() };
						let payload = subxt_client::apis()
							.revive_api()
							.eth_estimate_gas(tx.into(), config.into())
							.unvalidated();
						runtime_api
							.call(payload)
							.await?
							.map(|gas_estimate| gas_estimate.0)
							.map_err(|err| ClientError::TransactError(err.0))
					}
				},
				|_| {
					let runtime_api = self.runtime_api.clone();
					let tx = tx.clone();
					async move {
						let input = EstimateGasInputPayloadV1 {
							tx,
							timestamp_override,
							state_overrides: None,
						};
						let payload = subxt_client::apis().revive_api().eth_estimate_gas_versioned(
							EstimateGasVersionedInputPayload::from(input).into(),
						);
						runtime_api
							.call(payload)
							.await?
							.map(|output| {
								EstimateGasOutputPayloadV1::try_from(output.0)
									.expect("v1 input must produce v1 output; qed")
									.gas_estimate
							})
							.map_err(|err| ClientError::TransactError(err.0))
					}
				},
			)
			.or_else(|| {
				self.dry_run(tx.clone(), block, None)
					.map(|future| future.map_ok(|info| info.eth_gas).boxed())
			})
	}

	/// Dry run a transaction and return the [`EthTransactInfoV1`] for the transaction.
	pub fn dry_run(
		&self,
		tx: GenericTransactionV1,
		block: BlockId,
		state_overrides: Option<StateOverrideSetV1>,
	) -> Option<BoxFuture<'_, Result<EthTransactInfoV1<Balance>, ClientError>>> {
		let timestamp_override = block.is_pending().then(|| Timestamp::current().as_millis());

		let eth_transact_future =
			self.capabilities.eth_transact.handle::<'_, _, _, _, BoxFuture<'_, _>, _>(
				|| {
					if state_overrides.is_some() {
						return None;
					};

					let tx = tx.clone();
					let runtime_api = self.runtime_api.clone();
					Some(Box::pin(async move {
						let payload =
							subxt_client::apis().revive_api().eth_transact(tx.into()).unvalidated();
						runtime_api
							.call(payload)
							.await?
							.map(|transact_info| transact_info.0)
							.map_err(|err| ClientError::TransactError(err.0))
					}))
				},
				|_| {
					let tx = tx.clone();
					let state_overrides = state_overrides.clone();
					let runtime_api = self.runtime_api.clone();
					async move {
						let input = TransactInputPayloadV1 {
							tx,
							timestamp_override,
							perform_balance_checks: true,
							state_overrides,
						};
						let payload = subxt_client::apis().revive_api().eth_transact_versioned(
							TransactVersionedInputPayload::from(input).into(),
						);
						runtime_api
							.call(payload)
							.await?
							.map(|output| {
								TransactOutputPayloadV1::try_from(output.0)
									.expect("v1 input must produce v1 output; qed")
									.transact_info
							})
							.map_err(|err| ClientError::TransactError(err.0))
					}
				},
			);
		let eth_transact_with_config_future = self
			.capabilities
			.eth_transact_with_config
			.handle::<'_, _, _, _, _, BoxFuture<'_, _>>(
				|| {
					let tx = tx.clone();
					let state_overrides = state_overrides.clone();
					let runtime_api = self.runtime_api.clone();
					async move {
						let config = DryRunConfigV1 {
							timestamp_override,
							state_overrides,
							..Default::default()
						};
						let payload = subxt_client::apis()
							.revive_api()
							.eth_transact_with_config(tx.into(), config.into())
							.unvalidated();
						runtime_api
							.call(payload)
							.await?
							.map(|transact_info| transact_info.0)
							.map_err(|err| ClientError::TransactError(err.0))
					}
				},
				|_| None::<BoxFuture<'_, _>>,
			);

		eth_transact_future.or(eth_transact_with_config_future)
	}

	/// Get the nonce of the given address.
	pub fn nonce(&self, address: H160) -> Option<BoxFuture<'_, Result<U256, ClientError>>> {
		self.capabilities.nonce.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis().revive_api().nonce(address).unvalidated();
					runtime_api.call(payload).await.map(|nonce| nonce.into()).map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = NonceInputPayloadV1 { address };
					let payload = subxt_client::apis()
						.revive_api()
						.nonce_versioned(NonceVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							NonceOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.nonce
								.into()
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the gas price.
	pub fn gas_price(&self) -> Option<BoxFuture<'_, Result<U256, ClientError>>> {
		self.capabilities.gas_price.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis().revive_api().gas_price().unvalidated();
					runtime_api.call(payload).await.map(|gas_price| gas_price.0).map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = GasPriceInputPayloadV1;
					let payload = subxt_client::apis()
						.revive_api()
						.gas_price_versioned(GasPriceVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							GasPriceOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.gas_price
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the block gas limit.
	pub fn block_gas_limit(&self) -> Option<BoxFuture<'_, Result<U256, ClientError>>> {
		self.capabilities.block_gas_limit.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis().revive_api().block_gas_limit().unvalidated();
					runtime_api.call(payload).await.map(|gas_limit| gas_limit.0).map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = BlockGasLimitInputPayloadV1;
					let payload = subxt_client::apis().revive_api().block_gas_limit_versioned(
						BlockGasLimitVersionedInputPayload::from(input).into(),
					);
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							BlockGasLimitOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.block_gas_limit
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the miner address.
	pub fn block_author(&self) -> Option<BoxFuture<'_, Result<H160, ClientError>>> {
		self.capabilities.block_author.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis().revive_api().block_author().unvalidated();
					runtime_api.call(payload).await.map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = BlockAuthorInputPayloadV1;
					let payload = subxt_client::apis().revive_api().block_author_versioned(
						BlockAuthorVersionedInputPayload::from(input).into(),
					);
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							BlockAuthorOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.block_author
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the trace for the given transaction index in the given block.
	pub fn trace_tx(
		&self,
		block: sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		transaction_index: u32,
		tracer_type: TracerTypeV1,
	) -> Option<BoxFuture<'_, Result<TraceV1, ClientError>>> {
		match self.capabilities.trace_tx {
			Unavailable => None,
			Available(Unversioned) => {
				let runtime_api = self.runtime_api.clone();
				let future = Box::pin(async move {
					let payload = subxt_client::apis()
						.revive_api()
						.trace_tx(block.into(), transaction_index, tracer_type.into())
						.unvalidated();
					runtime_api
						.call(payload)
						.await?
						.map(|trace| trace.0)
						.ok_or(ClientError::EthExtrinsicNotFound)
				});
				Some(future)
			},
			Available(Versioned(_)) => {
				let runtime_api = self.runtime_api.clone();
				let future = Box::pin(async move {
					let input = TraceTxInputPayloadV1 {
						block: block.into(),
						tx_index: transaction_index,
						config: tracer_type,
					};
					let payload = subxt_client::apis()
						.revive_api()
						.trace_tx_versioned(TraceTxVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							TraceTxOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.trace
						})
						.map_err(ClientError::from)
						.and_then(|trace| trace.ok_or(ClientError::EthExtrinsicNotFound))
				});
				Some(future)
			},
		}
	}

	/// Get the trace for the given block.
	pub fn trace_block(
		&self,
		block: sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		tracer_type: TracerTypeV1,
	) -> Option<BoxFuture<'_, Result<Vec<(u32, TraceV1)>, ClientError>>> {
		match self.capabilities.trace_block {
			Unavailable => None,
			Available(Unversioned) => {
				let runtime_api = self.runtime_api.clone();
				let future = Box::pin(async move {
					let payload = subxt_client::apis()
						.revive_api()
						.trace_block(block.into(), tracer_type.into())
						.unvalidated();
					runtime_api
						.call(payload)
						.await
						.map(|traces| {
							traces.into_iter().map(|(idx, trace)| (idx, trace.0)).collect()
						})
						.map_err(Into::into)
				});
				Some(future)
			},
			Available(Versioned(_)) => {
				let runtime_api = self.runtime_api.clone();
				let future = Box::pin(async move {
					let input =
						TraceBlockInputPayloadV1 { block: block.into(), config: tracer_type };
					let payload = subxt_client::apis()
						.revive_api()
						.trace_block_versioned(TraceBlockVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							TraceBlockOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.traces
						})
						.map_err(Into::into)
				});
				Some(future)
			},
		}
	}

	/// Get the trace for the given call.
	pub fn trace_call(
		&self,
		transaction: GenericTransactionV1,
		tracer_type: TracerTypeV1,
		state_overrides: Option<StateOverrideSetV1>,
	) -> Option<BoxFuture<'_, Result<TraceV1, ClientError>>> {
		let trace_call_future =
			self.capabilities.trace_call.handle::<'_, _, _, _, BoxFuture<'_, _>, _>(
				|| {
					if state_overrides.is_some() {
						return None;
					};

					let transaction = transaction.clone();
					let tracer_type = tracer_type.clone();
					let runtime_api = self.runtime_api.clone();
					Some(Box::pin(async move {
						let payload = subxt_client::apis()
							.revive_api()
							.trace_call(transaction.into(), tracer_type.into())
							.unvalidated();
						runtime_api
							.call(payload)
							.await?
							.map(|trace| trace.0)
							.map_err(|err| ClientError::TransactError(err.0))
					}))
				},
				|_| {
					let transaction = transaction.clone();
					let tracer_type = tracer_type.clone();
					let state_overrides = state_overrides.clone();
					let runtime_api = self.runtime_api.clone();
					async move {
						let input = TraceCallInputPayloadV1 {
							tx: transaction,
							config: tracer_type,
							state_overrides,
						};
						let payload = subxt_client::apis().revive_api().trace_call_versioned(
							TraceCallVersionedInputPayload::from(input).into(),
						);
						runtime_api
							.call(payload)
							.await?
							.map(|output| {
								TraceCallOutputPayloadV1::try_from(output.0)
									.expect("v1 input must produce v1 output; qed")
									.trace
							})
							.map_err(|err| ClientError::TransactError(err.0))
					}
				},
			);
		let trace_call_with_config_future = self
			.capabilities
			.trace_call_with_config
			.handle::<'_, _, _, _, _, BoxFuture<'_, _>>(
				|| {
					let transaction = transaction.clone();
					let tracer_type = tracer_type.clone();
					let state_overrides = state_overrides.clone();
					let runtime_api = self.runtime_api.clone();
					async move {
						let config = TracingConfigV1 { state_overrides };
						let payload = subxt_client::apis()
							.revive_api()
							.trace_call_with_config(
								transaction.into(),
								tracer_type.into(),
								config.into(),
							)
							.unvalidated();
						runtime_api
							.call(payload)
							.await?
							.map(|trace| trace.0)
							.map_err(|err| ClientError::TransactError(err.0))
					}
				},
				|_| None::<BoxFuture<'_, _>>,
			);

		trace_call_future.or(trace_call_with_config_future)
	}

	/// Get the code of the given address.
	pub fn code(&self, address: H160) -> Option<BoxFuture<'_, Result<Vec<u8>, ClientError>>> {
		self.capabilities.code.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis().revive_api().code(address).unvalidated();
					runtime_api.call(payload).await.map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = CodeInputPayloadV1 { address };
					let payload = subxt_client::apis()
						.revive_api()
						.code_versioned(CodeVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							CodeOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.code
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the current Ethereum block.
	pub fn eth_block(&self) -> Option<BoxFuture<'_, Result<BlockV1, ClientError>>> {
		self.capabilities.eth_block.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis().revive_api().eth_block().unvalidated();
					runtime_api.call(payload).await.map(|block| block.0).map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = BlockInputPayloadV1;
					let payload = subxt_client::apis()
						.revive_api()
						.eth_block_versioned(BlockVersionedInputPayload::from(input).into());
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							BlockOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.block
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the Ethereum block hash for the given block number.
	pub fn eth_block_hash(
		&self,
		number: U256,
	) -> Option<BoxFuture<'_, Result<Option<H256>, ClientError>>> {
		self.capabilities.eth_block_hash.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload = subxt_client::apis()
						.revive_api()
						.eth_block_hash(number.into())
						.unvalidated();
					runtime_api.call(payload).await.map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = BlockHashInputPayloadV1 { block_number: number };
					let payload = subxt_client::apis().revive_api().eth_block_hash_versioned(
						BlockHashVersionedInputPayload::from(input).into(),
					);
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							BlockHashOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.block_hash
						})
						.map_err(Into::into)
				}
			},
		)
	}

	/// Get the receipt data for the current block.
	pub fn eth_receipt_data(
		&self,
	) -> Option<BoxFuture<'_, Result<Vec<ReceiptGasInfoV1>, ClientError>>> {
		self.capabilities.eth_receipt_data.handle(
			|| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let payload =
						subxt_client::apis().revive_api().eth_receipt_data().unvalidated();
					runtime_api
						.call(payload)
						.await
						.map(|receipt_data| receipt_data.into_iter().map(|item| item.0).collect())
						.map_err(Into::into)
				}
			},
			|_| {
				let runtime_api = self.runtime_api.clone();
				async move {
					let input = ReceiptDataInputPayloadV1;
					let payload = subxt_client::apis().revive_api().eth_receipt_data_versioned(
						ReceiptDataVersionedInputPayload::from(input).into(),
					);
					runtime_api
						.call(payload)
						.await
						.map(|output| {
							ReceiptDataOutputPayloadV1::try_from(output.0)
								.expect("v1 input must produce v1 output; qed")
								.receipt_data
						})
						.map_err(Into::into)
				}
			},
		)
	}
}

/// Stores the capabilities of pallet-revive's runtime API.
///
/// New methods were added to pallet-revive over time without making proper use of frame's API
/// version. Therefore, there is no clean mapping of "in API version X, function A was added" or
/// anything of this sort. All such information needs to be obtained by analyzing the metadata at
/// a particular block to deduce such information.
///
/// This structure provides precisely this information and is used to answer the question "is method
/// X available on the runtime API for this block or not".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub struct ReviveRuntimeApiCapabilities {
	pub eth_block: MethodStatus,
	pub eth_block_hash: MethodStatus,
	pub eth_receipt_data: MethodStatus,
	pub block_gas_limit: MethodStatus,
	pub max_extrinsic_weight_in_gas: MethodStatus,
	pub balance: MethodStatus,
	pub gas_price: MethodStatus,
	pub nonce: MethodStatus,
	pub call: MethodStatus,
	pub instantiate: MethodStatus,
	pub eth_transact: MethodStatus,
	pub eth_transact_with_config: MethodStatus,
	pub eth_estimate_gas: MethodStatus,
	pub eth_pre_dispatch_weight: MethodStatus,
	pub upload_code: MethodStatus,
	pub get_storage: MethodStatus,
	pub runtime_pallets_address: MethodStatus,
	pub code: MethodStatus,
	pub account_id: MethodStatus,
	pub new_balance_with_dust: MethodStatus,
	pub block_author: MethodStatus,
	pub address: MethodStatus,
	pub trace_block: MethodStatus,
	pub trace_tx: MethodStatus,
	pub trace_call: MethodStatus,
	pub trace_call_with_config: MethodStatus,
}

impl ReviveRuntimeApiCapabilities {
	/// Constructs the revive runtime API capabilities.
	///
	/// This requires the metadata and the runtime API at the same block in order to be constructed.
	/// If they come from different blocks then the object created might end up with corrupted state
	/// that is not representative of any real block on the network.
	pub async fn new(
		metadata: &RuntimeMetadataV16,
		runtime_api: RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>,
	) -> Self {
		let version_declarations =
			runtime_api.call(subxt_client::apis().revive_api().version_declarations()).await;

		version_declarations
			.iter()
			.flat_map(|declarations| declarations.0.iter())
			.map(|(method, version)| (method, Versioned(*version)))
			.chain(
				metadata
					.apis
					.iter()
					.filter(|runtime_api| runtime_api.name == "ReviveApi")
					.max_by(|a, b| a.version.0.cmp(&b.version.0))
					.into_iter()
					.flat_map(|api| api.methods.iter())
					.map(|method| (method.name.as_str(), Unversioned)),
			)
			.fold(Self::default(), |this, (method, versioning_status)| {
				this.with_method(method, versioning_status)
			})
	}

	fn with_method(
		mut self,
		key: impl AsRef<str>,
		incoming_method_status: MethodVersioningStatus,
	) -> Self {
		let key = key.as_ref();
		let key = key.strip_suffix("_versioned").unwrap_or(key);
		let existing_method_status = match key {
			"eth_block" => Some(&mut self.eth_block),
			"eth_block_hash" => Some(&mut self.eth_block_hash),
			"eth_receipt_data" => Some(&mut self.eth_receipt_data),
			"block_gas_limit" => Some(&mut self.block_gas_limit),
			"max_extrinsic_weight_in_gas" => Some(&mut self.max_extrinsic_weight_in_gas),
			"balance" => Some(&mut self.balance),
			"gas_price" => Some(&mut self.gas_price),
			"nonce" => Some(&mut self.nonce),
			"call" => Some(&mut self.call),
			"instantiate" => Some(&mut self.instantiate),
			"eth_transact" => Some(&mut self.eth_transact),
			"eth_transact_with_config" => Some(&mut self.eth_transact_with_config),
			"eth_estimate_gas" => Some(&mut self.eth_estimate_gas),
			"eth_pre_dispatch_weight" => Some(&mut self.eth_pre_dispatch_weight),
			"upload_code" => Some(&mut self.upload_code),
			"get_storage" => Some(&mut self.get_storage),
			"runtime_pallets_address" => Some(&mut self.runtime_pallets_address),
			"code" => Some(&mut self.code),
			"account_id" => Some(&mut self.account_id),
			"new_balance_with_dust" => Some(&mut self.new_balance_with_dust),
			"block_author" => Some(&mut self.block_author),
			"address" => Some(&mut self.address),
			"trace_block" => Some(&mut self.trace_block),
			"trace_tx" => Some(&mut self.trace_tx),
			"trace_call" => Some(&mut self.trace_call),
			"trace_call_with_config" => Some(&mut self.trace_call_with_config),
			_ => None,
		};
		let Some(existing_method_status) = existing_method_status else { return self };

		match (existing_method_status, incoming_method_status) {
			(this @ (Unavailable | Available(Unversioned)), Unversioned | Versioned(_)) => {
				*this = Available(incoming_method_status)
			},
			(Available(Versioned(existing_version)), Versioned(incoming_version)) => {
				*existing_version = (*existing_version).max(incoming_version)
			},
			(Available(Versioned(..)), Unversioned) => {},
		}

		self
	}
}

/// Defines the status of a runtime API function in pallet-revive and whether it's available or not.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Default)]
pub enum MethodStatus {
	/// The runtime API method is not available on the runtime API of the selected block (e.g., the
	/// `estimate_gas` runtime API function which was added later on).
	#[default]
	Unavailable,

	/// The runtime API method is available on the runtime API, and may either be versioned or
	/// unversioned.
	Available(MethodVersioningStatus),
}

impl MethodStatus {
	/// Selects and boxes the future produced by the handler matching this status.
	fn handle<'a, T, Rtn1, Rtn2, Fut1, Fut2>(
		&'a self,
		unversioned_handler: impl FnOnce() -> Rtn1,
		versioned_handler: impl FnOnce(u8) -> Rtn2,
	) -> Option<BoxFuture<'a, T>>
	where
		Rtn1: Into<Option<Fut1>>,
		Rtn2: Into<Option<Fut2>>,
		Fut1: Future<Output = T> + Send + 'a,
		Fut2: Future<Output = T> + Send + 'a,
	{
		match *self {
			Unavailable => None,
			Available(Unversioned) => unversioned_handler().into().map(|fut| fut.boxed()),
			Available(Versioned(v)) => versioned_handler(v).into().map(|fut| fut.boxed()),
		}
	}
}

/// Defines the status of a runtime API function's versioning.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum MethodVersioningStatus {
	/// The method is available on the runtime API and is not versioned (e.g., the specified block
	/// has a pre-versioning runtime).
	Unversioned,

	/// The method is available on the runtime API and is versioned. The provided value is the
	/// highest version supported by the runtime.
	Versioned(u8),
}
