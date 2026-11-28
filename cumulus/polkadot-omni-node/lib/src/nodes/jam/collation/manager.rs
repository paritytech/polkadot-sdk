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

//! The collation manager: everything one collation task tracks, and how it handles each event.

use crate::{
	common::{
		types::{JamClient, ParachainBackend},
		ConstructNodeRuntimeApi, NodeBlock,
	},
	nodes::jam::{
		authorizer::AuthoritySource,
		code,
		collation::{
			in_flight::{
				forget_package, plan_on_jam_block, InFlight, InFlightPackages, StatusSubscriptions,
			},
			policy::{ForgetWhenAnchorExpires, PolicyAction, ResubmissionPolicy},
		},
		message::{AuthoringHold, JamCollatorMessage},
		package::{
			source::{record_submitted_hash, Anchored, PackageSource},
			PackageParams,
		},
		store::{hash_ledger::WpHashLedger, pov_store::BlockPovs},
		LOG_TARGET,
	},
};
use cumulus_client_jam_authorizer::authorizer::AuraAuthorizer;
use futures::stream::SelectAll;
use jam_interface::{
	BlockDesc, BoxStream, CoreIndex, HeaderHash, JamChainSource, JamStateSource,
	JamWorkPackageSubmission, Slot as JamSlot, WorkPackageHash, WorkPackageStatus,
};
use sp_runtime::traits::{HashingFor, Header as HeaderT};
use std::{collections::HashMap, sync::Arc};

pub(crate) struct Manager<Block: NodeBlock, RuntimeApi, Jam> {
	pub(crate) para_client: Arc<JamClient<Block, RuntimeApi>>,
	pub(crate) para_backend: Arc<ParachainBackend<Block>>,
	pub(crate) jam: Arc<Jam>,
	/// The para's AURA authorizers by anchor slot: what every package here runs under, and what
	/// signs it.
	pub(crate) authorizers: Arc<dyn AuthoritySource<Block>>,
	/// The service settings every package is assembled from.
	pub(crate) params: PackageParams,
	pub(crate) policy: ForgetWhenAnchorExpires,
	/// Shared with the builder; refreshed by [`Manager::log_state`] on every mutation.
	pub(crate) hold: AuthoringHold,
	pub(crate) announce_block: Arc<dyn Fn(Block::Hash, Option<Vec<u8>>) + Send + Sync>,
	/// Which work package this node submitted for which block. Signing is non-deterministic, so
	/// a package's hash can only be remembered, never recomputed — not even by its author.
	pub(crate) hash_ledger: WpHashLedger<JamClient<Block, RuntimeApi>>,
	pub(crate) packages: InFlightPackages<Block>,
	/// Where every block this collator authored is recorded, so its PoV can be rebuilt later.
	pub(crate) block_povs: BlockPovs<Block>,
	/// The para head last seen in JAM state, for the log alone; `None` until the stream reports
	/// one. No decision reads it: it is a strictly later observation than the anchor a package
	/// carries, and letting it override the anchor is exactly the class of race that
	/// cost three fixes.
	pub(crate) included_head: Option<Block::Hash>,
	/// The accumulated head's hash and number, updated by `on_para_head`.
	pub(crate) accumulated_head: Option<(Block::Hash, <Block::Header as HeaderT>::Number)>,
	/// The slot of the JAM block whose state last changed `accumulated_head`. The head stream
	/// carries it, because the best-block stream may deliver that block before or after the head
	/// it moved, and `tip_slot` at the time would be one slot off half the time.
	pub(crate) head_changed_at: JamSlot,
	/// The JAM slot at which the last resubmission round started, if any.
	pub(crate) last_round_at: Option<JamSlot>,
	/// The latest JAM block's slot, updated on every `on_jam_block`.
	pub(crate) tip_slot: JamSlot,
	pub(crate) statuses: SelectAll<BoxStream<'static, (WorkPackageHash, WorkPackageStatus)>>,
	/// One handle per live status subscription, fired when its package stops being tracked.
	pub(crate) subscriptions: StatusSubscriptions,
	/// The tip slot each core last got a package from this collator at; see
	/// [`due_for_sending`](crate::nodes::jam::collation::in_flight::due_for_sending).
	pub(crate) sent_at: HashMap<CoreIndex, JamSlot>,
}

