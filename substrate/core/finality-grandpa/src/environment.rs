// Copyright 2018-2019 Parity Technologies (UK) Ltd.
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

use std::sync::Arc;
use std::time::{Duration, Instant};

use log::{debug, warn, info};
use parity_codec::Encode;
use futures::prelude::*;
use tokio::timer::Delay;
use parking_lot::RwLock;

use client::{
	backend::Backend, BlockchainEvents, CallExecutor, Client, error::Error as ClientError
};
use grandpa::{
	BlockNumberOps, Equivocation, Error as GrandpaError, round::State as RoundState, voter, VoterSet,
};
use runtime_primitives::generic::BlockId;
use runtime_primitives::traits::{
	As, Block as BlockT, Header as HeaderT, NumberFor, One, Zero,
};
use substrate_primitives::{Blake2Hasher, ed25519, H256, Pair};

use crate::{
	Commit, Config, Error, Network, Precommit, Prevote,
	CommandOrError, NewAuthoritySet, VoterCommand,
};

use crate::authorities::SharedAuthoritySet;
use crate::consensus_changes::SharedConsensusChanges;
use crate::justification::GrandpaJustification;
use crate::until_imported::UntilVoteTargetImported;

use ed25519::Public as AuthorityId;

/// Data about a completed round.
pub(crate) type CompletedRound<H, N> = (u64, RoundState<H, N>);

/// A read-only view of the last completed round.
pub(crate) struct LastCompletedRound<H, N> {
	inner: RwLock<CompletedRound<H, N>>,
}

impl<H: Clone, N: Clone> LastCompletedRound<H, N> {
	/// Create a new tracker based on some starting last-completed round.
	pub(crate) fn new(round: CompletedRound<H, N>) -> Self {
		LastCompletedRound { inner: RwLock::new(round) }
	}

	/// Read the last completed round.
	pub(crate) fn read(&self) -> CompletedRound<H, N> {
		self.inner.read().clone()
	}

	// NOTE: not exposed outside of this module intentionally.
	fn with<F, R>(&self, f: F) -> R
		where F: FnOnce(&mut CompletedRound<H, N>) -> R
	{
		f(&mut *self.inner.write())
	}
}

/// The environment we run GRANDPA in.
pub(crate) struct Environment<B, E, Block: BlockT, N: Network<Block>, RA> {
	pub(crate) inner: Arc<Client<B, E, Block, RA>>,
	pub(crate) voters: Arc<VoterSet<AuthorityId>>,
	pub(crate) config: Config,
	pub(crate) authority_set: SharedAuthoritySet<Block::Hash, NumberFor<Block>>,
	pub(crate) consensus_changes: SharedConsensusChanges<Block::Hash, NumberFor<Block>>,
	pub(crate) network: N,
	pub(crate) set_id: u64,
	pub(crate) last_completed: LastCompletedRound<Block::Hash, NumberFor<Block>>,
}

