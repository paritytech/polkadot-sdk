use criterion::{
	criterion_group, criterion_main, AxisScale, BenchmarkId, Criterion, PlotConfiguration,
	Throughput,
};
use sc_network::{
	config::{
		FullNetworkConfiguration, IncomingRequest, NetworkConfiguration, NonDefaultSetConfig,
		NonReservedPeerMode, NotificationHandshake, OutgoingResponse, Params, ProtocolId, Role,
		SetConfig,
	},
	IfDisconnected, NetworkBackend, NetworkRequest, NetworkWorker, NotificationMetrics,
	NotificationService, Roles,
};
use sc_network_common::sync::message::BlockAnnouncesHandshake;
use sc_network_types::build_multiaddr;
use sp_runtime::traits::Zero;
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

pub fn create_network_worker(
	listen_addr: sc_network::Multiaddr,
) -> (
	NetworkWorker<runtime::Block, runtime::Hash>,
	async_channel::Receiver<IncomingRequest>,
	Box<dyn NotificationService>,
) {
	let (tx, rx) = async_channel::bounded(10);
	let request_response_config =
		NetworkWorker::<runtime::Block, runtime::Hash>::request_response_config(
			"/request-response/1".into(),
			vec![],
			MAX_SIZE,
			MAX_SIZE,
			Duration::from_secs(2),
			Some(tx),
		);
	let mut net_conf = NetworkConfiguration::new_local();
	net_conf.listen_addresses = vec![listen_addr];
	let mut network_config = FullNetworkConfiguration::new(&net_conf, None);
	network_config.add_request_response_protocol(request_response_config);
	let (block_announce_config, notification_service) = NonDefaultSetConfig::new(
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
	let worker = NetworkWorker::<runtime::Block, runtime::Hash>::new(Params::<
		runtime::Block,
		runtime::Hash,
		NetworkWorker<_, _>,
	> {
		block_announce_config,
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

	(worker, rx, notification_service)
}

async fn run_consistently(size: usize, limit: usize) {
	let mut received_counter = 0;
	let listen_address1 = get_listen_address();
	let listen_address2 = get_listen_address();
	let (mut worker1, _rx1, _notification_service1) = create_network_worker(listen_address1);
	let service1 = worker1.service().clone();
	let (worker2, rx2, _notification_service2) = create_network_worker(listen_address2.clone());
	let peer_id2 = *worker2.local_peer_id();

	worker1.add_known_address(peer_id2, listen_address2.into());

	let requests = async move {
		while received_counter < limit {
			received_counter += 1;
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
		}
	};
	let network1_run = worker1.run();
	let network2_run = worker2.run();
	tokio::pin!(requests);
	tokio::pin!(network1_run);
	tokio::pin!(network2_run);

	loop {
		tokio::select! {
			_ = &mut network1_run => {},
			_ = &mut network2_run => {},
			_ = &mut requests => break,
			res = rx2.recv() => {
				let IncomingRequest { pending_response, .. } = res.unwrap();
				pending_response.send(OutgoingResponse {
					result: Ok(vec![0; size]),
					reputation_changes: vec![],
					sent_feedback: None,
				}).unwrap();
			}
		}
	}
}

async fn run_with_backpressure(size: usize, limit: usize) {
	let listen_address1 = get_listen_address();
	let listen_address2 = get_listen_address();
	let (mut worker1, _rx1, _notification_service1) = create_network_worker(listen_address1);
	let service1 = worker1.service().clone();
	let (worker2, rx2, _notification_service2) = create_network_worker(listen_address2.clone());
	let peer_id2 = *worker2.local_peer_id();

	worker1.add_known_address(peer_id2, listen_address2.into());

	let requests = (0..limit).into_iter().map(|_| {
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
	});

	let requests = futures::future::join_all(requests);
	let network1_run = worker1.run();
	let network2_run = worker2.run();
	tokio::pin!(requests);
	tokio::pin!(network1_run);
	tokio::pin!(network2_run);

	loop {
		tokio::select! {
			_ = &mut network1_run => {},
			_ = &mut network2_run => {},
			responses = &mut requests => {
				for res in responses {
					res.unwrap().unwrap();
				}
				break;
			},
			res = rx2.recv() => {
				let IncomingRequest { pending_response, .. } = res.unwrap();
				pending_response.send(OutgoingResponse {
					result: Ok(vec![0; size]),
					reputation_changes: vec![],
					sent_feedback: None,
				}).unwrap();
			},
		}
	}
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
			BenchmarkId::new("consistently", label),
			&(size, REQUESTS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| run_consistently(size, limit));
			},
		);
		group.bench_with_input(
			BenchmarkId::new("with_backpressure", label),
			&(size, REQUESTS),
			|b, &(size, limit)| {
				b.to_async(&rt).iter(|| run_with_backpressure(size, limit));
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
