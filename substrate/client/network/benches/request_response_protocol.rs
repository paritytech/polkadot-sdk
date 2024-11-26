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
		FullNetworkConfiguration, IncomingRequest, NetworkConfiguration, NonReservedPeerMode,
		NotificationHandshake, OutgoingResponse, Params, ProtocolId, Role, SetConfig,
	},
	IfDisconnected, Litep2pNetworkBackend, NetworkBackend, NetworkRequest, NetworkWorker,
	NotificationMetrics, NotificationService, Roles,
};
use sc_network_common::{sync::message::BlockAnnouncesHandshake, ExHashT};
use sc_network_types::build_multiaddr;
use sp_core::H256;
use sp_runtime::traits::{Block as BlockT, Zero};
use std::{
	net::{IpAddr, Ipv4Addr, TcpListener},
	str::FromStr,
	time::Duration,
};
use substrate_test_runtime_client::runtime;

const MAX_SIZE: u64 = 2u64.pow(30);
const SAMPLE_SIZE: usize = 50;
const REQUESTS: usize = 50;
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

fn get_listen_address() -> sc_network::Multiaddr {
	let ip = Ipv4Addr::from_str("127.0.0.1").unwrap();
	let listener = TcpListener::bind((IpAddr::V4(ip), 0)).unwrap(); // Bind to a random port
	let local_addr = listener.local_addr().unwrap();
	let port = local_addr.port();

	build_multiaddr!(Ip4(ip), Tcp(port))
}

pub fn create_network_worker<B, H, N>(
	listen_addr: sc_network::Multiaddr,
) -> (N, async_channel::Receiver<IncomingRequest>, Box<dyn NotificationService>)
where
	B: BlockT<Hash = H256> + 'static,
	H: ExHashT,
	N: NetworkBackend<B, H>,
{
	let (tx, rx) = async_channel::bounded(10);
	let request_response_config = N::request_response_config(
		"/request-response/1".into(),
		vec![],
		MAX_SIZE,
		MAX_SIZE,
		Duration::from_secs(2),
		Some(tx),
	);
	let role = Role::Full;
	let mut net_conf = NetworkConfiguration::new_local();
	net_conf.listen_addresses = vec![listen_addr];
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
	let worker = N::new(Params::<B, H, N> {
		block_announce_config,
		role,
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

	(worker, rx, notification_service)
}

async fn run_serially<B, H, N>(size: usize, limit: usize)
where
	B: BlockT<Hash = H256> + 'static,
	H: ExHashT,
	N: NetworkBackend<B, H>,
{
	let listen_address1 = get_listen_address();
	let listen_address2 = get_listen_address();
	let (worker1, _rx1, _notification_service1) = create_network_worker::<B, H, N>(listen_address1);
	let service1 = worker1.network_service().clone();
	let (worker2, rx2, _notification_service2) =
		create_network_worker::<B, H, N>(listen_address2.clone());
	let peer_id2 = worker2.network_service().local_peer_id();

	worker1.network_service().add_known_address(peer_id2, listen_address2.into());

	let network1_run = worker1.run();
	let network2_run = worker2.run();
	let (break_tx, break_rx) = async_channel::bounded(10);
	let requests = async move {
		let mut sent_counter = 0;
		while sent_counter < limit {
			let _ = service1
				.request(
					peer_id2.into(),
					"/request-response/1".into(),
					vec![0; 2],
					None,
					IfDisconnected::TryConnect,
				)
				.await
				.unwrap();
			sent_counter += 1;
		}
		let _ = break_tx.send(()).await;
	};

	let network1 = tokio::spawn(async move {
		tokio::pin!(requests);
		tokio::pin!(network1_run);
		loop {
			tokio::select! {
				_ = &mut network1_run => {},
				_ = &mut requests => break,
			}
		}
	});
	let network2 = tokio::spawn(async move {
		tokio::pin!(network2_run);
		loop {
			tokio::select! {
				_ = &mut network2_run => {},
				res = rx2.recv() => {
					let IncomingRequest { pending_response, .. } = res.unwrap();
					pending_response.send(OutgoingResponse {
						result: Ok(vec![0; size]),
						reputation_changes: vec![],
						sent_feedback: None,
					}).unwrap();
				},
				_ = break_rx.recv() => break,
			}
		}
	});

	let _ = tokio::join!(network1, network2);
}

// The libp2p request-response implementation does not provide any backpressure feedback.
// So this benchmark is useless until we implement it for litep2p.
#[allow(dead_code)]
async fn run_with_backpressure<B, H, N>(size: usize, limit: usize)
where
	B: BlockT<Hash = H256> + 'static,
	H: ExHashT,
	N: NetworkBackend<B, H>,
{
	let listen_address1 = get_listen_address();
	let listen_address2 = get_listen_address();
	let (worker1, _rx1, _notification_service1) = create_network_worker::<B, H, N>(listen_address1);
	let service1 = worker1.network_service().clone();
	let (worker2, rx2, _notification_service2) =
		create_network_worker::<B, H, N>(listen_address2.clone());
	let peer_id2 = worker2.network_service().local_peer_id();

	worker1.network_service().add_known_address(peer_id2, listen_address2.into());

	let network1_run = worker1.run();
	let network2_run = worker2.run();
	let (break_tx, break_rx) = async_channel::bounded(10);
	let requests = futures::future::join_all((0..limit).into_iter().map(|_| {
		let (tx, rx) = futures::channel::oneshot::channel();
		service1.start_request(
			peer_id2.into(),
			"/request-response/1".into(),
			vec![0; 8],
			None,
			tx,
			IfDisconnected::TryConnect,
		);
		rx
	}));

	let network1 = tokio::spawn(async move {
		tokio::pin!(requests);
		tokio::pin!(network1_run);
		loop {
			tokio::select! {
				_ = &mut network1_run => {},
				responses = &mut requests => {
					for res in responses {
						res.unwrap().unwrap();
					}
					let _ = break_tx.send(()).await;
					break;
				},
			}
		}
	});
	let network2 = tokio::spawn(async move {
		tokio::pin!(network2_run);
		loop {
			tokio::select! {
				_ = &mut network2_run => {},
				res = rx2.recv() => {
					let IncomingRequest { pending_response, .. } = res.unwrap();
					pending_response.send(OutgoingResponse {
						result: Ok(vec![0; size]),
						reputation_changes: vec![],
						sent_feedback: None,
					}).unwrap();
				},
				_ = break_rx.recv() => break,
			}
		}
	});

	let _ = tokio::join!(network1, network2);
}

fn run_benchmark(c: &mut Criterion) {
	let rt = tokio::runtime::Runtime::new().unwrap();
	let plot_config = PlotConfiguration::default().summary_scale(AxisScale::Logarithmic);
	let mut group = c.benchmark_group("request_response_benchmark");
	group.plot_config(plot_config);

	for &(exponent, label) in EXPONENTS.iter() {
		let size = 2usize.pow(exponent);
		group.throughput(Throughput::Bytes(REQUESTS as u64 * size as u64));
		group.bench_with_input(
			BenchmarkId::new("libp2p/serially", label),
			&(size, REQUESTS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| {
					run_serially::<runtime::Block, runtime::Hash, NetworkWorker<_, _>>(size, limit)
				});
			},
		);
		group.bench_with_input(
			BenchmarkId::new("litep2p/serially", label),
			&(size, REQUESTS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| {
					run_serially::<runtime::Block, runtime::Hash, Litep2pNetworkBackend>(
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
