// Copyright 2019 Parity Technologies (UK) Ltd.
// This file is part of Substrate.

// Substrate is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Substrate is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Substrate.  If not, see <http://www.gnu.org/licenses/>.

//! Private implementation details of BABE digests.

use primitives::sr25519::Signature;
use babe_primitives::{self, BABE_ENGINE_ID, SlotNumber};
use runtime_primitives::{DigestItem, generic::OpaqueDigestItemId};
use std::fmt::Debug;
use parity_codec::{Decode, Encode, Codec, Input};
use schnorrkel::{vrf::{VRFProof, VRFOutput, VRF_OUTPUT_LENGTH, VRF_PROOF_LENGTH}};

/// A BABE pre-digest.  It includes:
///
/// * The public key of the author.
/// * The VRF proof.
/// * The VRF output.
/// * The slot number.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BabePreDigest {
	pub(super) vrf_output: VRFOutput,
	pub(super) proof: VRFProof,
	pub(super) index: babe_primitives::AuthorityIndex,
	pub(super) slot_num: SlotNumber,
}

/// The prefix used by BABE for its VRF keys.
pub const BABE_VRF_PREFIX: &'static [u8] = b"substrate-babe-vrf";

type RawBabePreDigest = (
	[u8; VRF_OUTPUT_LENGTH],
	[u8; VRF_PROOF_LENGTH],
	u64,
	u64,
);

impl Encode for BabePreDigest {
	fn encode(&self) -> Vec<u8> {
		let tmp: RawBabePreDigest = (
			*self.vrf_output.as_bytes(),
			self.proof.to_bytes(),
			self.index,
			self.slot_num,
		);
		parity_codec::Encode::encode(&tmp)
	}
}

impl Decode for BabePreDigest {
	fn decode<R: Input>(i: &mut R) -> Option<Self> {
		let (output, proof, index, slot_num): RawBabePreDigest = Decode::decode(i)?;

		// Verify (at compile time) that the sizes in babe_primitives are correct
		let _: [u8; babe_primitives::VRF_OUTPUT_LENGTH] = output;
		let _: [u8; babe_primitives::VRF_PROOF_LENGTH] = proof;
		Some(BabePreDigest {
			proof: VRFProof::from_bytes(&proof).ok()?,
			vrf_output: VRFOutput::from_bytes(&output).ok()?,
			index,
			slot_num,
		})
	}
}

/// A digest item which is usable with BABE consensus.
pub trait CompatibleDigestItem: Sized {
	/// Construct a digest item which contains a BABE pre-digest.
	fn babe_pre_digest(seal: BabePreDigest) -> Self;

	/// If this item is an BABE pre-digest, return it.
	fn as_babe_pre_digest(&self) -> Option<BabePreDigest>;

	/// Construct a digest item which contains a BABE seal.
	fn babe_seal(signature: Signature) -> Self;

	/// If this item is a BABE signature, return the signature.
	fn as_babe_seal(&self) -> Option<Signature>;
}

impl<Hash> CompatibleDigestItem for DigestItem<Hash> where
	Hash: Debug + Send + Sync + Eq + Clone + Codec + 'static
{
	fn babe_pre_digest(digest: BabePreDigest) -> Self {
		DigestItem::PreRuntime(BABE_ENGINE_ID, digest.encode())
	}

	fn as_babe_pre_digest(&self) -> Option<BabePreDigest> {
		self.try_to(OpaqueDigestItemId::PreRuntime(&BABE_ENGINE_ID))
	}

	fn babe_seal(signature: Signature) -> Self {
		DigestItem::Seal(BABE_ENGINE_ID, signature.encode())
	}

	fn as_babe_seal(&self) -> Option<Signature> {
		self.try_to(OpaqueDigestItemId::Seal(&BABE_ENGINE_ID))
	}
}
