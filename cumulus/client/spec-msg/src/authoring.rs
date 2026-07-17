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

//! The authoring half of the receiver pipeline: the messaging inherent from
//! the verified pool, and the candidate's POV lifts from the built blocks'
//! consumption records.
//!
//! # Inherent
//!
//! [`inherent_data_at`] builds the block's [`SpecMsgInherentData`] at the
//! authoring parent: the runtime's resume cursors (`consumed_streams()`)
//! select the contiguous continuation of each pooled run, bounded by the
//! [`InherentBudget`] so the runtime's per-block caps are never hit by an
//! honest provider — leftover backlog stays pooled for the next block
//! (partial consumption is a lift case, fully supported). The result *is*
//! an `sp_inherents::InherentDataProvider` (issue 06): include it in the
//! node's `create_inherent_data_providers` closure and it folds itself into
//! the authoring inherent data — an empty pool puts no data at all, and the
//! block simply carries no spec-msg inherent.
//!
//! # Lifts
//!
//! [`assemble_lifts`] builds one [`RequiresLift`] per stream the candidate's
//! consumption records touch, entirely from public data: the records via
//! the `consumption_record()` runtime API (the same implementation the
//! `validate_block` wrapper calls in-wasm — the runtime stores only
//! frontiers, enough to *verify* lifts, never to *generate* them, which is
//! why assembly is necessarily node-side), the proof material from the
//! pool's verified runs. The caught-up hot path is a bare tree proof;
//! partial consumption generates the extension over the retained leaf
//! hashes. [`lift_assembler`] wraps it into the closure
//! `cumulus-client-collator`'s `CollatorService` accepts — collations whose
//! blocks consumed anything then go out as `ParachainBlockData::V3`.
//! **Resubmission** needs no special path: lifts are pure functions of
//! public data, so re-running the assembler against the then-current pool
//! regenerates them for the *unchanged* blocks.

use std::sync::Arc;

use sp_api::{ApiExt, ProvideRuntimeApi};
use sp_runtime::traits::Block as BlockT;

use cumulus_primitives_core::{ParaId, SpecMsgApi};
use cumulus_primitives_spec_messaging::{
	ConsumedStream, ConsumptionRecord, Interval, LiftsBySource, MmrFrontier, RequiresLift,
	SpecMsgInherentData, StreamId, StreamsRoot,
};

use crate::{
	pool::{InherentBudget, LiftMaterialError, SpecMsgPool},
	LOG_TARGET,
};

/// Builds the messaging inherent data for a block on `parent` from the
/// verified pool. Returns empty data — which provides nothing — when the
/// runtime does not participate, an API call fails, or nothing is pooled.
pub fn inherent_data_at<Block, Client>(
	para_id: ParaId,
	client: &Client,
	pool: &SpecMsgPool,
	parent: Block::Hash,
	budget: InherentBudget,
) -> SpecMsgInherentData
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: SpecMsgApi<Block>,
{
	let api = client.runtime_api();
	match api.has_api::<dyn SpecMsgApi<Block>>(parent) {
		Ok(true) => (),
		Ok(false) => return SpecMsgInherentData::default(),
		Err(error) => {
			tracing::debug!(target: LOG_TARGET, ?error, "Failed to query the spec-msg api");
			return SpecMsgInherentData::default();
		},
	}

	let (consumed, out_channels) = match (api.consumed_streams(parent), api.out_channels(parent)) {
		(Ok(consumed), Ok(out_channels)) => (consumed, out_channels),
		(Err(error), _) | (_, Err(error)) => {
			tracing::debug!(target: LOG_TARGET, ?error, "Failed to read the consumption views");
			return SpecMsgInherentData::default();
		},
	};

	let channel_cursors: Vec<(ParaId, StreamId, u64)> = consumed
		.iter()
		.flat_map(|(source, streams)| {
			streams.iter().filter_map(move |stream| match stream {
				consumed @ ConsumedStream::Channel { from, .. } => {
					Some((*source, consumed.stream_id(para_id), from.0))
				},
				ConsumedStream::Broadcast { .. } => None,
			})
		})
		.collect();
	let registers: Vec<(ParaId, StreamId)> = out_channels
		.keys()
		.map(|channel| {
			(
				channel.peer,
				StreamId::Ack { recipient: para_id, domain: channel.domain, num: channel.num },
			)
		})
		.collect();

	pool.build_inherent(&channel_cursors, &registers, budget)
}

