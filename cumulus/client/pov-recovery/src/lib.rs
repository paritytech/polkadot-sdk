// Copyright 2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Parachain PoV recovery
//!
//! A parachain needs to build PoVs that are send to the relay chain to progress. These PoVs are
//! erasure encoded and one piece of it is stored by each relay chain validator. As the relay chain
//! decides on which PoV per parachain to include and thus, to progess the parachain it can happen
//! that the block corresponding to this PoV isn't propagated in the parachain network. This can have
//! several reasons, either a malicious collator that managed to include its own PoV and doesn't want
//! to share it with the rest of the network or maybe a collator went down before it could distribute
//! the block in the network. When something like this happens we can use the PoV recovery algorithm
//! implemented in this crate to recover a PoV and to propagate it with the rest of the network.
//!
//! It works in the following way:
//!
//! 1. For every included relay chain block we note the backed candidate of our parachain. If the
//!    block belonging to the PoV is already known, we do nothing. Otherwise we start
//!    a timer that waits a random time between 0..relay_chain_slot_length before starting to recover
//!    the PoV.
//!
//! 2. If between starting and firing the timer the block is imported, we skip the recovery of the
//!    PoV.
//!
//! 3. If the timer fired we recover the PoV using the relay chain PoV recovery protocol. After it
//!    is recovered, we restore the block and import it.
//!
//! If we need to recover multiple PoV blocks (which should hopefully not happen in real life), we
//! make sure that the blocks are imported in the correct order.

use sc_client_api::{BlockBackend, BlockchainEvents, UsageProvider};
use sc_consensus::import_queue::{ImportQueueService, IncomingBlock};
use sp_consensus::{BlockOrigin, BlockStatus};
use sp_runtime::{
	generic::BlockId,
	traits::{Block as BlockT, Header as HeaderT, NumberFor},
};

use polkadot_node_primitives::{AvailableData, POV_BOMB_LIMIT};
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::v2::{
	CandidateReceipt, CommittedCandidateReceipt, Id as ParaId, SessionIndex,
};

use cumulus_primitives_core::ParachainBlockData;
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};

use codec::Decode;
use futures::{
	channel::mpsc::Receiver, select, stream::FuturesUnordered, Future, FutureExt, Stream, StreamExt,
};
use futures_timer::Delay;
use rand::{thread_rng, Rng};

use std::{
	collections::{HashMap, VecDeque},
	pin::Pin,
	sync::Arc,
	time::Duration,
};

mod active_candidate_recovery;
use active_candidate_recovery::ActiveCandidateRecovery;

const LOG_TARGET: &str = "cumulus-pov-recovery";

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
	/// Recovery delay range. Randomizing the start of the recovery within this interval
	/// can be used to prevent self-DOSing if the recovery request is part of a
	/// distributed protocol and there is the possibility that multiple actors are
	/// requiring to perform the recovery action at approximately the same time.
	pub delay: RecoveryDelay,
	/// Recovery type.
	pub kind: RecoveryKind,
}

/// The delay between observing an unknown block and triggering the recovery of a block.
#[derive(Clone, Copy)]
pub struct RecoveryDelay {
	/// Start recovering after `min` delay.
	pub min: Duration,
	/// Start recovering before `max` delay.
	pub max: Duration,
}

/// Represents an outstanding block candidate.
struct Candidate<Block: BlockT> {
	receipt: CandidateReceipt,
	session_index: SessionIndex,
	block_number: NumberFor<Block>,
	parent_hash: Block::Hash,
	// Lazy recovery has been submitted.
	waiting_recovery: bool,
}

