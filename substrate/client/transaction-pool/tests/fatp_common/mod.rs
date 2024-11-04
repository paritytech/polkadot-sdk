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

use sc_transaction_pool::{ChainApi, PoolLimit};
use sc_transaction_pool_api::ChainEvent;
use sp_runtime::transaction_validity::TransactionSource;
use std::sync::Arc;
use substrate_test_runtime_client::{
	runtime::{Block, Hash, Header},
	AccountKeyring::*,
};
use substrate_test_runtime_transaction_pool::{uxt, TestApi};
pub const LOG_TARGET: &str = "txpool";

use sc_transaction_pool::ForkAwareTxPool;

pub fn invalid_hash() -> Hash {
	Default::default()
}

pub fn new_best_block_event(
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

pub fn finalized_block_event(
	pool: &ForkAwareTxPool<TestApi, Block>,
	from: Hash,
	to: Hash,
) -> ChainEvent<Block> {
	let t = pool.api().tree_route(from, to).expect("Tree route exists");

	let e = t.enacted().iter().map(|h| h.hash).collect::<Vec<_>>();
	ChainEvent::Finalized { hash: to, tree_route: Arc::from(&e[0..e.len() - 1]) }
}

pub struct TestPoolBuilder {
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
	pub fn new() -> Self {
		Self::default()
	}

	pub fn with_api(mut self, api: Arc<TestApi>) -> Self {
		self.api = Some(api);
		self
	}

	pub fn with_mempool_count_limit(mut self, mempool_count_limit: usize) -> Self {
		self.mempool_max_transactions_count = mempool_count_limit;
		self.use_default_limits = false;
		self
	}

	pub fn with_ready_count(mut self, ready_count: usize) -> Self {
		self.ready_limits.count = ready_count;
		self.use_default_limits = false;
		self
	}

	pub fn with_ready_bytes_size(mut self, ready_bytes_size: usize) -> Self {
		self.ready_limits.total_bytes = ready_bytes_size;
		self.use_default_limits = false;
		self
	}

	pub fn with_future_count(mut self, future_count: usize) -> Self {
		self.future_limits.count = future_count;
		self.use_default_limits = false;
		self
	}

	pub fn with_future_bytes_size(mut self, future_bytes_size: usize) -> Self {
		self.future_limits.total_bytes = future_bytes_size;
		self.use_default_limits = false;
		self
	}

	pub fn build(
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

pub fn pool_with_api(
	test_api: Arc<TestApi>,
) -> (ForkAwareTxPool<TestApi, Block>, futures::executor::ThreadPool) {
	let builder = TestPoolBuilder::new();
	let (pool, _, threadpool) = builder.with_api(test_api).build();
	(pool, threadpool)
}

pub fn pool() -> (ForkAwareTxPool<TestApi, Block>, Arc<TestApi>, futures::executor::ThreadPool) {
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

#[macro_export]
macro_rules! assert_ready_iterator {
	($hash:expr, $pool:expr, [$( $xt:expr ),+]) => {{
		let ready_iterator = $pool.ready_at($hash).now_or_never().unwrap();
		let expected = vec![ $($pool.api().hash_and_length(&$xt).0),+];
		let output: Vec<_> = ready_iterator.collect();
		log::debug!(target:LOG_TARGET, "expected: {:#?}", expected);
		log::debug!(target:LOG_TARGET, "output: {:#?}", output);
		assert_eq!(expected.len(), output.len());
		assert!(
			output.iter().zip(expected.iter()).all(|(o,e)| {
				o.hash == *e
			})
		);
	}};
}

pub const SOURCE: TransactionSource = TransactionSource::External;

#[cfg(test)]
pub mod test_chain_with_forks {
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

	pub fn print_block(api: Arc<TestApi>, hash: Hash) {
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
	fn test_chain_works() {
		sp_tracing::try_init_simple();
		let (api, f) = chain(None);
		log::debug!("forks: {f:#?}");
		f[0].iter().for_each(|h| print_block(api.clone(), h.hash()));
		f[1].iter().for_each(|h| print_block(api.clone(), h.hash()));
		let tr = api.tree_route(f[0][5].hash(), f[1][5].hash()).unwrap();
		log::debug!("{:#?}", tr);
		log::debug!("e:{:#?}", tr.enacted());
		log::debug!("r:{:#?}", tr.retracted());
	}
}
