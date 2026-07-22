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
	ClientError, H160, LOG_TARGET, Log, ReceiptInfo, client::SubstrateBlockNumber,
	substrate_client::RawExtrinsic,
};
use pallet_revive::{
	create1,
	evm::{GenericTransaction, H256, TransactionSigned, U256},
};
use pallet_revive_types::runtime_api::ReceiptGasInfoV1;
use sp_crypto_hashing::keccak_256;
use std::{
	future::Future,
	pin::Pin,
	sync::{
		Arc,
		atomic::{AtomicU32, Ordering},
	},
};

const ETH_TRANSACT_CALL_INDEX: u8 = 0;

/// Sentinel value meaning "not yet discovered".
const NOT_DISCOVERED: u32 = u32::MAX;

type FetchReceiptDataFn = Arc<
	dyn Fn(H256) -> Pin<Box<dyn Future<Output = Option<Vec<ReceiptGasInfoV1>>> + Send>>
		+ Send
		+ Sync,
>;

type FetchEthBlockHashFn =
	Arc<dyn Fn(H256, u64) -> Pin<Box<dyn Future<Output = Option<H256>> + Send>> + Send + Sync>;

type RecoverEthAddressFn = Arc<dyn Fn(&TransactionSigned) -> Result<H160, ()> + Send + Sync>;

type FetchBlockEventsFn =
	Arc<dyn Fn(H256) -> Pin<Box<dyn Future<Output = Option<BlockEvents>> + Send>> + Send + Sync>;

/// Fetches the raw extrinsics for a given block hash.
type FetchBlockExtrinsicsFn = Arc<
	dyn Fn(H256) -> Pin<Box<dyn Future<Output = Option<Vec<RawExtrinsic>>> + Send>> + Send + Sync,
>;

#[derive(Clone, Debug, Default)]
pub struct ExtrinsicEvents {
	pub success: bool,
	pub logs: Vec<(H160, Vec<H256>, Option<Vec<u8>>)>,
}

/// All extrinsic events for a block.
pub type BlockEvents = Vec<ExtrinsicEvents>;

/// Utility to extract receipts from extrinsics.
#[derive(Clone)]
pub struct ReceiptExtractor {
	/// Fetch the receipt data info.
	fetch_receipt_data: FetchReceiptDataFn,

	/// Fetch ethereum block hash.
	fetch_eth_block_hash: FetchEthBlockHashFn,
	/// Fetch decoded events for every extrinsic in a block (optional).
	fetch_block_events: Option<FetchBlockEventsFn>,
	/// Fetch raw extrinsics for a block.
	fetch_block_extrinsics: Option<FetchBlockExtrinsicsFn>,

	/// The earliest block from which receipts should be extracted.
	/// Configured at startup; blocks before this are skipped entirely.
	earliest_receipt_block: Option<SubstrateBlockNumber>,

	/// Auto-discovered first EVM block on the chain.
	/// Stored as `NOT_DISCOVERED` (u32::MAX) when not yet known.
	/// Only ever decreases once set.
	first_evm_block: Arc<AtomicU32>,

	/// Recover the ethereum address from a transaction signature.
	recover_eth_address: RecoverEthAddressFn,

	/// The pallet index of `pallet-revive` in this runtime.
	pallet_index: u8,
}

impl ReceiptExtractor {
	/// Check if the block is before the earliest block.
	pub fn is_before_earliest_block(&self, block_number: SubstrateBlockNumber) -> bool {
		block_number < self.earliest_receipt_block.unwrap_or_default()
	}

	/// Check if `block_number` is before the auto-discovered `first_evm_block`.
	/// Returns `false` when first_evm_block has not yet been discovered.
	pub fn is_before_first_evm_block(&self, block_number: SubstrateBlockNumber) -> bool {
		let first = self.first_evm_block.load(Ordering::Acquire);
		if first == NOT_DISCOVERED {
			return false;
		}
		block_number < first
	}

	/// Return the auto-discovered first EVM block, or `None` if not yet discovered.
	pub fn first_evm_block(&self) -> Option<SubstrateBlockNumber> {
		let v = self.first_evm_block.load(Ordering::Acquire);
		if v == NOT_DISCOVERED { None } else { Some(v) }
	}

