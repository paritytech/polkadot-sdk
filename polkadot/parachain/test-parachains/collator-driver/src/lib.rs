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

//! The relay-chain-driven collation loop used by the test parachain collators.
//!
//! Production collators build collations on their own schedule and hand the resulting segments
//! to the collator protocol themselves (see `cumulus-client-collator`). The test parachains have
//! no such machinery: they author one chain of collations per imported relay-chain block. This
//! crate keeps that loop in one place.
//!
//! Every relay-chain block import spawns one task, which queries the claim queue, the persisted
//! validation data, the validation code hash and the validator count, then asks the parachain for
//! one collation per assigned core, chaining them on the same relay parent, and distributes each
//! as a single-candidate V2 segment. The runtime queries and the erasure coding must not run on
//! the import loop: falling behind it means the overseer deactivates a leaf before we wait on it.

#![deny(missing_docs)]

use cumulus_client_collator::collation::{
	build_segment_entry, build_segment_entry_without_ump_check, SegmentEntryParams,
};
use futures::{channel::oneshot, future::BoxFuture, FutureExt, StreamExt};
use polkadot_node_primitives::{Collation, SegmentCollation, UpwardMessages};
use polkadot_node_subsystem::messages::{CollatorProtocolMessage, Segment, SegmentEntry};
use polkadot_node_subsystem_util::{runtime::ClaimQueueSnapshot, TimeoutExt};
use polkadot_overseer::Handle as OverseerHandle;
use polkadot_primitives::{
	runtime_api::ParachainHost, transpose_claim_queue, Block, CandidateCommitments,
	CandidateDescriptorVersion, CommittedCandidateReceiptError, CoreIndex, Hash, Id as ParaId,
	OccupiedCoreAssumption, PersistedValidationData, SessionIndex, ValidationCodeHash,
};
use sc_client_api::BlockchainEvents;
use sp_api::ProvideRuntimeApi;
use sp_core::traits::SpawnNamed;
use std::{collections::HashSet, sync::Arc, time::Duration};

#[cfg(test)]
mod tests;

const LOG_TARGET: &str = "parachain::collator-driver";

/// A leaf the overseer already deactivated is never answered, so the wait needs a cap. Kept well
/// below any relay-chain block time: the overseer sees the same import notification we do, so the
/// normal wait is negligible, and a stuck leaf must not hold its task open across several leaves.
const ACTIVATION_TIMEOUT: Duration = Duration::from_secs(1);

/// Builds a collation on top of the given [`PersistedValidationData`].
///
/// Returns `None` if the parachain cannot author on top of the given state.
pub type CollationBuilder = Box<
	dyn Fn(Hash, &PersistedValidationData) -> BoxFuture<'static, Option<Collation>> + Send + Sync,
>;

/// How the built collations are distributed over the cores assigned to the para.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DistributionMode {
	/// Author one collation per assigned core, chained on the same relay parent, and distribute
	/// each on the core the parachain committed to. This is what an honest collator does.
	OnePerAssignedCore,
	/// Author a single collation and distribute the very same candidate on every assigned core,
	/// skipping the UMP-signal core-index check. Simulates a malicious collator.
	DuplicateToAllAssignedCores,
}

/// Parameters for [`run`].
pub struct Params<Client, Spawner> {
	/// The relay-chain client, used for block import notifications and runtime API calls.
	pub client: Arc<Client>,
	/// A handle to the relay-chain node's overseer.
	pub overseer_handle: OverseerHandle,
	/// The para we are collating for.
	pub para_id: ParaId,
	/// Builds the collations.
	pub build_collation: CollationBuilder,
	/// How to spread the collations over the assigned cores.
	pub distribution_mode: DistributionMode,
	/// Spawner for the per-leaf collation tasks.
	pub spawner: Spawner,
}

/// Drive collation generation off relay-chain block imports until the notification stream ends.
pub async fn run<Client, Spawner>(params: Params<Client, Spawner>)
where
	Client: ProvideRuntimeApi<Block> + BlockchainEvents<Block> + Send + Sync + 'static,
	Client::Api: ParachainHost<Block>,
	Spawner: SpawnNamed,
{
	let Params { client, overseer_handle, para_id, build_collation, distribution_mode, spawner } =
		params;

	let build_collation = Arc::new(build_collation);
	let mut import_notifications = client.import_notification_stream();

	while let Some(notification) = import_notifications.next().await {
		// One task per leaf, so that neither the activation wait nor the erasure coding of one
		// leaf delays the next: a leaf dequeued late here is already deactivated.
		spawner.spawn(
			"collator-driver-leaf",
			Some("collator-driver"),
			collate_on_leaf(
				client.clone(),
				overseer_handle.clone(),
				para_id,
				notification.hash,
				build_collation.clone(),
				distribution_mode,
			)
			.boxed(),
		);
	}
}

