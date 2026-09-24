// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
// SPDX-License-Identifier: GPL-3.0-or-later WITH Classpath-exception-2.0

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE. See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus. If not, see <https://www.gnu.org/licenses/>.

//! The `builder -> collator` channel: the [`CollatorMessage`] payload and its
//! [`CollatorMessageBuilder`]. Pure data; the collation task builds and submits the collations.

use cumulus_primitives_core::SchedulingProof;
use polkadot_primitives::{
	CoreIndex, Hash as RelayHash, PersistedValidationData, ValidationCodeHash,
};
use sp_api::StorageProof;
use sp_runtime::traits::Block as BlockT;

/// A message from the block builder to the collation task: a single V2 collation, or a V3 segment.
pub(super) enum CollatorMessage<Block: BlockT> {
	/// A single V2 collation (no scheduling proof).
	Collation { core_index: CoreIndex, parts: CollationParts<Block> },
	/// A V3 segment: collations sharing a scheduling parent and target core.
	Segment {
		/// Shared by the whole segment; V3/V4-only, so always present.
		scheduling_proof: SchedulingProof,
		core_index: CoreIndex,
		/// This core's resubmittable-block headers, oldest first. Empty for now.
		// TODO: hydrate these into parts, submitted ahead of `bundle`.
		resubmittable_headers: Vec<Block::Header>,
		/// The freshly-built bundle for this core, if any.
		bundle: Option<CollationParts<Block>>,
	},
}

/// The block builder's output for one collation: the built blocks plus the context the collation
/// task needs to turn them into a `SegmentCollation`.
#[derive(Clone)]
pub(super) struct CollationParts<Block: BlockT> {
	/// Relay block the parachain block(s) execute against.
	pub relay_parent: RelayHash,
	/// Parent of the first block.
	pub parent_header: Block::Header,
	/// The built blocks, in order.
	pub blocks: Vec<Block>,
	/// Storage proof covering all of `blocks`.
	pub proof: StorageProof,
	pub validation_code_hash: ValidationCodeHash,
	pub validation_data: PersistedValidationData,
}

/// Builds a [`CollatorMessage`] for one core: a `Collation` without a scheduling proof, a `Segment`
/// with one.
pub(super) struct CollatorMessageBuilder<Block: BlockT> {
	core_index: CoreIndex,
	bundle: Option<CollationParts<Block>>,
	scheduling_proof: Option<SchedulingProof>,
}

impl<Block: BlockT> CollatorMessageBuilder<Block> {
	pub(super) fn new(core_index: CoreIndex) -> Self {
		Self { core_index, bundle: None, scheduling_proof: None }
	}

	/// Attach the freshly-built bundle.
	pub(super) fn with_bundle(mut self, bundle: CollationParts<Block>) -> Self {
		self.bundle = Some(bundle);
		self
	}

	/// Attach a V3 scheduling proof.
	pub(super) fn with_scheduling_proof(mut self, scheduling_proof: SchedulingProof) -> Self {
		self.scheduling_proof = Some(scheduling_proof);
		self
	}

	/// Build the message. `None` when there is nothing to submit for the core.
	pub(super) fn build(self) -> Option<CollatorMessage<Block>> {
		match self.scheduling_proof {
			Some(scheduling_proof) => {
				let bundle = self.bundle?;

				Some(CollatorMessage::Segment {
					scheduling_proof,
					core_index: self.core_index,
					// Empty for now.
					resubmittable_headers: Vec::new(),
					bundle: Some(bundle),
				})
			},
			None => self
				.bundle
				.map(|parts| CollatorMessage::Collation { core_index: self.core_index, parts }),
		}
	}
}

impl<Block: BlockT> CollationParts<Block> {
	/// The new chain tip (last built block), used to chain the next core's PoV parent.
	pub(super) fn tip_header(&self) -> &Block::Header {
		self.blocks
			.last()
			.expect("collation parts always carry at least one built block; qed")
			.header()
	}
}
