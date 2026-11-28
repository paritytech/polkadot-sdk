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

//! Upward message signals in a parachain candidate.

use crate::SignedSchedulingInfo;
use alloc::{vec, vec::Vec};
use codec::{Decode, Encode};
use core::mem::discriminant;
use cumulus_primitives_spec_messaging::{
	build_requires, streams_root::try_read_streams_root, ConsumptionRecord, LiftError,
	LiftsBySource, StreamsRoot,
};
use polkadot_primitives::{ClaimQueueOffset, UMPSignal, UMP_SEPARATOR};
use sp_runtime::Digest;

/// Split upward messages at the first `UMP_SEPARATOR`. Drop later separators so they never
/// count as signals.
pub fn split_at_separator(messages: Vec<Vec<u8>>) -> (Vec<Vec<u8>>, Vec<Vec<u8>>) {
	let mut iter = messages.into_iter();
	let messages = iter.by_ref().take_while(|m| !m.is_empty()).collect();
	let signals = iter.filter(|m| !m.is_empty()).collect();
	(messages, signals)
}

/// Why the candidate's UMP signal tail cannot be built.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum SignalError {
	/// A block emitted bytes that do not decode as a `UMPSignal`.
	UndecodableSignal,
	/// More than one block signal selects a core.
	DuplicateSelectCore,
	/// More than one block signal names an approved peer.
	DuplicateApprovedPeer,
	/// A block emitted `Provides` or `Requires`, which only the tail assembly may produce.
	BlockEmittedSpecMsgSignal,
	/// A header contains a malformed or duplicate `SPMS` item.
	InvalidStreamsRootDigest,
	/// The consumption records and the lifts do not synthesize a `Requires`.
	RequiresSynthesis(LiftError),
}

impl core::fmt::Display for SignalError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			Self::UndecodableSignal => write!(f, "Failed to decode `UMPSignal`"),
			Self::DuplicateSelectCore => {
				write!(f, "Parachain emitted more than one `SelectCore` UMP signal")
			},
			Self::DuplicateApprovedPeer => {
				write!(f, "Parachain emitted more than one `ApprovedPeer` UMP signal")
			},
			Self::BlockEmittedSpecMsgSignal => {
				write!(f, "Parachain block emitted a speculative-messaging UMP signal")
			},
			Self::InvalidStreamsRootDigest => write!(f, "Invalid SPMS header digest"),
			Self::RequiresSynthesis(error) => {
				write!(f, "Speculative Messaging `Requires` synthesis failed: {:?}", error)
			},
		}
	}
}

fn scheduling_signals(
	raw: &[Vec<u8>],
	signed: Option<&SignedSchedulingInfo>,
) -> Result<Vec<UMPSignal>, SignalError> {
	let mut signed_core = signed.map(|info| {
		UMPSignal::SelectCore(
			info.payload.core_selector,
			ClaimQueueOffset(info.payload.claim_queue_offset),
		)
	});
	let mut signed_peer = signed.map(|info| UMPSignal::ApprovedPeer(info.payload.peer_id.clone()));
	let mut signals = Vec::new();
	for bytes in raw {
		let signal =
			UMPSignal::decode(&mut &bytes[..]).map_err(|_| SignalError::UndecodableSignal)?;
		let duplicate = signals.iter().any(|seen| discriminant(seen) == discriminant(&signal));
		match signal {
			UMPSignal::SelectCore(..) if duplicate => return Err(SignalError::DuplicateSelectCore),
			UMPSignal::ApprovedPeer(_) if duplicate => {
				return Err(SignalError::DuplicateApprovedPeer)
			},
			UMPSignal::SelectCore(..) => signals.push(signed_core.take().unwrap_or(signal)),
			UMPSignal::ApprovedPeer(_) => signals.push(signed_peer.take().unwrap_or(signal)),
			UMPSignal::Provides(_) | UMPSignal::Requires(_) => {
				return Err(SignalError::BlockEmittedSpecMsgSignal)
			},
		}
	}

	Ok(signals)
}