impl<Block: BlockT<Hash=H256>, B, E, N, RA> grandpa::Chain<Block::Hash, NumberFor<Block>> for Environment<B, E, Block, N, RA> where
	Block: 'static,
	B: Backend<Block, Blake2Hasher> + 'static,
	E: CallExecutor<Block, Blake2Hasher> + 'static,
	N: Network<Block> + 'static,
	N::In: 'static,
	NumberFor<Block>: BlockNumberOps,
{
	fn ancestry(&self, base: Block::Hash, block: Block::Hash) -> Result<Vec<Block::Hash>, GrandpaError> {
		if base == block { return Err(GrandpaError::NotDescendent) }

		let tree_route_res = ::client::blockchain::tree_route(
			self.inner.backend().blockchain(),
			BlockId::Hash(block),
			BlockId::Hash(base),
		);

		let tree_route = match tree_route_res {
			Ok(tree_route) => tree_route,
			Err(e) => {
				debug!(target: "afg", "Encountered error computing ancestry between block {:?} and base {:?}: {:?}",
					block, base, e);

				return Err(GrandpaError::NotDescendent);
			}
		};

		if tree_route.common_block().hash != base {
			return Err(GrandpaError::NotDescendent);
		}

		// skip one because our ancestry is meant to start from the parent of `block`,
		// and `tree_route` includes it.
		Ok(tree_route.retracted().iter().skip(1).map(|e| e.hash).collect())
	}

	fn best_chain_containing(&self, block: Block::Hash) -> Option<(Block::Hash, NumberFor<Block>)> {
		// NOTE: when we finalize an authority set change through the sync protocol the voter is
		//       signalled asynchronously. therefore the voter could still vote in the next round
		//       before activating the new set. the `authority_set` is updated immediately thus we
		//       restrict the voter based on that.
		if self.set_id != self.authority_set.inner().read().current().0 {
			return None;
		}

		// we refuse to vote beyond the current limit number where transitions are scheduled to
		// occur.
		// once blocks are finalized that make that transition irrelevant or activate it,
		// we will proceed onwards. most of the time there will be no pending transition.
		let limit = self.authority_set.current_limit();
		debug!(target: "afg", "Finding best chain containing block {:?} with number limit {:?}", block, limit);

		match self.inner.best_containing(block, None) {
			Ok(Some(mut best_hash)) => {
				let base_header = self.inner.header(&BlockId::Hash(block)).ok()?
					.expect("Header known to exist after `best_containing` call; qed");

				if let Some(limit) = limit {
					// this is a rare case which might cause issues,
					// might be better to return the header itself.
					if *base_header.number() > limit {
						debug!(target: "afg", "Encountered error finding best chain containing {:?} with limit {:?}: target block is after limit",
							block,
							limit,
						);
						return None;
					}
				}

				let mut best_header = self.inner.header(&BlockId::Hash(best_hash)).ok()?
					.expect("Header known to exist after `best_containing` call; qed");

				// we target a vote towards 3/4 of the unfinalized chain (rounding up)
				let target = {
					let two = NumberFor::<Block>::one() + One::one();
					let three = two + One::one();
					let four = three + One::one();

					let diff = *best_header.number() - *base_header.number();
					let diff = ((diff * three) + two) / four;

					*base_header.number() + diff
				};

				// unless our vote is currently being limited due to a pending change
				let target = limit.map(|limit| limit.min(target)).unwrap_or(target);

				// walk backwards until we find the target block
				loop {
					if *best_header.number() < target { unreachable!(); }
					if *best_header.number() == target {
						return Some((best_hash, *best_header.number()));
					}

					best_hash = *best_header.parent_hash();
					best_header = self.inner.header(&BlockId::Hash(best_hash)).ok()?
						.expect("Header known to exist after `best_containing` call; qed");
				}
			},
			Ok(None) => {
				debug!(target: "afg", "Encountered error finding best chain containing {:?}: couldn't find target block", block);
				None
			}
			Err(e) => {
				debug!(target: "afg", "Encountered error finding best chain containing {:?}: {:?}", block, e);
				None
			}
		}
	}
}

