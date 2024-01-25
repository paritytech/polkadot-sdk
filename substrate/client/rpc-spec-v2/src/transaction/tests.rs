use super::*;
use crate::{
	chain_head::test_utils::ChainHeadMockClient, hex_string,
	transaction::TransactionBroadcast as RpcTransactionBroadcast,
};
use assert_matches::assert_matches;
use codec::Encode;
use futures::Future;
use jsonrpsee::{core::error::Error, rpc_params, RpcModule};
use sc_transaction_pool::*;
use sc_transaction_pool_api::{ChainEvent, MaintainedTransactionPool, TransactionPool};
use sp_core::testing::TaskExecutor;
use std::{pin::Pin, sync::Arc, time::Duration};
use substrate_test_runtime_client::{prelude::*, AccountKeyring::*, Client};
use substrate_test_runtime_transaction_pool::{uxt, TestApi};

type Block = substrate_test_runtime_client::runtime::Block;

/// Initial Alice account nonce.
const ALICE_NONCE: u64 = 209;

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

fn setup_api() -> (
	Arc<TestApi>,
	Arc<BasicPool<TestApi, Block>>,
	Arc<ChainHeadMockClient<Client<Backend>>>,
	RpcModule<
		TransactionBroadcast<BasicPool<TestApi, Block>, ChainHeadMockClient<Client<Backend>>>,
	>,
) {
	let (pool, api, _) = maintained_pool();
	let pool = Arc::new(pool);

	let builder = TestClientBuilder::new();
	let client = Arc::new(builder.build());
	let client_mock = Arc::new(ChainHeadMockClient::new(client.clone()));

	let tx_api = RpcTransactionBroadcast::new(
		client_mock.clone(),
		pool.clone(),
		Arc::new(TaskExecutor::default()),
	)
	.into_rpc();

	(api, pool, client_mock, tx_api)
}

#[tokio::test]
async fn tx_broadcast_enters_pool() {
	let (api, pool, client_mock, tx_api) = setup_api();

	// Start at block 1.
	let block_1_header = api.push_block(1, vec![], true);

	let uxt = uxt(Alice, ALICE_NONCE);
	let xt = hex_string(&uxt.encode());

	let operation_id: String =
		tx_api.call("transaction_unstable_broadcast", rpc_params![&xt]).await.unwrap();

	// Announce block 1 to `transaction_unstable_broadcast`.
	client_mock.trigger_import_stream(block_1_header).await;

	// Ensure the tx propagated from `transaction_unstable_broadcast` to the transaction pool.

	// TODO: Improve testability by extending the `transaction_unstable_broadcast` with
	// a middleware trait that intercepts the transaction status for testing.
	let mut num_retries = 12;
	while num_retries > 0 && pool.status().ready != 1 {
		tokio::time::sleep(Duration::from_secs(5)).await;
		num_retries -= 1;
	}
	assert_eq!(1, pool.status().ready);
	assert_eq!(uxt.encode().len(), pool.status().ready_bytes);

	// Import block 2 with the transaction included.
	let block_2_header = api.push_block(2, vec![uxt.clone()], true);
	let block_2 = block_2_header.hash();

	// Announce block 2 to the pool.
	let event = ChainEvent::NewBestBlock { hash: block_2, tree_route: None };
	pool.maintain(event).await;

	assert_eq!(0, pool.status().ready);

	// Stop call can still be made.
	let _: () = tx_api
		.call("transaction_unstable_stop", rpc_params![&operation_id])
		.await
		.unwrap();
}

#[tokio::test]
async fn tx_broadcast_invalid_tx() {
	let (_, _, _, tx_api) = setup_api();

	// Invalid parameters.
	let err = tx_api
		.call::<_, serde_json::Value>("transaction_unstable_broadcast", [1u8])
		.await
		.unwrap_err();
	assert_matches!(err,
		Error::Call(err) if err.code() == super::error::json_rpc_spec::INVALID_PARAM_ERROR && err.message() == "Invalid params"
	);

	// Invalid transaction that cannot be decoded. The broadcast silently exits.
	let xt = "0xdeadbeef";
	let operation_id: String =
		tx_api.call("transaction_unstable_broadcast", rpc_params![&xt]).await.unwrap();

	// Make an invalid stop call.
	let err = tx_api
		.call::<_, serde_json::Value>("transaction_unstable_stop", ["invalid_operation_id"])
		.await
		.unwrap_err();
	assert_matches!(err,
		Error::Call(err) if err.code() == super::error::json_rpc_spec::INVALID_PARAM_ERROR && err.message() == "Invalid operation id"
	);

	// Ensure stop can be called, the tx was decoded and the broadcast future terminated.
	let _: () = tx_api
		.call("transaction_unstable_stop", rpc_params![&operation_id])
		.await
		.unwrap();
}
