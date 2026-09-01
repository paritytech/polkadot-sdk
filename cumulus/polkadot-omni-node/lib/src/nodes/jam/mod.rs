// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! The JAM collator (phase-1 PoC): a parachain node whose backing chain is JAM instead of a
//! relay chain.
//!
//! Two tasks form the collator loop:
//!
//! - the [builder task](builder_task) authors on a wall-clock parachain-slot timer, anchoring at
//!   the JAM tip it caches, building on the deepest block the local database holds below the head
//!   JAM has accumulated rather than waiting for inclusion — or on a sibling of that head once it
//!   has stood still long enough to say the branch above it is lost — with the shared authoring
//!   primitives and a *mocked* parachain inherent, and feeds the channel;
//! - the [collation task](collation_task) turns each block into one independent work package —
//!   phase 5a links nothing to anything, so there is no prerequisite, no import and no export —
//!   submits it, follows `workPackageStatus` for every package in flight, and drives
//!   resubmission and re-anchoring behind a pluggable [policy](resubmission).
//!
//! Heads following reuses `cumulus_client_consensus_common::run_parachain_consensus` with
//! streams built from the parachain service's per-para state entry (`ParaInfo.head_data`).

pub(crate) mod authorizer;
pub(crate) mod builder_task;
pub(crate) mod collation_task;
pub(crate) mod resubmission;

use codec::Decode;
use futures::{Stream, StreamExt};
use jam_cumulus_facade::service_state::{ParaInfo, para_info_key};
use jam_interface::{
	BlockDesc, HeaderHash, JamStateSource, ServiceId, Slot as JamSlot, StateRootHash, StorageKey,
};
use jam_state_helpers::{StateKey, StateProof};
use jam_types::RefineContext;
use sp_runtime::traits::Block as BlockT;
use sp_timestamp::Timestamp;

pub(crate) const LOG_TARGET: &str = "jam-collator";

pub(crate) const JAM_SLOT_DURATION_MS: u64 = 6000;

/// The [`cumulus_primitives_core::AdditionalData`] key under which the anchor state proof
/// travels inside the PoV.
///
/// Namespaced by producer, next to the relay chain's own `"polkadot/relay_proof"`. This is a
/// wire contract with the parachain service, which reads exactly this key; a test pins it
/// against the reader's own constant.
pub(crate) const ANCHOR_STATE_PROOF_KEY: &str = "jam/anchor_state_proof";

/// Soft bound on the state proof the node returns. One key's proof is bounded by the trie depth,
/// so this only has to be comfortably large.
const PROOF_SIZE_LIMIT: u32 = 64 * 1024;

/// Message from the builder task to the collation task: one built parachain block plus the JAM
/// context it was built against.
pub(crate) struct JamCollatorMessage<Block: BlockT> {
	pub parent_header: Block::Header,
	pub block: Block,
	pub proof: sp_api::StorageProof,
	/// The refine context captured at build time; the anchor inside it decides the submission
	/// window.
	pub context: RefineContext,
	/// The anchor's state root, repeated here because it also has to travel inside the PoV: the
	/// service checks that the proof was built against the same state its refine context names.
	pub anchor_state_root: [u8; 32],
	/// Proof of the para's included head at the anchor, already verified against
	/// `anchor_state_root`.
	pub anchor_state_proof: StateProof,
	/// The timeslot of the anchor block, which starts the ~8-block clock the package has to be
	/// reported within. The collation task needs it to tell an expired anchor apart from any
	/// other reason a package failed.
	pub anchor_slot: JamSlot,
	/// The JAM best block that triggered this build (for logging).
	pub triggered_by: BlockDesc,
}

/// The 31-octet JAM state key of a para's head entry in the parachain service's storage.
///
/// Three parties must derive this identically — the collator asking for a proof, the node
/// serving it and the service verifying it in-core — so both halves come from shared code:
/// the service-local key from the facade, the state-key merklization from `jam-state-helpers`.
pub(crate) fn para_head_state_key(service_id: ServiceId, para_id: u32) -> StateKey {
	jam_state_helpers::service_value_state_key(service_id, &para_info_key(para_id.into()))
}

/// Fetch a proof of the para's head at `anchor` and check it against that anchor's state root.
///
/// Returns the proof to ship inside the PoV together with the value it proves; `None` means the
/// proof shows the para has no head yet, which is how a first block is recognised. Verifying
/// with the very code the service runs means a proof refine would reject never leaves the node.
///
/// Phase 5a kept this proof deliberately even though it decides nothing about lineage any more —
/// that is the parachain service's job at accumulate, and a block whose parent is not the proven
/// head is now perfectly legal (it is buffered, not rejected). The proof stays because it
/// exercises the in-core proof-read path the real PVF's `jam_chain_read` will need, and because
/// it is what lets refine tell a parachain's very first block from any other. Costing about
/// eleven trie nodes a package, that is worth keeping running.
pub(crate) async fn fetch_anchor_state_proof<Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	state_root: &StateRootHash,
	service_id: ServiceId,
	para_id: u32,
) -> Result<(StateProof, Option<Vec<u8>>), String> {
	let key = para_head_state_key(service_id, para_id);
	let range_proof = jam
		.state_proof(anchor, StorageKey(key), StorageKey(key), PROOF_SIZE_LIMIT)
		.await
		.map_err(|error| format!("state proof: {error}"))?;
	// polkajam's `RangeProof` is a host-side JSON type with no SCALE codec at all, so the form
	// that travels in the PoV is `jam-state-helpers`' own; converting is the host's job.
	let proof = StateProof {
		nodes: range_proof.nodes.iter().map(|node| **node).collect(),
		values: range_proof.values.iter().map(|(key, value)| (**key, value.to_vec())).collect(),
	};
	let proved = jam_state_helpers::verify(&proof, state_root, &key)
		.map_err(|error| format!("the node's own state proof does not verify: {error:?}"))?;
	Ok((proof, proved))
}

