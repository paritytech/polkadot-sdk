// Copyright (C) Parity Technologies (UK) Ltd.
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

//! The Secretary Program's origins.

pub use pallet_origins::*;

#[frame_support::pallet]
pub mod pallet_origins {
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(PhantomData<T>);

	/// The pallet configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[derive(PartialEq, Eq, Clone, MaxEncodedLen, Encode, Decode, TypeInfo, RuntimeDebug)]
	#[pallet::origin]
	pub enum Origin {
		Secretary,
	}

	/// Implementation of the [EnsureOrigin] trait for the [Origin::Secretary] origin.
	pub struct Secretary;
	impl<O: Into<Result<Origin, O>> + From<Origin>> EnsureOrigin<O> for Secretary {
		type Success = ();
		fn try_origin(o: O) -> Result<Self::Success, O> {
			o.into().and_then(|o| match o {
				Origin::Secretary => Ok(()),
			})
		}

		#[cfg(feature = "runtime-benchmarks")]
		fn try_successful_origin() -> Result<O, ()> {
			Ok(O::from(Origin::Secretary))
		}
	}
}
