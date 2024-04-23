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
	hex_string,
	transaction::{TransactionBlock, TransactionEvent},
};
use assert_matches::assert_matches;
use codec::Encode;
use jsonrpsee::rpc_params;
use sc_transaction_pool_api::{ChainEvent, MaintainedTransactionPool};
use sp_core::H256;
use std::sync::Arc;
use substrate_test_runtime_client::AccountKeyring::*;
use substrate_test_runtime_transaction_pool::uxt;

// Test helpers.
use crate::transaction::tests::setup::{setup_api_tx, ALICE_NONCE};

#[tokio::test]
async fn tx_invalid_bytes() {
	let (_api, _pool, _client_mock, tx_api, _exec_middleware, _pool_middleware) = setup_api_tx();

	// This should not rely on the tx pool state.
	let mut sub = tx_api
		.subscribe_unbounded("transactionWatch_v1_submitAndWatch", rpc_params![&"0xdeadbeef"])
		.await
		.unwrap();

	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_matches!(event, TransactionEvent::Invalid(_));
}

#[tokio::test]
async fn tx_in_finalized() {
	let (api, pool, client, tx_api, _exec_middleware, _pool_middleware) = setup_api_tx();
	let block_1_header = api.push_block(1, vec![], true);
	client.set_best_block(block_1_header.hash(), 1);

	let uxt = uxt(Alice, ALICE_NONCE);
	let xt = hex_string(&uxt.encode());

	let mut sub = tx_api
		.subscribe_unbounded("transactionWatch_v1_submitAndWatch", rpc_params![&xt])
		.await
		.unwrap();

	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(event, TransactionEvent::Validated);

	// Import block 2 with the transaction included.
	let block_2_header = api.push_block(2, vec![uxt.clone()], true);
	let block_2 = block_2_header.hash();

	// Announce block 2 to the pool.
	let event = ChainEvent::NewBestBlock { hash: block_2, tree_route: None };
	pool.inner_pool.maintain(event).await;
	let event = ChainEvent::Finalized { hash: block_2, tree_route: Arc::from(vec![]) };
	pool.inner_pool.maintain(event).await;

	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(
		event,
		TransactionEvent::BestChainBlockIncluded(Some(TransactionBlock {
			hash: block_2,
			index: 0
		}))
	);
	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(event, TransactionEvent::Finalized(TransactionBlock { hash: block_2, index: 0 }));
}

#[tokio::test]
async fn tx_with_pruned_best_block() {
	let (api, pool, client, tx_api, _exec_middleware, _pool_middleware) = setup_api_tx();
	let block_1_header = api.push_block(1, vec![], true);
	client.set_best_block(block_1_header.hash(), 1);

	let uxt = uxt(Alice, ALICE_NONCE);
	let xt = hex_string(&uxt.encode());

	let mut sub = tx_api
		.subscribe_unbounded("transactionWatch_v1_submitAndWatch", rpc_params![&xt])
		.await
		.unwrap();

	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(event, TransactionEvent::Validated);

	// Import block 2 with the transaction included.
	let block_2_header = api.push_block(2, vec![uxt.clone()], true);
	let block_2 = block_2_header.hash();
	let event = ChainEvent::NewBestBlock { hash: block_2, tree_route: None };
	pool.inner_pool.maintain(event).await;

	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(
		event,
		TransactionEvent::BestChainBlockIncluded(Some(TransactionBlock {
			hash: block_2,
			index: 0
		}))
	);

	// Import block 2 again without the transaction included.
	let block_2_header = api.push_block(2, vec![], true);
	let block_2 = block_2_header.hash();
	let event = ChainEvent::NewBestBlock { hash: block_2, tree_route: None };
	pool.inner_pool.maintain(event).await;

	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(event, TransactionEvent::BestChainBlockIncluded(None));

	let block_2_header = api.push_block(2, vec![uxt.clone()], true);
	let block_2 = block_2_header.hash();
	let event = ChainEvent::NewBestBlock { hash: block_2, tree_route: None };
	pool.inner_pool.maintain(event).await;

	// The tx is validated again against the new block.
	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(event, TransactionEvent::Validated);

	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(
		event,
		TransactionEvent::BestChainBlockIncluded(Some(TransactionBlock {
			hash: block_2,
			index: 0
		}))
	);

	let event = ChainEvent::Finalized { hash: block_2, tree_route: Arc::from(vec![]) };
	pool.inner_pool.maintain(event).await;
	let event: TransactionEvent<H256> = get_next_event_sub!(&mut sub);
	assert_eq!(event, TransactionEvent::Finalized(TransactionBlock { hash: block_2, index: 0 }));
}
