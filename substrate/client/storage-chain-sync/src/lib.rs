// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// This program is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// This program is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with this program. If not, see <https://www.gnu.org/licenses/>.

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
//! delegating to the inner block import. Tip-sync and body-carrying gap-sync only; warp-sync and
//! header-only gap-sync pass through.
//!
//! [`StorageChainBlockImport::import_block`] dispatches a tip-sync block to one of three paths
//! based on what the consensus engine has already done:
//!
//! * [`import_with_attached_changes`]: the import params already carry executed
//!   `StorageChanges`. The wrapper queries the runtime API against the post-execution overlay
//!   to discover the renew set.
//! * [`import_by_executing_block`]: no attached changes. The wrapper executes the block via
//!   the runtime API once, then installs the resulting `StorageChanges` into the import params
//!   so the inner block-import does not re-execute.
//! * [`import_gap_sync_block`]: body-carrying gap-sync. The renew set is derived from the body
//!   itself by tail-hashing against runtime-declared metadata, not via runtime execution.
//!
//! [`import_with_attached_changes`]: StorageChainBlockImport::import_with_attached_changes
//! [`import_by_executing_block`]: StorageChainBlockImport::import_by_executing_block
//! [`import_gap_sync_block`]: StorageChainBlockImport::import_gap_sync_block

mod fetcher;

pub(crate) use fetcher::FetchError;
pub use fetcher::{BitswapPeerSource, IndexedTransactionFetcher, NetworkHandle, SyncingHandle};

use codec::Encode;
use sc_client_api::{BlockBackend, PrefetchedIndexedTransactions};
use sc_consensus::{
	BlockCheckParams, BlockImport, BlockImportParams, ImportResult, StateAction,
	StorageChanges as ConsensusStorageChanges,
};
use sp_api::{
	ApiExt, CallApiAt, CallContext, Core, ProofRecorder, ProvideRuntimeApi, TransactionOutcome,
};
use sp_blockchain::HeaderBackend;
use sp_consensus::{BlockOrigin, Error as ConsensusError};
use sp_core::storage::ChildInfo;

use sp_runtime::traits::{Block as BlockT, HashingFor, Header as HeaderT};
use sp_state_machine::{IndexOperation, OverlayedChanges, StorageChanges};
use sp_transaction_storage_proof::{
	runtime_api::TransactionStorageApi, ContentHash, HashingAlgorithm, IndexedTransactionInfo,
};
use sp_trie::proof_size_extension::ProofSizeExt;

use std::{collections::HashSet, marker::PhantomData, sync::Arc};

const LOG_TARGET: &str = "storage-chain-block-import";

