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
	ClientError, H160, LOG_TARGET,
};
use codec::Encode;
use pallet_revive::evm::{block_hash::ReceiptGasInfo, Block};
use sp_core::{H256, U256};
use subxt::{storage::Storage, OnlineClient};

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
		let query = subxt::dynamic::storage("Revive", "ReceiptInfoData", ());

		let Some(info) = self.0.fetch(&query).await? else {
			return Err(ClientError::ReceiptDataNotFound);
		};
		let bytes = info.into_encoded();
		codec::Decode::decode(&mut &bytes[..]).map_err(|err| err.into())
	}

	/// Get the Ethereum block from storage.
	pub async fn get_ethereum_block(&self) -> Result<Block, ClientError> {
		let query = subxt::dynamic::storage("Revive", "EthereumBlock", ());

		let Some(info) = self.0.fetch(&query).await? else {
			return Err(ClientError::EthereumBlockNotFound);
		};
		let bytes = info.into_encoded();
		codec::Decode::decode(&mut &bytes[..]).map_err(|err| err.into())
	}

	pub async fn get_ethereum_block_hash(&self, number: u64) -> Result<H256, ClientError> {
		let u256_encoded = U256::from(number).0.encode();
		let key = subxt::dynamic::Value::from_bytes(u256_encoded);
		let query = subxt::dynamic::storage("Revive", "BlockHash", vec![key]);
		log::debug!(target: LOG_TARGET, "get_ethereum_block_hash number = {number}");

		let Some(hash) = self.0.fetch(&query).await.inspect_err(|e| {
			log::error!(target: LOG_TARGET, "get_ethereum_block_hash number = {number} err = {e:?}");
		})?
		else {
			log::error!(target: LOG_TARGET, "get_ethereum_block_hash number = {number} Ethereum block not found");
			return Err(ClientError::EthereumBlockNotFound);
		};

		let bytes = hash.into_encoded();
		log::debug!(target: LOG_TARGET, "get_ethereum_block_hash number = {number} bytes = {bytes:?}");
		codec::Decode::decode(&mut &bytes[..]).map_err(|err| err.into())
	}
}
