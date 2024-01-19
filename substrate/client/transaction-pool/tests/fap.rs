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

use futures::{executor::block_on, FutureExt};
use sc_transaction_pool::ChainApi;
use sc_transaction_pool_api::{
	error::Error as TxPoolError, ChainEvent, MaintainedTransactionPool, TransactionPool,
};
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource, UnknownTransaction};
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::{Block, Hash, Header},
	AccountKeyring::*,
};
use substrate_test_runtime_transaction_pool::{uxt, TestApi};
const LOG_TARGET: &str = "txpool";

use sc_transaction_pool::fork_aware_pool::ForkAwareTxPool;
use substrate_test_runtime::{Nonce, TransferData};

fn pool() -> (ForkAwareTxPool<TestApi, Block>, Arc<TestApi>) {
	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());
	(pool, api)
}

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

fn create_basic_pool_with_genesis(test_api: Arc<TestApi>) -> ForkAwareTxPool<TestApi, Block> {
	let genesis_hash = {
		test_api
			.chain()
			.read()
			.block_by_number
			.get(&0)
			.map(|blocks| blocks[0].0.header.hash())
			.expect("there is block 0. qed")
	};
	ForkAwareTxPool::new_test(test_api, genesis_hash, genesis_hash)
}

fn create_basic_pool(test_api: Arc<TestApi>) -> ForkAwareTxPool<TestApi, Block> {
	create_basic_pool_with_genesis(test_api)
}

const SOURCE: TransactionSource = TransactionSource::External;

#[cfg(test)]
mod test_chain_with_forks {
	use super::*;

	pub fn chain(
		include_xts: Option<&dyn Fn(usize, usize) -> bool>,
	) -> (Arc<TestApi>, Vec<Vec<Header>>) {
		//
		//     F01 - F02 - F03 - F04 - F05
		//    /
		// F00
		//    \
		//     F11 - F12 - F13 - F14 - F15

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
//
// todo: [validated_pool/pool related, probably can be reused]:
// fn prune_tags_should_work()
// fn should_ban_invalid_transactions()
// fn should_correctly_prune_transactions_providing_more_than_one_tag()
//
//
// fn resubmit_tx_of_fork_that_is_not_part_of_retracted()
// fn resubmit_from_retracted_fork()
// fn ready_set_should_not_resolve_before_block_update()
// fn ready_set_should_resolve_after_block_update()
// fn ready_set_should_eventually_resolve_when_block_update_arrives()
// fn import_notification_to_pool_maintain_works()
// fn pruning_a_transaction_should_remove_it_from_best_transaction()
// fn stale_transactions_are_pruned()
// fn finalized_only_handled_correctly()
// fn best_block_after_finalized_handled_correctly()
// fn switching_fork_with_finalized_works()
// fn switching_fork_multiple_times_works()
// fn two_blocks_delayed_finalization_works()
// fn delayed_finalization_does_not_retract()
// fn best_block_after_finalization_does_not_retract()
//
// watcher needed?
// fn should_revalidate_during_maintenance()
// fn should_not_retain_invalid_hashes_from_retracted()
// fn should_revalidate_across_many_blocks()
// fn should_push_watchers_during_maintenance()
// fn finalization()  //with_watcher!
// fn fork_aware_finalization()
// fn prune_and_retract_tx_at_same_time()
//
// review, difficult to unerstand:

#[test]
fn fap_no_view_future_and_ready_submit_one_fails() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

	let header01a = api.push_block(1, vec![], true);

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(header01a.hash(), SOURCE, xt0.clone()),
		pool.submit_one(header01a.hash(), SOURCE, xt1.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));

	assert!(results.iter().all(|r| {
		matches!(
			&r.as_ref().unwrap_err().0,
			TxPoolError::UnknownTransaction(UnknownTransaction::CannotLookup,)
		)
	}));
}

#[test]
fn fap_no_view_future_and_ready_submit_many_fails() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

	let header01a = api.push_block(1, vec![], true);

	let xts0 = (200..205).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts1 = (205..210).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts2 = (215..220).map(|i| uxt(Alice, i)).collect::<Vec<_>>();

	let submissions = vec![
		pool.submit_at(header01a.hash(), SOURCE, xts0.clone()),
		pool.submit_at(header01a.hash(), SOURCE, xts1.clone()),
		pool.submit_at(header01a.hash(), SOURCE, xts2.clone()),
	];

	let results = block_on(futures::future::join_all(submissions));

	assert!(results.into_iter().flat_map(|x| x.unwrap()).all(|r| {
		matches!(
			&r.as_ref().unwrap_err().0,
			TxPoolError::UnknownTransaction(UnknownTransaction::CannotLookup,)
		)
	}));
}

