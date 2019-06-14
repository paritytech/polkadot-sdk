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

use std::{sync::Arc, thread, collections::HashMap};
use crossbeam_channel::{self as channel, Receiver, Sender};
use parking_lot::Mutex;
use runtime_primitives::{Justification, traits::{
	Block as BlockT, Header as HeaderT, NumberFor,
}};
use crate::{error::Error as ConsensusError, well_known_cache_keys::Id as CacheKeyId, block_import::{
	BlockImport, BlockOrigin, ImportBlock, ImportedAux, ImportResult, JustificationImport,
	FinalityProofImport, FinalityProofRequestBuilder,
}};

/// Reputation change for peers which send us a block with an incomplete header.
const INCOMPLETE_HEADER_REPUTATION_CHANGE: i32 = -(1 << 20);
/// Reputation change for peers which send us a block which we fail to verify.
const VERIFICATION_FAIL_REPUTATION_CHANGE: i32 = -(1 << 20);
/// Reputation change for peers which send us a bad block.
const BAD_BLOCK_REPUTATION_CHANGE: i32 = -(1 << 29);
/// Reputation change for peers which send us a block with bad justifications.
const BAD_JUSTIFICATION_REPUTATION_CHANGE: i32 = -(1 << 16);

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
pub trait ImportQueue<B: BlockT>: Send + Sync {
	/// Start background work for the queue as necessary.
	///
	/// This is called automatically by the network service when synchronization
	/// begins.
	fn start(&self, _link: Box<dyn Link<B>>) -> Result<(), std::io::Error> {
		Ok(())
	}
	/// Import bunch of blocks.
	fn import_blocks(&self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>);
	/// Import a block justification.
	fn import_justification(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, justification: Justification);
	/// Import block finality proof.
	fn import_finality_proof(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, finality_proof: Vec<u8>);
}

/// Basic block import queue that performs import in the caller thread.
pub struct BasicSyncQueue<B: BlockT, V: Verifier<B>> {
	data: Arc<BasicSyncQueueData<B, V>>,
}

struct BasicSyncQueueData<B: BlockT, V: Verifier<B>> {
	link: Mutex<Option<Box<dyn Link<B>>>>,
	block_import: SharedBlockImport<B>,
	verifier: Arc<V>,
	justification_import: Option<SharedJustificationImport<B>>,
	finality_proof_import: Option<SharedFinalityProofImport<B>>,
}

impl<B: BlockT, V: Verifier<B>> BasicSyncQueue<B, V> {
	pub fn new(
		block_import: SharedBlockImport<B>,
		verifier: Arc<V>,
		justification_import: Option<SharedJustificationImport<B>>,
		finality_proof_import: Option<SharedFinalityProofImport<B>>,
	) -> Self {
		BasicSyncQueue {
			data: Arc::new(BasicSyncQueueData {
				link: Mutex::new(None),
				block_import,
				verifier,
				justification_import,
				finality_proof_import,
			}),
		}
	}
}

impl<B: BlockT, V: 'static + Verifier<B>> ImportQueue<B> for BasicSyncQueue<B, V> {
	fn start(&self, link: Box<dyn Link<B>>) -> Result<(), std::io::Error> {
		if let Some(justification_import) = self.data.justification_import.as_ref() {
			justification_import.on_start(&*link);
		}
		*self.data.link.lock() = Some(link);
		Ok(())
	}

	fn import_blocks(&self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>) {
		if blocks.is_empty() {
			return;
		}

		let (imported, count, results) = import_many_blocks(
			&*self.data.block_import,
			origin,
			blocks,
			self.data.verifier.clone(),
		);

		let link_ref = self.data.link.lock();
		let link = match link_ref.as_ref() {
			Some(link) => link,
			None => {
				trace!(target: "sync", "Trying to import blocks before starting import queue");
				return;
			},
		};

		process_import_results(&**link, results);

		trace!(target: "sync", "Imported {} of {}", imported, count);
	}

	fn import_justification(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, justification: Justification) {
		import_single_justification(
			&*self.data.link.lock(),
			&self.data.justification_import,
			who,
			hash,
			number,
			justification,
		)
	}

	fn import_finality_proof(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, finality_proof: Vec<u8>) {
		let result = import_single_finality_proof(
			&self.data.finality_proof_import,
			&*self.data.verifier,
			&who,
			hash,
			number,
			finality_proof,
		);
		if let Some(link) = self.data.link.lock().as_ref() {
			link.finality_proof_imported(who, (hash, number), result);
		}
	}
}

