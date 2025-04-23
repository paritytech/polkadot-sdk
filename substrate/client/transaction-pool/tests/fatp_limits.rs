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

//! Tests of limits for fork-aware transaction pool.

pub mod fatp_common;

use fatp_common::{
	finalized_block_event, invalid_hash, new_best_block_event, TestPoolBuilder, LOG_TARGET, SOURCE,
};
use futures::{executor::block_on, FutureExt};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{
	error::Error as TxPoolError, MaintainedTransactionPool, TransactionPool, TransactionStatus,
};
use std::thread::sleep;
use substrate_test_runtime_client::Sr25519Keyring::*;
use substrate_test_runtime_transaction_pool::uxt;
use tracing::debug;

#[test]
fn fatp_limits_no_views_mempool_count() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(2).build();

	let header = api.push_block(1, vec![], true);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(header.hash(), SOURCE, xt0.clone()),
		pool.submit_one(header.hash(), SOURCE, xt1.clone()),
		pool.submit_one(header.hash(), SOURCE, xt2.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));
	let mut results = results.iter();

	assert!(results.next().unwrap().is_ok());
	assert!(results.next().unwrap().is_ok());
	assert!(matches!(
		results.next().unwrap().as_ref().unwrap_err().0,
		TxPoolError::ImmediatelyDropped
	));
}

#[test]
fn fatp_limits_ready_count_works() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 500);

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	//note: we need Charlie to be first as the oldest is removed.
	//For 3x alice, all tree would be removed.
	//(alice,bob,charlie would work too)
	let xt0 = uxt(Charlie, 500);
	let xt1 = uxt(Alice, 200);
	let xt2 = uxt(Alice, 201);

	let submissions = vec![
		pool.submit_one(header01.hash(), SOURCE, xt0.clone()),
		pool.submit_one(header01.hash(), SOURCE, xt1.clone()),
		pool.submit_one(header01.hash(), SOURCE, xt2.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));
	assert!(results.iter().all(Result::is_ok));
	//charlie was not included into view:
	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);
	//todo: can we do better? We don't have API to check if event was processed internally.
	let mut counter = 0;
	while pool.mempool_len().0 == 3 {
		sleep(std::time::Duration::from_millis(1));
		counter = counter + 1;
		if counter > 20 {
			assert!(false, "timeout");
		}
	}
	assert_eq!(pool.mempool_len().0, 2);

	//branch with alice transactions:
	let header02b = api.push_block(2, vec![xt1.clone(), xt2.clone()], true);
	let event = new_best_block_event(&pool, Some(header01.hash()), header02b.hash());
	block_on(pool.maintain(event));
	assert_eq!(pool.mempool_len().0, 2);
	assert_pool_status!(header02b.hash(), &pool, 0, 0);
	assert_ready_iterator!(header02b.hash(), pool, []);

	//branch with alice/charlie transactions shall also work:
	let header02a = api.push_block(2, vec![xt0.clone(), xt1.clone()], true);
	api.set_nonce(header02a.hash(), Alice.into(), 201);
	let event = new_best_block_event(&pool, Some(header02b.hash()), header02a.hash());
	block_on(pool.maintain(event));
	assert_eq!(pool.mempool_len().0, 2);
	// assert_pool_status!(header02a.hash(), &pool, 1, 0);
	assert_ready_iterator!(header02a.hash(), pool, [xt2]);
}

#[test]
fn fatp_limits_ready_count_works_for_submit_at() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 500);

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Charlie, 500);
	let xt1 = uxt(Alice, 200);
	let xt2 = uxt(Alice, 201);

	let results = block_on(pool.submit_at(
		header01.hash(),
		SOURCE,
		vec![xt0.clone(), xt1.clone(), xt2.clone()],
	))
	.unwrap();

	assert!(matches!(results[0].as_ref().unwrap_err().0, TxPoolError::ImmediatelyDropped));
	assert!(results[1].as_ref().is_ok());
	assert!(results[2].as_ref().is_ok());
	assert_eq!(pool.mempool_len().0, 2);
	//charlie was not included into view:
	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);
}

