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

use std::{collections::VecDeque, path::PathBuf, sync::Arc, time::Instant};

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::ValidationCodeHashProvider;
use cumulus_client_resubmission_store::ResubmissionStore;
use cumulus_primitives_core::SchedulingProof;
use cumulus_relay_chain_interface::RelayChainInterface;
use futures::{prelude::*, stream::FusedStream};
use polkadot_node_primitives::{
	MaybeCompressedPoV, SegmentCollation, SubmitSegmentParams, MAX_SEGMENT_LEN,
};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{
	CandidateDescriptorVersion, CollatorPair, CoreIndex, Header as RelayHeader, Id as ParaId,
};
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::{
	traits::{Block as BlockT, Header},
	BoundedVec,
};

use crate::export_pov_to_path;

use super::{
	block_builder_task::MAX_HEDGED_SIBLINGS,
	message::{CollationParts, CollatorMessage},
	resubmittable_segment::ResubmittableSegment,
};

const LOG_TARGET: &str = "aura::cumulus::collation_task";

/// Extra PoV builds one core may spend on hedging: a sibling rebuilds the whole segment, so a
/// long backlog buys fewer siblings, and past this length none at all.
const MAX_HEDGED_REBUILDS_PER_CORE: usize = 6;

/// Hedged siblings a core submitting `entries` PoVs can afford (0 when there is nothing).
fn hedge_budget(entries: usize) -> usize {
	MAX_HEDGED_REBUILDS_PER_CORE
		.checked_div(entries)
		.unwrap_or_default()
		.min(MAX_HEDGED_SIBLINGS)
}

/// A segment submission, kept as headers only and re-hydrated when rebuilt. Both the last
/// submitted segment and a queued hedge (the same segment under a sibling's proof) use it.
struct SegmentSubmission<Block: BlockT> {
	core_index: CoreIndex,
	headers: Vec<Block::Header>,
	proof: SchedulingProof,
	resubmitted: usize,
	fresh: usize,
	/// End of the production relay slot; a hedge is dropped unbuilt after it.
	valid_until: Instant,
}

/// Hedged rebuilds of `last` under each sibling chain, within its budget; empty unless `last` is
/// on `core_index`.
fn hedges_for<Block: BlockT>(
	last: Option<SegmentSubmission<Block>>,
	core_index: CoreIndex,
	header_chains: Vec<Vec<RelayHeader>>,
) -> Vec<SegmentSubmission<Block>> {
	let Some(last) = last.filter(|last| last.core_index == core_index) else {
		tracing::debug!(
			target: LOG_TARGET,
			?core_index,
			"Dropping hedges without a hedgeable segment.",
		);
		return Vec::new();
	};

	header_chains
		.into_iter()
		.take(hedge_budget(last.resubmitted + last.fresh))
		.map(|header_chain| SegmentSubmission {
			core_index,
			headers: last.headers.clone(),
			proof: SchedulingProof { header_chain, ..last.proof.clone() },
			resubmitted: last.resubmitted,
			fresh: last.fresh,
			valid_until: last.valid_until,
		})
		.collect()
}

/// Remember a handled message's segment; a segment for `core`, sent or not, supersedes that core's
/// queued hedges.
fn record_submission<Block: BlockT>(
	pending: &mut VecDeque<SegmentSubmission<Block>>,
	last_segment: &mut Option<SegmentSubmission<Block>>,
	core: Option<CoreIndex>,
	submitted: Option<SegmentSubmission<Block>>,
) {
	if let Some(core) = core {
		pending.retain(|hedge| hedge.core_index != core);
	}
	*last_segment = submitted;
}

/// Next unit of work: a ready message always wins over a queued hedged rebuild.
enum Work<Block: BlockT> {
	Message(CollatorMessage<Block>),
	Hedge(SegmentSubmission<Block>),
}

