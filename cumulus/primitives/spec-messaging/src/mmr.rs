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

//! MMR primitives for the Speculative Messaging protocol.
//!
//! Hashing is [`SpecHasher`] with the crate's domain tags: leaves by [`leaf_hash`], inner nodes by
//! [`SpecMerge::merge`], peak bagging by [`SpecMerge::merge_peaks`] (`mmr_lib` calls it only when
//! bagging), stream roots by [`MmrRoot::from_bag`].
//!
//! [`SpecMerge`] is the `mmr_lib::Merge` for inclusion proofs (`gen_proof` / `MerkleProof::verify`)
//! and ancestry proofs (`gen_ancestry_proof` / `verify_incremental`). [`MmrFrontier`] is the
//! peaks-only state: the sender's on-chain accumulator, and what proofs extend from.

use alloc::vec::Vec;
use codec::{Decode, DecodeWithMemTracking, Encode, Error as CodecError, Input, MaxEncodedLen};
use mmr_lib::{
	ancestry_proof::bagging_peaks_hashes, helper::get_peaks, Error as MmrError, Merge,
	NodeMerkleProof,
};
use polkadot_core_primitives::Hash;
use scale_info::TypeInfo;
use sp_core::ConstU32;
use sp_runtime::{traits::Hash as HashT, BoundedVec};

use crate::{SpecHasher, INNER_TAG, LEAF_TAG, LEAF_VERSION, PEAK_TAG, ROOT_TAG};

/// Upper bound on any leaf count node positions are derived from; `mmr_lib`'s node count overflows
/// `u64` above 2^63. Peer-supplied counts are checked against this first.
pub const MAX_MMR_LEAF_COUNT: u64 = 1 << 48;

/// Upper bound on a peak set or connecting-node list: one hash per bit of [`MAX_MMR_LEAF_COUNT`].
pub const MAX_MMR_HASHES: u32 = 48;

/// A leaf position within a stream's MMR.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	PartialOrd,
	Ord,
	Debug,
	Default,
	TypeInfo,
)]
pub struct MessagePosition(pub u64);

/// Domain-tagged `mmr_lib::Merge` for the speculative-messaging MMR.
pub struct SpecMerge;

impl Merge for SpecMerge {
	type Item = Hash;

	fn merge(left: &Hash, right: &Hash) -> Result<Hash, MmrError> {
		Ok(tagged_node(INNER_TAG, left, right))
	}

	fn merge_peaks(left: &Hash, right: &Hash) -> Result<Hash, MmrError> {
		Ok(tagged_node(PEAK_TAG, left, right))
	}
}

fn tagged_node(tag: u8, left: &Hash, right: &Hash) -> Hash {
	let mut preimage = [0u8; 1 + 32 + 32];
	preimage[0] = tag;
	preimage[1..33].copy_from_slice(left.as_bytes());
	preimage[33..65].copy_from_slice(right.as_bytes());
	<SpecHasher as HashT>::hash(&preimage)
}

/// Hash a payload into a stream MMR leaf at [`LEAF_VERSION`]. Source, stream
/// and position are structural and not in the preimage.
pub fn leaf_hash(payload: &[u8]) -> Hash {
	leaf_hash_with_version(LEAF_VERSION, payload)
}

/// Hash a payload into a stream MMR leaf at an explicit leaf version. Used to test format
/// disjointness across format revisions.
pub fn leaf_hash_with_version(leaf_version: u8, payload: &[u8]) -> Hash {
	let mut preimage = Vec::with_capacity(2 + payload.len());
	preimage.push(LEAF_TAG);
	preimage.push(leaf_version);
	preimage.extend_from_slice(payload);
	<SpecHasher as HashT>::hash(&preimage)
}

/// The root of an empty MMR.
pub fn empty_root() -> MmrRoot {
	MmrRoot::from_bag(0, Hash::zero())
}

/// Bag the peaks (highest to lowest) with `mmr_lib`'s own bagging, so the result equals
/// `MMR::get_root`. Not a stream root (see [`MmrRoot::from_bag`]). No peaks bag to zero.
pub fn bag_peaks(peaks: &[Hash]) -> Hash {
	// `SpecMerge` is infallible, so `Err` means no peaks.
	bagging_peaks_hashes::<Hash, SpecMerge>(peaks.to_vec()).unwrap_or_default()
}

