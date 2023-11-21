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

//! Warp sync support.

pub use sp_consensus_grandpa::{AuthorityList, SetId};

use crate::{
	chain_sync::validate_blocks,
	schema::v1::StateResponse,
	state::{ImportResult, StateSync},
	types::{BadPeer, OpaqueStateRequest, OpaqueStateResponse},
};
use codec::{Decode, Encode};
use futures::channel::oneshot;
use libp2p::PeerId;
use log::{debug, error, info, trace, warn};
use sc_client_api::ProofProvider;
use sc_consensus::{BlockImportError, BlockImportStatus, IncomingBlock};
use sc_network_common::sync::message::{
	BlockAttributes, BlockData, BlockRequest, Direction, FromBlock,
};
use sp_blockchain::HeaderBackend;
use sp_consensus::BlockOrigin;
use sp_runtime::traits::{Block as BlockT, Header, NumberFor, Zero};
use std::{collections::HashMap, fmt, sync::Arc};

/// Log target for this file.
const LOG_TARGET: &'static str = "sync";

/// Number of peers that need to be connected before warp sync is started.
const MIN_PEERS_TO_START_WARP_SYNC: usize = 3;

/// Scale-encoded warp sync proof response.
pub struct EncodedProof(pub Vec<u8>);

/// Warp sync request
#[derive(Encode, Decode, Debug, Clone)]
pub struct WarpProofRequest<B: BlockT> {
	/// Start collecting proofs from this block.
	pub begin: B::Hash,
}

/// Proof verification result.
pub enum VerificationResult<Block: BlockT> {
	/// Proof is valid, but the target was not reached.
	Partial(SetId, AuthorityList, Block::Hash),
	/// Target finality is proved.
	Complete(SetId, AuthorityList, Block::Header),
}

/// Warp sync backend. Handles retrieving and verifying warp sync proofs.
pub trait WarpSyncProvider<Block: BlockT>: Send + Sync {
	/// Generate proof starting at given block hash. The proof is accumulated until maximum proof
	/// size is reached.
	fn generate(
		&self,
		start: Block::Hash,
	) -> Result<EncodedProof, Box<dyn std::error::Error + Send + Sync>>;
	/// Verify warp proof against current set of authorities.
	fn verify(
		&self,
		proof: &EncodedProof,
		set_id: SetId,
		authorities: AuthorityList,
	) -> Result<VerificationResult<Block>, Box<dyn std::error::Error + Send + Sync>>;
	/// Get current list of authorities. This is supposed to be genesis authorities when starting
	/// sync.
	fn current_authorities(&self) -> AuthorityList;
}

mod rep {
	use sc_network::ReputationChange as Rep;

	/// Unexpected response received form a peer
	pub const UNEXPECTED_RESPONSE: Rep = Rep::new(-(1 << 29), "Unexpected response");

	/// Peer provided invalid warp proof data
	pub const BAD_WARP_PROOF: Rep = Rep::new(-(1 << 29), "Bad warp proof");

	/// Peer did not provide us with advertised block data.
	pub const NO_BLOCK: Rep = Rep::new(-(1 << 29), "No requested block data");

	/// Reputation change for peers which send us non-requested block data.
	pub const NOT_REQUESTED: Rep = Rep::new(-(1 << 29), "Not requested block data");

	/// Reputation change for peers which send us a block which we fail to verify.
	pub const VERIFICATION_FAIL: Rep = Rep::new(-(1 << 29), "Block verification failed");

	/// Peer response data does not have requested bits.
	pub const BAD_RESPONSE: Rep = Rep::new(-(1 << 12), "Incomplete response");

	/// Reputation change for peers which send us a known bad state.
	pub const BAD_STATE: Rep = Rep::new(-(1 << 29), "Bad state");
}

/// Reported warp sync phase.
#[derive(Clone, Eq, PartialEq, Debug)]
pub enum WarpSyncPhase<Block: BlockT> {
	/// Waiting for peers to connect.
	AwaitingPeers { required_peers: usize },
	/// Waiting for target block to be received.
	AwaitingTargetBlock,
	/// Downloading and verifying grandpa warp proofs.
	DownloadingWarpProofs,
	/// Downloading target block.
	DownloadingTargetBlock,
	/// Downloading state data.
	DownloadingState,
	/// Importing state.
	ImportingState,
	/// Downloading block history.
	DownloadingBlocks(NumberFor<Block>),
	/// Warp sync is complete.
	Complete,
}

