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

//! Tests of invalid transactions handling for fork-aware transaction pool.

pub mod fatp_common;

use fatp_common::{
	finalized_block_event, invalid_hash, new_best_block_event, pool, TestPoolBuilder, LOG_TARGET,
	SOURCE,
};
use futures::{executor::block_on, FutureExt};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{
	error::{Error as TxPoolError, IntoPoolError},
	MaintainedTransactionPool, TransactionPool, TransactionStatus,
};
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionValidityError};
use substrate_test_runtime_client::Sr25519Keyring::*;
use substrate_test_runtime_transaction_pool::uxt;

#[test]
fn fatp_invalid_three_views_stale_gets_rejected() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 200);

	let header02a = api.push_block_with_parent(header01.hash(), vec![], true);
	let header02b = api.push_block_with_parent(header01.hash(), vec![], true);
	let header02c = api.push_block_with_parent(header01.hash(), vec![], true);
	api.set_nonce(header02a.hash(), Alice.into(), 201);
	api.set_nonce(header02b.hash(), Alice.into(), 201);
	api.set_nonce(header02c.hash(), Alice.into(), 201);

	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02a.hash()), header02b.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02b.hash()), header02c.hash())));

	let result0 = block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone()));
	let result1 = block_on(pool.submit_one(invalid_hash(), SOURCE, xt1.clone()));

	assert!(matches!(
		result0.as_ref().unwrap_err().0,
		TxPoolError::InvalidTransaction(InvalidTransaction::Stale)
	));
	assert!(matches!(
		result1.as_ref().unwrap_err().0,
		TxPoolError::InvalidTransaction(InvalidTransaction::Stale)
	));
}

#[test]
fn fatp_invalid_three_views_invalid_gets_rejected() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 200);
	let header02a = api.push_block_with_parent(header01.hash(), vec![], true);
	let header02b = api.push_block_with_parent(header01.hash(), vec![], true);
	let header02c = api.push_block_with_parent(header01.hash(), vec![], true);

	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02a.hash()), header02b.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02b.hash()), header02c.hash())));

	api.add_invalid(&xt0);
	api.add_invalid(&xt1);

	let result0 = block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone()));
	let result1 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).map(|_| ());

	assert!(matches!(
		result0.as_ref().unwrap_err().0,
		TxPoolError::InvalidTransaction(InvalidTransaction::Custom(_))
	));
	assert!(matches!(
		result1.as_ref().unwrap_err().0,
		TxPoolError::InvalidTransaction(InvalidTransaction::Custom(_))
	));
}

#[test]
fn fatp_transactions_purging_invalid_on_finalization_works() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let xt1 = uxt(Alice, 200);
	let xt2 = uxt(Alice, 201);
	let xt3 = uxt(Alice, 202);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let watcher1 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let watcher2 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let watcher3 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_eq!(api.validation_requests().len(), 3);
	assert_eq!(pool.status_all()[&header01.hash()].ready, 3);
	assert_eq!(pool.mempool_len(), (0, 3));

	let header02 = api.push_block(2, vec![], true);
	api.add_invalid(&xt1);
	api.add_invalid(&xt2);
	api.add_invalid(&xt3);
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header02.hash())));

	// wait 10 blocks for revalidation
	let mut prev_header = header02.clone();
	for n in 3..=11 {
		let header = api.push_block(n, vec![], true);
		let event = finalized_block_event(&pool, prev_header.hash(), header.hash());
		block_on(pool.maintain(event));
		prev_header = header;
	}

	assert_eq!(pool.mempool_len(), (0, 0));

	assert_watcher_stream!(watcher1, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(watcher2, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(watcher3, [TransactionStatus::Ready, TransactionStatus::Invalid]);
}

#[test]
fn fatp_transactions_purging_invalid_on_finalization_works2() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();
	let xt1 = uxt(Alice, 200);
	let xt2 = uxt(Alice, 201);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt1.clone())).unwrap();

	let watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	assert_eq!(pool.status_all()[&header01.hash()].ready, 2);
	assert_eq!(api.validation_requests().len(), 2);

	let header02 = api.push_block(2, vec![xt1.clone()], true);
	api.add_invalid(&xt2);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));

	assert_eq!(pool.status_all()[&header02.hash()].ready, 1);

	// wait 10 blocks for revalidation
	let mut prev_header = header02.clone();
	for _ in 3..=11 {
		let header = api.push_block_with_parent(prev_header.hash(), vec![], true);
		let event = finalized_block_event(&pool, prev_header.hash(), header.hash());
		block_on(pool.maintain(event));
		prev_header = header;
	}

	assert_watcher_stream!(watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_eq!(pool.status_all()[&prev_header.hash()].ready, 0);
}