/// [`MmrFrontier::try_append`] refused a leaf: the frontier already holds [`MAX_MMR_LEAF_COUNT`].
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct FrontierFull;

/// A stream's committed root at some point in its history ([`Self::from_bag`]). A newtype so it
/// cannot be confused with a `StreamsRoot`, the commitment-tree root.
#[derive(
	Clone,
	Copy,
	Encode,
	Decode,
	DecodeWithMemTracking,
	MaxEncodedLen,
	PartialEq,
	Eq,
	Debug,
	TypeInfo,
)]
pub struct MmrRoot(pub Hash);

impl MmrRoot {
	/// Peak hashes do not commit their heights, so the same peaks bag identically at many leaf
	/// counts; binding the count makes a forged count, and so a forged position, yield a root
	/// nothing commits to.
	pub fn from_bag(leaf_count: u64, bag: Hash) -> Self {
		let mut preimage = [0u8; 1 + 8 + 32];
		preimage[0] = ROOT_TAG;
		preimage[1..9].copy_from_slice(&leaf_count.to_le_bytes());
		preimage[9..41].copy_from_slice(bag.as_bytes());
		Self(<SpecHasher as HashT>::hash(&preimage))
	}
}

/// A peaks-only MMR frontier: the O(log n) state a stream continues from, and the accumulator that
/// continues it. `leaf_count` fixes where an extension proof's connecting nodes go.
///
/// Peak hashes equal `mmr_lib`'s, so [`root`](Self::root) is `mmr_lib::MMR::get_root` bound to the
/// leaf count, and proofs from a full `mmr_lib::MMR` over the same leaves verify against it.
///
/// Fields are private, so every frontier holds the [`from_parts`](Self::from_parts) rules.
#[derive(Clone, Encode, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo, Default)]
pub struct MmrFrontier {
	/// Number of leaves these peaks summarize; also the position of the next message to append.
	leaf_count: u64,
	/// MMR peaks, highest to lowest. At most [`MAX_MMR_HASHES`].
	peaks: BoundedVec<Hash, ConstU32<MAX_MMR_HASHES>>,
}

impl MmrFrontier {
	pub fn new() -> Self {
		Self::default()
	}

	/// A frontier from `(peaks, leaf_count)`. `None` unless there is one peak per set bit of
	/// `leaf_count` and `leaf_count <= MAX_MMR_LEAF_COUNT`.
	pub fn from_parts(peaks: Vec<Hash>, leaf_count: u64) -> Option<Self> {
		if leaf_count > MAX_MMR_LEAF_COUNT || peaks.len() != leaf_count.count_ones() as usize {
			return None;
		}
		// ≤ 48 is implied by the set-bit count up to `MAX_MMR_LEAF_COUNT`, so this cannot fail.
		BoundedVec::try_from(peaks).ok().map(|peaks| Self { leaf_count, peaks })
	}

	/// The peaks, highest to lowest.
	pub fn peaks(&self) -> &[Hash] {
		&self.peaks
	}

	pub fn leaf_count(&self) -> u64 {
		self.leaf_count
	}

	/// Append a leaf, merging equal-height peaks as `mmr_lib` does. At [`MAX_MMR_LEAF_COUNT`] the
	/// leaf is refused and the frontier left unchanged, so every frontier still decodes.
	pub fn try_append(&mut self, leaf: Hash) -> Result<(), FrontierFull> {
		if self.leaf_count >= MAX_MMR_LEAF_COUNT {
			return Err(FrontierFull);
		}
		let mut node = leaf;
		// Going from `leaf_count` to `leaf_count + 1` merges exactly
		// `trailing_zeros(leaf_count + 1)` pairs of peaks (binary-counter carry).
		let merges = (self.leaf_count + 1).trailing_zeros();
		for _ in 0..merges {
			let left = self
				.peaks
				.pop()
				.expect("the constructors enforce one peak per set bit of `leaf_count`; qed");
			node = <SpecMerge as Merge>::merge(&left, &node).expect("SpecMerge is infallible; qed");
		}
		self.peaks
			.try_push(node)
			.expect("a leaf count up to `MAX_MMR_LEAF_COUNT` never has more than 48 set bits; qed");
		self.leaf_count += 1;
		Ok(())
	}

