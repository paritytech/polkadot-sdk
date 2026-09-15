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
	ClientError, H160, LOG_TARGET, Log, ReceiptGasInfoV1, ReceiptInfo, SyntheticTransactionV1,
	client::{
		SubstrateBlock, SubstrateBlockNumber,
		version_aware_runtime_api::VersionAwareRuntimeApiProvider,
	},
	subxt_client::{
		SrcChainConfig,
		revive::{
			calls::EthTransact,
			events::{ContractEmitted, EthExtrinsicRevert},
		},
	},
};

use pallet_revive::{
	create1,
	evm::{GenericTransaction, H256, TransactionSigned, U256},
};
use sp_crypto_hashing::keccak_256;
use std::{
	collections::{BTreeMap, HashMap, HashSet},
	future::Future,
	pin::Pin,
	sync::{
		Arc,
		atomic::{AtomicU64, Ordering},
	},
};
use subxt::{
	client::OfflineClientAtBlockT,
	events::{DecodeAsEvent, Phase},
	extrinsics::Extrinsics,
};

type EventDetails<'a> = subxt::events::Event<'a, SrcChainConfig>;

/// Outcome of decoding a single pallet-revive event.
enum ReviveEvent {
	Revert,
	Log(Log),
}

/// Decode a single event detail into a [`ReviveEvent`], or `None` if it is not a pallet-revive
/// event we care about.
fn decode_revive_event(
	event: &EventDetails<'_>,
	block_number: U256,
	transaction_hash: H256,
	transaction_index: usize,
	block_hash: H256,
) -> Option<ReviveEvent> {
	let pallet_name = event.pallet_name();
	let event_name = event.event_name();

	if EthExtrinsicRevert::is_event(pallet_name, event_name) {
		return Some(ReviveEvent::Revert);
	}
	if ContractEmitted::is_event(pallet_name, event_name) {
		match event.decode_fields_as::<ContractEmitted>() {
			Some(Ok(evt)) => {
				return Some(ReviveEvent::Log(Log {
					address: evt.contract,
					topics: evt.topics,
					data: Some(evt.data.into()),
					block_number,
					transaction_hash,
					transaction_index: transaction_index.into(),
					block_hash,
					log_index: event.index().into(),
					..Default::default()
				}));
			},
			Some(Err(err)) => log::warn!(
				target: LOG_TARGET,
				"Failed to decode ContractEmitted event {} in block {block_number} (tx {transaction_hash:?}): {err:?}, log dropped from receipt",
				event.index()
			),
			// `is_event()` already confirmed the variant, so this is unreachable in practice.
			None => {},
		}
	}
	None
}

/// Iterate decoded block events and bucket revert flags and logs per extrinsic.
///
/// `ContractEmitted` logs whose extrinsic is not an ethereum transaction (`eth_tx_hash_for`
/// returns `None`) are "outside-of-frame" logs — emitted by a runtime component (e.g. a
/// pallet-assets balance-change mirror) during a plain extrinsic. The runtime aggregates them into
/// one synthetic transaction per block; here they are collected into a separate bucket, tagged with
/// the synthetic transaction's hash and index, and returned alongside the per-extrinsic logs.
///
/// Events are stored sequentially without size markers, so a single
/// undecodable event (e.g. from a runtime upgrade that shifted variant
/// indices) corrupts the offset for all subsequent events.
/// Decode errors are logged and skipped to avoid losing the entire receipt.
///
/// Returns `(reverted_extrinsics, logs_by_extrinsic, outside_frame_logs)`.
fn extract_revive_events(
	block_events: &subxt::events::Events<SrcChainConfig>,
	substrate_block_number: SubstrateBlockNumber,
	eth_block_number: U256,
	eth_block_hash: H256,
	eth_tx_hash_for: impl Fn(usize) -> Option<H256>,
	synthetic_tx_hash: H256,
	synthetic_tx_index: usize,
) -> (HashSet<usize>, HashMap<usize, Vec<Log>>, Vec<Log>) {
	let mut reverted_extrinsics: HashSet<usize> = HashSet::new();
	let mut logs_by_extrinsic: HashMap<usize, Vec<Log>> = HashMap::new();
	let mut outside_frame_logs: Vec<Log> = Vec::new();

	for (event_index, event_result) in block_events.iter().enumerate() {
		let event = match event_result {
			Ok(e) => e,
			Err(err) => {
				log::debug!(
					target: LOG_TARGET,
					"Failed to decode event {event_index} in block #{substrate_block_number}: {err:?}"
				);
				continue;
			},
		};

		// Only an `ApplyExtrinsic` event can belong to an ethereum transaction. A mirror firing in
		// `on_initialize` (e.g. the message queue servicing an inbound XCM asset deposit) is
		// committed into the synthetic transaction just like one from a plain extrinsic, so treat
		// every other phase as outside-of-frame rather than dropping it.
		let eth_tx = match event.phase() {
			Phase::ApplyExtrinsic(idx) => {
				let idx = idx as usize;
				eth_tx_hash_for(idx).map(|hash| (idx, hash))
			},
			Phase::Initialization | Phase::Finalization => None,
		};

		match eth_tx {
			Some((extrinsic_index, eth_tx_hash)) => match decode_revive_event(
				&event,
				eth_block_number,
				eth_tx_hash,
				extrinsic_index,
				eth_block_hash,
			) {
				Some(ReviveEvent::Revert) => {
					reverted_extrinsics.insert(extrinsic_index);
				},
				Some(ReviveEvent::Log(log)) => {
					logs_by_extrinsic.entry(extrinsic_index).or_default().push(log);
				},
				None => {},
			},
			// Not an ethereum transaction: a `ContractEmitted` here is an outside-of-frame log,
			// attributed to the block's synthetic transaction. Reverts are meaningless here.
			None => {
				if let Some(ReviveEvent::Log(log)) = decode_revive_event(
					&event,
					eth_block_number,
					synthetic_tx_hash,
					synthetic_tx_index,
					eth_block_hash,
				) {
					outside_frame_logs.push(log);
				}
			},
		}
	}

	(reverted_extrinsics, logs_by_extrinsic, outside_frame_logs)
}