/// A runtime-declared indexed-transaction entry that needs to be bitswap-fetched.
///
/// Carries everything the fetcher needs to construct a CID and locate the bytes:
/// the content hash, the hashing algorithm the producer used, and the runtime-declared
/// CID codec. Codec is preserved (not coerced to RAW) so the on-wire CID matches what
/// the producing runtime announced.
#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) struct RenewWant {
	pub hash: ContentHash,
	pub hashing: HashingAlgorithm,
	pub cid_codec: u64,
}

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
		params: BlockImportParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		if !self.should_intercept(&params) {
			return self.inner.import_block(params).await;
		}

		if matches!(params.origin, BlockOrigin::GapSync) {
			return self.import_gap_sync_block(params).await;
		}

		if params.state_action.as_storage_changes().is_some() {
			self.import_with_attached_changes(params).await
		} else {
			self.import_by_executing_block(params).await
		}
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
	/// True iff the block needs bitswap prefetch. Body must be present and the runtime
	/// must expose `TransactionStorageApi v2+`.
	fn should_intercept(&self, params: &BlockImportParams<Block>) -> bool {
		if params.body.is_none() {
			return false;
		}
		match params.origin {
			BlockOrigin::NetworkInitialSync |
			BlockOrigin::NetworkBroadcast |
			BlockOrigin::ConsensusBroadcast |
			BlockOrigin::Own |
			BlockOrigin::GapSync => {},
			BlockOrigin::Genesis | BlockOrigin::File | BlockOrigin::WarpSync => return false,
		}
		let parent_hash = *params.header.parent_hash();
		self.client
			.runtime_api()
			.has_api_with::<dyn TransactionStorageApi<Block>, _>(parent_hash, |v| v >= 2)
			.unwrap_or(false)
	}

	/// Import path for blocks whose `BlockImportParams::state_action` already carries
	/// the executed `StorageChanges` (consensus executed the block before handing it to
	/// the import queue). Reads the renew set from the attached `transaction_index_changes`
	/// using the runtime API queried against the post-execution overlay.
	async fn import_with_attached_changes(
		&self,
		params: BlockImportParams<Block>,
	) -> Result<ImportResult, ConsensusError> {
		let parent_hash = *params.header.parent_hash();
		let block_number = *params.header.number();

		let changes = params
			.state_action
			.as_storage_changes()
			.expect("dispatcher gates on as_storage_changes().is_some(); qed");
		let infos =
			self.indexed_transactions_with_storage_changes(parent_hash, block_number, changes)?;
		let renews = verified_renews_from_index_ops(
			&changes.transaction_index_changes,
			&infos,
			"runtime-api overlay",
		)?;
		if !renews.is_empty() {
			log::debug!(
				target: LOG_TARGET,
				"block #{block_number:?} ({parent_hash:?}): runtime-API overlay path, \
				 {} indexed entries, {} renew hashes",
				infos.len(),
				renews.len(),
			);
		}

		self.apply_prefetch_and_forward(params, renews).await
	}

	/// Import path for blocks without attached `StorageChanges`. Executes the block via
	/// the runtime API to discover the renew set, then installs the resulting
	/// `StorageChanges` into `params.state_action` so the inner block-import does not
	/// re-execute.
	async fn import_by_executing_block(
		&self,
		mut params: BlockImportParams<Block>,
	) -> Result<ImportResult, ConsensusError> {
		let parent_hash = *params.header.parent_hash();
		let block_number = *params.header.number();

		let (gen_storage_changes, infos) = self.execute_block(&params)?;
		let renews = verified_renews_from_index_ops(
			&gen_storage_changes.transaction_index_changes,
			&infos,
			"block execution",
		)?;
		if !renews.is_empty() {
			log::debug!(
				target: LOG_TARGET,
				"block #{block_number:?} ({parent_hash:?}): block-execution path, \
				 {} indexed entries, {} renew hashes",
				infos.len(),
				renews.len(),
			);
		}

		params.state_action =
			StateAction::ApplyChanges(ConsensusStorageChanges::Changes(gen_storage_changes));

		self.apply_prefetch_and_forward(params, renews).await
	}

	/// Shared tail of the tip-block import paths: drop entries already on disk,
	/// bitswap-fetch the rest, attach them to `params`, and forward to the inner
	/// block-import. Used by both [`Self::import_with_attached_changes`] and
	/// [`Self::import_by_executing_block`]; gap-sync uses a different attach path.
	async fn apply_prefetch_and_forward(
		&self,
		mut params: BlockImportParams<Block>,
		renews: HashSet<RenewWant>,
	) -> Result<ImportResult, ConsensusError> {
		let missing = self.filter_missing(renews);
		let payload = self.fetch_all(missing).await?;
		Self::attach_prefetched(&mut params, payload);
		self.inner.import_block(params).await
	}

	/// Gap-sync dispatch: queries `TransactionStorageApi::indexed_transactions` at the
	/// latest finalized state (post-warp), classifies body extrinsics into synthetic
	/// `IndexOperation::Insert`/`Renew` ops via tail-hashing, bitswap-fetches missing
	/// renew payloads, and attaches both `ops` and `renew_payloads` to the new
	/// `PrefetchedIndexedTransactions` carrier so the backend can populate the
	/// `TRANSACTION` column without runtime execution.
	///
	/// Non-archive nodes normally receive header-only gap-sync blocks; those pass through
	/// because `should_intercept` requires a body.
	async fn import_gap_sync_block(
		&self,
		mut params: BlockImportParams<Block>,
	) -> Result<ImportResult, ConsensusError> {
		let parent_hash = *params.header.parent_hash();
		let block_number = *params.header.number();
		let finalized_hash = self.client.info().finalized_hash;

		let infos = self
			.client
			.runtime_api()
			.indexed_transactions(finalized_hash, block_number)
			.map_err(|e| ConsensusError::Other(format!("indexed_transactions: {e}").into()))?;
		let infos_len = infos.len();
		let body = params.body.as_ref().ok_or_else(|| {
			ConsensusError::Other("StorageChainBlockImport: gap-sync body absent after gate".into())
		})?;
		let classified = classify_body::<Block>(&infos, body);

		let missing = self.filter_missing(classified.renews);
		let payload = self.fetch_all(missing).await?;

		if !classified.ops.is_empty() || !payload.is_empty() {
			log::info!(
				target: LOG_TARGET,
				"gap-sync block #{block_number:?} ({parent_hash:?}, finalized={finalized_hash:?}): \
				 {infos_len} indexed entries, {} synthetic ops, {} renew payloads",
				classified.ops.len(),
				payload.len(),
			);
		}

		params.prefetched_indexed_transactions = PrefetchedIndexedTransactions {
			ops: classified.ops,
			renew_payloads: payload.into_iter().map(|(h, b)| (sp_core::H256::from(h), b)).collect(),
		};

		self.inner.import_block(params).await
	}

	/// Drops every entry whose data is already in the local TRANSACTION column.
	fn filter_missing(&self, renews: HashSet<RenewWant>) -> HashSet<RenewWant> {
		renews
			.into_iter()
			.filter(|w| !self.client.has_indexed_transaction(w.hash.into()).unwrap_or(false))
			.collect()
	}

	/// Bitswap-fetch every missing entry. Errors if any entry was not served.
	async fn fetch_all(
		&self,
		missing: HashSet<RenewWant>,
	) -> Result<Vec<(ContentHash, Vec<u8>)>, ConsensusError> {
		if missing.is_empty() {
			return Ok(Vec::new());
		}

		let wants: Vec<RenewWant> = missing.into_iter().collect();
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
			.map(|w| {
				let data = acquired
					.get(&w.hash)
					.expect("all hashes present; len equality verified above; qed")
					.clone();
				(w.hash, data)
			})
			.collect();

		Ok(payload)
	}

	/// Attach prefetched `(content_hash, bytes)` pairs to
	/// [`BlockImportParams::prefetched_indexed_transactions`] for the backend writer.
	///
	/// The tip-block paths populate only `renew_payloads`; runtime execution produces the
	/// actual `IndexOperation::Renew` ops via `update_transaction_index`, so synthetic ops
	/// stay empty here.
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
	/// `StorageChanges`. Caller must reassign `params.state_action` before forwarding so the
	/// inner block-import does not re-execute.
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

