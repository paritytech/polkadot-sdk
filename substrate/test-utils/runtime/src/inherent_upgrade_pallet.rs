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

//! A pallet for testing that inherents are checked by the runtime that built the block, after
//! its migrations were applied.
//!
//! The pallet is not part of the `without_inherent_upgrade_pallet` runtime, so that runtime is
//! not able to decode the inherent of this pallet.

pub use pallet::*;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	use frame_support::{
		inherent::{InherentData, InherentIdentifier, ProvideInherent},
		pallet_prelude::*,
	};
	use frame_system::pallet_prelude::*;

	/// The inherent identifier.
	///
	/// The inherent data is a `bool`. When `true`, the inherent check requires [`Migrated`] to be
	/// set.
	pub const INHERENT_IDENTIFIER: InherentIdentifier = *b"upgrade0";

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	/// Set by `on_runtime_upgrade`.
	#[pallet::storage]
	pub type Migrated<T: Config> = StorageValue<_, bool, ValueQuery>;

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_runtime_upgrade() -> Weight {
			Migrated::<T>::put(true);
			T::DbWeight::get().writes(1)
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		#[pallet::weight((0, DispatchClass::Mandatory))]
		pub fn inherent(origin: OriginFor<T>) -> DispatchResult {
			ensure_none(origin)?;
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = sp_inherents::MakeFatalError<()>;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			data.get_data::<bool>(&INHERENT_IDENTIFIER)
				.ok()
				.flatten()
				.map(|_| Call::inherent {})
		}

		fn check_inherent(_: &Self::Call, data: &InherentData) -> Result<(), Self::Error> {
			let requires_migration =
				data.get_data::<bool>(&INHERENT_IDENTIFIER).ok().flatten().unwrap_or(false);

			if requires_migration && !Migrated::<T>::get() {
				Err(().into())
			} else {
				Ok(())
			}
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::inherent {})
		}
	}
}
