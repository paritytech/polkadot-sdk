// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! V3 scheduling types for low-latency parachain block production.
//!
//! V3 candidates separate the relay parent (execution context) from the scheduling parent (a recent
//! relay tip used for core assignment), so blocks can build on older relay parents while still
//! being scheduled off recent relay state.
//!
//! # Resubmission
//!
//! If a candidate isn't backed in time, another collator can resubmit it with a fresh
//! `scheduling_parent` — same `relay_parent`, no re-execution — by providing a
//! `signed_scheduling_info` proving it is the eligible author for the slot at
//! `internal_scheduling_parent`.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use polkadot_primitives::{
	ApprovedPeerId, ClaimQueueOffset, CoreSelector, Header as RelayChainHeader, Slot, UMPSignal,
	UMP_SEPARATOR,
};
use sp_runtime::traits::{BlakeTwo256, Hash as HashT};

/// Payload a collator signs to resubmit a candidate.
///
/// Binds the core selection and credited peer to an internal scheduling parent, preventing replay
/// across scheduling contexts.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SchedulingInfoPayload {
	/// Which core to use (indexes into the parachain's assigned cores).
	pub core_selector: CoreSelector,
	/// The claim queue offset.
	pub claim_queue_offset: u8,
	/// Peer ID credited for successful collation delivery.
	pub peer_id: ApprovedPeerId,
	/// The internal scheduling parent whose slot decides the eligible author that must sign this
	/// payload.
	pub internal_scheduling_parent: polkadot_primitives::Hash,
}

/// Signed scheduling info for candidate resubmission: a [`SchedulingInfoPayload`] plus the
/// collator's signature over it, proving eligibility for the slot at `internal_scheduling_parent`.
///
/// `claim_queue_offset` comes from the runtime's `relay_parent_offset`, not this struct, so the
/// collator cannot override it.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SignedSchedulingInfo {
	/// The scheduling information.
	pub payload: SchedulingInfoPayload,
	/// The eligible collator's signature over the SCALE-encoded [`SchedulingInfoPayload`].
	///
	/// A fixed 64-byte blob, decodable as either sr25519 or ed25519 (both are 64 bytes).
	pub signature: [u8; 64],
}

impl SchedulingInfoPayload {
	/// Create a new scheduling info payload.
	pub fn new(
		core_selector: CoreSelector,
		claim_queue_offset: u8,
		peer_id: ApprovedPeerId,
		internal_scheduling_parent: polkadot_primitives::Hash,
	) -> Self {
		Self { core_selector, claim_queue_offset, peer_id, internal_scheduling_parent }
	}
}

/// The scheduling-signal tail (`SelectCore`/`ApprovedPeer`) a candidate emits after the first
/// `UMP_SEPARATOR`.
///
/// Single source of truth shared by the collator and the PVF (`validate_block`) so their tails
/// can't drift. The relay decoder (`CandidateCommitments::ump_signals`) rejects a repeated variant
/// or any third signal, and parses only the run after the first `UMP_SEPARATOR`.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SchedulingSignals {
	select_core: Option<(CoreSelector, ClaimQueueOffset)>,
	approved_peer: Option<ApprovedPeerId>,
}

impl SchedulingSignals {
	/// Parse the encoded `UMPSignal`s a PoV's blocks emitted after the in-block `UMP_SEPARATOR`.
	///
	/// Panics on a repeated variant even when the values match: the relay decoder counts
	/// occurrences, not distinct values, so a duplicate is a bug regardless.
	pub fn from_block_signals(raw: &[Vec<u8>]) -> Self {
		let mut signals = Self::default();
		for bytes in raw {
			// Exhaustive on purpose (no `_`): a new `UMPSignal` variant must fail to compile
			// here, forcing a deliberate decision rather than being silently dropped.
			match UMPSignal::decode(&mut &bytes[..]).expect("Failed to decode `UMPSignal`") {
				UMPSignal::SelectCore(selector, offset) => {
					if signals.select_core.replace((selector, offset)).is_some() {
						panic!("Parachain emitted more than one `SelectCore` UMP signal");
					}
				},
				UMPSignal::ApprovedPeer(peer_id) => {
					if signals.approved_peer.replace(peer_id).is_some() {
						panic!("Parachain emitted more than one `ApprovedPeer` UMP signal");
					}
				},
			}
		}
		signals
	}

