// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! BlockImport wrapper that bitswap-fetches missing TRANSACTION-column entries before
//! delegating to the inner block import. Tip-sync only; gap-sync and warp-sync pass through.
//!
//! See `StorageChainBlockImport::classify_renew_hashes` for the Case A / B / C discovery
//! dispatch.

mod fetcher;

pub(crate) use fetcher::FetchError;
pub use fetcher::{BitswapPeerSource, IndexedTransactionFetcher, NetworkHandle, SyncingHandle};

use codec::{Decode, Encode};
use sc_client_api::BlockBackend;
use sc_consensus::{
	BlockCheckParams, BlockImport, BlockImportParams, ImportResult, StateAction,
	StorageChanges as ConsensusStorageChanges,
};
use sc_network::bitswap::RAW_CODEC;
use sp_api::{
	ApiExt, CallApiAt, CallApiAtParams, CallContext, Core, ProofRecorder, ProvideRuntimeApi,
	TransactionOutcome,
};
use sp_consensus::{BlockOrigin, Error as ConsensusError};
use sp_core::storage::ChildInfo;
use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_state_machine::{IndexOperation, OverlayedChanges, StorageChanges};
use sp_transaction_storage_proof::{
	runtime_api::TransactionStorageApi, ContentHash, HashingAlgorithm, IndexedTransactionInfo,
};
use sp_trie::proof_size_extension::ProofSizeExt;
use std::{cell::RefCell, collections::HashSet, marker::PhantomData, sync::Arc};

const LOG_TARGET: &str = "storage-chain-block-import";
const INDEXED_TRANSACTIONS_API: &str = "TransactionStorageApi_indexed_transactions";

/// Block-import wrapper that bitswap-fetches missing TRANSACTION-column entries
/// for tip-sync blocks before delegating to the inner block import.
pub struct StorageChainBlockImport<Block: BlockT, Inner, Client> {
	inner: Inner,
	client: Arc<Client>,
	fetcher: IndexedTransactionFetcher<Block>,
	_phantom: PhantomData<Block>,
}

impl<Block: BlockT, Inner: Clone, Client> Clone for StorageChainBlockImport<Block, Inner, Client> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			client: self.client.clone(),
			fetcher: self.fetcher.clone(),
			_phantom: PhantomData,
		}
	}
}

impl<Block: BlockT, Inner, Client> StorageChainBlockImport<Block, Inner, Client> {
	pub fn new(
		inner: Inner,
		client: Arc<Client>,
		fetcher: IndexedTransactionFetcher<Block>,
	) -> Self {
		Self { inner, client, fetcher, _phantom: PhantomData }
	}
}

#[async_trait::async_trait]
impl<Block, Inner, Client> BlockImport<Block> for StorageChainBlockImport<Block, Inner, Client>
where
	Block: BlockT<Hash = sc_client_db::DbHash>,
	Inner: BlockImport<Block, Error = ConsensusError> + Send + Sync,
	Client: ProvideRuntimeApi<Block> + CallApiAt<Block> + BlockBackend<Block> + Send + Sync,
	Client::Api: TransactionStorageApi<Block> + Core<Block>,
{
	type Error = ConsensusError;

	async fn check_block(
		&self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block).await
	}

	async fn import_block(
		&self,
		mut params: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		if !self.should_intercept(&params) {
			return self.inner.import_block(params).await;
		}

		let renews = self.classify_renew_hashes(&mut params)?;
		let missing = self.filter_missing(renews);
		let FetchedRenews { payload } = self.fetch_all(missing).await?;
		Self::attach_prefetched(&mut params, payload);

		let result = self.inner.import_block(params).await?;

		Ok(result)
	}
}

