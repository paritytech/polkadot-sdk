// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

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

//! Parachain bootnode discovery.
//!
//! The discovery works as follows:
//!  1. We start parachain bootnode content provider discovery on the relay chain DHT in
//!     [`BootnodeDiscovery::start_discovery`].
//!  2. We handle every provider discovered in [`BootnodeDiscovery::handle_providers`] and try to
//!     request the bootnodes from the provider over a `/paranode` request-response protocol.
//!  3. The request result is handled in [`BootnodeDiscovery::handle_response`]. If the request
//!     fails this is a sign of the provider addresses not being cached by the remote / dropped by
//!     the networking library (the case with libp2p). In this case we perform a `FIND_NODE` query
//!     to get the provider addresses first and repeat the request once we know them.
//!  4. When the request over the `/paranode` protocol succeeds, we add the bootnode addresses as
//!     known addresses to the parachain networking.
//!  5. If the content provider discovery had completed, all `FIND_NODE` queries finished, and all
//!     requests over the `/paranode` protocol succeded or failed, but we have not found any
//!     bootnode addresses, we repeat the discovery process after a cooldown period.

use crate::{config::MAX_ADDRESSES, schema::Response};
use codec::{CompactRef, Decode, Encode};
use cumulus_primitives_core::{relay_chain::Hash as RelayHash, ParaId};
use cumulus_relay_chain_interface::{RelayChainError, RelayChainInterface, RelayChainResult};
use futures::{
	channel::oneshot,
	future::{BoxFuture, Fuse, FusedFuture},
	pin_mut,
	stream::FuturesUnordered,
	FutureExt, StreamExt,
};
use log::{debug, error, info, trace, warn};
use parachains_common::Hash as ParaHash;
use prost::Message;
use sc_network::{
	event::{DhtEvent, Event},
	request_responses::{IfDisconnected, RequestFailure},
	service::traits::NetworkService,
	KademliaKey, Multiaddr, PeerId, ProtocolName,
};
use sp_consensus_babe::{Epoch, Randomness};
use std::{collections::HashSet, pin::Pin, sync::Arc, time::Duration};
use tokio::time::{sleep, Sleep};

/// Log target for this file.
const LOG_TARGET: &str = "bootnodes::discovery";

/// Delay before retrying discovery in case of failure. Needed to rate-limit the attempts,
/// especially in small testnets where a discovery attempt can be almost instant.
const RETRY_DELAY: Duration = Duration::from_secs(30);

/// Parachain bootnode discovery parameters.
pub struct BootnodeDiscoveryParams {
	/// Parachain ID.
	pub para_id: ParaId,
	/// Parachain node network service.
	pub parachain_network: Arc<dyn NetworkService>,
	/// Parachain genesis hash.
	pub parachain_genesis_hash: ParaHash,
	/// Parachain fork ID.
	pub parachain_fork_id: Option<String>,
	/// Relay chain interface.
	pub relay_chain_interface: Arc<dyn RelayChainInterface>,
	/// Relay chain network service.
	pub relay_chain_network: Arc<dyn NetworkService>,
	/// `/paranode` protocol name.
	pub paranode_protocol_name: ProtocolName,
}

/// Parachain bootnode discovery service.
pub struct BootnodeDiscovery {
	para_id_scale_compact: Vec<u8>,
	parachain_network: Arc<dyn NetworkService>,
	parachain_genesis_hash: ParaHash,
	parachain_fork_id: Option<String>,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	relay_chain_network: Arc<dyn NetworkService>,
	latest_relay_chain_hash: Option<RelayHash>,
	key_being_discovered: Option<KademliaKey>,
	paranode_protocol_name: ProtocolName,
	pending_responses: FuturesUnordered<
		BoxFuture<
			'static,
			(PeerId, Result<Result<(Vec<u8>, ProtocolName), RequestFailure>, oneshot::Canceled>),
		>,
	>,
	direct_requests: HashSet<PeerId>,
	find_node_queries: HashSet<PeerId>,
	pending_start_discovery: Pin<Box<Fuse<Sleep>>>,
	succeeded: bool,
}

