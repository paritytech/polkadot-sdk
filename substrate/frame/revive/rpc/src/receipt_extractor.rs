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
	client::{storage_api::StorageApi, SubstrateBlock, SubstrateBlockNumber},
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
	evm::{
		block_hash::ReceiptGasInfo, GenericTransaction, Log, ReceiptInfo, TransactionSigned, H256,
		U256,
	},
};
use sp_core::keccak_256;
use std::{future::Future, pin::Pin, sync::Arc};
use subxt::{blocks::ExtrinsicDetails, OnlineClient};

type FetchGasPriceFn = Arc<
	dyn Fn(H256) -> Pin<Box<dyn Future<Output = Result<U256, ClientError>> + Send>> + Send + Sync,
>;

type FetchReceiptDataFn = Arc<
	dyn Fn(H256) -> Pin<Box<dyn Future<Output = Option<Vec<ReceiptGasInfo>>> + Send>> + Send + Sync,
>;

type FetchEthBlockHashFn =
	Arc<dyn Fn(H256, u64) -> Pin<Box<dyn Future<Output = Option<H256>> + Send>> + Send + Sync>;

/// Utility to extract receipts from extrinsics.
#[derive(Clone)]
pub struct ReceiptExtractor {
	/// Fetch the receipt data info.
	fetch_receipt_data: FetchReceiptDataFn,

	/// Fetch ethereum block hash.
	fetch_eth_block_hash: FetchEthBlockHashFn,

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
	/// Check if the block is before the earliest block.
	pub fn is_before_earliest_block(&self, block_number: SubstrateBlockNumber) -> bool {
		block_number < self.earliest_receipt_block.unwrap_or_default()
	}

	/// Create a new `ReceiptExtractor` with the given native to eth ratio.
	pub async fn new(
		api: OnlineClient<SrcChainConfig>,
		earliest_receipt_block: Option<SubstrateBlockNumber>,
	) -> Result<Self, ClientError> {
		let native_to_eth_ratio = native_to_eth_ratio(&api).await?;

		let api_inner = api.clone();
		let fetch_eth_block_hash = Arc::new(move |block_hash, block_number| {
			let api_inner = api_inner.clone();

			let fut = async move {
				let storage_api = StorageApi::new(api_inner.storage().at(block_hash));
				storage_api.get_ethereum_block_hash(block_number).await.ok()
			};

			Box::pin(fut) as Pin<Box<_>>
		});

		let api_inner = api.clone();
		let fetch_gas_price = Arc::new(move |block_hash| {
			let api_inner = api_inner.clone();

			let fut = async move {
				let runtime_api = api_inner.runtime_api().at(block_hash);
				let payload = subxt_client::apis().revive_api().gas_price();
				let base_gas_price = runtime_api.call(payload).await?;
				Ok(*base_gas_price)
			};

			Box::pin(fut) as Pin<Box<_>>
		});

		let api_inner = api.clone();
		let fetch_receipt_data = Arc::new(move |block_hash| {
			let api_inner = api_inner.clone();

			let fut = async move {
				let storage_api = StorageApi::new(api_inner.storage().at(block_hash));
				storage_api.get_receipt_data().await.ok()
			};

			Box::pin(fut) as Pin<Box<_>>
		});

		Ok(Self {
			fetch_receipt_data,
			fetch_eth_block_hash,
			fetch_gas_price,
			native_to_eth_ratio,
			earliest_receipt_block,
		})
	}

	#[cfg(test)]
	pub fn new_mock() -> Self {
		let fetch_receipt_data = Arc::new(|_| Box::pin(std::future::ready(None)) as Pin<Box<_>>);
		// This method is useful when testing eth - substrate mapping.
		let fetch_eth_block_hash = Arc::new(|block_hash: H256, block_number: u64| {
			// Generate hash from substrate block hash and number
			let bytes: Vec<u8> = [block_hash.as_bytes(), &block_number.to_be_bytes()].concat();
			let eth_block_hash = H256::from(keccak_256(&bytes));
			Box::pin(std::future::ready(Some(eth_block_hash))) as Pin<Box<_>>
		});
		let fetch_gas_price =
			Arc::new(|_| Box::pin(std::future::ready(Ok(U256::from(1000)))) as Pin<Box<_>>);

		Self {
			fetch_receipt_data,
			fetch_eth_block_hash,
			fetch_gas_price,
			native_to_eth_ratio: 1_000_000,
			earliest_receipt_block: None,
		}
	}

