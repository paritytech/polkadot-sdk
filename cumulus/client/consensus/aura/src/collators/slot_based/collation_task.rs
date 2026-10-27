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
	message::{CollationParts, CollatorMessage},
	resubmittable_segment::ResubmittableSegment,
	scheduling::{SiblingParams, SiblingResolver},
};

const LOG_TARGET: &str = "aura::cumulus::collation_task";

/// A submitted segment, kept as headers only and re-hydrated when rebuilt under a sibling.
struct SegmentSubmission<Block: BlockT> {
	core_index: CoreIndex,
	headers: Vec<Block::Header>,
	proof: SchedulingProof,
	resubmitted: usize,
	fresh: usize,
}

impl<Block: BlockT> SegmentSubmission<Block> {
	/// `self` under each sibling's header chain; the signed scheduling info is kept, as it commits
	/// to the internal scheduling parent only.
	fn under(&self, header_chains: Vec<Vec<RelayHeader>>) -> Vec<Self> {
		header_chains
			.into_iter()
			.map(|header_chain| Self {
				core_index: self.core_index,
				headers: self.headers.clone(),
				proof: SchedulingProof { header_chain, ..self.proof.clone() },
				resubmitted: self.resubmitted,
				fresh: self.fresh,
			})
			.collect()
	}
}

/// Work deferred behind main submissions, dropped once its `valid_until` passes.
enum Deferred<Block: BlockT> {
	/// A submitted segment whose siblings are not looked up yet.
	Resolve(SegmentSubmission<Block>, SiblingParams),
	/// A submitted segment rebuilt under one sibling's header chain.
	Sibling(SegmentSubmission<Block>, Instant),
}

impl<Block: BlockT> Deferred<Block> {
	fn segment(&self) -> &SegmentSubmission<Block> {
		match self {
			Self::Resolve(segment, _) | Self::Sibling(segment, _) => segment,
		}
	}

	fn valid_until(&self) -> Instant {
		match self {
			Self::Resolve(_, params) => params.valid_until,
			Self::Sibling(_, valid_until) => *valid_until,
		}
	}
}

/// Queue sibling resolution for a submitted segment; a segment for `core_index`, sent or not,
/// supersedes that core's earlier sibling work.
fn defer_siblings<Block: BlockT>(
	deferred: &mut VecDeque<Deferred<Block>>,
	core_index: CoreIndex,
	submitted: Option<(SegmentSubmission<Block>, SiblingParams)>,
) {
	deferred.retain(|work| work.segment().core_index != core_index);
	deferred.extend(submitted.map(|(segment, siblings)| Deferred::Resolve(segment, siblings)));
}

/// Next unit of work: a ready message always wins over deferred work.
enum Work<Block: BlockT> {
	Message(CollatorMessage<Block>),
	Deferred(Deferred<Block>),
}