impl BootnodeDiscovery {
	/// Create a new bootnode discovery service.
	pub fn new(
		BootnodeDiscoveryParams {
			para_id,
			parachain_network,
			parachain_genesis_hash,
			parachain_fork_id,
			relay_chain_interface,
			relay_chain_network,
			paranode_protocol_name,
		}: BootnodeDiscoveryParams,
	) -> Self {
		Self {
			para_id_scale_compact: CompactRef(&para_id).encode(),
			parachain_network,
			parachain_genesis_hash,
			parachain_fork_id,
			relay_chain_interface,
			relay_chain_network,
			latest_relay_chain_hash: None,
			key_being_discovered: None,
			paranode_protocol_name,
			pending_responses: FuturesUnordered::default(),
			direct_requests: HashSet::new(),
			find_node_queries: HashSet::new(),
			// Trigger the discovery immediately on startup.
			pending_start_discovery: Box::pin(sleep(Duration::ZERO).fuse()),
			succeeded: false,
		}
	}

	async fn current_epoch(&mut self, hash: RelayHash) -> RelayChainResult<Epoch> {
		let res = self
			.relay_chain_interface
			.call_runtime_api("BabeApi_current_epoch", hash, &[])
			.await?;
		Decode::decode(&mut &*res).map_err(Into::into)
	}

	fn epoch_key(&self, randomness: Randomness) -> KademliaKey {
		self.para_id_scale_compact
			.clone()
			.into_iter()
			.chain(randomness.into_iter())
			.collect::<Vec<_>>()
			.into()
	}

	/// Start bootnode discovery.
	async fn start_discovery(&mut self) -> RelayChainResult<()> {
		let Some(hash) = self.latest_relay_chain_hash else {
			error!(
				target: LOG_TARGET,
				"Failed to start bootnode discovery: no relay chain hash available. This is a bug.",
			);
			// This is a graceful panic via the failure of essential task.
			return Err(RelayChainError::GenericError("no relay chain hash available".to_string()));
		};

		let current_epoch = self.current_epoch(hash).await?;
		let current_epoch_key = self.epoch_key(current_epoch.randomness);
		self.key_being_discovered = Some(current_epoch_key.clone());
		self.relay_chain_network.get_providers(current_epoch_key.clone());

		debug!(
			target: LOG_TARGET,
			"Started discovery of parachain bootnode providers for current epoch key {}",
			hex::encode(current_epoch_key),
		);

		Ok(())
	}

	/// Schedule bootnode discovery if needed. Returns `false` if the discovery event loop should be
	/// terminated.
	fn maybe_retry_discovery(&mut self) -> bool {
		let discovery_in_progress = self.key_being_discovered.is_some() ||
			!self.pending_responses.is_empty() ||
			!self.find_node_queries.is_empty();
		let discovery_scheduled = !self.pending_start_discovery.is_terminated();

		if discovery_in_progress || discovery_scheduled {
			// Discovery is already in progress or scheduled, just continue the event loop.
			true
		} else {
			if self.succeeded {
				// No need to start discovery again if the previous attempt succeeded.
				info!(
					target: LOG_TARGET,
					"Parachain bootnode discovery on the relay chain DHT succeeded",
				);

				false
			} else {
				debug!(
					target: LOG_TARGET,
					"Retrying parachain bootnode discovery on the relay chain DHT in {RETRY_DELAY:?}",
				);
				self.pending_start_discovery = Box::pin(sleep(RETRY_DELAY).fuse());

				true
			}
		}
	}

	fn request_bootnode(&mut self, peer_id: PeerId) {
		trace!(
			target: LOG_TARGET,
			"Requesting parachain bootnode from the relay chain {peer_id:?}",
		);

		let (tx, rx) = oneshot::channel();

		self.relay_chain_network.start_request(
			peer_id,
			self.paranode_protocol_name.clone(),
			self.para_id_scale_compact.clone(),
			None,
			tx,
			IfDisconnected::TryConnect,
		);

		self.pending_responses.push(async move { (peer_id, rx.await) }.boxed());
	}

