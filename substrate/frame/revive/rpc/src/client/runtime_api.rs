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
	client::Balance,
	subxt_client::{self, SrcChainConfig},
	ClientError, LOG_TARGET,
};
use pallet_revive::{
	evm::{GenericTransaction, Trace, H160, U256},
	EthTransactInfo,
};
use subxt::OnlineClient;

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
		let payload = subxt_client::apis().revive_api().balance(address);
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
		let payload = subxt_client::apis().revive_api().get_storage(contract_address, key);
		let result = self.0.call(payload).await?.map_err(|_| ClientError::ContractNotFound)?;
		Ok(result)
	}

	/// Dry run a transaction and returns the [`EthTransactInfo`] for the transaction.
	pub async fn dry_run(
		&self,
		tx: GenericTransaction,
	) -> Result<EthTransactInfo<Balance>, ClientError> {
		let payload = subxt_client::apis().revive_api().eth_transact(tx.into());
		let result = self.0.call(payload).await?;
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
		let payload = subxt_client::apis().revive_api().nonce(address);
		let nonce = self.0.call(payload).await?;
		Ok(nonce.into())
	}

	/// Get the gas price
	pub async fn gas_price(&self) -> Result<U256, ClientError> {
		let payload = subxt_client::apis().revive_api().gas_price();
		let gas_price = self.0.call(payload).await?;
		Ok(*gas_price)
	}

	/// Convert a weight to a fee.
	pub async fn block_gas_limit(&self) -> Result<U256, ClientError> {
		let payload = subxt_client::apis().revive_api().block_gas_limit();
		let gas_limit = self.0.call(payload).await?;
		Ok(*gas_limit)
	}

	/// Get the miner address
	pub async fn block_author(&self) -> Result<Option<H160>, ClientError> {
		let payload = subxt_client::apis().revive_api().block_author();
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
	pub async fn trace_call(
		&self,
		transaction: GenericTransaction,
		tracer_type: crate::TracerType,
	) -> Result<Trace, ClientError> {
		let payload = subxt_client::apis()
			.revive_api()
			.trace_call(transaction.into(), tracer_type.into())
			.unvalidated();

		let trace = self.0.call(payload).await?.map_err(|err| ClientError::TransactError(err.0))?;
		Ok(trace.0)
	}
}
