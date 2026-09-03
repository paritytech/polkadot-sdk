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

use authorizer::AuraAuthorizer;
use codec::Decode;
use futures::{Stream, StreamExt};
use jam_cumulus_facade::service_state::{ParaInfo, para_info_key};
use jam_interface::{
	AuthPool, AuthorizerHash, BlockDesc, CoreIndex, HeaderHash, JamChainSource, JamStateSource,
	ServiceId, Slot as JamSlot, StateRootHash, StorageKey,
};
use jam_state_helpers::{StateKey, StateProof};
use jam_types::RefineContext;
use sp_runtime::traits::Block as BlockT;
use sp_timestamp::Timestamp;
use std::{future::Future, time::Instant};

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
	/// The core whose authorizer pool held this para's authorizer at the anchor, if any. `None`
	/// means the package must not be submitted anywhere — no guarantor would authorize it.
	pub submit_target: Option<CoreIndex>,
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

/// How old a lookup anchor may be when a guarantor sees it.
///
/// polkajam refuses to guarantee a package whose lookup anchor is older than this
/// (`max_lookup_anchor_age`, 24 slots on tiny networks), on top of requiring it to be finalized
/// and its `lookup_anchor_slot` to be exactly that block's slot.
const MAX_LOOKUP_ANCHOR_AGE: JamSlot = 24;
/// How much of that age is reserved for the package still being in flight.
///
/// A package is submitted at its anchor and has [`REPORT_DEADLINE_SLOTS`](super::collation_task)
/// slots to be reported, so a lookup anchor picked this close to the limit would expire in flight
/// — and a package that dies for that reason looks exactly like one that was never submitted.
const LOOKUP_ANCHOR_SAFETY_MARGIN: JamSlot = 8;

/// What a walk back through the finalized chain found to use as a lookup anchor.
#[derive(Debug, PartialEq, Eq)]
struct LookupAnchorWalk {
	/// The newest block within one turn of the round-robin whose slot names this collator.
	chosen: Option<BlockDesc>,
	/// How many blocks the walk looked at, the block it started from included.
	walked: u32,
	/// Why the walk ended before exhausting its window — a JAM read that failed, or the chain
	/// running out under it near genesis. Not an error in itself: a shorter walk simply has fewer
	/// candidates.
	stopped_early: Option<String>,
}

/// The lookup anchor a package must carry for this collator's token to be accepted.
///
/// The AURA authorizer derives the expected collator from `refine_context().lookup_anchor_slot` —
/// the anchor's own slot is not visible in-core — so it is the *lookup* anchor, not the anchor,
/// that has to name us. Walking back through finalized blocks is safe: they are all finalized by
/// construction, which is polkajam's other requirement, and one full turn of the round-robin is
/// as far as the arithmetic can go before repeating itself.
///
/// Walks newest-first from `newest` and stops at the first block that names this collator, so on
/// the common case (our own slot) it costs no JAM read at all.
async fn walk_back_to_our_slot<Parent, Fut>(
	newest: BlockDesc,
	authorizer: &AuraAuthorizer,
	parent: Parent,
) -> LookupAnchorWalk
where
	Parent: Fn(HeaderHash) -> Fut,
	Fut: Future<Output = Result<BlockDesc, String>>,
{
	let window = authorizer.round_robin_window();
	let mut block = newest;
	for walked in 1..=window {
		let ours = authorizer.names_us(block.slot);
		tracing::debug!(
			target: LOG_TARGET,
			candidate = ?block.header_hash,
			slot = block.slot,
			expected_collator = authorizer.collator_for(block.slot),
			own_index = authorizer.own_index(),
			walked,
			window,
			accepted = ours,
			"Considered a block as the package's lookup anchor.",
		);
		if ours {
			return LookupAnchorWalk { chosen: Some(block), walked, stopped_early: None };
		}
		if walked < window {
			match parent(block.header_hash).await {
				Ok(parent) => block = parent,
				Err(error) =>
					return LookupAnchorWalk { chosen: None, walked, stopped_early: Some(error) },
			}
		}
	}
	LookupAnchorWalk { chosen: None, walked: window, stopped_early: None }
}

/// Whether a lookup anchor this old would still be inside polkajam's window when the package
/// carrying it is reported, rather than expiring in flight.
fn lookup_anchor_survives_reporting(anchor_slot: JamSlot, lookup_anchor_slot: JamSlot) -> bool {
	anchor_slot.saturating_sub(lookup_anchor_slot) <=
		MAX_LOOKUP_ANCHOR_AGE.saturating_sub(LOOKUP_ANCHOR_SAFETY_MARGIN)
}