/// Returns runtime-verified renew wants for host-call renew operations.
fn verified_renews_from_index_ops(
	ops: &[IndexOperation],
	infos: &[IndexedTransactionInfo],
	context: &'static str,
) -> Result<HashSet<RenewWant>, ConsensusError> {
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
		renews.insert(RenewWant { hash, hashing: info.hashing, cid_codec: info.cid_codec });
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

/// Result of [`classify_body`]: the synthetic `IndexOperation`s plus the renew wants
/// whose bytes need to be bitswap-fetched.
pub(crate) struct ClassifiedBody {
	pub ops: Vec<IndexOperation>,
	pub renews: HashSet<RenewWant>,
}

/// Classifies every fetchable `IndexedTransactionInfo` entry against the block body.
///
/// For each `info` whose tail bytes (`body[info.extrinsic_index][len - info.size..]`)
/// hash to `info.content_hash` under the declared algorithm, emits an
/// `IndexOperation::Insert` (data is local to the body, no fetch needed). For each
/// remaining fetchable entry, emits an `IndexOperation::Renew` and adds a [`RenewWant`]
/// to the renew-fetch set.
///
/// Pure; no side effects. The renew set is what the wrapper bitswap-fetches; the ops
/// vec is what the wrapper hands to the backend via
/// `PrefetchedIndexedTransactions.ops`.
fn classify_body<Block: BlockT>(
	infos: &[IndexedTransactionInfo],
	body: &[Block::Extrinsic],
) -> ClassifiedBody {
	let mut ops = Vec::new();
	let mut renews = HashSet::new();

	for info in infos {
		let extrinsic_index = info.extrinsic_index;
		let Some(ext) = body.get(extrinsic_index as usize) else {
			log::error!(target: LOG_TARGET, "Unable to fetch extrinsic.");
			continue;
		};
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
			renews.insert(RenewWant {
				hash: info.content_hash,
				hashing: info.hashing,
				cid_codec: info.cid_codec,
			});
		}
	}

	ClassifiedBody { ops, renews }
}