/// Encapsulates the logic of the pov recovery.
pub struct PoVRecovery<Block: BlockT, PC, RC> {
	/// All the pending candidates that we are waiting for to be imported or that need to be
	/// recovered when `next_candidate_to_recover` tells us to do so.
	candidates: HashMap<Block::Hash, Candidate<Block>>,
	/// A stream of futures that resolve to hashes of candidates that need to be recovered.
	///
	/// The candidates to the hashes are stored in `pending_candidates`. If a candidate is not
	/// available anymore in this map, it means that it was already imported.
	next_candidate_to_recover: FuturesUnordered<Pin<Box<dyn Future<Output = Block::Hash> + Send>>>,
	active_candidate_recovery: ActiveCandidateRecovery<Block>,
	/// Blocks that wait that the parent is imported.
	///
	/// Uses parent -> blocks mapping.
	waiting_for_parent: HashMap<Block::Hash, Vec<Block>>,
	recovery_delay: RecoveryDelay,
	parachain_client: Arc<PC>,
	parachain_import_queue: Box<dyn ImportQueueService<Block>>,
	relay_chain_interface: RC,
	para_id: ParaId,
	/// Explicit block recovery requests channel.
	recovery_chan_rx: Receiver<RecoveryRequest<Block>>,
}

impl<Block: BlockT, PC, RCInterface> PoVRecovery<Block, PC, RCInterface>
where
	PC: BlockBackend<Block> + BlockchainEvents<Block> + UsageProvider<Block>,
	RCInterface: RelayChainInterface + Clone,
{
	/// Create a new instance.
	pub fn new(
		overseer_handle: OverseerHandle,
		recovery_delay: RecoveryDelay,
		parachain_client: Arc<PC>,
		parachain_import_queue: Box<dyn ImportQueueService<Block>>,
		relay_chain_interface: RCInterface,
		para_id: ParaId,
		recovery_chan_rx: Receiver<RecoveryRequest<Block>>,
	) -> Self {
		Self {
			candidates: HashMap::new(),
			next_candidate_to_recover: Default::default(),
			active_candidate_recovery: ActiveCandidateRecovery::new(overseer_handle),
			recovery_delay,
			waiting_for_parent: HashMap::new(),
			parachain_client,
			parachain_import_queue,
			relay_chain_interface,
			para_id,
			recovery_chan_rx,
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
		self.recover(RecoveryRequest {
			hash,
			delay: self.recovery_delay,
			kind: RecoveryKind::Simple,
		});
	}

	/// Handle an imported block.
	fn handle_block_imported(&mut self, block_hash: &Block::Hash) {
		self.candidates.get_mut(block_hash).map(|candidate| {
			// Prevents triggering an already enqueued recovery request
			candidate.waiting_recovery = false;
		});
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

	/// Clear `waiting_for_parent` from the given `hash` and do this recursively for all child
	/// blocks.
	fn clear_waiting_for_parent(&mut self, hash: Block::Hash) {
		let mut blocks_to_delete = vec![hash];

		while let Some(delete) = blocks_to_delete.pop() {
			if let Some(childs) = self.waiting_for_parent.remove(&delete) {
				blocks_to_delete.extend(childs.iter().map(BlockT::hash));
			}
		}
	}

	/// Handle a recovered candidate.
	async fn handle_candidate_recovered(
		&mut self,
		block_hash: Block::Hash,
		available_data: Option<AvailableData>,
	) {
		let available_data = match available_data {
			Some(data) => data,
			None => {
				self.clear_waiting_for_parent(block_hash);
				return
			},
		};

		let raw_block_data = match sp_maybe_compressed_blob::decompress(
			&available_data.pov.block_data.0,
			POV_BOMB_LIMIT,
		) {
			Ok(r) => r,
			Err(error) => {
				tracing::debug!(target: LOG_TARGET, ?error, "Failed to decompress PoV");

				self.clear_waiting_for_parent(block_hash);

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

				self.clear_waiting_for_parent(block_hash);

				return
			},
		};

		let block = block_data.into_block();

		let parent = *block.header().parent_hash();

		match self.parachain_client.block_status(&BlockId::hash(parent)) {
			Ok(BlockStatus::Unknown) => {
				if self.active_candidate_recovery.is_being_recovered(&parent) {
					tracing::debug!(
						target: LOG_TARGET,
						?block_hash,
						parent_hash = ?parent,
						"Parent is still being recovered, waiting.",
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

					self.clear_waiting_for_parent(block_hash);

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

				self.clear_waiting_for_parent(block_hash);

				return
			},
			// Any other status is fine to "ignore/accept"
			_ => (),
		}

		self.import_block(block).await;
	}

	/// Import the given `block`.
	///
	/// This will also recursivley drain `waiting_for_parent` and import them as well.
	async fn import_block(&mut self, block: Block) {
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
		let RecoveryRequest { mut hash, delay, kind } = req;
		let mut to_recover = Vec::new();

		let do_recover = loop {
			let candidate = match self.candidates.get_mut(&hash) {
				Some(candidate) => candidate,
				None => {
					tracing::debug!(
						target: LOG_TARGET,
						block_hash = ?hash,
						"Cound not recover. Block was never announced as candidate"
					);
					break false
				},
			};

			match self.parachain_client.block_status(&BlockId::Hash(hash)) {
				Ok(BlockStatus::Unknown) if !candidate.waiting_recovery => {
					candidate.waiting_recovery = true;
					to_recover.push(hash);
				},
				Ok(_) => break true,
				Err(e) => {
					tracing::error!(
						target: LOG_TARGET,
						error = ?e,
						block_hash = ?hash,
						"Failed to get block status",
					);
					break false
				},
			}

			if kind == RecoveryKind::Simple {
				break true
			}

			hash = candidate.parent_hash;
		};

		if do_recover {
			for hash in to_recover.into_iter().rev() {
				let delay =
					delay.min + delay.max.saturating_sub(delay.min).mul_f64(thread_rng().gen());
				tracing::debug!(
					target: LOG_TARGET,
					block_hash = ?hash,
					"Starting {:?} block recovery in {:?} sec",
					kind,
					delay.as_secs(),
				);
				self.next_candidate_to_recover.push(
					async move {
						Delay::new(delay).await;
						hash
					}
					.boxed(),
				);
			}
		}
	}

	/// Run the pov-recovery.
	pub async fn run(mut self) {
		let mut imported_blocks = self.parachain_client.import_notification_stream().fuse();
		let mut finalized_blocks = self.parachain_client.finality_notification_stream().fuse();
		let pending_candidates =
			match pending_candidates(self.relay_chain_interface.clone(), self.para_id).await {
				Ok(pending_candidate_stream) => pending_candidate_stream.fuse(),
				Err(err) => {
					tracing::error!(target: LOG_TARGET, error = ?err, "Unable to retrieve pending candidate stream.");
					return
				},
			};

		futures::pin_mut!(pending_candidates);

		loop {
			select! {
				pending_candidate = pending_candidates.next() => {
					if let Some((receipt, session_index)) = pending_candidate {
						self.handle_pending_candidate(receipt, session_index);
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
						self.handle_block_imported(&imported.hash);
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
				next_to_recover = self.next_candidate_to_recover.next() => {
					if let Some(block_hash) = next_to_recover {
						self.recover_candidate(block_hash).await;
					}
				},
				(block_hash, available_data) =
					self.active_candidate_recovery.wait_for_recovery().fuse() =>
				{
					self.handle_candidate_recovered(block_hash, available_data).await;
				},
			}
		}
	}
}

/// Returns a stream over pending candidates for the parachain corresponding to `para_id`.
async fn pending_candidates(
	relay_chain_client: impl RelayChainInterface + Clone,
	para_id: ParaId,
) -> RelayChainResult<impl Stream<Item = (CommittedCandidateReceipt, SessionIndex)>> {
	let import_notification_stream = relay_chain_client.import_notification_stream().await?;

	let filtered_stream = import_notification_stream.filter_map(move |n| {
		let client_for_closure = relay_chain_client.clone();
		async move {
			let hash = n.hash();
			let pending_availability_result = client_for_closure
				.candidate_pending_availability(hash, para_id)
				.await
				.map_err(|e| {
					tracing::error!(
						target: LOG_TARGET,
						error = ?e,
						"Failed to fetch pending candidates.",
					)
				});
			let session_index_result =
				client_for_closure.session_index_for_child(hash).await.map_err(|e| {
					tracing::error!(
						target: LOG_TARGET,
						error = ?e,
						"Failed to fetch session index.",
					)
				});

			if let Ok(Some(candidate)) = pending_availability_result {
				session_index_result.map(|session_index| (candidate, session_index)).ok()
			} else {
				None
			}
		}
	});
	Ok(filtered_stream)
}
