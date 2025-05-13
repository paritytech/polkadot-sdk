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

use crate::{types::AccountIdOf, *};
use sp_runtime::traits::{CheckedAdd, CheckedDiv, CheckedMul, CheckedSub};
use sp_std::collections::btree_map::BTreeMap;

pub struct CrowdloanMigrator<T> {
	_marker: sp_std::marker::PhantomData<T>,
}

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Clone, PartialEq, Eq)]
pub enum RcCrowdloanMessage<BlockNumber, AccountId, Balance> {
	/// Reserve for some slot leases.
	LeaseReserve {
		/// The block number at which this deposit can be unreserved.
		unreserve_block: BlockNumber,
		/// Account that has `amount` reserved.
		account: AccountId,
		/// Parachain ID that this crowdloan belongs to.
		///
		/// Note that Bifrost ID 3356 is now 2030.
		para_id: ParaId,
		/// Amount that was reserved for the lease.
		///
		/// This is not necessarily the same as the full crowdloan contribution amount, since there
		/// can be contributions after the lease candle auction ended. But it is the same for solo
		/// bidders. The amount that was contributed after the cutoff will be held as *free* by the
		/// crowdloan pot account.
		amount: Balance,
	},
	/// Contribute to a crowdloan.
	CrowdloanContribution {
		/// The block number at which this contribution can be withdrawn.
		withdraw_block: BlockNumber,
		/// The contributor that will have `amount` deposited.
		contributor: AccountId,
		/// Parachain ID that this crowdloan belongs to.
		///
		/// Note that Bifrost ID 3356 is now 2030.
		para_id: ParaId,
		/// Amount that was loaned to the crowdloan.
		amount: Balance,
		/// The crowdloan pot account that will have `amount` removed.
		crowdloan_account: AccountId,
	},
	/// Reserve amount on a crowdloan pot account.
	CrowdloanReserve {
		/// The block number at which this deposit can be unreserved.
		unreserve_block: BlockNumber,
		/// The account that has `amount` reserved.
		///
		/// This is often the parachain manager or some multisig account from the parachain team
		/// who initiated the crowdloan.
		depositor: AccountId,
		/// Parachain ID that this crowdloan belongs to.
		///
		/// Note that Bifrost ID 3356 is now 2030.
		para_id: ParaId,
		/// Amount that was reserved to create the crowdloan.
		///
		/// Normally this is 500 DOT. TODO: Should sanity check.
		amount: Balance,
	},
}

pub type RcCrowdloanMessageOf<T> =
	RcCrowdloanMessage<BlockNumberFor<T>, AccountIdOf<T>, crate::BalanceOf<T>>;

#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, RuntimeDebug, Clone, PartialEq, Eq)]
#[cfg_attr(feature = "stable2503", derive(DecodeWithMemTracking))]
pub enum CrowdloanStage {
	Setup,
	LeaseReserve { last_key: Option<ParaId> },
	CrowdloanContribution { last_key: Option<ParaId> },
	CrowdloanReserve,
	Finished,
}