impl<B, E, Block: BlockT<Hash=H256>, N, RA> voter::Environment<Block::Hash, NumberFor<Block>> for Environment<B, E, Block, N, RA> where
	Block: 'static,
	B: Backend<Block, Blake2Hasher> + 'static,
	E: CallExecutor<Block, Blake2Hasher> + 'static + Send + Sync,
	N: Network<Block> + 'static + Send,
	N::In: 'static + Send,
	RA: 'static + Send + Sync,
	NumberFor<Block>: BlockNumberOps,
{
	type Timer = Box<dyn Future<Item = (), Error = Self::Error> + Send>;
	type Id = AuthorityId;
	type Signature = ed25519::Signature;

	// regular round message streams
	type In = Box<dyn Stream<
		Item = ::grandpa::SignedMessage<Block::Hash, NumberFor<Block>, Self::Signature, Self::Id>,
		Error = Self::Error,
	> + Send>;
	type Out = Box<dyn Sink<
		SinkItem = ::grandpa::Message<Block::Hash, NumberFor<Block>>,
		SinkError = Self::Error,
	> + Send>;

	type Error = CommandOrError<Block::Hash, NumberFor<Block>>;

	fn round_data(
		&self,
		round: u64
	) -> voter::RoundData<Self::Timer, Self::In, Self::Out> {
		let now = Instant::now();
		let prevote_timer = Delay::new(now + self.config.gossip_duration * 2);
		let precommit_timer = Delay::new(now + self.config.gossip_duration * 4);

		let incoming = crate::communication::checked_message_stream::<Block, _>(
			self.network.messages_for(round, self.set_id),
			self.voters.clone(),
		);

		let local_key = self.config.local_key.as_ref()
			.filter(|pair| self.voters.contains_key(&pair.public().into()));

		let (out_rx, outgoing) = crate::communication::outgoing_messages::<Block, _>(
			round,
			self.set_id,
			local_key.cloned(),
			self.voters.clone(),
			self.network.clone(),
		);

		// schedule incoming messages from the network to be held until
		// corresponding blocks are imported.
		let incoming = UntilVoteTargetImported::new(
			self.inner.import_notification_stream(),
			self.inner.clone(),
			incoming,
		);

		// join incoming network messages with locally originating ones.
		let incoming = Box::new(out_rx.select(incoming).map_err(Into::into));

		// schedule network message cleanup when sink drops.
		let outgoing = Box::new(outgoing.sink_map_err(Into::into));

		voter::RoundData {
			prevote_timer: Box::new(prevote_timer.map_err(|e| Error::Timer(e).into())),
			precommit_timer: Box::new(precommit_timer.map_err(|e| Error::Timer(e).into())),
			incoming,
			outgoing,
		}
	}

	fn completed(&self, round: u64, state: RoundState<Block::Hash, NumberFor<Block>>) -> Result<(), Self::Error> {
		debug!(
			target: "afg", "Voter {} completed round {} in set {}. Estimate = {:?}, Finalized in round = {:?}",
			self.config.name(),
			round,
			self.set_id,
			state.estimate.as_ref().map(|e| e.1),
			state.finalized.as_ref().map(|e| e.1),
		);

		self.last_completed.with(|last_completed| {
			let set_state = crate::aux_schema::VoterSetState::Live(round, state.clone());
			crate::aux_schema::write_voter_set_state(&**self.inner.backend(), &set_state)?;

			*last_completed = (round, state); // after writing to DB successfully.
			Ok(())
		})
	}

	fn finalize_block(&self, hash: Block::Hash, number: NumberFor<Block>, round: u64, commit: Commit<Block>) -> Result<(), Self::Error> {
		use client::blockchain::HeaderBackend;

		let status = self.inner.backend().blockchain().info()?;
		if number <= status.finalized_number && self.inner.backend().blockchain().hash(number)? == Some(hash) {
			// This can happen after a forced change (triggered by the finality tracker when finality is stalled), since
			// the voter will be restarted at the median last finalized block, which can be lower than the local best
			// finalized block.
			warn!(target: "afg", "Re-finalized block #{:?} ({:?}) in the canonical chain, current best finalized is #{:?}",
				  hash,
				  number,
				  status.finalized_number,
			);

			return Ok(());
		}

		finalize_block(
			&*self.inner,
			&self.authority_set,
			&self.consensus_changes,
			Some(As::sa(self.config.justification_period)),
			hash,
			number,
			(round, commit).into(),
		)
	}

	fn round_commit_timer(&self) -> Self::Timer {
		use rand::{thread_rng, Rng};

		//random between 0-1 seconds.
		let delay: u64 = thread_rng().gen_range(0, 1000);
		Box::new(Delay::new(
			Instant::now() + Duration::from_millis(delay)
		).map_err(|e| Error::Timer(e).into()))
	}

	fn prevote_equivocation(
		&self,
		_round: u64,
		equivocation: ::grandpa::Equivocation<Self::Id, Prevote<Block>, Self::Signature>
	) {
		warn!(target: "afg", "Detected prevote equivocation in the finality worker: {:?}", equivocation);
		// nothing yet; this could craft misbehavior reports of some kind.
	}

	fn precommit_equivocation(
		&self,
		_round: u64,
		equivocation: Equivocation<Self::Id, Precommit<Block>, Self::Signature>
	) {
		warn!(target: "afg", "Detected precommit equivocation in the finality worker: {:?}", equivocation);
		// nothing yet
	}
}

pub(crate) enum JustificationOrCommit<Block: BlockT> {
	Justification(GrandpaJustification<Block>),
	Commit((u64, Commit<Block>)),
}

impl<Block: BlockT> From<(u64, Commit<Block>)> for JustificationOrCommit<Block> {
	fn from(commit: (u64, Commit<Block>)) -> JustificationOrCommit<Block> {
		JustificationOrCommit::Commit(commit)
	}
}