	/// Build the tail from a verified `SignedSchedulingInfo`, which wholesale replaces the block's
	/// own signals — the signer signed all three fields.
	pub fn from_scheduling_info(signed_info: &SignedSchedulingInfo) -> Self {
		let payload = &signed_info.payload;
		Self {
			select_core: Some((
				payload.core_selector,
				ClaimQueueOffset(payload.claim_queue_offset),
			)),
			approved_peer: Some(payload.peer_id.clone()),
		}
	}

	fn is_empty(&self) -> bool {
		self.select_core.is_none() && self.approved_peer.is_none()
	}

	/// The encoded UMP messages for this tail: empty when there are no signals, otherwise
	/// `[UMP_SEPARATOR, SelectCore?, ApprovedPeer?]`. Order is `SelectCore` then `ApprovedPeer`,
	/// matching `pallet_parachain_system::send_ump_signals`. Nothing — not even the separator — is
	/// emitted when empty, since the relay decoder keys off the first `UMP_SEPARATOR`.
	pub fn into_ump_messages(self) -> Vec<Vec<u8>> {
		if self.is_empty() {
			return Vec::new();
		}
		let mut messages = Vec::with_capacity(3);
		messages.push(UMP_SEPARATOR);
		if let Some((selector, offset)) = self.select_core {
			messages.push(UMPSignal::SelectCore(selector, offset).encode());
		}
		if let Some(peer_id) = self.approved_peer {
			messages.push(UMPSignal::ApprovedPeer(peer_id).encode());
		}
		messages
	}
}

/// V3 scheduling proof included in the PoV.
///
/// Proves ancestry from `scheduling_parent` back to the internal scheduling parent; the PVF
/// validates it against the `relay_parent`/`scheduling_parent` in the candidate descriptor.
#[derive(Clone, Encode, Decode, Debug, PartialEq, Eq)]
pub struct SchedulingProof {
	/// Relay chain headers chaining `scheduling_parent` backward: each header's `parent_hash` is
	/// the next header's hash. The first header hashes to the candidate's `scheduling_parent`, the
	/// last header's `parent_hash` is the internal scheduling parent. Length is the runtime's
	/// `RelayParentOffset`.
	pub header_chain: Vec<RelayChainHeader>,
	/// The header at `internal_scheduling_parent`; its hash must equal the internal scheduling
	/// parent derived from `header_chain` (the last header's `parent_hash`, or `scheduling_parent`
	/// if the chain is empty).
	pub internal_scheduling_parent_header: RelayChainHeader,
	/// Optional signed core-selection override:
	///
	/// - `None`, `relay_parent == internal_scheduling_parent`: initial submission; core selection
	///   comes from the block's UMP signals.
	/// - `Some`, `relay_parent == internal_scheduling_parent`: initial submission with an explicit
	///   (optional) core selection.
	/// - `Some`, `relay_parent != internal_scheduling_parent`: resubmission (required); the
	///   signature overrides the block's UMP signals and is verified against the eligible author
	///   for the slot at `internal_scheduling_parent`.
	pub signed_scheduling_info: Option<SignedSchedulingInfo>,
}

impl SchedulingProof {
	/// Create a new scheduling proof.
	pub fn new(
		header_chain: Vec<RelayChainHeader>,
		internal_scheduling_parent_header: RelayChainHeader,
		signed_scheduling_info: Option<SignedSchedulingInfo>,
	) -> Self {
		Self { header_chain, internal_scheduling_parent_header, signed_scheduling_info }
	}

