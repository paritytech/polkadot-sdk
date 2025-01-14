// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! The collation generation subsystem is the interface between polkadot and the collators.
//!
//! # Protocol
//!
//! On every `ActiveLeavesUpdate`:
//!
//! * If there is no collation generation config, ignore.
//! * Otherwise, for each `activated` head in the update:
//!   * Determine if the para is scheduled on any core by fetching the `availability_cores` Runtime
//!     API.
//!   * Use the Runtime API subsystem to fetch the full validation data.
//!   * Invoke the `collator`, and use its outputs to produce a [`CandidateReceipt`], signed with
//!     the configuration's `key`.
//!   * Dispatch a [`CollatorProtocolMessage::DistributeCollation`]`(receipt, pov)`.

#![deny(missing_docs)]

use codec::Encode;
use error::{Error, Result};
use futures::{channel::oneshot, future::FutureExt, select};
use polkadot_node_primitives::{
	AvailableData, Collation, CollationGenerationConfig, CollationSecondedSignal, PoV,
	SubmitCollationParams,
};
use polkadot_node_subsystem::{
	messages::{CollationGenerationMessage, CollatorProtocolMessage, RuntimeApiMessage},
	overseer, ActiveLeavesUpdate, FromOrchestra, OverseerSignal, SpawnedSubsystem,
	SubsystemContext, SubsystemError, SubsystemResult, SubsystemSender,
};
use polkadot_node_subsystem_util::{
	request_claim_queue, request_persisted_validation_data, request_session_index_for_child,
	request_validation_code_hash, request_validators,
	runtime::{request_node_features, ClaimQueueSnapshot},
};
use polkadot_primitives::{
	collator_signature_payload,
	node_features::FeatureIndex,
	vstaging::{
		transpose_claim_queue, CandidateDescriptorV2, CandidateReceiptV2 as CandidateReceipt,
		ClaimQueueOffset, CommittedCandidateReceiptV2, TransposedClaimQueue,
	},
	CandidateCommitments, CandidateDescriptor, CollatorPair, CoreIndex, Hash, Id as ParaId,
	NodeFeatures, OccupiedCoreAssumption, PersistedValidationData, SessionIndex,
	ValidationCodeHash,
};
use schnellru::{ByLength, LruMap};
use sp_core::crypto::Pair;
use std::sync::Arc;

mod error;

#[cfg(test)]
mod tests;

mod metrics;
use self::metrics::Metrics;

const LOG_TARGET: &'static str = "parachain::collation-generation";

/// Collation Generation Subsystem
pub struct CollationGenerationSubsystem {
	config: Option<Arc<CollationGenerationConfig>>,
	session_info_cache: SessionInfoCache,
	metrics: Metrics,
}

#[overseer::contextbounds(CollationGeneration, prefix = self::overseer)]
impl CollationGenerationSubsystem {
	/// Create a new instance of the `CollationGenerationSubsystem`.
	pub fn new(metrics: Metrics) -> Self {
		Self { config: None, metrics, session_info_cache: SessionInfoCache::new() }
	}

	/// Run this subsystem
	///
	/// Conceptually, this is very simple: it just loops forever.
	///
	/// - On incoming overseer messages, it starts or stops jobs as appropriate.
	/// - On other incoming messages, if they can be converted into `Job::ToJob` and include a hash,
	///   then they're forwarded to the appropriate individual job.
	/// - On outgoing messages from the jobs, it forwards them to the overseer.
	///
	/// If `err_tx` is not `None`, errors are forwarded onto that channel as they occur.
	/// Otherwise, most are logged and then discarded.
	async fn run<Context>(mut self, mut ctx: Context) {
		loop {
			select! {
				incoming = ctx.recv().fuse() => {
					if self.handle_incoming::<Context>(incoming, &mut ctx).await {
						break;
					}
				},
			}
		}
	}

