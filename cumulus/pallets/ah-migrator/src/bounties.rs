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
use pallet_rc_migrator::bounties::{RcBountiesMessage, RcBountiesMessageOf};

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
				pallet_bounties::Bounties::<T>::insert(index, bounty);
			},
		}

		log::debug!(target: LOG_TARGET, "Processed bounties message");
		Ok(())
	}
}
