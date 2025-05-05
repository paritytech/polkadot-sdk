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

//! A simple interface to interact with a Proof-of-Personhood system.

#![cfg_attr(not(feature = "std"), no_std)]
#![recursion_limit = "128"]

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::reality::{AddOnlyPeopleTrait, PeopleTrait, PersonalId};
use scale_info::TypeInfo;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

type MemberOf<T> = <<T as Config>::People as AddOnlyPeopleTrait>::Member;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::{pallet_prelude::*, Twox64Concat};
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The configuration of the pallet dummy DIM.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;

		/// The runtime event type.
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The origin which may command personhood updates through this pallet. Root can always do
		/// this.
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// The maximum number of people supported in a single operation.
		type MaxPersonBatchSize: Get<u32>;

		/// Who to tell when we recognise personhood.
		type People: PeopleTrait;
	}

	/// The record of recognized people.
	#[derive(
		Clone,
		PartialEq,
		Eq,
		RuntimeDebug,
		Encode,
		Decode,
		MaxEncodedLen,
		TypeInfo,
		DecodeWithMemTracking,
	)]
	pub struct Record<Key> {
		/// The key of the person.
		pub key: Key,
		/// Flag describing the suspension status.
		pub suspended: bool,
	}

	/// The personal IDs that are reserved by unproven people.
	#[pallet::storage]
	pub type ReservedIds<T: Config> = StorageMap<_, Blake2_128Concat, PersonalId, (), OptionQuery>;

	/// The people we track along with their records.
	#[pallet::storage]
	pub type People<T: Config> =
		StorageMap<_, Twox64Concat, PersonalId, Record<MemberOf<T>>, OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A number of IDs was reserved.
		IdsReserved { count: u32 },
		/// An ID was renewed.
		IdRenewed { id: PersonalId },
		/// A reserved ID was removed.
		IdUnreserved { id: PersonalId },
		/// Register multiple people.
		PeopleRegistered { count: u32 },
		/// Suspend a number of people.
		PeopleSuspended { count: u32 },
		/// Someone's personhood was resumed.
		PersonhoodResumed { id: PersonalId },
		/// The pallet enabled suspensions.
		SuspensionsStarted,
		/// The pallet disabled suspensions.
		SuspensionsEnded,
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The personal ID does not belong to a recognized person.
		NotPerson,
		/// The personal ID does not belong to a suspended person.
		NotSuspended,
		/// The personal ID is not reserved and awaiting recognition.
		NotReserved,
		/// The operation does not support this many people.
		TooManyPeople,
	}

	#[pallet::call(weight = <T as Config>::WeightInfo)]
	impl<T: Config> Pallet<T> {
		/// Reserve a number of personal IDs.
		#[pallet::weight(T::WeightInfo::reserve_ids(T::MaxPersonBatchSize::get()))]
		#[pallet::call_index(0)]
		pub fn reserve_ids(origin: OriginFor<T>, count: u32) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			ensure!(count <= T::MaxPersonBatchSize::get(), Error::<T>::TooManyPeople);
			for _ in 0..count {
				let id = T::People::reserve_new_id();
				ReservedIds::<T>::insert(id, ());
			}
			Self::deposit_event(Event::IdsReserved { count });
			Ok(().into())
		}

		/// Renew a personal ID. The ID must not be in use.
		#[pallet::call_index(1)]
		pub fn renew_id_reservation(
			origin: OriginFor<T>,
			id: PersonalId,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			T::People::renew_id_reservation(id)?;
			ReservedIds::<T>::insert(id, ());

			Self::deposit_event(Event::IdRenewed { id });
			Ok(().into())
		}

		/// Cancel a personal ID reservation.
		#[pallet::call_index(2)]
		pub fn cancel_id_reservation(
			origin: OriginFor<T>,
			id: PersonalId,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			T::People::cancel_id_reservation(id)?;
			ReservedIds::<T>::remove(id);

			Self::deposit_event(Event::IdUnreserved { id });
			Ok(().into())
		}

		/// Grant personhood for a list of candidates that have reserved personal IDs.
		#[pallet::weight(T::WeightInfo::recognize_personhood(T::MaxPersonBatchSize::get()))]
		#[pallet::call_index(3)]
		pub fn recognize_personhood(
			origin: OriginFor<T>,
			ids_and_keys: BoundedVec<(PersonalId, MemberOf<T>), T::MaxPersonBatchSize>,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			let count = ids_and_keys.len() as u32;
			for (id, key) in ids_and_keys.into_iter() {
				ReservedIds::<T>::take(id).ok_or(Error::<T>::NotReserved)?;
				People::<T>::insert(id, Record { key: key.clone(), suspended: false });
				T::People::recognize_personhood(id, Some(key))?;
			}

			Self::deposit_event(Event::PeopleRegistered { count });
			Ok(().into())
		}

		/// Suspend the personhood of a list of recognized people. The people must not currently be
		/// suspended.
		#[pallet::weight(T::WeightInfo::suspend_personhood(T::MaxPersonBatchSize::get()))]
		#[pallet::call_index(4)]
		pub fn suspend_personhood(
			origin: OriginFor<T>,
			ids: BoundedVec<PersonalId, T::MaxPersonBatchSize>,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			T::People::suspend_personhood(&ids[..])?;
			let count = ids.len() as u32;
			for id in ids.into_iter() {
				let mut record = People::<T>::get(id).ok_or(Error::<T>::NotPerson)?;
				record.suspended = true;
				People::<T>::insert(id, record);
			}

			Self::deposit_event(Event::PeopleSuspended { count });
			Ok(().into())
		}

		/// Resume someone's personhood. The person must currently be suspended.
		#[pallet::call_index(5)]
		pub fn resume_personhood(
			origin: OriginFor<T>,
			id: PersonalId,
		) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			let mut record = People::<T>::get(id).ok_or(Error::<T>::NotPerson)?;
			ensure!(record.suspended, Error::<T>::NotSuspended);
			T::People::recognize_personhood(id, None)?;
			record.suspended = false;
			People::<T>::insert(id, record);

			Self::deposit_event(Event::PersonhoodResumed { id });
			Ok(().into())
		}

		/// Start a mutation session in the underlying `People` interface. This call does not check
		/// whether a mutation session is already ongoing and can start new sessions.
		#[pallet::call_index(6)]
		pub fn start_mutation_session(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			T::People::start_people_set_mutation_session()?;
			Self::deposit_event(Event::SuspensionsStarted);
			Ok(().into())
		}

		/// End a mutation session in the underlying `People` interface. This call can end multiple
		/// mutation sessions, even ones not started by this pallet.
		///
		/// This call will fail if no mutation session is ongoing.
		#[pallet::call_index(7)]
		pub fn end_mutation_session(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			T::UpdateOrigin::ensure_origin_or_root(origin)?;
			T::People::end_people_set_mutation_session()?;
			Self::deposit_event(Event::SuspensionsEnded);
			Ok(().into())
		}
	}
}
