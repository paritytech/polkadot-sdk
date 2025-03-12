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

use std::collections::{BTreeMap, HashMap, HashSet};

use futures::{channel::oneshot, select, FutureExt, StreamExt};

use polkadot_node_subsystem_util::{
	request_candidate_events, request_candidates_pending_availability,
};
use sp_keystore::KeystorePtr;

use common::{
	Advertisement, CollationFetchOutcome, CollationFetchResponse, DeclarationOutcome, PeerState,
	ProspectiveCandidate, Score, FAILED_FETCH_SLASH, INVALID_COLLATION_SLASH,
	VALID_INCLUDED_CANDIDATE_BUMP,
};
use polkadot_node_network_protocol::{
	self as net_protocol, peer_set::CollationVersion, v1 as protocol_v1, v2 as protocol_v2,
	OurView, PeerId, Versioned,
};
use polkadot_node_primitives::{SignedFullStatement, Statement};
use polkadot_node_subsystem::{
	messages::{
		CandidateBackingMessage, ChainApiMessage, CollatorProtocolMessage, NetworkBridgeEvent,
		NetworkBridgeTxMessage, ParentHeadData, ProspectiveParachainsMessage,
		ProspectiveValidationDataRequest,
	},
	overseer, CollatorProtocolSenderTrait, FromOrchestra, OverseerSignal,
};
use polkadot_primitives::{
	vstaging::{CandidateEvent, CandidateReceiptV2 as CandidateReceipt},
	CandidateHash, Hash, HeadData, Id as ParaId, PersistedValidationData,
};

use crate::error::{Result, SecondingError};
use collation_manager::CollationManager;
pub use metrics::Metrics;
use peer_manager::PeerManager;

use super::LOG_TARGET;

mod collation_manager;
mod common;
mod metrics;
mod peer_manager;

/// The main run loop.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
pub(crate) async fn run<Context>(
	ctx: Context,
	keystore: KeystorePtr,
) -> std::result::Result<(), crate::error::FatalError> {
	run_inner(ctx, keystore).await
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn run_inner<Context>(
	mut ctx: Context,
	keystore: KeystorePtr,
) -> std::result::Result<(), crate::error::FatalError> {
	let mut state = State::new(keystore);

	loop {
		select! {
			res = ctx.recv().fuse() => {
				match res {
					Ok(FromOrchestra::Communication { msg }) => {
						gum::trace!(target: LOG_TARGET, msg = ?msg, "received a message");
						process_msg(
							&mut ctx,
							msg,
							&mut state,
						).await;
					}
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => break,
					Ok(FromOrchestra::Signal(_)) => continue,
				}
			},
			resp = state.collation_manager.response_stream().select_next_some() => {
				state.handle_fetched_collation(ctx.sender(), resp).await;
			}
		}

		// Now try triggering advertisement fetching, if we have room in any of the active leaves
		// (any of them are in Waiting state).
		// TODO: we could optimise to not always re-run this code. Have the other functions return
		// whether or not we should attempt launching fetch requests. However, most messages could
		// indeed trigger a new legitimate request so it's probably not worth optimising.
		state.try_launch_fetch_requests(ctx.sender()).await;
	}

	Ok(())
}

/// The main message receiver switch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn process_msg<Context>(ctx: &mut Context, msg: CollatorProtocolMessage, state: &mut State) {
	use CollatorProtocolMessage::*;

	match msg {
		CollateOn(id) => {
			gum::warn!(
				target: LOG_TARGET,
				para_id = %id,
				"CollateOn message is not expected on the validator side of the protocol",
			);
		},
		DistributeCollation { .. } => {
			gum::warn!(
				target: LOG_TARGET,
				"DistributeCollation message is not expected on the validator side of the protocol",
			);
		},
		NetworkBridgeUpdate(event) =>
			if let Err(e) = handle_network_msg(ctx, state, event).await {
				gum::warn!(
					target: LOG_TARGET,
					err = ?e,
					"Failed to handle incoming network message",
				);
			},
		Seconded(parent, stmt) => {
			state.handle_collation_seconded(ctx.sender(), parent, stmt).await;
		},
		Invalid(_parent, candidate_receipt) => {
			state.handle_invalid_collation(candidate_receipt).await;
		},
	}
}