/// Build the candidate's UMP tail from the blocks' scheduling signals in order, followed by
/// `Provides` and `Requires`. A signed override replaces only the scheduling signals the blocks
/// emitted. An empty tail omits the separator because the relay decoder reads signals after the
/// first `UMP_SEPARATOR`.
pub fn ump_signal_tail(
	raw: &[Vec<u8>],
	signed: Option<&SignedSchedulingInfo>,
	provides: Option<StreamsRoot>,
	records: &[ConsumptionRecord],
	lifts: Option<&LiftsBySource>,
) -> Result<Vec<Vec<u8>>, SignalError> {
	let scheduling = scheduling_signals(raw, signed)?;
	let no_lifts = LiftsBySource::default();
	let requires = build_requires(records, lifts.unwrap_or(&no_lifts))
		.map_err(SignalError::RequiresSynthesis)?;

	if scheduling.is_empty() && provides.is_none() && requires.is_none() {
		return Ok(Vec::new());
	}
	let mut tail = vec![UMP_SEPARATOR];
	tail.extend(scheduling.iter().map(Encode::encode));
	if let Some(root) = provides {
		tail.push(UMPSignal::Provides(root).encode());
	}
	if let Some(requires) = requires {
		tail.push(UMPSignal::Requires(requires).encode());
	}
	Ok(tail)
}

/// The last root in bundle order, or `None` if no header carries an `SPMS` item.
/// A malformed or duplicate item in any header invalidates the bundle.
pub fn bundle_provides<'a>(
	digests: impl IntoIterator<Item = &'a Digest>,
) -> Result<Option<StreamsRoot>, SignalError> {
	let mut last = None;
	for digest in digests {
		if let Some(root) =
			try_read_streams_root(digest).map_err(|_| SignalError::InvalidStreamsRootDigest)?
		{
			last = Some(root);
		}
	}
	Ok(last)
}

#[cfg(test)]
mod tests {
	use super::*;
	use alloc::{collections::BTreeMap, vec};
	use cumulus_primitives_spec_messaging::{
		streams_root::{gen_stream_proof, streams_root},
		Interval, MmrExtensionProof, MmrFrontier, MmrRoot, RequiresLift, StreamId, SPMS_ENGINE_ID,
	};
	use polkadot_primitives::{
		ApprovedPeerId, ClaimQueueOffset, CoreSelector, Hash as RHash, Id as ParaId, RequiresSet,
	};
	use sp_runtime::DigestItem;