	/// [`try_append`](Self::try_append) for a frontier this chain built itself, from the empty
	/// frontier one leaf at a time.
	///
	/// # Panics
	///
	/// At [`MAX_MMR_LEAF_COUNT`], which no stream reaches: `2^48` leaves are beyond any chain's
	/// lifetime. Never call it on a frontier built from untrusted parts.
	pub fn append(&mut self, leaf: Hash) {
		self.try_append(leaf)
			.expect("a frontier built leaf by leaf never reaches `MAX_MMR_LEAF_COUNT`; qed")
	}

	/// The stream root ([`MmrRoot::from_bag`]). The empty frontier bags to zero ([`empty_root`]),
	/// so a frontier that has consumed nothing compares and extends like any other.
	pub fn root(&self) -> MmrRoot {
		MmrRoot::from_bag(self.leaf_count, bag_peaks(&self.peaks))
	}

	/// The `mmr_lib` node count (size) of this frontier's MMR. Total under the leaf-count
	/// invariant.
	pub(crate) fn mmr_size(&self) -> u64 {
		mmr_size(self.leaf_count)
	}
}

/// The `mmr_lib` node count (size) of an MMR with `leaf_count` leaves. Total up to
/// [`MAX_MMR_LEAF_COUNT`].
pub(crate) fn mmr_size(leaf_count: u64) -> u64 {
	if leaf_count == 0 {
		0
	} else {
		mmr_lib::leaf_index_to_mmr_size(leaf_count - 1)
	}
}

/// Field order matches the derived `Encode`; the decoded pair goes through
/// [`from_parts`](MmrFrontier::from_parts).
impl Decode for MmrFrontier {
	fn decode<I: Input>(input: &mut I) -> Result<Self, CodecError> {
		let leaf_count = u64::decode(input)?;
		let peaks = BoundedVec::<Hash, ConstU32<MAX_MMR_HASHES>>::decode(input)?;
		Self::from_parts(peaks.into_inner(), leaf_count)
			.ok_or_else(|| "MmrFrontier: inconsistent peaks / leaf count".into())
	}
}

impl DecodeWithMemTracking for MmrFrontier {}

/// Why an MMR proof verification rejected its input. Typed so callers can tell a malformed or
/// forged proof from a non-forward or out-of-range one.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProofError {
	/// The proof is structurally malformed: a wrong connecting-node count, or `mmr_lib` could not
	/// reconstruct a root from it.
	InvalidProof,
	/// An extension does not go strictly forward (its target is not newer than the frontier).
	NotForward,
	/// A leaf count exceeds [`MAX_MMR_LEAF_COUNT`].
	LeafCountTooLarge,
}

/// An append-only MMR extension proof: O(log n) connecting nodes that, with a frontier's own peaks,
/// reconstruct a newer root. The `mmr_lib` `gen_ancestry_proof` form of the design's
/// `connecting_nodes`. Verification computes the new root; it is never declared alongside the
/// proof. It holds node hashes only, no message payloads.
#[derive(
	Clone, Encode, Decode, DecodeWithMemTracking, MaxEncodedLen, PartialEq, Eq, Debug, TypeInfo,
)]
pub struct MMRExtensionProof {
	/// Leaf count of the extended MMR. The `mmr_lib` node count is derived from it, so no
	/// independent size can be smuggled in. The frontier's own count with empty
	/// `connecting_nodes` is the identity extension.
	/// Compact-encoded, since this is PoV-carried.
	#[codec(compact)]
	pub leaf_count: u64,
	/// The connecting-node hashes, in `mmr_lib`'s proof order. Positions are not stored: an MMR's
	/// shape is fixed by its size, so they are derived at verify time from the two leaf counts
	/// (`ancestry_proof_positions`). Empty for the identity.
	pub connecting_nodes: BoundedVec<Hash, ConstU32<MAX_MMR_HASHES>>,
}

impl MMRExtensionProof {
	/// The identity extension of `from`: its own leaf count and no connecting nodes.
	pub fn identity(from: &MmrFrontier) -> Self {
		Self { leaf_count: from.leaf_count(), connecting_nodes: BoundedVec::new() }
	}

