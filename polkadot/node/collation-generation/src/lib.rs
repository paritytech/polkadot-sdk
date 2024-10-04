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
use futures::{channel::oneshot, future::FutureExt, join, select};
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
	request_async_backing_params, request_availability_cores, request_para_backing_state,
	request_persisted_validation_data, request_session_index_for_child,
	request_validation_code_hash, request_validators,
	runtime::{fetch_claim_queue, request_node_features},
};
use polkadot_primitives::{
	collator_signature_payload,
	node_features::FeatureIndex,
	vstaging::{
		transpose_claim_queue, CandidateDescriptorV2, CandidateReceiptV2 as CandidateReceipt,
		CommittedCandidateReceiptV2, CoreState, TransposedClaimQueue,
	},
	AsyncBackingParams, CandidateCommitments, CandidateDescriptor, CollatorPair, CoreIndex, Hash,
	Id as ParaId, NodeFeatures, OccupiedCoreAssumption, PersistedValidationData, ScheduledCore,
	SessionIndex, ValidationCodeHash,
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
	session_info: SessionInfoCache,
	metrics: Metrics,
}

#[overseer::contextbounds(CollationGeneration, prefix = self::overseer)]
impl CollationGenerationSubsystem {
	/// Create a new instance of the `CollationGenerationSubsystem`.
	pub fn new(metrics: Metrics) -> Self {
		Self { config: None, metrics, session_info: SessionInfoCache::new() }
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
				if let Err(err) =
					self.handle_new_activation(activated.into_iter().map(|v| v.hash), ctx).await
				{
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

		// We need to swap the parent-head data, but all other fields here will be correct.
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

		validation_data.parent_head = parent_head;

		let session_index =
			request_session_index_for_child(relay_parent, ctx.sender()).await.await??;

		let session_info = self.session_info.get(relay_parent, session_index, ctx.sender()).await?;
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
		)
		.await?;

		Ok(())
	}

	async fn handle_new_activation<Context>(
		&mut self,
		activated: impl IntoIterator<Item = Hash>,
		ctx: &mut Context,
	) -> Result<()> {
		let Some(config) = &self.config else {
			return Ok(());
		};

		// If there is no collation function provided, bail out early.
		// Important: Lookahead collator and slot based collator do not use `CollatorFn`.
		if config.collator.is_none() {
			return Ok(())
		}

		let para_id = config.para_id;

		let _overall_timer = self.metrics.time_new_activations();

		for relay_parent in activated {
			let _relay_parent_timer = self.metrics.time_new_activations_relay_parent();

			let session_index =
				request_session_index_for_child(relay_parent, ctx.sender()).await.await??;

			let session_info =
				self.session_info.get(relay_parent, session_index, ctx.sender()).await?;
			let async_backing_params = session_info.async_backing_params;
			let n_validators = session_info.n_validators;

			let availability_cores =
				request_availability_cores(relay_parent, ctx.sender()).await.await??;

			// The loop bellow will fill in cores that the para is allowed to build on.
			let mut cores_to_build_on = Vec::new();

			for (core_idx, core) in availability_cores.into_iter().enumerate() {
				let scheduled_core = match core {
					CoreState::Scheduled(scheduled_core) => scheduled_core,
					CoreState::Occupied(occupied)
						if async_backing_params.max_candidate_depth >= 1 =>
					{
						// maximum candidate depth when building on top of a block
						// pending availability is necessarily 1 - the depth of the
						// pending block is 0 so the child has depth 1.

						match occupied.next_up_on_available {
							Some(scheduled_core) => scheduled_core,
							None => continue,
						}
					},
					CoreState::Occupied(_) => {
						gum::trace!(
							target: LOG_TARGET,
							core_idx = %core_idx,
							relay_parent = ?relay_parent,
							"core is occupied. Keep going.",
						);
						continue
					},
					CoreState::Free => {
						gum::trace!(
							target: LOG_TARGET,
							core_idx = %core_idx,
							"core is not assigned to any para. Keep going.",
						);
						continue
					},
				};

				if scheduled_core.para_id != config.para_id {
					gum::trace!(
						target: LOG_TARGET,
						core_idx = %core_idx,
						relay_parent = ?relay_parent,
						our_para = %config.para_id,
						their_para = %scheduled_core.para_id,
						"core is not assigned to our para. Keep going.",
					);
				} else {
					// Accumulate cores for building collation(s) outside the loop.
					cores_to_build_on.push(CoreIndex(core_idx as u32));
				}
			}

			// Skip to next relay parent if there is no core assigned to us.
			if cores_to_build_on.is_empty() {
				continue
			}

			// We are being very optimistic here, but one of the cores could be pending availability
			// for some more blocks, or even time out.
			// For timeout assumption the collator can't really know because it doesn't receive
			// bitfield gossip.
			let para_backing_state =
				request_para_backing_state(relay_parent, config.para_id, ctx.sender())
					.await
					.await??
					.ok_or(Error::MissingParaBackingState)?;

			let para_assumption = if para_backing_state.pending_availability.is_empty() {
				OccupiedCoreAssumption::Free
			} else {
				OccupiedCoreAssumption::Included
			};

			gum::debug!(
				target: LOG_TARGET,
				relay_parent = ?relay_parent,
				our_para = %para_id,
				?para_assumption,
				"Occupied core(s) assumption",
			);

			let mut validation_data = match request_persisted_validation_data(
				relay_parent,
				para_id,
				para_assumption,
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
					continue
				},
			};

			let validation_code_hash = match request_validation_code_hash(
				relay_parent,
				para_id,
				para_assumption,
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
					continue
				},
			};