/// Bridge event switch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn handle_network_msg<Context>(
	ctx: &mut Context,
	state: &mut State,
	bridge_message: NetworkBridgeEvent<net_protocol::CollatorProtocolMessage>,
) -> Result<()> {
	use NetworkBridgeEvent::*;

	match bridge_message {
		PeerConnected(peer_id, _observed_role, _protocol_version, _) => {
			// let version = match protocol_version.try_into() {
			// 	Ok(version) => version,
			// 	Err(err) => {
			// 		// Network bridge is expected to handle this.
			// 		gum::error!(
			// 			target: LOG_TARGET,
			// 			?peer_id,
			// 			?observed_role,
			// 			?err,
			// 			"Unsupported protocol version"
			// 		);
			// 		return Ok(())
			// 	},
			// };
			state.handle_connected(ctx.sender(), peer_id).await;
		},
		PeerDisconnected(peer_id) => {
			state.handle_disconnected(peer_id);
		},
		NewGossipTopology { .. } => {
			// impossible!
		},
		PeerViewChange(_, _) => {
			// We don't really care about a peer's view.
		},
		OurViewChange(view) => state.handle_our_view_change(ctx.sender(), view).await,
		PeerMessage(remote, msg) => {
			process_incoming_peer_message(ctx, state, remote, msg).await;
		},
		UpdatedAuthorityIds { .. } => {
			// The validator side doesn't deal with `AuthorityDiscoveryId`s.
		},
	}

	Ok(())
}

#[overseer::contextbounds(CollatorProtocol, prefix = overseer)]
async fn process_incoming_peer_message<Context>(
	ctx: &mut Context,
	state: &mut State,
	origin: PeerId,
	msg: Versioned<protocol_v1::CollatorProtocolMessage, protocol_v2::CollatorProtocolMessage>,
) {
	use protocol_v1::CollatorProtocolMessage as V1;
	use protocol_v2::CollatorProtocolMessage as V2;

	match msg {
		Versioned::V1(V1::Declare(_collator_id, para_id, _signature)) |
		Versioned::V2(V2::Declare(_collator_id, para_id, _signature)) |
		Versioned::V3(V2::Declare(_collator_id, para_id, _signature)) => {
			state.handle_declare(ctx.sender(), origin, para_id).await;
		},
		Versioned::V1(V1::CollationSeconded(..)) |
		Versioned::V2(V2::CollationSeconded(..)) |
		Versioned::V3(V2::CollationSeconded(..)) => {
			gum::warn!(
				target: LOG_TARGET,
				peer_id = ?origin,
				"Unexpected `CollationSeconded` message, decreasing reputation",
			);

			// modify_reputation(&mut state.reputation, ctx.sender(), origin,
			// COST_UNEXPECTED_MESSAGE) 	.await;
		},
		Versioned::V1(V1::AdvertiseCollation(relay_parent)) =>
			state.handle_advertisement(ctx.sender(), origin, relay_parent, None).await,
		Versioned::V3(V2::AdvertiseCollation {
			relay_parent,
			candidate_hash,
			parent_head_data_hash,
		}) |
		Versioned::V2(V2::AdvertiseCollation {
			relay_parent,
			candidate_hash,
			parent_head_data_hash,
		}) =>
			state
				.handle_advertisement(
					ctx.sender(),
					origin,
					relay_parent,
					Some(ProspectiveCandidate { candidate_hash, parent_head_data_hash }),
				)
				.await,
	}
}

/// All state relevant for the validator side of the protocol lives here.
struct State {
	collation_manager: CollationManager,

	peer_manager: PeerManager,

	keystore: KeystorePtr,
}

impl State {
	fn new(keystore: KeystorePtr) -> Self {
		Self {
			peer_manager: PeerManager::default(),
			collation_manager: CollationManager::default(),
			keystore,
		}
	}