/// Which cores currently hold this para's authorizer, and therefore where its packages may go.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub(crate) struct PoolScan {
	/// The core to submit to. `None` means no core holds the para's authorizer at all — the
	/// builder keeps authoring, but nothing may be submitted.
	pub target: Option<CoreIndex>,
	/// The other cores holding it. Spreading packages over several is elastic scaling, not this
	/// phase, so they are logged rather than used — but a reassignment in progress is exactly the
	/// state that shows up here, and it has to be visible.
	pub also_on: Vec<CoreIndex>,
}

/// Find the cores whose authorizer pool holds `wanted`, lowest index first.
///
/// The pool rather than the queue: our cores are dedicated and a queue holds 80 copies of one
/// hash, so the pool converges to 8 copies of it and a scan sees an assignment in the very block
/// it lands. It also drains for 8 blocks after a core is freed, which is the handover overlap
/// that keeps packages already in flight valid.
fn scan_pools<'a>(
	pools: impl IntoIterator<Item = &'a AuthPool>,
	wanted: AuthorizerHash,
) -> PoolScan {
	let mut holders = pools
		.into_iter()
		.enumerate()
		.filter(|(_, pool)| pool.contains(&wanted))
		.map(|(core, _)| core as CoreIndex);
	PoolScan { target: holders.next(), also_on: holders.collect() }
}

/// Read the authorizer pools at `anchor` and work out where this para's packages belong.
pub(crate) async fn scan_pools_at<Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	authorizer: &AuraAuthorizer,
) -> Result<PoolScan, String> {
	let wanted = authorizer.hash();
	let started = Instant::now();
	let pools = jam
		.auth_pools(anchor)
		.await
		.map_err(|error| format!("authorizer pools at {anchor:?}: {error}"))?;
	let elapsed_ms = started.elapsed().as_millis();

	for (core, pool) in pools.iter().enumerate() {
		tracing::debug!(
			target: LOG_TARGET,
			method = "stateValue",
			key = "C(1) AuthPools",
			at = ?anchor,
			core,
			pool_len = pool.len(),
			holds_ours = pool.contains(&wanted),
			copies_of_ours = pool.iter().filter(|hash| **hash == wanted).count(),
			?pool,
			elapsed_ms,
			"JAM read: one core's authorizer pool.",
		);
	}

	let scan = scan_pools(pools.iter(), wanted);
	tracing::debug!(
		target: LOG_TARGET,
		at = ?anchor,
		authorizer_hash = ?wanted,
		cores = pools.len(),
		target = ?scan.target,
		also_on = ?scan.also_on,
		elapsed_ms,
		"Scanned the authorizer pools for this para's authorizer.",
	);
	Ok(scan)
}

/// Run one JAM read, logging the method, the block it was read at, what came back and the
/// round-trip: latency is a first-class debugging signal on this path.
pub(crate) async fn jam_read<T, F>(
	method: &'static str,
	at: HeaderHash,
	read: F,
) -> Result<T, String>
where
	F: Future<Output = jam_interface::Result<T>>,
	T: std::fmt::Debug,
{
	let started = Instant::now();
	let result = read.await;
	let elapsed_ms = started.elapsed().as_millis();
	match &result {
		Ok(value) => tracing::debug!(
			target: LOG_TARGET,
			method,
			?at,
			?value,
			elapsed_ms,
			"JAM read.",
		),
		Err(error) => tracing::warn!(
			target: LOG_TARGET,
			method,
			?at,
			?error,
			elapsed_ms,
			"JAM read failed.",
		),
	}
	result.map_err(|error| format!("{method}: {error}"))
}

