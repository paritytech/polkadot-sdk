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

//! Import Queue primitive: something which can verify and import blocks.
//!
//! This serves as an intermediate and abstracted step between synchronization
//! and import. Each mode of consensus will have its own requirements for block
//! verification. Some algorithms can verify in parallel, while others only
//! sequentially.
//!
//! The `ImportQueue` trait allows such verification strategies to be
//! instantiated. The `BasicQueue` and `BasicVerifier` traits allow serial
//! queues to be instantiated simply.

use log::{debug, trace};
use std::{
	fmt,
	time::{Duration, Instant},
};

use sp_consensus::{error::Error as ConsensusError, BlockOrigin};
use sp_runtime::{
	generic::DigestItem,
	traits::{Block as BlockT, Header as _, NumberFor},
	Justifications,
};

use crate::{
	block_import::{
		BlockCheckParams, BlockImport, BlockImportParams, ImportResult, ImportedAux, ImportedState,
		JustificationImport, StateAction,
	},
	metrics::Metrics,
};

pub use basic_queue::BasicQueue;

const LOG_TARGET: &str = "sync::import-queue";

/// A commonly-used Import Queue type.
///
/// This defines the transaction type of the `BasicQueue` to be the transaction type for a client.
pub type DefaultImportQueue<Block> = BasicQueue<Block>;

mod basic_queue;
pub mod buffered_link;
pub mod mock;

/// Shared block import struct used by the queue.
pub type BoxBlockImport<B> = Box<dyn BlockImport<B, Error = ConsensusError> + Send + Sync>;

/// Shared justification import struct used by the queue.
pub type BoxJustificationImport<B> =
	Box<dyn JustificationImport<B, Error = ConsensusError> + Send + Sync>;

/// Maps to the RuntimeOrigin used by the network.
pub type RuntimeOrigin = sc_network_types::PeerId;

/// Block data used by the queue.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IncomingBlock<B: BlockT> {
	/// Block header hash.
	pub hash: <B as BlockT>::Hash,
	/// Block header if requested.
	pub header: Option<<B as BlockT>::Header>,
	/// Block body if requested.
	pub body: Option<Vec<<B as BlockT>::Extrinsic>>,
	/// Indexed block body if requested.
	pub indexed_body: Option<Vec<Vec<u8>>>,
	/// Justification(s) if requested.
	pub justifications: Option<Justifications>,
	/// The peer, we received this from
	pub origin: Option<RuntimeOrigin>,
	/// Allow importing the block skipping state verification if parent state is missing.
	pub allow_missing_state: bool,
	/// Skip block execution and state verification.
	pub skip_execution: bool,
	/// Re-validate existing block.
	pub import_existing: bool,
	/// Do not compute new state, but rather set it to the given set.
	pub state: Option<ImportedState<B>>,
	/// Opaque SCALE-encoded additional data attached to this block.
	pub additional_data: Option<sp_additional_data::AdditionalData>,
}

/// Verify a justification of a block
#[async_trait::async_trait]
pub trait Verifier<B: BlockT>: Send + Sync {
	/// Verify the given block data and return the `BlockImportParams` to
	/// continue the block import process.
	async fn verify(&self, block: BlockImportParams<B>) -> Result<BlockImportParams<B>, String>;
}

/// Blocks import queue API.
///
/// The `import_*` methods can be called in order to send elements for the import queue to verify.
pub trait ImportQueueService<B: BlockT>: Send {
	/// Import a bunch of blocks, every next block must be an ancestor of the previous block in the
	/// list.
	fn import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>);

	/// Import block justifications.
	fn import_justifications(
		&mut self,
		who: RuntimeOrigin,
		hash: B::Hash,
		number: NumberFor<B>,
		justifications: Justifications,
	);
}

#[async_trait::async_trait]
pub trait ImportQueue<B: BlockT>: Send {
	/// Get a copy of the handle to [`ImportQueueService`].
	fn service(&self) -> Box<dyn ImportQueueService<B>>;

	/// Get a reference to the handle to [`ImportQueueService`].
	fn service_ref(&mut self) -> &mut dyn ImportQueueService<B>;

	/// This method should behave in a way similar to `Future::poll`. It can register the current
	/// task and notify later when more actions are ready to be polled. To continue the comparison,
	/// it is as if this method always returned `Poll::Pending`.
	fn poll_actions(&mut self, cx: &mut futures::task::Context, link: &dyn Link<B>);

