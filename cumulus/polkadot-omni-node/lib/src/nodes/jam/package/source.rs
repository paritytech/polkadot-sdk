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

//! What a fresh block's work package is assembled from, and the prerequisite it names.

use crate::nodes::jam::{
	package::{build_pov, extrinsic_spec, work_package, work_package_hash, PackageParams},
	store::{hash_ledger::WpHashLedger, pov_store::BlockPov},
	LOG_TARGET,
};
use cumulus_client_jam_authorizer::authorizer::AuraAuthorizer;
use cumulus_primitives_core::CumulusDigestItem;
use jam_interface::{CoreIndex, Slot as JamSlot, WorkPackage, WorkPackageHash};
use jam_types::{ExtrinsicSpec, RefineContext, VecSet};
use parachain_service_core::authorizer::Authorizer;
use sc_client_api::backend::AuxStore;
use sp_additional_data::AdditionalData;
use sp_runtime::traits::{Block as BlockT, Header as HeaderT};
use sp_trie::CompactProof;

/// The prerequisite a package for a block must name: the work package this node submitted for
/// the block's parent, when the ledger still remembers it.
///
/// A miss starts a new chain, and so does a ledger read failure — a broken link only costs the
/// link, while dropping the block over it would lose the block for good. One prerequisite is all
/// a linear chain ever needs, well under the protocol's cap of eight.
pub(crate) fn prerequisites_for_parent<C: AuxStore>(
	ledger: &WpHashLedger<C>,
	parent_hash: [u8; 32],
) -> VecSet<WorkPackageHash> {
	match ledger.get(&parent_hash) {
		Ok(Some(parent_wp_hash)) => vec![parent_wp_hash].into(),
		Ok(None) => Default::default(),
		Err(error) => {
			tracing::warn!(
				target: LOG_TARGET,
				?error,
				?parent_hash,
				"Unable to read the parent's work-package hash; starting a new chain.",
			);
			Default::default()
		},
	}
}

/// Record the hash of the package submitted for `block_hash` in the ledger and return it, so the
/// block's child can name it as its prerequisite.
///
/// A write failure must not drop the block: the only consequence is that the child starts a new
/// chain, so it is logged and swallowed.
pub(crate) fn record_submitted_hash<C: AuxStore>(
	ledger: &WpHashLedger<C>,
	block_hash: [u8; 32],
	package: &WorkPackage,
) -> WorkPackageHash {
	let wp_hash = work_package_hash(package);
	if let Err(error) = ledger.insert(&block_hash, wp_hash) {
		tracing::warn!(
			target: LOG_TARGET,
			?error,
			?block_hash,
			?wp_hash,
			"Unable to record the work-package hash; this block's child starts a new chain.",
		);
	}
	wp_hash
}

/// The parts a package is assembled from: the built block(s), the parachain storage proof
/// witnessing them, the parent header (travels in the V4 PoV), and the work-item settings.
pub(crate) struct PackageSource<Block: BlockT> {
	pub(crate) blocks: Vec<Block>,
	pub(crate) proof: CompactProof,
	/// SCALE-encoded header of the block `blocks[0]` extends. Travels untrusted in the V4 PoV;
	/// the parachain service's accumulate verifies it against its stored head.
	pub(crate) parent_header: Block::Header,
	/// The additional-data map assembled at build time, carried in the V4 PoV's
	/// `additional_data` slot for `blocks[0]`.
	pub(crate) additional_data: AdditionalData,
	pub(crate) validation_code_hash: [u8; 32],
	/// The service settings the work item is assembled from.
	pub(crate) params: PackageParams,
	/// The para's AURA authorizer. Every core this para runs on holds
	/// `blake2b(code_hash ‖ config)` of exactly this in its pool, so it is as much a part of the
	/// package's identity as the block inside it.
	pub(crate) authorizer: Authorizer,
}

/// A refine context plus the anchor-derived submission target.
#[derive(Clone)]
pub(crate) struct Anchored {
	pub(crate) context: RefineContext,
	/// The anchor's timeslot — the start of the window the package has to be reported in.
	pub(crate) anchor_slot: JamSlot,
	/// The core the pool scan at this anchor named, if any. Anchor-derived like everything else
	/// here, and fixed for the package's life.
	pub(crate) submit_target: Option<CoreIndex>,
}

impl Anchored {
	/// The anchoring of a fresh block's package: `context` with the work package this node
	/// submitted for the block's parent as its prerequisite (see [`prerequisites_for_parent`]).
	///
	/// The prerequisite goes in before the package is signed: the token covers the context, so a
	/// prerequisite set afterwards would leave the signature over the wrong bytes and every
	/// guarantor would reject the package.
	pub(crate) fn after_parent<C: AuxStore>(
		ledger: &WpHashLedger<C>,
		parent_hash: [u8; 32],
		context: RefineContext,
		anchor_slot: JamSlot,
		submit_target: Option<CoreIndex>,
	) -> Self {
		Anchored {
			context: RefineContext {
				prerequisites: prerequisites_for_parent(ledger, parent_hash),
				..context
			},
			anchor_slot,
			submit_target,
		}
	}
}

impl<Block: BlockT> PackageSource<Block> {
	/// Assemble the work package for `anchored`, still unauthorized, and hand back the PoV it
	/// carries.
	///
	/// The PoV travels as work-item extrinsic 0, not in the payload: CE 133 caps the first message
	/// (core index plus package, payloads included) at 200 KiB, while extrinsics ride the bulk
	/// channel, bounded only by `max_input`. The payload's `ParachainCandidate` keeps its
	/// `validation_code` — the parachain service still reads that — and carries no PoV.
	///
	/// The token cannot be built here: it signs a hash of the finished package, so authorizing is
	/// the step after this one
	/// ([`authorized_package`](Self::authorized_package)).
	pub(crate) fn package(&self, anchored: &Anchored) -> (WorkPackage, Vec<u8>) {
		let pov = build_pov(
			&self.blocks,
			&self.proof,
			&self.parent_header,
			&self.additional_data,
			Vec::new(),
		);
		let spec = extrinsic_spec(&pov);
		let package = work_package(
			vec![spec],
			self.validation_code_hash,
			&self.params,
			self.authorizer.clone(),
			anchored.context.clone(),
		);
		(package, pov)
	}

	/// Assemble the package for `anchored` and sign it as this collator.
	///
	/// One step, always taken together: an unsigned package is one no guarantor will look at, and
	/// the signature is over the finished package, so the two cannot be reordered.
	pub(crate) fn authorized_package(
		&self,
		anchored: &Anchored,
		authorizer: &AuraAuthorizer,
	) -> Result<(WorkPackage, Vec<u8>), String> {
		let (mut package, pov) = self.package(anchored);
		authorizer.authorize(&mut package)?;
		Ok((package, pov))
	}

	/// What the PoV store keeps for the block once its package carries `spec` as the PoV: the
	/// entry the import path records for a block it re-executes. `None` for a block without a
	/// `JamParent` digest, which the import path does not record either.
	pub(crate) fn into_block_pov(self, spec: ExtrinsicSpec) -> Option<BlockPov<Block>> {
		let block = self.blocks.into_iter().next()?;
		let header = block.header();
		let parent_hash = *header.parent_hash();
		let number = *header.number();
		let jam_parent = CumulusDigestItem::find_jam_parent_info(header.digest())?;
		Some(BlockPov {
			block,
			parent_header: self.parent_header,
			parent_hash,
			number,
			spec,
			jam_parent,
			compact_proof: self.proof,
			additional_data: self.additional_data,
		})
	}
}