impl<Block: BlockT> From<GrandpaJustification<Block>> for JustificationOrCommit<Block> {
	fn from(justification: GrandpaJustification<Block>) -> JustificationOrCommit<Block> {
		JustificationOrCommit::Justification(justification)
	}
}

/// Finalize the given block and apply any authority set changes. If an
/// authority set change is enacted then a justification is created (if not
/// given) and stored with the block when finalizing it.
/// This method assumes that the block being finalized has already been imported.
pub(crate) fn finalize_block<B, Block: BlockT<Hash=H256>, E, RA>(
	client: &Client<B, E, Block, RA>,
	authority_set: &SharedAuthoritySet<Block::Hash, NumberFor<Block>>,
	consensus_changes: &SharedConsensusChanges<Block::Hash, NumberFor<Block>>,
	justification_period: Option<NumberFor<Block>>,
	hash: Block::Hash,
	number: NumberFor<Block>,
	justification_or_commit: JustificationOrCommit<Block>,
) -> Result<(), CommandOrError<Block::Hash, NumberFor<Block>>> where
	B: Backend<Block, Blake2Hasher>,
	E: CallExecutor<Block, Blake2Hasher> + Send + Sync,
	RA: Send + Sync,
{
	// lock must be held through writing to DB to avoid race
	let mut authority_set = authority_set.inner().write();

	// FIXME #1483: clone only when changed
	let old_authority_set = authority_set.clone();
	// holds the old consensus changes in case it is changed below, needed for
	// reverting in case of failure
	let mut old_consensus_changes = None;

	let mut consensus_changes = consensus_changes.lock();
	let canon_at_height = |canon_number| {
		// "true" because the block is finalized
		canonical_at_height(client, (hash, number), true, canon_number)
	};

	let update_res: Result<_, Error> = client.lock_import_and_run(|import_op| {
		let status = authority_set.apply_standard_changes(
			hash,
			number,
			&is_descendent_of(client, None),
		).map_err(|e| Error::Safety(e.to_string()))?;

		// check if this is this is the first finalization of some consensus changes
		let (alters_consensus_changes, finalizes_consensus_changes) = consensus_changes
			.finalize((number, hash), &canon_at_height)?;

		if alters_consensus_changes {
			old_consensus_changes = Some(consensus_changes.clone());

			let write_result = crate::aux_schema::update_consensus_changes(
				&*consensus_changes,
				|insert| client.apply_aux(import_op, insert, &[]),
			);

			if let Err(e) = write_result {
				warn!(target: "finality", "Failed to write updated consensus changes to disk. Bailing.");
				warn!(target: "finality", "Node is in a potentially inconsistent state.");

				return Err(e.into());
			}
		}

		// NOTE: this code assumes that honest voters will never vote past a
		// transition block, thus we don't have to worry about the case where
		// we have a transition with `effective_block = N`, but we finalize
		// `N+1`. this assumption is required to make sure we store
		// justifications for transition blocks which will be requested by
		// syncing clients.
		let justification = match justification_or_commit {
			JustificationOrCommit::Justification(justification) => Some(justification.encode()),
			JustificationOrCommit::Commit((round_number, commit)) => {
				let mut justification_required =
					// justification is always required when block that enacts new authorities
					// set is finalized
					status.new_set_block.is_some() ||
					// justification is required when consensus changes are finalized
					finalizes_consensus_changes;

				// justification is required every N blocks to be able to prove blocks
				// finalization to remote nodes
				if !justification_required {
					if let Some(justification_period) = justification_period {
						let last_finalized_number = client.info()?.chain.finalized_number;
						justification_required =
							(!last_finalized_number.is_zero() || number - last_finalized_number == justification_period) &&
							(last_finalized_number / justification_period != number / justification_period);
					}
				}

				if justification_required {
					let justification = GrandpaJustification::from_commit(
						client,
						round_number,
						commit,
					)?;

					Some(justification.encode())
				} else {
					None
				}
			},
		};

		debug!(target: "afg", "Finalizing blocks up to ({:?}, {})", number, hash);

		// ideally some handle to a synchronization oracle would be used
		// to avoid unconditionally notifying.
		client.apply_finality(import_op, BlockId::Hash(hash), justification, true).map_err(|e| {
			warn!(target: "finality", "Error applying finality to block {:?}: {:?}", (hash, number), e);
			e
		})?;

		let new_authorities = if let Some((canon_hash, canon_number)) = status.new_set_block {
			// the authority set has changed.
			let (new_id, set_ref) = authority_set.current();

			if set_ref.len() > 16 {
				info!("Applying GRANDPA set change to new set with {} authorities", set_ref.len());
			} else {
				info!("Applying GRANDPA set change to new set {:?}", set_ref);
			}

			Some(NewAuthoritySet {
				canon_hash,
				canon_number,
				set_id: new_id,
				authorities: set_ref.to_vec(),
			})
		} else {
			None
		};

		if status.changed {
			let write_result = crate::aux_schema::update_authority_set(
				&authority_set,
				new_authorities.as_ref(),
				|insert| client.apply_aux(import_op, insert, &[]),
			);

			if let Err(e) = write_result {
				warn!(target: "finality", "Failed to write updated authority set to disk. Bailing.");
				warn!(target: "finality", "Node is in a potentially inconsistent state.");

				return Err(e.into());
			}
		}

		Ok(new_authorities.map(VoterCommand::ChangeAuthorities))
	});

	match update_res {
		Ok(Some(command)) => Err(CommandOrError::VoterCommand(command)),
		Ok(None) => Ok(()),
		Err(e) => {
			*authority_set = old_authority_set;

			if let Some(old_consensus_changes) = old_consensus_changes {
				*consensus_changes = old_consensus_changes;
			}

			Err(CommandOrError::Error(e))
		}
	}
}

