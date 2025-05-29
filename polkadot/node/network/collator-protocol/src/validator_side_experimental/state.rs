// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::{
	validator_side::notify_collation_seconded,
	validator_side_experimental::{
		collation_manager::CollationManager,
		common::{
			Advertisement, CanSecond, CollationFetchResponse, PeerInfo, PeerState,
			ProspectiveCandidate, TryAcceptOutcome, INVALID_COLLATION_SLASH,
		},
		error::FatalResult,
		peer_manager::Backend,
		Metrics, PeerManager,
	},
	LOG_TARGET,
};
use fatality::Split;
use futures::stream::FusedStream;
use polkadot_node_network_protocol::{peer_set::CollationVersion, OurView, PeerId};
use polkadot_node_primitives::{SignedFullStatement, Statement};
use polkadot_node_subsystem::{
	messages::{CandidateBackingMessage, IfDisconnected, NetworkBridgeTxMessage},
	CollatorProtocolSenderTrait,
};
use polkadot_primitives::{
	vstaging::CandidateReceiptV2 as CandidateReceipt, BlockNumber, Hash, Id as ParaId,
};
use sp_keystore::KeystorePtr;

/// All state relevant for the validator side of the protocol lives here.
pub struct State<B> {
	peer_manager: PeerManager<B>,
	collation_manager: CollationManager,
	keystore: KeystorePtr,
	_metrics: Metrics,
}

impl<B: Backend> State<B> {
	/// Instantiate a new subsystem `State`.
	pub fn new(
		peer_manager: PeerManager<B>,
		collation_manager: CollationManager,
		keystore: KeystorePtr,
		metrics: Metrics,
	) -> Self {
		Self { peer_manager, collation_manager, keystore, _metrics: metrics }
	}

