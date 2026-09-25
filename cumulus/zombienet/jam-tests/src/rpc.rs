// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

//! The two JSON-RPC clients the harness needs: one for a JAM node, one for a collator.

use anyhow::Context;
use jam_types::{AnyBytes, AnyHash, AnyVec};
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
		match WsClientBuilder::default()
			// The collator's `:code` read is a ~14 MB hex response, above jsonrpsee's 10 MB
			// default cap.
			.max_response_size(128 * 1024 * 1024)
			.build(url)
			.await
		{
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
		let block: Value = self
			.client
			.request("finalizedBlock", rpc_params![])
			.await
			.context("finalizedBlock")?;
		Ok(block["slot"].as_u64().unwrap_or(0))
	}

	/// The header hash of the latest finalized block.
	///
	/// This is the lookup anchor a preimage read has to name: only a finalized block may be a
	/// work package's anchor, so it is also the block a validator resolves the preimage from.
	/// polkajam spells the call `finalizedBlock`; a JAM node has no substrate
	/// `chain_getFinalizedHead`.
	pub async fn finalized_header_hash(&self) -> anyhow::Result<Value> {
		let block: Value = self
			.client
			.request("finalizedBlock", rpc_params![])
			.await
			.context("finalizedBlock")?;
		Ok(block["header_hash"].clone())
	}

	/// The header hash of the best block, which is the block every state read is taken at.
	pub async fn best_block_hash(&self) -> anyhow::Result<Value> {
		let best: Value =
			self.client.request("bestBlock", rpc_params![]).await.context("bestBlock")?;
		Ok(best["header_hash"].clone())
	}

	/// What service `service` has stored under `key` in the posterior state of block `at`.
	///
	/// This is the read the collator makes (`cumulus/jam/rpc-interface`), on the harness's own
	/// connection. `None` is an answer and not a failure: it means the service has no entry under
	/// that key. The two byte strings travel as [`AnyVec`] and [`AnyBytes`] so the base64 the JAM
	/// RPC speaks comes from polkajam's own serde rather than from a second spelling of it here.
	pub async fn service_value(
		&self,
		at: &Value,
		service: u32,
		key: &[u8],
	) -> anyhow::Result<Option<Vec<u8>>> {
		let value: Option<AnyBytes> = self
			.client
			.request("serviceValue", rpc_params![at, service, AnyVec(key.to_vec())])
			.await
			.context("serviceValue")?;
		Ok(value.map(|bytes| bytes.0.to_vec()))
	}

	/// Submit `blob` as the preimage service `service` is requesting.
	///
	/// The blob travels as [`AnyBytes`], so polkajam's own serde is what spells it as the base64
	/// the JAM RPC speaks. The call returns as soon as the node accepts the blob; it does not
	/// wait for the preimage to be integrated, so the caller has to read
	/// [`Self::service_request`] back at a finalized block to see the provision land.
	///
	/// A blob above `jam_types::MAX_PREIMAGE_BLOB_LEN` is refused with polkajam's `BlobTooLarge`
	/// error.
	pub async fn submit_preimage(&self, service: u32, blob: &[u8]) -> anyhow::Result<()> {
		self.client
			.request("submitPreimage", rpc_params![service, AnyBytes(blob.to_vec().into())])
			.await
			.context("submitPreimage")
	}

	/// The preimage request `(hash, len)` of service `service` in the posterior state of `at`.
	///
	/// `None` means the service has neither requested nor been provided the preimage;
	/// `Some([])` means it has been requested and not yet provided; a non-empty list is the
	/// slots the request went through — provided, forgotten, requested again.
	pub async fn service_request(
		&self,
		at: &Value,
		service: u32,
		hash: &[u8; 32],
		len: u32,
	) -> anyhow::Result<Option<Vec<u64>>> {
		let slots: Option<Vec<u64>> = self
			.client
			.request("serviceRequest", rpc_params![at, service, AnyHash(*hash), len])
			.await
			.context("serviceRequest")?;
		Ok(slots)
	}
}