	/// Set the auto-discovered first EVM block.
	/// The value only decreases: if a smaller block number is discovered later, it wins.
	pub fn set_first_evm_block(&self, block_number: SubstrateBlockNumber) {
		// Use a compare-and-swap loop so the value only ever decreases.
		let mut current = self.first_evm_block.load(Ordering::Acquire);
		loop {
			if current != NOT_DISCOVERED && block_number >= current {
				// Already at or below this value; nothing to do.
				return;
			}
			match self.first_evm_block.compare_exchange(
				current,
				block_number,
				Ordering::AcqRel,
				Ordering::Acquire,
			) {
				Ok(_) => return,
				Err(actual) => current = actual,
			}
		}
	}

	/// Create a `ReceiptExtractor` for the **native in-process** path with custom closures.
	pub fn new_native(
		pallet_index: u8,
		fetch_receipt_data_fn: impl Fn(
			H256,
		) -> Pin<
			Box<dyn Future<Output = Option<Vec<ReceiptGasInfoV1>>> + Send>,
		> + Send
		+ Sync
		+ 'static,
		fetch_eth_block_hash_fn: impl Fn(
			H256,
			u64,
		) -> Pin<Box<dyn Future<Output = Option<H256>> + Send>>
		+ Send
		+ Sync
		+ 'static,
		earliest_receipt_block: Option<SubstrateBlockNumber>,
		fetch_block_events_fn: Option<
			impl Fn(H256) -> Pin<Box<dyn Future<Output = Option<BlockEvents>> + Send>>
			+ Send
			+ Sync
			+ 'static,
		>,
		fetch_block_extrinsics_fn: Option<
			impl Fn(H256) -> Pin<Box<dyn Future<Output = Option<Vec<RawExtrinsic>>> + Send>>
			+ Send
			+ Sync
			+ 'static,
		>,
	) -> Self {
		Self {
			fetch_receipt_data: Arc::new(fetch_receipt_data_fn),
			fetch_eth_block_hash: Arc::new(fetch_eth_block_hash_fn),
			fetch_block_events: fetch_block_events_fn.map(|f| Arc::new(f) as FetchBlockEventsFn),
			fetch_block_extrinsics: fetch_block_extrinsics_fn
				.map(|f| Arc::new(f) as FetchBlockExtrinsicsFn),
			earliest_receipt_block,
			first_evm_block: Arc::new(AtomicU32::new(NOT_DISCOVERED)),
			recover_eth_address: Arc::new(|signed_tx: &TransactionSigned| {
				signed_tx.recover_eth_address()
			}),
			pallet_index,
		}
	}

	/// Build a `ReceiptExtractor` for the native path directly from a Substrate client.
	#[allow(deprecated)]
	pub fn new_native_from_client<Client, Block, Moment>(
		client: Arc<Client>,
		earliest_receipt_block: Option<SubstrateBlockNumber>,
		pallet_index: u8,
	) -> Self
	where
		Block: sp_runtime::traits::Block<Hash = sp_core::H256, Extrinsic = sp_runtime::OpaqueExtrinsic>
			+ Send
			+ Sync
			+ 'static,
		Moment: codec::Codec + Clone + Send + Sync + 'static,
		Client: sp_api::ProvideRuntimeApi<Block>
			+ sc_client_api::HeaderBackend<Block>
			+ sc_client_api::BlockBackend<Block>
			+ Send
			+ Sync
			+ 'static,
		Client::Api: crate::native_client::ReviveRuntimeApiT<Block, Moment>,
	{
		use codec::Encode;
		use pallet_revive::ReviveApi;

		let client_for_data = client.clone();
		let fetch_receipt_data_fn = move |block_hash: H256| {
			let client = client_for_data.clone();
			let fut = async move { client.runtime_api().eth_receipt_data(block_hash).ok() };
			Box::pin(fut) as Pin<Box<dyn Future<Output = Option<Vec<ReceiptGasInfoV1>>> + Send>>
		};

		let client_for_hash = client.clone();
		let fetch_eth_block_hash_fn = move |block_hash: H256, block_number: u64| {
			let client = client_for_hash.clone();
			let fut = async move {
				client
					.runtime_api()
					.eth_block_hash(block_hash, U256::from(block_number))
					.ok()
					.flatten()
			};
			Box::pin(fut) as Pin<Box<dyn Future<Output = Option<H256>> + Send>>
		};

		let client_for_ext = client.clone();
		let fetch_block_extrinsics_fn = move |block_hash: H256| {
			let client = client_for_ext.clone();
			let fut = async move {
				let body = client.block_body(block_hash).ok()??;
				Some(
					body.into_iter()
						.enumerate()
						.map(|(index, ext)| RawExtrinsic { payload: ext.encode(), index })
						.collect(),
				)
			};
			Box::pin(fut) as Pin<Box<dyn Future<Output = Option<Vec<RawExtrinsic>>> + Send>>
		};

		Self::new_native(
			pallet_index,
			fetch_receipt_data_fn,
			fetch_eth_block_hash_fn,
			earliest_receipt_block,
			None::<fn(H256) -> Pin<Box<dyn Future<Output = Option<BlockEvents>> + Send>>>,
			Some(fetch_block_extrinsics_fn),
		)
	}

