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
	SpanRecord,
	EraRewardPoints,
	RewardDestination,
	ValidatorPrefs,
	UnappliedSlash,
	SlashingSpans,
>
// We do not want to pull in the Config trait; hence this
where
	AccountId: Ord + Debug + Clone,
	Balance: HasCompact + MaxEncodedLen + Debug + PartialEq + Clone,
	StakingLedger: Debug + PartialEq + Clone,
	Nominations: Debug + PartialEq + Clone,
	SpanRecord: Debug + PartialEq + Clone,
	EraRewardPoints: Debug + PartialEq + Clone,
	RewardDestination: Debug + PartialEq + Clone,
	ValidatorPrefs: Debug + PartialEq + Clone,
	UnappliedSlash: Debug + PartialEq + Clone,
	SlashingSpans: Debug + PartialEq + Clone,
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
	SlashingSpans {
		account: AccountId,
		spans: SlashingSpans,
	},
	SpanSlash {
		account: AccountId,
		span: SpanIndex,
		slash: SpanRecord,
	},
}

/// Untranslated message for the staking migration.
pub type RcStakingMessageOf<T> = RcStakingMessage<
	<T as frame_system::Config>::AccountId,
	<T as pallet_staking::Config>::CurrencyBalance,
	pallet_staking::StakingLedger<T>,
	pallet_staking::Nominations<T>,
	pallet_staking::slashing::SpanRecord<<T as pallet_staking::Config>::CurrencyBalance>,
	pallet_staking::EraRewardPoints<<T as frame_system::Config>::AccountId>,
	pallet_staking::RewardDestination<<T as frame_system::Config>::AccountId>,
	pallet_staking::ValidatorPrefs,
	pallet_staking::UnappliedSlash<
		<T as frame_system::Config>::AccountId,
		<T as pallet_staking::Config>::CurrencyBalance,
	>,
	pallet_staking::slashing::SlashingSpans,
>;

/// Translated staking message that the Asset Hub can understand.
///
/// This will normally have been created by using `RcStakingMessage::convert`.
pub type AhEquivalentStakingMessageOf<T> = RcStakingMessage<
	<T as frame_system::Config>::AccountId,
	<T as pallet_staking_async::Config>::CurrencyBalance,
	pallet_staking_async::StakingLedger<T>,
	pallet_staking_async::Nominations<T>,
	pallet_staking_async::slashing::SpanRecord<
		<T as pallet_staking_async::Config>::CurrencyBalance,
	>,
	pallet_staking_async::EraRewardPoints<T>,
	pallet_staking_async::RewardDestination<<T as frame_system::Config>::AccountId>,
	pallet_staking_async::ValidatorPrefs,
	pallet_staking_async::UnappliedSlash<T>,
	pallet_staking_async::slashing::SlashingSpans,
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

impl<T, Ah> IntoAh<pallet_staking::StakingLedger<T>, pallet_staking_async::StakingLedger<Ah>>
	for pallet_staking::StakingLedger<T>
where
	T: pallet_staking::Config,
	Ah: pallet_staking_async::Config<AccountId = AccountIdOf<T>, CurrencyBalance = BalanceOf<T>>,
{
	fn intoAh(ledger: pallet_staking::StakingLedger<T>) -> pallet_staking_async::StakingLedger<Ah> {
		pallet_staking_async::StakingLedger {
			stash: ledger.stash,
			total: ledger.total,
			active: ledger.active,
			unlocking: ledger
				.unlocking
				.into_iter()
				.map(pallet_staking::UnlockChunk::intoAh)
				.collect::<Vec<_>>()
				.defensive_truncate_into(),
			// legacy_claimed_rewards not migrated
			controller: ledger.controller,
		}
	}
}

// NominationsQuota is an associated trait - not a type, therefore more mental gymnastics are needed
impl<T, Ah, SNomQuota, SSNomQuota>
	IntoAh<pallet_staking::Nominations<T>, pallet_staking_async::Nominations<Ah>>
	for pallet_staking::Nominations<T>