	fn handle_providers(&mut self, providers: Vec<PeerId>) {
		debug!(
			target: LOG_TARGET,
			"Found parachain bootnode providers on the relay chain: {providers:?}",
		);

		for peer_id in providers {
			if peer_id == self.relay_chain_network.local_peer_id() {
				continue;
			}

			// libp2p may yield the same provider multiple times; skip if we alredy queried it.
			if self.direct_requests.contains(&peer_id) || self.find_node_queries.contains(&peer_id)
			{
				continue;
			}

			// Directly request a bootnode from the peer without performing a `FIND_NODE` query
			// first. With litep2p backend this will likely succeed, because cached provider
			// addresses are automatically added to the transport manager known addresses list.
			//
			// With libp2p backend, or if the remote did not return the cached addresses of the
			// provider, the request will fail and we will perform a `FIND_NODE` query.
			self.direct_requests.insert(peer_id);
			self.request_bootnode(peer_id);
		}
	}

	fn handle_response(
		&mut self,
		peer_id: PeerId,
		res: Result<Result<(Vec<u8>, ProtocolName), RequestFailure>, oneshot::Canceled>,
	) {
		let direct_request = self.direct_requests.remove(&peer_id);

		let response = match res {
			Ok(Ok((payload, _))) => match Response::decode(payload.as_slice()) {
				Ok(response) => response,
				Err(e) => {
					warn!(
						target: LOG_TARGET,
						"Failed to decode parachain bootnode response from {peer_id:?}: {e}",
					);
					return;
				},
			},
			Ok(Err(e)) => {
				if direct_request {
					// It only makes sense to try to find the node on the DHT in case of "address
					// not available" error. Unfortunately, libp2p and litep2p backends report such
					// errors differently, and also some network library could break the error
					// reporting in the future. So, to be on the safe side and avoid subtle bugs,
					// we always try to find the node on the DHT in case of the request failure.
					debug!(
						target: LOG_TARGET,
						"Failed to directly query parachain bootnode from {peer_id:?}: {e}. \
						 Starting FIND_NODE query on the DHT",
					);
					self.find_node_queries.insert(peer_id);
					self.relay_chain_network.find_closest_peers(peer_id);
				} else {
					debug!(
						target: LOG_TARGET,
						"Failed to query parachain bootnode from {peer_id:?} after finding
						 the node addresses on the DHT: {e}",
					);
				}
				return;
			},
			Err(_) => {
				debug!(
					target: LOG_TARGET,
					"Parachain bootnode request to {peer_id:?} canceled. \
					 The node is likely terminating.",
				);
				return;
			},
		};

		match (response.genesis_hash, response.fork_id) {
			(genesis_hash, fork_id)
				if genesis_hash == self.parachain_genesis_hash.as_ref() &&
					fork_id == self.parachain_fork_id => {},
			(genesis_hash, fork_id) => {
				warn!(
					target: LOG_TARGET,
					"Received invalid parachain bootnode response from {peer_id:?}: \
					 genesis hash {}, fork ID {:?} don't match expected genesis hash {}, fork ID {:?}",
					hex::encode(genesis_hash),
					fork_id,
					hex::encode(self.parachain_genesis_hash),
					self.parachain_fork_id,
				);
				return;
			},
		}

		let paranode_peer_id = match PeerId::from_bytes(response.peer_id.as_slice()) {
			Ok(peer_id) => peer_id,
			Err(e) => {
				warn!(
					target: LOG_TARGET,
					"Failed to decode parachain peer ID in response from {peer_id:?}: {e}",
				);
				return;
			},
		};

		if paranode_peer_id == self.parachain_network.local_peer_id() {
			warn!(
				target: LOG_TARGET,
				"Received own parachain node peer ID in bootnode response from {peer_id:?}. \
				 This should not happen as we don't request parachain bootnodes from self.",
			);
			return;
		}

		let paranode_addresses = response
			.addrs
			.into_iter()
			.map(Multiaddr::try_from)
			.take(MAX_ADDRESSES)
			.collect::<Result<Vec<_>, _>>();
		let paranode_addresses = match paranode_addresses {
			Ok(paranode_addresses) => paranode_addresses,
			Err(e) => {
				warn!(
					target: LOG_TARGET,
					"Failed to decode parachain node addresses in response from {peer_id:?}: {e}",
				);
				return;
			},
		};

		debug!(
			target: LOG_TARGET,
			"Discovered parachain bootnode {paranode_peer_id:?} with addresses {paranode_addresses:?}",
		);

		paranode_addresses.into_iter().for_each(|addr| {
			self.parachain_network.add_known_address(paranode_peer_id, addr);
			self.succeeded = true;
		});
	}