/// Run the lookup-anchor walk against the live chain and apply the two rules a guarantor will
/// apply to the answer: it has to name this collator, and it has to survive the package's time in
/// flight.
///
/// `None` means the tick has to be skipped — there is no lookup anchor this collator could sign
/// against, so any package built now would be refused in-core with nothing but silence to show
/// for it. That is a loud log, not a quiet one.
pub(crate) async fn choose_lookup_anchor<Jam: JamChainSource + ?Sized>(
	jam: &Jam,
	anchor: &BlockDesc,
	newest: BlockDesc,
	authorizer: &AuraAuthorizer,
) -> Option<BlockDesc> {
	let started = Instant::now();
	let walk = walk_back_to_our_slot(newest, authorizer, |hash| {
		jam_read("parent", hash, jam.parent(hash))
	})
	.await;

	let Some(chosen) = walk.chosen else {
		tracing::warn!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			anchor_slot = anchor.slot,
			walk_started_at = ?newest.header_hash,
			walk_started_at_slot = newest.slot,
			blocks_walked = walk.walked,
			window = authorizer.round_robin_window(),
			collator_set_size = authorizer.collator_set_size(),
			own_index = authorizer.own_index(),
			stopped_early = ?walk.stopped_early,
			elapsed_ms = started.elapsed().as_millis(),
			"No finalized block within one turn of the round-robin names this collator, so there \
			 is no lookup anchor a package of ours could be signed against; skipping this tick.",
		);
		return None;
	};

	let age = anchor.slot.saturating_sub(chosen.slot);
	if !lookup_anchor_survives_reporting(anchor.slot, chosen.slot) {
		tracing::warn!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			anchor_slot = anchor.slot,
			lookup_anchor = ?chosen.header_hash,
			lookup_anchor_slot = chosen.slot,
			age,
			max_age = MAX_LOOKUP_ANCHOR_AGE,
			safety_margin = LOOKUP_ANCHOR_SAFETY_MARGIN,
			blocks_walked = walk.walked,
			elapsed_ms = started.elapsed().as_millis(),
			"The newest lookup anchor naming this collator would expire before the package \
			 carrying it could be reported; skipping this tick.",
		);
		return None;
	}

	tracing::info!(
		target: LOG_TARGET,
		anchor = ?anchor.header_hash,
		anchor_slot = anchor.slot,
		lookup_anchor = ?chosen.header_hash,
		lookup_anchor_slot = chosen.slot,
		age,
		blocks_walked = walk.walked,
		own_index = authorizer.own_index(),
		elapsed_ms = started.elapsed().as_millis(),
		"Chose the lookup anchor whose slot names this collator.",
	);
	Some(chosen)
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
	use super::{authorizer::tests::authorizer_of, *};
	/// A finalized JAM chain, newest first, one block per slot ending at `newest_slot`. Block
	/// `slot` is named by its own slot number so the walk is legible from the assertions.
	fn finalized_chain(newest_slot: JamSlot, length: u32) -> Vec<BlockDesc> {
		(0..length)
			.map(|back| {
				let slot = newest_slot - back;
				BlockDesc { header_hash: HeaderHash::from([slot as u8; 32]), slot }
			})
			.collect()
	}

	/// Walk `chain` (newest first) as the builder walks the finalized chain, and say what it
	/// picked. Every step past the first is one `parent` round-trip on the real thing.
	fn walk(chain: &[BlockDesc], authorizer: &AuraAuthorizer) -> LookupAnchorWalk {
		futures::executor::block_on(walk_back_to_our_slot(chain[0], authorizer, |hash| async move {
			chain
				.iter()
				.position(|block| block.header_hash == hash)
				.and_then(|index| chain.get(index + 1))
				.copied()
				.ok_or_else(|| "the chain ends here".to_string())
		}))
	}

	/// The authorizer reads the *lookup* anchor's slot, so a package is only ever signable by the
	/// collator that slot names. Anchoring at the freshest finalized block, as phase 5a did, names
	/// whoever happens to own that slot — so the builder walks back to the newest one that names
	/// it, and takes the newest such block rather than any of them, because every slot of extra
	/// age is a slot off the package's reporting window.
	#[test]
	fn the_lookup_anchor_is_the_newest_finalized_block_naming_us() {
		let charlie = authorizer_of("alice,bob,charlie", "Charlie", 1);
		// Slots 20..=12; charlie is collator 2, so slots 20 and 17 name him.
		let chain = finalized_chain(20, 9);

		let from_20 = walk(&chain, &charlie);

		assert_eq!(from_20.chosen.expect("slot 20 names charlie").slot, 20);
		assert_eq!(from_20.walked, 1, "our own slot costs no `parent` read at all");

		let from_19 = walk(&chain[1..], &charlie);
		assert_eq!(from_19.chosen.expect("slot 17 names charlie").slot, 17);
		assert_eq!(from_19.walked, 3, "slots 19 and 18 are somebody else's");
	}

	/// A para slot several JAM timeslots long makes the round-robin coarser, and the walk has to
	/// use the same divisor the guest will: `--jam-slot-duration` is part of the config the
	/// authorizer hash commits to.
	#[test]
	fn the_walk_uses_the_paras_slot_duration() {
		let bob = authorizer_of("alice,bob", "Bob", 3);
		// Para slot = 3 timeslots; bob is collator 1, so timeslots 3..=5 and 9..=11 are his.
		assert_eq!(walk(&finalized_chain(11, 12), &bob).chosen.expect("slot 11").slot, 11);
		assert_eq!(walk(&finalized_chain(8, 12), &bob).chosen.expect("slot 5").slot, 5);
	}

	/// The walk is bounded by one turn of the round-robin: past that the arithmetic only repeats,
	/// so a longer walk would be JAM reads spent on an answer already known to be no. A collator
	/// alone in its set is named by every slot and never walks.
	#[test]
	fn the_walk_stops_after_one_turn_of_the_round_robin() {
		let alice = authorizer_of("alice,bob,charlie", "Alice", 1);
		assert_eq!(alice.round_robin_window(), 3);

		// Slot 20 names collator 2, 19 names 1, 18 names 0 — the last block in the window.
		let found = walk(&finalized_chain(20, 9), &alice);
		assert_eq!(found.chosen.expect("slot 18 names alice").slot, 18);
		assert_eq!(found.walked, 3);

		let alone = authorizer_of("alice", "Alice", 1);
		assert_eq!(walk(&finalized_chain(20, 9), &alone).walked, 1);
	}

	/// A chain that ends under the walk — near genesis, or a JAM read that failed mid-walk — is
	/// not a silent no: the tick is skipped either way, but the reason has to reach the log.
	#[test]
	fn a_walk_that_runs_out_of_chain_says_so() {
		let charlie = authorizer_of("alice,bob,charlie", "Charlie", 1);
		// Slots 19 and 18 name bob and alice; the chain stops before charlie's turn comes round.
		let short = walk(&finalized_chain(19, 2), &charlie);

		assert_eq!(short.chosen, None);
		assert_eq!(short.walked, 2);
		assert!(short.stopped_early.is_some(), "the reason is carried, not swallowed");
	}

	/// One core's authorizer pool, holding `hashes` as the eight-deep window it is.
	fn pool(hashes: &[AuthorizerHash]) -> AuthPool {
		AuthPool::truncate_from(hashes.to_vec())
	}

	fn other_hash() -> AuthorizerHash {
		AuthorizerHash([0xee; 32])
	}

	/// Nothing waits on JAM to author, but a package can only be submitted to a core whose pool
	/// holds this para's authorizer — anywhere else a guarantor would refuse it. So the scan is
	/// what turns "which core?" from a flag into a fact read off the chain.
	#[test]
	fn a_core_holding_the_paras_authorizer_is_where_its_packages_go() {
		let ours = authorizer_of("alice", "Alice", 1).hash();
		let pools = vec![pool(&[other_hash()]), pool(&[other_hash(), ours])];

		let scan = scan_pools(&pools, ours);

		assert_eq!(scan.target, Some(1));
		assert!(scan.also_on.is_empty());
	}

	/// A core that holds no copy of our hash is not ours, however full its pool is; with none of
	/// them holding it the para has no core at all, which is a state the collator has to survive
	/// rather than crash on — it keeps authoring and submits nothing.
	#[test]
	fn no_core_holding_it_is_no_target_at_all() {
		let ours = authorizer_of("alice", "Alice", 1).hash();

		assert_eq!(scan_pools(&[pool(&[other_hash()]), pool(&[])], ours), PoolScan::default());
		assert_eq!(scan_pools(&[], ours).target, None, "a chain with no cores names none");
	}

	/// Mid-reassignment both the old and the new core hold our hash while the old one's pool
	/// drains. Taking the lowest index keeps consecutive packages on one core (bursting across
	/// cores is elastic scaling, not this phase) and the rest are reported, because a core that
	/// silently held our packages would be invisible in the log.
	#[test]
	fn several_cores_holding_it_go_to_the_lowest_index() {
		let ours = authorizer_of("alice", "Alice", 1).hash();
		let pools = vec![pool(&[ours]), pool(&[other_hash()]), pool(&[ours]), pool(&[ours])];

		let scan = scan_pools(&pools, ours);

		assert_eq!(scan.target, Some(0));
		assert_eq!(scan.also_on, vec![2, 3]);
	}

	/// Two paras on one collator set differ only in their config, so their hashes differ and one
	/// para's core must never look like the other's.
	#[test]
	fn another_paras_core_is_not_ours() {
		let ours = authorizer_of("alice,bob", "Alice", 1).hash();
		let theirs = authorizer_of("alice,bob", "Alice", 2).hash();
		assert_ne!(ours, theirs);

		assert_eq!(scan_pools(&[pool(&[theirs])], ours).target, None);
	}

	/// polkajam refuses a lookup anchor older than `max_lookup_anchor_age`, and it applies that
	/// when the package is *reported*, not when it is built. A package has the report deadline to
	/// get there, so the builder has to keep that much of the window in hand — otherwise the
	/// package dies in flight and looks exactly like one that was never submitted.
	#[test]
	fn a_lookup_anchor_must_outlive_the_packages_time_in_flight() {
		let budget = MAX_LOOKUP_ANCHOR_AGE - LOOKUP_ANCHOR_SAFETY_MARGIN;
		assert!(lookup_anchor_survives_reporting(100, 100 - budget));
		assert!(!lookup_anchor_survives_reporting(100, 100 - budget - 1));
		assert!(lookup_anchor_survives_reporting(0, 0), "a chain younger than the window is fine");
	}


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