	// handle an incoming message. return true if we should break afterwards.
	// note: this doesn't strictly need to be a separate function; it's more an administrative
	// function so that we don't clutter the run loop. It could in principle be inlined directly
	// into there. it should hopefully therefore be ok that it's an async function mutably borrowing
	// self.
	async fn handle_incoming<Context>(
		&mut self,
		incoming: SubsystemResult<FromOrchestra<<Context as SubsystemContext>::Message>>,
		ctx: &mut Context,
	) -> bool {
		match incoming {
			Ok(FromOrchestra::Signal(OverseerSignal::ActiveLeaves(ActiveLeavesUpdate {
				activated,
				..
			}))) => {
				if let Err(err) = self.handle_new_activation(activated.map(|v| v.hash), ctx).await {
					gum::warn!(target: LOG_TARGET, err = ?err, "failed to handle new activation");
				}

				false
			},
			Ok(FromOrchestra::Signal(OverseerSignal::Conclude)) => true,
			Ok(FromOrchestra::Communication {
				msg: CollationGenerationMessage::Initialize(config),
			}) => {
				if self.config.is_some() {
					gum::error!(target: LOG_TARGET, "double initialization");
				} else {
					self.config = Some(Arc::new(config));
				}
				false
			},
			Ok(FromOrchestra::Communication {
				msg: CollationGenerationMessage::Reinitialize(config),
			}) => {
				self.config = Some(Arc::new(config));
				false
			},
			Ok(FromOrchestra::Communication {
				msg: CollationGenerationMessage::SubmitCollation(params),
			}) => {
				if let Err(err) = self.handle_submit_collation(params, ctx).await {
					gum::error!(target: LOG_TARGET, ?err, "Failed to submit collation");
				}

				false
			},
			Ok(FromOrchestra::Signal(OverseerSignal::BlockFinalized(..))) => false,
			Err(err) => {
				gum::error!(
					target: LOG_TARGET,
					err = ?err,
					"error receiving message from subsystem context: {:?}",
					err
				);
				true
			},
		}
	}

	async fn handle_submit_collation<Context>(
		&mut self,
		params: SubmitCollationParams,
		ctx: &mut Context,
	) -> Result<()> {
		let Some(config) = &self.config else {
			return Err(Error::SubmittedBeforeInit);
		};
		let _timer = self.metrics.time_submit_collation();

		let SubmitCollationParams {
			relay_parent,
			collation,
			parent_head,
			validation_code_hash,
			result_sender,
			core_index,
		} = params;

		let mut validation_data = match request_persisted_validation_data(
			relay_parent,
			config.para_id,
			OccupiedCoreAssumption::TimedOut,
			ctx.sender(),
		)
		.await
		.await??
		{
			Some(v) => v,
			None => {
				gum::debug!(
					target: LOG_TARGET,
					relay_parent = ?relay_parent,
					our_para = %config.para_id,
					"No validation data for para - does it exist at this relay-parent?",
				);
				return Ok(())
			},
		};

		// We need to swap the parent-head data, but all other fields here will be correct.
		validation_data.parent_head = parent_head;

		let claim_queue = request_claim_queue(relay_parent, ctx.sender()).await.await??;

		let session_index =
			request_session_index_for_child(relay_parent, ctx.sender()).await.await??;

		let session_info =
			self.session_info_cache.get(relay_parent, session_index, ctx.sender()).await?;
		let collation = PreparedCollation {
			collation,
			relay_parent,
			para_id: config.para_id,
			validation_data,
			validation_code_hash,
			n_validators: session_info.n_validators,
			core_index,
			session_index,
		};

		construct_and_distribute_receipt(
			collation,
			config.key.clone(),
			ctx.sender(),
			result_sender,
			&mut self.metrics,
			session_info.v2_receipts,
			&transpose_claim_queue(claim_queue),
		)
		.await?;

		Ok(())
	}

