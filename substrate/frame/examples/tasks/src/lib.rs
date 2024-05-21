// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! This pallet demonstrates the use of the `pallet::task` api for service work.
#![cfg_attr(not(feature = "std"), no_std)]

use frame_support::dispatch::DispatchResult;
use frame_system::offchain::SendTransactionTypes;
#[cfg(feature = "experimental")]
use frame_system::offchain::SubmitTransaction;
use frame_support::traits::fungible::Mutate;
use sp_runtime::traits::StaticLookup;

type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

// Re-export pallet items so that they can be accessed from the crate namespace.
pub use pallet::*;

pub mod mock;
pub mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

pub mod weights;
pub use weights::*;

#[cfg(feature = "experimental")]
const LOG_TARGET: &str = "pallet-example-tasks";

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::error]
	pub enum Error<T> {
		/// The referenced task was not found.
		NotFound,
		AlreadyFunded,
	}
	
	#[pallet::tasks_experimental]
	impl<T: Config> Pallet<T> {
		/// Add a pair of numbers into the totals and remove them.
		#[pallet::task_list(Numbers::<T>::iter_keys())]
		#[pallet::task_condition(|i| Numbers::<T>::contains_key(i))]
		#[pallet::task_weight(T::WeightInfo::add_number_into_total())]
		#[pallet::task_index(0)]
		pub fn add_number_into_total(i: u32) -> DispatchResult {
			let v = Numbers::<T>::take(i).ok_or(Error::<T>::NotFound)?;
			Total::<T>::mutate(|(total_keys, total_values)| {
				*total_keys += i;
				*total_values += v;
			});
			Ok(())
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight({ 0 })] // does not matter
		pub fn faucet(origin: OriginFor<T>, who: AccountIdLookupOf<T>) -> DispatchResult {
			ensure_none(origin)?;

			let who = T::Lookup::lookup(who)?;

			if <frame_system::Pallet<T>>::account(&who) == Default::default() {
				<frame_system::Pallet<T>>::inc_providers(&who);	
			}
			
			T::Currency::mint_into(&who, 1_000_000_000u32.into())?; // TODO 1000 is a placeholder
			Ok(())
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "experimental")]
		fn offchain_worker(_block_number: BlockNumberFor<T>) {
			if let Some(key) = Numbers::<T>::iter_keys().next() {
				// Create a valid task
				let task = Task::<T>::AddNumberIntoTotal { i: key };
				let runtime_task = <T as Config>::RuntimeTask::from(task);
				let call = frame_system::Call::<T>::do_task { task: runtime_task.into() };

				// Submit the task as an unsigned transaction
				let res =
					SubmitTransaction::<T, frame_system::Call<T>>::submit_unsigned_transaction(
						call.into(),
					);
				match res {
					Ok(_) => log::info!(target: LOG_TARGET, "Submitted the task."),
					Err(e) => log::error!(target: LOG_TARGET, "Error submitting task: {:?}", e),
				}
			}
		}
	}

	#[pallet::config]
	pub trait Config:
		SendTransactionTypes<frame_system::Call<Self>> + frame_system::Config
	{
		type RuntimeTask: frame_support::traits::Task
			+ IsType<<Self as frame_system::Config>::RuntimeTask>
			+ From<Task<Self>>;
		type WeightInfo: WeightInfo;
		type Currency: Mutate<Self::AccountId>;
	}

	#[pallet::validate_unsigned]
	impl<T: Config> ValidateUnsigned for Pallet<T> {
		type Call = Call<T>;
		fn validate_unsigned(_: TransactionSource, call: &Self::Call) -> TransactionValidity {
			let Call::faucet { who } = call else {
				return InvalidTransaction::Call.into();
			};
			let who = T::Lookup::lookup(who.clone())?;

			// Check that the account does not yet exist.
			if <frame_system::Pallet<T>>::account(&who) != Default::default() {
				return InvalidTransaction::Custom(0).into();
			}

			ValidTransaction::with_tag_prefix("ExampleTasks")
				.priority(0) // Faucet, so low prio
				.longevity(5) // 5 blocks longevity to prevent too much spam
				.and_provides(who)
				.propagate(true)
				.build()
		}

		fn pre_dispatch(_: &Self::Call) -> Result<(), TransactionValidityError> {
			Ok(())
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Some running total.
	#[pallet::storage]
	pub type Total<T: Config> = StorageValue<_, (u32, u32), ValueQuery>;

	/// Numbers to be added into the total.
	#[pallet::storage]
	pub type Numbers<T: Config> = StorageMap<_, Twox64Concat, u32, u32, OptionQuery>;
}
