// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Parachain PoV recovery
//!
//! A parachain needs to build PoVs that are send to the relay chain to progress. These PoVs are
//! erasure encoded and one piece of it is stored by each relay chain validator. As the relay chain
//! decides on which PoV per parachain to include and thus, to progress the parachain it can happen
//! that the block corresponding to this PoV isn't propagated in the parachain network. This can
//! have several reasons, either a malicious collator that managed to include its own PoV and
//! doesn't want to share it with the rest of the network or maybe a collator went down before it
//! could distribute the block in the network. When something like this happens we can use the PoV
//! recovery algorithm implemented in this crate to recover a PoV and to propagate it with the rest
//! of the network.
//!
//! It works in the following way:
//!
//! 1. For every included relay chain block we note the backed candidate of our parachain. If the
//!    block belonging to the PoV is already known, we do nothing. Otherwise we start a timer that
//!    waits for a randomized time inside a specified interval before starting to
//! recover    the PoV.
//!
//! 2. If between starting and firing the timer the block is imported, we skip the recovery of the
//!    PoV.
//!
//! 3. If the timer fired we recover the PoV using the relay chain PoV recovery protocol.
//!
//! 4a. After it is recovered, we restore the block and import it.
//!
//! 4b. Since we are trying to recover pending candidates, availability is not guaranteed. If the
//! block     PoV is not yet available, we retry.
//!
//! If we need to recover multiple PoV blocks (which should hopefully not happen in real life), we
//! make sure that the blocks are imported in the correct order.

use sc_client_api::{BlockBackend, BlockchainEvents, UsageProvider};
use sc_consensus::import_queue::{ImportQueueService, IncomingBlock};
use sp_api::RuntimeApiInfo;
use sp_consensus::{BlockOrigin, BlockStatus, SyncOracle};
use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};

use polkadot_node_primitives::{PoV, POV_BOMB_LIMIT};
use polkadot_node_subsystem::messages::{AvailabilityRecoveryMessage, RuntimeApiRequest};
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{
	vstaging::{
		CandidateReceiptV2 as CandidateReceipt,
		CommittedCandidateReceiptV2 as CommittedCandidateReceipt,
	},
	Id as ParaId, SessionIndex,
};

use cumulus_primitives_core::ParachainBlockData;
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};

use codec::Decode;
use futures::{
	channel::mpsc::Receiver, select, stream::FuturesUnordered, Future, FutureExt, Stream, StreamExt,
};
use futures_timer::Delay;
use rand::{distributions::Uniform, prelude::Distribution, thread_rng};

use std::{
	collections::{HashMap, HashSet, VecDeque},
	pin::Pin,
	sync::Arc,
	time::Duration,
};

#[cfg(test)]
mod tests;

mod active_candidate_recovery;
use active_candidate_recovery::ActiveCandidateRecovery;

const LOG_TARGET: &str = "cumulus-pov-recovery";

/// Test-friendly wrapper trait for the overseer handle.
/// Can be used to simulate failing recovery requests.
#[async_trait::async_trait]
pub trait RecoveryHandle: Send {
	async fn send_recovery_msg(
		&mut self,
		message: AvailabilityRecoveryMessage,
		origin: &'static str,
	);
}

#[async_trait::async_trait]
impl RecoveryHandle for OverseerHandle {
	async fn send_recovery_msg(
		&mut self,
		message: AvailabilityRecoveryMessage,
		origin: &'static str,
	) {
		self.send_msg(message, origin).await;
	}
}

/// Type of recovery to trigger.
#[derive(Debug, PartialEq)]
pub enum RecoveryKind {
	/// Single block recovery.
	Simple,
	/// Full ancestry recovery.
	Full,
}

/// Structure used to trigger an explicit recovery request via `PoVRecovery`.
pub struct RecoveryRequest<Block: BlockT> {
	/// Hash of the last block to recover.
	pub hash: Block::Hash,
	/// Recovery type.
	pub kind: RecoveryKind,
}

/// The delay between observing an unknown block and triggering the recovery of a block.
/// Randomizing the start of the recovery within this interval
/// can be used to prevent self-DOSing if the recovery request is part of a
/// distributed protocol and there is the possibility that multiple actors are
/// requiring to perform the recovery action at approximately the same time.
#[derive(Clone, Copy)]
pub struct RecoveryDelayRange {
	/// Start recovering after `min` delay.
	pub min: Duration,
	/// Start recovering before `max` delay.
	pub max: Duration,
}

