// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
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

//! Aura (Authority-Round) digests
//!
//! This implements the digests for AuRa, to allow the private
//! `CompatibleDigestItem` trait to appear in public interfaces.

use crate::AURA_ENGINE_ID;
use codec::{Codec, Encode};
use sp_consensus_slots::Slot;
use sp_runtime::generic::{Digest, DigestItem};

/// A digest item which is usable with aura consensus.
pub trait CompatibleDigestItem<Signature>: Sized {
	/// Construct a digest item which contains a signature on the hash.
	fn aura_seal(signature: Signature) -> Self;

	/// If this item is an Aura seal, return the signature.
	fn as_aura_seal(&self) -> Option<Signature>;

	/// Construct a digest item which contains the slot number
	fn aura_pre_digest(slot: Slot) -> Self;

	/// If this item is an AuRa pre-digest, return the slot number
	fn as_aura_pre_digest(&self) -> Option<Slot>;
}

impl<Signature> CompatibleDigestItem<Signature> for DigestItem
where
	Signature: Codec,
{
	fn aura_seal(signature: Signature) -> Self {
		DigestItem::Seal(AURA_ENGINE_ID, signature.encode())
	}

	fn as_aura_seal(&self) -> Option<Signature> {
		self.seal_try_to(&AURA_ENGINE_ID)
	}

	fn aura_pre_digest(slot: Slot) -> Self {
		DigestItem::PreRuntime(AURA_ENGINE_ID, slot.encode())
	}

	fn as_aura_pre_digest(&self) -> Option<Slot> {
		self.pre_runtime_try_to(&AURA_ENGINE_ID)
	}
}

/// Errors in seal lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SealLookupError {
	/// The digest is empty.
	Unsealed,
	/// The last digest item is not a valid AuRa seal.
	BadSeal,
	/// There are multiple AuRa seals.
	MultipleSeals,
}

/// Errors in pre-digest lookup.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreDigestLookupError {
	/// Multiple AuRa pre-runtime digests
	MultiplePreDigests,
	/// No valid Aura pre-runtime digest found
	NoDigestFound,
}

impl core::fmt::Display for PreDigestLookupError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			Self::MultiplePreDigests => write!(f, "Multiple AuRa pre-runtime digests"),
			Self::NoDigestFound => write!(f, "No valid Aura pre-runtime digest found"),
		}
	}
}

impl core::error::Error for PreDigestLookupError {}

impl core::fmt::Display for SealLookupError {
	fn fmt(&self, f: &mut core::fmt::Formatter) -> core::fmt::Result {
		match self {
			Self::Unsealed => write!(f, "The digest is empty"),
			Self::BadSeal => write!(f, "The last digest item is not a valid AuRa seal"),
			Self::MultipleSeals => write!(f, "There are multiple AuRa seals"),
		}
	}
}

impl core::error::Error for SealLookupError {}

/// Removes the AuRa seal from `digest`.
///
/// The seal must be the last digest item and the only AuRa seal in `digest`. Returns the removed
/// digest item and the signature it contains.
pub fn take_seal<Signature: Codec>(
	digest: &mut Digest,
) -> Result<(DigestItem, Signature), SealLookupError> {
	let seal = digest.pop().ok_or(SealLookupError::Unsealed)?;
	let signature =
		CompatibleDigestItem::<Signature>::as_aura_seal(&seal).ok_or(SealLookupError::BadSeal)?;

	if digest
		.logs()
		.iter()
		.any(|d| matches!(d, DigestItem::Seal(id, _) if *id == AURA_ENGINE_ID))
	{
		return Err(SealLookupError::MultipleSeals);
	}

	Ok((seal, signature))
}

/// Returns the slot of the AuRa pre-runtime digest.
///
/// There must be exactly one AuRa pre-runtime digest in `digest` and it must decode to a [`Slot`].
pub fn find_pre_digest(digest: &Digest) -> Result<Slot, PreDigestLookupError> {
	let mut pre_digests = digest
		.logs()
		.iter()
		.filter(|d| matches!(d, DigestItem::PreRuntime(id, _) if *id == AURA_ENGINE_ID));

	let pre_digest = pre_digests.next().ok_or(PreDigestLookupError::NoDigestFound)?;

	if pre_digests.next().is_some() {
		return Err(PreDigestLookupError::MultiplePreDigests);
	}

	CompatibleDigestItem::<()>::as_aura_pre_digest(pre_digest)
		.ok_or(PreDigestLookupError::NoDigestFound)
}

#[cfg(test)]
mod tests {
	use super::*;

	type Signature = u64;

	fn seal(sig: Signature) -> DigestItem {
		CompatibleDigestItem::aura_seal(sig)
	}

	fn pre_digest(slot: u64) -> DigestItem {
		CompatibleDigestItem::<Signature>::aura_pre_digest(slot.into())
	}

	fn digest(logs: Vec<DigestItem>) -> Digest {
		Digest { logs }
	}

	#[test]
	fn take_seal_works() {
		let other = DigestItem::Other(vec![1]);
		let mut d = digest(vec![pre_digest(1), other.clone(), seal(5)]);

		assert_eq!(take_seal::<Signature>(&mut d), Ok((seal(5), 5)));
		assert_eq!(d, digest(vec![pre_digest(1), other]));
	}

	#[test]
	fn take_seal_rejects_invalid_seals() {
		let trailing_bytes = DigestItem::Seal(AURA_ENGINE_ID, (5u64, 1u8).encode());
		let other_engine = DigestItem::Seal(*b"babe", 5u64.encode());

		for (logs, expected) in [
			(vec![], SealLookupError::Unsealed),
			(vec![pre_digest(1)], SealLookupError::BadSeal),
			(vec![seal(5), pre_digest(1)], SealLookupError::BadSeal),
			(vec![seal(4), pre_digest(1), seal(5)], SealLookupError::MultipleSeals),
			(vec![pre_digest(1), trailing_bytes], SealLookupError::BadSeal),
			(vec![pre_digest(1), other_engine], SealLookupError::BadSeal),
		] {
			assert_eq!(
				take_seal::<Signature>(&mut digest(logs.clone())),
				Err(expected),
				"{logs:?}"
			);
		}
	}

	#[test]
	fn find_pre_digest_works() {
		let other_engine = DigestItem::PreRuntime(*b"babe", 2u64.encode());

		assert_eq!(find_pre_digest(&digest(vec![other_engine, pre_digest(1)])), Ok(1.into()));
	}

	#[test]
	fn find_pre_digest_rejects_invalid_pre_digests() {
		let trailing_bytes = DigestItem::PreRuntime(AURA_ENGINE_ID, (1u64, 1u8).encode());

		for (logs, expected) in [
			(vec![], PreDigestLookupError::NoDigestFound),
			(vec![trailing_bytes.clone()], PreDigestLookupError::NoDigestFound),
			(vec![pre_digest(1), pre_digest(1)], PreDigestLookupError::MultiplePreDigests),
			(vec![trailing_bytes, pre_digest(1)], PreDigestLookupError::MultiplePreDigests),
		] {
			assert_eq!(find_pre_digest(&digest(logs.clone())), Err(expected), "{logs:?}");
		}
	}
}