	async fn handle_new_activation<Context>(
		&mut self,
		maybe_activated: Option<Hash>,
		ctx: &mut Context,
	) -> Result<()> {
		let Some(config) = &self.config else {
			return Ok(());
		};

		let Some(relay_parent) = maybe_activated else { return Ok(()) };

		// If there is no collation function provided, bail out early.
		// Important: Lookahead collator and slot based collator do not use `CollatorFn`.
		if config.collator.is_none() {
			return Ok(())
		}

		let para_id = config.para_id;

		let _timer = self.metrics.time_new_activation();

		let session_index =
			request_session_index_for_child(relay_parent, ctx.sender()).await.await??;

		let session_info =
			self.session_info_cache.get(relay_parent, session_index, ctx.sender()).await?;
		let n_validators = session_info.n_validators;

		let claim_queue =
			ClaimQueueSnapshot::from(request_claim_queue(relay_parent, ctx.sender()).await.await??);

		let assigned_cores = claim_queue
			.iter_all_claims()
			.filter_map(|(core_idx, para_ids)| {
				para_ids.iter().any(|&para_id| para_id == config.para_id).then_some(*core_idx)
			})
			.collect::<Vec<_>>();

		// Nothing to do if no core is assigned to us at any depth.
		if assigned_cores.is_empty() {
			return Ok(())
		}

		// We are being very optimistic here, but one of the cores could be pending availability
		// for some more blocks, or even time out. We assume all cores are being freed.

		let mut validation_data = match request_persisted_validation_data(
			relay_parent,
			para_id,
			// Just use included assumption always. If there are no pending candidates it's a
			// no-op.
			OccupiedCoreAssumption::Included,
			ctx.sender(),
		)
		.await
		.await??
		{
			Some(v) => v,
			None => {
				gum::debug!(
					target: LOG_TARGET,
					relay_parent = ?relay_parent,
					our_para = %para_id,
					"validation data is not available",
				);
				return Ok(())
			},
		};

		let validation_code_hash = match request_validation_code_hash(
			relay_parent,
			para_id,
			// Just use included assumption always. If there are no pending candidates it's a
			// no-op.
			OccupiedCoreAssumption::Included,
			ctx.sender(),
		)
		.await
		.await??
		{
			Some(v) => v,
			None => {
				gum::debug!(
					target: LOG_TARGET,
					relay_parent = ?relay_parent,
					our_para = %para_id,
					"validation code hash is not found.",
				);
				return Ok(())
			},
		};

		let task_config = config.clone();
		let metrics = self.metrics.clone();
		let mut task_sender = ctx.sender().clone();

		ctx.spawn(
			"chained-collation-builder",
			Box::pin(async move {
				// Build the first collation in advance to find the claim queue offset and retrieve
				// the list of cores to submit collations on.
				let collator_fn = match task_config.collator.as_ref() {
					Some(x) => x,
					None => return,
				};

				let (mut collation, mut result_sender) =
					match collator_fn(relay_parent, &validation_data).await {
						Some(collation) => collation.into_inner(),
						None => {
							gum::debug!(
								target: LOG_TARGET,
								?para_id,
								"collator returned no collation on collate",
							);
							return
						},
					};

				// Use the core_selector method from CandidateCommitments to extract CoreSelector
				// and ClaimQueueOffset.
				let mut commitments = CandidateCommitments::default();
				commitments.upward_messages = collation.upward_messages.clone();

				let (mut core_selector, init_cq_offset) = match commitments.core_selector() {
					Ok(Some((sel, off))) => (Some(sel), off),
					Ok(None) => (None, ClaimQueueOffset(0)),
					Err(err) => {
						gum::debug!(
							target: LOG_TARGET,
							?para_id,
							"error processing UMP signals: {}",
							err
						);
						return
					},
				};

				// Identify the cores to build collations on using the given claim queue offset.
				let cores_to_build_on = claim_queue
					.iter_claims_at_depth(init_cq_offset.0 as usize)
					.filter_map(|(core_idx, para_id)| {
						(para_id == task_config.para_id).then_some(core_idx)
					})
					.collect::<Vec<_>>();

				// Track used core selector indexes not to submit collations on the same core.
				let mut used_core_selector_indexes = vec![false; cores_to_build_on.len()];

				let transposed_claim_queue = transpose_claim_queue(claim_queue.0);

				for core_index in &cores_to_build_on {
					let descriptor_core_index = match core_selector {
						Some(ref core_selector) => {
							// Calculate the core selector index and ensure it hasn't been used.
							let core_selector_index =
								core_selector.0 as usize % cores_to_build_on.len();

							if used_core_selector_indexes[core_selector_index] {
								gum::debug!(
									target: LOG_TARGET,
									?para_id,
									"parachain repeatedly selected the same core index",
								);
								return
							}
							used_core_selector_indexes[core_selector_index] = true;

							let commitments_core_index = cores_to_build_on[core_selector_index];
							commitments_core_index
						},
						// Fallback to the sequential core_index if no valid CoreSelector is found.
						None => *core_index,
					};

					let parent_head = collation.head_data.clone();
					if let Err(err) = construct_and_distribute_receipt(
						PreparedCollation {
							collation,
							para_id,
							relay_parent,
							validation_data: validation_data.clone(),
							validation_code_hash,
							n_validators,
							core_index: descriptor_core_index,
							session_index,
						},
						task_config.key.clone(),
						&mut task_sender,
						result_sender,
						&metrics,
						session_info.v2_receipts,
						&transposed_claim_queue,
					)
					.await
					{
						gum::error!(
							target: LOG_TARGET,
							"Failed to construct and distribute collation: {}",
							err
						);
						return
					}

					// No need to build more collations if we have already built and submitted
					// cores_to_build_on.len() collations.
					if core_index == cores_to_build_on.last().unwrap() {
						return
					}

					// Chain the collations. All else stays the same as we build the chained
					// collation on same relay parent.
					validation_data.parent_head = parent_head;

					// Prepare the next collation.
					(collation, result_sender) =
						match collator_fn(relay_parent, &validation_data).await {
							Some(collation) => collation.into_inner(),
							None => {
								gum::debug!(
									target: LOG_TARGET,
									?para_id,
									"collator returned no collation on collate",
								);
								return
							},
						};

					// Determine the new core selector and claim queue offset values.
					commitments.upward_messages = collation.upward_messages.clone();
					let (new_core_selector, new_cq_offset) = match commitments.core_selector() {
						Ok(Some((sel, off))) => (Some(sel), off),
						Ok(None) => (None, ClaimQueueOffset(0)),
						Err(err) => {
							gum::debug!(
								target: LOG_TARGET,
								?para_id,
								"error processing UMP signals: {}",
								err
							);
							return
						},
					};

					// We generally assume that the claim queue offset will not change often,
					// but if it does at any point, it is fine to skip that relay chain block
					// and readjust the cores later.
					if new_cq_offset.0 != init_cq_offset.0 {
						gum::debug!(
							target: LOG_TARGET,
							?para_id,
							"claim queue offset changed between blocks (initial: {}, current: {})",
							init_cq_offset.0,
							new_cq_offset.0
						);
						return
					}

					// We either use core selectors to choose cores for building blocks or iterate
					// over all cores sequentially if no core selector is provided. A mixed approach
					// is not acceptable.
					if core_selector.is_none() && new_core_selector.is_some() {
						gum::debug!(
							target: LOG_TARGET,
							?para_id,
							"previous block(s) had no core selector, but the current block has one",
						);
						return
					} else if core_selector.is_some() && new_core_selector.is_none() {
						gum::debug!(
							target: LOG_TARGET,
							?para_id,
							"previous block(s) had a core selector, but the current block does not",
						);
						return
					} else {
						core_selector = new_core_selector;
					}
				}
			}),
		)?;

		Ok(())
	}
}