	/// Start asynchronous runner for import queue.
	///
	/// Takes an object implementing [`Link`] which allows the import queue to
	/// influence the synchronization process.
	async fn run(self, link: &dyn Link<B>);
}

/// The result of importing a justification.
#[derive(Debug, PartialEq)]
pub enum JustificationImportResult {
	/// Justification was imported successfully.
	Success,

	/// Justification was not imported successfully.
	Failure,

	/// Justification was not imported successfully, because it is outdated.
	OutdatedJustification,
}

/// Hooks that the verification queue can use to influence the synchronization
/// algorithm.
pub trait Link<B: BlockT>: Send + Sync {
	/// Batch of blocks imported, with or without error.
	fn blocks_processed(
		&self,
		_imported: usize,
		_count: usize,
		_results: Vec<(BlockImportResult<B>, B::Hash)>,
	) {
	}

	/// Justification import result.
	fn justification_imported(
		&self,
		_who: RuntimeOrigin,
		_hash: &B::Hash,
		_number: NumberFor<B>,
		_import_result: JustificationImportResult,
	) {
	}

	/// Request a justification for the given block.
	fn request_justification(&self, _hash: &B::Hash, _number: NumberFor<B>) {}
}

/// Block import successful result.
#[derive(Debug, PartialEq)]
pub enum BlockImportStatus<BlockNumber: fmt::Debug + PartialEq> {
	/// Imported known block.
	ImportedKnown(BlockNumber, Option<RuntimeOrigin>),
	/// Imported unknown block.
	ImportedUnknown(BlockNumber, ImportedAux, Option<RuntimeOrigin>),
}

impl<BlockNumber: fmt::Debug + PartialEq> BlockImportStatus<BlockNumber> {
	/// Returns the imported block number.
	pub fn number(&self) -> &BlockNumber {
		match self {
			BlockImportStatus::ImportedKnown(n, _) |
			BlockImportStatus::ImportedUnknown(n, _, _) => n,
		}
	}
}

/// Block import error.
#[derive(Debug, thiserror::Error)]
pub enum BlockImportError {
	/// Block missed header, can't be imported
	#[error("block is missing a header (origin = {0:?})")]
	IncompleteHeader(Option<RuntimeOrigin>),

	/// Block verification failed, can't be imported
	#[error("block verification failed (origin = {0:?}): {1}")]
	VerificationFailed(Option<RuntimeOrigin>, String),

	/// Block is known to be Bad
	#[error("bad block (origin = {0:?})")]
	BadBlock(Option<RuntimeOrigin>),

	/// Parent state is missing.
	#[error("block is missing parent state")]
	MissingState,

	/// Block has an unknown parent
	#[error("block has an unknown parent")]
	UnknownParent,

	/// Block import has been cancelled. This can happen if the parent block fails to be imported.
	#[error("import has been cancelled")]
	Cancelled,

	/// Other error.
	#[error("consensus error: {0}")]
	Other(ConsensusError),
}

type BlockImportResult<B> = Result<BlockImportStatus<NumberFor<B>>, BlockImportError>;

/// Single block import function.
pub async fn import_single_block<B: BlockT, V: Verifier<B>>(
	import_handle: &mut impl BlockImport<B, Error = ConsensusError>,
	block_origin: BlockOrigin,
	block: IncomingBlock<B>,
	verifier: &V,
) -> BlockImportResult<B> {
	match verify_single_block_metered(import_handle, block_origin, block, verifier, None).await? {
		SingleBlockVerificationOutcome::Imported(import_status) => Ok(import_status),
		SingleBlockVerificationOutcome::Verified(import_parameters) => {
			import_single_block_metered(import_handle, import_parameters, None).await
		},
	}
}

