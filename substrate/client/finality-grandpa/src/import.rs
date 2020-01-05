// Copyright 2018-2020 Parity Technologies (UK) Ltd.
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

use std::{sync::Arc, collections::HashMap};

use log::{debug, trace, info};
use parity_scale_codec::Encode;
use futures::sync::mpsc;
use parking_lot::RwLockWriteGuard;

use sp_blockchain::{HeaderBackend, BlockStatus, well_known_cache_keys};
use sc_client_api::{backend::Backend, CallExecutor, utils::is_descendent_of};
use sc_client::Client;
use sp_consensus::{
	BlockImport, Error as ConsensusError,
	BlockCheckParams, BlockImportParams, ImportResult, JustificationImport,
	SelectChain,
};
use sp_finality_grandpa::{GRANDPA_ENGINE_ID, ScheduledChange, ConsensusLog};
use sp_runtime::Justification;
use sp_runtime::generic::{BlockId, OpaqueDigestItemId};
use sp_runtime::traits::{
	Block as BlockT, DigestFor, Header as HeaderT, NumberFor, Zero,
};
use sp_core::{H256, Blake2Hasher};

use crate::{Error, CommandOrError, NewAuthoritySet, VoterCommand};
use crate::authorities::{AuthoritySet, SharedAuthoritySet, DelayKind, PendingChange};
use crate::consensus_changes::SharedConsensusChanges;
use crate::environment::finalize_block;
use crate::justification::GrandpaJustification;

/// A block-import handler for GRANDPA.
///
/// This scans each imported block for signals of changing authority set.
/// If the block being imported enacts an authority set change then:
/// - If the current authority set is still live: we import the block
/// - Otherwise, the block must include a valid justification.
///
/// When using GRANDPA, the block import worker should be using this block import
/// object.
pub struct GrandpaBlockImport<B, E, Block: BlockT<Hash=H256>, RA, SC> {
	inner: Arc<Client<B, E, Block, RA>>,
	select_chain: SC,
	authority_set: SharedAuthoritySet<Block::Hash, NumberFor<Block>>,
	send_voter_commands: mpsc::UnboundedSender<VoterCommand<Block::Hash, NumberFor<Block>>>,
	consensus_changes: SharedConsensusChanges<Block::Hash, NumberFor<Block>>,
}

impl<B, E, Block: BlockT<Hash=H256>, RA, SC: Clone> Clone for
	GrandpaBlockImport<B, E, Block, RA, SC>
{
	fn clone(&self) -> Self {
		GrandpaBlockImport {
			inner: self.inner.clone(),
			select_chain: self.select_chain.clone(),
			authority_set: self.authority_set.clone(),
			send_voter_commands: self.send_voter_commands.clone(),
			consensus_changes: self.consensus_changes.clone(),
		}
	}
}

impl<B, E, Block: BlockT<Hash=H256>, RA, SC> JustificationImport<Block>
	for GrandpaBlockImport<B, E, Block, RA, SC> where
		NumberFor<Block>: finality_grandpa::BlockNumberOps,
		B: Backend<Block, Blake2Hasher> + 'static,
		E: CallExecutor<Block, Blake2Hasher> + 'static + Clone + Send + Sync,
		DigestFor<Block>: Encode,
		RA: Send + Sync,
		SC: SelectChain<Block>,
{
	type Error = ConsensusError;

	fn on_start(&mut self) -> Vec<(Block::Hash, NumberFor<Block>)> {
		let mut out = Vec::new();
		let chain_info = self.inner.chain_info();

		// request justifications for all pending changes for which change blocks have already been imported
		let authorities = self.authority_set.inner().read();
		for pending_change in authorities.pending_changes() {
			if pending_change.delay_kind == DelayKind::Finalized &&
				pending_change.effective_number() > chain_info.finalized_number &&
				pending_change.effective_number() <= chain_info.best_number
			{
				let effective_block_hash = if !pending_change.delay.is_zero() {
					self.select_chain.finality_target(
						pending_change.canon_hash,
						Some(pending_change.effective_number()),
					)
				} else {
					Ok(Some(pending_change.canon_hash))
				};

				if let Ok(Some(hash)) = effective_block_hash {
					if let Ok(Some(header)) = self.inner.header(&BlockId::Hash(hash)) {
						if *header.number() == pending_change.effective_number() {
							out.push((header.hash(), *header.number()));
						}
					}
				}
			}
		}

		out
	}

	fn import_justification(
		&mut self,
		hash: Block::Hash,
		number: NumberFor<Block>,
		justification: Justification,
	) -> Result<(), Self::Error> {
		self.import_justification(hash, number, justification, false)
	}
}

