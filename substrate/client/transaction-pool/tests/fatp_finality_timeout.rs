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

//! Tests for finality timeout handling for fork-aware transaction pool.

pub mod fatp_common;

use std::cmp::min;

use fatp_common::{
	finalized_block_event, invalid_hash, new_best_block_event, TestPoolBuilder, LOG_TARGET, SOURCE,
};
use futures::{executor::block_on, FutureExt};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{MaintainedTransactionPool, TransactionPool, TransactionStatus};
use substrate_test_runtime_client::Sr25519Keyring::*;
use substrate_test_runtime_transaction_pool::uxt;

#[test]
fn fatp_finality_timeout_works() {
	sp_tracing::try_init_simple();

	const FINALITY_TIMEOUT_THRESHOLD: usize = 10;

	let (pool, api, _) = TestPoolBuilder::new()
		.with_finality_timeout_threshold(FINALITY_TIMEOUT_THRESHOLD)
		.build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);
	let xt3 = uxt(Dave, 500);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 4, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1, xt2, xt3]);

	let header02a = api.push_block_with_parent(
		header01.hash(),
		vec![xt0.clone(), xt1.clone(), xt2.clone(), xt3.clone()],
		true,
	);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));
	assert_pool_status!(header02a.hash(), &pool, 0, 0);

	let header02b = api.push_block_with_parent(header01.hash(), vec![xt0, xt1, xt2, xt3], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02a.hash()), header02b.hash())));
	assert_pool_status!(header02b.hash(), &pool, 0, 0);

	let mut prev_header = header02b.clone();
	for n in 3..66 {
		let header = api.push_block_with_parent(prev_header.hash(), vec![], true);
		let event = new_best_block_event(&pool, Some(prev_header.hash()), header.hash());
		block_on(pool.maintain(event));

		prev_header = header;
		if n < 3 + FINALITY_TIMEOUT_THRESHOLD {
			assert_eq!(pool.active_views_count(), 2);
		} else {
			assert_eq!(pool.active_views_count(), 1);
			assert_eq!(pool.inactive_views_count(), FINALITY_TIMEOUT_THRESHOLD);
		}
	}

	for (i, watcher) in
		vec![xt0_watcher, xt1_watcher, xt2_watcher, xt3_watcher].into_iter().enumerate()
	{
		assert_watcher_stream!(
			watcher,
			[
				TransactionStatus::Ready,
				TransactionStatus::InBlock((header02a.hash(), i)),
				TransactionStatus::InBlock((header02b.hash(), i)),
				TransactionStatus::FinalityTimeout(min(header02a.hash(), header02b.hash()))
			]
		);
	}
}

#[test]
fn fatp_finalized_still_works_after_finality_stall() {
	sp_tracing::try_init_simple();

	const FINALITY_TIMEOUT_THRESHOLD: usize = 10;

	let (pool, api, _) = TestPoolBuilder::new()
		.with_finality_timeout_threshold(FINALITY_TIMEOUT_THRESHOLD)
		.build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);
	let xt3 = uxt(Dave, 500);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 4, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1, xt2, xt3]);

	let header02a = api.push_block_with_parent(
		header01.hash(),
		vec![xt0.clone(), xt1.clone(), xt2.clone()],
		true,
	);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));
	assert_pool_status!(header02a.hash(), &pool, 1, 0);

	let header02b = api.push_block_with_parent(header01.hash(), vec![xt0, xt1, xt2], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02a.hash()), header02b.hash())));
	assert_pool_status!(header02b.hash(), &pool, 1, 0);

	let header03b = api.push_block_with_parent(header02b.hash(), vec![xt3], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02b.hash()), header03b.hash())));
	assert_pool_status!(header03b.hash(), &pool, 0, 0);

	let mut prev_header = header03b.clone();
	for block_n in 4..=3 + FINALITY_TIMEOUT_THRESHOLD {
		let header = api.push_block_with_parent(prev_header.hash(), vec![], true);
		let event = new_best_block_event(&pool, Some(prev_header.hash()), header.hash());
		block_on(pool.maintain(event));

		prev_header = header;
		if block_n == 3 + FINALITY_TIMEOUT_THRESHOLD {
			//finality timeout triggered
			assert_eq!(pool.active_views_count(), 1);
			assert_eq!(pool.inactive_views_count(), FINALITY_TIMEOUT_THRESHOLD);
		} else {
			assert_eq!(pool.active_views_count(), 2);
		}
	}

	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header03b.hash())));

	for (i, watcher) in vec![xt0_watcher, xt1_watcher, xt2_watcher].into_iter().enumerate() {
		assert_watcher_stream!(
			watcher,
			[
				TransactionStatus::Ready,
				TransactionStatus::InBlock((header02a.hash(), i)),
				TransactionStatus::InBlock((header02b.hash(), i)),
				TransactionStatus::FinalityTimeout(min(header02a.hash(), header02b.hash()))
			]
		);
	}

	assert_watcher_stream!(
		xt3_watcher,
		[
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03b.hash(), 0)),
			TransactionStatus::Finalized((header03b.hash(), 0))
		]
	);
}

#[test]
fn fatp_finality_timeout_works_for_txs_included_before_finalized() {
	sp_tracing::try_init_simple();

	const FINALITY_TIMEOUT_THRESHOLD: usize = 10;

	let (pool, api, _) = TestPoolBuilder::new()
		.with_finality_timeout_threshold(FINALITY_TIMEOUT_THRESHOLD)
		.build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);
	let xt3 = uxt(Dave, 500);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 4, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1, xt2, xt3]);

	let header02a = api.push_block_with_parent(
		header01.hash(),
		vec![xt0.clone(), xt1.clone(), xt2.clone()],
		true,
	);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));
	assert_pool_status!(header02a.hash(), &pool, 1, 0);

	let header02b = api.push_block_with_parent(header01.hash(), vec![xt0, xt1, xt2], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02a.hash()), header02b.hash())));
	assert_pool_status!(header02b.hash(), &pool, 1, 0);

	let header03b = api.push_block_with_parent(header02b.hash(), vec![xt3], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02b.hash()), header03b.hash())));
	assert_pool_status!(header03b.hash(), &pool, 0, 0);

	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header02b.hash())));

	let mut prev_header = header03b.clone();
	for block_n in 4..=4 + FINALITY_TIMEOUT_THRESHOLD {
		let header = api.push_block_with_parent(prev_header.hash(), vec![], true);
		let event = new_best_block_event(&pool, Some(prev_header.hash()), header.hash());
		block_on(pool.maintain(event));

		prev_header = header;
		assert_eq!(pool.active_views_count(), 1);
		if block_n == 4 + FINALITY_TIMEOUT_THRESHOLD {
			//finality timeout triggered
			assert_eq!(pool.inactive_views_count(), FINALITY_TIMEOUT_THRESHOLD);
		}
	}

	for (i, watcher) in vec![xt0_watcher, xt1_watcher, xt2_watcher].into_iter().enumerate() {
		assert_watcher_stream!(
			watcher,
			[
				TransactionStatus::Ready,
				TransactionStatus::InBlock((header02a.hash(), i)),
				TransactionStatus::InBlock((header02b.hash(), i)),
				TransactionStatus::Finalized((header02b.hash(), i))
			]
		);
	}

	assert_watcher_stream!(
		xt3_watcher,
		[
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03b.hash(), 0)),
			TransactionStatus::FinalityTimeout(header03b.hash())
		]
	);
}
