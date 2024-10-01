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

//! Tests for fork-aware transaction pool.

use futures::{executor::block_on, task::Poll, FutureExt, StreamExt};
use sc_transaction_pool::{ChainApi, PoolLimit};
use sc_transaction_pool_api::{
	error::{Error as TxPoolError, IntoPoolError},
	ChainEvent, MaintainedTransactionPool, TransactionPool, TransactionStatus,
};
use sp_runtime::transaction_validity::{InvalidTransaction, TransactionSource};
use std::{sync::Arc, time::Duration};
use substrate_test_runtime_client::{
	runtime::{Block, Hash, Header},
	AccountKeyring::*,
};
use substrate_test_runtime_transaction_pool::{uxt, TestApi};
const LOG_TARGET: &str = "txpool";

use sc_transaction_pool::ForkAwareTxPool;

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

struct TestPoolBuilder {
	api: Option<Arc<TestApi>>,
	use_default_limits: bool,
	ready_limits: sc_transaction_pool::PoolLimit,
	future_limits: sc_transaction_pool::PoolLimit,
	mempool_max_transactions_count: usize,
}

impl Default for TestPoolBuilder {
	fn default() -> Self {
		Self {
			api: None,
			use_default_limits: true,
			ready_limits: PoolLimit { count: 8192, total_bytes: 20 * 1024 * 1024 },
			future_limits: PoolLimit { count: 512, total_bytes: 1 * 1024 * 1024 },
			mempool_max_transactions_count: usize::MAX,
		}
	}
}

impl TestPoolBuilder {
	fn new() -> Self {
		Self::default()
	}

	fn with_api(mut self, api: Arc<TestApi>) -> Self {
		self.api = Some(api);
		self
	}

	fn with_mempool_count_limit(mut self, mempool_count_limit: usize) -> Self {
		self.mempool_max_transactions_count = mempool_count_limit;
		self.use_default_limits = false;
		self
	}

	#[allow(dead_code)]
	fn with_ready_count(mut self, ready_count: usize) -> Self {
		self.ready_limits.count = ready_count;
		self.use_default_limits = false;
		self
	}

	#[allow(dead_code)]
	fn with_ready_bytes_size(mut self, ready_bytes_size: usize) -> Self {
		self.ready_limits.total_bytes = ready_bytes_size;
		self.use_default_limits = false;
		self
	}

	#[allow(dead_code)]
	fn with_future_count(mut self, future_count: usize) -> Self {
		self.future_limits.count = future_count;
		self.use_default_limits = false;
		self
	}

	#[allow(dead_code)]
	fn with_future_bytes_size(mut self, future_bytes_size: usize) -> Self {
		self.future_limits.total_bytes = future_bytes_size;
		self.use_default_limits = false;
		self
	}

	fn build(
		self,
	) -> (ForkAwareTxPool<TestApi, Block>, Arc<TestApi>, futures::executor::ThreadPool) {
		let api = self
			.api
			.unwrap_or(Arc::from(TestApi::with_alice_nonce(200).enable_stale_check()));

		let genesis_hash = api
			.chain()
			.read()
			.block_by_number
			.get(&0)
			.map(|blocks| blocks[0].0.header.hash())
			.expect("there is block 0. qed");

		let (pool, txpool_task) = if self.use_default_limits {
			ForkAwareTxPool::new_test(api.clone(), genesis_hash, genesis_hash)
		} else {
			ForkAwareTxPool::new_test_with_limits(
				api.clone(),
				genesis_hash,
				genesis_hash,
				self.ready_limits,
				self.future_limits,
				self.mempool_max_transactions_count,
			)
		};

		let thread_pool = futures::executor::ThreadPool::new().unwrap();
		thread_pool.spawn_ok(txpool_task);

		(pool, api, thread_pool)
	}
}

fn pool_with_api(
	test_api: Arc<TestApi>,
) -> (ForkAwareTxPool<TestApi, Block>, futures::executor::ThreadPool) {
	let builder = TestPoolBuilder::new();
	let (pool, _, threadpool) = builder.with_api(test_api).build();
	(pool, threadpool)
}

fn pool() -> (ForkAwareTxPool<TestApi, Block>, Arc<TestApi>, futures::executor::ThreadPool) {
	let builder = TestPoolBuilder::new();
	builder.build()
}

#[macro_export]
macro_rules! assert_pool_status {
	($hash:expr, $pool:expr, $ready:expr, $future:expr) => {
		{
			log::debug!(target:LOG_TARGET, "stats: {:#?}", $pool.status_all());
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
		//       (fork 0)
		//     F01 - F02 - F03 - F04 - F05 | Alice nonce increasing, alice's txs
		//    /
		// F00
		//    \  (fork 1)
		//     F11 - F12 - F13 - F14 - F15 | Bob nonce increasing, Bob's txs
		//
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
					log::debug!("{},{} -> add", fork, block);
					vec![uxt(account, (200 + block - 1) as u64)]
				} else {
					log::debug!("{},{} -> skip", fork, block);
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
		log::debug!(
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
		log::debug!("forks: {f:#?}");
		f[0].iter().for_each(|h| print_block(api.clone(), h.hash()));
		f[1].iter().for_each(|h| print_block(api.clone(), h.hash()));
		let tr = api.tree_route(f[0][4].hash(), f[1][3].hash());
		log::debug!("{:#?}", tr);
		if let Ok(tr) = tr {
			log::debug!("e:{:#?}", tr.enacted());
			log::debug!("r:{:#?}", tr.retracted());
		}
	}
}

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

	let mut submission = block_on(pool.submit_and_watch(invalid_hash(), SOURCE, xt3.clone()));
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
	assert_eq!(xt2_status, vec![TransactionStatus::Ready,]);
}