#[test]
fn should_not_retain_invalid_hashes_from_retracted() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();
	let xt = uxt(Alice, 200);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));
	let watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt.clone())).unwrap();

	let header02a = api.push_block_with_parent(header01.hash(), vec![xt.clone()], true);

	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));
	assert_eq!(pool.status_all()[&header02a.hash()].ready, 0);

	api.add_invalid(&xt);
	let header02b = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02b.hash())));

	// wait 10 blocks for revalidation
	let mut prev_header = header02b.clone();
	for _ in 3..=11 {
		let header = api.push_block_with_parent(prev_header.hash(), vec![], true);
		let event = finalized_block_event(&pool, prev_header.hash(), header.hash());
		block_on(pool.maintain(event));
		prev_header = header;
	}

	assert_watcher_stream!(
		watcher,
		[
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02a.hash(), 0)),
			TransactionStatus::Invalid
		]
	);

	//todo: shall revalidation check finalized (fork's tip) view?
	assert_eq!(pool.status_all()[&prev_header.hash()].ready, 0);
}

#[test]
fn fatp_watcher_invalid_many_revalidation() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 202);
	let xt3 = uxt(Alice, 203);
	let xt4 = uxt(Alice, 204);

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone()),
	];

	let submissions = block_on(futures::future::join_all(submissions));
	assert_eq!(pool.status_all()[&header01.hash()].ready, 5);

	let mut watchers = submissions.into_iter().map(Result::unwrap).collect::<Vec<_>>();
	let xt4_watcher = watchers.remove(4);
	let xt3_watcher = watchers.remove(3);
	let xt2_watcher = watchers.remove(2);
	let xt1_watcher = watchers.remove(1);
	let xt0_watcher = watchers.remove(0);

	api.add_invalid(&xt3);
	api.add_invalid(&xt4);

	let header02 = api.push_block(2, vec![], true);
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header02.hash())));

	//todo: shall revalidation check finalized (fork's tip) view?
	assert_eq!(pool.status_all()[&header02.hash()].ready, 5);

	let header03 = api.push_block(3, vec![xt0.clone(), xt1.clone(), xt2.clone()], true);
	block_on(pool.maintain(finalized_block_event(&pool, header02.hash(), header03.hash())));

	// wait 10 blocks for revalidation
	let mut prev_header = header03.clone();
	for n in 4..=11 {
		let header = api.push_block(n, vec![], true);
		let event = finalized_block_event(&pool, prev_header.hash(), header.hash());
		block_on(pool.maintain(event));
		prev_header = header;
	}

	assert_watcher_stream!(
		xt0_watcher,
		[
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 0)),
			TransactionStatus::Finalized((header03.hash(), 0))
		]
	);
	assert_watcher_stream!(
		xt1_watcher,
		[
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 1)),
			TransactionStatus::Finalized((header03.hash(), 1))
		]
	);
	assert_watcher_stream!(
		xt2_watcher,
		[
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 2)),
			TransactionStatus::Finalized((header03.hash(), 2))
		]
	);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(xt4_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
}

#[test]
fn fatp_watcher_invalid_fails_on_submission() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 150);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()));
	let xt0_watcher = xt0_watcher.map(|_| ());

	assert_pool_status!(header01.hash(), &pool, 0, 0);
	// Alice's nonce in state is 200, tx is 150.
	assert!(matches!(
		xt0_watcher.unwrap_err().into_pool_error(),
		Ok(TxPoolError::InvalidTransaction(InvalidTransaction::Stale))
	));
}