impl<Block, Inner, Client> StorageChainBlockImport<Block, Inner, Client>
where
	Block: BlockT<Hash = sc_client_db::DbHash>,
	Client: ProvideRuntimeApi<Block> + CallApiAt<Block> + BlockBackend<Block> + Send + Sync,
	Client::Api: TransactionStorageApi<Block> + Core<Block>,
{
	/// True iff the block needs bitswap prefetch (tip-only, body present, runtime API v2+).
	fn should_intercept(&self, params: &BlockImportParams<Block>) -> bool {
		if params.body.is_none() {
			return false;
		}
		match params.origin {
			BlockOrigin::NetworkInitialSync |
			BlockOrigin::NetworkBroadcast |
			BlockOrigin::ConsensusBroadcast |
			BlockOrigin::Own => {},
			BlockOrigin::Genesis |
			BlockOrigin::File |
			BlockOrigin::WarpSync |
			BlockOrigin::GapSync => return false,
		}
		let parent_hash = *params.header.parent_hash();
		self.client
			.runtime_api()
			.has_api_with::<dyn TransactionStorageApi<Block>, _>(parent_hash, |v| v >= 2)
			.unwrap_or(false)
	}

	/// Discovers renew hashes via Case A (incoming changes), B (execute once), or C (runtime API).
	///
	/// `&mut params` because Case B reassigns `params.state_action` to the executed
	/// `StorageChanges` so the inner block-import skips re-execution.
	fn classify_renew_hashes(
		&self,
		params: &mut BlockImportParams<Block>,
	) -> Result<HashSet<(ContentHash, HashingAlgorithm)>, ConsensusError> {
		let parent_hash = *params.header.parent_hash();
		let block_number = *params.header.number();
		let body = params.body.as_ref().ok_or_else(|| {
			ConsensusError::Other("StorageChainBlockImport: body absent after gate".into())
		})?;

		if let Some(changes) = params.state_action.as_storage_changes() {
			let infos =
				self.indexed_transactions_with_storage_changes(parent_hash, block_number, changes)?;
			let renews = verified_renews_from_index_ops(
				&changes.transaction_index_changes,
				&infos,
				"case A",
			)?;
			if !renews.is_empty() {
				log::debug!(
					target: LOG_TARGET,
					"block #{block_number:?} ({parent_hash:?}): case A runtime-API overlay, \
					 {} indexed entries, {} renew hashes",
					infos.len(),
					renews.len(),
				);
			}
			return Ok(renews);
		}

		if matches!(params.origin, BlockOrigin::GapSync) {
			let runtime_api = self.client.runtime_api();
			let infos =
				runtime_api.indexed_transactions(parent_hash, block_number).map_err(|e| {
					ConsensusError::Other(
						format!("indexed_transactions runtime API failed: {e}").into(),
					)
				})?;
			let renews = body_classify_renews::<Block>(&infos, body);
			if !renews.is_empty() {
				log::debug!(
					target: LOG_TARGET,
					"block #{block_number:?} ({parent_hash:?}): case C runtime-API, \
					 {} indexed entries, {} renew hashes",
					infos.len(),
					renews.len(),
				);
			}
			return Ok(renews);
		}

		let (gen_storage_changes, infos) = self.execute_block(params)?;
		let renews = verified_renews_from_index_ops(
			&gen_storage_changes.transaction_index_changes,
			&infos,
			"case B",
		)?;
		if !renews.is_empty() {
			log::debug!(
				target: LOG_TARGET,
				"block #{block_number:?} ({parent_hash:?}): case B execute-once runtime-API, \
				 {} indexed entries, {} renew hashes",
				infos.len(),
				renews.len(),
			);
		}

		params.state_action =
			StateAction::ApplyChanges(ConsensusStorageChanges::Changes(gen_storage_changes));

		Ok(renews)
	}

	/// Drops every entry whose data is already in the local TRANSACTION column.
	fn filter_missing(
		&self,
		renews: HashSet<(ContentHash, HashingAlgorithm)>,
	) -> HashSet<(ContentHash, HashingAlgorithm)> {
		renews
			.into_iter()
			.filter(|(hash, _)| {
				!self.client.has_indexed_transaction((*hash).into()).unwrap_or(false)
			})
			.collect()
	}

	/// Bitswap-fetch every missing entry. Errors if any entry was not served.
	async fn fetch_all(
		&self,
		missing: HashSet<(ContentHash, HashingAlgorithm)>,
	) -> Result<FetchedRenews, ConsensusError> {
		if missing.is_empty() {
			return Ok(FetchedRenews::default());
		}

		let wants: Vec<(ContentHash, HashingAlgorithm)> = missing.into_iter().collect();
		let acquired = self.fetcher.fetch_many(&wants).await?;

		if acquired.len() != wants.len() {
			let missing_count = wants.len() - acquired.len();
			return Err(ConsensusError::Other(
				format!("bitswap fetch: {missing_count} of {} entries not served", wants.len())
					.into(),
			));
		}

		let payload: Vec<(ContentHash, Vec<u8>)> = wants
			.iter()
			.map(|(hash, _)| {
				let data = acquired
					.get(hash)
					.expect("all hashes present; len equality verified above; qed")
					.clone();
				(*hash, data)
			})
			.collect();

		Ok(FetchedRenews { payload })
	}

	/// Attach prefetched `(content_hash, bytes)` pairs to
	/// [`BlockImportParams::prefetched_indexed_transactions`] for the backend writer.
	fn attach_prefetched(
		params: &mut BlockImportParams<Block>,
		fetched: Vec<(ContentHash, Vec<u8>)>,
	) {
		if fetched.is_empty() {
			return;
		}
		for (hash, _) in &fetched {
			log::info!(
				target: LOG_TARGET,
				"attaching bitswap-fetched indexed transaction {hash:?} to BlockImportParams",
			);
		}
		params.prefetched_indexed_transactions = fetched;
	}

	/// Query TransactionStorageApi against parent state plus supplied StorageChanges.
	fn indexed_transactions_with_storage_changes(
		&self,
		parent_hash: Block::Hash,
		block_number: sp_runtime::traits::NumberFor<Block>,
		changes: &StorageChanges<HashingFor<Block>>,
	) -> Result<Vec<IndexedTransactionInfo>, ConsensusError> {
		let has_api = self
			.client
			.runtime_api()
			.has_api_with::<dyn TransactionStorageApi<Block>, _>(parent_hash, |v| v >= 2)
			.unwrap_or(false);
		if !has_api {
			return Ok(Vec::new());
		}

		let overlayed_changes = RefCell::new(overlay_from_storage_changes::<Block>(changes));
		let recorder = None;
		let mut extensions = sp_externalities::Extensions::new();
		self.client.initialize_extensions(parent_hash, &mut extensions).map_err(|e| {
			ConsensusError::Other(
				format!("indexed_transactions: initialize_extensions: {e}").into(),
			)
		})?;
		let extensions = RefCell::new(extensions);

		let encoded = (block_number,).encode();
		overlayed_changes.borrow_mut().start_transaction();
		let raw = self.client.call_api_at(CallApiAtParams {
			at: parent_hash,
			function: INDEXED_TRANSACTIONS_API,
			arguments: encoded,
			overlayed_changes: &overlayed_changes,
			call_context: CallContext::Onchain { import: true },
			recorder: &recorder,
			extensions: &extensions,
		});

		overlayed_changes
			.borrow_mut()
			.rollback_transaction()
			.expect("transaction was opened immediately above; qed");

		let raw = raw.map_err(|e| {
			ConsensusError::Other(format!("indexed_transactions: call_api_at: {e}").into())
		})?;

		Vec::<IndexedTransactionInfo>::decode(&mut &raw[..]).map_err(|e| {
			ConsensusError::Other(format!("indexed_transactions: decode result: {e}").into())
		})
	}

	/// Execute via runtime API once, query indexed metadata on the same `ApiRef`, and obtain
	/// `StorageChanges` (Case B). Caller must reassign `params.state_action` before forwarding.
	fn execute_block(
		&self,
		params: &BlockImportParams<Block>,
	) -> Result<(StorageChanges<HashingFor<Block>>, Vec<IndexedTransactionInfo>), ConsensusError> {
		let parent_hash = *params.header.parent_hash();
		let body = params.body.clone().unwrap_or_default();
		let block = Block::new(params.header.clone(), body);

		let recorder = ProofRecorder::<Block>::default();

		let mut runtime_api = self.client.runtime_api();
		runtime_api.set_call_context(CallContext::Onchain { import: true });
		runtime_api.record_proof_with_recorder(recorder.clone());
		runtime_api.register_extension(ProofSizeExt::new(recorder));

		runtime_api.execute_block(parent_hash, block.into()).map_err(|e| {
			ConsensusError::Other(format!("execute_block: runtime_api.execute_block: {e}").into())
		})?;

		let infos = runtime_api
			.execute_in_transaction(|api| {
				TransactionOutcome::Rollback(
					api.indexed_transactions(parent_hash, *params.header.number()),
				)
			})
			.map_err(|e| {
				ConsensusError::Other(
					format!("execute_block: indexed_transactions runtime API failed: {e}").into(),
				)
			})?;

		let state = self.client.state_at(parent_hash).map_err(|e| {
			ConsensusError::Other(format!("execute_block: state_at({parent_hash:?}): {e}").into())
		})?;

		let gen_storage_changes =
			runtime_api.into_storage_changes(&state, parent_hash).map_err(|e| {
				ConsensusError::Other(format!("execute_block: into_storage_changes: {e}").into())
			})?;

		if params.header.state_root() != &gen_storage_changes.transaction_storage_root {
			return Err(ConsensusError::Other(
				format!(
					"execute_block: state root mismatch: header={:?}, executed={:?}",
					params.header.state_root(),
					gen_storage_changes.transaction_storage_root,
				)
				.into(),
			));
		}

		Ok((gen_storage_changes, infos))
	}
}