/// Interface to a basic block import queue that is importing blocks
/// sequentially in a separate thread, with pluggable verification.
#[derive(Clone)]
pub struct BasicQueue<B: BlockT> {
	sender: Option<Sender<BlockImportMsg<B>>>,
}

impl<B: BlockT> Drop for BasicQueue<B> {
	fn drop(&mut self) {
		if let Some(sender) = self.sender.take() {
			let (shutdown_sender, shutdown_receiver) = channel::unbounded();
			if sender.send(BlockImportMsg::Shutdown(shutdown_sender)).is_ok() {
				let _ = shutdown_receiver.recv();
			}
		}
	}
}

/// "BasicQueue" is a wrapper around a channel sender to the "BlockImporter".
/// "BasicQueue" itself does not keep any state or do any importing work, and
/// can therefore be send to other threads.
///
/// "BasicQueue" implements "ImportQueue" by sending messages to the
/// "BlockImporter", which runs in it's own thread.
///
/// The "BlockImporter" is responsible for handling incoming requests from the
/// "BasicQueue". Some of these requests are handled by the "BlockImporter"
/// itself, such as "is_importing", "status", and justifications.
///
/// The "import block" work will be offloaded to a single "BlockImportWorker",
/// running in another thread. Offloading the work is done via a channel,
/// ensuring blocks in this implementation are imported sequentially and in
/// order (as received by the "BlockImporter").
///
/// As long as the "BasicQueue" is not dropped, the "BlockImporter" will keep
/// running. The "BlockImporter" owns a sender to the "BlockImportWorker",
/// ensuring that the worker is kept alive until that sender is dropped.
impl<B: BlockT> BasicQueue<B> {
	/// Instantiate a new basic queue, with given verifier.
	pub fn new<V: 'static + Verifier<B>>(
		verifier: Arc<V>,
		block_import: SharedBlockImport<B>,
		justification_import: Option<SharedJustificationImport<B>>,
		finality_proof_import: Option<SharedFinalityProofImport<B>>,
		finality_proof_request_builder: Option<SharedFinalityProofRequestBuilder<B>>,
	) -> Self {
		let (result_sender, result_port) = channel::unbounded();
		let worker_sender = BlockImportWorker::new(
			result_sender,
			verifier.clone(),
			block_import,
			finality_proof_import.clone(),
		);
		let importer_sender = BlockImporter::new(
			result_port,
			worker_sender,
			verifier,
			justification_import,
			finality_proof_import,
			finality_proof_request_builder,
		);

		Self {
			sender: Some(importer_sender),
		}
	}

	/// Send synchronization request to the block import channel.
	///
	/// The caller should wait for Link::synchronized() call to ensure that it
	/// has synchronized with ImportQueue.
	#[cfg(any(test, feature = "test-helpers"))]
	pub fn synchronize(&self) {
		if let Some(ref sender) = self.sender {
			let _ = sender.send(BlockImportMsg::Synchronize);
		}
	}
}

impl<B: BlockT> ImportQueue<B> for BasicQueue<B> {
	fn start(&self, link: Box<dyn Link<B>>) -> Result<(), std::io::Error> {
		let connect_err = || Err(std::io::Error::new(
			std::io::ErrorKind::Other,
			"Failed to connect import queue threads",
		));
		if let Some(ref sender) = self.sender {
			let (start_sender, start_port) = channel::unbounded();
			let _ = sender.send(BlockImportMsg::Start(link, start_sender));
			start_port.recv().unwrap_or_else(|_| connect_err())
		} else {
			connect_err()
		}
	}

	fn import_blocks(&self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>) {
		if blocks.is_empty() {
			return;
		}

		if let Some(ref sender) = self.sender {
			let _ = sender.send(BlockImportMsg::ImportBlocks(origin, blocks));
		}
	}