	/// Create a `ReceiptExtractor` from a [`crate::SubstrateClient`] implementation.
	pub fn new_from_substrate_client<C>(
		client: C,
		earliest_receipt_block: Option<SubstrateBlockNumber>,
	) -> Self
	where
		C: crate::SubstrateClient + Clone + 'static,
	{
		let pallet_index = client.pallet_revive_index();

		let client_for_data = client.clone();
		let fetch_receipt_data_fn = move |block_hash: H256| {
			let c = client_for_data.clone();
			let fut = async move { c.eth_receipt_data(block_hash).await.ok() };
			Box::pin(fut) as Pin<Box<dyn Future<Output = Option<Vec<ReceiptGasInfoV1>>> + Send>>
		};

		let client_for_hash = client.clone();
		let fetch_eth_block_hash_fn = move |block_hash: H256, block_number: u64| {
			let c = client_for_hash.clone();
			let fut = async move {
				c.eth_block_hash(block_hash, U256::from(block_number)).await.ok().flatten()
			};
			Box::pin(fut) as Pin<Box<dyn Future<Output = Option<H256>> + Send>>
		};

		let client_for_ext = client.clone();
		let fetch_block_extrinsics_fn = move |block_hash: H256| {
			let c = client_for_ext.clone();
			let fut = async move { c.block_extrinsics(block_hash).await.ok() };
			Box::pin(fut) as Pin<Box<dyn Future<Output = Option<Vec<RawExtrinsic>>> + Send>>
		};

		let client_for_events = client.clone();
		let fetch_block_events_fn = move |block_hash: H256| {
			let c = client_for_events.clone();
			let fut = async move { c.block_events(block_hash).await.ok().flatten() };
			Box::pin(fut) as Pin<Box<dyn Future<Output = Option<BlockEvents>> + Send>>
		};

		Self::new_native(
			pallet_index,
			fetch_receipt_data_fn,
			fetch_eth_block_hash_fn,
			earliest_receipt_block,
			Some(fetch_block_events_fn),
			Some(fetch_block_extrinsics_fn),
		)
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
			fetch_block_events: None,
			fetch_block_extrinsics: None,
			earliest_receipt_block: None,
			first_evm_block: Arc::new(AtomicU32::new(NOT_DISCOVERED)),
			recover_eth_address: Arc::new(|signed_tx: &TransactionSigned| {
				signed_tx.recover_eth_address()
			}),
			pallet_index: 0,
		}
	}

	/// Try to decode a raw extrinsic as an `eth_transact` call.
	fn decode_eth_transact(&self, raw: &RawExtrinsic) -> Option<Vec<u8>> {
		let bytes = &raw.payload;

		let mut offset = 0usize;
		let first = *bytes.get(offset)?;
		let len_len = match first & 0b11 {
			0 => 1,
			1 => 2,
			2 => 4,
			_ => return None,
		};
		offset += len_len;

		let _version = *bytes.get(offset)?;
		offset += 1;

		let pallet_idx = *bytes.get(offset)?;
		offset += 1;
		let call_idx = *bytes.get(offset)?;
		offset += 1;

		if pallet_idx != self.pallet_index || call_idx != ETH_TRANSACT_CALL_INDEX {
			return None;
		}

		use codec::Decode;
		let mut rest = &bytes[offset..];
		let payload = Vec::<u8>::decode(&mut rest).ok()?;
		Some(payload)
	}

	/// Extract a receipt from a decoded call and its associated events.
	fn build_receipt(
		&self,
		substrate_block_number: u32,
		eth_block_hash: H256,
		ext_events: &ExtrinsicEvents,
		eth_payload: &[u8],
		receipt_gas_info: &ReceiptGasInfoV1,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let transaction_hash = H256(keccak_256(eth_payload));
		let signed_tx =
			TransactionSigned::decode(eth_payload).map_err(|_| ClientError::TxDecodingFailed)?;

		let from = (self.recover_eth_address)(&signed_tx).map_err(|_| {
			log::error!(target: LOG_TARGET, "Failed to recover eth address from signed tx");
			ClientError::RecoverEthAddressFailed
		})?;

		let tx_info = GenericTransaction::from_signed(
			signed_tx.clone(),
			receipt_gas_info.effective_gas_price,
			Some(from),
		);

		let block_number: U256 = substrate_block_number.into();

		let logs = ext_events
			.logs
			.iter()
			.enumerate()
			.map(|(log_idx, (address, topics, data))| Log {
				address: *address,
				topics: topics.clone(),
				data: data.as_ref().map(|d| d.clone().into()),
				block_number,
				transaction_hash,
				transaction_index: transaction_index.into(),
				block_hash: eth_block_hash,
				log_index: log_idx.into(),
				..Default::default()
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
			receipt_gas_info.effective_gas_price,
			U256::from(receipt_gas_info.gas_used),
			ext_events.success,
			transaction_hash,
			transaction_index.into(),
			tx_info.r#type.unwrap_or_default(),
		);
		Ok((signed_tx, receipt))
	}

	/// Extract receipts from a block using `RawExtrinsic` data.
	pub async fn extract_from_block_raw(
		&self,
		_block_hash: H256,
		block_number: u32,
		eth_block_hash: H256,
		raw_extrinsics: &[RawExtrinsic],
		receipt_data: &[ReceiptGasInfoV1],
		block_events: Option<&[ExtrinsicEvents]>,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		let eth_extrinsics: Vec<(usize, Vec<u8>)> = raw_extrinsics
			.iter()
			.filter_map(|raw| {
				let payload = self.decode_eth_transact(raw)?;
				Some((raw.index, payload))
			})
			.collect();

		if eth_extrinsics.len() != receipt_data.len() {
			log::error!(
				target: LOG_TARGET,
				"Receipt data length ({}) != eth extrinsics ({}) for block #{block_number}",
				receipt_data.len(),
				eth_extrinsics.len()
			);
			return Err(ClientError::ReceiptDataLengthMismatch);
		}

		let mut results = Vec::with_capacity(eth_extrinsics.len());
		for (i, (ext_index, eth_payload)) in eth_extrinsics.iter().enumerate() {
			let events = if let Some(all_events) = block_events {
				all_events.get(*ext_index).cloned().unwrap_or_default()
			} else {
				// Native path: no event scanning available; mark success = true.
				ExtrinsicEvents { success: true, logs: vec![] }
			};

			let receipt = self.build_receipt(
				block_number,
				eth_block_hash,
				&events,
				eth_payload,
				&receipt_data[i],
				*ext_index,
			)?;
			results.push(receipt);
		}
		Ok(results)
	}

	/// Get the Ethereum block hash for the Substrate block with specific hash.
	pub async fn get_ethereum_block_hash(
		&self,
		block_hash: &H256,
		block_number: u64,
	) -> Option<H256> {
		(self.fetch_eth_block_hash)(*block_hash, block_number).await
	}

	/// Extract receipts from a block, given only its hash and number.
	pub async fn extract_from_block_by_hash(
		&self,
		block_hash: H256,
		block_number: SubstrateBlockNumber,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		if self.is_before_earliest_block(block_number) {
			return Ok(vec![]);
		}

		let eth_block_hash = (self.fetch_eth_block_hash)(block_hash, block_number as u64)
			.await
			.unwrap_or(block_hash);

		let receipt_data = (self.fetch_receipt_data)(block_hash)
			.await
			.ok_or(ClientError::ReceiptDataNotFound)?;

		let block_events = if let Some(ref fetch_events) = self.fetch_block_events {
			fetch_events(block_hash).await
		} else {
			None
		};

		let raw = if let Some(ref fetch_extrinsics) = self.fetch_block_extrinsics {
			fetch_extrinsics(block_hash).await.ok_or(ClientError::BlockNotFound)?
		} else {
			return Err(ClientError::NativeClientError(
				"fetch_block_extrinsics not configured".to_string(),
			));
		};

		self.extract_from_block_raw(
			block_hash,
			block_number,
			eth_block_hash,
			&raw,
			&receipt_data,
			block_events.as_deref(),
		)
		.await
	}

	/// Extract a single receipt from a block by transaction index.
	pub async fn extract_from_transaction_by_hash(
		&self,
		block_hash: H256,
		block_number: SubstrateBlockNumber,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let receipts = self.extract_from_block_by_hash(block_hash, block_number).await?;
		receipts
			.into_iter()
			.find(|(_, r)| r.transaction_index.as_usize() == transaction_index)
			.ok_or(ClientError::EthExtrinsicNotFound)
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use pallet_revive::evm::{Account, TransactionLegacyUnsigned, TransactionUnsigned};

	fn signed_call(account: &Account, tx: TransactionUnsigned) -> (Vec<u8>, H256) {
		let payload = account.sign_transaction(tx).signed_payload();
		let hash = H256(keccak_256(&payload));
		(payload, hash)
	}

	fn legacy_call_tx(to: H160) -> TransactionUnsigned {
		TransactionUnsigned::from(TransactionLegacyUnsigned {
			chain_id: Some(U256::from(1)),
			to: Some(to),
			gas: U256::from(21_000),
			..Default::default()
		})
	}

	fn gas_info() -> ReceiptGasInfoV1 {
		ReceiptGasInfoV1 {
			gas_used: U256::from(21_000),
			effective_gas_price: U256::from(1_000_000_000),
		}
	}

	#[test]
	fn build_receipt_for_call() {
		let extractor = ReceiptExtractor::new_mock();
		let account = Account::default();
		let eth_block_hash = H256::from([0xAB; 32]);
		let block_number = 42u32;
		let (payload, tx_hash) = signed_call(&account, legacy_call_tx(account.address()));

		// Successful call
		let (signed_tx, receipt) = extractor
			.build_receipt(
				block_number,
				eth_block_hash,
				&ExtrinsicEvents { success: true, logs: vec![] },
				&payload,
				&gas_info(),
				3,
			)
			.unwrap();

		assert!(receipt.is_success());
		assert_eq!(receipt.from, account.address());
		assert_eq!(receipt.to, Some(account.address()));
		assert_eq!(receipt.contract_address, None);
		assert_eq!(receipt.block_hash, eth_block_hash);
		assert_eq!(receipt.block_number, U256::from(block_number));
		assert_eq!(receipt.transaction_hash, tx_hash);
		assert_eq!(receipt.transaction_index, U256::from(3));
		assert_eq!(receipt.gas_used, U256::from(21_000));
		assert_eq!(signed_tx.recover_eth_address().unwrap(), account.address());

		// Same call, but reverted
		let (payload, _tx_hash) = signed_call(&account, legacy_call_tx(account.address()));
		let (_, receipt) = extractor
			.build_receipt(
				block_number,
				eth_block_hash,
				&ExtrinsicEvents { success: false, logs: vec![] },
				&payload,
				&gas_info(),
				3,
			)
			.unwrap();

		assert!(!receipt.is_success());
		assert_eq!(receipt.from, account.address());
	}

	#[test]
	fn build_receipt_for_deploy() {
		let extractor = ReceiptExtractor::new_mock();
		let account = Account::default();
		let deploy_tx = TransactionUnsigned::from(TransactionLegacyUnsigned {
			chain_id: Some(U256::from(1)),
			gas: U256::from(100_000),
			nonce: U256::from(0),
			..Default::default()
		});
		let (payload, _) = signed_call(&account, deploy_tx);

		let (_, receipt) = extractor
			.build_receipt(
				1u32,
				H256::zero(),
				&ExtrinsicEvents { success: true, logs: vec![] },
				&payload,
				&gas_info(),
				0,
			)
			.unwrap();

		assert!(receipt.is_success());
		assert_eq!(receipt.to, None);
		assert_eq!(receipt.contract_address, Some(create1(&account.address(), 0)));
		assert_eq!(receipt.from, account.address());
	}

	#[test]
	fn build_receipt_rejects_invalid_payload() {
		let extractor = ReceiptExtractor::new_mock();

		// Corrupt payload
		let payload = vec![0xde, 0xadu8];
		let err = extractor
			.build_receipt(
				1u32,
				H256::zero(),
				&ExtrinsicEvents { success: true, logs: vec![] },
				&payload,
				&gas_info(),
				0,
			)
			.unwrap_err();
		assert!(matches!(err, ClientError::TxDecodingFailed));

		// Valid payload but address recovery fails
		let extractor = ReceiptExtractor {
			recover_eth_address: Arc::new(|_| Err(())),
			..ReceiptExtractor::new_mock()
		};
		let account = Account::default();
		let (payload, _) = signed_call(&account, legacy_call_tx(account.address()));
		let err = extractor
			.build_receipt(
				1u32,
				H256::zero(),
				&ExtrinsicEvents { success: true, logs: vec![] },
				&payload,
				&gas_info(),
				0,
			)
			.unwrap_err();
		assert!(matches!(err, ClientError::RecoverEthAddressFailed));
	}

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
