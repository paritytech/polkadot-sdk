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
use chrono::TimeZone;
use pallet_rc_migrator::crowdloan::RcCrowdloanMessage;

impl<T: Config> Pallet<T> {
	pub fn do_receive_crowdloan_messages(
		messages: Vec<RcCrowdloanMessageOf<T>>,
	) -> Result<(), Error<T>> {
		let (mut good, mut bad) = (0, 0);
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Crowdloan,
			count: messages.len() as u32,
		});
		log::info!(target: LOG_TARGET, "Received {} crowdloan messages", messages.len());

		for message in messages {
			match Self::do_process_crowdloan_message(message) {
				Ok(()) => good += 1,
				Err(e) => {
					bad += 1;
					log::error!(target: LOG_TARGET, "Error while integrating crowdloan message: {:?}", e);
				},
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Crowdloan,
			count_good: good,
			count_bad: bad,
		});

		Ok(())
	}

	pub fn do_process_crowdloan_message(message: RcCrowdloanMessageOf<T>) -> Result<(), Error<T>> {
		match message {
			RcCrowdloanMessage::LeaseReserve { unreserve_block, account, para_id, amount } => {
				log::info!(target: LOG_TARGET, "Integrating lease reserve for para_id: {:?}, account: {:?}, amount: {:?}, unreserve_block: {:?}", &para_id, &account, &amount, &unreserve_block);
				defensive_assert!(!pallet_ah_ops::RcLeaseReserve::<T>::contains_key((
					unreserve_block,
					para_id,
					&account
				)));

				pallet_ah_ops::RcLeaseReserve::<T>::insert(
					(unreserve_block, para_id, &account),
					amount,
				);
			},
			RcCrowdloanMessage::CrowdloanContribution {
				withdraw_block,
				contributor,
				para_id,
				amount,
				crowdloan_account,
			} => {
				log::info!(target: LOG_TARGET, "Integrating crowdloan contribution for para_id: {:?}, contributor: {:?}, amount: {:?}, crowdloan_account: {:?}, withdraw_block: {:?}", &para_id, &contributor, &amount, &crowdloan_account, &withdraw_block);
				defensive_assert!(!pallet_ah_ops::RcCrowdloanContribution::<T>::contains_key((
					withdraw_block,
					para_id,
					&contributor
				)));

				pallet_ah_ops::RcCrowdloanContribution::<T>::insert(
					(withdraw_block, para_id, &contributor),
					(crowdloan_account, amount),
				);
			},
			RcCrowdloanMessage::CrowdloanReserve {
				unreserve_block,
				para_id,
				amount,
				depositor,
			} => {
				log::info!(target: LOG_TARGET, "Integrating crowdloan reserve for para_id: {:?}, amount: {:?}, depositor: {:?}", &para_id, &amount, &depositor);
				defensive_assert!(!pallet_ah_ops::RcCrowdloanReserve::<T>::contains_key((
					unreserve_block,
					para_id,
					&depositor
				)));

				pallet_ah_ops::RcCrowdloanReserve::<T>::insert(
					(unreserve_block, para_id, &depositor),
					amount,
				);
			},
		}

		Ok(())
	}
}

pub struct CrowdloanMigrationCheck<T>(pub PhantomData<T>);

#[cfg(feature = "std")]
impl<T: Config> CrowdloanMigrationCheck<T>
where
	BlockNumberFor<T>: Into<u64>,
{
	pub fn post_check() {
		println!("Lease reserve info");
		let lease_reserves = pallet_ah_ops::RcLeaseReserve::<T>::iter().collect::<Vec<_>>();
		for ((unlock_block, para_id, who), value) in &lease_reserves {
			println!(
				"Lease Reserve [{unlock_block}] {para_id} {who}: {} ({})",
				value / 10_000_000_000,
				Self::block_to_date(*unlock_block)
			);
		}

		let total_reserved = lease_reserves.iter().map(|((_, _, _), value)| value).sum::<u128>();
		println!(
			"Num lease reserves: {}, total reserved amount: {}",
			lease_reserves.len(),
			total_reserved / 10_000_000_000
		);

		println!("Crowdloan reserve info");
		let crowdloan_reserves = pallet_ah_ops::RcCrowdloanReserve::<T>::iter().collect::<Vec<_>>();
		for ((unlock_block, para_id, who), value) in &crowdloan_reserves {
			println!(
				"Crowdloan Reserve [{unlock_block}] {para_id} {who}: {} ({})",
				value / 10_000_000_000,
				Self::block_to_date(*unlock_block)
			);
		}

		let total_reserved =
			crowdloan_reserves.iter().map(|((_, _, _), value)| value).sum::<u128>();
		println!(
			"Num crowdloan reserves: {}, total reserved amount: {}",
			crowdloan_reserves.len(),
			total_reserved / 10_000_000_000
		);
	}

	#[cfg(feature = "std")]
	fn block_to_date(block: BlockNumberFor<T>) -> chrono::DateTime<chrono::Utc> {
		let anchor_block: u64 =
			<T as crate::Config>::RcBlockNumberProvider::current_block_number().into();
		// We are using the time from AH here, not relay. But the snapshots are taken together.
		let anchor_timestamp: u64 = pallet_timestamp::Now::<T>::get().into();

		let block_diff: u64 = (block.into() - anchor_block).into();
		let add_time_ms: i64 = (block_diff * 6_000) as i64;

		// convert anchor_timestamp to chrono timestamp
		let anchor_timestamp = chrono::Utc.timestamp_millis_opt(anchor_timestamp as i64).unwrap();
		let block_timestamp = anchor_timestamp + chrono::Duration::milliseconds(add_time_ms);
		block_timestamp
	}
}
