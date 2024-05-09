// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

//! Basic delivery strategy. The strategy selects nonces if:
//!
//! 1) there are more nonces on the source side than on the target side;
//! 2) new nonces may be proved to target node (i.e. they have appeared at the block, which is known
//!    to the target node).

use crate::message_race_loop::{
	NoncesRange, RaceState, RaceStrategy, SourceClientNonces, TargetClientNonces,
};

use async_trait::async_trait;
use bp_messages::MessageNonce;
use relay_utils::HeaderId;
use std::{collections::VecDeque, fmt::Debug, marker::PhantomData, ops::RangeInclusive};

/// Queue of nonces known to the source node.
pub type SourceRangesQueue<SourceHeaderHash, SourceHeaderNumber, SourceNoncesRange> =
	VecDeque<(HeaderId<SourceHeaderHash, SourceHeaderNumber>, SourceNoncesRange)>;

/// Nonces delivery strategy.
#[derive(Debug)]
pub struct BasicStrategy<
	SourceHeaderNumber,
	SourceHeaderHash,
	TargetHeaderNumber,
	TargetHeaderHash,
	SourceNoncesRange,
	Proof,
> {
	/// All queued nonces.
	///
	/// The queue may contain already delivered nonces. We only remove entries from this
	/// queue after corresponding nonces are finalized by the target chain.
	source_queue: SourceRangesQueue<SourceHeaderHash, SourceHeaderNumber, SourceNoncesRange>,
	/// The best nonce known to target node at its best block. `None` if it has not been received
	/// yet.
	best_target_nonce: Option<MessageNonce>,
	/// Unused generic types dump.
	_phantom: PhantomData<(TargetHeaderNumber, TargetHeaderHash, Proof)>,
}

impl<
		SourceHeaderNumber,
		SourceHeaderHash,
		TargetHeaderNumber,
		TargetHeaderHash,
		SourceNoncesRange,
		Proof,
	>
	BasicStrategy<
		SourceHeaderNumber,
		SourceHeaderHash,
		TargetHeaderNumber,
		TargetHeaderHash,
		SourceNoncesRange,
		Proof,
	> where
	SourceHeaderHash: Clone,
	SourceHeaderNumber: Clone + Ord,
	SourceNoncesRange: NoncesRange,
{
	/// Create new delivery strategy.
	pub fn new() -> Self {
		BasicStrategy {
			source_queue: VecDeque::new(),
			best_target_nonce: None,
			_phantom: Default::default(),
		}
	}

	/// Reference to source queue.
	pub(crate) fn source_queue(
		&self,
	) -> &VecDeque<(HeaderId<SourceHeaderHash, SourceHeaderNumber>, SourceNoncesRange)> {
		&self.source_queue
	}

	/// Mutable reference to source queue to use in tests.
	#[cfg(test)]
	pub(crate) fn source_queue_mut(
		&mut self,
	) -> &mut VecDeque<(HeaderId<SourceHeaderHash, SourceHeaderNumber>, SourceNoncesRange)> {
		&mut self.source_queue
	}

	/// Returns indices of source queue entries, which may be delivered to the target node.
	///
	/// The function may skip some nonces from the queue front if nonces from this entry are
	/// already available at the **best** target block. After this block is finalized, the entry
	/// will be removed from the queue.
	///
	/// All entries before and including the range end index, are guaranteed to be witnessed
	/// at source blocks that are known to be finalized at the target node.
	///
	/// Returns `None` if no entries may be delivered.
	pub fn available_source_queue_indices<
		RS: RaceState<
			HeaderId<SourceHeaderHash, SourceHeaderNumber>,
			HeaderId<TargetHeaderHash, TargetHeaderNumber>,
		>,
	>(
		&self,
		race_state: RS,
	) -> Option<RangeInclusive<usize>> {
		// if we do not know best nonce at target node, we can't select anything
		let best_target_nonce = self.best_target_nonce?;

		// if we have already selected nonces that we want to submit, do nothing
		if race_state.nonces_to_submit().is_some() {
			return None
		}

		// if we already submitted some nonces, do nothing
		if race_state.nonces_submitted().is_some() {
			return None
		}

		// find first entry that may be delivered to the target node
		let begin_index = self
			.source_queue
			.iter()
			.enumerate()
			.skip_while(|(_, (_, nonces))| nonces.end() <= best_target_nonce)
			.map(|(index, _)| index)
			.next()?;

		// 1) we want to deliver all nonces, starting from `target_nonce + 1`
		// 2) we can't deliver new nonce until header, that has emitted this nonce, is finalized
		// by target client
		// 3) selector is used for more complicated logic
		//
		// => let's first select range of entries inside deque that are already finalized at
		// the target client and pass this range to the selector
		let best_header_at_target = race_state.best_finalized_source_header_id_at_best_target()?;
		let end_index = self
			.source_queue
			.iter()
			.enumerate()
			.skip(begin_index)
			.take_while(|(_, (queued_at, _))| queued_at.0 <= best_header_at_target.0)
			.map(|(index, _)| index)
			.last()?;

		Some(begin_index..=end_index)
	}

	/// Remove all nonces that are less than or equal to given nonce from the source queue.
	fn remove_le_nonces_from_source_queue(&mut self, nonce: MessageNonce) {
		while let Some((queued_at, queued_range)) = self.source_queue.pop_front() {
			if let Some(range_to_requeue) = queued_range.greater_than(nonce) {
				self.source_queue.push_front((queued_at, range_to_requeue));
				break
			}
		}
	}
}