/// Returns the renew-want set from [`classify_body`] only, discarding the ops vec.
///
/// `#[cfg(test)]`-only thin wrapper. Kept available so the regression test that pins the
/// "renew set == `classify_body(..).renews`" invariant stays callable from outside.
#[cfg(test)]
fn body_classify_renews<Block: BlockT>(
	infos: &[IndexedTransactionInfo],
	body: &[Block::Extrinsic],
) -> HashSet<RenewWant> {
	classify_body::<Block>(infos, body).renews
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
	use sc_network::bitswap::RAW_CODEC;
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
	fn body_classify_renews_accepts_non_raw_codec() {
		let body = vec![extrinsic(&[4, 5, 6])];
		let infos = vec![info(
			[9; 32],
			body[0].encode().len() as u32,
			HashingAlgorithm::Blake2b256,
			0x70,
			0,
		)];

		assert_eq!(
			body_classify_renews::<Block>(&infos, &body),
			HashSet::from([RenewWant {
				hash: [9; 32],
				hashing: HashingAlgorithm::Blake2b256,
				cid_codec: 0x70,
			}]),
		);
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
		assert_eq!(
			renews,
			HashSet::from([RenewWant {
				hash: [1; 32],
				hashing: HashingAlgorithm::Sha2_256,
				cid_codec: RAW_CODEC,
			}]),
		);
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
				RenewWant {
					hash: [2; 32],
					hashing: HashingAlgorithm::Blake2b256,
					cid_codec: RAW_CODEC,
				},
				RenewWant {
					hash: [3; 32],
					hashing: HashingAlgorithm::Keccak256,
					cid_codec: RAW_CODEC,
				},
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

		assert_eq!(
			renews,
			HashSet::from([RenewWant {
				hash: [4; 32],
				hashing: HashingAlgorithm::Blake2b256,
				cid_codec: RAW_CODEC,
			}]),
		);
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

		assert_eq!(
			renews,
			HashSet::from([RenewWant {
				hash,
				hashing: HashingAlgorithm::Keccak256,
				cid_codec: RAW_CODEC,
			}]),
		);
	}

	#[test]
	fn indexed_transactions_after_execute_block_requires_runtime_metadata_for_renew() {
		let hash = [9; 32];
		let ops = vec![IndexOperation::Renew { extrinsic: 0, hash: hash.to_vec() }];
		let err = verified_renews_from_index_ops(&ops, &[], "block execution").unwrap_err();
		let msg = format!("{err}");

		assert!(
			msg.contains("block execution: runtime API missing metadata"),
			"unexpected: {msg}",
		);
	}

	// Tests for `classify_body` covering both halves of the split (synthetic ops +
	// renew-fetch set).

	#[test]
	fn classify_body_pure_stores_only_emits_inserts() {
		let body = vec![extrinsic(&[1, 2, 3]), extrinsic(&[4, 5, 6]), extrinsic(&[7, 8, 9])];
		let infos = vec![
			body_info(&body[0], 0, HashingAlgorithm::Blake2b256, RAW_CODEC),
			body_info(&body[1], 1, HashingAlgorithm::Sha2_256, RAW_CODEC),
			body_info(&body[2], 2, HashingAlgorithm::Keccak256, RAW_CODEC),
		];

		let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

		assert_eq!(ops.len(), 3, "every entry must produce an op");
		for op in &ops {
			assert!(matches!(op, IndexOperation::Insert { .. }), "all stores expected");
		}
		assert!(renew_wants.is_empty(), "no fetches required for pure stores");
	}

	#[test]
	fn classify_body_pure_renews_only_emits_renews_and_fetch_set() {
		let body = vec![extrinsic(&[1]), extrinsic(&[2]), extrinsic(&[3])];
		let infos = vec![
			info(
				[0xA1; 32],
				body[0].encode().len() as u32,
				HashingAlgorithm::Blake2b256,
				RAW_CODEC,
				0,
			),
			info(
				[0xB2; 32],
				body[1].encode().len() as u32,
				HashingAlgorithm::Sha2_256,
				RAW_CODEC,
				1,
			),
			info(
				[0xC3; 32],
				body[2].encode().len() as u32,
				HashingAlgorithm::Keccak256,
				RAW_CODEC,
				2,
			),
		];

		let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

		assert_eq!(ops.len(), 3, "every entry must produce an op");
		for op in &ops {
			assert!(matches!(op, IndexOperation::Renew { .. }), "all renews expected");
		}
		assert_eq!(
			renew_wants,
			HashSet::from([
				RenewWant {
					hash: [0xA1; 32],
					hashing: HashingAlgorithm::Blake2b256,
					cid_codec: RAW_CODEC,
				},
				RenewWant {
					hash: [0xB2; 32],
					hashing: HashingAlgorithm::Sha2_256,
					cid_codec: RAW_CODEC,
				},
				RenewWant {
					hash: [0xC3; 32],
					hashing: HashingAlgorithm::Keccak256,
					cid_codec: RAW_CODEC,
				},
			]),
		);
	}

	#[test]
	fn classify_body_mixed_store_renew_emits_both() {
		let body = vec![extrinsic(&[10]), extrinsic(&[20]), extrinsic(&[30]), extrinsic(&[40])];
		let infos = vec![
			body_info(&body[0], 0, HashingAlgorithm::Blake2b256, RAW_CODEC), // store
			info(
				[0xAB; 32],
				body[1].encode().len() as u32,
				HashingAlgorithm::Sha2_256,
				RAW_CODEC,
				1,
			), // renew
			body_info(&body[2], 2, HashingAlgorithm::Keccak256, RAW_CODEC),  // store
			info(
				[0xCD; 32],
				body[3].encode().len() as u32,
				HashingAlgorithm::Blake2b256,
				RAW_CODEC,
				3,
			), // renew
		];

		let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

		assert_eq!(ops.len(), 4);
		// Extrinsic-index order matches input order (the loop is sequential).
		assert!(matches!(ops[0], IndexOperation::Insert { extrinsic: 0, .. }));
		assert!(matches!(ops[1], IndexOperation::Renew { extrinsic: 1, .. }));
		assert!(matches!(ops[2], IndexOperation::Insert { extrinsic: 2, .. }));
		assert!(matches!(ops[3], IndexOperation::Renew { extrinsic: 3, .. }));
		assert_eq!(
			renew_wants,
			HashSet::from([
				RenewWant {
					hash: [0xAB; 32],
					hashing: HashingAlgorithm::Sha2_256,
					cid_codec: RAW_CODEC,
				},
				RenewWant {
					hash: [0xCD; 32],
					hashing: HashingAlgorithm::Blake2b256,
					cid_codec: RAW_CODEC,
				},
			]),
		);
	}

	#[test]
	fn classify_body_per_hashing_dispatches_correctly() {
		for hashing in
			[HashingAlgorithm::Blake2b256, HashingAlgorithm::Sha2_256, HashingAlgorithm::Keccak256]
		{
			let body = vec![extrinsic(&[0xFE])];
			let infos = vec![body_info(&body[0], 0, hashing, RAW_CODEC)];

			let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

			assert_eq!(ops.len(), 1, "{hashing:?}: one op expected");
			assert!(
				matches!(ops[0], IndexOperation::Insert { .. }),
				"{hashing:?}: matching tail must classify as store",
			);
			assert!(renew_wants.is_empty(), "{hashing:?}: no fetch needed");
		}
	}

	#[test]
	fn classify_body_oversized_tail_classifies_as_renew() {
		let body = vec![extrinsic(&[0xAA])];
		let oversized = body[0].encode().len() as u32 + 1;
		let infos = vec![info([0x77; 32], oversized, HashingAlgorithm::Blake2b256, RAW_CODEC, 0)];

		let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

		assert_eq!(ops.len(), 1);
		assert!(matches!(ops[0], IndexOperation::Renew { .. }));
		assert_eq!(
			renew_wants,
			HashSet::from([RenewWant {
				hash: [0x77; 32],
				hashing: HashingAlgorithm::Blake2b256,
				cid_codec: RAW_CODEC,
			}]),
		);
	}

	#[test]
	fn classify_body_skips_u32_max_extrinsic_index() {
		let body = vec![extrinsic(&[0xBB])];
		let encoded_len = body[0].encode().len() as u32;
		let infos =
			vec![info([0x55; 32], encoded_len, HashingAlgorithm::Blake2b256, RAW_CODEC, u32::MAX)];

		let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

		assert!(ops.is_empty(), "u32::MAX extrinsic_index must be skipped");
		assert!(renew_wants.is_empty(), "u32::MAX extrinsic_index must not request a fetch");
	}

	#[test]
	fn classify_body_accepts_non_raw_codec() {
		let body = vec![extrinsic(&[0xCC])];
		let encoded_len = body[0].encode().len() as u32;
		let infos = vec![info([0x33; 32], encoded_len, HashingAlgorithm::Blake2b256, 0x70, 0)];

		let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

		assert_eq!(ops.len(), 1, "non-RAW codec must still be classified");
		assert!(matches!(ops[0], IndexOperation::Renew { .. }));
		assert_eq!(
			renew_wants,
			HashSet::from([RenewWant {
				hash: [0x33; 32],
				hashing: HashingAlgorithm::Blake2b256,
				cid_codec: 0x70,
			}]),
			"runtime-declared codec must be preserved in the renew-want set",
		);
	}

	#[test]
	fn classify_body_skips_extrinsic_index_out_of_range() {
		let body = vec![extrinsic(&[0xDD])];
		let encoded_len = body[0].encode().len() as u32;
		let infos =
			vec![info([0x22; 32], encoded_len, HashingAlgorithm::Blake2b256, RAW_CODEC, 99)];

		let ClassifiedBody { ops, renews: renew_wants } = classify_body::<Block>(&infos, &body);

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
			info(
				[0xEE; 32],
				body[1].encode().len() as u32,
				HashingAlgorithm::Sha2_256,
				RAW_CODEC,
				1,
			), // renew
		];

		let renews = body_classify_renews::<Block>(&infos, &body);
		let expected_renews = classify_body::<Block>(&infos, &body).renews;

		assert_eq!(renews, expected_renews);
		assert_eq!(
			renews,
			HashSet::from([RenewWant {
				hash: [0xEE; 32],
				hashing: HashingAlgorithm::Sha2_256,
				cid_codec: RAW_CODEC,
			}]),
		);
	}
}
