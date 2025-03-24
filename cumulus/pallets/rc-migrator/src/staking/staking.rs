// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Election Provider Multi-Block migration.

use crate::*;
pub use frame_election_provider_support::PageIndex;
use pallet_staking::Forcing;
use sp_core::H256;
use sp_runtime::{Perbill, Percent};
use sp_staking::SessionIndex;

pub struct StakingMigrator<T> {
	_phantom: PhantomData<T>,
}

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	Clone,
	Default,
	PartialEq,
	Eq,
	RuntimeDebug,
	TypeInfo,
	MaxEncodedLen,
)]
pub enum StakingStage<AccountId> {
	#[default]
	Values,
	Invulnerables,
	Bonded(Option<AccountId>),
	Finished,
}

pub type StakingStageOf<T> = StakingStage<<T as frame_system::Config>::AccountId>;

#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, RuntimeDebug, Clone, PartialEq, Eq)]
pub struct StakingValues<Balance> {
	pub validator_count: u32,
	pub min_validator_count: u32,
	pub min_nominator_bond: Balance,
	pub min_validator_bond: Balance,
	pub min_active_stake: Balance,
	pub min_commission: Perbill,
	pub max_validators_count: Option<u32>,
	pub max_nominators_count: Option<u32>,
	pub force_era: Forcing,
	pub max_staked_rewards: Option<Percent>,
	pub slash_reward_fraction: Perbill,
	pub canceled_slash_payout: Balance,
	pub current_planned_session: SessionIndex,
	pub chill_threshold: Option<Percent>,
	pub next_election_page: Option<PageIndex>,
}

pub type StakingValuesOf<T> = StakingValues<<T as pallet_staking::Config>::CurrencyBalance>;

#[derive(Encode, Decode, DecodeWithMemTracking, TypeInfo, RuntimeDebug, Clone, PartialEq, Eq)]
pub enum StakingMessage<AccountId, Balance> {
	Values(StakingValues<Balance>),
	Invulnerables(Vec<AccountId>),
	Bonded { stash: AccountId, controller: AccountId },
}

pub type StakingMessageOf<T> = StakingMessage<
	<T as frame_system::Config>::AccountId,
	<T as pallet_staking::Config>::CurrencyBalance,
>;

impl<T: Config> StakingMigrator<T> {
	pub fn take_values() -> StakingValuesOf<T> {
		use pallet_staking::*;

		StakingValues {
			validator_count: ValidatorCount::<T>::take(),
			min_validator_count: MinimumValidatorCount::<T>::take(),
			min_nominator_bond: MinNominatorBond::<T>::take(),
			min_validator_bond: MinValidatorBond::<T>::take(),
			min_active_stake: MinimumActiveStake::<T>::take(),
			min_commission: MinCommission::<T>::take(),
			max_validators_count: MaxValidatorsCount::<T>::take(),
			max_nominators_count: MaxNominatorsCount::<T>::take(),
			force_era: ForceEra::<T>::take(),
			max_staked_rewards: MaxStakedRewards::<T>::take(),
			slash_reward_fraction: SlashRewardFraction::<T>::take(),
			canceled_slash_payout: CanceledSlashPayout::<T>::take(),
			current_planned_session: CurrentPlannedSession::<T>::take(),
			chill_threshold: ChillThreshold::<T>::take(),
			next_election_page: NextElectionPage::<T>::take(),
		}
	}

	pub fn put_values(values: StakingValuesOf<T>) {
		use pallet_staking::*;

		ValidatorCount::<T>::put(&values.validator_count);
		MinimumValidatorCount::<T>::put(&values.min_validator_count);
		MinNominatorBond::<T>::put(&values.min_nominator_bond);
		MinValidatorBond::<T>::put(&values.min_validator_bond);
		MinimumActiveStake::<T>::put(&values.min_active_stake);
		MinCommission::<T>::put(&values.min_commission);
		MaxValidatorsCount::<T>::set(values.max_validators_count);
		MaxNominatorsCount::<T>::set(values.max_nominators_count);
		ForceEra::<T>::put(values.force_era);
		MaxStakedRewards::<T>::set(values.max_staked_rewards);
		SlashRewardFraction::<T>::set(values.slash_reward_fraction);
		CanceledSlashPayout::<T>::set(values.canceled_slash_payout);
		CurrentPlannedSession::<T>::put(values.current_planned_session);
		ChillThreshold::<T>::set(values.chill_threshold);
		NextElectionPage::<T>::set(values.next_election_page);
	}
}

impl<T: Config> PalletMigration for StakingMigrator<T> {
	type Key = StakingStageOf<T>;
	type Error = Error<T>;

	fn migrate_many(
		current_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key.unwrap_or_default();
		let mut messages = Vec::new();

		loop {
			if weight_counter
				.try_consume(<T as frame_system::Config>::DbWeight::get().reads_writes(1, 1))
				.is_err()
			{
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}

			if messages.len() > 10_000 {
				log::warn!("Weight allowed very big batch, stopping");
				break;
			}

			inner_key = match inner_key {
				StakingStage::Values => {
					let Values = Self::take_values();
					messages.push(StakingMessage::Values(Values));
					StakingStage::Invulnerables
				},
				StakingStage::Invulnerables => {
					let invulnerables = pallet_staking::Invulnerables::<T>::take();
					messages.push(StakingMessage::Invulnerables(invulnerables.into_inner()));
					StakingStage::Bonded(None)
				},
				StakingStage::Bonded(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Bonded::<T>::iter_from_key(who)
					} else {
						pallet_staking::Bonded::<T>::iter()
					};

					match iter.next() {
						Some((stash, controller)) => {
							pallet_staking::Bonded::<T>::remove(&stash);
							messages
								.push(StakingMessage::Bonded { stash: stash.clone(), controller });
							StakingStage::Bonded(Some(stash))
						},
						None => StakingStage::Finished,
					}
				},

				_ => todo!(),
				StakingStage::Finished => {
					break;
				},
			};
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm(messages, |messages| {
				types::AhMigratorCall::<T>::ReceiveStakingMessages { messages }
			})?;
		}

		if inner_key == StakingStage::Finished {
			Ok(None)
		} else {
			Ok(Some(inner_key))
		}
	}
}
