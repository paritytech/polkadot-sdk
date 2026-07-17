// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: Apache-2.0

//! Speculative Messaging signal assembly for `validate_block`.
//!
//! Blocks emit only `Provides(StreamsRoot)` — one hash, from the block-end
//! tree fold — and the last provides-emitting block's root is the
//! candidate's `Provides` (streams are append-only, the newest root
//! supersedes; intermediate bundle roots never reach the relay chain).
//!
//! `Requires` is NEVER block-emitted: blocks write a consumption record,
//! and the wrapper stitches the per-stream intervals across the bundle,
//! verifies one POV-carried lift per recorded stream and synthesizes the
//! candidate's entries (`cumulus_primitives_spec_messaging::build_requires`
//! — the verification itself lives with the primitives). One code path for
//! steady state, partial consumption, resubmission and bundles.
//!
//! Any verification failure panics, invalidating the candidate. The wrapper
//! cannot judge staleness (no relay state here; window matching stays
//! relay-side at inclusion) — its rule is mechanical: one lift per recorded
//! stream, verified, roots converging per source.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_primitives_core::relay_chain::UMPSignal;
use cumulus_primitives_spec_messaging::{
	build_requires, ConsumptionRecord, LiftsBySource, RequiresSet, StreamsRoot,
};
use frame_support::{traits::Get, BoundedVec};

/// The Speculative Messaging part of a candidate's UMP signal tail.
///
/// Built once per candidate — on both the plain and the
/// scheduling-override path: a `signed_scheduling_info` replaces only the
/// *scheduling* signals, never the messaging commitments.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SpecMessagingSignals {
	provides: Option<StreamsRoot>,
	requires: Option<RequiresSet>,
}

impl SpecMessagingSignals {
	/// Folds the bundle's block-emitted `Provides` signals (last emitter
	/// wins; blocks without the signal compose freely) and synthesizes the
	/// `Requires` set from the bundle-ordered consumption records and the
	/// POV-carried lifts.
	///
	/// Panics — invalidating the candidate — on a block-emitted `Requires`
	/// or any lift verification failure.
	pub fn build(
		raw: &[Vec<u8>],
		records: &[ConsumptionRecord],
		lifts: Option<&LiftsBySource>,
	) -> Self {
		let provides = fold_provides(raw);

		let no_lifts = LiftsBySource::default();
		let requires = match build_requires(records, lifts.unwrap_or(&no_lifts)) {
			Ok(requires) => requires,
			Err(error) => panic!("Speculative Messaging `Requires` synthesis failed: {:?}", error),
		};

		Self { provides, requires }
	}

	/// `true` when there is nothing to emit.
	pub(crate) fn is_empty(&self) -> bool {
		self.provides.is_none() && self.requires.is_none()
	}

	/// Pushes the signals — `Provides` then `Requires` — into the
	/// candidate's message vec. The caller (the scheduling tail emission)
	/// owns the `UMP_SEPARATOR`.
	pub(crate) fn emit_into<S: Get<u32>>(self, upward_messages: &mut BoundedVec<Vec<u8>, S>) {
		if let Some(root) = self.provides {
			upward_messages
				.try_push(UMPSignal::Provides(root).encode())
				.expect("UMPSignals does not fit in UMPMessages");
		}
		if let Some(requires) = self.requires {
			upward_messages
				.try_push(UMPSignal::Requires(requires).encode())
				.expect("UMPSignals does not fit in UMPMessages");
		}
	}
}

