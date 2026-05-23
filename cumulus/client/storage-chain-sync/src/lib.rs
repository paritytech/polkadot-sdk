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

use codec::Encode;
use sc_client_api::{BlockBackend, PrefetchedIndexedTransactions};
use sc_consensus::{
	BlockCheckParams, BlockImport, BlockImportParams, ImportResult, StateAction,
	StorageChanges as ConsensusStorageChanges,
};
use sc_network::bitswap::RAW_CODEC;
use sp_api::{ApiExt, CallApiAt, CallContext, Core, ProofRecorder, ProvideRuntimeApi, TransactionOutcome};
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockOrigin, Error as ConsensusError};
use sp_core::storage::ChildInfo;

use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_trie::proof_size_extension::ProofSizeExt;
use sp_state_machine::{IndexOperation, OverlayedChanges, StorageChanges};
use sp_transaction_storage_proof::{
	runtime_api::TransactionStorageApi, ContentHash, HashingAlgorithm, IndexedTransactionInfo,
};

use std::{collections::HashSet, marker::PhantomData, sync::Arc};

const LOG_TARGET: &str = "storage-chain-block-import";

/// Block-import wrapper that bitswap-fetches missing TRANSACTION-column entries
/// for tip-sync blocks before delegating to the inner block import.
pub struct StorageChainBlockImport<Block: BlockT, Inner, Client> {
	inner: Inner,
	client: Arc<Client>,
	fetcher: IndexedTransactionFetcher<Block>,
	/// Test-only flag: when `true`, `should_intercept` admits `BlockOrigin::GapSync`
	/// blocks and the wrapper exercises its gap-sync dispatch path. In production
	/// builds this field exists but is always read as `false` (see
	/// `intercept_gap_sync_enabled`).
	intercept_gap_sync: bool,
	_phantom: PhantomData<Block>,
}

impl<Block: BlockT, Inner: Clone, Client> Clone for StorageChainBlockImport<Block, Inner, Client> {
	fn clone(&self) -> Self {
		Self {
			inner: self.inner.clone(),
			client: self.client.clone(),
			fetcher: self.fetcher.clone(),
			intercept_gap_sync: self.intercept_gap_sync,
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
		Self {
			inner,
			client,
			fetcher,
			intercept_gap_sync: false,
			_phantom: PhantomData,
		}
	}

	/// Test-only: bypass the production origin filter so the wrapper intercepts
	/// `BlockOrigin::GapSync` blocks. Used by gap-sync test cases while sync-layer
	/// body fetching inside the pruning window is still pending. Has no effect on
	/// production builds (which never reach this method).
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn intercept_gap_sync_for_test(&mut self) {
		self.intercept_gap_sync = true;
	}
}

#[async_trait::async_trait]
impl<Block, Inner, Client> BlockImport<Block> for StorageChainBlockImport<Block, Inner, Client>
where
	Block: BlockT<Hash = sc_client_db::DbHash>,
	Inner: BlockImport<Block, Error = ConsensusError> + Send + Sync,
	Client: ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ HeaderBackend<Block>
		+ CallApiAt<Block>
		+ Send
		+ Sync,
	Client::Api: TransactionStorageApi<Block> + Core<Block> + ApiExt<Block>,
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