impl<Block: BlockT> fmt::Display for WarpSyncPhase<Block> {
	fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
		match self {
			Self::AwaitingPeers { required_peers } =>
				write!(f, "Waiting for {required_peers} peers to be connected"),
			Self::AwaitingTargetBlock => write!(f, "Waiting for target block to be received"),
			Self::DownloadingWarpProofs => write!(f, "Downloading finality proofs"),
			Self::DownloadingTargetBlock => write!(f, "Downloading target block"),
			Self::DownloadingState => write!(f, "Downloading state"),
			Self::ImportingState => write!(f, "Importing state"),
			Self::DownloadingBlocks(n) => write!(f, "Downloading block history (#{})", n),
			Self::Complete => write!(f, "Warp sync is complete"),
		}
	}
}

/// Reported warp sync progress.
#[derive(Clone, Eq, PartialEq, Debug)]
pub struct WarpSyncProgress<Block: BlockT> {
	/// Estimated download percentage.
	pub phase: WarpSyncPhase<Block>,
	/// Total bytes downloaded so far.
	pub total_bytes: u64,
}

/// The different types of warp syncing, passed to `build_network`.
pub enum WarpSyncParams<Block: BlockT> {
	/// Standard warp sync for the chain.
	WithProvider(Arc<dyn WarpSyncProvider<Block>>),
	/// Skip downloading proofs and wait for a header of the state that should be downloaded.
	///
	/// It is expected that the header provider ensures that the header is trusted.
	WaitForTarget(oneshot::Receiver<<Block as BlockT>::Header>),
}

/// Warp sync configuration as accepted by [`WarpSync`].
pub enum WarpSyncConfig<Block: BlockT> {
	/// Standard warp sync for the chain.
	WithProvider(Arc<dyn WarpSyncProvider<Block>>),
	/// Skip downloading proofs and wait for a header of the state that should be downloaded.
	///
	/// It is expected that the header provider ensures that the header is trusted.
	WaitForTarget,
}

impl<Block: BlockT> WarpSyncParams<Block> {
	/// Split `WarpSyncParams` into `WarpSyncConfig` and warp sync target block header receiver.
	pub fn split(
		self,
	) -> (WarpSyncConfig<Block>, Option<oneshot::Receiver<<Block as BlockT>::Header>>) {
		match self {
			WarpSyncParams::WithProvider(provider) =>
				(WarpSyncConfig::WithProvider(provider), None),
			WarpSyncParams::WaitForTarget(rx) => (WarpSyncConfig::WaitForTarget, Some(rx)),
		}
	}
}

/// Warp sync phase.
enum Phase<B: BlockT, Client> {
	/// Waiting for enough peers to connect.
	WaitingForPeers { warp_sync_provider: Arc<dyn WarpSyncProvider<B>> },
	/// Downloading warp proofs.
	WarpProof {
		set_id: SetId,
		authorities: AuthorityList,
		last_hash: B::Hash,
		warp_sync_provider: Arc<dyn WarpSyncProvider<B>>,
	},
	/// Waiting for target block to be set externally if we skip warp proofs downloading,
	/// and start straight from the target block (used by parachains warp sync).
	PendingTargetBlock,
	/// Downloading target block.
	TargetBlock(B::Header),
	/// Downloading and importing state.
	State(StateSync<B, Client>),
	/// Warp sync is complete.
	Complete,
}

/// Import warp proof result.
pub enum WarpProofImportResult {
	/// Import was successful.
	Success,
	/// Bad proof.
	BadResponse,
}

/// Import target block result.
pub enum TargetBlockImportResult {
	/// Import was successful.
	Success,
	/// Invalid block.
	BadResponse,
}

enum PeerState {
	Available,
	DownloadingProofs,
	DownloadingTargetBlock,
	DownloadingState,
}

impl PeerState {
	fn is_available(&self) -> bool {
		matches!(self, PeerState::Available)
	}
}

struct Peer<B: BlockT> {
	best_number: NumberFor<B>,
	state: PeerState,
}

