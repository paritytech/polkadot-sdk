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

use pallet_revive::{
	create1,
	evm::{GenericTransaction, H256, Log, ReceiptGasInfo, ReceiptInfo, TransactionSigned, U256},
};
use sp_core::keccak_256;
use std::{
	collections::{HashMap, HashSet},
	future::Future,
	pin::Pin,
	sync::{
		Arc,
		atomic::{AtomicU32, Ordering},
	},
};
use subxt::{
	OnlineClient,
	events::{Phase, StaticEvent},
};

type EventDetails = subxt::events::EventDetails<SrcChainConfig>;

/// Outcome of decoding a single pallet-revive event.
enum ReviveEvent {
	Revert,
	Log(Log),
}

/// Decode a single event detail into a [`ReviveEvent`], or `None` if it is not a pallet-revive
/// event we care about.
fn decode_revive_event(
	event: &EventDetails,
	block_number: U256,
	transaction_hash: H256,
	transaction_index: u32,
	eth_block_hash: H256,
) -> Option<ReviveEvent> {
	if event.pallet_name() != ContractEmitted::PALLET {
		return None;
	}
	if event.variant_name() == EthExtrinsicRevert::EVENT {
		return Some(ReviveEvent::Revert);
	}
	if event.variant_name() == ContractEmitted::EVENT {
		match event.as_event::<ContractEmitted>().ok().flatten() {
			Some(evt) => {
				return Some(ReviveEvent::Log(Log {
					address: evt.contract,
					topics: evt.topics,
					data: Some(evt.data.into()),
					block_number,
					transaction_hash,
					transaction_index: transaction_index.into(),
					block_hash: eth_block_hash,
					log_index: event.index().into(),
					..Default::default()
				}));
			},
			None => log::warn!(
				target: LOG_TARGET,
				"Failed to decode ContractEmitted event {} in block {block_number} (tx {transaction_hash:?}), log dropped from receipt",
				event.index()
			),
		}
	}
	None
}

/// Fetch block events and collect revert flags and logs for the given EthTransact
/// extrinsics in a single pass. Events for other extrinsics are skipped.
///
/// Returns `(revert_set, logs_by_ext)` keyed by extrinsic index.
async fn extract_revive_events(
	block: &SubstrateBlock,
	block_number: U256,
	eth_block_hash: H256,
	tx_hash_for: impl Fn(u32) -> Option<H256>,
) -> Result<(HashSet<u32>, HashMap<u32, Vec<Log>>), ClientError> {
	let mut revert_set: HashSet<u32> = HashSet::new();
	let mut logs_by_ext: HashMap<u32, Vec<Log>> = HashMap::new();

	let block_events = block.events().await.inspect_err(|err| {
		log::debug!(
			target: LOG_TARGET,
			"Error fetching events for block #{}: {err:?}",
			block.number()
		);
	})?;

	for (idx, event_result) in block_events.iter().enumerate() {
		let event = match event_result {
			Ok(e) => e,
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"Failed to decode event {idx} in block #{}: {err:?}",
					block.number()
				);
				continue;
			},
		};

		let ext_idx = match event.phase() {
			Phase::ApplyExtrinsic(i) => i,
			_ => continue,
		};

		let Some(tx_hash) = tx_hash_for(ext_idx) else { continue };

		match decode_revive_event(&event, block_number, tx_hash, ext_idx, eth_block_hash) {
			Some(ReviveEvent::Revert) => {
				revert_set.insert(ext_idx);
			},
			Some(ReviveEvent::Log(log)) => {
				logs_by_ext.entry(ext_idx).or_default().push(log);
			},
			None => {},
		}
	}

	Ok((revert_set, logs_by_ext))
}

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

	/// Decode the raw call payload into a [`TransactionSigned`] and construct its [`ReceiptInfo`].
	fn decode_transaction_and_build_receipt(
		&self,
		eth_block_hash: H256,
		block_number: U256,
		call: EthTransact,
		transaction_hash: H256,
		receipt_gas_info: ReceiptGasInfo,
		transaction_index: usize,
		success: bool,
		logs: Vec<Log>,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
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
	///
	/// Fetches block events once in a single pass before building receipts
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

		let (ext_list, tx_hash_by_ext): (Vec<_>, HashMap<u32, H256>) = self
			.get_block_extrinsics(block)
			.await?
			.map(|(call, rec, ext_idx)| {
				let hash = H256(keccak_256(&call.payload));
				let ext_idx = ext_idx as u32;
				((call, hash, rec, ext_idx), (ext_idx, hash))
			})
			.unzip();

		if ext_list.is_empty() {
			return Ok(vec![]);
		}

		let block_number: U256 = block.number().into();
		let eth_block_hash =
			self.resolve_eth_block_hash(block.hash(), block.number() as u64).await;

		let (revert_set, mut logs_by_ext) =
			extract_revive_events(block, block_number, eth_block_hash, |idx| {
				tx_hash_by_ext.get(&idx).copied()
			})
			.await?;

		ext_list
			.into_iter()
			.map(|(call, transaction_hash, receipt_gas_info, ext_idx)| {
				let success = !revert_set.contains(&ext_idx);
				let logs = logs_by_ext.remove(&ext_idx).unwrap_or_default();
				self.decode_transaction_and_build_receipt(
					eth_block_hash,
					block_number,
					call,
					transaction_hash,
					receipt_gas_info,
					ext_idx as usize,
					success,
					logs,
				)
				.inspect_err(|err| {
					log::warn!(target: LOG_TARGET, "Error extracting extrinsic: {err:?}");
				})
			})
			.collect()
	}

	/// Return the ETH extrinsics of the block grouped with reconstruction receipt info and
	/// extrinsic index
	pub async fn get_block_extrinsics(
		&self,
		block: &SubstrateBlock,
	) -> Result<impl Iterator<Item = (EthTransact, ReceiptGasInfo, usize)>, ClientError> {
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
				Some((call, ext_idx))
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
				.map(|((call, ext_idx), rec)| (call, rec, ext_idx)))
		}
	}

	/// Extract a [`TransactionSigned`] and a [`ReceiptInfo`] for a specific transaction in a
	/// [`SubstrateBlock`]
	pub async fn extract_from_transaction(
		&self,
		block: &SubstrateBlock,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let (eth_call, receipt_gas_info, transaction_hash) = self
			.get_block_extrinsics(block)
			.await?
			.find_map(|(call, rec, ext_idx)| {
				if ext_idx != transaction_index {
					return None;
				}
				let hash = H256(keccak_256(&call.payload));
				Some((call, rec, hash))
			})
			.ok_or_else(|| {
				log::trace!(target: LOG_TARGET,
					"extract_from_transaction: no EVM extrinsic at tx_index {transaction_index} \
					 in block #{} ({:?})", block.number(), block.hash());
				ClientError::EthExtrinsicNotFound
			})?;

		let substrate_block_hash = block.hash();
		let eth_block_hash =
			self.resolve_eth_block_hash(substrate_block_hash, substrate_block_number).await;

		let eth_block_number: U256 = block.number().into();

		let (revert_set, mut logs_by_ext) =
			extract_revive_events(block, eth_block_number, eth_block_hash, |idx| {
				(idx == transaction_index as u32).then_some(transaction_hash)
			})
			.await?;

		let success = !revert_set.contains(&(transaction_index as u32));
		let logs = logs_by_ext.remove(&(transaction_index as u32)).unwrap_or_default();

		self.decode_transaction_and_build_receipt(
			eth_block_hash,
			eth_block_number,
			eth_call,
			transaction_hash,
			receipt_gas_info,
			transaction_index,
			success,
			logs,
		)
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
