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

//! The speculative-messaging part of a candidate's UMP signal tail, and the assembly of the whole
//! tail.
//!
//! Blocks emit `Provides` (once per PoV, buffered by the pallet until the last block) and never
//! `Requires`. [`SpecMessagingSignals::build`] takes the `Provides` through under the usual rule,
//! at most one of each signal, and synthesizes the candidate's `Requires` from the blocks'
//! consumption records and the PoV-carried lifts: one code path for steady state, partial
//! consumption, resubmission and bundles.
//!
//! Single source of truth shared by the collator and the PVF (`validate_block`), like
//! [`SchedulingSignals`]: both assemble the tail with [`ump_signal_tail`], so their commitments
//! can't drift. The PVF panics on any [`SpecMessagingError`], invalidating the candidate; the
//! collator skips the collation instead. Neither has relay state, so neither judges staleness;
//! window matching stays relay-side at inclusion.

use crate::scheduling::SchedulingSignals;
use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_primitives_spec_messaging::{
	build_requires, ConsumptionRecord, LiftError, LiftsBySource, StreamsRoot,
};
use polkadot_primitives::{RequiresSet, UMPSignal, UMP_SEPARATOR};

/// Why the speculative-messaging part of a candidate's tail cannot be built. Each is deterministic
/// in the blocks' signals, the records and the PoV, so every validator reaches the same verdict.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpecMessagingError {
	/// A block emitted bytes that do not decode as a `UMPSignal`.
	UndecodableSignal,
	/// The PoV's blocks emitted more than one `Provides`.
	DuplicateProvides,
	/// A block emitted `Requires`, which only the tail assembly may produce.
	BlockEmittedRequires,
	/// The consumption records and the lifts do not synthesize a `Requires`.
	RequiresSynthesis(LiftError),
}

impl core::fmt::Display for SpecMessagingError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			Self::UndecodableSignal => write!(f, "Failed to decode `UMPSignal`"),
			Self::DuplicateProvides => {
				write!(f, "Parachain emitted more than one `Provides` UMP signal")
			},
			Self::BlockEmittedRequires => {
				write!(f, "Parachain block emitted a `Requires` UMP signal")
			},
			Self::RequiresSynthesis(error) => {
				write!(f, "Speculative Messaging `Requires` synthesis failed: {:?}", error)
			},
		}
	}
}

/// The speculative-messaging part of a candidate's UMP signal tail. Built once per candidate, on
/// the plain and the scheduling-override path alike: a `signed_scheduling_info` replaces only the
/// scheduling signals.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct SpecMessagingSignals {
	provides: Option<StreamsRoot>,
	requires: Option<RequiresSet>,
}

impl SpecMessagingSignals {
	/// Take the bundle's `Provides` from the blocks' encoded `UMPSignal`s, and synthesize
	/// `Requires` from the bundle-ordered consumption records and the PoV-carried lifts.
	pub fn build(
		raw: &[Vec<u8>],
		records: &[ConsumptionRecord],
		lifts: Option<&LiftsBySource>,
	) -> Result<Self, SpecMessagingError> {
		let provides = parse_provides(raw)?;

		let no_lifts = LiftsBySource::default();
		let requires = build_requires(records, lifts.unwrap_or(&no_lifts))
			.map_err(SpecMessagingError::RequiresSynthesis)?;

		Ok(Self { provides, requires })
	}

	/// Whether there is nothing to emit.
	pub fn is_empty(&self) -> bool {
		self.provides.is_none() && self.requires.is_none()
	}

	/// The encoded signals, `Provides` then `Requires`, without a `UMP_SEPARATOR`.
	fn into_ump_messages(self) -> Vec<Vec<u8>> {
		let mut messages = Vec::with_capacity(2);
		if let Some(root) = self.provides {
			messages.push(UMPSignal::Provides(root).encode());
		}
		if let Some(requires) = self.requires {
			messages.push(UMPSignal::Requires(requires).encode());
		}
		messages
	}
}

/// The candidate's complete UMP signal tail: one `UMP_SEPARATOR`, then `SelectCore`,
/// `ApprovedPeer`, `Provides`, `Requires`, each only if present. Empty, not even a separator, when
/// there are no signals, since the relay decoder keys off the first `UMP_SEPARATOR`.
pub fn ump_signal_tail(
	scheduling: SchedulingSignals,
	spec_messaging: SpecMessagingSignals,
) -> Vec<Vec<u8>> {
	// `into_ump_messages` already starts with the separator unless the scheduling part is empty.
	let mut tail = scheduling.into_ump_messages();
	if tail.is_empty() && !spec_messaging.is_empty() {
		tail.push(UMP_SEPARATOR);
	}
	tail.extend(spec_messaging.into_ump_messages());
	tail
}

