// Copyright (C) Parity Technologies and the various Polkadot contributors, see Contributions.md
// for a list of specific contributors.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use crate::*;
use pallet_rc_migrator::treasury::{
	alias as treasury_alias, RcSpendStatus, RcSpendStatusOf, TreasuryMigrator,
};
use pallet_treasury::{ProposalIndex, SpendIndex};

impl<T: Config> Pallet<T> {
	pub fn do_receive_treasury_messages(messages: Vec<RcTreasuryMessageOf<T>>) -> DispatchResult {
		log::info!(target: LOG_TARGET, "Processing {} treasury messages", messages.len());
		Self::deposit_event(Event::BatchReceived {
			pallet: PalletEventName::Treasury,
			count: messages.len() as u32,
		});
		let (mut count_good, mut count_bad) = (0, 0);

		for message in messages {
			match Self::do_process_treasury_message(message) {
				Ok(()) => count_good += 1,
				Err(_) => count_bad += 1,
			}
		}

		Self::deposit_event(Event::BatchProcessed {
			pallet: PalletEventName::Treasury,
			count_good,
			count_bad,
		});
		log::info!(target: LOG_TARGET, "Processed {} treasury messages", count_good);

		Ok(())
	}

	fn do_process_treasury_message(message: RcTreasuryMessageOf<T>) -> Result<(), Error<T>> {
		log::debug!(target: LOG_TARGET, "Processing treasury message: {:?}", message);

		match message {
			RcTreasuryMessage::ProposalCount(proposal_count) => {
				pallet_treasury::ProposalCount::<T>::put(proposal_count);
			},
			RcTreasuryMessage::Proposals((proposal_index, proposal)) => {
				pallet_treasury::Proposals::<T>::insert(proposal_index, proposal);
			},
			RcTreasuryMessage::Approvals(approvals) => {
				let approvals = BoundedVec::<_, <T as pallet_treasury::Config>::MaxApprovals>::defensive_truncate_from(approvals);
				pallet_treasury::Approvals::<T>::put(approvals);
			},
			RcTreasuryMessage::SpendCount(spend_count) => {
				treasury_alias::SpendCount::<T>::put(spend_count);
			},
			RcTreasuryMessage::Spends { id: spend_index, status: spend } => {
				let treasury_alias::SpendStatus {
					asset_kind,
					amount,
					beneficiary,
					valid_from,
					expire_at,
					status,
				} = spend;
				let (asset_kind, beneficiary) =
					T::RcToAhTreasurySpend::convert((asset_kind, beneficiary)).map_err(|_| {
						defensive!(
							"Failed to convert RC treasury spend to AH treasury spend: {:?}",
							spend_index
						);
						Error::FailedToConvertType
					})?;
				let spend = treasury_alias::SpendStatus {
					asset_kind,
					amount,
					beneficiary,
					valid_from,
					expire_at,
					status,
				};
				log::debug!(target: LOG_TARGET, "Mapped treasury spend: {:?}", spend);
				treasury_alias::Spends::<T>::insert(spend_index, spend);
			},
			// TODO: migrate with new sdk version
			// RcTreasuryMessage::LastSpendPeriod(last_spend_period) => {
			// 	pallet_treasury::LastSpendPeriod::<T>::put(last_spend_period);
			// },
			RcTreasuryMessage::Funds => {
				Self::migrate_treasury_funds();
			},
		}

		Ok(())
	}

	/// Migrate treasury funds.
	///
	/// Transfer all assets from old treasury account id on Asset Hub (account id derived from the
	/// treasury pallet location on RC from the perspective of AH) to new account id on Asset Hub
	/// (the treasury account id used on RC).
	fn migrate_treasury_funds() {
		let (old_account_id, assets) = T::TreasuryAccounts::get();
		let account_id = pallet_treasury::Pallet::<T>::account_id();

		// transfer all assets from old treasury account id to new account id
		for asset in assets {
			let reducible = T::Assets::reducible_balance(
				asset.clone(),
				&old_account_id,
				Preservation::Expendable,
				Fortitude::Polite,
			);

			match T::Assets::transfer(
				asset.clone(),
				&old_account_id,
				&account_id,
				reducible,
				Preservation::Expendable,
			) {
				Ok(_) => log::info!(
					target: LOG_TARGET,
					"Transferred treasury funds from old account {:?} to new account {:?} \
					for asset: {:?}, amount: {:?}",
					old_account_id,
					account_id,
					asset,
					reducible
				),
				Err(e) => {
					log::error!(
						target: LOG_TARGET,
						"Failed to transfer treasury funds from old account {:?} to new \
						account {:?} for asset: {:?}, amount: {:?}, error: {:?}",
						old_account_id,
						account_id,
						asset,
						reducible,
						e
					);
				},
			}
		}

		let reducible = <<T as Config>::Currency as Inspect<T::AccountId>>::reducible_balance(
			&old_account_id,
			Preservation::Expendable,
			Fortitude::Polite,
		);

		match <<T as Config>::Currency as Mutate<T::AccountId>>::transfer(
			&old_account_id,
			&account_id,
			reducible,
			Preservation::Expendable,
		) {
			Ok(_) => log::info!(
				target: LOG_TARGET,
				"Transferred treasury native asset funds from old account {:?} \
				to new account {:?} amount: {:?}",
				old_account_id,
				account_id,
				reducible
			),
			Err(e) => log::error!(
				target: LOG_TARGET,
				"Failed to transfer treasury funds from new account {:?} \
				to old account {:?} amount: {:?}, error: {:?}",
				account_id,
				old_account_id,
				reducible,
				e
			),
		};
	}
}

