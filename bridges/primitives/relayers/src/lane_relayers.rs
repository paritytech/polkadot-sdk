// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Bridge lane relayers.

pub use bp_messages::RelayerRewardAtSource;

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::CloneNoBound;
use scale_info::TypeInfo;
use sp_runtime::{
	traits::{Get, Zero},
	BoundedVec, RuntimeDebug,
};

/// A relayer registration on the lane. Includes reward that the relayer wants to receive
/// for delivering every message.
#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct LaneRegistration<AccountId> {
	/// A relayer account identifier.
	relayer: AccountId,
	/// A reward that is paid to relayer for delivering a single message.
	relayer_reward_per_message: RelayerRewardAtSource,
}

/// Active lane registration for relayer that currently gets priority boost.
#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct ActiveLaneRegistration<AccountId> {
	/// Base lane registration.
	base: LaneRegistration<AccountId>,
	/// Flag, which is set to true if active relayer has delivered at least one message
	/// in current epoch. All such "mergeable" relayers will be inserted to the next set
	/// before next epoch start using current bid (`base.relayer_reward_per_message`).
	is_mergeable: bool,
}

impl<AccountId> LaneRegistration<AccountId> {
	/// Create new instance.
	pub fn new(relayer: AccountId, relayer_reward_per_message: RelayerRewardAtSource) -> Self {
		LaneRegistration { relayer, relayer_reward_per_message }
	}

	/// Return relayer account identifier.
	pub fn relayer(&self) -> &AccountId {
		&self.relayer
	}

	/// Return expected relayer reward.
	pub fn relayer_reward_per_message(&self) -> RelayerRewardAtSource {
		self.relayer_reward_per_message
	}
}

impl<AccountId> ActiveLaneRegistration<AccountId> {
	/// Create new instance.
	pub fn new(relayer: AccountId, relayer_reward_per_message: RelayerRewardAtSource) -> Self {
		ActiveLaneRegistration {
			base: LaneRegistration::new(relayer, relayer_reward_per_message),
			is_mergeable: false,
		}
	}

	/// Return relayer account identifier.
	pub fn relayer(&self) -> &AccountId {
		&self.base.relayer
	}

	/// Return expected relayer reward.
	pub fn relayer_reward_per_message(&self) -> RelayerRewardAtSource {
		self.base.relayer_reward_per_message
	}
}

/// A set of relayers that have explicitly registered themselves at a given lane.
///
/// Every relayer inside this set receives additional priority boost when it submits
/// message delivers messages at given lane. The boost only happens inside the slot,
/// assigned to relayer.
///
/// The active set will eventually be replaced with the [`NextLaneRelayersSet`]. Before
/// replacing, all relayers from the active set, who have delivered at least one messsage
/// at passed epoch, are reinserted into the next set. So if lane is active and relayers
/// area actually delivering messages, they can only be replaced by the relayers, offering
/// lower expected reward.
#[derive(Clone, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(MaxActiveRelayersPerLane))]
pub struct ActiveLaneRelayersSet<AccountId, BlockNumber, MaxActiveRelayersPerLane: Get<u32>> {
	/// Number of block, where the active set has been enacted.
	enacted_at: BlockNumber,
	/// An active set of lane relayers.
	///
	/// It is a circular queue. Every relayer in the queue is assigned the slot (fixed number
	/// of blocks), starting from [`Self::enacted_at`]. Once the slot of last relayer ends,
	/// next slot will be assigned to the first relayer and so on.
	active_set: BoundedVec<ActiveLaneRegistration<AccountId>, MaxActiveRelayersPerLane>,
}

impl<AccountId: Ord, BlockNumber: Zero, MaxActiveRelayersPerLane: Get<u32>> Default
	for ActiveLaneRelayersSet<AccountId, BlockNumber, MaxActiveRelayersPerLane>
{
	fn default() -> Self {
		ActiveLaneRelayersSet { enacted_at: Zero::zero(), active_set: BoundedVec::new() }
	}
}

impl<AccountId, BlockNumber, MaxActiveRelayersPerLane>
	ActiveLaneRelayersSet<AccountId, BlockNumber, MaxActiveRelayersPerLane>
