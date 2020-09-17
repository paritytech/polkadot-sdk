// This file is part of Substrate.

// Copyright (C) 2017-2020 Parity Technologies (UK) Ltd.
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

use crate::worker::schema;

use std::{iter::FromIterator, sync::{Arc, Mutex}};

use futures::channel::mpsc::channel;
use futures::executor::{block_on, LocalPool};
use futures::future::{poll_fn, FutureExt};
use futures::sink::SinkExt;
use futures::task::LocalSpawn;
use futures::poll;
use libp2p::{kad, core::multiaddr, PeerId};

use sp_api::{ProvideRuntimeApi, ApiRef};
use sp_core::{crypto::Public, testing::KeyStore};
use sp_runtime::traits::{Zero, Block as BlockT, NumberFor};
use substrate_test_runtime_client::runtime::Block;

use super::*;

#[test]
fn interval_at_with_start_now() {
	let start = Instant::now();

	let mut interval = interval_at(
		std::time::Instant::now(),
		std::time::Duration::from_secs(10),
	);

	futures::executor::block_on(async {
		interval.next().await;
	});

	assert!(
		Instant::now().saturating_duration_since(start) < Duration::from_secs(1),
		"Expected low resolution instant interval to fire within less than a second.",
	);
}

#[test]
fn interval_at_is_queuing_ticks() {
	let start = Instant::now();

	let interval = interval_at(start, std::time::Duration::from_millis(100));

	// Let's wait for 200ms, thus 3 elements should be queued up (1st at 0ms, 2nd at 100ms, 3rd
	// at 200ms).
	std::thread::sleep(Duration::from_millis(200));

	futures::executor::block_on(async {
		interval.take(3).collect::<Vec<()>>().await;
	});

	// Make sure we did not wait for more than 300 ms, which would imply that `at_interval` is
	// not queuing ticks.
	assert!(
		Instant::now().saturating_duration_since(start) < Duration::from_millis(300),
		"Expect interval to /queue/ events when not polled for a while.",
	);
}

#[test]
fn interval_at_with_initial_delay() {
	let start = Instant::now();

	let mut interval = interval_at(
		std::time::Instant::now() + Duration::from_millis(100),
		std::time::Duration::from_secs(10),
	);

	futures::executor::block_on(async {
		interval.next().await;
	});

	assert!(
		Instant::now().saturating_duration_since(start) > Duration::from_millis(100),
		"Expected interval with initial delay not to fire right away.",
	);
}

#[derive(Clone)]
pub(crate) struct TestApi {
	pub(crate) authorities: Vec<AuthorityId>,
}

impl ProvideRuntimeApi<Block> for TestApi {
	type Api = RuntimeApi;

	fn runtime_api<'a>(&'a self) -> ApiRef<'a, Self::Api> {
		RuntimeApi {
			authorities: self.authorities.clone(),
		}.into()
	}
}

/// Blockchain database header backend. Does not perform any validation.
impl<Block: BlockT> HeaderBackend<Block> for TestApi {
	fn header(
		&self,
		_id: BlockId<Block>,
	) -> std::result::Result<Option<Block::Header>, sp_blockchain::Error> {
		Ok(None)
	}

	fn info(&self) -> sc_client_api::blockchain::Info<Block> {
		sc_client_api::blockchain::Info {
			best_hash: Default::default(),
			best_number: Zero::zero(),
			finalized_hash: Default::default(),
			finalized_number: Zero::zero(),
			genesis_hash: Default::default(),
			number_leaves: Default::default(),
		}
	}

	fn status(
		&self,
		_id: BlockId<Block>,
	) -> std::result::Result<sc_client_api::blockchain::BlockStatus, sp_blockchain::Error> {
		Ok(sc_client_api::blockchain::BlockStatus::Unknown)
	}

	fn number(
		&self,
		_hash: Block::Hash,
	) -> std::result::Result<Option<NumberFor<Block>>, sp_blockchain::Error> {
		Ok(None)
	}

	fn hash(
		&self,
		_number: NumberFor<Block>,
	) -> std::result::Result<Option<Block::Hash>, sp_blockchain::Error> {
		Ok(None)
	}
}

pub(crate) struct RuntimeApi {
	authorities: Vec<AuthorityId>,
}

