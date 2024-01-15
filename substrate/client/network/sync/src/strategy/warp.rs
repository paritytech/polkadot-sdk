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

//! Warp syncing strategy. Bootstraps chain by downloading warp proofs and state.

pub use sp_consensus_grandpa::{AuthorityList, SetId};

use crate::{
	strategy::chain_sync::validate_blocks,
	types::{BadPeer, SyncState, SyncStatus},
	LOG_TARGET,
};
use codec::{Decode, Encode};
use futures::channel::oneshot;
use libp2p::PeerId;
use log::{debug, error, trace};
use sc_network_common::sync::message::{
	BlockAnnounce, BlockAttributes, BlockData, BlockRequest, Direction, FromBlock,
};
use sp_blockchain::HeaderBackend;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor, Zero},
	Justifications, SaturatedConversion,
};
use std::{collections::HashMap, fmt, sync::Arc};

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

/// Warp sync phase used by warp sync state machine.
enum Phase<B: BlockT> {
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
	/// Warp sync is complete.
	Complete,
}

enum PeerState {
	Available,
	DownloadingProofs,
	DownloadingTargetBlock,
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

/// Action that should be performed on [`WarpSync`]'s behalf.
pub enum WarpSyncAction<B: BlockT> {
	/// Send warp proof request to peer.
	SendWarpProofRequest { peer_id: PeerId, request: WarpProofRequest<B> },
	/// Send block request to peer. Always implies dropping a stale block request to the same peer.
	SendBlockRequest { peer_id: PeerId, request: BlockRequest<B> },
	/// Disconnect and report peer.
	DropPeer(BadPeer),
	/// Warp sync has finished.
	Finished,
}

pub struct WarpSyncResult<B: BlockT> {
	pub target_header: B::Header,
	pub target_body: Option<Vec<B::Extrinsic>>,
	pub target_justifications: Option<Justifications>,
}

/// Warp sync state machine. Accumulates warp proofs and state.
pub struct WarpSync<B: BlockT, Client> {
	phase: Phase<B>,
	client: Arc<Client>,
	total_proof_bytes: u64,
	total_state_bytes: u64,
	peers: HashMap<PeerId, Peer<B>>,
	actions: Vec<WarpSyncAction<B>>,
	result: Option<WarpSyncResult<B>>,
}

impl<B, Client> WarpSync<B, Client>
where
	B: BlockT,
	Client: HeaderBackend<B> + 'static,
{
	/// Create a new instance. When passing a warp sync provider we will be checking for proof and
	/// authorities. Alternatively we can pass a target block when we want to skip downloading
	/// proofs, in this case we will continue polling until the target block is known.
	pub fn new(client: Arc<Client>, warp_sync_config: WarpSyncConfig<B>) -> Self {
		if client.info().finalized_state.is_some() {
			error!(
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
				result: None,
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
			result: None,
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

	/// Notify that a new peer has connected.
	pub fn add_peer(&mut self, peer_id: PeerId, _best_hash: B::Hash, best_number: NumberFor<B>) {
		self.peers.insert(peer_id, Peer { best_number, state: PeerState::Available });

		self.try_to_start_warp_sync();
	}

	/// Notify that a peer has disconnected.
	pub fn remove_peer(&mut self, peer_id: &PeerId) {
		self.peers.remove(peer_id);
	}

	/// Submit a validated block announcement.
	///
	/// Returns new best hash & best number of the peer if they are updated.
	#[must_use]
	pub fn on_validated_block_announce(
		&mut self,
		is_best: bool,
		peer_id: PeerId,
		announce: &BlockAnnounce<B::Header>,
	) -> Option<(B::Hash, NumberFor<B>)> {
		is_best.then_some({
			let best_number = *announce.header.number();
			let best_hash = announce.header.hash();
			if let Some(ref mut peer) = self.peers.get_mut(&peer_id) {
				peer.best_number = best_number;
			}
			// Let `SyncingEngine` know that we should update the peer info.
			(best_hash, best_number)
		})
	}

	/// Start warp sync as soon as we have enough peers.
	fn try_to_start_warp_sync(&mut self) {
		let Phase::WaitingForPeers { warp_sync_provider } = &self.phase else { return };

		if self.peers.len() < MIN_PEERS_TO_START_WARP_SYNC {
			return
		}

		self.phase = Phase::WarpProof {
			set_id: 0,
			authorities: warp_sync_provider.current_authorities(),
			last_hash: self.client.info().genesis_hash,
			warp_sync_provider: Arc::clone(warp_sync_provider),
		};
		trace!(target: LOG_TARGET, "Started warp sync with {} peers.", self.peers.len());
	}

	/// Process warp proof response.
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
		mut blocks: Vec<BlockData<B>>,
	) -> Result<(), BadPeer> {
		if let Some(peer) = self.peers.get_mut(&peer_id) {
			peer.state = PeerState::Available;
		}

		let Phase::TargetBlock(header) = &mut self.phase else {
			debug!(target: LOG_TARGET, "Unexpected target block response from {peer_id}");
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
				target: LOG_TARGET,
				"Downloading target block failed: missing header in response from {peer_id}.",
			);
			return Err(BadPeer(peer_id, rep::VERIFICATION_FAIL))
		};

		if block_header != header {
			debug!(
				target: LOG_TARGET,
				"Downloading target block failed: different header in response from {peer_id}.",
			);
			return Err(BadPeer(peer_id, rep::VERIFICATION_FAIL))
		}

		if block.body.is_none() {
			debug!(
				target: LOG_TARGET,
				"Downloading target block failed: missing body in response from {peer_id}.",
			);
			return Err(BadPeer(peer_id, rep::VERIFICATION_FAIL))
		}

		self.result = Some(WarpSyncResult {
			target_header: header.clone(),
			target_body: block.body,
			target_justifications: block.justifications,
		});
		self.phase = Phase::Complete;
		self.actions.push(WarpSyncAction::Finished);
		Ok(())
	}

	/// Reserve a peer for a request assigning `new_state`.
	fn schedule_next_peer(
		&mut self,
		new_state: PeerState,
		min_best_number: Option<NumberFor<B>>,
	) -> Option<PeerId> {
		let mut targets: Vec<_> = self.peers.values().map(|p| p.best_number).collect();
		if targets.is_empty() {
			return None
		}
		targets.sort();
		let median = targets[targets.len() / 2];
		let threshold = std::cmp::max(median, min_best_number.unwrap_or(Zero::zero()));
		// Find a random peer that is synced as much as peer majority and is above
		// `min_best_number`.
		for (peer_id, peer) in self.peers.iter_mut() {
			if peer.state.is_available() && peer.best_number >= threshold {
				peer.state = new_state;
				return Some(*peer_id)
			}
		}
		None
	}

	/// Produce warp proof request.
	fn warp_proof_request(&mut self) -> Option<(PeerId, WarpProofRequest<B>)> {
		let Phase::WarpProof { last_hash, .. } = &self.phase else { return None };

		// Copy `last_hash` early to cut the borrowing tie.
		let begin = *last_hash;

		if self
			.peers
			.values()
			.any(|peer| matches!(peer.state, PeerState::DownloadingProofs))
		{
			// Only one warp proof request at a time is possible.
			return None
		}

		let peer_id = self.schedule_next_peer(PeerState::DownloadingProofs, None)?;
		trace!(target: LOG_TARGET, "New WarpProofRequest to {peer_id}, begin hash: {begin}.");

		Some((peer_id, WarpProofRequest { begin }))
	}

	/// Produce target block request.
	fn target_block_request(&mut self) -> Option<(PeerId, BlockRequest<B>)> {
		let Phase::TargetBlock(target_header) = &self.phase else { return None };

		if self
			.peers
			.values()
			.any(|peer| matches!(peer.state, PeerState::DownloadingTargetBlock))
		{
			// Only one target block request at a time is possible.
			return None
		}

		// Cut the borrowing tie.
		let target_hash = target_header.hash();
		let target_number = *target_header.number();

		let peer_id =
			self.schedule_next_peer(PeerState::DownloadingTargetBlock, Some(target_number))?;

		trace!(
			target: LOG_TARGET,
			"New target block request to {peer_id}, target: {} ({}).",
			target_hash,
			target_number,
		);

		Some((
			peer_id,
			BlockRequest::<B> {
				id: 0,
				fields: BlockAttributes::HEADER |
					BlockAttributes::BODY |
					BlockAttributes::JUSTIFICATION,
				from: FromBlock::Hash(target_hash),
				direction: Direction::Ascending,
				max: Some(1),
			},
		))
	}

	/// Returns warp sync estimated progress (stage, bytes received).
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
			Phase::Complete => WarpSyncProgress {
				phase: WarpSyncPhase::Complete,
				total_bytes: self.total_proof_bytes + self.total_state_bytes,
			},
		}
	}