enum AppliedChanges<H, N> {
	Standard(bool), // true if the change is ready to be applied (i.e. it's a root)
	Forced(NewAuthoritySet<H, N>),
	None,
}

impl<H, N> AppliedChanges<H, N> {
	fn needs_justification(&self) -> bool {
		match *self {
			AppliedChanges::Standard(_) => true,
			AppliedChanges::Forced(_) | AppliedChanges::None => false,
		}
	}
}

struct PendingSetChanges<'a, Block: 'a + BlockT> {
	just_in_case: Option<(
		AuthoritySet<Block::Hash, NumberFor<Block>>,
		RwLockWriteGuard<'a, AuthoritySet<Block::Hash, NumberFor<Block>>>,
	)>,
	applied_changes: AppliedChanges<Block::Hash, NumberFor<Block>>,
	do_pause: bool,
}

impl<'a, Block: 'a + BlockT> PendingSetChanges<'a, Block> {
	// revert the pending set change explicitly.
	fn revert(self) { }

	fn defuse(mut self) -> (AppliedChanges<Block::Hash, NumberFor<Block>>, bool) {
		self.just_in_case = None;
		let applied_changes = ::std::mem::replace(&mut self.applied_changes, AppliedChanges::None);
		(applied_changes, self.do_pause)
	}
}

impl<'a, Block: 'a + BlockT> Drop for PendingSetChanges<'a, Block> {
	fn drop(&mut self) {
		if let Some((old_set, mut authorities)) = self.just_in_case.take() {
			*authorities = old_set;
		}
	}
}

fn find_scheduled_change<B: BlockT>(header: &B::Header)
	-> Option<ScheduledChange<NumberFor<B>>>
{
	let id = OpaqueDigestItemId::Consensus(&GRANDPA_ENGINE_ID);

	let filter_log = |log: ConsensusLog<NumberFor<B>>| match log {
		ConsensusLog::ScheduledChange(change) => Some(change),
		_ => None,
	};

	// find the first consensus digest with the right ID which converts to
	// the right kind of consensus log.
	header.digest().convert_first(|l| l.try_to(id).and_then(filter_log))
}

fn find_forced_change<B: BlockT>(header: &B::Header)
	-> Option<(NumberFor<B>, ScheduledChange<NumberFor<B>>)>
{
	let id = OpaqueDigestItemId::Consensus(&GRANDPA_ENGINE_ID);

	let filter_log = |log: ConsensusLog<NumberFor<B>>| match log {
		ConsensusLog::ForcedChange(delay, change) => Some((delay, change)),
		_ => None,
	};

	// find the first consensus digest with the right ID which converts to
	// the right kind of consensus log.
	header.digest().convert_first(|l| l.try_to(id).and_then(filter_log))
}

impl<B, E, Block: BlockT<Hash=H256>, RA, SC>
	GrandpaBlockImport<B, E, Block, RA, SC>