impl<T: Config> PalletMigration for CrowdloanMigrator<T>
	where
	crate::BalanceOf<T>:
		From<<<T as polkadot_runtime_common::slots::Config>::Currency as frame_support::traits::Currency<sp_runtime::AccountId32>>::Balance>,
	crate::BalanceOf<T>:
		From<<<<T as polkadot_runtime_common::crowdloan::Config>::Auctioneer as polkadot_runtime_common::traits::Auctioneer<<<<T as frame_system::Config>::Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number>>::Currency as frame_support::traits::Currency<sp_runtime::AccountId32>>::Balance>,
{
	type Key = CrowdloanStage;
	type Error = Error<T>;

	fn migrate_many(
		current_key: Option<Self::Key>,
		weight_counter: &mut WeightMeter,
	) -> Result<Option<Self::Key>, Self::Error> {
		let mut inner_key = current_key.unwrap_or(CrowdloanStage::Setup);
		let mut messages = XcmBatchAndMeter::new_from_config::<T>();

		loop {
			if weight_counter
				.try_consume(T::DbWeight::get().reads_writes(2, 1))
				.is_err() || weight_counter.try_consume(messages.consume_weight()).is_err()
			{
				log::info!("RC weight limit reached at batch length {}, stopping", messages.len());
				if messages.is_empty() {
					return Err(Error::OutOfWeight);
				} else {
					break;
				}
			}
			if T::MaxAhWeight::get().any_lt(T::AhWeightInfo::receive_crowdloan_messages((messages.len() + 1) as u32)) {
				log::info!("AH weight limit reached at batch length {}, stopping", messages.len());
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
				CrowdloanStage::Setup => {
					inner_key = CrowdloanStage::LeaseReserve { last_key: None };

					// Only thing to do here is to re-map the bifrost crowdloan: https://polkadot.subsquare.io/referenda/524
					if cfg!(feature = "ahm-polkadot") {
						let leases = pallet_slots::Leases::<T>::take(ParaId::from(2030));
						if leases.is_empty() {
							defensive!("Bifrost fund maybe already ended, remove this");
							continue;
						}

						// It would be better if we can re-map all contributions to the new para id, but
						// that requires to iterate them all, so we go the other way around; changing
						// the leases to the old Bifrost Crowdloan.
						pallet_slots::Leases::<T>::insert(ParaId::from(3356), leases);
						log::info!(target: LOG_TARGET, "Migrated Bifrost Leases from crowdloan 2030 to 3356");
					}
					inner_key
				},
				CrowdloanStage::LeaseReserve { last_key } => {
					let mut iter = match last_key.clone() {
						Some(last_key) => pallet_slots::Leases::<T>::iter_from_key(last_key),
						None => pallet_slots::Leases::<T>::iter(),
					};

					match iter.next() {
						Some((para_id, leases)) => {
							inner_key = CrowdloanStage::LeaseReserve { last_key: Some(para_id) };

							let Some(last_lease) = leases.last() else {
								// This seems to be fine, but i don't know why it happens, see https://github.com/paritytech/polkadot-sdk/blob/db3ff60b5af2a9017cb968a4727835f3d00340f0/polkadot/runtime/common/src/slots/mod.rs#L108-L109
								log::warn!(target: LOG_TARGET, "Empty leases for para_id: {:?}", para_id);
								continue;
							};

							let Some((lease_acc, lease_amount)) = last_lease else {
								// See https://github.com/paritytech/polkadot-sdk/blob/db3ff60b5af2a9017cb968a4727835f3d00340f0/polkadot/runtime/common/src/slots/mod.rs#L115
								defensive!("Last lease cannot be None");
								continue;
							};

							// Sanity check that all leases have the same account and amount:
							#[cfg(feature = "std")]
							for (acc, amount) in leases.iter().flatten() {
								defensive_assert!(acc == lease_acc, "All leases should have the same account");
								defensive_assert!(amount == lease_amount, "All leases should have the same amount");
							}

							// NOTE: Max instead of sum, see https://github.com/paritytech/polkadot-sdk/blob/db3ff60b5af2a9017cb968a4727835f3d00340f0/polkadot/runtime/common/src/slots/mod.rs#L102-L103
							let amount: crate::BalanceOf<T> = leases.iter().flatten().map(|(_acc, amount)| amount).max().cloned().unwrap_or_default().into();

							if amount == 0u32.into() {
								defensive_assert!(para_id < ParaId::from(2000), "Only system chains are allowed to have zero lease reserve");
								continue;
							}

							let unreserve_block = num_leases_to_ending_block::<T>(leases.len() as u32).defensive().map_err(|_| Error::<T>::Unreachable)?;

							log::debug!(target: LOG_TARGET, "Migrating out lease reserve for para_id: {:?}, account: {:?}, amount: {:?}, unreserve_block: {:?}", &para_id, &lease_acc, &amount, &unreserve_block);
							messages.push(RcCrowdloanMessage::LeaseReserve { unreserve_block, account: lease_acc.clone(), para_id, amount });
							inner_key
						},
						None => CrowdloanStage::CrowdloanContribution { last_key: None },
					}
				},
				CrowdloanStage::CrowdloanContribution { last_key } => {
					let mut funds_iter = match last_key.clone() {
						Some(last_key) => pallet_crowdloan::Funds::<T>::iter_from_key(last_key),
						None => pallet_crowdloan::Funds::<T>::iter(),
					};

					let (para_id, fund) = match funds_iter.next() {
						Some((para_id, fund)) => (para_id, fund),
						None => {
							inner_key = CrowdloanStage::CrowdloanReserve;
							continue;
						},
					};

					let mut contributions_iter = pallet_crowdloan::Pallet::<T>::contribution_iterator(fund.fund_index);

					while let Some((contributor, (amount, memo))) = contributions_iter.next() {
						if weight_counter.try_consume(T::DbWeight::get().reads_writes(1, 1)).is_err() {
							// we break in outer loop for simplicity, but still consume the weight.
							log::info!("RC weight limit reached at contributions withdrawal iteration: {}, continuing", messages.len());
						}

						if T::MaxAhWeight::get().any_lt(T::AhWeightInfo::receive_crowdloan_messages((messages.len() + 1) as u32)) {
							// we break in outer loop for simplicity.
							log::info!("AH weight limit reached at contributions withdrawal iteration: {}, continuing", messages.len());
						}

						// Dont really care about memos, but we can add them, if needed.
						if !memo.is_empty() {
							log::warn!(target: LOG_TARGET, "Discarding crowdloan memo of length: {}", &memo.len());
						}

						let leases = pallet_slots::Leases::<T>::get(para_id);
						if leases.is_empty() {
							defensive_assert!(fund.raised == 0u32.into(), "Crowdloan should be empty if there are no leases");
						}

						let crowdloan_account = pallet_crowdloan::Pallet::<T>::fund_account_id(fund.fund_index);
						let withdraw_block = num_leases_to_ending_block::<T>(leases.len() as u32).defensive().map_err(|_| Error::<T>::Unreachable)?;
							// We directly remove so that we dont have to store a cursor:
						pallet_crowdloan::Pallet::<T>::contribution_kill(fund.fund_index, &contributor);

						log::debug!(target: LOG_TARGET, "Migrating out crowdloan contribution for para_id: {:?}, contributor: {:?}, amount: {:?}, withdraw_block: {:?}", &para_id, &contributor, &amount, &withdraw_block);							

						messages.push(RcCrowdloanMessage::CrowdloanContribution { withdraw_block, contributor, para_id, amount: amount.into(), crowdloan_account });
					}
					CrowdloanStage::CrowdloanContribution { last_key: Some(para_id) }
				},
				CrowdloanStage::CrowdloanReserve => {
					// TODO: not much slower without last_key?
					match pallet_crowdloan::Funds::<T>::iter().next() {
						Some((para_id, fund)) => {
							inner_key = CrowdloanStage::CrowdloanReserve;
							pallet_crowdloan::Funds::<T>::take(para_id);

							let leases = pallet_slots::Leases::<T>::get(para_id);
							if leases.is_empty() {
								defensive_assert!(fund.raised == 0u32.into(), "Fund should be empty");
								continue;
							}
							let unreserve_block = num_leases_to_ending_block::<T>(leases.len() as u32).defensive().map_err(|_| Error::<T>::Unreachable)?;

							log::debug!(target: LOG_TARGET, "Migrating out crowdloan deposit for para_id: {:?}, fund_index: {:?}, amount: {:?}, depositor: {:?}", &para_id, &fund.fund_index, &fund.deposit, &fund.depositor);

							messages.push(RcCrowdloanMessage::CrowdloanReserve { unreserve_block, para_id, amount: fund.deposit.into(), depositor: fund.depositor });
							inner_key
						},
						None => CrowdloanStage::Finished,
					}
				},
				CrowdloanStage::Finished =>	break,
			}
		}

		if !messages.is_empty() {
			Pallet::<T>::send_chunked_xcm_and_track(
				messages,
				|messages| types::AhMigratorCall::<T>::ReceiveCrowdloanMessages { messages },
				|len| T::AhWeightInfo::receive_crowdloan_messages(len),
			)?;
		}

		if inner_key == CrowdloanStage::Finished {
			Ok(None)
		} else {
			Ok(Some(inner_key))
		}
	}
}

