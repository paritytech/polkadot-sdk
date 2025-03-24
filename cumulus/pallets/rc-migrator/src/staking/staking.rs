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

//! Pallet staking migration.

use crate::*;
pub use frame_election_provider_support::PageIndex;
use pallet_staking::{
	slashing::{SlashingSpans, SpanIndex, SpanRecord},
	ActiveEraInfo, EraRewardPoints, Forcing, Nominations, RewardDestination, StakingLedger,
	ValidatorPrefs,
};
use sp_runtime::{Perbill, Percent};
use sp_staking::{EraIndex, ExposurePage, Page, PagedExposureMetadata, SessionIndex};

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
	Ledger(Option<AccountId>),
	Payee(Option<AccountId>),
	Validators(Option<AccountId>),
	Nominators(Option<AccountId>),
	VirtualStakers(Option<AccountId>),
	ErasStartSessionIndex(Option<EraIndex>),
	ErasStakersOverview(Option<(EraIndex, AccountId)>),
	ErasStakersPaged(Option<(EraIndex, AccountId, Page)>),
	ClaimedRewards(Option<(EraIndex, AccountId)>),
	ErasValidatorPrefs(Option<(EraIndex, AccountId)>),
	ErasValidatorReward(Option<EraIndex>),
	ErasRewardPoints(Option<EraIndex>),
	ErasTotalStake(Option<EraIndex>),
	BondedEras,
	ValidatorSlashInEra(Option<(EraIndex, AccountId)>),
	NominatorSlashInEra(Option<(EraIndex, AccountId)>),
	SlashingSpans(Option<AccountId>),
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
	pub current_era: Option<EraIndex>,
	pub active_era: Option<ActiveEraInfo>,
	pub force_era: Forcing,
	pub max_staked_rewards: Option<Percent>,
	pub slash_reward_fraction: Perbill,
	pub canceled_slash_payout: Balance,
	pub current_planned_session: SessionIndex,
	pub chill_threshold: Option<Percent>,
}

pub type StakingValuesOf<T> = StakingValues<<T as pallet_staking::Config>::CurrencyBalance>;

pub type BalanceOf<T> = <T as pallet_staking::Config>::CurrencyBalance;
pub type AccountIdOf<T> = <T as frame_system::Config>::AccountId;

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	RuntimeDebug,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
)]
#[scale_info(skip_type_params(T))]
pub enum StakingMessage<T: pallet_staking::Config> {
	Values(StakingValues<BalanceOf<T>>),
	Invulnerables(Vec<AccountIdOf<T>>),
	Bonded {
		stash: AccountIdOf<T>,
		controller: AccountIdOf<T>,
	},
	// Stupid staking pallet forces us to use `T` since its staking ledger requires that...
	Ledger {
		controller: AccountIdOf<T>,
		ledger: StakingLedger<T>,
	},
	Payee {
		stash: AccountIdOf<T>,
		payment: RewardDestination<AccountIdOf<T>>,
	},
	Validators {
		stash: AccountIdOf<T>,
		validators: ValidatorPrefs,
	},
	Nominators {
		stash: AccountIdOf<T>,
		nominations: Nominations<T>,
	},
	VirtualStakers(AccountIdOf<T>),
	ErasStartSessionIndex {
		era: EraIndex,
		session: SessionIndex,
	},
	ErasStakersOverview {
		era: EraIndex,
		validator: AccountIdOf<T>,
		exposure: PagedExposureMetadata<BalanceOf<T>>,
	},
	ErasStakersPaged {
		era: EraIndex,
		validator: AccountIdOf<T>,
		page: Page,
		exposure: ExposurePage<AccountIdOf<T>, BalanceOf<T>>,
	},
	ClaimedRewards {
		era: EraIndex,
		validator: AccountIdOf<T>,
		rewards: Vec<Page>,
	},
	ErasValidatorPrefs {
		era: EraIndex,
		validator: AccountIdOf<T>,
		prefs: ValidatorPrefs,
	},
	ErasValidatorReward {
		era: EraIndex,
		reward: BalanceOf<T>,
	},
	ErasRewardPoints {
		era: EraIndex,
		points: EraRewardPoints<AccountIdOf<T>>,
	},
	ErasTotalStake {
		era: EraIndex,
		total_stake: BalanceOf<T>,
	},
	BondedEras(Vec<(EraIndex, SessionIndex)>),
	ValidatorSlashInEra {
		era: EraIndex,
		validator: AccountIdOf<T>,
		slash: (Perbill, BalanceOf<T>),
	},
	NominatorSlashInEra {
		era: EraIndex,
		validator: AccountIdOf<T>,
		slash: BalanceOf<T>,
	},
	SlashingSpans {
		account: AccountIdOf<T>,
		spans: SlashingSpans,
	},
	SpanSlash {
		account: AccountIdOf<T>,
		span: SpanIndex,
		slash: SpanRecord<BalanceOf<T>>,
	},
}

