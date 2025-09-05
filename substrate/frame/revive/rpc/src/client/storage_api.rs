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
	subxt_client::{
		self,
		runtime_types::pallet_revive::storage::{AccountType, ContractInfo},
		SrcChainConfig,
	},
	ClientError, H160,
};
use sp_core::{H256, U256};
use subxt::{storage::Storage, OnlineClient};

use pallet_revive::evm::{block_hash::ReceiptGasInfo, Block};

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

		let query = subxt_client::storage().revive().account_info_of(contract_address);
		self.0
			.fetch(&query)
			.await?
			.and_then(|info| match info.account_type {
				AccountType::Contract(contract_info) => Some(contract_info),
				_ => None,
			})
			.ok_or(ClientError::ContractNotFound)
	}

	/// Get the contract trie id for the given contract address.
	pub async fn get_contract_trie_id(&self, address: &H160) -> Result<Vec<u8>, ClientError> {
		let ContractInfo { trie_id, .. } = self.get_contract_info(address).await?;
		Ok(trie_id.0)
	}

	/// Get the receipt data from storage.
	pub async fn get_receipt_data(&self) -> Result<Vec<ReceiptGasInfo>, ClientError> {
		let query = subxt_client::storage().revive().receipt_info_data();

		let Some(receipt_info_data) = self.0.fetch(&query).await? else {
			log::warn!(target: LOG_TARGET, "Receipt data not found");
			return Err(ClientError::ReceiptDataNotFound);
		};
		log::trace!(target: LOG_TARGET, "Receipt data found");
		let receipt_info_data = receipt_info_data.into_iter().map(|entry| entry.0).collect();
		Ok(receipt_info_data)
	}

	/// Get the Ethereum block from storage.
	pub async fn get_ethereum_block(&self) -> Result<Block, ClientError> {
		let query = subxt_client::storage().revive().ethereum_block();
		let Some(block) = self.0.fetch(&query).await? else {
			log::warn!(target: LOG_TARGET, "Ethereum block not found");
			return Err(ClientError::EthereumBlockNotFound);
		};
		log::trace!(target: LOG_TARGET, "Ethereum block found hash: {:?}", block.hash);
		Ok(block.0)
	}

	pub async fn get_ethereum_block_hash(&self, number: u64) -> Result<H256, ClientError> {
		// Convert u64 to the wrapped U256 type that subxt expects
		let number_u256 = subxt::utils::Static(U256::from(number));

		let query = subxt_client::storage().revive().block_hash(number_u256);

		let Some(hash) = self.0.fetch(&query).await? else {
			log::warn!(target: LOG_TARGET, "Ethereum block #{number} hash not found");
			return Err(ClientError::EthereumBlockNotFound);
		};

		log::trace!(target: LOG_TARGET, "Ethereum block #{number} hash: {hash:?}");

		Ok(hash)
	}
}