impl RecoveryDelayRange {
	/// Produce a randomized duration between `min` and `max`.
	fn duration(&self) -> Duration {
		Uniform::from(self.min..=self.max).sample(&mut thread_rng())
	}
}

/// Represents an outstanding block candidate.
struct Candidate<Block: BlockT> {
	receipt: CandidateReceipt,
	session_index: SessionIndex,
	block_number: NumberFor<Block>,
	parent_hash: Block::Hash,
	// Lazy recovery has been submitted.
	// Should be true iff a block is either queued to be recovered or
	// recovery is currently in progress.
	waiting_recovery: bool,
}

/// Queue that is used to decide when to start PoV-recovery operations.
struct RecoveryQueue<Block: BlockT> {
	recovery_delay_range: RecoveryDelayRange,
	// Queue that keeps the hashes of blocks to be recovered.
	recovery_queue: VecDeque<Block::Hash>,
	// Futures that resolve when a new recovery should be started.
	signaling_queue: FuturesUnordered<Pin<Box<dyn Future<Output = ()> + Send>>>,
}

impl<Block: BlockT> RecoveryQueue<Block> {
	pub fn new(recovery_delay_range: RecoveryDelayRange) -> Self {
		Self {
			recovery_delay_range,
			recovery_queue: Default::default(),
			signaling_queue: Default::default(),
		}
	}

	/// Add hash of a block that should go to the end of the recovery queue.
	/// A new recovery will be signaled after `delay` has passed.
	pub fn push_recovery(&mut self, hash: Block::Hash) {
		let delay = self.recovery_delay_range.duration();
		tracing::debug!(
			target: LOG_TARGET,
			block_hash = ?hash,
			"Adding block to queue and adding new recovery slot in {:?} sec",
			delay.as_secs(),
		);
		self.recovery_queue.push_back(hash);
		self.signaling_queue.push(
			async move {
				Delay::new(delay).await;
			}
			.boxed(),
		);
	}

	/// Get the next hash for block recovery.
	pub async fn next_recovery(&mut self) -> Block::Hash {
		loop {
			if self.signaling_queue.next().await.is_some() {
				if let Some(hash) = self.recovery_queue.pop_front() {
					return hash
				} else {
					tracing::error!(
						target: LOG_TARGET,
						"Recovery was signaled, but no candidate hash available. This is a bug."
					);
				};
			}
			futures::pending!()
		}
	}
}

/// Encapsulates the logic of the pov recovery.
pub struct PoVRecovery<Block: BlockT, PC, RC> {
	/// All the pending candidates that we are waiting for to be imported or that need to be
	/// recovered when `next_candidate_to_recover` tells us to do so.
	candidates: HashMap<Block::Hash, Candidate<Block>>,
	/// A stream of futures that resolve to hashes of candidates that need to be recovered.
	///
	/// The candidates to the hashes are stored in `candidates`. If a candidate is not
	/// available anymore in this map, it means that it was already imported.
	candidate_recovery_queue: RecoveryQueue<Block>,
	active_candidate_recovery: ActiveCandidateRecovery<Block>,
	/// Blocks that wait that the parent is imported.
	///
	/// Uses parent -> blocks mapping.
	waiting_for_parent: HashMap<Block::Hash, Vec<Block>>,
	parachain_client: Arc<PC>,
	parachain_import_queue: Box<dyn ImportQueueService<Block>>,
	relay_chain_interface: RC,
	para_id: ParaId,
	/// Explicit block recovery requests channel.
	recovery_chan_rx: Receiver<RecoveryRequest<Block>>,
	/// Blocks that we are retrying currently
	candidates_in_retry: HashSet<Block::Hash>,
	parachain_sync_service: Arc<dyn SyncOracle + Sync + Send>,
}