/// The candidate's `Provides`, from the encoded `UMPSignal`s the PoV's blocks emitted. The pallet
/// emits it once per PoV, on the last block, so a second one is a bug, like any other repeated
/// signal. Runs on every candidate, so the "blocks never emit `Requires`" rule lives here rather
/// than in the scheduling parse, which a `signed_scheduling_info` skips.
fn parse_provides(raw: &[Vec<u8>]) -> Result<Option<StreamsRoot>, SpecMessagingError> {
	let mut provides = None;
	for bytes in raw {
		match UMPSignal::decode(&mut &bytes[..])
			.map_err(|_| SpecMessagingError::UndecodableSignal)?
		{
			UMPSignal::Provides(root) => {
				if provides.replace(root).is_some() {
					return Err(SpecMessagingError::DuplicateProvides);
				}
			},
			// `Requires` is synthesized here and must never be block-emitted.
			UMPSignal::Requires(_) => return Err(SpecMessagingError::BlockEmittedRequires),
			// Scheduling signals belong to `SchedulingSignals`.
			UMPSignal::SelectCore(..) | UMPSignal::ApprovedPeer(..) => {},
		}
	}
	Ok(provides)
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::{collections::BTreeMap, vec};
	use cumulus_primitives_spec_messaging::{
		streams_root::{gen_stream_proof, streams_root},
		Interval, MMRExtensionProof, MmrFrontier, MmrRoot, RequiresLift, StreamId,
	};
	use polkadot_primitives::{ClaimQueueOffset, CoreSelector, Hash as RHash, Id as ParaId};

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

	fn select_core() -> UMPSignal {
		UMPSignal::SelectCore(CoreSelector(0), ClaimQueueOffset(0))
	}

	#[test]
	fn provides_is_taken_through_and_scheduling_signals_are_ignored() {
		let raw = vec![select_core().encode(), UMPSignal::Provides(root(2)).encode()];
		let signals = SpecMessagingSignals::build(&raw, &[], None).unwrap();
		assert_eq!(signals.provides, Some(root(2)));
		assert_eq!(signals.requires, None);
	}

	#[test]
	fn second_provides_is_rejected() {
		// The pallet emits one `Provides` per PoV; two means a runtime bug, not a fold.
		let raw =
			vec![UMPSignal::Provides(root(1)).encode(), UMPSignal::Provides(root(2)).encode()];
		assert_eq!(
			SpecMessagingSignals::build(&raw, &[], None),
			Err(SpecMessagingError::DuplicateProvides)
		);
	}

	#[test]
	fn all_idle_bundle_emits_nothing() {
		let signals = SpecMessagingSignals::build(&[], &[], None).unwrap();
		assert!(signals.is_empty());
		assert!(ump_signal_tail(SchedulingSignals::default(), signals).is_empty());
	}

	#[test]
	fn block_emitted_requires_is_rejected() {
		let requires = RequiresSet::try_from_iter([(ParaId::from(1), root(1))]).unwrap();
		let raw = vec![UMPSignal::Requires(requires).encode()];
		assert_eq!(
			SpecMessagingSignals::build(&raw, &[], None),
			Err(SpecMessagingError::BlockEmittedRequires)
		);
	}

	#[test]
	fn undecodable_signal_is_rejected() {
		assert_eq!(
			SpecMessagingSignals::build(&[vec![0xff]], &[], None),
			Err(SpecMessagingError::UndecodableSignal)
		);
	}

	#[test]
	fn failed_synthesis_is_rejected() {
		// A consumption record without its lift must not synthesize.
		let stream = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let (record, _, _) = source_fixture(2000.into(), stream, 5);
		assert!(matches!(
			SpecMessagingSignals::build(&[], &[record], None),
			Err(SpecMessagingError::RequiresSynthesis(_))
		));
	}

	#[test]
	fn synthesized_requires_matches_the_committed_root() {
		// Record and lift in, `Requires` naming the source's committed root out, with `Provides`
		// before `Requires` in the tail bytes.
		let stream = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let source = ParaId::from(2000);
		let (record, lifts, committed) = source_fixture(source, stream, 5);

		let raw = vec![UMPSignal::Provides(root(9)).encode()];
		let signals = SpecMessagingSignals::build(&raw, &[record], Some(&lifts)).unwrap();

		let expected = RequiresSet::try_from_iter([(source, committed)]).unwrap();
		assert_eq!(signals.requires, Some(expected.clone()));
		assert_eq!(
			ump_signal_tail(SchedulingSignals::default(), signals),
			vec![
				UMP_SEPARATOR,
				UMPSignal::Provides(root(9)).encode(),
				UMPSignal::Requires(expected).encode()
			]
		);
	}

	#[test]
	fn tail_has_one_separator_and_canonical_order() {
		// Scheduling and spec-messaging parts together: one separator, then `SelectCore`,
		// `Provides`.
		let raw = vec![UMPSignal::Provides(root(4)).encode(), select_core().encode()];
		let tail = ump_signal_tail(
			SchedulingSignals::from_block_signals(&raw),
			SpecMessagingSignals::build(&raw, &[], None).unwrap(),
		);
		assert_eq!(
			tail,
			vec![UMP_SEPARATOR, select_core().encode(), UMPSignal::Provides(root(4)).encode()]
		);
	}

	#[test]
	fn error_messages_are_stable() {
		// `validate_block` panics with these; tests and operators match on them.
		assert_eq!(
			SpecMessagingError::DuplicateProvides.to_string(),
			"Parachain emitted more than one `Provides` UMP signal"
		);
		assert_eq!(
			SpecMessagingError::BlockEmittedRequires.to_string(),
			"Parachain block emitted a `Requires` UMP signal"
		);
		assert!(SpecMessagingError::RequiresSynthesis(LiftError::LiftCountMismatch)
			.to_string()
			.starts_with("Speculative Messaging `Requires` synthesis failed"));
	}
}