where
	NumberFor<Block>: finality_grandpa::BlockNumberOps,
	B: Backend<Block, Blake2Hasher> + 'static,
	E: CallExecutor<Block, Blake2Hasher> + 'static + Clone + Send + Sync,
	DigestFor<Block>: Encode,
	RA: Send + Sync,
{
	// check for a new authority set change.
	fn check_new_change(&self, header: &Block::Header, hash: Block::Hash)
		-> Option<PendingChange<Block::Hash, NumberFor<Block>>>
	{
		// check for forced change.
		if let Some((median_last_finalized, change)) = find_forced_change::<Block>(header) {
			return Some(PendingChange {
				next_authorities: change.next_authorities,
				delay: change.delay,
				canon_height: *header.number(),
				canon_hash: hash,
				delay_kind: DelayKind::Best { median_last_finalized },
			});
		}

		// check normal scheduled change.
		let change = find_scheduled_change::<Block>(header)?;
		Some(PendingChange {
			next_authorities: change.next_authorities,
			delay: change.delay,
			canon_height: *header.number(),
			canon_hash: hash,
			delay_kind: DelayKind::Finalized,
		})
	}

	fn make_authorities_changes<'a>(&'a self, block: &mut BlockImportParams<Block>, hash: Block::Hash)
		-> Result<PendingSetChanges<'a, Block>, ConsensusError>
	{
		// when we update the authorities, we need to hold the lock
		// until the block is written to prevent a race if we need to restore
		// the old authority set on error or panic.
		struct InnerGuard<'a, T: 'a> {
			old: Option<T>,
			guard: Option<RwLockWriteGuard<'a, T>>,
		}

		impl<'a, T: 'a> InnerGuard<'a, T> {
			fn as_mut(&mut self) -> &mut T {
				&mut **self.guard.as_mut().expect("only taken on deconstruction; qed")
			}

			fn set_old(&mut self, old: T) {
				if self.old.is_none() {
					// ignore "newer" old changes.
					self.old = Some(old);
				}
			}

			fn consume(mut self) -> Option<(T, RwLockWriteGuard<'a, T>)> {
				if let Some(old) = self.old.take() {
					Some((old, self.guard.take().expect("only taken on deconstruction; qed")))
				} else {
					None
				}
			}
		}

		impl<'a, T: 'a> Drop for InnerGuard<'a, T> {
			fn drop(&mut self) {
				if let (Some(mut guard), Some(old)) = (self.guard.take(), self.old.take()) {
					*guard = old;
				}
			}
		}

		let number = block.header.number().clone();
		let maybe_change = self.check_new_change(
			&block.header,
			hash,
		);

		// returns a function for checking whether a block is a descendent of another
		// consistent with querying client directly after importing the block.
		let parent_hash = *block.header.parent_hash();
		let is_descendent_of = is_descendent_of(&*self.inner, Some((&hash, &parent_hash)));

		let mut guard = InnerGuard {
			guard: Some(self.authority_set.inner().write()),
			old: None,
		};

		// whether to pause the old authority set -- happens after import
		// of a forced change block.
		let mut do_pause = false;

		// add any pending changes.
		if let Some(change) = maybe_change {
			let old = guard.as_mut().clone();
			guard.set_old(old);

			if let DelayKind::Best { .. } = change.delay_kind {
				do_pause = true;
			}

			guard.as_mut().add_pending_change(
				change,
				&is_descendent_of,
			).map_err(|e| ConsensusError::from(ConsensusError::ClientImport(e.to_string())))?;
		}

		let applied_changes = {
			let forced_change_set = guard.as_mut().apply_forced_changes(hash, number, &is_descendent_of)
				.map_err(|e| ConsensusError::ClientImport(e.to_string()))
				.map_err(ConsensusError::from)?;

			if let Some((median_last_finalized_number, new_set)) = forced_change_set {
				let new_authorities = {
					let (set_id, new_authorities) = new_set.current();

					// we will use the median last finalized number as a hint
					// for the canon block the new authority set should start
					// with. we use the minimum between the median and the local
					// best finalized block.
					let best_finalized_number = self.inner.chain_info().finalized_number;
					let canon_number = best_finalized_number.min(median_last_finalized_number);
					let canon_hash =
						self.inner.header(&BlockId::Number(canon_number))
							.map_err(|e| ConsensusError::ClientImport(e.to_string()))?
							.expect("the given block number is less or equal than the current best finalized number; \
									 current best finalized number must exist in chain; qed.")
							.hash();

					NewAuthoritySet {
						canon_number,
						canon_hash,
						set_id,
						authorities: new_authorities.to_vec(),
					}
				};
				let old = ::std::mem::replace(guard.as_mut(), new_set);
				guard.set_old(old);

				AppliedChanges::Forced(new_authorities)
			} else {
				let did_standard = guard.as_mut().enacts_standard_change(hash, number, &is_descendent_of)
					.map_err(|e| ConsensusError::ClientImport(e.to_string()))
					.map_err(ConsensusError::from)?;

				if let Some(root) = did_standard {
					AppliedChanges::Standard(root)
				} else {
					AppliedChanges::None
				}
			}
		};

		// consume the guard safely and write necessary changes.
		let just_in_case = guard.consume();
		if let Some((_, ref authorities)) = just_in_case {
			let authorities_change = match applied_changes {
				AppliedChanges::Forced(ref new) => Some(new),
				AppliedChanges::Standard(_) => None, // the change isn't actually applied yet.
				AppliedChanges::None => None,
			};

			crate::aux_schema::update_authority_set::<Block, _, _>(
				authorities,
				authorities_change,
				|insert| block.auxiliary.extend(
					insert.iter().map(|(k, v)| (k.to_vec(), Some(v.to_vec())))
				)
			);
		}

		Ok(PendingSetChanges { just_in_case, applied_changes, do_pause })
	}
}