sp_api::mock_impl_runtime_apis! {
	impl AuthorityDiscoveryApi<Block> for RuntimeApi {
		type Error = sp_blockchain::Error;

		fn authorities(&self) -> Vec<AuthorityId> {
			self.authorities.clone()
		}
	}
}

pub struct TestNetwork {
	peer_id: PeerId,
	external_addresses: Vec<Multiaddr>,
	// Whenever functions on `TestNetwork` are called, the function arguments are added to the
	// vectors below.
	pub put_value_call: Arc<Mutex<Vec<(kad::record::Key, Vec<u8>)>>>,
	pub get_value_call: Arc<Mutex<Vec<kad::record::Key>>>,
	pub set_priority_group_call: Arc<Mutex<Vec<(String, HashSet<Multiaddr>)>>>,
}

impl Default for TestNetwork {
	fn default() -> Self {
		TestNetwork {
			peer_id: PeerId::random(),
			external_addresses: vec![
				"/ip6/2001:db8::/tcp/30333"
					.parse().unwrap(),
			],
			put_value_call: Default::default(),
			get_value_call: Default::default(),
			set_priority_group_call: Default::default(),
		}
	}
}

impl NetworkProvider for TestNetwork {
	fn set_priority_group(
		&self,
		group_id: String,
		peers: HashSet<Multiaddr>,
	) -> std::result::Result<(), String> {
		self.set_priority_group_call
			.lock()
			.unwrap()
			.push((group_id, peers));
		Ok(())
	}
	fn put_value(&self, key: kad::record::Key, value: Vec<u8>) {
		self.put_value_call.lock().unwrap().push((key, value));
	}
	fn get_value(&self, key: &kad::record::Key) {
		self.get_value_call.lock().unwrap().push(key.clone());
	}
}

impl NetworkStateInfo for TestNetwork {
	fn local_peer_id(&self) -> PeerId {
		self.peer_id.clone()
	}

	fn external_addresses(&self) -> Vec<Multiaddr> {
		self.external_addresses.clone()
	}
}

fn build_dht_event(
	addresses: Vec<Multiaddr>,
	public_key: AuthorityId,
	key_store: &BareCryptoStorePtr,
) -> (libp2p::kad::record::Key, Vec<u8>) {
	let mut serialized_addresses = vec![];
	schema::AuthorityAddresses {
		addresses: addresses.into_iter().map(|a| a.to_vec()).collect()
	}.encode(&mut serialized_addresses)
		.map_err(Error::EncodingProto)
		.unwrap();

	let signature = key_store.read()
		.sign_with(
			key_types::AUTHORITY_DISCOVERY,
			&public_key.clone().into(),
			serialized_addresses.as_slice(),
		)
		.map_err(|_| Error::Signing)
		.unwrap();

	let mut signed_addresses = vec![];
	schema::SignedAuthorityAddresses {
		addresses: serialized_addresses.clone(),
		signature,
	}
	.encode(&mut signed_addresses)
		.map_err(Error::EncodingProto)
		.unwrap();

	let key = hash_authority_id(&public_key.to_raw_vec());
	let value = signed_addresses;
	(key, value)
}

#[test]
fn new_registers_metrics() {
	let (_dht_event_tx, dht_event_rx) = channel(1000);
	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = KeyStore::new();
	let test_api = Arc::new(TestApi {
		authorities: vec![],
	});

	let registry = prometheus_endpoint::Registry::new();

	let (_to_worker, from_service) = mpsc::channel(0);
	Worker::new(
		from_service,
		test_api,
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(key_store),
		Some(registry.clone()),
	);

	assert!(registry.gather().len() > 0);
}

#[test]
fn triggers_dht_get_query() {
	sp_tracing::try_init_simple();
	let (_dht_event_tx, dht_event_rx) = channel(1000);

	// Generate authority keys
	let authority_1_key_pair = AuthorityPair::from_seed_slice(&[1; 32]).unwrap();
	let authority_2_key_pair = AuthorityPair::from_seed_slice(&[2; 32]).unwrap();

	let test_api = Arc::new(TestApi {
		authorities: vec![authority_1_key_pair.public(), authority_2_key_pair.public()],
	});

	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = KeyStore::new();

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(key_store),
		None,
	);

	worker.refill_pending_lookups_queue().unwrap();

	futures::executor::block_on(futures::future::poll_fn(|cx| {
		assert_eq!(Poll::Pending, worker.poll_unpin(cx));
		Poll::Ready(())
	}));

	// Expect authority discovery to request new records from the dht.
	assert_eq!(network.get_value_call.lock().unwrap().len(), 2);
}