			let task_config = config.clone();
			let metrics = self.metrics.clone();
			let mut task_sender = ctx.sender().clone();

			ctx.spawn(
				"chained-collation-builder",
				Box::pin(async move {
					for core_index in cores_to_build_on {
						let collator_fn = match task_config.collator.as_ref() {
							Some(x) => x,
							None => return,
						};

						let (collation, result_sender) =
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

						let parent_head = collation.head_data.clone();
						if let Err(err) = construct_and_distribute_receipt(
							PreparedCollation {
								collation,
								para_id,
								relay_parent,
								validation_data: validation_data.clone(),
								validation_code_hash,
								n_validators,
								core_index,
								session_index,
							},
							task_config.key.clone(),
							&mut task_sender,
							result_sender,
							&metrics,
							session_info.v2_receipts,
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

						// Chain the collations. All else stays the same as we build the chained
						// collation on same relay parent.
						validation_data.parent_head = parent_head;
					}
				}),
			)?;
		}

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
struct SessionInfo {
	v2_receipts: bool,
	n_validators: usize,
	async_backing_params: AsyncBackingParams,
}

struct SessionInfoCache(LruMap<SessionIndex, SessionInfo>);

impl SessionInfoCache {
	fn new() -> Self {
		Self(LruMap::new(ByLength::new(2)))
	}

	async fn get<Sender: SubsystemSender<RuntimeApiMessage>>(
		&mut self,
		relay_parent: Hash,
		session_index: SessionIndex,
		sender: &mut Sender,
	) -> Result<SessionInfo> {
		if let Some(info) = self.0.get(&session_index) {
			return Ok(info.clone())
		}

		let (validators, async_backing_params) = join!(
			request_validators(relay_parent, &mut sender.clone()).await,
			request_async_backing_params(relay_parent, &mut sender.clone()).await,
		);

		let node_features = request_node_features(relay_parent, session_index, sender)
			.await?
			.unwrap_or(NodeFeatures::EMPTY);

		let n_validators = validators??.len();
		let async_backing_params = async_backing_params??;

		let info = SessionInfo {
			v2_receipts: node_features
				.get(FeatureIndex::CandidateReceiptV2 as usize)
				.map(|b| *b)
				.unwrap_or(false),
			n_validators,
			async_backing_params,
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
		let claim_queue = fetch_claim_queue(sender, relay_parent)
			.await
			.map_err(Error::UtilRuntime)?
			.ok_or(Error::ClaimQueueNotAvailable)?;

		let transposed_claim_queue = transpose_claim_queue(claim_queue.0);

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
