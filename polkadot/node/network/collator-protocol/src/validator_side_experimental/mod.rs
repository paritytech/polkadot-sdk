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

mod collation_manager;
mod common;
mod error;
mod metrics;
mod peer_manager;
mod state;
#[cfg(test)]
mod tests;

use common::MAX_STORED_SCORES_PER_PARA;
use futures::{select, FutureExt, StreamExt};
use polkadot_node_network_protocol::{
	self as net_protocol, v1 as protocol_v1, v2 as protocol_v2, CollationProtocols, PeerId,
};
use polkadot_node_subsystem::{
	messages::{CollatorProtocolMessage, NetworkBridgeEvent},
	overseer, ActivatedLeaf, FromOrchestra, OverseerSignal,
};
use sp_keystore::KeystorePtr;

use collation_manager::CollationManager;
use common::ProspectiveCandidate;
use error::{log_error, FatalError, FatalResult, Result};

use peer_manager::{Db, PeerManager};

use state::State;

pub use metrics::Metrics;

use crate::LOG_TARGET;

/// The main run loop.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
pub(crate) async fn run<Context>(
	mut ctx: Context,
	keystore: KeystorePtr,
	metrics: Metrics,
) -> FatalResult<()> {
	if let Some(state) = initialize(&mut ctx, keystore, metrics).await? {
		run_inner(ctx, state).await?;
	}

	Ok(())
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn initialize<Context>(
	ctx: &mut Context,
	keystore: KeystorePtr,
	metrics: Metrics,
) -> FatalResult<Option<State<Db>>> {
	loop {
		let first_leaf = match wait_for_first_leaf(ctx).await? {
			Some(activated_leaf) => {
				gum::debug!(
					target: LOG_TARGET,
					number = activated_leaf.number,
					hash = ?activated_leaf.hash,
					"Got the first active leaf notification, trying to initialize subsystem."
				);
				activated_leaf
			},
			None => return Ok(None),
		};

		let collation_manager =
			CollationManager::new(ctx.sender(), keystore.clone(), first_leaf).await?;

		let scheduled_paras = collation_manager.assignments();

		let backend = Db::new(MAX_STORED_SCORES_PER_PARA).await;

		match PeerManager::startup(backend, ctx.sender(), scheduled_paras.into_iter().collect())
			.await
		{
			Ok(peer_manager) =>
				return Ok(Some(State::new(peer_manager, collation_manager, metrics))),
			Err(err) => {
				log_error(Err(err))?;
				continue
			},
		}
	}
}

/// Wait for `ActiveLeavesUpdate`, returns `None` if `Conclude` signal came first.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn wait_for_first_leaf<Context>(ctx: &mut Context) -> FatalResult<Option<ActivatedLeaf>> {
	loop {
		match ctx.recv().await.map_err(FatalError::SubsystemReceive)? {
			FromOrchestra::Signal(OverseerSignal::Conclude) => return Ok(None),
			FromOrchestra::Signal(OverseerSignal::ActiveLeaves(update)) => {
				if let Some(activated) = update.activated {
					return Ok(Some(activated))
				}
			},
			FromOrchestra::Signal(OverseerSignal::BlockFinalized(_, _)) => {},
			FromOrchestra::Communication { msg } => {
				// TODO: we should actually disconnect peers connected on collation protocol while
				// we're still bootstrapping. OR buffer these messages until we've bootstrapped.
				gum::warn!(
					target: LOG_TARGET,
					?msg,
					"Received msg before first active leaves update. This is not expected - message will be dropped."
				)
			},
		}
	}
}