/// Returns runtime-verified renew pairs for host-call renew operations.
fn verified_renews_from_index_ops(
	ops: &[IndexOperation],
	infos: &[IndexedTransactionInfo],
	context: &'static str,
) -> Result<HashSet<(ContentHash, HashingAlgorithm)>, ConsensusError> {
	let mut renews = HashSet::new();
	for op in ops {
		let IndexOperation::Renew { hash, .. } = op else { continue };
		let hash: ContentHash = hash.as_slice().try_into().map_err(|_| {
			ConsensusError::Other(format!("{context}: malformed renew content hash").into())
		})?;
		let info = infos.iter().find(|info| info.content_hash == hash).ok_or_else(|| {
			ConsensusError::Other(
				format!("{context}: runtime API missing metadata for renew hash {hash:?}").into(),
			)
		})?;
		if info.cid_codec == RAW_CODEC {
			renews.insert((hash, info.hashing));
		}
	}
	Ok(renews)
}

fn overlay_from_storage_changes<Block: BlockT>(
	changes: &StorageChanges<HashingFor<Block>>,
) -> OverlayedChanges<HashingFor<Block>> {
	let mut overlay = OverlayedChanges::default();
	for (key, value) in &changes.main_storage_changes {
		overlay.set_storage(key.clone(), value.clone());
	}
	for (storage_key, changes) in &changes.child_storage_changes {
		let child_info = ChildInfo::new_default(storage_key);
		for (key, value) in changes {
			overlay.set_child_storage(&child_info, key.clone(), value.clone());
		}
	}
	overlay
}

