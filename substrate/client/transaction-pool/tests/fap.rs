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

//! Tests for top-level transaction pool api

use futures::{executor::block_on, task::Poll, FutureExt, StreamExt};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{
	error::{Error as TxPoolError, IntoPoolError},
	ChainEvent, MaintainedTransactionPool, TransactionPool, TransactionStatus,
};
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::{Block, Hash, Header},
	AccountKeyring::*,
};
use substrate_test_runtime_transaction_pool::{uxt, TestApi};
const LOG_TARGET: &str = "txpool";

use sc_transaction_pool::fork_aware_pool::{ForkAwareTxPool, ImportNotificationTask};

fn invalid_hash() -> Hash {
	Default::default()
}

fn new_best_block_event(
	pool: &ForkAwareTxPool<TestApi, Block>,
	from: Option<Hash>,
	to: Hash,
) -> ChainEvent<Block> {
	ChainEvent::NewBestBlock {
		hash: to,
		tree_route: from.map(|from| {
			// note: real tree route in NewBestBlock event does not contain 'to' block.
			Arc::from(
				pool.api()
					.tree_route(from, pool.api().block_header(to).unwrap().unwrap().parent_hash)
					.expect("Tree route exists"),
			)
		}),
	}
}

fn finalized_block_event(
	pool: &ForkAwareTxPool<TestApi, Block>,
	from: Hash,
	to: Hash,
) -> ChainEvent<Block> {
	let t = pool.api().tree_route(from, to).expect("Tree route exists");

	let e = t.enacted().iter().map(|h| h.hash).collect::<Vec<_>>();
	ChainEvent::Finalized { hash: to, tree_route: Arc::from(&e[0..e.len() - 1]) }
}

fn create_basic_pool_with_genesis(
	test_api: Arc<TestApi>,
) -> (ForkAwareTxPool<TestApi, Block>, ImportNotificationTask) {
	let genesis_hash = test_api
		.chain()
		.read()
		.block_by_number
		.get(&0)
		.map(|blocks| blocks[0].0.header.hash())
		.expect("there is block 0. qed");

	ForkAwareTxPool::new_test(test_api, genesis_hash, genesis_hash)
}

fn create_basic_pool(
	test_api: Arc<TestApi>,
) -> (ForkAwareTxPool<TestApi, Block>, futures::executor::ThreadPool) {
	let (pool, import_notification_task) = create_basic_pool_with_genesis(test_api);
	let thread_pool = futures::executor::ThreadPool::new().unwrap();
	thread_pool.spawn_ok(import_notification_task);
	(pool, thread_pool)
}

fn pool() -> (ForkAwareTxPool<TestApi, Block>, Arc<TestApi>, futures::executor::ThreadPool) {
	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, thread_pool) = create_basic_pool(api.clone());
	(pool, api, thread_pool)
}

macro_rules! assert_pool_status {
	($hash:expr, $pool:expr, $ready:expr, $future:expr) => {
		{
			log::info!(target:LOG_TARGET, "stats: {:#?}", $pool.status_all());
			let status = &$pool.status_all()[&$hash];
			assert_eq!(status.ready, $ready, "ready");
			assert_eq!(status.future, $future, "future");
		}
	}
}

const SOURCE: TransactionSource = TransactionSource::External;

#[cfg(test)]
mod test_chain_with_forks {
	use super::*;

	pub fn chain(
		include_xts: Option<&dyn Fn(usize, usize) -> bool>,
	) -> (Arc<TestApi>, Vec<Vec<Header>>) {
		// Fork layout:
		//
		//     F01 - F02 - F03 - F04 - F05 | Alice nonce increasing, alice's txs
		//    /
		// F00
		//    \
		//     F11 - F12 - F13 - F14 - F15 | Bob nonce increasing, bob's txs
		//
		// e.g. F03 contains uxt(Alice, 202), nonces: Alice = 203, Bob = 200
		//      F12 contains uxt(Bob,   201), nonces: Alice = 200, Bob = 202

		let api = Arc::from(TestApi::empty().enable_stale_check());

		let genesis = api.genesis_hash();

		let mut forks = vec![Vec::with_capacity(6), Vec::with_capacity(6)];
		let accounts = vec![Alice, Bob];
		accounts.iter().for_each(|a| api.set_nonce(genesis, (*a).into(), 200));

		for fork in 0..2 {
			let account = accounts[fork];
			forks[fork].push(api.block_header(genesis).unwrap().unwrap());
			let mut parent = genesis;
			for block in 1..6 {
				let xts = if include_xts.map_or(true, |v| v(fork, block)) {
					log::info!("{},{} -> add", fork, block);
					vec![uxt(account, (200 + block - 1) as u64)]
				} else {
					log::info!("{},{} -> skip", fork, block);
					vec![]
				};
				let header = api.push_block_with_parent(parent, xts, true);
				parent = header.hash();
				api.set_nonce(header.hash(), account.into(), (200 + block) as u64);
				forks[fork].push(header);
			}
		}

		(api, forks)
	}

	fn print_block(api: Arc<TestApi>, hash: Hash) {
		let accounts = vec![Alice.into(), Bob.into()];
		let header = api.block_header(hash).unwrap().unwrap();

		let nonces = accounts
			.iter()
			.map(|a| api.chain().read().nonces.get(&hash).unwrap().get(a).map(Clone::clone))
			.collect::<Vec<_>>();
		log::info!(
			"number: {:?} hash: {:?}, parent: {:?}, nonces:{:?}",
			header.number,
			header.hash(),
			header.parent_hash,
			nonces
		);
	}

	#[test]
	fn test() {
		sp_tracing::try_init_simple();
		let (api, f) = chain(None);
		log::info!("forks: {f:#?}");
		f[0].iter().for_each(|h| print_block(api.clone(), h.hash()));
		f[1].iter().for_each(|h| print_block(api.clone(), h.hash()));
		let tr = api.tree_route(f[0][4].hash(), f[1][3].hash());
		log::info!("{:#?}", tr);
		if let Ok(tr) = tr {
			log::info!("e:{:#?}", tr.enacted());
			log::info!("r:{:#?}", tr.retracted());
		}
	}
}

