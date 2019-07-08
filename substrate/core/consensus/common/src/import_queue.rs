// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

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

use std::{sync::Arc, collections::HashMap};
use runtime_primitives::{Justification, traits::{Block as BlockT, Header as _, NumberFor}};
use crate::{error::Error as ConsensusError, well_known_cache_keys::Id as CacheKeyId};
use crate::block_import::{
	BlockImport, BlockOrigin, ImportBlock, ImportedAux, JustificationImport, ImportResult,
	FinalityProofImport, FinalityProofRequestBuilder,
};

pub use basic_queue::BasicQueue;

mod basic_queue;
pub mod buffered_link;

/// Shared block import struct used by the queue.
pub type SharedBlockImport<B> = Arc<dyn BlockImport<B, Error = ConsensusError> + Send + Sync>;

/// Shared justification import struct used by the queue.
pub type SharedJustificationImport<B> = Arc<dyn JustificationImport<B, Error=ConsensusError> + Send + Sync>;

/// Shared finality proof import struct used by the queue.
pub type SharedFinalityProofImport<B> = Arc<dyn FinalityProofImport<B, Error=ConsensusError> + Send + Sync>;

/// Shared finality proof request builder struct used by the queue.
pub type SharedFinalityProofRequestBuilder<B> = Arc<dyn FinalityProofRequestBuilder<B> + Send + Sync>;

/// Maps to the Origin used by the network.
pub type Origin = libp2p::PeerId;

/// Block data used by the queue.
#[derive(Debug, PartialEq, Eq, Clone)]
pub struct IncomingBlock<B: BlockT> {
	/// Block header hash.
	pub hash: <B as BlockT>::Hash,
	/// Block header if requested.
	pub header: Option<<B as BlockT>::Header>,
	/// Block body if requested.
	pub body: Option<Vec<<B as BlockT>::Extrinsic>>,
	/// Justification if requested.
	pub justification: Option<Justification>,
	/// The peer, we received this from
	pub origin: Option<Origin>,
}

/// Verify a justification of a block
pub trait Verifier<B: BlockT>: Send + Sync {
	/// Verify the given data and return the ImportBlock and an optional
	/// new set of validators to import. If not, err with an Error-Message
	/// presented to the User in the logs.
	fn verify(
		&self,
		origin: BlockOrigin,
		header: B::Header,
		justification: Option<Justification>,
		body: Option<Vec<B::Extrinsic>>,
	) -> Result<(ImportBlock<B>, Option<Vec<(CacheKeyId, Vec<u8>)>>), String>;
}

/// Blocks import queue API.
///
/// The `import_*` methods can be called in order to send elements for the import queue to verify.
/// Afterwards, call `poll_actions` to determine how to respond to these elements.
pub trait ImportQueue<B: BlockT>: Send {
	/// Import bunch of blocks.
	fn import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>);
	/// Import a block justification.
	fn import_justification(
		&mut self,
		who: Origin,
		hash: B::Hash,
		number: NumberFor<B>,
		justification: Justification
	);
	/// Import block finality proof.
	fn import_finality_proof(
		&mut self,
		who: Origin,
		hash: B::Hash,
		number: NumberFor<B>,
		finality_proof: Vec<u8>
	);
	/// Polls for actions to perform on the network.
	///
	/// This method should behave in a way similar to `Future::poll`. It can register the current
	/// task and notify later when more actions are ready to be polled. To continue the comparison,
	/// it is as if this method always returned `Ok(Async::NotReady)`.
	fn poll_actions(&mut self, link: &mut dyn Link<B>);
}

/// Hooks that the verification queue can use to influence the synchronization
/// algorithm.
pub trait Link<B: BlockT>: Send {
	/// Block imported.
	fn block_imported(&mut self, _hash: &B::Hash, _number: NumberFor<B>) {}
	/// Batch of blocks imported, with or without error.
	fn blocks_processed(&mut self, _processed_blocks: Vec<B::Hash>, _has_error: bool) {}
	/// Justification import result.
	fn justification_imported(&mut self, _who: Origin, _hash: &B::Hash, _number: NumberFor<B>, _success: bool) {}
	/// Clear all pending justification requests.
	fn clear_justification_requests(&mut self) {}
	/// Request a justification for the given block.
	fn request_justification(&mut self, _hash: &B::Hash, _number: NumberFor<B>) {}
	/// Finality proof import result.
	///
	/// Even though we have asked for finality proof of block A, provider could return proof of
	/// some earlier block B, if the proof for A was too large. The sync module should continue
	/// asking for proof of A in this case.
	fn finality_proof_imported(
		&mut self,
		_who: Origin,
		_request_block: (B::Hash, NumberFor<B>),
		_finalization_result: Result<(B::Hash, NumberFor<B>), ()>,
	) {}
	/// Request a finality proof for the given block.
	fn request_finality_proof(&mut self, _hash: &B::Hash, _number: NumberFor<B>) {}
	/// Remember finality proof request builder on start.
	fn set_finality_proof_request_builder(&mut self, _request_builder: SharedFinalityProofRequestBuilder<B>) {}
	/// Adjusts the reputation of the given peer.
	fn report_peer(&mut self, _who: Origin, _reputation_change: i32) {}
	/// Restart sync.
	fn restart(&mut self) {}
}