	/// Get the number of peers known to warp sync.
	pub fn num_peers(&self) -> usize {
		self.peers.len()
	}

	/// Returns the current sync status.
	pub fn status(&self) -> SyncStatus<B> {
		SyncStatus {
			state: match &self.phase {
				Phase::WaitingForPeers { .. } => SyncState::Downloading { target: Zero::zero() },
				Phase::WarpProof { .. } => SyncState::Downloading { target: Zero::zero() },
				Phase::PendingTargetBlock => SyncState::Downloading { target: Zero::zero() },
				Phase::TargetBlock(header) => SyncState::Downloading { target: *header.number() },
				Phase::Complete => SyncState::Idle,
			},
			best_seen_block: match &self.phase {
				Phase::WaitingForPeers { .. } => None,
				Phase::WarpProof { .. } => None,
				Phase::PendingTargetBlock => None,
				Phase::TargetBlock(header) => Some(*header.number()),
				Phase::Complete => None,
			},
			num_peers: self.peers.len().saturated_into(),
			num_connected_peers: self.peers.len().saturated_into(),
			queued_blocks: 0,
			state_sync: None,
			warp_sync: Some(self.progress()),
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

		std::mem::take(&mut self.actions).into_iter()
	}

	/// Take the result of finished warp sync, returning `None` if the sync was unsuccessful.
	#[must_use]
	pub fn take_result(&mut self) -> Option<WarpSyncResult<B>> {
		self.result.take()
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use sc_block_builder::BlockBuilderBuilder;
	use sp_blockchain::{BlockStatus, Error as BlockchainError, HeaderBackend, Info};
	use sp_consensus_grandpa::{AuthorityList, SetId};
	use sp_runtime::traits::{Block as BlockT, Header as HeaderT, NumberFor};
	use std::{io::ErrorKind, sync::Arc};
	use substrate_test_runtime_client::{
		runtime::{Block, Hash},
		BlockBuilderExt, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};

	mockall::mock! {
		pub Client<B: BlockT> {}

		impl<B: BlockT> HeaderBackend<B> for Client<B> {
			fn header(&self, hash: B::Hash) -> Result<Option<B::Header>, BlockchainError>;
			fn info(&self) -> Info<B>;
			fn status(&self, hash: B::Hash) -> Result<BlockStatus, BlockchainError>;
			fn number(
				&self,
				hash: B::Hash,
			) -> Result<Option<<<B as BlockT>::Header as HeaderT>::Number>, BlockchainError>;
			fn hash(&self, number: NumberFor<B>) -> Result<Option<B::Hash>, BlockchainError>;
		}
	}

	mockall::mock! {
		pub WarpSyncProvider<B: BlockT> {}

		impl<B: BlockT> super::WarpSyncProvider<B> for WarpSyncProvider<B> {
			fn generate(
				&self,
				start: B::Hash,
			) -> Result<EncodedProof, Box<dyn std::error::Error + Send + Sync>>;
			fn verify(
				&self,
				proof: &EncodedProof,
				set_id: SetId,
				authorities: AuthorityList,
			) -> Result<VerificationResult<B>, Box<dyn std::error::Error + Send + Sync>>;
			fn current_authorities(&self) -> AuthorityList;
		}
	}

	fn mock_client_with_state() -> MockClient<Block> {
		let mut client = MockClient::<Block>::new();
		let genesis_hash = Hash::random();
		client.expect_info().return_once(move || Info {
			best_hash: genesis_hash,
			best_number: 0,
			genesis_hash,
			finalized_hash: genesis_hash,
			finalized_number: 0,
			// We need some finalized state to render warp sync impossible.
			finalized_state: Some((genesis_hash, 0)),
			number_leaves: 0,
			block_gap: None,
		});

		client
	}

	fn mock_client_without_state() -> MockClient<Block> {
		let mut client = MockClient::<Block>::new();
		let genesis_hash = Hash::random();
		client.expect_info().returning(move || Info {
			best_hash: genesis_hash,
			best_number: 0,
			genesis_hash,
			finalized_hash: genesis_hash,
			finalized_number: 0,
			finalized_state: None,
			number_leaves: 0,
			block_gap: None,
		});

		client
	}

	#[test]
	fn warp_sync_with_provider_for_db_with_finalized_state_is_noop() {
		let client = mock_client_with_state();
		let provider = MockWarpSyncProvider::<Block>::new();
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Warp sync instantly finishes
		let actions = warp_sync.actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(actions[0], WarpSyncAction::Finished));

		// ... with no result.
		assert!(warp_sync.take_result().is_none());
	}

	#[test]
	fn warp_sync_to_target_for_db_with_finalized_state_is_noop() {
		let client = mock_client_with_state();
		let config = WarpSyncConfig::WaitForTarget;
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Warp sync instantly finishes
		let actions = warp_sync.actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(actions[0], WarpSyncAction::Finished));

		// ... with no result.
		assert!(warp_sync.take_result().is_none());
	}

