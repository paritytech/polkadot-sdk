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

//! The messaging inherent's data: payloads fetched off-chain, handed to the
//! receiver runtime for consumption by recomputation.
//!
//! Deliberately NO roots of any kind and no relay state: the receiver
//! runtime hashes what it is given and appends to its frontiers — binding
//! the resulting endpoints to committed sender roots is the `validate_block`
//! wrapper's job, via the consumption record and POV-carried lifts
//! ([`crate::record`], [`crate::lift`]). A tampered payload merely yields an
//! endpoint no lift can bind; node-side pre-verification (fetching under an
//! included `StreamsRoot`) is what protects the honest collator.

use alloc::vec::Vec;
use polkadot_parachain_primitives::primitives::Id as ParaId;

use crate::{mmr::MmrInclusionProof, stream_id::StreamId};

/// The identifier of the Speculative Messaging inherent (an
/// `sp_inherents::InherentIdentifier`).
pub const INHERENT_IDENTIFIER: [u8; 8] = *b"specmsg0";

/// What the collator feeds the receiver runtime per block: fetched channel
/// payloads and register head reads. Built node-side by the inherent data
/// provider from material verified under included source roots; the runtime
/// re-verifies everything it needs by recomputation.
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
pub struct SpecMsgInherentData {
	/// Ordered payloads per consumed channel stream — at most one item per
	/// stream (the STF rejects duplicates). No roots of any kind.
	pub messages: Vec<(ParaId, StreamId, Vec<Vec<u8>>)>,
	/// Register head reads: the latest leaf of the named ack stream plus
	/// the MMR inclusion proof pinning its placement (position and peaks).
	pub register_reads: Vec<(ParaId, StreamId, Vec<u8>, MmrInclusionProof)>,
}

impl SpecMsgInherentData {
	/// `true` if there is nothing to process. An empty set produces no
	/// inherent at all — an absent inherent is a valid block that consumes
	/// nothing.
	pub fn is_empty(&self) -> bool {
		self.messages.is_empty() && self.register_reads.is_empty()
	}
}

/// The node-side half of the inherent, mirroring `ParachainInherentData`:
/// the receiver's inherent data provider (built by the fetch pipeline from
/// material verified under included source roots) folds itself into the
/// authoring inherent data under [`INHERENT_IDENTIFIER`].
///
/// An empty set puts NO data at all: `create_inherent` then produces no
/// call, and the block simply carries no spec-msg inherent.
#[cfg(feature = "std")]
#[async_trait::async_trait]
impl sp_inherents::InherentDataProvider for SpecMsgInherentData {
	async fn provide_inherent_data(
		&self,
		inherent_data: &mut sp_inherents::InherentData,
	) -> Result<(), sp_inherents::Error> {
		if self.is_empty() {
			return Ok(());
		}
		inherent_data.put_data(INHERENT_IDENTIFIER, self)
	}

	async fn try_handle_error(
		&self,
		_: &sp_inherents::InherentIdentifier,
		_: &[u8],
	) -> Option<Result<(), sp_inherents::Error>> {
		None
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::mmr::MmrInclusionProof;
	use sp_inherents::InherentDataProvider;

	#[test]
	fn empty_data_puts_nothing_non_empty_round_trips() {
		// Empty pool → no `put_data` at all: an absent inherent is a valid
		// block that consumes nothing.
		let mut inherent_data = sp_inherents::InherentData::new();
		futures::executor::block_on(
			SpecMsgInherentData::default().provide_inherent_data(&mut inherent_data),
		)
		.expect("providing empty data succeeds");
		assert_eq!(
			inherent_data.get_data::<SpecMsgInherentData>(&INHERENT_IDENTIFIER).unwrap(),
			None
		);

		// Non-empty data round-trips through the inherent bytes.
		let data = SpecMsgInherentData {
			messages: alloc::vec![(
				ParaId::from(2000),
				StreamId::Channel { recipient: 2001.into(), domain: 0, num: 0 },
				alloc::vec![alloc::vec![1, 2, 3]],
			)],
			register_reads: alloc::vec![(
				ParaId::from(2000),
				StreamId::Ack { recipient: 2001.into(), domain: 0, num: 0 },
				alloc::vec![4, 5],
				MmrInclusionProof { mmr_size: 1, items: alloc::vec![] },
			)],
		};
		let mut inherent_data = sp_inherents::InherentData::new();
		futures::executor::block_on(data.provide_inherent_data(&mut inherent_data))
			.expect("providing data succeeds");
		assert_eq!(
			inherent_data.get_data::<SpecMsgInherentData>(&INHERENT_IDENTIFIER).unwrap(),
			Some(data)
		);
	}
}
