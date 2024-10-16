use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use sc_network::{
	config::{
		FullNetworkConfiguration, MultiaddrWithPeerId, NetworkConfiguration, NonDefaultSetConfig,
		NonReservedPeerMode, NotificationHandshake, Params, ProtocolId, Role, SetConfig,
	},
	service::traits::NotificationEvent,
	NetworkWorker, NotificationMetrics, NotificationService, Roles,
};
use sc_network_common::sync::message::BlockAnnouncesHandshake;
use sp_runtime::traits::Zero;
use substrate_test_runtime_client::runtime;

pub fn create_network_worker(
) -> (NetworkWorker<runtime::Block, runtime::Hash>, Box<dyn NotificationService>) {
	let role = Role::Full;
	let genesis_hash = runtime::Hash::zero();
	let (block_announce_config, notification_service) = NonDefaultSetConfig::new(
		"/block-announces/1".into(),
		vec!["/bench-notifications-protocol/block-announces/1".into()],
		2u64.pow(MAX_EXPONENT),
		Some(NotificationHandshake::new(BlockAnnouncesHandshake::<runtime::Block>::build(
			Roles::from(&role),
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
		network_config: FullNetworkConfiguration::new(&NetworkConfiguration::new_local(), None),
		protocol_id: ProtocolId::from("bench-protocol-name"),
		fork_id: None,
		metrics_registry: None,
		bitswap_config: None,
		notification_metrics: NotificationMetrics::new(None),
	})
	.unwrap();

	(worker, notification_service)
}

async fn get_listen_address(
	worker: &mut NetworkWorker<runtime::Block, runtime::Hash>,
) -> sc_network::Multiaddr {
	loop {
		let _ = worker.next_action().await;
		let mut listen_addresses = worker.listen_addresses().cloned().collect::<Vec<_>>();
		if !listen_addresses.is_empty() {
			return listen_addresses.pop().unwrap().into();
		}
	}
}

async fn run_consistently(size: usize, limit: usize) {
	let mut received_counter = 0;
	let (worker1, mut notification_service1) = create_network_worker();
	let (mut worker2, mut notification_service2) = create_network_worker();
	let peer_id2: sc_network::PeerId = (*worker2.local_peer_id()).into();
	let listen_address2 = get_listen_address(&mut worker2).await;

	worker1
		.add_reserved_peer(MultiaddrWithPeerId { multiaddr: listen_address2, peer_id: peer_id2 })
		.unwrap();

	let network1_run = worker1.run();
	let network2_run = worker2.run();
	tokio::pin!(network1_run);
	tokio::pin!(network2_run);

	loop {
		tokio::select! {
			_ = &mut network1_run => {},
			_ = &mut network2_run => {},
			event = notification_service1.next_event() => {
				match event {
					Some(NotificationEvent::NotificationStreamOpened { .. }) => {
						notification_service1
							.send_async_notification(&peer_id2, vec![0; size])
							.await
							.unwrap();
					},
					event => panic!("Unexpected event {:?}", event),
				};
			},
			event = notification_service2.next_event() => {
				match event {
					Some(NotificationEvent::ValidateInboundSubstream { result_tx, .. }) => {
						result_tx.send(sc_network::service::traits::ValidationResult::Accept).unwrap();
					},
					Some(NotificationEvent::NotificationStreamOpened { .. }) => {},
					Some(NotificationEvent::NotificationReceived { .. }) => {
						received_counter += 1;
						if received_counter >= limit { break }
						notification_service1
							.send_async_notification(&peer_id2, vec![0; size])
							.await
							.unwrap();
					},
					event => panic!("Unexpected event {:?}", event),
				};
			},
		}
	}
}

async fn run_with_backpressure(size: usize, limit: usize) {
	let (worker1, mut notification_service1) = create_network_worker();
	let (mut worker2, mut notification_service2) = create_network_worker();
	let peer_id2: sc_network::PeerId = (*worker2.local_peer_id()).into();
	let listen_address2 = get_listen_address(&mut worker2).await;

	worker1
		.add_reserved_peer(MultiaddrWithPeerId { multiaddr: listen_address2, peer_id: peer_id2 })
		.unwrap();

	let network1_run = worker1.run();
	let network2_run = worker2.run();

	let network1 = tokio::spawn(async move {
		let mut sent_counter = 0;
		tokio::pin!(network1_run);
		loop {
			tokio::select! {
				_ = &mut network1_run => {},
				event = notification_service1.next_event() => {
					match event {
						Some(NotificationEvent::NotificationStreamOpened { .. }) => {
							while sent_counter < limit {
								sent_counter += 1;
								notification_service1
									.send_async_notification(&peer_id2, vec![0; size])
									.await
									.unwrap();
							}
						},
						Some(NotificationEvent::NotificationStreamClosed { .. }) => {
							if sent_counter != limit { panic!("Stream closed unexpectedly") }
							break
						},
						event => panic!("Unexpected event {:?}", event),
					};
				},
			}
		}
	});
	let network2 = tokio::spawn(async move {
		let mut received_counter = 0;
		tokio::pin!(network2_run);
		loop {
			tokio::select! {
				_ = &mut network2_run => {},
				event = notification_service2.next_event() => {
					match event {
						Some(NotificationEvent::ValidateInboundSubstream { result_tx, .. }) => {
							result_tx.send(sc_network::service::traits::ValidationResult::Accept).unwrap();
						},
						Some(NotificationEvent::NotificationStreamOpened { .. }) => {},
						Some(NotificationEvent::NotificationStreamClosed { .. }) => {
							if received_counter != limit { panic!("Stream closed unexpectedly") }
							break
						},
						Some(NotificationEvent::NotificationReceived { .. }) => {
							received_counter += 1;
							if received_counter >= limit { break }
						},
						event => panic!("Unexpected event {:?}", event),
					};
				},
			}
		}
	});

	let _ = tokio::join!(network1, network2);
}

const MAX_EXPONENT: u32 = 27;

fn run_benchmark(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let mut group = c.benchmark_group("notifications_benchmark");

	for exponent in 0..=MAX_EXPONENT {
		let size = 2usize.pow(exponent);
		let notifications = 2usize.pow((MAX_EXPONENT - exponent).max(10).min(20));
		group.throughput(Throughput::Bytes(notifications as u64 * size as u64));
		group.bench_with_input(
			format!("consistently/{}/{}", notifications, size),
			&(size, notifications),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| run_consistently(size, limit));
			},
		);
		group.bench_with_input(
			format!("backpressure/{}/{}", notifications, size),
			&(size, notifications),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| run_with_backpressure(size, limit));
			},
		);
	}
}

criterion_group!(benches, run_benchmark);
criterion_main!(benches);
