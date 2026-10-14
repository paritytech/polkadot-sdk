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

//! State sync strategy.

use crate::{
	schema::v1::{StateRequest, StateResponse},
	service::network::NetworkServiceHandle,
	strategy::{
		disconnected_peers::DisconnectedPeers,
		state_sync::{ImportResult, StateSync, StateSyncProvider},
		StrategyKey, SyncingAction,
	},
	types::{BadPeer, SyncState, SyncStatus},
	LOG_TARGET,
};
use futures::{channel::oneshot, FutureExt};
use log::{debug, error, trace};
use prost::Message;
use sc_client_api::{PartialStateImport, ProofProvider};
use sc_consensus::{BlockImportError, BlockImportStatus, IncomingBlock};
use sc_network::{IfDisconnected, ProtocolName};
use sc_network_common::sync::message::BlockAnnounce;
use sc_network_types::PeerId;
use sp_consensus::BlockOrigin;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	Justifications, SaturatedConversion,
};
use std::{any::Any, collections::HashMap, sync::Arc};

mod rep {
	use sc_network::ReputationChange as Rep;

	/// Peer response data does not have requested bits.
	pub const BAD_RESPONSE: Rep = Rep::new(-(1 << 12), "Incomplete response");

	/// Reputation change for peers which send us a known bad state.
	pub const BAD_STATE: Rep = Rep::new(-(1 << 29), "Bad state");
}

enum PeerState {
	Available,
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

/// Syncing strategy that downloads and imports a recent state directly.
pub struct StateStrategy<B: BlockT> {
	state_sync: Box<dyn StateSyncProvider<B>>,
	peers: HashMap<PeerId, Peer<B>>,
	disconnected_peers: DisconnectedPeers,
	actions: Vec<SyncingAction<B>>,
	protocol_name: ProtocolName,
	succeeded: bool,
	/// Set once the state sync gave up on its target. No more requests after that.
	aborted: bool,
}

impl<B: BlockT> StateStrategy<B> {
	/// Strategy key used by state sync.
	pub const STRATEGY_KEY: StrategyKey = StrategyKey::new("State");

	/// Create a new instance. Fails if the client cannot record the target, see [`StateSync::new`].
	pub fn new<Client>(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
		initial_peers: impl Iterator<Item = (PeerId, NumberFor<B>)>,
		protocol_name: ProtocolName,
	) -> sp_blockchain::Result<Self>
	where
		Client: ProofProvider<B> + PartialStateImport<B> + Send + Sync + 'static,
	{
		let state_sync = StateSync::new(client, target_header, target_body, target_justifications)?;
		Ok(Self::new_with_provider(Box::new(state_sync), initial_peers, protocol_name))
	}

	/// Create a new instance with a custom state sync provider.
	///
	/// Note: In most cases, users should use [`StateStrategy::new`].
	/// This method is intended for custom sync strategies and advanced use cases.
	pub fn new_with_provider(
		state_sync_provider: Box<dyn StateSyncProvider<B>>,
		initial_peers: impl Iterator<Item = (PeerId, NumberFor<B>)>,
		protocol_name: ProtocolName,
	) -> Self {
		Self {
			state_sync: state_sync_provider,
			peers: initial_peers
				.map(|(peer_id, best_number)| {
					(peer_id, Peer { best_number, state: PeerState::Available })
				})
				.collect(),
			disconnected_peers: DisconnectedPeers::new(),
			actions: Vec::new(),
			protocol_name,
			succeeded: false,
			aborted: false,
		}
	}

	/// Notify that a new peer has connected.
	pub fn add_peer(&mut self, peer_id: PeerId, _best_hash: B::Hash, best_number: NumberFor<B>) {
		self.peers.insert(peer_id, Peer { best_number, state: PeerState::Available });
	}

