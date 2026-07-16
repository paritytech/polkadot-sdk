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

//! Requires lifts: the POV-carried proofs binding a block's recorded
//! consumption to currently committed `StreamsRoot`s.
//!
//! A lift is a pure function of *public* data — the source's streams and its
//! committed roots. It carries no signature and nothing secret, so anyone
//! can generate it and assemble a valid candidate around the unaltered
//! block; every (re)submission regenerates the lifts against the
//! then-current provides, so a block never goes stale.

use alloc::vec::Vec;
use polkadot_parachain_primitives::primitives::Id as ParaId;

use crate::{mmr::MMRExtensionProof, tree::TreeInclusionProof};

/// One lift, carried in the POV (never in the block or commitments), for
/// one stream of the consumption record.
///
/// Matched positionally to the record's streams within each source — the
/// record supplies the [`crate::StreamId`], and a mispaired lift cannot
/// verify: the tree walk binds the record's key, so landing on a committed
/// root means being a valid lift for exactly that stream.
///
/// On the hot path (single block, caught up) `advances` and `extension` are
/// empty and the lift is a bare tree proof, ~300 B per stream.
#[derive(
	Clone,
	codec::Encode,
	codec::Decode,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct RequiresLift {
	/// One proof per gap in the stream's interval chain, in gap order (see
	/// the wrapper's stitching); empty for prefix streams and
	/// single-context reads.
	pub advances: Vec<MMRExtensionProof>,
	/// Extends the chain's endpoint to the stream's current state;
	/// verification *yields* the current stream root. Empty when the
	/// endpoint already is the target root's entry.
	pub extension: MMRExtensionProof,
	/// Walked from the computed stream root, *yields* the `StreamsRoot` the
	/// requires entry becomes — validated by the relay chain's window
	/// match.
	pub tree_proof: TreeInclusionProof,
}

/// Canonical transport of a candidate's lifts: per source, positionally
/// matched to the consumption record's streams of that source (which are in
/// [`crate::StreamId`]'s canonical order).
///
/// Manual `Decode` REJECTS non-strictly-increasing `ParaId`s — same
/// canonicality discipline as `RequiresSet`: the bytes come from an
/// untrusted submitter and must have exactly one valid form.
#[derive(
	Clone,
	codec::Encode,
	codec::DecodeWithMemTracking,
	Debug,
	Default,
	Eq,
	PartialEq,
	scale_info::TypeInfo,
)]
pub struct LiftsBySource(Vec<(ParaId, Vec<RequiresLift>)>);

impl codec::Decode for LiftsBySource {
	fn decode<I: codec::Input>(input: &mut I) -> Result<Self, codec::Error> {
		let inner = Vec::<(ParaId, Vec<RequiresLift>)>::decode(input)?;

		for pair in inner.windows(2) {
			if pair[0].0 >= pair[1].0 {
				return Err(codec::Error::from(
					"LiftsBySource entries must be sorted by strictly increasing ParaId",
				));
			}
		}

		Ok(Self(inner))
	}
}

impl LiftsBySource {
	/// Builds from an arbitrary (possibly unordered) iterator, sorting by
	/// source and rejecting duplicates.
	pub fn try_from_iter(
		it: impl IntoIterator<Item = (ParaId, Vec<RequiresLift>)>,
	) -> Result<Self, LiftsError> {
		let mut entries: Vec<(ParaId, Vec<RequiresLift>)> = it.into_iter().collect();
		entries.sort_by_key(|(source, _)| *source);

		if entries.windows(2).any(|w| w[0].0 == w[1].0) {
			return Err(LiftsError::DuplicateSource);
		}

		Ok(Self(entries))
	}

	/// The lifts of `source`, or `None` if absent.
	pub fn get(&self, source: ParaId) -> Option<&[RequiresLift]> {
		self.0
			.binary_search_by_key(&source, |(id, _)| *id)
			.ok()
			.map(|idx| self.0[idx].1.as_slice())
	}

	/// Iterates all `(source, lifts)` entries in sorted order.
	pub fn iter(&self) -> impl Iterator<Item = &(ParaId, Vec<RequiresLift>)> {
		self.0.iter()
	}

	/// Number of sources.
	pub fn len(&self) -> usize {
		self.0.len()
	}

	/// `true` if no source carries lifts.
	pub fn is_empty(&self) -> bool {
		self.0.is_empty()
	}
}

/// Errors constructing a [`LiftsBySource`].
#[derive(Debug, PartialEq, Eq)]
pub enum LiftsError {
	/// The same source appears more than once.
	DuplicateSource,
}

#[cfg(test)]
mod tests {
	use super::*;
	use codec::{Decode, Encode};

	fn lift() -> RequiresLift {
		RequiresLift::default()
	}

	#[test]
	fn try_from_iter_sorts_and_rejects_duplicates() {
		let lifts = LiftsBySource::try_from_iter([
			(ParaId::from(2), alloc::vec![lift()]),
			(ParaId::from(1), alloc::vec![lift(), lift()]),
		])
		.unwrap();
		let sources: Vec<ParaId> = lifts.iter().map(|(id, _)| *id).collect();
		assert_eq!(sources, alloc::vec![ParaId::from(1), ParaId::from(2)]);
		assert_eq!(lifts.get(ParaId::from(1)).unwrap().len(), 2);
		assert_eq!(lifts.get(ParaId::from(3)), None);

		assert_eq!(
			LiftsBySource::try_from_iter([
				(ParaId::from(1), alloc::vec![]),
				(ParaId::from(1), alloc::vec![]),
			]),
			Err(LiftsError::DuplicateSource)
		);
	}

	#[test]
	fn decode_rejects_unsorted_sources() {
		let ok = LiftsBySource::try_from_iter([
			(ParaId::from(1), alloc::vec![lift()]),
			(ParaId::from(2), alloc::vec![]),
		])
		.unwrap();
		let encoded = ok.encode();
		assert_eq!(LiftsBySource::decode(&mut &encoded[..]).unwrap(), ok);

		let bad: Vec<(ParaId, Vec<RequiresLift>)> =
			alloc::vec![(ParaId::from(2), alloc::vec![]), (ParaId::from(1), alloc::vec![])];
		assert!(LiftsBySource::decode(&mut &bad.encode()[..]).is_err());

		let dup: Vec<(ParaId, Vec<RequiresLift>)> =
			alloc::vec![(ParaId::from(1), alloc::vec![]), (ParaId::from(1), alloc::vec![])];
		assert!(LiftsBySource::decode(&mut &dup.encode()[..]).is_err());
	}
}