#[async_trait]
impl<
		SourceHeaderNumber,
		SourceHeaderHash,
		TargetHeaderNumber,
		TargetHeaderHash,
		SourceNoncesRange,
		Proof,
	>
	RaceStrategy<
		HeaderId<SourceHeaderHash, SourceHeaderNumber>,
		HeaderId<TargetHeaderHash, TargetHeaderNumber>,
		Proof,
	>
	for BasicStrategy<
		SourceHeaderNumber,
		SourceHeaderHash,
		TargetHeaderNumber,
		TargetHeaderHash,
		SourceNoncesRange,
		Proof,
	> where
	SourceHeaderHash: Clone + Debug + Send + Sync,
	SourceHeaderNumber: Clone + Ord + Debug + Send + Sync,
	SourceNoncesRange: NoncesRange + Debug + Send + Sync,
	TargetHeaderHash: Debug + Send + Sync,
	TargetHeaderNumber: Debug + Send + Sync,
	Proof: Debug + Send + Sync,
{
	type SourceNoncesRange = SourceNoncesRange;
	type ProofParameters = ();
	type TargetNoncesData = ();

	fn is_empty(&self) -> bool {
		self.source_queue.is_empty()
	}

	async fn required_source_header_at_target<
		RS: RaceState<
			HeaderId<SourceHeaderHash, SourceHeaderNumber>,
			HeaderId<TargetHeaderHash, TargetHeaderNumber>,
		>,
	>(
		&self,
		race_state: RS,
	) -> Option<HeaderId<SourceHeaderHash, SourceHeaderNumber>> {
		let current_best = race_state.best_finalized_source_header_id_at_best_target()?;
		self.source_queue
			.back()
			.and_then(|(h, _)| if h.0 > current_best.0 { Some(h.clone()) } else { None })
	}

	fn best_at_source(&self) -> Option<MessageNonce> {
		let best_in_queue = self.source_queue.back().map(|(_, range)| range.end());
		match (best_in_queue, self.best_target_nonce) {
			(Some(best_in_queue), Some(best_target_nonce)) if best_in_queue > best_target_nonce =>
				Some(best_in_queue),
			(_, Some(best_target_nonce)) => Some(best_target_nonce),
			(_, None) => None,
		}
	}

	fn best_at_target(&self) -> Option<MessageNonce> {
		self.best_target_nonce
	}

	fn source_nonces_updated(
		&mut self,
		at_block: HeaderId<SourceHeaderHash, SourceHeaderNumber>,
		nonces: SourceClientNonces<SourceNoncesRange>,
	) {
		let best_in_queue = self
			.source_queue
			.back()
			.map(|(_, range)| range.end())
			.or(self.best_target_nonce)
			.unwrap_or_default();
		self.source_queue.extend(
			nonces
				.new_nonces
				.greater_than(best_in_queue)
				.into_iter()
				.map(move |range| (at_block.clone(), range)),
		)
	}

	fn reset_best_target_nonces(&mut self) {
		self.best_target_nonce = None;
	}

	fn best_target_nonces_updated<
		RS: RaceState<
			HeaderId<SourceHeaderHash, SourceHeaderNumber>,
			HeaderId<TargetHeaderHash, TargetHeaderNumber>,
		>,
	>(
		&mut self,
		nonces: TargetClientNonces<()>,
		race_state: &mut RS,
	) {
		let nonce = nonces.latest_nonce;

		// if **some** of nonces that we have selected to submit already present at the
		// target chain => select new nonces
		let need_to_select_new_nonces = race_state
			.nonces_to_submit()
			.map(|nonces| nonce >= *nonces.start())
			.unwrap_or(false);
		if need_to_select_new_nonces {
			log::trace!(
				target: "bridge",
				"Latest nonce at target is {}. Clearing nonces to submit: {:?}",
				nonce,
				race_state.nonces_to_submit(),
			);

			race_state.reset_nonces_to_submit();
		}

		// if **some** of nonces that we have submitted already present at the
		// target chain => select new nonces
		let need_new_nonces_to_submit = race_state
			.nonces_submitted()
			.map(|nonces| nonce >= *nonces.start())
			.unwrap_or(false);
		if need_new_nonces_to_submit {
			log::trace!(
				target: "bridge",
				"Latest nonce at target is {}. Clearing submitted nonces: {:?}",
				nonce,
				race_state.nonces_submitted(),
			);

			race_state.reset_nonces_submitted();
		}

		self.best_target_nonce = Some(nonce);
	}

	fn finalized_target_nonces_updated<
		RS: RaceState<
			HeaderId<SourceHeaderHash, SourceHeaderNumber>,
			HeaderId<TargetHeaderHash, TargetHeaderNumber>,
		>,
	>(
		&mut self,
		nonces: TargetClientNonces<()>,
		_race_state: &mut RS,
	) {
		self.remove_le_nonces_from_source_queue(nonces.latest_nonce);
		self.best_target_nonce = Some(std::cmp::max(
			self.best_target_nonce.unwrap_or(nonces.latest_nonce),
			nonces.latest_nonce,
		));
	}

	async fn select_nonces_to_deliver<
		RS: RaceState<
			HeaderId<SourceHeaderHash, SourceHeaderNumber>,
			HeaderId<TargetHeaderHash, TargetHeaderNumber>,
		>,
	>(
		&self,
		race_state: RS,
	) -> Option<(RangeInclusive<MessageNonce>, Self::ProofParameters)> {
		let available_indices = self.available_source_queue_indices(race_state)?;
		let range_begin = std::cmp::max(
			self.best_target_nonce? + 1,
			self.source_queue[*available_indices.start()].1.begin(),
		);
		let range_end = self.source_queue[*available_indices.end()].1.end();
		Some((range_begin..=range_end, ()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{
		message_lane::{MessageLane, SourceHeaderIdOf, TargetHeaderIdOf},
		message_lane_loop::tests::{
			header_id, TestMessageLane, TestMessagesProof, TestSourceHeaderHash,
			TestSourceHeaderNumber,
		},
		message_race_loop::RaceStateImpl,
	};

	type SourceNoncesRange = RangeInclusive<MessageNonce>;

	type TestRaceStateImpl = RaceStateImpl<
		SourceHeaderIdOf<TestMessageLane>,
		TargetHeaderIdOf<TestMessageLane>,
		TestMessagesProof,
		(),
	>;

	type BasicStrategy<P> = super::BasicStrategy<
		<P as MessageLane>::SourceHeaderNumber,
		<P as MessageLane>::SourceHeaderHash,
		<P as MessageLane>::TargetHeaderNumber,
		<P as MessageLane>::TargetHeaderHash,
		SourceNoncesRange,
		<P as MessageLane>::MessagesProof,
	>;

	fn source_nonces(new_nonces: SourceNoncesRange) -> SourceClientNonces<SourceNoncesRange> {
		SourceClientNonces { new_nonces, confirmed_nonce: None }
	}

	fn target_nonces(latest_nonce: MessageNonce) -> TargetClientNonces<()> {
		TargetClientNonces { latest_nonce, nonces_data: () }
	}

	#[test]
	fn strategy_is_empty_works() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		assert!(strategy.is_empty());
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=1));
		assert!(!strategy.is_empty());
	}

	#[test]
	fn best_at_source_is_never_lower_than_target_nonce() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		assert_eq!(strategy.best_at_source(), None);
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=5));
		assert_eq!(strategy.best_at_source(), None);
		strategy.best_target_nonces_updated(target_nonces(10), &mut TestRaceStateImpl::default());
		assert_eq!(strategy.source_queue, vec![(header_id(1), 1..=5)]);
		assert_eq!(strategy.best_at_source(), Some(10));
	}

	#[test]
	fn source_nonce_is_never_lower_than_known_target_nonce() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		strategy.best_target_nonces_updated(target_nonces(10), &mut TestRaceStateImpl::default());
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=5));
		assert_eq!(strategy.source_queue, vec![]);
	}

	#[test]
	fn source_nonce_is_never_lower_than_latest_known_source_nonce() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=5));
		strategy.source_nonces_updated(header_id(2), source_nonces(1..=3));
		strategy.source_nonces_updated(header_id(2), source_nonces(1..=5));
		assert_eq!(strategy.source_queue, vec![(header_id(1), 1..=5)]);
	}

	#[test]
	fn updated_target_nonce_removes_queued_entries() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=5));
		strategy.source_nonces_updated(header_id(2), source_nonces(6..=10));
		strategy.source_nonces_updated(header_id(3), source_nonces(11..=15));
		strategy.source_nonces_updated(header_id(4), source_nonces(16..=20));
		strategy
			.finalized_target_nonces_updated(target_nonces(15), &mut TestRaceStateImpl::default());
		assert_eq!(strategy.source_queue, vec![(header_id(4), 16..=20)]);
		strategy
			.finalized_target_nonces_updated(target_nonces(17), &mut TestRaceStateImpl::default());
		assert_eq!(strategy.source_queue, vec![(header_id(4), 18..=20)]);
	}

	#[test]
	fn selected_nonces_are_dropped_on_target_nonce_update() {
		let mut state = TestRaceStateImpl::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		state.nonces_to_submit = Some((header_id(1), 5..=10, (5..=10, None)));
		// we are going to submit 5..=10, so having latest nonce 4 at target is fine
		strategy.best_target_nonces_updated(target_nonces(4), &mut state);
		assert!(state.nonces_to_submit.is_some());
		// any nonce larger than 4 invalidates the `nonces_to_submit`
		for nonce in 5..=11 {
			state.nonces_to_submit = Some((header_id(1), 5..=10, (5..=10, None)));
			strategy.best_target_nonces_updated(target_nonces(nonce), &mut state);
			assert!(state.nonces_to_submit.is_none());
		}
	}

	#[test]
	fn submitted_nonces_are_dropped_on_target_nonce_update() {
		let mut state = TestRaceStateImpl::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		state.nonces_submitted = Some(5..=10);
		// we have submitted 5..=10, so having latest nonce 4 at target is fine
		strategy.best_target_nonces_updated(target_nonces(4), &mut state);
		assert!(state.nonces_submitted.is_some());
		// any nonce larger than 4 invalidates the `nonces_submitted`
		for nonce in 5..=11 {
			state.nonces_submitted = Some(5..=10);
			strategy.best_target_nonces_updated(target_nonces(nonce), &mut state);
			assert!(state.nonces_submitted.is_none());
		}
	}

	#[async_std::test]
	async fn nothing_is_selected_if_something_is_already_selected() {
		let mut state = TestRaceStateImpl::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		state.nonces_to_submit = Some((header_id(1), 1..=10, (1..=10, None)));
		strategy.best_target_nonces_updated(target_nonces(0), &mut state);
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=10));
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, None);
	}

	#[async_std::test]
	async fn nothing_is_selected_if_something_is_already_submitted() {
		let mut state = TestRaceStateImpl::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		state.nonces_submitted = Some(1..=10);
		strategy.best_target_nonces_updated(target_nonces(0), &mut state);
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=10));
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, None);
	}

	#[async_std::test]
	async fn select_nonces_to_deliver_works() {
		let mut state = TestRaceStateImpl::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		strategy.best_target_nonces_updated(target_nonces(0), &mut state);
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=1));
		strategy.source_nonces_updated(header_id(2), source_nonces(2..=2));
		strategy.source_nonces_updated(header_id(3), source_nonces(3..=6));
		strategy.source_nonces_updated(header_id(5), source_nonces(7..=8));

		state.best_finalized_source_header_id_at_best_target = Some(header_id(4));
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, Some((1..=6, ())));
		strategy.best_target_nonces_updated(target_nonces(6), &mut state);
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, None);

		state.best_finalized_source_header_id_at_best_target = Some(header_id(5));
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, Some((7..=8, ())));
		strategy.best_target_nonces_updated(target_nonces(8), &mut state);
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, None);
	}

	#[test]
	fn available_source_queue_indices_works() {
		let mut state = TestRaceStateImpl::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		strategy.best_target_nonces_updated(target_nonces(0), &mut state);
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=3));
		strategy.source_nonces_updated(header_id(2), source_nonces(4..=6));
		strategy.source_nonces_updated(header_id(3), source_nonces(7..=9));

		state.best_finalized_source_header_id_at_best_target = Some(header_id(0));
		assert_eq!(strategy.available_source_queue_indices(state.clone()), None);

		state.best_finalized_source_header_id_at_best_target = Some(header_id(1));
		assert_eq!(strategy.available_source_queue_indices(state.clone()), Some(0..=0));

		state.best_finalized_source_header_id_at_best_target = Some(header_id(2));
		assert_eq!(strategy.available_source_queue_indices(state.clone()), Some(0..=1));

		state.best_finalized_source_header_id_at_best_target = Some(header_id(3));
		assert_eq!(strategy.available_source_queue_indices(state.clone()), Some(0..=2));

		state.best_finalized_source_header_id_at_best_target = Some(header_id(4));
		assert_eq!(strategy.available_source_queue_indices(state), Some(0..=2));
	}

	#[test]
	fn remove_le_nonces_from_source_queue_works() {
		let mut state = TestRaceStateImpl::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new();
		strategy.best_target_nonces_updated(target_nonces(0), &mut state);
		strategy.source_nonces_updated(header_id(1), source_nonces(1..=3));
		strategy.source_nonces_updated(header_id(2), source_nonces(4..=6));
		strategy.source_nonces_updated(header_id(3), source_nonces(7..=9));

		fn source_queue_nonces(
			source_queue: &SourceRangesQueue<
				TestSourceHeaderHash,
				TestSourceHeaderNumber,
				SourceNoncesRange,
			>,
		) -> Vec<MessageNonce> {
			source_queue.iter().flat_map(|(_, range)| range.clone()).collect()
		}

		strategy.remove_le_nonces_from_source_queue(1);
		assert_eq!(source_queue_nonces(&strategy.source_queue), vec![2, 3, 4, 5, 6, 7, 8, 9],);

		strategy.remove_le_nonces_from_source_queue(5);
		assert_eq!(source_queue_nonces(&strategy.source_queue), vec![6, 7, 8, 9],);

		strategy.remove_le_nonces_from_source_queue(9);
		assert_eq!(source_queue_nonces(&strategy.source_queue), Vec::<MessageNonce>::new(),);

		strategy.remove_le_nonces_from_source_queue(100);
		assert_eq!(source_queue_nonces(&strategy.source_queue), Vec::<MessageNonce>::new(),);
	}

	#[async_std::test]
	async fn previous_nonces_are_selected_if_reorg_happens_at_target_chain() {
		let source_header_1 = header_id(1);
		let target_header_1 = header_id(1);

		// we start in perfect sync state - all headers are synced and finalized on both ends
		let mut state = TestRaceStateImpl {
			best_finalized_source_header_id_at_source: Some(source_header_1),
			best_finalized_source_header_id_at_best_target: Some(source_header_1),
			best_target_header_id: Some(target_header_1),
			best_finalized_target_header_id: Some(target_header_1),
			nonces_to_submit: None,
			nonces_to_submit_batch: None,
			nonces_submitted: None,
		};

		// in this state we have 1 available nonce for delivery
		let mut strategy = BasicStrategy::<TestMessageLane> {
			source_queue: vec![(header_id(1), 1..=1)].into_iter().collect(),
			best_target_nonce: Some(0),
			_phantom: PhantomData,
		};
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, Some((1..=1, ())),);

		// let's say we have submitted 1..=1
		state.nonces_submitted = Some(1..=1);

		// then new nonce 2 appear at the source block 2
		let source_header_2 = header_id(2);
		state.best_finalized_source_header_id_at_source = Some(source_header_2);
		strategy.source_nonces_updated(
			source_header_2,
			SourceClientNonces { new_nonces: 2..=2, confirmed_nonce: None },
		);
		// and nonce 1 appear at the best block of the target node (best finalized still has 0
		// nonces)
		let target_header_2 = header_id(2);
		state.best_target_header_id = Some(target_header_2);
		strategy.best_target_nonces_updated(
			TargetClientNonces { latest_nonce: 1, nonces_data: () },
			&mut state,
		);

		// then best target header is retracted
		strategy.best_target_nonces_updated(
			TargetClientNonces { latest_nonce: 0, nonces_data: () },
			&mut state,
		);

		// ... and some fork with zero delivered nonces is finalized
		let target_header_2_fork = header_id(2_1);
		state.best_finalized_source_header_id_at_source = Some(source_header_2);
		state.best_finalized_source_header_id_at_best_target = Some(source_header_2);
		state.best_target_header_id = Some(target_header_2_fork);
		state.best_finalized_target_header_id = Some(target_header_2_fork);
		strategy.finalized_target_nonces_updated(
			TargetClientNonces { latest_nonce: 0, nonces_data: () },
			&mut state,
		);

		// now we have to select nonce 1 for delivery again
		assert_eq!(strategy.select_nonces_to_deliver(state.clone()).await, Some((1..=2, ())),);
	}
}