where
	AccountId: Clone + Ord,
	BlockNumber: Copy,
	MaxActiveRelayersPerLane: Get<u32>,
{
	/// Returns block, where this set has been enacted.
	pub fn enacted_at(&self) -> &BlockNumber {
		&self.enacted_at
	}

	/// Returns relayer entry from the active set.
	pub fn relayer(&self, relayer: &AccountId) -> Option<&ActiveLaneRegistration<AccountId>> {
		self.active_set.iter().find(|r| r.base.relayer() == relayer)
	}

	/// Returns relayers from the active set.
	pub fn relayers(&self) -> &[ActiveLaneRegistration<AccountId>] {
		self.active_set.as_slice()
	}

	/// Note message, delivered by given relayer.
	///
	/// Returns true if we have updated anything in the structure.
	pub fn note_delivered_message(&mut self, relayer: &AccountId) -> bool {
		self.active_set
			.iter_mut()
			.find(|r| r.relayer() == relayer)
			.map(|r| {
				let prev_is_mergeable = r.is_mergeable;
				r.is_mergeable = true;
				prev_is_mergeable != r.is_mergeable
			})
			.unwrap_or(false)
	}

	/// Activate next set of relayers.
	///
	/// This set is replaced with the `next_set` contents.
	///
	/// Returns false if `current_block` is lesser than the block where `next_set` may be enacted
	pub fn activate_next_set<MaxNextRelayersPerLane: Get<u32>>(
		&mut self,
		current_block: BlockNumber,
		mut next_set: NextLaneRelayersSet<AccountId, BlockNumber, MaxNextRelayersPerLane>,
		is_lane_registration_active: impl Fn(&AccountId) -> bool,
	) -> bool
	where
		BlockNumber: Ord,
	{
		// ensure that we can enact the next set
		if next_set.may_enact_at > current_block {
			return false
		}

		// merge mergeable relayers into next set
		for relayer in &self.active_set {
			// relayer has not delivered any new messages, do not merge
			if !relayer.is_mergeable {
				continue
			}

			// if relayer lane registration is no longer active, do not merge
			if !is_lane_registration_active(relayer.relayer()) {
				continue
			}

			// else only push it to the next set if it is not yet there to avoid overwriting
			// expected reward
			let is_in_next_set = next_set.relayer(relayer.relayer()).is_some();
			if !is_in_next_set {
				// we do not care if relayer stays in the set - we only need to try
				let _ = next_set
					.try_insert(relayer.relayer().clone(), relayer.relayer_reward_per_message());
			}
		}
		// ...and finally fill the active set with new best relayers
		self.active_set = BoundedVec::truncate_from(
			next_set
				.next_set
				.into_inner()
				.into_iter()
				.map(|base| ActiveLaneRegistration { base, is_mergeable: false })
				.collect(),
		);
		// finally - remember block where we have activated the set
		self.enacted_at = current_block;

		true
	}
}

/// A set of relayers that will become active at next lane epoch.
///
/// The active set of lane relayers is required to change periodically (at `next_set_may_enact_at`).
/// An interval, when the same relayers set is active is called epoch. Every relayer in the epoch
/// is guaranteed to have at least one slot, but epochs may have different lengths.
///
/// We change the set to guarantee that inactive relayers are removed from the set eventually
/// and are replaced by active relayers. The relayer will be scheduled for autoremoval if it
/// has not delivered any messages during previous epoch.
///
/// Relayers are bargaining for the place in the set by offering lower reward for delivering
/// messages. Relayer, which agrees to get a lower reward will likely to replace a "more greedy"
/// relayer in the `next_set`.
#[derive(CloneNoBound, Decode, Encode, Eq, PartialEq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
#[scale_info(skip_type_params(MaxNextRelayersPerLane))]
pub struct NextLaneRelayersSet<
	AccountId: Clone,
	BlockNumber: Clone,
	MaxNextRelayersPerLane: Get<u32>,
> {
	/// Number of block, where the active set may be replaced with the [`Self::next_set`].
	///
	/// We do not allow immediate changes of the active set, because relayers
	/// may change it so that they are always assigned at the current slot.
	may_enact_at: BlockNumber,
	/// Next set of lane relayers.
	///
	/// It is a bounded priority queue. Relayers that are working for larger reward are replaced
	/// with relayers, that are working for smaller reward.
	next_set: BoundedVec<LaneRegistration<AccountId>, MaxNextRelayersPerLane>,
}

impl<AccountId, BlockNumber, MaxNextRelayersPerLane>
	NextLaneRelayersSet<AccountId, BlockNumber, MaxNextRelayersPerLane>
