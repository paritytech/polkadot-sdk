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
use std::{sync::Arc, vec};
use substrate_test_runtime_client::Sr25519Keyring::*;
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

#[tokio::test]
async fn tx_slow_client_replace_old_messages() {
	let (api, pool, client, tx_api, _exec_middleware, _pool_middleware) = setup_api_tx();
	let block_1_header = api.push_block(1, vec![], true);
	client.set_best_block(block_1_header.hash(), 1);

	let uxt = uxt(Alice, ALICE_NONCE);
	let xt = hex_string(&uxt.encode());

	// The subscription itself has a buffer of length 1 and no way to create
	// it without a buffer.
	//
	// Then `transactionWatch` has its own buffer of length 3 which leads to
	// that it's limited to 5 items in the tests.
	//
	// 1. Send will complete immediately
	// 2. Send will be pending in the subscription sink (not possible to cancel)
	// 3. The rest of messages will be kept in a RingBuffer and older messages are replaced by newer
	//    items.
	let mut sub = tx_api
		.subscribe("transactionWatch_v1_submitAndWatch", rpc_params![&xt], 1)
		.await
		.unwrap();

	// Import block 2 with the transaction included.
	let block = api.push_block(2, vec![uxt.clone()], true);
	let block_hash = block.hash();
	let event = ChainEvent::NewBestBlock { hash: block_hash, tree_route: None };
	pool.inner_pool.maintain(event).await;

	let mut block2_hash = None;

	// Import block 2 again without the transaction included.
	for _ in 0..10 {
		let block_not_imported = api.push_block(2, vec![], true);
		let event = ChainEvent::NewBestBlock { hash: block_not_imported.hash(), tree_route: None };
		pool.inner_pool.maintain(event).await;

		let block2 = api.push_block(2, vec![uxt.clone()], true);
		block2_hash = Some(block2.hash());
		let event = ChainEvent::NewBestBlock { hash: block2.hash(), tree_route: None };

		pool.inner_pool.maintain(event).await;
	}

	let block2_hash = block2_hash.unwrap();

	// Finalize the transaction
	let event = ChainEvent::Finalized { hash: block2_hash, tree_route: Arc::from(vec![]) };
	pool.inner_pool.maintain(event).await;

	// Hack to mimic a slow client.
	tokio::time::sleep(std::time::Duration::from_secs(10)).await;

	// Read the events.
	let mut res: Vec<TransactionEvent<_>> = Vec::new();

	while let Some(item) = tokio::time::timeout(std::time::Duration::from_secs(5), sub.next())
		.await
		.unwrap()
	{
		let (ev, _) = item.unwrap();
		res.push(ev);
	}

	// BestBlockIncluded(None) is dropped and not seen.
	let exp = vec![
		// First message
		TransactionEvent::Validated,
		// Second message
		TransactionEvent::BestChainBlockIncluded(Some(TransactionBlock {
			hash: block_hash,
			index: 0,
		})),
		// Most recent 3 messages.
		TransactionEvent::Validated,
		TransactionEvent::BestChainBlockIncluded(Some(TransactionBlock {
			hash: block2_hash,
			index: 0,
		})),
		TransactionEvent::Finalized(TransactionBlock { hash: block2_hash, index: 0 }),
	];

	assert_eq!(res, exp);
}