	#[test]
	fn warp_sync_with_provider_for_empty_db_doesnt_finish_instantly() {
		let client = mock_client_without_state();
		let provider = MockWarpSyncProvider::<Block>::new();
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// No actions are emitted.
		assert_eq!(warp_sync.actions().count(), 0)
	}

	#[test]
	fn warp_sync_to_target_for_empty_db_doesnt_finish_instantly() {
		let client = mock_client_without_state();
		let config = WarpSyncConfig::WaitForTarget;
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// No actions are emitted.
		assert_eq!(warp_sync.actions().count(), 0)
	}

	#[test]
	fn warp_sync_is_started_only_when_there_is_enough_peers() {
		let client = mock_client_without_state();
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Warp sync is not started when there is not enough peers.
		for _ in 0..(MIN_PEERS_TO_START_WARP_SYNC - 1) {
			warp_sync.add_peer(PeerId::random(), Hash::random(), 10);
			assert!(matches!(warp_sync.phase, Phase::WaitingForPeers { .. }))
		}

		// Now we have enough peers and warp sync is started.
		warp_sync.add_peer(PeerId::random(), Hash::random(), 10);
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }))
	}

	#[test]
	fn no_peer_is_scheduled_if_no_peers_connected() {
		let client = mock_client_without_state();
		let provider = MockWarpSyncProvider::<Block>::new();
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		assert!(warp_sync.schedule_next_peer(PeerState::DownloadingProofs, None).is_none());
	}

	#[test]
	fn enough_peers_are_used_in_tests() {
		// Tests below use 10 peers. Fail early if it's less than a threshold for warp sync.
		assert!(
			10 >= MIN_PEERS_TO_START_WARP_SYNC,
			"Tests must be updated to use that many initial peers.",
		);
	}

	#[test]
	fn at_least_median_synced_peer_is_scheduled() {
		for _ in 0..100 {
			let client = mock_client_without_state();
			let mut provider = MockWarpSyncProvider::<Block>::new();
			provider
				.expect_current_authorities()
				.once()
				.return_const(AuthorityList::default());
			let config = WarpSyncConfig::WithProvider(Arc::new(provider));
			let mut warp_sync = WarpSync::new(Arc::new(client), config);

			for best_number in 1..11 {
				warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
			}

			let peer_id = warp_sync.schedule_next_peer(PeerState::DownloadingProofs, None);
			assert!(warp_sync.peers.get(&peer_id.unwrap()).unwrap().best_number >= 6);
		}
	}

	#[test]
	fn min_best_number_peer_is_scheduled() {
		for _ in 0..10 {
			let client = mock_client_without_state();
			let mut provider = MockWarpSyncProvider::<Block>::new();
			provider
				.expect_current_authorities()
				.once()
				.return_const(AuthorityList::default());
			let config = WarpSyncConfig::WithProvider(Arc::new(provider));
			let mut warp_sync = WarpSync::new(Arc::new(client), config);

			for best_number in 1..11 {
				warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
			}

			let peer_id = warp_sync.schedule_next_peer(PeerState::DownloadingProofs, Some(10));
			assert!(warp_sync.peers.get(&peer_id.unwrap()).unwrap().best_number == 10);
		}
	}

	#[test]
	fn no_warp_proof_request_in_another_phase() {
		let client = mock_client_without_state();
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// Manually set to another phase.
		warp_sync.phase = Phase::PendingTargetBlock;

		// No request is made.
		assert!(warp_sync.warp_proof_request().is_none());
	}

	#[test]
	fn warp_proof_request_starts_at_last_hash() {
		let client = mock_client_without_state();
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));

		let known_last_hash = Hash::random();

		// Manually set last hash to known value.
		match &mut warp_sync.phase {
			Phase::WarpProof { last_hash, .. } => {
				*last_hash = known_last_hash;
			},
			_ => panic!("Invalid phase."),
		}

		let (_peer_id, request) = warp_sync.warp_proof_request().unwrap();
		assert_eq!(request.begin, known_last_hash);
	}

	#[test]
	fn no_parallel_warp_proof_requests() {
		let client = mock_client_without_state();
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Make sure we have enough peers to make requests.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));

		// First request is made.
		assert!(warp_sync.warp_proof_request().is_some());
		// Second request is not made.
		assert!(warp_sync.warp_proof_request().is_none());
	}

	#[test]
	fn bad_warp_proof_response_drops_peer() {
		let client = mock_client_without_state();
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		// Warp proof verification fails.
		provider.expect_verify().return_once(|_proof, _set_id, _authorities| {
			Err(Box::new(std::io::Error::new(ErrorKind::Other, "test-verification-failure")))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));

		// Consume `SendWarpProofRequest` action.
		let actions = warp_sync.actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		let WarpSyncAction::SendWarpProofRequest { peer_id: request_peer_id, .. } = actions[0]
		else {
			panic!("Invalid action");
		};

		warp_sync.on_warp_proof_response(&request_peer_id, EncodedProof(Vec::new()));

		// We only interested in alredy generated actions, not new requests.
		let actions = std::mem::take(&mut warp_sync.actions);
		assert_eq!(actions.len(), 1);
		assert!(matches!(
			actions[0],
			WarpSyncAction::DropPeer(BadPeer(peer_id, _rep)) if peer_id == request_peer_id
		));
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));
	}

	#[test]
	fn partial_warp_proof_doesnt_advance_phase() {
		let client = mock_client_without_state();
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		// Warp proof is partial.
		provider.expect_verify().return_once(|_proof, set_id, authorities| {
			Ok(VerificationResult::Partial(set_id, authorities, Hash::random()))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));

		// Consume `SendWarpProofRequest` action.
		let actions = warp_sync.actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		let WarpSyncAction::SendWarpProofRequest { peer_id: request_peer_id, .. } = actions[0]
		else {
			panic!("Invalid action");
		};

		warp_sync.on_warp_proof_response(&request_peer_id, EncodedProof(Vec::new()));

		assert!(warp_sync.actions.is_empty(), "No extra actions generated");
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));
	}

	#[test]
	fn complete_warp_proof_advances_phase() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		let target_header = target_block.header().clone();
		// Warp proof is complete.
		provider.expect_verify().return_once(move |_proof, set_id, authorities| {
			Ok(VerificationResult::Complete(set_id, authorities, target_header))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));

		// Consume `SendWarpProofRequest` action.
		let actions = warp_sync.actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		let WarpSyncAction::SendWarpProofRequest { peer_id: request_peer_id, .. } = actions[0]
		else {
			panic!("Invalid action.");
		};

		warp_sync.on_warp_proof_response(&request_peer_id, EncodedProof(Vec::new()));

		assert!(warp_sync.actions.is_empty(), "No extra actions generated.");
		assert!(
			matches!(warp_sync.phase, Phase::TargetBlock(header) if header == *target_block.header())
		);
	}

	#[test]
	fn no_target_block_requests_in_another_phase() {
		let client = mock_client_without_state();
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(Arc::new(client), config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}
		// We are not in `Phase::TargetBlock`
		assert!(matches!(warp_sync.phase, Phase::WarpProof { .. }));

		// No request is made.
		assert!(warp_sync.target_block_request().is_none());
	}

	#[test]
	fn target_block_request_is_correct() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		let target_header = target_block.header().clone();
		// Warp proof is complete.
		provider.expect_verify().return_once(move |_proof, set_id, authorities| {
			Ok(VerificationResult::Complete(set_id, authorities, target_header))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// Manually set `TargetBlock` phase.
		warp_sync.phase = Phase::TargetBlock(target_block.header().clone());

		let (_peer_id, request) = warp_sync.target_block_request().unwrap();
		assert_eq!(request.from, FromBlock::Hash(target_block.header().hash()));
		assert_eq!(
			request.fields,
			BlockAttributes::HEADER | BlockAttributes::BODY | BlockAttributes::JUSTIFICATION
		);
		assert_eq!(request.max, Some(1));
	}

	#[test]
	fn externally_set_target_block_is_requested() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		let target_header = target_block.header().clone();
		let config = WarpSyncConfig::WaitForTarget;
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// No actions generated so far.
		assert_eq!(warp_sync.actions().count(), 0);

		warp_sync.set_target_block(target_header);
		assert!(matches!(warp_sync.phase, Phase::TargetBlock(_)));

		let (_peer_id, request) = warp_sync.target_block_request().unwrap();
		assert_eq!(request.from, FromBlock::Hash(target_block.header().hash()));
		assert_eq!(
			request.fields,
			BlockAttributes::HEADER | BlockAttributes::BODY | BlockAttributes::JUSTIFICATION
		);
		assert_eq!(request.max, Some(1));
	}

	#[test]
	fn no_parallel_target_block_requests() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		let target_header = target_block.header().clone();
		// Warp proof is complete.
		provider.expect_verify().return_once(move |_proof, set_id, authorities| {
			Ok(VerificationResult::Complete(set_id, authorities, target_header))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// Manually set `TargetBlock` phase.
		warp_sync.phase = Phase::TargetBlock(target_block.header().clone());

		// First target block request is made.
		assert!(warp_sync.target_block_request().is_some());
		// No parallel request is made.
		assert!(warp_sync.target_block_request().is_none());
	}

	#[test]
	fn target_block_response_with_no_blocks_drops_peer() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;
		let target_header = target_block.header().clone();
		// Warp proof is complete.
		provider.expect_verify().return_once(move |_proof, set_id, authorities| {
			Ok(VerificationResult::Complete(set_id, authorities, target_header))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// Manually set `TargetBlock` phase.
		warp_sync.phase = Phase::TargetBlock(target_block.header().clone());

		let (peer_id, request) = warp_sync.target_block_request().unwrap();

		// Empty block response received.
		let response = Vec::new();
		// Peer is dropped.
		assert!(matches!(
			warp_sync.on_block_response_inner(peer_id, request, response),
			Err(BadPeer(id, _rep)) if id == peer_id,
		));
	}

	#[test]
	fn target_block_response_with_extra_blocks_drops_peer() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;

		let mut extra_block_builder = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap();
		extra_block_builder
			.push_storage_change(vec![1, 2, 3], Some(vec![4, 5, 6]))
			.unwrap();
		let extra_block = extra_block_builder.build().unwrap().block;

		let target_header = target_block.header().clone();
		// Warp proof is complete.
		provider.expect_verify().return_once(move |_proof, set_id, authorities| {
			Ok(VerificationResult::Complete(set_id, authorities, target_header))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// Manually set `TargetBlock` phase.
		warp_sync.phase = Phase::TargetBlock(target_block.header().clone());

		let (peer_id, request) = warp_sync.target_block_request().unwrap();

		// Block response with extra blocks received.
		let response = vec![
			BlockData::<Block> {
				hash: target_block.header().hash(),
				header: Some(target_block.header().clone()),
				body: Some(target_block.extrinsics().iter().cloned().collect::<Vec<_>>()),
				indexed_body: None,
				receipt: None,
				message_queue: None,
				justification: None,
				justifications: None,
			},
			BlockData::<Block> {
				hash: extra_block.header().hash(),
				header: Some(extra_block.header().clone()),
				body: Some(extra_block.extrinsics().iter().cloned().collect::<Vec<_>>()),
				indexed_body: None,
				receipt: None,
				message_queue: None,
				justification: None,
				justifications: None,
			},
		];
		// Peer is dropped.
		assert!(matches!(
			warp_sync.on_block_response_inner(peer_id, request, response),
			Err(BadPeer(id, _rep)) if id == peer_id,
		));
	}

	#[test]
	fn target_block_response_with_wrong_block_drops_peer() {
		sp_tracing::try_init_simple();

		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;

		let mut wrong_block_builder = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap();
		wrong_block_builder
			.push_storage_change(vec![1, 2, 3], Some(vec![4, 5, 6]))
			.unwrap();
		let wrong_block = wrong_block_builder.build().unwrap().block;

		let target_header = target_block.header().clone();
		// Warp proof is complete.
		provider.expect_verify().return_once(move |_proof, set_id, authorities| {
			Ok(VerificationResult::Complete(set_id, authorities, target_header))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// Manually set `TargetBlock` phase.
		warp_sync.phase = Phase::TargetBlock(target_block.header().clone());

		let (peer_id, request) = warp_sync.target_block_request().unwrap();

		// Wrong block received.
		let response = vec![BlockData::<Block> {
			hash: wrong_block.header().hash(),
			header: Some(wrong_block.header().clone()),
			body: Some(wrong_block.extrinsics().iter().cloned().collect::<Vec<_>>()),
			indexed_body: None,
			receipt: None,
			message_queue: None,
			justification: None,
			justifications: None,
		}];
		// Peer is dropped.
		assert!(matches!(
			warp_sync.on_block_response_inner(peer_id, request, response),
			Err(BadPeer(id, _rep)) if id == peer_id,
		));
	}

	#[test]
	fn correct_target_block_response_sets_strategy_result() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut provider = MockWarpSyncProvider::<Block>::new();
		provider
			.expect_current_authorities()
			.once()
			.return_const(AuthorityList::default());
		let mut target_block_builder = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap();
		target_block_builder
			.push_storage_change(vec![1, 2, 3], Some(vec![4, 5, 6]))
			.unwrap();
		let target_block = target_block_builder.build().unwrap().block;
		let target_header = target_block.header().clone();
		// Warp proof is complete.
		provider.expect_verify().return_once(move |_proof, set_id, authorities| {
			Ok(VerificationResult::Complete(set_id, authorities, target_header))
		});
		let config = WarpSyncConfig::WithProvider(Arc::new(provider));
		let mut warp_sync = WarpSync::new(client, config);

		// Make sure we have enough peers to make a request.
		for best_number in 1..11 {
			warp_sync.add_peer(PeerId::random(), Hash::random(), best_number);
		}

		// Manually set `TargetBlock` phase.
		warp_sync.phase = Phase::TargetBlock(target_block.header().clone());

		let (peer_id, request) = warp_sync.target_block_request().unwrap();

		// Correct block received.
		let body = Some(target_block.extrinsics().iter().cloned().collect::<Vec<_>>());
		let justifications = Some(Justifications::from((*b"FRNK", Vec::new())));
		let response = vec![BlockData::<Block> {
			hash: target_block.header().hash(),
			header: Some(target_block.header().clone()),
			body: body.clone(),
			indexed_body: None,
			receipt: None,
			message_queue: None,
			justification: None,
			justifications: justifications.clone(),
		}];

		assert!(warp_sync.on_block_response_inner(peer_id, request, response).is_ok());

		// Strategy finishes.
		let actions = warp_sync.actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(actions[0], WarpSyncAction::Finished));

		// With correct result.
		let result = warp_sync.take_result().unwrap();
		assert_eq!(result.target_header, *target_block.header());
		assert_eq!(result.target_body, body);
		assert_eq!(result.target_justifications, justifications);
	}
}