pub type StakingMessageOf<T> = StakingMessage<T>;

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
			current_era: CurrentEra::<T>::take(),
			active_era: ActiveEra::<T>::take(),
			force_era: ForceEra::<T>::take(),
			max_staked_rewards: MaxStakedRewards::<T>::take(),
			slash_reward_fraction: SlashRewardFraction::<T>::take(),
			canceled_slash_payout: CanceledSlashPayout::<T>::take(),
			current_planned_session: CurrentPlannedSession::<T>::take(),
			chill_threshold: ChillThreshold::<T>::take(),
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
		CurrentEra::<T>::set(values.current_era);
		ActiveEra::<T>::set(values.active_era);
		ForceEra::<T>::put(values.force_era);
		MaxStakedRewards::<T>::set(values.max_staked_rewards);
		SlashRewardFraction::<T>::set(values.slash_reward_fraction);
		CanceledSlashPayout::<T>::set(values.canceled_slash_payout);
		CurrentPlannedSession::<T>::put(values.current_planned_session);
		ChillThreshold::<T>::set(values.chill_threshold);
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
					let values = Self::take_values();
					messages.push(StakingMessage::Values(values));
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
						None => StakingStage::Ledger(None),
					}
				},
				StakingStage::Ledger(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Ledger::<T>::iter_from_key(who)
					} else {
						pallet_staking::Ledger::<T>::iter()
					};

					match iter.next() {
						Some((controller, ledger)) => {
							pallet_staking::Ledger::<T>::remove(&controller);
							messages.push(StakingMessage::Ledger {
								controller: controller.clone(),
								ledger,
							});
							StakingStage::Ledger(Some(controller))
						},
						None => StakingStage::Payee(None),
					}
				},
				StakingStage::Payee(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Payee::<T>::iter_from_key(who)
					} else {
						pallet_staking::Payee::<T>::iter()
					};

					match iter.next() {
						Some((stash, payment)) => {
							pallet_staking::Payee::<T>::remove(&stash);
							messages.push(StakingMessage::Payee { stash: stash.clone(), payment });
							StakingStage::Payee(Some(stash))
						},
						None => StakingStage::Validators(None),
					}
				},
				StakingStage::Validators(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Validators::<T>::iter_from(
							pallet_staking::Validators::<T>::hashed_key_for(who),
						)
					} else {
						pallet_staking::Validators::<T>::iter()
					};

					match iter.next() {
						Some((stash, validators)) => {
							pallet_staking::Validators::<T>::remove(&stash);
							messages.push(StakingMessage::Validators {
								stash: stash.clone(),
								validators,
							});
							StakingStage::Validators(Some(stash))
						},
						None => StakingStage::Nominators(None),
					}
				},
				StakingStage::Nominators(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::Nominators::<T>::iter_from(
							pallet_staking::Nominators::<T>::hashed_key_for(who),
						)
					} else {
						pallet_staking::Nominators::<T>::iter()
					};

					match iter.next() {
						Some((stash, nominations)) => {
							pallet_staking::Nominators::<T>::remove(&stash);
							messages.push(StakingMessage::Nominators {
								stash: stash.clone(),
								nominations,
							});
							StakingStage::Nominators(Some(stash))
						},
						None => StakingStage::VirtualStakers(None),
					}
				},
				StakingStage::VirtualStakers(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::VirtualStakers::<T>::iter_from(
							// Counted maps dont have the convenience function here
							pallet_staking::VirtualStakers::<T>::hashed_key_for(who),
						)
					} else {
						pallet_staking::VirtualStakers::<T>::iter()
					};

					match iter.next() {
						Some((staker, ())) => {
							pallet_staking::VirtualStakers::<T>::remove(&staker);
							messages.push(StakingMessage::VirtualStakers(staker.clone()));
							StakingStage::VirtualStakers(Some(staker))
						},
						None => StakingStage::ErasStartSessionIndex(None),
					}
				},
				StakingStage::ErasStartSessionIndex(who) => {
					let mut iter = if let Some(who) = who {
						pallet_staking::ErasStartSessionIndex::<T>::iter_from_key(who)
					} else {
						pallet_staking::ErasStartSessionIndex::<T>::iter()
					};

					match iter.next() {
						Some((era, session)) => {
							pallet_staking::ErasStartSessionIndex::<T>::remove(&era);
							messages.push(StakingMessage::ErasStartSessionIndex { era, session });
							StakingStage::ErasStartSessionIndex(Some(era))
						},
						None => StakingStage::ErasStakersOverview(None),
					}
				},
				StakingStage::ErasStakersOverview(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ErasStakersOverview::<T>::iter_from(
							pallet_staking::ErasStakersOverview::<T>::hashed_key_for(
								progress.0, progress.1,
							),
						)
					} else {
						pallet_staking::ErasStakersOverview::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, exposure)) => {
							pallet_staking::ErasStakersOverview::<T>::remove(&era, &validator);
							messages.push(StakingMessage::ErasStakersOverview {
								era,
								validator: validator.clone(),
								exposure,
							});
							StakingStage::ErasStakersOverview(Some((era, validator)))
						},
						None => StakingStage::ErasStakersPaged(None),
					}
				},
				StakingStage::ErasStakersPaged(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ErasStakersPaged::<T>::iter_from(
							pallet_staking::ErasStakersPaged::<T>::hashed_key_for(progress),
						)
					} else {
						pallet_staking::ErasStakersPaged::<T>::iter()
					};

					match iter.next() {
						Some(((era, validator, page), exposure)) => {
							pallet_staking::ErasStakersPaged::<T>::remove((
								&era, &validator, &page,
							));
							messages.push(StakingMessage::ErasStakersPaged {
								era,
								validator: validator.clone(),
								page,
								exposure,
							});
							StakingStage::ErasStakersPaged(Some((era, validator, page)))
						},
						None => StakingStage::ClaimedRewards(None),
					}
				},
				StakingStage::ClaimedRewards(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ClaimedRewards::<T>::iter_from(
							pallet_staking::ClaimedRewards::<T>::hashed_key_for(
								progress.0, progress.1,
							),
						)
					} else {
						pallet_staking::ClaimedRewards::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, rewards)) => {
							pallet_staking::ClaimedRewards::<T>::remove(&era, &validator);
							messages.push(StakingMessage::ClaimedRewards {
								era,
								validator: validator.clone(),
								rewards,
							});
							StakingStage::ClaimedRewards(Some((era, validator)))
						},
						None => StakingStage::ErasValidatorPrefs(None),
					}
				},
				StakingStage::ErasValidatorPrefs(progress) => {
					let mut iter = if let Some(progress) = progress {
						pallet_staking::ErasValidatorPrefs::<T>::iter_from(
							pallet_staking::ErasValidatorPrefs::<T>::hashed_key_for(
								progress.0, progress.1,
							),
						)
					} else {
						pallet_staking::ErasValidatorPrefs::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, prefs)) => {
							pallet_staking::ErasValidatorPrefs::<T>::remove(&era, &validator);
							messages.push(StakingMessage::ErasValidatorPrefs {
								era,
								validator: validator.clone(),
								prefs,
							});
							StakingStage::ErasValidatorPrefs(Some((era, validator)))
						},
						None => StakingStage::ErasValidatorReward(None),
					}
				},
				StakingStage::ErasValidatorReward(era) => {
					let mut iter = if let Some(era) = era {
						pallet_staking::ErasValidatorReward::<T>::iter_from_key(era)
					} else {
						pallet_staking::ErasValidatorReward::<T>::iter()
					};

					match iter.next() {
						Some((era, reward)) => {
							pallet_staking::ErasValidatorReward::<T>::remove(&era);
							messages.push(StakingMessage::ErasValidatorReward { era, reward });
							StakingStage::ErasValidatorReward(Some(era))
						},
						None => StakingStage::ErasRewardPoints(None),
					}
				},
				StakingStage::ErasRewardPoints(era) => {
					let mut iter = if let Some(era) = era {
						pallet_staking::ErasRewardPoints::<T>::iter_from_key(era)
					} else {
						pallet_staking::ErasRewardPoints::<T>::iter()
					};

					match iter.next() {
						Some((era, points)) => {
							pallet_staking::ErasRewardPoints::<T>::remove(&era);
							messages.push(StakingMessage::ErasRewardPoints { era, points });
							StakingStage::ErasRewardPoints(Some(era))
						},
						None => StakingStage::ErasTotalStake(None),
					}
				},
				StakingStage::ErasTotalStake(era) => {
					let mut iter = if let Some(era) = era {
						pallet_staking::ErasTotalStake::<T>::iter_from_key(era)
					} else {
						pallet_staking::ErasTotalStake::<T>::iter()
					};

					match iter.next() {
						Some((era, total_stake)) => {
							pallet_staking::ErasTotalStake::<T>::remove(&era);
							messages.push(StakingMessage::ErasTotalStake { era, total_stake });
							StakingStage::ErasTotalStake(Some(era))
						},
						None => StakingStage::BondedEras,
					}
				},
				StakingStage::BondedEras => {
					let bonded_eras = pallet_staking::BondedEras::<T>::take();
					messages.push(StakingMessage::BondedEras(bonded_eras));
					StakingStage::ValidatorSlashInEra(None)
				},
				StakingStage::ValidatorSlashInEra(next) => {
					let mut iter = if let Some(next) = next {
						pallet_staking::ValidatorSlashInEra::<T>::iter_from(
							pallet_staking::ValidatorSlashInEra::<T>::hashed_key_for(
								next.0, next.1,
							),
						)
					} else {
						pallet_staking::ValidatorSlashInEra::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, slash)) => {
							pallet_staking::ValidatorSlashInEra::<T>::remove(&era, &validator);
							messages.push(StakingMessage::ValidatorSlashInEra {
								era,
								validator: validator.clone(),
								slash,
							});
							StakingStage::ValidatorSlashInEra(Some((era, validator)))
						},
						None => StakingStage::NominatorSlashInEra(None),
					}
				},
				StakingStage::NominatorSlashInEra(next) => {
					let mut iter = if let Some(next) = next {
						pallet_staking::NominatorSlashInEra::<T>::iter_from(
							pallet_staking::NominatorSlashInEra::<T>::hashed_key_for(
								next.0, next.1,
							),
						)
					} else {
						pallet_staking::NominatorSlashInEra::<T>::iter()
					};

					match iter.next() {
						Some((era, validator, slash)) => {
							pallet_staking::NominatorSlashInEra::<T>::remove(&era, &validator);
							messages.push(StakingMessage::NominatorSlashInEra {
								era,
								validator: validator.clone(),
								slash,
							});
							StakingStage::NominatorSlashInEra(Some((era, validator)))
						},
						None => StakingStage::SlashingSpans(None),
					}
				},
				StakingStage::SlashingSpans(account) => {
					let mut iter = if let Some(account) = account {
						pallet_staking::SlashingSpans::<T>::iter_from_key(account)
					} else {
						pallet_staking::SlashingSpans::<T>::iter()
					};

					match iter.next() {
						Some((account, spans)) => {
							pallet_staking::SlashingSpans::<T>::remove(&account);
							messages.push(StakingMessage::SlashingSpans {
								account: account.clone(),
								spans,
							});
							StakingStage::SlashingSpans(Some(account))
						},
						None => StakingStage::Finished,
					}
				},
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
