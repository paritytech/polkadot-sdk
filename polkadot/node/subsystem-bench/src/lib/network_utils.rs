// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use crate::environment::{TestEnvironmentDependencies, GENESIS_HASH};
use polkadot_node_network_protocol::{
	authority_discovery::AuthorityDiscovery,
	peer_set::{peer_sets_info, IsAuthority, PeerSet, PeerSetProtocolNames},
};
use polkadot_primitives::AuthorityDiscoveryId;
use polkadot_service::runtime_traits::Zero;
use prometheus::Registry;
use sc_network::{
	config::{
		FullNetworkConfiguration, NetworkConfiguration, NodeKeyConfig, NonReservedPeerMode, Params,
		ProtocolId, Role, SetConfig, TransportConfig,
	},
	service::traits::NetworkService,
	Multiaddr, NetworkBackend, NotificationConfig, NotificationMetrics, NotificationService,
};
use sc_network_common::sync::message::BlockAnnouncesHandshake;
use sc_network_types::PeerId;
use sp_consensus::SyncOracle;
use sp_core::H256;
use sp_runtime::traits::Block as BlockT;
use std::{
	collections::{HashMap, HashSet},
	sync::Arc,
};

#[derive(Clone)]
pub struct DummySyncOracle;

impl SyncOracle for DummySyncOracle {
	fn is_major_syncing(&self) -> bool {
		false
	}

	fn is_offline(&self) -> bool {
		false
	}
}

#[derive(Clone, Debug)]
pub struct DummyAuthotiryDiscoveryService {
	pub(crate) by_peer_id: HashMap<PeerId, HashSet<AuthorityDiscoveryId>>,
	pub(crate) by_authority_id: HashMap<AuthorityDiscoveryId, HashSet<sc_network::Multiaddr>>,
}

impl DummyAuthotiryDiscoveryService {
	pub fn new(
		by_peer_id: HashMap<PeerId, AuthorityDiscoveryId>,
		by_authority_id: HashMap<AuthorityDiscoveryId, Multiaddr>,
	) -> Self {
		Self {
			by_peer_id: by_peer_id
				.into_iter()
				.map(|(peer_id, authority_id)| (peer_id, HashSet::from([authority_id])))
				.collect(),
			by_authority_id: by_authority_id
				.into_iter()
				.map(|(authority_id, multiaddr)| (authority_id, HashSet::from([multiaddr])))
				.collect(),
		}
	}
}

#[async_trait::async_trait]
impl AuthorityDiscovery for DummyAuthotiryDiscoveryService {
	async fn get_addresses_by_authority_id(
		&mut self,
		authority: AuthorityDiscoveryId,
	) -> Option<HashSet<sc_network::Multiaddr>> {
		self.by_authority_id.get(&authority).cloned()
	}

	async fn get_authority_ids_by_peer_id(
		&mut self,
		peer_id: PeerId,
	) -> Option<HashSet<AuthorityDiscoveryId>> {
		self.by_peer_id.get(&peer_id).cloned()
	}
}

pub(crate) fn build_network_config<B, N>(
	node_key: NodeKeyConfig,
	listen_addr: Multiaddr,
	metrics_registry: Option<Registry>,
) -> FullNetworkConfiguration<B, B::Hash, N>
where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let mut net_conf = NetworkConfiguration::new_local();
	net_conf.transport = TransportConfig::MemoryOnly;
	net_conf.listen_addresses = vec![listen_addr];
	net_conf.node_key = node_key;

	FullNetworkConfiguration::<B, B::Hash, N>::new(&net_conf, metrics_registry)
}

pub(crate) fn build_network_worker<B, N>(
	dependencies: &TestEnvironmentDependencies,
	network_config: FullNetworkConfiguration<B, B::Hash, N>,
	role: Role,
	protocol_id: ProtocolId,
	notification_metrics: NotificationMetrics,
	metrics_registry: Option<Registry>,
	task_group: &str,
) -> (N, Arc<dyn NetworkService>, Box<dyn NotificationService>)
where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let peer_store_handle = network_config.peer_store_handle();
	let block_announce_protocol =
		format!("/{}/block-announces/1", array_bytes::bytes2hex("", GENESIS_HASH.as_ref()));
	let (block_announce_config, block_announce_service) = N::notification_config(
		block_announce_protocol.into(),
		std::iter::once(format!("/{}/block-announces/1", protocol_id.as_ref()).into()).collect(),
		1024 * 1024,
		Some(sc_network::config::NotificationHandshake::new(BlockAnnouncesHandshake::<B>::build(
			sc_network::Roles::from(&role),
			Zero::zero(),
			GENESIS_HASH,
			GENESIS_HASH,
		))),
		// NOTE: `set_config` will be ignored by `protocol.rs` as the block announcement
		// protocol is still hardcoded into the peerset.
		SetConfig {
			in_peers: 0,
			out_peers: 0,
			reserved_nodes: vec![],
			non_reserved_mode: NonReservedPeerMode::Deny,
		},
		notification_metrics.clone(),
		Arc::clone(&peer_store_handle),
	);
	let spawn_handle = dependencies.task_manager.spawn_handle();
	let task_group: &'static str = Box::leak(task_group.to_string().into_boxed_str());
	let worker = N::new(Params::<B, B::Hash, N> {
		block_announce_config,
		role,
		executor: {
			let handle = Clone::clone(&spawn_handle);
			Box::new(move |fut| {
				handle.spawn("libp2p-node", Some(task_group), fut);
			})
		},
		genesis_hash: GENESIS_HASH,
		network_config,
		protocol_id,
		fork_id: None,
		metrics_registry,
		bitswap_config: None,
		notification_metrics,
	})
	.unwrap();
	let network_service = worker.network_service();

	(worker, network_service, block_announce_service)
}

pub(crate) fn build_peer_set_services<B, N>(
	network_config: &mut FullNetworkConfiguration<B, B::Hash, N>,
	notification_metrics: &NotificationMetrics,
) -> (PeerSetProtocolNames, HashMap<PeerSet, Box<dyn NotificationService>>)
where
	B: BlockT<Hash = H256> + 'static,
	N: NetworkBackend<B, B::Hash>,
{
	let peer_set_protocol_names = PeerSetProtocolNames::new(GENESIS_HASH, None);
	let peer_store_handle = network_config.peer_store_handle();
	let peer_set_services = peer_sets_info::<B, N>(
		IsAuthority::Yes,
		&peer_set_protocol_names,
		notification_metrics.clone(),
		Arc::clone(&peer_store_handle),
	)
	.into_iter()
	.map(|(mut config, (peerset, service))| {
		if matches!(peerset, PeerSet::Validation) {
			config.allow_non_reserved(10_000, 10_000);
		}
		network_config.add_notification_protocol(config);
		(peerset, service)
	})
	.collect::<HashMap<PeerSet, Box<dyn NotificationService>>>();

	(peer_set_protocol_names, peer_set_services)
}