/// Why the candidate's lifts could not be assembled. Loud by design: a
/// candidate whose blocks consumed anything is invalid without lifts.
#[derive(Debug, thiserror::Error)]
pub enum AssembleError {
	/// Reading a block's consumption record failed.
	#[error("runtime api error: {0}")]
	Api(#[from] sp_api::ApiError),
	/// No pooled material covers a recorded stream (or it is bound under a
	/// root diverging from the source's other streams — a fetch round is
	/// mid-flight; regeneration succeeds once it completes).
	#[error("lift material for a recorded stream is unavailable: {0}")]
	Material(#[from] LiftMaterialError),
	/// Two streams of one source would lift to different roots.
	#[error("recorded streams of one source are bound under diverging roots")]
	DivergentRoots,
	/// A stream's interval chain has a gap. The MVP provider hands each
	/// register read to exactly one block, so gaps never arise from own
	/// authoring; advance-proof generation is post-MVP.
	#[error("interval chain has a gap; advance proofs are not generated in the MVP")]
	Gap,
	/// A record names a stream kind the MVP does not consume.
	#[error("recorded stream kind is not liftable in the MVP")]
	UnsupportedStream,
	/// The synthesized lifts do not form a canonical `LiftsBySource`.
	#[error("lifts do not form a canonical per-source set")]
	NonCanonical,
}

/// Assembles the candidate's lifts for the built (and imported) `blocks`,
/// bundle order, by reading each block's `consumption_record()` and lifting
/// every recorded stream from the pool's verified material. Empty when
/// nothing was consumed.
pub fn assemble_lifts<Block, Client>(
	client: &Client,
	pool: &SpecMsgPool,
	blocks: &[Block],
) -> Result<LiftsBySource, AssembleError>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block>,
	Client::Api: SpecMsgApi<Block>,
{
	let api = client.runtime_api();
	let mut records = Vec::with_capacity(blocks.len());
	for block in blocks {
		let hash = block.hash();
		if !api.has_api::<dyn SpecMsgApi<Block>>(hash)? {
			continue;
		}
		records.push(api.consumption_record(hash)?);
	}
	assemble_from_records(pool, &records)
}

/// The pure assembly core over the bundle's records — what
/// [`assemble_lifts`] runs after collecting them via the runtime API.
pub fn assemble_from_records(
	pool: &SpecMsgPool,
	records: &[ConsumptionRecord],
) -> Result<LiftsBySource, AssembleError> {
	// Merge in bundle order — mirroring the wrapper's `build_requires`, so
	// the produced lifts match its stitched chains positionally (per source
	// the BTreeMap iterates in canonical `StreamId` order, exactly the
	// record's).
	let mut merged: std::collections::BTreeMap<
		ParaId,
		std::collections::BTreeMap<StreamId, Vec<Interval>>,
	> = Default::default();
	for record in records {
		for (source, streams) in &record.entries {
			let per_source = merged.entry(*source).or_default();
			for (stream, interval) in streams {
				per_source.entry(*stream).or_default().push(interval.clone());
			}
		}
	}

	let mut lifts: Vec<(ParaId, Vec<RequiresLift>)> = Vec::with_capacity(merged.len());
	for (source, streams) in &merged {
		let mut entry_root: Option<StreamsRoot> = None;
		let mut source_lifts = Vec::with_capacity(streams.len());
		for (stream, intervals) in streams {
			let endpoint = chain_endpoint(intervals)?;
			let (lift, root) = match stream {
				StreamId::Channel { .. } => {
					pool.channel_lift(*source, stream, endpoint.leaf_count)?
				},
				StreamId::Ack { .. } => pool.register_lift(*source, stream, endpoint.leaf_count)?,
				StreamId::Broadcast { .. } | StreamId::Private { .. } => {
					return Err(AssembleError::UnsupportedStream)
				},
			};
			match entry_root {
				None => entry_root = Some(root),
				Some(existing) if existing == root => (),
				Some(_) => return Err(AssembleError::DivergentRoots),
			}
			source_lifts.push(lift);
		}
		lifts.push((*source, source_lifts));
	}

	LiftsBySource::try_from_iter(lifts).map_err(|_| AssembleError::NonCanonical)
}

/// The endpoint of one stream's interval chain (bundle order): each
/// interval must start where the previous one ended — channel streams do by
/// statehood, register reads because the MVP provider hands each read once.
fn chain_endpoint(intervals: &[Interval]) -> Result<MmrFrontier, AssembleError> {
	let (first, rest) = intervals.split_first().ok_or(AssembleError::Gap)?;
	let mut end = first.end.clone();
	for interval in rest {
		if interval.start != end.root() {
			return Err(AssembleError::Gap);
		}
		end = interval.end.clone();
	}
	Ok(end)
}

/// Wraps [`assemble_lifts`] into the lift-assembler closure
/// `CollatorService::with_spec_msg_lift_assembler` accepts. Assembly
/// failures are logged loudly and yield no lifts — the candidate then fails
/// validation if it consumed anything, exactly as visible as submitting
/// nothing (the material regenerates on the next round; blocks never go
/// stale).
pub fn lift_assembler<Block, Client>(
	client: Arc<Client>,
	pool: Arc<SpecMsgPool>,
) -> Arc<dyn Fn(&[Block]) -> LiftsBySource + Send + Sync>
where
	Block: BlockT,
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	Client::Api: SpecMsgApi<Block>,
{
	Arc::new(move |blocks| match assemble_lifts(&*client, &pool, blocks) {
		Ok(lifts) => lifts,
		Err(error) => {
			tracing::error!(
				target: LOG_TARGET,
				?error,
				"Failed to assemble the candidate's lifts",
			);
			LiftsBySource::default()
		},
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{exchange::PeerRegistry, fetch::fetch_source, test_support::*};
	use cumulus_primitives_spec_messaging::{
		build_requires,
		test_utils::{record, SourceFixture, StreamFixture},
	};
	use futures::executor::block_on;
	use parking_lot::RwLock;
	use sc_network::PeerId;
	use std::collections::HashMap;

	const SOURCE: u32 = 2000;
	const RECEIVER: u32 = 2001;

	fn source() -> ParaId {
		ParaId::from(SOURCE)
	}

	fn all_payloads(stream: &StreamId, count: u64) -> Vec<Vec<u8>> {
		(0..count).map(|position| payload(stream, position)).collect()
	}

	fn fixture(stream: StreamId, count: u64) -> StreamFixture {
		StreamFixture::from_payloads(stream, &all_payloads(&stream, count))
	}

	/// The sender-side reference over the archive's exact current state:
	/// lifts assembled from the pool must be bit-identical to this
	/// fixture's — the shared material of issue 05's wrapper tests.
	fn source_fixture(count: u64) -> SourceFixture {
		SourceFixture::new(
			source(),
			vec![fixture(channel(RECEIVER), count), fixture(ack(RECEIVER), count)],
		)
	}

	/// An archive-served pool fetched to the head under the newest root.
	/// Returns `(network, registry, pool, root)`.
	fn fetched_pool(
		blocks: u32,
		per_block: u64,
	) -> (MockExchange, PeerRegistry, SpecMsgPool, StreamsRoot) {
		let (_aux, mut archive) = new_archive();
		let roots = import_blocks(
			&mut archive,
			&[channel(RECEIVER), ack(RECEIVER)],
			blocks,
			per_block,
			1_000,
		);
		let root = *roots.last().expect("blocks were imported");
		let honest = PeerId::random();
		let network = MockExchange {
			archive: RwLock::new(archive),
			behavior: HashMap::from([(honest, PeerBehavior::Honest)]),
			hits: Default::default(),
		};
		let registry = PeerRegistry::default();
		registry.set_peers(source(), vec![honest]);
		let pool = SpecMsgPool::default();
		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			root,
			&[(channel(RECEIVER), 0)],
			&[ack(RECEIVER)],
			1024 * 1024,
		))
		.expect("fetch round succeeds");
		(network, registry, pool, root)
	}

	#[test]
	fn partial_consumption_assembles_a_verifying_lift() {
		// The block consumed 6 of its 10-message backlog: the lift's
		// non-empty extension must land the entry on the committed root —
		// verified through the very code the `validate_block` wrapper runs.
		let (_network, _registry, pool, _root) = fetched_pool(5, 2);
		let reference = source_fixture(10);
		let stream = channel(RECEIVER);
		let records = [record([(source(), stream, reference.stream(&stream).interval(0, 6))])];

		let lifts = assemble_from_records(&pool, &records).expect("material is pooled");
		let lift = &lifts.get(source()).expect("source has lifts")[0];
		assert!(!lift.extension.is_empty());
		// Bit-identical to the sender-side reference fixture — both sides
		// generate over the same public data.
		assert_eq!(*lift, reference.lift(&stream, 6, &[]));

		let set = build_requires(&records, &lifts)
			.expect("wrapper accepts the lifts")
			.expect("something was consumed");
		assert_eq!(set.get(source()), Some(&reference.streams_root()));
	}

	#[test]
	fn caught_up_channel_and_register_read_converge_on_a_bare_tree_proof() {
		// Hot path: the block consumed the full backlog and acted on a
		// register head read. Both lifts degenerate to bare tree proofs and
		// converge on the source's one committed root.
		let (_network, _registry, pool, _root) = fetched_pool(5, 2);
		let reference = source_fixture(10);
		let (data, register) = (channel(RECEIVER), ack(RECEIVER));
		let records = [record([
			(source(), data, reference.stream(&data).interval(0, 10)),
			(source(), register, reference.stream(&register).read_context(10)),
		])];

		let lifts = assemble_from_records(&pool, &records).expect("material is pooled");
		let source_lifts = lifts.get(source()).expect("source has lifts");
		// Record order is canonical StreamId order; both are bare tree
		// proofs matching the reference fixture.
		assert_eq!(source_lifts.len(), 2);
		for lift in source_lifts {
			assert!(lift.advances.is_empty() && lift.extension.is_empty());
		}

		let set = build_requires(&records, &lifts)
			.expect("wrapper accepts the lifts")
			.expect("something was consumed");
		assert_eq!(set.get(source()), Some(&reference.streams_root()));
	}

	#[test]
	fn regenerated_lifts_target_the_newer_root_for_unchanged_blocks() {
		// The window slid past the fetch root: two more sender blocks
		// commit a newer root. Regeneration — a fresh fetch round plus
		// re-running the assembler over the UNCHANGED records — retargets
		// the lift; the block's bytes (the records) are untouched inputs.
		let (network, registry, pool, old_root) = fetched_pool(5, 2);
		let stream = channel(RECEIVER);
		let records = [record([(source(), stream, fixture(stream, 10).interval(0, 6))])];
		let old_lifts = assemble_from_records(&pool, &records).expect("material is pooled");

		// The sender chain advances: 2 more blocks × 2 messages per stream.
		let new_root = {
			let mut archive = network.archive.write();
			let mut new_root = None;
			for number in 6..=7 {
				let sends = [channel(RECEIVER), ack(RECEIVER)]
					.into_iter()
					.map(|stream| {
						let start = (number as u64 - 1) * 2;
						(stream, (start..start + 2).map(|i| payload(&stream, i)).collect())
					})
					.collect();
				new_root = archive
					.import_block_at(
						block_hash(number),
						block_hash(number - 1),
						number,
						sends,
						1_000 + number as u64,
					)
					.expect("extends the tip");
			}
			new_root.expect("streams exist")
		};
		assert_ne!(new_root, old_root);

		// A register read still bound to the OLD root cannot converge with
		// channel material re-bound under the new one: mid-regeneration
		// mixes are refused, never mis-lifted.
		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			new_root,
			&[(stream, 6)],
			&[],
			1024 * 1024,
		))
		.expect("regeneration round succeeds");
		let mixed = [record([
			(source(), stream, fixture(stream, 14).interval(0, 6)),
			(source(), ack(RECEIVER), fixture(ack(RECEIVER), 10).read_context(10)),
		])];
		assert!(matches!(assemble_from_records(&pool, &mixed), Err(AssembleError::DivergentRoots)));

		// The full regeneration round (registers included) retargets the
		// unchanged record's lift at the newer root.
		block_on(fetch_source(
			&network,
			&registry,
			&pool,
			source(),
			new_root,
			&[(stream, 6)],
			&[ack(RECEIVER)],
			1024 * 1024,
		))
		.expect("regeneration round succeeds");

		let reference = source_fixture(14);
		let new_lifts = assemble_from_records(&pool, &records).expect("material re-pooled");
		let lift = &new_lifts.get(source()).expect("source has lifts")[0];
		assert_eq!(*lift, reference.lift(&stream, 6, &[]));
		assert_ne!(old_lifts, new_lifts);

		let set = build_requires(&records, &new_lifts)
			.expect("wrapper accepts the regenerated lifts")
			.expect("something was consumed");
		assert_eq!(set.get(source()), Some(&reference.streams_root()));
	}

