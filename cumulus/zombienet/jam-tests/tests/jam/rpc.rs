// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The two JSON-RPC clients the harness needs: one for a JAM node, one for a collator.

use anyhow::Context;
use jsonrpsee::{
	core::client::ClientT,
	rpc_params,
	ws_client::{WsClient, WsClientBuilder},
};
use serde_json::Value;
use std::time::Duration;
use tokio::time::{sleep, Instant};

async fn connect(url: &str, deadline: Instant) -> anyhow::Result<WsClient> {
	let mut last_error = None;
	while Instant::now() < deadline {
		match WsClientBuilder::default().build(url).await {
			Ok(client) => return Ok(client),
			Err(error) => {
				last_error = Some(error);
				sleep(Duration::from_millis(500)).await;
			},
		}
	}
	Err(anyhow::anyhow!("{url} never accepted a connection: {last_error:?}"))
}

/// A JAM node's RPC. The methods are polkajam's own (see `jam-std-common/src/rpc.rs`), not
/// substrate's.
pub struct JamRpc {
	client: WsClient,
}

impl JamRpc {
	/// Connect, then wait until the node reports a synced chain that has moved past genesis.
	///
	/// A finalized block past genesis is what says the validators found each other and the chain
	/// is running; everything the collators need is already in its state by then.
	pub async fn wait_ready(url: &str, deadline: Instant) -> anyhow::Result<Self> {
		let rpc = JamRpc { client: connect(url, deadline).await? };

		while Instant::now() < deadline {
			let synced = rpc.sync_state().await.map(|state| state["status"] == "Completed");
			if synced.unwrap_or(false) && rpc.finalized_slot().await.unwrap_or(0) > 0 {
				return Ok(rpc);
			}
			sleep(Duration::from_secs(2)).await;
		}

		let state = rpc.sync_state().await;
		Err(anyhow::anyhow!("{url} did not finalize a block in time (syncState: {state:?})"))
	}

	async fn sync_state(&self) -> anyhow::Result<Value> {
		self.client.request("syncState", rpc_params![]).await.context("syncState")
	}

	/// The JAM timeslot of the latest finalized block. Slot 0 is genesis.
	pub async fn finalized_slot(&self) -> anyhow::Result<u64> {
		let block: Value =
			self.client.request("finalizedBlock", rpc_params![]).await.context("finalizedBlock")?;
		Ok(block["slot"].as_u64().unwrap_or(0))
	}

	/// The service ids known at the best block.
	pub async fn services(&self) -> anyhow::Result<Vec<u64>> {
		let best: Value =
			self.client.request("bestBlock", rpc_params![]).await.context("bestBlock")?;
		let services: Vec<u64> = self
			.client
			.request("listServices", rpc_params![best["header_hash"].clone()])
			.await
			.context("listServices")?;
		Ok(services)
	}
}

/// The height a substrate header carries, which the RPC spells as a hex string.
fn number_of(header: &Value) -> anyhow::Result<u64> {
	let number = header["number"].as_str().context("header has no number")?;
	u64::from_str_radix(number.trim_start_matches("0x"), 16)
		.with_context(|| format!("header number {number} is not hex"))
}

/// How far a collator's chain has got.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Height {
	pub best: u64,
	pub finalized: u64,
}

/// A collator's substrate RPC.
pub struct CollatorRpc {
	client: WsClient,
}

impl CollatorRpc {
	pub async fn connect(url: &str, deadline: Instant) -> anyhow::Result<Self> {
		Ok(CollatorRpc { client: connect(url, deadline).await? })
	}

	async fn header_number(&self, hash: Option<Value>) -> anyhow::Result<u64> {
		let params = match hash {
			Some(hash) => rpc_params![hash],
			None => rpc_params![],
		};
		let header: Value =
			self.client.request("chain_getHeader", params).await.context("chain_getHeader")?;
		number_of(&header)
	}

	/// The height of the block `hash`, or `None` if this node has never seen it.
	///
	/// Two parachains have disjoint block hashes, so "the collator of this para knows the head JAM
	/// accumulated for it" is what says an accumulated head belongs to this chain and to no other.
	pub async fn height_of(&self, hash: &str) -> anyhow::Result<Option<u64>> {
		let header: Value = self
			.client
			.request("chain_getHeader", rpc_params![hash])
			.await
			.context("chain_getHeader")?;
		if header.is_null() {
			return Ok(None);
		}
		number_of(&header).map(Some)
	}

	pub async fn height(&self) -> anyhow::Result<Height> {
		let best = self.header_number(None).await?;
		let finalized_hash: Value = self
			.client
			.request("chain_getFinalizedHead", rpc_params![])
			.await
			.context("chain_getFinalizedHead")?;
		let finalized = self.header_number(Some(finalized_hash)).await?;
		Ok(Height { best, finalized })
	}
}