#[test]
fn fap_one_view_future_and_ready_submit_one_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

	let header01a = api.push_block(1, vec![], true);
	// let header01b = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01a.hash());
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	let xt1 = uxt(Alice, 202);

	let submissions = vec![
		pool.submit_one(header01a.hash(), SOURCE, xt0.clone()),
		pool.submit_one(header01a.hash(), SOURCE, xt1.clone()),
	];

	block_on(futures::future::join_all(submissions));

	log::info!(target:LOG_TARGET, "stats: {:?}", pool.status_all());

	let status = &pool.status_all()[&header01a.hash()];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 1);
}

#[test]
fn fap_one_view_future_and_ready_submit_many_works() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

	let header01a = api.push_block(1, vec![], true);
	// let header01b = api.push_block(1, vec![], true);

	let event = new_best_block_event(&pool, None, header01a.hash());
	block_on(pool.maintain(event));

	let xts0 = (200..205).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts1 = (205..210).map(|i| uxt(Alice, i)).collect::<Vec<_>>();
	let xts2 = (215..220).map(|i| uxt(Alice, i)).collect::<Vec<_>>();

	let submissions = vec![
		pool.submit_at(header01a.hash(), SOURCE, xts0.clone()),
		pool.submit_at(header01a.hash(), SOURCE, xts1.clone()),
		pool.submit_at(header01a.hash(), SOURCE, xts2.clone()),
	];

	block_on(futures::future::join_all(submissions));

	log::info!(target:LOG_TARGET, "stats: {:?}", pool.status_all());

	let status = &pool.status_all()[&header01a.hash()];
	assert_eq!(status.ready, 10);
	assert_eq!(status.future, 5);
}

#[test]
fn fap_one_view_stale_submit_one_fails() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

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

	let status = &pool.status_all()[&header.hash()];
	assert_eq!(status.ready, 0);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_one_view_stale_submit_many_fails() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

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

	//xts2 contains one ready transaction
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

	let status = &pool.status_all()[&header.hash()];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_one_view_future_turns_to_ready_works() {
	let (pool, api) = pool();

	let header = api.push_block(1, vec![], true);
	let at = header.hash();
	let event = new_best_block_event(&pool, None, at);
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 201);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	assert!(pool.ready(at).unwrap().count() == 0);
	let status = &pool.status_all()[&at];
	assert_eq!(status.ready, 0);
	assert_eq!(status.future, 1);

	let xt1 = uxt(Alice, 200);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt1.clone())).unwrap();
	let ready: Vec<_> = pool.ready(at).unwrap().map(|v| v.data.clone()).collect();
	assert_eq!(ready, vec![xt1, xt0]);
	let status = &pool.status_all()[&at];
	assert_eq!(status.ready, 2);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_one_view_ready_turns_to_stale_works() {
	let (pool, api) = pool();

	let header = api.push_block(1, vec![], true);
	let block1 = header.hash();
	let event = new_best_block_event(&pool, None, block1);
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 200);
	block_on(pool.submit_one(invalid_hash(), SOURCE, xt0.clone())).unwrap();
	let pending: Vec<_> = pool.ready(block1).unwrap().map(|v| v.data.clone()).collect();
	assert_eq!(pending, vec![xt0.clone()]);
	assert_eq!(pool.status_all()[&block1].ready, 1);

	// todo: xt0 shall become stale, and this does not neccesarily requires transaction in block 2.
	// nonce setting should be enough, but revalidation is required!
	let header = api.push_block(2, vec![uxt(Alice, 200)], true);
	let block2 = header.hash();
	api.set_nonce(block2, Alice.into(), 201);
	let event = new_best_block_event(&pool, Some(block1), block2);
	block_on(pool.maintain(event));
	let status = &pool.status_all()[&block2];
	assert!(pool.ready(block2).unwrap().count() == 0);
	assert_eq!(status.ready, 0);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_two_views_future_and_ready_sumbit_one() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

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

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let status = &pool.status_all()[&header01a.hash()];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 1);

	let status = &pool.status_all()[&header01b.hash()];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_two_views_future_and_ready_sumbit_many() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

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

	let status = &pool.status_all()[&header01a.hash()];
	assert_eq!(status.ready, 10);
	assert_eq!(status.future, 5);

	let status = &pool.status_all()[&header01b.hash()];
	assert_eq!(status.ready, 5);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_linear_progress() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let pool = create_basic_pool(api.clone());

	let f00 = forks[0][0].hash();
	let f13 = forks[1][3].hash();

	let event = new_best_block_event(&pool, None, f00);
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];

	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f00), f13);
	log::info!(target:LOG_TARGET, "event: {:#?}", event);
	block_on(pool.maintain(event));

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let status = &pool.status_all()[&f00];
	assert_eq!(status.ready, 0);
	assert_eq!(status.future, 1);

	let status = &pool.status_all()[&f13];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_fork_reorg() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let pool = create_basic_pool(api.clone());

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

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let status = &pool.status_all()[&f03];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 2);

	let status = &pool.status_all()[&f13];
	assert_eq!(status.ready, 6);
	assert_eq!(status.future, 0);

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

	let (pool, api) = pool();
	let genesis = api.genesis_hash();
	let event = new_best_block_event(&pool, None, genesis);
	block_on(pool.maintain(event));

	block_on(pool.submit_one(api.expect_hash_from_number(0), SOURCE, xt.clone()))
		.expect("1. Imported");
	assert_eq!(pool.status_all()[&genesis].ready, 1);

	let header = api.push_block(1, vec![xt.clone()], true);
	let fork_header = api.push_block(1, vec![xt], true);

	let event = new_best_block_event(&pool, Some(header.hash()), fork_header.hash());
	api.set_nonce(header.hash(), Alice.into(), 201);
	block_on(pool.maintain(event));
	assert_eq!(pool.status_all()[&fork_header.hash()].ready, 0);

	let event = new_best_block_event(&pool, Some(api.genesis_hash()), fork_header.hash());
	api.set_nonce(fork_header.hash(), Alice.into(), 201);
	block_on(pool.maintain(event));

	assert_eq!(pool.status_all()[&fork_header.hash()].ready, 0);
}

