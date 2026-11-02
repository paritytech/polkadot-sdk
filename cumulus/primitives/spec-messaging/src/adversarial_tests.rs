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

//! Adversarial tests. The attacker keeps every proof well-formed and lies only about what a value
//! means: a count, a position, a key, or what the payload bytes are. Each test changes that meaning
//! and requires a failure or a root nothing commits to. Malformed input is covered next to each
//! verifier; this module covers well-formed lies, grouped by what the attacker controls.

use crate::{
	fetch::{verify_messages, MessagesRequest, MessagesResponse, VerifyError},
	lift::{stitch, Interval, LiftError},
	mmr::{
		bag_peaks, leaf_hash, leaf_hash_with_version, MMRExtensionProof, MmrFrontier, MmrRoot,
		SpecMerge, MAX_MMR_LEAF_COUNT,
	},
	streams_root::{gen_stream_proof, streams_root, streams_root_from_proof},
	test_utils::{extension, frontier_at, leaves},
	StreamId, INNER_TAG, PEAK_TAG, ROOT_TAG,
};
use alloc::collections::BTreeMap;
use mmr_lib::Merge;
use polkadot_core_primitives::Hash;
use sp_core::H256;
use sp_runtime::BoundedVec;

/// Counts an attacker can claim: every small one, and the edges of the allowed range.
fn claimable_counts() -> impl Iterator<Item = u64> {
	(1..=1024)
		.chain((1 << 47)..(1 << 47) + 64)
		.chain(MAX_MMR_LEAF_COUNT - 64..=MAX_MMR_LEAF_COUNT)
}

/// A1, the frontier's `leaf_count`. Peaks do not commit their heights, so one set of peaks is a
/// valid frontier at every count with the same number of set bits, and bags the same at each. No
/// other count may reproduce the root.
#[test]
fn forged_count_never_reproduces_a_root() {
	let all = leaves(64);
	for n in 1..=64u64 {
		let honest = frontier_at(&all, n as usize);
		let mut forged_any = false;
		for count in claimable_counts().filter(|count| *count != n) {
			let Some(forged) = MmrFrontier::from_parts(honest.peaks().to_vec(), count) else {
				continue;
			};
			forged_any = true;
			assert_eq!(bag_peaks(forged.peaks()), bag_peaks(honest.peaks()));
			assert_ne!(forged.root(), honest.root(), "{count} leaves pose as {n}");
		}
		assert!(forged_any, "no count shares the shape of {n}");
	}
}

/// A2, the head read's `frontier` leaf count. A lossy head read (`start: None`) takes the frontier
/// at `m - 1` and appends the head payload to reach `m`. If a peer claims the `k` peaks of `m - 1`
/// leaves sit at a different count with `k` set bits, appending the head payload produces a
/// frontier at a forged position whose root does not match the committed `StreamsRoot`.
#[test]
fn forged_head_frontier_never_moves_a_leaf() {
	let stream = StreamId::Channel { recipient: 2000.into(), domain: 0, num: 0 };
	let payloads: Vec<Vec<u8>> = (0..13u8).map(|i| vec![i; 4]).collect();
	let all: Vec<Hash> = payloads.iter().map(|p| leaf_hash(p)).collect();
	for m in 1..=all.len() {
		let committed_stream_root = frontier_at(&all, m).root();
		let entries = BTreeMap::from([(stream, committed_stream_root.0)]);
		let (under, tree_proof) = gen_stream_proof(&entries, stream).unwrap();
		let req = MessagesRequest { stream, under, start: None, max_bytes: 64 };
		let honest_frontier = frontier_at(&all, m - 1);
		let honest = MessagesResponse {
			frontier: honest_frontier.clone(),
			payloads: BoundedVec::truncate_from(vec![payloads[m - 1].clone()]),
			extension: MMRExtensionProof::identity(),
			tree_proof: tree_proof.clone(),
		};
		assert_eq!(verify_messages(&req, &honest), Ok(()));

		for count in claimable_counts()
			.filter(|count| *count != (m - 1) as u64 && *count < MAX_MMR_LEAF_COUNT)
		{
			let Some(forged_frontier) =
				MmrFrontier::from_parts(honest_frontier.peaks().to_vec(), count)
			else {
				continue;
			};
			let forged = MessagesResponse { frontier: forged_frontier, ..honest.clone() };
			assert_eq!(verify_messages(&req, &forged), Err(VerifyError::RootMismatch));
		}
		if let Some(ceiling_frontier) =
			MmrFrontier::from_parts(honest_frontier.peaks().to_vec(), MAX_MMR_LEAF_COUNT)
		{
			let at_ceiling = MessagesResponse { frontier: ceiling_frontier, ..honest.clone() };
			assert_eq!(verify_messages(&req, &at_ceiling), Err(VerifyError::MalformedResponse));
		}
	}

	// Specifically: the single peak of 2 leaves, claimed as `2^47` leaves, puts the head at
	// position `2^47`. The frontier after appending leaf 2 has `2^47 + 1` leaves, and its root is
	// not the committed 3-leaf root.
	let mut forged = MmrFrontier::from_parts(frontier_at(&all, 2).peaks().to_vec(), 1 << 47)
		.expect("2^47 has one set bit, matching 2 leaves; qed");
	forged.append(all[2]);
	assert_eq!(forged.leaf_count(), (1 << 47) + 1);
	assert_ne!(forged.root(), frontier_at(&all, 3).root());
}

