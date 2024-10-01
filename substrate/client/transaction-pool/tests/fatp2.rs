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

// single file is just too big. re-use some common components and silence warnings.
#![allow(unused_imports)]
#![allow(dead_code)]

//! Tests for fork-aware transaction pool.

mod fatp_common;
use fatp_common::*;

#[test]
fn fatp_limits_watcher_xxx2() {
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

	let mut ready_iterator = pool.ready_at(header01.hash()).now_or_never().unwrap();
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt1).0);
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt2).0);
	assert!(ready_iterator.next().is_none());

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 2, 0);

	let mut ready_iterator = pool.ready_at(header02.hash()).now_or_never().unwrap();
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt2).0);
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt0).0);
	assert!(ready_iterator.next().is_none());

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt0_status, vec![TransactionStatus::Ready]);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();
	assert_eq!(xt1_status, vec![TransactionStatus::Ready, TransactionStatus::Dropped]);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt2_status, vec![TransactionStatus::Ready,]);
}

#[test]
fn fatp_limits_watcher_xxx3() {
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
	let mut ready_iterator = pool.ready_at(header01.hash()).now_or_never().unwrap();
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt1).0);
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt2).0);
	assert!(ready_iterator.next().is_none());

	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 2, 0);
	let mut ready_iterator = pool.ready_at(header02.hash()).now_or_never().unwrap();
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt1).0);
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt2).0);
	assert!(ready_iterator.next().is_none());

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
	assert_eq!(xt2_status, vec![TransactionStatus::Ready,]);
}

#[test]
fn fatp_limits_watcher_xxx4() {
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
	let mut ready_iterator = pool.ready_at(header01.hash()).now_or_never().unwrap();
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt1).0);
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt2).0);
	assert!(ready_iterator.next().is_none());

	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));

	let submission = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone()));
	let xt3_watcher = submission.unwrap();

	assert_pool_status!(header02.hash(), &pool, 2, 0);
	let mut ready_iterator = pool.ready_at(header02.hash()).now_or_never().unwrap();
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt2).0);
	assert_eq!(ready_iterator.next().unwrap().hash, api.hash_and_length(&xt3).0);
	assert!(ready_iterator.next().is_none());

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
	assert_eq!(xt2_status, vec![TransactionStatus::Ready,]);

	let xt3_status = futures::executor::block_on_stream(xt3_watcher).take(1).collect::<Vec<_>>();
	assert_eq!(xt3_status, vec![TransactionStatus::Ready,]);
}