//todo:
//Add some more tests:
// - view.ready iterator
// - stale transaction submission when there is single view only (expect error)
// - stale transaction submission when there are more views (expect ok)
// - view count (e.g. same new block notified twice)
// - invalid with many views (different cases)
//
// done:
// fn submission_should_work()
// fn multiple_submission_should_work()
// fn early_nonce_should_be_culled()
// fn late_nonce_should_be_queued()
// fn only_prune_on_new_best()
// fn should_prune_old_during_maintenance()
// fn should_resubmit_from_retracted_during_maintenance() (shitty name)
// fn should_not_resubmit_from_retracted_during_maintenance_if_tx_is_also_in_enacted()
// fn finalization()
// fn finalization()  //with_watcher!
// fn should_revalidate_across_many_blocks()
//
// fn prune_and_retract_tx_at_same_time() (w/o retracted event) |
//      fap_watcher_fork_retract_and_finalize
//
// fn resubmit_tx_of_fork_that_is_not_part_of_retracted() | fap_retract_all_forks()
// fn resubmit_from_retracted_fork() | fap_fork_reorg
// fn fork_aware_finalization() | fap_watcher_finalizing_forks()
//
// fn ready_set_should_not_resolve_before_block_update() |
// 		fap_ready_at_does_not_trigger_after_submit
//		fap_ready_at_does_not_trigger
// fn ready_set_should_resolve_after_block_update() | fap_ready_at_triggered_by_maintain
// fn ready_set_should_eventually_resolve_when_block_update_arrives() |
// 		fap_ready_at_triggered_by_maintain2
//
// fn pruning_a_transaction_should_remove_it_from_best_transaction() |
// 		fap_one_view_ready_gets_pruned
//
// fn stale_transactions_are_pruned() | fap_linear_old_ready_becoming_stale
//
// fn finalized_only_handled_correctly() | fap_watcher_finalized (todo: no view?)
//
// fn best_block_after_finalized_handled_correctly() | fap_watcher_best_block_after_finalized
// 		fap_watcher_best_block_after_finalized2 fn switching_fork_with_finalized_works()
//
// fn switching_fork_multiple_times_works() | fap_watcher_switching_fork_multiple_times_works
//
// todo: double events?
// fn two_blocks_delayed_finalization_works() | fap_watcher_two_blocks_delayed_finalization_works
//
// fn delayed_finalization_does_not_retract() | fap_watcher_delayed_finalization_does_not_retract
//
//
// fn best_block_after_finalization_does_not_retract() |
// 		fap_watcher_best_block_after_finalization_does_not_retract
//
// fn should_push_watchers_during_maintenance() | fap_watcher_invalid_many_revalidation
//
// fn should_not_retain_invalid_hashes_from_retracted() |
// should_not_retain_invalid_hashes_from_retracted
//
// fn should_revalidate_during_maintenance() | should_revalidate_during_maintenance()
//
// -------------------------------------------------------------------------
//
// todo: [validated_pool/pool related, probably can be reused]:
// fn prune_tags_should_work()
// fn should_ban_invalid_transactions()
// fn should_correctly_prune_transactions_providing_more_than_one_tag()
//
// review, difficult to unerstand:
// fn import_notification_to_pool_maintain_works()

#[test]
fn fap_no_view_future_and_ready_submit_one_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header = api.push_block(1, vec![], true);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(header.hash(), SOURCE, xt0.clone()),
		pool.submit_one(header.hash(), SOURCE, xt1.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));

	assert!(results.iter().all(|r| { r.is_ok() }));
}

#[test]
fn fap_no_view_future_and_ready_submit_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header = api.push_block(1, vec![], true);

	let xts0 = (200..205).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts1 = (205..210).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts2 = (215..220).map(|i| uxt(Alice, i)).collect::<Vec<_>>();

	let submissions = vec![
		pool.submit_at(header.hash(), SOURCE, xts0.clone()),
		pool.submit_at(header.hash(), SOURCE, xts1.clone()),
		pool.submit_at(header.hash(), SOURCE, xts2.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));

	assert!(results.into_iter().flat_map(|x| x.unwrap()).all(|r| { r.is_ok() }));
}

#[test]
fn fap_one_view_future_and_ready_submit_one_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header = api.push_block(1, vec![], true);
	// let header01b = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(header.hash(), SOURCE, xt0.clone()),
		pool.submit_one(header.hash(), SOURCE, xt1.clone()),
	];

	block_on(futures::future::join_all(submissions));

	assert_pool_status!(header.hash(), &pool, 1, 1);
}

#[test]
fn fap_one_view_future_and_ready_submit_many_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header = api.push_block(1, vec![], true);
	// let header01b = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header.hash());
	block_on(pool.maintain(event));

	let xts0 = (200..205).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts1 = (205..210).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts2 = (215..220).map(|i| uxt(Alice, i)).collect::<Vec<_>>();

	let submissions = vec![
		pool.submit_at(header.hash(), SOURCE, xts0.clone()),
		pool.submit_at(header.hash(), SOURCE, xts1.clone()),
		pool.submit_at(header.hash(), SOURCE, xts2.clone()),
	];

	block_on(futures::future::join_all(submissions));

	assert_pool_status!(header.hash(), &pool, 10, 5);
}

#[test]
fn fap_one_view_stale_submit_one_fails() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 100);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	let results = block_on(futures::future::join_all(submissions));

	//xt0 should be stale
	assert!(matches!(
		&results[0].as_ref().unwrap_err().0,
		TxPoolError::InvalidTransaction(InvalidTransaction::Stale,)
	));

	assert_pool_status!(header.hash(), &pool, 0, 0);
}

#[test]
fn fap_one_view_stale_submit_many_fails() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header.hash());
	block_on(pool.maintain(event));

	let xts0 = (100..105).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts1 = (105..110).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts2 = (195..201).map(|i| uxt(Alice, i)).collect::<Vec<_>>();

	let submissions = vec![
		pool.submit_at(header.hash(), SOURCE, xts0.clone()),
		pool.submit_at(header.hash(), SOURCE, xts1.clone()),
		pool.submit_at(header.hash(), SOURCE, xts2.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));

	log::info!("{:#?}", results);

	//xts2 contains one ready transaction (nonce:200)
	//todo: submit_at result is not ordered as the input
	assert_eq!(
		results
			.into_iter()
			.flat_map(|x| x.unwrap())
			.filter(Result::is_err)
			.filter(|r| {
				matches!(
					&r.as_ref().unwrap_err().0,
					TxPoolError::InvalidTransaction(InvalidTransaction::Stale,)
				)
			})
			.count(),
		xts0.len() + xts1.len() + xts2.len() - 1
	);

	assert_pool_status!(header.hash(), &pool, 1, 0);
}

#[test]
fn fap_one_view_future_turns_to_ready_works() {
	let (pool, api, _) = pool();

	let header = api.push_block(1, vec![], true);
	let at = header.hash();
	let event = new_best_block_event(&pool, None, at);
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 201);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert!(pool.ready(at).unwrap().count() == 0);
	assert_pool_status!(at, &pool, 0, 1);

	let xt1 = uxt(Alice, 200);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let ready: Vec<_> = pool.ready(at).unwrap().map(|v| v.data.clone()).collect();
	assert_eq!(ready, vec![xt1, xt0]);
	assert_pool_status!(at, &pool, 2, 0);
}

#[test]
fn fap_one_view_ready_gets_pruned() {
	let (pool, api, _) = pool();

	let header = api.push_block(1, vec![], true);
	let block1 = header.hash();
	let event = new_best_block_event(&pool, None, block1);
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let pending: Vec<_> = pool.ready(block1).unwrap().map(|v| v.data.clone()).collect();
	assert_eq!(pending, vec![xt0.clone()]);
	assert_eq!(pool.status_all()[&block1].ready, 1);

	let header = api.push_block(2, vec![xt0], true);
	let block2 = header.hash();
	let event = new_best_block_event(&pool, Some(block1), block2);
	block_on(pool.maintain(event));
	assert_pool_status!(block2, &pool, 0, 0);
	assert!(pool.ready(block2).unwrap().count() == 0);
}