impl<Block: BlockT, PC, RCInterface> PoVRecovery<Block, PC, RCInterface>
where
	PC: BlockBackend<Block> + BlockchainEvents<Block> + UsageProvider<Block>,
	RCInterface: RelayChainInterface + Clone,
{
	/// Create a new instance.
	pub fn new(
		recovery_handle: Box<dyn RecoveryHandle>,
		recovery_delay_range: RecoveryDelayRange,
		parachain_client: Arc<PC>,
		parachain_import_queue: Box<dyn ImportQueueService<Block>>,
		relay_chain_interface: RCInterface,
		para_id: ParaId,
		recovery_chan_rx: Receiver<RecoveryRequest<Block>>,
		parachain_sync_service: Arc<dyn SyncOracle + Sync + Send>,
	) -> Self {
		Self {
			candidates: HashMap::new(),
			candidate_recovery_queue: RecoveryQueue::new(recovery_delay_range),
			active_candidate_recovery: ActiveCandidateRecovery::new(recovery_handle),
			waiting_for_parent: HashMap::new(),
			parachain_client,
			parachain_import_queue,
			relay_chain_interface,
			para_id,
			candidates_in_retry: HashSet::new(),
			recovery_chan_rx,
			parachain_sync_service,
		}
	}

	/// Handle a new pending candidate.
	fn handle_pending_candidate(
		&mut self,
		receipt: CommittedCandidateReceipt,
		session_index: SessionIndex,
	) {
		let header = match Block::Header::decode(&mut &receipt.commitments.head_data.0[..]) {
			Ok(header) => header,
			Err(e) => {
				tracing::warn!(
					target: LOG_TARGET,
					error = ?e,
					"Failed to decode parachain header from pending candidate",
				);
				return
			},
		};

		if *header.number() <= self.parachain_client.usage_info().chain.finalized_number {
			return
		}

		let hash = header.hash();

		if self.candidates.contains_key(&hash) {
			return
		}

		tracing::debug!(target: LOG_TARGET, block_hash = ?hash, "Adding outstanding candidate");
		self.candidates.insert(
			hash,
			Candidate {
				block_number: *header.number(),
				receipt: receipt.to_plain(),
				session_index,
				parent_hash: *header.parent_hash(),
				waiting_recovery: false,
			},
		);

		// If required, triggers a lazy recovery request that will eventually be blocked
		// if in the meantime the block is imported.
		self.recover(RecoveryRequest { hash, kind: RecoveryKind::Simple });
	}

	/// Block is no longer waiting for recovery
	fn clear_waiting_recovery(&mut self, block_hash: &Block::Hash) {
		if let Some(candidate) = self.candidates.get_mut(block_hash) {
			// Prevents triggering an already enqueued recovery request
			candidate.waiting_recovery = false;
		}
	}

	/// Handle a finalized block with the given `block_number`.
	fn handle_block_finalized(&mut self, block_number: NumberFor<Block>) {
		self.candidates.retain(|_, pc| pc.block_number > block_number);
	}

	/// Recover the candidate for the given `block_hash`.
	async fn recover_candidate(&mut self, block_hash: Block::Hash) {
		match self.candidates.get(&block_hash) {
			Some(candidate) if candidate.waiting_recovery => {
				tracing::debug!(target: LOG_TARGET, ?block_hash, "Issuing recovery request");
				self.active_candidate_recovery.recover_candidate(block_hash, candidate).await;
			},
			_ => (),
		}
	}

	/// Clear `waiting_for_parent` and `waiting_recovery` for the candidate with `hash`.
	/// Also clears children blocks waiting for this parent.
	fn reset_candidate(&mut self, hash: Block::Hash) {
		let mut blocks_to_delete = vec![hash];

		while let Some(delete) = blocks_to_delete.pop() {
			if let Some(children) = self.waiting_for_parent.remove(&delete) {
				blocks_to_delete.extend(children.iter().map(BlockT::hash));
			}
		}
		self.clear_waiting_recovery(&hash);
	}

	/// Handle a recovered candidate.
	async fn handle_candidate_recovered(&mut self, block_hash: Block::Hash, pov: Option<&PoV>) {
		let pov = match pov {
			Some(pov) => {
				self.candidates_in_retry.remove(&block_hash);
				pov
			},
			None =>
				if self.candidates_in_retry.insert(block_hash) {
					tracing::debug!(target: LOG_TARGET, ?block_hash, "Recovery failed, retrying.");
					self.candidate_recovery_queue.push_recovery(block_hash);
					return
				} else {
					tracing::warn!(
						target: LOG_TARGET,
						?block_hash,
						"Unable to recover block after retry.",
					);
					self.candidates_in_retry.remove(&block_hash);
					self.reset_candidate(block_hash);
					return
				},
		};

		let raw_block_data =
			match sp_maybe_compressed_blob::decompress(&pov.block_data.0, POV_BOMB_LIMIT) {
				Ok(r) => r,
				Err(error) => {
					tracing::debug!(target: LOG_TARGET, ?error, "Failed to decompress PoV");

					self.reset_candidate(block_hash);
					return
				},
			};

		let block_data = match ParachainBlockData::<Block>::decode(&mut &raw_block_data[..]) {
			Ok(d) => d,
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?error,
					"Failed to decode parachain block data from recovered PoV",
				);

				self.reset_candidate(block_hash);
				return
			},
		};

		let block = block_data.into_block();

		let parent = *block.header().parent_hash();

		match self.parachain_client.block_status(parent) {
			Ok(BlockStatus::Unknown) => {
				// If the parent block is currently being recovered or is scheduled to be recovered,
				// we want to wait for the parent.
				let parent_scheduled_for_recovery =
					self.candidates.get(&parent).map_or(false, |parent| parent.waiting_recovery);
				if parent_scheduled_for_recovery {
					tracing::debug!(
						target: LOG_TARGET,
						?block_hash,
						parent_hash = ?parent,
						parent_scheduled_for_recovery,
						waiting_blocks = self.waiting_for_parent.len(),
						"Waiting for recovery of parent.",
					);

					self.waiting_for_parent.entry(parent).or_default().push(block);
					return
				} else {
					tracing::debug!(
						target: LOG_TARGET,
						?block_hash,
						parent_hash = ?parent,
						"Parent not found while trying to import recovered block.",
					);

					self.reset_candidate(block_hash);
					return
				}
			},
			Err(error) => {
				tracing::debug!(
					target: LOG_TARGET,
					block_hash = ?parent,
					?error,
					"Error while checking block status",
				);

				self.reset_candidate(block_hash);
				return
			},
			// Any other status is fine to "ignore/accept"
			_ => (),
		}

		self.import_block(block);
	}

	/// Import the given `block`.
	///
	/// This will also recursively drain `waiting_for_parent` and import them as well.
	fn import_block(&mut self, block: Block) {
		let mut blocks = VecDeque::new();

		tracing::debug!(target: LOG_TARGET, block_hash = ?block.hash(), "Importing block retrieved using pov_recovery");
		blocks.push_back(block);

		let mut incoming_blocks = Vec::new();

		while let Some(block) = blocks.pop_front() {
			let block_hash = block.hash();
			let (header, body) = block.deconstruct();

			incoming_blocks.push(IncomingBlock {
				hash: block_hash,
				header: Some(header),
				body: Some(body),
				import_existing: false,
				allow_missing_state: false,
				justifications: None,
				origin: None,
				skip_execution: false,
				state: None,
				indexed_body: None,
			});

			if let Some(waiting) = self.waiting_for_parent.remove(&block_hash) {
				blocks.extend(waiting);
			}
		}

		self.parachain_import_queue
			.import_blocks(BlockOrigin::ConsensusBroadcast, incoming_blocks);
	}

	/// Attempts an explicit recovery of one or more blocks.
	pub fn recover(&mut self, req: RecoveryRequest<Block>) {
		let RecoveryRequest { mut hash, kind } = req;
		let mut to_recover = Vec::new();

		loop {
			let candidate = match self.candidates.get_mut(&hash) {
				Some(candidate) => candidate,
				None => {
					tracing::debug!(
						target: LOG_TARGET,
						block_hash = ?hash,
						"Could not recover. Block was never announced as candidate"
					);
					return
				},
			};

			match self.parachain_client.block_status(hash) {
				Ok(BlockStatus::Unknown) if !candidate.waiting_recovery => {
					candidate.waiting_recovery = true;
					to_recover.push(hash);
				},
				Ok(_) => break,
				Err(e) => {
					tracing::error!(
						target: LOG_TARGET,
						error = ?e,
						block_hash = ?hash,
						"Failed to get block status",
					);
					for hash in to_recover {
						self.clear_waiting_recovery(&hash);
					}
					return
				},
			}

			if kind == RecoveryKind::Simple {
				break
			}

			hash = candidate.parent_hash;
		}

		for hash in to_recover.into_iter().rev() {
			self.candidate_recovery_queue.push_recovery(hash);
		}
	}

	/// Run the pov-recovery.
	pub async fn run(mut self) {
		let mut imported_blocks = self.parachain_client.import_notification_stream().fuse();
		let mut finalized_blocks = self.parachain_client.finality_notification_stream().fuse();
		let pending_candidates = match pending_candidates(
			self.relay_chain_interface.clone(),
			self.para_id,
			self.parachain_sync_service.clone(),
		)
		.await
		{
			Ok(pending_candidates_stream) => pending_candidates_stream.fuse(),
			Err(err) => {
				tracing::error!(target: LOG_TARGET, error = ?err, "Unable to retrieve pending candidate stream.");
				return
			},
		};

		futures::pin_mut!(pending_candidates);
		loop {
			select! {
				next_pending_candidates = pending_candidates.next() => {
					if let Some((candidates, session_index)) = next_pending_candidates {
						for candidate in candidates {
							self.handle_pending_candidate(candidate, session_index);
						}
					} else {
						tracing::debug!(target: LOG_TARGET, "Pending candidates stream ended");
						return;
					}
				},
				recovery_req = self.recovery_chan_rx.next() => {
					if let Some(req) = recovery_req {
						self.recover(req);
					} else {
						tracing::debug!(target: LOG_TARGET, "Recovery channel stream ended");
						return;
					}
				},
				imported = imported_blocks.next() => {
					if let Some(imported) = imported {
						self.clear_waiting_recovery(&imported.hash);

						// We need to double check that no blocks are waiting for this block.
						// Can happen when a waiting child block is queued to wait for parent while the parent block is still
						// in the import queue.
						if let Some(waiting_blocks) = self.waiting_for_parent.remove(&imported.hash) {
							for block in waiting_blocks {
								tracing::debug!(target: LOG_TARGET, block_hash = ?block.hash(), resolved_parent = ?imported.hash, "Found new waiting child block during import, queuing.");
								self.import_block(block);
							}
						};

					} else {
						tracing::debug!(target: LOG_TARGET,	"Imported blocks stream ended");
						return;
					}
				},
				finalized = finalized_blocks.next() => {
					if let Some(finalized) = finalized {
						self.handle_block_finalized(*finalized.header.number());
					} else {
						tracing::debug!(target: LOG_TARGET,	"Finalized blocks stream ended");
						return;
					}
				},
				next_to_recover = self.candidate_recovery_queue.next_recovery().fuse() => {
						self.recover_candidate(next_to_recover).await;
				},
				(block_hash, pov) =
					self.active_candidate_recovery.wait_for_recovery().fuse() =>
				{
					self.handle_candidate_recovered(block_hash, pov.as_deref()).await;
				},
			}
		}
	}
}

