//! `StreamsRoot` — the sender's per-block stream commitment (spec-msg v0.5).
//!
//! Each block, a sender commits ONE hash — the `StreamsRoot`, the root of a **keyed binary Patricia
//! trie** over `(StreamId, current stream root)` for every active stream. A receiver reads it (from
//! the sender's header digest, or `paras::Heads`) and proves the stream it consumes against it —
//! obtaining the sender's committed root for that stream without a sender-state proof.
//!
//! # Why a keyed trie (not a positional Merkle tree)
//!
//! The trie is keyed by the `StreamId`'s frozen 8-byte encoding, so a proof is a **walk driven by
//! the key**: verification recomputes the leaf from the caller's `StreamId` and checks each branch
//! direction against that key's bits. This is the property the requires-lift synthesis depends on —
//! lifts are matched *positionally* to the consumption record's streams, and a mispaired lift (a
//! proof built for stream A checked as stream B) cannot verify, because the walk binds the key. A
//! positional `binary_merkle_tree` (index + siblings) does not give this. Patricia compression
//! (branch only where keys diverge) also keeps proofs `O(log S)` in the number of streams, not
//! `O(64)` in the key width, and makes the tree stable under insertion (only the inserted path is
//! perturbed).
//!
//! Roles:
//! - **sender / node** builds the root ([`streams_root`]) and serves membership proofs
//!   ([`gen_stream_proof`]);
//! - **receiver** reads the root ([`read_streams_root`]) and verifies membership
//!   ([`verify_stream_membership`]).
//!
//! Hashing (domain-tagged, `blake2_256` via [`SpecHasher`]):
//! - leaf `= H(STREAMS_LEAF_TAG ++ key[8] ++ stream_root[32])` — binds the full key;
//! - inner `= H(STREAMS_INNER_TAG ++ split_bit[1] ++ left[32] ++ right[32])` — binds the branch
//!   depth.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode};
use polkadot_core_primitives::Hash;
use sp_core::ConstU32;
use sp_runtime::{traits::Hash as HashT, BoundedVec, Digest, DigestItem};

use crate::{
	stream::StreamId, SpecHasher, SPMS_ENGINE_ID, STREAMS_INNER_TAG, STREAMS_LEAF_TAG,
	STREAM_ID_LEN,
};

/// Bit width of a `StreamId` key (the Patricia trie's maximum depth).
const KEY_BITS: usize = STREAM_ID_LEN * 8;

// The proof container's decode bound below must equal the trie depth.
const _: () = assert!(KEY_BITS == 64);

/// The sender's per-block stream commitment root — a newtype over `Hash`, deliberately
/// distinct from an MMR / stream root ("confusing roots must not typecheck"). Defined
/// relay-side and re-exported here so both sides share one type.
pub use polkadot_primitives::StreamsRoot;

/// One step of a `StreamsRoot` trie walk, from an inner node toward the proven leaf.
///
/// The branch *direction* is not stored — it is derived from the proven key's bit at `split_bit`
/// during verification (the key binding), so a step is just `(split_bit, sibling)`.
#[derive(
	Clone, Copy, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, scale_info::TypeInfo,
)]
pub struct TreeStep {
	/// Key-bit index this node branches on (`0..KEY_BITS`, `0` = MSB of the key). Bound into the
	/// inner-node hash; the proven key's bit here also gives the branch direction.
	pub split_bit: u8,
	/// Hash of the sibling subtree (the side the proven key does NOT descend to).
	pub sibling: Hash,
}

/// A membership proof that `(stream, stream_root)` is committed under a `StreamsRoot` — served by
/// the sender/node, verified by the receiver. The walk is keyed: it carries no leaf index, only the
/// branch steps, and the key drives verification.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, PartialEq, Eq, Debug, scale_info::TypeInfo,
)]
pub struct StreamProof {
	/// Branch steps **leaf to root**, split bits strictly decreasing (deeper branches split on
	/// later bits) — any other ordering is rejected at verification. Empty for a single-stream
	/// tree. Bounded at the trie depth, so an over-long proof is rejected at decode.
	pub steps: BoundedVec<TreeStep, ConstU32<64>>,
}

/// Bit `i` of an 8-byte key, `0` = MSB of byte 0 (so big-endian byte order = bit order).
fn bit(key: &[u8; STREAM_ID_LEN], i: usize) -> bool {
	(key[i >> 3] >> (7 - (i & 7))) & 1 == 1
}

/// The `StreamId`'s frozen 8-byte canonical encoding, as the trie key.
fn key_of(stream: &StreamId) -> [u8; STREAM_ID_LEN] {
	let mut key = [0u8; STREAM_ID_LEN];
	// `StreamId::encode` is always exactly `STREAM_ID_LEN` bytes (frozen, consensus-critical).
	key.copy_from_slice(&stream.encode());
	key
}

