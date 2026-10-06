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

use std::{collections::VecDeque, path::PathBuf, sync::Arc};

use cumulus_client_collator::service::ServiceInterface as CollatorServiceInterface;
use cumulus_client_consensus_common::ValidationCodeHashProvider;
use cumulus_client_resubmission_store::ResubmissionStore;
use cumulus_relay_chain_interface::RelayChainInterface;

use polkadot_node_primitives::{
	MaybeCompressedPoV, SegmentCollation, SubmitSegmentParams, UpwardMessages, MAX_SEGMENT_LEN,
};
use polkadot_node_subsystem::messages::CollationGenerationMessage;
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{
	CandidateDescriptorVersion, CollatorPair, CoreIndex, Header as RelayHeader, Id as ParaId,
};

use codec::{Decode, Encode};
use cumulus_primitives_core::{
	relay_chain::{BlockId, UMPSignal, UMP_SEPARATOR},
	ClaimQueueOffset, SchedulingProof, SignedSchedulingInfo,
};
use futures::{prelude::*, stream::FusedStream};

use crate::export_pov_to_path;
use sc_utils::mpsc::TracingUnboundedReceiver;
use sp_runtime::traits::{Block as BlockT, Header};

use super::{CollatorMessage, CollatorSegmentEntry, CollatorSegmentMessage};

const LOG_TARGET: &str = "aura::cumulus::collation_task";

/// One hedged rebuild of a submitted segment under a sibling's proof, deferred so later main
/// submissions go first. Holds headers only; it re-hydrates its entries when served.
struct Hedge<Block: BlockT> {
	core_index: CoreIndex,
	headers: Vec<Block::Header>,
	proof: SchedulingProof,
	resubmitted: usize,
	fresh: usize,
}

/// The last submitted segment, kept so a following `Hedges` message can rebuild it.
struct SubmittedSegment<Block: BlockT> {
	core_index: CoreIndex,
	headers: Vec<Block::Header>,
	/// The chosen proof; hedges share its signed payload and differ only in the header chain.
	proof: SchedulingProof,
	resubmitted: usize,
	fresh: usize,
}

/// Hedged rebuilds of `last` under each sibling chain; empty unless `last` is on `core_index`.
fn hedges_for<Block: BlockT>(
	last: Option<SubmittedSegment<Block>>,
	core_index: CoreIndex,
	header_chains: Vec<Vec<RelayHeader>>,
) -> Vec<Hedge<Block>> {
	let Some(last) = last.filter(|last| last.core_index == core_index) else {
		tracing::debug!(
			target: LOG_TARGET,
			?core_index,
			"Dropping hedges without a matching segment.",
		);
		return Vec::new();
	};

	header_chains
		.into_iter()
		.map(|header_chain| Hedge {
			core_index,
			headers: last.headers.clone(),
			proof: SchedulingProof { header_chain, ..last.proof.clone() },
			resubmitted: last.resubmitted,
			fresh: last.fresh,
		})
		.collect()
}

/// Remember a handled message's segment; a new segment supersedes its core's queued hedges.
fn record_submission<Block: BlockT>(
	pending: &mut VecDeque<Hedge<Block>>,
	last_segment: &mut Option<SubmittedSegment<Block>>,
	submitted: Option<SubmittedSegment<Block>>,
) {
	if let Some(segment) = &submitted {
		pending.retain(|hedge| hedge.core_index != segment.core_index);
	}
	*last_segment = submitted;
}

/// Next unit of work: a ready message always wins over a queued hedged rebuild.
enum Work<Block: BlockT> {
	Message(CollatorMessage<Block>),
	Hedge(Hedge<Block>),
}

