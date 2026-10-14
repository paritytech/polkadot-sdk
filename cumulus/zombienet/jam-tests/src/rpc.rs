// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The two JSON-RPC clients the harness needs: one for a JAM node, one for a collator.
//!
//! The JAM node's client lives in `cumulus-zombienet-sdk-helpers`, whose backing-agnostic helpers
//! need it too; it is re-exported here.

use anyhow::Context;
use cumulus_zombienet_sdk_helpers::jam_rpc::connect;
pub use cumulus_zombienet_sdk_helpers::jam_rpc::{provide_validation_code, JamRpc};
use jsonrpsee::{core::client::ClientT, rpc_params, ws_client::WsClient};
use serde_json::Value;
use tokio::time::Instant;

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
		let header: Value = self
			.client
			.request("chain_getHeader", params)
			.await
			.context("chain_getHeader")?;
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

	/// `state_getStorage(key, None)` at the best block, hex-decoded.
	pub async fn storage(&self, key: &str) -> anyhow::Result<Vec<u8>> {
		let value: Value = self
			.client
			.request("state_getStorage", rpc_params![key, Value::Null])
			.await
			.context("state_getStorage")?;
		let hex = match value.as_str() {
			Some(hex) => hex,
			None => return Err(anyhow::anyhow!("state_getStorage({key}) returned {value:?}")),
		};
		sp_core::bytes::from_hex(hex).with_context(|| format!("decoding {hex} as hex"))
	}

	/// `author_rotateKeys` — run `SessionKeys_generate_session_keys` against the node's keystore
	/// and return the public session keys as the hex string the JSON-RPC `Bytes` encoding uses.
	pub async fn rotate_keys(&self) -> anyhow::Result<String> {
		let value: Value = self
			.client
			.request("author_rotateKeys", rpc_params![])
			.await
			.context("author_rotateKeys")?;
		value
			.as_str()
			.map(|hex| hex.to_owned())
			.with_context(|| format!("author_rotateKeys returned {value:?}"))
	}
}
