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

//! The messages that we use to send the staking data over from RC to AH.

extern crate alloc;

use crate::{
	staking::{AccountIdOf, BalanceOf, IntoAh, StakingMigrator},
	*,
};
use alloc::collections::BTreeMap;
use codec::{EncodeLike, HasCompact};
use core::fmt::Debug;
pub use frame_election_provider_support::PageIndex;
use frame_support::traits::DefensiveTruncateInto;
use pallet_staking::{
	slashing::{SlashingSpans, SpanIndex, SpanRecord},
	ActiveEraInfo, EraRewardPoints, Forcing, Nominations, RewardDestination, StakingLedger,
	ValidatorPrefs,
};
use sp_runtime::{Perbill, Percent};
use sp_staking::{EraIndex, ExposurePage, Page, PagedExposureMetadata, SessionIndex};

#[derive(
	Encode,
	Decode,
	DecodeWithMemTracking,
	TypeInfo,
	RuntimeDebugNoBound,
	CloneNoBound,
	PartialEqNoBound,
	EqNoBound,
)]
#[scale_info(skip_type_params(T))]
pub enum RcStakingMessage<
	AccountId,
	Balance,
	StakingLedger,
	Nominations,
	EraRewardPoints,
	RewardDestination,
	ValidatorPrefs,
	UnappliedSlash,
>
// We do not want to pull in the Config trait; hence this
where
	AccountId: Ord + Debug + Clone,
	Balance: HasCompact + MaxEncodedLen + Debug + PartialEq + Clone,
	StakingLedger: Debug + PartialEq + Clone,
	Nominations: Debug + PartialEq + Clone,
	EraRewardPoints: Debug + PartialEq + Clone,
	RewardDestination: Debug + PartialEq + Clone,
	ValidatorPrefs: Debug + PartialEq + Clone,
	UnappliedSlash: Debug + PartialEq + Clone,
{
	Values(StakingValues<Balance>),
	Invulnerables(Vec<AccountId>),
	Bonded {
		stash: AccountId,
		controller: AccountId,
	},
	// Stupid staking pallet forces us to use `T` since its staking ledger requires that...
	Ledger {
		controller: AccountId,
		ledger: StakingLedger,
	},
	Payee {
		stash: AccountId,
		payment: RewardDestination,
	},
	Validators {
		stash: AccountId,
		validators: ValidatorPrefs,
	},
	Nominators {
		stash: AccountId,
		nominations: Nominations,
	},
	VirtualStakers(AccountId),
	ErasStakersOverview {
		era: EraIndex,
		validator: AccountId,
		exposure: PagedExposureMetadata<Balance>,
	},
	ErasStakersPaged {
		era: EraIndex,
		validator: AccountId,
		page: Page,
		exposure: ExposurePage<AccountId, Balance>,
	},
	ClaimedRewards {
		era: EraIndex,
		validator: AccountId,
		rewards: Vec<Page>,
	},
	ErasValidatorPrefs {
		era: EraIndex,
		validator: AccountId,
		prefs: ValidatorPrefs,
	},
	ErasValidatorReward {
		era: EraIndex,
		reward: Balance,
	},
	ErasRewardPoints {
		era: EraIndex,
		points: EraRewardPoints,
	},
	ErasTotalStake {
		era: EraIndex,
		total_stake: Balance,
	},
	UnappliedSlashes {
		era: EraIndex,
		slash: UnappliedSlash,
	},
	BondedEras(Vec<(EraIndex, SessionIndex)>),
	ValidatorSlashInEra {
		era: EraIndex,
		validator: AccountId,
		slash: (Perbill, Balance),
	},
	NominatorSlashInEra {
		era: EraIndex,
		validator: AccountId,
		slash: Balance,
	},
}

/// Untranslated message for the staking migration.
pub type RcStakingMessageOf<T> = RcStakingMessage<
	<T as frame_system::Config>::AccountId,
	<T as pallet_staking::Config>::CurrencyBalance,
	pallet_staking::StakingLedger<T>,
	pallet_staking::Nominations<T>,
	pallet_staking::EraRewardPoints<<T as frame_system::Config>::AccountId>,
	pallet_staking::RewardDestination<<T as frame_system::Config>::AccountId>, /* encodes the
	                                                                            * same as AH */
	pallet_staking::ValidatorPrefs, // encodes the same as AH
	pallet_staking::UnappliedSlash<
		<T as frame_system::Config>::AccountId,
		<T as pallet_staking::Config>::CurrencyBalance,
	>, // encodes the same as AH
>;