#[test]
fn fatp_limits_ready_count_works_for_submit_and_watch() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 500);

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Charlie, 500);
	let xt1 = uxt(Alice, 200);
	let xt2 = uxt(Bob, 300);
	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 2);
	api.set_priority(&xt2, 1);

	let result0 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()));
	let result1 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()));
	let result2 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).map(|_| ());

	assert!(matches!(result2.unwrap_err().0, TxPoolError::ImmediatelyDropped));
	assert!(result0.is_ok());
	assert!(result1.is_ok());
	assert_eq!(pool.mempool_len().1, 2);
	//charlie was not included into view:
	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1]);
}

#[test]
fn fatp_limits_future_count_works() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_future_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 500);

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);

	let xt1 = uxt(Charlie, 501);
	let xt2 = uxt(Alice, 201);
	let xt3 = uxt(Alice, 202);

	block_on(pool.submit_one(header01.hash(), SOURCE, xt1.clone())).unwrap();
	block_on(pool.submit_one(header01.hash(), SOURCE, xt2.clone())).unwrap();
	block_on(pool.submit_one(header01.hash(), SOURCE, xt3.clone())).unwrap();

	//charlie was not included into view due to limits:
	assert_pool_status!(header01.hash(), &pool, 0, 2);
	//todo: can we do better? We don't have API to check if event was processed internally.
	let mut counter = 0;
	while pool.mempool_len().0 != 2 {
		sleep(std::time::Duration::from_millis(1));
		counter = counter + 1;
		if counter > 20 {
			assert!(false, "timeout");
		}
	}

	let header02 = api.push_block(2, vec![xt0], true);
	api.set_nonce(header02.hash(), Alice.into(), 201); //redundant
	let event = new_best_block_event(&pool, Some(header01.hash()), header02.hash());
	block_on(pool.maintain(event));

	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().0, 2);
}

#[test]
fn fatp_limits_watcher_mempool_doesnt_prevent_dropping() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Charlie, 400);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Alice, 200);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 2, 0);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	debug!(target: LOG_TARGET, ?xt0_status, "xt0_status");
	assert_eq!(xt0_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();

	assert_eq!(xt1_status, vec![TransactionStatus::Ready]);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	debug!(target: LOG_TARGET, ?xt2_status, "xt2_status");

	assert_eq!(xt2_status, vec![TransactionStatus::Ready]);
}

#[test]
fn fatp_limits_watcher_non_intial_view_drops_transaction() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Dave, 500);
	let xt1 = uxt(Charlie, 400);
	let xt2 = uxt(Bob, 300);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();

	// make sure tx0 is actually dropped before checking iterator
	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt0_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt1, xt2]);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready]);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt2_status, vec![TransactionStatus::Ready]);
}

#[test]
fn fatp_limits_watcher_finalized_transaction_frees_ready_space() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Dave, 500);
	let xt1 = uxt(Charlie, 400);
	let xt2 = uxt(Bob, 300);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt0_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt1, xt2]);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready]);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt2_status, vec![TransactionStatus::Ready]);
}

#[test]
fn fatp_limits_watcher_view_can_drop_transcation() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Dave, 500);
	let xt1 = uxt(Charlie, 400);
	let xt2 = uxt(Bob, 300);
	let xt3 = uxt(Alice, 200);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt0_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped,]);

	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	assert_pool_status!(header02.hash(), pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt2, xt3]);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt2_status, vec![TransactionStatus::Ready]);

	let xt3_status = futures::executor::block_on_stream(xt3_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt3_status, vec![TransactionStatus::Ready]);
}

