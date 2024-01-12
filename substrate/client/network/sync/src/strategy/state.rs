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
	schema::v1::StateResponse,
	strategy::state_sync::{ImportResult, StateSync, StateSyncProvider},
	types::{BadPeer, OpaqueStateRequest, OpaqueStateResponse, SyncState, SyncStatus},
	LOG_TARGET,
};
use libp2p::PeerId;
use log::{debug, error, trace};
use sc_client_api::ProofProvider;
use sc_consensus::{BlockImportError, BlockImportStatus, IncomingBlock};
use sc_network_common::sync::message::BlockAnnounce;
use sp_consensus::BlockOrigin;
use sp_runtime::{
	traits::{Block as BlockT, Header, NumberFor},
	Justifications, SaturatedConversion,
};
use std::{collections::HashMap, sync::Arc};

mod rep {
	use sc_network::ReputationChange as Rep;

	/// Peer response data does not have requested bits.
	pub const BAD_RESPONSE: Rep = Rep::new(-(1 << 12), "Incomplete response");

	/// Reputation change for peers which send us a known bad state.
	pub const BAD_STATE: Rep = Rep::new(-(1 << 29), "Bad state");
}

/// Action that should be performed on [`StateStrategy`]'s behalf.
pub enum StateStrategyAction<B: BlockT> {
	/// Send state request to peer.
	SendStateRequest { peer_id: PeerId, request: OpaqueStateRequest },
	/// Disconnect and report peer.
	DropPeer(BadPeer),
	/// Import blocks.
	ImportBlocks { origin: BlockOrigin, blocks: Vec<IncomingBlock<B>> },
	/// State sync has finished.
	Finished,
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
	actions: Vec<StateStrategyAction<B>>,
	succeded: bool,
}

impl<B: BlockT> StateStrategy<B> {
	/// Create a new instance.
	pub fn new<Client>(
		client: Arc<Client>,
		target_header: B::Header,
		target_body: Option<Vec<B::Extrinsic>>,
		target_justifications: Option<Justifications>,
		skip_proof: bool,
		initial_peers: impl Iterator<Item = (PeerId, NumberFor<B>)>,
	) -> Self
	where
		Client: ProofProvider<B> + Send + Sync + 'static,
	{
		let peers = initial_peers
			.map(|(peer_id, best_number)| {
				(peer_id, Peer { best_number, state: PeerState::Available })
			})
			.collect();
		Self {
			state_sync: Box::new(StateSync::new(
				client,
				target_header,
				target_body,
				target_justifications,
				skip_proof,
			)),
			peers,
			actions: Vec::new(),
			succeded: false,
		}
	}

	// Create a new instance with a custom state sync provider.
	// Used in tests.
	#[cfg(test)]
	fn new_with_provider(
		state_sync_provider: Box<dyn StateSyncProvider<B>>,
		initial_peers: impl Iterator<Item = (PeerId, NumberFor<B>)>,
	) -> Self {
		Self {
			state_sync: state_sync_provider,
			peers: initial_peers
				.map(|(peer_id, best_number)| {
					(peer_id, Peer { best_number, state: PeerState::Available })
				})
				.collect(),
			actions: Vec::new(),
			succeded: false,
		}
	}