	/// Handle a new peer connection.
	pub async fn handle_peer_connected<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		version: CollationVersion,
	) {
		let outcome = self
			.peer_manager
			.try_accept_connection(
				sender,
				peer_id,
				PeerInfo { version, state: PeerState::Connected },
			)
			.await;

		match outcome {
			TryAcceptOutcome::Added => {},
			TryAcceptOutcome::Replaced(other_peers) => {
				gum::trace!(
					target: LOG_TARGET,
					"Peer {:?} replaced the connection slots of other peers: {:?}",
					peer_id,
					&other_peers
				);
				self.collation_manager.remove_peers(other_peers);
			},
			TryAcceptOutcome::Rejected => {
				gum::debug!(
					target: LOG_TARGET,
					?peer_id,
					"Peer connection was rejected. Going to disconnect",
				);
			},
		}
	}

	/// Handle a peer disconnection.
	pub async fn handle_peer_disconnected(&mut self, peer_id: PeerId) {
		gum::trace!(
			target: LOG_TARGET,
			?peer_id,
			"Peer disconnected",
		);

		self.peer_manager.disconnected(&peer_id);

		self.collation_manager.remove_peers([peer_id].into_iter().collect());
	}

	/// Handle a peer's declaration message.
	pub async fn handle_declare<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		para_id: ParaId,
	) {
		if !self.peer_manager.declared(sender, peer_id, para_id).await {
			self.collation_manager.remove_peers([peer_id].into_iter().collect());
		}
	}

	/// Handle our view update.
	pub async fn handle_our_view_change<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		new_view: OurView,
	) -> FatalResult<()> {
		gum::trace!(
			target: LOG_TARGET,
			?new_view,
			"Handling our view change",
		);
		let old_assignments = self.collation_manager.assignments();

		self.collation_manager.view_update(sender, &self.keystore, new_view).await?;

		let new_assignments = self.collation_manager.assignments();
		gum::trace!(
			target: LOG_TARGET,
			?old_assignments,
			?new_assignments,
			"Old assignments vs new assignments",
		);

		let maybe_disconnected_peers =
			self.peer_manager.scheduled_paras_update(sender, new_assignments).await;

		if !maybe_disconnected_peers.is_empty() {
			gum::trace!(
				target: LOG_TARGET,
				?maybe_disconnected_peers,
				"Disconnecting peers due to our view change",
			);
		}

		self.collation_manager.remove_peers(maybe_disconnected_peers);

		Ok(())
	}

	/// Handle a finalized block notification.
	pub async fn handle_finalized_block<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		hash: Hash,
		number: BlockNumber,
	) -> FatalResult<()> {
		gum::trace!(
			target: LOG_TARGET,
			?hash,
			number,
			"Processing new block finality notification",
		);

		// Process reputation bumps
		if let Err(err) = self
			.peer_manager
			.update_reputations_on_new_finalized_block(sender, (hash, number))
			.await
		{
			err.split()?.log();
		}

		// Process potential changes in the registered paras set. TODO: we need a new runtime API
		// for it self.peer_manager.registered_paras_update(registered_paras);

		Ok(())
	}

	/// Handle a new advertisement.
	pub async fn handle_advertisement<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
		relay_parent: Hash,
		maybe_prospective_candidate: Option<ProspectiveCandidate>,
	) {
		gum::debug!(
			target: LOG_TARGET,
			?relay_parent,
			?maybe_prospective_candidate,
			?peer_id,
			"Received advertisement",
		);

		let Some(PeerInfo { state, .. }) = self.peer_manager.peer_info(&peer_id) else {
			gum::warn!(
				target: LOG_TARGET,
				?relay_parent,
				?peer_id,
				?maybe_prospective_candidate,
				"Received an advertisement from an unconnected peer"
			);
			return
		};

		// Advertised without being declared. Not a big waste of our time, so ignore it.
		let PeerState::Collating(para_id) = state else {
			gum::warn!(
				target: LOG_TARGET,
				?relay_parent,
				?maybe_prospective_candidate,
				?peer_id,
				"Received advertisement for undeclared peer",
			);
			return
		};

		// We have a result here but it's not worth affecting reputations, because advertisements
		// are cheap and quickly triaged.
		match self
			.collation_manager
			.try_accept_advertisement(
				sender,
				Advertisement {
					peer_id,
					para_id: *para_id,
					relay_parent,
					prospective_candidate: maybe_prospective_candidate,
				},
			)
			.await
		{
			Err(err) => {
				gum::info!(
					target: LOG_TARGET,
					?relay_parent,
					?maybe_prospective_candidate,
					?peer_id,
					?err,
					"Advertisement rejected",
				);
			},
			Ok(()) => {
				gum::debug!(
					target: LOG_TARGET,
					?relay_parent,
					?maybe_prospective_candidate,
					?peer_id,
					"Advertisement accepted",
				);
			},
		}
	}

	pub fn collation_response_stream(
		&mut self,
	) -> &mut impl FusedStream<Item = CollationFetchResponse> {
		self.collation_manager.response_stream()
	}

	pub async fn handle_fetched_collation<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		res: CollationFetchResponse,
	) {
		let advertisement = res.0;

		if let Err(err) = &res.1 {
			gum::debug!(
				target: LOG_TARGET,
				?advertisement,
				"Collation fetch attempt failed: {}",
				err
			);
		} else {
			gum::debug!(
				target: LOG_TARGET,
				?advertisement,
				"Collation fetch attempt succeeded",
			);
		}

		let can_second = self.collation_manager.completed_fetch(sender, res).await;

		match can_second {
			CanSecond::Yes(candidate_receipt, pov, pvd) => {
				sender
					.send_message(CandidateBackingMessage::Second(
						candidate_receipt.descriptor.relay_parent(),
						candidate_receipt,
						pvd,
						pov,
					))
					.await;

				gum::debug!(
					target: LOG_TARGET,
					?advertisement,
					"Started seconding"
				);
			},
			CanSecond::No(maybe_slash, reject_info) => {
				if let Some(slash) = maybe_slash {
					self.peer_manager
						.slash_reputation(&reject_info.peer_id, &reject_info.para_id, slash)
						.await;
				}

				self.collation_manager.release_slot(
					&reject_info.relay_parent,
					reject_info.para_id,
					reject_info.maybe_candidate_hash.as_ref(),
					reject_info.maybe_output_head_hash,
				);
			},
			CanSecond::BlockedOnParent(_, _) => {},
		};
	}

	pub async fn handle_invalid_collation(&mut self, receipt: CandidateReceipt) {
		let relay_parent = receipt.descriptor.relay_parent();
		let candidate_hash = receipt.hash();

		gum::debug!(
			target: LOG_TARGET,
			para_id = ?receipt.descriptor.para_id(),
			?relay_parent,
			?candidate_hash,
			"Invalid collation",
		);

		self.collation_manager.release_slot(
			&relay_parent,
			receipt.descriptor.para_id(),
			Some(&candidate_hash),
			Some(receipt.descriptor.para_head()),
		);

		let Some(peer_id) = self
			.collation_manager
			.get_peer_id_of_fetched_collation(&relay_parent, &candidate_hash)
		else {
			gum::warn!(
				target: LOG_TARGET,
				para_id = ?receipt.descriptor.para_id(),
				?relay_parent,
				?candidate_hash,
				"Could not find the peerid of the invalid collation",
			);
			return
		};

		self.peer_manager
			.slash_reputation(&peer_id, &receipt.descriptor.para_id(), INVALID_COLLATION_SLASH)
			.await;
	}

	pub async fn handle_collation_seconded<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		statement: SignedFullStatement,
	) {
		let receipt = match statement.payload() {
			Statement::Seconded(receipt) => receipt,
			Statement::Valid(_) => {
				gum::warn!(
					target: LOG_TARGET,
					?statement,
					"Seconded message received with a `Valid` statement",
				);
				return
			},
		};

		let candidate_hash = receipt.hash();
		let relay_parent = receipt.descriptor.relay_parent();
		let para_id = receipt.descriptor.para_id();

		gum::debug!(
			target: LOG_TARGET,
			?para_id,
			?relay_parent,
			?candidate_hash,
			"Collation seconded",
		);

		let (peer_id, unblocked_collations) = self
			.collation_manager
			.seconded(
				sender,
				&relay_parent,
				&candidate_hash,
				&para_id,
				receipt.descriptor.para_head(),
			)
			.await;

		if let Some((peer_id, PeerInfo { version, .. })) = peer_id
			.and_then(|peer_id| self.peer_manager.peer_info(&peer_id).map(|info| (peer_id, info)))
		{
			notify_collation_seconded(sender, peer_id, *version, relay_parent, statement).await;
		}

		if !unblocked_collations.is_empty() {
			gum::debug!(
				target: LOG_TARGET,
				?relay_parent,
				?candidate_hash,
				?para_id,
				"Seconded candidate unblocked {} collations",
				unblocked_collations.len(),
			);

			self.try_second_unblocked_collations(sender, unblocked_collations).await;
		}
	}

	pub async fn try_launch_new_fetch_requests<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
	) {
		let peer_manager = &self.peer_manager;
		let connected_rep_query_fn = move |peer_id: &PeerId, para_id: &ParaId| {
			peer_manager.connected_peer_score(peer_id, para_id)
		};

		let requests = self.collation_manager.try_making_new_fetch_requests(connected_rep_query_fn);

		if !requests.is_empty() {
			gum::debug!(
				target: LOG_TARGET,
				?requests,
				"Sending {} collation fetch requests",
				requests.len()
			);

			sender
				.send_message(NetworkBridgeTxMessage::SendRequests(
					requests,
					IfDisconnected::ImmediateError,
				))
				.await;
		}
	}

	async fn try_second_unblocked_collations<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		unblocked_collations: Vec<CanSecond>,
	) {
		for can_second_unblocked in unblocked_collations {
			match can_second_unblocked {
				CanSecond::Yes(candidate_receipt, pov, pvd) => {
					let relay_parent = candidate_receipt.descriptor.relay_parent();
					let candidate_hash = candidate_receipt.hash();
					let para_id = candidate_receipt.descriptor.para_id();

					sender
						.send_message(CandidateBackingMessage::Second(
							relay_parent,
							candidate_receipt,
							pvd,
							pov,
						))
						.await;

					gum::debug!(
						target: LOG_TARGET,
						?relay_parent,
						?candidate_hash,
						?para_id,
						"Started seconding unblocked collation"
					);
				},
				CanSecond::No(maybe_slash, reject_info) => {
					gum::warn!(
						target: LOG_TARGET,
						relay_parent = ?reject_info.relay_parent,
						maybe_candidate_hash = ?reject_info.maybe_candidate_hash,
						para_id = ?reject_info.para_id,
						"Cannot second unblocked collation"
					);

					if let Some(slash) = maybe_slash {
						self.peer_manager
							.slash_reputation(&reject_info.peer_id, &reject_info.para_id, slash)
							.await;
					}

					self.collation_manager.release_slot(
						&reject_info.relay_parent,
						reject_info.para_id,
						reject_info.maybe_candidate_hash.as_ref(),
						reject_info.maybe_output_head_hash,
					);
				},
				CanSecond::BlockedOnParent(parent, reject_info) => {
					gum::warn!(
						target: LOG_TARGET,
						relay_parent = ?reject_info.relay_parent,
						maybe_candidate_hash = ?reject_info.maybe_candidate_hash,
						?parent,
						para_id = ?reject_info.para_id,
						"Cannot second unblocked collation even though its parent was just seconded"
					);

					self.collation_manager.release_slot(
						&reject_info.relay_parent,
						reject_info.para_id,
						reject_info.maybe_candidate_hash.as_ref(),
						reject_info.maybe_output_head_hash,
					);
				},
			}
		}
	}
}
