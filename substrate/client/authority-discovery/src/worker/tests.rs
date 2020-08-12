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
		vec!["/ip6/2001:db8::/tcp/30333".parse().unwrap()]
	}
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
fn request_addresses_of_others_triggers_dht_get_query() {
	let _ = ::env_logger::try_init();
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

	worker.request_addresses_of_others().unwrap();

	// Expect authority discovery to request new records from the dht.
	assert_eq!(network.get_value_call.lock().unwrap().len(), 2);
}

#[test]
fn publish_discover_cycle() {
	let _ = ::env_logger::try_init();

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

	let dht_event = {
		let addresses = vec![
			sentry_multiaddr.to_vec(),
			random_multiaddr.to_vec(),
		];

		let mut serialized_addresses = vec![];
		schema::AuthorityAddresses { addresses }
		.encode(&mut serialized_addresses)
			.map_err(Error::EncodingProto)
			.unwrap();

		let signature = validator_key_store.read()
			.sign_with(
				key_types::AUTHORITY_DISCOVERY,
				&validator_public.clone().into(),
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

		let key = hash_authority_id(&validator_public.to_raw_vec());
		let value = signed_addresses;
		(key, value)
	};

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

	let dht_event = {
		let addresses = vec![
			multiaddr_with_peer_id.to_vec(),
			multiaddr_without_peer_id.to_vec(),
		];

		let mut serialized_addresses = vec![];
		schema::AuthorityAddresses { addresses }
		.encode(&mut serialized_addresses)
			.map_err(Error::EncodingProto)
			.unwrap();

		let signature = remote_key_store.read()
			.sign_with(
				key_types::AUTHORITY_DISCOVERY,
				&remote_public.clone().into(),
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

		let key = hash_authority_id(&remote_public.to_raw_vec());
		let value = signed_addresses;
		(key, value)
	};

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

	local_worker.handle_dht_value_found_event(vec![dht_event]).unwrap();

	assert_eq!(
		Some(&vec![multiaddr_with_peer_id]),
		local_worker.addr_cache.get_addresses_by_authority_id(&remote_public.into()),
		"Expect worker to only cache `Multiaddr`s with `PeerId`s.",
	);
}