	/// Notify that a peer has disconnected.
	pub fn remove_peer(&mut self, peer_id: &PeerId) {
		if let Some(state) = self.peers.remove(peer_id) {
			if !state.state.is_available() {
				if let Some(bad_peer) =
					self.disconnected_peers.on_disconnect_during_request(*peer_id)
				{
					self.actions.push(SyncingAction::DropPeer(bad_peer));
				}
			}
		}
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
		is_best.then(|| {
			let best_number = *announce.header.number();
			let best_hash = announce.header.hash();
			if let Some(ref mut peer) = self.peers.get_mut(&peer_id) {
				peer.best_number = best_number;
			}
			// Let `SyncingEngine` know that we should update the peer info.
			(best_hash, best_number)
		})
	}

	/// Process state response.
	pub fn on_state_response(&mut self, peer_id: &PeerId, response: Vec<u8>) {
		if let Err(bad_peer) = self.on_state_response_inner(peer_id, &response) {
			self.actions.push(SyncingAction::DropPeer(bad_peer));
		}
	}

	fn on_state_response_inner(
		&mut self,
		peer_id: &PeerId,
		response: &[u8],
	) -> Result<(), BadPeer> {
		if let Some(peer) = self.peers.get_mut(&peer_id) {
			peer.state = PeerState::Available;
		}

		let response = match StateResponse::decode(response) {
			Ok(response) => response,
			Err(error) => {
				debug!(
					target: LOG_TARGET,
					"Failed to decode state response from peer {peer_id:?}: {error:?}.",
				);

				return Err(BadPeer(*peer_id, rep::BAD_RESPONSE));
			},
		};

		debug!(
			target: LOG_TARGET,
			"Importing state data from {}, {} bytes of proof.",
			peer_id,
			response.proof.len(),
		);

		match self.state_sync.import(response) {
			ImportResult::Import(block) => {
				self.queue_target_import(block);
				Ok(())
			},
			ImportResult::Continue => Ok(()),
			ImportResult::Abort => {
				self.abort();
				Ok(())
			},
			ImportResult::BadResponse => {
				debug!(target: LOG_TARGET, "Bad state data received from {peer_id}");
				Err(BadPeer(*peer_id, rep::BAD_STATE))
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
		trace!(target: LOG_TARGET, "State sync: imported {imported} of {count}.");

		let results = results
			.into_iter()
			.filter_map(|(result, hash)| {
				if hash == self.state_sync.target_hash() {
					Some(result)
				} else {
					debug!(
						target: LOG_TARGET,
						"Unexpected block processed: {hash} with result {result:?}.",
					);
					None
				}
			})
			.collect::<Vec<_>>();

		if !results.is_empty() {
			// We processed the target block
			results.iter().filter_map(|result| result.as_ref().err()).for_each(|e| {
				error!(
					target: LOG_TARGET,
					"Failed to import target block with state: {e:?}."
				);
			});
			self.succeeded |= results.into_iter().any(|result| result.is_ok());
			self.actions.push(SyncingAction::Finished);
		}
	}

	/// Queue the import of the target block. Its whole state is in the database, and the block
	/// import checks that it is complete and marks it present.
	fn queue_target_import(&mut self, block: IncomingBlock<B>) {
		debug!(target: LOG_TARGET, "State download is complete. Import is queued");
		self.actions.push(SyncingAction::ImportBlocks {
			origin: BlockOrigin::NetworkInitialSync,
			blocks: vec![block],
		});
	}

	/// Give up on the target: the strategy finishes without success and requests nothing more.
	fn abort(&mut self) {
		error!(
			target: LOG_TARGET,
			"State sync aborted: the state could not be written to the database. Falling back to \
			 full sync.",
		);
		self.aborted = true;
		self.actions.push(SyncingAction::Finished);
	}

	/// Produce state request.
	fn state_request(&mut self) -> Option<(PeerId, StateRequest)> {
		// After an abort `Finished` is already queued, but `actions()` would still send one more
		// request before the holder swaps strategies, and its response would land on a strategy
		// that does not expect it.
		if self.aborted {
			return None;
		}

		// Nothing to ask while a chunk waits to be written: its retry decides what is next.
		if self.state_sync.is_complete() || self.state_sync.has_pending_write() {
			return None;
		}

		if self
			.peers
			.values()
			.any(|peer| matches!(peer.state, PeerState::DownloadingState))
		{
			// Only one state request at a time is possible.
			return None;
		}

		let peer_id =
			self.schedule_next_peer(PeerState::DownloadingState, self.state_sync.target_number())?;
		let request = self.state_sync.next_request();
		trace!(
			target: LOG_TARGET,
			"New state request to {peer_id}: {request:?}.",
		);
		Some((peer_id, request))
	}

	fn schedule_next_peer(
		&mut self,
		new_state: PeerState,
		min_best_number: NumberFor<B>,
	) -> Option<PeerId> {
		let mut targets: Vec<_> = self.peers.values().map(|p| p.best_number).collect();
		if targets.is_empty() {
			return None;
		}
		targets.sort();
		let median = targets[targets.len() / 2];
		let threshold = std::cmp::max(median, min_best_number);
		// Find a random peer that is synced as much as peer majority and is above
		// `min_best_number`.
		for (peer_id, peer) in self.peers.iter_mut() {
			if peer.state.is_available() &&
				peer.best_number >= threshold &&
				self.disconnected_peers.is_peer_available(peer_id)
			{
				peer.state = new_state;
				return Some(*peer_id);
			}
		}
		None
	}

	/// Returns the current sync status.
	pub fn status(&self) -> SyncStatus<B> {
		SyncStatus {
			state: if self.state_sync.is_complete() {
				SyncState::Idle
			} else {
				SyncState::Downloading { target: self.state_sync.target_number() }
			},
			best_seen_block: Some(self.state_sync.target_number()),
			num_peers: self.peers.len().saturated_into(),
			queued_blocks: 0,
			state_sync: Some(self.state_sync.progress()),
			warp_sync: None,
		}
	}

	/// Get actions that should be performed.
	#[must_use]
	pub fn actions(
		&mut self,
		network_service: &NetworkServiceHandle,
	) -> impl Iterator<Item = SyncingAction<B>> {
		// A download that completed in an earlier run has only the import left.
		if let Some(block) = self.state_sync.take_target_import() {
			self.queue_target_import(block);
		}
		// Retry a pending chunk write.
		match self.state_sync.retry_pending_write() {
			Some(ImportResult::Import(block)) => self.queue_target_import(block),
			Some(ImportResult::Abort) => self.abort(),
			Some(ImportResult::Continue | ImportResult::BadResponse) | None => {},
		}

		let state_request = self.state_request().into_iter().map(|(peer_id, request)| {
			let (tx, rx) = oneshot::channel();

			network_service.start_request(
				peer_id,
				self.protocol_name.clone(),
				request.encode_to_vec(),
				tx,
				IfDisconnected::ImmediateError,
			);

			SyncingAction::StartRequest {
				peer_id,
				key: Self::STRATEGY_KEY,
				request: async move {
					Ok(rx.await?.and_then(|(response, protocol_name)| {
						Ok((Box::new(response) as Box<dyn Any + Send>, protocol_name))
					}))
				}
				.boxed(),
			}
		});
		self.actions.extend(state_request);

		std::mem::take(&mut self.actions).into_iter()
	}

	/// Check if state sync has succeeded.
	#[must_use]
	pub fn is_succeeded(&self) -> bool {
		self.succeeded
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		schema::v1::{StateRequest, StateResponse},
		service::network::NetworkServiceProvider,
		strategy::state_sync::{ImportResult, StateSyncProgress, StateSyncProvider},
	};
	use codec::Decode;
	use sc_block_builder::BlockBuilderBuilder;
	use sc_client_api::partial_state::{PartialStateTarget, ResumablePartialState};
	use sc_consensus::{ImportedAux, ImportedState};
	use sp_core::H256;
	use sp_runtime::traits::Zero;
	use substrate_test_runtime_client::{
		runtime::{Block, Hash},
		BlockBuilderExt, DefaultTestClientBuilderExt, TestClientBuilder, TestClientBuilderExt,
	};

	mockall::mock! {
		pub StateSync<B: BlockT> {}

		impl<B: BlockT> StateSyncProvider<B> for StateSync<B> {
			fn import(&mut self, response: StateResponse) -> ImportResult<B>;
			fn next_request(&self) -> StateRequest;
			fn take_target_import(&mut self) -> Option<IncomingBlock<B>>;
			fn retry_pending_write(&mut self) -> Option<ImportResult<B>>;
			fn has_pending_write(&self) -> bool;
			fn is_complete(&self) -> bool;
			fn target_number(&self) -> NumberFor<B>;
			fn target_hash(&self) -> B::Hash;
			fn progress(&self) -> StateSyncProgress;
		}
	}

	#[test]
	fn no_peer_is_scheduled_if_no_peers_connected() {
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

		let mut state_strategy = StateStrategy::new(
			client,
			target_header,
			None,
			None,
			std::iter::empty(),
			ProtocolName::Static(""),
		)
		.unwrap();

		assert!(state_strategy
			.schedule_next_peer(PeerState::DownloadingState, Zero::zero())
			.is_none());
	}

	#[test]
	fn at_least_median_synced_peer_is_scheduled() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;

		for _ in 0..100 {
			let peers = (1..=10)
				.map(|best_number| (PeerId::random(), best_number))
				.collect::<HashMap<_, _>>();
			let initial_peers = peers.iter().map(|(p, n)| (*p, *n));

			let mut state_strategy = StateStrategy::new(
				client.clone(),
				target_block.header().clone(),
				None,
				None,
				initial_peers,
				ProtocolName::Static(""),
			)
			.unwrap();

			let peer_id =
				state_strategy.schedule_next_peer(PeerState::DownloadingState, Zero::zero());
			assert!(*peers.get(&peer_id.unwrap()).unwrap() >= 6);
		}
	}

	#[test]
	fn min_best_number_peer_is_scheduled() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;

		for _ in 0..10 {
			let peers = (1..=10)
				.map(|best_number| (PeerId::random(), best_number))
				.collect::<HashMap<_, _>>();
			let initial_peers = peers.iter().map(|(p, n)| (*p, *n));

			let mut state_strategy = StateStrategy::new(
				client.clone(),
				target_block.header().clone(),
				None,
				None,
				initial_peers,
				ProtocolName::Static(""),
			)
			.unwrap();

			let peer_id = state_strategy.schedule_next_peer(PeerState::DownloadingState, 10);
			assert!(*peers.get(&peer_id.unwrap()).unwrap() == 10);
		}
	}