#[test]
fn fap_one_view_ready_turns_to_stale_works() {
	let (pool, api, _) = pool();

	let header = api.push_block(1, vec![], true);
	let block1 = header.hash();
	let event = new_best_block_event(&pool, None, block1);
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let pending: Vec<_> = pool.ready(block1).unwrap().map(|v| v.data.clone()).collect();
	assert_eq!(pending, vec![xt0.clone()]);
	assert_eq!(pool.status_all()[&block1].ready, 1);

	let header = api.push_block(2, vec![], true);
	let block2 = header.hash();
	//tricky: typically the block2 shall contain conflicting transaction for Alice. In this test we
	//want to check revalidation, so we manually adjust nonce.
	api.set_nonce(block2, Alice.into(), 201);
	let event = new_best_block_event(&pool, Some(block1), block2);
	//note: blocking revalidation (w/o background worker) which is used in this test will detect
	// xt0 is stale
	block_on(pool.maintain(event));
	//todo: should it work at all? (it requires better revalidation: mempool keeping validated txs)
	// assert_pool_status!(block2, &pool, 0, 0);
	// assert!(pool.ready(block2).unwrap().count() == 0);
}

#[test]
fn fap_two_views_future_and_ready_sumbit_one() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let genesis = api.genesis_hash();
	let header01a = api.push_block(1, vec![], true);
	let header01b = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01a.hash());
	block_on(pool.maintain(event));

	let event = new_best_block_event(&pool, None, header01b.hash());
	block_on(pool.maintain(event));

	api.set_nonce(header01b.hash(), Alice.into(), 202);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(genesis, SOURCE, xt0.clone()),
		pool.submit_one(genesis, SOURCE, xt1.clone()),
	];

	block_on(futures::future::join_all(submissions));

	assert_pool_status!(header01a.hash(), &pool, 1, 1);
	assert_pool_status!(header01b.hash(), &pool, 1, 0);
}

#[test]
fn fap_two_views_future_and_ready_sumbit_many() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01a = api.push_block(1, vec![], true);
	let header01b = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01a.hash());
	block_on(pool.maintain(event));

	let event = new_best_block_event(&pool, None, header01b.hash());
	block_on(pool.maintain(event));

	api.set_nonce(header01b.hash(), Alice.into(), 215);

	let xts0 = (200..205).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts1 = (205..210).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts2 = (215..220).map(|i| uxt(Alice, i)).collect::<Vec<_>>();

	let submissions = vec![
		pool.submit_at(invalid_hash(), SOURCE, xts0.clone()),
		pool.submit_at(invalid_hash(), SOURCE, xts1.clone()),
		pool.submit_at(invalid_hash(), SOURCE, xts2.clone()),
	];

	block_on(futures::future::join_all(submissions));

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	assert_pool_status!(header01a.hash(), &pool, 10, 5);
	assert_pool_status!(header01b.hash(), &pool, 5, 0);
}

#[test]
fn fap_linear_progress() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let (pool, _) = create_basic_pool(api.clone());

	let f11 = forks[1][1].hash();
	let f13 = forks[1][3].hash();

	let event = new_best_block_event(&pool, None, f11);
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];

	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f11), f13);
	log::info!(target:LOG_TARGET, "event: {:#?}", event);
	block_on(pool.maintain(event));

	//note: we only keep tip of the fork
	assert_eq!(pool.views_len(), 1);
	assert_pool_status!(f13, &pool, 1, 0);
}

#[test]
fn fap_linear_old_ready_becoming_stale() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	// Our initial transactions
	let xts = vec![uxt(Alice, 300), uxt(Alice, 301), uxt(Alice, 302)];

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	xts.into_iter().for_each(|xt| {
		block_on(pool.submit_one(invalid_hash(), SOURCE, xt)).unwrap();
	});
	assert_eq!(pool.status_all()[&header01.hash()].ready, 0);
	assert_eq!(pool.status_all()[&header01.hash()].future, 3);

	// Import enough blocks to make our transactions stale (longevity is 64)
	let mut prev_header = header01;
	for n in 2..66 {
		let header = api.push_block(n, vec![], true);
		let event = new_best_block_event(&pool, Some(prev_header.hash()), header.hash());
		block_on(pool.maintain(event));

		if n == 65 {
			assert_eq!(pool.status_all()[&header.hash()].ready, 0);
			assert_eq!(pool.status_all()[&header.hash()].future, 0);
		} else {
			assert_eq!(pool.status_all()[&header.hash()].ready, 0);
			assert_eq!(pool.status_all()[&header.hash()].future, 3);
		}
		prev_header = header;
	}
}

#[test]
fn fap_fork_reorg() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let (pool, _) = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	let event = new_best_block_event(&pool, None, f03);
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 203);
	let xt1 = uxt(Bob, 204);
	let xt2 = uxt(Alice, 203);
	let submissions = vec![
		pool.submit_one(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_one(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_one(invalid_hash(), SOURCE, xt2.clone()),
	];

	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f03), f13);
	log::info!(target:LOG_TARGET, "event: {:#?}", event);
	block_on(pool.maintain(event));

	assert_pool_status!(f03, &pool, 1, 2);
	assert_pool_status!(f13, &pool, 6, 0);

	//check if ready for block[1][3] contains resubmitted transactions
	let mut expected = forks[0]
		.iter()
		.take(4)
		.flat_map(|h| block_on(api.block_body(h.hash())).unwrap().unwrap())
		.collect::<Vec<_>>();
	expected.extend_from_slice(&[xt0, xt1, xt2]);

	let ready_f13 = pool.ready(f13).unwrap().collect::<Vec<_>>();
	expected.iter().for_each(|e| {
		assert!(ready_f13.iter().any(|v| v.data == *e));
	});
	assert_eq!(expected.len(), ready_f13.len());
}

#[test]
fn fap_fork_do_resubmit_same_tx() {
	let xt = uxt(Alice, 200);

	let (pool, api, _) = pool();
	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	block_on(pool.submit_one(api.expect_hash_from_number(0), SOURCE, xt.clone())).unwrap();
	assert_eq!(pool.status_all()[&header01.hash()].ready, 1);

	let header02a = api.push_block(1, vec![xt.clone()], true);
	let header02b = api.push_block(1, vec![xt], true);

	let event = new_best_block_event(&pool, Some(header02a.hash()), header02b.hash());
	api.set_nonce(header02a.hash(), Alice.into(), 201);
	block_on(pool.maintain(event));
	assert_eq!(pool.status_all()[&header02b.hash()].ready, 0);

	let event = new_best_block_event(&pool, Some(api.genesis_hash()), header02b.hash());
	api.set_nonce(header02b.hash(), Alice.into(), 201);
	block_on(pool.maintain(event));

	assert_eq!(pool.status_all()[&header02b.hash()].ready, 0);
}