	/// The scheduling parent hash: the first/newest header in `header_chain`, or
	/// `internal_scheduling_parent_header.hash()` when the chain is empty (they coincide at
	/// `relay_parent_offset = 0`).
	pub fn scheduling_parent(&self) -> polkadot_primitives::Hash {
		self.header_chain
			.first()
			.map(BlakeTwo256::hash_of)
			.unwrap_or_else(|| self.internal_scheduling_parent_header.hash())
	}
}

/// Verifier for V3 scheduling: reports whether V3 is enabled and verifies a candidate's
/// [`SignedSchedulingInfo`].
pub trait VerifySchedulingSignature {
	/// Whether V3 scheduling validation is enabled.
	const V3_SCHEDULING_ENABLED: bool;

	/// Verify `signed_info` against the author eligible at `relay_slot` (the internal scheduling
	/// parent's slot).
	fn verify(signed_info: &SignedSchedulingInfo, relay_slot: Slot) -> bool;
}

/// Default no-op wiring: V3 disabled, scheduling info accepted unconditionally. A real verifier
/// should also turn V3 on.
impl VerifySchedulingSignature for () {
	const V3_SCHEDULING_ENABLED: bool = false;

	fn verify(_signed_info: &SignedSchedulingInfo, _relay_slot: Slot) -> bool {
		true
	}
}

#[cfg(test)]
mod tests {
	use super::{SchedulingInfoPayload, SchedulingSignals, SignedSchedulingInfo};
	use alloc::vec;
	use codec::Encode;
	use polkadot_primitives::{
		ApprovedPeerId, ClaimQueueOffset, CoreSelector, UMPSignal, UMP_SEPARATOR,
	};

	fn peer(byte: u8) -> ApprovedPeerId {
		ApprovedPeerId::try_from(vec![byte; 4]).expect("4 bytes fits the bound; qed")
	}

	fn signed_with(
		core_selector: CoreSelector,
		claim_queue_offset: u8,
		peer_id: ApprovedPeerId,
	) -> SignedSchedulingInfo {
		SignedSchedulingInfo {
			payload: SchedulingInfoPayload::new(
				core_selector,
				claim_queue_offset,
				peer_id,
				Default::default(),
			),
			signature: [0u8; 64],
		}
	}

	#[test]
	fn from_block_signals_roundtrips_select_core_and_approved_peer() {
		// Both signals present: parsed, then emitted as [SEPARATOR, SelectCore, ApprovedPeer] in
		// that exact order.
		let raw = vec![
			UMPSignal::SelectCore(CoreSelector(7), ClaimQueueOffset(1)).encode(),
			UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
		];
		assert_eq!(
			SchedulingSignals::from_block_signals(&raw).into_ump_messages(),
			vec![
				UMP_SEPARATOR,
				UMPSignal::SelectCore(CoreSelector(7), ClaimQueueOffset(1)).encode(),
				UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
			]
		);
	}

	#[test]
	fn from_block_signals_select_core_only() {
		// Block emitted only a `SelectCore`: no `ApprovedPeer`, one signal emitted.
		let raw = vec![UMPSignal::SelectCore(CoreSelector(3), ClaimQueueOffset(0)).encode()];
		assert_eq!(
			SchedulingSignals::from_block_signals(&raw).into_ump_messages(),
			vec![
				UMP_SEPARATOR,
				UMPSignal::SelectCore(CoreSelector(3), ClaimQueueOffset(0)).encode()
			]
		);
	}

	#[test]
	#[should_panic(expected = "more than one `SelectCore`")]
	fn from_block_signals_panics_on_duplicate_select_core_same_value() {
		// Two identical `SelectCore` signals: still an error. The relay decoder counts
		// occurrences, not distinct values, so matching duplicates would be rejected too.
		let raw = vec![
			UMPSignal::SelectCore(CoreSelector(1), ClaimQueueOffset(0)).encode(),
			UMPSignal::SelectCore(CoreSelector(1), ClaimQueueOffset(0)).encode(),
		];
		let _ = SchedulingSignals::from_block_signals(&raw);
	}

