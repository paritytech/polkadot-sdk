// Copyright 2017-2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use super::state_full::split_range;
use self::error::Error;

use std::sync::Arc;
use assert_matches::assert_matches;
use futures::stream::Stream;
use primitives::storage::well_known_keys;
use sr_io::blake2_256;
use test_client::{
	prelude::*,
	consensus::BlockOrigin,
	runtime,
};

#[test]
fn should_return_storage() {
	const KEY: &[u8] = b":mock";
	const VALUE: &[u8] = b"hello world";
	const STORAGE_KEY: &[u8] = b":child_storage:default:child";
	const CHILD_VALUE: &[u8] = b"hello world !";

	let mut core = tokio::runtime::Runtime::new().unwrap();
	let client = TestClientBuilder::new()
		.add_extra_storage(KEY.to_vec(), VALUE.to_vec())
		.add_extra_child_storage(STORAGE_KEY.to_vec(), KEY.to_vec(), CHILD_VALUE.to_vec())
		.build();
	let genesis_hash = client.genesis_hash();
	let client = new_full(Arc::new(client), Subscriptions::new(Arc::new(core.executor())));
	let key = StorageKey(KEY.to_vec());
	let storage_key = StorageKey(STORAGE_KEY.to_vec());

	assert_eq!(
		client.storage(key.clone(), Some(genesis_hash).into()).wait()
			.map(|x| x.map(|x| x.0.len())).unwrap().unwrap() as usize,
		VALUE.len(),
	);
	assert_matches!(
		client.storage_hash(key.clone(), Some(genesis_hash).into()).wait()
			.map(|x| x.is_some()),
		Ok(true)
	);
	assert_eq!(
		client.storage_size(key.clone(), None).wait().unwrap().unwrap() as usize,
		VALUE.len(),
	);
	assert_eq!(
		core.block_on(
			client.child_storage(storage_key, key, Some(genesis_hash).into())
				.map(|x| x.map(|x| x.0.len()))
		).unwrap().unwrap() as usize,
		CHILD_VALUE.len(),
	);

}

#[test]
fn should_return_child_storage() {
	let core = tokio::runtime::Runtime::new().unwrap();
	let client = Arc::new(test_client::TestClientBuilder::new()
		.add_child_storage("test", "key", vec![42_u8])
		.build());
	let genesis_hash = client.genesis_hash();
	let client = new_full(client, Subscriptions::new(Arc::new(core.executor())));
	let child_key = StorageKey(well_known_keys::CHILD_STORAGE_KEY_PREFIX.iter().chain(b"test").cloned().collect());
	let key = StorageKey(b"key".to_vec());


	assert_matches!(
		client.child_storage(child_key.clone(), key.clone(), Some(genesis_hash).into()).wait(),
		Ok(Some(StorageData(ref d))) if d[0] == 42 && d.len() == 1
	);
	assert_matches!(
		client.child_storage_hash(child_key.clone(), key.clone(), Some(genesis_hash).into())
			.wait().map(|x| x.is_some()),
		Ok(true)
	);
	assert_matches!(
		client.child_storage_size(child_key.clone(), key.clone(), None).wait(),
		Ok(Some(1))
	);
}

#[test]
fn should_call_contract() {
	let core = tokio::runtime::Runtime::new().unwrap();
	let client = Arc::new(test_client::new());
	let genesis_hash = client.genesis_hash();
	let client = new_full(client, Subscriptions::new(Arc::new(core.executor())));

	assert_matches!(
		client.call("balanceOf".into(), Bytes(vec![1,2,3]), Some(genesis_hash).into()).wait(),
		Err(Error::Client(_))
	)
}

#[test]
fn should_notify_about_storage_changes() {
	let mut core = tokio::runtime::Runtime::new().unwrap();
	let remote = core.executor();
	let (subscriber, id, transport) = Subscriber::new_test("test");

	{
		let client = Arc::new(test_client::new());
		let api = new_full(client.clone(), Subscriptions::new(Arc::new(remote)));

		api.subscribe_storage(Default::default(), subscriber, None.into());

		// assert id assigned
		assert_eq!(core.block_on(id), Ok(Ok(SubscriptionId::Number(1))));

		let mut builder = client.new_block(Default::default()).unwrap();
		builder.push_transfer(runtime::Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Ferdie.into(),
			amount: 42,
			nonce: 0,
		}).unwrap();
		client.import(BlockOrigin::Own, builder.bake().unwrap()).unwrap();
	}

	// assert notification sent to transport
	let (notification, next) = core.block_on(transport.into_future()).unwrap();
	assert!(notification.is_some());
	// no more notifications on this channel
	assert_eq!(core.block_on(next.into_future()).unwrap().0, None);
}

#[test]
fn should_send_initial_storage_changes_and_notifications() {
	let mut core = tokio::runtime::Runtime::new().unwrap();
	let remote = core.executor();
	let (subscriber, id, transport) = Subscriber::new_test("test");

	{
		let client = Arc::new(test_client::new());
		let api = new_full(client.clone(), Subscriptions::new(Arc::new(remote)));

		let alice_balance_key = blake2_256(&runtime::system::balance_of_key(AccountKeyring::Alice.into()));

		api.subscribe_storage(Default::default(), subscriber, Some(vec![
			StorageKey(alice_balance_key.to_vec()),
		]).into());

		// assert id assigned
		assert_eq!(core.block_on(id), Ok(Ok(SubscriptionId::Number(1))));

		let mut builder = client.new_block(Default::default()).unwrap();
		builder.push_transfer(runtime::Transfer {
			from: AccountKeyring::Alice.into(),
			to: AccountKeyring::Ferdie.into(),
			amount: 42,
			nonce: 0,
		}).unwrap();
		client.import(BlockOrigin::Own, builder.bake().unwrap()).unwrap();
	}

	// assert initial values sent to transport
	let (notification, next) = core.block_on(transport.into_future()).unwrap();
	assert!(notification.is_some());
	// assert notification sent to transport
	let (notification, next) = core.block_on(next.into_future()).unwrap();
	assert!(notification.is_some());
	// no more notifications on this channel
	assert_eq!(core.block_on(next.into_future()).unwrap().0, None);
}

