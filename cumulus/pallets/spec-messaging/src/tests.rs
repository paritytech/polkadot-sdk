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

use crate::{mock::*, BlockStreamsRoot, Error, OutboundFrontier, OutboundMessages, TreeRoot};
use codec::Encode;
use cumulus_primitives_spec_messaging::{
	compute_streams_root, hash_leaf, prove_stream, MessagePosition, MmrFrontier, MmrRoot,
	SpecHasher, StreamId, StreamsRoot, LEAF_VERSION, SPMS_ENGINE_ID,
};
use frame_support::traits::{OnFinalize, OnInitialize};
use sp_runtime::{generic::DigestItem, Digest};
use std::collections::BTreeMap;

fn channel(recipient: u32) -> StreamId {
	StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
}

fn ack(recipient: u32) -> StreamId {
	StreamId::Ack { recipient: recipient.into(), domain: 0, num: 0 }
}

fn broadcast(num: u32) -> StreamId {
	StreamId::Broadcast { domain: 0, subdomain: 0, num }
}

fn append(stream: StreamId, payload: &[u8]) -> Result<MessagePosition, Error<Test>> {
	SpecMessaging::append_to_stream(stream, payload.to_vec())
}

/// Executes one full block — init hooks, `f` as the block's payload,
/// finalize hooks — and returns `f`'s result plus the block's digest.
fn run_block<R>(f: impl FnOnce() -> R) -> (R, Digest) {
	let n = System::block_number() + 1;
	System::initialize(&n, &Default::default(), &Default::default());
	SpecMessaging::on_initialize(n);
	let result = f();
	SpecMessaging::on_finalize(n);
	(result, System::digest())
}

/// The SPMS consensus digest payloads of a block.
fn spms_digests(digest: &Digest) -> Vec<Vec<u8>> {
	digest
		.logs()
		.iter()
		.filter_map(|item| match item {
			DigestItem::Consensus(SPMS_ENGINE_ID, data) => Some(data.clone()),
			_ => None,
		})
		.collect()
}

/// Reference model: every stream's full payload history, folded from
/// scratch through the issue-00 primitives on demand.
#[derive(Default)]
struct Reference(BTreeMap<StreamId, Vec<Vec<u8>>>);

impl Reference {
	fn push(&mut self, stream: StreamId, payload: &[u8]) -> MessagePosition {
		let history = self.0.entry(stream).or_default();
		history.push(payload.to_vec());
		MessagePosition(history.len() as u64 - 1)
	}

	fn entries(&self) -> BTreeMap<StreamId, MmrRoot> {
		self.0
			.iter()
			.map(|(stream, history)| {
				let mut frontier = MmrFrontier::default();
				for payload in history {
					frontier.append_leaf(hash_leaf::<SpecHasher>(LEAF_VERSION, payload));
				}
				(*stream, frontier.root())
			})
			.collect()
	}

	fn root(&self) -> Option<StreamsRoot> {
		compute_streams_root(&self.entries())
	}
}

#[test]
fn positions_continue_contiguously_across_blocks() {
	new_test_ext().execute_with(|| {
		let stream = channel(2001);

		run_block(|| {
			for i in 0..3u64 {
				assert_eq!(append(stream, b"first"), Ok(MessagePosition(i)));
			}
			// The stored frontier is untouched all block.
			assert_eq!(OutboundFrontier::<Test>::get(stream).leaf_count, 0);
		});

		run_block(|| {
			// Bumped exactly once, at this block's init.
			assert_eq!(OutboundFrontier::<Test>::get(stream).leaf_count, 3);
			assert_eq!(append(stream, b"second"), Ok(MessagePosition(3)));
			assert_eq!(append(stream, b"second"), Ok(MessagePosition(4)));
		});

		run_block(|| {
			assert_eq!(OutboundFrontier::<Test>::get(stream).leaf_count, 5);
			assert!(OutboundFrontier::<Test>::get(stream).is_consistent());
		});
	});
}