	fn handle_dht_event(&mut self, event: DhtEvent) {
		match event {
			DhtEvent::ProvidersFound(key, providers)
				// libp2p generates empty events, so also check if `providers` are not empty.
				if Some(key.clone()) == self.key_being_discovered && !providers.is_empty() =>
					self.handle_providers(providers),
			DhtEvent::NoMoreProviders(key) if Some(key.clone()) == self.key_being_discovered => {
				debug!(
					target: LOG_TARGET,
					"Parachain bootnode providers discovery finished for key {}",
					hex::encode(key),
				);
				self.key_being_discovered = None;
			},
			DhtEvent::ProvidersNotFound(key) if Some(key.clone()) == self.key_being_discovered => {
				debug!(
					target: LOG_TARGET,
					"Parachain bootnode providers not found for key {}",
					hex::encode(key),
				);
				self.key_being_discovered = None;
			},
			DhtEvent::ClosestPeersFound(peer_id, peers)	if self.find_node_queries.remove(&peer_id) => {
				if let Some((_, addrs)) = peers
					.into_iter()
					.find(|(peer, addrs)| peer == &peer_id && !addrs.is_empty())
				{
					trace!(
						target: LOG_TARGET,
						"Found addresses on the DHT for parachain bootnode provider {peer_id:?}: {addrs:?}",
					);
					for address in addrs {
						self.relay_chain_network.add_known_address(peer_id, address);
					}
					self.request_bootnode(peer_id);
				} else {
					debug!(
						target: LOG_TARGET,
						"Failed to find addresses on the DHT for parachain bootnode provider {peer_id:?}",
					);
				}
			},
			DhtEvent::ClosestPeersNotFound(peer_id) if self.find_node_queries.remove(&peer_id) => {
				debug!(
					target: LOG_TARGET,
					"Failed to find addresses on the DHT for parachain bootnode provider {peer_id:?}",
				);
			},
			_ => {},
		}
	}

	/// Run the bootnode discovery service.
	pub async fn run(mut self) -> RelayChainResult<()> {
		let mut import_notification_stream =
			self.relay_chain_interface.import_notification_stream().await?.fuse();
		let dht_event_stream = self
			.relay_chain_network
			.event_stream("parachain-bootnode-discovery")
			.filter_map(|e| async move {
				match e {
					Event::Dht(e) => Some(e),
					_ => None,
				}
			})
			.fuse();
		pin_mut!(dht_event_stream);

		// Make sure the relay chain hash is always available before starting the discovery.
		let header = import_notification_stream.select_next_some().await;
		self.latest_relay_chain_hash = Some(header.hash());

		loop {
			if !self.maybe_retry_discovery() {
				return Ok(());
			}

			tokio::select! {
				_ = &mut self.pending_start_discovery => {
					self.start_discovery().await?;
				},
				header = import_notification_stream.select_next_some() => {
					self.latest_relay_chain_hash = Some(header.hash());
				},
				event = dht_event_stream.select_next_some() => self.handle_dht_event(event),
				(peer_id, res) = self.pending_responses.select_next_some(),
					if !self.pending_responses.is_empty() =>
						self.handle_response(peer_id, res),
			}
		}
	}
}