/// Translated staking message that the Asset Hub can understand.
///
/// This will normally have been created by using `RcStakingMessage::convert`.
pub type AhEquivalentStakingMessageOf<T> = RcStakingMessage<
	<T as frame_system::Config>::AccountId,
	<T as pallet_staking_async::Config>::CurrencyBalance,
	pallet_staking_async::ledger::StakingLedger2<
		<T as frame_system::Config>::AccountId,
		pallet_staking_async::BalanceOf<T>,
		ConstU32<32>, // <T as pallet_staking_async::Config>::MaxUnlockingChunks,
	>,
	pallet_staking_async::Nominations<
		<T as frame_system::Config>::AccountId,
		ConstU32<16>, //pallet_staking_async::MaxNominationsOf<T>,
	>,
	pallet_staking_async::EraRewardPoints<
		<T as frame_system::Config>::AccountId,
		ConstU32<1000>, // <T as pallet_staking_async::Config>::MaxValidatorSet,
	>,
	pallet_staking_async::RewardDestination<<T as frame_system::Config>::AccountId>,
	pallet_staking_async::ValidatorPrefs,
	pallet_staking_async::UnappliedSlash<
		<T as frame_system::Config>::AccountId,
		<T as pallet_staking_async::Config>::CurrencyBalance,
		ConstU32<1000>, // <T as pallet_staking_async::Config>::MaxValidatorSet,
	>,
>;

// translated version of `AhEquivalentStakingMessageOf`
pub type RcEquivalentStakingMessageOf<T> = RcStakingMessage<
	<T as frame_system::Config>::AccountId,
	<T as pallet_staking::Config>::CurrencyBalance,
	pallet_staking_async::ledger::StakingLedger2<
		<T as frame_system::Config>::AccountId,
		pallet_staking::BalanceOf<T>,
		ConstU32<32>, // <T as pallet_staking_async::Config>::MaxUnlockingChunks,
	>,
	pallet_staking_async::Nominations<
		<T as frame_system::Config>::AccountId,
		ConstU32<16>, //pallet_staking_async::MaxNominationsOf<T>,
	>,
	pallet_staking_async::EraRewardPoints<
		<T as frame_system::Config>::AccountId,
		ConstU32<1000>, // <T as pallet_staking_async::Config>::MaxValidatorSet,
	>,
	pallet_staking_async::RewardDestination<<T as frame_system::Config>::AccountId>,
	pallet_staking_async::ValidatorPrefs,
	pallet_staking_async::UnappliedSlash<
		<T as frame_system::Config>::AccountId,
		<T as pallet_staking::Config>::CurrencyBalance,
		ConstU32<1000>, // <T as pallet_staking_async::Config>::MaxValidatorSet,
	>,
>;

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

pub type RcStakingValuesOf<T> = StakingValues<<T as pallet_staking::Config>::CurrencyBalance>;
pub type AhStakingValuesOf<T> = StakingValues<<T as pallet_staking_async::Config>::CurrencyBalance>;

impl<Rc: pallet_staking::Config>
	IntoAh<
		pallet_staking::UnlockChunk<BalanceOf<Rc>>,
		pallet_staking_async::UnlockChunk<BalanceOf<Rc>>,
	> for MessageTranslator<Rc>
{
	fn intoAh(
		chunk: pallet_staking::UnlockChunk<BalanceOf<Rc>>,
	) -> pallet_staking_async::UnlockChunk<BalanceOf<Rc>> {
		pallet_staking_async::UnlockChunk { value: chunk.value, era: chunk.era }
	}
}

impl<Rc>
	IntoAh<
		pallet_staking::StakingLedger<Rc>,
		pallet_staking_async::ledger::StakingLedger2<
			AccountIdOf<Rc>,
			BalanceOf<Rc>,
			ConstU32<32>, // <T as pallet_staking_async::Config>::MaxUnlockingChunks,
		>,
	> for MessageTranslator<Rc>
where
	Rc: pallet_staking::Config,
{
	fn intoAh(
		ledger: pallet_staking::StakingLedger<Rc>,
	) -> pallet_staking_async::ledger::StakingLedger2<
		AccountIdOf<Rc>,
		BalanceOf<Rc>,
		ConstU32<32>, // <T as pallet_staking_async::Config>::MaxUnlockingChunks,
	> {
		pallet_staking_async::ledger::StakingLedger2 {
			stash: ledger.stash,
			total: ledger.total,
			active: ledger.active,
			unlocking: ledger
				.unlocking
				.into_iter()
				.map(MessageTranslator::<Rc>::intoAh)
				.collect::<Vec<_>>()
				.defensive_truncate_into(),
			// legacy_claimed_rewards not migrated
			controller: ledger.controller,
		}
	}
}

