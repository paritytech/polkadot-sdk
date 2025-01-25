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
	client::SubstrateBlock,
	subxt_client::{
		revive::{calls::types::EthTransact, events::ContractEmitted},
		system::events::ExtrinsicSuccess,
		transaction_payment::events::TransactionFeePaid,
		SrcChainConfig,
	},
	ClientError, LOG_TARGET,
};
use futures::{stream, StreamExt};
use jsonrpsee::core::async_trait;
use pallet_revive::{
	create1,
	evm::{GenericTransaction, Log, ReceiptInfo, TransactionSigned, H256, U256},
};
use sp_core::keccak_256;
use tokio::join;

mod cache;
pub use cache::CacheReceiptProvider;

mod db;
pub use db::DBReceiptProvider;

/// Provide means to store and retrieve receipts.
#[async_trait]
pub trait ReceiptProvider: Send + Sync {
	/// Insert receipts into the provider.
	async fn insert(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]);

	/// Remove receipts with the given block hash.
	async fn remove(&self, block_hash: &H256);

	/// Get the receipt for the given block hash and transaction index.
	async fn receipt_by_block_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: &U256,
	) -> Option<ReceiptInfo>;

	/// Get the number of receipts per block.
	async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize>;

	/// Get the receipt for the given transaction hash.
	async fn receipt_by_hash(&self, transaction_hash: &H256) -> Option<ReceiptInfo>;

	/// Get the signed transaction for the given transaction hash.
	async fn signed_tx_by_hash(&self, transaction_hash: &H256) -> Option<TransactionSigned>;
}

#[async_trait]
impl<Main: ReceiptProvider, Fallback: ReceiptProvider> ReceiptProvider for (Main, Fallback) {
	async fn insert(&self, block_hash: &H256, receipts: &[(TransactionSigned, ReceiptInfo)]) {
		join!(self.0.insert(block_hash, receipts), self.1.insert(block_hash, receipts));
	}

	async fn remove(&self, block_hash: &H256) {
		join!(self.0.remove(block_hash), self.1.remove(block_hash));
	}

	async fn receipt_by_block_hash_and_index(
		&self,
		block_hash: &H256,
		transaction_index: &U256,
	) -> Option<ReceiptInfo> {
		if let Some(receipt) =
			self.0.receipt_by_block_hash_and_index(block_hash, transaction_index).await
		{
			return Some(receipt);
		}

		self.1.receipt_by_block_hash_and_index(block_hash, transaction_index).await
	}

	async fn receipts_count_per_block(&self, block_hash: &H256) -> Option<usize> {
		if let Some(count) = self.0.receipts_count_per_block(block_hash).await {
			return Some(count);
		}
		self.1.receipts_count_per_block(block_hash).await
	}

	async fn receipt_by_hash(&self, hash: &H256) -> Option<ReceiptInfo> {
		if let Some(receipt) = self.0.receipt_by_hash(hash).await {
			return Some(receipt);
		}
		self.1.receipt_by_hash(hash).await
	}

	async fn signed_tx_by_hash(&self, hash: &H256) -> Option<TransactionSigned> {
		if let Some(tx) = self.0.signed_tx_by_hash(hash).await {
			return Some(tx);
		}
		self.1.signed_tx_by_hash(hash).await
	}
}