#[test]
fn fatp_limits_watcher_empty_and_full_view_immediately_drops() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(4).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);
	api.set_nonce(api.genesis_hash(), Eve.into(), 600);
	api.set_nonce(api.genesis_hash(), Ferdie.into(), 700);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);

	let xt3 = uxt(Dave, 500);
	let xt4 = uxt(Eve, 600);
	let xt5 = uxt(Ferdie, 700);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt0_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().1, 2);

	let header02e = api.push_block_with_parent(
		header01.hash(),
		vec![xt0.clone(), xt1.clone(), xt2.clone()],
		true,
	);
	api.set_nonce(header02e.hash(), Alice.into(), 201);
	api.set_nonce(header02e.hash(), Bob.into(), 301);
	api.set_nonce(header02e.hash(), Charlie.into(), 401);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02e.hash())));

	assert_pool_status!(header02e.hash(), &pool, 0, 0);

	let header02f = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02f.hash())));
	assert_pool_status!(header02f.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02f.hash(), pool, [xt1, xt2]);

	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();
	let xt4_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();
	let result5 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt5.clone())).map(|_| ());

	//xt5 hits internal mempool limit
	assert!(matches!(result5.unwrap_err().0, TxPoolError::ImmediatelyDropped));

	assert_pool_status!(header02e.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02e.hash(), pool, [xt3, xt4]);
	assert_eq!(pool.mempool_len().1, 4);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(
		xt1_status,
		vec![TransactionStatus::Ready, TransactionStatus::InBlock((header02e.hash(), 1))]
	);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(
		xt2_status,
		vec![TransactionStatus::Ready, TransactionStatus::InBlock((header02e.hash(), 2))]
	);

	let xt3_status = futures::executor::block_on_stream(xt3_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt3_status, vec![TransactionStatus::Ready]);
	let xt4_status = futures::executor::block_on_stream(xt4_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt4_status, vec![TransactionStatus::Ready]);
}

#[test]
fn fatp_limits_watcher_empty_and_full_view_drops_with_event() {
	// it is almost copy of fatp_limits_watcher_empty_and_full_view_immediately_drops, but the
	// mempool_count limit is set to 5 (vs 4).
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(5).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);
	api.set_nonce(api.genesis_hash(), Eve.into(), 600);
	api.set_nonce(api.genesis_hash(), Ferdie.into(), 700);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);

	let xt3 = uxt(Dave, 500);
	let xt4 = uxt(Eve, 600);
	let xt5 = uxt(Ferdie, 700);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt0_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().1, 2);

	let header02e = api.push_block_with_parent(
		header01.hash(),
		vec![xt0.clone(), xt1.clone(), xt2.clone()],
		true,
	);
	api.set_nonce(header02e.hash(), Alice.into(), 201);
	api.set_nonce(header02e.hash(), Bob.into(), 301);
	api.set_nonce(header02e.hash(), Charlie.into(), 401);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02e.hash())));

	assert_pool_status!(header02e.hash(), &pool, 0, 0);

	let header02f = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02f.hash())));
	assert_pool_status!(header02f.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02f.hash(), pool, [xt1, xt2]);

	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();
	let xt4_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();
	let xt5_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt5.clone())).unwrap();

	assert_pool_status!(header02e.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02e.hash(), pool, [xt4, xt5]);

	let xt3_status = futures::executor::block_on_stream(xt3_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt3_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	//xt5 got dropped
	assert_eq!(pool.mempool_len().1, 4);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(
		xt1_status,
		vec![TransactionStatus::Ready, TransactionStatus::InBlock((header02e.hash(), 1))]
	);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(
		xt2_status,
		vec![TransactionStatus::Ready, TransactionStatus::InBlock((header02e.hash(), 2))]
	);

	let xt4_status = futures::executor::block_on_stream(xt4_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt4_status, vec![TransactionStatus::Ready]);

	let xt5_status = futures::executor::block_on_stream(xt5_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt5_status, vec![TransactionStatus::Ready]);
}

fn large_uxt(x: usize) -> substrate_test_runtime::Extrinsic {
	substrate_test_runtime::ExtrinsicBuilder::new_include_data(vec![x as u8; 1024]).build()
}