#[test]
fn fatp_watcher_invalid_single_revalidation() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, Some(api.genesis_hash()), header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	api.add_invalid(&xt0);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	let event = finalized_block_event(&pool, header01.hash(), header02.hash());
	block_on(pool.maintain(event));

	// wait 10 blocks for revalidation
	let mut prev_header = header02;
	for n in 3..=11 {
		let header = api.push_block(n, vec![], true);
		let event = finalized_block_event(&pool, prev_header.hash(), header.hash());
		block_on(pool.maintain(event));
		prev_header = header;
	}

	let xt0_events = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	log::debug!("xt0_events: {:#?}", xt0_events);
	assert_eq!(xt0_events, vec![TransactionStatus::Ready, TransactionStatus::Invalid]);
}

#[test]
fn fatp_watcher_invalid_single_revalidation2() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert_eq!(pool.mempool_len(), (0, 1));
	api.add_invalid(&xt0);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0_events = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	log::debug!("xt0_events: {:#?}", xt0_events);
	assert_eq!(xt0_events, vec![TransactionStatus::Invalid]);
	assert_eq!(pool.mempool_len(), (0, 0));
}

#[test]
fn fatp_watcher_invalid_single_revalidation3() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = pool();

	let xt0 = uxt(Alice, 150);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert_eq!(pool.mempool_len(), (0, 1));

	let header01 = api.push_block(1, vec![], true);
	let event = finalized_block_event(&pool, api.genesis_hash(), header01.hash());
	block_on(pool.maintain(event));

	// wait 10 blocks for revalidation
	let mut prev_header = header01;
	for n in 2..=11 {
		let header = api.push_block(n, vec![], true);
		let event = finalized_block_event(&pool, prev_header.hash(), header.hash());
		block_on(pool.maintain(event));
		prev_header = header;
	}

	let xt0_events = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	log::debug!("xt0_events: {:#?}", xt0_events);
	assert_eq!(xt0_events, vec![TransactionStatus::Invalid]);
	assert_eq!(pool.mempool_len(), (0, 0));
}

#[test]
fn fatp_invalid_report_stale_or_future_works_as_expected() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = TestPoolBuilder::new().build();
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

	// future/stale are ignored when at is None
	let xt0_report = (
		pool.api().hash_and_length(&xt0).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::Future)),
	);
	let invalid_txs = [xt0_report].into();
	let result = pool.report_invalid(None, invalid_txs);
	assert!(result.is_empty());
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1, xt2, xt3]);

	// future/stale are applied when at is provided
	let xt0_report = (
		pool.api().hash_and_length(&xt0).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::Future)),
	);
	let xt1_report = (
		pool.api().hash_and_length(&xt1).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
	);
	let invalid_txs = [xt0_report, xt1_report].into();
	let result = pool.report_invalid(Some(header01.hash()), invalid_txs);
	// stale/future does not cause tx to be removed from the pool
	assert!(result.is_empty());
	// assert_eq!(result[0].hash, pool.api().hash_and_length(&xt0).0);
	assert_ready_iterator!(header01.hash(), pool, [xt2, xt3]);

	// None error means force removal
	// todo

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
}

#[test]
fn fatp_invalid_report_future_dont_remove_from_pool() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = TestPoolBuilder::new().build();
	api.set_nonce(api.genesis_hash(), Bob.into(), 300);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 400);
	api.set_nonce(api.genesis_hash(), Dave.into(), 500);
	api.set_nonce(api.genesis_hash(), Eve.into(), 600);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 300);
	let xt2 = uxt(Charlie, 400);
	let xt3 = uxt(Dave, 500);
	let xt4 = uxt(Eve, 600);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();
	let xt4_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 5, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1, xt2, xt3, xt4]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));

	assert_pool_status!(header02.hash(), &pool, 5, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt0, xt1, xt2, xt3, xt4]);

	let xt0_report = (
		pool.api().hash_and_length(&xt0).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::Stale)),
	);
	let xt1_report = (
		pool.api().hash_and_length(&xt1).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::Future)),
	);
	let xt4_report = (
		pool.api().hash_and_length(&xt4).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::BadProof)),
	);
	let invalid_txs = [xt0_report, xt1_report, xt4_report].into();
	let result = pool.report_invalid(Some(header01.hash()), invalid_txs);

	assert_watcher_stream!(xt4_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);

	// future does not cause tx to be removed from the pool
	assert!(result.len() == 1);
	assert!(result[0].hash == pool.api().hash_and_length(&xt4).0);
	assert_ready_iterator!(header01.hash(), pool, [xt2, xt3]);

	assert_pool_status!(header02.hash(), &pool, 4, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt0, xt1, xt2, xt3]);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
}