// NominationsQuota is an associated trait - not a type, therefore more mental gymnastics are needed
impl<Rc: pallet_staking::Config>
	IntoAh<
		pallet_staking::Nominations<Rc>,
		pallet_staking_async::Nominations<
			AccountIdOf<Rc>,
			ConstU32<16>, //pallet_staking_async::MaxNominationsOf<T>,
		>,
	> for MessageTranslator<Rc>
{
	fn intoAh(
		nominations: pallet_staking::Nominations<Rc>,
	) -> pallet_staking_async::Nominations<
		AccountIdOf<Rc>,
		ConstU32<16>, //pallet_staking_async::MaxNominationsOf<T>,
	> {
		let targets = nominations.targets.into_inner().try_into().defensive().unwrap_or_default(); // FIXME

		pallet_staking_async::Nominations {
			targets,
			submitted_in: nominations.submitted_in,
			suppressed: nominations.suppressed,
		}
	}
}

impl<Rc: pallet_staking::Config>
	IntoAh<
		pallet_staking::EraRewardPoints<AccountIdOf<Rc>>,
		pallet_staking_async::EraRewardPoints<
			<Rc as frame_system::Config>::AccountId,
			ConstU32<1000>, // <T as pallet_staking_async::Config>::MaxValidatorSet,
		>,
	> for MessageTranslator<Rc>
{
	fn intoAh(
		points: pallet_staking::EraRewardPoints<AccountIdOf<Rc>>,
	) -> pallet_staking_async::EraRewardPoints<
		AccountIdOf<Rc>,
		ConstU32<1000>, // <T as pallet_staking_async::Config>::MaxValidatorSet,
	> {
		let individual = points.individual.try_into().defensive().unwrap_or_default(); // FIXME
		pallet_staking_async::EraRewardPoints { total: points.total, individual }
	}
}

impl<Rc: pallet_staking::Config>
	IntoAh<
		pallet_staking::RewardDestination<AccountIdOf<Rc>>,
		pallet_staking_async::RewardDestination<AccountIdOf<Rc>>,
	> for MessageTranslator<Rc>
{
	fn intoAh(
		destination: pallet_staking::RewardDestination<AccountIdOf<Rc>>,
	) -> pallet_staking_async::RewardDestination<AccountIdOf<Rc>> {
		match destination {
			pallet_staking::RewardDestination::Staked =>
				pallet_staking_async::RewardDestination::Staked,
			pallet_staking::RewardDestination::Stash =>
				pallet_staking_async::RewardDestination::Stash,
			pallet_staking::RewardDestination::Controller =>
				pallet_staking_async::RewardDestination::Controller,
			pallet_staking::RewardDestination::Account(account) =>
				pallet_staking_async::RewardDestination::Account(account),
			pallet_staking::RewardDestination::None =>
				pallet_staking_async::RewardDestination::None,
		}
	}
}

impl<Rc: pallet_staking::Config>
	IntoAh<pallet_staking::ValidatorPrefs, pallet_staking_async::ValidatorPrefs>
	for MessageTranslator<Rc>
{
	fn intoAh(prefs: pallet_staking::ValidatorPrefs) -> pallet_staking_async::ValidatorPrefs {
		pallet_staking_async::ValidatorPrefs {
			commission: prefs.commission,
			blocked: prefs.blocked,
		}
	}
}

impl IntoAh<pallet_staking::Forcing, pallet_staking_async::Forcing> for MessageTranslator<()> {
	fn intoAh(forcing: pallet_staking::Forcing) -> pallet_staking_async::Forcing {
		match forcing {
			pallet_staking::Forcing::NotForcing => pallet_staking_async::Forcing::NotForcing,
			pallet_staking::Forcing::ForceNew => pallet_staking_async::Forcing::ForceNew,
			pallet_staking::Forcing::ForceNone => pallet_staking_async::Forcing::ForceNone,
			pallet_staking::Forcing::ForceAlways => pallet_staking_async::Forcing::ForceAlways,
		}
	}
}

impl IntoAh<pallet_staking::ActiveEraInfo, pallet_staking_async::ActiveEraInfo>
	for MessageTranslator<()>
{
	fn intoAh(active_era: pallet_staking::ActiveEraInfo) -> pallet_staking_async::ActiveEraInfo {
		pallet_staking_async::ActiveEraInfo { index: active_era.index, start: active_era.start }
	}
}

