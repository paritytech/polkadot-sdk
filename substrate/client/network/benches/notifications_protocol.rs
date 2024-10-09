use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sc_network::{
	config::{
		FullNetworkConfiguration, NetworkConfiguration, NonDefaultSetConfig, NonReservedPeerMode,
		NotificationHandshake, Params, ProtocolId, Role, SetConfig, TransportConfig,
	},
	Multiaddr, NetworkPeers, NetworkRequest, NetworkStateInfo, NetworkWorker, NotificationMetrics,
	Roles,
};
use sc_network_common::sync::message::BlockAnnouncesHandshake;
use sc_network_types::build_multiaddr;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Zero;
use std::sync::Arc;
use substrate_test_runtime_client::{runtime, TestClientBuilder, TestClientBuilderExt};

pub fn create_network_worker() -> NetworkWorker<runtime::Block, runtime::Hash> {
	let protocol_id = ProtocolId::from("bench-protocol-name");
	let role = Role::Full;
	let network_config = NetworkConfiguration::new_local();
	let full_net_config = FullNetworkConfiguration::new(&network_config, None);
	let client = Arc::new(TestClientBuilder::with_default_backend().build_with_longest_chain().0);
	let genesis_hash = client.hash(Zero::zero()).ok().flatten().expect("Genesis block exists; qed");
	let (block_announce_config, _notification_service) = NonDefaultSetConfig::new(
		format!("/{}/block-announces/1", array_bytes::bytes2hex("", genesis_hash.as_ref())).into(),
		std::iter::once(format!("/{}/block-announces/1", protocol_id.as_ref()).into()).collect(),
		1024 * 1024,
		Some(NotificationHandshake::new(BlockAnnouncesHandshake::<runtime::Block>::build(
			Roles::from(&role),
			Zero::zero(),
			genesis_hash,
			genesis_hash,
		))),
		SetConfig {
			in_peers: 1,
			out_peers: 1,
			reserved_nodes: Vec::new(),
			non_reserved_mode: NonReservedPeerMode::Deny,
		},
	);
	NetworkWorker::<runtime::Block, runtime::Hash>::new(Params::<
		runtime::Block,
		runtime::Hash,
		NetworkWorker<_, _>,
	> {
		block_announce_config,
		role,
		executor: Box::new(|f| {
			tokio::spawn(f);
		}),
		genesis_hash,
		network_config: full_net_config,
		protocol_id,
		fork_id: None,
		metrics_registry: None,
		bitswap_config: None,
		notification_metrics: NotificationMetrics::new(None),
	})
	.unwrap()
}

async fn run() {
	let mut worker1 = create_network_worker();
	let mut worker2 = create_network_worker();

	let worker1_peer_id = *worker1.local_peer_id();
	let worker2_peer_id = *worker2.local_peer_id();

	let listen_addr1 = loop {
		let _ = worker1.next_action().await;
		let listen_addresses1 = worker1.listen_addresses().collect::<Vec<_>>();
		if !listen_addresses1.is_empty() {
			println!("{} {:?}", count, listen_addresses1);
			break listen_addresses1.pop().unwrap();
		}
	};
	let listen_addr2 = loop {
		let _ = worker2.next_action().await;
		let listen_addresses1 = worker2.listen_addresses().collect::<Vec<_>>();
		if !listen_addresses1.is_empty() {
			println!("{} {:?}", count, listen_addresses1);
			break listen_addresses1.pop().unwrap();
		}
	};

	let service1 = worker1.service();
	let service2 = worker2.service();

	service1.add_known_address(worker2_peer_id, listen_addr2);
	service2.add_known_address(worker1_peer_id, listen_addr1);

	tokio::join! {
		worker1.run(),
		worker2.run(),
	};
}

fn run_benchmark(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();

	c.bench_with_input(BenchmarkId::new("notifications_benchmark", ""), &(), |b, _| {
		b.to_async(&rt).iter(|| run());
	});
}

criterion_group!(benches, run_benchmark);
criterion_main!(benches);