where
	AccountId: Clone + PartialOrd,
	BlockNumber: Copy,
	MaxNextRelayersPerLane: Get<u32>,
{
	/// Creates new empty relayers set, where next sets enacts at given block.
	pub fn empty(may_enact_at: BlockNumber) -> Self {
		NextLaneRelayersSet { may_enact_at, next_set: BoundedVec::new() }
	}

	/// Returns block, starting from which the `next_set` may be enacted.
	pub fn may_enact_at(&self) -> BlockNumber {
		self.may_enact_at
	}

	/// Set block, starting from which the `next_set` may be enacted.
	pub fn set_may_enact_at(&mut self, may_enact_at: BlockNumber) {
		self.may_enact_at = may_enact_at;
	}

	/// Returns relayer entry from the next set.
	pub fn relayer(&self, relayer: &AccountId) -> Option<&LaneRegistration<AccountId>> {
		self.next_set.iter().find(|r| r.relayer() == relayer)
	}

	/// Returns relayers from the next set.
	pub fn relayers(&self) -> &[LaneRegistration<AccountId>] {
		self.next_set.as_slice()
	}

	/// Try insert relayer to the next set.
	///
	/// Returns `true` if relayer has been added to the set and false otherwise.
	pub fn try_insert(
		&mut self,
		relayer: AccountId,
		relayer_reward_per_message: RelayerRewardAtSource,
	) -> bool {
		// first, remove existing entry for the same relayer from the set
		self.try_remove(&relayer);
		// now try to insert new entry into the queue
		self.next_set
			.force_insert_keep_left(
				self.select_position_in_next_set(relayer_reward_per_message),
				LaneRegistration { relayer, relayer_reward_per_message },
			)
			.is_ok()
	}

	/// Try remove relayer from the next set.
	///
	/// Returns `true` if relayer has been removed from the set.
	pub fn try_remove(&mut self, relayer: &AccountId) -> Option<LaneRegistration<AccountId>> {
		self.next_set
			.iter()
			.enumerate()
			.find(|(_, r)| r.relayer == *relayer)
			.map(|(index, _)| index)
			.map(|index| self.next_set.remove(index))
	}

	/// Selects position to insert relayer, wanting to receive `reward` for every delivered
	/// message. If there are multiple relayers with that reward, relayers that are already
	/// in the set are prioritized above the new relayer.
	fn select_position_in_next_set(
		&self,
		relayer_reward_per_message: RelayerRewardAtSource,
	) -> usize {
		// we need to insert new entry **after** the last entry with the same `reward`. Otherwise it
		// may be used to push relayers our of the queue
		let mut initial_position = self
			.next_set
			.binary_search_by_key(&relayer_reward_per_message, |entry| {
				entry.relayer_reward_per_message
			})
			.unwrap_or_else(|position| position);
		while self
			.next_set
			.get(initial_position)
			.map(|entry| entry.relayer_reward_per_message == relayer_reward_per_message)
			.unwrap_or(false)
		{
			initial_position += 1;
		}
		initial_position
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use sp_runtime::traits::ConstU32;

	const MAX_ACTIVE_LANE_RELAYERS: u32 = 2;
	const MAX_NEXT_LANE_RELAYERS: u32 = 4;
	type TestActiveLaneRelayersSet =
		ActiveLaneRelayersSet<u64, u64, ConstU32<MAX_ACTIVE_LANE_RELAYERS>>;
	type TestNextLaneRelayersSet = NextLaneRelayersSet<u64, u64, ConstU32<MAX_NEXT_LANE_RELAYERS>>;

	fn mergeable_relayers(active_set: &TestActiveLaneRelayersSet) -> Vec<u64> {
		active_set
			.active_set
			.iter()
			.filter(|r| r.is_mergeable)
			.map(|r| *r.relayer())
			.collect::<Vec<_>>()
	}

	#[test]
	fn note_delivered_message_works() {
		let mut active_set: TestActiveLaneRelayersSet = ActiveLaneRelayersSet {
			enacted_at: 0,
			active_set: vec![
				ActiveLaneRegistration::new(100, 0),
				ActiveLaneRegistration::new(200, 0),
			]
			.try_into()
			.unwrap(),
		};

		// when registered relayer delivers first message
		assert!(active_set.note_delivered_message(&100));
		assert_eq!(mergeable_relayers(&active_set), vec![100],);

		// when registered relayer delivers second message
		assert!(!active_set.note_delivered_message(&100));
		assert_eq!(mergeable_relayers(&active_set), vec![100],);

		// when another registered relayer delivers a message
		assert!(active_set.note_delivered_message(&200));
		assert_eq!(mergeable_relayers(&active_set), vec![100, 200],);

		// when unregistered relayer delivers a message
		assert!(!active_set.note_delivered_message(&300));
		assert_eq!(mergeable_relayers(&active_set), vec![100, 200],);
	}

	#[test]
	fn active_set_activate_next_set_works() {
		let mut active_set: TestActiveLaneRelayersSet =
			ActiveLaneRelayersSet { enacted_at: 0, active_set: vec![].try_into().unwrap() };
		let mut next_set: TestNextLaneRelayersSet = NextLaneRelayersSet {
			may_enact_at: 100,
			next_set: vec![
				LaneRegistration::new(100, 10),
				LaneRegistration::new(200, 11),
				LaneRegistration::new(300, 12),
				LaneRegistration::new(400, 13),
			]
			.try_into()
			.unwrap(),
		};

		// when we can't yet activate next set, it returns false
		assert!(!active_set.activate_next_set(0, next_set.clone(), |_| true));

		// only two relayers are selected from the next set when the active set is empty
		assert!(active_set.activate_next_set(100, next_set.clone(), |_| true));
		assert_eq!(active_set.enacted_at, 100);
		assert_eq!(
			active_set.active_set,
			BoundedVec::<_, ConstU32<MAX_ACTIVE_LANE_RELAYERS>>::try_from(vec![
				ActiveLaneRegistration::new(100, 10),
				ActiveLaneRegistration::new(200, 11),
			])
			.unwrap(),
		);
		assert_eq!(mergeable_relayers(&active_set), Vec::<u64>::new(),);

		// spam relayers are occupying the whole next set and then they leave in favor of some
		// expensive relayers. At the same time, both relayers from the active set were delivering
		// messages => active set is not changed
		for r in active_set.active_set.iter_mut() {
			r.is_mergeable = true;
		}
		next_set.next_set = vec![
			LaneRegistration::new(300, 1000),
			LaneRegistration::new(400, 1100),
			LaneRegistration::new(500, 1200),
			LaneRegistration::new(600, 1300),
		]
		.try_into()
		.unwrap();
		assert!(active_set.activate_next_set(100, next_set.clone(), |_| true));
		assert_eq!(
			active_set.active_set,
			BoundedVec::<_, ConstU32<MAX_ACTIVE_LANE_RELAYERS>>::try_from(vec![
				ActiveLaneRegistration::new(100, 10),
				ActiveLaneRegistration::new(200, 11),
			])
			.unwrap(),
		);

		// better relayers appear in the next set
		// => even if active relayers were delivering messages, they lose their slots
		for r in active_set.active_set.iter_mut() {
			r.is_mergeable = true;
		}
		next_set.next_set = vec![
			LaneRegistration::new(700, 5),
			LaneRegistration::new(800, 5),
			LaneRegistration::new(100, 10),
			LaneRegistration::new(200, 11),
		]
		.try_into()
		.unwrap();
		assert!(active_set.activate_next_set(100, next_set.clone(), |_| true));
		assert_eq!(
			active_set.active_set,
			BoundedVec::<_, ConstU32<MAX_ACTIVE_LANE_RELAYERS>>::try_from(vec![
				ActiveLaneRegistration::new(700, 5),
				ActiveLaneRegistration::new(800, 5),
			])
			.unwrap(),
		);

		// one of active relayers deregisters => next epoch will start without it
		for r in active_set.active_set.iter_mut() {
			r.is_mergeable = true;
		}
		next_set.next_set = vec![
			LaneRegistration::new(700, 5),
			LaneRegistration::new(100, 10),
			LaneRegistration::new(200, 11),
			LaneRegistration::new(300, 1000),
		]
		.try_into()
		.unwrap();
		assert!(active_set.activate_next_set(100, next_set.clone(), |relayer| *relayer != 800));
		assert_eq!(
			active_set.active_set,
			BoundedVec::<_, ConstU32<MAX_ACTIVE_LANE_RELAYERS>>::try_from(vec![
				ActiveLaneRegistration::new(700, 5),
				ActiveLaneRegistration::new(100, 10),
			])
			.unwrap(),
		);

		// if relayer is in the next set already, we do not remerge it because we may rewrite its
		// updated bid
		for r in active_set.active_set.iter_mut() {
			r.is_mergeable = true;
		}
		next_set.next_set = vec![LaneRegistration::new(700, 100), LaneRegistration::new(100, 200)]
			.try_into()
			.unwrap();
		assert!(active_set.activate_next_set(100, next_set.clone(), |relayer| *relayer != 800));
		assert_eq!(
			active_set.active_set,
			BoundedVec::<_, ConstU32<MAX_ACTIVE_LANE_RELAYERS>>::try_from(vec![
				ActiveLaneRegistration::new(700, 100),
				ActiveLaneRegistration::new(100, 200),
			])
			.unwrap(),
		);

		// all relayers deregister themselves and no relayers have submitted any messages => new
		// active set will be empty
		next_set.next_set = vec![].try_into().unwrap();
		assert!(active_set.activate_next_set(100, next_set.clone(), |_| true));
		assert_eq!(
			active_set.active_set,
			BoundedVec::<_, ConstU32<MAX_ACTIVE_LANE_RELAYERS>>::try_from(vec![]).unwrap()
		);
	}

	#[test]
	fn next_set_try_insert_works() {
		let mut relayers: TestNextLaneRelayersSet =
			NextLaneRelayersSet { may_enact_at: 100, next_set: vec![].try_into().unwrap() };

		// first `MAX_NEXT_LANE_RELAYERS` are simply filling the set
		let max_next_lane_relayers: u64 = MAX_NEXT_LANE_RELAYERS as _;
		for i in 0..max_next_lane_relayers {
			assert!(relayers.try_insert(i, (max_next_lane_relayers - i) * 10));
		}
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 2, relayer_reward_per_message: 20 },
				LaneRegistration { relayer: 1, relayer_reward_per_message: 30 },
				LaneRegistration { relayer: 0, relayer_reward_per_message: 40 },
			],
		);

		// try to insert relayer who wants reward, that is larger than anyone in the set
		// => the set is not changed
		assert!(!relayers.try_insert(4, 50));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 2, relayer_reward_per_message: 20 },
				LaneRegistration { relayer: 1, relayer_reward_per_message: 30 },
				LaneRegistration { relayer: 0, relayer_reward_per_message: 40 },
			],
		);

		// replace worst relayer in the set
		assert!(relayers.try_insert(5, 35));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 2, relayer_reward_per_message: 20 },
				LaneRegistration { relayer: 1, relayer_reward_per_message: 30 },
				LaneRegistration { relayer: 5, relayer_reward_per_message: 35 },
			],
		);

		// insert best relayer to the set, pushing worst relayer out of set
		assert!(relayers.try_insert(6, 5));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 6, relayer_reward_per_message: 5 },
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 2, relayer_reward_per_message: 20 },
				LaneRegistration { relayer: 1, relayer_reward_per_message: 30 },
			],
		);

		// insert best relayer to the set, pushing worst relayer out of set
		assert!(relayers.try_insert(6, 5));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 6, relayer_reward_per_message: 5 },
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 2, relayer_reward_per_message: 20 },
				LaneRegistration { relayer: 1, relayer_reward_per_message: 30 },
			],
		);

		// insert relayer to the middle of the set, pushing worst relayer out of set
		assert!(relayers.try_insert(7, 15));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 6, relayer_reward_per_message: 5 },
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 7, relayer_reward_per_message: 15 },
				LaneRegistration { relayer: 2, relayer_reward_per_message: 20 },
			],
		);

		// insert couple of relayer that want the same reward as some relayer in the middle of the
		// queue => they are inserted **after** existing relayers
		assert!(relayers.try_insert(8, 10));
		assert!(relayers.try_insert(9, 10));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 6, relayer_reward_per_message: 5 },
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 8, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 9, relayer_reward_per_message: 10 },
			],
		);

		// insert next relayer, similar to previous => it isn't inserted
		assert!(!relayers.try_insert(10, 10));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 6, relayer_reward_per_message: 5 },
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 8, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 9, relayer_reward_per_message: 10 },
			],
		);

		// update expected reward of existing relayer => the set order is changed
		assert!(relayers.try_insert(8, 2));
		assert_eq!(
			relayers.next_set.as_slice(),
			&[
				LaneRegistration { relayer: 8, relayer_reward_per_message: 2 },
				LaneRegistration { relayer: 6, relayer_reward_per_message: 5 },
				LaneRegistration { relayer: 3, relayer_reward_per_message: 10 },
				LaneRegistration { relayer: 9, relayer_reward_per_message: 10 },
			],
		);
	}

	#[test]
	fn next_set_try_remove_works() {
		let mut relayers: TestNextLaneRelayersSet =
			NextLaneRelayersSet { may_enact_at: 100, next_set: vec![].try_into().unwrap() };

		assert!(relayers.try_insert(1, 0));
		assert!(relayers.try_remove(&1).is_some());
		assert!(relayers.try_remove(&1).is_none());
	}
}