#[test]
fn lifecycle_bump_and_clear_is_atomic() {
	new_test_ext().execute_with(|| {
		let one = channel(2001);
		let two = ack(2002);

		run_block(|| {
			for _ in 0..3 {
				append(one, b"to one").unwrap();
			}
			for _ in 0..2 {
				append(two, b"to two").unwrap();
			}
		});

		// Before the next block's init: messages still readable in the
		// finalized block's state, frontiers still at the previous state,
		// the fold memo set.
		assert_eq!(OutboundMessages::<Test>::get(one).len(), 3);
		assert_eq!(OutboundMessages::<Test>::get(two).len(), 2);
		assert_eq!(OutboundFrontier::<Test>::get(one).leaf_count, 0);
		assert!(BlockStreamsRoot::<Test>::get().is_some());

		System::initialize(&2, &Default::default(), &Default::default());
		SpecMessaging::on_initialize(2);

		// After init: cleared and advanced by exactly the cleared counts,
		// in one step; the previous block's memo is gone.
		assert_eq!(OutboundMessages::<Test>::iter().count(), 0);
		assert_eq!(OutboundFrontier::<Test>::get(one).leaf_count, 3);
		assert_eq!(OutboundFrontier::<Test>::get(two).leaf_count, 2);
		assert_eq!(BlockStreamsRoot::<Test>::get(), None);
	});
}

#[test]
fn tree_fold_matches_reference_over_many_blocks() {
	// Property test: whatever mix of streams a block touches, the folded
	// StreamsRoot equals a reference tree built from scratch over all
	// streams' full payload histories — pinning hash_leaf + frontier +
	// tree store against the issue-00 primitives.
	new_test_ext().execute_with(|| {
		let streams = [
			channel(2001),
			channel(2002),
			StreamId::Channel { recipient: 0x0A0B_0C0Du32.into(), domain: 0xEE, num: 0x1234 },
			ack(2001),
			ack(2002),
			broadcast(0),
			broadcast(1),
			StreamId::Broadcast { domain: 1, subdomain: 2, num: 3 },
			StreamId::private(0x80, [1, 2, 3, 4, 5, 6, 7]).unwrap(),
			StreamId::private(0xFF, [0; 7]).unwrap(),
		];
		let mut reference = Reference::default();
		let mut lcg = 0x5DEE_CE66_D411u64;
		let mut rand = move || {
			lcg = lcg.wrapping_mul(6364136223846793005).wrapping_add(1442695040888963407);
			lcg >> 33
		};

		for block in 0u8..12 {
			let idle = block % 5 == 4;
			let (touched, digest) = run_block(|| {
				let mut touched = false;
				if idle {
					return touched;
				}
				for (index, stream) in streams.iter().enumerate() {
					for _ in 0..rand() % 3 {
						let mut payload = vec![block, index as u8];
						payload.extend(std::iter::repeat(0xAB).take((rand() % 30) as usize));
						payload.push(rand() as u8);
						let position = append(*stream, &payload).unwrap();
						assert_eq!(position, reference.push(*stream, &payload));
						touched = true;
					}
				}
				touched
			});

			if touched {
				let root = BlockStreamsRoot::<Test>::get().expect("streams were touched");
				assert_eq!(Some(root), reference.root(), "block {block}");
				assert_eq!(spms_digests(&digest), vec![root.encode()], "block {block}");
			} else {
				assert_eq!(BlockStreamsRoot::<Test>::get(), None, "block {block}");
				assert!(spms_digests(&digest).is_empty(), "block {block}");
			}
			// The stored tree cache always reflects everything folded so
			// far, idle blocks included.
			assert_eq!(
				TreeRoot::<Test>::get().map(|node| StreamsRoot(node.hash)),
				reference.root(),
				"block {block}"
			);
		}
	});
}

