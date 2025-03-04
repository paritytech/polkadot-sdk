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
use futures::StreamExt;
use ip_network::IpNetwork;
use log::{debug, warn};
use prost::Message;
use sc_network::{
	config::OutgoingResponse, multiaddr::Protocol, request_responses::IncomingRequest,
	service::traits::NetworkService, KademliaKey, Multiaddr,
};
use sp_consensus_babe::{digests::CompatibleDigestItem, Epoch, Randomness};
use sp_runtime::traits::Header as _;
use std::{collections::HashSet, sync::Arc};

/// Log target for this file.
const LOG_TARGET: &str = "bootnodes::discovery";

/// Parachain bootnode discovery parameters.
pub struct BootnodeDiscoveryParams {
	/// Parachain ID.
	pub para_id: ParaId,
	/// Parachain genesis hash.
	pub parachain_genesis_hash: Vec<u8>,
	/// Parachain fork ID.
	pub parachain_fork_id: Option<String>,
	/// Relay chain network service.
	pub relay_chain_network: Arc<dyn NetworkService>,
}

/// Parachain bootnode discovery service.
pub struct BootnodeDiscovery {
	para_id: ParaId,
	parachain_genesis_hash: Vec<u8>,
	parachain_fork_id: Option<String>,
	relay_chain_network: Arc<dyn NetworkService>,
}

impl BootnodeDiscovery {
	/// Create a new bootnode discovery service.
	pub fn new(
		BootnodeDiscoveryParams {
			para_id,
			parachain_genesis_hash,
			parachain_fork_id,
			relay_chain_network,
		}: BootnodeDiscoveryParams,
	) -> Self {
		Self { para_id, parachain_genesis_hash, parachain_fork_id, relay_chain_network }
	}

	/// Run the bootnode discovery service.
	pub async fn run(self) {}
}
