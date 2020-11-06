// Copyright 2019-2020 Parity Technologies (UK) Ltd.
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
//! 2) new nonces may be proved to target node (i.e. they have appeared at the
//!    block, which is known to the target node).

use crate::message_race_loop::{ClientNonces, RaceState, RaceStrategy};

use num_traits::{One, Zero};
use relay_utils::HeaderId;
use std::{collections::VecDeque, marker::PhantomData, ops::RangeInclusive};

/// Nonces delivery strategy.
#[derive(Debug)]
pub struct BasicStrategy<SourceHeaderNumber, SourceHeaderHash, TargetHeaderNumber, TargetHeaderHash, Nonce, Proof> {
	/// All queued nonces.
	source_queue: VecDeque<(HeaderId<SourceHeaderHash, SourceHeaderNumber>, Nonce)>,
	/// Best nonce known to target node.
	target_nonce: Nonce,
	/// Max nonces to relay in single transaction.
	max_nonces_to_relay_in_single_tx: Nonce,
	/// Unused generic types dump.
	_phantom: PhantomData<(TargetHeaderNumber, TargetHeaderHash, Proof)>,
}

impl<SourceHeaderNumber, SourceHeaderHash, TargetHeaderNumber, TargetHeaderHash, Nonce: Default, Proof>
	BasicStrategy<SourceHeaderNumber, SourceHeaderHash, TargetHeaderNumber, TargetHeaderHash, Nonce, Proof>
{
	/// Create new delivery strategy.
	pub fn new(max_nonces_to_relay_in_single_tx: Nonce) -> Self {
		BasicStrategy {
			source_queue: VecDeque::new(),
			target_nonce: Default::default(),
			max_nonces_to_relay_in_single_tx,
			_phantom: Default::default(),
		}
	}
}

impl<SourceHeaderNumber, SourceHeaderHash, TargetHeaderNumber, TargetHeaderHash, Nonce, Proof>
	RaceStrategy<
		HeaderId<SourceHeaderHash, SourceHeaderNumber>,
		HeaderId<TargetHeaderHash, TargetHeaderNumber>,
		Nonce,
		Proof,
	> for BasicStrategy<SourceHeaderNumber, SourceHeaderHash, TargetHeaderNumber, TargetHeaderHash, Nonce, Proof>