/// The wall-clock timestamp of a JAM timeslot: slots are 6 s, counted from the JAM common era.
///
/// This makes the parachain timestamp a pure function of the JAM anchor — deterministic under
/// import re-execution, and pre-aligned with the phase-4 time-base remap.
pub(crate) fn jam_slot_timestamp(slot: JamSlot) -> Timestamp {
	Timestamp::new((jam_types::JAM_COMMON_ERA + u64::from(slot) * 6) * 1000)
}

/// The JAM timeslot a wall-clock timestamp falls into — the inverse of [`jam_slot_timestamp`].
///
/// The builder is driven by the wall clock, so it has to know which JAM slot it is in: that is
/// what tells it whether the JAM tip it caches is current, and it is the slot the mocked
/// parachain inherent advertises.
pub(crate) fn jam_slot_at(timestamp: Timestamp) -> JamSlot {
	(timestamp
		.as_millis()
		.saturating_sub(jam_types::JAM_COMMON_ERA * 1000)
		/ JAM_SLOT_DURATION_MS) as JamSlot
}

/// The fake relay slot the mocked parachain inherent advertises for a JAM timeslot (both are
/// 6 s, so this is the timestamp expressed in relay slots).
pub(crate) fn jam_slot_as_relay_slot(slot: JamSlot) -> u64 {
	jam_slot_timestamp(slot).as_millis() / JAM_SLOT_DURATION_MS
}

/// Stream of raw para-head bytes for heads following: every change of the para's `ParaInfo`
/// entry in the parachain service's state yields the new `head_data` bytes.
///
/// Absent values (para not registered yet) and undecodable values are dropped, matching the
/// relay-chain streams' behavior of dropping absent heads.
pub(crate) async fn para_head_stream(
	jam: &impl JamStateSource,
	parachain_service: ServiceId,
	para_id: u32,
	finalized: bool,
) -> jam_interface::Result<impl Stream<Item = Vec<u8>> + Send> {
	let key = para_info_key(para_id.into());
	let stream = jam.service_value_stream(parachain_service, &key, finalized).await?;
	Ok(stream.filter_map(move |update| {
		futures::future::ready(match update.value {
			Some(bytes) => match ParaInfo::decode(&mut &bytes[..]) {
				Ok(info) => {
					tracing::debug!(
						target: LOG_TARGET,
						para_id,
						finalized,
						at = ?update.header_hash,
						slot = update.slot,
						head_len = info.head_data.len(),
						"Para head changed in JAM state.",
					);
					Some(info.head_data.into_inner())
				},
				Err(error) => {
					tracing::warn!(
						target: LOG_TARGET,
						para_id,
						?error,
						"Failed to decode ParaInfo from JAM state; dropping update.",
					);
					None
				},
			},
			None => {
				tracing::debug!(
					target: LOG_TARGET,
					para_id,
					finalized,
					at = ?update.header_hash,
					"Para not (yet) present in the parachain service's state.",
				);
				None
			},
		})
	}))
}

#[cfg(test)]
mod tests {
	use super::*;

	#[test]
	fn jam_slot_timestamp_is_common_era_based() {
		assert_eq!(jam_slot_timestamp(0).as_millis(), jam_types::JAM_COMMON_ERA * 1000);
		assert_eq!(jam_slot_timestamp(10).as_millis(), (jam_types::JAM_COMMON_ERA + 60) * 1000);
	}

	#[test]
	fn relay_slot_advances_once_per_jam_slot() {
		assert_eq!(jam_slot_as_relay_slot(1), jam_slot_as_relay_slot(0) + 1);
	}

	/// The wall-clock-driven builder derives its JAM slot from the clock and its timestamp from
	/// that slot, so the two conversions have to be exact inverses.
	#[test]
	fn jam_slot_at_inverts_jam_slot_timestamp() {
		for slot in [0, 1, 12_345] {
			assert_eq!(jam_slot_at(jam_slot_timestamp(slot)), slot);
		}
	}

	/// Anything inside a slot belongs to that slot; only the next boundary moves it on.
	#[test]
	fn jam_slot_at_floors_to_the_slot_start() {
		let slot_start = jam_slot_timestamp(7).as_millis();
		assert_eq!(jam_slot_at(Timestamp::new(slot_start + JAM_SLOT_DURATION_MS - 1)), 7);
		assert_eq!(jam_slot_at(Timestamp::new(slot_start + JAM_SLOT_DURATION_MS)), 8);
	}

	/// Before the common era there are no slots; clamping to slot 0 keeps a badly-set clock from
	/// wrapping the subtraction.
	#[test]
	fn jam_slot_at_clamps_before_the_common_era() {
		assert_eq!(jam_slot_at(Timestamp::new(0)), 0);
	}
}
