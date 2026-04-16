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
	ClientError, H160, LOG_TARGET,
	client::{SubstrateBlock, SubstrateBlockNumber, runtime_api::RuntimeApi},
	subxt_client::{
		SrcChainConfig,
		revive::{
			calls::types::EthTransact,
			events::{ContractEmitted, EthExtrinsicRevert},
		},
	},
};

use futures::{StreamExt, stream};
use pallet_revive::{
	create1,
	evm::{GenericTransaction, H256, Log, ReceiptGasInfo, ReceiptInfo, TransactionSigned, U256},
};
use sp_core::keccak_256;
use std::{
	future::Future,
	pin::Pin,
	sync::{
		Arc,
		atomic::{AtomicU32, Ordering},
	},
};
use subxt::{OnlineClient, events::StaticEvent};

type ExtrinsicEvents = subxt::blocks::ExtrinsicEvents<SrcChainConfig>;
type ExtrinsicDetails =
	subxt::blocks::ExtrinsicDetails<SrcChainConfig, OnlineClient<SrcChainConfig>>;

type FetchReceiptDataFn = Arc<
	dyn Fn(H256) -> Pin<Box<dyn Future<Output = Option<Vec<ReceiptGasInfo>>> + Send>> + Send + Sync,
>;

type FetchEthBlockHashFn =
	Arc<dyn Fn(H256, u64) -> Pin<Box<dyn Future<Output = Option<H256>> + Send>> + Send + Sync>;

type RecoverEthAddressFn = Arc<dyn Fn(&TransactionSigned) -> Result<H160, ()> + Send + Sync>;

/// Utility to extract receipts from extrinsics.
#[derive(Clone)]
pub struct ReceiptExtractor {
	/// Fetch the receipt data info.
	fetch_receipt_data: FetchReceiptDataFn,

	/// Fetch ethereum block hash.
	fetch_eth_block_hash: FetchEthBlockHashFn,

	/// Auto-discovered first EVM block on the chain.
	/// Set once during backward sync when the first non-EVM block is encountered.
	/// Uses `u32::MAX` as sentinel for "not yet discovered".
	first_evm_block: Arc<AtomicU32>,

	/// Recover the ethereum address from a transaction signature.
	recover_eth_address: RecoverEthAddressFn,
}

impl ReceiptExtractor {
	/// Create a new `ReceiptExtractor`.
	pub async fn new(api: OnlineClient<SrcChainConfig>) -> Result<Self, ClientError> {
		Self::new_with_custom_address_recovery(
			api,
			Arc::new(|signed_tx: &TransactionSigned| signed_tx.recover_eth_address()),
		)
		.await
	}

