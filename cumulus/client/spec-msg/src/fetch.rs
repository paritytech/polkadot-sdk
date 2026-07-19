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
//! the lift assembler succeeds from local material. Failed rounds are
//! simply retried on the next trigger — fetching is root-keyed and
//! idempotent.
//!
//! This node-side pre-verification protects the collator from building on
//! bad p2p data; consensus-level enforcement is the PVF's lift check
//! (issue 05). [`RelayProvidesEvent::Pending`] prefetch hints are ignored
//! in the MVP: material fetched under a merely backed root cannot be
//! consumed or lifted, and keeping it out of the pool avoids poisoning the
//! contiguous runs on abandoned forks.

use std::sync::Arc;

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
	monitor::RelayProvidesEvent,
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

/// Errors of one fetch round. All are retried on the next trigger.
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
/// target for the source is `root`.
///
/// Register reads are best-effort: an unpublished register is refused at
/// the transport level, indistinguishable from a failure — neither may
/// poison the round.
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
	for (stream, cursor) in channels {
		fetch_channel_stream(network, peers, pool, source, root, *stream, *cursor, chunk_bytes)
			.await?;
	}

	for stream in registers {
		if let Err(error) = fetch_register(network, peers, pool, source, root, *stream).await {
			tracing::debug!(
				target: LOG_TARGET,
				%error,
				source = %u32::from(source),
				?stream,
				"Register head read not served (unpublished register, or no peer)",
			);
		}
	}

	pool.complete_round(source, root);
	Ok(())
}

/// Fetches one channel stream's backlog under `root`, chunked and resumed
/// from `base + received`, into the pool.
async fn fetch_channel_stream(
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	pool: &SpecMsgPool,
	source: ParaId,
	root: StreamsRoot,
	stream: StreamId,
	cursor: u64,
	chunk_bytes: u32,
) -> Result<(), FetchError> {
	// Resume from the pooled run where it covers the runtime cursor; a
	// stale or absent run restarts trust-free from the cursor (the
	// response's verified `start_peaks` seed the new run's base).
	pool.prune_channel(source, &stream, cursor);
	let (mut start, mut own) = match pool.resume(source, &stream) {
		Some((end, frontier)) => (end, Some(frontier)),
		None => (cursor, None),
	};

	for _ in 0..MAX_CHUNKS_PER_ROUND {
		let request = MessagesRequest {
			stream,
			start: MessagePosition(start),
			under: root,
			max_bytes: chunk_bytes,
		};
		let (verified, response) =
			request_messages(network, peers, source, &request, own.as_ref()).await?;

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

		if caught_up {
			return Ok(());
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
	while let Ok(event) = events.recv().await {
		let provides = match event {
			RelayProvidesEvent::Included(provides) => provides,
			// Prefetch hints are a post-MVP latency win; see the module
			// docs for why the MVP pool holds included-bound material only.
			RelayProvidesEvent::Pending(_) => continue,
		};

		if let Err(error) =
			fetch_included(para_id, &*parachain, &network, &*peers, &pool, &provides).await
		{
			tracing::warn!(
				target: LOG_TARGET,
				?error,
				source = %u32::from(provides.source),
				root = ?provides.root,
				"Fetch round failed; retrying on the next included root",
			);
		}
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

/// Resolves what to fetch from the own runtime and runs the round.
async fn fetch_included<Block, Client>(
	para_id: ParaId,
	parachain: &Client,
	network: &impl ExchangeNetwork,
	peers: &dyn SourcePeers,
	pool: &SpecMsgPool,
	provides: &crate::monitor::SourceProvides,
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
	use parking_lot::RwLock;
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
			pool.build_inherent(&[], &[(source(), ack(RECEIVER))], InherentBudget::default());
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
}
