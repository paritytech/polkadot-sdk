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
	pub parachain_genesis_hash: Vec<u8>,
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
	parachain_genesis_hash: Vec<u8>,
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
		// Plain RFC-0008 bootnode advertisement; capability-scoped advertisement is opted into by
		// dedicated callers (e.g. a spec-msg serving node) via `start_capability_advertisement`.
		capability: Vec::new(),
	});

	if let Err(e) = bootnode_advertisement.run().await {
		error!(target: LOG_TARGET, "Bootnode advertisement terminated with error: {e}");
	}
}

async fn bootnode_discovery(
	para_id: ParaId,
	parachain_network: Arc<dyn NetworkService>,
	parachain_genesis_hash: Vec<u8>,
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
		// Own-parachain bootstrap: addresses are injected into the parachain
		// network, not streamed to a caller.
		discovered_tx: None,
		// Plain RFC-0008 bootnode discovery; capability-scoped discovery is opted into by dedicated
		// callers (e.g. `cumulus-client-source-discovery`) constructing the params directly.
		capability: Vec::new(),
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
				parachain_genesis_hash.clone(),
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

/// Params for [`start_capability_advertisement`].
pub struct StartCapabilityAdvertisementParams<'a> {
	/// Task manager.
	pub task_manager: &'a mut TaskManager,
	/// Parachain ID whose bootnodes are advertised under the capability-scoped key.
	pub para_id: ParaId,
	/// Capability tag mixed into the DHT provider key, e.g. `b"spec-msg/v1".to_vec()`.
	pub capability: Vec<u8>,
	/// Relay chain interface.
	pub relay_chain_interface: Arc<dyn RelayChainInterface>,
	/// Relay chain network service.
	pub relay_chain_network: Arc<dyn NetworkService>,
	/// Parachain node network service.
	pub parachain_network: Arc<dyn NetworkService>,
}

/// Start a DHT-only **capability** advertisement task: publishes this node's bootnodes under the
/// capability-scoped provider key (`para_id ++ capability ++ randomness`), so capability-aware
/// discoverers (e.g. `cumulus-client-source-discovery`) resolve *only* nodes serving that
/// capability — sidestepping the closest-K dilution where the serving subset is lost among all of a
/// parachain's collators under the single plain key.
///
/// It reuses [`BootnodeAdvertisement`]'s epoch tracking + provider (re)publication but answers
/// **no** `/paranode` requests (there is one `/paranode` protocol per node; those are served by the
/// plain advertiser). Run this *alongside* [`start_bootnode_tasks`] on a serving node so it stays
/// discoverable both as a plain bootnode and under the capability.
pub fn start_capability_advertisement(
	StartCapabilityAdvertisementParams {
		task_manager,
		para_id,
		capability,
		relay_chain_interface,
		relay_chain_network,
		parachain_network,
	}: StartCapabilityAdvertisementParams,
) {
	log::info!(
		target: LOG_TARGET,
		"Starting DHT capability advertisement for para {para_id} under capability {}",
		String::from_utf8_lossy(&capability),
	);
	task_manager.spawn_essential_handle().spawn(
		"cumulus-dht-capability-advertisement",
		None,
		async move {
			// Idle `/paranode` channel: this task only publishes the capability-scoped DHT provider
			// key. Keeping the sender alive holds the receiver open, so `recv()` pends forever and
			// the advertiser's request branch never fires (a *closed* channel would instead
			// terminate it). Genesis/fork/public-addresses are only read when building `/paranode`
			// responses (never here), so they are left empty.
			let (request_sender, request_receiver) = async_channel::bounded::<IncomingRequest>(1);
			let advertisement = BootnodeAdvertisement::new(BootnodeAdvertisementParams {
				para_id,
				relay_chain_interface,
				relay_chain_network,
				request_receiver,
				parachain_network,
				advertise_non_global_ips: false,
				parachain_genesis_hash: Vec::new(),
				parachain_fork_id: None,
				public_addresses: Vec::new(),
				capability,
			});
			let result = advertisement.run().await;
			// Hold the idle `/paranode` sender for the whole run, then release it.
			drop(request_sender);
			if let Err(e) = result {
				error!(target: LOG_TARGET, "Capability advertisement terminated with error: {e}");
			}
		},
	);
}