	fn import_justification(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, justification: Justification) {
		if let Some(ref sender) = self.sender {
			let _ = sender.send(BlockImportMsg::ImportJustification(who.clone(), hash, number, justification));
		}
	}

	fn import_finality_proof(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, finality_proof: Vec<u8>) {
		if let Some(ref sender) = self.sender {
			let _ = sender.send(BlockImportMsg::ImportFinalityProof(who, hash, number, finality_proof));
		}
	}
}

pub enum BlockImportMsg<B: BlockT> {
	ImportBlocks(BlockOrigin, Vec<IncomingBlock<B>>),
	ImportJustification(Origin, B::Hash, NumberFor<B>, Justification),
	ImportFinalityProof(Origin, B::Hash, NumberFor<B>, Vec<u8>),
	Start(Box<dyn Link<B>>, Sender<Result<(), std::io::Error>>),
	Shutdown(Sender<()>),
	#[cfg(any(test, feature = "test-helpers"))]
	Synchronize,
}

#[cfg_attr(test, derive(Debug))]
pub enum BlockImportWorkerMsg<B: BlockT> {
	ImportBlocks(BlockOrigin, Vec<IncomingBlock<B>>),
	ImportedBlocks(
		Vec<(
			Result<BlockImportResult<NumberFor<B>>, BlockImportError>,
			B::Hash,
		)>,
	),
	ImportFinalityProof(Origin, B::Hash, NumberFor<B>, Vec<u8>),
	ImportedFinalityProof(Origin, (B::Hash, NumberFor<B>), Result<(B::Hash, NumberFor<B>), ()>),
	Shutdown(Sender<()>),
	#[cfg(any(test, feature = "test-helpers"))]
	Synchronize,
}

enum ImportMsgType<B: BlockT> {
	FromWorker(BlockImportWorkerMsg<B>),
	FromNetwork(BlockImportMsg<B>),
}

struct BlockImporter<B: BlockT> {
	port: Receiver<BlockImportMsg<B>>,
	result_port: Receiver<BlockImportWorkerMsg<B>>,
	worker_sender: Option<Sender<BlockImportWorkerMsg<B>>>,
	link: Option<Box<dyn Link<B>>>,
	verifier: Arc<dyn Verifier<B>>,
	justification_import: Option<SharedJustificationImport<B>>,
	finality_proof_import: Option<SharedFinalityProofImport<B>>,
	finality_proof_request_builder: Option<SharedFinalityProofRequestBuilder<B>>,
}

impl<B: BlockT> BlockImporter<B> {
	fn new(
		result_port: Receiver<BlockImportWorkerMsg<B>>,
		worker_sender: Sender<BlockImportWorkerMsg<B>>,
		verifier: Arc<dyn Verifier<B>>,
		justification_import: Option<SharedJustificationImport<B>>,
		finality_proof_import: Option<SharedFinalityProofImport<B>>,
		finality_proof_request_builder: Option<SharedFinalityProofRequestBuilder<B>>,
	) -> Sender<BlockImportMsg<B>> {
		trace!(target: "block_import", "Creating new Block Importer!");
		let (sender, port) = channel::bounded(4);
		let _ = thread::Builder::new()
			.name("ImportQueue".into())
			.spawn(move || {
				let mut importer = BlockImporter {
					port,
					result_port,
					worker_sender: Some(worker_sender),
					link: None,
					verifier,
					justification_import,
					finality_proof_import,
					finality_proof_request_builder,
				};
				while importer.run() {
					// Importing until all senders have been dropped...
				}
			})
			.expect("ImportQueue thread spawning failed");
		sender
	}

	fn run(&mut self) -> bool {
		trace!(target: "import_queue", "Running import queue");
		let msg = select! {
			recv(self.port) -> msg => {
				match msg {
					// Our sender has been dropped, quitting.
					Err(_) => return false,
					Ok(msg) => ImportMsgType::FromNetwork(msg)
				}
			},
			recv(self.result_port) -> msg => {
				match msg {
					Err(_) => unreachable!("1. We hold a sender to the Worker, 2. it should not quit until that sender is dropped; qed"),
					Ok(msg) => ImportMsgType::FromWorker(msg),
				}
			}
		};
		match msg {
			ImportMsgType::FromNetwork(msg) => self.handle_network_msg(msg),
			ImportMsgType::FromWorker(msg) => self.handle_worker_msg(msg),
		}
	}

