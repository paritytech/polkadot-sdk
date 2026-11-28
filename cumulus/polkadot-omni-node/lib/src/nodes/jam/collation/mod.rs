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

//! The JAM collation manager: the work-package lifecycle for the blocks this collator authors,
//! including resubmission of lost blocks.
//!
//! One task owns everything. It keeps the packages *it* submitted —
//! [`InFlightPackages`](in_flight::InFlightPackages) — and selects over the builder's channel, the
//! para-head stream, the status subscriptions of every submitted package, and the JAM best-block
//! stream. Every new JAM block is one look at β (the recent-blocks history): a package named there
//! is reported, and one whose anchor has expired is forgotten.
//!
//! # New-block path
//!
//! Each block the builder hands over becomes **one work package** with a single extrinsic (the
//! PoV): no imported segment, `export_count = 0`, submitted with `submitWorkPackage`. A package
//! names the work package of the block it builds on as its single prerequisite, when the ledger
//! still remembers it; a miss starts a new chain.
//!
//! # Resubmission
//!
//! When the accumulated head stalls for [`RESUBMIT_STALL_SLOTS`](policy::RESUBMIT_STALL_SLOTS) JAM
//! slots and it is this collator's round-robin turn, a **resubmission round** rebuilds up to
//! [`MAX_RESUBMIT_BLOCKS`](policy::MAX_RESUBMIT_BLOCKS) blocks above the head into new work
//! packages: the leading run of the local best chain above the head whose PoVs the store holds and
//! whose original packages can no longer be reported
//! ([`original_package_expired`](policy::original_package_expired)) — a block whose first package
//! may still land is not rebuilt beside it. Each resubmission package carries the rebuilt PoV as
//! its only extrinsic, and the PoV's `ParachainBlockData` carries the anchor chain (encoded JAM
//! headers, one walk per round cut per package) that lets the PVF verify the block's `JamParent`
//! digest against the current chain. Package *k* > 0 names package *k* − 1's hash as its
//! prerequisite, so the guarantor processes them in order, and any failure ends the round instead
//! of skipping a block, so no package ever names a prerequisite that was never sent.
//!
//! A round's packages are not sent back to back: [pacing](#pacing) releases them one per JAM
//! slot, in prerequisite order, all anchored at the round's tip. The last of
//! [`MAX_RESUBMIT_BLOCKS`](policy::MAX_RESUBMIT_BLOCKS) goes out `MAX_RESUBMIT_BLOCKS - 1` slots
//! after that anchor, well inside [`REPORT_DEADLINE_SLOTS`](policy::REPORT_DEADLINE_SLOTS).
//!
//! The resubmission predicate ([`resubmission_due`](policy::resubmission_due)) is pure and
//! evaluated after every JAM block. It guards against double rounds via `last_round_at` and against
//! rebuilding while a round is already in flight.
//!
//! # Package policy
//!
//! At every new JAM block β says whether a package appeared on chain. If it did, the package
//! is reported; if the anchor has expired it is forgotten; otherwise it waits. A package is
//! sent once — there is no byte-identical resend — and a package is never re-anchored: its
//! block's `JamParent` digest pins it to the anchor it was built against. A lost block travels
//! again only inside a *new* package whose anchor chain proves that old anchor to the PVF (see
//! Resubmission above).
//!
//! # Pacing
//!
//! A core takes at most one report per JAM block, and its guarantors refuse a package outright
//! (`TooBusy`) while they are still busy with earlier ones. So every core gets at most one new
//! package from this collator per JAM slot ([`due_for_sending`](in_flight::due_for_sending)): a
//! package whose core already got one at the current tip slot stays in flight *unsent*, holding its
//! PoV, and goes out at a later JAM block, in submission order per core and never ahead of a
//! prerequisite that is still held. A held package lives under the same rules as a sent one: it
//! expires with its anchor and cascades like any other.
//!
//! Failure handling is per package and cascades: a forgotten package takes every package that
//! named it, so no child outlives the prerequisite it named. The block stays in the local
//! database and the PoV store, the next parachain slot authors on whatever is deepest there,
//! and a resubmission round rebuilds it once its package has expired.
//!
//! Every package runs under the para's own [AURA
//! authorizer](cumulus_client_jam_authorizer::authorizer) and carries a token this collator signs
//! with its aura key.
//!
//! The PoV is NOT zstd-compressed (parasim rejects
//! compressed PoVs; JIP-2 is silent on compression).

pub(crate) mod anchor_chain;
pub(crate) mod in_flight;
pub(crate) mod manager;
pub(crate) mod policy;
pub(crate) mod resubmit;
#[cfg(test)]
mod tests;