/// Extract a [`TransactionSigned`] and a [`ReceiptInfo`] and  from an extrinsic.
pub async fn extract_receipt_from_extrinsic(
	block: &SubstrateBlock,
	ext: subxt::blocks::ExtrinsicDetails<SrcChainConfig, subxt::OnlineClient<SrcChainConfig>>,
	call: EthTransact,
) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
	let transaction_index = ext.index();
	let block_number = U256::from(block.number());
	let block_hash = block.hash();
	let events = ext.events().await?;

	let success = events.has::<ExtrinsicSuccess>().inspect_err(|err| {
		log::debug!(target: LOG_TARGET, "Failed to lookup for ExtrinsicSuccess event in block {block_number}: {err:?}")
	})?;
	let tx_fees = events
		.find_first::<TransactionFeePaid>()?
		.ok_or(ClientError::TxFeeNotFound)
		.inspect_err(
			|err| log::debug!(target: LOG_TARGET, "TransactionFeePaid not found in events for block {block_number}\n{err:?}")
		)?;
	let transaction_hash = H256(keccak_256(&call.payload));

	let signed_tx =
		TransactionSigned::decode(&call.payload).map_err(|_| ClientError::TxDecodingFailed)?;
	let from = signed_tx.recover_eth_address().map_err(|_| {
		log::error!(target: LOG_TARGET, "Failed to recover eth address from signed tx");
		ClientError::RecoverEthAddressFailed
	})?;

	let tx_info = GenericTransaction::from_signed(signed_tx.clone(), Some(from));
	let gas_price = tx_info.gas_price.unwrap_or_default();
	let gas_used = (tx_fees.tip.saturating_add(tx_fees.actual_fee))
		.checked_div(gas_price.as_u128())
		.unwrap_or_default();

	// get logs from ContractEmitted event
	let logs = events
		.iter()
		.filter_map(|event_details| {
			let event_details = event_details.ok()?;
			let event = event_details.as_event::<ContractEmitted>().ok()??;

			Some(Log {
				address: event.contract,
				topics: event.topics,
				data: Some(event.data.into()),
				block_number: Some(block_number),
				transaction_hash,
				transaction_index: Some(transaction_index.into()),
				block_hash: Some(block_hash),
				log_index: Some(event_details.index().into()),
				..Default::default()
			})
		})
		.collect();

	let contract_address = if tx_info.to.is_none() {
		Some(create1(
			&from,
			tx_info
				.nonce
				.unwrap_or_default()
				.try_into()
				.map_err(|_| ClientError::ConversionFailed)?,
		))
	} else {
		None
	};

	log::debug!(target: LOG_TARGET, "Adding receipt for tx hash: {transaction_hash:?} - block: {block_number:?}");
	let receipt = ReceiptInfo::new(
		block_hash,
		block_number,
		contract_address,
		from,
		logs,
		tx_info.to,
		gas_price,
		gas_used.into(),
		success,
		transaction_hash,
		transaction_index.into(),
		tx_info.r#type.unwrap_or_default(),
	);
	Ok((signed_tx, receipt))
}

///  Extract receipts from block.
pub async fn extract_receipts_from_block(
	block: &SubstrateBlock,
) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
	// Filter extrinsics from pallet_revive
	let extrinsics = block.extrinsics().await.inspect_err(|err| {
		log::debug!(target: LOG_TARGET, "Error fetching for #{:?} extrinsics: {err:?}", block.number());
	})?;

	let extrinsics = extrinsics.iter().flat_map(|ext| {
		let call = ext.as_extrinsic::<EthTransact>().ok()??;
		Some((ext, call))
	});

	stream::iter(extrinsics)
		.map(|(ext, call)| async move { extract_receipt_from_extrinsic(block, ext, call).await })
		.buffer_unordered(10)
		.collect::<Vec<Result<_, _>>>()
		.await
		.into_iter()
		.collect::<Result<Vec<_>, _>>()
}

///  Extract receipt from transaction
pub async fn extract_receipts_from_transaction(
	block: &SubstrateBlock,
	transaction_index: usize,
) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
	let extrinsics = block.extrinsics().await?;
	let ext = extrinsics
		.iter()
		.nth(transaction_index)
		.ok_or(ClientError::EthExtrinsicNotFound)?;

	let call = ext
		.as_extrinsic::<EthTransact>()?
		.ok_or_else(|| ClientError::EthExtrinsicNotFound)?;
	extract_receipt_from_extrinsic(block, ext, call).await
}