/// Returns the revive transactions from a block.
fn extract_eth_transacts<C: OfflineClientAtBlockT<SrcChainConfig>>(
	block_extrinsics: &Extrinsics<'_, SrcChainConfig, C>,
	block_number: SubstrateBlockNumber,
) -> Result<Vec<(EthTransact, usize)>, ClientError> {
	let mut extrinsics = Vec::new();
	for (ext_idx, ext) in block_extrinsics.iter().enumerate() {
		let ext = match ext {
			Ok(ext) => ext,
			// Don't error here since the call type is unknown. An undecodable `eth_transact` shows
			// up as a length mismatch against the runtime's gas entries.
			Err(err) => {
				log::debug!(target: LOG_TARGET,
					"Failed to decode extrinsic {ext_idx} of block #{block_number}: {err:?}");
				continue;
			},
		};
		match ext.decode_call_data_fields_as::<EthTransact>() {
			Some(Ok(call)) => extrinsics.push((call, ext_idx)),
			Some(Err(err)) => {
				log::error!(target: LOG_TARGET,
					"Failed to decode the EthTransact call in extrinsic {ext_idx} of block \
					#{block_number}: {err:?}");
				return Err(subxt::Error::from(err).into());
			},
			// Not a revive transaction.
			None => {},
		}
	}

	Ok(extrinsics)
}

/// Reconcile the runtime's per-transaction gas entries against the ethereum transactions decoded
/// from the block body.
///
/// A difference means the two sides disagree about what the block contains — most likely an
/// extrinsic the metadata could not decode. Erroring beats pairing every later transaction with the
/// wrong gas info.
fn check_receipt_data_len(
	receipt_data: &[ReceiptGasInfoV1],
	extrinsics_len: usize,
) -> Result<(), ClientError> {
	if receipt_data.len() != extrinsics_len {
		log::error!(
			target: LOG_TARGET,
			"Receipt data length ({}) does not match extrinsics length ({extrinsics_len})",
			receipt_data.len(),
		);
		return Err(ClientError::ReceiptDataLengthMismatch);
	}

	Ok(())
}

/// Cut the logs rebuilt from block events down to what the block actually committed.
///
/// The two can disagree: the runtime bounds the buffer these logs are drained from, and a log that
/// arrives past the bound is still deposited as an event. The events are therefore a superset, in
/// emission order, of what reached the block's `logs_bloom` and `receipts_root` — so the committed
/// set is the first `committed` of them, and serving more would hand out logs the header does not
/// commit to.
///
/// A count only catches a difference in size. Losing one log to a decode failure (see
/// [`extract_revive_events`]) while the buffer dropped another leaves the counts equal over
/// different members, which this cannot see; closing that would need the runtime to report the logs
/// themselves rather than how many there were.
fn reconcile_outside_frame_logs(
	mut logs: Vec<Log>,
	committed: u32,
	substrate_block_number: SubstrateBlockNumber,
) -> Vec<Log> {
	let committed = committed as usize;
	if logs.len() == committed {
		return logs;
	}

	log::warn!(
		target: LOG_TARGET,
		"Block #{substrate_block_number} committed {committed} outside-of-frame log(s) but {} \
		decoded from its events",
		logs.len(),
	);
	logs.truncate(committed);
	logs
}

/// Outcome of querying the runtime for a block's receipt gas entries.
enum ReceiptData {
	/// The block's entries: one per ethereum transaction, plus the synthetic transaction's when the
	/// runtime reports one.
	Available { receipt_data: Vec<ReceiptGasInfoV1>, synthetic: Option<SyntheticTransactionV1> },
	/// The runtime at this block has no `eth_receipt_data` API.
	///
	/// Permanent, and expected for pre-EVM history — such a block has neither ethereum
	/// transactions nor mirrored logs, so it is read as having no entries rather than as an error.
	Unsupported,
	/// The query failed. May succeed on retry, so it must not be mistaken for an empty block.
	Failed,
}

type FetchReceiptDataFn =
	Arc<dyn Fn(SubstrateBlock) -> Pin<Box<dyn Future<Output = ReceiptData> + Send>> + Send + Sync>;

type FetchEthBlockHashFn = Arc<
	dyn Fn(H256, SubstrateBlockNumber) -> Pin<Box<dyn Future<Output = Option<H256>> + Send>>
		+ Send
		+ Sync,
>;

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
	/// Uses `u64::MAX` as sentinel for "not yet discovered".
	first_evm_block: Arc<AtomicU64>,

	/// Recover the ethereum address from a transaction signature.
	recover_eth_address: RecoverEthAddressFn,

	/// EVM chain id. Used to rebuild the synthetic transaction that carries a block's
	/// outside-of-frame logs (its hash must match the one the runtime committed).
	chain_id: u64,
}