	fn handle_network_msg(&mut self, msg: BlockImportMsg<B>) -> bool {
		match msg {
			BlockImportMsg::ImportBlocks(origin, incoming_blocks) => {
				self.handle_import_blocks(origin, incoming_blocks)
			},
			BlockImportMsg::ImportJustification(who, hash, number, justification) => {
				import_single_justification(
					&self.link,
					&self.justification_import,
					who,
					hash,
					number,
					justification,
				);
			},
			BlockImportMsg::ImportFinalityProof(who, hash, number, finality_proof) => {
				self.handle_import_finality_proof(who, hash, number, finality_proof)
			},
			BlockImportMsg::Start(link, sender) => {
				if let Some(finality_proof_request_builder) = self.finality_proof_request_builder.take() {
					link.set_finality_proof_request_builder(finality_proof_request_builder);
				}
				if let Some(justification_import) = self.justification_import.as_ref() {
					justification_import.on_start(&*link);
				}
				if let Some(finality_proof_import) = self.finality_proof_import.as_ref() {
					finality_proof_import.on_start(&*link);
				}
				self.link = Some(link);
				let _ = sender.send(Ok(()));
			},
			BlockImportMsg::Shutdown(result_sender) => {
				// stop worker thread
				if let Some(worker_sender) = self.worker_sender.take() {
					let (sender, receiver) = channel::unbounded();
					if worker_sender.send(BlockImportWorkerMsg::Shutdown(sender)).is_ok() {
						let _ = receiver.recv();
					}
				}
				// send shutdown notification
				let _ = result_sender.send(());
				return false;
			},
			#[cfg(any(test, feature = "test-helpers"))]
			BlockImportMsg::Synchronize => {
				trace!(target: "sync", "Received synchronization message");
				if let Some(ref worker_sender) = self.worker_sender {
					let _ = worker_sender.send(BlockImportWorkerMsg::Synchronize);
				}
			},
		}
		true
	}

	fn handle_worker_msg(&mut self, msg: BlockImportWorkerMsg<B>) -> bool {
		let link = match self.link.as_ref() {
			Some(link) => link,
			None => {
				trace!(target: "sync", "Received import result while import-queue has no link");
				return true;
			},
		};

		let results = match msg {
			BlockImportWorkerMsg::ImportedBlocks(results) => (results),
			BlockImportWorkerMsg::ImportedFinalityProof(who, request_block, finalization_result) => {
				link.finality_proof_imported(who, request_block, finalization_result);
				return true;
			},
			#[cfg(any(test, feature = "test-helpers"))]
			BlockImportWorkerMsg::Synchronize => {
				trace!(target: "sync", "Synchronizing link");
				link.synchronized();
				return true;
			},
			BlockImportWorkerMsg::ImportBlocks(_, _)
				| BlockImportWorkerMsg::ImportFinalityProof(_, _, _, _)
				| BlockImportWorkerMsg::Shutdown(_)
					=> unreachable!("Import Worker does not send Import*/Shutdown messages; qed"),
		};

		process_import_results(&**link, results);
		true
	}

	fn handle_import_finality_proof(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, finality_proof: Vec<u8>) {
		if let Some(ref worker_sender) = self.worker_sender {
			trace!(target: "sync", "Scheduling finality proof of {}/{} for import", number, hash);
			let _ = worker_sender.send(BlockImportWorkerMsg::ImportFinalityProof(who, hash, number, finality_proof));
		}
	}

	fn handle_import_blocks(&mut self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>) {
		if let Some(ref worker_sender) = self.worker_sender {
			trace!(target: "sync", "Scheduling {} blocks for import", blocks.len());
			let _ = worker_sender.send(BlockImportWorkerMsg::ImportBlocks(origin, blocks));
		}
	}
}