	async fn handle_our_view_change<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		new_view: OurView,
	) {
		gum::trace!(
			target: LOG_TARGET,
			?new_view,
			"Handling our view change",
		);
		let old_assignments = self.collation_manager.assignments();
		let new_leaves = self.collation_manager.view_update(sender, &self.keystore, new_view).await;

		let updates = extract_reputation_updates_from_new_leaves(sender, &new_leaves[..]).await;
		gum::trace!(
			target: LOG_TARGET,
			?new_leaves,
			"Rep updates: {:#?}",
			updates
		);
		self.peer_manager.update_reputations_on_new_leaf(updates);

		let new_assignments = self.collation_manager.assignments();
		gum::trace!(
			target: LOG_TARGET,
			?old_assignments,
			?new_assignments,
			"Old assignments vs new assignments",
		);

		let maybe_disconnected_peers =
			self.peer_manager.scheduled_paras_update(sender, new_assignments.clone()).await;

		if !maybe_disconnected_peers.is_empty() {
			gum::trace!(
				target: LOG_TARGET,
				?maybe_disconnected_peers,
				"Disconnecting peers due to our view change",
			);
		}

		self.collation_manager.remove_peers(maybe_disconnected_peers);
	}

	async fn handle_advertisement<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		origin: PeerId,
		relay_parent: Hash,
		maybe_prospective_candidate: Option<ProspectiveCandidate>,
	) {
		let Some(peer_state) = self.peer_manager.peer_state(&origin) else { return };

		// Advertised without being declared. Not a big waste of our time, so ignore it
		let PeerState::Collating(para_id) = peer_state else {
			gum::trace!(
				target: LOG_TARGET,
				?relay_parent,
				?maybe_prospective_candidate,
				peer_id = ?origin,
				"Received advertisement for undeclared peer",
			);
			return
		};

		// TODO: we'll later need to handle maybe_prospective_candidate being None.

		// We have a result here but it's not worth affecting reputations, because advertisements
		// are cheap and quickly triaged.
		match self
			.collation_manager
			.try_accept_advertisement(
				sender,
				Advertisement {
					peer_id: origin,
					para_id: *para_id,
					relay_parent,
					prospective_candidate: maybe_prospective_candidate.unwrap(),
				},
			)
			.await
		{
			Err(err) => {
				gum::debug!(
					target: LOG_TARGET,
					?relay_parent,
					?maybe_prospective_candidate,
					peer_id = ?origin,
					?err,
					"Advertisement rejected",
				);
			},
			Ok(()) => {
				gum::debug!(
					target: LOG_TARGET,
					?relay_parent,
					?maybe_prospective_candidate,
					peer_id = ?origin,
					"Advertisement accepted",
				);
			},
		}
	}

	async fn handle_declare<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		origin: PeerId,
		para_id: ParaId,
	) {
		match self.peer_manager.declared(sender, origin, para_id).await {
			DeclarationOutcome::Disconnected => {
				self.collation_manager.remove_peers(vec![origin]);
				gum::trace!(
					target: LOG_TARGET,
					?para_id,
					peer_id = ?origin,
					"Peer declared but rejected",
				);
			},
			DeclarationOutcome::Switched(old_para_id) => {
				gum::trace!(
					target: LOG_TARGET,
					?para_id,
					?old_para_id,
					peer_id = ?origin,
					"Peer switched collating paraid",
				);
				self.collation_manager.remove_peers(vec![origin]);
			},
			DeclarationOutcome::Accepted => {
				gum::trace!(
					target: LOG_TARGET,
					?para_id,
					peer_id = ?origin,
					"Peer declared",
				);
			},
		};
	}

	fn handle_disconnected(&mut self, peer_id: PeerId) {
		gum::trace!(
			target: LOG_TARGET,
			?peer_id,
			"Peer disconnected",
		);

		self.peer_manager.handle_disconnected(peer_id);

		self.collation_manager.remove_peers(vec![peer_id]);
	}

	async fn handle_connected<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		peer_id: PeerId,
	) {
		let disconnected = self.peer_manager.try_accept(sender, peer_id).await;

		let accepted = !disconnected.contains(&peer_id);
		if accepted {
			gum::trace!(
				target: LOG_TARGET,
				?peer_id,
				"Peer connection accepted",
			);
		} else {
			gum::trace!(
				target: LOG_TARGET,
				?peer_id,
				"Peer connection rejected",
			);
		}

		self.collation_manager.remove_peers(disconnected);
	}

	async fn handle_fetched_collation<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		res: CollationFetchResponse,
	) {
		let advertisement = res.0;
		let relay_parent = advertisement.relay_parent;
		let candidate_hash = advertisement.prospective_candidate.candidate_hash;

		gum::trace!(
			target: LOG_TARGET,
			?advertisement,
			"Collation fetch attempt finished"
		);

		let outcome = self.collation_manager.completed_fetch(res);

		let rep_slash = match outcome {
			CollationFetchOutcome::Success(fetched_collation) => {
				let pvd = request_prospective_validation_data(
					sender,
					relay_parent,
					fetched_collation.parent_head_data_hash,
					fetched_collation.candidate_receipt.descriptor.para_id(),
					fetched_collation.maybe_parent_head_data.clone(),
				)
				.await
				.unwrap();

				// TODO: handle collations whose parent we don't know yet.
				let pvd = pvd.unwrap();

				if let Err(err) = persisted_validation_data_sanity_check(
					&pvd,
					&fetched_collation.candidate_receipt,
					fetched_collation
						.maybe_parent_head_data
						.as_ref()
						.and_then(|head| Some((head, &fetched_collation.parent_head_data_hash))),
				) {
					gum::warn!(
						target: LOG_TARGET,
						?advertisement,
						"Invalid fetched collation: {}",
						err
					);

					// TOOD: this slash is not ok
					FAILED_FETCH_SLASH
				} else {
					sender
						.send_message(CandidateBackingMessage::Second(
							relay_parent,
							fetched_collation.candidate_receipt,
							pvd,
							fetched_collation.pov,
						))
						.await;

					gum::trace!(
						target: LOG_TARGET,
						?advertisement,
						"Started seconding"
					);

					return
				}
			},
			CollationFetchOutcome::TryNew(update) => update,
		};

		self.peer_manager.slash_reputation(
			&advertisement.peer_id,
			&advertisement.para_id,
			rep_slash,
		);

		// reset collation status
		self.collation_manager
			.release_slot(&advertisement.relay_parent, &candidate_hash);
	}

	async fn handle_collation_seconded<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
		relay_parent: Hash,
		statement: SignedFullStatement,
	) {
		let receipt = match statement.payload() {
			Statement::Seconded(receipt) => receipt,
			Statement::Valid(_) => {
				gum::warn!(
					target: LOG_TARGET,
					?statement,
					?relay_parent,
					"Seconded message received with a `Valid` statement",
				);
				return
			},
		};

		gum::debug!(
			target: LOG_TARGET,
			para_id = ?receipt.descriptor.para_id(),
			?relay_parent,
			candidate_hash = ?receipt.hash(),
			"Collation seconded",
		);

		let Some(peer_id) = self.collation_manager.seconded(
			&relay_parent,
			&receipt.hash(),
			&receipt.descriptor.para_id(),
		) else {
			return
		};

		notify_collation_seconded(sender, peer_id, CollationVersion::V2, relay_parent, statement)
			.await;

		// TODO: see if we've unblocked other collations here too.
	}

	async fn handle_invalid_collation(&mut self, receipt: CandidateReceipt) {
		let relay_parent = receipt.descriptor.relay_parent();
		let candidate_hash = receipt.hash();

		gum::debug!(
			target: LOG_TARGET,
			para_id = ?receipt.descriptor.para_id(),
			?relay_parent,
			?candidate_hash,
			"Invalid collation",
		);

		if let Some(peer_id) = self.collation_manager.release_slot(&relay_parent, &candidate_hash) {
			self.peer_manager.slash_reputation(
				&peer_id,
				&receipt.descriptor.para_id(),
				INVALID_COLLATION_SLASH,
			);
		}
	}

	async fn try_launch_fetch_requests<Sender: CollatorProtocolSenderTrait>(
		&mut self,
		sender: &mut Sender,
	) {
		gum::debug!(
			target: LOG_TARGET,
			"Peer reputations: {:#?}",
			self.peer_manager.reputation_db.0
		);

		self.collation_manager
			.try_launch_fetch_requests(sender, &self.peer_manager)
			.await;
	}
}

