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

use crate::{
	ClientError,
	client::Balance,
	subxt_client::{self, SrcChainConfig},
};
use futures::{StreamExt, TryFutureExt, stream};
use pallet_revive::{
	DryRunConfig, EthTransactInfo, TracingConfig,
	evm::{
		Block as EthBlock, BlockNumberOrTagOrHash, BlockTag, GenericTransaction, H160,
		ReceiptGasInfo, StateOverrideSet, Trace, U256,
	},
};
use sp_core::H256;
use sp_timestamp::Timestamp;
use subxt::{Error::Metadata, OnlineClient, error::MetadataError, ext::subxt_rpcs::UserError};

const LOG_TARGET: &str = "eth-rpc::runtime_api";

/// A Wrapper around subxt Runtime API
#[derive(Clone)]
pub struct RuntimeApi(subxt::runtime_api::RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>);

impl RuntimeApi {
	/// Create a new instance.
	pub fn new(
		api: subxt::runtime_api::RuntimeApi<SrcChainConfig, OnlineClient<SrcChainConfig>>,
	) -> Self {
		Self(api)
	}

	/// Get the balance of the given address.
	pub async fn balance(&self, address: H160) -> Result<U256, ClientError> {
		let address = address.0.into();
		let payload = subxt_client::apis().revive_api().balance(address).unvalidated();
		let balance = self.0.call(payload).await?;
		Ok(*balance)
	}

	/// Get the contract storage for the given contract address and key.
	pub async fn get_storage(
		&self,
		contract_address: H160,
		key: [u8; 32],
	) -> Result<Option<Vec<u8>>, ClientError> {
		let contract_address = contract_address.0.into();
		let payload = subxt_client::apis()
			.revive_api()
			.get_storage(contract_address, key)
			.unvalidated();
		let result = self.0.call(payload).await?.map_err(|_| ClientError::ContractNotFound)?;
		Ok(result)
	}

	/// Estimates the minimum gas limit required for the transaction execution. Returns a [`U256`]
	/// of the gas limit.
	pub async fn estimate_gas(
		&self,
		tx: GenericTransaction,
		block: BlockNumberOrTagOrHash,
	) -> Result<U256, ClientError> {
		let timestamp_override = match block {
			BlockNumberOrTagOrHash::BlockTag(BlockTag::Pending) => {
				Some(Timestamp::current().as_millis())
			},
			_ => None,
		};

		// Not all versions of pallet-revive have all of the runtime functions that we require. Thus
		// we need to be able to perform the gas estimation through any of the runtime functions
		// that the pallet may have available which is why we make use of this stream. The functions
		// with higher priority are put at the start while the functions with lower priority are at
		// the end.
		let mut stream =
			// Estimate through the `estimate_gas` function
			stream::once(Box::pin(async {
				let payload = subxt_client::apis()
					.revive_api()
					.eth_estimate_gas(
						tx.clone().into(),
						DryRunConfig::default().with_timestamp_override(timestamp_override).into(),
					)
					.unvalidated();
				self.0.call(payload).await.map(|value| value.map(|value| value.0))
			}))
			// Otherwise, estimate through `eth_transact_with_config`
			.chain(stream::once(Box::pin(async {
				let payload = subxt_client::apis()
					.revive_api()
					.eth_transact_with_config(
						tx.clone().into(),
						DryRunConfig::default().with_timestamp_override(timestamp_override).into(),
					)
					.unvalidated();
				self.0.call(payload).await.map(|value| value.map(|value| value.eth_gas))
			})))
			// Otherwise, estimate through `eth_transact`
			.chain(stream::once(Box::pin(async {
				let payload =
					subxt_client::apis().revive_api().eth_transact(tx.clone().into()).unvalidated();
				self.0.call(payload).await.map(|value| value.map(|value| value.eth_gas))
			})));

		while let Some(result) = stream.next().await {
			match result {
				Ok(estimation) => {
					return estimation.map_err(|err| ClientError::TransactError(err.0));
				},
				Err(Metadata(MetadataError::RuntimeMethodNotFound(name))) => {
					log::debug!(target: LOG_TARGET, "Method {name:?} not found falling back");
				},
				Err(subxt::Error::Rpc(subxt::error::RpcError::ClientError(
					subxt::ext::subxt_rpcs::Error::User(UserError { message, .. }),
				))) if message.contains("is not found") => {
					log::debug!(target: LOG_TARGET, "{message:?} not found falling back")
				},
				Err(err) => return Err(err.into()),
			}
		}

		Err(ClientError::NoEstimationMethodSucceeded.into())
	}