#[test]
fn fatp_invalid_tx_is_removed_from_the_pool() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = TestPoolBuilder::new().build();
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

	let xt0_report = (
		pool.api().hash_and_length(&xt0).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::BadProof)),
	);
	let xt1_report = (pool.api().hash_and_length(&xt1).0, None);
	let invalid_txs = [xt0_report, xt1_report].into();
	let result = pool.report_invalid(Some(header01.hash()), invalid_txs);
	assert!(result.iter().any(|tx| tx.hash == pool.api().hash_and_length(&xt0).0));
	assert_pool_status!(header01.hash(), &pool, 2, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt2, xt3]);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02.hash(), pool, [xt2, xt3]);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
}

#[test]
fn fatp_invalid_tx_is_removed_from_the_pool_future_subtree_stays() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = TestPoolBuilder::new().build();

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 202);
	let xt3 = uxt(Alice, 203);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 4, 0);
	assert_ready_iterator!(header01.hash(), pool, [xt0, xt1, xt2, xt3]);

	let xt0_report = (
		pool.api().hash_and_length(&xt0).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::BadProof)),
	);
	let invalid_txs = [xt0_report].into();
	let result = pool.report_invalid(Some(header01.hash()), invalid_txs);
	assert_eq!(result[0].hash, pool.api().hash_and_length(&xt0).0);
	assert_pool_status!(header01.hash(), &pool, 0, 0);
	assert_ready_iterator!(header01.hash(), pool, []);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02.hash())));
	assert_pool_status!(header02.hash(), &pool, 0, 3);
	assert_future_iterator!(header02.hash(), pool, [xt1, xt2, xt3]);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
}

#[test]
fn fatp_invalid_tx_is_removed_from_the_pool2() {
	sp_tracing::try_init_simple();

	let (pool, api, _) = TestPoolBuilder::new().build();
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

	let header02a = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));
	assert_pool_status!(header02a.hash(), &pool, 4, 0);
	assert_ready_iterator!(header02a.hash(), pool, [xt0, xt1, xt2, xt3]);

	let header02b = api.push_block_with_parent(header01.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02a.hash()), header02b.hash())));

	assert_pool_status!(header02b.hash(), &pool, 4, 0);
	assert_ready_iterator!(header02b.hash(), pool, [xt0, xt1, xt2, xt3]);

	let xt0_report = (
		pool.api().hash_and_length(&xt0).0,
		Some(TransactionValidityError::Invalid(InvalidTransaction::BadProof)),
	);
	let xt1_report = (pool.api().hash_and_length(&xt1).0, None);
	let invalid_txs = [xt0_report, xt1_report].into();
	let result = pool.report_invalid(Some(header01.hash()), invalid_txs);
	assert!(result.iter().any(|tx| tx.hash == pool.api().hash_and_length(&xt0).0));
	assert_ready_iterator!(header01.hash(), pool, [xt2, xt3]);
	assert_pool_status!(header02a.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02a.hash(), pool, [xt2, xt3]);
	assert_pool_status!(header02b.hash(), &pool, 2, 0);
	assert_ready_iterator!(header02b.hash(), pool, [xt2, xt3]);

	let header03 = api.push_block_with_parent(header02b.hash(), vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02b.hash()), header03.hash())));
	assert_pool_status!(header03.hash(), &pool, 2, 0);
	assert_ready_iterator!(header03.hash(), pool, [xt2, xt3]);

	assert_watcher_stream!(xt0_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(xt1_watcher, [TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_watcher_stream!(xt2_watcher, [TransactionStatus::Ready]);
	assert_watcher_stream!(xt3_watcher, [TransactionStatus::Ready]);
}
