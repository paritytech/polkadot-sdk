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

//! Tests of priorities for fork-aware transaction pool.

pub mod fatp_common;

use fatp_common::{invalid_hash, new_best_block_event, TestPoolBuilder, LOG_TARGET, SOURCE};
use futures::{executor::block_on, FutureExt};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{
	error::Error as TxPoolError, LocalTransactionPool, MaintainedTransactionPool, TransactionPool,
	TransactionStatus,
};
use substrate_test_runtime_client::Sr25519Keyring::*;
use substrate_test_runtime_transaction_pool::uxt;
use tracing::info;
#[test]
fn fatp_prio_ready_higher_evicts_lower() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 200);

	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 3);

	let result0 = block_on(pool.submit_one(header01.hash(), SOURCE, xt0.clone()));
	let result1 = block_on(pool.submit_one(header01.hash(), SOURCE, xt1.clone()));

	info!(target: LOG_TARGET, ?result0, "r0");
	info!(target: LOG_TARGET, ?result1, "r1");
	info!(target: LOG_TARGET, len = ?pool.mempool_len(), "len");
	info!(target: LOG_TARGET, status = ?pool.status_all()[&header01.hash()], "len");
	assert_ready_iterator!(header01.hash(), pool, [xt1]);
	assert_pool_status!(header01.hash(), &pool, 1, 0);
}

#[test]
fn fatp_prio_watcher_ready_higher_evicts_lower() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 200);

	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 3);

	let xt0_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt1.clone())).unwrap();

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(
		xt0_status,
		vec![TransactionStatus::Ready, TransactionStatus::Usurped(api.hash_and_length(&xt1).0)]
	);
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready]);

	info!(target: LOG_TARGET, len = ?pool.mempool_len(), "len");
	info!(target: LOG_TARGET, pool_status = ?pool.status_all()[&header01.hash()], "len");
	assert_ready_iterator!(header01.hash(), pool, [xt1]);
	assert_pool_status!(header01.hash(), &pool, 1, 0);
}

#[test]
fn fatp_prio_watcher_future_higher_evicts_lower() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(3).build();

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 201);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 200);

	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 3);

	let xt0_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt2.clone())).unwrap();

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();

	assert_eq!(
		xt0_status,
		vec![TransactionStatus::Future, TransactionStatus::Usurped(api.hash_and_length(&xt2).0)]
	);
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Future, TransactionStatus::Ready]);
	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt2_status, vec![TransactionStatus::Ready]);

	assert_eq!(pool.mempool_len().1, 2);
	assert_ready_iterator!(header01.hash(), pool, [xt2, xt1]);
	assert_pool_status!(header01.hash(), &pool, 2, 0);
}

#[test]
fn fatp_prio_watcher_ready_lower_prio_gets_dropped_from_all_views() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 200);

	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 3);

	let xt0_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt0.clone())).unwrap();

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	let header03a = api.push_block_with_parent(header02.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header03a.hash())));

	let header03b = api.push_block_with_parent(header02.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header03a.hash()), header03b.hash())));

	assert_pool_status!(header03a.hash(), &pool, 1, 0);
	assert_ready_iterator!(header03a.hash(), pool, [xt0]);
	assert_pool_status!(header03b.hash(), &pool, 1, 0);
	assert_ready_iterator!(header03b.hash(), pool, [xt0]);
	assert_ready_iterator!(header01.hash(), pool, [xt0]);
	assert_ready_iterator!(header02.hash(), pool, [xt0]);

	let xt1_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt1.clone())).unwrap();

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready]);
	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(
		xt0_status,
		vec![TransactionStatus::Ready, TransactionStatus::Usurped(api.hash_and_length(&xt1).0)]
	);
	assert_ready_iterator!(header03a.hash(), pool, [xt1]);
	assert_ready_iterator!(header03b.hash(), pool, [xt1]);
	assert_ready_iterator!(header01.hash(), pool, [xt1]);
	assert_ready_iterator!(header02.hash(), pool, [xt1]);
}

#[test]
fn fatp_prio_watcher_future_lower_prio_gets_dropped_from_all_views() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(3).with_ready_count(2).build();

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 201);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 200);

	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 3);

	let xt0_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt0.clone())).unwrap();

	let xt1_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt1.clone())).unwrap();

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	let header03a = api.push_block_with_parent(header02.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header03a.hash())));

	let header03b = api.push_block_with_parent(header02.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header03a.hash()), header03b.hash())));

	assert_pool_status!(header03a.hash(), &pool, 0, 2);
	assert_future_iterator!(header03a.hash(), pool, [xt0, xt1]);
	assert_pool_status!(header03b.hash(), &pool, 0, 2);
	assert_future_iterator!(header03b.hash(), pool, [xt0, xt1]);
	assert_future_iterator!(header01.hash(), pool, [xt0, xt1]);
	assert_future_iterator!(header02.hash(), pool, [xt0, xt1]);

	let xt2_watcher =
		block_on(pool.submit_and_watch(header01.hash(), SOURCE, xt2.clone())).unwrap();

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt2_status, vec![TransactionStatus::Ready]);
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Future]);
	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(
		xt0_status,
		vec![TransactionStatus::Future, TransactionStatus::Usurped(api.hash_and_length(&xt2).0)]
	);
	assert_future_iterator!(header03a.hash(), pool, []);
	assert_future_iterator!(header03b.hash(), pool, []);
	assert_future_iterator!(header01.hash(), pool, []);
	assert_future_iterator!(header02.hash(), pool, []);

	assert_ready_iterator!(header03a.hash(), pool, [xt2, xt1]);
	assert_ready_iterator!(header03b.hash(), pool, [xt2, xt1]);
	assert_ready_iterator!(header01.hash(), pool, [xt2, xt1]);
	assert_ready_iterator!(header02.hash(), pool, [xt2, xt1]);
}

