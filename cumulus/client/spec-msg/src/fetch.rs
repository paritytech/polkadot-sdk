// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! The receiver's fetch pipeline: from a monitor trigger to a verified pool.
//!
//! On every [`RelayProvidesEvent::Included`] `(source, root, _)` the
//! pipeline runs one *round* against the source's peers:
//!
//! - per consumed channel stream (`consumed_streams()`), chunked [`MessagesRequest`]s from the
//!   resume position (`base + received`) under exactly `root` — the MVP tier policy: the newest
//!   *included* root, dependency-free by definition. Every chunk is verified independently
//!   ([`verify_messages_response`]); a poisoned response discards the response and the peer and the
//!   chunk is refetched from the next peer. Verified runs land in the [`SpecMsgPool`], each round
//!   re-binding the entire pooled history under the round's root (a caught-up stream still gets one
//!   payload-free exchange — its refreshed extension/tree-proof pair is the lift material).
//! - per outbound channel (`out_channels()`), the peer's ack register head read: `EventRequest {
//!   at: None, under: root }`, verified ([`verify_event_response`]) and pooled with its inclusion
//!   proof (the inherent's shape) and read context (the lift's).
//!
//! A completed round marks `root` as the source's lift target; the inherent
//! provider hands over only target-bound material, which is what guarantees
//! the lift assembler succeeds from local material. A failed round is
//! retried under the SAME root with bounded exponential backoff
//! ([`MAX_ROUND_RETRIES`] / [`ROUND_RETRY_DELAY`]) — a quiet sender offers
//! no next root, so waiting for one would turn a single transient failure
//! into an indefinite stall. Fetching is root-keyed and idempotent, so
//! retries are always safe; a fresh included root supersedes a scheduled
//! retry and resets the budget. Rounds that cannot profit from a retry
//! (an empty peer set, local errors) wait for the next trigger instead —
//! and a round whose only failures are never-written-stream refusals is
//! not a failed round at all (see [`fetch_source`]): retrying those under
//! the same root is provably futile, recovery rides the next included root.
//!
//! This node-side pre-verification protects the collator from building on
//! bad p2p data; consensus-level enforcement is the PVF's lift check
//! (issue 05). [`RelayProvidesEvent::Pending`] prefetch hints are ignored
//! in the MVP: material fetched under a merely backed root cannot be
//! consumed or lifted, and keeping it out of the pool avoids poisoning the
//! contiguous runs on abandoned forks.

use std::{collections::HashMap, future::Future, pin::Pin, sync::Arc, time::Duration};

use futures::{stream::FuturesUnordered, StreamExt};
use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_blockchain::HeaderBackend;
use sp_runtime::traits::Block as BlockT;

use cumulus_primitives_core::{ParaId, SpecMsgApi};
use cumulus_primitives_spec_messaging::{
	ConsumedStream, EventRequest, ExchangeRequest, ExchangeResponse, MessagePosition,
	MessagesRequest, MessagesResponse, MmrFrontier, StreamId, StreamsRoot,
};

use crate::{
	exchange::{exchange_once, ExchangeError, ExchangeNetwork, SourcePeers},
	monitor::{RelayProvidesEvent, SourceProvides},
	pool::{ChannelBinding, PoolError, RegisterRead, SpecMsgPool},
	verify::{verify_event_response, verify_messages_response, VerifiedEvent, VerifiedMessages},
	LOG_TARGET,
};

/// Per-chunk payload byte bound of the fetch loop. Well under the server's
/// hard cap and the transport's response size; the loop resumes from
/// `base + received` until the stream's proven head under the round's root.
pub const FETCH_CHUNK_BYTES: u32 = 512 * 1024;

/// Hard bound on chunks per stream and round — termination guarantee
/// against a backlog that grows faster than it is fetched (the next round
/// continues where this one stopped).
const MAX_CHUNKS_PER_ROUND: usize = 4_096;

/// In-root retry budget: how many times one included root's failed round is
/// re-run before the source waits for its next trigger. A fresh included
/// root supersedes any scheduled retry and resets the budget; so does a
/// completed round. The budget keeps a quiet sender (no new `Provides`)
/// from stalling a source indefinitely on one transient failure, while the
/// bound guarantees no retry storm outlives the failure.
const MAX_ROUND_RETRIES: u32 = 4;

/// Backoff before the first in-root retry; doubles per attempt
/// ([`retry_delay`]) up to [`ROUND_RETRY_DELAY_CAP`].
const ROUND_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Upper bound on the in-root retry backoff.
const ROUND_RETRY_DELAY_CAP: Duration = Duration::from_secs(16);

/// The backoff before in-root retry `attempt` (1-based): exponential from
/// [`ROUND_RETRY_DELAY`], capped at [`ROUND_RETRY_DELAY_CAP`].
fn retry_delay(attempt: u32) -> Duration {
	// The shift is clamped: the cap is reached long before the type's width
	// matters.
	let factor = 1u32 << attempt.saturating_sub(1).min(16);
	ROUND_RETRY_DELAY.saturating_mul(factor).min(ROUND_RETRY_DELAY_CAP)
}