async fn extract_reputation_updates_from_new_leaves<Sender: CollatorProtocolSenderTrait>(
	sender: &mut Sender,
	leaves: &[Hash],
) -> BTreeMap<ParaId, HashMap<PeerId, Score>> {
	// TODO: this could be much easier if we added a new CandidateEvent variant that includes the
	// info we need (the approved peer)
	let mut included_candidates_per_rp: HashMap<Hash, HashMap<ParaId, HashSet<CandidateHash>>> =
		HashMap::new();

	for leaf in leaves {
		let candidate_events =
			request_candidate_events(*leaf, sender).await.await.unwrap().unwrap();

		for event in candidate_events {
			if let CandidateEvent::CandidateIncluded(receipt, _, _, _) = event {
				included_candidates_per_rp
					.entry(*leaf)
					.or_default()
					.entry(receipt.descriptor.para_id())
					.or_default()
					.insert(receipt.hash());
			}
		}
	}

	let mut updates: BTreeMap<ParaId, HashMap<PeerId, Score>> = BTreeMap::new();
	for (rp, per_para) in included_candidates_per_rp {
		let parent = get_parent(sender, rp).await;

		for (para_id, included_candidates) in per_para {
			let candidates_pending_availability =
				request_candidates_pending_availability(parent, para_id, sender)
					.await
					.await
					.unwrap()
					.unwrap();

			for candidate in candidates_pending_availability {
				if included_candidates.contains(&candidate.hash()) {
					if let Some(approved_peer) = candidate.commitments.approved_peer() {
						if let Ok(peer_id) = PeerId::from_bytes(&approved_peer.0) {
							*(updates.entry(para_id).or_default().entry(peer_id).or_default()) +=
								VALID_INCLUDED_CANDIDATE_BUMP;
						}
					}
				}
			}
		}
	}

	updates
}

