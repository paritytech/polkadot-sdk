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

use fatp_common::{
	finalized_block_event, invalid_hash, new_best_block_event, TestPoolBuilder, LOG_TARGET, SOURCE,
};
use futures::{executor::block_on, FutureExt};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{
	error::Error as TxPoolError, MaintainedTransactionPool, TransactionPool, TransactionStatus,
};
use std::{thread::sleep, time::Duration};
use substrate_test_runtime_client::AccountKeyring::*;
use substrate_test_runtime_transaction_pool::uxt;

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

	log::info!("r0 => {:?}", result0);
	log::info!("r1 => {:?}", result1);
	log::info!("len: {:?}", pool.mempool_len());
	log::info!("len: {:?}", pool.status_all()[&header01.hash()]);
	assert_ready_iterator!(header01.hash(), pool, [xt1]);
	assert_pool_status!(header01.hash(), &pool, 1, 0);

	// let results = block_on(futures::future::join_all(submissions));
	// assert!(results.iter().all(Result::is_ok));
	// //charlie was not included into view:
	// assert_pool_status!(header01.hash(), &pool, 2, 0);

	// //branch with alice transactions:
	// let header02b = api.push_block(2, vec![xt1.clone(), xt2.clone()], true);
	// let event = new_best_block_event(&pool, Some(header01.hash()), header02b.hash());
	// block_on(pool.maintain(event));
	// assert_eq!(pool.mempool_len().0, 2);
	// assert_pool_status!(header02b.hash(), &pool, 0, 0);
	// assert_ready_iterator!(header02b.hash(), pool, []);
	//
	// //branch with alice/charlie transactions shall also work:
	// let header02a = api.push_block(2, vec![xt0.clone(), xt1.clone()], true);
	// api.set_nonce(header02a.hash(), Alice.into(), 201);
	// let event = new_best_block_event(&pool, Some(header02b.hash()), header02a.hash());
	// block_on(pool.maintain(event));
	// assert_eq!(pool.mempool_len().0, 2);
	// // assert_pool_status!(header02a.hash(), &pool, 1, 0);
	// assert_ready_iterator!(header02a.hash(), pool, [xt2]);
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
	assert_eq!(xt0_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready]);

	log::info!("len: {:?}", pool.mempool_len());
	log::info!("len: {:?}", pool.status_all()[&header01.hash()]);
	assert_ready_iterator!(header01.hash(), pool, [xt1]);
	assert_pool_status!(header01.hash(), &pool, 1, 0);

	// let results = block_on(futures::future::join_all(submissions));
	// assert!(results.iter().all(Result::is_ok));
	// //charlie was not included into view:
	// assert_pool_status!(header01.hash(), &pool, 2, 0);

	// //branch with alice transactions:
	// let header02b = api.push_block(2, vec![xt1.clone(), xt2.clone()], true);
	// let event = new_best_block_event(&pool, Some(header01.hash()), header02b.hash());
	// block_on(pool.maintain(event));
	// assert_eq!(pool.mempool_len().0, 2);
	// assert_pool_status!(header02b.hash(), &pool, 0, 0);
	// assert_ready_iterator!(header02b.hash(), pool, []);
	//
	// //branch with alice/charlie transactions shall also work:
	// let header02a = api.push_block(2, vec![xt0.clone(), xt1.clone()], true);
	// api.set_nonce(header02a.hash(), Alice.into(), 201);
	// let event = new_best_block_event(&pool, Some(header02b.hash()), header02a.hash());
	// block_on(pool.maintain(event));
	// assert_eq!(pool.mempool_len().0, 2);
	// // assert_pool_status!(header02a.hash(), &pool, 1, 0);
	// assert_ready_iterator!(header02a.hash(), pool, [xt2]);
}
