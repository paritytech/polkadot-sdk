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

//! # Authorization Transaction Extension Example Pallet
//!
//! **This pallet serves as an example and is not meant to be used in production.**
//!
//! FRAME Transaction Extension reference implementation, origin mutation, origin authorization and
//! integration in a `TransactionExtension` pipeline.
//!
//! The [TransactionExtension](sp_runtime::traits::TransactionExtension) used in this example is
//! [AuthorizeCoownership](extensions::AuthorizeCoownership). If activated, the extension will
//! authorize 2 signers as coowners, with a [coowner origin](pallet_coownership::Origin) specific to
//! the [coownership example pallet](pallet_coownership), by validating a signature of the rest of
//! the transaction from each party. This means any extensions after ours in the pipeline, their
//! implicits and the actual call. The extension pipeline used in our example checks the genesis
//! hash, transaction version and mortality of the transaction after the `AuthorizeCoownership` runs
//! as we want these transactions to run regardless of what origin passes through them and/or we
//! want their implicit data in any signature authorization happening earlier in the pipeline.
//!
//! In this example, aside from the [AuthorizeCoownership](extensions::AuthorizeCoownership)
//! extension, we use the following pallets:
//! - [pallet_coownership] - provides a coowner origin and the functionality to authorize it.
//! - [pallet_assets] - a dummy asset pallet that tracks assets, identified by an
//!   [AssetId](pallet_assets::AssetId), and their respective owners, which can be either an
//!   [account](pallet_assets::Owner::Single) or a [pair of owners](pallet_assets::Owner::Double).
//!
//! Assets are created in [pallet_assets] using the
//! [create_asset](pallet_assets::Call::create_asset) call, which accepts traditionally signed
//! origins (a single account) or coowner origins, authorized through the
//! [CoownerOrigin](pallet_assets::Config::CoownerOrigin) type.
//!
//! ### Example runtime setup
#![doc = docify::embed!("src/mock.rs", example_runtime)]
//!
//! ### Example usage
#![doc = docify::embed!("src/tests.rs", create_coowned_asset_works)]
//!
//! This example does not focus on any pallet logic or syntax, but rather on `TransactionExtension`
//! functionality. The pallets used are just skeletons to provide storage state and custom origin
//! choices and requirements, as shown in the examples. Any weight and/or
//! transaction fee is out of scope for this example.

#![cfg_attr(not(feature = "std"), no_std)]

pub mod extensions;

#[cfg(test)]
mod mock;
#[cfg(test)]
mod tests;

extern crate alloc;

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet_coownership {
	use super::*;
	use frame_support::traits::OriginTrait;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The aggregated origin which the dispatch will take.
		type RuntimeOrigin: OriginTrait<PalletsOrigin = Self::PalletsOrigin>
			+ From<Self::PalletsOrigin>
			+ IsType<<Self as frame_system::Config>::RuntimeOrigin>;

		/// The caller origin, overarching type of all pallets origins.
		type PalletsOrigin: From<Origin<Self>> + TryInto<Origin<Self>, Error = Self::PalletsOrigin>;
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// Origin that this pallet can authorize. For the purposes of this example, it's just two
	/// accounts that own something together.
	#[pallet::origin]
	#[derive(Clone, PartialEq, Eq, RuntimeDebug, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub enum Origin<T: Config> {
		Coowners(T::AccountId, T::AccountId),
	}
}

#[frame_support::pallet(dev_mode)]
pub mod pallet_assets {
	use super::*;

	pub type AssetId = u32;

	/// Type that describes possible owners of a particular asset.
	#[derive(Clone, PartialEq, Eq, RuntimeDebug, Encode, Decode, MaxEncodedLen, TypeInfo)]
	pub enum Owner<AccountId> {
		Single(AccountId),
		Double(AccountId, AccountId),
	}

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Type that can authorize an account pair coowner origin.
		type CoownerOrigin: EnsureOrigin<
			Self::RuntimeOrigin,
			Success = (Self::AccountId, Self::AccountId),
		>;
	}

	/// Map that holds the owner information for each asset it manages.
	#[pallet::storage]
	pub type AssetOwners<T> =
		StorageMap<_, Blake2_128Concat, AssetId, Owner<<T as frame_system::Config>::AccountId>>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::error]
	pub enum Error<T> {
		/// Asset already exists.
		AlreadyExists,
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Simple call that just creates an asset with a specific `AssetId`. This call will fail if
		/// there is already an asset with the same `AssetId`.
		///
		/// The origin is either a single account (traditionally signed origin) or a coowner origin.
		#[pallet::call_index(0)]
		pub fn create_asset(origin: OriginFor<T>, asset_id: AssetId) -> DispatchResult {
			let owner: Owner<T::AccountId> = match T::CoownerOrigin::try_origin(origin) {
				Ok((first, second)) => Owner::Double(first, second),
				Err(origin) => ensure_signed(origin).map(|account| Owner::Single(account))?,
			};
			AssetOwners::<T>::try_mutate(asset_id, |maybe_owner| {
				if maybe_owner.is_some() {
					return Err(Error::<T>::AlreadyExists);
				}
				*maybe_owner = Some(owner);
				Ok(())
			})?;
			Ok(())
		}
	}
}