/// A ready message, else the oldest queued hedge; a hedge is never raced, so never cancelled.
/// After `receiver` closes, every queued hedge is served before `None`.
async fn next_work<Block: BlockT>(
	receiver: &mut (impl FusedStream<Item = CollatorMessage<Block>> + Unpin),
	pending: &mut VecDeque<Hedge<Block>>,
) -> Option<Work<Block>> {
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

	// Read-side handle over the resubmission aux store, used to hydrate the V3 unincluded segment
	// off the block-production hot path. Cheap to hold (wraps the backend `Arc`).
	let resubmission_store = ResubmissionStore::new(para_backend.clone());

	// Hedged rebuilds deferred behind their segment's main submission; served only when no new
	// message is waiting, so main submissions for other cores always go first.
	let mut pending: VecDeque<Hedge<Block>> = VecDeque::new();
	// `Hedges` always directly follows its `Segment`, so only the last message's segment is kept.
	let mut last_segment: Option<SubmittedSegment<Block>> = None;

	while let Some(work) = next_work(&mut collator_receiver, &mut pending).await {
		match work {
			Work::Message(CollatorMessage::Hedges { core_index, header_chains }) => {
				pending.extend(hedges_for(last_segment.take(), core_index, header_chains));
			},
			Work::Message(message) => {
				let submitted = message
					.handle(
						&collator_service,
						&mut overseer_handle,
						relay_client.clone(),
						export_pov.clone(),
						&*para_backend,
						&code_hash_provider,
						&resubmission_store,
					)
					.await;
				record_submission(&mut pending, &mut last_segment, submitted);
			},
			Work::Hedge(Hedge { core_index, headers, proof, resubmitted, fresh }) => {
				// Store rows are written at import unless the session lookup failed; a block
				// without one is skipped here, though the main submission carried it.
				let entries = super::unincluded_segment::hydrate_segment(
					headers,
					&*para_backend,
					&code_hash_provider,
					&resubmission_store,
				);
				submit_segment(
					entries,
					proof,
					core_index,
					true,
					resubmitted,
					fresh,
					&collator_service,
					&mut overseer_handle,
					&relay_client,
					export_pov.clone(),
				)
				.await;
			},
		}
	}
}

/// Build one segment's collations under `scheduling_proof` and submit them for `core_index`.
///
/// Entries that fail to build or whose session lookup fails are skipped — they do not abort the
/// whole segment. `resubmitted` and `fresh` only feed the logs.
async fn submit_segment<Block, RClient, CS>(
	entries: Vec<CollatorSegmentEntry<Block>>,
	scheduling_proof: SchedulingProof,
	core_index: CoreIndex,
	hedged: bool,
	resubmitted: usize,
	fresh: usize,
	collator_service: &CS,
	overseer_handle: &mut OverseerHandle,
	relay_client: &RClient,
	export_pov: Option<PathBuf>,
) where
	Block: BlockT,
	RClient: RelayChainInterface + Clone + 'static,
	CS: CollatorServiceInterface<Block>,
{
	// Logged on every submission so a rejected one can be traced to its scheduling parent.
	let scheduling_parent = scheduling_proof.scheduling_parent();
	let total_entries = resubmitted + fresh;

	let mut collations = Vec::with_capacity(total_entries);
	for entry in entries {
		if let Some(collation) = build_collation(
			entry,
			Some(scheduling_proof.clone()),
			collator_service,
			relay_client,
			export_pov.clone(),
		)
		.await
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
		return;
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
		dropped = total_entries.saturating_sub(collations.len()),
		"Submitting segment for core.",
	);

	overseer_handle
		.send_msg(
			CollationGenerationMessage::SubmitSegment(SubmitSegmentParams {
				scheduling_parent,
				core_index,
				candidates_descriptor_version: CandidateDescriptorVersion::V3,
				collations: sp_runtime::BoundedVec::truncate_from(collations),
			}),
			"SubmitSegment",
		)
		.await;
}