struct BlockImportWorker<B: BlockT, V: Verifier<B>> {
	result_sender: Sender<BlockImportWorkerMsg<B>>,
	block_import: SharedBlockImport<B>,
	finality_proof_import: Option<SharedFinalityProofImport<B>>,
	verifier: Arc<V>,
}

impl<B: BlockT, V: 'static + Verifier<B>> BlockImportWorker<B, V> {
	pub fn new(
		result_sender: Sender<BlockImportWorkerMsg<B>>,
		verifier: Arc<V>,
		block_import: SharedBlockImport<B>,
		finality_proof_import: Option<SharedFinalityProofImport<B>>,
	) -> Sender<BlockImportWorkerMsg<B>> {
		let (sender, port) = channel::bounded(4);
		let _ = thread::Builder::new()
			.name("ImportQueueWorker".into())
			.spawn(move || {
				let worker = BlockImportWorker {
					result_sender,
					verifier,
					block_import,
					finality_proof_import,
				};
				for msg in port.iter() {
					// Working until all senders have been dropped...
					match msg {
						BlockImportWorkerMsg::ImportBlocks(origin, blocks) => {
							worker.import_a_batch_of_blocks(origin, blocks);
						},
						BlockImportWorkerMsg::ImportFinalityProof(who, hash, number, proof) => {
							worker.import_finality_proof(who, hash, number, proof);
						},
						BlockImportWorkerMsg::Shutdown(result_sender) => {
							let _ = result_sender.send(());
							break;
						},
						#[cfg(any(test, feature = "test-helpers"))]
						BlockImportWorkerMsg::Synchronize => {
							trace!(target: "sync", "Sending sync message");
							let _ = worker.result_sender.send(BlockImportWorkerMsg::Synchronize);
						},
						BlockImportWorkerMsg::ImportedBlocks(_)
							| BlockImportWorkerMsg::ImportedFinalityProof(_, _, _)
								=> unreachable!("Import Worker does not receive the Imported* messages; qed"),
					}
				}
			})
			.expect("ImportQueueWorker thread spawning failed");
		sender
	}

	fn import_a_batch_of_blocks(&self, origin: BlockOrigin, blocks: Vec<IncomingBlock<B>>) {
		let (imported, count, results) = import_many_blocks(
			&*self.block_import,
			origin,
			blocks,
			self.verifier.clone(),
		);

		let _ = self
			.result_sender
			.send(BlockImportWorkerMsg::ImportedBlocks(results));

		trace!(target: "sync", "Imported {} of {}", imported, count);
	}

	fn import_finality_proof(&self, who: Origin, hash: B::Hash, number: NumberFor<B>, finality_proof: Vec<u8>) {
		let result = import_single_finality_proof(
			&self.finality_proof_import,
			&*self.verifier,
			&who,
			hash,
			number,
			finality_proof,
		);

		let _ = self
			.result_sender
			.send(BlockImportWorkerMsg::ImportedFinalityProof(who, (hash, number), result));
	}
}