/// A ready message, else the oldest unexpired queued hedge; a hedge is never raced, so never
/// cancelled. After `receiver` closes, every unexpired hedge is served before `None`.
async fn next_work<Block: BlockT>(
	receiver: &mut (impl FusedStream<Item = CollatorMessage<Block>> + Unpin),
	pending: &mut VecDeque<SegmentSubmission<Block>>,
) -> Option<Work<Block>> {
	let queued = pending.len();
	let now = Instant::now();
	pending.retain(|hedge| hedge.valid_until > now);
	if pending.len() < queued {
		tracing::debug!(
			target: LOG_TARGET,
			expired = queued - pending.len(),
			"Dropping hedges past their relay slot.",
		);
	}

	if pending.is_empty() {
		return receiver.next().await.map(Work::Message);
	}

	futures::select_biased! {
		message = receiver.next() => if let Some(message) = message {
			return Some(Work::Message(message));
		},
		default => {},
		complete => {},
	}

	pending.pop_front().map(Work::Hedge)
}

/// Parameters for the collation task.
pub struct Params<Block: BlockT, RClient, CS, Backend, CHP> {
	/// A handle to the relay-chain client.
	pub relay_client: RClient,
	/// The collator key used to sign collations before submitting to validators.
	pub collator_key: CollatorPair,
	/// The para's ID.
	pub para_id: ParaId,
	/// Whether we should reinitialize the collator config (i.e. we are transitioning to aura).
	pub reinitialize: bool,
	/// Collator service interface
	pub collator_service: CS,
	/// Receiver channel for collation/segment messages from the block builder task.
	pub collator_receiver: TracingUnboundedReceiver<CollatorMessage<Block>>,
	/// When set, the collator will export every produced `POV` to this folder.
	pub export_pov: Option<PathBuf>,
	/// The para client's backend, used to hydrate resubmitted segment entries from the
	/// resubmission store — done here, off the block-production hot path.
	pub para_backend: Arc<Backend>,
	/// Validation code hash provider, used while hydrating resubmitted segment entries.
	pub code_hash_provider: CHP,
}

/// Asynchronously executes the collation task for a parachain.
///
/// This function initializes the collator subsystems necessary for producing and submitting
/// collations to the relay chain. It listens for new best relay chain block notifications and
/// handles collator messages. If our parachain is scheduled on a core and we have a candidate,
/// the task will build a collation and send it to the relay chain.
pub async fn run_collation_task<Block, RClient, CS, Backend, CHP>(
	Params {
		relay_client,
		collator_key,
		para_id,
		reinitialize,
		collator_service,
		mut collator_receiver,
		export_pov,
		para_backend,
		code_hash_provider,
	}: Params<Block, RClient, CS, Backend, CHP>,
) where
	Block: BlockT,
	CS: CollatorServiceInterface<Block> + Send + Sync + 'static,
	RClient: RelayChainInterface + Clone + 'static,
	Backend: sc_client_api::Backend<Block> + 'static,
	CHP: ValidationCodeHashProvider<Block::Hash> + Send + Sync + 'static,
{
	let Ok(mut overseer_handle) = relay_client.overseer_handle() else {
		tracing::error!(target: LOG_TARGET, "Failed to get overseer handle.");
		return;
	};

	cumulus_client_collator::initialize_collator_subsystems(
		&mut overseer_handle,
		collator_key,
		para_id,
		reinitialize,
	)
	.await;

	// Read-side handle over the resubmission aux store, used to hydrate the V3 resubmittable
	// headers off the block-production hot path. Cheap to hold (wraps the backend `Arc`).
	let resubmission_store = ResubmissionStore::new(para_backend.clone());

	let mut submitter = CollationSubmitter {
		collator_service,
		overseer_handle,
		export_pov,
		para_backend,
		code_hash_provider,
		resubmission_store,
	};

	// Hedged rebuilds deferred behind their segment's main submission; served only when no new
	// message is waiting, so main submissions for other cores always go first.
	let mut pending: VecDeque<SegmentSubmission<Block>> = VecDeque::new();
	// `Hedges` always directly follows its `Segment`, so only the last message's segment is kept.
	let mut last_segment: Option<SegmentSubmission<Block>> = None;

	while let Some(work) = next_work(&mut collator_receiver, &mut pending).await {
		match work {
			Work::Message(CollatorMessage::Collation { core_index, parts }) => {
				submitter.submit_collation(core_index, parts).await;
				record_submission(&mut pending, &mut last_segment, None, None);
			},
			Work::Message(CollatorMessage::Segment {
				scheduling_proof,
				core_index,
				resubmittable_headers,
				bundle,
				valid_until,
			}) => {
				let submitted = submitter
					.submit_segment(
						scheduling_proof,
						core_index,
						resubmittable_headers,
						bundle,
						valid_until,
					)
					.await;
				record_submission(&mut pending, &mut last_segment, Some(core_index), submitted);
			},
			Work::Message(CollatorMessage::Hedges { core_index, header_chains }) => {
				pending.extend(hedges_for(last_segment.take(), core_index, header_chains));
			},
			Work::Hedge(hedge) => submitter.submit_hedge(hedge).await,
		}
	}
}

