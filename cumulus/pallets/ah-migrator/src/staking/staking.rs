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
use frame_support::traits::DefensiveTruncateInto;
use sp_runtime::Perbill;

impl<T: Config> Pallet<T> {
	pub fn staking_migration_start_hook() {}

	pub fn staking_migration_finish_hook() {}

	pub fn do_receive_staking_messages(messages: Vec<T::RcStakingMessage>) -> Result<(), Error<T>> {
		let (mut good, mut bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Integrating {} StakingMessages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Staking,
			count: messages.len() as u32,
		});

		for message in messages {
			let translated = T::RcStakingMessage::intoAh(message);
			match Self::do_receive_staking_message(translated) {
				Ok(_) => good += 1,
				Err(_) => bad += 1,
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Staking,
			count_good: good as u32,
			count_bad: bad as u32,
		});

		Ok(())
	}

	fn do_receive_staking_message(
		message: AhEquivalentStakingMessageOf<T>,
	) -> Result<(), Error<T>> {
		use RcStakingMessage::*;

		match message {
			Values(values) => {
				log::debug!(target: LOG_TARGET, "Integrating StakingValues");
				pallet_rc_migrator::staking::StakingMigrator::<T>::put_values(values);
			},
			Invulnerables(invulnerables) => {
				log::debug!(target: LOG_TARGET, "Integrating StakingInvulnerables");
				let bounded = BoundedVec::truncate_from(invulnerables);
				pallet_staking_async::Invulnerables::<T>::put(bounded);
			},
			Bonded { stash, controller } => {
				log::debug!(target: LOG_TARGET, "Integrating Bonded of stash {:?}", stash);
				pallet_staking_async::Bonded::<T>::insert(stash, controller);
			},
			Ledger { controller, ledger } => {
				log::debug!(target: LOG_TARGET, "Integrating Ledger of controller {:?}", controller);
				pallet_staking_async::Ledger::<T>::insert(controller, ledger);
			},
			Payee { stash, payment } => {
				log::debug!(target: LOG_TARGET, "Integrating Payee of stash {:?}", stash);
				pallet_staking_async::Payee::<T>::insert(stash, payment);
			},
			Validators { stash, validators } => {
				log::debug!(target: LOG_TARGET, "Integrating Validators of stash {:?}", stash);
				pallet_staking_async::Validators::<T>::insert(stash, validators);
			},
			Nominators { stash, nominations } => {
				log::debug!(target: LOG_TARGET, "Integrating Nominators of stash {:?}", stash);
				pallet_staking_async::Nominators::<T>::insert(stash, nominations);
			},
			VirtualStakers(staker) => {
				log::debug!(target: LOG_TARGET, "Integrating VirtualStakers of staker {:?}", staker);
				pallet_staking_async::VirtualStakers::<T>::insert(staker, ());
			},
			ErasStakersOverview { era, validator, exposure } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasStakersOverview {:?}/{:?}", validator, era);
				pallet_staking_async::ErasStakersOverview::<T>::insert(era, validator, exposure);
			},
			ErasStakersPaged { era, validator, page, exposure } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasStakersPaged {:?}/{:?}/{:?}", validator, era, page);
				pallet_staking_async::ErasStakersPaged::<T>::insert(
					(era, validator, page),
					exposure,
				);
			},
			ClaimedRewards { era, validator, rewards } => {
				// NOTE: This is being renamed from `ClaimedRewards` to `ErasClaimedRewards`
				log::debug!(target: LOG_TARGET, "Integrating ErasClaimedRewards {:?}/{:?}", validator, era);
				let bounded = BoundedVec::<_, pallet_staking_async::ErasClaimedRewardsBound<T>>::truncate_from(rewards);
				let weak_bounded = WeakBoundedVec::force_from(bounded.into_inner(), None);
				pallet_staking_async::ErasClaimedRewards::<T>::insert(era, validator, weak_bounded);
			},
			ErasValidatorPrefs { era, validator, prefs } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasValidatorPrefs {:?}/{:?}", validator, era);
				pallet_staking_async::ErasValidatorPrefs::<T>::insert(era, validator, prefs);
			},
			ErasValidatorReward { era, reward } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasValidatorReward of era {:?}", era);
				pallet_staking_async::ErasValidatorReward::<T>::insert(era, reward);
			},
			ErasRewardPoints { era, points } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasRewardPoints of era {:?}", era);
				pallet_staking_async::ErasRewardPoints::<T>::insert(era, points);
			},
			ErasTotalStake { era, total_stake } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasTotalStake of era {:?}", era);
				pallet_staking_async::ErasTotalStake::<T>::insert(era, total_stake);
			},
			UnappliedSlashes { era, slash } => {
				log::debug!(target: LOG_TARGET, "Integrating UnappliedSlashes of era {:?}", era);
				let slash_key = (slash.validator.clone(), Perbill::from_percent(99), 9999);
				pallet_staking_async::UnappliedSlashes::<T>::insert(era, slash_key, slash);
			},
			BondedEras(bonded_eras) => {
				log::debug!(target: LOG_TARGET, "Integrating BondedEras");
				let bounded = BoundedVec::truncate_from(bonded_eras); // FIXME
				pallet_staking_async::BondedEras::<T>::put(bounded);
			},
			ValidatorSlashInEra { era, validator, slash } => {
				log::debug!(target: LOG_TARGET, "Integrating ValidatorSlashInEra {:?}/{:?}", validator, era);
				pallet_staking_async::ValidatorSlashInEra::<T>::insert(era, validator, slash);
			},
			NominatorSlashInEra { era, validator, slash } => {
				log::debug!(target: LOG_TARGET, "Integrating NominatorSlashInEra {:?}/{:?}", validator, era);
				pallet_staking_async::NominatorSlashInEra::<T>::insert(era, validator, slash);
			},
			SlashingSpans { account, spans } => {
				log::debug!(target: LOG_TARGET, "Integrating SlashingSpans {:?}", account);
				pallet_staking_async::SlashingSpans::<T>::insert(account, spans);
			},
			SpanSlash { account, span, slash } => {
				log::debug!(target: LOG_TARGET, "Integrating SpanSlash {:?}/{:?}", account, span);
				pallet_staking_async::SpanSlash::<T>::insert((account, span), slash);
			},
		}

		Ok(())
	}
}