	/// Extract a [`TransactionSigned`] and a [`ReceiptInfo`] from an extrinsic.
	async fn extract_from_extrinsic(
		&self,
		substrate_block: &SubstrateBlock,
		eth_block_hash: H256,
		ext: subxt::blocks::ExtrinsicDetails<SrcChainConfig, subxt::OnlineClient<SrcChainConfig>>,
		call: EthTransact,
		maybe_receipt: Option<ReceiptGasInfo>,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let events = ext.events().await?;
		let block_number: U256 = substrate_block.number().into();

		let success = events.has::<ExtrinsicSuccess>().inspect_err(|err| {
			log::debug!(
				target: LOG_TARGET,
				"Failed to lookup for ExtrinsicSuccess event in block {block_number}: {err:?}"
			);
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

		let base_gas_price = (self.fetch_gas_price)(substrate_block.hash()).await?;
		let tx_info =
			GenericTransaction::from_signed(signed_tx.clone(), base_gas_price, Some(from));

		let gas_price = tx_info.gas_price.unwrap_or_default();
		let gas_used = if let Some(receipt) = maybe_receipt {
			// If we have a receipt, use it to accurately represent the gas.
			U256::from(receipt.gas_used)
		} else {
			// Otherwise, calculate gas used from the transaction fees.
			U256::from(tx_fees.tip.saturating_add(tx_fees.actual_fee))
				.saturating_mul(self.native_to_eth_ratio.into())
				.checked_div(gas_price)
				.unwrap_or_default()
		};

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
					block_hash: eth_block_hash,
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

		let receipt = ReceiptInfo::new(
			eth_block_hash,
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
		if self.is_before_earliest_block(block.number()) {
			return Ok(vec![]);
		}

		let ext_iter = self.get_block_extrinsics(block).await?;

		let substrate_block_number = block.number() as u64;
		let substrate_block_hash = block.hash();
		let eth_block_hash =
			(self.fetch_eth_block_hash)(substrate_block_hash, substrate_block_number)
				.await
				.unwrap_or(substrate_block_hash);

		// Process extrinsics in order while maintaining parallelism within buffer window
		stream::iter(ext_iter)
			.enumerate()
			.map(|(idx, (ext, call, receipt))| async move {
				self.extract_from_extrinsic(block, eth_block_hash, ext, call, receipt, idx)
					.await
					.inspect_err(|err| {
						log::warn!(target: LOG_TARGET, "Error extracting extrinsic: {err:?}");
					})
			})
			.buffered(10)
			.collect::<Vec<Result<_, _>>>()
			.await
			.into_iter()
			.collect::<Result<Vec<_>, _>>()
	}

	/// Return the ETH extrinsics of the block grouped with reconstruction receipt info.
	async fn get_block_extrinsics(
		&self,
		block: &SubstrateBlock,
	) -> Result<
		impl Iterator<
			Item = (
				ExtrinsicDetails<SrcChainConfig, OnlineClient<SrcChainConfig>>,
				EthTransact,
				Option<ReceiptGasInfo>,
			),
		>,
		ClientError,
	> {
		// Filter extrinsics from pallet_revive
		let extrinsics = block.extrinsics().await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Error fetching for #{:?} extrinsics: {err:?}", block.number());
		})?;

		let mut maybe_data = (self.fetch_receipt_data)(block.hash()).await;
		let extrinsics: Vec<_> = extrinsics
			.iter()
			.flat_map(|ext| {
				let call = ext.as_extrinsic::<EthTransact>().ok()??;
				Some((ext, call))
			})
			.collect();

		// Sanity check we received enough data from the pallet revive.
		if let Some(data) = &mut maybe_data {
			if data.len() != extrinsics.len() {
				log::warn!(
					target: LOG_TARGET,
					"Receipt data length ({}) does not match extrinsics length ({})",
					data.len(),
					extrinsics.len()
				);
				maybe_data = None;
			}
		}

		Ok(extrinsics
			.into_iter()
			.zip(
				maybe_data
					.unwrap_or_default()
					.into_iter()
					.map(Some)
					.chain(std::iter::repeat(None)),
			)
			.map(|((extr, call), rec)| (extr, call, rec)))
	}

	/// Extract receipt from transaction
	pub async fn extract_from_transaction(
		&self,
		block: &SubstrateBlock,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let ext_iter = self.get_block_extrinsics(block).await?;

		let (ext, eth_call, maybe_receipt) = ext_iter
			.into_iter()
			.nth(transaction_index)
			.ok_or(ClientError::EthExtrinsicNotFound)?;

		let substrate_block_number = block.number() as u64;
		let substrate_block_hash = block.hash();
		let eth_block_hash =
			(self.fetch_eth_block_hash)(substrate_block_hash, substrate_block_number)
				.await
				.unwrap_or(substrate_block_hash);

		self.extract_from_extrinsic(
			block,
			eth_block_hash,
			ext,
			eth_call,
			maybe_receipt,
			transaction_index,
		)
		.await
	}

	/// Get the Ethereum block hash for the given Substrate block.
	pub async fn get_ethereum_block_hash(
		&self,
		block_hash: &H256,
		block_number: u64,
	) -> Option<H256> {
		(self.fetch_eth_block_hash)(*block_hash, block_number).await
	}
}
