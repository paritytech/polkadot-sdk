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

//! The speculative-messaging part of a candidate's UMP signal tail, assembled by `validate_block`.
//!
//! Blocks emit `Provides` (once per PoV, buffered by the pallet until the last block) and never
//! `Requires`. This pass takes the `Provides` through under the wrapper's usual rule, at most one
//! of each signal, and synthesizes the candidate's `Requires` from the blocks' consumption records
//! and the PoV-carried lifts: one code path for steady state, partial consumption, resubmission
//! and bundles.
//!
//! Any verification failure panics, invalidating the candidate. The wrapper has no relay state, so
//! it cannot judge staleness; window matching stays relay-side at inclusion. Its rule is
//! mechanical: one lift per recorded stream, verified, roots converging per source.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_primitives_core::relay_chain::UMPSignal;
use cumulus_primitives_spec_messaging::{
	build_requires, ConsumptionRecord, LiftsBySource, StreamsRoot,
};
use frame_support::{traits::Get, BoundedVec};
use polkadot_primitives::RequiresSet;

/// The speculative-messaging part of a candidate's UMP signal tail. Built once per candidate, on
/// the plain and the scheduling-override path alike: a `signed_scheduling_info` replaces only the
/// scheduling signals.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SpecMessagingSignals {
	provides: Option<StreamsRoot>,
	requires: Option<RequiresSet>,
}

impl SpecMessagingSignals {
	/// Take the bundle's `Provides` and synthesize `Requires` from the bundle-ordered consumption
	/// records and the PoV-carried lifts.
	///
	/// Panics, invalidating the candidate, on a second `Provides`, a block-emitted `Requires`, or
	/// any lift failure.
	pub fn build(
		raw: &[Vec<u8>],
		records: &[ConsumptionRecord],
		lifts: Option<&LiftsBySource>,
	) -> Self {
		let provides = parse_provides(raw);

		let no_lifts = LiftsBySource::default();
		let requires = match build_requires(records, lifts.unwrap_or(&no_lifts)) {
			Ok(requires) => requires,
			Err(error) => panic!("Speculative Messaging `Requires` synthesis failed: {:?}", error),
		};

		Self { provides, requires }
	}

	/// Whether there is nothing to emit.
	pub(crate) fn is_empty(&self) -> bool {
		self.provides.is_none() && self.requires.is_none()
	}

	/// Push the signals, `Provides` then `Requires`. The caller owns the `UMP_SEPARATOR`.
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

/// The candidate's `Provides`, from the encoded `UMPSignal`s the PoV's blocks emitted. The pallet
/// emits it once per PoV, on the last block, so a second one is a bug and panics like any other
/// repeated signal. Runs on every candidate, so the "blocks never emit `Requires`" rule lives here
/// rather than in the scheduling parse, which a `signed_scheduling_info` skips.
fn parse_provides(raw: &[Vec<u8>]) -> Option<StreamsRoot> {
	let mut provides = None;
	for bytes in raw {
		match UMPSignal::decode(&mut &bytes[..]).expect("Failed to decode `UMPSignal`") {
			UMPSignal::Provides(root) => {
				if provides.replace(root).is_some() {
					panic!("Parachain emitted more than one `Provides` UMP signal");
				}
			},
			// `Requires` is synthesized here and must never be block-emitted.
			UMPSignal::Requires(_) => panic!("Parachain block emitted a `Requires` UMP signal"),
			// Scheduling signals belong to `SchedulingSignals`.
			UMPSignal::SelectCore(..) | UMPSignal::ApprovedPeer(..) => {},
		}
	}
	provides
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::collections::BTreeMap;
	use cumulus_primitives_core::relay_chain::Hash as RHash;
	use cumulus_primitives_spec_messaging::{
		streams_root::{gen_stream_proof, streams_root},
		ConsumptionRecord, Interval, MMRExtensionProof, MmrFrontier, MmrRoot, RequiresLift,
		StreamId,
	};
	use polkadot_primitives::{ClaimQueueOffset, CoreSelector, Id as ParaId};

	fn root(byte: u8) -> StreamsRoot {
		StreamsRoot(RHash::repeat_byte(byte))
	}

	/// A source that committed one channel stream of `n` messages: the frontier the receiver
	/// consumed up to, the source's `StreamsRoot`, and the identity lift that binds them.
	fn source_fixture(
		source: ParaId,
		stream: StreamId,
		n: u64,
	) -> (ConsumptionRecord, LiftsBySource, StreamsRoot) {
		let mut frontier = MmrFrontier::new();
		for i in 0..n {
			frontier.append(RHash::from_low_u64_be(i + 1));
		}
		let entries = BTreeMap::from([(stream, frontier.root().0)]);
		let committed = streams_root(&entries).unwrap();
		let (_, tree_proof) = gen_stream_proof(&entries, stream).unwrap();
		let record = ConsumptionRecord {
			entries: BTreeMap::from([(
				source,
				BTreeMap::from([(
					stream,
					Interval { start: MmrRoot(RHash::zero()), end: frontier },
				)]),
			)]),
		};
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MMRExtensionProof::identity(),
			tree_proof,
		};
		let lifts = LiftsBySource::try_from(BTreeMap::from([(source, vec![lift])])).unwrap();
		(record, lifts, committed)
	}