#[test]
fn fap_fork_stale_switch_to_future() {
	sp_tracing::try_init_simple();

	// note: there are no xts in blocks on fork 0!
	let (api, forks) = test_chain_with_forks::chain(Some(&|f, b| match (f, b) {
		(0, _) => false,
		_ => true,
	}));
	let (pool, _) = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	let event = new_best_block_event(&pool, None, f03);
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 203);
	let xt1 = uxt(Bob, 204);
	let xt2 = uxt(Alice, 201);
	let submissions = vec![
		pool.submit_one(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_one(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_one(invalid_hash(), SOURCE, xt2.clone()),
	];
	let submission_results = block_on(futures::future::join_all(submissions));

	//xt2 should be stale
	assert!(matches!(
		&submission_results[2].as_ref().unwrap_err().0,
		TxPoolError::InvalidTransaction(InvalidTransaction::Stale,)
	));

	let event = new_best_block_event(&pool, Some(f03), f13);
	log::info!(target:LOG_TARGET, "event: {:#?}", event);
	block_on(pool.maintain(event));

	assert_pool_status!(f03, &pool, 0, 2);
	assert_pool_status!(f13, &pool, 2, 1);

	let futures_f03 = pool.futures(f03).unwrap();
	let futures_f13 = pool.futures(f13).unwrap();
	let ready_f13 = pool.ready(f13).unwrap().collect::<Vec<_>>();
	assert!(futures_f13.iter().any(|v| v.data == xt2));
	assert!(futures_f03.iter().any(|v| v.data == xt0));
	assert!(futures_f03.iter().any(|v| v.data == xt1));
	assert!(ready_f13.iter().any(|v| v.data == xt0));
	assert!(ready_f13.iter().any(|v| v.data == xt1));
}

#[test]
fn fap_fork_no_xts_ready_switch_to_future() {
	//this scenario w/o xts is not likely to happen, but similar thing (xt changing from ready to
	//future) could occur e.g. when runtime was updated on fork1.
	sp_tracing::try_init_simple();

	// note: there are no xts in blocks!
	let (api, forks) = test_chain_with_forks::chain(Some(&|f, b| match (f, b) {
		_ => false,
	}));
	let (pool, _) = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	let event = new_best_block_event(&pool, None, f03);
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f03), f13);
	block_on(pool.maintain(event));

	assert_pool_status!(f03, &pool, 1, 0);

	//xt0 becomes future, and this may only happen after view revalidation
	//todo: should it work at all? (it requires better revalidation: mempool keeping validated txs)
	// assert_pool_status!(f13, &pool, 0, 1);
}

#[test]
fn fap_ready_at_does_not_trigger() {
	//this scenario w/o xts is not likely to happen, but similar thing (xt changing from ready to
	//future) could occur e.g. when runtime was updated on fork1.
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let (pool, _) = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	assert!(pool.ready_at(f03).now_or_never().is_none());
	assert!(pool.ready_at(f13).now_or_never().is_none());
}

#[test]
fn fap_ready_at_does_not_trigger_after_submit() {
	//this scenario w/o xts is not likely to happen, but similar thing (xt changing from ready to
	//future) could occur e.g. when runtime was updated on fork1.
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let (pool, _) = create_basic_pool(api.clone());

	let xt0 = uxt(Alice, 200);
	let _ = block_on(pool.submit_one(invalid_hash(), SOURCE, xt0));

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	assert!(pool.ready_at(f03).now_or_never().is_none());
	assert!(pool.ready_at(f13).now_or_never().is_none());
}

#[test]
fn fap_ready_at_triggered_by_maintain() {
	//this scenario w/o xts is not likely to happen, but similar thing (xt changing from ready to
	//future) could occur e.g. when runtime was updated on fork1.
	sp_tracing::try_init_simple();
	let (api, forks) = test_chain_with_forks::chain(Some(&|f, b| match (f, b) {
		_ => false,
	}));
	let (pool, _) = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	assert!(pool.ready_at(f03).now_or_never().is_none());

	let event = new_best_block_event(&pool, None, f03);
	block_on(pool.maintain(event));

	assert!(pool.ready_at(f03).now_or_never().is_some());

	let xt0 = uxt(Alice, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f03), f13);
	log::info!(target:LOG_TARGET, "event: {:#?}", event);
	assert!(pool.ready_at(f13).now_or_never().is_none());
	block_on(pool.maintain(event));
	assert!(pool.ready_at(f03).now_or_never().is_some());
	assert!(pool.ready_at(f13).now_or_never().is_some());
}

#[test]
fn fap_ready_at_triggered_by_maintain2() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let xt0 = uxt(Alice, 200);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	// let (pool, api, _guard) = maintained_pool();
	// let header = api.push_block(1, vec![], true);
	//
	// let xt1 = uxt(Alice, 209);
	//
	// block_on(pool.submit_one(api.expect_hash_from_number(1), SOURCE, xt1.clone()))
	// 	.expect("1. Imported");

	let noop_waker = futures::task::noop_waker();
	let mut context = futures::task::Context::from_waker(&noop_waker);

	let mut ready_set_future = pool.ready_at(header01.hash());
	if ready_set_future.poll_unpin(&mut context).is_ready() {
		panic!("Ready set should not be ready before block update!");
	}

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));
	// block_on(pool.maintain(block_event(header)));

	match ready_set_future.poll_unpin(&mut context) {
		Poll::Pending => {
			panic!("Ready set should become ready after block update!");
		},
		Poll::Ready(iterator) => {
			let data = iterator.collect::<Vec<_>>();
			assert_eq!(data.len(), 1);
		},
	}
}

#[test]
fn fap_linear_progress_finalization() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let (pool, _) = create_basic_pool(api.clone());

	let f00 = forks[0][0].hash();
	let f12 = forks[1][2].hash();
	let f14 = forks[1][4].hash();

	let event = new_best_block_event(&pool, None, f00);
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 204);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f00), f12);
	block_on(pool.maintain(event));
	assert_pool_status!(f12, &pool, 0, 1);
	assert_eq!(pool.views_len(), 1);

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let event = ChainEvent::Finalized { hash: f14, tree_route: Arc::from(vec![]) };
	block_on(pool.maintain(event));

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	assert_eq!(pool.views_len(), 1);
	assert_pool_status!(f14, &pool, 1, 0);
}

#[test]
fn fap_fork_finalization_removes_stale_views() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let (pool, _) = create_basic_pool(api.clone());

	let f00 = forks[0][0].hash();
	let f12 = forks[1][2].hash();
	let f14 = forks[1][4].hash();
	let f02 = forks[0][2].hash();
	let f03 = forks[0][3].hash();
	let f04 = forks[0][4].hash();

	let xt0 = uxt(Bob, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f00), f12);
	block_on(pool.maintain(event));
	let event = new_best_block_event(&pool, Some(f00), f14);
	block_on(pool.maintain(event));
	let event = new_best_block_event(&pool, Some(f00), f02);
	block_on(pool.maintain(event));

	//only views at the tip of forks are kept
	assert_eq!(pool.views_len(), 2);

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let event = ChainEvent::Finalized { hash: f03, tree_route: Arc::from(vec![]) };
	block_on(pool.maintain(event));
	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());
	// note: currently the pruning views only cleans views with block number less then finalized
	// blcock. views with higher number on other forks are not cleaned (will be done in next round).
	assert_eq!(pool.views_len(), 2);

	let event = ChainEvent::Finalized { hash: f04, tree_route: Arc::from(vec![]) };
	block_on(pool.maintain(event));
	assert_eq!(pool.views_len(), 1);
}