#[cfg(feature = "std")]
impl<T: Config> crate::types::AhMigrationCheck for TreasuryMigrator<T> {
	// (proposals ids, historical proposals count, approvals ids, spends, historical spends count)
	type RcPrePayload =
		(Vec<ProposalIndex>, u32, Vec<ProposalIndex>, Vec<(SpendIndex, RcSpendStatusOf<T>)>, u32);
	type AhPrePayload = ();

	fn pre_check(_: Self::RcPrePayload) -> Self::AhPrePayload {
		// Assert storage 'Treasury::ProposalCount::ah_pre::empty'
		assert_eq!(
			pallet_treasury::ProposalCount::<T>::get(),
			0,
			"ProposalCount should be 0 on Asset Hub before migration"
		);

		// Assert storage 'Treasury::Approvals::ah_pre::empty'
		assert!(
			pallet_treasury::Approvals::<T>::get().is_empty(),
			"Approvals should be empty on Asset Hub before migration"
		);

		// Assert storage 'Treasury::Proposals::ah_pre::empty'
		assert!(
			pallet_treasury::Proposals::<T>::iter().next().is_none(),
			"Proposals should be empty on Asset Hub before migration"
		);

		// Assert storage 'Treasury::SpendCount::ah_pre::empty'
		assert_eq!(
			treasury_alias::SpendCount::<T>::get(),
			0,
			"SpendCount should be 0 on Asset Hub before migration"
		);

		// Assert storage 'Treasury::Spends::ah_pre::empty'
		assert!(
			treasury_alias::Spends::<T>::iter().next().is_none(),
			"Spends should be empty on Asset Hub before migration"
		);
	}

	fn post_check(
		(proposals, proposals_count, approvals, spends, spends_count): Self::RcPrePayload,
		_: Self::AhPrePayload,
	) {
		// Assert storage 'Treasury::ProposalCount::ah_post::correct'
		assert_eq!(
			pallet_treasury::ProposalCount::<T>::get(),
			proposals_count,
			"ProposalCount on Asset Hub should match Relay Chain value"
		);

		// Assert storage 'Treasury::SpendCount::ah_post::correct'
		assert_eq!(
			treasury_alias::SpendCount::<T>::get(),
			spends_count,
			"SpendCount on Asset Hub should match Relay Chain value"
		);

		// Assert storage 'Treasury::ProposalCount::ah_post::consistent'
		// Assert storage 'Treasury::Proposals::ah_post::length'
		assert_eq!(
			pallet_treasury::Proposals::<T>::iter_keys().count() as u32,
			proposals.len() as u32,
			"Number of active proposals on Asset Hub should match Relay Chain value"
		);

		// Assert storage 'Treasury::Proposals::ah_post::consistent'
		// Assert storage 'Treasury::Proposals::ah_post::correct'
		assert_eq!(
			proposals,
			pallet_treasury::Proposals::<T>::iter_keys().collect::<Vec<_>>(),
			"Proposals IDs on Asset Hub should match Relay Chain proposal IDs"
		);

		// Assert storage 'Treasury::Approvals::ah_post::correct'
		// Assert storage 'Treasury::Approvals::ah_post::consistent'
		assert_eq!(
			pallet_treasury::Approvals::<T>::get().into_inner(),
			approvals,
			"Approvals on Asset Hub should match Relay Chain approvals"
		);

		// Assert storage 'Treasury::Approvals::ah_post::length'
		assert_eq!(
			pallet_treasury::Approvals::<T>::get().into_inner().len(),
			approvals.len(),
			"Treasury::Approvals::ah_post::length"
		);

		// Assert storage 'Treasury::SpendCount::ah_post::consistent'
		// Assert storage 'Treasury::SpendCount::ah_post::length'
		assert_eq!(
			treasury_alias::Spends::<T>::iter_keys().count() as u32,
			spends.len() as u32,
			"Number of active spends on Asset Hub should match Relay Chain value"
		);

		// Assert storage 'Treasury::Spends::ah_post::consistent'
		let mut ah_spends = Vec::new();
		for (spend_id, spend) in treasury_alias::Spends::<T>::iter() {
			ah_spends.push((
				spend_id,
				RcSpendStatus {
					amount: spend.amount,
					valid_from: spend.valid_from,
					expire_at: spend.expire_at,
					status: spend.status.clone(),
				},
			));
		}
		// Assert storage 'Treasury::Spends::ah_post::correct'
		assert_eq!(
			ah_spends, spends,
			"Spends on Asset Hub should match migrated Spends from the relay chain"
		);
	}
}