/// Collate on one relay-chain leaf, once the overseer has activated it.
async fn collate_on_leaf<Client>(
	client: Arc<Client>,
	mut overseer_handle: OverseerHandle,
	para_id: ParaId,
	relay_parent: Hash,
	build_collation: Arc<CollationBuilder>,
	distribution_mode: DistributionMode,
) where
	Client: ProvideRuntimeApi<Block> + Send + Sync + 'static,
	Client::Api: ParachainHost<Block>,
{
	// The runtime API guard is cheap and rules out most leaves, so it runs before the wait.
	let Some(leaf) = leaf_info(&*client, relay_parent, para_id) else { return };

	if !wait_for_activation(&mut overseer_handle, relay_parent).await {
		return;
	}

	match distribution_mode {
		DistributionMode::OnePerAssignedCore => {
			collate_on_assigned_cores(
				&mut overseer_handle,
				para_id,
				relay_parent,
				leaf,
				&build_collation,
			)
			.await
		},
		DistributionMode::DuplicateToAllAssignedCores => {
			duplicate_to_assigned_cores(
				&mut overseer_handle,
				para_id,
				relay_parent,
				leaf,
				&build_collation,
			)
			.await
		},
	}
}

/// Block until the overseer reports `relay_parent` as an active leaf. Returns `false` if the
/// wait could not be completed, in which case the leaf must be skipped rather than collated on.
async fn wait_for_activation(overseer_handle: &mut OverseerHandle, relay_parent: Hash) -> bool {
	let (tx, rx) = oneshot::channel();
	overseer_handle.wait_for_activation(relay_parent, tx).await;

	match rx.timeout(ACTIVATION_TIMEOUT).await {
		Some(Ok(Ok(()))) => true,
		Some(Ok(Err(error))) => {
			gum::debug!(
				target: LOG_TARGET,
				?relay_parent,
				?error,
				"Overseer failed to activate the leaf, not collating on it",
			);
			false
		},
		Some(Err(_)) => {
			gum::debug!(
				target: LOG_TARGET,
				?relay_parent,
				"Activation response dropped, not collating on this leaf",
			);
			false
		},
		None => {
			// A skipped leaf silently costs the para a leaf's worth of candidates, with no retry.
			gum::warn!(
				target: LOG_TARGET,
				?relay_parent,
				?ACTIVATION_TIMEOUT,
				"Leaf skipped: the overseer did not activate it before the timeout expired",
			);
			false
		},
	}
}

/// Everything we need to know about a relay-chain leaf in order to collate on it.
struct LeafInfo {
	claim_queue: ClaimQueueSnapshot,
	validation_data: PersistedValidationData,
	validation_code_hash: ValidationCodeHash,
	session_index: SessionIndex,
	n_validators: usize,
}

/// Query the runtime for the data needed to collate on `relay_parent`.
///
/// Returns `None` if the para is not assigned to any core or if any query fails.
fn leaf_info<Client>(client: &Client, relay_parent: Hash, para_id: ParaId) -> Option<LeafInfo>
where
	Client: ProvideRuntimeApi<Block>,
	Client::Api: ParachainHost<Block>,
{
	let api = client.runtime_api();

	macro_rules! query {
		($what:expr, $name:literal) => {
			match $what {
				Ok(value) => value,
				Err(error) => {
					gum::error!(
						target: LOG_TARGET,
						?relay_parent,
						"Failed to query {} runtime API: {error:?}",
						$name,
					);
					return None;
				},
			}
		};
	}

	/// As `query!`, but for runtime APIs returning `Option`: logs and bails when absent, rather
	/// than returning `None` silently.
	macro_rules! query_some {
		($what:expr, $name:literal) => {
			match query!($what, $name) {
				Some(value) => value,
				None => {
					gum::debug!(
						target: LOG_TARGET,
						?relay_parent,
						?para_id,
						"{} is not available",
						$name,
					);
					return None;
				},
			}
		};
	}

	let claim_queue =
		ClaimQueueSnapshot::from(query!(api.claim_queue(relay_parent), "claim queue"));

	// Nothing to do if no core is assigned to us at any depth.
	if !claim_queue.iter_all_claims().any(|(_, paras)| paras.contains(&para_id)) {
		gum::trace!(
			target: LOG_TARGET,
			?relay_parent,
			?para_id,
			"Para is not assigned to any core",
		);
		return None;
	}

	// We are being very optimistic here, but one of the cores could be pending availability for
	// some more blocks, or even time out. We assume all cores are being freed.
	let validation_data = query_some!(
		api.persisted_validation_data(relay_parent, para_id, OccupiedCoreAssumption::Included),
		"persisted validation data"
	);
	let validation_code_hash = query_some!(
		api.validation_code_hash(relay_parent, para_id, OccupiedCoreAssumption::Included),
		"validation code hash"
	);
	let session_index =
		query!(api.session_index_for_child(relay_parent), "session index for child");
	let n_validators = query!(api.validators(relay_parent), "validators").len();

	Some(LeafInfo {
		claim_queue,
		validation_data,
		validation_code_hash,
		session_index,
		n_validators,
	})
}

