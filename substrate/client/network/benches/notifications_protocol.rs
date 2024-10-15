use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion};
use sc_network::{
	config::{
		FullNetworkConfiguration, NetworkConfiguration, NonDefaultSetConfig, NonReservedPeerMode,
		NotificationHandshake, Params, ProtocolId, Role, SetConfig, TransportConfig,
	},
	Multiaddr, NetworkPeers, NetworkRequest, NetworkStateInfo, NetworkWorker, NotificationMetrics,
	NotificationService, Roles,
};
use sc_network_common::sync::message::BlockAnnouncesHandshake;
use sc_network_types::build_multiaddr;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Zero;
use std::sync::Arc;
use substrate_test_runtime_client::{runtime, TestClientBuilder, TestClientBuilderExt};

pub fn create_network_worker(
) -> (NetworkWorker<runtime::Block, runtime::Hash>, Box<dyn NotificationService>) {
	let protocol_id = ProtocolId::from("bench-protocol-name");
	let role = Role::Full;
	let network_config = NetworkConfiguration::new_local();

	let mut full_net_config = FullNetworkConfiguration::new(&network_config, None);
	let (under_bench_config, under_bench_service) = NonDefaultSetConfig::new(
		String::from("under-benchmarking").into(),
		vec![],
		1024 * 1024 * 1024,
		None,
		SetConfig {
			in_peers: 1,
			out_peers: 1,
			reserved_nodes: Vec::new(),
			non_reserved_mode: NonReservedPeerMode::Accept,
		},
	);
	full_net_config.add_notification_protocol(under_bench_config);

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
	let worker = NetworkWorker::<runtime::Block, runtime::Hash>::new(Params::<
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
	.unwrap();

	(worker, under_bench_service)
}

async fn run() {
	let (mut worker1, protocol_service1) = create_network_worker();
	let (mut worker2, protocol_service2) = create_network_worker();

	let worker1_peer_id = *worker1.local_peer_id();
	let worker2_peer_id = *worker2.local_peer_id();

	let listen_addr1 = loop {
		let _ = worker1.next_action().await;
		let mut listen_addresses1 = worker1.listen_addresses().cloned().collect::<Vec<_>>();
		if !listen_addresses1.is_empty() {
			break listen_addresses1.pop().unwrap();
		}
	};
	let listen_addr2 = loop {
		let _ = worker2.next_action().await;
		let mut listen_addresses1 = worker2.listen_addresses().cloned().collect::<Vec<_>>();
		if !listen_addresses1.is_empty() {
			break listen_addresses1.pop().unwrap();
		}
	};

	let worker_service1 = worker1.service();
	let worker_service2 = worker2.service();

	worker_service1.add_known_address(worker2_peer_id.into(), listen_addr2.into());
	worker_service2.add_known_address(worker1_peer_id.into(), listen_addr1.into());

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