impl<Block: BlockT> CollatorMessage<Block> {
	/// Build the collation(s) carried by this message and forward them to the collation-generation
	/// subsystem via [`CollationGenerationMessage::SubmitSegment`]: a single collation becomes a
	/// one-element V2 segment, a segment is submitted as V3.
	///
	/// Returns `Some` only for a V3 segment that got submitted, so a following `Hedges` message can
	/// rebuild it. `Hedges` itself is handled by the caller, not here.
	async fn handle<RClient, Backend, CHP>(
		self,
		collator_service: &impl CollatorServiceInterface<Block>,
		overseer_handle: &mut OverseerHandle,
		relay_client: RClient,
		export_pov: Option<PathBuf>,
		para_backend: &Backend,
		code_hash_provider: &CHP,
		resubmission_store: &ResubmissionStore<Block, Backend>,
	) -> Option<SubmittedSegment<Block>>
	where
		RClient: RelayChainInterface + Clone + 'static,
		Backend: sc_client_api::Backend<Block>,
		CHP: ValidationCodeHashProvider<Block::Hash>,
	{
		match self {
			CollatorMessage::Collation { core_index, entry } => {
				// Single collations are submitted as a one-element V2 segment: no scheduling
				// proof, the scheduling parent is the collation's relay parent.
				let Some(segment_collation) =
					build_collation(entry, None, collator_service, &relay_client, export_pov).await
				else {
					return None;
				};
				let scheduling_parent = segment_collation.relay_parent;

				tracing::debug!(target: LOG_TARGET, ?core_index, ?scheduling_parent, "Submitting collation for core.");

				overseer_handle
					.send_msg(
						CollationGenerationMessage::SubmitSegment(SubmitSegmentParams {
							scheduling_parent,
							core_index,
							candidates_descriptor_version: CandidateDescriptorVersion::V2,
							collations: sp_runtime::BoundedVec::truncate_from(vec![
								segment_collation,
							]),
						}),
						"SubmitSegment",
					)
					.await;

				None
			},
			CollatorMessage::Segment(CollatorSegmentMessage {
				scheduling_proof,
				core_index,
				unincluded_headers,
				bundle,
			}) => {
				// Hydrated here (proof/body reads), off the block-production hot path, and
				// prepended oldest first ahead of the fresh bundle.
				let requested = unincluded_headers.len();
				let mut all_entries = super::unincluded_segment::hydrate_segment(
					unincluded_headers,
					para_backend,
					code_hash_provider,
					resubmission_store,
				);
				let resubmitted = all_entries.len();
				all_entries.extend(bundle);
				let fresh = all_entries.len() - resubmitted;

				// Nothing hydrated, so every proof below would report the same failure.
				if all_entries.is_empty() {
					tracing::debug!(
						target: LOG_TARGET,
						?core_index,
						requested,
						"Segment hydrated empty; nothing submitted for core.",
					);
					return None;
				}

				// Hedges keep only the headers; the entries move into the main submission.
				let headers: Vec<_> = all_entries
					.iter()
					.flat_map(|e| e.blocks.iter().map(|b| b.header().clone()))
					.collect();

				submit_segment(
					all_entries,
					scheduling_proof.clone(),
					core_index,
					false,
					resubmitted,
					fresh,
					collator_service,
					overseer_handle,
					&relay_client,
					export_pov.clone(),
				)
				.await;

				Some(SubmittedSegment {
					core_index,
					headers,
					proof: scheduling_proof,
					resubmitted,
					fresh,
				})
			},
			CollatorMessage::Hedges { .. } => {
				unreachable!("`Hedges` is matched by the run loop before `handle`; qed")
			},
		}
	}
}

/// Mirror the PVF's `signed_scheduling_info` override on the collator side: strip the existing
/// scheduling tail from `upward_messages` (everything from the first `UMP_SEPARATOR` onwards)
/// and re-emit it from `signed_info.payload`. After this, collation-generation's
/// `parse_ump_signals` reads the same `SelectCore`/`ApprovedPeer` the PVF would compute.
fn override_ump_scheduling_tail(
	upward_messages: &mut UpwardMessages,
	signed_info: &SignedSchedulingInfo,
) {
	// Strip everything from the first `UMP_SEPARATOR` onwards (the existing scheduling tail).
	if let Some(pos) = upward_messages.iter().position(|m| m == &UMP_SEPARATOR) {
		for bytes in upward_messages.iter().skip(pos + 1) {
			// NOTE: intentionally exhaustive (no `_` arm), mirroring
			// `SchedulingSignals::from_block_signals`: a new `UMPSignal` variant must fail to
			// compile here, because the truncate below would silently drop it.
			match UMPSignal::decode(&mut &bytes[..]).expect("Failed to decode `UMPSignal`") {
				UMPSignal::SelectCore(..) | UMPSignal::ApprovedPeer(..) => {},
			}
		}
		upward_messages.truncate(pos);
	}

	// Re-emit the tail using the signed info's selector/offset/peer.
	let _ = upward_messages.try_push(UMP_SEPARATOR);
	let _ = upward_messages.try_push(
		UMPSignal::SelectCore(
			signed_info.payload.core_selector,
			ClaimQueueOffset(signed_info.payload.claim_queue_offset),
		)
		.encode(),
	);
	let _ = upward_messages
		.try_push(UMPSignal::ApprovedPeer(signed_info.payload.peer_id.clone()).encode());
}

