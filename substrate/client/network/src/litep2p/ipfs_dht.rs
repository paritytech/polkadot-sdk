// This file is part of Substrate

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

//! Implementation of indexed transaction publishing to IPFS Kademlia DHT.

use crate::{
	config::MultiaddrWithPeerId,
	ipfs_block_provider::{BlockProvider, Change},
};
use cid::Cid;
use futures::StreamExt;
use litep2p::{
	protocol::libp2p::kademlia::{
		Config, ConfigBuilder, KademliaEvent, KademliaHandle, Quorum, RecordKey,
	},
	types::multiaddr::Multiaddr,
	PeerId,
};
use log::trace;
use sp_core::hexdisplay::HexDisplay;
use std::{
	collections::{HashMap, HashSet},
	num::NonZeroUsize,
};

/// Log target for this file.
const LOG_TARGET: &str = "sub-libp2p::ipfs::dht";

/// IPFS Kademlia protocol name.
const KAD_PROTOCOL: &str = "/ipfs/kad/1.0.0";

/// Quorum to treat publishing successful. Note that litep2p tries to publish a provider to all
/// target peers and does not terminate the query once the quorum is reached.
const QUORUM: Quorum = Quorum::N(NonZeroUsize::new(10).expect("10 > 0; qed"));

/// Raw codec type.
// TODO: we should store codec on chain and use a proper one here.
const RAW_CODEC: u64 = 0x55;

pub(crate) struct IpfsDht {
	kademlia_handle: KademliaHandle,
	block_provider: Box<dyn BlockProvider>,
}

impl IpfsDht {
	pub fn new(
		bootnodes: Vec<MultiaddrWithPeerId>,
		block_provider: Box<dyn BlockProvider>,
	) -> (Self, Config) {
		let known_peers = {
			// Convert bootnodes into expected format deduplicating the addresses.
			let mut known_peers: HashMap<PeerId, HashSet<Multiaddr>> = HashMap::new();

			for address in bootnodes {
				let peer_id = address.peer_id.into();
				let multiaddr = address.concat().into();

				known_peers.entry(peer_id).or_default().insert(multiaddr);
			}

			known_peers.into_iter().map(|(k, v)| (k, v.into_iter().collect())).collect()
		};

		// TODO: we likely need to increase the number of providers in the memstore.
		let (config, kademlia_handle) = ConfigBuilder::new()
			.with_protocol_names(vec![KAD_PROTOCOL.into()])
			.with_known_peers(known_peers)
			.build();

		(Self { kademlia_handle, block_provider }, config)
	}

	pub async fn run(mut self) {
		let mut changes = self.block_provider.changes();

		loop {
			tokio::select! {
				change = changes.next() => {
					match change {
						None => {
							trace!(target: LOG_TARGET, "BlockProvider terminated, terminating IpfsDht");
							return
						},
						Some(Change::Added(multihash)) => {
							let cid = Cid::new_v1(RAW_CODEC, multihash);
							let key = RecordKey::new(&cid.hash().to_bytes());

							trace!(
								target: LOG_TARGET,
								"IPFS DHT start providing key: {}, CID: {}",
								HexDisplay::from(&key.as_ref()),
								cid,
							);

							self.kademlia_handle.start_providing(key, QUORUM).await;
						},
						Some(Change::Removed(multihash)) => {
							let cid = Cid::new_v1(RAW_CODEC, multihash);
							let key = RecordKey::new(&cid.hash().to_bytes());

							trace!(
								target: LOG_TARGET,
								"IPFS DHT stop providing key: {}, CID: {}",
								HexDisplay::from(&key.as_ref()),
								cid,
							);

							self.kademlia_handle.stop_providing(key).await;
						},
					}
				},
				event = self.kademlia_handle.next() => {
					match event {
						None => {
							trace!(target: LOG_TARGET, "IPFS Kademlia terminated, terminating IpfsDht");
							return
						}
						Some(KademliaEvent::AddProviderSuccess { query_id: _, provided_key }) => trace!(
							target: LOG_TARGET,
							"IPFS DHT provider publish success, key: {}",
							HexDisplay::from(&provided_key.as_ref()),
						),
						Some(KademliaEvent::QueryFailed { query_id: _ }) => trace!(
							target: LOG_TARGET,
							"IPFS DHT provider publish failed",
						),
						// We are not interested in other events.
						Some(_) => {},
					}
				}
			}
		}
	}
}
