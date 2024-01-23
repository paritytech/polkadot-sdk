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

//! An index is a short form of an address. This module handles allocation
//! of indices for a newly created accounts.

#![cfg_attr(not(feature = "std"), no_std)]

mod benchmarking;
mod mock;
mod tests;
pub mod weights;
pub mod migrations;

use codec::Codec;
use frame_support::traits::{
	fungible::{InspectHold, MutateHold},
	tokens::{
		Fortitude,
		Precision,
	},
	Consideration,
	Footprint,
	StorageVersion,
};
use sp_runtime::{
	traits::{AtLeast32Bit, LookupError, Saturating, StaticLookup, Zero},
	MultiAddress,
};
use sp_std::prelude::*;
pub use weights::WeightInfo;

type BalanceOf<T> =
	<<T as Config>::Currency as frame_support::traits::fungible::Inspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;
type TicketOf<T> = <T as Config>::Consideration;
type AccountIdLookupOf<T> = <<T as frame_system::Config>::Lookup as StaticLookup>::Source;

pub use pallet::*;

/// The logging target of this pallet.
pub const LOG_TARGET: &'static str = "runtime::indices";

/// The current storage version.
const STORAGE_VERSION: StorageVersion = StorageVersion::new(1);

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	/// The module's config trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Type used for storing an account's index; implies the maximum number of accounts the
		/// system can hold.
		type AccountIndex: Parameter
			+ Member
			+ MaybeSerializeDeserialize
			+ Codec
			+ Default
			+ AtLeast32Bit
			+ Copy
			+ MaxEncodedLen;

		// TODO: How to inspect held balance only with Considerations?
		// Possible to add Currency when #[cfg(any(feature = "try-runtime", test))]?
		/// The currency trait.
		type Currency: MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ InspectHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// The overarching runtime hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// A means of providing some cost while data is stored on-chain.
		type Consideration: Consideration<Self::AccountId>;

		/// The deposit needed for reserving an index.
		#[pallet::constant]
		type Deposit: Get<BalanceOf<Self>>;

		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Weight information for extrinsics in this pallet.
		type WeightInfo: WeightInfo;
	}

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// A reason for this pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// The funds are held as deposit for claiming an index.
		#[codec(index = 0)]
		ClaimedIndex,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Assign an previously unassigned index.
		///
		/// Payment: `Deposit` is reserved from the sender account.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `index`: the index to be claimed. This must not be in use.
		///
		/// Emits `IndexAssigned` if successful.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::claim())]
		pub fn claim(origin: OriginFor<T>, index: T::AccountIndex) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Accounts::<T>::try_mutate(index, |maybe_value| -> DispatchResult {
				ensure!(maybe_value.is_none(), Error::<T>::InUse);
				let ticket =
					T::Consideration::new(
						&who,
						Footprint::from_parts(
							1,
							sp_std::mem::size_of::<<T as frame_system::Config>::AccountId>() as usize
						)
					)?;
				*maybe_value = Some((who.clone(), Some(ticket), false));
				Ok(())
			})?;
			Self::deposit_event(Event::IndexAssigned { who, index });
			Ok(())
		}

		/// Assign an index already owned by the sender to another account. The balance reservation
		/// is effectively transferred to the new account.
		///
		/// The dispatch origin for this call must be _Signed_.
		///
		/// - `index`: the index to be re-assigned. This must be owned by the sender.
		/// - `new`: the new owner of the index. This function is a no-op if it is equal to sender.
		///
		/// Emits `IndexAssigned` if successful.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::transfer())]
		pub fn transfer(
			origin: OriginFor<T>,
			new: AccountIdLookupOf<T>,
			index: T::AccountIndex,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			let new = T::Lookup::lookup(new)?;
			ensure!(who != new, Error::<T>::NotTransfer);

			Accounts::<T>::try_mutate(index, |maybe_value| -> DispatchResult {
				let (account_id, maybe_ticket, perm)
					= maybe_value.take().ok_or(Error::<T>::NotAssigned)?;
				ensure!(!perm, Error::<T>::Permanent);
				ensure!(account_id == who, Error::<T>::NotOwner);

				// Done in two steps as `transfer` does not exist for `HoldConsideration`
				// Alternatively `transfer` could be implemented
				let maybe_new_ticket = if let Some(ticket) = maybe_ticket {
					let new_ticket = ticket.clone();
					ticket.drop(&account_id)?;
					Some(new_ticket)
				} else { None };

				*maybe_value = Some((new.clone(), maybe_new_ticket, false));

				Ok(())
			})?;
			Self::deposit_event(Event::IndexAssigned { who: new, index });
			Ok(())
		}

		/// Free up an index owned by the sender.
		///
		/// Payment: Any previous deposit placed for the index is unreserved in the sender account.
		///
		/// The dispatch origin for this call must be _Signed_ and the sender must own the index.
		///
		/// - `index`: the index to be freed. This must be owned by the sender.
		///
		/// Emits `IndexFreed` if successful.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::free())]
		pub fn free(origin: OriginFor<T>, index: T::AccountIndex) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Accounts::<T>::try_mutate(index, |maybe_value| -> DispatchResult {
				let (account_id, maybe_ticket, perm) = maybe_value.take().ok_or(Error::<T>::NotAssigned)?;
				ensure!(!perm, Error::<T>::Permanent);
				ensure!(account_id == who, Error::<T>::NotOwner);

				if let Some(ticket) = maybe_ticket {
					ticket.drop(&account_id)?;
				};
				Ok(())
			})?;
			Self::deposit_event(Event::IndexFreed { index });
			Ok(())
		}

		/// Force an index to an account. This doesn't require a deposit. If the index is already
		/// held, then any deposit is reimbursed to its current owner.
		///
		/// The dispatch origin for this call must be _Root_.
		///
		/// - `index`: the index to be (re-)assigned.
		/// - `new`: the new owner of the index. This function is a no-op if it is equal to sender.
		/// - `freeze`: if set to `true`, will freeze the index so it cannot be transferred.
		///
		/// Emits `IndexAssigned` if successful.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::force_transfer())]
		pub fn force_transfer(
			origin: OriginFor<T>,
			new: AccountIdLookupOf<T>,
			index: T::AccountIndex,
			freeze: bool,
		) -> DispatchResult {
			ensure_root(origin)?;
			let new = T::Lookup::lookup(new)?;

			Accounts::<T>::try_mutate(index, |maybe_value| -> DispatchResult {
				if let Some((account_id, maybe_ticket, _)) = maybe_value.take() {
					if let Some(ticket) = maybe_ticket {
						ticket.drop(&account_id)?;
					}
				}
				*maybe_value = Some((new.clone(), None, freeze));
				Ok(())
			})?;
			Self::deposit_event(Event::IndexAssigned { who: new, index });
			Ok(())
		}

		/// Freeze an index so it will always point to the sender account. This consumes the
		/// deposit.
		///
		/// The dispatch origin for this call must be _Signed_ and the signing account must have a
		/// non-frozen account `index`.
		///
		/// - `index`: the index to be frozen in place.
		///
		/// Emits `IndexFrozen` if successful.
		///
		/// ## Complexity
		/// - `O(1)`.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::freeze())]
		pub fn freeze(origin: OriginFor<T>, index: T::AccountIndex) -> DispatchResult {
			let who = ensure_signed(origin)?;

			Accounts::<T>::try_mutate(index, |maybe_value| -> DispatchResult {
				let (account_id, maybe_ticket, perm) = maybe_value.take().ok_or(Error::<T>::NotAssigned)?;
				ensure!(!perm, Error::<T>::Permanent);
				ensure!(account_id == who, Error::<T>::NotOwner);
				if let Some(ticket) = maybe_ticket {
					ticket.burn(&account_id);
				}
				*maybe_value = Some((account_id, None, true));
				Ok(())
			})?;
			Self::deposit_event(Event::IndexFrozen { index, who });
			Ok(())
		}
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A account index was assigned.
		IndexAssigned { who: T::AccountId, index: T::AccountIndex },
		/// A account index has been freed up (unassigned).
		IndexFreed { index: T::AccountIndex },
		/// A account index has been frozen to its current account ID.
		IndexFrozen { index: T::AccountIndex, who: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The index was not already assigned.
		NotAssigned,
		/// The index is assigned to another account.
		NotOwner,
		/// The index was not available.
		InUse,
		/// The source and destination accounts are identical.
		NotTransfer,
		/// The index is permanent and may not be freed/changed.
		Permanent,
	}

	/// The lookup from index to account.
	#[pallet::storage]
	pub type Accounts<T: Config> =
		StorageMap<_, Blake2_128Concat, T::AccountIndex, (T::AccountId, Option<TicketOf<T>>, bool)>;

	#[pallet::genesis_config]
	#[derive(frame_support::DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub indices: Vec<(T::AccountIndex, T::AccountId)>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		fn build(&self) {
			for (a, b) in &self.indices {
				<Accounts<T>>::insert(a, (b, Option::<TicketOf<T>>::None, false))
			}
		}
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), sp_runtime::TryRuntimeError> {
			Self::do_try_state()?;
			Ok(())
		}
	}
}

