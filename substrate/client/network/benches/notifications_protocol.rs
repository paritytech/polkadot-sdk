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

use assert_matches::assert_matches;
use criterion::{
	criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion, PlotConfiguration,
	Throughput,
};
use sc_network::{
	config::{
		FullNetworkConfiguration, MultiaddrWithPeerId, NetworkConfiguration, NonReservedPeerMode,
		NotificationHandshake, Params, ProtocolId, Role, SetConfig,
	},
	service::traits::NotificationEvent,
	Litep2pNetworkBackend, NetworkBackend, NetworkWorker, NotificationMetrics, NotificationService,
	PeerId, Roles,
};
use sc_network_common::{sync::message::BlockAnnouncesHandshake, ExHashT};
use sc_network_types::build_multiaddr;
use sp_core::H256;
use sp_runtime::traits::{Block as BlockT, Zero};
use std::{
	net::{IpAddr, Ipv4Addr, TcpListener},
	str::FromStr,
	sync::Arc,
};
use substrate_test_runtime_client::runtime;
use tokio::sync::Mutex;

const MAX_SIZE: u64 = 2u64.pow(30);
const SAMPLE_SIZE: usize = 50;
const NOTIFICATIONS: usize = 50;
const EXPONENTS: &[(u32, &'static str)] = &[
	(6, "64B"),
	(9, "512B"),
	(12, "4KB"),
	(15, "64KB"),
	(18, "256KB"),
	(21, "2MB"),
	(24, "16MB"),
	(27, "128MB"),
];

// TODO: It's be better to bind system-provided port when initializing the worker
fn get_listen_address() -> sc_network::Multiaddr {
	let ip = Ipv4Addr::from_str("127.0.0.1").unwrap();
	let listener = TcpListener::bind((IpAddr::V4(ip), 0)).unwrap(); // Bind to a random port
	let local_addr = listener.local_addr().unwrap();
	let port = local_addr.port();

	build_multiaddr!(Ip4(ip), Tcp(port))
}

fn create_network_worker<B, H, N>(
	listen_addr: sc_network::Multiaddr,
) -> (N, Box<dyn NotificationService>)
where
	B: BlockT<Hash = H256> + 'static,
	H: ExHashT,
	N: NetworkBackend<B, H>,
{
	let role = Role::Full;
	let mut net_conf = NetworkConfiguration::new_local();
	net_conf.listen_addresses = vec![listen_addr];
	let network_config = FullNetworkConfiguration::<B, H, N>::new(&net_conf, None);
	let genesis_hash = runtime::Hash::zero();
	let (block_announce_config, notification_service) = N::notification_config(
		"/block-announces/1".into(),
		vec!["/bench-notifications-protocol/block-announces/1".into()],
		MAX_SIZE,
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
		NotificationMetrics::new(None),
		network_config.peer_store_handle(),
	);
	let worker = N::new(Params::<B, H, N> {
		block_announce_config,
		role,
		executor: Box::new(|f| {
			tokio::spawn(f);
		}),
		genesis_hash,
		network_config,
		protocol_id: ProtocolId::from("bench-protocol-name"),
		fork_id: None,
		metrics_registry: None,
		bitswap_config: None,
		notification_metrics: NotificationMetrics::new(None),
	})
	.unwrap();

	(worker, notification_service)
}

struct BenchSetup {
	notification_service1: Arc<Mutex<Box<dyn NotificationService>>>,
	notification_service2: Arc<Mutex<Box<dyn NotificationService>>>,
	peer_id2: PeerId,
}

fn setup_workers<B, H, N>() -> Arc<BenchSetup>
where
	B: BlockT<Hash = H256> + 'static,
	H: ExHashT,
	N: NetworkBackend<B, H>,
{
	let listen_address1 = get_listen_address();
	let listen_address2 = get_listen_address();
	let (worker1, notification_service1) = create_network_worker::<B, H, N>(listen_address1);
	let (worker2, notification_service2) =
		create_network_worker::<B, H, N>(listen_address2.clone());
	let peer_id2: sc_network::PeerId = worker2.network_service().local_peer_id().into();

	worker1
		.network_service()
		.add_reserved_peer(MultiaddrWithPeerId { multiaddr: listen_address2, peer_id: peer_id2 })
		.unwrap();

	let notification_service1 = Arc::new(Mutex::new(notification_service1));
	let notification_service2 = Arc::new(Mutex::new(notification_service2));

	tokio::spawn(worker1.run());
	tokio::spawn(worker2.run());

	let ready = tokio::spawn({
		let notification_service1 = Arc::clone(&notification_service1);
		let notification_service2 = Arc::clone(&notification_service2);

		async move {
			let mut notification_service1 = notification_service1.lock().await;
			let mut notification_service2 = notification_service2.lock().await;
			loop {
				tokio::select! {
					Some(event) = notification_service1.next_event() => {
						if let NotificationEvent::NotificationStreamOpened { .. } = event {
							break;
						}
					},
					Some(event) = notification_service2.next_event() => {
						if let NotificationEvent::ValidateInboundSubstream { result_tx, .. } = event {
							result_tx.send(sc_network::service::traits::ValidationResult::Accept).unwrap();
						}
					},
				}
			}
		}
	});

	tokio::task::block_in_place(|| {
		let _ = tokio::runtime::Handle::current().block_on(ready);
	});

	Arc::new(BenchSetup { notification_service1, notification_service2, peer_id2 })
}

async fn run_serially(setup: Arc<BenchSetup>, size: usize, limit: usize) {
	let (tx, rx) = async_channel::bounded(1);
	let _ = tx.send(Some(())).await;
	let network1 = tokio::spawn({
		let notification_service1 = Arc::clone(&setup.notification_service1);
		let peer_id2 = setup.peer_id2;
		async move {
			let mut notification_service1 = notification_service1.lock().await;
			while let Ok(message) = rx.recv().await {
				let Some(_) = message else { break };
				notification_service1
					.send_async_notification(&peer_id2, vec![0; size])
					.await
					.unwrap();
			}
		}
	});
	let network2 = tokio::spawn({
		let notification_service2 = Arc::clone(&setup.notification_service2);
		async move {
			let mut notification_service2 = notification_service2.lock().await;
			let mut received_counter = 0;
			while let Some(event) = notification_service2.next_event().await {
				if let NotificationEvent::NotificationReceived { .. } = event {
					received_counter += 1;
					if received_counter >= limit {
						let _ = tx.send(None).await;
						break;
					}
					let _ = tx.send(Some(())).await;
				}
			}
		}
	});

	let _ = tokio::join!(network1, network2);
}

async fn run_with_backpressure(setup: Arc<BenchSetup>, size: usize, limit: usize) {
	let (tx, rx) = async_channel::bounded(1);
	let network1 = tokio::spawn({
		let setup = Arc::clone(&setup);
		async move {
			let mut notification_service1 = setup.notification_service1.lock().await;
			for _ in 0..limit {
				notification_service1
					.send_async_notification(&setup.peer_id2, vec![0; size])
					.await
					.unwrap();
			}
			let _ = rx.recv().await;
		}
	});
	let network2 = tokio::spawn({
		let setup = Arc::clone(&setup);
		async move {
			let mut notification_service2 = setup.notification_service2.lock().await;
			let mut received_counter = 0;
			while let Some(event) = notification_service2.next_event().await {
				if let NotificationEvent::NotificationReceived { .. } = event {
					received_counter += 1;
					if received_counter >= limit {
						let _ = tx.send(()).await;
						break;
					}
				}
			}
		}
	});

	let _ = tokio::join!(network1, network2);
}

fn run_benchmark(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
	let mut group = c.benchmark_group("notifications_benchmark");
	group.plot_config(plot_config);

	for &(exponent, label) in EXPONENTS.iter() {
		let size = 2usize.pow(exponent);
		group.throughput(Throughput::Bytes(NOTIFICATIONS as u64 * size as u64));

		group.bench_with_input(
			BenchmarkId::new("libp2p/serially", label),
			&(size, NOTIFICATIONS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter_batched_ref(
					setup_workers::<runtime::Block, runtime::Hash, NetworkWorker<_, _>>,
					|setup| run_serially(Arc::clone(setup), size, limit),
					criterion::BatchSize::SmallInput,
				);
			},
		);
		group.bench_with_input(
			BenchmarkId::new("litep2p/serially", label),
			&(size, NOTIFICATIONS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter_batched_ref(
					setup_workers::<runtime::Block, runtime::Hash, Litep2pNetworkBackend>,
					|setup| run_serially(Arc::clone(setup), size, limit),
					criterion::BatchSize::SmallInput,
				);
			},
		);
		group.bench_with_input(
			BenchmarkId::new("libp2p/with_backpressure", label),
			&(size, NOTIFICATIONS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter_batched_ref(
					setup_workers::<runtime::Block, runtime::Hash, NetworkWorker<_, _>>,
					|setup| run_with_backpressure(Arc::clone(setup), size, limit),
					criterion::BatchSize::SmallInput,
				);
			},
		);
		group.bench_with_input(
			BenchmarkId::new("litep2p/with_backpressure", label),
			&(size, NOTIFICATIONS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter_batched_ref(
					setup_workers::<runtime::Block, runtime::Hash, Litep2pNetworkBackend>,
					|setup| run_with_backpressure(Arc::clone(setup), size, limit),
					criterion::BatchSize::SmallInput,
				);
			},
		);
	}
}

criterion_group! {
	name = benches;
	config = Criterion::default().sample_size(SAMPLE_SIZE);
	targets = run_benchmark
}
criterion_main!(benches);
