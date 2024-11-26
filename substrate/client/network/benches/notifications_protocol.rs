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
	Roles,
};
use sc_network_common::{sync::message::BlockAnnouncesHandshake, ExHashT};
use sc_network_types::build_multiaddr;
use sp_core::H256;
use sp_runtime::traits::{Block as BlockT, Zero};
use std::{
	net::{IpAddr, Ipv4Addr, TcpListener},
	str::FromStr,
};
use substrate_test_runtime_client::runtime;

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

async fn run_serially<B, H, N>(size: usize, limit: usize)
where
	B: BlockT<Hash = H256> + 'static,
	H: ExHashT,
	N: NetworkBackend<B, H>,
{
	let listen_address1 = get_listen_address();
	let listen_address2 = get_listen_address();
	let (worker1, mut notification_service1) = create_network_worker::<B, H, N>(listen_address1);
	let (worker2, mut notification_service2) =
		create_network_worker::<B, H, N>(listen_address2.clone());
	let peer_id2: sc_network::PeerId = worker2.network_service().local_peer_id().into();

	worker1
		.network_service()
		.add_reserved_peer(MultiaddrWithPeerId { multiaddr: listen_address2, peer_id: peer_id2 })
		.unwrap();

	let network1_run = worker1.run();
	let network2_run = worker2.run();
	let (tx, rx) = async_channel::bounded(10);

	let network1 = tokio::spawn(async move {
		let mut sent_counter = 0;
		tokio::pin!(network1_run);
		loop {
			tokio::select! {
				_ = &mut network1_run => {},
				event = notification_service1.next_event() => {
					match event {
						Some(NotificationEvent::NotificationStreamOpened { .. }) => {
							sent_counter += 1;
							notification_service1
								.send_async_notification(&peer_id2, vec![0; size])
								.await
								.unwrap();
						},
						Some(NotificationEvent::NotificationStreamClosed { .. }) => {
							if sent_counter >= limit {
								break;
							}
							panic!("Unexpected stream closure {:?}", event);
						}
						event => panic!("Unexpected event {:?}", event),
					};
				},
				message = rx.recv() => {
					match message {
						Ok(Some(_)) => {
							sent_counter += 1;
							notification_service1
								.send_async_notification(&peer_id2, vec![0; size])
								.await
								.unwrap();
						},
						Ok(None) => break,
						Err(err) => panic!("Unexpected error {:?}", err),

					}
				}
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
						Some(NotificationEvent::NotificationReceived { .. }) => {
							received_counter += 1;
							if received_counter >= limit {
								let _ = tx.send(None).await;
								break
							}
							let _ = tx.send(Some(())).await;
						},
						event => panic!("Unexpected event {:?}", event),
					};
				},
			}
		}
	});

	let _ = tokio::join!(network1, network2);
}

async fn run_with_backpressure<B, H, N>(size: usize, limit: usize)
where
	B: BlockT<Hash = H256> + 'static,
	H: ExHashT,
	N: NetworkBackend<B, H>,
{
	let listen_address1 = get_listen_address();
	let listen_address2 = get_listen_address();
	let (worker1, mut notification_service1) = create_network_worker::<B, H, N>(listen_address1);
	let (worker2, mut notification_service2) =
		create_network_worker::<B, H, N>(listen_address2.clone());
	let peer_id2: sc_network::PeerId = worker2.network_service().local_peer_id().into();

	worker1
		.network_service()
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
				b.to_async(&rt).iter(|| {
					run_serially::<runtime::Block, runtime::Hash, NetworkWorker<_, _>>(size, limit)
				});
			},
		);
		group.bench_with_input(
			BenchmarkId::new("litep2p/serially", label),
			&(size, NOTIFICATIONS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| {
					run_serially::<runtime::Block, runtime::Hash, Litep2pNetworkBackend>(
						size, limit,
					)
				});
			},
		);
		group.bench_with_input(
			BenchmarkId::new("libp2p/with_backpressure", label),
			&(size, NOTIFICATIONS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| {
					run_with_backpressure::<runtime::Block, runtime::Hash, NetworkWorker<_, _>>(
						size, limit,
					)
				});
			},
		);
		group.bench_with_input(
			BenchmarkId::new("litep2p/with_backpressure", label),
			&(size, NOTIFICATIONS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| {
					run_with_backpressure::<runtime::Block, runtime::Hash, Litep2pNetworkBackend>(
						size, limit,
					)
				});
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