#[overseer::subsystem(CollationGeneration, error=SubsystemError, prefix=self::overseer)]
impl<Context> CollationGenerationSubsystem {
	fn start(self, ctx: Context) -> SpawnedSubsystem {
		let future = async move {
			self.run(ctx).await;
			Ok(())
		}
		.boxed();

		SpawnedSubsystem { name: "collation-generation-subsystem", future }
	}
}

#[derive(Clone)]
struct PerSessionInfo {
	v2_receipts: bool,
	n_validators: usize,
}

struct SessionInfoCache(LruMap<SessionIndex, PerSessionInfo>);

impl SessionInfoCache {
	fn new() -> Self {
		Self(LruMap::new(ByLength::new(2)))
	}

	async fn get<Sender: SubsystemSender<RuntimeApiMessage>>(
		&mut self,
		relay_parent: Hash,
		session_index: SessionIndex,
		sender: &mut Sender,
	) -> Result<PerSessionInfo> {
		if let Some(info) = self.0.get(&session_index) {
			return Ok(info.clone())
		}

		let n_validators =
			request_validators(relay_parent, &mut sender.clone()).await.await??.len();

		let node_features = request_node_features(relay_parent, session_index, sender)
			.await?
			.unwrap_or(NodeFeatures::EMPTY);

		let info = PerSessionInfo {
			v2_receipts: node_features
				.get(FeatureIndex::CandidateReceiptV2 as usize)
				.map(|b| *b)
				.unwrap_or(false),
			n_validators,
		};
		self.0.insert(session_index, info);
		Ok(self.0.get(&session_index).expect("Just inserted").clone())
	}
}

struct PreparedCollation {
	collation: Collation,
	para_id: ParaId,
	relay_parent: Hash,
	validation_data: PersistedValidationData,
	validation_code_hash: ValidationCodeHash,
	n_validators: usize,
	core_index: CoreIndex,
	session_index: SessionIndex,
}