/// Calculate the lease ending block from the number of remaining leases (including the current).
///
/// # Example
///
/// We are in the middle of period 3 and there are 2 leases left:
/// |-0-|-1-|-2-|-3-|-4-|-5-|
///               ^-----^
/// Then this function returns the end block number of period 4 (start block of period 5).
pub fn num_leases_to_ending_block<T: Config>(num_leases: u32) -> Result<BlockNumberFor<T>, ()> {
	let now = frame_system::Pallet::<T>::block_number();
	let num_leases: BlockNumberFor<T> = num_leases.into();
	let offset = <T as pallet_slots::Config>::LeaseOffset::get();
	let period = <T as pallet_slots::Config>::LeasePeriod::get();

	// Sanity check:
	if now < offset {
		return Err(());
	}

	// The current period: (now - offset) / period
	let current_period = now.checked_sub(&offset).and_then(|x| x.checked_div(&period)).ok_or(())?;
	// (current_period + num_leases) * period + offset
	let last_period_end_block = current_period
		.checked_add(&num_leases)
		.and_then(|x| x.checked_mul(&period))
		.and_then(|x| x.checked_add(&offset))
		.ok_or(())?;
	Ok(last_period_end_block)
}

#[derive(Debug, Clone)]
pub enum PreCheckMessage<BlockNumber, AccountId, Balance> {
	// LeaseReserve {
	// 	unreserve_block: u32,
	// 	account: String,
	// 	para_id: u32,
	// 	amount: u128,
	// },
	// CrowdloanContribution {
	// 	withdraw_block: u32,
	// 	contributor: String,
	// 	para_id: u32,
	// 	amount: u128,
	// 	crowdloan_account: String,
	// },
	// CrowdloanReserve {
	// 	unreserve_block: u32,
	// 	depositor: String,
	// 	para_id: u32,
	// 	amount: u128,
	// },
	LeaseReserve {
		unreserve_block: BlockNumber,
		account: AccountId,
		para_id: ParaId,
		amount: Balance,
	},
	CrowdloanContribution {
		withdraw_block: BlockNumber,
		contributor: AccountId,
		para_id: ParaId,
		amount: Balance,
		crowdloan_account: AccountId,
	},
	CrowdloanReserve {
		unreserve_block: BlockNumber,
		depositor: AccountId,
		para_id: ParaId,
		amount: Balance,
	},
}