impl<Rc> IntoAh<RcStakingMessageOf<Rc>, RcEquivalentStakingMessageOf<Rc>> for MessageTranslator<Rc>
where
	Rc: pallet_staking::Config,
{
	fn intoAh(message: RcStakingMessageOf<Rc>) -> RcEquivalentStakingMessageOf<Rc> {
		use RcStakingMessage::*;
		match message {
			// It looks like nothing happens here, but it does. We swap the omitted generics of
			// `RcStakingMessage` from `T` to `Ah`:
			Values(values) => Values(values),
			Invulnerables(invulnerables) => Invulnerables(invulnerables),
			Bonded { stash, controller } => Bonded { stash, controller },
			Ledger { controller, ledger } =>
				Ledger { controller, ledger: MessageTranslator::<Rc>::intoAh(ledger) },
			Payee { stash, payment } =>
				Payee { stash, payment: MessageTranslator::<Rc>::intoAh(payment) },
			Validators { stash, validators } =>
				Validators { stash, validators: MessageTranslator::<Rc>::intoAh(validators) },
			Nominators { stash, nominations } =>
				Nominators { stash, nominations: MessageTranslator::<Rc>::intoAh(nominations) },
			VirtualStakers(staker) => VirtualStakers(staker),
			ErasStakersOverview { era, validator, exposure } =>
				ErasStakersOverview { era, validator, exposure },
			ErasStakersPaged { era, validator, page, exposure } =>
				ErasStakersPaged { era, validator, page, exposure },
			ClaimedRewards { era, validator, rewards } =>
				ClaimedRewards { era, validator, rewards },
			ErasValidatorPrefs { era, validator, prefs } =>
				ErasValidatorPrefs { era, validator, prefs: MessageTranslator::<Rc>::intoAh(prefs) },
			ErasValidatorReward { era, reward } => ErasValidatorReward { era, reward },
			ErasRewardPoints { era, points } =>
				ErasRewardPoints { era, points: MessageTranslator::<Rc>::intoAh(points) },
			ErasTotalStake { era, total_stake } => ErasTotalStake { era, total_stake },
			UnappliedSlashes { era, slash } => {
				// Translate according to https://github.com/paritytech/polkadot-sdk/blob/43ea306f6307dff908551cb91099ef6268502ee0/substrate/frame/staking/src/migrations.rs#L94-L108
				UnappliedSlashes {
					era,
					slash: pallet_staking_async::UnappliedSlash {
						validator: slash.validator,
						own: slash.own,
						// TODO defensive truncate
						others: WeakBoundedVec::force_from(slash.others, None),
						payout: slash.payout,
						reporter: slash.reporters.first().cloned(),
					},
				}
			},
			BondedEras(eras) => BondedEras(eras),
			ValidatorSlashInEra { era, validator, slash } =>
				ValidatorSlashInEra { era, validator, slash },
			NominatorSlashInEra { era, validator, slash } =>
				NominatorSlashInEra { era, validator, slash },
		}
	}
}

impl<T: pallet_staking::Config> StakingMigrator<T> {
	pub fn take_values() -> RcStakingValuesOf<T> {
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
}

impl<T: pallet_staking_async::Config> StakingMigrator<T> {
	pub fn put_values(values: AhStakingValuesOf<T>) {
		use pallet_staking_async::*;
		use IntoAh;

		ValidatorCount::<T>::put(&values.validator_count);
		// MinimumValidatorCount is not migrated
		MinNominatorBond::<T>::put(&values.min_nominator_bond);
		MinValidatorBond::<T>::put(&values.min_validator_bond);
		MinimumActiveStake::<T>::put(&values.min_active_stake);
		MinCommission::<T>::put(&values.min_commission);
		MaxValidatorsCount::<T>::set(values.max_validators_count);
		MaxNominatorsCount::<T>::set(values.max_nominators_count);
		let active_era = values.active_era.map(|v| MessageTranslator::<()>::intoAh(v));

		ActiveEra::<T>::set(active_era.clone());
		CurrentEra::<T>::set(active_era.map(|a| a.index));
		ForceEra::<T>::put(MessageTranslator::<()>::intoAh(values.force_era));
		MaxStakedRewards::<T>::set(values.max_staked_rewards);
		SlashRewardFraction::<T>::set(values.slash_reward_fraction);
		CanceledSlashPayout::<T>::set(values.canceled_slash_payout);
		// CurrentPlannedSession is not migrated
		ChillThreshold::<T>::set(values.chill_threshold);
	}
}

pub struct MessageTranslator<Rc>(core::marker::PhantomData<Rc>);