/// Build one collation from an entry: build the PoV, export it if configured, and look up the
/// session index. Returns `None` if the collation could not be built or the session lookup failed.
async fn build_collation<Block: BlockT, RClient: RelayChainInterface + Clone + 'static>(
	entry: CollatorSegmentEntry<Block>,
	scheduling_proof: Option<SchedulingProof>,
	collator_service: &impl CollatorServiceInterface<Block>,
	relay_client: &RClient,
	export_pov: Option<PathBuf>,
) -> Option<SegmentCollation> {
	let CollatorSegmentEntry {
		relay_parent,
		parent_header,
		blocks,
		proof,
		validation_code_hash,
		validation_data,
	} = entry;

	// Capture the signed scheduling info before `build_multi_block_collation` consumes the proof;
	// used to pre-apply the PVF's UMP-signal override below.
	let scheduling_signals_override =
		scheduling_proof.as_ref().and_then(|p| p.signed_scheduling_info.clone());

	let (mut collation, block_data) = match collator_service.build_multi_block_collation(
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

	// Pre-apply the PVF's UMP-signal override locally. The PVF replaces the block's emitted
	// `SelectCore`/`ApprovedPeer` signals wholesale with the ones in `signed_scheduling_info`
	// (see `cumulus_pallet_parachain_system::validate_block::implementation`). Doing the same
	// rewrite on `collation.upward_messages` here lets collation-generation's `parse_ump_signals`
	// see the post-override signals, so a historical entry whose body committed to a different
	// selector than the segment's `core_index` won't trip `CoreIndexMismatch` at the collator-side
	// pre-check, and the committed commitments match what the PVF produces.
	if let Some(signed_info) = scheduling_signals_override.as_ref() {
		override_ump_scheduling_tail(&mut collation.upward_messages, signed_info);
	}

	block_data.log_size_info();

	if let MaybeCompressedPoV::Compressed(ref pov) = collation.proof_of_validity {
		if let Some(pov_path) = export_pov {
			if let Ok(Some(relay_parent_header)) =
				relay_client.header(BlockId::Hash(relay_parent)).await
			{
				if let Some(header) = block_data.blocks().first().map(|b| b.header()) {
					export_pov_to_path::<Block>(
						pov_path,
						pov.clone(),
						header.hash(),
						*header.number(),
						parent_header.clone(),
						relay_parent_header.state_root,
						relay_parent_header.number,
						validation_data.max_pov_size,
					);
				}
			} else {
				tracing::error!(target: LOG_TARGET, "Failed to get relay parent header from hash: {relay_parent:?}");
			}
		}

		tracing::info!(
			target: LOG_TARGET,
			block_numbers = ?block_data.blocks().iter().map(|b| *b.header().number()).collect::<Vec<_>>(),
			"Compressed PoV size: {}kb",
			pov.block_data.0.len() as f64 / 1024f64,
		);
	}

	let session_index = match relay_client.session_index_for_child(relay_parent).await {
		Ok(session_index) => session_index,
		Err(err) => {
			tracing::error!(
				target: LOG_TARGET,
				?err,
				?relay_parent,
				"Failed to fetch session index."
			);
			return None;
		},
	};

	Some(SegmentCollation {
		relay_parent,
		collation,
		validation_code_hash,
		session_index,
		validation_data,
	})
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::SchedulingInfoPayload;
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

	fn hedge(core_index: CoreIndex, proof: u32) -> Hedge<Block> {
		Hedge {
			core_index,
			headers: vec![header(100)],
			proof: scheduling_proof(proof),
			resubmitted: 0,
			fresh: 1,
		}
	}

	fn segment_message(core_index: CoreIndex) -> CollatorMessage<Block> {
		CollatorMessage::Segment(CollatorSegmentMessage {
			scheduling_proof: scheduling_proof(0),
			core_index,
			unincluded_headers: vec![],
			bundle: None,
		})
	}

	/// A ready message always wins over a queued hedge; once drained, hedges are served oldest
	/// first until the queue is drained.
	#[tokio::test]
	async fn hedged_rebuilds_yield_to_new_messages() {
		let mut pending = VecDeque::from([hedge(CoreIndex(0), 1), hedge(CoreIndex(0), 2)]);

		let (tx, mut rx) = sc_utils::mpsc::tracing_unbounded("test", 16);
		tx.unbounded_send(segment_message(CoreIndex(1)))
			.expect("receiver is alive; qed");
		drop(tx);

		// The waiting message wins first.
		match next_work(&mut rx, &mut pending).await.expect("message is queued; qed") {
			Work::Message(CollatorMessage::Segment(segment)) => {
				assert_eq!(segment.core_index, CoreIndex(1))
			},
			_ => panic!("expected the queued message"),
		}

		for expected in [1, 2] {
			match next_work(&mut rx, &mut pending).await.expect("hedge queued; qed") {
				Work::Hedge(served) => {
					assert_eq!(served.proof, scheduling_proof(expected));
					assert_eq!(served.headers, vec![header(100)]);
				},
				_ => panic!("expected a hedge"),
			}
		}

		// Receiver closed and queue drained: nothing left.
		assert!(next_work(&mut rx, &mut pending).await.is_none());
	}

	/// With the channel open, an idle receiver lets a queued hedge through, and a message sent
	/// meanwhile still wins over the remaining hedges.
	#[tokio::test]
	async fn open_idle_channel_serves_hedges() {
		let mut pending = VecDeque::from([hedge(CoreIndex(0), 1), hedge(CoreIndex(0), 2)]);
		let (tx, mut rx) = sc_utils::mpsc::tracing_unbounded("test", 16);

		let work = tokio::time::timeout(Duration::from_secs(5), next_work(&mut rx, &mut pending))
			.await
			.expect("an idle open channel must not block a queued hedge");
		assert!(matches!(work, Some(Work::Hedge(Hedge { core_index: CoreIndex(0), .. }))));

		tx.unbounded_send(segment_message(CoreIndex(1)))
			.expect("receiver is alive; qed");
		match next_work(&mut rx, &mut pending).await {
			Some(Work::Message(CollatorMessage::Segment(segment))) => {
				assert_eq!(segment.core_index, CoreIndex(1))
			},
			_ => panic!("expected the message ahead of the last hedge"),
		}
		assert!(matches!(next_work(&mut rx, &mut pending).await, Some(Work::Hedge(_))));
		drop(tx);
	}

	/// A `Hedges` message rebuilds the last segment once per chain, keeping the chosen proof's
	/// other fields; without a segment on the same core it yields nothing.
	#[test]
	fn follow_up_hedges_rebuild_the_last_segment() {
		let last = || SubmittedSegment::<Block> {
			core_index: CoreIndex(0),
			headers: vec![header(100), header(101)],
			proof: scheduling_proof(7),
			resubmitted: 1,
			fresh: 1,
		};
		let chains = vec![vec![scheduling_proof(8).internal_scheduling_parent_header], vec![]];

		let hedges = hedges_for(Some(last()), CoreIndex(0), chains.clone());
		assert_eq!(hedges.len(), 2);
		for (hedge, chain) in hedges.iter().zip(&chains) {
			assert_eq!(&hedge.proof.header_chain, chain);
			assert_eq!(
				hedge.proof.internal_scheduling_parent_header,
				scheduling_proof(7).internal_scheduling_parent_header
			);
			assert_eq!(
				hedge.proof.signed_scheduling_info,
				scheduling_proof(7).signed_scheduling_info
			);
			assert_eq!(hedge.headers, vec![header(100), header(101)]);
			assert_eq!((hedge.core_index, hedge.resubmitted, hedge.fresh), (CoreIndex(0), 1, 1));
		}

		assert!(hedges_for(Some(last()), CoreIndex(1), chains.clone()).is_empty());
		assert!(hedges_for::<Block>(None, CoreIndex(0), chains).is_empty());
	}

	/// Drives the run loop's bookkeeping: hedges follow their own segment and are superseded by
	/// the next one; a message in between orphans them.
	#[test]
	fn run_loop_bookkeeping() {
		let segment = |core| SubmittedSegment::<Block> {
			core_index: core,
			headers: vec![header(100)],
			proof: scheduling_proof(7),
			resubmitted: 0,
			fresh: 1,
		};
		let (mut pending, mut last) = (VecDeque::new(), None);

		record_submission(&mut pending, &mut last, Some(segment(CoreIndex(0))));
		pending.extend(hedges_for(last.take(), CoreIndex(0), vec![vec![], vec![]]));
		assert_eq!(pending.len(), 2);
		// Another core's segment leaves core 0's hedges queued.
		record_submission(&mut pending, &mut last, Some(segment(CoreIndex(1))));
		assert_eq!(pending.iter().filter(|h| h.core_index == CoreIndex(0)).count(), 2);
		record_submission(&mut pending, &mut last, Some(segment(CoreIndex(0))));
		assert!(pending.is_empty());

		record_submission(&mut pending, &mut last, None);
		pending.extend(hedges_for(last.take(), CoreIndex(0), vec![vec![]]));
		assert!(pending.is_empty());
	}
}