#[test]
fn publish_discover_cycle() {
	sp_tracing::try_init_simple();

	// Node A publishing its address.

	let (_dht_event_tx, dht_event_rx) = channel(1000);

	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let node_a_multiaddr = {
		let peer_id = network.local_peer_id();
		let address = network.external_addresses().pop().unwrap();

		address.with(multiaddr::Protocol::P2p(
			peer_id.into(),
		))
	};

	let key_store = KeyStore::new();
	let node_a_public = key_store
		.write()
		.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
		.unwrap();
	let test_api = Arc::new(TestApi {
		authorities: vec![node_a_public.into()],
	});

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(key_store),
		None,
	);

	worker.publish_ext_addresses().unwrap();

	// Expect authority discovery to put a new record onto the dht.
	assert_eq!(network.put_value_call.lock().unwrap().len(), 1);

	let dht_event = {
		let (key, value) = network.put_value_call.lock().unwrap().pop().unwrap();
		sc_network::DhtEvent::ValueFound(vec![(key, value)])
	};

	// Node B discovering node A's address.

	let (mut dht_event_tx, dht_event_rx) = channel(1000);
	let test_api = Arc::new(TestApi {
		// Make sure node B identifies node A as an authority.
		authorities: vec![node_a_public.into()],
	});
	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = KeyStore::new();

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(key_store),
		None,
	);

	dht_event_tx.try_send(dht_event).unwrap();

	let f = |cx: &mut Context<'_>| -> Poll<()> {
		worker.refill_pending_lookups_queue().unwrap();
		worker.start_new_lookups();

		// Make authority discovery handle the event.
		if let Poll::Ready(e) = worker.handle_dht_events(cx) {
			panic!("Unexpected error: {:?}", e);
		}
		worker.set_priority_group().unwrap();

		// Expect authority discovery to set the priority set.
		assert_eq!(network.set_priority_group_call.lock().unwrap().len(), 1);

		assert_eq!(
			network.set_priority_group_call.lock().unwrap()[0],
			(
				"authorities".to_string(),
				HashSet::from_iter(vec![node_a_multiaddr.clone()].into_iter())
			)
		);

		Poll::Ready(())
	};

	let _ = block_on(poll_fn(f));
}

#[test]
fn terminate_when_event_stream_terminates() {
	let (dht_event_tx, dht_event_rx) = channel(1000);
	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = KeyStore::new();
	let test_api = Arc::new(TestApi {
		authorities: vec![],
	});

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(key_store),
		None,
	);

	block_on(async {
		assert_eq!(Poll::Pending, poll!(&mut worker));

		// Simulate termination of the network through dropping the sender side of the dht event
		// channel.
		drop(dht_event_tx);

		assert_eq!(
			Poll::Ready(()), poll!(&mut worker),
			"Expect the authority discovery module to terminate once the sending side of the dht \
			event channel is terminated.",
		);
	});
}

#[test]
fn continue_operating_when_service_channel_is_dropped() {
	let (_dht_event_tx, dht_event_rx) = channel(0);
	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = KeyStore::new();
	let test_api = Arc::new(TestApi {
		authorities: vec![],
	});

	let (to_worker, from_service) = mpsc::channel(0);
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(key_store),
		None,
	);

	block_on(async {
		assert_eq!(Poll::Pending, poll!(&mut worker));

		drop(to_worker);

		for _ in 0..100 {
			assert_eq!(
				Poll::Pending, poll!(&mut worker),
				"Expect authority discovery `Worker` not to panic when service channel is dropped.",
			);
		}
	});
}

