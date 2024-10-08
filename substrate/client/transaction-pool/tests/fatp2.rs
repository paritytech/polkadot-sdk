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
use substrate_test_runtime_client::AccountKeyring::*;
use substrate_test_runtime_transaction_pool::uxt;

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

	//branch with alice transactions:
	let header02b = api.push_block(2, vec![xt1.clone(), xt2.clone()], true);
	let event = new_best_block_event(&pool, Some(header01.hash()), header02b.hash());
	block_on(pool.maintain(event));
	assert_eq!(pool.mempool_len().0, 3);
	//charlie was resubmitted from mmepool into the view:
	assert_pool_status!(header02b.hash(), &pool, 1, 0);
	assert_ready_iterator!(header02b.hash(), pool, [xt0]);

	//branch with alice/charlie transactions shall also work:
	let header02a = api.push_block(2, vec![xt0.clone(), xt1.clone()], true);
	let event = new_best_block_event(&pool, Some(header02b.hash()), header02a.hash());
	block_on(pool.maintain(event));
	assert_eq!(pool.mempool_len().0, 3);
	assert_pool_status!(header02a.hash(), &pool, 1, 0);
	assert_ready_iterator!(header02a.hash(), pool, [xt2]);
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

	let submissions = vec![
		pool.submit_one(header01.hash(), SOURCE, xt1.clone()),
		pool.submit_one(header01.hash(), SOURCE, xt2.clone()),
		pool.submit_one(header01.hash(), SOURCE, xt3.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));
	assert!(results.iter().all(Result::is_ok));
	//charlie was not included into view due to limits:
	assert_pool_status!(header01.hash(), &pool, 0, 2);

	let header02 = api.push_block(2, vec![xt0], true);
	api.set_nonce(header02.hash(), Alice.into(), 201); //redundant
	let event = new_best_block_event(&pool, Some(header01.hash()), header02.hash());
	block_on(pool.maintain(event));

	//charlie was resubmitted from mmepool into the view:
	assert_pool_status!(header02.hash(), &pool, 2, 1);
	assert_eq!(pool.mempool_len().0, 3);
}

#[test]
fn fatp_limits_watcher_mempool_prevents_dropping() {
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

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone()),
	];
	let mut submissions = block_on(futures::future::join_all(submissions));
	let xt2_watcher = submissions.remove(2).unwrap();
	let xt1_watcher = submissions.remove(1).unwrap();
	let xt0_watcher = submissions.remove(0).unwrap();

	assert_pool_status!(header01.hash(), &pool, 2, 0);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(1).collect::<Vec<_>>();

	log::debug!("xt0_status: {:#?}", xt0_status);

	assert_eq!(xt0_status, vec![TransactionStatus::Ready]);
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();

	assert_eq!(xt1_status, vec![TransactionStatus::Ready]);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	log::debug!("xt2_status: {:#?}", xt2_status);

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

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone()),
	];
	let mut submissions = block_on(futures::future::join_all(submissions));
	let xt2_watcher = submissions.remove(2).unwrap();
	let xt1_watcher = submissions.remove(1).unwrap();
	let xt0_watcher = submissions.remove(0).unwrap();

	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt2, xt0]);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt0_status, vec![TransactionStatus::Ready]);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

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

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone()),
	];
	let mut submissions = block_on(futures::future::join_all(submissions));
	let xt2_watcher = submissions.remove(2).unwrap();
	let xt1_watcher = submissions.remove(1).unwrap();
	let xt0_watcher = submissions.remove(0).unwrap();
	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt1, xt2]);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(3).collect::<Vec<_>>();
	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0))
		]
	);

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

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone()),
	];
	let mut submissions = block_on(futures::future::join_all(submissions));
	let xt2_watcher = submissions.remove(2).unwrap();
	let xt1_watcher = submissions.remove(1).unwrap();
	let xt0_watcher = submissions.remove(0).unwrap();

	assert_ready_iterator!(header01.hash(), pool, [xt1, xt2]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));

	let submission = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone()));
	let xt3_watcher = submission.unwrap();

	assert_pool_status!(header02.hash(), pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt2, xt3]);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(3).collect::<Vec<_>>();
	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0))
		]
	);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt2_status, vec![TransactionStatus::Ready]);

	let xt3_status = futures::executor::block_on_stream(xt3_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt3_status, vec![TransactionStatus::Ready]);
}