/// Hooks that the verification queue can use to influence the synchronization
/// algorithm.
pub trait Link<B: BlockT>: Send {
	/// Block imported.
	fn block_imported(&self, _hash: &B::Hash, _number: NumberFor<B>) {}
	/// Batch of blocks imported, with or without error.
	fn blocks_processed(&self, _processed_blocks: Vec<B::Hash>, _has_error: bool) {}
	/// Justification import result.
	fn justification_imported(&self, _who: Origin, _hash: &B::Hash, _number: NumberFor<B>, _success: bool) {}
	/// Clear all pending justification requests.
	fn clear_justification_requests(&self) {}
	/// Request a justification for the given block.
	fn request_justification(&self, _hash: &B::Hash, _number: NumberFor<B>) {}
	/// Finality proof import result.
	///
	/// Even though we have asked for finality proof of block A, provider could return proof of
	/// some earlier block B, if the proof for A was too large. The sync module should continue
	/// asking for proof of A in this case.
	fn finality_proof_imported(
		&self,
		_who: Origin,
		_request_block: (B::Hash, NumberFor<B>),
		_finalization_result: Result<(B::Hash, NumberFor<B>), ()>,
	) {}
	/// Request a finality proof for the given block.
	fn request_finality_proof(&self, _hash: &B::Hash, _number: NumberFor<B>) {}
	/// Remember finality proof request builder on start.
	fn set_finality_proof_request_builder(&self, _request_builder: SharedFinalityProofRequestBuilder<B>) {}
	/// Adjusts the reputation of the given peer.
	fn report_peer(&self, _who: Origin, _reputation_change: i32) {}
	/// Restart sync.
	fn restart(&self) {}
	/// Synchronization request has been processed.
	#[cfg(any(test, feature = "test-helpers"))]
	fn synchronized(&self) {}
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

/// Imports single notification and send notification to the link (if provided).
fn import_single_justification<B: BlockT>(
	link: &Option<Box<dyn Link<B>>>,
	justification_import: &Option<SharedJustificationImport<B>>,
	who: Origin,
	hash: B::Hash,
	number: NumberFor<B>,
	justification: Justification,
) {
	let success = justification_import.as_ref().map(|justification_import| {
		justification_import.import_justification(hash, number, justification)
			.map_err(|e| {
				debug!(
					target: "sync",
					"Justification import failed with {:?} for hash: {:?} number: {:?} coming from node: {:?}",
					e,
					hash,
					number,
					who,
				);
				e
			}).is_ok()
	}).unwrap_or(false);

	if let Some(ref link) = link {
		link.justification_imported(who, &hash, number, success);
	}
}

/// Imports single finality_proof.
fn import_single_finality_proof<B: BlockT, V: Verifier<B>>(
	finality_proof_import: &Option<SharedFinalityProofImport<B>>,
	verifier: &V,
	who: &Origin,
	hash: B::Hash,
	number: NumberFor<B>,
	finality_proof: Vec<u8>,
) -> Result<(B::Hash, NumberFor<B>), ()> {
	let result = finality_proof_import.as_ref().map(|finality_proof_import| {
		finality_proof_import.import_finality_proof(hash, number, finality_proof, verifier)
			.map_err(|e| {
				debug!(
					"Finality proof import failed with {:?} for hash: {:?} number: {:?} coming from node: {:?}",
					e,
					hash,
					number,
					who,
				);
			})
	}).unwrap_or(Err(()));

	trace!(target: "sync", "Imported finality proof for {}/{}", number, hash);

	result
}

/// Process result of block(s) import.
fn process_import_results<B: BlockT>(
	link: &dyn Link<B>,
	results: Vec<(
		Result<BlockImportResult<NumberFor<B>>, BlockImportError>,
		B::Hash,
	)>,
)
{
	let mut has_error = false;
	let mut hashes = vec![];
	for (result, hash) in results {
		hashes.push(hash);

		if has_error {
			continue;
		}

		if result.is_err() {
			has_error = true;
		}

		match result {
			Ok(BlockImportResult::ImportedKnown(number)) => link.block_imported(&hash, number),
			Ok(BlockImportResult::ImportedUnknown(number, aux, who)) => {
				link.block_imported(&hash, number);

				if aux.clear_justification_requests {
					trace!(target: "sync", "Block imported clears all pending justification requests {}: {:?}", number, hash);
					link.clear_justification_requests();
				}

				if aux.needs_justification {
					trace!(target: "sync", "Block imported but requires justification {}: {:?}", number, hash);
					link.request_justification(&hash, number);
				}

				if aux.bad_justification {
					if let Some(peer) = who {
						info!("Sent block with bad justification to import");
						link.report_peer(peer, BAD_JUSTIFICATION_REPUTATION_CHANGE);
					}
				}

				if aux.needs_finality_proof {
					trace!(target: "sync", "Block imported but requires finality proof {}: {:?}", number, hash);
					link.request_finality_proof(&hash, number);
				}
			},
			Err(BlockImportError::IncompleteHeader(who)) => {
				if let Some(peer) = who {
					info!("Peer sent block with incomplete header to import");
					link.report_peer(peer, INCOMPLETE_HEADER_REPUTATION_CHANGE);
					link.restart();
				}
			},
			Err(BlockImportError::VerificationFailed(who, e)) => {
				if let Some(peer) = who {
					info!("Verification failed from peer: {}", e);
					link.report_peer(peer, VERIFICATION_FAIL_REPUTATION_CHANGE);
					link.restart();
				}
			},
			Err(BlockImportError::BadBlock(who)) => {
				if let Some(peer) = who {
					info!("Bad block");
					link.report_peer(peer, BAD_BLOCK_REPUTATION_CHANGE);
					link.restart();
				}
			},
			Err(BlockImportError::UnknownParent) | Err(BlockImportError::Error) => {
				link.restart();
			},
		};
	}
	link.blocks_processed(hashes, has_error);
}

/// Import several blocks at once, returning import result for each block.
fn import_many_blocks<B: BlockT, V: Verifier<B>>(
	import_handle: &dyn BlockImport<B, Error = ConsensusError>,
	blocks_origin: BlockOrigin,
	blocks: Vec<IncomingBlock<B>>,
	verifier: Arc<V>,
) -> (usize, usize, Vec<(
	Result<BlockImportResult<NumberFor<B>>, BlockImportError>,
	B::Hash,
)>) {
	let count = blocks.len();
	let mut imported = 0;

	let blocks_range = match (
		blocks.first().and_then(|b| b.header.as_ref().map(|h| h.number())),
		blocks.last().and_then(|b| b.header.as_ref().map(|h| h.number())),
	) {
		(Some(first), Some(last)) if first != last => format!(" ({}..{})", first, last),
		(Some(first), Some(_)) => format!(" ({})", first),
		_ => Default::default(),
	};

	trace!(target: "sync", "Starting import of {} blocks {}", count, blocks_range);

	let mut results = vec![];

	let mut has_error = false;

	// Blocks in the response/drain should be in ascending order.
	for block in blocks {
		let block_hash = block.hash;
		let import_result = if has_error {
			Err(BlockImportError::Error)
		} else {
			import_single_block(
				import_handle,
				blocks_origin.clone(),
				block,
				verifier.clone(),
			)
		};
		let was_ok = import_result.is_ok();
		results.push((import_result, block_hash));
		if was_ok {
			imported += 1;
		} else {
			has_error = true;
		}
	}

	(imported, count, results)
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

#[cfg(test)]
mod tests {
	use super::*;
	use crate::block_import::ForkChoiceStrategy;
	use libp2p::PeerId;
	use test_client::runtime::{Block, Hash};

	#[derive(Debug, PartialEq)]
	enum LinkMsg {
		BlockImported,
		FinalityProofImported,
		Disconnected,
		Restarted,
	}

	#[derive(Clone)]
	struct TestLink {
		sender: Sender<LinkMsg>,
	}

	impl TestLink {
		fn new(sender: Sender<LinkMsg>) -> TestLink {
			TestLink {
				sender,
			}
		}
	}

	impl Link<Block> for TestLink {
		fn block_imported(&self, _hash: &Hash, _number: NumberFor<Block>) {
			let _ = self.sender.send(LinkMsg::BlockImported);
		}
		fn finality_proof_imported(
			&self,
			_: Origin,
			_: (Hash, NumberFor<Block>),
			_: Result<(Hash, NumberFor<Block>), ()>,
		) {
			let _ = self.sender.send(LinkMsg::FinalityProofImported);
		}
		fn report_peer(&self, _: Origin, _: i32) {
			let _ = self.sender.send(LinkMsg::Disconnected);
		}
		fn restart(&self) {
			let _ = self.sender.send(LinkMsg::Restarted);
		}
	}

	impl<B: BlockT> Verifier<B> for () {
		fn verify(
			&self,
			origin: BlockOrigin,
			header: B::Header,
			justification: Option<Justification>,
			body: Option<Vec<B::Extrinsic>>,
		) -> Result<(ImportBlock<B>, Option<Vec<(CacheKeyId, Vec<u8>)>>), String> {
			Ok((ImportBlock {
				origin,
				header,
				body,
				finalized: false,
				justification,
				post_digests: vec![],
				auxiliary: Vec::new(),
				fork_choice: ForkChoiceStrategy::LongestChain,
			}, None))
		}
	}

	#[test]
	fn process_import_result_works() {
		let (result_sender, result_port) = channel::unbounded();
		let (worker_sender, _) = channel::unbounded();
		let (link_sender, link_port) = channel::unbounded();
		let importer_sender = BlockImporter::<Block>::new(result_port, worker_sender, Arc::new(()), None, None, None);
		let link = TestLink::new(link_sender);
		let (ack_sender, start_ack_port) = channel::bounded(4);
		let _ = importer_sender.send(BlockImportMsg::Start(Box::new(link.clone()), ack_sender));

		// Ensure the importer handles Start before any result messages.
		let _ = start_ack_port.recv();

		// Send a known
		let results = vec![(Ok(BlockImportResult::ImportedKnown(Default::default())), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::BlockImported));

		// Send a second known
		let results = vec![(Ok(BlockImportResult::ImportedKnown(Default::default())), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::BlockImported));

		// Send an unknown
		let results = vec![(Ok(BlockImportResult::ImportedUnknown(Default::default(), Default::default(), None)), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::BlockImported));

		// Send an unknown with peer and bad justification
		let peer_id = PeerId::random();
		let results = vec![(Ok(BlockImportResult::ImportedUnknown(Default::default(),
			ImportedAux {
				needs_justification: true,
				clear_justification_requests: false,
				bad_justification: true,
				needs_finality_proof: false,
			},
			Some(peer_id.clone()))), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::BlockImported));
		assert_eq!(link_port.recv(), Ok(LinkMsg::Disconnected));

		// Send an incomplete header
		let results = vec![(Err(BlockImportError::IncompleteHeader(Some(peer_id.clone()))), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::Disconnected));
		assert_eq!(link_port.recv(), Ok(LinkMsg::Restarted));

		// Send an unknown parent
		let results = vec![(Err(BlockImportError::UnknownParent), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::Restarted));

		// Send a verification failed
		let results = vec![(Err(BlockImportError::VerificationFailed(Some(peer_id.clone()), String::new())), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::Disconnected));
		assert_eq!(link_port.recv(), Ok(LinkMsg::Restarted));

		// Send an error
		let results = vec![(Err(BlockImportError::Error), Default::default())];
		let _ = result_sender.send(BlockImportWorkerMsg::ImportedBlocks(results)).ok().unwrap();
		assert_eq!(link_port.recv(), Ok(LinkMsg::Restarted));

		// Drop the importer sender first, ensuring graceful shutdown.
		drop(importer_sender);
	}

	#[test]
	fn process_finality_proof_import_result_works() {
		let (result_sender, result_port) = channel::unbounded();
		let (worker_sender, worker_receiver) = channel::unbounded();
		let (link_sender, link_port) = channel::unbounded();
		let importer_sender = BlockImporter::<Block>::new(result_port, worker_sender, Arc::new(()), None, None, None);
		let link = TestLink::new(link_sender);
		let (ack_sender, start_ack_port) = channel::bounded(4);
		let _ = importer_sender.send(BlockImportMsg::Start(Box::new(link.clone()), ack_sender));
		let who = Origin::random();

		// Ensure the importer handles Start before any result messages.
		start_ack_port.recv().unwrap().unwrap();

		// Send finality proof import request to BlockImporter
		importer_sender.send(BlockImportMsg::ImportFinalityProof(
			who.clone(),
			Default::default(),
			1,
			vec![42],
		)).unwrap();

		// Wait until this request is redirected to the BlockImportWorker
		match worker_receiver.recv().unwrap() {
			BlockImportWorkerMsg::ImportFinalityProof(
				cwho,
				chash,
				1,
				cproof,
			) => {
				assert_eq!(cwho, who);
				assert_eq!(chash, Default::default());
				assert_eq!(cproof, vec![42]);
			},
			_ => unreachable!("Unexpected work request received"),
		}

		// Send ack of proof import from BlockImportWorker to BlockImporter
		result_sender.send(BlockImportWorkerMsg::ImportedFinalityProof(
			who.clone(),
			(Default::default(), 0),
			Ok((Default::default(), 0)),
		)).unwrap();

		// Wait for finality proof import result
		assert_eq!(link_port.recv(), Ok(LinkMsg::FinalityProofImported));

		// Drop the importer sender first, ensuring graceful shutdown.
		drop(importer_sender);
	}
}

