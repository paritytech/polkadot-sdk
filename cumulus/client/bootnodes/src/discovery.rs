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

//! Parachain bootnodes discovery.

use codec::{Compact, CompactRef, Decode, Encode};
use cumulus_primitives_core::{
	relay_chain::{Hash, Header},
	ParaId,
};
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use futures::{pin_mut, Stream, StreamExt};
use ip_network::IpNetwork;
use log::{debug, warn};
use prost::Message;
use sc_network::{
	config::OutgoingResponse,
	event::{DhtEvent, Event},
	multiaddr::Protocol,
	request_responses::{IfDisconnected, IncomingRequest},
	service::traits::NetworkService,
	KademliaKey, Multiaddr, PeerId, ProtocolName,
};
use sp_consensus_babe::{digests::CompatibleDigestItem, Epoch, Randomness};
use sp_runtime::traits::Header as _;
use std::{collections::HashSet, marker::Send, pin::Pin, sync::Arc};

/// Log target for this file.
const LOG_TARGET: &str = "bootnodes::discovery";

/// Parachain bootnode discovery parameters.
pub struct BootnodeDiscoveryParams {
	/// Parachain ID.
	pub para_id: ParaId,
	/// Parachain node network service.
	pub parachain_network: Arc<dyn NetworkService>,
	/// Parachain genesis hash.
	pub parachain_genesis_hash: Vec<u8>,
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
	para_id: ParaId,
	para_id_scale_compact: Vec<u8>,
	parachain_network: Arc<dyn NetworkService>,
	parachain_genesis_hash: Vec<u8>,
	parachain_fork_id: Option<String>,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	relay_chain_network: Arc<dyn NetworkService>,
	parachain_key: Option<KademliaKey>,
	paranode_protocol_name: ProtocolName,
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
			para_id,
			para_id_scale_compact: CompactRef(&para_id).encode(),
			parachain_network,
			parachain_genesis_hash,
			parachain_fork_id,
			relay_chain_interface,
			relay_chain_network,
			parachain_key: None,
			paranode_protocol_name,
		}
	}

	async fn current_epoch(&self, hash: Hash) -> RelayChainResult<Epoch> {
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

	async fn request_providers(&mut self, header: Header) -> RelayChainResult<()> {
		let current_epoch = self.current_epoch(header.hash()).await?;
		let current_epoch_key = self.epoch_key(current_epoch.randomness);
		self.parachain_key = Some(current_epoch_key.clone());
		self.relay_chain_network.get_providers(current_epoch_key.clone());

		debug!(
			target: LOG_TARGET,
			"Started discovery of parachain bootnode providers for current epoch key {}",
			hex::encode(current_epoch_key),
		);

		Ok(())
	}

	async fn handle_providers(&mut self, providers: Vec<PeerId>) {
		debug!(
			target: LOG_TARGET,
			"Found parachain bootnode providers: {providers:?}",
		);

		for peer_id in providers {
			if peer_id == self.relay_chain_network.local_peer_id() {
				continue;
			}

			// TODO: query nodes asynchronously in parallel.
			let res = self
				.relay_chain_network
				.request(
					peer_id,
					self.paranode_protocol_name.clone(),
					self.para_id_scale_compact.clone(),
					None,
					IfDisconnected::TryConnect,
				)
				.await;

			let payload = match res {
				Ok((payload, _)) => payload,
				Err(e) => {
					warn!(
						target: LOG_TARGET,
						"Failed to query parachain bootnode from {peer_id:?}: {e}",
					);
					continue;
				},
			};

			let response = match crate::schema::Response::decode(payload.as_slice()) {
				Ok(response) => response,
				Err(e) => {
					warn!(
						target: LOG_TARGET,
						"Failed to decode parachain bootnode response from {peer_id:?}: {e}",
					);
					continue;
				},
			};

			match (response.genesis_hash, response.fork_id) {
				(genesis_hash, fork_id)
					if genesis_hash == self.parachain_genesis_hash &&
						fork_id == self.parachain_fork_id => {},
				(genesis_hash, fork_id) => {
					warn!(
						target: LOG_TARGET,
						"Received invalid parachain bootnode response from {peer_id:?}: \
						 genesis hash {}, fork ID {:?} don't match expected genesis hash {}, fork ID {:?}",
						hex::encode(genesis_hash),
						fork_id,
						hex::encode(self.parachain_genesis_hash.clone()),
						self.parachain_fork_id,
					);
					continue;
				},
			}

			let paranode_peer_id = match PeerId::from_bytes(response.peer_id.as_slice()) {
				Ok(peer_id) => peer_id,
				Err(e) => {
					warn!(
						target: LOG_TARGET,
						"Failed to decode parachain peer ID in response from {peer_id:?}: {e}",
					);
					continue;
				},
			};

			if paranode_peer_id == self.parachain_network.local_peer_id() {
				continue;
			}

			let paranode_addresses = response
				.addrs
				.into_iter()
				.map(Multiaddr::try_from)
				.collect::<Result<Vec<_>, _>>();
			let paranode_addresses = match paranode_addresses {
				Ok(paranode_addresses) => paranode_addresses,
				Err(e) => {
					warn!(
						target: LOG_TARGET,
						"Failed to decode parachain addresses in response from {peer_id:?}: {e}",
					);
					continue;
				},
			};

			debug!(
				target: LOG_TARGET,
				"Discovered parachain bootnode {paranode_peer_id:?} with addresses {paranode_addresses:?}",
			);

			paranode_addresses.into_iter().for_each(|addr| {
				self.parachain_network.add_known_address(paranode_peer_id.clone(), addr);
			});
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

		// Get the first imported block to derive the provider key and request providers.
		let header = import_notification_stream.select_next_some().await;
		drop(import_notification_stream);
		self.request_providers(header).await?;

		// Wait for matching `DhtEvent::ProvidersFound` and query `/paranode` protocol.
		loop {
			match dht_event_stream.select_next_some().await {
				DhtEvent::ProvidersFound(key, providers) => {
					if Some(key) == self.parachain_key {
						self.handle_providers(providers).await;
						break;
						// TODO: libp2p may return found providers in multiple events.
					}
				},
				DhtEvent::ProvidersNotFound(key) =>
					if Some(key.clone()) == self.parachain_key {
						warn!(
							target: LOG_TARGET,
							"Parachain bootnode providers not found for current epoch key {}",
							hex::encode(key),
						);
						break;
					},
				_ => {},
			}
		}

		drop(dht_event_stream);

		// Do not terminate the essential task.
		std::future::pending().await
	}
}