/// Using the given base get the block at the given height on this chain. The
/// target block must be an ancestor of base, therefore `height <= base.height`.
pub(crate) fn canonical_at_height<B, E, Block: BlockT<Hash=H256>, RA>(
	client: &Client<B, E, Block, RA>,
	base: (Block::Hash, NumberFor<Block>),
	base_is_canonical: bool,
	height: NumberFor<Block>,
) -> Result<Option<Block::Hash>, ClientError> where
	B: Backend<Block, Blake2Hasher>,
	E: CallExecutor<Block, Blake2Hasher> + Send + Sync,
{
	use runtime_primitives::traits::{One, Zero, BlockNumberToHash};

	if height > base.1 {
		return Ok(None);
	}

	if height == base.1 {
		if base_is_canonical {
			return Ok(Some(base.0));
		} else {
			return Ok(client.block_number_to_hash(height));
		}
	} else if base_is_canonical {
		return Ok(client.block_number_to_hash(height));
	}

	let one = NumberFor::<Block>::one();

	// start by getting _canonical_ block with number at parent position and then iterating
	// backwards by hash.
	let mut current = match client.header(&BlockId::Number(base.1 - one))? {
		Some(header) => header,
		_ => return Ok(None),
	};

	// we've already checked that base > height above.
	let mut steps = base.1 - height - one;

	while steps > NumberFor::<Block>::zero() {
		current = match client.header(&BlockId::Hash(*current.parent_hash()))? {
			Some(header) => header,
			_ => return Ok(None),
		};

		steps -= one;
	}

	Ok(Some(current.hash()))
}

/// Returns a function for checking block ancestry, the returned function will
/// return `true` if the given hash (second parameter) is a descendent of the
/// base (first parameter). If the `current` parameter is defined, it should
/// represent the current block `hash` and its `parent hash`, if given the
/// function that's returned will assume that `hash` isn't part of the local DB
/// yet, and all searches in the DB will instead reference the parent.
pub fn is_descendent_of<'a, B, E, Block: BlockT<Hash=H256>, RA>(
	client: &'a Client<B, E, Block, RA>,
	current: Option<(&'a H256, &'a H256)>,
) -> impl Fn(&H256, &H256) -> Result<bool, client::error::Error> + 'a
where B: Backend<Block, Blake2Hasher>,
	  E: CallExecutor<Block, Blake2Hasher> + Send + Sync,
{
	move |base, hash| {
		if base == hash { return Ok(false); }

		let mut hash = hash;
		if let Some((current_hash, current_parent_hash)) = current {
			if base == current_hash { return Ok(false); }
			if hash == current_hash {
				if base == current_parent_hash {
					return Ok(true);
				} else {
					hash = current_parent_hash;
				}
			}
		}

		let tree_route = client::blockchain::tree_route(
			client.backend().blockchain(),
			BlockId::Hash(*hash),
			BlockId::Hash(*base),
		)?;

		Ok(tree_route.common_block().hash == *base)
	}
}
