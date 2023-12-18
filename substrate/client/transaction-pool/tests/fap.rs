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

use futures::executor::block_on;
use sc_transaction_pool_api::{ChainEvent, MaintainedTransactionPool, TransactionPool};
use sp_runtime::transaction_validity::TransactionSource;
use std::sync::Arc;
use substrate_test_runtime_client::{runtime::Block, AccountKeyring::*};
use substrate_test_runtime_transaction_pool::{uxt, TestApi};

const LOG_TARGET: &str = "txpool";

use sc_transaction_pool::fork_aware_pool::ForkAwareTxPool;

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

//todo:
//Add some more tests:
// - view.ready iterator

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

	let invalid_hash = Default::default();

	let submissions = vec![
		pool.submit_at(invalid_hash, SOURCE, xts0.clone()),
		pool.submit_at(invalid_hash, SOURCE, xts1.clone()),
		pool.submit_at(invalid_hash, SOURCE, xts2.clone()),
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