#[test]
fn multi_stream_block_updates_both_paths_under_one_root() {
	new_test_ext().execute_with(|| {
		let (a, b, c, d) = (channel(2001), ack(2001), broadcast(0), channel(2002));
		let mut reference = Reference::default();

		run_block(|| {
			for stream in [a, b, c] {
				append(stream, b"genesis").unwrap();
				reference.push(stream, b"genesis");
			}
		});
		let root_before = BlockStreamsRoot::<Test>::get().unwrap();
		let entries_before = reference.entries();
		assert_eq!(Some(root_before), reference.root());

		// Two streams touched — one of them brand new — in one block.
		run_block(|| {
			for stream in [a, d] {
				append(stream, b"update").unwrap();
				reference.push(stream, b"update");
			}
		});
		let root_after = BlockStreamsRoot::<Test>::get().unwrap();
		let entries_after = reference.entries();
		assert_ne!(root_before, root_after);
		// Both updated paths land under ONE root, matching the reference.
		assert_eq!(Some(root_after), reference.root());

		// Untouched streams' entries are unchanged, and an inclusion proof
		// for the old (unchanged) entry still verifies under the new root.
		for untouched in [b, c] {
			assert_eq!(entries_before[&untouched], entries_after[&untouched]);
			let proof = prove_stream(&entries_after, &untouched).unwrap();
			assert_eq!(proof.verify(&untouched, &entries_before[&untouched]).unwrap(), root_after);
		}
	});
}

#[test]
fn header_digest_equals_provides_root_and_is_absent_when_idle() {
	new_test_ext().execute_with(|| {
		let (_, digest) = run_block(|| {
			append(channel(2001), b"hello").unwrap();
		});

		// Exactly one SPMS digest item, equal to the root fed to the
		// `Provides` emission.
		let provides = SpecMessaging::current_streams_root().unwrap();
		assert_eq!(spms_digests(&digest), vec![provides.encode()]);

		// An idle block deposits nothing and provides nothing.
		let (_, digest) = run_block(|| {});
		assert!(spms_digests(&digest).is_empty());
		assert_eq!(SpecMessaging::current_streams_root(), None);
	});
}

#[test]
fn commit_streams_root_is_idempotent() {
	new_test_ext().execute_with(|| {
		let (roots, digest) = run_block(|| {
			append(channel(2001), b"hello").unwrap();
			// The emission hook may run the fold before this pallet's
			// `on_finalize` does (hook ordering is runtime-configured);
			// repeated calls yield the same root, one digest.
			(SpecMessaging::commit_streams_root(), SpecMessaging::commit_streams_root())
		});
		assert_eq!(roots.0, roots.1);
		assert_eq!(roots.0, SpecMessaging::current_streams_root());
		assert_eq!(spms_digests(&digest), vec![roots.0.unwrap().encode()]);
	});
}

#[test]
fn per_block_caps_error_and_leave_state_unchanged() {
	new_test_ext().execute_with(|| {
		let full = channel(2001);
		let untouched = channel(2002);

		run_block(|| {
			for i in 0..8u64 {
				assert_eq!(append(full, b"fits"), Ok(MessagePosition(i)));
			}
			// Count cap: the 9th append fails, the vec still holds 8.
			assert_eq!(append(full, b"fits"), Err(Error::<Test>::TooManyMessages));
			assert_eq!(OutboundMessages::<Test>::decode_len(full), Some(8));

			// Size cap: nothing is created for the stream at all.
			let oversized = vec![0u8; 65];
			assert_eq!(
				SpecMessaging::append_to_stream(untouched, oversized),
				Err(Error::<Test>::MessageTooBig)
			);
			assert_eq!(OutboundMessages::<Test>::decode_len(untouched), None);
			// A message of exactly MaxMsgLen is fine.
			assert_eq!(append(untouched, &[0u8; 64]), Ok(MessagePosition(0)));
		});

		run_block(|| {
			// Only the appended messages made it into the frontiers.
			assert_eq!(OutboundFrontier::<Test>::get(full).leaf_count, 8);
			assert_eq!(OutboundFrontier::<Test>::get(untouched).leaf_count, 1);
		});
	});
}