#[test]
fn fap_fork_stale_switch_to_future() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(Some(&|f, b| match (f, b) {
		(0, _) => false,
		_ => true,
	}));
	let pool = create_basic_pool(api.clone());

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

	//xt2 should be stale (todo:move to new test?)
	assert!(matches!(
		&submission_results[2].as_ref().unwrap_err().0,
		TxPoolError::InvalidTransaction(InvalidTransaction::Stale,)
	));

	let event = new_best_block_event(&pool, Some(f03), f13);
	log::info!(target:LOG_TARGET, "event: {:#?}", event);
	block_on(pool.maintain(event));

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let status = &pool.status_all()[&f03];
	assert_eq!(status.ready, 0);
	assert_eq!(status.future, 2);

	let status = &pool.status_all()[&f13];
	assert_eq!(status.ready, 2);
	assert_eq!(status.future, 1);

	let futures_f03 = pool.futures(f03).unwrap();
	let futures_f13 = pool.futures(f13).unwrap();
	let ready_f13 = pool.ready(f13).unwrap().collect::<Vec<_>>();
	assert!(futures_f13.iter().any(|v| v.data == xt2));
	assert!(futures_f03.iter().any(|v| v.data == xt0));
	assert!(futures_f03.iter().any(|v| v.data == xt1));
	assert!(ready_f13.iter().any(|v| v.data == xt0));
	assert!(ready_f13.iter().any(|v| v.data == xt1));
}

//todo - fix this test
#[test]
fn fap_fork_no_xts_ready_switch_to_future() {
	//this scenario w/o xts is not likely to happen, but similar thing (xt changing from ready to
	//future) could occur e.g. when runtime was updated on fork1.
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(Some(&|f, b| match (f, b) {
		_ => false,
	}));
	let pool = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	let event = new_best_block_event(&pool, None, f03);
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f03), f13);
	block_on(pool.maintain(event));

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let status = &pool.status_all()[&f03];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 0);

	// todo: xt0 shall become future, and this may only happen after view revalidation
	// let status = &pool.status_all()[&f13];
	// assert_eq!(status.ready, 0);
	// assert_eq!(status.future, 1);
}

#[test]
fn fap_ready_at_does_not_trigger() {
	//this scenario w/o xts is not likely to happen, but similar thing (xt changing from ready to
	//future) could occur e.g. when runtime was updated on fork1.
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let pool = create_basic_pool(api.clone());

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
	let pool = create_basic_pool(api.clone());

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
fn fap_linear_progress_finalization() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let pool = create_basic_pool(api.clone());

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
	let status = &pool.status_all()[&f12];
	assert_eq!(status.ready, 0);
	assert_eq!(status.future, 1);
	assert_eq!(pool.views_len(), 2);

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	let event = ChainEvent::Finalized { hash: f14, tree_route: Arc::from(vec![]) };
	block_on(pool.maintain(event));

	log::info!(target:LOG_TARGET, "stats: {:#?}", pool.status_all());

	assert_eq!(pool.views_len(), 1);
	let status = &pool.status_all()[&f14];
	assert_eq!(status.ready, 1);
	assert_eq!(status.future, 0);
}

#[test]
fn fap_fork_finalization_removes_stale_views() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let pool = create_basic_pool(api.clone());

	let f00 = forks[0][0].hash();
	let f12 = forks[1][2].hash();
	let f14 = forks[1][4].hash();
	let f02 = forks[0][2].hash();
	let f03 = forks[0][3].hash();
	let f04 = forks[0][4].hash();

	let event = new_best_block_event(&pool, None, f00);
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = new_best_block_event(&pool, Some(f00), f12);
	block_on(pool.maintain(event));
	let event = new_best_block_event(&pool, Some(f00), f14);
	block_on(pool.maintain(event));
	let event = new_best_block_event(&pool, Some(f00), f02);
	block_on(pool.maintain(event));

	assert_eq!(pool.views_len(), 4);

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