	#[test]
	fn inherent_hands_the_contiguous_continuation_within_budget() {
		// 10 payloads of 16 bytes each are pooled per stream.
		let (_network, _registry, pool, root) = fetched_pool(5, 2);
		let stream = channel(RECEIVER);

		// Cursor 4: ancestors consumed four; a 64-byte budget hands exactly
		// the four-payload continuation 4..8, the rest stays pooled.
		let budget = InherentBudget { max_bytes: 64, max_streams: 8 };
		let data = pool.build_inherent(&[(source(), stream, 4)], &[], budget);
		assert_eq!(data.messages, vec![(source(), stream, all_payloads(&stream, 8)[4..].to_vec())]);
		assert!(data.register_reads.is_empty());
		assert_eq!(pool.pooled_payloads(source(), &stream), 6);

		// Register reads are handed at most once per verified read.
		let data =
			pool.build_inherent(&[], &[(source(), ack(RECEIVER))], InherentBudget::default());
		assert_eq!(data.register_reads.len(), 1);
		let again =
			pool.build_inherent(&[], &[(source(), ack(RECEIVER))], InherentBudget::default());
		assert!(again.is_empty());

		// Material not bound to the source's current target is withheld —
		// the guarantee that what a block consumes is liftable locally.
		let foreign = StreamsRoot(polkadot_core_primitives::Hash::repeat_byte(0xEE));
		pool.complete_round(source(), foreign);
		let withheld =
			pool.build_inherent(&[(source(), stream, 4)], &[], InherentBudget::default());
		assert!(withheld.is_empty());
		pool.complete_round(source(), root);

		// A cursor beyond the pooled run (another collator's block consumed
		// material we never fetched) drops the stale ledger.
		let beyond = pool.build_inherent(&[(source(), stream, 12)], &[], InherentBudget::default());
		assert!(beyond.is_empty());
		assert_eq!(pool.pooled_payloads(source(), &stream), 0);
	}