impl ReceiptExtractor {
	/// Create a new `ReceiptExtractor`.
	pub async fn new(
		runtime_api_provider: VersionAwareRuntimeApiProvider,
	) -> Result<Self, ClientError> {
		Self::new_with_custom_address_recovery(
			runtime_api_provider,
			Arc::new(|signed_tx: &TransactionSigned| signed_tx.recover_eth_address()),
		)
		.await
	}

	/// Create a new `ReceiptExtractor` with custom Ethereum address recovery logic.
	///
	/// Use `ReceiptExtractor::new` if the default Ethereum address recovery
	/// logic ([`TransactionSigned::recover_eth_address`] based) is enough.
	pub async fn new_with_custom_address_recovery(
		runtime_api_provider: VersionAwareRuntimeApiProvider,
		recover_eth_address_fn: RecoverEthAddressFn,
	) -> Result<Self, ClientError> {
		let chain_id = {
			let query = crate::subxt_client::constants().revive().chain_id().unvalidated();
			let at_block = runtime_api_provider.api().at_current_block().await?;
			at_block.constants().entry(query)?
		};

		let provider = runtime_api_provider.clone();
		let fetch_eth_block_hash = Arc::new(move |substrate_block_hash, substrate_block_number| {
			let provider = provider.clone();

			let fut = async move {
				let runtime_api = provider
					.at_block_hash_and_number(substrate_block_hash, substrate_block_number)
					.await
					.inspect_err(|err| {
						log::debug!(
							target: LOG_TARGET,
							"Failed to access the runtime API at block #{substrate_block_number} \
							({substrate_block_hash:?}) for an eth_block_hash query: {err:?}"
						);
					})
					.ok()?;
				runtime_api
					.eth_block_hash(U256::from(substrate_block_number))?
					.await
					.inspect_err(|err| {
						log::debug!(
							target: LOG_TARGET,
							"Failed to query eth_block_hash at block #{substrate_block_number} \
							({substrate_block_hash:?}): {err:?}"
						);
					})
					.ok()
					.flatten()
			};

			Box::pin(fut) as Pin<Box<_>>
		});

		let provider = runtime_api_provider;
		let fetch_receipt_data = Arc::new(move |at_block: SubstrateBlock| {
			let provider = provider.clone();

			let fut = async move {
				let block_hash = at_block.block_hash();
				let runtime_api = match provider.at_resolved_block(at_block).await {
					Ok(api) => api,
					Err(err) => {
						log::debug!(
							target: LOG_TARGET,
							"Failed to access the runtime API at block {block_hash:?} for an \
							eth_receipt_data query: {err:?}"
						);
						return ReceiptData::Failed;
					},
				};
				let Some(query) = runtime_api.eth_receipt_data() else {
					return ReceiptData::Unsupported;
				};
				match query.await {
					Ok((receipt_data, synthetic)) => {
						ReceiptData::Available { receipt_data, synthetic }
					},
					Err(err) => {
						log::debug!(
							target: LOG_TARGET,
							"Failed to query eth_receipt_data at block {block_hash:?}: {err:?}"
						);
						ReceiptData::Failed
					},
				}
			};

			Box::pin(fut) as Pin<Box<_>>
		});

		Ok(Self {
			fetch_receipt_data,
			fetch_eth_block_hash,
			first_evm_block: Arc::new(AtomicU64::new(u64::MAX)),
			recover_eth_address: recover_eth_address_fn,
			chain_id,
		})
	}

	#[cfg(test)]
	pub fn new_mock() -> Self {
		let fetch_receipt_data =
			Arc::new(|_| Box::pin(std::future::ready(ReceiptData::Unsupported)) as Pin<Box<_>>);
		// This method is useful when testing eth - substrate mapping.
		let fetch_eth_block_hash =
			Arc::new(|block_hash: H256, block_number: SubstrateBlockNumber| {
				// Generate hash from substrate block hash and number
				let bytes: Vec<u8> = [block_hash.as_bytes(), &block_number.to_be_bytes()].concat();
				let eth_block_hash = H256::from(keccak_256(&bytes));
				Box::pin(std::future::ready(Some(eth_block_hash))) as Pin<Box<_>>
			});

		Self {
			fetch_receipt_data,
			fetch_eth_block_hash,
			first_evm_block: Arc::new(AtomicU64::new(u64::MAX)),
			recover_eth_address: Arc::new(|signed_tx: &TransactionSigned| {
				signed_tx.recover_eth_address()
			}),
			chain_id: 420_420_420,
		}
	}

	/// Check if the block is before the `first_evm_block` floor.
	/// When sentinel (`u64::MAX`), no blocks are rejected (permissive default).
	pub fn is_before_first_evm_block(&self, block_number: SubstrateBlockNumber) -> bool {
		let val = self.first_evm_block.load(Ordering::Acquire);
		val != u64::MAX && block_number < val
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
		(val != u64::MAX).then_some(val)
	}