impl<T: Config> crate::types::RcMigrationCheck for CrowdloanMigrator<T>
	where
	crate::BalanceOf<T>:
		From<<<T as polkadot_runtime_common::slots::Config>::Currency as frame_support::traits::Currency<sp_runtime::AccountId32>>::Balance>,
	crate::BalanceOf<T>:
		From<<<<T as polkadot_runtime_common::crowdloan::Config>::Auctioneer as polkadot_runtime_common::traits::Auctioneer<<<<T as frame_system::Config>::Block as sp_runtime::traits::Block>::Header as sp_runtime::traits::Header>::Number>>::Currency as frame_support::traits::Currency<sp_runtime::AccountId32>>::Balance>,
{
	type RcPrePayload = Vec<PreCheckMessage<BlockNumberFor<T>, AccountIdOf<T>, crate::BalanceOf<T>>>;

	fn pre_check() -> Self::RcPrePayload {
		let mut all_messages = Vec::new();

		// Process leases and store them for later use
		let mut processed_leases: BTreeMap<ParaId, _> = BTreeMap::new();
		for (para_id, leases) in pallet_slots::Leases::<T>::iter() {
			// Stay consistent with migrate_many: remap for leases
			let remapped_para_id = if cfg!(feature = "ahm-polkadot") && para_id == ParaId::from(2030) {
				// re-map the bifrost crowdloan: https://polkadot.subsquare.io/referenda/524
				ParaId::from(3356)
			} else {
				para_id
			};

			let Some(last_lease) = leases.last() else {
				log::warn!(target: LOG_TARGET, "Empty leases for para_id: {:?}", para_id);
				continue;
			};
			let Some((lease_acc, _)) = last_lease else {
				continue;
			};

			let amount: crate::BalanceOf<T> = leases.iter()
				.flatten()
				.map(|(_acc, amount)| amount)
				.max()
				.cloned()
				.unwrap_or_default()
				.into();

			if amount == 0u32.into() {
				defensive_assert!(remapped_para_id < ParaId::from(2000), "Only system chains are allowed to have zero lease reserve");
				continue;
			}

			let block_number = num_leases_to_ending_block::<T>(leases.len() as u32)
				.defensive()
				.map_err(|_| Error::<T>::Unreachable)
				.unwrap_or_default();

			// Push the LeaseReserve message
			all_messages.push(PreCheckMessage::LeaseReserve {
				unreserve_block: block_number,
				account: lease_acc.clone(),
				para_id: remapped_para_id,
				amount,
			});

			// Store the leases for later use
			processed_leases.insert(remapped_para_id, leases);
		}

		// Process crowdloan funds and contributions
		for (original_para_id, fund) in pallet_crowdloan::Funds::<T>::iter() {
			let para_id = if cfg!(feature = "ahm-polkadot") && original_para_id == ParaId::from(2030) {
				// re-map the bifrost crowdloan: https://polkadot.subsquare.io/referenda/524
				ParaId::from(3356)
			} else {
				original_para_id
			};
			// Use the leases we have already processed and remapped
			let leases = processed_leases.get(&para_id)
				.cloned()
				.unwrap_or_default();
			let block_number = num_leases_to_ending_block::<T>(leases.len() as u32)
				.defensive()
				.map_err(|_| Error::<T>::Unreachable)
				.unwrap_or_default();

			let reserve_leases = pallet_slots::Leases::<T>::get(original_para_id);
			let unreserve_block = num_leases_to_ending_block::<T>(reserve_leases.len() as u32).defensive().map_err(|_| Error::<T>::Unreachable).unwrap_or_default();

			let crowdloan_account = pallet_crowdloan::Pallet::<T>::fund_account_id(fund.fund_index);
			let contributions_iter = pallet_crowdloan::Pallet::<T>::contribution_iterator(fund.fund_index);

			for (contributor, (amount, _)) in contributions_iter {
				all_messages.push(PreCheckMessage::CrowdloanContribution {
					withdraw_block: block_number,
					contributor,
					para_id,
					amount: amount.into(),
					crowdloan_account: crowdloan_account.clone(),
				});
			}

			if !reserve_leases.is_empty() {
				all_messages.push(PreCheckMessage::CrowdloanReserve {
					unreserve_block,
					depositor: fund.depositor,
					para_id,
					amount: fund.deposit.into(),
				});
			}
		}

		all_messages
	}

	fn post_check(_: Self::RcPrePayload) {
		use sp_std::collections::btree_map::BTreeMap;

		// Process leases first, similar to pre_check
		let mut processed_leases: BTreeMap<ParaId, _> = BTreeMap::new();
		for (para_id, leases) in pallet_slots::Leases::<T>::iter() {
			// Remap Bifrost para_id consistently with pre_check
			let remapped_para_id = if cfg!(feature = "ahm-polkadot") && para_id == ParaId::from(2030) {
				// re-map the bifrost crowdloan: https://polkadot.subsquare.io/referenda/524
				ParaId::from(3356)
			} else {
				para_id
			};
			processed_leases.insert(remapped_para_id, leases);
		}

		// Process crowdloan funds and their contributions
		let mut crowdloan_data: BTreeMap<ParaId, Vec<(BlockNumberFor<T>, AccountIdOf<T>, BalanceOf<T>)>> = BTreeMap::new();
		for (para_id, fund) in pallet_crowdloan::Funds::<T>::iter() {
			// Collect all contributions for this fund
			let contributions: Vec<_> = pallet_crowdloan::Pallet::<T>::contribution_iterator(fund.fund_index)
				.map(|(contributor, (amount, _memo))| {
					// We don't need to decode block numbers here since we just want to verify everything is empty
					(BlockNumberFor::<T>::default(), contributor, amount.try_into().unwrap_or_default())
				})
				.collect();

			if !contributions.is_empty() {
				crowdloan_data.insert(para_id, contributions);
			}
		}

		// Verify that all crowdloan data has been properly migrated
		assert!(
			crowdloan_data.is_empty(),
			"Crowdloan contributions should be empty after migration"
		);

		// Assert storage "Crowdloan::Funds::rc_post::empty"
		assert!(
			pallet_crowdloan::Funds::<T>::iter().next().is_none(),
			"pallet_crowdloan::Funds should be empty after migration"
		);
	}
}