#[test]
fn fap_watcher_invalid_fails_on_submission() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 150);
	api.add_invalid(&xt0);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()));
	let xt0_watcher = xt0_watcher.map(|_| ());

	assert_pool_status!(header01.hash(), &pool, 0, 0);
	assert!(matches!(
		xt0_watcher.unwrap_err().into_pool_error(),
		Ok(TxPoolError::InvalidTransaction(InvalidTransaction::Stale))
	));
}

#[test]
fn fap_watcher_invalid_single_revalidation() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, Some(api.genesis_hash()), header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	api.add_invalid(&xt0);

	let header02 = api.push_block_with_parent(header01.hash(), vec![], true);
	let event = finalized_block_event(&pool, header01.hash(), header02.hash());
	block_on(pool.maintain(event));

	let xt0_events = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	log::info!("xt0_events: {:#?}", xt0_events);
	assert_eq!(xt0_events, vec![TransactionStatus::Ready, TransactionStatus::Invalid]);
}

#[test]
fn fap_watcher_invalid_single_revalidation2() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert_eq!(pool.mempool_len(), (0, 1));
	api.add_invalid(&xt0);

	let header01 = api.push_block(1, vec![], true);
	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0_events = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	log::info!("xt0_events: {:#?}", xt0_events);
	assert_eq!(xt0_events, vec![TransactionStatus::Invalid]);
	assert_eq!(pool.mempool_len(), (0, 0));
}

#[test]
fn fap_watcher_invalid_single_revalidation3() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let xt0 = uxt(Alice, 150);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert_eq!(pool.mempool_len(), (0, 1));

	let header01 = api.push_block(1, vec![], true);
	let event = finalized_block_event(&pool, api.genesis_hash(), header01.hash());
	block_on(pool.maintain(event));

	let xt0_events = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	log::info!("xt0_events: {:#?}", xt0_events);
	assert_eq!(xt0_events, vec![TransactionStatus::Invalid]);
	assert_eq!(pool.mempool_len(), (0, 0));
}

#[test]
fn fap_watcher_future() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 202);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 0, 1);

	let header02 = api.push_block(2, vec![], true);
	let event = ChainEvent::Finalized {
		hash: header02.hash(),
		tree_route: Arc::from(vec![header01.hash()]),
	};
	block_on(pool.maintain(event));

	assert_pool_status!(header02.hash(), &pool, 0, 1);

	let xt0_events = block_on(xt0_watcher.take(1).collect::<Vec<_>>());
	assert_eq!(xt0_events, vec![TransactionStatus::Future]);
}

#[test]
fn fap_watcher_ready() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 1, 0);

	let header02 = api.push_block(2, vec![], true);
	let event = ChainEvent::Finalized {
		hash: header02.hash(),
		tree_route: Arc::from(vec![header01.hash()]),
	};
	block_on(pool.maintain(event));

	assert_pool_status!(header02.hash(), &pool, 1, 0);

	let xt0_events = block_on(xt0_watcher.take(1).collect::<Vec<_>>());
	assert_eq!(xt0_events, vec![TransactionStatus::Ready]);
}

#[test]
fn fap_watcher_finalized() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 1, 0);

	let header02 = api.push_block(2, vec![xt0], true);
	let event = ChainEvent::Finalized {
		hash: header02.hash(),
		tree_route: Arc::from(vec![header01.hash()]),
	};
	block_on(pool.maintain(event));

	assert_pool_status!(header02.hash(), &pool, 0, 0);

	let xt0_events = block_on(xt0_watcher.collect::<Vec<_>>());
	assert_eq!(
		xt0_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_in_block() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	assert_pool_status!(header01.hash(), &pool, 1, 0);

	let header02 = api.push_block(2, vec![xt0], true);

	let event = new_best_block_event(&pool, Some(header01.hash()), header02.hash());
	block_on(pool.maintain(event));
	let xt0_events = block_on(xt0_watcher.take(2).collect::<Vec<_>>());
	assert_eq!(
		xt0_events,
		vec![TransactionStatus::Ready, TransactionStatus::InBlock((header02.hash(), 0)),]
	);
}

#[test]
fn fap_watcher_future_and_finalized() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
	];

	let mut submissions = block_on(futures::future::join_all(submissions));
	let xt1_watcher = submissions.remove(1).unwrap();
	let xt0_watcher = submissions.remove(0).unwrap();

	assert_pool_status!(header01.hash(), &pool, 1, 1);

	let header02 = api.push_block(2, vec![xt0], true);
	let event = ChainEvent::Finalized {
		hash: header02.hash(),
		tree_route: Arc::from(vec![header01.hash()]),
	};
	// let event = new_best_block_event(&pool, Some(header01.hash()), header02.hash());
	block_on(pool.maintain(event));

	assert_pool_status!(header02.hash(), &pool, 0, 1);

	let xt1_status = block_on(xt1_watcher.take(1).collect::<Vec<_>>());
	assert_eq!(xt1_status, vec![TransactionStatus::Future]);
	let xt0_status = block_on(xt0_watcher.collect::<Vec<_>>());
	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_two_finalized_in_different_block() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());
	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Dave.into(), 200);

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Bob, 200);
	let xt3 = uxt(Dave, 200);

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone()),
	];
	let mut submissions = block_on(futures::future::join_all(submissions));
	let xt2_watcher = submissions.remove(2).unwrap();
	let xt1_watcher = submissions.remove(1).unwrap();
	let xt0_watcher = submissions.remove(0).unwrap();

	assert_pool_status!(header01.hash(), &pool, 3, 0);

	let header02 = api.push_block(2, vec![xt3.clone(), xt2.clone(), xt0.clone()], true);
	api.set_nonce(header02.hash(), Alice.into(), 201);
	//note: no maintain for block02 (!)

	let header03 = api.push_block(3, vec![xt1.clone()], true);
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header03.hash())));

	assert_pool_status!(header03.hash(), &pool, 0, 0);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).collect::<Vec<_>>();

	assert_eq!(
		xt1_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 0)),
			TransactionStatus::Finalized((header03.hash(), 0))
		]
	);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();

	log::info!("xt0_status: {:#?}", xt0_status);

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 2)),
			TransactionStatus::Finalized((header02.hash(), 2))
		]
	);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).collect::<Vec<_>>();
	log::info!("xt2_status: {:#?}", xt2_status);

	assert_eq!(
		xt2_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 1)),
			TransactionStatus::Finalized((header02.hash(), 1))
		]
	);
}