/// Result of [`StorageChainBlockImport::fetch_all`]: the fetched payload bytes.
#[derive(Default)]
struct FetchedRenews {
	payload: Vec<(ContentHash, Vec<u8>)>,
}

/// Returns runtime-declared renew (hash, hashing) pairs whose bytes are not in the body.
/// These entries must be fetched from elsewhere.
///
/// Filters out non-RAW codec entries (not bitswap-fetchable). Pure; no side effects.
fn body_classify_renews<Block: BlockT>(
	infos: &[IndexedTransactionInfo],
	body: &[Block::Extrinsic],
) -> HashSet<(ContentHash, HashingAlgorithm)> {
	let mut renews = HashSet::new();
	let is_fetchable = |info: &IndexedTransactionInfo| {
		info.cid_codec == RAW_CODEC && info.extrinsic_index != u32::MAX
	};

	for info in infos.iter().filter(|info| is_fetchable(info)) {
		let Some(ext) = body.get(info.extrinsic_index as usize) else { continue };
		let encoded = ext.encode();
		let size = info.size as usize;
		if encoded.len() < size {
			renews.insert((info.content_hash, info.hashing));
			continue;
		}

		let tail = &encoded[encoded.len() - size..];
		if info.hashing.hash(tail) != info.content_hash {
			renews.insert((info.content_hash, info.hashing));
		}
	}

	renews
}

