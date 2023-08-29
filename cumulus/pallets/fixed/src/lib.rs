// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

//! Fixed pallet.
//!
//! A pallet to manage a fixed set of collators in a parachain.
//!
//! ## Overview
//!
//! The Fixed pallet manages the collators of a parachain. **Collation is _not_ a
//! secure activity** and this pallet does not implement any game-theoretic mechanisms to meet BFT
//! safety assumptions of the chosen set. There are no rewards and no balances.
//!
//! The pallet starts with a genesis config of a set of collators. Then,
//! through root privileges, collators can be added or removed from the set.
//!
//! Note: Eventually the Pot distribution may be modified as discussed in
//! [this issue](https://github.com/paritytech/statemint/issues/21#issuecomment-810481073).

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	pub use crate::weights::WeightInfo;
	use frame_support::{
		pallet_prelude::*,
		traits::{EnsureOrigin, ValidatorRegistration},
		BoundedVec, DefaultNoBound,
	};
	use frame_system::pallet_prelude::*;
	use pallet_session::SessionManager;
	use sp_runtime::traits::Convert;
	use sp_staking::SessionIndex;
	use sp_std::vec::Vec;

	/// The current storage version.
	const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

	/// A convertor from collators id. Since this pallet does not have stash/controller, this is
	/// just identity.
	pub struct IdentityCollator;
	impl<T> sp_runtime::traits::Convert<T, Option<T>> for IdentityCollator {
		fn convert(t: T) -> Option<T> {
			Some(t)
		}
	}

	/// Configure the pallet by specifying the parameters and types on which it depends.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Origin that can dictate updating parameters of this pallet.
		type UpdateOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		/// Maximum number of collators.
		type MaxCollators: Get<u32>;

		/// A stable ID for a validator.
		type ValidatorId: Member + Parameter;

		/// A conversion from account ID to validator ID.
		///
		/// Its cost must be at most one storage read.
		type ValidatorIdOf: Convert<Self::AccountId, Option<Self::ValidatorId>>;

		/// Validate a user is registered
		type ValidatorRegistration: ValidatorRegistration<Self::ValidatorId>;

		// /// The weight information of this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// The collator list.
	#[pallet::storage]
	#[pallet::getter(fn collators)]
	pub type Collators<T: Config> =
		StorageValue<_, BoundedVec<T::AccountId, T::MaxCollators>, ValueQuery>;

	/// Last block authored by collator. Probably useless since we don't do
	/// rewards but may be useful for UX?
	#[pallet::storage]
	#[pallet::getter(fn last_authored_block)]
	pub type LastAuthoredBlock<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, BlockNumberFor<T>, ValueQuery>;

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub collators: Vec<T::AccountId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			let duplicate_collators =
				self.collators.iter().collect::<sp_std::collections::btree_set::BTreeSet<_>>();
			assert!(
				duplicate_collators.len() == self.collators.len(),
				"duplicate collators in genesis."
			);

			let mut bounded_collators =
				BoundedVec::<_, T::MaxCollators>::try_from(self.collators.clone())
					.expect("genesis collators are more than T::MaxCollators");

			bounded_collators.sort();
			<Collators<T>>::put(bounded_collators);
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A new collator was added.
		CollatorAdded { account_id: T::AccountId },
		/// An collator was removed.
		CollatorRemoved { account_id: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// Account is already a collator.
		AlreadyCollator,
		/// Account is not a collator.
		NotCollator,
		/// The collator list is saturated.
		TooManyCollators,
		/// Account has no associated validator ID.
		NoAssociatedValidatorId,
		/// Validator ID is not yet registered.
		ValidatorNotRegistered,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(T::MaxCollators::get() > 0, "chain must have at least one collator");
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::add_collator(
			T::MaxCollators::get().saturating_sub(1),
		))]
		pub fn add_collator(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			let validator_key = T::ValidatorIdOf::convert(who.clone())
				.ok_or(Error::<T>::NoAssociatedValidatorId)?;
			ensure!(
				T::ValidatorRegistration::is_registered(&validator_key),
				Error::<T>::ValidatorNotRegistered
			);

			<Collators<T>>::try_mutate(|collators| -> DispatchResult {
				match collators.binary_search(&who) {
					Ok(_) => return Err(Error::<T>::AlreadyCollator)?,
					Err(pos) => collators
						.try_insert(pos, who.clone())
						.map_err(|_| Error::<T>::TooManyCollators)?,
				}
				Ok(())
			})?;

			Self::deposit_event(Event::CollatorAdded { account_id: who });

			Ok(())
		}

		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove_collator(T::MaxCollators::get()))]
		pub fn remove_collator(origin: OriginFor<T>, who: T::AccountId) -> DispatchResult {
			T::UpdateOrigin::ensure_origin(origin)?;

			<Collators<T>>::try_mutate(|collators| -> DispatchResult {
				let pos = collators.binary_search(&who).map_err(|_| Error::<T>::NotCollator)?;
				collators.remove(pos);
				Ok(())
			})?;

			Self::deposit_event(Event::CollatorRemoved { account_id: who });
			Ok(())
		}
	}

	/// Keep track of number of authored blocks per authority.
	impl<T: Config + pallet_authorship::Config>
		pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T>
	{
		fn note_author(author: T::AccountId) {
			<LastAuthoredBlock<T>>::insert(author, frame_system::Pallet::<T>::block_number());

			// frame_system::Pallet::<T>::register_extra_weight_unchecked(
			// 	T::WeightInfo::note_author(),
			// 	DispatchClass::Mandatory,
			// );
		}
	}

	/// Play the role of the session manager.
	impl<T: Config> SessionManager<T::AccountId> for Pallet<T> {
		fn new_session(index: SessionIndex) -> Option<Vec<T::AccountId>> {
			log::info!(
				"assembling new collators for new session {} at #{:?}",
				index,
				<frame_system::Pallet<T>>::block_number(),
			);

			let collators = <Collators<T>>::get().iter().cloned().collect();

			// frame_system::Pallet::<T>::register_extra_weight_unchecked(
			// 	T::WeightInfo::new_session(something),
			// 	DispatchClass::Mandatory,
			// );
			Some(collators)
		}
		fn start_session(_: SessionIndex) {
			// we don't care.
		}
		fn end_session(_: SessionIndex) {
			// we don't care.
		}
	}
}