	/// Exact encoded byte size of a skip/extension proof from `n1` leaves to `n2` leaves, computed
	/// from the same `ancestry_proof_positions` count that [`Self::calculate_root`] enforces.
	///
	/// Returns `Some(0)` when `n1 == n2` (no gap, so no proof is added to `advances`), and `None`
	/// outside `1 <= n1 <= n2 <= MAX_MMR_LEAF_COUNT`. Never panics.
	pub fn encoded_len_for(n1: u64, n2: u64) -> Option<usize> {
		if n1 == 0 || n1 > n2 || n2 > MAX_MMR_LEAF_COUNT {
			return None;
		}
		if n1 == n2 {
			return Some(0);
		}
		let nodes = mmr_lib::ancestry_proof::ancestry_proof_positions(mmr_size(n1), mmr_size(n2))
			.ok()?
			.len();
		Some(
			codec::Compact(n2).encoded_size() +
				codec::Compact(nodes as u32).encoded_size() +
				nodes * Hash::len_bytes(),
		)
	}

	/// Extend `from` and return the new root, computed from `from`'s peaks and the connecting
	/// nodes; `Err` if the proof does not fit that placement. The identity extension yields
	/// `from`'s own root. Comparing the root is the caller's job. `from`'s contents are not
	/// re-checked: a forged peak yields a root no committed `StreamsRoot` can bind.
	pub fn calculate_root(&self, from: &MmrFrontier) -> Result<MmrRoot, ProofError> {
		// `mmr_lib` accepts counts up to 2^63; bound the proof's to `MAX_MMR_LEAF_COUNT` first.
		// `from` is already in range by construction.
		if self.leaf_count > MAX_MMR_LEAF_COUNT {
			return Err(ProofError::LeafCountTooLarge);
		}
		if self.leaf_count < from.leaf_count() {
			return Err(ProofError::NotForward);
		}
		// The identity: the same count with no connecting nodes. This includes the empty frontier.
		if self.leaf_count == from.leaf_count() {
			return if self.connecting_nodes.is_empty() {
				Ok(from.root())
			} else {
				Err(ProofError::InvalidProof)
			};
		}
		let new_mmr_size = mmr_size(self.leaf_count);

		// Extending from the empty stream: nothing binds, so the connecting nodes must be the new
		// MMR's peaks in `get_peaks` order.
		if from.leaf_count() == 0 {
			// The nodes must form a frontier of exactly `self.leaf_count` leaves; `from_parts`
			// checks that.
			return MmrFrontier::from_parts(self.connecting_nodes.to_vec(), self.leaf_count)
				.map(|f| f.root())
				.ok_or(ProofError::InvalidProof);
		}

		// `from`'s peaks are complete subtrees of the new MMR at unchanged positions; with the
		// connecting nodes they reconstruct the new root under `SpecMerge`.
		let old_mmr_size = from.mmr_size();
		let old_positions = get_peaks(old_mmr_size);
		// Positions are not carried: derive them from the two sizes and pair them with the hashes
		// in `mmr_lib`'s proof order. A length mismatch means the proof does not fit the claimed
		// extension.
		let positions =
			mmr_lib::ancestry_proof::ancestry_proof_positions(old_mmr_size, new_mmr_size)
				.map_err(|_| ProofError::InvalidProof)?;
		if positions.len() != self.connecting_nodes.len() {
			return Err(ProofError::InvalidProof);
		}
		let proof: Vec<(u64, Hash)> =
			positions.into_iter().zip(self.connecting_nodes.iter().copied()).collect();
		let nodes: Vec<(u64, Hash)> =
			old_positions.into_iter().zip(from.peaks().iter().copied()).collect();
		NodeMerkleProof::<Hash, SpecMerge>::new(new_mmr_size, proof)
			.calculate_root(nodes)
			.map(|bag| MmrRoot::from_bag(self.leaf_count, bag))
			.map_err(|_| ProofError::InvalidProof)
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::test_utils::{extension, frontier_at, leaves, mmr_size};
	use mmr_lib::{
		leaf_index_to_pos,
		util::{MemMMR, MemStore},
		MerkleProof,
	};
	use sp_core::H256;
	fn h(byte: u8) -> Hash {
		Hash::repeat_byte(byte)
	}

	/// Root over the first `k` leaves.
	fn root_at(all: &[Hash], k: usize) -> MmrRoot {
		frontier_at(all, k).root()
	}

	#[test]
	fn merge_and_merge_peaks_are_domain_separated() {
		let a = h(1);
		let b = h(2);
		// Inner-node merge and peak-bagging of the same inputs must differ (domain tags).
		assert_ne!(
			<SpecMerge as Merge>::merge(&a, &b).unwrap(),
			<SpecMerge as Merge>::merge_peaks(&a, &b).unwrap()
		);
	}

	#[test]
	fn merge_is_order_sensitive() {
		let a = h(1);
		let b = h(2);
		assert_ne!(
			<SpecMerge as Merge>::merge(&a, &b).unwrap(),
			<SpecMerge as Merge>::merge(&b, &a).unwrap()
		);
	}

	#[test]
	fn from_parts_enforces_append_invariants() {
		// Consistent pairs round-trip, including the empty accumulator.
		assert!(MmrFrontier::from_parts(Vec::new(), 0).is_some());
		assert!(MmrFrontier::from_parts(vec![h(1)], 1).is_some());
		assert!(MmrFrontier::from_parts(vec![h(1), h(2)], 3).is_some());
		// One peak per set bit, or `append` would pop from an empty vec.
		assert!(MmrFrontier::from_parts(Vec::new(), 1).is_none());
		assert!(MmrFrontier::from_parts(vec![h(1)], 3).is_none());
		assert!(MmrFrontier::from_parts(vec![h(1), h(2)], 1).is_none());
		// And within the ceiling, or the node-position arithmetic downstream is not total.
		assert!(MmrFrontier::from_parts(vec![h(1)], MAX_MMR_LEAF_COUNT).is_some());
		assert!(MmrFrontier::from_parts(vec![h(1)], 1 << 63).is_none());
		assert!(MmrFrontier::from_parts(vec![h(1); MAX_MMR_HASHES as usize], u64::MAX).is_none());
	}

	#[test]
	fn accumulator_root_matches_mmr_lib() {
		// Build via the peaks-only accumulator and via a full mmr_lib MMR; the
		// roots and peak-count invariant must agree.
		let mut acc = MmrFrontier::new();
		let store = MemStore::<Hash>::default();
		let mut reference = MemMMR::<Hash, SpecMerge>::new(0, &store);
		for i in 1..=5u8 {
			acc.append(h(i));
			reference.push(h(i)).unwrap();
			assert_eq!(acc.peaks().len() as u32, acc.leaf_count().count_ones());
		}
		assert_eq!(acc.root(), MmrRoot::from_bag(5, reference.get_root().unwrap()));
	}

	#[test]
	fn empty_accumulator_root_is_empty_root() {
		// A fresh accumulator is valid, and its root is the defined constant, not a panic in
		// `bag_peaks`.
		assert_eq!(bag_peaks(&[]), Hash::zero());
		assert_eq!(MmrFrontier::new().root(), empty_root());
		assert_eq!(MmrFrontier::from_parts(Vec::new(), 0).unwrap().root(), empty_root());
	}

	#[test]
	fn decode_enforces_the_same_invariants_as_from_parts() {
		// A frontier off the wire goes through `from_parts`, so it holds the invariants `append`
		// and the position arithmetic rely on. Consistent bytes round-trip;
		let good = MmrFrontier::from_parts(vec![h(1), h(2)], 3).unwrap();
		assert_eq!(MmrFrontier::decode(&mut &good.encode()[..]).unwrap(), good);
		// … inconsistent ones are rejected at decode, not later in a pop or a multiply.
		let bad_shape = (3u64, vec![h(1)]).encode();
		assert!(MmrFrontier::decode(&mut &bad_shape[..]).is_err());
		let too_large = (1u64 << 63, vec![h(1)]).encode();
		assert!(MmrFrontier::decode(&mut &too_large[..]).is_err());
		// The bound is checked at the length prefix: 49 peaks are refused before being read.
		let too_many = (u64::MAX, vec![h(1); MAX_MMR_HASHES as usize + 1]).encode();
		assert!(MmrFrontier::decode(&mut &too_many[..]).is_err());
		// And the wire order is the design's: `leaf_count` first.
		assert_eq!(good.encode(), (3u64, vec![h(1), h(2)]).encode());
	}

	#[test]
	fn empty_root_frozen_vector() {
		// FROZEN consensus vector: blake2b-256 of `ROOT_TAG = 0x04`, a zero `u64` count and a zero
		// bag, the defined root of an empty frontier.
		assert_eq!(
			empty_root().0,
			Hash::from(hex_literal::hex!(
				"3efd2c3528b8f163c2e2349211af73f86181b748be1b6a5eebfbadc9428643ce"
			))
		);
	}

	#[test]
	fn mmr_root_frozen_vector() {
		// Frozen consensus vector for tags `LEAF/INNER/PEAK/ROOT = 0x1/0x2/0x3/0x4`: a change to
		// the tags or the bagging order is a consensus break and fails this.
		let mut acc = MmrFrontier::new();
		for i in 1..=5u8 {
			acc.append(h(i));
		}
		assert_eq!(
			acc.root().0,
			Hash::from(hex_literal::hex!(
				"1190cb02577a8bafec9f05c38e64ca9155b65e9d862604c4f3d920dff4e5a85f"
			))
		);
	}

	#[test]
	fn mmr_root_frozen_vectors_at_64_and_65_leaves() {
		// Frozen vectors, computed by an independent Python model: 64 leaves make one peak, 65 add
		// a second to bag.
		let mut acc = MmrFrontier::new();
		for i in 1..=64u8 {
			acc.append(h(i));
		}
		assert_eq!(acc.peaks().len(), 1);
		assert_eq!(
			acc.root().0,
			Hash::from(hex_literal::hex!(
				"41cec1de3c0e0af93a02932404fc195f0fbb56f542abbce0681b4632da0848c2"
			))
		);
		acc.append(h(65));
		assert_eq!(acc.peaks().len(), 2);
		assert_eq!(
			acc.root().0,
			Hash::from(hex_literal::hex!(
				"109ccf71e0b753f6ff76d7f48e59f6376c1347bea4556b20c4a453a2cb8ca28d"
			))
		);
	}

	#[test]
	fn no_append_carries_a_frontier_past_the_ceiling() {
		// A frontier built from untrusted parts can sit at the ceiling. Its next leaf is refused
		// and the frontier left as it was, so it still decodes; one below, the last leaf fits.
		let mut full = MmrFrontier::from_parts(vec![h(1)], MAX_MMR_LEAF_COUNT).unwrap();
		assert_eq!(full.try_append(h(2)), Err(FrontierFull));
		assert_eq!(full.leaf_count(), MAX_MMR_LEAF_COUNT);
		assert_eq!(MmrFrontier::decode(&mut &full.encode()[..]).unwrap(), full);

		let below = MAX_MMR_LEAF_COUNT - 1;
		let mut last =
			MmrFrontier::from_parts(vec![h(1); below.count_ones() as usize], below).unwrap();
		assert_eq!(last.try_append(h(2)), Ok(()));
		assert_eq!(last.leaf_count(), MAX_MMR_LEAF_COUNT);
		assert_eq!(MmrFrontier::decode(&mut &last.encode()[..]).unwrap(), last);
	}

	#[test]
	fn inclusion_proof_round_trips() {
		let store = MemStore::<Hash>::default();
		let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
		let positions: Vec<u64> = (0..6u8).map(|i| mmr.push(h(i)).unwrap()).collect();
		let root = mmr.get_root().unwrap();

		let proof: MerkleProof<Hash, SpecMerge> =
			mmr.gen_proof(vec![leaf_index_to_pos(1), leaf_index_to_pos(4)]).unwrap();

		assert!(proof.verify(root, vec![(positions[1], h(1)), (positions[4], h(4))]).unwrap());
		assert!(!proof.verify(root, vec![(positions[1], h(99)), (positions[4], h(4))]).unwrap());
	}

	#[test]
	fn extension_computes_root_and_rejects_tampering() {
		let all = leaves(5);
		let ext = extension(&all, 2, 5);
		// Computes root@5 from frontier@2 + O(log n) connecting nodes (never a declared root).
		assert_eq!(ext.calculate_root(&frontier_at(&all, 2)), Ok(root_at(&all, 5)));
		// Wrong starting frontier → does not reconstruct the extended root.
		assert_ne!(ext.calculate_root(&frontier_at(&all, 3)), Ok(root_at(&all, 5)));
		// Tampered connecting node → a different computed root.
		let mut bad = ext.clone();
		bad.connecting_nodes[0] = H256::repeat_byte(0xff);
		assert_ne!(bad.calculate_root(&frontier_at(&all, 2)), Ok(root_at(&all, 5)));
		// Identity extension yields the frontier's own root.
		let id = MMRExtensionProof::identity(&frontier_at(&all, 3));
		assert_eq!(id.calculate_root(&frontier_at(&all, 3)), Ok(root_at(&all, 3)));
	}

	/// `connecting_nodes` stores bare hashes, so the positions we derive must match what
	/// `gen_ancestry_proof` emits. Checked for every `(old, new)` pair up to 256 leaves, which
	/// includes the seven- and eight-peak sizes 127 and 255. A change in `mmr_lib` trips this;
	/// `Cargo.lock` pins it.
	#[test]
	fn ancestry_positions_matches_mmr_lib() {
		for n in 2..=256usize {
			let all = leaves(n as u64);
			let store = MemStore::<Hash>::default();
			let mut mmr = MemMMR::<Hash, SpecMerge>::new(0, &store);
			for l in &all {
				mmr.push(*l).unwrap();
			}
			for k in 1..n {
				let ap = mmr.gen_ancestry_proof(mmr_size(k)).unwrap();
				let expected: Vec<u64> =
					ap.prev_peaks_proof.proof_items().iter().map(|(p, _)| *p).collect();
				let got =
					mmr_lib::ancestry_proof::ancestry_proof_positions(mmr_size(k), mmr_size(n))
						.unwrap();
				assert_eq!(got, expected, "position mismatch at k={k}, n={n}");
			}
		}
	}

	#[test]
	fn extension_leaf_count_disambiguates_equal_node_counts() {
		// 1→3 and 1→4 both take two nodes, placed differently, so only `leaf_count` tells them
		// apart.
		let all = leaves(4);
		let to3 = extension(&all, 1, 3);
		let to4 = extension(&all, 1, 4);
		assert_eq!(to3.connecting_nodes.len(), 2);
		assert_eq!(to4.connecting_nodes.len(), 2);
		let from1 = frontier_at(&all, 1);
		assert_eq!(to3.calculate_root(&from1), Ok(root_at(&all, 3)));
		assert_eq!(to4.calculate_root(&from1), Ok(root_at(&all, 4)));
		// The same two nodes under the other count place differently and reach neither root.
		let relabelled =
			MMRExtensionProof { leaf_count: 4, connecting_nodes: to3.connecting_nodes };
		let got = relabelled.calculate_root(&from1);
		assert_ne!(got, Ok(root_at(&all, 3)));
		assert_ne!(got, Ok(root_at(&all, 4)));
	}

	#[test]
	fn extension_verify_rejects_non_forward() {
		let all = leaves(5);
		// A real extension pointed *back* below the frontier's size is rejected up front, before
		// any crypto work.
		let mut ext = extension(&all, 2, 5);
		ext.leaf_count = 1;
		assert_eq!(ext.calculate_root(&frontier_at(&all, 2)), Err(ProofError::NotForward));
	}

	#[test]
	fn extension_verify_bounds_untrusted_leaf_counts() {
		let all = leaves(5);
		// The proof's own count. A value whose derived node count would overflow `u64` is rejected
		// before any position is computed from it.
		let mut ext = extension(&all, 2, 5);
		ext.leaf_count = u64::MAX;
		assert_eq!(ext.calculate_root(&frontier_at(&all, 2)), Err(ProofError::LeafCountTooLarge));
		ext.leaf_count = MAX_MMR_LEAF_COUNT + 1;
		assert_eq!(ext.calculate_root(&frontier_at(&all, 2)), Err(ProofError::LeafCountTooLarge));
		// The frontier's count is not this function's to check. An out-of-range `MmrFrontier`
		// cannot be built, from `from_parts` or from `Decode`.
		assert!(MmrFrontier::from_parts(vec![all[0]], 1 << 63).is_none());
	}

	#[test]
	fn identity_verifies_to_the_frontier_root() {
		let all = leaves(5);
		for f in [MmrFrontier::new(), frontier_at(&all, 1), frontier_at(&all, 5)] {
			let id = MMRExtensionProof::identity(&f);
			assert_eq!(id.leaf_count, f.leaf_count());
			assert!(id.connecting_nodes.is_empty());
			assert_eq!(id.calculate_root(&f), Ok(f.root()));
		}
		assert_eq!(
			MMRExtensionProof::identity(&MmrFrontier::new()).calculate_root(&MmrFrontier::new()),
			Ok(empty_root()),
		);
	}

	#[test]
	fn empty_extension_is_only_the_identity() {
		let all = leaves(5);
		let f = frontier_at(&all, 3);
		let empty =
			|leaf_count| MMRExtensionProof { leaf_count, connecting_nodes: BoundedVec::new() };
		// Below the frontier, including `0` on a non-empty one.
		assert_eq!(empty(0).calculate_root(&f), Err(ProofError::NotForward));
		assert_eq!(empty(2).calculate_root(&f), Err(ProofError::NotForward));
		// Above it, no nodes cannot reach the new root.
		assert_eq!(empty(4).calculate_root(&f), Err(ProofError::InvalidProof));
		assert_eq!(empty(5).calculate_root(&f), Err(ProofError::InvalidProof));
		assert_eq!(
			empty(MAX_MMR_LEAF_COUNT + 1).calculate_root(&f),
			Err(ProofError::LeafCountTooLarge)
		);
		// From the empty frontier any non-zero count without peaks is malformed.
		assert_eq!(empty(1).calculate_root(&MmrFrontier::new()), Err(ProofError::InvalidProof));
	}

	#[test]
	fn equal_count_with_connecting_nodes_is_invalid() {
		let all = leaves(5);
		for f in [MmrFrontier::new(), frontier_at(&all, 3)] {
			let mut id = MMRExtensionProof::identity(&f);
			id.connecting_nodes.try_push(h(1)).unwrap();
			assert_eq!(id.calculate_root(&f), Err(ProofError::InvalidProof));
		}
	}

	#[test]
	fn the_longest_valid_extension_fits_the_node_ceiling() {
		// Worst case, one node per set bit: 1 → the largest all-ones count. The lift's PoV
		// reservation relies on it.
		let nodes = mmr_lib::ancestry_proof::ancestry_proof_positions(
			mmr_size(1),
			mmr_size((MAX_MMR_LEAF_COUNT - 1) as usize),
		)
		.unwrap()
		.len();
		assert_eq!(nodes, 48);
		assert!(nodes <= MAX_MMR_HASHES as usize);
	}

	#[test]
	fn over_long_extension_proof_fails_to_decode() {
		// The bound is in the type, so an over-long proof is rejected at the length prefix.
		let max = MAX_MMR_HASHES as usize;
		let at_bound = (codec::Compact(5u64), vec![H256::zero(); max]).encode();
		assert!(MMRExtensionProof::decode(&mut &at_bound[..]).is_ok());
		let over = (codec::Compact(5u64), vec![H256::zero(); max + 1]).encode();
		assert!(MMRExtensionProof::decode(&mut &over[..]).is_err());
	}

	#[test]
	fn leaf_hash_is_payload_only_and_version_disjoint() {
		let payload = b"hello";
		// Matches the explicit preimage `H(LEAF_TAG ++ version ++ payload)`.
		let mut pre = Vec::new();
		pre.push(LEAF_TAG);
		pre.push(0u8);
		pre.extend_from_slice(payload);
		assert_eq!(leaf_hash(payload), <SpecHasher as HashT>::hash(&pre));
		assert_eq!(leaf_hash(payload), leaf_hash_with_version(0, payload));
		// Different version → different leaf (hash-disjoint domains).
		assert_ne!(leaf_hash(payload), leaf_hash_with_version(1, payload));
	}

	#[test]
	fn leaf_hash_frozen_vector() {
		// FROZEN consensus vector: `H(0x01 ‖ 0x00 ‖ "hello")`. Any change to `LEAF_TAG`,
		// `LEAF_VERSION`, the preimage layout or the hasher breaks this.
		assert_eq!(
			leaf_hash(b"hello"),
			Hash::from(hex_literal::hex!(
				"cd31917fb8992dae762dbaaf276d8eb65aa89cdfb87daf69e05f8c08b490e78b"
			))
		);
	}
}