	/// Create a new `ReceiptExtractor` with custom Ethereum address recovery logic.
	///
	/// Use `ReceiptExtractor::new` if the default Ethereum address recovery
	/// logic ([`TransactionSigned::recover_eth_address`] based) is enough.
	pub async fn new_with_custom_address_recovery(
		api: OnlineClient<SrcChainConfig>,
		recover_eth_address_fn: RecoverEthAddressFn,
	) -> Result<Self, ClientError> {
		let api_inner = api.clone();
		let fetch_eth_block_hash = Arc::new(move |block_hash, block_number| {
			let api_inner = api_inner.clone();

			let fut = async move {
				let runtime_api = RuntimeApi::new(api_inner.runtime_api().at(block_hash));
				runtime_api.eth_block_hash(U256::from(block_number)).await.ok().flatten()
			};

			Box::pin(fut) as Pin<Box<_>>
		});

		let api_inner = api.clone();
		let fetch_receipt_data = Arc::new(move |block_hash| {
			let api_inner = api_inner.clone();

			let fut = async move {
				let runtime_api = RuntimeApi::new(api_inner.runtime_api().at(block_hash));
				runtime_api.eth_receipt_data().await.ok()
			};

			Box::pin(fut) as Pin<Box<_>>
		});

		Ok(Self {
			fetch_receipt_data,
			fetch_eth_block_hash,
			first_evm_block: Arc::new(AtomicU32::new(u32::MAX)),
			recover_eth_address: recover_eth_address_fn,
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

		Self {
			fetch_receipt_data,
			fetch_eth_block_hash,
			first_evm_block: Arc::new(AtomicU32::new(u32::MAX)),
			recover_eth_address: Arc::new(|signed_tx: &TransactionSigned| {
				signed_tx.recover_eth_address()
			}),
		}
	}

	/// Check if the block is before the `first_evm_block` floor.
	/// When sentinel (`u32::MAX`), no blocks are rejected (permissive default).
	pub fn is_before_first_evm_block(&self, block_number: SubstrateBlockNumber) -> bool {
		let val = self.first_evm_block.load(Ordering::Acquire);
		val != u32::MAX && block_number < val
	}

	/// Set the first EVM block. Only stores if lower than the current value.
	pub fn set_first_evm_block(&self, block_number: SubstrateBlockNumber) {
		let prev = self.first_evm_block.fetch_min(block_number, Ordering::AcqRel);
		if block_number > prev {
			log::debug!(target: LOG_TARGET,
				"Ignored attempt to raise first_evm_block to #{block_number}, current is #{prev}");
		}
	}

	/// The auto-discovered first EVM block, or `None` if not yet discovered.
	pub fn first_evm_block(&self) -> Option<SubstrateBlockNumber> {
		let val = self.first_evm_block.load(Ordering::Acquire);
		(val != u32::MAX).then_some(val)
	}

	/// Resolve the Ethereum block hash for a substrate block, falling back to the substrate hash.
	async fn resolve_eth_block_hash(
		&self,
		substrate_block_hash: H256,
		substrate_block_number: u64,
	) -> H256 {
		match (self.fetch_eth_block_hash)(substrate_block_hash, substrate_block_number).await {
			Some(hash) => hash,
			None => {
				log::trace!(target: LOG_TARGET,
					"eth_block_hash returned None for substrate block \
					 #{substrate_block_number} ({substrate_block_hash:?}), \
					 falling back to substrate hash as ETH hash");
				substrate_block_hash
			},
		}
	}

	/// Extract revert status and logs from block events in a single pass.
	///
	/// Events are stored sequentially without size markers, so a single
	/// undecodable event (e.g. from a runtime upgrade that shifted variant
	/// indices) corrupts the offset for all subsequent events. We log and
	/// skip decode errors to avoid losing the entire receipt.
	fn extract_revert_status_and_logs(
		events: &ExtrinsicEvents,
		block_number: U256,
		transaction_hash: H256,
		transaction_index: usize,
		eth_block_hash: H256,
	) -> (bool, Vec<Log>) {
		let mut success = true;
		let mut logs = Vec::new();

		for event_details in events.iter().enumerate().filter_map(|(idx, ev)| {
			ev.inspect_err(|err| {
				log::debug!(
					target: LOG_TARGET,
					"Failed to decode event {idx} in block {block_number} (tx {transaction_hash:?}): {err:?}"
				);
			})
			.ok()
		}) {
			// Both EthExtrinsicRevert and ContractEmitted belong to pallet Revive.
			if event_details.pallet_name() != ContractEmitted::PALLET {
				continue;
			}

			if event_details.variant_name() == EthExtrinsicRevert::EVENT {
				success = false;
			} else if event_details.variant_name() == ContractEmitted::EVENT {
				if let Some(event) = event_details.as_event::<ContractEmitted>().ok().flatten() {
					logs.push(Log {
						address: event.contract,
						topics: event.topics,
						data: Some(event.data.into()),
						block_number,
						transaction_hash,
						transaction_index: transaction_index.into(),
						block_hash: eth_block_hash,
						log_index: event_details.index().into(),
						..Default::default()
					});
				} else {
					log::warn!(
						target: LOG_TARGET,
						"Failed to decode ContractEmitted event {} in block {block_number} (tx {transaction_hash:?}), log dropped from receipt",
						event_details.index()
					);
				}
			}
		}

		(success, logs)
	}

	/// Extract a [`TransactionSigned`] and a [`ReceiptInfo`] from an extrinsic.
	async fn extract_from_extrinsic(
		&self,
		substrate_block: &SubstrateBlock,
		eth_block_hash: H256,
		ext: ExtrinsicDetails,
		call: EthTransact,
		receipt_gas_info: ReceiptGasInfo,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let block_number: U256 = substrate_block.number().into();
		let transaction_hash = H256(keccak_256(&call.payload));

		let (success, logs) = Self::extract_revert_status_and_logs(
			&ext.events().await?,
			block_number,
			transaction_hash,
			transaction_index,
			eth_block_hash,
		);

		let signed_tx =
			TransactionSigned::decode(&call.payload).map_err(|_| ClientError::TxDecodingFailed)?;
		let from = (self.recover_eth_address)(&signed_tx).map_err(|_| {
			log::error!(target: LOG_TARGET, "Failed to recover eth address from signed tx");
			ClientError::RecoverEthAddressFailed
		})?;

		let tx_info = GenericTransaction::from_signed(
			signed_tx.clone(),
			receipt_gas_info.effective_gas_price,
			Some(from),
		);

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
			receipt_gas_info.effective_gas_price,
			U256::from(receipt_gas_info.gas_used),
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
		let eth_block_hash = self.resolve_eth_block_hash(block.hash(), block.number() as u64).await;

		self.extract_from_block_with_eth_hash(block, eth_block_hash).await
	}

	/// Extract receipts from block, using a pre-fetched ethereum block hash.
	pub async fn extract_from_block_with_eth_hash(
		&self,
		block: &SubstrateBlock,
		eth_block_hash: H256,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		if self.is_before_first_evm_block(block.number()) {
			return Ok(vec![]);
		}

		let ext_iter = self.get_block_extrinsics(block).await?;

		// Process extrinsics in order while maintaining parallelism within buffer window
		stream::iter(ext_iter)
			.map(|(ext, call, receipt, ext_idx)| async move {
				self.extract_from_extrinsic(block, eth_block_hash, ext, call, receipt, ext_idx)
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

	/// Return the ETH extrinsics of the block grouped with reconstruction receipt info and
	/// extrinsic index
	pub async fn get_block_extrinsics(
		&self,
		block: &SubstrateBlock,
	) -> Result<
		impl Iterator<Item = (ExtrinsicDetails, EthTransact, ReceiptGasInfo, usize)>,
		ClientError,
	> {
		// Filter extrinsics from pallet_revive
		let extrinsics = block.extrinsics().await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Error fetching for #{:?} extrinsics: {err:?}", block.number());
		})?;

		let receipt_data = (self.fetch_receipt_data)(block.hash()).await.ok_or_else(|| {
			log::trace!(target: LOG_TARGET,
				"Receipt data not found for block #{} ({:?})",
				block.number(), block.hash());
			ClientError::ReceiptDataNotFound
		})?;
		let extrinsics: Vec<_> = extrinsics
			.iter()
			.enumerate()
			.flat_map(|(ext_idx, ext)| {
				let call = ext.as_extrinsic::<EthTransact>().ok()??;
				Some((ext, call, ext_idx))
			})
			.collect();

		// Sanity check we received enough data from the pallet revive.
		if receipt_data.len() != extrinsics.len() {
			log::error!(
				target: LOG_TARGET,
				"Receipt data length ({}) does not match extrinsics length ({})",
				receipt_data.len(),
				extrinsics.len()
			);
			Err(ClientError::ReceiptDataLengthMismatch)
		} else {
			Ok(extrinsics
				.into_iter()
				.zip(receipt_data)
				.map(|((extr, call, ext_idx), rec)| (extr, call, rec, ext_idx)))
		}
	}

	/// Extract a [`TransactionSigned`] and a [`ReceiptInfo`] for a specific transaction in a
	/// [`SubstrateBlock`]
	pub async fn extract_from_transaction(
		&self,
		block: &SubstrateBlock,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let ext_iter = self.get_block_extrinsics(block).await?;

		let (ext, eth_call, receipt_gas_info, _) = ext_iter
			.into_iter()
			.find(|(_, _, _, ext_idx)| *ext_idx == transaction_index)
			.ok_or_else(|| {
				log::trace!(target: LOG_TARGET,
					"extract_from_transaction: no EVM extrinsic at tx_index {transaction_index} \
					 in block #{} ({:?})", block.number(), block.hash());
				ClientError::EthExtrinsicNotFound
			})?;

		let substrate_block_number = block.number() as u64;
		let substrate_block_hash = block.hash();
		let eth_block_hash =
			self.resolve_eth_block_hash(substrate_block_hash, substrate_block_number).await;

		self.extract_from_extrinsic(
			block,
			eth_block_hash,
			ext,
			eth_call,
			receipt_gas_info,
			transaction_index,
		)
		.await
	}

	/// Get the Ethereum block hash for the Substrate block with specific hash.
	pub async fn get_ethereum_block_hash(
		&self,
		block_hash: &H256,
		block_number: u64,
	) -> Option<H256> {
		(self.fetch_eth_block_hash)(*block_hash, block_number).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn defaults_and_first_evm_block_only_decreases() {
		let extractor = ReceiptExtractor::new_mock();

		assert!(extractor.first_evm_block().is_none());

		// first_evm_block only decreases
		extractor.set_first_evm_block(100);
		assert_eq!(extractor.first_evm_block(), Some(100));

		extractor.set_first_evm_block(50);
		assert_eq!(extractor.first_evm_block(), Some(50));

		// Higher value is ignored
		extractor.set_first_evm_block(100);
		assert_eq!(extractor.first_evm_block(), Some(50));
	}
}
