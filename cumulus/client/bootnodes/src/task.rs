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

//! Parachain bootnodes advertisement and discovery service.

use crate::{
	advertisement::{BootnodeAdvertisement, BootnodeAdvertisementParams},
	config::paranode_protocol_name,
	discovery::{BootnodeDiscovery, BootnodeDiscoveryParams},
};
use cumulus_primitives_core::{relay_chain::BlockId, ParaId};
use cumulus_relay_chain_interface::RelayChainInterface;
use log::{debug, error};
use num_traits::Zero;
use parachains_common::Hash as ParaHash;
use sc_network::{request_responses::IncomingRequest, service::traits::NetworkService, Multiaddr};
use sc_service::TaskManager;
use std::sync::Arc;

/// Log target for this crate.
const LOG_TARGET: &str = "bootnodes";

/// Bootnode advertisement task params.
pub struct StartBootnodeTasksParams<'a> {
	/// Enable embedded DHT bootnode.
	pub embedded_dht_bootnode: bool,
	/// Enable DHT bootnode discovery.
	pub dht_bootnode_discovery: bool,
	/// Parachain ID.
	pub para_id: ParaId,
	/// Task manager.
	pub task_manager: &'a mut TaskManager,
	/// Relay chain interface.
	pub relay_chain_interface: Arc<dyn RelayChainInterface>,
	/// Relay chain fork ID.
	pub relay_chain_fork_id: Option<String>,
	/// Relay chain network service.
	pub relay_chain_network: Arc<dyn NetworkService>,
	/// `/paranode` protocol request receiver.
	pub request_receiver: async_channel::Receiver<IncomingRequest>,
	/// Parachain node network service.
	pub parachain_network: Arc<dyn NetworkService>,
	/// Whether to advertise non-global IP addresses.
	pub advertise_non_global_ips: bool,
	/// Parachain genesis hash.
	pub parachain_genesis_hash: ParaHash,
	/// Parachain fork ID.
	pub parachain_fork_id: Option<String>,
	/// Parachain public addresses provided by the operator.
	pub parachain_public_addresses: Vec<Multiaddr>,
}

async fn bootnode_advertisement(
	para_id: ParaId,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	relay_chain_network: Arc<dyn NetworkService>,
	request_receiver: async_channel::Receiver<IncomingRequest>,
	parachain_network: Arc<dyn NetworkService>,
	advertise_non_global_ips: bool,
	parachain_genesis_hash: ParaHash,
	parachain_fork_id: Option<String>,
	public_addresses: Vec<Multiaddr>,
) {
	let bootnode_advertisement = BootnodeAdvertisement::new(BootnodeAdvertisementParams {
		para_id,
		relay_chain_interface,
		relay_chain_network,
		request_receiver,
		parachain_network,
		advertise_non_global_ips,
		parachain_genesis_hash,
		parachain_fork_id,
		public_addresses,
	});

	if let Err(e) = bootnode_advertisement.run().await {
		error!(target: LOG_TARGET, "Bootnode advertisement terminated with error: {e}");
	}
}

async fn bootnode_discovery(
	para_id: ParaId,
	parachain_network: Arc<dyn NetworkService>,
	parachain_genesis_hash: ParaHash,
	parachain_fork_id: Option<String>,
	relay_chain_interface: Arc<dyn RelayChainInterface>,
	relay_chain_fork_id: Option<String>,
	relay_chain_network: Arc<dyn NetworkService>,
) {
	let relay_chain_genesis_hash =
		match relay_chain_interface.header(BlockId::Number(Zero::zero())).await {
			Ok(Some(header)) => header.hash().as_bytes().to_vec(),
			Ok(None) => {
				error!(
					target: LOG_TARGET,
					"Bootnode discovery: relay chain genesis hash does not exist",
				);
				// Make essential task fail.
				return;
			},
			Err(e) => {
				error!(
					target: LOG_TARGET,
					"Bootnode discovery: failed to obtain relay chain genesis hash: {e}",
				);
				// Make essential task fail.
				return;
			},
		};

	let paranode_protocol_name =
		paranode_protocol_name(relay_chain_genesis_hash, relay_chain_fork_id.as_deref());

	let bootnode_discovery = BootnodeDiscovery::new(BootnodeDiscoveryParams {
		para_id,
		parachain_network,
		parachain_genesis_hash,
		parachain_fork_id,
		relay_chain_interface,
		relay_chain_network,
		paranode_protocol_name,
	});

	match bootnode_discovery.run().await {
		// Do not terminate the essentil task if bootnode discovery succeeded.
		Ok(()) => std::future::pending().await,
		Err(e) => error!(target: LOG_TARGET, "Bootnode discovery terminated with error: {e}"),
	}
}

/// Start parachain bootnode advertisement and discovery tasks.
pub fn start_bootnode_tasks(
	StartBootnodeTasksParams {
		embedded_dht_bootnode,
		dht_bootnode_discovery,
		para_id,
		task_manager,
		relay_chain_interface,
		relay_chain_fork_id,
		relay_chain_network,
		request_receiver,
		parachain_network,
		advertise_non_global_ips,
		parachain_genesis_hash,
		parachain_fork_id,
		parachain_public_addresses,
	}: StartBootnodeTasksParams,
) {
	debug!(
		target: LOG_TARGET,
		"Embedded DHT bootnode enabled: {embedded_dht_bootnode}; \
		 DHT bootnode discovery enabled: {dht_bootnode_discovery}",
	);

	if embedded_dht_bootnode {
		task_manager.spawn_essential_handle().spawn(
			"cumulus-dht-bootnode-advertisement",
			None,
			bootnode_advertisement(
				para_id,
				relay_chain_interface.clone(),
				relay_chain_network.clone(),
				request_receiver,
				parachain_network.clone(),
				advertise_non_global_ips,
				parachain_genesis_hash,
				parachain_fork_id.clone(),
				parachain_public_addresses,
			),
		);
	}

	if dht_bootnode_discovery {
		task_manager.spawn_essential_handle().spawn(
			"cumulus-dht-bootnode-discovery",
			None,
			bootnode_discovery(
				para_id,
				parachain_network,
				parachain_genesis_hash,
				parachain_fork_id,
				relay_chain_interface,
				relay_chain_fork_id,
				relay_chain_network,
			),
		);
	}
}