	#[test]
	fn backedoff_number_peer_is_not_scheduled() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;

		let peers = (1..=10)
			.map(|best_number| (PeerId::random(), best_number))
			.collect::<Vec<(_, _)>>();
		let ninth_peer = peers[8].0;
		let tenth_peer = peers[9].0;
		let initial_peers = peers.iter().map(|(p, n)| (*p, *n));

		let mut state_strategy = StateStrategy::new(
			client.clone(),
			target_block.header().clone(),
			None,
			None,
			initial_peers,
			ProtocolName::Static(""),
		)
		.unwrap();

		// Disconnecting a peer without an inflight request has no effect on persistent states.
		state_strategy.remove_peer(&tenth_peer);
		assert!(state_strategy.disconnected_peers.is_peer_available(&tenth_peer));

		// Disconnect the peer with an inflight request.
		state_strategy.add_peer(tenth_peer, H256::random(), 10);
		let peer_id: Option<PeerId> =
			state_strategy.schedule_next_peer(PeerState::DownloadingState, 10);
		assert_eq!(tenth_peer, peer_id.unwrap());
		state_strategy.remove_peer(&tenth_peer);

		// Peer is backed off.
		assert!(!state_strategy.disconnected_peers.is_peer_available(&tenth_peer));

		// No peer available for 10'th best block because of the backoff.
		state_strategy.add_peer(tenth_peer, H256::random(), 10);
		let peer_id: Option<PeerId> =
			state_strategy.schedule_next_peer(PeerState::DownloadingState, 10);
		assert!(peer_id.is_none());