#[test]
fn fap_no_view_pool_watcher_two_finalized_in_different_block() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());
	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Dave.into(), 200);

	let header01 = api.push_block(1, vec![], true);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Bob, 200);
	let xt3 = uxt(Dave, 200);

	let submissions = vec![
		pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone()),
		pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone()),
	];
	let mut submissions = block_on(futures::future::join_all(submissions));
	let xt2_watcher = submissions.remove(2).unwrap();
	let xt1_watcher = submissions.remove(1).unwrap();
	let xt0_watcher = submissions.remove(0).unwrap();

	let header02 = api.push_block(2, vec![xt3.clone(), xt2.clone(), xt0.clone()], true);
	api.set_nonce(header02.hash(), Alice.into(), 201);
	api.set_nonce(header02.hash(), Bob.into(), 201);
	api.set_nonce(header02.hash(), Dave.into(), 201);
	//note: no maintain for block02 (!)

	let header03 = api.push_block(3, vec![xt1.clone()], true);
	api.set_nonce(header03.hash(), Alice.into(), 202);
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header03.hash())));

	assert_pool_status!(header03.hash(), &pool, 0, 0);

	let xt1_status = futures::executor::block_on_stream(xt1_watcher).collect::<Vec<_>>();

	log::info!("xt1_status: {:#?}", xt1_status);

	assert_eq!(
		xt1_status,
		vec![
			TransactionStatus::InBlock((header03.hash(), 0)),
			TransactionStatus::Finalized((header03.hash(), 0))
		]
	);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();

	log::info!("xt0_status: {:#?}", xt0_status);

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::InBlock((header02.hash(), 2)),
			TransactionStatus::Finalized((header02.hash(), 2))
		]
	);

	let xt2_status = futures::executor::block_on_stream(xt2_watcher).collect::<Vec<_>>();
	log::info!("xt2_status: {:#?}", xt2_status);

	assert_eq!(
		xt2_status,
		vec![
			TransactionStatus::InBlock((header02.hash(), 1)),
			TransactionStatus::Finalized((header02.hash(), 1))
		]
	);
}

#[test]
fn fap_watcher_in_block_across_many_blocks() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 201);
	let xt2 = uxt(Alice, 202);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let _ = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	assert_pool_status!(header01.hash(), &pool, 2, 0);

	let header02 = api.push_block(2, vec![], true);
	let event = new_best_block_event(&pool, Some(header01.hash()), header02.hash());
	block_on(pool.maintain(event));

	let _ = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	//note 1: transaction is not submitted to views that are not at the tip of the fork
	//note 2: only views at tip of the fork are kept
	assert_eq!(pool.views_len(), 1);
	assert_pool_status!(header02.hash(), &pool, 3, 0);

	let header03 = api.push_block(3, vec![xt0.clone()], true);
	let event = new_best_block_event(&pool, Some(header02.hash()), header03.hash());
	block_on(pool.maintain(event));

	assert_pool_status!(header03.hash(), &pool, 2, 0);

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).take(2).collect::<Vec<_>>();

	log::info!("xt1_status: {:#?}", xt0_status);
	assert_eq!(
		xt0_status,
		vec![TransactionStatus::Ready, TransactionStatus::InBlock((header03.hash(), 0)),]
	);
}

#[test]
fn fap_watcher_dropping_listener_should_work() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);

	let _ = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert_pool_status!(header01.hash(), &pool, 1, 0);

	let header02 = api.push_block(2, vec![], true);
	let event = new_best_block_event(&pool, Some(header01.hash()), header02.hash());
	block_on(pool.maintain(event));
}

#[test]
fn fap_watcher_fork_retract_and_finalize() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let header01 = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert_pool_status!(header01.hash(), &pool, 1, 0);

	let header02a = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);
	let event = new_best_block_event(&pool, Some(header01.hash()), header02a.hash());
	block_on(pool.maintain(event));
	assert_pool_status!(header02a.hash(), &pool, 0, 0);

	let header02b = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);
	let event = ChainEvent::Finalized {
		hash: header02b.hash(),
		tree_route: Arc::from(vec![header01.hash()]),
	};
	block_on(pool.maintain(event));
	assert_pool_status!(header02b.hash(), &pool, 0, 0);
	let xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();

	log::info!("xt0_status: {:#?}", xt0_status);

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02a.hash(), 0)),
			TransactionStatus::InBlock((header02b.hash(), 0)),
			TransactionStatus::Finalized((header02b.hash(), 0)),
		]
	);
}

#[test]
fn fap_retract_all_forks() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	let (pool, _) = create_basic_pool(api.clone());
	let genesis = api.genesis_hash();

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 200);

	let header02a = api.push_block_with_parent(genesis, vec![xt0.clone()], true);
	let event = new_best_block_event(&pool, Some(genesis), header02a.hash());
	block_on(pool.maintain(event));
	assert_pool_status!(header02a.hash(), &pool, 0, 0);

	let header02b = api.push_block_with_parent(genesis, vec![xt1.clone()], true);
	let event = new_best_block_event(&pool, Some(header02a.hash()), header02b.hash());
	block_on(pool.maintain(event));
	assert_pool_status!(header02b.hash(), &pool, 1, 0);

	let header02c = api.push_block_with_parent(genesis, vec![], true);
	let event =
		ChainEvent::Finalized { hash: header02c.hash(), tree_route: Arc::from(vec![genesis]) };
	block_on(pool.maintain(event));
	assert_pool_status!(header02c.hash(), &pool, 2, 0);
}