impl<B, E, Block: BlockT<Hash=H256>, RA, SC> BlockImport<Block>
	for GrandpaBlockImport<B, E, Block, RA, SC> where
		NumberFor<Block>: finality_grandpa::BlockNumberOps,
		B: Backend<Block, Blake2Hasher> + 'static,
		E: CallExecutor<Block, Blake2Hasher> + 'static + Clone + Send + Sync,
		DigestFor<Block>: Encode,
		RA: Send + Sync,
{
	type Error = ConsensusError;

	fn import_block(
		&mut self,
		mut block: BlockImportParams<Block>,
		new_cache: HashMap<well_known_cache_keys::Id, Vec<u8>>,
	) -> Result<ImportResult, Self::Error> {
		let hash = block.post_header().hash();
		let number = block.header.number().clone();

		// early exit if block already in chain, otherwise the check for
		// authority changes will error when trying to re-import a change block
		match self.inner.status(BlockId::Hash(hash)) {
			Ok(BlockStatus::InChain) => return Ok(ImportResult::AlreadyInChain),
			Ok(BlockStatus::Unknown) => {},
			Err(e) => return Err(ConsensusError::ClientImport(e.to_string()).into()),
		}

		let pending_changes = self.make_authorities_changes(&mut block, hash)?;

		// we don't want to finalize on `inner.import_block`
		let mut justification = block.justification.take();
		let enacts_consensus_change = !new_cache.is_empty();
		let import_result = (&*self.inner).import_block(block, new_cache);

		let mut imported_aux = {
			match import_result {
				Ok(ImportResult::Imported(aux)) => aux,
				Ok(r) => {
					debug!(target: "afg", "Restoring old authority set after block import result: {:?}", r);
					pending_changes.revert();
					return Ok(r);
				},
				Err(e) => {
					debug!(target: "afg", "Restoring old authority set after block import error: {:?}", e);
					pending_changes.revert();
					return Err(ConsensusError::ClientImport(e.to_string()).into());
				},
			}
		};

		let (applied_changes, do_pause) = pending_changes.defuse();

		// Send the pause signal after import but BEFORE sending a `ChangeAuthorities` message.
		if do_pause {
			let _ = self.send_voter_commands.unbounded_send(
				VoterCommand::Pause(format!("Forced change scheduled after inactivity"))
			);
		}

		let needs_justification = applied_changes.needs_justification();

		match applied_changes {
			AppliedChanges::Forced(new) => {
				// NOTE: when we do a force change we are "discrediting" the old set so we
				// ignore any justifications from them. this block may contain a justification
				// which should be checked and imported below against the new authority
				// triggered by this forced change. the new grandpa voter will start at the
				// last median finalized block (which is before the block that enacts the
				// change), full nodes syncing the chain will not be able to successfully
				// import justifications for those blocks since their local authority set view
				// is still of the set before the forced change was enacted, still after #1867
				// they should import the block and discard the justification, and they will
				// then request a justification from sync if it's necessary (which they should
				// then be able to successfully validate).
				let _ = self.send_voter_commands.unbounded_send(VoterCommand::ChangeAuthorities(new));

				// we must clear all pending justifications requests, presumably they won't be
				// finalized hence why this forced changes was triggered
				imported_aux.clear_justification_requests = true;
			},
			AppliedChanges::Standard(false) => {
				// we can't apply this change yet since there are other dependent changes that we
				// need to apply first, drop any justification that might have been provided with
				// the block to make sure we request them from `sync` which will ensure they'll be
				// applied in-order.
				justification.take();
			},
			_ => {},
		}

		match justification {
			Some(justification) => {
				self.import_justification(hash, number, justification, needs_justification).unwrap_or_else(|err| {
					if needs_justification || enacts_consensus_change {
						debug!(target: "afg", "Imported block #{} that enacts authority set change with \
							invalid justification: {:?}, requesting justification from peers.", number, err);
						imported_aux.bad_justification = true;
						imported_aux.needs_justification = true;
					}
				});
			},
			None => {
				if needs_justification {
					trace!(
						target: "afg",
						"Imported unjustified block #{} that enacts authority set change, waiting for finality for enactment.",
						number,
					);

					imported_aux.needs_justification = true;
				}

				// we have imported block with consensus data changes, but without justification
				// => remember to create justification when next block will be finalized
				if enacts_consensus_change {
					self.consensus_changes.lock().note_change((number, hash));
				}
			}
		}

		Ok(ImportResult::Imported(imported_aux))
	}

	fn check_block(
		&mut self,
		block: BlockCheckParams<Block>,
	) -> Result<ImportResult, Self::Error> {
		self.inner.check_block(block)
	}
}