	#[test]
	fn uncovered_records_and_gapped_chains_fail_loudly() {
		let (_network, _registry, pool, _root) = fetched_pool(3, 2);
		let stream = channel(RECEIVER);
		let reference = fixture(stream, 6);

		// A record of a stream the pool never fetched.
		let unknown = channel(RECEIVER + 1);
		let records = [record([(source(), unknown, fixture(unknown, 4).interval(0, 4))])];
		assert!(matches!(
			assemble_from_records(&pool, &records),
			Err(AssembleError::Material(LiftMaterialError::NotCovered))
		));

		// A gapped interval chain (two register read contexts): advance
		// proofs are not generated in the MVP — refuse, do not mis-lift.
		let gapped = [
			record([(source(), ack(RECEIVER), fixture(ack(RECEIVER), 6).read_context(3))]),
			record([(source(), ack(RECEIVER), fixture(ack(RECEIVER), 6).read_context(5))]),
		];
		assert!(matches!(assemble_from_records(&pool, &gapped), Err(AssembleError::Gap)));

		// Channel intervals chaining by equality across a bundle assemble
		// fine (the single lift covers the stitched chain).
		let bundle = [
			record([(source(), stream, reference.interval(0, 3))]),
			record([(source(), stream, reference.interval(3, 6))]),
		];
		let lifts = assemble_from_records(&pool, &bundle).expect("chained bundle assembles");
		let set = build_requires(&bundle, &lifts)
			.expect("wrapper accepts the bundle's lift")
			.expect("something was consumed");
		assert_eq!(set.get(source()), Some(&source_fixture(6).streams_root()));

		// An empty record synthesizes nothing and carries no lifts.
		assert_eq!(
			assemble_from_records(&pool, &[]).expect("empty assembles"),
			LiftsBySource::default()
		);
	}
}
