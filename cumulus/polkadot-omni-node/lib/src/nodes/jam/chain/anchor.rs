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

//! The anchor a tick builds against: its JAM reads, its lookup anchor, and the para-head state
//! proof every block carries.

use crate::{
	common::NodeBlock,
	nodes::jam::{
		authorizer::AuthoritySource,
		chain::{
			para_head::jam_read,
			pools::{scan_pools_at, PoolScan},
		},
		LOG_TARGET,
	},
};
use codec::Encode;
use cumulus_client_jam_authorizer::authorizer::AuraAuthorizer;
use cumulus_jam_state_reader::{JamProofFinalizer, JamProofReader, JamStateExt, JAM_PROOF_KEY};
use jam_interface::{
	BlockDesc, HeaderHash, JamChainSource, JamStateSource, ServiceId, Slot as JamSlot,
	StateRootHash, StorageKey,
};
use jam_types::RefineContext;
use parachain_service_core::{
	para_info_key, service_value_state_key, verify as verify_state_proof, StateKey, StateProof,
};
use sp_additional_data::{hash_value, AdditionalData, AdditionalDataExt, AdditionalDataFinalizer};
use sp_externalities::Extensions;
use std::sync::Arc;

/// How many JAM blocks back from the cached tip the anchor is taken.
///
/// Zero anchors at the tip itself, which buys a slot of proof freshness; the knob mirrors the
/// relay collator's `relay_parent_offset` for the day a fork-prone JAM chain wants distance.
pub(crate) const ANCHOR_OFFSET: u32 = 0;

/// Soft bound on the state proof the node returns. One key's proof is bounded by the trie depth,
/// so this only has to be comfortably large.
const PROOF_SIZE_LIMIT: u32 = 64 * 1024;

/// How old a lookup anchor may be when a guarantor sees it.
///
/// polkajam refuses to guarantee a package whose lookup anchor is older than this
/// (`max_lookup_anchor_age`, 24 slots on tiny networks), on top of requiring it to be finalized
/// and its `lookup_anchor_slot` to be exactly that block's slot.
pub(crate) const MAX_LOOKUP_ANCHOR_AGE: JamSlot = 24;
/// How much of that age is reserved for the package still being in flight.
///
/// A package is submitted at its anchor and has
/// [`REPORT_DEADLINE_SLOTS`](crate::nodes::jam::collation::policy::REPORT_DEADLINE_SLOTS)
/// slots to be reported, so a lookup anchor picked this close to the limit would expire in flight
/// — and a package that dies for that reason looks exactly like one that was never submitted.
pub(crate) const LOOKUP_ANCHOR_SAFETY_MARGIN: JamSlot = 8;

/// Everything one tick reads from JAM at its anchor.
pub(crate) struct AnchorReads {
	pub(crate) anchor: BlockDesc,
	pub(crate) context: RefineContext,
	/// The para's entry in the service's state, exactly as stored; `None` = no head yet, or not
	/// read (`read_head` false).
	pub(crate) included: Option<Vec<u8>>,
	/// Which cores hold the para's authorizer at this anchor, and therefore where the packages
	/// built on it may be submitted.
	pub(crate) pool_scan: PoolScan,
	/// The authorizer JAM uses for this anchor, which signs the turn's packages.
	pub(crate) authorizer: Arc<AuraAuthorizer>,
}

