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

//! Parachain bootnode advertisement.

use crate::config::MAX_ADDRESSES;
use codec::{Compact, CompactRef, Decode, Encode};
use cumulus_primitives_core::{
	relay_chain::{Hash as RelayHash, Header as RelayHeader},
	ParaId,
};
use cumulus_relay_chain_interface::{RelayChainInterface, RelayChainResult};
use futures::{future::Fuse, pin_mut, FutureExt, StreamExt};
use ip_network::IpNetwork;
use log::{debug, error, trace, warn};
use parachains_common::Hash as ParaHash;
use prost::Message;
use sc_network::{
	config::OutgoingResponse,
	event::{DhtEvent, Event},
	multiaddr::Protocol,
	request_responses::IncomingRequest,
	service::traits::NetworkService,
	KademliaKey, Multiaddr,
};
use sp_consensus_babe::{digests::CompatibleDigestItem, Epoch, Randomness};
use sp_runtime::traits::Header as _;
use std::{collections::HashSet, pin::Pin, sync::Arc};
use tokio::time::Sleep;

/// Log target for this file.
const LOG_TARGET: &str = "bootnodes::advertisement";

/// Delay before retrying the DHT content provider publish operation.
const RETRY_DELAY: std::time::Duration = std::time::Duration::from_secs(30);

/// Parachain bootnode advertisement parameters.
pub struct BootnodeAdvertisementParams {
	/// Parachain ID.
	pub para_id: ParaId,
	/// Relay chain interface.
	pub relay_chain_interface: Arc<dyn RelayChainInterface>,
	/// Relay chain node network service.
	pub relay_chain_network: Arc<dyn NetworkService>,
	/// Bootnode request-response protocol request receiver.
	pub request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Parachain node network service.
	pub parachain_network: Arc<dyn NetworkService>,
	/// Whether to advertise non-global IPs.
	pub advertise_non_global_ips: bool,
	/// Parachain genesis hash.
	pub parachain_genesis_hash: ParaHash,
	/// Parachain fork ID.
	pub parachain_fork_id: Option<String>,
	/// Parachain side public addresses.
	pub public_addresses: Vec<Multiaddr>,
}

/// Parachain bootnode advertisement service.
pub struct BootnodeAdvertisement {
	para_id: ParaId,
	para_id_scale_compact: Vec<u8>,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	relay_chain_network: Arc<dyn NetworkService>,
	current_epoch_key: Option<KademliaKey>,
	next_epoch_key: Option<KademliaKey>,
	current_epoch_publish_retry: Pin<Box<Fuse<Sleep>>>,
	next_epoch_publish_retry: Pin<Box<Fuse<Sleep>>>,
	request_receiver: async_channel::Receiver<IncomingRequest>,
	parachain_network: Arc<dyn NetworkService>,
	advertise_non_global_ips: bool,
	parachain_genesis_hash: ParaHash,
	parachain_fork_id: Option<String>,
	public_addresses: Vec<Multiaddr>,
}

impl BootnodeAdvertisement {
	/// Create a new bootnode advertisement service.
	pub fn new(
		BootnodeAdvertisementParams {
			para_id,
			relay_chain_interface,
			relay_chain_network,
			request_receiver,
			parachain_network,
			advertise_non_global_ips,
			parachain_genesis_hash,
			parachain_fork_id,
			public_addresses,
		}: BootnodeAdvertisementParams,
	) -> Self {
		// Discard `/p2p/<peer_id>` from public addresses on initialization to not generate warnings
		// on every request for what is an operator mistake.
		let local_peer_id = parachain_network.local_peer_id();
		let public_addresses = public_addresses
			.into_iter()
			.filter_map(|mut addr| match addr.iter().last() {
				Some(Protocol::P2p(peer_id)) if &peer_id == local_peer_id.as_ref() => {
					addr.pop();
					Some(addr)
				},
				Some(Protocol::P2p(_)) => {
					warn!(
						target: LOG_TARGET,
						"Discarding public address containing not our peer ID: {addr}",
					);
					None
				},
				_ => Some(addr),
			})
			.collect();

		Self {
			para_id,
			para_id_scale_compact: CompactRef(&para_id).encode(),
			relay_chain_interface,
			relay_chain_network,
			current_epoch_key: None,
			next_epoch_key: None,
			current_epoch_publish_retry: Box::pin(Fuse::terminated()),
			next_epoch_publish_retry: Box::pin(Fuse::terminated()),
			request_receiver,
			parachain_network,
			advertise_non_global_ips,
			parachain_genesis_hash,
			parachain_fork_id,
			public_addresses,
		}
	}