impl From<FetchError> for ConsensusError {
	fn from(e: FetchError) -> Self {
		ConsensusError::Other(format!("bitswap: {e}").into())
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::Encode;
	use sp_runtime::{generic, traits::BlakeTwo256, OpaqueExtrinsic};
	use std::collections::HashSet;

	type Block = generic::Block<generic::Header<u32, BlakeTwo256>, OpaqueExtrinsic>;

	fn info(
		content_hash: ContentHash,
		size: u32,
		alg: HashingAlgorithm,
		codec: u64,
		extrinsic_index: u32,
	) -> IndexedTransactionInfo {
		IndexedTransactionInfo {
			content_hash,
			size,
			hashing: alg,
			cid_codec: codec,
			extrinsic_index,
		}
	}

	fn extrinsic(bytes: &[u8]) -> OpaqueExtrinsic {
		OpaqueExtrinsic::from_blob(bytes.to_vec())
	}

	fn body_info(
		ext: &OpaqueExtrinsic,
		extrinsic_index: u32,
		hashing: HashingAlgorithm,
		codec: u64,
	) -> IndexedTransactionInfo {
		let encoded = ext.encode();
		info(hashing.hash(&encoded), encoded.len() as u32, hashing, codec, extrinsic_index)
	}

	#[test]
	fn body_classify_renews_returns_empty_for_supported_insert() {
		let body = vec![extrinsic(&[1, 2, 3])];
		let infos = vec![body_info(&body[0], 0, HashingAlgorithm::Blake2b256, RAW_CODEC)];

		assert!(body_classify_renews::<Block>(&infos, &body).is_empty());
	}

	#[test]
	fn body_classify_renews_filters_unsupported_non_raw_codec() {
		let body = vec![extrinsic(&[4, 5, 6])];
		let infos = vec![info(
			[9; 32],
			body[0].encode().len() as u32,
			HashingAlgorithm::Blake2b256,
			0x70,
			0,
		)];

		assert!(body_classify_renews::<Block>(&infos, &body).is_empty());
	}

	#[test]
	fn body_classify_renews_returns_single_supported_renew() {
		let body = vec![extrinsic(&[7, 8, 9])];
		let infos = vec![info(
			[1; 32],
			body[0].encode().len() as u32,
			HashingAlgorithm::Sha2_256,
			RAW_CODEC,
			0,
		)];

		let renews = body_classify_renews::<Block>(&infos, &body);
		assert_eq!(renews, HashSet::from([([1; 32], HashingAlgorithm::Sha2_256)]));
	}

	#[test]
	fn body_classify_renews_flattens_multi_renews_at_same_index() {
		let body = vec![extrinsic(&[10, 11, 12])];
		let encoded_len = body[0].encode().len() as u32;
		let infos = vec![
			info([2; 32], encoded_len, HashingAlgorithm::Blake2b256, RAW_CODEC, 0),
			info([3; 32], encoded_len, HashingAlgorithm::Keccak256, RAW_CODEC, 0),
		];

		let renews = body_classify_renews::<Block>(&infos, &body);
		assert_eq!(
			renews,
			HashSet::from([
				([2; 32], HashingAlgorithm::Blake2b256),
				([3; 32], HashingAlgorithm::Keccak256),
			]),
		);
	}

	#[test]
	fn body_classify_renews_accepts_matching_tail_for_each_hashing() {
		for hashing in
			[HashingAlgorithm::Blake2b256, HashingAlgorithm::Sha2_256, HashingAlgorithm::Keccak256]
		{
			let body = vec![extrinsic(&[0x42])];
			let infos = vec![body_info(&body[0], 0, hashing, RAW_CODEC)];

			assert!(
				body_classify_renews::<Block>(&infos, &body).is_empty(),
				"{hashing:?} matching tail should be an insert",
			);
		}
	}

	#[test]
	fn body_classify_renews_treats_oversized_tail_as_renew() {
		let body = vec![extrinsic(&[0xaa])];
		let infos = vec![info(
			[4; 32],
			body[0].encode().len() as u32 + 1,
			HashingAlgorithm::Blake2b256,
			RAW_CODEC,
			0,
		)];

		let renews = body_classify_renews::<Block>(&infos, &body);

		assert_eq!(renews, HashSet::from([([4; 32], HashingAlgorithm::Blake2b256)]));
	}

	#[test]
	fn body_classify_renews_ignores_unknown_and_out_of_range_indexes() {
		let body = vec![extrinsic(&[0xaa])];
		let encoded_len = body[0].encode().len() as u32;
		let infos = vec![
			info([5; 32], encoded_len, HashingAlgorithm::Blake2b256, RAW_CODEC, u32::MAX),
			info([6; 32], encoded_len, HashingAlgorithm::Blake2b256, RAW_CODEC, 99),
		];

		assert!(body_classify_renews::<Block>(&infos, &body).is_empty());
	}

	#[test]
	fn verified_renews_from_index_ops_uses_metadata_hashing_with_unknown_extrinsic_index() {
		let hash = [7; 32];
		let ops = vec![IndexOperation::Renew { extrinsic: 0, hash: hash.to_vec() }];
		let infos = vec![info(hash, 32, HashingAlgorithm::Keccak256, RAW_CODEC, u32::MAX)];

		let renews = verified_renews_from_index_ops(&ops, &infos, "test").unwrap();

		assert_eq!(renews, HashSet::from([(hash, HashingAlgorithm::Keccak256)]));
	}

	#[test]
	fn indexed_transactions_after_execute_block_requires_runtime_metadata_for_renew() {
		let hash = [9; 32];
		let ops = vec![IndexOperation::Renew { extrinsic: 0, hash: hash.to_vec() }];
		let err = verified_renews_from_index_ops(&ops, &[], "case B").unwrap_err();
		let msg = format!("{err}");

		assert!(msg.contains("case B: runtime API missing metadata"), "unexpected: {msg}");
	}
}