/// The JAM reads one tick makes at its anchor, plus the refine context built around it.
///
/// `Ok(None)` means the reads disagree with each other and the tick has to be skipped.
/// `read_head` adds the para head read: a resubmission round builds no block on it, so it skips
/// that read.
pub(crate) async fn read_anchor<Block, Jam>(
	jam: &Jam,
	tip: BlockDesc,
	service_id: ServiceId,
	para_id: u32,
	authorizers: &dyn AuthoritySource<Block>,
	at_hash: Block::Hash,
	read_head: bool,
) -> Result<Option<AnchorReads>, String>
where
	Block: NodeBlock,
	Jam: JamChainSource + JamStateSource + ?Sized,
{
	// Anchoring at the cached tip itself buys a slot of freshness over anchoring at its parent;
	// `ANCHOR_OFFSET` walks back from it when distance is wanted.
	let mut anchor = tip;
	for _ in 0..ANCHOR_OFFSET {
		anchor = jam_read("parent", anchor.header_hash, jam.parent(anchor.header_hash)).await?;
	}
	let state_root =
		jam_read("stateRoot", anchor.header_hash, jam.state_root(anchor.header_hash)).await?;
	let beefy_root =
		jam_read("beefyRoot", anchor.header_hash, jam.beefy_root(anchor.header_hash)).await?;
	let finalized = jam_read("finalizedBlock", anchor.header_hash, jam.finalized_block()).await?;
	let newest_lookup_anchor =
		jam_read("parent", finalized.header_hash, jam.parent(finalized.header_hash)).await?;
	// The set JAM authorizes at this anchor, and with it whether this node may author at all.
	// The parent of the finalized block is the lookup anchor: it is finalized, recent, and the
	// authorizer hash covers whichever set the anchor slot names.
	let Some(authorizer) = authorizers.for_anchor(at_hash, anchor.slot)? else {
		tracing::debug!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			anchor_slot = anchor.slot,
			"The JAM authority set at this anchor does not name this node; skipping this tick.",
		);
		return Ok(None);
	};
	if !authorizer.names_us(anchor.slot) {
		tracing::debug!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			anchor_slot = anchor.slot,
			expected_collator = authorizer.collator_for(anchor.slot),
			own_index = authorizer.own_index(),
			"The anchor slot names another collator; skipping this tick.",
		);
		return Ok(None);
	}
	let Some(lookup_anchor) = choose_lookup_anchor(&anchor, newest_lookup_anchor) else {
		return Ok(None);
	};
	let lookup_anchor_state_root =
		jam_read("stateRoot", lookup_anchor.header_hash, jam.state_root(lookup_anchor.header_hash))
			.await?;
	let pool_scan = scan_pools_at(jam, anchor.header_hash, &authorizer).await?;
	let included = if read_head {
		jam_read(
			"serviceValue",
			anchor.header_hash,
			jam.service_value(anchor.header_hash, service_id, &para_info_key(para_id.into())),
		)
		.await?
	} else {
		None
	};

	Ok(Some(AnchorReads {
		anchor,
		context: RefineContext {
			anchor: anchor.header_hash,
			anchor_slot: anchor.slot,
			state_root,
			beefy_root,
			lookup_anchor: lookup_anchor.header_hash,
			lookup_anchor_slot: lookup_anchor.slot,
			lookup_anchor_state_root,
			prerequisites: Default::default(),
		},
		included,
		pool_scan,
		authorizer,
	}))
}

/// Whether a lookup anchor this old would still be inside polkajam's window when the package
/// carrying it is reported, rather than expiring in flight.
pub(crate) fn lookup_anchor_survives_reporting(
	anchor_slot: JamSlot,
	lookup_anchor_slot: JamSlot,
) -> bool {
	anchor_slot.saturating_sub(lookup_anchor_slot) <=
		MAX_LOOKUP_ANCHOR_AGE.saturating_sub(LOOKUP_ANCHOR_SAFETY_MARGIN)
}

/// The lookup anchor a package carries: the newest finalized block, as long as it survives the
/// package's time in flight.
///
/// Under slot-bound sets the *anchor* slot selects the collator and the set is announced a
/// session ahead, so the round-robin walk this replaces is gone; what remains is polkajam's
/// freshness rule. `None` means the tick has to be skipped — a package anchored now would carry
/// a lookup anchor that expires before it could be reported.
pub(crate) fn choose_lookup_anchor(anchor: &BlockDesc, newest: BlockDesc) -> Option<BlockDesc> {
	let age = anchor.slot.saturating_sub(newest.slot);
	if !lookup_anchor_survives_reporting(anchor.slot, newest.slot) {
		tracing::warn!(
			target: LOG_TARGET,
			anchor = ?anchor.header_hash,
			anchor_slot = anchor.slot,
			lookup_anchor = ?newest.header_hash,
			lookup_anchor_slot = newest.slot,
			age,
			max_age = MAX_LOOKUP_ANCHOR_AGE,
			safety_margin = LOOKUP_ANCHOR_SAFETY_MARGIN,
			"The newest finalized block would expire before the package carrying it could be
			 reported; skipping this tick.",
		);
		return None;
	}

	tracing::debug!(
		target: LOG_TARGET,
		anchor = ?anchor.header_hash,
		anchor_slot = anchor.slot,
		lookup_anchor = ?newest.header_hash,
		lookup_anchor_slot = newest.slot,
		age,
		"Chose the newest finalized block as the lookup anchor.",
	);
	Some(newest)
}

/// The 31-octet JAM state key of a para's head entry in the parachain service's storage.
///
/// Three parties must derive this identically — the collator asking for a proof, the node
/// serving it and the service verifying it in-core — so both halves come from shared code:
/// the service-local key from the facade, the state-key merklization from `jam-state-helpers`.
fn para_head_state_key(service_id: ServiceId, para_id: u32) -> StateKey {
	service_value_state_key(service_id, &para_info_key(para_id.into()))
}