/// First bit index (from the MSB) at which two distinct keys differ. Callers guarantee `a != b`.
fn first_diverging_bit(a: &[u8; STREAM_ID_LEN], b: &[u8; STREAM_ID_LEN]) -> usize {
	(0..KEY_BITS).find(|&i| bit(a, i) != bit(b, i)).expect("keys are distinct; qed")
}

/// `H(STREAMS_LEAF_TAG ++ key ++ stream_root)`.
fn hash_leaf(key: &[u8; STREAM_ID_LEN], stream_root: &Hash) -> Hash {
	let mut pre = [0u8; 1 + STREAM_ID_LEN + 32];
	pre[0] = STREAMS_LEAF_TAG;
	pre[1..1 + STREAM_ID_LEN].copy_from_slice(key);
	pre[1 + STREAM_ID_LEN..].copy_from_slice(stream_root.as_bytes());
	<SpecHasher as HashT>::hash(&pre)
}

/// `H(STREAMS_INNER_TAG ++ split_bit ++ left ++ right)`.
fn hash_inner(split_bit: u8, left: &Hash, right: &Hash) -> Hash {
	let mut pre = [0u8; 1 + 1 + 32 + 32];
	pre[0] = STREAMS_INNER_TAG;
	pre[1] = split_bit;
	pre[2..34].copy_from_slice(left.as_bytes());
	pre[34..].copy_from_slice(right.as_bytes());
	<SpecHasher as HashT>::hash(&pre)
}

/// Split a key-sorted, non-empty slice at its top-level branch: the bit where its first and last
/// keys diverge, with all `bit == 0` keys before all `bit == 1` keys. Returns `(split_bit, left,
/// right)`, both sides non-empty. Callers guarantee `entries.len() >= 2` and distinct keys.
fn split<'a>(
	entries: &'a [([u8; STREAM_ID_LEN], Hash)],
) -> (usize, &'a [([u8; STREAM_ID_LEN], Hash)], &'a [([u8; STREAM_ID_LEN], Hash)]) {
	let split_bit = first_diverging_bit(&entries[0].0, &entries[entries.len() - 1].0);
	let p = entries.partition_point(|(k, _)| !bit(k, split_bit));
	let (left, right) = entries.split_at(p);
	(split_bit, left, right)
}

/// Trie root over a key-sorted, non-empty slice.
fn node_hash(entries: &[([u8; STREAM_ID_LEN], Hash)]) -> Hash {
	if let [(key, root)] = entries {
		return hash_leaf(key, root);
	}
	let (split_bit, left, right) = split(entries);
	hash_inner(split_bit as u8, &node_hash(left), &node_hash(right))
}

/// Sort `entries` into canonical (key) order for trie construction.
fn sorted(mut entries: Vec<(StreamId, Hash)>) -> Vec<([u8; STREAM_ID_LEN], Hash)> {
	entries.sort_by_key(|(stream, _)| *stream);
	entries.into_iter().map(|(stream, root)| (key_of(&stream), root)).collect()
}

/// The `StreamsRoot` over `(stream, stream_root)` entries, `None` when there are no active streams.
/// Entries need not be sorted or deduplicated by the caller for ordering, but keys MUST be unique
/// (the messaging inherent carries at most one item per stream). Sender/node side.
pub fn streams_root(entries: Vec<(StreamId, Hash)>) -> Option<StreamsRoot> {
	let entries = sorted(entries);
	if entries.is_empty() {
		return None;
	}
	Some(StreamsRoot(node_hash(&entries)))
}

/// Walk `entries` (key-sorted) toward `key`, pushing each branch step (root-first) and returning
/// the subtree's root. The key is guaranteed present in `entries`.
fn prove(
	entries: &[([u8; STREAM_ID_LEN], Hash)],
	key: &[u8; STREAM_ID_LEN],
	steps: &mut Vec<TreeStep>,
) -> Hash {
	if let [(k, root)] = entries {
		return hash_leaf(k, root);
	}
	let (split_bit, left, right) = split(entries);
	let target_right = bit(key, split_bit);
	let (sibling, descend) =
		if target_right { (node_hash(left), right) } else { (node_hash(right), left) };
	steps.push(TreeStep { split_bit: split_bit as u8, sibling });
	let child = prove(descend, key, steps);
	if target_right {
		hash_inner(split_bit as u8, &sibling, &child)
	} else {
		hash_inner(split_bit as u8, &child, &sibling)
	}
}

