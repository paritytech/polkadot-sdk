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

use crate::{
	peer_store::{PeerStore, PeerStoreHandle, PeerStoreProvider},
	protocol::notifications::{
		service::notification_service, Notifications, NotificationsOut, ProtocolConfig,
	},
	protocol_controller::{ProtoSetConfig, ProtocolController, SetId},
	service::metrics::NotificationMetrics,
	NotificationService,
};

use futures::prelude::*;
use libp2p::{
	core::upgrade,
	identity,
	swarm::{Swarm, SwarmEvent},
	PeerId,
};
use sc_utils::mpsc::tracing_unbounded;
use std::{collections::HashSet, iter, num::NonZeroUsize, pin::Pin, sync::Arc, time::Duration};

use litep2p::{
	config::ConfigBuilder as Litep2pConfigBuilder,
	protocol::notification::{
		Config as NotificationConfig, NotificationEvent as Litep2pNotificationEvent,
		NotificationHandle, ValidationResult as Litep2pValidationResult,
	},
	transport::tcp::config::Config as TcpConfig,
	types::protocol::ProtocolName as Litep2pProtocolName,
	Litep2p, Litep2pEvent,
};

fn setup_libp2p(
	in_peers: u32,
	out_peers: u32,
) -> (Swarm<Notifications>, PeerStoreHandle, Box<dyn NotificationService>) {
	let local_key = identity::Keypair::generate_ed25519();
	let local_peer_id = PeerId::from(local_key.public());
	let peer_store = PeerStore::new(vec![], None);

	let (to_notifications, from_controller) =
		tracing_unbounded("test_protocol_controller_to_notifications", 10_000);

	let (protocol_handle_pair, notif_service) = notification_service("/foo".into());

	let (controller_handle, controller) = ProtocolController::new(
		SetId::from(0usize),
		ProtoSetConfig {
			in_peers,
			out_peers,
			reserved_nodes: HashSet::new(),
			reserved_only: false,
		},
		to_notifications.clone(),
		Arc::new(peer_store.handle()),
	);
	let peer_store_handle = peer_store.handle();
	tokio::spawn(controller.run());
	tokio::spawn(peer_store.run());

	let (notif_handle, command_stream) = protocol_handle_pair.split();

	let behaviour = Notifications::new(
		vec![controller_handle],
		from_controller,
		NotificationMetrics::new(None),
		iter::once((
			ProtocolConfig {
				name: "/foo".into(),
				fallback_names: Vec::new(),
				handshake: vec![1, 2, 3, 4],
				max_notification_size: 1024 * 1024,
			},
			notif_handle,
			command_stream,
		)),
	);
	let transport = crate::transport::build_transport(local_key.clone().into(), false);

	let mut swarm = {
		struct SpawnImpl {}
		impl libp2p::swarm::Executor for SpawnImpl {
			fn exec(&self, f: Pin<Box<dyn Future<Output = ()> + Send>>) {
				tokio::spawn(async move { f.await });
			}
		}

		let config = libp2p::swarm::Config::with_executor(SpawnImpl {})
			.with_substream_upgrade_protocol_override(upgrade::Version::V1)
			.with_notify_handler_buffer_size(NonZeroUsize::new(32).expect("32 != 0; qed"))
			// NOTE: 24 is somewhat arbitrary and should be tuned in the future if
			// necessary. See <https://github.com/paritytech/substrate/pull/6080>
			.with_per_connection_event_buffer_size(24)
			.with_max_negotiating_inbound_streams(2048)
			.with_idle_connection_timeout(Duration::from_secs(10));

		Swarm::new(transport.0, behaviour, local_peer_id, config)
	};

	swarm.listen_on("/ip6/::1/tcp/0".parse().unwrap()).unwrap();

	(swarm, peer_store_handle, notif_service)
}

async fn setup_litep2p() -> (Litep2p, NotificationHandle) {
	let (notif_config, handle) = NotificationConfig::new(
		Litep2pProtocolName::from("/foo"),
		1024 * 1024,
		vec![1, 2, 3, 4],
		Vec::new(),
		false,
		64,
		64,
		true,
	);

	let keypair = litep2p::crypto::ed25519::Keypair::generate();
	let config1 = Litep2pConfigBuilder::new()
		.with_keypair(keypair)
		.with_tcp(TcpConfig {
			listen_addresses: vec!["/ip6/::1/tcp/0".parse().unwrap()],
			..Default::default()
		})
		.with_notification_protocol(notif_config)
		.build();
	let litep2p = Litep2p::new(config1).unwrap();

	(litep2p, handle)
}
