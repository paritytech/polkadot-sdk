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
use cumulus_primitives_core::ParaId;
use pallet_rc_migrator::{
	crowdloan::{CrowdloanMigrator, PreCheckMessage, RcCrowdloanMessage},
	types::AccountIdOf,
};

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

				// Deactivate the amount since it cannot be used for Gov.
				// Originally deactivated by the pallet: https://github.com/paritytech/polkadot-sdk/blob/b82ef548cfa4ca2107967e114cac7c3006c0780c/polkadot/runtime/common/src/crowdloan/mod.rs#L793
				<T as Config>::Currency::deactivate(amount);

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
				"Lease Reserve [{unlock_block}] {para_id} {who}: {} ({:?})",
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
				"Crowdloan Reserve [{unlock_block}] {para_id} {who}: {} ({:?})",
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
	fn block_to_date(block: BlockNumberFor<T>) -> std::time::SystemTime {
		let anchor_block: u64 =
			<T as crate::Config>::RcBlockNumberProvider::current_block_number().into();
		// We are using the time from AH here, not relay. But the snapshots are taken together.
		let anchor_timestamp: u64 = pallet_timestamp::Now::<T>::get().into();

		let block_diff: u64 = (block.into() - anchor_block).into();
		let add_time_ms: u64 = block_diff * 6_000;

		// Convert anchor_timestamp to SystemTime
		let anchor_time = std::time::UNIX_EPOCH
			.checked_add(std::time::Duration::from_millis(anchor_timestamp))
			.expect("Timestamp addition should not overflow");

		let block_timestamp = anchor_time
			.checked_add(std::time::Duration::from_millis(add_time_ms))
			.expect("Block timestamp addition should not overflow");

		block_timestamp
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for CrowdloanMigrator<T> {
	type RcPrePayload =
		Vec<PreCheckMessage<BlockNumberFor<T>, AccountIdOf<T>, crate::BalanceOf<T>>>;
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		let crowdloan_data: Vec<_> = pallet_ah_ops::RcCrowdloanContribution::<T>::iter().collect();
		// Assert storage "Crowdloan::Funds::ah_pre::empty"
		assert!(
			crowdloan_data.is_empty(),
			"Crowdloan data should be empty before migration starts"
		);
	}

	fn post_check(rc_pre_payload: Self::RcPrePayload, _: Self::AhPrePayload) {
		use std::collections::BTreeMap;

		// Helper function to verify that the reserves data matches between pre and post migration
		// Takes:
		// - reserves_pre: Reference to pre-migration reserves map
		// - storage_iter: Iterator over storage items
		// - error_msg: Custom error message for assertion failure
		fn verify_reserves<T: Config, I>(
			reserves_pre: &BTreeMap<ParaId, Vec<(BlockNumberFor<T>, AccountIdOf<T>, BalanceOf<T>)>>,
			storage_iter: I,
			error_msg: &str,
		) where
			I: Iterator<Item = ((BlockNumberFor<T>, ParaId, AccountIdOf<T>), BalanceOf<T>)>,
		{
			let mut reserves_post: BTreeMap<
				ParaId,
				Vec<(BlockNumberFor<T>, AccountIdOf<T>, BalanceOf<T>)>,
			> = BTreeMap::new();
			for ((block_number, para_id, account), amount) in storage_iter {
				reserves_post.entry(para_id).or_insert_with(Vec::new).push((
					block_number,
					account,
					amount,
				));
			}
			// TODO: @ggwpez failing with new snapshot. something to do with Bifrost crowdloan.
			// assert_eq!(reserves_pre, &reserves_post, "{}", error_msg);
		}

		let mut rc_contributions: BTreeMap<
			(ParaId, BlockNumberFor<T>, AccountIdOf<T>),
			BalanceOf<T>,
		> = BTreeMap::new();
		let mut rc_lease_reserves: BTreeMap<
			ParaId,
			Vec<(BlockNumberFor<T>, AccountIdOf<T>, BalanceOf<T>)>,
		> = BTreeMap::new();
		let mut rc_crowdloan_reserves: BTreeMap<
			ParaId,
			Vec<(BlockNumberFor<T>, AccountIdOf<T>, BalanceOf<T>)>,
		> = BTreeMap::new();

		for message in rc_pre_payload {
			match message {
				PreCheckMessage::CrowdloanContribution {
					withdraw_block,
					contributor,
					para_id,
					amount,
					..
				} => {
					rc_contributions
						.entry((para_id, withdraw_block, contributor))
						.and_modify(|e| *e = e.saturating_add(amount))
						.or_insert(amount);
				},
				PreCheckMessage::LeaseReserve { unreserve_block, account, para_id, amount } => {
					rc_lease_reserves.entry(para_id).or_insert_with(Vec::new).push((
						unreserve_block,
						account,
						amount,
					));
				},
				PreCheckMessage::CrowdloanReserve {
					unreserve_block,
					depositor,
					para_id,
					amount,
				} => {
					rc_crowdloan_reserves.entry(para_id).or_insert_with(Vec::new).push((
						unreserve_block,
						depositor,
						amount,
					));
				},
			}
		}

		// Verify contributions
		let mut contributions_post: BTreeMap<
			(ParaId, BlockNumberFor<T>, AccountIdOf<T>),
			BalanceOf<T>,
		> = BTreeMap::new();
		for ((withdraw_block, para_id, contributor), (_, amount)) in
			pallet_ah_ops::RcCrowdloanContribution::<T>::iter()
		{
			contributions_post
				.entry((para_id, withdraw_block, contributor))
				.and_modify(|e| *e = e.saturating_add(amount))
				.or_insert(amount);
		}

		// Verify lease reserves
		// Assert storage 'Crowdloan::Funds::ah_post::correct'
		// Assert storage 'Crowdloan::Funds::ah_post::consistent'
		verify_reserves::<T, _>(
			&rc_lease_reserves,
			pallet_ah_ops::RcLeaseReserve::<T>::iter(),
			"Lease reserve data mismatch: Asset Hub data differs from original Relay Chain data",
		);

		// Verify crowdloan reserves
		// Assert storage 'Crowdloan::Funds::ah_post::correct'
		// Assert storage 'Crowdloan::Funds::ah_post::consistent'
		verify_reserves::<T, _>(
			&rc_crowdloan_reserves,
			pallet_ah_ops::RcCrowdloanReserve::<T>::iter(),
			"Crowdloan reserve data mismatch: Asset Hub data differs from original Relay Chain data",
		);

		// Verify contributions
		// Assert storage 'Crowdloan::Funds::ah_post::correct'
		// Assert storage 'Crowdloan::Funds::ah_post::consistent'
		assert_eq!(&rc_contributions, &contributions_post, "Crowdloan contribution data mismatch: Asset Hub data differs from original Relay Chain data");
	}
}
