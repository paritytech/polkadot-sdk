use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use sc_network::{
	config::{
		FullNetworkConfiguration, IncomingRequest, MultiaddrWithPeerId, NetworkConfiguration,
		NonDefaultSetConfig, NonReservedPeerMode, NotificationHandshake, Params, ProtocolId, Role,
		SetConfig,
	},
	service::traits::NotificationEvent,
	IfDisconnected, NetworkBackend, NetworkRequest, NetworkWorker, NotificationMetrics,
	NotificationService, Roles, MAX_RESPONSE_SIZE,
};
use sc_network_common::sync::message::BlockAnnouncesHandshake;
use sc_network_sync::service::network::Network;
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Zero;
use std::{sync::Arc, time::Duration};
use substrate_test_runtime_client::{runtime, TestClientBuilder, TestClientBuilderExt};

pub fn dummy_block_announce_config() -> NonDefaultSetConfig {
	let (block_announce_config, _notification_service) = NonDefaultSetConfig::new(
		"/block-announces/1".into(),
		vec![],
		1024,
		Some(NotificationHandshake::new(BlockAnnouncesHandshake::<runtime::Block>::build(
			Roles::from(&Role::Full),
			Zero::zero(),
			runtime::Hash::zero(),
			runtime::Hash::zero(),
		))),
		SetConfig {
			in_peers: 1,
			out_peers: 1,
			reserved_nodes: vec![],
			non_reserved_mode: NonReservedPeerMode::Accept,
		},
	);
	block_announce_config
}

pub fn create_network_worker(
) -> (NetworkWorker<runtime::Block, runtime::Hash>, async_channel::Receiver<IncomingRequest>) {
	let (tx, rx) = async_channel::bounded(10);
	let request_response_config =
		NetworkWorker::<runtime::Block, runtime::Hash>::request_response_config(
			"request_response".into(),
			vec![],
			MAX_RESPONSE_SIZE,
			MAX_RESPONSE_SIZE,
			Duration::from_secs(2),
			Some(tx),
		);
	let mut network_config =
		FullNetworkConfiguration::new(&NetworkConfiguration::new_local(), None);
	network_config.add_request_response_protocol(request_response_config);
	let worker = NetworkWorker::<runtime::Block, runtime::Hash>::new(Params::<
		runtime::Block,
		runtime::Hash,
		NetworkWorker<_, _>,
	> {
		block_announce_config: dummy_block_announce_config(),
		role: Role::Full,
		executor: Box::new(|f| {
			tokio::spawn(f);
		}),
		genesis_hash: runtime::Hash::zero(),
		network_config,
		protocol_id: ProtocolId::from("bench-request-response-protocol"),
		fork_id: None,
		metrics_registry: None,
		bitswap_config: None,
		notification_metrics: NotificationMetrics::new(None),
	})
	.unwrap();

	(worker, rx)
}

async fn get_listen_address(
	worker: &mut NetworkWorker<runtime::Block, runtime::Hash>,
) -> sc_network::types::Multiaddr {
	loop {
		let _ = worker.next_action().await;
		let mut listen_addresses = worker.listen_addresses().cloned().collect::<Vec<_>>();
		if !listen_addresses.is_empty() {
			return listen_addresses.pop().unwrap();
		}
	}
}

async fn run(size: usize, limit: usize) {
	let mut received_counter = 0;
	let (mut worker1, rx1) = create_network_worker();
	let service1 = worker1.service().clone();
	let (mut worker2, rx2) = create_network_worker();
	let peer_id2 = *worker2.local_peer_id();
	let listen_address2 = get_listen_address(&mut worker2).await;

	worker1.add_known_address(peer_id2, listen_address2);

	tokio::spawn(worker1.run());
	tokio::spawn(worker2.run());
	let (tx, rx) = futures::channel::oneshot::channel();

	service1.start_request(
		peer_id2.into(),
		"request_response".into(),
		vec![0; 8],
		None,
		tx,
		IfDisconnected::TryConnect,
	);

	let xxx = rx.await;
	println!("xxx {:?}", xxx);

	loop {
		tokio::select! {
			// res = rx => {
			// 	println!("req {:?}", res);
			// },
			x1 = rx1.recv() => {
				println!("1 {:?}", x1);
			},
			x2 = rx2.recv() => {
				println!("2 {:?}", x2);
			}
		}
	}
}

fn run_benchmark(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("request_response_benchmark");

	for exponent in 1..4 {
		let notifications = 10usize;
		let size = 2usize.pow(exponent);
		group.throughput(Throughput::Bytes(notifications as u64 * size as u64));
		group.bench_with_input(
			format!("{}/{}", notifications, size),
			&(size, notifications),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| run(size, limit));
			},
		);
	}
}

criterion_group!(benches, run_benchmark);
criterion_main!(benches);