/// Author one collation per assigned core, chaining them on the same relay parent.
async fn collate_on_assigned_cores(
	overseer_handle: &mut OverseerHandle,
	para_id: ParaId,
	relay_parent: Hash,
	leaf: LeafInfo,
	build_collation: &CollationBuilder,
) {
	let LeafInfo {
		claim_queue,
		mut validation_data,
		validation_code_hash,
		session_index,
		n_validators,
	} = leaf;

	let transposed_claim_queue = transpose_claim_queue(claim_queue.0.clone());
	let n_assigned_cores = claim_queue
		.iter_all_claims()
		.filter(|(_, paras)| paras.contains(&para_id))
		.count();

	// Track the cores we already submitted on, so that a parachain repeatedly selecting the same
	// core does not get more than one candidate backed there.
	let mut used_cores = HashSet::new();

	for index in 0..n_assigned_cores {
		let Some(collation) = build_collation(relay_parent, &validation_data).await else {
			gum::debug!(target: LOG_TARGET, ?para_id, "Collator returned no collation");
			return;
		};

		let core_index = match select_core_index(
			&claim_queue,
			para_id,
			&collation.upward_messages,
			index,
			&used_cores,
		) {
			Ok(core_index) => core_index,
			// Core reuse means the para committed the same selector twice and its chain is
			// truncated here; the base logged that at `warn`. The other variants are ordinary.
			Err(error @ CoreSelectionError::CoreReused(_)) => {
				gum::warn!(target: LOG_TARGET, ?para_id, "Not collating: {error}");
				return;
			},
			Err(error) => {
				gum::debug!(target: LOG_TARGET, ?para_id, "Not collating: {error}");
				return;
			},
		};
		used_cores.insert(core_index);

		// Chain the collations: all else stays the same as we build on the same relay parent.
		let parent_head = collation.head_data.clone();
		let entry = match build_segment_entry(
			SegmentEntryParams {
				collation: SegmentCollation {
					collation,
					relay_parent,
					validation_data: validation_data.clone(),
					validation_code_hash,
					session_index,
				},
				para_id,
				core_index,
				n_validators,
			},
			&transposed_claim_queue,
			CandidateDescriptorVersion::V2,
		) {
			Ok(entry) => entry,
			Err(error) => {
				gum::error!(target: LOG_TARGET, ?para_id, "Failed to build segment entry: {error}");
				return;
			},
		};
		validation_data.parent_head = parent_head;

		distribute(overseer_handle, para_id, core_index, entry).await;
	}
}

/// Author a single collation and distribute the very same candidate on every core assigned to
/// the para at the claim queue offset the collation committed to.
async fn duplicate_to_assigned_cores(
	overseer_handle: &mut OverseerHandle,
	para_id: ParaId,
	relay_parent: Hash,
	leaf: LeafInfo,
	build_collation: &CollationBuilder,
) {
	let LeafInfo {
		claim_queue,
		validation_data,
		validation_code_hash,
		session_index,
		n_validators,
		..
	} = leaf;

	let Some(collation) = build_collation(relay_parent, &validation_data).await else {
		gum::info!(target: LOG_TARGET, ?para_id, "Collator returned no collation");
		return;
	};

	// The cores are read at the offset the collation itself committed to, so that they are the
	// same set the core selection resolves against.
	let cores = match duplication_cores(&claim_queue, para_id, &collation.upward_messages) {
		Ok(cores) => cores,
		Err(error) => {
			gum::warn!(target: LOG_TARGET, ?para_id, "Not distributing: {error}");
			return;
		},
	};

	if cores.len() == 1 {
		gum::info!(
			target: LOG_TARGET,
			"Malus collator configured with duplicate collations, but only 1 core assigned. \
			Collator will not do anything malicious.",
		);
	}

	// The UMP-signal check enforces that the parachain selects the core the candidate is
	// submitted on, so it has to be skipped in order to submit the same candidate on several
	// cores.
	let entry = match build_segment_entry_without_ump_check(
		SegmentCollation {
			collation,
			relay_parent,
			validation_data,
			validation_code_hash,
			session_index,
		},
		n_validators,
	) {
		Ok(entry) => entry,
		Err(error) => {
			gum::error!(target: LOG_TARGET, ?para_id, "Failed to build segment entry: {error}");
			return;
		},
	};

	for core_index in cores {
		distribute(overseer_handle, para_id, core_index, entry.clone()).await;
	}
}