#[test]
fn fatp_limits_ready_size_works() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_ready_bytes_size(3390).with_future_bytes_size(0).build();

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = large_uxt(0);
	let xt1 = large_uxt(1);
	let xt2 = large_uxt(2);

	let submissions = vec![
		pool.submit_one(header01.hash(), SOURCE, xt0.clone()),
		pool.submit_one(header01.hash(), SOURCE, xt1.clone()),
		pool.submit_one(header01.hash(), SOURCE, xt2.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));
	assert!(results.iter().all(Result::is_ok));
	//charlie was not included into view:
	assert_pool_status!(header01.hash(), &pool, 3, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1, xt2]);

	let xt3 = large_uxt(3);
	let result3 = block_on(pool.submit_one(header01.hash(), SOURCE, xt3.clone()));
	assert!(matches!(result3.as_ref().unwrap_err().0, TxPoolError::ImmediatelyDropped));
}

#[test]
fn fatp_limits_future_size_works() {
	sp_tracing::try_init_simple();
	const UXT_SIZE: usize = 137;

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder
		.with_ready_bytes_size(UXT_SIZE)
		.with_future_bytes_size(3 * UXT_SIZE)
		.build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 500);

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 201);
	let xt1 = uxt(Charlie, 501);
	let xt2 = uxt(Alice, 201);
	let xt3 = uxt(Alice, 202);
	assert_eq!(api.hash_and_length(&xt0).1, UXT_SIZE);
	assert_eq!(api.hash_and_length(&xt1).1, UXT_SIZE);
	assert_eq!(api.hash_and_length(&xt2).1, UXT_SIZE);
	assert_eq!(api.hash_and_length(&xt3).1, UXT_SIZE);

	let _ = block_on(pool.submit_one(header01.hash(), SOURCE, xt0.clone())).unwrap();
	let _ = block_on(pool.submit_one(header01.hash(), SOURCE, xt1.clone())).unwrap();
	let _ = block_on(pool.submit_one(header01.hash(), SOURCE, xt2.clone())).unwrap();
	let _ = block_on(pool.submit_one(header01.hash(), SOURCE, xt3.clone())).unwrap();

	//todo: can we do better? We don't have API to check if event was processed internally.
	let mut counter = 0;
	while pool.mempool_len().0 == 4 {
		sleep(std::time::Duration::from_millis(1));
		counter = counter + 1;
		if counter > 20 {
			assert!(false, "timeout");
		}
	}
	assert_pool_status!(header01.hash(), &pool, 0, 3);
	assert_eq!(pool.mempool_len().0, 3);
}

#[test]
fn fatp_limits_watcher_ready_transactions_are_not_droped_when_view_is_dropped() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(6).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);
	api.set_nonce(api.genesis_hash(), Eve.into(), 600);
	api.set_nonce(api.genesis_hash(), Ferdie.into(), 700);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);

	let xt3 = uxt(Dave, 500);
	let xt4 = uxt(Eve, 600);
	let xt5 = uxt(Ferdie, 700);

	let _xt0_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let _xt1_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().1, 2);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	let _xt2_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let _xt3_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().1, 4);

	let header03 = api.push_block_with_parent(header02.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02.hash()), header03.hash())));

	let _xt4_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();
	let _xt5_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt5.clone())).unwrap();

	assert_pool_status!(header03.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().1, 6);

	let header04 =
		api.push_block_with_parent(header03.hash(), vec![xt4.clone(), xt5.clone()], true);
	api.set_nonce(header04.hash(), Alice.into(), 201);
	api.set_nonce(header04.hash(), Bob.into(), 301);
	api.set_nonce(header04.hash(), Charlie.into(), 401);
	api.set_nonce(header04.hash(), Dave.into(), 501);
	api.set_nonce(header04.hash(), Eve.into(), 601);
	api.set_nonce(header04.hash(), Ferdie.into(), 701);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header03.hash()), header04.hash())));

	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1]);
	assert_ready_iterator!(header02.hash(), pool, [xt2, xt3]);
	assert_ready_iterator!(header03.hash(), pool, [xt4, xt5]);
	assert_ready_iterator!(header04.hash(), pool, []);

	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header01.hash())));
	assert!(!pool.status_all().contains_key(&header01.hash()));

	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header02.hash())));
	assert!(!pool.status_all().contains_key(&header02.hash()));

	//view 01 was dropped
	assert!(pool.ready_at(header01.hash()).now_or_never().is_none());
	assert_eq!(pool.mempool_len().1, 6);

	block_on(pool.maintain(finalized_block_event(&pool, header02.hash(), header03.hash())));

	//no revalidation has happened yet, all txs are kept
	assert_eq!(pool.mempool_len().1, 6);

	//view 03 is still there
	assert!(!pool.status_all().contains_key(&header03.hash()));

	//view 02 was dropped
	assert!(pool.ready_at(header02.hash()).now_or_never().is_none());

	let mut prev_header = header03;
	for n in 5..=11 {
		let header = api.push_block(n, vec![], true);
		let event = finalized_block_event(&pool, prev_header.hash(), header.hash());
		block_on(pool.maintain(event));
		prev_header = header;
	}

	//now revalidation has happened, all txs are dropped
	assert_eq!(pool.mempool_len().1, 0);
}