/// A ready message, else the oldest unexpired deferred work, which is never raced, so never
/// cancelled. Relay imports are recorded while idle. After `receiver` closes, unexpired deferred
/// work is served before `None`.
async fn next_work<Block: BlockT, RelayClient: RelayChainInterface + 'static>(
	receiver: &mut (impl FusedStream<Item = CollatorMessage<Block>> + Unpin),
	deferred: &mut VecDeque<Deferred<Block>>,
	siblings: &mut SiblingResolver<RelayClient>,
) -> Option<Work<Block>> {
	siblings.drain();
	let queued = deferred.len();
	let now = Instant::now();
	deferred.retain(|work| work.valid_until() > now);
	if deferred.len() < queued {
		tracing::debug!(
			target: LOG_TARGET,
			expired = queued - deferred.len(),
			"Dropping sibling work past its relay slot.",
		);
	}

	if deferred.is_empty() {
		loop {
			// `select!` skips a terminated stream, which would leave only the import branch.
			if receiver.is_terminated() {
				return None;
			}
			let absorb = siblings.absorb_next().fuse();
			futures::pin_mut!(absorb);
			futures::select_biased! {
				message = receiver.next() => return message.map(Work::Message),
				() = absorb => {},
			}
		}
	}

	futures::select_biased! {
		message = receiver.next() => if let Some(message) = message {
			return Some(Work::Message(message));
		},
		default => {},
		complete => {},
	}

	deferred.pop_front().map(Work::Deferred)
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

	// Sibling work deferred behind main submissions; an item starts only when no message is
	// waiting, so a message arriving meanwhile waits for that one item, at worst the slot's
	// sibling deadline plus one collation build.
	let mut deferred: VecDeque<Deferred<Block>> = VecDeque::new();
	let mut siblings = SiblingResolver::new(relay_client, para_id);
	siblings.ensure_subscribed().await;

	while let Some(work) = next_work(&mut collator_receiver, &mut deferred, &mut siblings).await {
		match work {
			Work::Message(CollatorMessage::Collation { core_index, parts }) => {
				submitter.submit_collation(core_index, parts).await;
			},
			Work::Message(CollatorMessage::Segment {
				scheduling_proof,
				core_index,
				resubmittable_headers,
				bundle,
				siblings: sibling_params,
			}) => {
				let submitted = submitter
					.submit_segment(
						scheduling_proof,
						core_index,
						resubmittable_headers,
						bundle,
						sibling_params,
					)
					.await;
				defer_siblings(&mut deferred, core_index, submitted);
			},
			Work::Deferred(Deferred::Resolve(segment, params)) => {
				let chains = siblings.resolve(&segment.proof, &params).await;
				// Ahead of other cores' work, keeping the core's siblings together.
				for sibling in segment.under(chains).into_iter().rev() {
					deferred.push_front(Deferred::Sibling(sibling, params.valid_until));
				}
			},
			Work::Deferred(Deferred::Sibling(segment, valid_until)) => {
				submitter.submit_to_sibling(segment, valid_until).await
			},
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

		tracing::debug!(
			target: LOG_TARGET,
			?core_index,
			?scheduling_parent,
			"Submitting collation for core.",
		);

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

	/// Submit a V3 segment; `Some` only if it sent something `siblings` may carry too.
	async fn submit_segment(
		&mut self,
		scheduling_proof: SchedulingProof,
		core_index: CoreIndex,
		resubmittable_headers: Vec<Block::Header>,
		bundle: Option<CollationParts<Block>>,
		siblings: Option<SiblingParams>,
	) -> Option<(SegmentSubmission<Block>, SiblingParams)> {
		// Hydrate the resubmitted headers here (proof/body reads), off the block-production hot
		// path, then prepend them (oldest first) to the freshly-built bundle.
		let mut all_parts = self.hydrate(resubmittable_headers);
		let resubmitted = all_parts.len();
		// `bundle` holds at most one freshly-built bundle.
		let fresh = usize::from(bundle.is_some());
		all_parts.extend(bundle);

		// Sibling rebuilds keep only the headers; the parts move into the main submission.
		let headers: Option<Vec<_>> = siblings.as_ref().map(|_| {
			all_parts
				.iter()
				.flat_map(|parts| parts.blocks.iter().map(|b| b.header().clone()))
				.collect()
		});

		let sent = self
			.send_segment(all_parts, scheduling_proof.clone(), core_index, None, resubmitted, fresh)
			.await;
		let (headers, siblings) = headers.zip(siblings).filter(|_| sent)?;

		let segment =
			SegmentSubmission { core_index, headers, proof: scheduling_proof, resubmitted, fresh };
		Some((segment, siblings))
	}

	/// Rebuild a submitted segment from its headers under a sibling's header chain. Skipped if any
	/// part fails to hydrate, rather than advertising less than the main submission did.
	async fn submit_to_sibling(&mut self, segment: SegmentSubmission<Block>, valid_until: Instant) {
		let SegmentSubmission { core_index, headers, proof, resubmitted, fresh } = segment;
		if Instant::now() >= valid_until {
			tracing::debug!(target: LOG_TARGET, ?core_index, "Sibling submission too late.");
			return;
		}
		let parts = self.hydrate(headers);
		if parts.len() != resubmitted + fresh {
			tracing::debug!(
				target: LOG_TARGET,
				?core_index,
				hydrated = parts.len(),
				expected = resubmitted + fresh,
				"Skipping sibling submission with missing parts.",
			);
			return;
		}
		self.send_segment(parts, proof, core_index, Some(valid_until), resubmitted, fresh)
			.await;
	}

	fn hydrate(&self, headers: Vec<Block::Header>) -> Vec<CollationParts<Block>> {
		ResubmittableSegment::new(headers).hydrate(
			&*self.para_backend,
			&self.code_hash_provider,
			&self.resubmission_store,
		)
	}

	/// Build `all_parts` under `scheduling_proof` and submit them for `core_index`; returns whether
	/// anything was sent. Parts that fail to build are skipped, not fatal to the segment, except in
	/// a sibling copy (`sibling_valid_until` set): sent whole before its deadline, or not at all.
	/// `resubmitted` and `fresh` only feed the logs.
	async fn send_segment(
		&mut self,
		all_parts: Vec<CollationParts<Block>>,
		scheduling_proof: SchedulingProof,
		core_index: CoreIndex,
		sibling_valid_until: Option<Instant>,
		resubmitted: usize,
		fresh: usize,
	) -> bool {
		// Logged on every submission so a rejected one can be traced to its scheduling parent.
		let scheduling_parent = scheduling_proof.scheduling_parent();
		let total = resubmitted + fresh;
		let sibling = sibling_valid_until.is_some();
		let expired =
			|| sibling_valid_until.is_some_and(|valid_until| Instant::now() >= valid_until);

		let mut collations = Vec::with_capacity(all_parts.len());
		for parts in all_parts {
			let built = if expired() {
				None
			} else {
				self.build_collation(parts, Some(scheduling_proof.clone())).await
			};
			match built {
				Some(collation) => collations.push(collation),
				None if sibling => {
					tracing::debug!(
						target: LOG_TARGET,
						?core_index,
						?scheduling_parent,
						expired = expired(),
						"Sibling copy incomplete; nothing submitted.",
					);
					return false;
				},
				None => {},
			}
		}
		if expired() {
			tracing::debug!(target: LOG_TARGET, ?core_index, "Sibling copy built too late.");
			return false;
		}

		if collations.is_empty() {
			tracing::debug!(
				target: LOG_TARGET,
				?core_index,
				?scheduling_parent,
				sibling,
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
				sibling,
				segment_len = collations.len(),
				max = MAX_SEGMENT_LEN,
				"Segment exceeds MAX_SEGMENT_LEN; truncating.",
			);
		}

		tracing::debug!(
			target: LOG_TARGET,
			?core_index,
			?scheduling_parent,
			sibling,
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
	use crate::collators::slot_based::tests::TestRelayClient;
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
		}
	}

	fn queued(proof: u32, expired: bool) -> Deferred<Block> {
		let segment = segment(CoreIndex(0), 0, 1);
		let valid_until = Instant::now() + Duration::from_secs(if expired { 0 } else { 60 });
		Deferred::Sibling(
			SegmentSubmission { proof: scheduling_proof(proof), ..segment },
			valid_until,
		)
	}

	fn segment_message(core_index: CoreIndex) -> CollatorMessage<Block> {
		CollatorMessage::Segment {
			scheduling_proof: scheduling_proof(0),
			core_index,
			resubmittable_headers: vec![],
			bundle: None,
			siblings: None,
		}
	}

	/// The next work as `("segment", core)` or `("sibling", proof)`; never blocks on deferred work.
	async fn serve(
		rx: &mut TracingUnboundedReceiver<CollatorMessage<Block>>,
		deferred: &mut VecDeque<Deferred<Block>>,
	) -> Option<(&'static str, u32)> {
		// Never subscribed, so it only ever waits.
		let mut siblings = SiblingResolver::new(TestRelayClient::new(Default::default()), 1.into());
		let work =
			tokio::time::timeout(Duration::from_secs(5), next_work(rx, deferred, &mut siblings))
				.await
				.expect("next_work never blocks here; qed");
		match work? {
			Work::Message(CollatorMessage::Segment { core_index, .. }) => {
				Some(("segment", core_index.0))
			},
			Work::Deferred(work) => {
				Some(("sibling", work.segment().proof.internal_scheduling_parent_header.number))
			},
			Work::Message(_) => panic!("only segments are sent"),
		}
	}

	/// A ready message always wins; an idle open channel lets the oldest live work through, expired
	/// work is never served, and after close the remaining work drains before `None`.
	#[tokio::test]
	async fn next_work_serves_messages_then_live_sibling_work() {
		let mut deferred = VecDeque::from([queued(1, true), queued(2, false), queued(3, false)]);
		let (tx, mut rx) = sc_utils::mpsc::tracing_unbounded("test", 16);

		tx.unbounded_send(segment_message(CoreIndex(1)))
			.expect("receiver is alive; qed");
		assert_eq!(serve(&mut rx, &mut deferred).await, Some(("segment", 1)));
		assert_eq!(serve(&mut rx, &mut deferred).await, Some(("sibling", 2)));
		tx.unbounded_send(segment_message(CoreIndex(2)))
			.expect("receiver is alive; qed");
		assert_eq!(serve(&mut rx, &mut deferred).await, Some(("segment", 2)));
		drop(tx);
		assert_eq!(serve(&mut rx, &mut deferred).await, Some(("sibling", 3)));
		assert_eq!(serve(&mut rx, &mut deferred).await, None);
	}

	/// A segment under each sibling chain keeps everything but the chain.
	#[test]
	fn segment_under_sibling_chains() {
		let chains = vec![vec![scheduling_proof(8).internal_scheduling_parent_header], vec![]];
		let segment = segment(CoreIndex(0), 1, 1);
		let siblings = segment.under(chains.clone());
		assert_eq!(siblings.len(), 2);
		for (sibling, chain) in siblings.iter().zip(&chains) {
			let proof = SchedulingProof { header_chain: chain.clone(), ..scheduling_proof(7) };
			assert_eq!(sibling.proof, proof);
			assert_eq!(sibling.headers, segment.headers);
			assert_eq!(
				(sibling.core_index, sibling.resubmitted, sibling.fresh),
				(CoreIndex(0), 1, 1)
			);
		}
	}

	/// A core's next segment, sent or not, drops its earlier sibling work; other cores keep theirs.
	#[test]
	fn next_segment_supersedes_sibling_work() {
		let params = SiblingParams {
			production_slot: Default::default(),
			core_indices: vec![],
			claim_queue_offset: 0,
			max_relay_parent_session_age: 0,
			valid_until: Instant::now(),
		};
		let submitted = |core| Some((segment(core, 0, 1), params.clone()));
		let mut deferred = VecDeque::new();
		defer_siblings(&mut deferred, CoreIndex(0), submitted(CoreIndex(0)));
		deferred.push_back(Deferred::Sibling(segment(CoreIndex(0), 0, 1), Instant::now()));
		defer_siblings(&mut deferred, CoreIndex(1), submitted(CoreIndex(1)));
		assert_eq!(deferred.len(), 3);

		defer_siblings(&mut deferred, CoreIndex(0), None);
		let cores: Vec<_> = deferred.iter().map(|work| work.segment().core_index).collect();
		assert_eq!(cores, vec![CoreIndex(1)]);
	}

	/// Relay imports arriving while `next_work` waits are recorded for later resolution.
	#[tokio::test]
	async fn next_work_records_imports_while_idle() {
		let (import_tx, import_rx) = futures::channel::mpsc::unbounded();
		let mut siblings = SiblingResolver::with_imports(
			TestRelayClient::new(Default::default()),
			1.into(),
			Box::pin(import_rx),
		);
		let (tx, mut rx) = sc_utils::mpsc::tracing_unbounded("test", 16);
		let mut deferred = VecDeque::new();

		let work = {
			let work = next_work(&mut rx, &mut deferred, &mut siblings);
			futures::pin_mut!(work);
			assert!(futures::poll!(work.as_mut()).is_pending());
			let import = crate::collators::slot_based::tests::relay_header_with_slot(
				1,
				Default::default(),
				1,
			);
			import_tx.unbounded_send(import).expect("receiver is alive; qed");
			// Absorbs the import and keeps waiting for a message.
			assert!(futures::poll!(work.as_mut()).is_pending());
			tx.unbounded_send(segment_message(CoreIndex(0)))
				.expect("receiver is alive; qed");
			work.await
		};
		assert!(matches!(work, Some(Work::Message(CollatorMessage::Segment { .. }))));
		assert_eq!(siblings.recorded(), 1);
	}
}