/// Notify a collator that its collation got seconded.
async fn notify_collation_seconded(
	sender: &mut impl overseer::CollatorProtocolSenderTrait,
	peer_id: PeerId,
	version: CollationVersion,
	relay_parent: Hash,
	statement: SignedFullStatement,
) {
	let statement = statement.into();
	let wire_message = match version {
		CollationVersion::V1 => Versioned::V1(protocol_v1::CollationProtocol::CollatorProtocol(
			protocol_v1::CollatorProtocolMessage::CollationSeconded(relay_parent, statement),
		)),
		CollationVersion::V2 => Versioned::V2(protocol_v2::CollationProtocol::CollatorProtocol(
			protocol_v2::CollatorProtocolMessage::CollationSeconded(relay_parent, statement),
		)),
	};
	sender
		.send_message(NetworkBridgeTxMessage::SendCollationMessage(vec![peer_id], wire_message))
		.await;
}

async fn request_prospective_validation_data<Sender>(
	sender: &mut Sender,
	candidate_relay_parent: Hash,
	parent_head_data_hash: Hash,
	para_id: ParaId,
	maybe_parent_head_data: Option<HeadData>,
) -> std::result::Result<Option<PersistedValidationData>, SecondingError>
where
	Sender: CollatorProtocolSenderTrait,
{
	let (tx, rx) = oneshot::channel();

	let parent_head_data = if let Some(head_data) = maybe_parent_head_data {
		ParentHeadData::WithData { head_data, hash: parent_head_data_hash }
	} else {
		ParentHeadData::OnlyHash(parent_head_data_hash)
	};

	let request =
		ProspectiveValidationDataRequest { para_id, candidate_relay_parent, parent_head_data };

	sender
		.send_message(ProspectiveParachainsMessage::GetProspectiveValidationData(request, tx))
		.await;

	rx.await.map_err(SecondingError::CancelledProspectiveValidationData)
}

async fn get_parent<Sender: CollatorProtocolSenderTrait>(sender: &mut Sender, hash: Hash) -> Hash {
	// TODO: we could use the implicit view for this info.
	let (tx, rx) = oneshot::channel();
	sender.send_message(ChainApiMessage::BlockHeader(hash, tx)).await;

	rx.await.unwrap().unwrap().unwrap().parent_hash
}

fn persisted_validation_data_sanity_check(
	persisted_validation_data: &PersistedValidationData,
	fetched: &CandidateReceipt,
	maybe_parent_head_and_hash: Option<(&HeadData, &Hash)>,
) -> std::result::Result<(), SecondingError> {
	if persisted_validation_data.hash() != fetched.descriptor().persisted_validation_data_hash() {
		return Err(SecondingError::PersistedValidationDataMismatch)
	}

	if maybe_parent_head_and_hash.map_or(false, |(head, hash)| head.hash() != *hash) {
		return Err(SecondingError::ParentHeadDataMismatch)
	}

	Ok(())
}