#[test]
fn fatp_limits_watcher_future_transactions_are_droped_when_view_is_dropped() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(6).with_future_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);
	api.set_nonce(api.genesis_hash(), Eve.into(), 600);
	api.set_nonce(api.genesis_hash(), Ferdie.into(), 700);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 201);
	let xt1 = uxt(Bob, 301);
	let xt2 = uxt(Charlie, 401);

	let xt3 = uxt(Dave, 501);
	let xt4 = uxt(Eve, 601);
	let xt5 = uxt(Ferdie, 701);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 0, 2);
	assert_eq!(pool.mempool_len().1, 2);
	assert_future_iterator!(header01.hash(), pool, [xt0, xt1]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_pool_status!(header02.hash(), &pool, 0, 2);
	assert_eq!(pool.mempool_len().1, 4);
	assert_future_iterator!(header02.hash(), pool, [xt2, xt3]);

	let header03 = api.push_block_with_parent(header02.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02.hash()), header03.hash())));

	let xt4_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();
	let xt5_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt5.clone())).unwrap();

	assert_pool_status!(header03.hash(), &pool, 0, 2);
	assert_eq!(pool.mempool_len().1, 6);
	assert_future_iterator!(header03.hash(), pool, [xt4, xt5]);

	let header04 = api.push_block_with_parent(header03.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header03.hash()), header04.hash())));

	assert_pool_status!(header04.hash(), &pool, 0, 2);
	assert_eq!(pool.futures().len(), 2);
	assert_future_iterator!(header04.hash(), pool, [xt4, xt5]);

	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header04.hash())));
	assert_eq!(pool.active_views_count(), 1);
	assert_eq!(pool.inactive_views_count(), 0);
	//todo: can we do better? We don't have API to check if event was processed internally.
	let mut counter = 0;
	while pool.mempool_len().1 != 2 {
		sleep(std::time::Duration::from_millis(1));
		counter = counter + 1;
		if counter > 20 {
			assert!(false, "timeout {}", pool.mempool_len().1);
		}
	}
	assert_eq!(pool.mempool_len().1, 2);
	assert_pool_status!(header04.hash(), &pool, 0, 2);
	assert_eq!(pool.futures().len(), 2);

	let to_be_checked = vec![xt0_watcher, xt1_watcher, xt2_watcher, xt3_watcher];
	for x in to_be_checked {
		let x_status = futures::executor::block_on_stream(x).take(2).collect::<Vec<_>>();
		assert_eq!(x_status, vec![TransactionStatus::Future, TransactionStatus::Dropped]);
	}

	let to_be_checked = vec![xt4_watcher, xt5_watcher];
	for x in to_be_checked {
		let x_status = futures::executor::block_on_stream(x).take(1).collect::<Vec<_>>();
		assert_eq!(x_status, vec![TransactionStatus::Future]);
	}
}
