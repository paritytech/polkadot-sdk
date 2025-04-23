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
	client::{SubstrateBlock, SubstrateBlockNumber},
	subxt_client::{
		self,
		revive::{calls::types::EthTransact, events::ContractEmitted},
		system::events::ExtrinsicSuccess,
		transaction_payment::events::TransactionFeePaid,
		SrcChainConfig,
	},
	ClientError, LOG_TARGET,
};
use futures::{stream, StreamExt};
use pallet_revive::{
	create1,
	evm::{GenericTransaction, Log, ReceiptInfo, TransactionSigned, H256, U256},
};
use sp_core::keccak_256;
use std::{future::Future, pin::Pin, sync::Arc};
use subxt::OnlineClient;

type FetchGasPriceFn = Arc<
	dyn Fn(H256) -> Pin<Box<dyn Future<Output = Result<U256, ClientError>> + Send>> + Send + Sync,
>;

/// Utility to extract receipts from extrinsics.
#[derive(Clone)]
pub struct ReceiptExtractor {
	/// Fetch the gas price from the chain.
	fetch_gas_price: FetchGasPriceFn,

	/// The native to eth decimal ratio, used to calculated gas from native fees.
	native_to_eth_ratio: u32,

	/// Earliest block number to consider when searching for transaction receipts.
	earliest_receipt_block: Option<SubstrateBlockNumber>,
}

/// Fetch the native_to_eth_ratio
async fn native_to_eth_ratio(api: &OnlineClient<SrcChainConfig>) -> Result<u32, ClientError> {
	let query = subxt_client::constants().revive().native_to_eth_ratio();
	api.constants().at(&query).map_err(|err| err.into())
}

impl ReceiptExtractor {
	/// Create a new `ReceiptExtractor` with the given native to eth ratio.
	pub async fn new(
		api: OnlineClient<SrcChainConfig>,
		earliest_receipt_block: Option<SubstrateBlockNumber>,
	) -> Result<Self, ClientError> {
		let native_to_eth_ratio = native_to_eth_ratio(&api).await?;

		let fetch_gas_price = Arc::new(move |block_hash| {
			let api_clone = api.clone();
			let fut = async move {
				let runtime_api = api_clone.runtime_api().at(block_hash);
				let payload = subxt_client::apis().revive_api().gas_price();
				let base_gas_price = runtime_api.call(payload).await?;
				Ok(*base_gas_price)
			};
			Box::pin(fut) as Pin<Box<_>>
		});

		Ok(Self { native_to_eth_ratio, fetch_gas_price, earliest_receipt_block })
	}

	#[cfg(test)]
	pub fn new_mock() -> Self {
		let fetch_gas_price =
			Arc::new(|_| Box::pin(std::future::ready(Ok(U256::from(1000)))) as Pin<Box<_>>);

		Self { native_to_eth_ratio: 1_000_000, fetch_gas_price, earliest_receipt_block: None }
	}

	/// Extract a [`TransactionSigned`] and a [`ReceiptInfo`] from an extrinsic.
	async fn extract_from_extrinsic(
		&self,
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

		let base_gas_price = (self.fetch_gas_price)(block_hash).await?;
		let tx_info =
			GenericTransaction::from_signed(signed_tx.clone(), base_gas_price, Some(from));
		let gas_price = tx_info.gas_price.unwrap_or_default();
		let gas_used = U256::from(tx_fees.tip.saturating_add(tx_fees.actual_fee))
			.saturating_mul(self.native_to_eth_ratio.into())
			.checked_div(gas_price)
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
					block_number,
					transaction_hash,
					transaction_index: transaction_index.into(),
					block_hash,
					log_index: event_details.index().into(),
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
			gas_used,
			success,
			transaction_hash,
			transaction_index.into(),
			tx_info.r#type.unwrap_or_default(),
		);
		Ok((signed_tx, receipt))
	}

	/// Extract receipts from block.
	pub async fn extract_from_block(
		&self,
		block: &SubstrateBlock,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		if let Some(earliest_receipt_block) = self.earliest_receipt_block {
			if block.number() < earliest_receipt_block {
				log::trace!(target: LOG_TARGET, "Block number {block_number} is less than earliest receipt block {earliest_receipt_block}. Skipping.", block_number = block.number(), earliest_receipt_block = earliest_receipt_block);
				return Ok(vec![]);
			}
		}

		// Filter extrinsics from pallet_revive
		let extrinsics = block.extrinsics().await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Error fetching for #{:?} extrinsics: {err:?}", block.number());
		})?;

		let extrinsics = extrinsics.iter().flat_map(|ext| {
			let call = ext.as_extrinsic::<EthTransact>().ok()??;
			Some((ext, call))
		});

		stream::iter(extrinsics)
			.map(|(ext, call)| async move { self.extract_from_extrinsic(block, ext, call).await })
			.buffer_unordered(10)
			.collect::<Vec<Result<_, _>>>()
			.await
			.into_iter()
			.collect::<Result<Vec<_>, _>>()
	}

	/// Extract receipt from transaction
	pub async fn extract_from_transaction(
		&self,
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
		self.extract_from_extrinsic(block, ext, call).await
	}
}