/// Returns a stream over pending candidates for the parachain corresponding to `para_id`.
async fn pending_candidates(
	relay_chain_client: impl RelayChainInterface + Clone,
	para_id: ParaId,
	sync_service: Arc<dyn SyncOracle + Sync + Send>,
) -> RelayChainResult<impl Stream<Item = (Vec<CommittedCandidateReceipt>, SessionIndex)>> {
	let import_notification_stream = relay_chain_client.import_notification_stream().await?;

	let filtered_stream = import_notification_stream.filter_map(move |n| {
		let client_for_closure = relay_chain_client.clone();
		let sync_oracle = sync_service.clone();
		async move {
			let hash = n.hash();
			if sync_oracle.is_major_syncing() {
				tracing::debug!(
					target: LOG_TARGET,
					relay_hash = ?hash,
					"Skipping candidate due to sync.",
				);
				return None
			}

			let runtime_api_version = client_for_closure
				.version(hash)
				.await
				.map_err(|e| {
					tracing::error!(
						target: LOG_TARGET,
						error = ?e,
						"Failed to fetch relay chain runtime version.",
					)
				})
				.ok()?;
			let parachain_host_runtime_api_version = runtime_api_version
				.api_version(
					&<dyn polkadot_primitives::runtime_api::ParachainHost<
						polkadot_primitives::Block,
					>>::ID,
				)
				.unwrap_or_default();

			// If the relay chain runtime does not support the new runtime API, fallback to the
			// deprecated one.
			let pending_availability_result = if parachain_host_runtime_api_version <
				RuntimeApiRequest::CANDIDATES_PENDING_AVAILABILITY_RUNTIME_REQUIREMENT
			{
				#[allow(deprecated)]
				client_for_closure
					.candidate_pending_availability(hash, para_id)
					.await
					.map_err(|e| {
						tracing::error!(
							target: LOG_TARGET,
							error = ?e,
							"Failed to fetch pending candidates.",
						)
					})
					.map(|candidate| candidate.into_iter().collect::<Vec<_>>())
			} else {
				client_for_closure.candidates_pending_availability(hash, para_id).await.map_err(
					|e| {
						tracing::error!(
							target: LOG_TARGET,
							error = ?e,
							"Failed to fetch pending candidates.",
						)
					},
				)
			};

			let session_index_result =
				client_for_closure.session_index_for_child(hash).await.map_err(|e| {
					tracing::error!(
						target: LOG_TARGET,
						error = ?e,
						"Failed to fetch session index.",
					)
				});

			if let Ok(candidates) = pending_availability_result {
				session_index_result.map(|session_index| (candidates, session_index)).ok()
			} else {
				None
			}
		}
	});
	Ok(filtered_stream)
}