/// Turns [`CollatorMessage`]s into collations and forwards them to the collation-generation
/// subsystem.
struct CollationSubmitter<Block: BlockT, CS, Backend, CHP> {
	collator_service: CS,
	overseer_handle: OverseerHandle,
	export_pov: Option<PathBuf>,
	/// Backend, store, and code-hash provider hydrate the resubmitted headers into
	/// [`CollationParts`] off the block-production hot path.
	para_backend: Arc<Backend>,
	code_hash_provider: CHP,
	resubmission_store: ResubmissionStore<Block, Backend>,
}

impl<Block, CS, Backend, CHP> CollationSubmitter<Block, CS, Backend, CHP>
where
	Block: BlockT,
	CS: CollatorServiceInterface<Block>,
	Backend: sc_client_api::Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	/// Submit a single collation as a one-element V2 segment: no scheduling proof, the scheduling
	/// parent is the relay parent.
	async fn submit_collation(&mut self, core_index: CoreIndex, parts: CollationParts<Block>) {
		let Some(segment_collation) = self.build_collation(parts, None).await else {
			return;
		};
		let scheduling_parent = segment_collation.relay_parent;

		tracing::debug!(target: LOG_TARGET, ?core_index, ?scheduling_parent, "Submitting collation for core.");

		self.overseer_handle
			.send_msg(
				CollationGenerationMessage::SubmitSegment(SubmitSegmentParams {
					scheduling_parent,
					core_index,
					candidates_descriptor_version: CandidateDescriptorVersion::V2,
					collations: BoundedVec::truncate_from(vec![segment_collation]),
				}),
				"SubmitSegment",
			)
			.await;
	}

	/// Submit a V3 segment; `Some` only if it sent something a following `Hedges` could rebuild.
	async fn submit_segment(
		&mut self,
		scheduling_proof: SchedulingProof,
		core_index: CoreIndex,
		resubmittable_headers: Vec<Block::Header>,
		bundle: Option<CollationParts<Block>>,
		valid_until: Instant,
	) -> Option<SegmentSubmission<Block>> {
		// Hydrate the resubmitted headers here (proof/body reads), off the block-production hot
		// path, then prepend them (oldest first) to the freshly-built bundle.
		let mut all_parts = self.hydrate(resubmittable_headers);
		let resubmitted = all_parts.len();
		// `bundle` holds at most one freshly-built bundle.
		let fresh = usize::from(bundle.is_some());
		all_parts.extend(bundle);

		// Hedges keep only the headers; the parts move into the main submission. Skipped when no
		// hedge can follow: offset 0 (empty chain, nothing to swap) or no budget for the backlog.
		let hedgeable =
			!scheduling_proof.header_chain.is_empty() && hedge_budget(resubmitted + fresh) > 0;
		let headers: Option<Vec<_>> = hedgeable.then(|| {
			all_parts
				.iter()
				.flat_map(|parts| parts.blocks.iter().map(|b| b.header().clone()))
				.collect()
		});

		let sent = self
			.send_segment(
				all_parts,
				scheduling_proof.clone(),
				core_index,
				false,
				resubmitted,
				fresh,
			)
			.await;

		Some(SegmentSubmission {
			core_index,
			headers: headers.filter(|_| sent)?,
			proof: scheduling_proof,
			resubmitted,
			fresh,
			valid_until,
		})
	}

	/// Rebuild a hedged segment from its headers and submit it under the sibling's proof.
	async fn submit_hedge(&mut self, hedge: SegmentSubmission<Block>) {
		let SegmentSubmission { core_index, headers, proof, resubmitted, fresh, .. } = hedge;
		// Store rows are written at build time; a block without one is skipped here, though the
		// main submission carried it.
		let parts = self.hydrate(headers);
		self.send_segment(parts, proof, core_index, true, resubmitted, fresh).await;
	}

	fn hydrate(&self, headers: Vec<Block::Header>) -> Vec<CollationParts<Block>> {
		ResubmittableSegment::new(headers).hydrate(
			&*self.para_backend,
			&self.code_hash_provider,
			&self.resubmission_store,
		)
	}

	/// Build `all_parts` under `scheduling_proof` and submit them for `core_index`; returns whether
	/// anything was sent. Parts that fail to build are skipped, not fatal to the segment.
	/// `resubmitted` and `fresh` only feed the logs.
	async fn send_segment(
		&mut self,
		all_parts: Vec<CollationParts<Block>>,
		scheduling_proof: SchedulingProof,
		core_index: CoreIndex,
		hedged: bool,
		resubmitted: usize,
		fresh: usize,
	) -> bool {
		// Logged on every submission so a rejected one can be traced to its scheduling parent.
		let scheduling_parent = scheduling_proof.scheduling_parent();
		let total = resubmitted + fresh;

		let mut collations = Vec::with_capacity(all_parts.len());
		for parts in all_parts {
			if let Some(collation) =
				self.build_collation(parts, Some(scheduling_proof.clone())).await
			{
				collations.push(collation);
			}
		}

		if collations.is_empty() {
			tracing::debug!(
				target: LOG_TARGET,
				?core_index,
				?scheduling_parent,
				hedged,
				resubmitted,
				fresh,
				"No collations built for segment; nothing submitted for core.",
			);
			return false;
		}

		if collations.len() > MAX_SEGMENT_LEN as usize {
			tracing::warn!(
				target: LOG_TARGET,
				?core_index,
				?scheduling_parent,
				hedged,
				segment_len = collations.len(),
				max = MAX_SEGMENT_LEN,
				"Segment exceeds MAX_SEGMENT_LEN; truncating.",
			);
		}

		tracing::debug!(
			target: LOG_TARGET,
			?core_index,
			?scheduling_parent,
			hedged,
			segment_len = collations.len(),
			resubmitted,
			fresh,
			dropped = total.saturating_sub(collations.len()),
			"Submitting segment for core.",
		);

		self.overseer_handle
			.send_msg(
				CollationGenerationMessage::SubmitSegment(SubmitSegmentParams {
					scheduling_parent,
					core_index,
					candidates_descriptor_version: CandidateDescriptorVersion::V3,
					collations: BoundedVec::truncate_from(collations),
				}),
				"SubmitSegment",
			)
			.await;
		true
	}
}