#[test]
fn fatp_prios_watcher_full_mempool_higher_prio_is_accepted() {
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

	api.set_priority(&xt0, 1);
	api.set_priority(&xt1, 2);
	api.set_priority(&xt2, 3);
	api.set_priority(&xt3, 4);

	api.set_priority(&xt4, 5);
	api.set_priority(&xt5, 6);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();

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
	assert_eq!(pool.mempool_len().1, 4);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);

	assert_ready_iterator!(header01.hash(), pool, []);
	assert_ready_iterator!(header02.hash(), pool, [xt3, xt2]);
	assert_ready_iterator!(header03.hash(), pool, [xt5, xt4]);
}

#[test]
fn fatp_prios_watcher_full_mempool_higher_prio_is_accepted_with_subtree() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(4).with_ready_count(4).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 202);
	let xt3 = uxt(Bob, 300);
	let xt4 = uxt(Charlie, 400);

	api.set_priority(&xt0, 1);
	api.set_priority(&xt1, 3);
	api.set_priority(&xt2, 3);
	api.set_priority(&xt3, 2);
	api.set_priority(&xt4, 2);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_ready_iterator!(header01.hash(), pool, [xt3, xt0, xt1, xt2]);
	assert_pool_status!(header01.hash(), &pool, 4, 0);
	assert_eq!(pool.mempool_len().1, 4);

	let xt4_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();
	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt3, xt4]);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt4_watcher, [TransactionStatus::Ready]);
}

#[test]
fn fatp_prios_watcher_full_mempool_higher_prio_is_accepted_with_subtree2() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(4).with_ready_count(4).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 202);
	let xt3 = uxt(Bob, 300);
	let xt4 = uxt(Charlie, 400);

	api.set_priority(&xt0, 1);
	api.set_priority(&xt1, 3);
	api.set_priority(&xt2, 3);
	api.set_priority(&xt3, 2);
	api.set_priority(&xt4, 2);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_ready_iterator!(header01.hash(), pool, [xt3, xt0, xt1, xt2]);
	assert_pool_status!(header01.hash(), &pool, 4, 0);
	assert_eq!(pool.mempool_len().1, 4);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	let xt4_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();
	assert_ready_iterator!(header01.hash(), pool, [xt3]);
	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt3, xt4]);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt4_watcher, [TransactionStatus::Ready]);
}

#[test]
fn fatp_prios_watcher_full_mempool_lower_prio_gets_rejected() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(2).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);
	let xt3 = uxt(Dave, 500);

	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 2);
	api.set_priority(&xt2, 2);
	api.set_priority(&xt3, 1);

	let _xt0_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let _xt1_watcher =
		block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().1, 2);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().1, 2);

	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1]);
	assert_ready_iterator!(header02.hash(), pool, [xt0, xt1]);

	let result2 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).map(|_| ());
	assert!(matches!(result2.as_ref().unwrap_err().0, TxPoolError::ImmediatelyDropped));
	let result3 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).map(|_| ());
	assert!(matches!(result3.as_ref().unwrap_err().0, TxPoolError::ImmediatelyDropped));
}

#[test]
fn fatp_prios_watcher_full_mempool_does_not_keep_dropped_transaction() {
	sp_tracing::try_init_simple();

	let builder = TestPoolBuilder::new();
	let (pool, api, _) = builder.with_mempool_count_limit(4).with_ready_count(2).build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);
	let xt3 = uxt(Dave, 500);

	api.set_priority(&xt0, 2);
	api.set_priority(&xt1, 2);
	api.set_priority(&xt2, 2);
	api.set_priority(&xt3, 2);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt2, xt3]);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready, TransactionStatus::Dropped]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
}

#[test]
fn fatp_prios_submit_local_full_mempool_higher_prio_is_accepted() {
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

	api.set_priority(&xt0, 1);
	api.set_priority(&xt1, 2);
	api.set_priority(&xt2, 3);
	api.set_priority(&xt3, 4);

	api.set_priority(&xt4, 5);
	api.set_priority(&xt5, 6);
	pool.submit_local(invalid_hash(), xt0.clone()).unwrap();
	pool.submit_local(invalid_hash(), xt1.clone()).unwrap();

	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().0, 2);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	pool.submit_local(invalid_hash(), xt2.clone()).unwrap();
	pool.submit_local(invalid_hash(), xt3.clone()).unwrap();

	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().0, 4);

	let header03 = api.push_block_with_parent(header02.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02.hash()), header03.hash())));

	pool.submit_local(invalid_hash(), xt4.clone()).unwrap();
	pool.submit_local(invalid_hash(), xt5.clone()).unwrap();

	assert_pool_status!(header03.hash(), &pool, 2, 0);
	assert_eq!(pool.mempool_len().0, 4);

	assert_ready_iterator!(header01.hash(), pool, []);
	assert_ready_iterator!(header02.hash(), pool, [xt3, xt2]);
	assert_ready_iterator!(header03.hash(), pool, [xt5, xt4]);
}