where
	SourceHeaderHash: Clone,
	SourceHeaderNumber: Clone + Ord,
	Nonce: Clone + Copy + From<u32> + Ord + std::ops::Add<Output = Nonce> + One + Zero,
{
	type ProofParameters = ();

	fn is_empty(&self) -> bool {
		self.source_queue.is_empty()
	}

	fn best_at_source(&self) -> Nonce {
		self.source_queue
			.back()
			.map(|(_, nonce)| *nonce)
			.unwrap_or_else(Zero::zero)
	}

	fn best_at_target(&self) -> Nonce {
		self.target_nonce
	}

	fn source_nonces_updated(
		&mut self,
		at_block: HeaderId<SourceHeaderHash, SourceHeaderNumber>,
		nonces: ClientNonces<Nonce>,
	) {
		let nonce = nonces.latest_nonce;

		if nonce <= self.target_nonce {
			return;
		}

		match self.source_queue.back() {
			Some((_, prev_nonce)) if *prev_nonce < nonce => (),
			Some(_) => return,
			None => (),
		}

		self.source_queue.push_back((at_block, nonce))
	}

	fn target_nonces_updated(
		&mut self,
		nonces: ClientNonces<Nonce>,
		race_state: &mut RaceState<
			HeaderId<SourceHeaderHash, SourceHeaderNumber>,
			HeaderId<TargetHeaderHash, TargetHeaderNumber>,
			Nonce,
			Proof,
		>,
	) {
		let nonce = nonces.latest_nonce;

		if nonce < self.target_nonce {
			return;
		}

		while let Some(true) = self
			.source_queue
			.front()
			.map(|(_, source_nonce)| *source_nonce <= nonce)
		{
			self.source_queue.pop_front();
		}

		let need_to_select_new_nonces = race_state
			.nonces_to_submit
			.as_ref()
			.map(|(_, nonces, _)| *nonces.end() <= nonce)
			.unwrap_or(false);
		if need_to_select_new_nonces {
			race_state.nonces_to_submit = None;
		}

		let need_new_nonces_to_submit = race_state
			.nonces_submitted
			.as_ref()
			.map(|nonces| *nonces.end() <= nonce)
			.unwrap_or(false);
		if need_new_nonces_to_submit {
			race_state.nonces_submitted = None;
		}

		self.target_nonce = nonce;
	}

	fn select_nonces_to_deliver(
		&mut self,
		race_state: &RaceState<
			HeaderId<SourceHeaderHash, SourceHeaderNumber>,
			HeaderId<TargetHeaderHash, TargetHeaderNumber>,
			Nonce,
			Proof,
		>,
	) -> Option<(RangeInclusive<Nonce>, Self::ProofParameters)> {
		// if we have already selected nonces that we want to submit, do nothing
		if race_state.nonces_to_submit.is_some() {
			return None;
		}

		// if we already submitted some nonces, do nothing
		if race_state.nonces_submitted.is_some() {
			return None;
		}

		// 1) we want to deliver all nonces, starting from `target_nonce + 1`
		// 2) we want to deliver at most `self.max_nonces_to_relay_in_single_tx` nonces in this batch
		// 3) we can't deliver new nonce until header, that has emitted this nonce, is finalized
		// by target client
		let nonces_begin = self.target_nonce + 1.into();
		let best_header_at_target = &race_state.target_state.as_ref()?.best_peer;
		let mut nonces_end = None;
		let mut i = Zero::zero();

		// https://github.com/paritytech/parity-bridges-common/issues/433
		// TODO: instead of limiting number of messages by number, provide custom limit callback here.
		// In delivery race it'll be weight-based callback. In receiving race it'll be unlimited callback.

		while i < self.max_nonces_to_relay_in_single_tx {
			let nonce = nonces_begin + i;

			// if queue is empty, we don't need to prove anything
			let (first_queued_at, first_queued_nonce) = match self.source_queue.front() {
				Some((first_queued_at, first_queued_nonce)) => ((*first_queued_at).clone(), *first_queued_nonce),
				None => break,
			};

			// if header that has queued the message is not yet finalized at bridged chain,
			// we can't prove anything
			if first_queued_at.0 > best_header_at_target.0 {
				break;
			}

			// ok, we may deliver this nonce
			nonces_end = Some(nonce);

			// probably remove it from the queue?
			if nonce == first_queued_nonce {
				self.source_queue.pop_front();
			}

			i = i + One::one();
		}

		nonces_end.map(|nonces_end| (RangeInclusive::new(nonces_begin, nonces_end), ()))
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::message_lane::MessageLane;
	use crate::message_lane_loop::{
		tests::{header_id, TestMessageLane, TestMessageNonce, TestMessagesProof},
		ClientState,
	};

	type BasicStrategy<P> = super::BasicStrategy<
		<P as MessageLane>::SourceHeaderNumber,
		<P as MessageLane>::SourceHeaderHash,
		<P as MessageLane>::TargetHeaderNumber,
		<P as MessageLane>::TargetHeaderHash,
		<P as MessageLane>::MessageNonce,
		<P as MessageLane>::MessagesProof,
	>;

	fn nonces(latest_nonce: TestMessageNonce) -> ClientNonces<TestMessageNonce> {
		ClientNonces {
			latest_nonce,
			confirmed_nonce: None,
		}
	}

	#[test]
	fn strategy_is_empty_works() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		assert_eq!(strategy.is_empty(), true);
		strategy.source_nonces_updated(header_id(1), nonces(1));
		assert_eq!(strategy.is_empty(), false);
	}

	#[test]
	fn source_nonce_is_never_lower_than_known_target_nonce() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		strategy.target_nonces_updated(nonces(10), &mut Default::default());
		strategy.source_nonces_updated(header_id(1), nonces(5));
		assert_eq!(strategy.source_queue, vec![]);
	}

	#[test]
	fn source_nonce_is_never_lower_than_latest_known_source_nonce() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		strategy.source_nonces_updated(header_id(1), nonces(5));
		strategy.source_nonces_updated(header_id(2), nonces(3));
		strategy.source_nonces_updated(header_id(2), nonces(5));
		assert_eq!(strategy.source_queue, vec![(header_id(1), 5)]);
	}

	#[test]
	fn target_nonce_is_never_lower_than_latest_known_target_nonce() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		strategy.target_nonces_updated(nonces(10), &mut Default::default());
		strategy.target_nonces_updated(nonces(5), &mut Default::default());
		assert_eq!(strategy.target_nonce, 10);
	}

	#[test]
	fn updated_target_nonce_removes_queued_entries() {
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		strategy.source_nonces_updated(header_id(1), nonces(5));
		strategy.source_nonces_updated(header_id(2), nonces(10));
		strategy.source_nonces_updated(header_id(3), nonces(15));
		strategy.source_nonces_updated(header_id(4), nonces(20));
		strategy.target_nonces_updated(nonces(15), &mut Default::default());
		assert_eq!(strategy.source_queue, vec![(header_id(4), 20)]);
	}

	#[test]
	fn selected_nonces_are_dropped_on_target_nonce_update() {
		let mut state = RaceState::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		state.nonces_to_submit = Some((header_id(1), 5..=10, (5..=10, None)));
		strategy.target_nonces_updated(nonces(7), &mut state);
		assert!(state.nonces_to_submit.is_some());
		strategy.target_nonces_updated(nonces(10), &mut state);
		assert!(state.nonces_to_submit.is_none());
	}

	#[test]
	fn submitted_nonces_are_dropped_on_target_nonce_update() {
		let mut state = RaceState::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		state.nonces_submitted = Some(5..=10);
		strategy.target_nonces_updated(nonces(7), &mut state);
		assert!(state.nonces_submitted.is_some());
		strategy.target_nonces_updated(nonces(10), &mut state);
		assert!(state.nonces_submitted.is_none());
	}

	#[test]
	fn nothing_is_selected_if_something_is_already_selected() {
		let mut state = RaceState::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		state.nonces_to_submit = Some((header_id(1), 1..=10, (1..=10, None)));
		strategy.source_nonces_updated(header_id(1), nonces(10));
		assert_eq!(strategy.select_nonces_to_deliver(&state), None);
	}

	#[test]
	fn nothing_is_selected_if_something_is_already_submitted() {
		let mut state = RaceState::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		state.nonces_submitted = Some(1..=10);
		strategy.source_nonces_updated(header_id(1), nonces(10));
		assert_eq!(strategy.select_nonces_to_deliver(&state), None);
	}

	#[test]
	fn select_nonces_to_deliver_works() {
		let mut state = RaceState::<_, _, TestMessageNonce, TestMessagesProof>::default();
		let mut strategy = BasicStrategy::<TestMessageLane>::new(4);
		strategy.source_nonces_updated(header_id(1), nonces(1));
		strategy.source_nonces_updated(header_id(2), nonces(2));
		strategy.source_nonces_updated(header_id(3), nonces(6));
		strategy.source_nonces_updated(header_id(5), nonces(8));

		state.target_state = Some(ClientState {
			best_self: header_id(0),
			best_peer: header_id(4),
		});
		assert_eq!(strategy.select_nonces_to_deliver(&state), Some((1..=4, ())));
		strategy.target_nonces_updated(nonces(4), &mut state);
		assert_eq!(strategy.select_nonces_to_deliver(&state), Some((5..=6, ())));
		strategy.target_nonces_updated(nonces(6), &mut state);
		assert_eq!(strategy.select_nonces_to_deliver(&state), None);

		state.target_state = Some(ClientState {
			best_self: header_id(0),
			best_peer: header_id(5),
		});
		assert_eq!(strategy.select_nonces_to_deliver(&state), Some((7..=8, ())));
		strategy.target_nonces_updated(nonces(8), &mut state);
		assert_eq!(strategy.select_nonces_to_deliver(&state), None);
	}
}