pub enum WarpSyncAction<B: BlockT> {
	/// Send warp proof request to peer.
	SendWarpProofRequest { peer_id: PeerId, request: WarpProofRequest<B> },
	/// Send block request to peer. Always implies dropping a stale block request to the same peer.
	SendBlockRequest { peer_id: PeerId, request: BlockRequest<B> },
	/// Send state request to peer.
	SendStateRequest { peer_id: PeerId, request: OpaqueStateRequest },
	/// Disconnect and report peer.
	DropPeer(BadPeer),
	/// Import blocks.
	ImportBlocks { origin: BlockOrigin, blocks: Vec<IncomingBlock<B>> },
	/// Warp sync has finished.
	Finished,
}

/// Warp sync state machine. Accumulates warp proofs and state.
pub struct WarpSync<B: BlockT, Client> {
	phase: Phase<B, Client>,
	client: Arc<Client>,
	total_proof_bytes: u64,
	total_state_bytes: u64,
	peers: HashMap<PeerId, Peer<B>>,
	actions: Vec<WarpSyncAction<B>>,
}

impl<B, Client> WarpSync<B, Client>
where
	B: BlockT,
	Client: HeaderBackend<B> + ProofProvider<B> + 'static,
{
	/// Create a new instance. When passing a warp sync provider we will be checking for proof and
	/// authorities. Alternatively we can pass a target block when we want to skip downloading
	/// proofs, in this case we will continue polling until the target block is known.
	pub fn new(client: Arc<Client>, warp_sync_config: WarpSyncConfig<B>) -> Self {
		if client.info().finalized_state.is_some() {
			warn!(
				target: LOG_TARGET,
				"Can't use warp sync mode with a partially synced database. Reverting to full sync mode."
			);
			return Self {
				client,
				phase: Phase::Complete,
				total_proof_bytes: 0,
				total_state_bytes: 0,
				peers: HashMap::new(),
				actions: vec![WarpSyncAction::Finished],
			}
		}

		let phase = match warp_sync_config {
			WarpSyncConfig::WithProvider(warp_sync_provider) =>
				Phase::WaitingForPeers { warp_sync_provider },
			WarpSyncConfig::WaitForTarget => Phase::PendingTargetBlock,
		};

		Self {
			client,
			phase,
			total_proof_bytes: 0,
			total_state_bytes: 0,
			peers: HashMap::new(),
			actions: Vec::new(),
		}
	}

	/// Set target block externally in case we skip warp proof downloading.
	pub fn set_target_block(&mut self, header: B::Header) {
		let Phase::PendingTargetBlock = self.phase else {
			error!(
				target: LOG_TARGET,
				"Attempt to set warp sync target block in invalid phase.",
			);
			debug_assert!(false);
			return
		};

		self.phase = Phase::TargetBlock(header);
	}

	pub fn new_peer(&mut self, peer_id: PeerId, _best_hash: B::Hash, best_number: NumberFor<B>) {
		self.peers.insert(peer_id, Peer { best_number, state: PeerState::Available });

		self.try_to_start_warp_sync();
	}

	pub fn peer_disconnected(&mut self, peer_id: &PeerId) {
		self.peers.remove(peer_id);
	}

	fn try_to_start_warp_sync(&mut self) {
		let Phase::WaitingForPeers { warp_sync_provider } = self.phase else { return };

		if self.peers.len() < MIN_PEERS_TO_START_WARP_SYNC {
			return
		}

		let genesis_hash =
			self.client.hash(Zero::zero()).unwrap().expect("Genesis hash always exists");
		self.phase = Phase::WarpProof {
			set_id: 0,
			authorities: warp_sync_provider.current_authorities(),
			last_hash: genesis_hash,
			warp_sync_provider,
		};
		trace!(target: LOG_TARGET, "Started warp sync with {} peers.", self.peers.len());
	}

	///  Process warp proof response.
	pub fn on_warp_proof_response(&mut self, peer_id: &PeerId, response: EncodedProof) {
		if let Some(peer) = self.peers.get_mut(peer_id) {
			peer.state = PeerState::Available;
		}

		let Phase::WarpProof { set_id, authorities, last_hash, warp_sync_provider } =
			&mut self.phase
		else {
			debug!(target: LOG_TARGET, "Unexpected warp proof response");
			self.actions
				.push(WarpSyncAction::DropPeer(BadPeer(*peer_id, rep::UNEXPECTED_RESPONSE)));
			return
		};

		match warp_sync_provider.verify(&response, *set_id, authorities.clone()) {
			Err(e) => {
				debug!(target: LOG_TARGET, "Bad warp proof response: {}", e);
				self.actions
					.push(WarpSyncAction::DropPeer(BadPeer(*peer_id, rep::BAD_WARP_PROOF)))
			},
			Ok(VerificationResult::Partial(new_set_id, new_authorities, new_last_hash)) => {
				log::debug!(target: LOG_TARGET, "Verified partial proof, set_id={:?}", new_set_id);
				*set_id = new_set_id;
				*authorities = new_authorities;
				*last_hash = new_last_hash;
				self.total_proof_bytes += response.0.len() as u64;
			},
			Ok(VerificationResult::Complete(new_set_id, _, header)) => {
				log::debug!(
					target: LOG_TARGET,
					"Verified complete proof, set_id={:?}. Continuing with target block download: {} ({}).",
					new_set_id,
					header.hash(),
					header.number(),
				);
				self.total_proof_bytes += response.0.len() as u64;
				self.phase = Phase::TargetBlock(header);
			},
		}
	}

	/// Process (target) block response.
	pub fn on_block_response(
		&mut self,
		peer_id: PeerId,
		request: BlockRequest<B>,
		blocks: Vec<BlockData<B>>,
	) {
		if let Err(bad_peer) = self.on_block_response_inner(peer_id, request, blocks) {
			self.actions.push(WarpSyncAction::DropPeer(bad_peer));
		}
	}

	fn on_block_response_inner(
		&mut self,
		peer_id: PeerId,
		request: BlockRequest<B>,
		blocks: Vec<BlockData<B>>,
	) -> Result<(), BadPeer> {
		if let Some(peer) = self.peers.get_mut(&peer_id) {
			peer.state = PeerState::Available;
		}

		let Phase::TargetBlock(header) = &mut self.phase else {
			debug!(target: "sync", "Unexpected target block response from {peer_id}");
			return Err(BadPeer(peer_id, rep::UNEXPECTED_RESPONSE))
		};

		if blocks.is_empty() {
			debug!(
				target: LOG_TARGET,
				"Downloading target block failed: empty block response from {peer_id}",
			);
			return Err(BadPeer(peer_id, rep::NO_BLOCK))
		}

		if blocks.len() > 1 {
			debug!(
				target: LOG_TARGET,
				"Too many blocks ({}) in warp target block response from {peer_id}",
				blocks.len(),
			);
			return Err(BadPeer(peer_id, rep::NOT_REQUESTED))
		}

		validate_blocks::<B>(&blocks, &peer_id, Some(request))?;

		let block = blocks.pop().expect("`blocks` len checked above; qed");

		let Some(block_header) = &block.header else {
			debug!(
				target: "sync",
				"Downloading target block failed: missing header in response from {peer_id}.",
			);
			return Err(BadPeer(peer_id, rep::VERIFICATION_FAIL))
		};

		if block_header != header {
			debug!(
				target: "sync",
				"Downloading target block failed: different header in response from {peer_id}.",
			);
			return Err(BadPeer(peer_id, rep::VERIFICATION_FAIL))
		}

		if block.body.is_none() {
			debug!(
				target: "sync",
				"Downloading target block failed: missing body in response from {peer_id}.",
			);
			return Err(BadPeer(peer_id, rep::VERIFICATION_FAIL))
		}

		debug!(
			target: LOG_TARGET,
			"Downloaded target block {} ({}), continuing with state sync.",
			header.hash(),
			header.number(),
		);
		let state_sync = StateSync::new(
			self.client.clone(),
			header.clone(),
			block.body,
			block.justifications,
			false,
		);
		self.phase = Phase::State(state_sync);
		Ok(())
	}

	/// Process state response.
	pub fn on_state_response(&mut self, peer_id: PeerId, response: OpaqueStateResponse) {
		if let Err(bad_peer) = self.on_state_response_inner(peer_id, response) {
			self.actions.push(WarpSyncAction::DropPeer(bad_peer));
		}
	}

	fn on_state_response_inner(
		&mut self,
		peer_id: PeerId,
		response: OpaqueStateResponse,
	) -> Result<(), BadPeer> {
		if let Some(peer) = self.peers.get_mut(&peer_id) {
			peer.state = PeerState::Available;
		}

		let Phase::State(state_sync) = &mut self.phase else {
			debug!(target: "sync", "Unexpected state response");
			return Err(BadPeer(peer_id, rep::UNEXPECTED_RESPONSE))
		};

		let response: Box<StateResponse> = response.0.downcast().map_err(|_error| {
			error!(
				target: LOG_TARGET,
				"Failed to downcast opaque state response, this is an implementation bug."
			);

			BadPeer(peer_id, rep::BAD_RESPONSE)
		})?;

		debug!(
			target: LOG_TARGET,
			"Importing state data from {} with {} keys, {} proof nodes.",
			peer_id,
			response.entries.len(),
			response.proof.len(),
		);

		let import_result = state_sync.import(*response);
		self.total_state_bytes = state_sync.progress().size;

		match import_result {
			ImportResult::Import(hash, header, state, body, justifications) => {
				let origin = BlockOrigin::NetworkInitialSync;
				let block = IncomingBlock {
					hash,
					header: Some(header),
					body,
					indexed_body: None,
					justifications,
					origin: None,
					allow_missing_state: true,
					import_existing: true,
					skip_execution: true,
					state: Some(state),
				};
				debug!(target: LOG_TARGET, "State download is complete. Import is queued");
				self.actions.push(WarpSyncAction::ImportBlocks { origin, blocks: vec![block] });
				Ok(())
			},
			ImportResult::Continue => Ok(()),
			ImportResult::BadResponse => {
				debug!(target: LOG_TARGET, "Bad state data received from {peer_id}");
				Err(BadPeer(peer_id, rep::BAD_STATE))
			},
		}
	}

	/// A batch of blocks have been processed, with or without errors.
	///
	/// Normally this should be called when target block with state is imported.
	pub fn on_blocks_processed(
		&mut self,
		imported: usize,
		count: usize,
		results: Vec<(Result<BlockImportStatus<NumberFor<B>>, BlockImportError>, B::Hash)>,
	) {
		let Phase::State(state_sync) = &self.phase else {
			debug!(target: LOG_TARGET, "Unexpected block import of {imported} of {count}.");
			return
		};

		trace!(target: LOG_TARGET, "Warp sync: imported {imported} of {count}.");

		for (result, hash) in results {
			if hash != state_sync.target() {
				debug!(
					target: LOG_TARGET,
					"Unexpected block processed: {hash} with result {result:?}.",
				);
				continue
			}

			if let Err(e) = result {
				error!(
					target: LOG_TARGET,
					"Warp sync failed. Failed to import target block with state: {e:?}."
				);
			}

			let total_mib = (self.total_proof_bytes + state_sync.progress().size) / (1024 * 1024);
			info!(
				target: LOG_TARGET,
				"Warp sync is complete ({total_mib} MiB), continuing with block sync.",
			);

			self.phase = Phase::Complete;
			self.actions.push(WarpSyncAction::Finished);
		}
	}

	/// Get candidate for warp/block request.
	fn select_synced_available_peer(
		&self,
		min_best_number: Option<NumberFor<B>>,
	) -> Option<(&PeerId, &Peer<B>)> {
		let mut targets: Vec<_> = self.peers.values().map(|p| p.best_number).collect();
		if !targets.is_empty() {
			targets.sort();
			let median = targets[targets.len() / 2];
			let threshold = std::cmp::max(median, min_best_number.unwrap_or(Zero::zero()));
			// Find a random peer that is synced as much as peer majority and is above
			// `best_number_at_least`.
			for (peer_id, peer) in self.peers.iter_mut() {
				if peer.state.is_available() && peer.best_number >= threshold {
					return Some((peer_id, peer))
				}
			}
		}

		None
	}

	/// Produce next warp proof request.
	fn warp_proof_request(&self) -> Option<(PeerId, WarpProofRequest<B>)> {
		let Phase::WarpProof { last_hash, .. } = &self.phase else { return None };

		if self
			.peers
			.iter()
			.any(|(_, peer)| matches!(peer.state, PeerState::DownloadingProofs))
		{
			// Only one warp proof request at a time is possible.
			return None
		}

		let Some((peer_id, peer)) = self.select_synced_available_peer(None) else { return None };

		trace!(target: LOG_TARGET, "New WarpProofRequest to {peer_id}, begin hash: {last_hash}.");
		peer.state = PeerState::DownloadingProofs;

		Some((*peer_id, WarpProofRequest { begin: *last_hash }))
	}

	/// Produce next target block request.
	fn target_block_request(&self) -> Option<(PeerId, BlockRequest<B>)> {
		let Phase::TargetBlock(target_header) = &self.phase else { return None };

		if self
			.peers
			.iter()
			.any(|(_, peer)| matches!(peer.state, PeerState::DownloadingTargetBlock))
		{
			// Only one target block request at a time is possible.
			return None
		}

		let Some((peer_id, peer)) =
			self.select_synced_available_peer(Some(*target_header.number()))
		else {
			return None
		};

		trace!(
			target: LOG_TARGET,
			"New target block request to {peer_id}, target: {} ({}).",
			target_header.hash(),
			target_header.number(),
		);
		peer.state = PeerState::DownloadingTargetBlock;

		Some((
			*peer_id,
			BlockRequest::<B> {
				id: 0,
				fields: BlockAttributes::HEADER |
					BlockAttributes::BODY |
					BlockAttributes::JUSTIFICATION,
				from: FromBlock::Hash(target_header.hash()),
				direction: Direction::Ascending,
				max: Some(1),
			},
		))
	}

	/// Produce next state request.
	fn state_request(&self) -> Option<(PeerId, OpaqueStateRequest)> {
		let Phase::State(state_sync) = &self.phase else { return None };

		if self
			.peers
			.iter()
			.any(|(_, peer)| matches!(peer.state, PeerState::DownloadingState))
		{
			// Only one state request at a time is possible.
			return None
		}

		let Some((peer_id, peer)) =
			self.select_synced_available_peer(Some(state_sync.target_block_num()))
		else {
			return None
		};

		peer.state = PeerState::DownloadingTargetBlock;
		let request = state_sync.next_request();
		trace!(
			target: LOG_TARGET,
			"New state request to {peer_id}: {request:?}.",
		);

		Some((*peer_id, OpaqueStateRequest(Box::new(request))))
	}

	/// Returns state sync estimated progress (stage, bytes received).
	pub fn progress(&self) -> WarpSyncProgress<B> {
		match &self.phase {
			Phase::WaitingForPeers { .. } => WarpSyncProgress {
				phase: WarpSyncPhase::AwaitingPeers {
					required_peers: MIN_PEERS_TO_START_WARP_SYNC,
				},
				total_bytes: self.total_proof_bytes,
			},
			Phase::WarpProof { .. } => WarpSyncProgress {
				phase: WarpSyncPhase::DownloadingWarpProofs,
				total_bytes: self.total_proof_bytes,
			},
			Phase::TargetBlock(_) => WarpSyncProgress {
				phase: WarpSyncPhase::DownloadingTargetBlock,
				total_bytes: self.total_proof_bytes,
			},
			Phase::PendingTargetBlock { .. } => WarpSyncProgress {
				phase: WarpSyncPhase::AwaitingTargetBlock,
				total_bytes: self.total_proof_bytes,
			},
			Phase::State(state_sync) => WarpSyncProgress {
				phase: if state_sync.is_complete() {
					WarpSyncPhase::ImportingState
				} else {
					WarpSyncPhase::DownloadingState
				},
				total_bytes: self.total_proof_bytes + state_sync.progress().size,
			},
			Phase::Complete => WarpSyncProgress {
				phase: WarpSyncPhase::Complete,
				total_bytes: self.total_proof_bytes + self.total_state_bytes,
			},
		}
	}

	/// Get actions that should be performed by the owner on [`WarpSync`]'s behalf
	#[must_use]
	pub fn actions(&mut self) -> impl Iterator<Item = WarpSyncAction<B>> {
		let warp_proof_request = self
			.warp_proof_request()
			.into_iter()
			.map(|(peer_id, request)| WarpSyncAction::SendWarpProofRequest { peer_id, request });
		self.actions.extend(warp_proof_request);

		let target_block_request = self
			.target_block_request()
			.into_iter()
			.map(|(peer_id, request)| WarpSyncAction::SendBlockRequest { peer_id, request });
		self.actions.extend(target_block_request);

		let state_request = self
			.state_request()
			.into_iter()
			.map(|(peer_id, request)| WarpSyncAction::SendStateRequest { peer_id, request });
		self.actions.extend(state_request);

		std::mem::take(&mut self.actions).into_iter()
	}
}
