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

//! Verification for BABE headers.
use schnorrkel::vrf::{VRFOutput, VRFProof};
use sr_primitives::{traits::Header, traits::DigestItemFor};
use primitives::{Pair, Public};
use babe_primitives::{Epoch, BabePreDigest, CompatibleDigestItem, AuthorityId};
use babe_primitives::{AuthoritySignature, SlotNumber, AuthorityIndex, AuthorityPair};
use slots::CheckedHeader;
use log::{debug, trace};
use super::{find_pre_digest, BlockT};
use super::authorship::{make_transcript, calculate_primary_threshold, check_primary_threshold, secondary_slot_author};

/// BABE verification parameters
pub(super) struct VerificationParams<'a, B: 'a + BlockT> {
	/// the header being verified.
	pub(super) header: B::Header,
	/// the pre-digest of the header being verified. this is optional - if prior
	/// verification code had to read it, it can be included here to avoid duplicate
	/// work.
	pub(super) pre_digest: Option<BabePreDigest>,
	/// the slot number of the current time.
	pub(super) slot_now: SlotNumber,
	/// epoch descriptor of the epoch this block _should_ be under, if it's valid.
	pub(super) epoch: &'a Epoch,
	/// genesis config of this BABE chain.
	pub(super) config: &'a super::Config,
}

macro_rules! babe_err {
	($($i: expr),+) => {
		{
			debug!(target: "babe", $($i),+);
			format!($($i),+)
		}
	};
}

/// Check a header has been signed by the right key. If the slot is too far in
/// the future, an error will be returned. If successful, returns the pre-header
/// and the digest item containing the seal.
///
/// The seal must be the last digest.  Otherwise, the whole header is considered
/// unsigned.  This is required for security and must not be changed.
///
/// This digest item will always return `Some` when used with `as_babe_pre_digest`.
///
/// The given header can either be from a primary or secondary slot assignment,
/// with each having different validation logic.
pub(super) fn check_header<B: BlockT + Sized>(
	params: VerificationParams<B>,
) -> Result<CheckedHeader<B::Header, VerifiedHeaderInfo<B>>, String> where
	DigestItemFor<B>: CompatibleDigestItem,
{
	let VerificationParams {
		mut header,
		pre_digest,
		slot_now,
		epoch,
		config,
	} = params;

	let authorities = &epoch.authorities;
	let pre_digest = pre_digest.map(Ok).unwrap_or_else(|| find_pre_digest::<B::Header>(&header))?;

	trace!(target: "babe", "Checking header");
	let seal = match header.digest_mut().pop() {
		Some(x) => x,
		None => return Err(babe_err!("Header {:?} is unsealed", header.hash())),
	};

	let sig = seal.as_babe_seal().ok_or_else(|| {
		babe_err!("Header {:?} has a bad seal", header.hash())
	})?;

	// the pre-hash of the header doesn't include the seal
	// and that's what we sign
	let pre_hash = header.hash();

	if pre_digest.slot_number() > slot_now {
		header.digest_mut().push(seal);
		return Ok(CheckedHeader::Deferred(header, pre_digest.slot_number()));
	}

	let author = match authorities.get(pre_digest.authority_index() as usize) {
		Some(author) => author.0.clone(),
		None => return Err(babe_err!("Slot author not found")),
	};

	match &pre_digest {
		BabePreDigest::Primary { vrf_output, vrf_proof, authority_index, slot_number } => {
			debug!(target: "babe", "Verifying Primary block");

			let digest = (vrf_output, vrf_proof, *authority_index, *slot_number);

			check_primary_header::<B>(
				pre_hash,
				digest,
				sig,
				&epoch,
				config.c,
			)?;
		},
		BabePreDigest::Secondary { authority_index, slot_number } if config.secondary_slots => {
			debug!(target: "babe", "Verifying Secondary block");

			let digest = (*authority_index, *slot_number);

			check_secondary_header::<B>(
				pre_hash,
				digest,
				sig,
				&epoch,
			)?;
		},
		_ => {
			return Err(babe_err!("Secondary slot assignments are disabled for the current epoch."));
		}
	}

	let info = VerifiedHeaderInfo {
		pre_digest: CompatibleDigestItem::babe_pre_digest(pre_digest),
		seal,
		author,
	};
	Ok(CheckedHeader::Checked(header, info))
}

pub(super) struct VerifiedHeaderInfo<B: BlockT> {
	pub(super) pre_digest: DigestItemFor<B>,
	pub(super) seal: DigestItemFor<B>,
	pub(super) author: AuthorityId,
}

/// Check a primary slot proposal header. We validate that the given header is
/// properly signed by the expected authority, and that the contained VRF proof
/// is valid. Additionally, the weight of this block must increase compared to
/// its parent since it is a primary block.
fn check_primary_header<B: BlockT + Sized>(
	pre_hash: B::Hash,
	pre_digest: (&VRFOutput, &VRFProof, AuthorityIndex, SlotNumber),
	signature: AuthoritySignature,
	epoch: &Epoch,
	c: (u64, u64),
) -> Result<(), String> {
	let (vrf_output, vrf_proof, authority_index, slot_number) = pre_digest;

	let author = &epoch.authorities[authority_index as usize].0;

	if AuthorityPair::verify(&signature, pre_hash, &author) {
		let (inout, _) = {
			let transcript = make_transcript(
				&epoch.randomness,
				slot_number,
				epoch.epoch_index,
			);

			schnorrkel::PublicKey::from_bytes(author.as_slice()).and_then(|p| {
				p.vrf_verify(transcript, vrf_output, vrf_proof)
			}).map_err(|s| {
				babe_err!("VRF verification failed: {:?}", s)
			})?
		};

		let threshold = calculate_primary_threshold(
			c,
			&epoch.authorities,
			authority_index as usize,
		);

		if !check_primary_threshold(&inout, threshold) {
			return Err(babe_err!("VRF verification of block by author {:?} failed: \
								  threshold {} exceeded", author, threshold));
		}

		Ok(())
	} else {
		Err(babe_err!("Bad signature on {:?}", pre_hash))
	}
}

/// Check a secondary slot proposal header. We validate that the given header is
/// properly signed by the expected authority, which we have a deterministic way
/// of computing. Additionally, the weight of this block must stay the same
/// compared to its parent since it is a secondary block.
fn check_secondary_header<B: BlockT>(
	pre_hash: B::Hash,
	pre_digest: (AuthorityIndex, SlotNumber),
	signature: AuthoritySignature,
	epoch: &Epoch,
) -> Result<(), String> {
	let (authority_index, slot_number) = pre_digest;

	// check the signature is valid under the expected authority and
	// chain state.
	let expected_author = secondary_slot_author(
		slot_number,
		&epoch.authorities,
		epoch.randomness,
	).ok_or_else(|| "No secondary author expected.".to_string())?;

	let author = &epoch.authorities[authority_index as usize].0;

	if expected_author != author {
		let msg = format!("Invalid author: Expected secondary author: {:?}, got: {:?}.",
			expected_author,
			author,
		);

		return Err(msg);
	}

	if AuthorityPair::verify(&signature, pre_hash.as_ref(), author) {
		Ok(())
	} else {
		Err(format!("Bad signature on {:?}", pre_hash))
	}
}