	#[test]
	fn provides_is_taken_through_and_scheduling_signals_are_ignored() {
		let raw = vec![
			UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0)).encode(),
			UMPSignal::Provides(root(2)).encode(),
		];
		let signals = SpecMessagingSignals::build(&raw, &[], None);
		assert_eq!(signals.provides, Some(root(2)));
		assert_eq!(signals.requires, None);
	}

	#[test]
	#[should_panic(expected = "more than one `Provides` UMP signal")]
	fn second_provides_invalidates() {
		// The pallet emits one `Provides` per PoV; two means a runtime bug, not a fold.
		let raw =
			vec![UMPSignal::Provides(root(1)).encode(), UMPSignal::Provides(root(2)).encode()];
		SpecMessagingSignals::build(&raw, &[], None);
	}

	#[test]
	fn all_idle_bundle_emits_nothing() {
		let signals = SpecMessagingSignals::build(&[], &[], None);
		assert!(signals.is_empty());

		let mut out = BoundedVec::<Vec<u8>, frame_support::traits::ConstU32<16>>::default();
		signals.emit_into(&mut out);
		assert!(out.is_empty());
	}

	#[test]
	#[should_panic(expected = "emitted a `Requires` UMP signal")]
	fn block_emitted_requires_invalidates() {
		let requires = RequiresSet::try_from_iter([(ParaId::from(1), root(1))]).unwrap();
		let raw = vec![UMPSignal::Requires(requires).encode()];
		SpecMessagingSignals::build(&raw, &[], None);
	}

	#[test]
	#[should_panic(expected = "`Requires` synthesis failed")]
	fn failed_synthesis_invalidates() {
		// A consumption record without its lift: `LiftSourceMismatch` must invalidate.
		let stream = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let (record, _, _) = source_fixture(2000.into(), stream, 5);
		SpecMessagingSignals::build(&[], &[record], None);
	}

	#[test]
	fn synthesized_requires_matches_the_committed_root() {
		// Record and lift in, `Requires` naming the source's committed root out, with `Provides`
		// before `Requires` in the tail bytes.
		let stream = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let source = ParaId::from(2000);
		let (record, lifts, committed) = source_fixture(source, stream, 5);

		let raw = vec![UMPSignal::Provides(root(9)).encode()];
		let signals = SpecMessagingSignals::build(&raw, &[record], Some(&lifts));

		let expected = RequiresSet::try_from_iter([(source, committed)]).unwrap();
		assert_eq!(signals.requires, Some(expected.clone()));

		let mut out = BoundedVec::<Vec<u8>, frame_support::traits::ConstU32<16>>::default();
		signals.emit_into(&mut out);
		assert_eq!(
			out.into_inner(),
			vec![UMPSignal::Provides(root(9)).encode(), UMPSignal::Requires(expected).encode()]
		);
	}
}