fn import_handler<Block>(
	number: NumberFor<Block>,
	hash: Block::Hash,
	parent_hash: Block::Hash,
	block_origin: Option<RuntimeOrigin>,
	import: Result<ImportResult, ConsensusError>,
) -> Result<BlockImportStatus<NumberFor<Block>>, BlockImportError>
where
	Block: BlockT,
{
	match import {
		Ok(ImportResult::AlreadyInChain) => {
			trace!(target: LOG_TARGET, "Block already in chain {}: {:?}", number, hash);
			Ok(BlockImportStatus::ImportedKnown(number, block_origin))
		},
		Ok(ImportResult::Imported(aux)) => {
			Ok(BlockImportStatus::ImportedUnknown(number, aux, block_origin))
		},
		Ok(ImportResult::MissingState) => {
			debug!(
				target: LOG_TARGET,
				"Parent state is missing for {}: {:?}, parent: {:?}", number, hash, parent_hash
			);
			Err(BlockImportError::MissingState)
		},
		Ok(ImportResult::UnknownParent) => {
			debug!(
				target: LOG_TARGET,
				"Block with unknown parent {}: {:?}, parent: {:?}", number, hash, parent_hash
			);
			Err(BlockImportError::UnknownParent)
		},
		Ok(ImportResult::KnownBad) => {
			debug!(target: LOG_TARGET, "Peer gave us a bad block {}: {:?}", number, hash);
			Err(BlockImportError::BadBlock(block_origin))
		},
		Err(e) => {
			debug!(target: LOG_TARGET, "Error importing block {}: {:?}: {}", number, hash, e);
			Err(BlockImportError::Other(e))
		},
	}
}

pub(crate) enum SingleBlockVerificationOutcome<Block: BlockT> {
	/// Block is already imported.
	Imported(BlockImportStatus<NumberFor<Block>>),
	/// Block is verified, but needs to be imported.
	Verified(SingleBlockImportParameters<Block>),
}

pub(crate) struct SingleBlockImportParameters<Block: BlockT> {
	import_block: BlockImportParams<Block>,
	hash: Block::Hash,
	block_origin: Option<RuntimeOrigin>,
	verification_time: Duration,
}

/// Single block import function with metering.
pub(crate) async fn verify_single_block_metered<B: BlockT, V: Verifier<B>>(
	import_handle: &impl BlockImport<B, Error = ConsensusError>,
	block_origin: BlockOrigin,
	block: IncomingBlock<B>,
	verifier: &V,
	metrics: Option<&Metrics>,
) -> Result<SingleBlockVerificationOutcome<B>, BlockImportError> {
	let peer = block.origin;
	let justifications = block.justifications;

	let Some(header) = block.header else {
		if let Some(ref peer) = peer {
			debug!(target: LOG_TARGET, "Header {} was not provided by {peer} ", block.hash);
		} else {
			debug!(target: LOG_TARGET, "Header {} was not provided ", block.hash);
		}
		return Err(BlockImportError::IncompleteHeader(peer));
	};

	let number = *header.number();
	let hash = block.hash;
	let parent_hash = *header.parent_hash();

	trace!(target: LOG_TARGET, "Block {number} ({hash}) has {:?} logs (origin: {:?})", header.digest().logs().len(), block_origin);

	// Skip block verification for warp synced blocks.
	// They have been verified within warp sync proof verification.
	if matches!(block_origin, BlockOrigin::WarpSync) {
		// Check point A: verify additional_data before bypassing the verifier.
		verify_additional_data_non_executing::<B>(&header, block.additional_data.as_ref())?;
		let mut import_block = BlockImportParams::new(block_origin, header);
		import_block.additional_data = block.additional_data;
		return Ok(SingleBlockVerificationOutcome::Verified(SingleBlockImportParameters {
			import_block,
			hash: block.hash,
			block_origin: peer,
			verification_time: Duration::ZERO,
		}));
	}

	match import_handler::<B>(
		number,
		hash,
		parent_hash,
		peer,
		import_handle
			.check_block(BlockCheckParams {
				hash,
				number,
				parent_hash,
				allow_missing_state: block.allow_missing_state,
				import_existing: block.import_existing,
				allow_missing_parent: block.state.is_some(),
			})
			.await,
	)? {
		BlockImportStatus::ImportedUnknown { .. } => (),
		r => {
			// Any other successful result means that the block is already imported.
			return Ok(SingleBlockVerificationOutcome::Imported(r));
		},
	}

	let started = Instant::now();

	let mut import_block = BlockImportParams::new(block_origin, header);
	import_block.body = block.body;
	import_block.justifications = justifications;
	import_block.post_hash = Some(hash);
	import_block.import_existing = block.import_existing;
	import_block.indexed_body = block.indexed_body;
	import_block.additional_data = block.additional_data;

	if let Some(state) = block.state {
		let changes = crate::block_import::StorageChanges::Import(state);
		import_block.state_action = StateAction::ApplyChanges(changes);
	} else if block.skip_execution {
		// Check point B: verify additional_data before skipping execution.
		verify_additional_data_non_executing::<B>(
			&import_block.header,
			import_block.additional_data.as_ref(),
		)?;
		import_block.state_action = StateAction::Skip;
	} else if block.allow_missing_state {
		import_block.state_action = StateAction::ExecuteIfPossible;
	}

	let import_block = verifier.verify(import_block).await.map_err(|msg| {
		if let Some(ref peer) = peer {
			trace!(
				target: LOG_TARGET,
				"Verifying {}({}) from {} failed: {}",
				number,
				hash,
				peer,
				msg
			);
		} else {
			trace!(target: LOG_TARGET, "Verifying {}({}) failed: {}", number, hash, msg);
		}
		if let Some(metrics) = metrics {
			metrics.report_verification(false, started.elapsed());
		}
		BlockImportError::VerificationFailed(peer, msg)
	})?;

	let verification_time = started.elapsed();
	if let Some(metrics) = metrics {
		metrics.report_verification(true, verification_time);
	}

	Ok(SingleBlockVerificationOutcome::Verified(SingleBlockImportParameters {
		import_block,
		hash,
		block_origin: peer,
		verification_time,
	}))
}

