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

impl<T: Config> Pallet<T> {
	pub fn do_receive_staking_messages(
		messages: Vec<RcStakingMessageOf<T>>,
	) -> Result<(), Error<T>> {
		let (mut good, mut bad) = (0, 0);
		log::info!(target: LOG_TARGET, "Integrating {} StakingMessages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Staking,
			count: messages.len() as u32,
		});

		for message in messages {
			match Self::do_receive_staking_message(message) {
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

	fn do_receive_staking_message(message: RcStakingMessageOf<T>) -> Result<(), Error<T>> {
		use RcStakingMessage::*;

		match message {
			Values(values) => {
				log::debug!(target: LOG_TARGET, "Integrating StakingValues");
				pallet_rc_migrator::staking::StakingMigrator::<T>::put_values(values);
			},
			Invulnerables(invulnerables) => {
				log::debug!(target: LOG_TARGET, "Integrating StakingInvulnerables");
				let Ok(bound_invulnerables) =
					BoundedVec::try_from(invulnerables).defensive_proof("Invulnerables must fit")
				else {
					return Err(Error::FailedToBoundVector);
				};

				pallet_staking::Invulnerables::<T>::put(bound_invulnerables);
			},
			Bonded { stash, controller } => {
				log::debug!(target: LOG_TARGET, "Integrating Bonded of stash {:?}", stash);
				pallet_staking::Bonded::<T>::insert(stash, controller);
			},
			Ledger { controller, ledger } => {
				log::debug!(target: LOG_TARGET, "Integrating Ledger of controller {:?}", controller);
				pallet_staking::Ledger::<T>::insert(controller, ledger);
			},
			Payee { stash, payment } => {
				log::debug!(target: LOG_TARGET, "Integrating Payee of stash {:?}", stash);
				pallet_staking::Payee::<T>::insert(stash, payment);
			},
			Validators { stash, validators } => {
				log::debug!(target: LOG_TARGET, "Integrating Validators of stash {:?}", stash);
				pallet_staking::Validators::<T>::insert(stash, validators);
			},
			Nominators { stash, nominations } => {
				log::debug!(target: LOG_TARGET, "Integrating Nominators of stash {:?}", stash);
				pallet_staking::Nominators::<T>::insert(stash, nominations);
			},
			VirtualStakers(staker) => {
				log::debug!(target: LOG_TARGET, "Integrating VirtualStakers of staker {:?}", staker);
				pallet_staking::VirtualStakers::<T>::insert(staker, ());
			},
			ErasStartSessionIndex { era, session } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasStartSessionIndex {:?}/{:?}", era, session);
				pallet_staking::ErasStartSessionIndex::<T>::insert(era, session);
			},
			ErasStakersOverview { era, validator, exposure } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasStakersOverview {:?}/{:?}", validator, era);
				pallet_staking::ErasStakersOverview::<T>::insert(era, validator, exposure);
			},
			ErasStakersPaged { era, validator, page, exposure } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasStakersPaged {:?}/{:?}/{:?}", validator, era, page);
				pallet_staking::ErasStakersPaged::<T>::insert((era, validator, page), exposure);
			},
			ClaimedRewards { era, validator, rewards } => {
				log::debug!(target: LOG_TARGET, "Integrating ClaimedRewards {:?}/{:?}", validator, era);
				pallet_staking::ClaimedRewards::<T>::insert(era, validator, rewards);
			},
			ErasValidatorPrefs { era, validator, prefs } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasValidatorPrefs {:?}/{:?}", validator, era);
				pallet_staking::ErasValidatorPrefs::<T>::insert(era, validator, prefs);
			},
			ErasValidatorReward { era, reward } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasValidatorReward of era {:?}", era);
				pallet_staking::ErasValidatorReward::<T>::insert(era, reward);
			},
			ErasRewardPoints { era, points } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasRewardPoints of era {:?}", era);
				pallet_staking::ErasRewardPoints::<T>::insert(era, points);
			},
			ErasTotalStake { era, total_stake } => {
				log::debug!(target: LOG_TARGET, "Integrating ErasTotalStake of era {:?}", era);
				pallet_staking::ErasTotalStake::<T>::insert(era, total_stake);
			},
			BondedEras(bonded_eras) => {
				log::debug!(target: LOG_TARGET, "Integrating BondedEras");
				pallet_staking::BondedEras::<T>::put(bonded_eras);
			},
			ValidatorSlashInEra { era, validator, slash } => {
				log::debug!(target: LOG_TARGET, "Integrating ValidatorSlashInEra {:?}/{:?}", validator, era);
				pallet_staking::ValidatorSlashInEra::<T>::insert(era, validator, slash);
			},
			NominatorSlashInEra { era, validator, slash } => {
				log::debug!(target: LOG_TARGET, "Integrating NominatorSlashInEra {:?}/{:?}", validator, era);
				pallet_staking::NominatorSlashInEra::<T>::insert(era, validator, slash);
			},
			SlashingSpans { account, spans } => {
				log::debug!(target: LOG_TARGET, "Integrating SlashingSpans {:?}", account);
				pallet_staking::SlashingSpans::<T>::insert(account, spans);
			},
			SpanSlash { account, span, slash } => {
				log::debug!(target: LOG_TARGET, "Integrating SpanSlash {:?}/{:?}", account, span);
				pallet_staking::SpanSlash::<T>::insert((account, span), slash);
			},
		}

		Ok(())
	}
}