/// Build the streams trie over `entries` and return `(StreamsRoot, proof)` proving `stream`'s
/// membership, or `None` if `stream` is absent. Sender/node side; the receiver only runs
/// [`verify_stream_membership`].
pub fn gen_stream_proof(
	entries: Vec<(StreamId, Hash)>,
	stream: StreamId,
) -> Option<(StreamsRoot, StreamProof)> {
	let entries = sorted(entries);
	let key = key_of(&stream);
	if !entries.iter().any(|(k, _)| *k == key) {
		return None;
	}
	let mut steps = Vec::new();
	let root = prove(&entries, &key, &mut steps);
	// `prove` pushes root-first; the wire order is leaf-to-root (split bits strictly
	// decreasing).
	steps.reverse();
	let steps = BoundedVec::try_from(steps).expect("trie depth <= KEY_BITS; qed");
	Some((StreamsRoot(root), StreamProof { steps }))
}

/// Read the sender's `StreamsRoot` from its header digest, if present.
pub fn read_streams_root(digest: &Digest) -> Option<StreamsRoot> {
	digest.logs().iter().find_map(|item| match item {
		DigestItem::Consensus(id, payload) if *id == SPMS_ENGINE_ID => {
			StreamsRoot::decode(&mut &payload[..]).ok()
		},
		_ => None,
	})
}

/// Fold a keyed membership proof for `(stream, stream_root)` up to the `StreamsRoot` it *implies*,
/// or `None` if a step is malformed (an out-of-range branch). Unlike [`verify_stream_membership`]
/// this **yields** the root rather than comparing it — the shape the requires-lift `tree_proof`
/// needs (`compute_tree_root` in the design): the walk is driven by `stream`'s key (each branch
/// direction is that key's bit at `split_bit`), so a proof built for a different stream folds the
/// siblings on the wrong sides and yields a different root.
pub fn streams_root_from_proof(
	stream: StreamId,
	stream_root: Hash,
	membership: &StreamProof,
) -> Option<StreamsRoot> {
	let key = key_of(&stream);
	let mut node = hash_leaf(&key, &stream_root);
	// Steps are leaf-to-root with strictly decreasing split bits — any other ordering is
	// rejected as early garbage (uniqueness itself rests on the key-in-leaf / bit-in-inner
	// commitments, not on this rule). The length bound (≤ trie depth) lives in the type,
	// enforced at decode.
	let mut prev: Option<u8> = None;
	for step in membership.steps.iter() {
		let split_bit = step.split_bit as usize;
		// Reject an out-of-range branch (would otherwise index the key out of bounds).
		if split_bit >= KEY_BITS {
			return None;
		}
		if prev.is_some_and(|p| step.split_bit >= p) {
			return None;
		}
		prev = Some(step.split_bit);
		// The branch direction is the proven key's bit at `split_bit` — this is the key binding: a
		// proof for a different stream folds the sibling on the wrong side and yields a different
		// root.
		node = if bit(&key, split_bit) {
			hash_inner(step.split_bit, &step.sibling, &node)
		} else {
			hash_inner(step.split_bit, &node, &step.sibling)
		};
	}
	Some(StreamsRoot(node))
}