/// Fetch a proof of the para's head **and its `parachain_log`** at `anchor`, checked against that
/// anchor's state root.
///
/// Returns the proof to carry in the block together with the head it proves; `None` means the
/// proof shows the para has no head yet, which is how a first block is recognised. Verifying
/// with the very code the service runs means a proof refine would reject never leaves the node.
///
/// Both keys are proven in one proof. The runtime reads its own `parachain_log` during block
/// execution — that is how the Coretime chain sees the service's `CoreNotAssignable` feedback —
/// and `JamProofReader::read` panics by design when a requested key is not authenticated. A
/// proof covering only the head key would therefore make every block that reads the log an
/// invalid candidate, so the collator has to carry the log's nodes from the start. The two
/// proofs are merged by node set: the nodes of a trie proof are content-addressed, so a union is
/// a superset that verifies both keys, and `StateProof` documents extra nodes as harmless.
pub(crate) async fn fetch_anchor_state_proof<Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	state_root: &StateRootHash,
	service_id: ServiceId,
	para_id: u32,
) -> Result<(StateProof, Option<Vec<u8>>), String> {
	let key = para_head_state_key(service_id, para_id);
	let log_key =
		service_value_state_key(service_id, &parachain_service_core::para_log_key(para_id.into()));

	let mut proof = fetch_proof_for(jam, anchor, key).await?;
	if log_key != key {
		let log_proof = fetch_proof_for(jam, anchor, log_key).await?;
		merge_proof(&mut proof, log_proof);
	}

	let proved = verify_state_proof(&proof, state_root, &key)
		.map_err(|error| format!("the node's own state proof does not verify: {error:?}"))?;
	// The log's nodes are carried for the runtime's own read; proving it here means a proof that
	// cannot serve that read fails on the node instead of inside a candidate.
	verify_state_proof(&proof, state_root, &log_key).map_err(|error| {
		format!("the node's own state proof does not cover the para's log: {error:?}")
	})?;

	Ok((proof, proved))
}

/// The state proof of one key at `anchor`, converted to the form that travels in the PoV.
async fn fetch_proof_for<Jam: JamStateSource + ?Sized>(
	jam: &Jam,
	anchor: HeaderHash,
	key: StateKey,
) -> Result<StateProof, String> {
	let range_proof = jam
		.state_proof(anchor, StorageKey(key), StorageKey(key), PROOF_SIZE_LIMIT)
		.await
		.map_err(|error| format!("state proof: {error}"))?;
	// polkajam's `RangeProof` is a host-side JSON type with no SCALE codec at all, so the form
	// that travels in the PoV is `jam-state-helpers`' own; converting is the host's job.
	Ok(StateProof {
		nodes: range_proof.nodes.iter().map(|node| **node).collect(),
		values: range_proof.values.iter().map(|(key, value)| (**key, value.to_vec())).collect(),
	})
}

/// Fold `extra`'s nodes and value preimages into `proof`.
///
/// Nodes are content-addressed, so duplicates are dropped by hash and the union verifies every
/// key either proof covered. Value preimages are keyed by their state key for the same reason.
fn merge_proof(proof: &mut StateProof, extra: StateProof) {
	for node in extra.nodes {
		if !proof.nodes.iter().any(|existing| existing == &node) {
			proof.nodes.push(node);
		}
	}
	for (key, value) in extra.values {
		if !proof.values.iter().any(|(existing, _)| existing == &key) {
			proof.values.push((key, value));
		}
	}
}

/// Register the JAM state reader and the additional-data finalizer over a verified anchor proof.
///
/// One [`StateProof`] backs both the read side — [`JamStateExt`], so the runtime's
/// `jam_state_read` serves reads during block execution — and the digest side — `AdditionalDataExt`
/// under [`JAM_PROOF_KEY`], so `frame_executive` deposits the `DigestItem::AdditionalData`
/// committing `hash_value((anchor_state_root, proof).encode())`. Returns the additional-data map
/// to carry in the PoV: the `JAM_PROOF_KEY` entry holding the encoded tuple.
pub(crate) fn register_jam_state_reader(
	extra_extensions: &mut Extensions,
	anchor_state_root: [u8; 32],
	proof: StateProof,
) -> AdditionalData {
	let encoded = (anchor_state_root, &proof).encode();
	let reader = Arc::new(JamProofReader::new(anchor_state_root, proof));
	extra_extensions.register(JamStateExt(Box::new(reader)));
	extra_extensions.register(AdditionalDataExt(
		[(
			JAM_PROOF_KEY.to_string(),
			Box::new(JamProofFinalizer { commitment: hash_value(&encoded) })
				as Box<dyn AdditionalDataFinalizer>,
		)]
		.into(),
	));
	core::iter::once((JAM_PROOF_KEY.to_string(), encoded)).collect()
}
