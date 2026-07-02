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
	ClientError, H160, ReceiptGasInfoV1,
	client::SubstrateBlockNumber,
	subxt_client::{
		self, SrcChainConfig,
		runtime_types::pallet_revive::storage::{AccountType, ContractInfo},
	},
};
use pallet_revive::evm::{Block as EthBlock, U256};
use sp_core::H256;
use subxt::{OnlineClient, storage::Storage};

const LOG_TARGET: &str = "eth-rpc::storage_api";

/// A wrapper around the Substrate Storage API.
#[derive(Clone)]
pub struct StorageApi(Storage<SrcChainConfig, OnlineClient<SrcChainConfig>>);

impl StorageApi {
	/// Create a new instance of the StorageApi.
	pub fn new(api: Storage<SrcChainConfig, OnlineClient<SrcChainConfig>>) -> Self {
		Self(api)
	}

	/// Get the contract info for the given contract address.
	pub async fn get_contract_info(
		&self,
		contract_address: &H160,
	) -> Result<ContractInfo, ClientError> {
		// TODO: remove once subxt is updated
		let contract_address: subxt::utils::H160 = contract_address.0.into();

		let query =
			subxt_client::storage().revive().account_info_of(contract_address).unvalidated();
		let Some(info) = self.0.fetch(&query).await? else {
			return Err(ClientError::ContractNotFound);
		};

		let AccountType::Contract(contract_info) = info.account_type else {
			return Err(ClientError::ContractNotFound);
		};

		Ok(contract_info)
	}

	/// Get the contract trie id for the given contract address.
	pub async fn get_contract_trie_id(&self, address: &H160) -> Result<Vec<u8>, ClientError> {
		let ContractInfo { trie_id, .. } = self.get_contract_info(address).await?;
		Ok(trie_id.0)
	}

	/// Current Ethereum block, read directly from the `EthereumBlock` storage value without
	/// invoking the runtime.
	pub async fn eth_block(&self) -> Result<EthBlock, ClientError> {
		let query = subxt_client::storage().revive().ethereum_block();
		let block = self.0.fetch_or_default(&query).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Ethereum block storage read failed, err: {err:?}");
		})?;
		Ok(block.0)
	}

	/// Ethereum block hash for `number`, read directly from the `BlockHash` storage map without
	/// invoking the runtime. Keeps the runtime's mapping: out-of-range or zero hash -> `None`.
	pub async fn eth_block_hash(&self, number: U256) -> Result<Option<H256>, ClientError> {
		let Ok(number) = SubstrateBlockNumber::try_from(number) else { return Ok(None) };
		let query = subxt_client::storage().revive().block_hash(number);
		let hash = self.0.fetch_or_default(&query).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Ethereum block hash storage read failed for #{number}, err: {err:?}");
		})?;
		Ok((hash != H256::zero()).then_some(hash))
	}

	/// Receipt data for the current block, read directly from the `ReceiptInfoData` storage value
	/// without invoking the runtime.
	pub async fn eth_receipt_data(&self) -> Result<Vec<ReceiptGasInfoV1>, ClientError> {
		let query = subxt_client::storage().revive().receipt_info_data();
		let receipt_data = self.0.fetch_or_default(&query).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "eth_receipt_data storage read failed: {err:?}");
		})?;
		let receipt_data = receipt_data.into_iter().map(|item| item.0.into()).collect();
		Ok(receipt_data)
	}
}