/// Verifies `additional_data` consistency against the block header's
/// [`DigestItem::AdditionalData`] digest for every **non-executing** import path.
///
/// Non-executing paths skip block execution and therefore cannot rely on the executing-path
/// backstop (re-running runtime host functions + header-equality check). This function is the
/// only mechanism that enforces the hash-vs-digest invariant on those paths.
///
/// # Enumerated non-executing import paths
///
/// The following are every production call site that reaches this check. All currently supply
/// `additional_data: None`, but the check is required for soundness: a malicious peer could
/// supply tampered data and there is no other mechanism to detect it.
///
/// **Check point A — WarpSync early return** (`import_queue.rs`, guarded by
/// `BlockOrigin::WarpSync`):
/// - `substrate/client/network/sync/src/strategy/warp.rs:406` — `proof_to_incoming_block`:
///   warp-proof authority-set headers imported with `BlockOrigin::WarpSync, skip_execution: true`.
///   These bypass the verifier entirely via the early-return guard in this file.
///
/// **Check point B — `StateAction::Skip` branch** (`import_queue.rs`, guarded by
/// `IncomingBlock::skip_execution`):
/// - `substrate/client/network/sync/src/strategy/chain_sync.rs:1360` —
///   `PeerSyncState::DownloadingGap`: gap-sync filling the post-warp-sync historic hole,
///   `import_existing: true, skip_execution: true, BlockOrigin::GapSync`. This is the *only*
///   production "background block download" path after warp sync; there is no separate warp.rs
///   background-import path (confirmed by plan todo 12).
/// - `substrate/client/network/sync/src/strategy/chain_sync.rs:1411` — regular sync in
///   `ChainSyncMode::LightState`: `skip_execution: self.skip_execution()` returns `true`; historic
///   blocks are not executed.
/// - `substrate/client/network/sync/src/strategy/chain_sync.rs:1554` —
///   `PeerSyncState::DownloadingStale`: stale blocks downloaded for completeness, `skip_execution:
///   true`.
///
/// # Invariant enforced
///
/// Exactly one of four outcomes is accepted; any other combination is rejected:
/// - Both absent → OK (block carries no additional data).
/// - Both present and `blake2_256(blob) == digest_hash` → OK.
/// - Both present but hash mismatch → rejected (`ClientImport` error).
/// - `additional_data` present, digest absent → rejected (data without commitment).
/// - Digest present, `additional_data` absent → rejected (commitment without data).
fn verify_additional_data_non_executing<B: BlockT>(
	header: &B::Header,
	additional_data: Option<&sp_additional_data::AdditionalData>,
) -> Result<(), BlockImportError> {
	let digest_hash: Option<[u8; 32]> = header.digest().logs().iter().find_map(|log| {
		if let DigestItem::AdditionalData(h) = log {
			Some(*h)
		} else {
			None
		}
	});
	match (additional_data, digest_hash) {
		(Some(blob), Some(expected)) => {
			let actual = sp_additional_data::hash(blob);
			if actual != expected {
				return Err(BlockImportError::Other(ConsensusError::ClientImport(
					"additional data hash mismatch on non-executing import path".into(),
				)));
			}
		},
		(Some(_), None) => {
			return Err(BlockImportError::Other(ConsensusError::ClientImport(
				"additional_data present but header has no AdditionalData digest on \
				 non-executing import path"
					.into(),
			)))
		},
		(None, Some(_)) => {
			return Err(BlockImportError::Other(ConsensusError::ClientImport(
				"header has AdditionalData digest but additional_data is absent on \
				 non-executing import path"
					.into(),
			)))
		},
		(None, None) => {},
	}
	Ok(())
}