#[test]
fn dont_stop_polling_when_error_is_returned() {
	#[derive(PartialEq, Debug)]
	enum Event {
		Processed,
		End,
	};

	let (mut dht_event_tx, dht_event_rx) = channel(1000);
	let (mut discovery_update_tx, mut discovery_update_rx) = channel(1000);
	let network: Arc<TestNetwork> = Arc::new(Default::default());
	let key_store = KeyStore::new();
	let test_api = Arc::new(TestApi {
		authorities: vec![],
	});
	let mut pool = LocalPool::new();

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut worker = Worker::new(
		from_service,
		test_api,
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(key_store),
		None,
	);

	// Spawn the authority discovery to make sure it is polled independently.
	//
	// As this is a local pool, only one future at a time will have the CPU and
	// can make progress until the future returns `Pending`.
	pool.spawner().spawn_local_obj(
		futures::future::poll_fn(move |ctx| {
			match std::pin::Pin::new(&mut worker).poll(ctx) {
				Poll::Ready(()) => {},
				Poll::Pending => {
					discovery_update_tx.send(Event::Processed).now_or_never();
					return Poll::Pending;
				},
			}
			let _ = discovery_update_tx.send(Event::End).now_or_never().unwrap();
			Poll::Ready(())
		}).boxed_local().into(),
	).expect("Spawns authority discovery");

	pool.run_until(
		// The future that drives the event stream
		async {
			// Send an event that should generate an error
			let _ = dht_event_tx.send(DhtEvent::ValueFound(Default::default())).now_or_never();
			// Send the same event again to make sure that the event stream needs to be polled twice
			// to be woken up again.
			let _ = dht_event_tx.send(DhtEvent::ValueFound(Default::default())).now_or_never();

			// Now we call `await` and give the control to the authority discovery future.
			assert_eq!(Some(Event::Processed), discovery_update_rx.next().await);

			// Drop the event rx to stop the authority discovery. If it was polled correctly, it
			// should end properly.
			drop(dht_event_tx);

			assert!(
				discovery_update_rx.collect::<Vec<Event>>()
					.await
					.into_iter()
					.any(|evt| evt == Event::End),
				"The authority discovery should have ended",
			);
		}
	);
}

/// In the scenario of a validator publishing the address of its sentry node to
/// the DHT, said sentry node should not add its own Multiaddr to the
/// peerset "authority" priority group.
#[test]
fn never_add_own_address_to_priority_group() {
	let validator_key_store = KeyStore::new();
	let validator_public = validator_key_store
		.write()
		.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
		.unwrap();

	let sentry_network: Arc<TestNetwork> = Arc::new(Default::default());

	let sentry_multiaddr = {
		let peer_id = sentry_network.local_peer_id();
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:2/tcp/30333".parse().unwrap();

		address.with(multiaddr::Protocol::P2p(peer_id.into()))
	};

	// Address of some other sentry node of `validator`.
	let random_multiaddr = {
		let peer_id = PeerId::random();
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:1/tcp/30333".parse().unwrap();

		address.with(multiaddr::Protocol::P2p(
			peer_id.into(),
		))
	};

	let dht_event = build_dht_event(
		vec![sentry_multiaddr, random_multiaddr.clone()],
		validator_public.into(),
		&validator_key_store,
	);

	let (_dht_event_tx, dht_event_rx) = channel(1);
	let sentry_test_api = Arc::new(TestApi {
		// Make sure the sentry node identifies its validator as an authority.
		authorities: vec![validator_public.into()],
	});

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut sentry_worker = Worker::new(
		from_service,
		sentry_test_api,
		sentry_network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Sentry,
		None,
	);

	sentry_worker.refill_pending_lookups_queue().unwrap();
	sentry_worker.start_new_lookups();

	sentry_worker.handle_dht_value_found_event(vec![dht_event]).unwrap();
	sentry_worker.set_priority_group().unwrap();

	assert_eq!(
		sentry_network.set_priority_group_call.lock().unwrap().len(), 1,
		"Expect authority discovery to set the priority set.",
	);

	assert_eq!(
		sentry_network.set_priority_group_call.lock().unwrap()[0],
		(
			"authorities".to_string(),
			HashSet::from_iter(vec![random_multiaddr.clone()].into_iter(),)
		),
		"Expect authority discovery to only add `random_multiaddr`."
	);
}

