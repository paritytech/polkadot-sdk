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

use crate::*;
use pallet_rc_migrator::bounties::{
	BountiesMigrator, RcBountiesMessage, RcBountiesMessageOf, RcPrePayload,
};

impl<T: Config> Pallet<T> {
	pub fn do_receive_bounties_messages(
		messages: Vec<RcBountiesMessageOf<T>>,
	) -> Result<(), Error<T>> {
		log::info!(target: LOG_TARGET, "Processing {} bounties messages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Bounties,
			count: messages.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);

		for message in messages {
			match Self::do_process_bounty_message(message) {
				Ok(()) => count_good += 1,
				Err(_) => count_bad += 1,
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Bounties,
			count_good,
			count_bad,
		});
		log::info!(target: LOG_TARGET, "Processed {}/{} bounties messages", count_good, count_bad);

		Ok(())
	}

	fn do_process_bounty_message(message: RcBountiesMessageOf<T>) -> Result<(), Error<T>> {
		log::debug!(target: LOG_TARGET, "Processing bounties message: {:?}", message);

		match message {
			RcBountiesMessage::BountyCount(count) => {
				log::debug!(target: LOG_TARGET, "Integrating bounties count: {:?}", count);
				pallet_bounties::BountyCount::<T>::put(count);
			},
			RcBountiesMessage::BountyApprovals(approvals) => {
				log::debug!(target: LOG_TARGET, "Integrating bounties approvals: {:?}", approvals);
				let approvals = BoundedVec::<
                    _,
                    <T as pallet_treasury::Config>::MaxApprovals
                >::defensive_truncate_from(approvals);
				pallet_bounties::BountyApprovals::<T>::put(approvals);
			},
			RcBountiesMessage::BountyDescriptions((index, description)) => {
				log::debug!(target: LOG_TARGET, "Integrating bounties descriptions: {:?}", description);
				let description = BoundedVec::<
					_,
					<T as pallet_bounties::Config>::MaximumReasonLength,
				>::defensive_truncate_from(description);
				pallet_bounties::BountyDescriptions::<T>::insert(index, description);
			},
			RcBountiesMessage::Bounties((index, bounty)) => {
				log::debug!(target: LOG_TARGET, "Integrating bounty: {:?}", index);
				pallet_rc_migrator::bounties::alias::Bounties::<T>::insert(index, bounty);
			},
		}

		log::debug!(target: LOG_TARGET, "Processed bounties message");
		Ok(())
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for BountiesMigrator<T> {
	type RcPrePayload = RcPrePayload<T>;
	type AhPrePayload = ();

	fn pre_check(_rc_pre_payload: Self::RcPrePayload) -> Self::AhPrePayload {
		// "Assert storage 'Bounties::BountyCount::ah_pre::empty'"
		assert_eq!(
			pallet_bounties::BountyCount::<T>::get(),
			0,
			"Bounty count should be empty on asset hub before migration"
		);

		// "Assert storage 'Bounties::Bounties::ah_pre::empty'"
		assert!(
			pallet_bounties::Bounties::<T>::iter().next().is_none(),
			"The Bounties map should be empty on asset hub before migration"
		);

		// "Assert storage 'Bounties::BountyDescriptions::ah_pre::empty'"
		assert!(
			pallet_bounties::BountyDescriptions::<T>::iter().next().is_none(),
			"The Bounty Descriptions map should be empty on asset hub before migration"
		);

		// "Assert storage 'Bounties::BountyApprovals::ah_pre::empty'"
		assert!(
			pallet_bounties::BountyApprovals::<T>::get().is_empty(),
			"The Bounty Approvals vec should be empty on asset hub before migration"
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _ah_pre_payload: Self::AhPrePayload) {
		let (rc_count, rc_bounties, rc_descriptions, rc_approvals) = rc_pre_payload;

		// Assert storage 'Bounties::BountyCount::ah_post::correct'
		assert_eq!(
			pallet_bounties::BountyCount::<T>::get(),
			rc_count,
			"Bounty count on Asset Hub should match the RC value"
		);

		// Assert storage 'Bounties::Bounties::ah_post::length'
		assert_eq!(
			pallet_bounties::Bounties::<T>::iter_keys().count() as u32,
			rc_bounties.len() as u32,
			"Bounties map length on Asset Hub should match the RC value"
		);

		// Assert storage 'Bounties::Bounties::ah_post::correct'
		// Assert storage 'Bounties::Bounties::ah_post::consistent'
		assert_eq!(
			pallet_bounties::Bounties::<T>::iter().collect::<Vec<_>>(),
			rc_bounties,
			"Bounties map value on Asset Hub should match the RC value"
		);

		// Assert storage 'Bounties::BountyDescriptions::ah_post::length'
		assert_eq!(
			pallet_bounties::BountyDescriptions::<T>::iter_keys().count() as u32,
			rc_descriptions.len() as u32,
			"Bounty description map length on Asset Hub should match RC value"
		);

		// Assert storage 'Bounties::BountyDescriptions::ah_post::correct'
		// Assert storage 'Bounties::BountyDescriptions::ah_post::consistent'
		assert_eq!(
			pallet_bounties::BountyDescriptions::<T>::iter()
				.map(|(key, bounded_vec)| { (key, bounded_vec.into_inner()) })
				.collect::<Vec<_>>(),
			rc_descriptions,
			"Bounty descriptions map value on Asset Hub should match RC value"
		);

		// Assert storage 'Bounties::BountyApprovals::ah_post::length'
		assert_eq!(
			pallet_bounties::BountyApprovals::<T>::get().into_inner().len(),
			rc_approvals.len(),
			"Bounty approvals vec value on Asset Hub should match RC values"
		);

		// Assert storage 'Bounties::BountyApprovals::ah_post::correct'
		// Assert storage 'Bounties::BountyApprovals::ah_post::consistent'
		assert_eq!(
			pallet_bounties::BountyApprovals::<T>::get().into_inner(),
			rc_approvals,
			"Bounty approvals vec value on Asset Hub should match RC values"
		);
	}
}