		if matches!(params.origin, BlockOrigin::GapSync) {
			return self.import_gap_sync_block(params).await;
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
	Inner: BlockImport<Block, Error = ConsensusError> + Send + Sync,
	Client: ProvideRuntimeApi<Block>
		+ BlockBackend<Block>
		+ HeaderBackend<Block>
		+ CallApiAt<Block>
		+ Send
		+ Sync,
	Client::Api: TransactionStorageApi<Block> + Core<Block> + ApiExt<Block>,
{
	/// True iff the block needs bitswap prefetch (tip-only by default; gap-sync only
	/// under the test-helpers override). Body must be present and the runtime must
	/// expose `TransactionStorageApi v2+`.
	fn should_intercept(&self, params: &BlockImportParams<Block>) -> bool {
		if params.body.is_none() {
			return false;
		}
		match params.origin {
			BlockOrigin::NetworkInitialSync |
			BlockOrigin::NetworkBroadcast |
			BlockOrigin::ConsensusBroadcast |
			BlockOrigin::Own => {},
			BlockOrigin::Genesis | BlockOrigin::File | BlockOrigin::WarpSync => return false,
			BlockOrigin::GapSync =>
				if !self.intercept_gap_sync_enabled() {
					return false;
				},
		}
		let parent_hash = *params.header.parent_hash();
		self.client
			.runtime_api()
			.has_api_with::<dyn TransactionStorageApi<Block>, _>(parent_hash, |v| v >= 2)
			.unwrap_or(false)
	}

	/// Reads the test-only `intercept_gap_sync` flag. In production builds this is
	/// hard-wired to `false`; the field itself only carries meaning under
	/// `cfg(any(test, feature = "test-helpers"))`.
	#[cfg(any(test, feature = "test-helpers"))]
	fn intercept_gap_sync_enabled(&self) -> bool {
		self.intercept_gap_sync
	}

	#[cfg(not(any(test, feature = "test-helpers")))]
	fn intercept_gap_sync_enabled(&self) -> bool {
		false
	}

	/// Discovers renew hashes via Case A (incoming changes) or Case B (execute once).
	///
	/// Gap-sync (formerly "Case C") has moved to its own dispatch path
	/// `import_gap_sync_block`, which uses the synthetic-ops carrier rather than
	/// returning a renew-set.
	///
	/// `&mut params` because Case B reassigns `params.state_action` to the executed
	/// `StorageChanges` so the inner block-import skips re-execution.
	fn classify_renew_hashes(
		&self,
		params: &mut BlockImportParams<Block>,
	) -> Result<HashSet<(ContentHash, HashingAlgorithm)>, ConsensusError> {
		let parent_hash = *params.header.parent_hash();
		let block_number = *params.header.number();

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

	/// Gap-sync dispatch: queries `TransactionStorageApi::indexed_transactions` at the
	/// latest finalized state (post-warp), classifies body extrinsics into synthetic
	/// `IndexOperation::Insert`/`Renew` ops via tail-hashing, bitswap-fetches missing
	/// renew payloads, and attaches both `ops` and `renew_payloads` to the new
	/// `PrefetchedIndexedTransactions` carrier so the backend can populate the
	/// `TRANSACTION` column without runtime execution.
	///
	/// Production gating: this is reachable only via `intercept_gap_sync_for_test`
	/// (see `should_intercept`); `BlockOrigin::GapSync` is otherwise filtered out.
	async fn import_gap_sync_block(
		&self,
		mut params: BlockImportParams<Block>,
	) -> Result<ImportResult, ConsensusError> {
		let parent_hash = *params.header.parent_hash();
		let block_number = *params.header.number();
		let finalized_hash = self.client.info().finalized_hash;

		let infos = self.indexed_transactions_at_finalized(block_number)?;
		let infos_len = infos.len();
		let body = params.body.as_ref().ok_or_else(|| {
			ConsensusError::Other(
				"StorageChainBlockImport: gap-sync body absent after gate".into(),
			)
		})?;
		let (synthetic_ops, renew_wants) = body_classify_to_ops::<Block>(&infos, body);

		let missing = self.filter_missing(renew_wants);
		let FetchedRenews { payload } = self.fetch_all(missing).await?;

		if !synthetic_ops.is_empty() || !payload.is_empty() {
			log::info!(
				target: LOG_TARGET,
				"gap-sync block #{block_number:?} ({parent_hash:?}, finalized={finalized_hash:?}): \
				 {infos_len} indexed entries, {} synthetic ops, {} renew payloads",
				synthetic_ops.len(),
				payload.len(),
			);
		}

		params.prefetched_indexed_transactions = PrefetchedIndexedTransactions {
			ops: synthetic_ops,
			renew_payloads: payload.into_iter().map(|(h, b)| (sp_core::H256::from(h), b)).collect(),
		};

		self.inner.import_block(params).await
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
	///
	/// The tip-block path (Case A/B) populates only `renew_payloads`; runtime execution
	/// produces the actual `IndexOperation::Renew` ops via `update_transaction_index`,
	/// so synthetic ops stay empty here.
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
		params.prefetched_indexed_transactions = PrefetchedIndexedTransactions {
			ops: Vec::new(),
			renew_payloads: fetched.into_iter().map(|(h, b)| (sp_core::H256::from(h), b)).collect(),
		};
	}

	/// Query `TransactionStorageApi::indexed_transactions` at the latest finalized
	/// state (post-warp). Used by the gap-sync dispatch to discover indexed metadata
	/// for a historical block whose `Transactions::<T>::insert(block_number, _)`
	/// was committed during that block's own `on_finalize` and is now visible at any
	/// finalized descendant's state (within the retention window).
	fn indexed_transactions_at_finalized(
		&self,
		block_number: sp_runtime::traits::NumberFor<Block>,
	) -> Result<Vec<IndexedTransactionInfo>, ConsensusError> {
		let finalized_hash = self.client.info().finalized_hash;
		self.client
			.runtime_api()
			.indexed_transactions(finalized_hash, block_number)
			.map_err(|e| ConsensusError::Other(format!("indexed_transactions: {e}").into()))
	}

	/// Query `TransactionStorageApi::indexed_transactions(block_number)` against the
	/// parent state plus supplied `StorageChanges`, so the runtime reads the
	/// post-execution state before the block is committed.
	fn indexed_transactions_with_storage_changes(
		&self,
		parent_hash: Block::Hash,
		block_number: sp_runtime::traits::NumberFor<Block>,
		changes: &StorageChanges<HashingFor<Block>>,
	) -> Result<Vec<IndexedTransactionInfo>, ConsensusError> {
		let mut api = self.client.runtime_api();
		api.set_overlayed_changes(overlay_from_storage_changes::<Block>(changes));
		api.indexed_transactions(parent_hash, block_number)
			.map_err(|e| ConsensusError::Other(format!("indexed_transactions: {e}").into()))
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

/// Classifies every fetchable `IndexedTransactionInfo` entry against the block body.
///
/// For each `info` whose tail bytes (`body[info.extrinsic_index][len - info.size..]`)
/// hash to `info.content_hash` under the declared algorithm, emits an
/// `IndexOperation::Insert` (data is local to the body, no fetch needed). For each
/// remaining fetchable entry, emits an `IndexOperation::Renew` and adds the
/// `(content_hash, hashing)` pair to the renew-fetch set.
///
/// Entries with `info.cid_codec != RAW_CODEC` or `info.extrinsic_index == u32::MAX`
/// are skipped entirely (not bitswap-fetchable; upstream `pallet-transaction-storage`
/// returns `u32::MAX`, only bulletin-chain-style pallets populate a real index).
///
/// Pure; no side effects. The fetch set is what the wrapper bitswap-fetches; the ops
/// vec is what the wrapper hands to the backend via
/// `PrefetchedIndexedTransactions.ops`.
fn body_classify_to_ops<Block: BlockT>(
	infos: &[IndexedTransactionInfo],
	body: &[Block::Extrinsic],
) -> (Vec<IndexOperation>, HashSet<(ContentHash, HashingAlgorithm)>) {
	let mut ops = Vec::new();
	let mut renew_wants = HashSet::new();
	let is_fetchable = |info: &IndexedTransactionInfo| {
		info.cid_codec == RAW_CODEC && info.extrinsic_index != u32::MAX
	};

	for info in infos.iter().filter(|info| is_fetchable(info)) {
		let extrinsic_index = info.extrinsic_index;
		let Some(ext) = body.get(extrinsic_index as usize) else { continue };
		let encoded = ext.encode();
		let size = info.size as usize;
		let matches_tail = encoded.len() >= size && {
			let tail = &encoded[encoded.len() - size..];
			info.hashing.hash(tail) == info.content_hash
		};
		if matches_tail {
			ops.push(IndexOperation::Insert {
				extrinsic: extrinsic_index,
				hash: info.content_hash.to_vec(),
				size: info.size,
			});
		} else {
			ops.push(IndexOperation::Renew {
				extrinsic: extrinsic_index,
				hash: info.content_hash.to_vec(),
			});
			renew_wants.insert((info.content_hash, info.hashing));
		}
	}

	(ops, renew_wants)
}

/// Returns runtime-declared renew (hash, hashing) pairs whose bytes are not in the body.
/// These entries must be fetched from elsewhere.
///
/// Filters out non-RAW codec entries (not bitswap-fetchable). Pure; no side effects.
/// Thin wrapper around `body_classify_to_ops` that discards the ops vec. Currently used
/// only by the regression test that verifies the delegate; kept available behind
/// `#[cfg(test)]` so that delegation remains a tested invariant if a future caller
/// needs the renew-only shape again.
#[cfg(test)]
fn body_classify_renews<Block: BlockT>(
	infos: &[IndexedTransactionInfo],
	body: &[Block::Extrinsic],
) -> HashSet<(ContentHash, HashingAlgorithm)> {
	body_classify_to_ops::<Block>(infos, body).1
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

	// W12: tests for `body_classify_to_ops` covering both halves of the split
	// (synthetic ops + renew-fetch set).

	#[test]
	fn body_classify_to_ops_pure_stores_only_emits_inserts() {
		let body = vec![extrinsic(&[1, 2, 3]), extrinsic(&[4, 5, 6]), extrinsic(&[7, 8, 9])];
		let infos = vec![
			body_info(&body[0], 0, HashingAlgorithm::Blake2b256, RAW_CODEC),
			body_info(&body[1], 1, HashingAlgorithm::Sha2_256, RAW_CODEC),
			body_info(&body[2], 2, HashingAlgorithm::Keccak256, RAW_CODEC),
		];

		let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

		assert_eq!(ops.len(), 3, "every entry must produce an op");
		for op in &ops {
			assert!(matches!(op, IndexOperation::Insert { .. }), "all stores expected");
		}
		assert!(renew_wants.is_empty(), "no fetches required for pure stores");
	}

	#[test]
	fn body_classify_to_ops_pure_renews_only_emits_renews_and_fetch_set() {
		let body = vec![extrinsic(&[1]), extrinsic(&[2]), extrinsic(&[3])];
		let infos = vec![
			info([0xA1; 32], body[0].encode().len() as u32, HashingAlgorithm::Blake2b256, RAW_CODEC, 0),
			info([0xB2; 32], body[1].encode().len() as u32, HashingAlgorithm::Sha2_256, RAW_CODEC, 1),
			info([0xC3; 32], body[2].encode().len() as u32, HashingAlgorithm::Keccak256, RAW_CODEC, 2),
		];

		let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

		assert_eq!(ops.len(), 3, "every entry must produce an op");
		for op in &ops {
			assert!(matches!(op, IndexOperation::Renew { .. }), "all renews expected");
		}
		assert_eq!(
			renew_wants,
			HashSet::from([
				([0xA1; 32], HashingAlgorithm::Blake2b256),
				([0xB2; 32], HashingAlgorithm::Sha2_256),
				([0xC3; 32], HashingAlgorithm::Keccak256),
			]),
		);
	}

	#[test]
	fn body_classify_to_ops_mixed_store_renew_emits_both() {
		let body = vec![
			extrinsic(&[10]),
			extrinsic(&[20]),
			extrinsic(&[30]),
			extrinsic(&[40]),
		];
		let infos = vec![
			body_info(&body[0], 0, HashingAlgorithm::Blake2b256, RAW_CODEC), // store
			info([0xAB; 32], body[1].encode().len() as u32, HashingAlgorithm::Sha2_256, RAW_CODEC, 1), // renew
			body_info(&body[2], 2, HashingAlgorithm::Keccak256, RAW_CODEC), // store
			info([0xCD; 32], body[3].encode().len() as u32, HashingAlgorithm::Blake2b256, RAW_CODEC, 3), // renew
		];

		let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

		assert_eq!(ops.len(), 4);
		// Extrinsic-index order matches input order (the loop is sequential).
		assert!(matches!(ops[0], IndexOperation::Insert { extrinsic: 0, .. }));
		assert!(matches!(ops[1], IndexOperation::Renew { extrinsic: 1, .. }));
		assert!(matches!(ops[2], IndexOperation::Insert { extrinsic: 2, .. }));
		assert!(matches!(ops[3], IndexOperation::Renew { extrinsic: 3, .. }));
		assert_eq!(
			renew_wants,
			HashSet::from([
				([0xAB; 32], HashingAlgorithm::Sha2_256),
				([0xCD; 32], HashingAlgorithm::Blake2b256),
			]),
		);
	}

	#[test]
	fn body_classify_to_ops_per_hashing_dispatches_correctly() {
		for hashing in
			[HashingAlgorithm::Blake2b256, HashingAlgorithm::Sha2_256, HashingAlgorithm::Keccak256]
		{
			let body = vec![extrinsic(&[0xFE])];
			let infos = vec![body_info(&body[0], 0, hashing, RAW_CODEC)];

			let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

			assert_eq!(ops.len(), 1, "{hashing:?}: one op expected");
			assert!(
				matches!(ops[0], IndexOperation::Insert { .. }),
				"{hashing:?}: matching tail must classify as store",
			);
			assert!(renew_wants.is_empty(), "{hashing:?}: no fetch needed");
		}
	}

	#[test]
	fn body_classify_to_ops_oversized_tail_classifies_as_renew() {
		let body = vec![extrinsic(&[0xAA])];
		let oversized = body[0].encode().len() as u32 + 1;
		let infos = vec![info([0x77; 32], oversized, HashingAlgorithm::Blake2b256, RAW_CODEC, 0)];

		let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

		assert_eq!(ops.len(), 1);
		assert!(matches!(ops[0], IndexOperation::Renew { .. }));
		assert_eq!(renew_wants, HashSet::from([([0x77; 32], HashingAlgorithm::Blake2b256)]));
	}

	#[test]
	fn body_classify_to_ops_skips_u32_max_extrinsic_index() {
		let body = vec![extrinsic(&[0xBB])];
		let encoded_len = body[0].encode().len() as u32;
		let infos = vec![info([0x55; 32], encoded_len, HashingAlgorithm::Blake2b256, RAW_CODEC, u32::MAX)];

		let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

		assert!(ops.is_empty(), "u32::MAX extrinsic_index must be skipped");
		assert!(renew_wants.is_empty(), "u32::MAX extrinsic_index must not request a fetch");
	}

	#[test]
	fn body_classify_to_ops_skips_non_raw_codec() {
		let body = vec![extrinsic(&[0xCC])];
		let encoded_len = body[0].encode().len() as u32;
		let infos = vec![info([0x33; 32], encoded_len, HashingAlgorithm::Blake2b256, 0x70, 0)];

		let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

		assert!(ops.is_empty(), "non-RAW codec must be skipped");
		assert!(renew_wants.is_empty(), "non-RAW codec must not request a fetch");
	}

	#[test]
	fn body_classify_to_ops_skips_extrinsic_index_out_of_range() {
		let body = vec![extrinsic(&[0xDD])];
		let encoded_len = body[0].encode().len() as u32;
		let infos = vec![info([0x22; 32], encoded_len, HashingAlgorithm::Blake2b256, RAW_CODEC, 99)];

		let (ops, renew_wants) = body_classify_to_ops::<Block>(&infos, &body);

		assert!(ops.is_empty(), "out-of-range extrinsic_index must be skipped");
		assert!(renew_wants.is_empty(), "out-of-range extrinsic_index must not request a fetch");
	}

	#[test]
	fn body_classify_renews_delegates_to_to_ops() {
		// Regression guard: old `body_classify_renews` must continue to return only the
		// renew-set half of the classifier.
		let body = vec![extrinsic(&[1]), extrinsic(&[2])];
		let infos = vec![
			body_info(&body[0], 0, HashingAlgorithm::Blake2b256, RAW_CODEC), // store
			info([0xEE; 32], body[1].encode().len() as u32, HashingAlgorithm::Sha2_256, RAW_CODEC, 1), // renew
		];

		let renews = body_classify_renews::<Block>(&infos, &body);
		let (_, expected_renews) = body_classify_to_ops::<Block>(&infos, &body);

		assert_eq!(renews, expected_renews);
		assert_eq!(renews, HashSet::from([([0xEE; 32], HashingAlgorithm::Sha2_256)]));
	}
}