/// Takes a prepared collation, along with its context, and produces a candidate receipt
/// which is distributed to validators.
async fn construct_and_distribute_receipt(
	collation: PreparedCollation,
	key: CollatorPair,
	sender: &mut impl overseer::CollationGenerationSenderTrait,
	result_sender: Option<oneshot::Sender<CollationSecondedSignal>>,
	metrics: &Metrics,
	v2_receipts: bool,
	transposed_claim_queue: &TransposedClaimQueue,
) -> Result<()> {
	let PreparedCollation {
		collation,
		para_id,
		relay_parent,
		validation_data,
		validation_code_hash,
		n_validators,
		core_index,
		session_index,
	} = collation;

	let persisted_validation_data_hash = validation_data.hash();
	let parent_head_data = validation_data.parent_head.clone();
	let parent_head_data_hash = validation_data.parent_head.hash();

	// Apply compression to the block data.
	let pov = {
		let pov = collation.proof_of_validity.into_compressed();
		let encoded_size = pov.encoded_size();

		// As long as `POV_BOMB_LIMIT` is at least `max_pov_size`, this ensures
		// that honest collators never produce a PoV which is uncompressed.
		//
		// As such, honest collators never produce an uncompressed PoV which starts with
		// a compression magic number, which would lead validators to reject the collation.
		if encoded_size > validation_data.max_pov_size as usize {
			return Err(Error::POVSizeExceeded(encoded_size, validation_data.max_pov_size as usize))
		}

		pov
	};

	let pov_hash = pov.hash();

	let signature_payload = collator_signature_payload(
		&relay_parent,
		&para_id,
		&persisted_validation_data_hash,
		&pov_hash,
		&validation_code_hash,
	);

	let erasure_root = erasure_root(n_validators, validation_data, pov.clone())?;

	let commitments = CandidateCommitments {
		upward_messages: collation.upward_messages,
		horizontal_messages: collation.horizontal_messages,
		new_validation_code: collation.new_validation_code,
		head_data: collation.head_data,
		processed_downward_messages: collation.processed_downward_messages,
		hrmp_watermark: collation.hrmp_watermark,
	};

	let receipt = if v2_receipts {
		let ccr = CommittedCandidateReceiptV2 {
			descriptor: CandidateDescriptorV2::new(
				para_id,
				relay_parent,
				core_index,
				session_index,
				persisted_validation_data_hash,
				pov_hash,
				erasure_root,
				commitments.head_data.hash(),
				validation_code_hash,
			),
			commitments,
		};

		ccr.check_core_index(&transposed_claim_queue)
			.map_err(Error::CandidateReceiptCheck)?;

		ccr.to_plain()
	} else {
		if commitments.core_selector().map_err(Error::CandidateReceiptCheck)?.is_some() {
			gum::warn!(
				target: LOG_TARGET,
				?pov_hash,
				?relay_parent,
				para_id = %para_id,
				"Candidate commitments contain UMP signal without v2 receipts being enabled.",
			);
		}
		CandidateReceipt {
			commitments_hash: commitments.hash(),
			descriptor: CandidateDescriptor {
				signature: key.sign(&signature_payload),
				para_id,
				relay_parent,
				collator: key.public(),
				persisted_validation_data_hash,
				pov_hash,
				erasure_root,
				para_head: commitments.head_data.hash(),
				validation_code_hash,
			}
			.into(),
		}
	};

	gum::debug!(
		target: LOG_TARGET,
		candidate_hash = ?receipt.hash(),
		?pov_hash,
		?relay_parent,
		para_id = %para_id,
		?core_index,
		"candidate is generated",
	);
	metrics.on_collation_generated();

	sender
		.send_message(CollatorProtocolMessage::DistributeCollation {
			candidate_receipt: receipt,
			parent_head_data_hash,
			pov,
			parent_head_data,
			result_sender,
			core_index,
		})
		.await;

	Ok(())
}

fn erasure_root(
	n_validators: usize,
	persisted_validation: PersistedValidationData,
	pov: PoV,
) -> Result<Hash> {
	let available_data =
		AvailableData { validation_data: persisted_validation, pov: Arc::new(pov) };

	let chunks = polkadot_erasure_coding::obtain_chunks_v1(n_validators, &available_data)?;
	Ok(polkadot_erasure_coding::branches(&chunks).root())
}