#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn run_inner<Context>(mut ctx: Context, mut state: State<Db>) -> FatalResult<()> {
	loop {
		select! {
			res = ctx.recv().fuse() => {
				match res {
					Ok(FromOrchestra::Communication { msg }) => {
						gum::trace!(target: LOG_TARGET, msg = ?msg, "received a message");
						process_msg(
							&mut ctx,
							&mut state,
							msg,
						).await;
					}
					Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) | Err(_) => break,
					Ok(FromOrchestra::Signal(OverseerSignal::BlockFinalized(hash, number))) => {
						state.handle_finalized_block(ctx.sender(), hash, number).await?;
					},
					Ok(FromOrchestra::Signal(_)) => continue,
				}
			},
			resp = state.collation_response_stream().select_next_some() => {
				state.handle_fetched_collation(ctx.sender(), resp).await;
			}
		}

		// Now try triggering advertisement fetching, if we have room in any of the active leaves
		// (any of them are in Waiting state).
		// We could optimise to not always re-run this code (have the other functions return
		// whether or not we should attempt launching fetch requests) However, most messages could
		// indeed trigger a new legitimate request.
		// Also, it takes constant time to run because we only try launching new requests for
		// unfulfilled claims. It's probably not worth optimising.
		state.try_launch_new_fetch_requests(ctx.sender()).await;
	}

	Ok(())
}

/// The main message receiver switch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn process_msg<Context>(
	ctx: &mut Context,
	state: &mut State<Db>,
	msg: CollatorProtocolMessage,
) {
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
		Seconded(_parent, stmt) => {
			state.handle_seconded_collation(ctx.sender(), stmt).await;
		},
		Invalid(_parent, candidate_receipt) => {
			state.handle_invalid_collation(candidate_receipt).await;
		},
		ConnectToBackingGroups => {
			gum::warn!(
				target: LOG_TARGET,
				"ConnectToBackingGroups message is not expected on the validator side of the protocol",
			);
		},
		DisconnectFromBackingGroups => {
			gum::warn!(
				target: LOG_TARGET,
				"DisconnectFromBackingGroups message is not expected on the validator side of the protocol",
			);
		},
	}
}

/// Bridge event switch.
#[overseer::contextbounds(CollatorProtocol, prefix = self::overseer)]
async fn handle_network_msg<Context>(
	ctx: &mut Context,
	state: &mut State<Db>,
	bridge_message: NetworkBridgeEvent<net_protocol::CollatorProtocolMessage>,
) -> Result<()> {
	use NetworkBridgeEvent::*;

	match bridge_message {
		PeerConnected(peer_id, observed_role, protocol_version, _) => {
			let version = match protocol_version.try_into() {
				Ok(version) => version,
				Err(err) => {
					// Network bridge is expected to handle this.
					gum::error!(
						target: LOG_TARGET,
						?peer_id,
						?observed_role,
						?err,
						"Unsupported protocol version"
					);
					return Ok(())
				},
			};
			state.handle_peer_connected(ctx.sender(), peer_id, version).await;
		},
		PeerDisconnected(peer_id) => {
			state.handle_peer_disconnected(peer_id).await;
		},
		NewGossipTopology { .. } => {
			// impossible!
		},
		PeerViewChange(_, _) => {
			// We don't really care about a peer's view.
		},
		OurViewChange(view) => {
			state.handle_our_view_change(ctx.sender(), view).await?;
		},
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
	state: &mut State<Db>,
	origin: PeerId,
	msg: CollationProtocols<
		protocol_v1::CollatorProtocolMessage,
		protocol_v2::CollatorProtocolMessage,
	>,
) {
	use protocol_v1::CollatorProtocolMessage as V1;
	use protocol_v2::CollatorProtocolMessage as V2;

	match msg {
		CollationProtocols::V1(V1::Declare(_collator_id, para_id, _signature)) |
		CollationProtocols::V2(V2::Declare(_collator_id, para_id, _signature)) => {
			state.handle_declare(ctx.sender(), origin, para_id).await;
		},
		CollationProtocols::V1(V1::CollationSeconded(..)) |
		CollationProtocols::V2(V2::CollationSeconded(..)) => {
			gum::warn!(
				target: LOG_TARGET,
				peer_id = ?origin,
				"Unexpected `CollationSeconded` message",
			);
		},
		CollationProtocols::V1(V1::AdvertiseCollation(relay_parent)) => {
			state.handle_advertisement(ctx.sender(), origin, relay_parent, None).await;
		},
		CollationProtocols::V2(V2::AdvertiseCollation {
			relay_parent,
			candidate_hash,
			parent_head_data_hash,
		}) => {
			state
				.handle_advertisement(
					ctx.sender(),
					origin,
					relay_parent,
					Some(ProspectiveCandidate { candidate_hash, parent_head_data_hash }),
				)
				.await;
		},
	}
}