	/// Notify that a new peer has connected.
	pub fn add_peer(&mut self, peer_id: PeerId, _best_hash: B::Hash, best_number: NumberFor<B>) {
		self.peers.insert(peer_id, Peer { best_number, state: PeerState::Available });
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

	/// Process state response.
	pub fn on_state_response(&mut self, peer_id: PeerId, response: OpaqueStateResponse) {
		if let Err(bad_peer) = self.on_state_response_inner(peer_id, response) {
			self.actions.push(StateStrategyAction::DropPeer(bad_peer));
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

		let response: Box<StateResponse> = response.0.downcast().map_err(|_error| {
			error!(
				target: LOG_TARGET,
				"Failed to downcast opaque state response, this is an implementation bug."
			);
			debug_assert!(false);

			BadPeer(peer_id, rep::BAD_RESPONSE)
		})?;

		debug!(
			target: LOG_TARGET,
			"Importing state data from {} with {} keys, {} proof nodes.",
			peer_id,
			response.entries.len(),
			response.proof.len(),
		);

		match self.state_sync.import(*response) {
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
				self.actions
					.push(StateStrategyAction::ImportBlocks { origin, blocks: vec![block] });
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
			self.succeded |= results.into_iter().any(|result| result.is_ok());
			self.actions.push(StateStrategyAction::Finished);
		}
	}

	/// Produce state request.
	fn state_request(&mut self) -> Option<(PeerId, OpaqueStateRequest)> {
		if self.state_sync.is_complete() {
			return None
		}

		if self
			.peers
			.values()
			.any(|peer| matches!(peer.state, PeerState::DownloadingState))
		{
			// Only one state request at a time is possible.
			return None
		}

		let peer_id =
			self.schedule_next_peer(PeerState::DownloadingState, self.state_sync.target_number())?;
		let request = self.state_sync.next_request();
		trace!(
			target: LOG_TARGET,
			"New state request to {peer_id}: {request:?}.",
		);
		Some((peer_id, OpaqueStateRequest(Box::new(request))))
	}

	fn schedule_next_peer(
		&mut self,
		new_state: PeerState,
		min_best_number: NumberFor<B>,
	) -> Option<PeerId> {
		let mut targets: Vec<_> = self.peers.values().map(|p| p.best_number).collect();
		if targets.is_empty() {
			return None
		}
		targets.sort();
		let median = targets[targets.len() / 2];
		let threshold = std::cmp::max(median, min_best_number);
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
			num_connected_peers: self.peers.len().saturated_into(),
			queued_blocks: 0,
			state_sync: Some(self.state_sync.progress()),
			warp_sync: None,
		}
	}

	/// Get the number of peers known to syncing.
	pub fn num_peers(&self) -> usize {
		self.peers.len()
	}

	/// Get actions that should be performed by the owner on [`WarpSync`]'s behalf
	#[must_use]
	pub fn actions(&mut self) -> impl Iterator<Item = StateStrategyAction<B>> {
		let state_request = self
			.state_request()
			.into_iter()
			.map(|(peer_id, request)| StateStrategyAction::SendStateRequest { peer_id, request });
		self.actions.extend(state_request);

		std::mem::take(&mut self.actions).into_iter()
	}

	/// Check if state sync has succeded.
	#[must_use]
	pub fn is_succeded(&self) -> bool {
		self.succeded
	}
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		schema::v1::{StateRequest, StateResponse},
		strategy::state_sync::{ImportResult, StateSyncProgress, StateSyncProvider},
	};
	use codec::Decode;
	use sc_block_builder::BlockBuilderBuilder;
	use sc_client_api::KeyValueStates;
	use sc_consensus::{ImportedAux, ImportedState};
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

