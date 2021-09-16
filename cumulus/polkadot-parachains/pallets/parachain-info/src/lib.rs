// Copyright 2020-2021 Parity Technologies (UK) Ltd.
// This file is part of Cumulus.

// Cumulus is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Cumulus is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Cumulus.  If not, see <http://www.gnu.org/licenses/>.

//! Minimal Pallet that injects a ParachainId into Runtime storage from

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use cumulus_primitives_core::ParaId;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::genesis_config]
	pub struct GenesisConfig {
		pub parachain_id: ParaId,
	}

	#[cfg(feature = "std")]
	impl Default for GenesisConfig {
		fn default() -> Self {
			Self { parachain_id: 100.into() }
		}
	}

	#[pallet::genesis_build]
	impl<T: Config> GenesisBuild<T> for GenesisConfig {
		fn build(&self) {
			<ParachainId<T>>::put(&self.parachain_id);
		}
	}

	#[pallet::type_value]
	pub(super) fn DefaultForParachainId() -> ParaId {
		100.into()
	}

	#[pallet::storage]
	#[pallet::getter(fn parachain_id)]
	pub(super) type ParachainId<T: Config> =
		StorageValue<_, ParaId, ValueQuery, DefaultForParachainId>;

	impl<T: Config> Get<ParaId> for Pallet<T> {
		fn get() -> ParaId {
			Self::parachain_id()
		}
	}
}