/// Verify that `stream_root` is the sender's committed root for `stream`, against a `StreamsRoot`
/// (read from the sender's header via [`read_streams_root`]). Receiver side.
///
/// The walk is keyed: the leaf is recomputed from `stream`, and each step's direction is taken from
/// `stream`'s bit at that branch — so a proof built for a different stream cannot verify.
pub fn verify_stream_membership(
	streams_root: StreamsRoot,
	stream: StreamId,
	stream_root: Hash,
	membership: &StreamProof,
) -> bool {
	streams_root_from_proof(stream, stream_root, membership) == Some(streams_root)
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::H256;

	fn ch(recipient: u32) -> StreamId {
		StreamId::Channel { recipient: recipient.into(), domain: 0, num: 0 }
	}
	fn root(b: u8) -> Hash {
		H256::repeat_byte(b)
	}

	fn entries() -> Vec<(StreamId, Hash)> {
		// deliberately unsorted — the primitives must canonicalize
		vec![
			(ch(4000), root(0xB)),
			(ch(2000), root(0xA)),
			(ch(3000), root(0xC)),
			(StreamId::Ack { recipient: 2000.into(), domain: 0, num: 0 }, root(0xD)),
			(StreamId::Broadcast { domain: 1, subdomain: 0, num: 7 }, root(0xE)),
		]
	}

	#[test]
	fn streams_root_frozen_vector() {
		// FROZEN consensus vector: this fixed entry set must always hash to this exact
		// `StreamsRoot`. Any change to the trie node encoding — leaf/inner tags, `split_bit`,
		// entry ordering, or the key binding — is a consensus break and breaks this test.
		// (Round-trip tests can't catch a silent byte-format change; this pins the bytes.)
		// Companion to `StreamId`'s frozen encoding.
		assert_eq!(
			streams_root(entries()).unwrap(),
			StreamsRoot(H256(hex_literal::hex!(
				"12de235fb6d96933d6ba770efe16f891c87fb8a88a75e37a384b88ba11dee0dc"
			))),
		);
	}

	#[test]
	fn round_trip_membership_verifies() {
		for (stream, sr) in entries() {
			let (r, proof) = gen_stream_proof(entries(), stream).unwrap();
			assert_eq!(Some(r), streams_root(entries()));
			assert!(verify_stream_membership(r, stream, sr, &proof), "{stream:?} must verify");
		}
	}

	#[test]
	fn wrong_root_or_stream_or_value_fails() {
		let (r, proof) = gen_stream_proof(entries(), ch(3000)).unwrap();
		// wrong stream_root for the (correct) stream
		assert!(!verify_stream_membership(r, ch(3000), root(0xFF), &proof));
		// right value/proof but claimed for a different stream (key binding must reject)
		assert!(!verify_stream_membership(r, ch(2000), root(0xC), &proof));
		// wrong streams root
		assert!(!verify_stream_membership(StreamsRoot(root(0x00)), ch(3000), root(0xC), &proof));
	}

	#[test]
	fn proof_is_not_transferable_between_streams() {
		// A proof minted for one stream must not verify for any other stream, even with that
		// other stream's own committed value — the keyed walk binds the StreamId.
		let (r, proof) = gen_stream_proof(entries(), ch(4000)).unwrap();
		for (other, other_root) in entries() {
			if other == ch(4000) {
				continue;
			}
			assert!(
				!verify_stream_membership(r, other, other_root, &proof),
				"proof for ch(4000) must not verify as {other:?}"
			);
		}
	}

	#[test]
	fn order_independent_root() {
		let mut reversed = entries();
		reversed.reverse();
		assert_eq!(streams_root(entries()), streams_root(reversed));
	}

	#[test]
	fn single_stream_tree_has_empty_proof() {
		let one = vec![(ch(2000), root(0xA))];
		let (r, proof) = gen_stream_proof(one.clone(), ch(2000)).unwrap();
		assert!(proof.steps.is_empty());
		assert_eq!(Some(r), streams_root(one));
		assert!(verify_stream_membership(r, ch(2000), root(0xA), &proof));
	}

	#[test]
	fn proof_is_logarithmic() {
		// 64 clustered channels: a Patricia proof is O(log S), never the 64-bit key width.
		let many: Vec<_> = (0..64u32).map(|i| (ch(2000 + i), root(i as u8))).collect();
		let (_r, proof) = gen_stream_proof(many, ch(2000 + 33)).unwrap();
		assert!(proof.steps.len() <= 7, "log2(64) = 6-ish, got {}", proof.steps.len());
	}

	#[test]
	fn absent_stream_has_no_proof() {
		assert!(gen_stream_proof(entries(), ch(9999)).is_none());
	}

	#[test]
	fn out_of_range_split_bit_is_rejected() {
		let (r, mut proof) = gen_stream_proof(entries(), ch(3000)).unwrap();
		// Splice the bogus step in FIRST (largest bit position), keeping the strictly
		// decreasing order intact — so the range check, not the order check, rejects it.
		let mut steps = vec![TreeStep { split_bit: 64, sibling: root(0) }];
		steps.extend(proof.steps.iter().copied());
		proof.steps = steps.try_into().unwrap();
		assert!(!verify_stream_membership(r, ch(3000), root(0xC), &proof));
	}

	#[test]
	fn over_long_proof_is_rejected_at_decode() {
		// A valid proof can never exceed the trie depth (`KEY_BITS`), so the container is
		// bounded in the type and longer wire input dies at decode, before any hashing.
		let raw = vec![TreeStep { split_bit: 0, sibling: root(0) }; KEY_BITS + 1].encode();
		assert!(StreamProof::decode(&mut &raw[..]).is_err());
	}

	#[test]
	fn non_decreasing_step_order_is_rejected() {
		// Wire order is leaf-to-root, split bits strictly decreasing; a reversed sequence
		// is early garbage even though every hash in it is genuine.
		let (r, proof) = gen_stream_proof(entries(), ch(3000)).unwrap();
		assert!(proof.steps.len() >= 2, "need a multi-step proof");
		let reversed: Vec<TreeStep> = proof.steps.iter().rev().copied().collect();
		let bad = StreamProof { steps: reversed.try_into().unwrap() };
		assert!(!verify_stream_membership(r, ch(3000), root(0xC), &bad));
	}

	#[test]
	fn read_streams_root_extracts_digest() {
		let r = streams_root(entries()).unwrap();
		let digest = Digest {
			logs: vec![
				DigestItem::Other(vec![1, 2, 3]),
				DigestItem::Consensus(SPMS_ENGINE_ID, r.encode()),
			],
		};
		assert_eq!(read_streams_root(&digest), Some(r));
		assert_eq!(read_streams_root(&Digest { logs: vec![] }), None);
	}
}
