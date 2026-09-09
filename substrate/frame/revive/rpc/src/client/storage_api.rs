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
	subxt_client::{
		self, SrcChainConfig,
		runtime_types::pallet_revive::storage::{AccountType, ContractInfo},
	},
};
use pallet_revive::evm::U256;
use pallet_revive_types::runtime_api::BlockV1;
use sp_core::H256;
use subxt::{client::OnlineClientAtBlock, error::StorageError};

const LOG_TARGET: &str = "eth-rpc::storage_api";

/// Checks if the error indicates that the pallet or storage entry is absent from the block's
/// metadata.
fn is_pre_revive_runtime(err: &StorageError) -> bool {
	matches!(err, StorageError::PalletNameNotFound(_) | StorageError::StorageEntryNotFound { .. })
}

/// A wrapper around the Substrate Storage API for a given block.
#[derive(Clone)]
pub struct StorageApi {
	at_block: OnlineClientAtBlock<SrcChainConfig>,
}

impl StorageApi {
	/// Create a new instance of the StorageApi anchored at `at_block`.
	pub fn new(at_block: OnlineClientAtBlock<SrcChainConfig>) -> Self {
		Self { at_block }
	}

	/// Get the contract info for the given contract address.
	pub async fn get_contract_info(
		&self,
		contract_address: &H160,
	) -> Result<ContractInfo, ClientError> {
		let contract_address: subxt::utils::H160 = contract_address.0.into();

		let query = subxt_client::storage().revive().account_info_of().unvalidated();
		let entry = self.at_block.storage().entry(query)?;
		let Some(info) = entry.try_fetch((contract_address,)).await? else {
			return Err(ClientError::ContractNotFound);
		};
		let info = info.decode()?;

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
	///
	/// The address is validated against the block's own metadata so revive storage layout
	/// changes are caught; a block whose runtime predates the storage item reports
	/// [`ClientError::BlockNotFound`].
	pub async fn eth_block(&self) -> Result<BlockV1, ClientError> {
		let query = subxt_client::storage().revive().ethereum_block();
		let entry = match self.at_block.storage().entry(query) {
			Ok(entry) => entry,
			Err(err) if is_pre_revive_runtime(&err) => return Err(ClientError::BlockNotFound),
			Err(err) => return Err(err.into()),
		};
		let block = entry.try_fetch(()).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Ethereum block storage read failed, err: {err:?}");
		})?;
		match block {
			Some(block) => Ok(block.decode()?.0),
			None => Err(ClientError::BlockNotFound),
		}
	}

	/// Ethereum block hash for `number`, read directly from the `BlockHash` storage map without
	/// invoking the runtime. Returns `None` when `number` is out of range or has no stored hash,
	/// including on blocks whose runtime predates pallet-revive.
	pub async fn eth_block_hash(&self, number: U256) -> Result<Option<H256>, ClientError> {
		let Ok(number) = u32::try_from(number) else { return Ok(None) };
		let query = subxt_client::storage().revive().block_hash();
		let entry = match self.at_block.storage().entry(query) {
			Ok(entry) => entry,
			Err(err) if is_pre_revive_runtime(&err) => return Ok(None),
			Err(err) => return Err(err.into()),
		};
		let hash = entry.try_fetch((number,)).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Ethereum block hash storage read failed for #{number}, err: {err:?}");
		})?;
		Ok(hash.map(|value| value.decode()).transpose()?)
	}

	/// Receipt data for the current block, read directly from the `ReceiptInfoData` storage value
	/// without invoking the runtime. Absent data reads as empty.
	pub async fn eth_receipt_data(&self) -> Result<Vec<ReceiptGasInfoV1>, ClientError> {
		let query = subxt_client::storage().revive().receipt_info_data();
		let entry = match self.at_block.storage().entry(query) {
			Ok(entry) => entry,
			Err(err) if is_pre_revive_runtime(&err) => return Ok(Vec::new()),
			Err(err) => return Err(err.into()),
		};
		let receipt_data = entry.try_fetch(()).await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "eth_receipt_data storage read failed: {err:?}");
		})?;
		let Some(receipt_data) = receipt_data else { return Ok(Vec::new()) };
		Ok(receipt_data.decode()?.into_iter().map(|item| item.0.into()).collect())
	}
}