/// Errors of one fetch round. Retryable ones ([`RoundError::retryable`])
/// are re-run in place with bounded backoff; the rest wait for the next
/// trigger.
#[derive(Debug, thiserror::Error)]
pub enum FetchError {
	/// Every candidate peer failed or was discarded.
	#[error("exchange failed: {0}")]
	Exchange(#[from] ExchangeError),
	/// The verified chunk did not fit the pooled run — a local bug.
	#[error("pool rejected a verified chunk: {0}")]
	Pool(#[from] PoolError),
	/// The chunk loop hit its per-round bound.
	#[error("chunk bound exhausted; resuming next round")]
	ChunkBound,
}

/// Runs one fetch round for `source` under the included `root`: `channels`
/// are the consumed channel streams with the runtime's resume cursors,
/// `registers` the ack streams to head-read. On success the pool's lift
/// target for the source is `root`. The round's in-flight marker
/// ([`SpecMsgPool::begin_round`]) is the CALLER's job: the fetch loop marks
/// the source at trigger receipt — ahead of the offer→round-entry hop this
/// function sits behind — and holds the guard across the whole round (see
/// [`run_spec_msg_fetcher`]), which is what authoring's bounded grace
/// window keys on ([`SpecMsgPool::wait_for_in_flight_rounds`]).
///
/// Register reads are best-effort: an unpublished register is refused at
/// the transport level, indistinguishable from a failure — neither may
/// poison the round. Channel streams nothing was ever proven for (cursor 0,
/// no pooled run) get the same tolerance — a never-written stream is
/// unservable under every root until the source's first send, and refusing
/// it must not block the register reads that complete the handshake. Such
/// refusals are tolerated outright (`debug!`) and never fail the round: the
/// stream's emptiness under `root` is persistent, so an in-root retry is
/// provably futile, and the first send that makes the stream servable
/// arrives with a fresh included root — the round's natural next trigger.
pub async fn fetch_source(
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	pool: &SpecMsgPool,
	source: ParaId,
	root: StreamsRoot,
	channels: &[(StreamId, u64)],
	registers: &[StreamId],
	chunk_bytes: u32,
) -> Result<(), FetchError> {
	let (mut leaves, mut bytes) = (0u64, 0u64);
	for (stream, cursor) in channels {
		match fetch_channel_stream(
			network,
			peers,
			pool,
			source,
			root,
			*stream,
			*cursor,
			chunk_bytes,
		)
		.await
		{
			Ok((stream_leaves, stream_bytes)) => {
				leaves += stream_leaves;
				bytes += stream_bytes;
			},
			// A consumed stream nothing was ever proven for (cursor 0, no
			// pooled run) may simply have never been written: the archive
			// cannot prove an empty stream under a root, so the server
			// refuses at the transport level — indistinguishable from a
			// transient failure, and persistent for every root predating
			// the source's first send (the accept-before-open handshake
			// window). Such a stream must not poison the round: the
			// register reads below are what completes the handshake, and
			// aborting here can deadlock the channel when the source stays
			// otherwise idle (no new root ever retriggers). Nor is the
			// failure reported after the round: an in-root retry can never
			// serve what no root proves, and the first send that makes the
			// stream servable arrives with a fresh included root — the
			// round's natural next trigger. (The cost: a source whose
			// archiver lags its own first send also waits for that root.)
			Err(error @ FetchError::Exchange(ExchangeError::Network(_)))
				if *cursor == 0 && pool.resume(source, stream).is_none() =>
			{
				tracing::debug!(
					target: LOG_TARGET,
					%error,
					source = %u32::from(source),
					?stream,
					"Channel stream not served (never-written stream, or refusal); \
					 continuing the round — recovery rides the next included root",
				);
			},
			Err(error) => return Err(error),
		}
	}

	let mut register_reads = 0usize;
	for stream in registers {
		match fetch_register(network, peers, pool, source, root, *stream).await {
			Ok(()) => register_reads += 1,
			Err(error) => tracing::debug!(
				target: LOG_TARGET,
				%error,
				source = %u32::from(source),
				?stream,
				"Register head read not served (unpublished register, or no peer)",
			),
		}
	}

	pool.complete_round(source, root);
	if leaves == 0 && register_reads == 0 {
		tracing::debug!(
			target: LOG_TARGET,
			source = %u32::from(source),
			?root,
			"Fetch round completed (nothing new)",
		);
	} else {
		tracing::info!(
			target: LOG_TARGET,
			source = %u32::from(source),
			?root,
			streams = channels.len(),
			leaves,
			bytes,
			register_reads,
			"Fetch round completed",
		);
	}
	// The round is complete — everything fetched is bound to `root` and
	// handable. Unserved never-written streams were tolerated above and are
	// NOT reported: returning them here would feed the bounded in-root
	// retry, and every such retry re-runs a round that is futile by design
	// (same-millisecond "completed"/"failed; retrying" pairs, re-issuing
	// the register reads each time) while consuming the budget a genuinely
	// transient failure may later need.
	Ok(())
}

/// Fetches one channel stream's backlog under `root`, chunked and resumed
/// from `base + received`, into the pool. Returns the round's newly
/// received `(leaves, payload bytes)`.
async fn fetch_channel_stream(
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	pool: &SpecMsgPool,
	source: ParaId,
	root: StreamsRoot,
	stream: StreamId,
	cursor: u64,
	chunk_bytes: u32,
) -> Result<(u64, u64), FetchError> {
	// Resume from the pooled run where it covers the runtime cursor; a
	// stale or absent run restarts trust-free from the cursor (the
	// response's verified `start_peaks` seed the new run's base).
	pool.prune_channel(source, &stream, cursor);
	let (mut start, mut own) = match pool.resume(source, &stream) {
		Some((end, frontier)) => (end, Some(frontier)),
		None => (cursor, None),
	};

	let (mut leaves, mut bytes) = (0u64, 0u64);
	for _ in 0..MAX_CHUNKS_PER_ROUND {
		let request = MessagesRequest {
			stream,
			start: MessagePosition(start),
			under: root,
			max_bytes: chunk_bytes,
		};
		let (verified, response) =
			request_messages(network, peers, source, &request, own.as_ref()).await?;
		leaves += response.payloads.len() as u64;
		bytes += response.payloads.iter().map(|payload| payload.len() as u64).sum::<u64>();

		let base = own.clone().unwrap_or_else(|| MmrFrontier {
			leaf_count: response.base.0,
			peaks: response
				.start_peaks
				.clone()
				.try_into()
				.expect("verification checked the peak set (≤ 64 peaks); qed"),
		});
		let binding = ChannelBinding {
			root,
			head: verified.head,
			extension: response.extension,
			tree_proof: response.tree_proof,
		};
		let caught_up = verified.end.leaf_count >= verified.head;
		pool.note_chunk(source, stream, base, response.payloads, verified.end.clone(), binding)?;
		tracing::debug!(
			target: LOG_TARGET,
			source = %u32::from(source),
			?stream,
			received = verified.end.leaf_count,
			head = verified.head,
			"Fetched chunk",
		);

		if caught_up {
			return Ok((leaves, bytes));
		}
		start = verified.end.leaf_count;
		own = Some(verified.end);
	}

	Err(FetchError::ChunkBound)
}

/// One verified [`MessagesRequest`] round trip, rotating through the
/// source's peers: transport failures rotate, verification failures (and
/// valid-but-stalling responses) discard the peer.
async fn request_messages(
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	source: ParaId,
	request: &MessagesRequest,
	own: Option<&MmrFrontier>,
) -> Result<(VerifiedMessages, MessagesResponse), ExchangeError> {
	let candidates = peers.peers(source);
	let mut last = ExchangeError::NoPeers;
	for peer in candidates {
		let wire = ExchangeRequest::Messages(request.clone());
		match exchange_once(network, peer, &wire).await {
			Ok(ExchangeResponse::Messages(response)) => {
				match verify_messages_response(request, &response, own) {
					// An empty chunk that claims a non-empty backlog is
					// valid but stalls the loop — an honest server always
					// serves at least one payload. Misbehavior.
					Ok(verified)
						if response.payloads.is_empty() && verified.head > request.start.0 =>
					{
						peers.report_bad(source, peer);
						last = ExchangeError::MalformedResponse;
					},
					Ok(verified) => return Ok((verified, response)),
					Err(error) => {
						peers.report_bad(source, peer);
						last = error.into();
					},
				}
			},
			Ok(ExchangeResponse::Event(_)) => {
				peers.report_bad(source, peer);
				last = ExchangeError::MalformedResponse;
			},
			Err(error @ ExchangeError::MalformedResponse) => {
				peers.report_bad(source, peer);
				last = error;
			},
			Err(error) => last = error,
		}
	}
	Err(last)
}

/// Fetches and pools one ack register head read under `root`.
async fn fetch_register(
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	pool: &SpecMsgPool,
	source: ParaId,
	root: StreamsRoot,
	stream: StreamId,
) -> Result<(), ExchangeError> {
	let request = EventRequest { stream, under: root, at: None };
	let (verified, response) = request_event(network, peers, source, &request).await?;
	let frontier = verified.frontier.expect("head reads (`at: None`) yield a frontier; qed");
	pool.note_register(
		source,
		stream,
		RegisterRead {
			root,
			payload: response.payload,
			inclusion: response.inclusion,
			frontier,
			tree_proof: response.tree_proof,
		},
	);
	Ok(())
}

/// One verified [`EventRequest`] round trip; peer policy as for messages.
async fn request_event(
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	source: ParaId,
	request: &EventRequest,
) -> Result<(VerifiedEvent, cumulus_primitives_spec_messaging::EventResponse), ExchangeError> {
	let candidates = peers.peers(source);
	let mut last = ExchangeError::NoPeers;
	for peer in candidates {
		let wire = ExchangeRequest::Event(request.clone());
		match exchange_once(network, peer, &wire).await {
			Ok(ExchangeResponse::Event(response)) => {
				match verify_event_response(request, &response) {
					Ok(verified) => return Ok((verified, response)),
					Err(error) => {
						peers.report_bad(source, peer);
						last = error.into();
					},
				}
			},
			Ok(ExchangeResponse::Messages(_)) => {
				peers.report_bad(source, peer);
				last = ExchangeError::MalformedResponse;
			},
			Err(error @ ExchangeError::MalformedResponse) => {
				peers.report_bad(source, peer);
				last = error;
			},
			Err(error) => last = error,
		}
	}
	Err(last)
}

/// Runs the receiver's fetch pipeline until the monitor's event channel
/// closes. Spawn next to [`crate::run_relay_provides_monitor`], consuming
/// its paired receiver; the pool is shared with the inherent provider and
/// the lift assembler ([`crate::authoring`]).
pub async fn run_spec_msg_fetcher<Block, Client, Network>(
	para_id: ParaId,
	parachain: Arc<Client>,
	network: Network,
	peers: Arc<dyn SourcePeers>,
	pool: Arc<SpecMsgPool>,
	events: async_channel::Receiver<RelayProvidesEvent>,
) where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
	Network: ExchangeNetwork,
{
	run_fetch_rounds(
		events,
		|provides| fetch_included(para_id, &*parachain, &network, &*peers, &pool, provides),
		|source| pool.begin_round(source),
		futures_timer::Delay::new,
	)
	.await
}

/// A failed round's scheduled in-root retry (see [`run_fetch_rounds`]).
struct PendingRetry {
	/// The trigger being retried — same source, same root.
	provides: SourceProvides,
	/// The 1-based retry attempt this entry will run.
	attempt: u32,
	/// Tags the backoff timer armed for this entry: a firing timer whose
	/// tag no longer matches was superseded (newer root or reschedule) and
	/// is ignored.
	generation: u64,
}

/// Drives fetch rounds from monitor events with bounded in-root retries —
/// the generic core of [`run_spec_msg_fetcher`]. `round` runs one round for
/// a trigger; `mark` marks the trigger's source in flight in the pool
/// ([`SpecMsgPool::begin_round`]) the moment a trigger is acquired — at
/// event receipt and at retry-timer fire alike, before the round future
/// even exists — returning the guard held across the round; `sleep`
/// provides the backoff timer (`mark` and `sleep` injected for tests).
///
/// Marking at trigger receipt instead of round entry is deliberate: the
/// proposer's inherent snapshot can fire within ~1–5 ms of the monitor's
/// offer, ahead of the offer→`fetch_source` hop (task wake plus runtime-api
/// resolution), and an unmarked round cannot be awaited by the grace window
/// ([`SpecMsgPool::wait_for_in_flight_rounds`]) — soak run 4's two residual
/// first-attempt losses. The guard drops when the round returns — failures
/// and cancellation included; a scheduled retry's backoff wait is
/// deliberately not covered (authoring must never wait out a sleeping
/// retry).
///
/// Rounds and retries run sequentially on this one task. Due backoff timers
/// are polled before the event channel so retry scheduling is deterministic;
/// the ordering is immaterial for correctness — rounds are root-keyed and
/// idempotent, and an event superseding a scheduled retry does so whenever
/// it is received before the timer fires.
async fn run_fetch_rounds<Round, RoundFut, Mark, Guard, Sleep, SleepFut>(
	events: async_channel::Receiver<RelayProvidesEvent>,
	mut round: Round,
	mark: Mark,
	sleep: Sleep,
) where
	Round: FnMut(SourceProvides) -> RoundFut,
	RoundFut: Future<Output = Result<(), RoundError>>,
	Mark: Fn(ParaId) -> Guard,
	Sleep: Fn(Duration) -> SleepFut,
	SleepFut: Future<Output = ()> + Send + 'static,
{
	let mut events = events.fuse();
	let mut pending: HashMap<ParaId, PendingRetry> = HashMap::new();
	let mut timers: FuturesUnordered<Pin<Box<dyn Future<Output = (ParaId, u64)> + Send>>> =
		FuturesUnordered::new();
	let mut generation: u64 = 0;

	loop {
		let (provides, attempt) = futures::select_biased! {
			due = timers.select_next_some() => {
				let (source, timer_generation) = due;
				match pending.get(&source) {
					Some(retry) if retry.generation == timer_generation => {
						let retry = pending.remove(&source).expect("entry just matched; qed");
						(retry.provides, retry.attempt)
					},
					// The timer was superseded by a newer root or a
					// rescheduled retry.
					_ => continue,
				}
			},
			event = events.next() => match event {
				Some(RelayProvidesEvent::Included(provides)) => {
					// A fresh included root supersedes the source's
					// scheduled retry and resets its budget.
					pending.remove(&provides.source);
					(provides, 0)
				},
				// Prefetch hints are a post-MVP latency win; see the module
				// docs for why the MVP pool holds included-bound material
				// only.
				Some(RelayProvidesEvent::Pending(_)) => continue,
				None => break,
			},
		};

		// In flight from this very moment: the guard exists before the round
		// future is constructed — no await, no task hop sits between the
		// trigger and the marker — and drops when the round returns, however
		// it returns.
		let result = {
			let _round = mark(provides.source);
			round(provides.clone()).await
		};
		let error = match result {
			Ok(()) => continue,
			Err(error) => error,
		};
		if !error.retryable() {
			tracing::warn!(
				target: LOG_TARGET,
				?error,
				source = %u32::from(provides.source),
				root = ?provides.root,
				"Fetch round failed; waiting for the next trigger",
			);
			continue;
		}
		if attempt >= MAX_ROUND_RETRIES {
			tracing::warn!(
				target: LOG_TARGET,
				?error,
				source = %u32::from(provides.source),
				root = ?provides.root,
				retries = MAX_ROUND_RETRIES,
				"Fetch round failed; in-root retries exhausted, waiting for the next included root",
			);
			continue;
		}
		let attempt = attempt + 1;
		let delay = retry_delay(attempt);
		tracing::warn!(
			target: LOG_TARGET,
			?error,
			source = %u32::from(provides.source),
			root = ?provides.root,
			attempt,
			max = MAX_ROUND_RETRIES,
			?delay,
			"Fetch round failed; retrying under the same root",
		);
		generation += 1;
		let source = provides.source;
		pending.insert(source, PendingRetry { provides, attempt, generation });
		let timer = sleep(delay);
		let tag = generation;
		timers.push(Box::pin(async move {
			timer.await;
			(source, tag)
		}));
	}
}

/// Errors preparing one fetch round (the round's own errors are
/// [`FetchError`]).
#[derive(Debug, thiserror::Error)]
enum RoundError {
	#[error("runtime api error: {0}")]
	Api(#[from] sp_api::ApiError),
	#[error(transparent)]
	Fetch(#[from] FetchError),
}

impl RoundError {
	/// Whether an in-root retry can plausibly succeed: transient transport
	/// and peer failures can, and so can a chunk-bound cutoff (the retry
	/// resumes where the round stopped). Local errors (runtime api, pool
	/// contract) cannot, nor can an empty peer set — backoff conjures no
	/// peers, so the source gives up until a trigger arrives.
	fn retryable(&self) -> bool {
		match self {
			Self::Api(_) => false,
			Self::Fetch(FetchError::Pool(_)) => false,
			Self::Fetch(FetchError::Exchange(ExchangeError::NoPeers)) => false,
			Self::Fetch(FetchError::Exchange(_)) => true,
			Self::Fetch(FetchError::ChunkBound) => true,
		}
	}
}

/// Resolves what to fetch from the own runtime and runs the round.
async fn fetch_included<Block, Client>(
	para_id: ParaId,
	parachain: &Client,
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	pool: &SpecMsgPool,
	provides: SourceProvides,
) -> Result<(), RoundError>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + HeaderBackend<Block>,
	Client::Api: SpecMsgApi<Block>,
{
	// The version gate, exactly like the monitor: no `SpecMsgApi`, nothing
	// is consumed.
	let best = parachain.info().best_hash;
	if !parachain.runtime_api().has_api::<dyn SpecMsgApi<Block>>(best)? {
		return Ok(());
	}

	let consumed = parachain.runtime_api().consumed_streams(best)?;
	let out_channels = parachain.runtime_api().out_channels(best)?;
	// Register-only sources are kept and fetched from too: an outbound
	// channel's handshake completes by reading the peer's ack register,
	// whether or not any inbound channel consumes that peer's data streams.
	pool.retain_sources(|source| {
		consumed.contains_key(source) || out_channels.keys().any(|channel| channel.peer == *source)
	});

	let channels: Vec<(StreamId, u64)> = consumed
		.get(&provides.source)
		.map(|streams| {
			streams
				.iter()
				.filter_map(|stream| match stream {
					consumed @ ConsumedStream::Channel { from, .. } => {
						Some((consumed.stream_id(para_id), from.0))
					},
					// Broadcast event streams are post-MVP; their fetch
					// discipline (`EventRequest`) ships via the register
					// reads below.
					ConsumedStream::Broadcast { .. } => None,
				})
				.collect()
		})
		.unwrap_or_default();

	// Which ack registers to read follows from the outbound channel views:
	// the peer publishes its register on ITS ack stream addressed to us.
	let registers: Vec<StreamId> = out_channels
		.keys()
		.filter(|channel| channel.peer == provides.source)
		.map(|channel| StreamId::Ack {
			recipient: para_id,
			domain: channel.domain,
			num: channel.num,
		})
		.collect();

	if channels.is_empty() && registers.is_empty() {
		return Ok(());
	}

	fetch_source(
		network,
		peers,
		pool,
		provides.source,
		provides.root,
		&channels,
		&registers,
		FETCH_CHUNK_BYTES,
	)
	.await?;
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		exchange::{PeerRegistry, SourcePeers},
		pool::InherentBudget,
		test_support::*,
	};
	use cumulus_primitives_spec_messaging::test_utils::StreamFixture;
	use futures::executor::block_on;
	use parking_lot::{Mutex, RwLock};
	use sc_network::PeerId;
	use std::collections::HashMap;

	const SOURCE: u32 = 2000;
	const RECEIVER: u32 = 2001;

	/// Payload byte budget forcing 2 payloads (16 B each) per chunk.
	const SMALL_CHUNK: u32 = 40;

	fn source() -> ParaId {
		ParaId::from(SOURCE)
	}

	/// All payloads of `stream` up to `count`, as the import fixtures
	/// produce them.
	fn all_payloads(stream: &StreamId, count: u64) -> Vec<Vec<u8>> {
		(0..count).map(|position| payload(stream, position)).collect()
	}

	fn fixture(stream: StreamId, count: u64) -> StreamFixture {
		StreamFixture::from_payloads(stream, &all_payloads(&stream, count))
	}

	/// A served archive over `blocks` × `per_block` messages on a channel
	/// and an ack stream, plus the head root.
	fn serving_archive(blocks: u32, per_block: u64) -> (MockExchange, StreamsRoot) {
		let (_aux, mut archive) = new_archive();
		let roots = import_blocks(
			&mut archive,
			&[channel(RECEIVER), ack(RECEIVER)],
			blocks,
			per_block,
			1_000,
		);
		let root = *roots.last().expect("blocks were imported");
		(
			MockExchange {
				archive: RwLock::new(archive),
				behavior: HashMap::new(),
				hits: Default::default(),
			},
			root,
		)
	}

	fn registry_with(peers: &[PeerId]) -> PeerRegistry {
		let registry = PeerRegistry::default();
		registry.set_peers(source(), peers.to_vec());
		registry
	}

	#[test]
	fn fetch_round_resumes_across_chunks_and_pools_the_backlog() {
		let (mut network, root) = serving_archive(6, 3);
		let honest = PeerId::random();
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(stream, 0)],
			&[ack(RECEIVER)],
			SMALL_CHUNK,
		))
		.expect("round succeeds");

		// The whole backlog was fetched, chunked, and resumed from
		// `base + received`: 18 payloads at 2 per chunk = 9 chunks.
		assert_eq!(pool.pooled_payloads(source(), &stream), 18);
		assert_eq!(pool.resume(source(), &stream), Some((18, fixture(stream, 18).frontier_at(18))));
		assert_eq!(pool.target(source()), Some(root));
		assert!(network.hits.lock().len() >= 9);

		// The register head read was fetched and verified alongside; the
		// pooled proof pins the head's placement.
		let data =
			pool.build_inherent(&[], &[(source(), ack(RECEIVER), None)], InherentBudget::default());
		let (_, _, payload, proof) = &data.register_reads[0];
		assert_eq!(*payload, crate::test_support::payload(&ack(RECEIVER), 17));
		let leaf = cumulus_primitives_spec_messaging::hash_leaf::<
			cumulus_primitives_spec_messaging::SpecHasher,
		>(cumulus_primitives_spec_messaging::LEAF_VERSION, payload);
		let (position, frontier) = proof.verify_head(leaf).expect("pooled proof verifies");
		assert_eq!(position, MessagePosition(17));
		assert_eq!(frontier, fixture(ack(RECEIVER), 18).frontier_at(18));

		// A second round under the same root is a cheap no-op refresh: the
		// caught-up stream re-binds with an empty extension.
		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(stream, 0)],
			&[],
			SMALL_CHUNK,
		))
		.expect("caught-up round succeeds");
		assert_eq!(pool.pooled_payloads(source(), &stream), 18);
	}

	#[test]
	fn poisoned_responses_drop_the_peer_and_are_refetched() {
		let (mut network, root) = serving_archive(4, 2);
		let poison = PeerId::random();
		let honest = PeerId::random();
		network.behavior.insert(poison, PeerBehavior::Poison);
		network.behavior.insert(honest, PeerBehavior::Honest);
		// The poisoning peer is preferred — every chunk hits it first until
		// it is dropped.
		let registry = registry_with(&[poison, honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(stream, 0)],
			&[],
			SMALL_CHUNK,
		))
		.expect("round succeeds via the honest peer");

		// The poisoned response never reached the pool: the pooled run is
		// byte-identical to the archive's payloads, and the resume frontier
		// is the honest one.
		assert_eq!(pool.pooled_payloads(source(), &stream), 8);
		assert_eq!(pool.resume(source(), &stream), Some((8, fixture(stream, 8).frontier_at(8))));

		// The poisoning peer was dropped on first contact and never asked
		// again; the honest peer served everything.
		assert_eq!(registry.peers(source()), vec![honest]);
		let hits = network.hits.lock();
		assert_eq!(hits.iter().filter(|peer| **peer == poison).count(), 1);
	}

	#[test]
	fn transport_failures_rotate_without_dropping_the_peer() {
		let (mut network, root) = serving_archive(2, 2);
		let refusing = PeerId::random();
		let honest = PeerId::random();
		network.behavior.insert(refusing, PeerBehavior::Refuse);
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[refusing, honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(stream, 0)],
			&[],
			SMALL_CHUNK,
		))
		.expect("round succeeds via the honest peer");

		assert_eq!(pool.pooled_payloads(source(), &stream), 4);
		// Refusals are protocol behavior (unservable requests), not
		// misbehavior: the peer stays registered.
		assert_eq!(registry.peers(source()), vec![refusing, honest]);
	}

	#[test]
	fn never_written_streams_do_not_poison_the_round() {
		// The peer's ack stream is published, but its channel stream to us
		// was never written: the archive cannot serve it under any root
		// (both failed runs, e.g. 13:57:54: "stream has no entry under the
		// named root"). The round must still pool the register read — it is
		// what completes the handshake — and mark the lift target; the
		// unserved stream is tolerated outright and the round succeeds (an
		// in-root retry could never serve it).
		let (_aux, mut archive) = new_archive();
		let roots = import_blocks(&mut archive, &[ack(RECEIVER)], 1, 1, 1_000);
		let root = *roots.last().expect("a block was imported");
		let honest = PeerId::random();
		let network = MockExchange {
			archive: RwLock::new(archive),
			behavior: HashMap::from([(honest, PeerBehavior::Honest)]),
			hits: Default::default(),
		};
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();

		let result = block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(channel(RECEIVER), 0)],
			&[ack(RECEIVER)],
			SMALL_CHUNK,
		));

		// The refusal-only round is a success — nothing is reported for the
		// in-root retry — and the round completed:
		assert!(result.is_ok());
		assert_eq!(pool.target(source()), Some(root));
		// ...and the register read is pooled and handable.
		let data =
			pool.build_inherent(&[], &[(source(), ack(RECEIVER), None)], InherentBudget::default());
		assert_eq!(data.register_reads.len(), 1);
		// The refusal is protocol behavior, not misbehavior.
		assert_eq!(registry.peers(source()), vec![honest]);
	}

	#[test]
	fn failures_on_streams_with_pooled_history_stay_round_fatal() {
		let (mut network, root) = serving_archive(2, 2);
		let honest = PeerId::random();
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(stream, 0)],
			&[],
			SMALL_CHUNK,
		))
		.expect("first round succeeds");
		assert_eq!(pool.target(source()), Some(root));

		// The sender advances (a new root) but the peer now refuses: a
		// stream WITH pooled history is a hard failure — the round must not
		// complete and the target must not move to a root nothing was
		// re-bound under.
		let new_root = {
			let mut archive = network.archive.write();
			archive
				.import_block_at(
					block_hash(3),
					block_hash(2),
					3,
					vec![(stream, vec![payload(&stream, 4)])],
					1_003,
				)
				.expect("extends the tip")
				.expect("the block carries sends")
		};
		network.behavior.insert(honest, PeerBehavior::Refuse);

		let result = block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			new_root,
			&[(stream, 0)],
			&[],
			SMALL_CHUNK,
		));
		assert!(matches!(result, Err(FetchError::Exchange(ExchangeError::Network(_)))));
		assert_eq!(pool.target(source()), Some(root));
	}

	#[test]
	fn no_peers_fails_the_round_without_marking_a_target() {
		let (network, root) = serving_archive(2, 2);
		let registry = PeerRegistry::default();
		let pool = SpecMsgPool::default();

		let result = block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(channel(RECEIVER), 0)],
			&[],
			SMALL_CHUNK,
		));
		assert!(matches!(result, Err(FetchError::Exchange(ExchangeError::NoPeers))));
		assert_eq!(pool.target(source()), None);
	}

	fn included(root: StreamsRoot) -> RelayProvidesEvent {
		RelayProvidesEvent::Included(SourceProvides {
			source: source(),
			root,
			relay_block: Default::default(),
		})
	}

	/// A transient round failure, as the retry loop classifies errors.
	fn transient() -> RoundError {
		RoundError::Fetch(FetchError::Exchange(ExchangeError::Network("transient".into())))
	}

	#[test]
	fn transient_round_failures_retry_within_the_current_root() {
		let (mut network, root) = serving_archive(2, 2);
		let honest = PeerId::random();
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		// Seed pooled history under the first root: the in-root retry is for
		// transient failures on history-bearing streams (a never-written
		// stream's refusal completes the round without one instead).
		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(stream, 0)],
			&[],
			SMALL_CHUNK,
		))
		.expect("seed round succeeds");
		assert_eq!(pool.pooled_payloads(source(), &stream), 4);

		// The sender advances, and the (fresh) peer refuses the new root's
		// round once — a genuinely transient failure — before serving.
		let new_root = {
			let mut archive = network.archive.write();
			archive
				.import_block_at(
					block_hash(3),
					block_hash(2),
					3,
					vec![(stream, vec![payload(&stream, 4)])],
					1_003,
				)
				.expect("extends the tip")
				.expect("the block carries sends")
		};
		let flaky = PeerId::random();
		network.behavior.insert(flaky, PeerBehavior::RefuseFirst(1));
		registry.set_peers(source(), vec![flaky]);

		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(new_root)).expect("channel is open");
		sender.close();

		let (network, registry, pool) = (&network, &registry, &pool);
		let delays = Mutex::new(Vec::new());
		block_on(run_fetch_rounds(
			events,
			|provides| async move {
				fetch_source(
					network,
					registry,
					pool,
					provides.source,
					provides.root,
					&[(stream, 0)],
					&[],
					SMALL_CHUNK,
				)
				.await
				.map_err(RoundError::from)
			},
			|source| pool.begin_round(source),
			|delay| {
				delays.lock().push(delay);
				futures::future::ready(())
			},
		));

		// The transient refusal cost exactly one in-root retry — the new
		// leaf landed and the target moved WITHOUT a second included root.
		assert_eq!(pool.pooled_payloads(source(), &stream), 5);
		assert_eq!(pool.target(source()), Some(new_root));
		assert_eq!(*delays.lock(), vec![ROUND_RETRY_DELAY]);
		// A transport refusal is not misbehavior: the peer stays registered.
		assert_eq!(registry.peers(source()), vec![flaky]);
	}

	#[test]
	fn never_written_refusals_do_not_schedule_in_root_retries() {
		// The handshake window (soak run 1, 13:55:18/13:55:20): the source
		// published its ack register one block before its first channel send,
		// so the round under the ack root finds a never-written channel
		// stream. Such a round completes without scheduling an in-root retry
		// or consuming its budget — every retry under that root is futile by
		// design — and recovery arrives with the next included root, the one
		// carrying the first send.
		let (_aux, mut archive) = new_archive();
		let roots = import_blocks(&mut archive, &[ack(RECEIVER)], 1, 1, 1_000);
		let handshake_root = *roots.last().expect("a block was imported");
		let stream = channel(RECEIVER);
		let send_root = archive
			.import_block_at(
				block_hash(2),
				block_hash(1),
				2,
				vec![(stream, vec![payload(&stream, 0)])],
				1_002,
			)
			.expect("extends the tip")
			.expect("the block carries sends");
		let honest = PeerId::random();
		let network = MockExchange {
			archive: RwLock::new(archive),
			behavior: HashMap::from([(honest, PeerBehavior::Honest)]),
			hits: Default::default(),
		};
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();

		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(handshake_root)).expect("channel is open");
		sender.try_send(included(send_root)).expect("channel is open");
		sender.close();

		let (network, registry, pool) = (&network, &registry, &pool);
		let calls = Mutex::new(Vec::new());
		let delays = Mutex::new(Vec::<Duration>::new());
		block_on(run_fetch_rounds(
			events,
			|provides| {
				calls.lock().push(provides.root);
				async move {
					fetch_source(
						network,
						registry,
						pool,
						provides.source,
						provides.root,
						&[(stream, 0)],
						&[ack(RECEIVER)],
						SMALL_CHUNK,
					)
					.await
					.map_err(RoundError::from)
				}
			},
			|source| pool.begin_round(source),
			|delay| {
				delays.lock().push(delay);
				futures::future::ready(())
			},
		));

		// Each root ran exactly once: no retry timer, no budget consumed.
		assert_eq!(*calls.lock(), vec![handshake_root, send_root]);
		assert!(delays.lock().is_empty());
		// The handshake round still pooled the register read (and marked its
		// target), and the next included root recovered the first send.
		assert_eq!(pool.pooled_payloads(source(), &stream), 1);
		assert_eq!(pool.target(source()), Some(send_root));
		let data =
			pool.build_inherent(&[], &[(source(), ack(RECEIVER), None)], InherentBudget::default());
		assert_eq!(data.register_reads.len(), 1);
	}

	#[test]
	fn retry_budget_exhausts_and_a_new_root_resets_it() {
		let root1 = StreamsRoot(polkadot_core_primitives::Hash::repeat_byte(1));
		let root2 = StreamsRoot(polkadot_core_primitives::Hash::repeat_byte(2));
		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(root1)).expect("channel is open");
		sender.try_send(included(root2)).expect("channel is open");
		sender.close();

		let calls = Mutex::new(Vec::new());
		let delays = Mutex::new(Vec::new());
		block_on(run_fetch_rounds(
			events,
			|provides| {
				calls.lock().push(provides.root);
				futures::future::ready(Err(transient()))
			},
			|_| (),
			|delay| {
				delays.lock().push(delay);
				futures::future::ready(())
			},
		));

		// Each root ran once plus MAX_ROUND_RETRIES in-root retries: the
		// exhausted budget stopped the retrying (no busy loop — nothing ran
		// again until the next event), and the next root reset it.
		let runs = 1 + MAX_ROUND_RETRIES as usize;
		let expected: Vec<StreamsRoot> = std::iter::repeat(root1)
			.take(runs)
			.chain(std::iter::repeat(root2).take(runs))
			.collect();
		assert_eq!(*calls.lock(), expected);
		// The backoff is exponential from ROUND_RETRY_DELAY and capped.
		let backoff: Vec<Duration> = [2, 4, 8, 16].map(Duration::from_secs).to_vec();
		assert_eq!(*backoff.last().expect("non-empty; qed"), ROUND_RETRY_DELAY_CAP);
		assert_eq!(*delays.lock(), [backoff.clone(), backoff].concat());
	}

	#[test]
	fn a_new_root_supersedes_a_scheduled_retry() {
		let root1 = StreamsRoot(polkadot_core_primitives::Hash::repeat_byte(1));
		let root2 = StreamsRoot(polkadot_core_primitives::Hash::repeat_byte(2));
		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(root1)).expect("channel is open");
		sender.try_send(included(root2)).expect("channel is open");
		sender.close();

		let calls = Mutex::new(Vec::new());
		block_on(run_fetch_rounds(
			events,
			|provides| {
				calls.lock().push(provides.root);
				let fail = provides.root == root1;
				async move {
					if fail {
						Err(transient())
					} else {
						Ok(())
					}
				}
			},
			|_| (),
			// Backoff timers that never fire: the fresh included root must
			// supersede the scheduled retry, not wait behind it — and the
			// loop must still terminate on channel close.
			|_| futures::future::pending(),
		));

		assert_eq!(*calls.lock(), vec![root1, root2]);
	}

	#[test]
	fn no_peers_gives_up_until_the_next_trigger() {
		let (network, root) = serving_archive(2, 2);
		let registry = PeerRegistry::default();
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(root)).expect("channel is open");
		sender.close();

		let (network, registry, pool) = (&network, &registry, &pool);
		let calls = Mutex::new(0u32);
		let delays = Mutex::new(Vec::<Duration>::new());
		block_on(run_fetch_rounds(
			events,
			|provides| {
				*calls.lock() += 1;
				async move {
					fetch_source(
						network,
						registry,
						pool,
						provides.source,
						provides.root,
						&[(stream, 0)],
						&[],
						SMALL_CHUNK,
					)
					.await
					.map_err(RoundError::from)
				}
			},
			|source| pool.begin_round(source),
			|delay| {
				delays.lock().push(delay);
				futures::future::ready(())
			},
		));

		// An empty peer set is never retried — backoff conjures no peers;
		// the source waits for its next trigger.
		assert_eq!(*calls.lock(), 1);
		assert!(delays.lock().is_empty());
		assert_eq!(pool.target(source()), None);
		// The failed round's in-flight marker was cleared: authoring's grace
		// window has nothing to wait on during a retry backoff.
		assert_eq!(pool.rounds_in_flight(), 0);
	}

	#[test]
	fn poisoned_rounds_discard_the_peer_and_retries_stop_at_no_peers() {
		let (mut network, root) = serving_archive(2, 2);
		let poison = PeerId::random();
		network.behavior.insert(poison, PeerBehavior::Poison);
		let registry = registry_with(&[poison]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(root)).expect("channel is open");
		sender.close();

		let (network, registry, pool) = (&network, &registry, &pool);
		let calls = Mutex::new(0u32);
		let delays = Mutex::new(Vec::new());
		block_on(run_fetch_rounds(
			events,
			|provides| {
				*calls.lock() += 1;
				async move {
					fetch_source(
						network,
						registry,
						pool,
						provides.source,
						provides.root,
						&[(stream, 0)],
						&[],
						SMALL_CHUNK,
					)
					.await
					.map_err(RoundError::from)
				}
			},
			|source| pool.begin_round(source),
			|delay| {
				// The failed round's guard dropped before its backoff was
				// armed: the wait is never marked in flight.
				assert_eq!(pool.rounds_in_flight(), 0);
				delays.lock().push(delay);
				futures::future::ready(())
			},
		));

		// Verification failure discarded the peer within the first run (a
		// retry never re-contacts it)...
		assert!(registry.peers(source()).is_empty());
		assert_eq!(network.hits.lock().iter().filter(|hit| **hit == poison).count(), 1);
		// ...and the first retry found no peers left and gave up: exactly
		// one backoff, two runs, no busy loop.
		assert_eq!(*delays.lock(), vec![ROUND_RETRY_DELAY]);
		assert_eq!(*calls.lock(), 2);
	}

	#[test]
	fn rounds_are_marked_in_flight_at_offer_receipt_not_round_entry() {
		// The run-4 residual (2/11 first-attempt losses): the proposer's
		// inherent snapshot can fire within ~1–5 ms of the monitor's offer —
		// ahead of the offer→`fetch_source` hop (task wake plus runtime-api
		// resolution) that used to create the in-flight marker, so the grace
		// window had nothing to await. Marking now happens the moment the
		// trigger is received: a snapshot racing ahead of the round's first
		// real work (stretched here to an unmissable 20 ms) finds the marker,
		// waits the round out and hands its material to the first block.
		let (mut network, root) = serving_archive(2, 2);
		let honest = PeerId::random();
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(root)).expect("channel is open");
		sender.close();

		let (network, registry, pool) = (&network, &registry, &pool);
		let fetcher = run_fetch_rounds(
			events,
			|provides| async move {
				// The round is in flight before any of its work has run...
				assert_eq!(pool.rounds_in_flight(), 1);
				futures_timer::Delay::new(Duration::from_millis(20)).await;
				fetch_source(
					network,
					registry,
					pool,
					provides.source,
					provides.root,
					&[(stream, 0)],
					&[ack(RECEIVER)],
					SMALL_CHUNK,
				)
				.await
				.map_err(RoundError::from)
			},
			|source| pool.begin_round(source),
			|_| futures::future::ready(()),
		);
		// ...and the proposer, snapshotting right after the offer, wins.
		let proposer = async {
			pool.wait_for_in_flight_rounds(Duration::from_secs(30)).await;
			assert_eq!(pool.rounds_in_flight(), 0);
			assert_eq!(pool.target(source()), Some(root));
			let data = pool.build_inherent(
				&[(source(), stream, 0)],
				&[(source(), ack(RECEIVER), None)],
				InherentBudget::default(),
			);
			assert_eq!(data.messages.len(), 1);
			assert_eq!(data.register_reads.len(), 1);
		};
		// `join` polls the fetcher first: the event is dequeued and marked in
		// that same synchronous poll, before the proposer's wait starts.
		block_on(futures::future::join(fetcher, proposer));
	}

	#[test]
	fn retries_mark_the_round_in_flight_at_fire_time() {
		// The retry path shares the receipt-time marking: a firing backoff
		// timer re-marks the source before its round runs, while the backoff
		// wait itself stays unmarked — the guard clears on round failure.
		let pool = SpecMsgPool::default();
		let root = StreamsRoot(polkadot_core_primitives::Hash::repeat_byte(1));
		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(root)).expect("channel is open");
		sender.close();

		let pool = &pool;
		let marks = Mutex::new(0usize);
		block_on(run_fetch_rounds(
			events,
			|_provides| {
				// Every run — the first and each in-root retry — is already
				// marked when its round future is created.
				assert_eq!(pool.rounds_in_flight(), 1);
				futures::future::ready(Err(transient()))
			},
			|source| {
				*marks.lock() += 1;
				pool.begin_round(source)
			},
			|_delay| {
				// The failed round's guard dropped before its backoff was
				// armed: authoring never waits out a sleeping retry.
				assert_eq!(pool.rounds_in_flight(), 0);
				futures::future::ready(())
			},
		));

		// The first run plus every in-root retry was marked at fire time...
		assert_eq!(*marks.lock(), 1 + MAX_ROUND_RETRIES as usize);
		// ...and nothing stays marked once the budget is exhausted.
		assert_eq!(pool.rounds_in_flight(), 0);
	}

	#[test]
	fn dropping_the_fetcher_mid_round_clears_the_in_flight_marker() {
		// A fetcher torn down mid-round (collator shutdown): the guard is
		// held across the round await inside the fetch loop, so cancelling
		// the task drops it — no authoring task keeps waiting on a round
		// that can never end.
		let pool = SpecMsgPool::default();
		let root = StreamsRoot(polkadot_core_primitives::Hash::repeat_byte(1));
		let (sender, events) = async_channel::unbounded();
		sender.try_send(included(root)).expect("channel is open");

		let mut fetcher = Box::pin(run_fetch_rounds(
			events,
			|_provides| futures::future::pending::<Result<(), RoundError>>(),
			|source| pool.begin_round(source),
			|_| futures::future::ready(()),
		));
		// One poll: the trigger is received and marked, and the round hangs.
		block_on(async {
			assert!(futures::poll!(fetcher.as_mut()).is_pending());
		});
		assert_eq!(pool.rounds_in_flight(), 1);

		drop(fetcher);
		assert_eq!(pool.rounds_in_flight(), 0);
		// The drop woke the grace window: a subsequent wait is free again.
		let start = std::time::Instant::now();
		block_on(pool.wait_for_in_flight_rounds(Duration::from_secs(30)));
		assert!(start.elapsed() < Duration::from_secs(1));
	}

	#[test]
	fn authoring_grace_window_is_free_when_no_round_is_in_flight() {
		// The common idle case: nothing in flight, the wait arms no timer
		// and registers no waiter — a 30 s bound returning instantly is the
		// proof the wait path was not taken.
		let pool = SpecMsgPool::default();
		let start = std::time::Instant::now();
		block_on(pool.wait_for_in_flight_rounds(Duration::from_secs(30)));
		assert!(start.elapsed() < Duration::from_secs(1));
		assert_eq!(pool.rounds_in_flight(), 0);
	}

	#[test]
	fn authoring_grace_window_picks_up_a_round_completing_within_the_bound() {
		// The soak-run race (runs 1–2: 1 win / 8 losses, 2–5 ms margins):
		// the proposer's pool snapshot lands while the round triggered by
		// the same relay import is still in flight. With the round marked in
		// flight the snapshot waits it out — well under the bound — and
		// hands the material to the first eligible block.
		let (mut network, root) = serving_archive(2, 2);
		let honest = PeerId::random();
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		// The trigger was received — the fetch loop marks at receipt...
		let guard = pool.begin_round(source());
		std::thread::scope(|scope| {
			let (network, registry, pool) = (&network, &registry, &pool);
			scope.spawn(move || {
				// ...its material lands 20 ms after the proposer started
				// waiting.
				std::thread::sleep(Duration::from_millis(20));
				block_on(fetch_source(
					network,
					registry,
					pool,
					source(),
					root,
					&[(stream, 0)],
					&[ack(RECEIVER)],
					SMALL_CHUNK,
				))
				.expect("round succeeds");
				drop(guard);
			});

			let start = std::time::Instant::now();
			block_on(pool.wait_for_in_flight_rounds(Duration::from_secs(30)));
			// Woken by the round ending, not by the bound.
			assert!(start.elapsed() < Duration::from_secs(10));
			assert_eq!(pool.rounds_in_flight(), 0);
			// The post-wait snapshot sees the completed round's material:
			// target marked, backlog and register read handable.
			assert_eq!(pool.target(source()), Some(root));
			let data = pool.build_inherent(
				&[(source(), stream, 0)],
				&[(source(), ack(RECEIVER), None)],
				InherentBudget::default(),
			);
			assert_eq!(data.messages.len(), 1);
			assert_eq!(data.register_reads.len(), 1);
		});
	}

	#[test]
	fn authoring_grace_window_is_a_hard_bound_and_expires_stale_rounds() {
		// A round that never completes: the wait returns at the bound with
		// the pre-round pool view — authoring is never held hostage.
		let pool = SpecMsgPool::default();
		let bound = Duration::from_millis(100);
		let start = std::time::Instant::now();
		let _hung = pool.begin_round(source());
		block_on(pool.wait_for_in_flight_rounds(bound));
		let waited = start.elapsed();
		assert!(waited >= bound);
		assert!(waited < Duration::from_secs(5));
		assert!(pool
			.build_inherent(&[], &[(source(), ack(RECEIVER), None)], InherentBudget::default())
			.is_empty());

		// The hung round is now past its grant (the bound runs from the
		// round's OWN start): a later proposal — the next relay parent, or
		// another fork — does not wait on it at all.
		let start = std::time::Instant::now();
		block_on(pool.wait_for_in_flight_rounds(bound));
		assert!(start.elapsed() < bound);
	}

	#[test]
	fn authoring_grace_window_waits_out_a_pending_offer_before_the_round_is_marked() {
		// Issue 10, sub-mode (a) — late offer (soak runs 11 A6, 18 B6): the
		// monitor has PUSHED an included offer but the fetcher has not yet
		// dequeued it, so NO round is marked in flight at the snapshot
		// instant. Pre-fix the window found nothing and returned immediately,
		// and the material rode the next block (+6 s). With the monitor's
		// pending-offer hint recorded, the window waits on the imminent round
		// and picks up its material once it starts and completes in-bound.
		let (mut network, root) = serving_archive(2, 2);
		let honest = PeerId::random();
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		// The monitor pushed the offer: a round is imminent, but nothing is
		// marked in flight yet (the offer is still in transit to the fetcher).
		pool.note_pending_offer(source());
		// Unambiguously isolate the late-offer path — the sub-mode the live
		// soak never exercised (`pending_offers` stayed 0 across all 116
		// Campaign-3 grace engagements), so this test is its sole coverage.
		// At the wait entry the ONLY grace anchor is the pending offer: no
		// round is in flight and no completion is retained, so any wait the
		// window takes below is attributable solely to the pending offer, not
		// the in-flight round or the store-visibility retention path.
		assert_eq!(pool.rounds_in_flight(), 0);
		assert_eq!(pool.pending_offers_in_flight(), 1);
		assert_eq!(pool.retained_completions(), 0);

		std::thread::scope(|scope| {
			let (network, registry, pool) = (&network, &registry, &pool);
			scope.spawn(move || {
				// The fetcher dequeues 20 ms later: it marks the real round
				// (superseding the hint) and runs it to completion.
				std::thread::sleep(Duration::from_millis(20));
				let guard = pool.begin_round(source());
				block_on(fetch_source(
					network,
					registry,
					pool,
					source(),
					root,
					&[(stream, 0)],
					&[ack(RECEIVER)],
					SMALL_CHUNK,
				))
				.expect("round succeeds");
				drop(guard);
			});

			let start = std::time::Instant::now();
			block_on(pool.wait_for_in_flight_rounds(Duration::from_secs(30)));
			// The wait was driven solely by the pending offer (the only anchor
			// at entry): it blocked until begin_round superseded the offer with
			// the real round and that round completed — not the immediate
			// return the pre-fix window (which could only await an
			// already-marked round) would have taken.
			assert!(start.elapsed() >= Duration::from_millis(20));
			assert!(start.elapsed() < Duration::from_secs(10));
			assert_eq!(pool.rounds_in_flight(), 0);
			// begin_round superseded the pending-offer hint — it is not left
			// dangling once the real round has run.
			assert_eq!(pool.pending_offers_in_flight(), 0);
			// The post-wait snapshot sees the round's material.
			assert_eq!(pool.target(source()), Some(root));
			let data = pool.build_inherent(
				&[(source(), stream, 0)],
				&[(source(), ack(RECEIVER), None)],
				InherentBudget::default(),
			);
			assert_eq!(data.messages.len(), 1);
			assert_eq!(data.register_reads.len(), 1);
		});
	}

	#[test]
	fn authoring_grace_window_re_reads_a_round_that_just_completed() {
		// Issue 10, sub-mode (b) — store visibility (soak runs 12 A2, 18 B4,
		// incl. run 18's KPI message): a round completed and its guard
		// already dropped a hair before the snapshot, so `rounds_in_flight()`
		// is 0 — yet the just-fetched material must still reach the first
		// block, not ride the next one (+6 s). The just-completed retention
		// makes the window wait a beat and re-read instead of sealing an
		// empty inherent. (In-process the write is already visible; the
		// retention closes the cross-thread publish→observe gap the soak saw.)
		let (mut network, root) = serving_archive(2, 2);
		let honest = PeerId::random();
		network.behavior.insert(honest, PeerBehavior::Honest);
		let registry = registry_with(&[honest]);
		let pool = SpecMsgPool::default();
		let stream = channel(RECEIVER);

		let guard = pool.begin_round(source());
		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(stream, 0)],
			&[ack(RECEIVER)],
			SMALL_CHUNK,
		))
		.expect("round succeeds");

		// The guard drops the instant before the snapshot — nothing is marked
		// in flight, exactly the state the pre-fix window returned empty on.
		let snapshot = std::time::Instant::now();
		drop(guard);
		assert_eq!(pool.rounds_in_flight(), 0);

		block_on(pool.wait_for_in_flight_rounds(crate::ROUND_GRACE_WINDOW));
		let waited = snapshot.elapsed();
		// The retention engaged (a brief re-read window), far under the bound.
		assert!(waited >= crate::pool::COMPLETION_RETENTION);
		assert!(waited < crate::ROUND_GRACE_WINDOW);
		// ...and the just-completed round's material is observed.
		assert_eq!(pool.target(source()), Some(root));
		let data = pool.build_inherent(
			&[(source(), stream, 0)],
			&[(source(), ack(RECEIVER), None)],
			InherentBudget::default(),
		);
		assert_eq!(data.messages.len(), 1);
		assert_eq!(data.register_reads.len(), 1);
	}

	#[test]
	fn authoring_grace_window_pending_offer_is_a_hard_bound() {
		// A pending offer whose round never materializes (fetcher wedged or
		// torn down after the monitor pushed): the window must not wait
		// forever — the offer is granted the same `bound` from its own push
		// instant as a round is from its start, then authoring proceeds.
		let pool = SpecMsgPool::default();
		let bound = Duration::from_millis(100);
		pool.note_pending_offer(source());
		// Only a pending offer anchors the window — no round in flight,
		// nothing retained — so the full-bound wait below is provably driven
		// by the offer, and the return at the bound is the offer's OWN hard
		// bound, not a round's.
		assert_eq!(pool.rounds_in_flight(), 0);
		assert_eq!(pool.pending_offers_in_flight(), 1);
		assert_eq!(pool.retained_completions(), 0);
		let start = std::time::Instant::now();
		block_on(pool.wait_for_in_flight_rounds(bound));
		let waited = start.elapsed();
		assert!(waited >= bound);
		assert!(waited < Duration::from_secs(5));

		// Past its grant (the bound runs from the offer's OWN push instant), a
		// later proposal does not wait on the stale offer at all.
		let start = std::time::Instant::now();
		block_on(pool.wait_for_in_flight_rounds(bound));
		assert!(start.elapsed() < bound);
	}

	#[test]
	fn authoring_grace_window_stays_free_with_no_round_offer_or_completion() {
		// The idle path is unchanged and zero-cost: no round in flight, no
		// pending offer and no recent completion means an immediate return —
		// a 30 s bound returning instantly is the proof no wait path was
		// taken.
		let pool = SpecMsgPool::default();
		let start = std::time::Instant::now();
		block_on(pool.wait_for_in_flight_rounds(Duration::from_secs(30)));
		assert!(start.elapsed() < Duration::from_secs(1));
		assert_eq!(pool.rounds_in_flight(), 0);
	}
}