	#[test]
	#[should_panic(expected = "more than one `SelectCore`")]
	fn from_block_signals_panics_on_duplicate_select_core_different_value() {
		let raw = vec![
			UMPSignal::SelectCore(CoreSelector(1), ClaimQueueOffset(0)).encode(),
			UMPSignal::SelectCore(CoreSelector(2), ClaimQueueOffset(0)).encode(),
		];
		let _ = SchedulingSignals::from_block_signals(&raw);
	}

	#[test]
	#[should_panic(expected = "more than one `ApprovedPeer`")]
	fn from_block_signals_panics_on_duplicate_approved_peer() {
		let raw = vec![
			UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
			UMPSignal::ApprovedPeer(peer(0xBB)).encode(),
		];
		let _ = SchedulingSignals::from_block_signals(&raw);
	}

	#[test]
	fn from_block_signals_empty_emits_nothing() {
		// No signals in, nothing out — not even a separator.
		assert!(SchedulingSignals::from_block_signals(&[]).into_ump_messages().is_empty());
	}

	#[test]
	fn from_scheduling_info_sources_all_fields() {
		// All three values — `core_selector`, `claim_queue_offset`, `peer_id` — are signed by the
		// resubmitting collator, so the override sources every field from the signed payload.
		// Distinct values ensure no field is sourced from the wrong place.
		let signed = signed_with(CoreSelector(7), 3, peer(0xAA));
		assert_eq!(
			SchedulingSignals::from_scheduling_info(&signed).into_ump_messages(),
			vec![
				UMP_SEPARATOR,
				UMPSignal::SelectCore(CoreSelector(7), ClaimQueueOffset(3)).encode(),
				UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
			]
		);
	}

	#[test]
	fn from_scheduling_info_emits_peer_verbatim_even_if_empty() {
		// The payload `peer_id` is a plain (non-`Option`) type → always emitted. An empty peer is
		// emitted verbatim as `ApprovedPeer([])`, NOT omitted and NOT replaced by the block's peer.
		let signed = signed_with(CoreSelector(5), 1, ApprovedPeerId::default());
		assert_eq!(
			SchedulingSignals::from_scheduling_info(&signed).into_ump_messages(),
			vec![
				UMP_SEPARATOR,
				UMPSignal::SelectCore(CoreSelector(5), ClaimQueueOffset(1)).encode(),
				UMPSignal::ApprovedPeer(ApprovedPeerId::default()).encode(),
			]
		);
	}

	#[test]
	fn override_matches_block_signals_when_values_agree() {
		// Given the same core info and peer id, the override emits the block's own tail byte for
		// byte. Resubmissions rely on this: the collator rebuilds the commitments from the signed
		// payload, and the PVF's override must land on identical bytes to pass the commitments
		// check at backing.
		let selector = CoreSelector(7);
		let offset = ClaimQueueOffset(3);
		let peer_id = peer(0xAA);

		let from_block = SchedulingSignals::from_block_signals(&[
			UMPSignal::SelectCore(selector, offset).encode(),
			UMPSignal::ApprovedPeer(peer_id.clone()).encode(),
		])
		.into_ump_messages();

		let from_signed =
			SchedulingSignals::from_scheduling_info(&signed_with(selector, offset.0, peer_id))
				.into_ump_messages();

		assert_eq!(from_block, from_signed);
	}

	#[test]
	fn from_scheduling_info_emits_even_when_block_emitted_nothing() {
		// The override is authoritative and independent of what the block emitted: a resubmission
		// always produces its tail.
		let signed = signed_with(CoreSelector(0), 0, peer(0xCC));
		assert!(!SchedulingSignals::from_scheduling_info(&signed).into_ump_messages().is_empty());
	}
}