	async fn current_epoch(&self, hash: RelayHash) -> RelayChainResult<Epoch> {
		let res = self
			.relay_chain_interface
			.call_runtime_api("BabeApi_current_epoch", hash, &[])
			.await?;
		Decode::decode(&mut &*res).map_err(Into::into)
	}

	async fn next_epoch(&self, hash: RelayHash) -> RelayChainResult<Epoch> {
		let res = self
			.relay_chain_interface
			.call_runtime_api("BabeApi_next_epoch", hash, &[])
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

	async fn current_and_next_epoch_keys(
		&self,
		header: RelayHeader,
	) -> (Option<KademliaKey>, Option<KademliaKey>) {
		let hash = header.hash();
		let number = header.number();

		let current_epoch = match self.current_epoch(hash).await {
			Ok(epoch) => Some(epoch),
			Err(e) => {
				warn!(
					target: LOG_TARGET,
					"Failed to query current epoch for #{number} {hash:?}: {e}",
				);

				None
			},
		};

		let next_epoch = match self.next_epoch(hash).await {
			Ok(epoch) => Some(epoch),
			Err(e) => {
				warn!(
					target: LOG_TARGET,
					"Failed to query next epoch for #{number} {hash:?}: {e}",
				);

				None
			},
		};

		(
			current_epoch.map(|epoch| self.epoch_key(epoch.randomness)),
			next_epoch.map(|epoch| self.epoch_key(epoch.randomness)),
		)
	}

	async fn handle_import_notification(&mut self, header: RelayHeader) {
		if let Some(ref old_current_epoch_key) = self.current_epoch_key {
			// Readvertise on start of new epoch only.
			let Some(next_epoch_descriptor) =
				header.digest().convert_first(|v| v.as_next_epoch_descriptor())
			else {
				return;
			};

			let next_epoch_key = self.epoch_key(next_epoch_descriptor.randomness);

			if Some(&next_epoch_key) == self.next_epoch_key.as_ref() {
				trace!(
					target: LOG_TARGET,
					"Next epoch descriptor contains the same randomness as the previous one, \
					 not considering this as epoch change (switched fork?)",
				);
				return;
			}

			// Epoch changed, cancel retry attempts.
			self.current_epoch_publish_retry = Box::pin(Fuse::terminated());
			self.next_epoch_publish_retry = Box::pin(Fuse::terminated());

			debug!(target: LOG_TARGET, "New epoch started, readvertising parachain bootnode.");

			// Stop advertisement of the obsolete key.
			debug!(
				target: LOG_TARGET,
				"Stopping advertisement of bootnode for old current epoch key {}",
				hex::encode(old_current_epoch_key.as_ref()),
			);
			self.relay_chain_network.stop_providing(old_current_epoch_key.clone());

			// Advertise current keys.
			self.current_epoch_key = self.next_epoch_key.clone();
			self.next_epoch_key = Some(next_epoch_key);

			if let Some(ref current_epoch_key) = self.current_epoch_key {
				debug!(
					target: LOG_TARGET,
					"Advertising bootnode for current (old next) epoch key {}",
					hex::encode(current_epoch_key.as_ref()),
				);
				self.relay_chain_network.start_providing(current_epoch_key.clone());
			}

			if let Some(ref next_epoch_key) = self.next_epoch_key {
				debug!(
					target: LOG_TARGET,
					"Advertising bootnode for next epoch key {}",
					hex::encode(next_epoch_key.as_ref()),
				);
				self.relay_chain_network.start_providing(next_epoch_key.clone());
			}
		} else {
			// First advertisement on startup.
			let (current_epoch_key, next_epoch_key) =
				self.current_and_next_epoch_keys(header).await;
			self.current_epoch_key = current_epoch_key.clone();
			self.next_epoch_key = next_epoch_key.clone();

			if let Some(current_epoch_key) = current_epoch_key {
				debug!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for current epoch key {}",
					hex::encode(current_epoch_key.as_ref()),
				);

				self.relay_chain_network.start_providing(current_epoch_key);
			} else {
				warn!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for current epoch failed: no key."
				);
			}

			if let Some(next_epoch_key) = next_epoch_key {
				debug!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for next epoch key {}",
					hex::encode(next_epoch_key.as_ref()),
				);

				self.relay_chain_network.start_providing(next_epoch_key);
			} else {
				warn!(
					target: LOG_TARGET,
					"Initial advertisement of bootnode for next epoch failed: no key."
				);
			}
		}
	}

	/// The list of parachain side addresses.
	///
	/// The addresses are sorted as follows:
	///  1) public addresses provided by the operator
	///  2) global listen addresses
	///  3) discovered external addresses
	///  4) non-global listen addresses
	///  5) loopback listen addresses
	fn paranode_addresses(&self) -> Vec<Multiaddr> {
		let local_peer_id = self.parachain_network.local_peer_id();

		// Discard `/p2p/<peer_id>` part. `None` if the address contains foreign peer ID.
		let without_p2p = |mut addr: Multiaddr| match addr.iter().last() {
			Some(Protocol::P2p(peer_id)) if &peer_id == local_peer_id.as_ref() => {
				addr.pop();
				Some(addr)
			},
			Some(Protocol::P2p(_)) => {
				warn!(
					target: LOG_TARGET,
					"Ignoring parachain side address containing not our peer ID: {addr}",
				);
				None
			},
			_ => Some(addr),
		};

		// Check if the address is global.
		let is_global = |address: &Multiaddr| {
			address.iter().all(|protocol| match protocol {
				// The `ip_network` library is used because its `is_global()` method is stable,
				// while `is_global()` in the standard library currently isn't.
				Protocol::Ip4(ip) => IpNetwork::from(ip).is_global(),
				Protocol::Ip6(ip) => IpNetwork::from(ip).is_global(),
				_ => true,
			})
		};

		// Check if the address is a loopback address.
		let is_loopback = |address: &Multiaddr| {
			address.iter().any(|protocol| match protocol {
				Protocol::Ip4(ip) => IpNetwork::from(ip).is_loopback(),
				Protocol::Ip6(ip) => IpNetwork::from(ip).is_loopback(),
				_ => false,
			})
		};

		// 1) public addresses provided by the operator
		let public_addresses = self.public_addresses.clone().into_iter();

		// 2) global listen addresses
		let global_listen_addresses =
			self.parachain_network.listen_addresses().into_iter().filter(is_global);

		// 3a) discovered external addresses (global)
		let global_external_addresses =
			self.parachain_network.external_addresses().into_iter().filter(is_global);

		// 3b) discovered external addresses (non-global)
		let non_global_external_addresses = self
			.parachain_network
			.external_addresses()
			.into_iter()
			.filter(|addr| !is_global(addr));

		// 4) non-global listen addresses
		let non_global_listen_addresses = self
			.parachain_network
			.listen_addresses()
			.into_iter()
			.filter(|addr| !is_global(addr) && !is_loopback(addr));

		// 5) loopback listen addresses
		let loopback_listen_addresses =
			self.parachain_network.listen_addresses().into_iter().filter(is_loopback);

		let mut seen = HashSet::new();

		public_addresses
			.chain(global_listen_addresses)
			.chain(global_external_addresses)
			.chain(
				self.advertise_non_global_ips
					.then_some(
						non_global_external_addresses
							.chain(non_global_listen_addresses)
							.chain(loopback_listen_addresses),
					)
					.into_iter()
					.flatten(),
			)
			.filter_map(without_p2p)
			// Deduplicate addresses.
			.filter(|addr| seen.insert(addr.clone()))
			.take(MAX_ADDRESSES)
			.collect()
	}

	fn handle_request(&mut self, req: IncomingRequest) {
		if req.payload == self.para_id_scale_compact {
			trace!(
				target: LOG_TARGET,
				"Serving paranode addresses request from {:?} for parachain ID {}",
				req.peer,
				self.para_id,
			);

			let response = crate::schema::Response {
				peer_id: self.parachain_network.local_peer_id().to_bytes(),
				addrs: self.paranode_addresses().iter().map(|a| a.to_vec()).collect(),
				genesis_hash: self.parachain_genesis_hash.clone().as_bytes().to_vec(),
				fork_id: self.parachain_fork_id.clone(),
			};

			let _ = req.pending_response.send(OutgoingResponse {
				result: Ok(response.encode_to_vec()),
				reputation_changes: Vec::new(),
				sent_feedback: None,
			});
		} else {
			let payload = req.payload;
			match Compact::<ParaId>::decode(&mut &payload[..]) {
				Ok(para_id) => {
					trace!(
						target: LOG_TARGET,
						"Ignoring request for parachain ID {} != self parachain ID {} from {:?}",
						para_id.0,
						self.para_id,
						req.peer,
					);
				},
				Err(e) => {
					trace!(
						target: LOG_TARGET,
						"Cannot decode parachain ID in a request from {:?}: {e}",
						req.peer,
					);
				},
			}
		}
	}

	fn handle_dht_event(&mut self, event: DhtEvent) {
		match event {
			DhtEvent::StartedProviding(key) =>
				if Some(&key) == self.current_epoch_key.as_ref() {
					debug!(
						target: LOG_TARGET,
						"Successfully published provider for current epoch key {}",
						hex::encode(key.as_ref()),
					);
				} else if Some(&key) == self.next_epoch_key.as_ref() {
					debug!(
						target: LOG_TARGET,
						"Successfully published provider for next epoch key {}",
						hex::encode(key.as_ref()),
					);
				},
			DhtEvent::StartProvidingFailed(key) =>
				if Some(&key) == self.current_epoch_key.as_ref() {
					debug!(
						target: LOG_TARGET,
						"Failed to publish provider for current epoch key {}. Retrying in {RETRY_DELAY:?}",
						hex::encode(key.as_ref()),
					);
					self.current_epoch_publish_retry =
						Box::pin(tokio::time::sleep(RETRY_DELAY).fuse());
				} else if Some(&key) == self.next_epoch_key.as_ref() {
					debug!(
						target: LOG_TARGET,
						"Failed to publish provider for next epoch key {}. Retrying in {RETRY_DELAY:?}",
						hex::encode(key.as_ref()),
					);
					self.next_epoch_publish_retry =
						Box::pin(tokio::time::sleep(RETRY_DELAY).fuse());
				},
			_ => {},
		}
	}

	fn retry_for_current_epoch(&mut self) {
		if let Some(current_epoch_key) = self.current_epoch_key.clone() {
			debug!(
				target: LOG_TARGET,
				"Retrying advertising bootnode for current epoch key {}",
				hex::encode(current_epoch_key.as_ref()),
			);
			self.relay_chain_network.start_providing(current_epoch_key);
		} else {
			error!(
				target: LOG_TARGET,
				"Retrying advertising bootnode for current epoch failed: no key. This is a bug."
			);
		}
	}

	fn retry_for_next_epoch(&mut self) {
		if let Some(next_epoch_key) = self.next_epoch_key.clone() {
			debug!(
				target: LOG_TARGET,
				"Retrying advertising bootnode for next epoch key {}",
				hex::encode(next_epoch_key.as_ref()),
			);
			self.relay_chain_network.start_providing(next_epoch_key);
		} else {
			error!(
				target: LOG_TARGET,
				"Retrying advertising bootnode for next epoch failed: no key. This is a bug."
			);
		}
	}

	/// Run the bootnode advertisement service.
	pub async fn run(mut self) -> RelayChainResult<()> {
		let mut import_notification_stream =
			self.relay_chain_interface.import_notification_stream().await?;
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

		loop {
			tokio::select! {
				header = import_notification_stream.next() => match header {
					Some(header) => self.handle_import_notification(header).await,
					None => {
						debug!(
							target: LOG_TARGET,
							"Import notification stream terminated, terminating bootnode advertisement."
						);
						return Ok(());
					}
				},
				req = self.request_receiver.recv() => match req {
					Ok(req) => {
						self.handle_request(req);
					},
					Err(_) => {
						debug!(
							target: LOG_TARGET,
							"Paranode request receiver terminated, terminating bootnode advertisement."
						);
						return Ok(());
					}
				},
				event = dht_event_stream.select_next_some() => self.handle_dht_event(event),
				() = &mut self.current_epoch_publish_retry => self.retry_for_current_epoch(),
				() = &mut self.next_epoch_publish_retry => self.retry_for_next_epoch(),
			}
		}
	}
}