impl<Block, RuntimeApi, Jam> Manager<Block, RuntimeApi, Jam>
where
	Block: NodeBlock,
	RuntimeApi: ConstructNodeRuntimeApi<Block, JamClient<Block, RuntimeApi>>,
	Jam: JamChainSource + JamStateSource + JamWorkPackageSubmission + 'static,
{
	/// Resolve an offchain-code marker at `block`; see [`code::ensure_code_blob_at`].
	pub(crate) async fn ensure_code_blob_at(&mut self, block: Block::Hash, anchor: HeaderHash) {
		code::ensure_code_blob_at(
			&self.para_client,
			&self.para_backend,
			&*self.jam,
			self.params.service_id,
			block,
			anchor,
		)
		.await;
	}

	/// The authorizer JAM uses at `anchor_slot`, or an error when this node is not in that set.
	///
	/// `at` is a block whose runtime knows the schedule: the package's parent for a fresh block,
	/// the local best for a resubmission round whose anchor is the JAM tip.
	pub(crate) fn authorizer_for(
		&self,
		at: Block::Hash,
		anchor_slot: JamSlot,
	) -> Result<Arc<AuraAuthorizer>, String> {
		self.authorizers.for_anchor(at, anchor_slot)?.ok_or_else(|| {
			format!(
				"this node holds no aura key of the authority set JAM authorizes at anchor slot \
				 {anchor_slot}",
			)
		})
	}

	/// A block from the builder: assemble its package, submit it, track it.
	///
	/// The package names the work package this node submitted for the parent block as its one
	/// prerequisite, when the ledger still remembers it. The parent it declares also travels
	/// inside the PoV, and the parachain service decides at accumulate whether that parent is
	/// the stored head, a head still to come (buffered) or a fork loser (dropped).
	pub(crate) async fn on_new_block(&mut self, message: JamCollatorMessage<Block>) {
		let JamCollatorMessage {
			parent_header,
			block,
			proof,
			context,
			anchor_slot,
			submit_target,
			additional_data,
			triggered_by,
		} = message;
		let block_hash = block.hash();
		let block_number = *block.header().number();
		let parent_hash = parent_header.hash();
		let state_root = *parent_header.state_root();

		let compact_proof = match proof.into_compact_proof::<HashingFor<Block>>(state_root) {
			Ok(compact_proof) => compact_proof,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					?error,
					"Failed to compact the storage proof; dropping the block.",
				);
				return;
			},
		};

		self.ensure_code_blob_at(parent_hash, context.anchor).await;

		let authorizer = match self.authorizer_for(parent_hash, anchor_slot) {
			Ok(authorizer) => authorizer,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					anchor_slot,
					error,
					"No authorizer for the package's anchor; dropping the block.",
				);
				return;
			},
		};

		let validation_code = match self.para_client.code_at(parent_hash) {
			Ok(code) => code,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					?error,
					"Failed to read the validation code; dropping the block.",
				);
				return;
			},
		};

		// The parent's `:code`, not this block's own. A `:code` write in block N+1 only takes
		// effect for validation at N+2, so N+1 is still validated with the old code; the
		// service's dual-code window accepts both while the upgrade is in flight. Hashing the
		// parent's code is therefore the code this package must commit to. Caveat: `system_version:
		// 3` runtimes defer the swap by two blocks (N+2 still runs the old code, N+3 the new one),
		// which the same parent-code hash covers.
		let validation_code_hash = sp_crypto_hashing::blake2_256(&validation_code);
		tracing::debug!(
			target: LOG_TARGET,
			?block_hash,
			?parent_hash,
			?validation_code_hash,
			"Hashed the parent's validation code for the work package.",
		);
		let source = PackageSource {
			blocks: vec![block],
			proof: compact_proof,
			// The additional-data map assembled at build time — the `JAM_PROOF_KEY` entry — is
			// part of what makes this block's package what it is, like the block it proves.
			additional_data,
			validation_code_hash,
			params: self.params.clone(),
			authorizer: authorizer.authorizer(),
			parent_header,
		};
		let anchored = Anchored::after_parent(
			&self.hash_ledger,
			parent_hash.into(),
			context,
			anchor_slot,
			submit_target,
		);
		let (package, pov) = match source.authorized_package(&anchored, &authorizer) {
			Ok(package) => package,
			Err(error) => {
				tracing::error!(
					target: LOG_TARGET,
					?block_hash,
					%block_number,
					lookup_anchor_slot = anchored.context.lookup_anchor_slot,
					error,
					"Failed to authorize the work package; dropping the block. The block stays in \
					 the local database, but nothing will make it accumulate.",
				);
				return;
			},
		};
		// Kept the way the import path keeps a block it re-executes, so a later resubmission can
		// rebuild this block's PoV too.
		match source.into_block_pov(package.items[0].extrinsics[0].clone()) {
			Some(block_pov) => self.block_povs.insert(block_hash, block_pov),
			None => tracing::debug!(
				target: LOG_TARGET,
				?block_hash,
				"The block carries no `JamParent` digest; not keeping its PoV for a rebuild.",
			),
		}
		let wp_hash = record_submitted_hash(&self.hash_ledger, block_hash.into(), &package);

		tracing::info!(
			target: LOG_TARGET,
			?block_hash,
			%block_number,
			?parent_hash,
			?wp_hash,
			prerequisite = ?anchored.context.prerequisites.as_ref().first(),
			core = ?anchored.submit_target,
			anchor = ?anchored.context.anchor,
			anchor_slot,
			lookup_anchor = ?anchored.context.lookup_anchor,
			lookup_anchor_slot = anchored.context.lookup_anchor_slot,
			expected_collator = authorizer.collator_for(anchored.context.anchor_slot),
			own_index = authorizer.own_index(),
			authorizer_hash = ?authorizer.hash(),
			token_len = package.authorization.len(),
			pov_len = package.items[0].extrinsics[0].len,
			in_flight = self.packages.len(),
			?triggered_by,
			"Assembled and signed the work package for the block.",
		);

		(self.announce_block)(block_hash, None);

		// Tracked before it is sent: pacing may hold it, and a failed submission may still have
		// reached a guarantor, so the next JAM block re-decides it either way.
		self.packages.entries.push_back(InFlight {
			block_hash,
			block_number,
			parent_hash,
			wp_hash,
			package,
			anchored,
			reported: false,
			resubmission: false,
			sends: 0,
			unsent: Some(pov),
		});
		self.send_due().await;
		self.log_state("a package was submitted");
	}

	/// A status update for one of the packages in flight.
	pub(crate) async fn on_status(&mut self, wp_hash: WorkPackageHash, status: WorkPackageStatus) {
		let Some(index) = self.packages.position_of_package(wp_hash) else {
			tracing::debug!(
				target: LOG_TARGET,
				?wp_hash,
				?status,
				"Status update for a package that is no longer in flight; ignoring.",
			);
			return;
		};
		let action = self.policy.on_status(&status);
		tracing::info!(
			target: LOG_TARGET,
			?wp_hash,
			block_hash = ?self.packages.entries[index].block_hash,
			block_number = %self.packages.entries[index].block_number,
			index,
			in_flight = self.packages.len(),
			?status,
			?action,
			"Work-package status update.",
		);
		match action {
			PolicyAction::Wait => {},
			PolicyAction::Reported => {
				self.packages.entries[index].reported = true;
				let reported_in = match &status {
					WorkPackageStatus::Reported { reported_in, .. } => Some(*reported_in),
					_ => None,
				};
				let entry = &self.packages.entries[index];
				tracing::info!(
					target: LOG_TARGET,
					wp_hash = ?entry.wp_hash,
					block_hash = ?entry.block_hash,
					jam_block = ?reported_in.map(|block| block.header_hash),
					jam_slot = reported_in.map(|block| block.slot),
					via = "status",
					"The work package appeared on chain in a JAM block.",
				);
			},
			PolicyAction::Forget => self.forget(index, &format!("{status:?}")),
		}
		self.log_state("a work-package status update arrived");
	}

	/// A new JAM block arrived: read β and re-decide every package that has not been reported.
	pub(crate) async fn on_jam_block(&mut self, tip: BlockDesc) {
		self.tip_slot = tip.slot;
		if self.packages.entries.is_empty() {
			self.resubmit_if_due(tip).await;
			return;
		}
		let history = match self.jam.recent_blocks(tip.header_hash).await {
			Ok(history) => history,
			Err(error) => {
				tracing::warn!(
					target: LOG_TARGET,
					?tip,
					?error,
					"Unable to read the recent-blocks history; leaving the packages for the next \
					 JAM block.",
				);
				return;
			},
		};
		let plan = plan_on_jam_block(&self.packages, tip.slot, &history, &self.policy);
		// Keyed by package hash rather than by index: forgetting one shifts every index after it,
		// and packages are independent, so several may come due with the same block.
		for (wp_hash, action, appeared) in plan {
			let Some(index) = self.packages.position_of_package(wp_hash) else { continue };
			match action {
				PolicyAction::Reported => {
					self.packages.entries[index].reported = true;
					let entry = &self.packages.entries[index];
					tracing::info!(
						target: LOG_TARGET,
						wp_hash = ?entry.wp_hash,
						block_hash = ?entry.block_hash,
						jam_block = ?appeared.map(|block| block.header_hash),
						jam_slot = appeared.map(|block| block.slot),
						via = "recent_history",
						"The work package appeared on chain in a JAM block.",
					);
				},
				PolicyAction::Forget => {
					self.forget(index, "anchor expired before the package appeared on chain")
				},
				PolicyAction::Wait => {},
			}
		}
		self.send_due().await;
		self.log_state("a JAM block arrived");
		self.resubmit_if_due(tip).await;
	}

	/// Give up on a package and, transitively, on every package that named it.
	///
	/// The block itself stays in the local database and the PoV store, so the next parachain slot
	/// simply authors on whatever is deepest there, and a resubmission round — this collator's or
	/// another's, whichever owns the turn once the head has stalled long enough — rebuilds the
	/// block into a new package. Until then the parachain's progress is what this costs:
	/// descendants of the lost block sit in the service's reorder buffer. This logs loudly enough
	/// to be the thing a stalled parachain is diagnosed from.
	fn forget(&mut self, index: usize, reason: &str) {
		let forgotten = forget_package(&mut self.packages, &self.hash_ledger, index);
		let cascade_len = forgotten.len();
		for entry in &forgotten {
			self.stop_following(entry.wp_hash, "forgotten");
			tracing::warn!(
				target: LOG_TARGET,
				block_hash = ?entry.block_hash,
				block_number = %entry.block_number,
				parent_hash = ?entry.parent_hash,
				wp_hash = ?entry.wp_hash,
				reason,
				sends = entry.sends,
				anchor_slot = entry.anchored.anchor_slot,
				cascade_len,
				in_flight = self.packages.len(),
				"Giving up on a work package. Its block stays in the local database, so authoring \
				 continues, but nothing this collator does will make that block accumulate.",
			);
		}
		self.log_state("a package was forgotten");
	}

	/// The para head advanced in JAM state.
	///
	/// Everything at or below the new head's height is settled: the service applies a head only
	/// if it chains onto the stored one, and sweeps the rest of that height out of its reorder
	/// buffer. So those packages either just accumulated or lost their fork, and either way this
	/// task is done with them.
	pub(crate) fn on_para_head(&mut self, header: &Block::Header, changed_at: JamSlot) {
		let hash = header.hash();
		let number = *header.number();
		let head_changed = self.accumulated_head.map_or(true, |(h, _)| h != hash);
		self.included_head = Some(hash);
		self.accumulated_head = Some((hash, number));
		if head_changed {
			self.head_changed_at = changed_at;
		}

		let settled = self.packages.remove_up_to(number);
		let accumulated = settled.iter().any(|entry| entry.block_hash == hash);
		let superseded: Vec<Block::Hash> = settled
			.iter()
			.filter(|entry| entry.block_hash != hash)
			.map(|entry| entry.block_hash)
			.collect();
		for entry in settled {
			let why = if entry.block_hash == hash { "accumulated" } else { "superseded" };
			self.stop_following(entry.wp_hash, why);
		}

		tracing::info!(
			target: LOG_TARGET,
			block_hash = ?hash,
			block_number = %number,
			accumulated,
			?superseded,
			remaining = self.packages.len(),
			"Para head advanced in JAM state; a package of ours accumulated if `accumulated`, and \
			 anything of ours at or below that height is settled either way.",
		);
		self.log_state("the para head advanced");
	}

	/// Close one package's status subscription. Every path that stops tracking a package goes
	/// through here: a handle outliving its entry is a subscription the node holds open for the
	/// rest of the connection's life, and enough of those stop the collator dead.
	fn stop_following(&mut self, wp_hash: WorkPackageHash, why: &str) {
		if self.subscriptions.close(wp_hash) {
			tracing::debug!(
				target: LOG_TARGET,
				?wp_hash,
				why,
				live_subscriptions = self.subscriptions.len(),
				"Closed a work-package status subscription.",
			);
		}
	}

	pub(crate) fn log_state(&self, after: &str) {
		let pending = self.resubmissions_pending();
		self.hold.set_pending(pending);
		tracing::debug!(
			target: LOG_TARGET,
			after,
			in_flight = self.packages.len(),
			blocks = ?self.packages.block_hashes(),
			included_head = ?self.included_head,
			accumulated_head = ?self.accumulated_head,
			head_changed_at = self.head_changed_at,
			last_round_at = ?self.last_round_at,
			tip_slot = self.tip_slot,
			pending,
			live_subscriptions = self.subscriptions.len(),
			"In-flight work packages.",
		);
	}

	/// Count in-flight resubmission packages that have not been reported yet.
	pub(crate) fn resubmissions_pending(&self) -> usize {
		self.packages.entries.iter().filter(|e| e.resubmission && !e.reported).count()
	}
}