		// Other requests can still happen.
		let peer_id: Option<PeerId> =
			state_strategy.schedule_next_peer(PeerState::DownloadingState, 9);
		assert_eq!(ninth_peer, peer_id.unwrap());
	}

	#[test]
	fn state_request_contains_correct_hash() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;

		let initial_peers = (1..=10).map(|best_number| (PeerId::random(), best_number));

		let mut state_strategy = StateStrategy::new(
			client.clone(),
			target_block.header().clone(),
			None,
			None,
			initial_peers,
			ProtocolName::Static(""),
		)
		.unwrap();

		let (_peer_id, request) = state_strategy.state_request().unwrap();
		let hash = Hash::decode(&mut &*request.block).unwrap();

		assert_eq!(hash, target_block.header().hash());
	}

	#[test]
	fn no_parallel_state_requests() {
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let target_block = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap()
			.build()
			.unwrap()
			.block;

		let initial_peers = (1..=10).map(|best_number| (PeerId::random(), best_number));

		let mut state_strategy = StateStrategy::new(
			client.clone(),
			target_block.header().clone(),
			None,
			None,
			initial_peers,
			ProtocolName::Static(""),
		)
		.unwrap();

		// First request is sent.
		assert!(state_strategy.state_request().is_some());

		// No parallel request is sent.
		assert!(state_strategy.state_request().is_none());
	}

	#[test]
	fn received_state_response_makes_peer_available_again() {
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_import().return_once(|_| ImportResult::Continue);
		let peer_id = PeerId::random();
		let initial_peers = std::iter::once((peer_id, 10));
		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			initial_peers,
			ProtocolName::Static(""),
		);
		// Manually set the peer's state.
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;

		let dummy_response = StateResponse::default().encode_to_vec();
		state_strategy.on_state_response(&peer_id, dummy_response);

		assert!(state_strategy.peers.get(&peer_id).unwrap().state.is_available());
	}

	#[test]
	fn bad_state_response_drops_peer() {
		let mut state_sync_provider = MockStateSync::<Block>::new();
		// Provider says that state response is bad.
		state_sync_provider.expect_import().return_once(|_| ImportResult::BadResponse);
		let peer_id = PeerId::random();
		let initial_peers = std::iter::once((peer_id, 10));
		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			initial_peers,
			ProtocolName::Static(""),
		);
		// Manually set the peer's state.
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;
		let dummy_response = StateResponse::default().encode_to_vec();
		// Receiving response drops the peer.
		assert!(matches!(
			state_strategy.on_state_response_inner(&peer_id, &dummy_response),
			Err(BadPeer(id, _rep)) if id == peer_id,
		));
	}

	#[test]
	fn partial_state_response_doesnt_generate_actions() {
		let mut state_sync_provider = MockStateSync::<Block>::new();
		// Sync provider says that the response is partial.
		state_sync_provider.expect_import().return_once(|_| ImportResult::Continue);
		let peer_id = PeerId::random();
		let initial_peers = std::iter::once((peer_id, 10));
		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			initial_peers,
			ProtocolName::Static(""),
		);
		// Manually set the peer's state .
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;

		let dummy_response = StateResponse::default().encode_to_vec();
		state_strategy.on_state_response(&peer_id, dummy_response);

		// No actions generated.
		assert_eq!(state_strategy.actions.len(), 0)
	}

	#[test]
	fn abort_finishes_without_success_and_stops_requesting() {
		let mut state_sync_provider = MockStateSync::<Block>::new();
		// Sync provider gave up on the target.
		state_sync_provider.expect_import().return_once(|_| ImportResult::Abort);
		state_sync_provider.expect_take_target_import().returning(|| None);
		state_sync_provider.expect_retry_pending_write().returning(|| None);
		let peer_id = PeerId::random();
		let initial_peers = std::iter::once((peer_id, 10));
		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			initial_peers,
			ProtocolName::Static(""),
		);
		// Manually set the peer's state .
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;

		let dummy_response = StateResponse::default().encode_to_vec();
		state_strategy.on_state_response(&peer_id, dummy_response);

		// The strategy finishes without success and, although the peer is available again, it
		// schedules no further state request.
		let network_provider = NetworkServiceProvider::new();
		let network_handle = network_provider.handle();
		let actions = state_strategy.actions(&network_handle).collect::<Vec<_>>();
		assert!(matches!(actions.as_slice(), [SyncingAction::Finished]));
		assert!(!state_strategy.is_succeeded());
	}

	/// A strategy whose provider has a chunk write pending and answers `result` to its retry.
	fn strategy_with_pending_write(result: Option<ImportResult<Block>>) -> StateStrategy<Block> {
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_take_target_import().returning(|| None);
		state_sync_provider.expect_retry_pending_write().return_once(move || result);
		state_sync_provider.expect_is_complete().return_const(false);
		state_sync_provider.expect_has_pending_write().return_const(true);
		StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			std::iter::once((PeerId::random(), 10)),
			ProtocolName::Static(""),
		)
	}

	/// While a chunk waits to be written no state request goes out, and its retry decides what
	/// is next: the final chunk's retry queues the target import like its response would, and a
	/// retry that gives up finishes the strategy without success.
	#[test]
	fn pending_write_blocks_requests_and_its_retry_decides() {
		let network_provider = NetworkServiceProvider::new();
		let network_handle = network_provider.handle();

		let mut waiting = strategy_with_pending_write(None);
		assert_eq!(waiting.actions(&network_handle).count(), 0);

		let block = IncomingBlock {
			hash: Hash::random(),
			header: None,
			body: None,
			justifications: None,
			origin: None,
			allow_missing_state: true,
			import_existing: true,
			skip_execution: true,
			state: Some(ImportedState),
		};
		let target_hash = block.hash;
		let mut written = strategy_with_pending_write(Some(ImportResult::Import(block)));
		let actions = written.actions(&network_handle).collect::<Vec<_>>();
		assert!(matches!(
			actions.as_slice(),
			[SyncingAction::ImportBlocks { blocks, .. }]
				if blocks.len() == 1 && blocks[0].hash == target_hash
		));

		let mut given_up = strategy_with_pending_write(Some(ImportResult::Abort));
		let actions = given_up.actions(&network_handle).collect::<Vec<_>>();
		assert!(matches!(actions.as_slice(), [SyncingAction::Finished]));
		assert!(!given_up.is_succeeded());
	}

	#[test]
	fn complete_state_response_leads_to_block_import() {
		// Build block to use for checks.
		let client = Arc::new(TestClientBuilder::new().set_no_genesis().build());
		let mut block_builder = BlockBuilderBuilder::new(&*client)
			.on_parent_block(client.chain_info().best_hash)
			.with_parent_block_number(client.chain_info().best_number)
			.build()
			.unwrap();
		block_builder.push_storage_change(vec![1, 2, 3], Some(vec![4, 5, 6])).unwrap();
		let block = block_builder.build().unwrap().block;
		let header = block.header().clone();
		let hash = header.hash();
		let body = Some(block.extrinsics().iter().cloned().collect::<Vec<_>>());
		let justifications = Some(Justifications::from((*b"FRNK", Vec::new())));

		// Reference values to check against.
		let expected_origin = BlockOrigin::NetworkInitialSync;
		let expected_block = IncomingBlock {
			hash,
			header: Some(header),
			body,
			justifications,
			origin: None,
			allow_missing_state: true,
			import_existing: true,
			skip_execution: true,
			state: Some(ImportedState),
		};

		// Prepare `StateSync`
		let mut state_sync_provider = MockStateSync::<Block>::new();
		let import = ImportResult::Import(expected_block.clone());
		state_sync_provider.expect_import().return_once(move |_| import);

		let expected_blocks = vec![expected_block];

		// Prepare `StateStrategy`.
		let peer_id = PeerId::random();
		let initial_peers = std::iter::once((peer_id, 10));
		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			initial_peers,
			ProtocolName::Static(""),
		);
		// Manually set the peer's state .
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;

		// Receive response.
		let dummy_response = StateResponse::default().encode_to_vec();
		state_strategy.on_state_response(&peer_id, dummy_response);

		assert_eq!(state_strategy.actions.len(), 1);
		assert!(matches!(
			&state_strategy.actions[0],
			SyncingAction::ImportBlocks { origin, blocks }
				if *origin == expected_origin && *blocks == expected_blocks,
		));
	}

	/// A download completed in an earlier run: one import at once, no request, done when it lands.
	#[test]
	fn resumed_complete_attempt_imports_the_target_without_a_request() {
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
		let target_hash = target_header.hash();
		let resumable = ResumablePartialState {
			target: PartialStateTarget { header: target_header, body: None, justifications: None },
			cursor: vec![vec![0x80]],
			complete: true,
		};
		let initial_peers = std::iter::once((PeerId::random(), 10));
		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(StateSync::resume(client, resumable)),
			initial_peers,
			ProtocolName::Static(""),
		);

		let network_provider = NetworkServiceProvider::new();
		let network_handle = network_provider.handle();
		let actions = state_strategy.actions(&network_handle).collect::<Vec<_>>();
		assert!(matches!(
			actions.as_slice(),
			[SyncingAction::ImportBlocks { blocks, .. }]
				if blocks.len() == 1 && blocks[0].hash == target_hash && blocks[0].state.is_some()
		));
		assert_eq!(state_strategy.actions(&network_handle).count(), 0, "no request, one import");

		state_strategy.on_blocks_processed(
			1,
			1,
			vec![(
				Ok(BlockImportStatus::ImportedUnknown(1, ImportedAux::default(), None)),
				target_hash,
			)],
		);
		let actions = state_strategy.actions(&network_handle).collect::<Vec<_>>();
		assert!(matches!(actions.as_slice(), [SyncingAction::Finished]));
		assert!(state_strategy.is_succeeded());
	}

	#[test]
	fn importing_unknown_block_doesnt_finish_strategy() {
		let target_hash = Hash::random();
		let unknown_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);

		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			std::iter::empty(),
			ProtocolName::Static(""),
		);

		// Unknown block imported.
		state_strategy.on_blocks_processed(
			1,
			1,
			vec![(
				Ok(BlockImportStatus::ImportedUnknown(1, ImportedAux::default(), None)),
				unknown_hash,
			)],
		);

		// No actions generated.
		assert_eq!(state_strategy.actions.len(), 0);
	}

	#[test]
	fn successfully_importing_target_block_finishes_strategy() {
		let target_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);

		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			std::iter::empty(),
			ProtocolName::Static(""),
		);

		// Target block imported.
		state_strategy.on_blocks_processed(
			1,
			1,
			vec![(
				Ok(BlockImportStatus::ImportedUnknown(1, ImportedAux::default(), None)),
				target_hash,
			)],
		);

		// Strategy finishes.
		assert_eq!(state_strategy.actions.len(), 1);
		assert!(matches!(&state_strategy.actions[0], SyncingAction::Finished));
	}

	#[test]
	fn failure_to_import_target_block_finishes_strategy() {
		let target_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);

		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			std::iter::empty(),
			ProtocolName::Static(""),
		);

		// Target block import failed.
		state_strategy.on_blocks_processed(
			1,
			1,
			vec![(
				Err(BlockImportError::VerificationFailed(None, String::from("test-error"))),
				target_hash,
			)],
		);

		// Strategy finishes.
		assert_eq!(state_strategy.actions.len(), 1);
		assert!(matches!(&state_strategy.actions[0], SyncingAction::Finished));
	}

	#[test]
	fn finished_strategy_doesnt_generate_more_actions() {
		let target_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);
		state_sync_provider.expect_is_complete().return_const(true);
		state_sync_provider.expect_take_target_import().returning(|| None);
		state_sync_provider.expect_retry_pending_write().returning(|| None);

		// Get enough peers for possible spurious requests.
		let initial_peers = (1..=10).map(|best_number| (PeerId::random(), best_number));

		let mut state_strategy = StateStrategy::new_with_provider(
			Box::new(state_sync_provider),
			initial_peers,
			ProtocolName::Static(""),
		);

		state_strategy.on_blocks_processed(
			1,
			1,
			vec![(
				Ok(BlockImportStatus::ImportedUnknown(1, ImportedAux::default(), None)),
				target_hash,
			)],
		);

		let network_provider = NetworkServiceProvider::new();
		let network_handle = network_provider.handle();

		// Strategy finishes.
		let actions = state_strategy.actions(&network_handle).collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(&actions[0], SyncingAction::Finished));

		// No more actions generated.
		assert_eq!(state_strategy.actions(&network_handle).count(), 0);
	}
}
