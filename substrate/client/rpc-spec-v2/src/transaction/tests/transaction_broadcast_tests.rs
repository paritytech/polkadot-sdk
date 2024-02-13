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

use crate::{hex_string, transaction::error::json_rpc_spec};
use assert_matches::assert_matches;
use codec::Encode;
use jsonrpsee::{core::error::Error, rpc_params};
use sc_transaction_pool_api::{ChainEvent, MaintainedTransactionPool, TransactionPool};
use substrate_test_runtime_client::AccountKeyring::*;
use substrate_test_runtime_transaction_pool::uxt;

// Test helpers.
use crate::transaction::tests::{
	middleware_pool::{MiddlewarePoolEvent, TxStatusTypeTest},
	setup::{setup_api, ALICE_NONCE},
};

#[tokio::test]
async fn tx_broadcast_enters_pool() {
	let (api, pool, client_mock, tx_api, mut exec_middleware, mut pool_middleware) = setup_api();

	// Start at block 1.
	let block_1_header = api.push_block(1, vec![], true);

	let uxt = uxt(Alice, ALICE_NONCE);
	let xt = hex_string(&uxt.encode());

	let operation_id: String =
		tx_api.call("transaction_unstable_broadcast", rpc_params![&xt]).await.unwrap();

	// Announce block 1 to `transaction_unstable_broadcast`.
	client_mock.trigger_import_stream(block_1_header).await;

	// Ensure the tx propagated from `transaction_unstable_broadcast` to the transaction pool.
	let event = get_next_event!(&mut pool_middleware);
	assert_eq!(
		event,
		MiddlewarePoolEvent::TransactionStatus {
			transaction: xt.clone(),
			status: TxStatusTypeTest::Ready
		}
	);

	assert_eq!(1, pool.inner_pool.status().ready);
	assert_eq!(uxt.encode().len(), pool.inner_pool.status().ready_bytes);

	// Import block 2 with the transaction included.
	let block_2_header = api.push_block(2, vec![uxt.clone()], true);
	let block_2 = block_2_header.hash();

	// Announce block 2 to the pool.
	let event = ChainEvent::NewBestBlock { hash: block_2, tree_route: None };
	pool.inner_pool.maintain(event).await;
	assert_eq!(0, pool.inner_pool.status().ready);

	let event = get_next_event!(&mut pool_middleware);
	assert_eq!(
		event,
		MiddlewarePoolEvent::TransactionStatus {
			transaction: xt.clone(),
			status: TxStatusTypeTest::InBlock((block_2, 0))
		}
	);

	// The future broadcast awaits for the finalized status to be reached.
	// Force the future to exit by calling stop.
	let _: () = tx_api
		.call("transaction_unstable_stop", rpc_params![&operation_id])
		.await
		.unwrap();

	// Ensure the broadcast future finishes.
	let _ = get_next_event!(&mut exec_middleware);
}

#[tokio::test]
async fn tx_broadcast_invalid_tx() {
	let (_, pool, _, tx_api, mut exec_middleware, _) = setup_api();

	// Invalid parameters.
	let err = tx_api
		.call::<_, serde_json::Value>("transaction_unstable_broadcast", [1u8])
		.await
		.unwrap_err();
	assert_matches!(err,
		Error::Call(err) if err.code() == json_rpc_spec::INVALID_PARAM_ERROR && err.message() == "Invalid params"
	);

	assert_eq!(0, pool.status().ready);

	// Invalid transaction that cannot be decoded. The broadcast silently exits.
	let xt = "0xdeadbeef";
	let operation_id: String =
		tx_api.call("transaction_unstable_broadcast", rpc_params![&xt]).await.unwrap();

	assert_eq!(0, pool.status().ready);

	// Await the broadcast future to exit.
	// Without this we'd be subject to races, where we try to call the stop before the tx is
	// dropped.
	let _ = get_next_event!(&mut exec_middleware);

	// The broadcast future was dropped, and the operation is no longer active.
	// When the operation is not active, either from the tx being finalized or a
	// terminal error; the stop method should return an error.
	let err = tx_api
		.call::<_, serde_json::Value>("transaction_unstable_stop", rpc_params![&operation_id])
		.await
		.unwrap_err();
	assert_matches!(err,
		Error::Call(err) if err.code() == json_rpc_spec::INVALID_PARAM_ERROR && err.message() == "Invalid operation id"
	);
}

#[tokio::test]
async fn tx_invalid_stop() {
	let (_, _, _, tx_api, _, _) = setup_api();

	// Make an invalid stop call.
	let err = tx_api
		.call::<_, serde_json::Value>("transaction_unstable_stop", ["invalid_operation_id"])
		.await
		.unwrap_err();
	assert_matches!(err,
		Error::Call(err) if err.code() == json_rpc_spec::INVALID_PARAM_ERROR && err.message() == "Invalid operation id"
	);
}