impl<B, E, Block: BlockT<Hash=H256>, RA, SC>
	GrandpaBlockImport<B, E, Block, RA, SC>
{
	pub(crate) fn new(
		inner: Arc<Client<B, E, Block, RA>>,
		select_chain: SC,
		authority_set: SharedAuthoritySet<Block::Hash, NumberFor<Block>>,
		send_voter_commands: mpsc::UnboundedSender<VoterCommand<Block::Hash, NumberFor<Block>>>,
		consensus_changes: SharedConsensusChanges<Block::Hash, NumberFor<Block>>,
	) -> GrandpaBlockImport<B, E, Block, RA, SC> {
		GrandpaBlockImport {
			inner,
			select_chain,
			authority_set,
			send_voter_commands,
			consensus_changes,
		}
	}
}

impl<B, E, Block: BlockT<Hash=H256>, RA, SC>
	GrandpaBlockImport<B, E, Block, RA, SC>
where
	NumberFor<Block>: finality_grandpa::BlockNumberOps,
	B: Backend<Block, Blake2Hasher> + 'static,
	E: CallExecutor<Block, Blake2Hasher> + 'static + Clone + Send + Sync,
	RA: Send + Sync,
{

	/// Import a block justification and finalize the block.
	///
	/// If `enacts_change` is set to true, then finalizing this block *must*
	/// enact an authority set change, the function will panic otherwise.
	fn import_justification(
		&mut self,
		hash: Block::Hash,
		number: NumberFor<Block>,
		justification: Justification,
		enacts_change: bool,
	) -> Result<(), ConsensusError> {
		let justification = GrandpaJustification::decode_and_verify_finalizes(
			&justification,
			(hash, number),
			self.authority_set.set_id(),
			&self.authority_set.current_authorities(),
		);

		let justification = match justification {
			Err(e) => return Err(ConsensusError::ClientImport(e.to_string()).into()),
			Ok(justification) => justification,
		};

		let result = finalize_block(
			&*self.inner,
			&self.authority_set,
			&self.consensus_changes,
			None,
			hash,
			number,
			justification.into(),
		);

		match result {
			Err(CommandOrError::VoterCommand(command)) => {
				info!(target: "afg", "Imported justification for block #{} that triggers \
					command {}, signaling voter.", number, command);

				// send the command to the voter
				let _ = self.send_voter_commands.unbounded_send(command);
			},
			Err(CommandOrError::Error(e)) => {
				return Err(match e {
					Error::Grandpa(error) => ConsensusError::ClientImport(error.to_string()),
					Error::Network(error) => ConsensusError::ClientImport(error),
					Error::Blockchain(error) => ConsensusError::ClientImport(error),
					Error::Client(error) => ConsensusError::ClientImport(error.to_string()),
					Error::Safety(error) => ConsensusError::ClientImport(error),
					Error::Timer(error) => ConsensusError::ClientImport(error.to_string()),
				}.into());
			},
			Ok(_) => {
				assert!(!enacts_change, "returns Ok when no authority set change should be enacted; qed;");
			},
		}

		Ok(())
	}
}