	/// Resolve the Ethereum block hash for a substrate block, falling back to the substrate hash.
	async fn resolve_eth_block_hash(
		&self,
		substrate_block_hash: H256,
		substrate_block_number: SubstrateBlockNumber,
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
		transaction_index: usize,
		receipt_gas_info: ReceiptGasInfoV1,
		reverted: bool,
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
			!reverted,
			transaction_hash,
			transaction_index.into(),
			tx_info.r#type.unwrap_or_default(),
		);
		Ok((signed_tx, receipt))
	}

	/// Rebuild the block's synthetic transaction payload and its hash. Must reproduce the exact
	/// bytes the runtime committed, hence the shared
	/// [`pallet_revive::evm::synthetic_log_transaction`] keyed by chain id and block number.
	fn synthetic_tx(&self, eth_block_number: U256) -> (Vec<u8>, H256) {
		let payload = pallet_revive::evm::synthetic_log_transaction(
			eth_block_number,
			U256::from(self.chain_id),
		);
		let hash = H256(keccak_256(&payload));
		(payload, hash)
	}

	/// Assemble the receipt for the block's synthetic transaction, carrying its outside-of-frame
	/// logs. The synthetic transaction has no real sender: `from` is zero and `to` /
	/// `contract_address` are `None`.
	fn build_synthetic_receipt(
		&self,
		eth_block_hash: H256,
		eth_block_number: U256,
		transaction_index: usize,
		gas_info: ReceiptGasInfoV1,
		logs: Vec<Log>,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let (payload, transaction_hash) = self.synthetic_tx(eth_block_number);
		let signed_tx =
			TransactionSigned::decode(&payload).map_err(|_| ClientError::TxDecodingFailed)?;
		let receipt = ReceiptInfo::new(
			eth_block_hash,
			eth_block_number,
			None,
			H160::zero(),
			logs,
			None,
			gas_info.effective_gas_price,
			U256::from(gas_info.gas_used),
			true,
			transaction_hash,
			transaction_index.into(),
			Default::default(),
		);
		Ok((signed_tx, receipt))
	}

	/// Extract receipts from block.
	pub async fn extract_from_block(
		&self,
		block: &SubstrateBlock,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		let eth_block_hash =
			self.resolve_eth_block_hash(block.block_hash(), block.block_number()).await;

		self.extract_from_block_with_eth_hash(block, eth_block_hash).await
	}

	/// Extract receipts from block, using a pre-fetched ethereum block hash.
	///
	/// Fetches block events once in a single pass before building receipts.
	pub async fn extract_from_block_with_eth_hash(
		&self,
		block: &SubstrateBlock,
		eth_block_hash: H256,
	) -> Result<Vec<(TransactionSigned, ReceiptInfo)>, ClientError> {
		if self.is_before_first_evm_block(block.block_number()) {
			return Ok(vec![]);
		}

		let (extrinsics, synthetic) = self.get_block_extrinsics(block).await?;
		let eth_tx_by_index: BTreeMap<usize, (EthTransact, H256, ReceiptGasInfoV1)> = extrinsics
			.into_iter()
			.map(|(call, receipt_gas_info, extrinsic_index)| {
				let hash = H256(keccak_256(&call.payload));
				(extrinsic_index, (call, hash, receipt_gas_info))
			})
			.collect();

		// Nothing to reconstruct: no ethereum transactions and no synthetic transaction (the
		// latter is present iff the runtime reported one).
		if eth_tx_by_index.is_empty() && synthetic.is_none() {
			return Ok(vec![]);
		}

		// The synthetic transaction is appended after every ethereum transaction.
		let synthetic_tx_index = eth_tx_by_index.keys().max().map_or(0, |max| max + 1);

		let substrate_block_number = block.block_number();
		let eth_block_number: U256 = substrate_block_number.into();
		let (_, synthetic_tx_hash) = self.synthetic_tx(eth_block_number);
		let block_events = block.events().fetch().await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Error fetching events for block #{substrate_block_number}: {err:?}");
		})?;
		let (reverted_extrinsics, mut logs_by_extrinsic, outside_frame_logs) =
			extract_revive_events(
				&block_events,
				substrate_block_number,
				eth_block_number,
				eth_block_hash,
				|idx| eth_tx_by_index.get(&idx).map(|(_, hash, _)| *hash),
				synthetic_tx_hash,
				synthetic_tx_index,
			);

		let mut receipts: Vec<_> = eth_tx_by_index
			.into_iter()
			.map(|(transaction_index, (call, transaction_hash, receipt_gas_info))| {
				let reverted = reverted_extrinsics.contains(&transaction_index);
				let logs = logs_by_extrinsic.remove(&transaction_index).unwrap_or_default();
				self.decode_transaction_and_build_receipt(
					eth_block_hash,
					eth_block_number,
					call,
					transaction_hash,
					transaction_index,
					receipt_gas_info,
					reverted,
					logs,
				)
				.inspect_err(|err| {
					log::warn!(target: LOG_TARGET, "Error extracting extrinsic: {err:?}");
				})
			})
			.collect::<Result<Vec<_>, _>>()?;

		// Append the synthetic transaction receipt for the block's outside-of-frame logs. Keyed on
		// the runtime's entry — the same condition `extract_from_transaction` uses — because that
		// entry is what the block header commits to. Deciding on the decoded logs instead would
		// omit a transaction the block contains whenever the logs fail to decode.
		if let Some(synthetic) = synthetic {
			let logs = reconcile_outside_frame_logs(
				outside_frame_logs,
				synthetic.log_count,
				substrate_block_number,
			);

			receipts.push(self.build_synthetic_receipt(
				eth_block_hash,
				eth_block_number,
				synthetic_tx_index,
				synthetic.gas_info,
				logs,
			)?);
		}

		Ok(receipts)
	}

	/// Return the ETH extrinsics of the block grouped with reconstruction receipt info and
	/// extrinsic index, plus the block's synthetic transaction when it has one.
	///
	/// See [`check_receipt_data_len`] for how the gas entries are reconciled.
	async fn get_block_extrinsics(
		&self,
		block: &SubstrateBlock,
	) -> Result<
		(Vec<(EthTransact, ReceiptGasInfoV1, usize)>, Option<SyntheticTransactionV1>),
		ClientError,
	> {
		let block_extrinsics = block.extrinsics().fetch().await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Error fetching for #{:?} extrinsics: {err:?}", block.block_number());
		})?;

		let block_number = block.block_number();
		let extrinsics = extract_eth_transacts(&block_extrinsics, block_number)?;

		// Queried unconditionally: a block with no ethereum transactions can still carry a
		// synthetic transaction for its outside-of-frame logs.
		let (receipt_data, synthetic) = match (self.fetch_receipt_data)(block.clone()).await {
			ReceiptData::Available { receipt_data, synthetic } => (receipt_data, synthetic),
			// A block predating the runtime API has no ethereum transactions and no mirrored
			// logs, so it reconstructs as an empty block instead of failing the request.
			ReceiptData::Unsupported => (Vec::new(), None),
			ReceiptData::Failed => {
				log::trace!(target: LOG_TARGET,
					"Receipt data not found for block #{} ({:?})",
					block.block_number(), block.block_hash());
				return Err(ClientError::ReceiptDataNotFound);
			},
		};

		check_receipt_data_len(&receipt_data, extrinsics.len())?;

		Ok((
			extrinsics
				.into_iter()
				.zip(receipt_data)
				.map(|((call, ext_idx), rec)| (call, rec, ext_idx))
				.collect(),
			synthetic,
		))
	}

	/// Extract a [`TransactionSigned`] and a [`ReceiptInfo`] for a specific transaction in a
	/// [`SubstrateBlock`]
	pub async fn extract_from_transaction(
		&self,
		block: &SubstrateBlock,
		transaction_index: usize,
	) -> Result<(TransactionSigned, ReceiptInfo), ClientError> {
		let (extrinsics, synthetic) = self.get_block_extrinsics(block).await?;
		let mut eth_tx_by_index: BTreeMap<usize, (EthTransact, H256, ReceiptGasInfoV1)> =
			extrinsics
				.into_iter()
				.map(|(call, receipt_gas_info, extrinsic_index)| {
					let hash = H256(keccak_256(&call.payload));
					(extrinsic_index, (call, hash, receipt_gas_info))
				})
				.collect();

		let synthetic_tx_index = eth_tx_by_index.keys().max().map_or(0, |max| max + 1);
		let is_synthetic = transaction_index == synthetic_tx_index && synthetic.is_some();

		if !eth_tx_by_index.contains_key(&transaction_index) && !is_synthetic {
			log::trace!(target: LOG_TARGET,
				"extract_from_transaction: no EVM extrinsic at tx_index {transaction_index} \
				 in block #{} ({:?})", block.block_number(), block.block_hash());
			return Err(ClientError::EthExtrinsicNotFound);
		}

		let substrate_block_number = block.block_number();
		let eth_block_number: U256 = substrate_block_number.into();
		let eth_block_hash =
			self.resolve_eth_block_hash(block.block_hash(), substrate_block_number).await;
		let (_, synthetic_tx_hash) = self.synthetic_tx(eth_block_number);
		let block_events = block.events().fetch().await.inspect_err(|err| {
			log::debug!(target: LOG_TARGET, "Error fetching events for block #{substrate_block_number}: {err:?}");
		})?;
		let (reverted_extrinsics, mut logs_by_extrinsic, outside_frame_logs) =
			extract_revive_events(
				&block_events,
				substrate_block_number,
				eth_block_number,
				eth_block_hash,
				|idx| eth_tx_by_index.get(&idx).map(|(_, hash, _)| *hash),
				synthetic_tx_hash,
				synthetic_tx_index,
			);

		if is_synthetic {
			let synthetic = synthetic.expect("is_synthetic implies Some; qed");
			let logs = reconcile_outside_frame_logs(
				outside_frame_logs,
				synthetic.log_count,
				substrate_block_number,
			);
			return self.build_synthetic_receipt(
				eth_block_hash,
				eth_block_number,
				synthetic_tx_index,
				synthetic.gas_info,
				logs,
			);
		}

		let (eth_call, transaction_hash, receipt_gas_info) =
			eth_tx_by_index.remove(&transaction_index).expect("presence checked above; qed");
		let reverted = reverted_extrinsics.contains(&transaction_index);
		let logs = logs_by_extrinsic.remove(&transaction_index).unwrap_or_default();
		self.decode_transaction_and_build_receipt(
			eth_block_hash,
			eth_block_number,
			eth_call,
			transaction_hash,
			transaction_index,
			receipt_gas_info,
			reverted,
			logs,
		)
	}

	/// Get the Ethereum block hash for the Substrate block with specific hash.
	pub async fn get_ethereum_block_hash(
		&self,
		block_hash: &H256,
		block_number: SubstrateBlockNumber,
	) -> Option<H256> {
		(self.fetch_eth_block_hash)(*block_hash, block_number).await
	}
}