pub(crate) async fn import_single_block_metered<Block: BlockT>(
	import_handle: &mut impl BlockImport<Block, Error = ConsensusError>,
	import_parameters: SingleBlockImportParameters<Block>,
	metrics: Option<&Metrics>,
) -> BlockImportResult<Block> {
	let started = Instant::now();

	let SingleBlockImportParameters { import_block, hash, block_origin, verification_time } =
		import_parameters;

	let number = *import_block.header.number();
	let parent_hash = *import_block.header.parent_hash();

	let imported = import_handle.import_block(import_block).await;
	if let Some(metrics) = metrics {
		metrics.report_verification_and_import(started.elapsed() + verification_time);
	}

	import_handler::<Block>(number, hash, parent_hash, block_origin, imported)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_additional_data::{hash, AdditionalData};
	use sp_runtime::generic::{Digest, DigestItem};
	use sp_test_primitives::{Block as TestBlock, Header as TestHeader};

	struct PanicImport;

	#[async_trait::async_trait]
	impl BlockImport<TestBlock> for PanicImport {
		type Error = ConsensusError;

		async fn check_block(
			&self,
			_: BlockCheckParams<TestBlock>,
		) -> Result<ImportResult, ConsensusError> {
			panic!("check_block must not be called on WarpSync path");
		}

		async fn import_block(
			&self,
			_: BlockImportParams<TestBlock>,
		) -> Result<ImportResult, ConsensusError> {
			panic!("import_block must not be called in this test");
		}
	}

	struct OkCheckImport;

	#[async_trait::async_trait]
	impl BlockImport<TestBlock> for OkCheckImport {
		type Error = ConsensusError;

		async fn check_block(
			&self,
			_: BlockCheckParams<TestBlock>,
		) -> Result<ImportResult, ConsensusError> {
			Ok(ImportResult::Imported(ImportedAux::default()))
		}

		async fn import_block(
			&self,
			_: BlockImportParams<TestBlock>,
		) -> Result<ImportResult, ConsensusError> {
			panic!("import_block must not be called in this test");
		}
	}

	struct PassVerifier;

	#[async_trait::async_trait]
	impl Verifier<TestBlock> for PassVerifier {
		async fn verify(
			&self,
			block: BlockImportParams<TestBlock>,
		) -> Result<BlockImportParams<TestBlock>, String> {
			Ok(block)
		}
	}

	fn make_blob() -> AdditionalData {
		AdditionalData::from([("test".to_string(), vec![1u8, 2, 3])])
	}

	fn header_with_digest_item(item: DigestItem) -> TestHeader {
		TestHeader::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: vec![item] },
		)
	}

	fn header_no_digest() -> TestHeader {
		TestHeader::new(
			1,
			Default::default(),
			Default::default(),
			Default::default(),
			Digest { logs: vec![] },
		)
	}

	fn incoming_warp(
		header: TestHeader,
		additional_data: Option<sp_additional_data::AdditionalData>,
	) -> IncomingBlock<TestBlock> {
		IncomingBlock {
			hash: sp_runtime::traits::Header::hash(&header),
			header: Some(header),
			body: None,
			indexed_body: None,
			justifications: None,
			origin: None,
			allow_missing_state: true,
			skip_execution: false,
			import_existing: false,
			state: None,
			additional_data,
		}
	}

	fn incoming_skip(
		header: TestHeader,
		additional_data: Option<sp_additional_data::AdditionalData>,
	) -> IncomingBlock<TestBlock> {
		IncomingBlock {
			hash: sp_runtime::traits::Header::hash(&header),
			header: Some(header),
			body: None,
			indexed_body: None,
			justifications: None,
			origin: None,
			allow_missing_state: false,
			skip_execution: true,
			import_existing: false,
			state: None,
			additional_data,
		}
	}

	// ── Check point A: WarpSync early return ─────────────────────────────────────────────────

	#[test]
	fn warp_correct_match_accepted() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let header = header_with_digest_item(DigestItem::AdditionalData(hash(&blob)));
			let block = incoming_warp(header, Some(blob));
			let r = verify_single_block_metered(
				&PanicImport,
				BlockOrigin::WarpSync,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(r.is_ok());
		});
	}

	#[test]
	fn warp_tampered_data_rejected() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let header = header_with_digest_item(DigestItem::AdditionalData(hash(&blob)));
			let mut tampered = blob.clone();
			tampered.insert("tampered".to_string(), vec![0xff]);
			let block = incoming_warp(header, Some(tampered));
			let r = verify_single_block_metered(
				&PanicImport,
				BlockOrigin::WarpSync,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(matches!(&r, Err(BlockImportError::Other(ConsensusError::ClientImport(m)))
					if m.contains("hash mismatch")),);
		});
	}

	#[test]
	fn warp_digest_present_data_absent_rejected() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let header = header_with_digest_item(DigestItem::AdditionalData(hash(&blob)));
			let block = incoming_warp(header, None);
			let r = verify_single_block_metered(
				&PanicImport,
				BlockOrigin::WarpSync,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(matches!(&r, Err(BlockImportError::Other(ConsensusError::ClientImport(m)))
					if m.contains("additional_data is absent")),);
		});
	}

	#[test]
	fn warp_data_present_digest_absent_rejected() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let block = incoming_warp(header_no_digest(), Some(blob));
			let r = verify_single_block_metered(
				&PanicImport,
				BlockOrigin::WarpSync,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(matches!(&r, Err(BlockImportError::Other(ConsensusError::ClientImport(m)))
					if m.contains("no AdditionalData digest")),);
		});
	}

	// ── Check point B: StateAction::Skip (skip_execution: true) ─────────────────────────────

	#[test]
	fn skip_exec_correct_match_accepted() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let header = header_with_digest_item(DigestItem::AdditionalData(hash(&blob)));
			let block = incoming_skip(header, Some(blob));
			let r = verify_single_block_metered(
				&OkCheckImport,
				BlockOrigin::NetworkBroadcast,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(r.is_ok());
		});
	}

	#[test]
	fn skip_exec_tampered_data_rejected() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let header = header_with_digest_item(DigestItem::AdditionalData(hash(&blob)));
			let mut tampered = blob.clone();
			tampered.insert("tampered".to_string(), vec![0xff]);
			let block = incoming_skip(header, Some(tampered));
			let r = verify_single_block_metered(
				&OkCheckImport,
				BlockOrigin::NetworkBroadcast,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(matches!(&r, Err(BlockImportError::Other(ConsensusError::ClientImport(m)))
					if m.contains("hash mismatch")),);
		});
	}

	#[test]
	fn skip_exec_digest_present_data_absent_rejected() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let header = header_with_digest_item(DigestItem::AdditionalData(hash(&blob)));
			let block = incoming_skip(header, None);
			let r = verify_single_block_metered(
				&OkCheckImport,
				BlockOrigin::NetworkBroadcast,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(matches!(&r, Err(BlockImportError::Other(ConsensusError::ClientImport(m)))
					if m.contains("additional_data is absent")),);
		});
	}

	#[test]
	fn skip_exec_data_present_digest_absent_rejected() {
		futures::executor::block_on(async {
			let blob = make_blob();
			let block = incoming_skip(header_no_digest(), Some(blob));
			let r = verify_single_block_metered(
				&OkCheckImport,
				BlockOrigin::NetworkBroadcast,
				block,
				&PassVerifier,
				None,
			)
			.await;
			assert!(matches!(&r, Err(BlockImportError::Other(ConsensusError::ClientImport(m)))
					if m.contains("no AdditionalData digest")),);
		});
	}
}
