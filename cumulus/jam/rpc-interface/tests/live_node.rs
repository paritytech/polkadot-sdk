// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Network tests against a locally running polkajam node. Ignored by default; run with a node
//! up (`polkajam-testnet --num-ordinary-nodes 1`, RPC on 19800) via `cargo test -- --ignored`.
//! Override the endpoint with `JAM_RPC_URL`.

use cumulus_jam_interface::{JamChainSource, JamStateSource};
use cumulus_jam_rpc_interface::JamRpcInterface;
use futures::StreamExt;
use url::Url;

async fn connect() -> JamRpcInterface {
	let url = std::env::var("JAM_RPC_URL").unwrap_or_else(|_| "ws://127.0.0.1:19800".into());
	let url = Url::parse(&url).expect("valid JAM_RPC_URL");
	let (interface, worker) = JamRpcInterface::new(vec![url]).await.expect("node reachable");
	tokio::spawn(worker);
	interface
}

#[tokio::test]
#[ignore = "needs a running polkajam node (JAM_RPC_URL)"]
async fn chain_following_works() {
	let interface = connect().await;

	let best = interface.best_block().await.expect("best block");
	let finalized = interface.finalized_block().await.expect("finalized block");
	assert!(finalized.slot <= best.slot);

	let mut best_stream = interface.best_block_stream().await.expect("best stream");
	let next = tokio::time::timeout(std::time::Duration::from_secs(30), best_stream.next())
		.await
		.expect("a best block within 30s")
		.expect("stream open");
	assert!(next.slot >= best.slot);

	let parent = interface.parent(next.header_hash).await.expect("parent");
	assert!(parent.slot < next.slot);
	interface.state_root(parent.header_hash).await.expect("state root");
	interface.beefy_root(parent.header_hash).await.expect("beefy root");
}

#[tokio::test]
#[ignore = "needs a running polkajam node (JAM_RPC_URL)"]
async fn auth_queues_scan_works() {
	let interface = connect().await;
	let best = interface.best_block().await.expect("best block");
	let anchor = interface.parent(best.header_hash).await.expect("anchor");

	let queues = interface.auth_queues(anchor.header_hash).await.expect("auth queues");
	assert!(queues.iter().next().is_some());
	interface.auth_pools(anchor.header_hash).await.expect("auth pools");
}
