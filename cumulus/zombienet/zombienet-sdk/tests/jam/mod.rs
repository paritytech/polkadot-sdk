// This file is part of Cumulus.

// Copyright (C) Parity Technologies (UK) Ltd.
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

#![cfg(feature = "jam")]

use anyhow::Context;
use codec::DecodeAll;
pub use cumulus_jam_zombienet_tests::para::Para;
use cumulus_jam_zombienet_tests::{para_head::read_para_head, rpc::JamRpc};
use parachain_service_core::{para_info_key, types::ParaId, ParaInfo};
use sp_crypto_hashing::blake2_256;
use sp_runtime::{generic::Header, traits::BlakeTwo256};
use std::time::Duration;
use tokio::time::{sleep, Instant};
use zombienet_sdk::{LocalFileSystem, Network};

/// The RPC URL of the spawned JAM network's ordinary node, `jam-or`.
///
/// Read back from the network handle, not pinned: zombienet-sdk 0.5.0 registers the JAM nodes
/// alongside the substrate ones, but they are a different node kind, so `Network::get_node`
/// does not find them — `Network::get_jam_node` does.
pub fn jam_rpc_url(network: &Network<LocalFileSystem>) -> anyhow::Result<String> {
	Ok(network
		.get_jam_node(cumulus_jam_zombienet_tests::network::ORDINARY_NODE)?
		.ws_uri())
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
	let hash = blake2_256(code);
	let len = code.len() as u32;
	let request = format!("(0x{}, {len})", hex::encode(hash));
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

/// Gap between accumulated-head polls; the head cannot advance more than once per JAM slot.
const HEAD_POLL: Duration = Duration::from_secs(3);

/// Wait until JAM has accumulated a head of at least `target` for `para`.
///
/// This is the read the old `jam-tests` harness made (`JamNetwork::para_head`), rebuilt on the
/// public [`JamRpc`]: `serviceValue` at the JAM best block, under the key the parachain service
/// files a para's [`ParaInfo`] at, then the header in `head_data`. The collator's own best-block
/// metric is not a faithful stand-in: it holds its slot while a package is overdue, so its height
/// trails the accumulated head and would let the assertion below fail on a healthy run.
pub async fn wait_for_jam_head(
	rpc: &JamRpc,
	service_id: u32,
	para: u32,
	target: u64,
	budget: Duration,
) -> anyhow::Result<()> {
	let deadline = Instant::now() + budget;
	let key = para_info_key(ParaId(para));
	let mut last = None;
	loop {
		let at = rpc.best_block_hash().await.context("bestBlock")?;
		if let Some(stored) = rpc.service_value(&at, service_id, &key).await? {
			let info = ParaInfo::decode_all(&mut &stored[..]).with_context(|| {
				format!("decoding {} bytes as the service's ParaInfo", stored.len())
			})?;
			let head = info.head_data.into_inner();
			let header =
				Header::<u32, BlakeTwo256>::decode_all(&mut &head[..]).with_context(|| {
					format!("decoding {} bytes of head_data as a header", head.len())
				})?;
			let number = u64::from(header.number);
			log::info!("JAM accumulated head for para {para}: #{number}");
			if number >= target {
				return Ok(());
			}
			last = Some(number);
		}
		anyhow::ensure!(
			Instant::now() < deadline,
			"JAM did not accumulate head #{target} for para {para} within {budget:?}; the last \
			 accumulated head was {last:?}"
		);
		sleep(HEAD_POLL).await;
	}
}

/// Wait until JAM's accumulated head for `para` has not increased for `still_for`.
///
/// Standing still is what a stall looks like from the chain: nothing announces that packages
/// stopped being reported, the head simply stops moving. The core tests use this to confirm a
/// parked core really stopped carrying the para's work; it mirrors the old harness's
/// `Run::wait_for_frozen_jam_head`, reading the accumulated head through [`read_para_head`].
///
/// The head is a number, so "not increased" and "unchanged" coincide, and no head at all
/// (`None`) is unchanged for as long as it stays absent — the caller decides whether that
/// counts as a stall.
pub async fn wait_for_frozen_jam_head(
	rpc: &JamRpc,
	service_id: u32,
	para: u32,
	still_for: Duration,
	budget: Duration,
) -> anyhow::Result<()> {
	let deadline = Instant::now() + budget;
	// The last head and the instant it was first seen. Reset the instant whenever the head
	// changes, so the elapsed time measured is only ever time spent on the current head.
	let mut frozen: Option<(Option<u64>, Instant)> = None;
	loop {
		let head = read_para_head(rpc, service_id, para).await?.map(|head| head.number);
		match &frozen {
			Some((last, since)) if *last == head && since.elapsed() >= still_for => {
				log::info!(
					"JAM's accumulated head for para {para} has stood still at {head:?} for \
					 {still_for:?}"
				);
				return Ok(());
			},
			Some((last, _)) if *last == head => {},
			_ => frozen = Some((head, Instant::now())),
		}
		anyhow::ensure!(
			Instant::now() < deadline,
			"JAM's accumulated head for para {para} did not stand still for {still_for:?} within \
			 {budget:?}; the last accumulated head was {head:?}"
		);
		sleep(HEAD_POLL).await;
	}
}

mod collator_progress;
mod core_assignment;
mod demo;
mod polkavm_authoring;
mod resubmission;