/// A3, the extension's `leaf_count` and the frontier it extends. Neither the honest connecting
/// nodes under another count, nor the honest proof applied to a frontier at a forged count, may
/// reach the committed root.
#[test]
fn forged_extension_never_reaches_the_committed_root() {
	let all = leaves(64);
	for (k, n) in [(1, 2), (2, 3), (3, 4), (2, 5), (5, 13), (13, 40), (7, 64)] {
		let from = frontier_at(&all, k);
		let target = frontier_at(&all, n).root();
		let honest = extension(&all, k, n);
		assert_eq!(honest.calculate_root(&from), Ok(target));

		for count in claimable_counts().filter(|count| *count != n as u64) {
			let forged = MMRExtensionProof { leaf_count: count, ..honest.clone() };
			assert_ne!(
				forged.calculate_root(&from),
				Ok(target),
				"{k} -> {count} poses as {k} -> {n}"
			);
		}
		for count in claimable_counts().filter(|count| *count != k as u64) {
			let Some(forged) = MmrFrontier::from_parts(from.peaks().to_vec(), count) else {
				continue;
			};
			assert_ne!(honest.calculate_root(&forged), Ok(target), "{count} leaves pose as {k}");
		}
	}
}

/// A4, the payload bytes. A payload equal to a node's preimage, with or without its tag, must not
/// hash to that node, and a node read as a leaf must not stand for the leaves below it.
#[test]
fn payload_bytes_never_pose_as_a_node() {
	let (left, right) = (H256::repeat_byte(1), H256::repeat_byte(2));
	let inner = SpecMerge::merge(&left, &right).unwrap();
	let bag = SpecMerge::merge_peaks(&left, &right).unwrap();
	let root = MmrRoot::from_bag(2, inner).0;
	let pair = [left.as_bytes(), right.as_bytes()].concat();
	let count_and_bag = [&2u64.to_le_bytes()[..], inner.as_bytes()].concat();
	let disguises = [
		(inner, [&[INNER_TAG][..], &pair].concat()),
		(inner, pair.clone()),
		(bag, [&[PEAK_TAG][..], &pair].concat()),
		(bag, pair),
		(root, [&[ROOT_TAG][..], &count_and_bag].concat()),
		(root, count_and_bag),
	];
	for version in [0, 1, u8::MAX] {
		for (node, payload) in &disguises {
			assert_ne!(leaf_hash_with_version(version, payload), *node);
		}
	}

	// Two leaves under one inner node, read as one leaf whose hash is that node: the peaks are the
	// same, the stream is not.
	let two = frontier_at(&[left, right], 2);
	let one = MmrFrontier::from_parts(two.peaks().to_vec(), 1).unwrap();
	assert_ne!(one.root(), two.root());
}

/// A5, the key a tree proof is checked under. Keys one bit apart (adjacent recipients, adjacent
/// channel numbers, `Channel` and `Ack` with the same fields) and absent keys must not verify with
/// another member's proof, and no member may verify with another value.
#[test]
fn tree_proof_never_verifies_under_another_key() {
	let channel =
		|recipient: u32, num| StreamId::Channel { recipient: recipient.into(), domain: 0, num };
	let ack = |recipient: u32, num| StreamId::Ack { recipient: recipient.into(), domain: 0, num };
	let members =
		[channel(2000, 0), channel(2000, 1), ack(2000, 0), ack(2001, 0), channel(2001, 1)];
	let absent = [channel(2000, 2), ack(2000, 1), channel(2001, 0), ack(2001, 1)];
	let entries: BTreeMap<StreamId, Hash> = members
		.iter()
		.zip(1u8..)
		.map(|(stream, byte)| (*stream, H256::repeat_byte(byte)))
		.collect();
	let committed = streams_root(&entries).unwrap();

	for (stream, value) in &entries {
		let (_, proof) = gen_stream_proof(&entries, *stream).unwrap();
		assert_eq!(streams_root_from_proof(*stream, *value, &proof), Some(committed));
		for other in members.iter().chain(&absent).filter(|other| *other != stream) {
			for any in entries.values() {
				assert_ne!(
					streams_root_from_proof(*other, *any, &proof),
					Some(committed),
					"{stream:?}'s proof verifies {other:?}"
				);
			}
		}
		for any in entries.values().filter(|any| *any != value) {
			assert_ne!(streams_root_from_proof(*stream, *any, &proof), Some(committed));
		}
	}
}

/// A6, the intervals a lift stitches (lexnv's relabel). `G = 2^10 + 6` and `22` share their low
/// bits and their peak count, so the peaks at `G`, claimed at `22`, take the next two leaves
/// through the same merges. A read of positions `G` and `G + 1` relabelled to `22` and `23` must
/// not chain into the genuine stream, or a later block could read them again as new.
#[test]
fn range_read_cannot_be_relabelled_to_a_lower_position() {
	const G: usize = (1 << 10) + 6;
	let all = leaves(G as u64 + 3);
	let claimed = MmrFrontier::from_parts(frontier_at(&all, G).peaks().to_vec(), 22).unwrap();
	let mut relabelled = claimed.clone();
	relabelled.append(all[G]);
	relabelled.append(all[G + 1]);
	let genuine = frontier_at(&all, G + 2);
	assert_eq!(relabelled.peaks(), genuine.peaks());
	assert_ne!(relabelled.root(), genuine.root());

	let read = Interval { start: claimed.root(), end: relabelled };
	let next = Interval { start: genuine.root(), end: frontier_at(&all, G + 3) };
	assert_eq!(stitch(&[read.clone(), next.clone()], &[]), Err(LiftError::MissingAdvance));
	let advance = extension(&all, 24, G + 2);
	assert_eq!(stitch(&[read, next], &[advance]), Err(LiftError::BrokenChain));
}
