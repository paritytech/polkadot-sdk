// Copyright 2022 Parity Technologies (UK) Ltd.
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

use polkadot_core_primitives::{Block, Hash, Header};
use sp_runtime::traits::{Block as BlockT, NumberFor};

use sc_network::{
	config::{
		NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake, ProtocolId, SetConfig,
	},
	NetworkService,
};

use sc_network_common::{role::Roles, sync::message::BlockAnnouncesHandshake};
use sc_service::{error::Error, Configuration, NetworkStarter, SpawnTaskHandle};
use sc_utils::mpsc::tracing_unbounded;

use std::{iter, sync::Arc};

/// Build the network service, the network status sinks and an RPC sender.
pub(crate) fn build_collator_network(
	config: &Configuration,
	spawn_handle: SpawnTaskHandle,
	genesis_hash: Hash,
	best_header: Header,
) -> Result<
	(Arc<NetworkService<Block, Hash>>, NetworkStarter, Box<dyn sp_consensus::SyncOracle + Send>),
	Error,
> {
	let protocol_id = config.protocol_id();
	let block_announce_config = get_block_announce_proto_config::<Block>(
		protocol_id.clone(),
		&None,
		Roles::from(&config.role),
		best_header.number,
		best_header.hash(),
		genesis_hash,
	);

	// RX is not used for anything because syncing is not started for the minimal node
	let (tx, _rx) = tracing_unbounded("mpsc_syncing_engine_protocol", 100_000);
	let network_params = sc_network::config::Params::<Block> {
		role: config.role.clone(),
		executor: {
			let spawn_handle = Clone::clone(&spawn_handle);
			Box::new(move |fut| {
				spawn_handle.spawn("libp2p-node", Some("networking"), fut);
			})
		},
		fork_id: None,
		network_config: config.network.clone(),
		genesis_hash,
		protocol_id,
		metrics_registry: config.prometheus_config.as_ref().map(|config| config.registry.clone()),
		block_announce_config,
		request_response_protocol_configs: Vec::new(),
		tx,
	};

	let network_worker = sc_network::NetworkWorker::new(network_params)?;
	let network_service = network_worker.service().clone();

	let (network_start_tx, network_start_rx) = futures::channel::oneshot::channel();

	// The network worker is responsible for gathering all network messages and processing
	// them. This is quite a heavy task, and at the time of the writing of this comment it
	// frequently happens that this future takes several seconds or in some situations
	// even more than a minute until it has processed its entire queue. This is clearly an
	// issue, and ideally we would like to fix the network future to take as little time as
	// possible, but we also take the extra harm-prevention measure to execute the networking
	// future using `spawn_blocking`.
	spawn_handle.spawn_blocking("network-worker", Some("networking"), async move {
		if network_start_rx.await.is_err() {
			tracing::warn!(
				"The NetworkStart returned as part of `build_network` has been silently dropped"
			);
			// This `return` might seem unnecessary, but we don't want to make it look like
			// everything is working as normal even though the user is clearly misusing the API.
			return
		}

		network_worker.run().await;
	});

	let network_starter = NetworkStarter::new(network_start_tx);

	Ok((network_service, network_starter, Box::new(SyncOracle {})))
}

struct SyncOracle;

impl sp_consensus::SyncOracle for SyncOracle {
	fn is_major_syncing(&self) -> bool {
		false
	}

	fn is_offline(&self) -> bool {
		true
	}
}

fn get_block_announce_proto_config<B: BlockT>(
	protocol_id: ProtocolId,
	fork_id: &Option<String>,
	roles: Roles,
	best_number: NumberFor<B>,
	best_hash: B::Hash,
	genesis_hash: B::Hash,
) -> NonDefaultSetConfig {
	let block_announces_protocol = {
		let genesis_hash = genesis_hash.as_ref();
		if let Some(ref fork_id) = fork_id {
			format!("/{}/{}/block-announces/1", array_bytes::bytes2hex("", genesis_hash), fork_id)
		} else {
			format!("/{}/block-announces/1", array_bytes::bytes2hex("", genesis_hash))
		}
	};

	NonDefaultSetConfig {
		notifications_protocol: block_announces_protocol.into(),
		fallback_names: iter::once(format!("/{}/block-announces/1", protocol_id.as_ref()).into())
			.collect(),
		max_notification_size: 1024 * 1024,
		handshake: Some(NotificationHandshake::new(BlockAnnouncesHandshake::<B>::build(
			roles,
			best_number,
			best_hash,
			genesis_hash,
		))),
		// NOTE: `set_config` will be ignored by `protocol.rs` as the block announcement
		// protocol is still hardcoded into the peerset.
		set_config: SetConfig {
			in_peers: 0,
			out_peers: 0,
			reserved_nodes: Vec::new(),
			non_reserved_mode: NonReservedPeerMode::Deny,
		},
	}
}
