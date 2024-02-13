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

use crate::{
	chain_head::test_utils::ChainHeadMockClient,
	transaction::{
		api::TransactionBroadcastApiServer,
		tests::executor::{TaskExecutorBroadcast, TaskExecutorRecv},
		TransactionBroadcast as RpcTransactionBroadcast,
	},
};
use futures::Future;
use jsonrpsee::RpcModule;
use sc_transaction_pool::*;
use std::{pin::Pin, sync::Arc};
use substrate_test_runtime_client::{prelude::*, Client};
use substrate_test_runtime_transaction_pool::TestApi;

use crate::transaction::tests::middleware_pool::{MiddlewarePool, MiddlewarePoolRecv};

pub type Block = substrate_test_runtime_client::runtime::Block;

/// Initial Alice account nonce.
pub const ALICE_NONCE: u64 = 209;

fn create_basic_pool_with_genesis(
	test_api: Arc<TestApi>,
) -> (BasicPool<TestApi, Block>, Pin<Box<dyn Future<Output = ()> + Send>>) {
	let genesis_hash = {
		test_api
			.chain()
			.read()
			.block_by_number
			.get(&0)
			.map(|blocks| blocks[0].0.header.hash())
			.expect("there is block 0. qed")
	};
	BasicPool::new_test(test_api, genesis_hash, genesis_hash)
}

fn maintained_pool() -> (BasicPool<TestApi, Block>, Arc<TestApi>, futures::executor::ThreadPool) {
	let api = Arc::new(TestApi::with_alice_nonce(ALICE_NONCE));
	let (pool, background_task) = create_basic_pool_with_genesis(api.clone());

	let thread_pool = futures::executor::ThreadPool::new().unwrap();
	thread_pool.spawn_ok(background_task);
	(pool, api, thread_pool)
}

pub fn setup_api() -> (
	Arc<TestApi>,
	Arc<MiddlewarePool>,
	Arc<ChainHeadMockClient<Client<Backend>>>,
	RpcModule<RpcTransactionBroadcast<MiddlewarePool, ChainHeadMockClient<Client<Backend>>>>,
	TaskExecutorRecv,
	MiddlewarePoolRecv,
) {
	let (pool, api, _) = maintained_pool();
	let (pool, pool_recv) = MiddlewarePool::new(Arc::new(pool).clone());
	let pool = Arc::new(pool);

	let builder = TestClientBuilder::new();
	let client = Arc::new(builder.build());
	let client_mock = Arc::new(ChainHeadMockClient::new(client.clone()));

	let (task_executor, executor_recv) = TaskExecutorBroadcast::new();

	let tx_api =
		RpcTransactionBroadcast::new(client_mock.clone(), pool.clone(), Arc::new(task_executor))
			.into_rpc();

	(api, pool, client_mock, tx_api, executor_recv, pool_recv)
}