#[test]
fn fap_watcher_finalizing_forks() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 200);
	api.set_nonce(api.genesis_hash(), Dave.into(), 200);
	api.set_nonce(api.genesis_hash(), Eve.into(), 200);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 200);
	let xt2 = uxt(Charlie, 200);
	let xt3 = uxt(Dave, 200);
	let xt4 = uxt(Eve, 200);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let header01 = api.push_block(1, vec![xt0.clone()], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header01.hash())));

	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let header02a = api.push_block_with_parent(header01.hash(), vec![xt1.clone()], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02a.hash())));

	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let header03a = api.push_block_with_parent(header02a.hash(), vec![xt2.clone()], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02a.hash()), header03a.hash())));

	let xt3_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone())).unwrap();
	let header02b = api.push_block_with_parent(header01.hash(), vec![xt3.clone()], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01.hash()), header02b.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header02b.hash())));

	let xt4_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt4.clone())).unwrap();
	let header03b = api.push_block_with_parent(header02b.hash(), vec![xt4.clone()], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02b.hash()), header03b.hash())));

	let header04b =
		api.push_block_with_parent(header03b.hash(), vec![xt1.clone(), xt2.clone()], true);
	block_on(pool.maintain(new_best_block_event(&pool, Some(header03b.hash()), header04b.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, header02b.hash(), header04b.hash())));

	//=======================

	let mut xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	let mut xt1_status = futures::executor::block_on_stream(xt1_watcher).collect::<Vec<_>>();
	let mut xt2_status = futures::executor::block_on_stream(xt2_watcher).collect::<Vec<_>>();
	let mut xt3_status = futures::executor::block_on_stream(xt3_watcher).collect::<Vec<_>>();
	let mut xt4_status = futures::executor::block_on_stream(xt4_watcher).collect::<Vec<_>>();

	// Ready may come from numerous views, simplify that, we don't care:
	xt0_status.dedup();
	xt1_status.dedup();
	xt2_status.dedup();
	xt3_status.dedup();
	xt4_status.dedup();

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header01.hash(), 0)),
			TransactionStatus::Finalized((header01.hash(), 0)),
		]
	);

	assert_eq!(
		xt1_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02a.hash(), 0)),
			TransactionStatus::InBlock((header04b.hash(), 0)),
			TransactionStatus::Finalized((header04b.hash(), 0)),
		]
	);
	assert_eq!(
		xt2_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03a.hash(), 0)),
			TransactionStatus::InBlock((header04b.hash(), 1)),
			TransactionStatus::Finalized((header04b.hash(), 1)),
		]
	);
	assert_eq!(
		xt3_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02b.hash(), 0)),
			TransactionStatus::Finalized((header02b.hash(), 0)),
		]
	);
	assert_eq!(
		xt4_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03b.hash(), 0)),
			TransactionStatus::Finalized((header03b.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_best_block_after_finalized() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());
	let header01 = api.push_block(1, vec![], true);
	let event = finalized_block_event(&pool, api.genesis_hash(), header01.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();

	// todo: shall we submit to finalized views? (if it is at the tip of the fork then yes?)
	// assert_pool_status!(header01.hash(), &pool, 1, 0);

	let header02 = api.push_block(2, vec![xt0.clone()], true);

	let event = finalized_block_event(&pool, header01.hash(), header02.hash());
	block_on(pool.maintain(event));
	let event = new_best_block_event(&pool, Some(header01.hash()), header02.hash());
	block_on(pool.maintain(event));

	let xt0_events = block_on(xt0_watcher.collect::<Vec<_>>());
	assert_eq!(
		xt0_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_best_block_after_finalized2() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let xt0 = uxt(Alice, 200);
	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	// assert_pool_status!(header01.hash(), &pool, 1, 0);

	let header01 = api.push_block(1, vec![xt0.clone()], true);

	let event = finalized_block_event(&pool, api.genesis_hash(), header01.hash());
	block_on(pool.maintain(event));
	let event = new_best_block_event(&pool, Some(api.genesis_hash()), header01.hash());
	block_on(pool.maintain(event));

	let xt0_events = block_on(xt0_watcher.collect::<Vec<_>>());
	assert_eq!(
		xt0_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header01.hash(), 0)),
			TransactionStatus::Finalized((header01.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_switching_fork_multiple_times_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	api.set_nonce(api.genesis_hash(), Bob.into(), 200);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 200);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let header01a = api.push_block(1, vec![xt0.clone()], true);

	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let header01b = api.push_block(1, vec![xt0.clone(), xt1.clone()], true);

	//note: finalized block here must be header01b.
	//It is because of how the order in which MultiViewListener is processing tx events and view
	//events. tx events from single view are processed first, then view commands are handled. If
	//finalization happens in first view reported then no events from others views will be
	//processed.

	block_on(pool.maintain(new_best_block_event(&pool, None, header01a.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01a.hash()), header01b.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(header01b.hash()), header01a.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header01b.hash())));

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).take(2).collect::<Vec<_>>();

	log::info!("xt0_status: {:#?}", xt0_status);
	log::info!("xt1_status: {:#?}", xt1_status);

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header01a.hash(), 0)),
			TransactionStatus::InBlock((header01b.hash(), 0)),
			TransactionStatus::Finalized((header01b.hash(), 0)),
		]
	);

	assert_eq!(
		xt1_status,
		vec![TransactionStatus::Ready, TransactionStatus::InBlock((header01b.hash(), 1)),]
	);
}

#[test]
fn fap_watcher_two_blocks_delayed_finalization_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 200);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 200);
	let xt2 = uxt(Charlie, 200);

	let header01 = api.push_block(1, vec![], true);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);

	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let header03 = api.push_block_with_parent(header02.hash(), vec![xt1.clone()], true);

	let xt2_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	let header04 = api.push_block_with_parent(header03.hash(), vec![xt2.clone()], true);

	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header01.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, None, header04.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header03.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, header03.hash(), header04.hash())));

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).collect::<Vec<_>>();
	let xt2_status = futures::executor::block_on_stream(xt2_watcher).collect::<Vec<_>>();

	//todo: double events.
	//view for header04 reported inblock for all xts.
	//Then finalization comes for header03. We need to create a view to sent finalization events.
	//But in_block are also sent because of pruning - normal process during view creation.
	//
	//Do not know what solution should be in this case?
	// - just jeep two events,
	// - block pruning somehow (seems like excessive additional logic not really needed)
	// - build view from recent best block? (retracting instead of enacting?)
	// - de-dup events in listener (implemented)

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0)),
		]
	);
	assert_eq!(
		xt1_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 0)),
			TransactionStatus::Finalized((header03.hash(), 0)),
		]
	);
	assert_eq!(
		xt2_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header04.hash(), 0)),
			TransactionStatus::Finalized((header04.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_delayed_finalization_does_not_retract() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 200);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 200);

	let header01 = api.push_block(1, vec![], true);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);

	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let header03 = api.push_block_with_parent(header02.hash(), vec![xt1.clone()], true);

	block_on(pool.maintain(new_best_block_event(&pool, None, header02.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(header02.hash()), header03.hash())));

	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header02.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, header02.hash(), header03.hash())));

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).collect::<Vec<_>>();

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0)),
		]
	);
	assert_eq!(
		xt1_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 0)),
			TransactionStatus::Finalized((header03.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_best_block_after_finalization_does_not_retract() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	api.set_nonce(api.genesis_hash(), Bob.into(), 200);
	api.set_nonce(api.genesis_hash(), Charlie.into(), 200);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Bob, 200);

	let header01 = api.push_block(1, vec![], true);

	let xt0_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let header02 = api.push_block_with_parent(header01.hash(), vec![xt0.clone()], true);

	let xt1_watcher = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let header03 = api.push_block_with_parent(header02.hash(), vec![xt1.clone()], true);

	block_on(pool.maintain(finalized_block_event(&pool, api.genesis_hash(), header01.hash())));
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header03.hash())));
	block_on(pool.maintain(new_best_block_event(&pool, Some(api.genesis_hash()), header02.hash())));

	let xt0_status = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	let xt1_status = futures::executor::block_on_stream(xt1_watcher).collect::<Vec<_>>();

	log::info!("xt0_status: {:#?}", xt0_status);
	log::info!("xt1_status: {:#?}", xt1_status);

	assert_eq!(
		xt0_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0)),
		]
	);
	assert_eq!(
		xt1_status,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 0)),
			TransactionStatus::Finalized((header03.hash(), 0)),
		]
	);
}

#[test]
fn fap_watcher_invalid_many_revalidation() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

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

	let xt0_events = futures::executor::block_on_stream(xt0_watcher).collect::<Vec<_>>();
	let xt1_events = futures::executor::block_on_stream(xt1_watcher).collect::<Vec<_>>();
	let xt2_events = futures::executor::block_on_stream(xt2_watcher).collect::<Vec<_>>();
	let xt3_events = futures::executor::block_on_stream(xt3_watcher).collect::<Vec<_>>();
	let xt4_events = futures::executor::block_on_stream(xt4_watcher).collect::<Vec<_>>();

	log::info!("xt0_events: {:#?}", xt0_events);
	log::info!("xt1_events: {:#?}", xt1_events);
	log::info!("xt2_events: {:#?}", xt2_events);
	log::info!("xt3_events: {:#?}", xt3_events);
	log::info!("xt4_events: {:#?}", xt4_events);

	assert_eq!(
		xt0_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 0)),
			TransactionStatus::Finalized((header03.hash(), 0))
		],
	);
	assert_eq!(
		xt1_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 1)),
			TransactionStatus::Finalized((header03.hash(), 1))
		],
	);
	assert_eq!(
		xt2_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header03.hash(), 2)),
			TransactionStatus::Finalized((header03.hash(), 2))
		],
	);
	assert_eq!(xt3_events, vec![TransactionStatus::Ready, TransactionStatus::Invalid],);
	assert_eq!(xt4_events, vec![TransactionStatus::Ready, TransactionStatus::Invalid],);
}