impl<Block, CS, Backend, CHP> CollationSubmitter<Block, CS, Backend, CHP>
where
	Block: BlockT,
	CS: CollatorServiceInterface<Block>,
	Backend: sc_client_api::Backend<Block>,
	CHP: ValidationCodeHashProvider<Block::Hash>,
{
	/// Build one `SegmentCollation` from `parts`: build the PoV and export it if configured. The
	/// session and relay-parent header data are carried in `parts`, resolved at build/hydration
	/// time. Returns `None` if the collation could not be built.
	async fn build_collation(
		&self,
		parts: CollationParts<Block>,
		scheduling_proof: Option<SchedulingProof>,
	) -> Option<SegmentCollation> {
		let CollationParts {
			relay_parent,
			parent_header,
			blocks,
			proof,
			validation_code_hash,
			validation_data,
			relay_parent_session,
			relay_parent_storage_root,
			relay_parent_number,
		} = parts;

		// The scheduling proof, including its UMP scheduling-tail override, is applied inside
		// `build_multi_block_collation`.
		let (collation, block_data) = match self.collator_service.build_multi_block_collation(
			&parent_header,
			blocks,
			proof,
			scheduling_proof,
		) {
			Some(collation) => collation,
			None => {
				tracing::warn!(target: LOG_TARGET, ?relay_parent, "Unable to build collation.");
				return None;
			},
		};

		block_data.log_size_info();

		if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
			if let Some(pov_path) = self.export_pov.as_ref() {
				if let Some(header) = block_data.blocks().first().map(|b| b.header()) {
					export_pov_to_path::<Block>(
						pov_path,
						pov.clone(),
						header.hash(),
						*header.number(),
						parent_header.clone(),
						relay_parent_storage_root,
						relay_parent_number,
						validation_data.max_pov_size,
					);
				}
			}

			tracing::info!(
				target: LOG_TARGET,
				block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
				"Compressed PoV size: {}kb",
				pov.block_data.0.len() as f64 / 1024f64,
			);
		}

		Some(SegmentCollation {
			relay_parent,
			collation,
			validation_code_hash,
			session_index: relay_parent_session,
			validation_data,
		})
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::{SchedulingInfoPayload, SignedSchedulingInfo};
	use cumulus_test_client::runtime::Block;
	use polkadot_primitives::CoreSelector;
	use std::time::Duration;

	/// A header distinguished only by its block number.
	fn header(number: u32) -> <Block as BlockT>::Header {
		Header::new(
			number,
			Default::default(),
			Default::default(),
			Default::default(),
			Default::default(),
		)
	}

	/// A proof distinguished only by its internal scheduling parent's relay height.
	fn scheduling_proof(number: u32) -> SchedulingProof {
		SchedulingProof {
			header_chain: vec![],
			internal_scheduling_parent_header: RelayHeader {
				parent_hash: Default::default(),
				number,
				state_root: Default::default(),
				extrinsics_root: Default::default(),
				digest: Default::default(),
			},
			signed_scheduling_info: Some(SignedSchedulingInfo {
				payload: SchedulingInfoPayload::new(
					CoreSelector(1),
					2,
					Default::default(),
					Default::default(),
				),
				signature: [7; 64],
			}),
		}
	}

	fn segment(
		core_index: CoreIndex,
		resubmitted: usize,
		fresh: usize,
	) -> SegmentSubmission<Block> {
		SegmentSubmission {
			core_index,
			headers: vec![header(100), header(101)],
			proof: scheduling_proof(7),
			resubmitted,
			fresh,
			valid_until: Instant::now() + Duration::from_secs(60),
		}
	}

	fn queued(proof: u32, expired: bool) -> SegmentSubmission<Block> {
		let segment = segment(CoreIndex(0), 0, 1);
		let valid_until = if expired { Instant::now() } else { segment.valid_until };
		SegmentSubmission { proof: scheduling_proof(proof), valid_until, ..segment }
	}

	fn segment_message(core_index: CoreIndex) -> CollatorMessage<Block> {
		CollatorMessage::Segment {
			scheduling_proof: scheduling_proof(0),
			core_index,
			resubmittable_headers: vec![],
			bundle: None,
			valid_until: Instant::now(),
		}
	}

	/// The next work as `("segment", core)` or `("hedge", proof)`; never blocks on a queued hedge.
	async fn serve(
		rx: &mut TracingUnboundedReceiver<CollatorMessage<Block>>,
		pending: &mut VecDeque<SegmentSubmission<Block>>,
	) -> Option<(&'static str, u32)> {
		let work = tokio::time::timeout(Duration::from_secs(5), next_work(rx, pending))
			.await
			.expect("next_work must not block");
		match work? {
			Work::Message(CollatorMessage::Segment { core_index, .. }) => {
				Some(("segment", core_index.0))
			},
			Work::Hedge(hedge) => {
				Some(("hedge", hedge.proof.internal_scheduling_parent_header.number))
			},
			Work::Message(_) => panic!("only segments are sent"),
		}
	}

	/// A ready message always wins; an idle open channel lets the oldest live hedge through, an
	/// expired one is never served, and after close the remaining hedges drain before `None`.
	#[tokio::test]
	async fn next_work_serves_messages_then_live_hedges() {
		let mut pending = VecDeque::from([queued(1, true), queued(2, false), queued(3, false)]);
		let (tx, mut rx) = sc_utils::mpsc::tracing_unbounded("test", 16);

		tx.unbounded_send(segment_message(CoreIndex(1)))
			.expect("receiver is alive; qed");
		assert_eq!(serve(&mut rx, &mut pending).await, Some(("segment", 1)));
		assert_eq!(serve(&mut rx, &mut pending).await, Some(("hedge", 2)));
		tx.unbounded_send(segment_message(CoreIndex(2)))
			.expect("receiver is alive; qed");
		assert_eq!(serve(&mut rx, &mut pending).await, Some(("segment", 2)));
		drop(tx);
		assert_eq!(serve(&mut rx, &mut pending).await, Some(("hedge", 3)));
		assert_eq!(serve(&mut rx, &mut pending).await, None);
	}

	/// Hedges rebuild their own core's last segment under each chain, within the backlog budget,
	/// and the next segment for that core, sent or not, supersedes them.
	#[test]
	fn hedges_follow_their_segment() {
		let chains = vec![vec![scheduling_proof(8).internal_scheduling_parent_header], vec![]];
		let last = segment(CoreIndex(0), 1, 1);
		let valid_until = last.valid_until;
		let hedges = hedges_for(Some(last), CoreIndex(0), chains.clone());
		assert_eq!(hedges.len(), 2);
		for (hedge, chain) in hedges.iter().zip(&chains) {
			let proof = SchedulingProof { header_chain: chain.clone(), ..scheduling_proof(7) };
			assert_eq!(hedge.proof, proof);
			assert_eq!(hedge.headers, vec![header(100), header(101)]);
			assert_eq!((hedge.core_index, hedge.resubmitted, hedge.fresh), (CoreIndex(0), 1, 1));
			assert_eq!(hedge.valid_until, valid_until);
		}
		assert!(hedges_for(Some(segment(CoreIndex(0), 1, 1)), CoreIndex(1), chains).is_empty());

		for (entries, kept) in [
			(0, 0),
			(1, MAX_HEDGED_SIBLINGS),
			(3, 2),
			(4, 1),
			(MAX_HEDGED_REBUILDS_PER_CORE + 1, 0),
		] {
			let last = Some(segment(CoreIndex(0), entries, 0));
			assert_eq!(hedges_for(last, CoreIndex(0), vec![vec![]; 3]).len(), kept);
		}
		for entries in 0..=(MAX_HEDGED_REBUILDS_PER_CORE + 2) {
			assert!(entries * hedge_budget(entries) <= MAX_HEDGED_REBUILDS_PER_CORE);
		}

		let (mut pending, mut last) = (VecDeque::new(), None);
		let record = |pending: &mut _, last: &mut _, core: CoreIndex, sent: bool| {
			record_submission(pending, last, Some(core), sent.then(|| segment(core, 0, 1)))
		};
		record(&mut pending, &mut last, CoreIndex(0), true);
		pending.extend(hedges_for(last.take(), CoreIndex(0), vec![vec![]; 2]));
		// Another core's segment leaves core 0's hedges queued; core 0's next one, even unsent,
		// drops them.
		record(&mut pending, &mut last, CoreIndex(1), true);
		assert_eq!(pending.len(), 2);
		record(&mut pending, &mut last, CoreIndex(0), false);
		assert!(pending.is_empty());

		// A V2 collation in between leaves nothing to hedge.
		record_submission(&mut pending, &mut last, None, None);
		pending.extend(hedges_for(last.take(), CoreIndex(0), vec![vec![]]));
		assert!(pending.is_empty());
	}
}