use crate::{
	common::{
		types::{JamClient, ParachainBackend},
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::{
		authorizer::AuthoritySource,
		chain::para_head::para_header_stream,
		collation::{
			in_flight::{InFlightPackages, StatusSubscriptions},
			manager::Manager,
			policy::{ForgetWhenAnchorExpires, REPORT_DEADLINE_SLOTS},
		},
		message::{AuthoringHold, JamCollatorMessage},
		package::PackageParams,
		store::{hash_ledger::WpHashLedger, pov_store::BlockPovs},
		LOG_TARGET,
	},
};
use futures::{channel::mpsc, stream::SelectAll, StreamExt};
use jam_interface::{JamChainSource, JamStateSource, JamWorkPackageSubmission, ServiceId};
use polkadot_primitives::Id as ParaId;
use std::{collections::HashMap, sync::Arc};

pub(crate) struct CollationTaskParams<Block: NodeBlock, RuntimeApi, Jam> {
	pub para_client: Arc<JamClient<Block, RuntimeApi>>,
	pub para_backend: Arc<ParachainBackend<Block>>,
	pub jam: Arc<Jam>,
	pub para_id: ParaId,
	pub service_id: ServiceId,
	/// The para's AURA authorizers by anchor slot: every package is signed with the one JAM
	/// authorizes at its own anchor.
	pub authorizers: Arc<dyn AuthoritySource<Block>>,
	pub message_receiver: mpsc::Receiver<JamCollatorMessage<Block>>,
	pub announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	/// The service settings every package is built with.
	pub params: PackageParams,
	/// The PoV store every block this collator authors is recorded in.
	pub block_povs: BlockPovs<Block>,
	/// Shared with the builder: how many in-flight resubmission packages are pending.
	pub hold: AuthoringHold,
}

pub(crate) async fn run_collation_task<Block, RuntimeApi, Jam>(
	params: CollationTaskParams<Block, RuntimeApi, Jam>,
) where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	let CollationTaskParams {
		para_client,
		para_backend,
		jam,
		para_id,
		service_id,
		authorizers,
		mut message_receiver,
		announce_block,
		params,
		block_povs,
		hold,
	} = params;

	let mut para_heads = match para_header_stream::<Block, _>(
		&*jam,
		Arc::clone(&para_client),
		service_id,
		para_id.into(),
		false,
	)
	.await
	{
		Ok(stream) => stream.boxed().fuse(),
		Err(error) => {
			tracing::error!(target: LOG_TARGET, ?error, "Unable to watch the para head.");
			return;
		},
	};

	let mut jam_blocks = match jam.best_block_stream().await {
		Ok(stream) => stream.fuse(),
		Err(error) => {
			tracing::error!(target: LOG_TARGET, ?error, "Unable to watch JAM best blocks.");
			return;
		},
	};

	tracing::info!(
		target: LOG_TARGET,
		?para_id,
		service_id,
		refine_gas_limit = params.refine_gas_limit,
		accumulate_gas_limit = params.accumulate_gas_limit,
		report_deadline_slots = REPORT_DEADLINE_SLOTS,
		"JAM collation task started.",
	);

	// Nothing is tracked after a restart and nothing needs to be: the packages this task lost
	// track of are either already accumulated or lost, and the builder authors from the local
	// database and the accumulated head either way.
	let hash_ledger = WpHashLedger::new(Arc::clone(&para_client));
	let mut manager = Manager {
		para_client,
		para_backend,
		jam,
		authorizers,
		params,
		policy: ForgetWhenAnchorExpires,
		announce_block,
		hash_ledger,
		hold,
		packages: InFlightPackages::new(),
		block_povs,
		included_head: None,
		accumulated_head: None,
		head_changed_at: 0,
		last_round_at: None,
		tip_slot: 0,
		statuses: SelectAll::new(),
		subscriptions: StatusSubscriptions::new(),
		sent_at: HashMap::new(),
	};

	loop {
		futures::select! {
			message = message_receiver.next() => {
				let Some(message) = message else {
					tracing::error!(target: LOG_TARGET, "Builder task is gone; stopping.");
					return;
				};
				manager.on_new_block(message).await;
			},
			head = para_heads.next() => {
				let Some((head, slot)) = head else {
					tracing::error!(target: LOG_TARGET, "Para-head stream ended; stopping.");
					return;
				};
				manager.on_para_head(&head, slot);
			},
			tip = jam_blocks.next() => {
				let Some(tip) = tip else {
					tracing::error!(target: LOG_TARGET, "JAM best-block stream ended; stopping.");
					return;
				};
				manager.on_jam_block(tip).await;
			},
			update = manager.statuses.next() => {
				// `None` only means the last subscription ended while nothing is in flight.
				if let Some((wp_hash, status)) = update {
					manager.on_status(wp_hash, status).await;
				}
			},
		}
	}
}