#[test]
fn should_not_retain_invalid_hashes_from_retracted() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());
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

	assert_eq!(
		futures::executor::block_on_stream(watcher).collect::<Vec<_>>(),
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02a.hash(), 0)),
			TransactionStatus::Invalid
		],
	);

	//todo: shall revalidation check finalized (fork's tip) view?
	assert_eq!(pool.status_all()[&header02b.hash()].ready, 0);
}

#[test]
fn should_revalidate_during_maintenance() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());
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

	//todo: shall revalidation check finalized (fork's tip) view?
	assert_eq!(pool.status_all()[&header02.hash()].ready, 1);

	assert_eq!(
		futures::executor::block_on_stream(watcher).collect::<Vec<_>>(),
		vec![TransactionStatus::Ready, TransactionStatus::Invalid],
	);
}

#[test]
fn fap_transactions_purging_stale_on_finalization_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let xt1 = uxt(Alice, 200);
	let xt2 = uxt(Alice, 201);
	let xt3 = uxt(Alice, 202);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let watcher1 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let watcher2 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_eq!(api.validation_requests().len(), 3);
	assert_eq!(pool.status_all()[&header01.hash()].ready, 3);
	assert_eq!(pool.mempool_len(), (1, 2));

	let header02 = api.push_block(2, vec![xt1.clone(), xt2.clone(), xt3.clone()], true);
	api.set_nonce(header02.hash(), Alice.into(), 203);
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header02.hash())));

	assert_eq!(pool.status_all()[&header02.hash()].ready, 0);
	assert_eq!(pool.mempool_len(), (0, 0));

	let xt1_events = futures::executor::block_on_stream(watcher1).collect::<Vec<_>>();
	let xt2_events = futures::executor::block_on_stream(watcher2).collect::<Vec<_>>();
	assert_eq!(
		xt1_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 0)),
			TransactionStatus::Finalized((header02.hash(), 0))
		],
	);
	assert_eq!(
		xt2_events,
		vec![
			TransactionStatus::Ready,
			TransactionStatus::InBlock((header02.hash(), 1)),
			TransactionStatus::Finalized((header02.hash(), 1))
		],
	);
}

#[test]
fn fap_transactions_purging_invalid_on_finalization_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let xt1 = uxt(Alice, 200);
	let xt2 = uxt(Alice, 201);
	let xt3 = uxt(Alice, 202);

	let header01 = api.push_block(1, vec![], true);
	block_on(pool.maintain(new_best_block_event(&pool, None, header01.hash())));

	let watcher1 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let watcher2 = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt2.clone())).unwrap();
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt3.clone())).unwrap();

	assert_eq!(api.validation_requests().len(), 3);
	assert_eq!(pool.status_all()[&header01.hash()].ready, 3);
	assert_eq!(pool.mempool_len(), (1, 2));

	let header02 = api.push_block(2, vec![], true);
	api.add_invalid(&xt1);
	api.add_invalid(&xt2);
	api.add_invalid(&xt3);
	block_on(pool.maintain(finalized_block_event(&pool, header01.hash(), header02.hash())));

	//todo: should it work at all? (it requires better revalidation: mempool keeping validated txs)
	//additionally it also requires revalidation of finalized view.
	// assert_eq!(pool.status_all()[&header02.hash()].ready, 0);
	assert_eq!(pool.mempool_len(), (0, 0));

	let xt1_events = futures::executor::block_on_stream(watcher1).collect::<Vec<_>>();
	let xt2_events = futures::executor::block_on_stream(watcher2).collect::<Vec<_>>();
	assert_eq!(xt1_events, vec![TransactionStatus::Ready, TransactionStatus::Invalid]);
	assert_eq!(xt2_events, vec![TransactionStatus::Ready, TransactionStatus::Invalid]);
}

#[test]
fn import_sink_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let genesis = api.genesis_hash();
	let header01a = api.push_block(1, vec![], true);
	let header01b = api.push_block(1, vec![], true);

	let import_stream = pool.import_notification_stream();

	let event = new_best_block_event(&pool, None, header01a.hash());
	block_on(pool.maintain(event));

	let event = new_best_block_event(&pool, None, header01b.hash());
	block_on(pool.maintain(event));

	api.set_nonce(header01b.hash(), Alice.into(), 202);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(genesis, SOURCE, xt0.clone()),
		pool.submit_one(genesis, SOURCE, xt1.clone()),
	];

	block_on(futures::future::join_all(submissions));

	assert_pool_status!(header01a.hash(), &pool, 1, 1);
	assert_pool_status!(header01b.hash(), &pool, 1, 0);

	let import_events =
		futures::executor::block_on_stream(import_stream).take(2).collect::<Vec<_>>();

	let expected_import_events = vec![api.hash_and_length(&xt0).0, api.hash_and_length(&xt1).0];
	assert!(import_events.iter().all(|v| expected_import_events.contains(v)));
}

#[test]
fn import_sink_works2() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let genesis = api.genesis_hash();
	let header01a = api.push_block(1, vec![], true);
	let header01b = api.push_block(1, vec![], true);

	let import_stream = pool.import_notification_stream();

	let event = new_best_block_event(&pool, None, header01a.hash());
	block_on(pool.maintain(event));

	let event = new_best_block_event(&pool, None, header01b.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(genesis, SOURCE, xt0.clone()),
		pool.submit_one(genesis, SOURCE, xt1.clone()),
	];

	block_on(futures::future::join_all(submissions));

	assert_pool_status!(header01a.hash(), &pool, 1, 1);
	assert_pool_status!(header01b.hash(), &pool, 1, 1);

	let import_events =
		futures::executor::block_on_stream(import_stream).take(1).collect::<Vec<_>>();

	let expected_import_events = vec![api.hash_and_length(&xt0).0];
	assert_eq!(import_events, expected_import_events);
}

#[test]
fn import_sink_works3() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let (pool, _) = create_basic_pool(api.clone());

	let import_stream = pool.import_notification_stream();
	let genesis = api.genesis_hash();

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(genesis, SOURCE, xt0.clone()),
		pool.submit_one(genesis, SOURCE, xt1.clone()),
	];

	let x = block_on(futures::future::join_all(submissions));

	let header01a = api.push_block(1, vec![], true);
	let header01b = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01a.hash());
	block_on(pool.maintain(event));

	let event = new_best_block_event(&pool, None, header01b.hash());
	block_on(pool.maintain(event));

	assert_pool_status!(header01a.hash(), &pool, 1, 1);
	assert_pool_status!(header01b.hash(), &pool, 1, 1);

	log::info!("xxx {x:#?}");

	let import_events =
		futures::executor::block_on_stream(import_stream).take(1).collect::<Vec<_>>();

	let expected_import_events = vec![api.hash_and_length(&xt0).0];
	assert_eq!(import_events, expected_import_events);
}

//todo: add test: check len of filter after finalization (!)