/// Folds the encoded `UMPSignal`s the PoV's blocks emitted after their
/// in-block `UMP_SEPARATOR`s: the last provides-emitting block's root is
/// the candidate's `Provides`.
///
/// This pass runs on EVERY candidate (unlike the scheduling parse, which a
/// `signed_scheduling_info` override skips), so the permanent "blocks must
/// never emit `Requires`" rejection lives here.
fn fold_provides(raw: &[Vec<u8>]) -> Option<StreamsRoot> {
	let mut provides = None;
	for bytes in raw {
		match UMPSignal::decode(&mut &bytes[..]).expect("Failed to decode `UMPSignal`") {
			UMPSignal::Provides(root) => provides = Some(root),
			// `Requires` is synthesized by this wrapper and must NEVER be
			// block-emitted — a block that does so is buggy or malicious
			// either way.
			UMPSignal::Requires(_) => {
				panic!("Parachain block emitted a `Requires` UMP signal")
			},
			// Scheduling signals are handled by the scheduling pass
			// (`scheduling::SchedulingSignals`).
			UMPSignal::SelectCore(..) | UMPSignal::ApprovedPeer(..) => {},
		}
	}
	provides
}

#[cfg(test)]
mod tests {
	use super::*;
	use cumulus_primitives_core::relay_chain::Hash as RHash;
	use cumulus_primitives_spec_messaging::{
		test_utils::{record, SourceFixture, StreamFixture},
		StreamId,
	};
	use polkadot_primitives::{ClaimQueueOffset, CoreSelector};

	fn root(byte: u8) -> StreamsRoot {
		StreamsRoot(RHash::repeat_byte(byte))
	}

	#[test]
	fn provides_fold_takes_the_last_emitting_block() {
		// Bundle of three blocks: first and third touched streams, the
		// second was idle. The candidate's `Provides` is the third's root;
		// scheduling signals in between are ignored by this pass.
		let raw = vec![
			UMPSignal::Provides(root(1)).encode(),
			UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0)).encode(),
			UMPSignal::Provides(root(2)).encode(),
		];
		let signals = SpecMessagingSignals::build(&raw, &[], None);
		assert_eq!(signals.provides, Some(root(2)));
		assert_eq!(signals.requires, None);
	}

	#[test]
	fn all_idle_bundle_emits_no_provides() {
		let signals = SpecMessagingSignals::build(&[], &[], None);
		assert!(signals.is_empty());

		let mut out = BoundedVec::<Vec<u8>, frame_support::traits::ConstU32<16>>::default();
		signals.emit_into(&mut out);
		assert!(out.is_empty());
	}

	#[test]
	#[should_panic(expected = "emitted a `Requires` UMP signal")]
	fn block_emitted_requires_invalidates() {
		let raw = vec![UMPSignal::Requires(RequiresSet::default()).encode()];
		SpecMessagingSignals::build(&raw, &[], None);
	}

	#[test]
	#[should_panic(expected = "`Requires` synthesis failed")]
	fn failed_synthesis_invalidates() {
		// A consumption record without its lift: `SourceMismatch` inside
		// `build_requires` must invalidate the candidate.
		let id = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let source = SourceFixture::new(2000.into(), vec![StreamFixture::new(id, 5)]);
		let records = [record([(source.para, id, source.stream(&id).interval(0, 5))])];
		SpecMessagingSignals::build(&[], &records, None);
	}

	#[test]
	fn synthesized_requires_matches_the_committed_root() {
		// End to end through the wrapper's entry point: record + lift in,
		// `Requires` naming the sender's committed root out — and the tail
		// bytes carry `Provides` before `Requires`.
		let id = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let source = SourceFixture::new(2000.into(), vec![StreamFixture::new(id, 5)]);
		let records = [record([(source.para, id, source.stream(&id).interval(0, 5))])];
		let lifts =
			LiftsBySource::try_from_iter([(source.para, vec![source.lift(&id, 5, &[])])]).unwrap();

		let raw = vec![UMPSignal::Provides(root(9)).encode()];
		let signals = SpecMessagingSignals::build(&raw, &records, Some(&lifts));

		let expected = RequiresSet::try_from_iter([(source.para, source.streams_root())]).unwrap();
		assert_eq!(signals.requires, Some(expected.clone()));

		let mut out = BoundedVec::<Vec<u8>, frame_support::traits::ConstU32<16>>::default();
		signals.emit_into(&mut out);
		assert_eq!(
			out.into_inner(),
			vec![UMPSignal::Provides(root(9)).encode(), UMPSignal::Requires(expected).encode(),]
		);
	}
}
