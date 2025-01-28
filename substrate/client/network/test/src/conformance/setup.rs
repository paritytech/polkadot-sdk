// This file is part of Substrate.

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

use sc_network::{
	config::{
		notification_service, FullNetworkConfiguration, IncomingRequest, MultiaddrWithPeerId,
		NetworkConfiguration, NonReservedPeerMode, NotificationHandshake, OutgoingResponse, Params,
		ProtocolId, Role, SetConfig,
	},
	service::traits::{NetworkService, NotificationEvent},
	IfDisconnected, Litep2pNetworkBackend, NetworkBackend, NetworkRequest, NetworkWorker,
	NotificationMetrics, NotificationService, PeerId, Roles,
};
use sc_network_common::{sync::message::BlockAnnouncesHandshake, ExHashT};
use sc_utils::notification;
use sp_core::H256;
use sp_runtime::traits::{Block as BlockT, Zero};
use std::{sync::Arc, time::Duration};
use substrate_test_runtime_client::runtime;
use tokio::{sync::Mutex, task::JoinHandle};

struct NetworkBackendClient {
	network_service: Arc<dyn NetworkService>,
	notification_service: Arc<Mutex<Box<dyn NotificationService>>>,
	receiver: async_channel::Receiver<IncomingRequest>,
}

/// Configure the network backend client for tests based on the given service.
///
/// This will setup:
/// - `/request-response/1` request response protocol with bounded channel of 32 requests
/// - `/block-announces/1` notification protocol
pub fn create_network_backend<N>() -> NetworkBackendClient
where
	N: NetworkBackend<runtime::Block, runtime::Hash>,
{
	let (tx, rx) = async_channel::bounded(32);
	let request_response_config = N::request_response_config(
		"/request-response/1".into(),
		vec![],
		1024,
		1024,
		Duration::from_secs(2),
		Some(tx),
	);

	let role = Role::Full;
	let net_conf = NetworkConfiguration::new_local();
	let mut network_config = FullNetworkConfiguration::new(&net_conf, None);
	network_config.add_request_response_protocol(request_response_config);
	let genesis_hash = runtime::Hash::zero();
	let (block_announce_config, notification_service) = N::notification_config(
		"/block-announces/1".into(),
		vec![],
		1024,
		Some(NotificationHandshake::new(BlockAnnouncesHandshake::<runtime::Block>::build(
			Roles::from(&Role::Full),
			Zero::zero(),
			genesis_hash,
			genesis_hash,
		))),
		SetConfig {
			in_peers: 1,
			out_peers: 1,
			reserved_nodes: vec![],
			non_reserved_mode: NonReservedPeerMode::Accept,
		},
		NotificationMetrics::new(None),
		network_config.peer_store_handle(),
	);
	let worker = N::new(Params::<runtime::Block, runtime::Hash, N> {
		block_announce_config,
		role,
		executor: Box::new(|f| {
			tokio::spawn(f);
		}),
		genesis_hash: runtime::Hash::zero(),
		network_config,
		protocol_id: ProtocolId::from("test"),
		fork_id: None,
		metrics_registry: None,
		bitswap_config: None,
		notification_metrics: NotificationMetrics::new(None),
	})
	.unwrap();
	let network_service = worker.network_service();

	// Run the worker in the backend.
	tokio::spawn(worker.run());

	NetworkBackendClient {
		network_service,
		notification_service: Arc::new(Mutex::new(notification_service)),
		receiver: rx,
	}
}