impl<T: Config> Pallet<T> {
	// PUBLIC IMMUTABLES

	/// Lookup an T::AccountIndex to get an Id, if there's one there.
	pub fn lookup_index(index: T::AccountIndex) -> Option<T::AccountId> {
		Accounts::<T>::get(index).map(|x| x.0)
	}

	/// Lookup an address to get an Id, if there's one there.
	pub fn lookup_address(a: MultiAddress<T::AccountId, T::AccountIndex>) -> Option<T::AccountId> {
		match a {
			MultiAddress::Id(i) => Some(i),
			MultiAddress::Index(i) => Self::lookup_index(i),
			_ => None,
		}
	}

	/// Ensure the correctness of the state of this pallet.
	///
	/// The following assertions must always apply.
	///
	/// Invariants:
	/// - If the index has been frozen, the held amount for `ClaimedIndex` reason should be zero.
	#[cfg(any(feature = "try-runtime", test))]
	fn do_try_state() -> Result<(), sp_runtime::TryRuntimeError> {
		// Check held amount is zero when an index is frozen
		// TODO: Can we really assume it is true? I think we can assume `burn_all_held()` will be able
		// to burn the whole amount at least for the tests.
		let zero_held = Accounts::<T>::iter()
			.filter(|(_, (_, _, perm))| *perm == true )
			.all(
				|(_, (account_id, ticket, _))|
					ticket.is_none()
			);

		frame_support::ensure!(zero_held, "Frozen indexes should hold zero balance");

		Ok(())
	}
}

impl<T: Config> StaticLookup for Pallet<T> {
	type Source = MultiAddress<T::AccountId, T::AccountIndex>;
	type Target = T::AccountId;

	fn lookup(a: Self::Source) -> Result<Self::Target, LookupError> {
		Self::lookup_address(a).ok_or(LookupError)
	}

	fn unlookup(a: Self::Target) -> Self::Source {
		MultiAddress::Id(a)
	}
}