#[test]
fn limit_number_of_addresses_added_to_cache_per_authority() {
	let remote_key_store = KeyStore::new();
	let remote_public = remote_key_store
		.write()
		.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
		.unwrap();

	let addresses = (0..100).map(|_| {
		let peer_id = PeerId::random();
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:1/tcp/30333".parse().unwrap();
		address.with(multiaddr::Protocol::P2p(
			peer_id.into(),
		))
	}).collect();

	let dht_event = build_dht_event(
		addresses,
		remote_public.into(),
		&remote_key_store,
	);

	let (_dht_event_tx, dht_event_rx) = channel(1);

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut worker = Worker::new(
		from_service,
		Arc::new(TestApi { authorities: vec![remote_public.into()] }),
		Arc::new(TestNetwork::default()),
		vec![],
		dht_event_rx.boxed(),
		Role::Sentry,
		None,
	);

	worker.refill_pending_lookups_queue().unwrap();
	worker.start_new_lookups();

	worker.handle_dht_value_found_event(vec![dht_event]).unwrap();
	assert_eq!(
		MAX_ADDRESSES_PER_AUTHORITY,
		worker.addr_cache.get_addresses_by_authority_id(&remote_public.into()).unwrap().len(),
	);
}

#[test]
fn do_not_cache_addresses_without_peer_id() {
	let remote_key_store = KeyStore::new();
	let remote_public = remote_key_store
		.write()
		.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
		.unwrap();

	let multiaddr_with_peer_id = {
		let peer_id = PeerId::random();
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:2/tcp/30333".parse().unwrap();

		address.with(multiaddr::Protocol::P2p(peer_id.into()))
	};

	let multiaddr_without_peer_id: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:1/tcp/30333".parse().unwrap();

	let dht_event = build_dht_event(
		vec![
			multiaddr_with_peer_id.clone(),
			multiaddr_without_peer_id,
		],
		remote_public.into(),
		&remote_key_store,
	);

	let (_dht_event_tx, dht_event_rx) = channel(1);
	let local_test_api = Arc::new(TestApi {
		// Make sure the sentry node identifies its validator as an authority.
		authorities: vec![remote_public.into()],
	});
	let local_network: Arc<TestNetwork> = Arc::new(Default::default());
	let local_key_store = KeyStore::new();

	let (_to_worker, from_service) = mpsc::channel(0);
	let mut local_worker = Worker::new(
		from_service,
		local_test_api,
		local_network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(local_key_store),
		None,
	);

	local_worker.refill_pending_lookups_queue().unwrap();
	local_worker.start_new_lookups();

	local_worker.handle_dht_value_found_event(vec![dht_event]).unwrap();

	assert_eq!(
		Some(&vec![multiaddr_with_peer_id]),
		local_worker.addr_cache.get_addresses_by_authority_id(&remote_public.into()),
		"Expect worker to only cache `Multiaddr`s with `PeerId`s.",
	);
}

#[test]
fn addresses_to_publish_adds_p2p() {
	let (_dht_event_tx, dht_event_rx) = channel(1000);
	let network: Arc<TestNetwork> = Arc::new(Default::default());

	assert!(!matches!(
		network.external_addresses().pop().unwrap().pop().unwrap(),
		multiaddr::Protocol::P2p(_)
	));

	let (_to_worker, from_service) = mpsc::channel(0);
	let worker = Worker::new(
		from_service,
		Arc::new(TestApi {
			authorities: vec![],
		}),
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(KeyStore::new()),
		Some(prometheus_endpoint::Registry::new()),
	);

	assert!(
		matches!(
			worker.addresses_to_publish().next().unwrap().pop().unwrap(),
			multiaddr::Protocol::P2p(_)
		),
		"Expect `addresses_to_publish` to append `p2p` protocol component.",
	);
}

/// Ensure [`Worker::addresses_to_publish`] does not add an additional `p2p` protocol component in
/// case one already exists.
#[test]
fn addresses_to_publish_respects_existing_p2p_protocol() {
	let (_dht_event_tx, dht_event_rx) = channel(1000);
	let network: Arc<TestNetwork> = Arc::new(TestNetwork {
		external_addresses: vec![
			"/ip6/2001:db8::/tcp/30333/p2p/QmcgpsyWgH8Y8ajJz1Cu72KnS5uo2Aa2LpzU7kinSupNKC"
				.parse().unwrap(),
		],
		.. Default::default()
	});

	let (_to_worker, from_service) = mpsc::channel(0);
	let worker = Worker::new(
		from_service,
		Arc::new(TestApi {
			authorities: vec![],
		}),
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Authority(KeyStore::new()),
		Some(prometheus_endpoint::Registry::new()),
	);

	assert_eq!(
		network.external_addresses, worker.addresses_to_publish().collect::<Vec<_>>(),
		"Expected Multiaddr from `TestNetwork` to not be altered.",
	);
}