where
	T: pallet_staking::Config<NominationsQuota = SNomQuota>,
	Ah: pallet_staking_async::Config<
		AccountId = AccountIdOf<T>,
		CurrencyBalance = BalanceOf<T>,
		NominationsQuota = SSNomQuota,
	>,
	SNomQuota: pallet_staking::NominationsQuota<BalanceOf<T>>,
	SSNomQuota: pallet_staking_async::NominationsQuota<
		pallet_staking_async::BalanceOf<Ah>,
		MaxNominations = SNomQuota::MaxNominations,
	>,
{
	fn intoAh(
		nominations: pallet_staking::Nominations<T>,
	) -> pallet_staking_async::Nominations<Ah> {
		pallet_staking_async::Nominations {
			targets: nominations.targets,
			submitted_in: nominations.submitted_in,
			suppressed: nominations.suppressed,
		}
	}
}

impl<Balance>
	IntoAh<
		pallet_staking::slashing::SpanRecord<Balance>,
		pallet_staking_async::slashing::SpanRecord<Balance>,
	> for pallet_staking::slashing::SpanRecord<Balance>
{
	fn intoAh(
		record: pallet_staking::slashing::SpanRecord<Balance>,
	) -> pallet_staking_async::slashing::SpanRecord<Balance> {
		pallet_staking_async::slashing::SpanRecord {
			slashed: record.slashed,
			paid_out: record.paid_out,
		}
	}
}

impl<AccountId: Ord, Ah: pallet_staking_async::Config<AccountId = AccountId>>
	IntoAh<pallet_staking::EraRewardPoints<AccountId>, pallet_staking_async::EraRewardPoints<Ah>>
	for pallet_staking::EraRewardPoints<AccountId>
where
	AccountId: Ord,
	Ah: pallet_staking_async::Config<AccountId = AccountId>,
{
	fn intoAh(
		points: pallet_staking::EraRewardPoints<AccountId>,
	) -> pallet_staking_async::EraRewardPoints<Ah> {
		let bounded = points
			.individual
			.into_iter()
			.take(<Ah as pallet_staking_async::Config>::MaxValidatorSet::get() as usize)
			.collect::<BTreeMap<_, _>>();
		pallet_staking_async::EraRewardPoints {
			total: points.total,
			individual: BoundedBTreeMap::try_from(bounded).defensive().unwrap_or_default(),
		}
	}
}