		let mut state_strategy =
			StateStrategy::new(client, target_header, None, None, false, std::iter::empty());

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
				false,
				initial_peers,
			);

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
				false,
				initial_peers,
			);

			let peer_id = state_strategy.schedule_next_peer(PeerState::DownloadingState, 10);
			assert!(*peers.get(&peer_id.unwrap()).unwrap() == 10);
		}
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
			false,
			initial_peers,
		);

		let (_peer_id, mut opaque_request) = state_strategy.state_request().unwrap();
		let request: &mut StateRequest = opaque_request.0.downcast_mut().unwrap();
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
			false,
			initial_peers,
		);

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
		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), initial_peers);
		// Manually set the peer's state.
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;

		let dummy_response = OpaqueStateResponse(Box::new(StateResponse::default()));
		state_strategy.on_state_response(peer_id, dummy_response);

		assert!(state_strategy.peers.get(&peer_id).unwrap().state.is_available());
	}

	#[test]
	fn bad_state_response_drops_peer() {
		let mut state_sync_provider = MockStateSync::<Block>::new();
		// Provider says that state response is bad.
		state_sync_provider.expect_import().return_once(|_| ImportResult::BadResponse);
		let peer_id = PeerId::random();
		let initial_peers = std::iter::once((peer_id, 10));
		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), initial_peers);
		// Manually set the peer's state.
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;
		let dummy_response = OpaqueStateResponse(Box::new(StateResponse::default()));
		// Receiving response drops the peer.
		assert!(matches!(
			state_strategy.on_state_response_inner(peer_id, dummy_response),
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
		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), initial_peers);
		// Manually set the peer's state .
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;

		let dummy_response = OpaqueStateResponse(Box::new(StateResponse::default()));
		state_strategy.on_state_response(peer_id, dummy_response);

		// No actions generated.
		assert_eq!(state_strategy.actions.len(), 0)
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
		let state = ImportedState { block: hash, state: KeyValueStates(Vec::new()) };
		let justifications = Some(Justifications::from((*b"FRNK", Vec::new())));

		// Prepare `StateSync`
		let mut state_sync_provider = MockStateSync::<Block>::new();
		let import = ImportResult::Import(
			hash,
			header.clone(),
			state.clone(),
			body.clone(),
			justifications.clone(),
		);
		state_sync_provider.expect_import().return_once(move |_| import);

		// Reference values to check against.
		let expected_origin = BlockOrigin::NetworkInitialSync;
		let expected_block = IncomingBlock {
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
		let expected_blocks = vec![expected_block];

		// Prepare `StateStrategy`.
		let peer_id = PeerId::random();
		let initial_peers = std::iter::once((peer_id, 10));
		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), initial_peers);
		// Manually set the peer's state .
		state_strategy.peers.get_mut(&peer_id).unwrap().state = PeerState::DownloadingState;

		// Receive response.
		let dummy_response = OpaqueStateResponse(Box::new(StateResponse::default()));
		state_strategy.on_state_response(peer_id, dummy_response);

		assert_eq!(state_strategy.actions.len(), 1);
		assert!(matches!(
			&state_strategy.actions[0],
			StateStrategyAction::ImportBlocks { origin, blocks }
				if *origin == expected_origin && *blocks == expected_blocks,
		));
	}

	#[test]
	fn importing_unknown_block_doesnt_finish_strategy() {
		let target_hash = Hash::random();
		let unknown_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);

		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), std::iter::empty());

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
	fn succesfully_importing_target_block_finishes_strategy() {
		let target_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);

		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), std::iter::empty());

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
		assert!(matches!(&state_strategy.actions[0], StateStrategyAction::Finished));
	}

	#[test]
	fn failure_to_import_target_block_finishes_strategy() {
		let target_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);

		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), std::iter::empty());

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
		assert!(matches!(&state_strategy.actions[0], StateStrategyAction::Finished));
	}

	#[test]
	fn finished_strategy_doesnt_generate_more_actions() {
		let target_hash = Hash::random();
		let mut state_sync_provider = MockStateSync::<Block>::new();
		state_sync_provider.expect_target_hash().return_const(target_hash);
		state_sync_provider.expect_is_complete().return_const(true);

		// Get enough peers for possible spurious requests.
		let initial_peers = (1..=10).map(|best_number| (PeerId::random(), best_number));

		let mut state_strategy =
			StateStrategy::new_with_provider(Box::new(state_sync_provider), initial_peers);

		state_strategy.on_blocks_processed(
			1,
			1,
			vec![(
				Ok(BlockImportStatus::ImportedUnknown(1, ImportedAux::default(), None)),
				target_hash,
			)],
		);

		// Strategy finishes.
		let actions = state_strategy.actions().collect::<Vec<_>>();
		assert_eq!(actions.len(), 1);
		assert!(matches!(&actions[0], StateStrategyAction::Finished));

		// No more actions generated.
		assert_eq!(state_strategy.actions().count(), 0);
	}
}