#[test]
fn lookup_throttling() {
	let remote_multiaddr = {
		let peer_id = PeerId::random();
		let address: Multiaddr = "/ip6/2001:db8:0:0:0:0:0:1/tcp/30333".parse().unwrap();

		address.with(multiaddr::Protocol::P2p(
			peer_id.into(),
		))
	};
	let remote_key_store = KeyStore::new();
	let remote_public_keys: Vec<AuthorityId> = (0..20).map(|_| {
		remote_key_store
			.write()
			.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
			.unwrap().into()
	}).collect();
	let remote_hash_to_key = remote_public_keys.iter()
		.map(|k| (hash_authority_id(k.as_ref()), k.clone()))
		.collect::<HashMap<_, _>>();


	let (mut dht_event_tx, dht_event_rx) = channel(1);
	let (_to_worker, from_service) = mpsc::channel(0);
	let network = Arc::new(TestNetwork::default());
	let mut worker = Worker::new(
		from_service,
		Arc::new(TestApi { authorities: remote_public_keys.clone() }),
		network.clone(),
		vec![],
		dht_event_rx.boxed(),
		Role::Sentry,
		None,
	);

	futures::executor::block_on(futures::future::poll_fn(|cx| {
		worker.refill_pending_lookups_queue().unwrap();

		// Assert worker to trigger MAX_IN_FLIGHT_LOOKUPS lookups.
		assert_eq!(Poll::Pending, worker.poll_unpin(cx));
		assert_eq!(worker.pending_lookups.len(), remote_public_keys.len() - MAX_IN_FLIGHT_LOOKUPS);
		assert_eq!(worker.in_flight_lookups.len(), MAX_IN_FLIGHT_LOOKUPS);
		assert_eq!(network.get_value_call.lock().unwrap().len(), MAX_IN_FLIGHT_LOOKUPS);

		// Make first lookup succeed.
		let remote_hash = network.get_value_call.lock().unwrap().pop().unwrap();
		let remote_key: AuthorityId = remote_hash_to_key.get(&remote_hash).unwrap().clone();
		let dht_event = {
			let (key, value) = build_dht_event(vec![remote_multiaddr.clone()], remote_key, &remote_key_store);
			sc_network::DhtEvent::ValueFound(vec![(key, value)])
		};
		dht_event_tx.try_send(dht_event).expect("Channel has capacity of 1.");

		// Assert worker to trigger another lookup.
		assert_eq!(Poll::Pending, worker.poll_unpin(cx));
		assert_eq!(worker.pending_lookups.len(), remote_public_keys.len() - MAX_IN_FLIGHT_LOOKUPS - 1);
		assert_eq!(worker.in_flight_lookups.len(), MAX_IN_FLIGHT_LOOKUPS);
		assert_eq!(network.get_value_call.lock().unwrap().len(), MAX_IN_FLIGHT_LOOKUPS);

		// Make second one fail.
		let remote_hash = network.get_value_call.lock().unwrap().pop().unwrap();
		let dht_event = sc_network::DhtEvent::ValueNotFound(remote_hash);
		dht_event_tx.try_send(dht_event).expect("Channel has capacity of 1.");

		// Assert worker to trigger another lookup.
		assert_eq!(Poll::Pending, worker.poll_unpin(cx));
		assert_eq!(worker.pending_lookups.len(), remote_public_keys.len() - MAX_IN_FLIGHT_LOOKUPS - 2);
		assert_eq!(worker.in_flight_lookups.len(), MAX_IN_FLIGHT_LOOKUPS);
		assert_eq!(network.get_value_call.lock().unwrap().len(), MAX_IN_FLIGHT_LOOKUPS);

		worker.refill_pending_lookups_queue().unwrap();

		// Assert worker to restock pending lookups and forget about in-flight lookups.
		assert_eq!(worker.pending_lookups.len(), remote_public_keys.len());
		assert_eq!(worker.in_flight_lookups.len(), 0);

		Poll::Ready(())
	}));
}