#[test]
fn should_query_storage() {
	fn run_tests(client: Arc<TestClient>) {
		let core = tokio::runtime::Runtime::new().unwrap();
		let api = new_full(client.clone(), Subscriptions::new(Arc::new(core.executor())));

		let add_block = |nonce| {
			let mut builder = client.new_block(Default::default()).unwrap();
			// fake change: None -> None -> None
			builder.push_storage_change(vec![1], None).unwrap();
			// fake change: None -> Some(value) -> Some(value)
			builder.push_storage_change(vec![2], Some(vec![2])).unwrap();
			// actual change: None -> Some(value) -> None
			builder.push_storage_change(vec![3], if nonce == 0 { Some(vec![3]) } else { None }).unwrap();
			// actual change: None -> Some(value)
			builder.push_storage_change(vec![4], if nonce == 0 { None } else { Some(vec![4]) }).unwrap();
			// actual change: Some(value1) -> Some(value2)
			builder.push_storage_change(vec![5], Some(vec![nonce as u8])).unwrap();
			let block = builder.bake().unwrap();
			let hash = block.header.hash();
			client.import(BlockOrigin::Own, block).unwrap();
			hash
		};
		let block1_hash = add_block(0);
		let block2_hash = add_block(1);
		let genesis_hash = client.genesis_hash();

		let mut expected = vec![
			StorageChangeSet {
				block: genesis_hash,
				changes: vec![
					(StorageKey(vec![1]), None),
					(StorageKey(vec![2]), None),
					(StorageKey(vec![3]), None),
					(StorageKey(vec![4]), None),
					(StorageKey(vec![5]), None),
				],
			},
			StorageChangeSet {
				block: block1_hash,
				changes: vec![
					(StorageKey(vec![2]), Some(StorageData(vec![2]))),
					(StorageKey(vec![3]), Some(StorageData(vec![3]))),
					(StorageKey(vec![5]), Some(StorageData(vec![0]))),
				],
			},
		];

		// Query changes only up to block1
		let keys = (1..6).map(|k| StorageKey(vec![k])).collect::<Vec<_>>();
		let result = api.query_storage(
			keys.clone(),
			genesis_hash,
			Some(block1_hash).into(),
		);

		assert_eq!(result.wait().unwrap(), expected);

		// Query all changes
		let result = api.query_storage(
			keys.clone(),
			genesis_hash,
			None.into(),
		);

		expected.push(StorageChangeSet {
			block: block2_hash,
			changes: vec![
				(StorageKey(vec![3]), None),
				(StorageKey(vec![4]), Some(StorageData(vec![4]))),
				(StorageKey(vec![5]), Some(StorageData(vec![1]))),
			],
		});
		assert_eq!(result.wait().unwrap(), expected);
	}

	run_tests(Arc::new(test_client::new()));
	run_tests(Arc::new(TestClientBuilder::new().set_support_changes_trie(true).build()));
}

#[test]
fn should_split_ranges() {
	assert_eq!(split_range(1, None), (0..1, None));
	assert_eq!(split_range(100, None), (0..100, None));
	assert_eq!(split_range(1, Some(0)), (0..1, None));
	assert_eq!(split_range(100, Some(50)), (0..50, Some(50..100)));
	assert_eq!(split_range(100, Some(99)), (0..99, Some(99..100)));
}


#[test]
fn should_return_runtime_version() {
	let core = tokio::runtime::Runtime::new().unwrap();

	let client = Arc::new(test_client::new());
	let api = new_full(client.clone(), Subscriptions::new(Arc::new(core.executor())));

	let result = "{\"specName\":\"test\",\"implName\":\"parity-test\",\"authoringVersion\":1,\
		\"specVersion\":1,\"implVersion\":1,\"apis\":[[\"0xdf6acb689907609b\",2],\
		[\"0x37e397fc7c91f5e4\",1],[\"0xd2bc9897eed08f15\",1],[\"0x40fe3ad401f8959a\",3],\
		[\"0xc6e9a76309f39b09\",1],[\"0xdd718d5cc53262d4\",1],[\"0xcbca25e39f142387\",1],\
		[\"0xf78b278be53f454c\",1],[\"0xab3c0572291feb8b\",1]]}";

	let runtime_version = api.runtime_version(None.into()).wait().unwrap();
	let serialized = serde_json::to_string(&runtime_version).unwrap();
	assert_eq!(serialized, result);

	let deserialized: RuntimeVersion = serde_json::from_str(result).unwrap();
	assert_eq!(deserialized, runtime_version);
}

#[test]
fn should_notify_on_runtime_version_initially() {
	let mut core = tokio::runtime::Runtime::new().unwrap();
	let (subscriber, id, transport) = Subscriber::new_test("test");

	{
		let client = Arc::new(test_client::new());
		let api = new_full(client.clone(), Subscriptions::new(Arc::new(core.executor())));

		api.subscribe_runtime_version(Default::default(), subscriber);

		// assert id assigned
		assert_eq!(core.block_on(id), Ok(Ok(SubscriptionId::Number(1))));
	}

	// assert initial version sent.
	let (notification, next) = core.block_on(transport.into_future()).unwrap();
	assert!(notification.is_some());
		// no more notifications on this channel
	assert_eq!(core.block_on(next.into_future()).unwrap().0, None);
}