#[cfg(test)]
mod tests {
	use super::*;

	use pallet_revive::evm::{Account, TransactionLegacyUnsigned, TransactionUnsigned};

	fn signed_call(account: &Account, tx: TransactionUnsigned) -> (EthTransact, H256) {
		let payload = account.sign_transaction(tx).signed_payload();
		let hash = H256(keccak_256(&payload));
		(EthTransact { payload }, hash)
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
		let block_number = U256::from(42);
		let (call, tx_hash) = signed_call(&account, legacy_call_tx(account.address()));

		// Successful call
		let (signed_tx, receipt) = extractor
			.decode_transaction_and_build_receipt(
				eth_block_hash,
				block_number,
				call,
				tx_hash,
				3,
				gas_info(),
				false,
				vec![],
			)
			.unwrap();

		assert!(receipt.is_success());
		assert_eq!(receipt.from, account.address());
		assert_eq!(receipt.to, Some(account.address()));
		assert_eq!(receipt.contract_address, None);
		assert_eq!(receipt.block_hash, eth_block_hash);
		assert_eq!(receipt.block_number, block_number);
		assert_eq!(receipt.transaction_hash, tx_hash);
		assert_eq!(receipt.transaction_index, U256::from(3));
		assert_eq!(receipt.gas_used, U256::from(21_000));
		assert_eq!(signed_tx.recover_eth_address().unwrap(), account.address());

		// Same call, but reverted
		let (call, tx_hash) = signed_call(&account, legacy_call_tx(account.address()));
		let (_, receipt) = extractor
			.decode_transaction_and_build_receipt(
				eth_block_hash,
				block_number,
				call,
				tx_hash,
				3,
				gas_info(),
				true,
				vec![],
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
		let (call, tx_hash) = signed_call(&account, deploy_tx);

		let (_, receipt) = extractor
			.decode_transaction_and_build_receipt(
				H256::zero(),
				U256::from(1),
				call,
				tx_hash,
				0,
				gas_info(),
				false,
				vec![],
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
		let call = EthTransact { payload: vec![0xde, 0xad] };
		let hash = H256(keccak_256(&call.payload));
		let err = extractor
			.decode_transaction_and_build_receipt(
				H256::zero(),
				U256::from(1),
				call,
				hash,
				0,
				gas_info(),
				false,
				vec![],
			)
			.unwrap_err();
		assert!(matches!(err, ClientError::TxDecodingFailed));

		// Valid payload but address recovery fails
		let extractor = ReceiptExtractor {
			recover_eth_address: Arc::new(|_| Err(())),
			..ReceiptExtractor::new_mock()
		};
		let account = Account::default();
		let (call, hash) = signed_call(&account, legacy_call_tx(account.address()));
		let err = extractor
			.decode_transaction_and_build_receipt(
				H256::zero(),
				U256::from(1),
				call,
				hash,
				0,
				gas_info(),
				false,
				vec![],
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

	use crate::block_info_provider::test::chain_config;
	use codec::{Compact, Encode};
	use frame_system::EventRecord;
	use revive_dev_runtime::{Runtime, RuntimeEvent};
	use subxt::{PolkadotConfig, client::OfflineClient, events::Events};

	/// An offline client carrying the generated runtime metadata for every block.
	fn offline_client() -> OfflineClient<PolkadotConfig> {
		OfflineClient::<PolkadotConfig>::new_with_config(chain_config())
	}

	/// Build `Events` by SCALE-encoding revive events against the generated runtime metadata.
	struct EventsBuilder {
		bytes: Vec<u8>,
		count: u32,
	}

	impl EventsBuilder {
		fn new() -> Self {
			Self { bytes: Vec::new(), count: 0 }
		}

		fn push_event(
			mut self,
			phase: frame_system::Phase,
			event: pallet_revive::Event<Runtime>,
		) -> Self {
			EventRecord::<RuntimeEvent, H256> {
				phase,
				event: RuntimeEvent::Revive(event),
				topics: vec![],
			}
			.encode_to(&mut self.bytes);
			self.count += 1;
			self
		}

		fn build(self) -> Events<SrcChainConfig> {
			let mut encoded_events = Vec::new();
			Compact(self.count).encode_to(&mut encoded_events);
			encoded_events.extend(self.bytes);

			let client = offline_client();
			let at_block =
				client.at_block(0u64).expect("spec version range covers all block numbers; qed");
			at_block.events().from_bytes(encoded_events)
		}
	}

	#[test]
	fn extract_revive_events_decodes_contract_emitted_log() {
		let contract = H160::from([0x11; 20]);
		let topics = vec![H256::from([0x22; 32]), H256::from([0x33; 32])];
		let data = vec![0xde, 0xad, 0xbe, 0xef];
		let events = EventsBuilder::new()
			.push_event(
				frame_system::Phase::ApplyExtrinsic(5),
				pallet_revive::Event::ContractEmitted {
					contract,
					data: data.clone(),
					topics: topics.clone(),
				},
			)
			.build();

		let tx_hash = H256::from([0xAA; 32]);
		let eth_block_hash = H256::from([0xBB; 32]);
		let substrate_block_number = 42u64;
		let eth_block_number = U256::from(substrate_block_number);

		let (reverts, logs, outside_frame) = extract_revive_events(
			&events,
			substrate_block_number,
			eth_block_number,
			eth_block_hash,
			|idx| (idx == 5).then_some(tx_hash),
			H256::zero(),
			99,
		);

		assert!(reverts.is_empty());
		assert!(outside_frame.is_empty());
		assert_eq!(logs.len(), 1);
		let log = &logs[&5][0];
		assert_eq!(log.address, contract);
		assert_eq!(log.topics, topics);
		assert_eq!(log.data.as_ref().unwrap().0, data);
		assert_eq!(log.block_hash, eth_block_hash);
		assert_eq!(log.block_number, eth_block_number);
		assert_eq!(log.transaction_hash, tx_hash);
		assert_eq!(log.transaction_index, U256::from(5));
	}

	#[test]
	fn extract_revive_events_buckets_non_eth_logs_as_outside_frame() {
		// A `ContractEmitted` that belongs to no ethereum transaction — because its extrinsic is
		// not one, or because it has no extrinsic at all — becomes an outside-of-frame log
		// attributed to the synthetic transaction. Reverts are meaningless in both cases and are
		// ignored.
		let empty_contract_emitted = pallet_revive::Event::ContractEmitted {
			contract: H160::zero(),
			data: vec![],
			topics: vec![],
		};
		let revert = pallet_revive::Event::EthExtrinsicRevert {
			dispatch_error: sp_runtime::DispatchError::Other("skipped-phase revert"),
		};
		let events = EventsBuilder::new()
			.push_event(frame_system::Phase::Finalization, empty_contract_emitted.clone())
			.push_event(frame_system::Phase::Initialization, revert.clone())
			.push_event(frame_system::Phase::ApplyExtrinsic(5), empty_contract_emitted)
			.push_event(frame_system::Phase::ApplyExtrinsic(5), revert)
			.build();

		let synthetic_hash = H256::from([0x99; 32]);
		// The tx-hash closure returns `Some` only for extrinsic 7 (not present), so extrinsic 5 is
		// treated as non-eth.
		let (reverts, logs, outside_frame) = extract_revive_events(
			&events,
			0,
			U256::zero(),
			H256::zero(),
			|idx| (idx == 7).then_some(H256::zero()),
			synthetic_hash,
			3,
		);

		assert!(reverts.is_empty());
		assert!(logs.is_empty());
		assert_eq!(outside_frame.len(), 2, "the hook-phase log and the non-eth extrinsic's log");
		for log in &outside_frame {
			assert_eq!(log.transaction_hash, synthetic_hash);
			assert_eq!(log.transaction_index, U256::from(3));
		}
	}

	#[test]
	fn extract_revive_events_buckets_hook_phase_logs_as_outside_frame() {
		// A mirror firing in `on_initialize` — on Asset Hub, the message queue servicing an inbound
		// XCM asset deposit — is committed into the synthetic transaction like any other
		// outside-of-frame log, so it must be served rather than dropped for lacking an extrinsic.
		let synthetic_hash = H256::from([0xEE; 32]);
		let events = EventsBuilder::new()
			.push_event(
				frame_system::Phase::Initialization,
				pallet_revive::Event::ContractEmitted {
					contract: H160::from([0xaa; 20]),
					data: vec![],
					topics: vec![],
				},
			)
			.push_event(
				frame_system::Phase::Finalization,
				pallet_revive::Event::ContractEmitted {
					contract: H160::from([0xbb; 20]),
					data: vec![],
					topics: vec![],
				},
			)
			.build();

		let (reverts, logs, outside_frame) = extract_revive_events(
			&events,
			0,
			U256::zero(),
			H256::zero(),
			// Every extrinsic index is an ethereum transaction, so only the phase can exclude
			// these.
			|_| Some(H256::from([0x77; 32])),
			synthetic_hash,
			4,
		);

		assert!(reverts.is_empty());
		assert!(logs.is_empty(), "a hook-phase log belongs to no extrinsic");
		assert_eq!(outside_frame.len(), 2, "both hook phases are bucketed");
		for log in &outside_frame {
			assert_eq!(log.transaction_hash, synthetic_hash);
			assert_eq!(log.transaction_index, U256::from(4));
		}
	}

	#[test]
	fn extract_revive_events_accumulates_per_extrinsic() {
		let tx0 = H256::from([0x01; 32]);
		let tx1 = H256::from([0x02; 32]);
		let tx2 = H256::from([0x03; 32]);
		let emitted_by = |contract: H160| pallet_revive::Event::ContractEmitted {
			contract,
			data: vec![],
			topics: vec![],
		};
		let events = EventsBuilder::new()
			.push_event(frame_system::Phase::ApplyExtrinsic(0), emitted_by(H160::from([0xaa; 20])))
			.push_event(frame_system::Phase::ApplyExtrinsic(0), emitted_by(H160::from([0xbb; 20])))
			.push_event(
				frame_system::Phase::ApplyExtrinsic(1),
				pallet_revive::Event::EthExtrinsicRevert {
					dispatch_error: sp_runtime::DispatchError::Other("tx-1 revert"),
				},
			)
			.push_event(frame_system::Phase::ApplyExtrinsic(2), emitted_by(H160::from([0xcc; 20])))
			.build();

		let (reverts, logs, outside_frame) = extract_revive_events(
			&events,
			0,
			U256::zero(),
			H256::zero(),
			|idx| match idx {
				0 => Some(tx0),
				1 => Some(tx1),
				2 => Some(tx2),
				_ => None,
			},
			H256::zero(),
			3,
		);

		assert!(outside_frame.is_empty());
		assert_eq!(reverts, [1usize].into_iter().collect::<HashSet<_>>());
		assert_eq!(logs[&0].len(), 2);
		assert_eq!(logs[&2].len(), 1);
		// log_index is block-wide
		assert_eq!(logs[&0][0].log_index, U256::from(0));
		assert_eq!(logs[&0][1].log_index, U256::from(1));
		assert_eq!(logs[&2][0].log_index, U256::from(3));
	}

	const ETH_TRANSACT_PAYLOAD: [u8; 4] = [0xde, 0xad, 0xbe, 0xef];

	/// SCALE-encode a bare extrinsic the way a block body carries it, length prefix included.
	fn encode_bare(call: revive_dev_runtime::RuntimeCall) -> Vec<u8> {
		let extrinsic: revive_dev_runtime::UncheckedExtrinsic =
			pallet_revive::evm::runtime::UncheckedExtrinsic(
				sp_runtime::generic::UncheckedExtrinsic::new_bare(call),
			);
		extrinsic.encode()
	}

	fn eth_transact_extrinsic() -> Vec<u8> {
		encode_bare(revive_dev_runtime::RuntimeCall::Revive(pallet_revive::Call::eth_transact {
			payload: ETH_TRANSACT_PAYLOAD.to_vec(),
		}))
	}

	fn non_revive_extrinsic() -> Vec<u8> {
		encode_bare(revive_dev_runtime::RuntimeCall::System(frame_system::Call::remark {
			remark: vec![0x01],
		}))
	}

	/// Run the extraction over a synthetic block body.
	async fn extract_from(blobs: Vec<Vec<u8>>) -> Result<Vec<(EthTransact, usize)>, ClientError> {
		const BLOCK_NUMBER: SubstrateBlockNumber = 42;

		let client = offline_client();
		let at_block = client
			.at_block(BLOCK_NUMBER)
			.expect("spec version range covers every block number; qed");
		let extrinsics = at_block.extrinsics().from_bytes(blobs).await;
		extract_eth_transacts(&extrinsics, BLOCK_NUMBER)
	}

	#[tokio::test]
	async fn extract_eth_transacts_collects_revive_calls() {
		let calls = extract_from(vec![non_revive_extrinsic(), eth_transact_extrinsic()])
			.await
			.unwrap();

		assert_eq!(calls.len(), 1, "only the revive extrinsic is collected");
		assert_eq!(calls[0].1, 1, "the extrinsic index is preserved");
		assert_eq!(calls[0].0.payload, ETH_TRANSACT_PAYLOAD, "the call fields are decoded");
	}

	#[tokio::test]
	async fn extract_eth_transacts_keeps_revive_calls_next_to_an_undecodable_one() {
		let calls = extract_from(vec![vec![0xff; 4], eth_transact_extrinsic()]).await.unwrap();

		assert_eq!(calls.len(), 1, "an undecodable extrinsic must not hide a decoded one");
		assert_eq!(calls[0].1, 1, "the extrinsic index is preserved");
		assert_eq!(calls[0].0.payload, ETH_TRANSACT_PAYLOAD, "the call fields are decoded");
	}

	/// `n` distinguishable gas entries, so a mispairing shows up as a wrong value.
	fn gas_infos(n: usize) -> Vec<ReceiptGasInfoV1> {
		(0..n)
			.map(|i| ReceiptGasInfoV1 {
				gas_used: U256::from(i),
				effective_gas_price: U256::from(1_000_000_000u64),
			})
			.collect()
	}

	#[test]
	fn receipt_data_is_reconciled_against_the_block_body() {
		// The synthetic transaction is reported separately, so a block carrying one still has
		// exactly one entry per ethereum transaction here.
		check_receipt_data_len(&gas_infos(2), 2).unwrap();

		// An extrinsic the metadata could not decode leaves an entry with no transaction to pair
		// it with. Accepting it would pair every later transaction with the preceding one's gas
		// info.
		let err = check_receipt_data_len(&gas_infos(3), 2).unwrap_err();
		assert!(matches!(err, ClientError::ReceiptDataLengthMismatch));
	}

	fn outside_frame_logs(n: usize) -> Vec<Log> {
		(0..n)
			.map(|i| Log { address: H160::from_low_u64_be(i as u64), ..Default::default() })
			.collect()
	}

	#[test]
	fn outside_frame_logs_are_served_as_emitted_when_they_all_fitted() {
		let logs = reconcile_outside_frame_logs(outside_frame_logs(3), 3, 1);

		assert_eq!(logs, outside_frame_logs(3));
	}

	#[test]
	fn outside_frame_logs_past_what_the_block_committed_are_not_served() {
		// The buffer fills in emission order and drops what arrives after, so the committed logs
		// are the leading ones. Serving the rest would hand out logs absent from the block's
		// `logs_bloom` and `receipts_root`.
		let logs = reconcile_outside_frame_logs(outside_frame_logs(5), 2, 1);

		assert_eq!(logs, outside_frame_logs(2));
	}

	#[test]
	fn fewer_outside_frame_logs_than_committed_are_served_as_decoded() {
		// Nothing to cut: the events decoded into less than the block committed, which the warning
		// reports and truncation cannot repair.
		let logs = reconcile_outside_frame_logs(outside_frame_logs(1), 4, 1);

		assert_eq!(logs, outside_frame_logs(1));
	}
}
