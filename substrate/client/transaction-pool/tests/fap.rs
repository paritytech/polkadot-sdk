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
use sc_transaction_pool_api::{ChainEvent, MaintainedTransactionPool, TransactionPool};
use sp_runtime::transaction_validity::TransactionSource;
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::{Block, Hash, Header},
	AccountKeyring::*,
};
use substrate_test_runtime_transaction_pool::{uxt, TestApi};

const LOG_TARGET: &str = "txpool";

use sc_transaction_pool::fork_aware_pool::ForkAwareTxPool;

fn invalid_hash() -> Hash {
	Default::default()
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

#[test]
fn fap_one_view_future_and_ready_submit_one() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

	let header01a = api.push_block(1, vec![], true);
	// let header01b = api.push_block(1, vec![], true);

	let event = ChainEvent::NewBestBlock { hash: header01a.hash(), tree_route: None };
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
fn fap_one_view_future_and_ready_submit_many() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

	let header01a = api.push_block(1, vec![], true);
	// let header01b = api.push_block(1, vec![], true);

	let event = ChainEvent::NewBestBlock { hash: header01a.hash(), tree_route: None };
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
fn fap_two_views_future_and_ready_sumbit_one() {
	sp_tracing::try_init_simple();

	let api = Arc::from(TestApi::with_alice_nonce(200).enable_stale_check());
	let pool = create_basic_pool(api.clone());

	let genesis = api.genesis_hash();
	let header01a = api.push_block(1, vec![], true);
	let header01b = api.push_block(1, vec![], true);

	let event = ChainEvent::NewBestBlock { hash: header01a.hash(), tree_route: None };
	block_on(pool.maintain(event));

	let event = ChainEvent::NewBestBlock { hash: header01b.hash(), tree_route: None };
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

	let event = ChainEvent::NewBestBlock { hash: header01a.hash(), tree_route: None };
	block_on(pool.maintain(event));

	let event = ChainEvent::NewBestBlock { hash: header01b.hash(), tree_route: None };
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
fn fap_lin_poc() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let pool = create_basic_pool(api.clone());

	let f00 = forks[0][0].hash();
	let f13 = forks[1][3].hash();

	let event = ChainEvent::NewBestBlock { hash: f00, tree_route: None };
	block_on(pool.maintain(event));

	let xt0 = uxt(Bob, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];

	block_on(futures::future::join_all(submissions));

	let event = ChainEvent::NewBestBlock {
		hash: f13,
		tree_route: api.tree_route(f00, f13).ok().map(Into::into),
	};
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
fn fap_fork_poc() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(None);
	let pool = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	let event = ChainEvent::NewBestBlock { hash: f03, tree_route: None };
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

	let event = ChainEvent::NewBestBlock {
		hash: f13,
		tree_route: api.tree_route(f03, f13).ok().map(Into::into),
	};
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
fn fap_fork_stale_poc() {
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(Some(&|f, b| match (f, b) {
		(0, _) => false,
		_ => true,
	}));
	let pool = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	let event = ChainEvent::NewBestBlock { hash: f03, tree_route: None };
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
		sc_transaction_pool_api::error::Error::InvalidTransaction(
			sp_runtime::transaction_validity::InvalidTransaction::Stale,
		)
	));

	let event = ChainEvent::NewBestBlock {
		hash: f13,
		tree_route: api.tree_route(f03, f13).ok().map(Into::into),
	};
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
fn fap_fork_no_xts_ready_to_future() {
	//this scenario w/o xts is not likely to happen, but similar thing (xt changing from ready to
	//future) could occur e.g. when runtime was updated on fork1.
	sp_tracing::try_init_simple();

	let (api, forks) = test_chain_with_forks::chain(Some(&|f, b| match (f, b) {
		_ => false,
	}));
	let pool = create_basic_pool(api.clone());

	let f03 = forks[0][3].hash();
	let f13 = forks[1][3].hash();

	let event = ChainEvent::NewBestBlock { hash: f03, tree_route: None };
	block_on(pool.maintain(event));

	let xt0 = uxt(Alice, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = ChainEvent::NewBestBlock {
		hash: f13,
		tree_route: api.tree_route(f03, f13).ok().map(Into::into),
	};
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

	let event = ChainEvent::NewBestBlock { hash: f03, tree_route: None };
	block_on(pool.maintain(event));

	assert!(pool.ready_at(f03).now_or_never().is_some());

	let xt0 = uxt(Alice, 203);
	let submissions = vec![pool.submit_one(invalid_hash(), SOURCE, xt0.clone())];
	block_on(futures::future::join_all(submissions));

	let event = ChainEvent::NewBestBlock {
		hash: f13,
		tree_route: api.tree_route(f03, f13).ok().map(Into::into),
	};
	log::info!(target:LOG_TARGET, "event: {:#?}", event);
	assert!(pool.ready_at(f13).now_or_never().is_none());
	block_on(pool.maintain(event));
	assert!(pool.ready_at(f03).now_or_never().is_some());
	assert!(pool.ready_at(f13).now_or_never().is_some());
}