	#[test]
	fn split_at_separator_drops_later_separators() {
		let (a, b, c) = (vec![1u8], vec![2u8], vec![3u8]);
		// No separator: everything is a message.
		assert_eq!(
			split_at_separator(vec![a.clone(), b.clone()]),
			(vec![a.clone(), b.clone()], vec![])
		);
		// Messages before the first separator, signals after it.
		assert_eq!(
			split_at_separator(vec![a.clone(), vec![], b.clone(), c.clone()]),
			(vec![a.clone()], vec![b.clone(), c.clone()]),
		);
		assert_eq!(
			split_at_separator(vec![a.clone(), vec![], b.clone(), vec![], vec![], c.clone()]),
			(vec![a], vec![b, c]),
		);
		// A separator with nothing after it yields no signals.
		assert_eq!(split_at_separator(vec![vec![]]), (vec![], vec![]));
	}

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
				vec![(stream, Interval { start: MmrRoot(RHash::zero()), end: frontier.clone() })],
			)]),
		};
		let lift = RequiresLift {
			advances: Vec::new(),
			extension: MmrExtensionProof::identity(&frontier),
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
		assert_eq!(bundle_provides([&sent, &idle, &resent]), Ok(Some(root(3))));
		// An idle last block still reports the root an earlier block produced.
		assert_eq!(bundle_provides([&sent, &idle]), Ok(Some(root(1))));
		assert_eq!(bundle_provides([&idle, &idle]), Ok(None));
	}

	#[test]
	fn invalid_digest_anywhere_rejects_the_bundle() {
		let valid = digest(&[root(1)]);
		let duplicate = digest(&[root(4), root(5)]);
		let malformed = Digest { logs: vec![DigestItem::Consensus(SPMS_ENGINE_ID, vec![])] };
		assert_eq!(
			bundle_provides([&valid, &duplicate]),
			Err(SignalError::InvalidStreamsRootDigest)
		);
		assert_eq!(
			bundle_provides([&malformed, &valid]),
			Err(SignalError::InvalidStreamsRootDigest)
		);
	}

	fn peer(byte: u8) -> ApprovedPeerId {
		vec![byte; 4].try_into().expect("four bytes fit in a peer ID; qed")
	}

	fn signed_with(
		core_selector: CoreSelector,
		claim_queue_offset: u8,
		peer_id: ApprovedPeerId,
	) -> SignedSchedulingInfo {
		SignedSchedulingInfo {
			payload: crate::SchedulingInfoPayload::new(
				core_selector,
				claim_queue_offset,
				peer_id,
				Default::default(),
			),
			signature: [0u8; 64],
		}
	}

	#[test]
	fn signed_override_takes_every_value_from_the_payload() {
		let signed = signed_with(CoreSelector(7), 3, peer(0xAA));
		let raw = [select_core().encode(), UMPSignal::ApprovedPeer(peer(1)).encode()];
		assert_eq!(
			ump_signal_tail(&raw, Some(&signed), None, &[], None).unwrap(),
			vec![
				UMP_SEPARATOR,
				UMPSignal::SelectCore(CoreSelector(7), ClaimQueueOffset(3)).encode(),
				UMPSignal::ApprovedPeer(peer(0xAA)).encode(),
			]
		);
	}

	#[test]
	fn signed_override_emits_an_empty_peer() {
		let signed = signed_with(CoreSelector(5), 1, ApprovedPeerId::default());
		let raw = [UMPSignal::ApprovedPeer(peer(1)).encode()];
		assert_eq!(
			ump_signal_tail(&raw, Some(&signed), None, &[], None).unwrap(),
			vec![UMP_SEPARATOR, UMPSignal::ApprovedPeer(ApprovedPeerId::default()).encode()]
		);
	}

	#[test]
	fn scheduling_order_is_preserved_with_or_without_an_override() {
		let core = select_core().encode();
		let peer_signal = UMPSignal::ApprovedPeer(peer(1)).encode();
		let signed = signed_with(CoreSelector(5), 1, peer(2));
		let signed_core = UMPSignal::SelectCore(CoreSelector(5), ClaimQueueOffset(1)).encode();
		let signed_peer = UMPSignal::ApprovedPeer(peer(2)).encode();
		let provides = UMPSignal::Provides(root(3)).encode();
		for (raw, replacement) in [
			([core.clone(), peer_signal.clone()], [signed_core.clone(), signed_peer.clone()]),
			([peer_signal, core], [signed_peer, signed_core]),
		] {
			for (info, expected) in [(None, &raw), (Some(&signed), &replacement)] {
				assert_eq!(
					ump_signal_tail(&raw, info, Some(root(3)), &[], None).unwrap(),
					vec![UMP_SEPARATOR, expected[0].clone(), expected[1].clone(), provides.clone()]
				);
			}
		}
	}

	#[test]
	fn signed_override_replaces_only_emitted_scheduling_signals() {
		let clean = UMPSignal::ApprovedPeer(peer(1)).encode();
		let mut padded = clean.clone();
		padded.push(0xff);
		assert_eq!(
			ump_signal_tail(&[padded.clone()], None, None, &[], None).unwrap(),
			vec![UMP_SEPARATOR, clean]
		);
		let signed = signed_with(CoreSelector(5), 1, peer(2));
		for (raw, expected) in [
			(vec![padded], vec![UMP_SEPARATOR, UMPSignal::ApprovedPeer(peer(2)).encode()]),
			(
				vec![select_core().encode()],
				vec![
					UMP_SEPARATOR,
					UMPSignal::SelectCore(CoreSelector(5), ClaimQueueOffset(1)).encode(),
				],
			),
			(vec![], vec![]),
		] {
			assert_eq!(ump_signal_tail(&raw, Some(&signed), None, &[], None).unwrap(), expected);
		}
	}

	#[test]
	fn both_paths_reject_invalid_block_signals() {
		let requires = RequiresSet::try_from_iter([(ParaId::from(1), root(1))]).unwrap();
		let peer_signal = UMPSignal::ApprovedPeer(peer(1)).encode();
		for (raw, error) in [
			(vec![select_core().encode(); 2], SignalError::DuplicateSelectCore),
			(
				vec![
					select_core().encode(),
					UMPSignal::SelectCore(CoreSelector(1), ClaimQueueOffset(0)).encode(),
				],
				SignalError::DuplicateSelectCore,
			),
			(vec![peer_signal.clone(); 2], SignalError::DuplicateApprovedPeer),
			(
				vec![peer_signal, UMPSignal::ApprovedPeer(peer(2)).encode()],
				SignalError::DuplicateApprovedPeer,
			),
			(vec![UMPSignal::Provides(root(2)).encode()], SignalError::BlockEmittedSpecMsgSignal),
			(vec![UMPSignal::Requires(requires).encode()], SignalError::BlockEmittedSpecMsgSignal),
			(vec![vec![0xff]], SignalError::UndecodableSignal),
		] {
			for signed in [None, Some(&signed_with(CoreSelector(5), 1, peer(2)))] {
				assert_eq!(ump_signal_tail(&raw, signed, None, &[], None), Err(error));
			}
		}
	}

	#[test]
	fn all_idle_bundle_emits_nothing() {
		assert!(ump_signal_tail(&[], None, None, &[], None).unwrap().is_empty());
	}

	#[test]
	fn provides_alone_has_a_separator() {
		let provides = root(3);
		assert_eq!(
			ump_signal_tail(&[], None, Some(provides), &[], None).unwrap(),
			vec![UMP_SEPARATOR, UMPSignal::Provides(provides).encode()]
		);
	}

	#[test]
	fn failed_synthesis_is_rejected() {
		// A consumption record without its lift must not synthesize.
		let stream = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let (record, _, _) = source_fixture(2000.into(), stream, 5);
		assert!(matches!(
			ump_signal_tail(&[], None, None, &[record], None),
			Err(SignalError::RequiresSynthesis(_))
		));
	}

	#[test]
	fn synthesized_requires_matches_the_committed_root() {
		// Record and lift in, `Requires` naming the source's committed root out, with `Provides`
		// before `Requires` in the tail bytes.
		let stream = StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 };
		let source = ParaId::from(2000);
		let (record, lifts, committed) = source_fixture(source, stream, 5);

		let expected = RequiresSet::try_from_iter([(source, committed)]).unwrap();
		assert_eq!(
			ump_signal_tail(&[], None, Some(root(9)), &[record], Some(&lifts)).unwrap(),
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
		let tail = ump_signal_tail(&raw, None, Some(root(4)), &[], None).unwrap();
		assert_eq!(
			tail,
			vec![UMP_SEPARATOR, select_core().encode(), UMPSignal::Provides(root(4)).encode()]
		);
	}

	#[test]
	fn error_messages_are_stable() {
		// `validate_block` panics with these; tests and operators match on them.
		assert_eq!(
			SignalError::BlockEmittedSpecMsgSignal.to_string(),
			"Parachain block emitted a speculative-messaging UMP signal"
		);
		assert!(SignalError::RequiresSynthesis(LiftError::LiftCountMismatch)
			.to_string()
			.starts_with("Speculative Messaging `Requires` synthesis failed"));
	}
}