/// How long the whole preimage step may take, covering both waits below. The request only
/// appears once the upgrade block accumulates and the provision only lands at a later finalized
/// block, so this is a few slots on a healthy network; the bound is loose enough for a loaded CI
/// machine and only exists so a stuck helper fails the test instead of hanging it.
const PROVIDE_VALIDATION_CODE_TIMEOUT: Duration = Duration::from_secs(240);

/// Gap between polls. Neither wait can advance more than once per block.
const PROVIDE_VALIDATION_CODE_POLL: Duration = Duration::from_secs(3);

/// The manual preimage step of a JAM runtime upgrade: wait for the parachain service to request
/// the new validation code, provide it with `submitPreimage`, and wait until JAM holds it at a
/// finalized block.
///
/// This is the out-of-band "manual intervention" of the JAM code-upgrade lifecycle (service
/// design §5.2 phase 3): refine emits `RequestCodeUpgrade`, accumulate arms
/// `ParaInfo.announced_upgrade`, and *someone outside the node* has to hand JAM the code. No node
/// or collator code may call [`JamRpc::submit_preimage`] — tests call this helper instead, the
/// way an operator would.
///
/// Both waits read the same request back, because `serviceRequest` is the only place the
/// lifecycle is visible:
///
/// * `None` — no request: the block that emitted `RequestCodeUpgrade` has not accumulated yet.
/// * `Some([])` — requested but not provided: the expected state before the submission.
/// * `Some([slot])` — provided at `slot`.
/// * `Some([a, b])` — forgotten.
/// * `Some([a, b, c])` — requested again and re-provided.
///
/// The request wait reads the **best** block: the soliciting block need not be finalized yet.
/// The provision wait fetches a fresh **finalized** anchor every poll and accepts only
/// `Some([slot])`, because a finalized block is what a work package may name as its lookup
/// anchor and therefore what a validator resolves the code from.
pub async fn provide_validation_code(
	jam: &JamRpc,
	service: u32,
	code: &[u8],
) -> anyhow::Result<()> {
	// The same hash the runtime derives when it calls `host::request_code_upgrade` in
	// `jam_validate_block`: blake2b-256 of the code, with its length.
	let hash = jam_std_common::hash_raw(code);
	let len = code.len() as u32;
	let request = format!("(0x{}, {len})", array_bytes::bytes2hex("", hash));
	let deadline = Instant::now() + PROVIDE_VALIDATION_CODE_TIMEOUT;

	// The service has to ask before anyone may provide: wait for the `RequestCodeUpgrade` block
	// to accumulate and arm the request.
	let mut last;
	loop {
		let best = jam.best_block_hash().await.context("bestBlock")?;
		last = jam.service_request(&best, service, &hash, len).await?;
		if last.is_some() {
			break;
		}
		anyhow::ensure!(
			Instant::now() < deadline,
			"service {service} never requested validation code {request}; the last \
			 serviceRequest answer was {last:?}"
		);
		sleep(PROVIDE_VALIDATION_CODE_POLL).await;
	}
	log::info!("service {service} requests validation code {request} ({last:?}); providing it");

	jam.submit_preimage(service, code).await?;

	// Only `[slot]` means provided; `[]` is still unprovided, `[a, b]` forgotten and `[a, b, c]`
	// a re-provision, none of which a fresh upgrade should ever see.
	let mut last;
	loop {
		let finalized = jam.finalized_header_hash().await.context("finalizedBlock")?;
		last = jam.service_request(&finalized, service, &hash, len).await?;
		if last.as_deref().map(<[u64]>::len) == Some(1) {
			log::info!("validation code {request} is provided at a finalized anchor");
			return Ok(());
		}
		anyhow::ensure!(
			Instant::now() < deadline,
			"validation code {request} was submitted but is not provided at a finalized anchor; \
			 the last serviceRequest answer was {last:?} ([] = requested, [a, b] = forgotten, \
			 [a, b, c] = re-provided)"
		);
		sleep(PROVIDE_VALIDATION_CODE_POLL).await;
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