impl<AccountId>
	IntoAh<
		pallet_staking::RewardDestination<AccountId>,
		pallet_staking_async::RewardDestination<AccountId>,
	> for pallet_staking::RewardDestination<AccountId>
{
	fn intoAh(
		destination: pallet_staking::RewardDestination<AccountId>,
	) -> pallet_staking_async::RewardDestination<AccountId> {
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

impl IntoAh<pallet_staking::ValidatorPrefs, pallet_staking_async::ValidatorPrefs>
	for pallet_staking::ValidatorPrefs
{
	fn intoAh(prefs: pallet_staking::ValidatorPrefs) -> pallet_staking_async::ValidatorPrefs {
		pallet_staking_async::ValidatorPrefs {
			commission: prefs.commission,
			blocked: prefs.blocked,
		}
	}
}
impl<Balance: HasCompact + MaxEncodedLen>
	IntoAh<pallet_staking::UnlockChunk<Balance>, pallet_staking_async::UnlockChunk<Balance>>
	for pallet_staking::UnlockChunk<Balance>
{
	fn intoAh(
		chunk: pallet_staking::UnlockChunk<Balance>,
	) -> pallet_staking_async::UnlockChunk<Balance> {
		pallet_staking_async::UnlockChunk { value: chunk.value, era: chunk.era }
	}
}

impl IntoAh<pallet_staking::slashing::SlashingSpans, pallet_staking_async::slashing::SlashingSpans>
	for pallet_staking::slashing::SlashingSpans
{
	fn intoAh(
		spans: pallet_staking::slashing::SlashingSpans,
	) -> pallet_staking_async::slashing::SlashingSpans {
		pallet_staking_async::slashing::SlashingSpans {
			span_index: spans.span_index,
			last_start: spans.last_start,
			last_nonzero_slash: spans.last_nonzero_slash,
			prior: spans.prior,
		}
	}
}
// StakingLedger requires a T instead of having a `StakingLedgerOf` :(
impl<T, Ah, SNomQuota, SSNomQuota> IntoAh<RcStakingMessageOf<T>, AhEquivalentStakingMessageOf<Ah>>
	for RcStakingMessageOf<T>
where
	T: pallet_staking::Config<NominationsQuota = SNomQuota>,
	Ah: pallet_staking_async::Config<
		NominationsQuota = SSNomQuota,
		CurrencyBalance = BalanceOf<T>,
		AccountId = AccountIdOf<T>,
	>,
	SNomQuota: pallet_staking::NominationsQuota<BalanceOf<T>>,
	SSNomQuota: pallet_staking_async::NominationsQuota<
		pallet_staking_async::BalanceOf<Ah>,
		MaxNominations = SNomQuota::MaxNominations,
	>,
{
	fn intoAh(message: RcStakingMessageOf<T>) -> AhEquivalentStakingMessageOf<Ah> {
		use RcStakingMessage::*;
		match message {
			// It looks like nothing happens here, but it does. We swap the omitted generics of
			// `RcStakingMessage` from `T` to `Ah`:
			Values(values) => Values(values),
			Invulnerables(invulnerables) => Invulnerables(invulnerables),
			Bonded { stash, controller } => Bonded { stash, controller },
			Ledger { controller, ledger } =>
				Ledger { controller, ledger: pallet_staking::StakingLedger::intoAh(ledger) },
			Payee { stash, payment } =>
				Payee { stash, payment: pallet_staking::RewardDestination::intoAh(payment) },
			Validators { stash, validators } =>
				Validators { stash, validators: pallet_staking::ValidatorPrefs::intoAh(validators) },
			Nominators { stash, nominations } =>
				Nominators { stash, nominations: pallet_staking::Nominations::intoAh(nominations) },
			VirtualStakers(staker) => VirtualStakers(staker),
			ErasStakersOverview { era, validator, exposure } =>
				ErasStakersOverview { era, validator, exposure },
			ErasStakersPaged { era, validator, page, exposure } =>
				ErasStakersPaged { era, validator, page, exposure },
			ClaimedRewards { era, validator, rewards } =>
				ClaimedRewards { era, validator, rewards },
			ErasValidatorPrefs { era, validator, prefs } => ErasValidatorPrefs {
				era,
				validator,
				prefs: pallet_staking::ValidatorPrefs::intoAh(prefs),
			},
			ErasValidatorReward { era, reward } => ErasValidatorReward { era, reward },
			ErasRewardPoints { era, points } =>
				ErasRewardPoints { era, points: pallet_staking::EraRewardPoints::intoAh(points) },
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
			SlashingSpans { account, spans } => SlashingSpans {
				account,
				spans: pallet_staking::slashing::SlashingSpans::intoAh(spans),
			},
			SpanSlash { account, span, slash } => SpanSlash {
				account,
				span,
				slash: pallet_staking::slashing::SpanRecord::intoAh(slash),
			},
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

		ValidatorCount::<T>::put(&values.validator_count);
		// MinimumValidatorCount is not migrated
		MinNominatorBond::<T>::put(&values.min_nominator_bond);
		MinValidatorBond::<T>::put(&values.min_validator_bond);
		MinimumActiveStake::<T>::put(&values.min_active_stake);
		MinCommission::<T>::put(&values.min_commission);
		MaxValidatorsCount::<T>::set(values.max_validators_count);
		MaxNominatorsCount::<T>::set(values.max_nominators_count);
		let active_era = values.active_era.map(pallet_staking::ActiveEraInfo::intoAh);

		ActiveEra::<T>::set(active_era.clone());
		CurrentEra::<T>::set(active_era.map(|a| a.index));
		ForceEra::<T>::put(pallet_staking::Forcing::intoAh(values.force_era));
		MaxStakedRewards::<T>::set(values.max_staked_rewards);
		SlashRewardFraction::<T>::set(values.slash_reward_fraction);
		CanceledSlashPayout::<T>::set(values.canceled_slash_payout);
		// CurrentPlannedSession is not migrated
		ChillThreshold::<T>::set(values.chill_threshold);
	}
}

impl IntoAh<pallet_staking::Forcing, pallet_staking_async::Forcing> for pallet_staking::Forcing {
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
	for pallet_staking::ActiveEraInfo
{
	fn intoAh(active_era: pallet_staking::ActiveEraInfo) -> pallet_staking_async::ActiveEraInfo {
		pallet_staking_async::ActiveEraInfo { index: active_era.index, start: active_era.start }
	}
}
