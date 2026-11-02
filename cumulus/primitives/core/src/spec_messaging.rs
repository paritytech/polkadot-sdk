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

//! The speculative-messaging part of a candidate's UMP signal tail.
//!
//! Blocks emit neither signal. Each block that touched a stream deposits its `StreamsRoot` as an
//! SPMS header digest, and the bundle's last one is its `Provides` ([`bundle_provides`]).
//! [`SpecMessagingSignals::build`] synthesizes `Requires` from the consumption records and the PoV
//! lifts. The collator and the PVF both assemble the tail with [`ump_signal_tail`], so they cannot
//! drift; the PVF panics on a [`SpecMessagingError`], the collator skips the collation.

use crate::scheduling::SchedulingSignals;
use alloc::vec::Vec;
use codec::{Decode, Encode};
use cumulus_primitives_spec_messaging::{
	build_requires, streams_root::read_streams_root, ConsumptionRecord, LiftError, LiftsBySource,
	StreamsRoot,
};
use polkadot_primitives::{RequiresSet, UMPSignal, UMP_SEPARATOR};
use sp_runtime::Digest;

/// Why the speculative-messaging part of a candidate's tail cannot be built. Each is deterministic
/// in the blocks' signals, the records and the PoV, so every validator reaches the same verdict.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SpecMessagingError {
	/// A block emitted bytes that do not decode as a `UMPSignal`.
	UndecodableSignal,
	/// A block emitted `Provides` or `Requires`, which only the tail assembly may produce.
	BlockEmittedSpecMsgSignal,
	/// The consumption records and the lifts do not synthesize a `Requires`.
	RequiresSynthesis(LiftError),
}

impl core::fmt::Display for SpecMessagingError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			Self::UndecodableSignal => write!(f, "Failed to decode `UMPSignal`"),
			Self::BlockEmittedSpecMsgSignal => {
				write!(f, "Parachain block emitted a speculative-messaging UMP signal")
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
	/// Pair the bundle's `provides` ([`bundle_provides`]) with `Requires` synthesized from the
	/// bundle-ordered consumption records and the PoV-carried lifts. `raw` are the blocks' encoded
	/// `UMPSignal`s, which must hold neither signal.
	pub fn build(
		provides: Option<StreamsRoot>,
		raw: &[Vec<u8>],
		records: &[ConsumptionRecord],
		lifts: Option<&LiftsBySource>,
	) -> Result<Self, SpecMessagingError> {
		reject_block_signals(raw)?;

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
	pub fn into_ump_messages(self) -> Vec<Vec<u8>> {
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

/// The bundle's `Provides`: the last `StreamsRoot` digest among its blocks' headers, in bundle
/// order. `None` if no block touched a stream.
pub fn bundle_provides<'a>(digests: impl IntoIterator<Item = &'a Digest>) -> Option<StreamsRoot> {
	digests.into_iter().filter_map(read_streams_root).last()
}

/// Blocks emit only scheduling signals. Runs on every candidate, unlike the scheduling parse an
/// override skips.
fn reject_block_signals(raw: &[Vec<u8>]) -> Result<(), SpecMessagingError> {
	for bytes in raw {
		match UMPSignal::decode(&mut &bytes[..])
			.map_err(|_| SpecMessagingError::UndecodableSignal)?
		{
			UMPSignal::Provides(_) | UMPSignal::Requires(_) => {
				return Err(SpecMessagingError::BlockEmittedSpecMsgSignal)
			},
			// Scheduling signals belong to `SchedulingSignals`.
			UMPSignal::SelectCore(..) | UMPSignal::ApprovedPeer(..) => {},
		}
	}
	Ok(())
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::{collections::BTreeMap, vec};
	use cumulus_primitives_spec_messaging::{
		streams_root::{gen_stream_proof, streams_root},
		Interval, MMRExtensionProof, MmrFrontier, MmrRoot, RequiresLift, StreamId, SPMS_ENGINE_ID,
	};
	use polkadot_primitives::{ClaimQueueOffset, CoreSelector, Hash as RHash, Id as ParaId};
	use sp_runtime::DigestItem;

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
				vec![(stream, Interval { start: MmrRoot(RHash::zero()), end: frontier })],
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

	fn digest(roots: &[StreamsRoot]) -> Digest {
		let spms = |r: &StreamsRoot| DigestItem::Consensus(SPMS_ENGINE_ID, r.encode());
		Digest { logs: roots.iter().map(spms).collect() }
	}

	#[test]
	fn provides_is_the_bundles_last_streams_root_digest() {
		let (sent, idle, resent) = (digest(&[root(1)]), digest(&[]), digest(&[root(3)]));
		assert_eq!(bundle_provides([&sent, &idle, &resent]), Some(root(3)));
		// An idle last block still reports the root an earlier block produced.
		assert_eq!(bundle_provides([&sent, &idle]), Some(root(1)));
		assert_eq!(bundle_provides([&idle, &idle]), None);
		// A header with two SPMS items carries no root.
		assert_eq!(bundle_provides([&sent, &digest(&[root(4), root(5)])]), Some(root(1)));
	}

	#[test]
	fn block_emitted_provides_is_rejected() {
		let raw = vec![select_core().encode(), UMPSignal::Provides(root(2)).encode()];
		assert_eq!(
			SpecMessagingSignals::build(None, &raw, &[], None),
			Err(SpecMessagingError::BlockEmittedSpecMsgSignal)
		);
	}

	#[test]
	fn all_idle_bundle_emits_nothing() {
		let signals = SpecMessagingSignals::build(None, &[], &[], None).unwrap();
		assert!(signals.is_empty());
		assert!(ump_signal_tail(SchedulingSignals::default(), signals).is_empty());
	}

	#[test]
	fn block_emitted_requires_is_rejected() {
		let requires = RequiresSet::try_from_iter([(ParaId::from(1), root(1))]).unwrap();
		let raw = vec![UMPSignal::Requires(requires).encode()];
		assert_eq!(
			SpecMessagingSignals::build(None, &raw, &[], None),
			Err(SpecMessagingError::BlockEmittedSpecMsgSignal)
		);
	}

	#[test]
	fn undecodable_signal_is_rejected() {
		assert_eq!(
			SpecMessagingSignals::build(None, &[vec![0xff]], &[], None),
			Err(SpecMessagingError::UndecodableSignal)
		);
	}

	#[test]
	fn failed_synthesis_is_rejected() {
		// A consumption record without its lift must not synthesize.
		let stream = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let (record, _, _) = source_fixture(2000.into(), stream, 5);
		assert!(matches!(
			SpecMessagingSignals::build(None, &[], &[record], None),
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

		let signals =
			SpecMessagingSignals::build(Some(root(9)), &[], &[record], Some(&lifts)).unwrap();

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
		let raw = vec![select_core().encode()];
		let tail = ump_signal_tail(
			SchedulingSignals::from_block_signals(&raw),
			SpecMessagingSignals::build(Some(root(4)), &raw, &[], None).unwrap(),
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
			SpecMessagingError::BlockEmittedSpecMsgSignal.to_string(),
			"Parachain block emitted a speculative-messaging UMP signal"
		);
		assert!(SpecMessagingError::RequiresSynthesis(LiftError::LiftCountMismatch)
			.to_string()
			.starts_with("Speculative Messaging `Requires` synthesis failed"));
	}
}