/// Block import successful result.
#[derive(Debug, PartialEq)]
pub enum BlockImportResult<N: ::std::fmt::Debug + PartialEq> {
	/// Imported known block.
	ImportedKnown(N),
	/// Imported unknown block.
	ImportedUnknown(N, ImportedAux, Option<Origin>),
}

/// Block import error.
#[derive(Debug, PartialEq)]
pub enum BlockImportError {
	/// Block missed header, can't be imported
	IncompleteHeader(Option<Origin>),
	/// Block verification failed, can't be imported
	VerificationFailed(Option<Origin>, String),
	/// Block is known to be Bad
	BadBlock(Option<Origin>),
	/// Block has an unknown parent
	UnknownParent,
	/// Other Error.
	Error,
}

/// Single block import function.
pub fn import_single_block<B: BlockT, V: Verifier<B>>(
	import_handle: &dyn BlockImport<B, Error = ConsensusError>,
	block_origin: BlockOrigin,
	block: IncomingBlock<B>,
	verifier: Arc<V>,
) -> Result<BlockImportResult<NumberFor<B>>, BlockImportError> {
	let peer = block.origin;

	let (header, justification) = match (block.header, block.justification) {
		(Some(header), justification) => (header, justification),
		(None, _) => {
			if let Some(ref peer) = peer {
				debug!(target: "sync", "Header {} was not provided by {} ", block.hash, peer);
			} else {
				debug!(target: "sync", "Header {} was not provided ", block.hash);
			}
			return Err(BlockImportError::IncompleteHeader(peer))
		},
	};

	trace!(target: "sync", "Header {} has {:?} logs", block.hash, header.digest().logs().len());

	let number = header.number().clone();
	let hash = header.hash();
	let parent = header.parent_hash().clone();

	let import_error = |e| {
		match e {
			Ok(ImportResult::AlreadyInChain) => {
				trace!(target: "sync", "Block already in chain {}: {:?}", number, hash);
				Ok(BlockImportResult::ImportedKnown(number))
			},
			Ok(ImportResult::Imported(aux)) => Ok(BlockImportResult::ImportedUnknown(number, aux, peer.clone())),
			Ok(ImportResult::UnknownParent) => {
				debug!(target: "sync", "Block with unknown parent {}: {:?}, parent: {:?}", number, hash, parent);
				Err(BlockImportError::UnknownParent)
			},
			Ok(ImportResult::KnownBad) => {
				debug!(target: "sync", "Peer gave us a bad block {}: {:?}", number, hash);
				Err(BlockImportError::BadBlock(peer.clone()))
			},
			Err(e) => {
				debug!(target: "sync", "Error importing block {}: {:?}: {:?}", number, hash, e);
				Err(BlockImportError::Error)
			}
		}
	};

	match import_error(import_handle.check_block(hash, parent))? {
		BlockImportResult::ImportedUnknown { .. } => (),
		r => return Ok(r), // Any other successful result means that the block is already imported.
	}

	let (import_block, maybe_keys) = verifier.verify(block_origin, header, justification, block.body)
		.map_err(|msg| {
			if let Some(ref peer) = peer {
				trace!(target: "sync", "Verifying {}({}) from {} failed: {}", number, hash, peer, msg);
			} else {
				trace!(target: "sync", "Verifying {}({}) failed: {}", number, hash, msg);
			}
			BlockImportError::VerificationFailed(peer.clone(), msg)
		})?;

	let mut cache = HashMap::new();
	if let Some(keys) = maybe_keys {
		cache.extend(keys.into_iter());
	}

	import_error(import_handle.import_block(import_block, cache))
}