	/// Dry run a transaction and returns the [`EthTransactInfo`] for the transaction.
	pub async fn dry_run(
		&self,
		tx: GenericTransaction,
		block: BlockNumberOrTagOrHash,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<EthTransactInfo<Balance>, ClientError> {
		let timestamp_override = match block {
			BlockNumberOrTagOrHash::BlockTag(BlockTag::Pending) => {
				Some(Timestamp::current().as_millis())
			},
			_ => None,
		};

		let config = DryRunConfig::default()
			.with_timestamp_override(timestamp_override)
			.with_state_overrides(state_overrides);

		let payload = subxt_client::apis()
			.revive_api()
			.eth_transact_with_config(tx.clone().into(), config.into())
			.unvalidated();

		let result = self
			.0
			.call(payload)
			.or_else(|err| async {
				match err {
					// This will be hit if subxt metadata (subxt uses the latest finalized block
					// metadata when the eth-rpc starts) does not contain the new method
					Metadata(MetadataError::RuntimeMethodNotFound(name)) => {
						log::debug!(target: LOG_TARGET, "Method {name:?} not found falling back to eth_transact");
						let payload =
							subxt_client::apis().revive_api().eth_transact(tx.into()).unvalidated();
						self.0.call(payload).await
					},
					// This will be hit if we are trying to hit a block where the runtime did not
					// have this new runtime `eth_transact_with_config` defined
					subxt::Error::Rpc(subxt::error::RpcError::ClientError(
						subxt::ext::subxt_rpcs::Error::User(UserError { message, .. }),
					)) if message.contains("eth_transact_with_config is not found") => {
						log::debug!(target: LOG_TARGET, "{message:?} not found falling back to eth_transact");
						let payload =
							subxt_client::apis().revive_api().eth_transact(tx.into()).unvalidated();
						self.0.call(payload).await
					},
					e => Err(e),
				}
			})
			.await?;

		match result {
			Err(err) => {
				log::debug!(target: LOG_TARGET, "Dry run failed {err:?}");
				Err(ClientError::TransactError(err.0))
			},
			Ok(result) => Ok(result.0),
		}
	}

	/// Get the nonce of the given address.
	pub async fn nonce(&self, address: H160) -> Result<U256, ClientError> {
		let address = address.0.into();
		let payload = subxt_client::apis().revive_api().nonce(address).unvalidated();
		let nonce = self.0.call(payload).await?;
		Ok(nonce.into())
	}

	/// Get the gas price
	pub async fn gas_price(&self) -> Result<U256, ClientError> {
		let payload = subxt_client::apis().revive_api().gas_price().unvalidated();
		let gas_price = self.0.call(payload).await?;
		Ok(*gas_price)
	}

	/// Convert a weight to a fee.
	pub async fn block_gas_limit(&self) -> Result<U256, ClientError> {
		let payload = subxt_client::apis().revive_api().block_gas_limit().unvalidated();
		let gas_limit = self.0.call(payload).await?;
		Ok(*gas_limit)
	}

	/// Get the miner address
	pub async fn block_author(&self) -> Result<H160, ClientError> {
		let payload = subxt_client::apis().revive_api().block_author().unvalidated();
		let author = self.0.call(payload).await?;
		Ok(author)
	}

	/// Get the trace for the given transaction index in the given block.
	pub async fn trace_tx(
		&self,
		block: sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		transaction_index: u32,
		tracer_type: crate::TracerType,
	) -> Result<Trace, ClientError> {
		let payload = subxt_client::apis()
			.revive_api()
			.trace_tx(block.into(), transaction_index, tracer_type.into())
			.unvalidated();

		let trace = self.0.call(payload).await?.ok_or(ClientError::EthExtrinsicNotFound)?.0;
		Ok(trace)
	}

	/// Get the trace for the given block.
	pub async fn trace_block(
		&self,
		block: sp_runtime::generic::Block<
			sp_runtime::generic::Header<u32, sp_runtime::traits::BlakeTwo256>,
			sp_runtime::OpaqueExtrinsic,
		>,
		tracer_type: crate::TracerType,
	) -> Result<Vec<(u32, Trace)>, ClientError> {
		let payload = subxt_client::apis()
			.revive_api()
			.trace_block(block.into(), tracer_type.into())
			.unvalidated();

		let traces = self.0.call(payload).await?.into_iter().map(|(idx, t)| (idx, t.0)).collect();
		Ok(traces)
	}

	/// Get the trace for the given call.
	///
	/// If `state_overrides` are provided, uses the `trace_call_with_config` runtime API
	/// which supports state overrides. Otherwise falls back to the original `trace_call`
	/// for backwards compatibility with older runtimes.
	pub async fn trace_call(
		&self,
		transaction: GenericTransaction,
		tracer_type: crate::TracerType,
		state_overrides: Option<StateOverrideSet>,
	) -> Result<Trace, ClientError> {
		let result = if let Some(overrides) = state_overrides {
			let config = TracingConfig::new().with_state_overrides(overrides);
			let payload = subxt_client::apis()
				.revive_api()
				.trace_call_with_config(transaction.into(), tracer_type.into(), config.into())
				.unvalidated();
			self.0.call(payload).await?
		} else {
			let payload = subxt_client::apis()
				.revive_api()
				.trace_call(transaction.into(), tracer_type.into())
				.unvalidated();
			self.0.call(payload).await?
		};

		match result {
			Err(err) => Err(ClientError::TransactError(err.0)),
			Ok(trace) => Ok(trace.0),
		}
	}

	/// Get the code of the given address.
	pub async fn code(&self, address: H160) -> Result<Vec<u8>, ClientError> {
		let payload = subxt_client::apis().revive_api().code(address).unvalidated();
		let code = self.0.call(payload).await?;
		Ok(code)
	}

	/// Get the current Ethereum block.
	pub async fn eth_block(&self) -> Result<EthBlock, ClientError> {
		let payload = subxt_client::apis().revive_api().eth_block().unvalidated();
		let block = self.0.call(payload).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Ethereum block not found, err: {err:?}");
		})?;
		Ok(block.0)
	}

	/// Get the Ethereum block hash for the given block number.
	pub async fn eth_block_hash(&self, number: U256) -> Result<Option<H256>, ClientError> {
		let payload = subxt_client::apis().revive_api().eth_block_hash(number.into()).unvalidated();
		let hash = self.0.call(payload).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Ethereum block hash for block #{number:?} not found, err: {err:?}");
		})?;
		Ok(hash)
	}

	/// Get the receipt data for the current block.
	pub async fn eth_receipt_data(&self) -> Result<Vec<ReceiptGasInfo>, ClientError> {
		let payload = subxt_client::apis().revive_api().eth_receipt_data().unvalidated();
		let receipt_data = self.0.call(payload).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "eth_receipt_data runtime call failed: {err:?}");
		})?;
		let receipt_data = receipt_data.into_iter().map(|item| item.0).collect();
		Ok(receipt_data)
	}
}