async fn distribute(
	overseer_handle: &mut OverseerHandle,
	para_id: ParaId,
	core_index: CoreIndex,
	entry: SegmentEntry,
) {
	gum::trace!(target: LOG_TARGET, ?para_id, "Distributing collation on core {}", core_index.0);

	overseer_handle
		.send_msg(
			CollatorProtocolMessage::DistributeSegment {
				core_index,
				para_id,
				segment: Segment::V2(entry),
			},
			"CollatorDriver",
		)
		.await;
}

/// Why no core could be picked for a collation.
#[derive(Debug, thiserror::Error)]
enum CoreSelectionError {
	#[error("error processing UMP signals: {0}")]
	UmpSignals(CommittedCandidateReceiptError),
	#[error("no core is assigned to the para at claim queue depth {0}")]
	NoAssignment(usize),
	#[error("parachain repeatedly selected the same core index: {0}")]
	CoreReused(u32),
}

/// The cores to duplicate a collation over, ordered so that the core the parachain selected is
/// distributed last.
fn duplication_cores(
	claim_queue: &ClaimQueueSnapshot,
	para_id: ParaId,
	upward_messages: &UpwardMessages,
) -> Result<Vec<CoreIndex>, CoreSelectionError> {
	let (selector, cq_offset) = core_selector(upward_messages, 0)?;
	let cores = cores_at_depth(claim_queue, para_id, cq_offset)?;

	// Resolved exactly as `select_core_index` does, against the same non-empty core set, so the
	// selected core is a member of `cores` by construction.
	let selected = cores[selector % cores.len()];

	Ok(distribution_order(cores, selected))
}

/// Order `cores` so that `selected` comes last.
///
/// The collator protocol stores only the first candidate per output head at a scheduling parent,
/// so the one that reaches validators must be on a core the parachain did not select — on the
/// selected core it is a valid candidate and no mismatch is ever detected.
fn distribution_order(mut cores: Vec<CoreIndex>, selected: CoreIndex) -> Vec<CoreIndex> {
	cores.sort_by_key(|core| *core == selected);
	cores
}

/// The core selector index and claim queue offset the collation committed to via its UMP signals.
/// Absent a commitment, the collation's position in the chain selects the core, at depth `0`.
fn core_selector(
	upward_messages: &UpwardMessages,
	index: usize,
) -> Result<(usize, usize), CoreSelectionError> {
	let commitments =
		CandidateCommitments { upward_messages: upward_messages.clone(), ..Default::default() };
	let ump_signals = commitments.ump_signals().map_err(CoreSelectionError::UmpSignals)?;

	Ok(ump_signals
		.core_selector()
		.map(|(selector, offset)| (selector.0 as usize, offset.0 as usize))
		.unwrap_or((index, 0)))
}

/// The cores assigned to the para at claim queue depth `cq_offset`, or an error if there are none.
fn cores_at_depth(
	claim_queue: &ClaimQueueSnapshot,
	para_id: ParaId,
	cq_offset: usize,
) -> Result<Vec<CoreIndex>, CoreSelectionError> {
	let cores = claim_queue
		.iter_claims_at_depth_for_para(cq_offset, para_id)
		.collect::<Vec<_>>();

	(!cores.is_empty())
		.then_some(cores)
		.ok_or(CoreSelectionError::NoAssignment(cq_offset))
}

/// Pick the core to submit the `index`-th chained collation of a leaf on.
///
/// The parachain may commit to a core selector and a claim queue offset via its UMP signals; if
/// it does not, the collation's position in the chain is used as the selector and the cores
/// assigned at depth `0` are considered.
fn select_core_index(
	claim_queue: &ClaimQueueSnapshot,
	para_id: ParaId,
	upward_messages: &UpwardMessages,
	index: usize,
	used_cores: &HashSet<CoreIndex>,
) -> Result<CoreIndex, CoreSelectionError> {
	let (selector, cq_offset) = core_selector(upward_messages, index)?;
	let cores_to_build_on = cores_at_depth(claim_queue, para_id, cq_offset)?;

	let core_index = cores_to_build_on[selector % cores_to_build_on.len()];
	if used_cores.contains(&core_index) {
		return Err(CoreSelectionError::CoreReused(core_index.0));
	}

	Ok(core_index)
}
