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

use crate::{
	new_worker_and_service_with_config,
	worker::{
		tests::{TestApi, TestNetwork},
		AddrCache, Role,
	},
	WorkerConfig,
};

use futures::{channel::mpsc::channel, executor::LocalPool, task::LocalSpawn};
use sc_network_types::ed25519;
use std::{collections::HashSet, sync::Arc};

use sc_network::{multiaddr::Protocol, Multiaddr, PeerId};
use sp_authority_discovery::AuthorityId;
use sp_core::{crypto::key_types, testing::TaskExecutor, traits::SpawnNamed};
use sp_keystore::{testing::MemoryKeystore, Keystore};

pub(super) fn create_spawner() -> Box<dyn SpawnNamed> {
	Box::new(TaskExecutor::new())
}

pub(super) fn test_config(path_buf: Option<std::path::PathBuf>) -> WorkerConfig {
	WorkerConfig { persisted_cache_directory: path_buf, ..Default::default() }
}

#[tokio::test]
async fn get_addresses_and_authority_id() {
	let (_dht_event_tx, dht_event_rx) = channel(0);
	let network: Arc<TestNetwork> = Arc::new(Default::default());

	let mut pool = LocalPool::new();

	let key_store = MemoryKeystore::new();

	let remote_authority_id: AuthorityId = pool.run_until(async {
		key_store
			.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
			.unwrap()
			.into()
	});

	let remote_peer_id = PeerId::random();
	let remote_addr = "/ip6/2001:db8:0:0:0:0:0:2/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::P2p(remote_peer_id.into()));

	let test_api = Arc::new(TestApi { authorities: vec![] });

	let tempdir = tempfile::tempdir().unwrap();
	let path = tempdir.path().to_path_buf();
	let (mut worker, mut service) = new_worker_and_service_with_config(
		test_config(Some(path)),
		test_api,
		network.clone(),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(key_store.into()),
		None,
		create_spawner(),
	);
	worker.inject_addresses(remote_authority_id.clone(), vec![remote_addr.clone()]);

	pool.spawner().spawn_local_obj(Box::pin(worker.run()).into()).unwrap();

	pool.run_until(async {
		assert_eq!(
			Some(HashSet::from([remote_addr])),
			service.get_addresses_by_authority_id(remote_authority_id.clone()).await,
		);
		assert_eq!(
			Some(HashSet::from([remote_authority_id])),
			service.get_authority_ids_by_peer_id(remote_peer_id.into()).await,
		);
	});
}

#[tokio::test]
async fn cryptos_are_compatible() {
	use sp_core::crypto::Pair;

	let libp2p_keypair = ed25519::Keypair::generate();
	let libp2p_public = libp2p_keypair.public();

	let sp_core_secret =
		{ sp_core::ed25519::Pair::from_seed_slice(&libp2p_keypair.secret().as_ref()).unwrap() };
	let sp_core_public = sp_core_secret.public();

	let message = b"we are more powerful than not to be better";

	let libp2p_signature = libp2p_keypair.sign(message);
	let sp_core_signature = sp_core_secret.sign(message); // no error expected...

	assert!(sp_core::ed25519::Pair::verify(
		&sp_core::ed25519::Signature::try_from(libp2p_signature.as_slice()).unwrap(),
		message,
		&sp_core_public
	));
	assert!(libp2p_public.verify(message, sp_core_signature.as_ref()));
}

#[tokio::test]
async fn when_addr_cache_is_persisted_with_authority_ids_then_when_worker_is_created_it_loads_the_persisted_cache(
) {
	// ARRANGE
	let (_dht_event_tx, dht_event_rx) = channel(0);
	let mut pool = LocalPool::new();
	let key_store = MemoryKeystore::new();

	let remote_authority_id: AuthorityId = pool.run_until(async {
		key_store
			.sr25519_generate_new(key_types::AUTHORITY_DISCOVERY, None)
			.unwrap()
			.into()
	});
	let remote_peer_id = PeerId::random();
	let remote_addr = "/ip6/2001:db8:0:0:0:0:0:2/tcp/30333"
		.parse::<Multiaddr>()
		.unwrap()
		.with(Protocol::P2p(remote_peer_id.into()));

	let tempdir = tempfile::tempdir().unwrap();
	let cache_path = tempdir.path().to_path_buf();

	// persist the remote_authority_id and remote_addr in the cache
	{
		let mut addr_cache = AddrCache::default();
		addr_cache.insert(remote_authority_id.clone(), vec![remote_addr.clone()]);
		let path_to_save = cache_path.join(crate::worker::ADDR_CACHE_FILE_NAME);
		addr_cache.serialize_and_persist(&path_to_save);
	}

	let (_, from_service) = futures::channel::mpsc::channel(0);

	// ACT
	// Create a worker with the persisted cache
	let worker = crate::worker::Worker::new(
		from_service,
		Arc::new(TestApi { authorities: vec![] }),
		Arc::new(TestNetwork::default()),
		Box::pin(dht_event_rx),
		Role::PublishAndDiscover(key_store.into()),
		None,
		test_config(Some(cache_path)),
		create_spawner(),
	);

	// ASSERT
	assert!(worker.contains_authority(&remote_authority_id));
}
