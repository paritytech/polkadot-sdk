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

//! Pallet for stuff specific to parachains' usage of XCM. Right now that's just the origin
//! used by parachains when receiving `Transact` messages from other parachains or the Relay chain
//! which must be natively represented.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use cumulus_primitives_core::ParaId;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_runtime::{traits::BadOrigin, RuntimeDebug};
use sp_std::prelude::*;
use xcm::latest::{ExecuteXcm, Outcome};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		type XcmExecutor: ExecuteXcm<Self::RuntimeCall>;
	}

	#[pallet::event]
	pub enum Event<T: Config> {
		/// Downward message is invalid XCM.
		/// \[ id \]
		InvalidFormat([u8; 32]),
		/// Downward message is unsupported version of XCM.
		/// \[ id \]
		UnsupportedVersion([u8; 32]),
		/// Downward message executed with the given outcome.
		/// \[ id, outcome \]
		ExecutedDownward([u8; 32], Outcome),
	}

	/// Origin for the parachains module.
	#[derive(PartialEq, Eq, Clone, Encode, Decode, TypeInfo, RuntimeDebug, MaxEncodedLen)]
	#[pallet::origin]
	pub enum Origin {
		/// It comes from the (parent) relay chain.
		Relay,
		/// It comes from a (sibling) parachain.
		SiblingParachain(ParaId),
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	impl From<ParaId> for Origin {
		fn from(id: ParaId) -> Origin {
			Origin::SiblingParachain(id)
		}
	}
	impl From<u32> for Origin {
		fn from(id: u32) -> Origin {
			Origin::SiblingParachain(id.into())
		}
	}
}

/// Ensure that the origin `o` represents a sibling parachain.
/// Returns `Ok` with the parachain ID of the sibling or an `Err` otherwise.
pub fn ensure_sibling_para<OuterOrigin>(o: OuterOrigin) -> Result<ParaId, BadOrigin>
where
	OuterOrigin: Into<Result<Origin, OuterOrigin>>,
{
	match o.into() {
		Ok(Origin::SiblingParachain(id)) => Ok(id),
		_ => Err(BadOrigin),
	}
}

/// Ensure that the origin `o` represents is the relay chain.
/// Returns `Ok` if it does or an `Err` otherwise.
pub fn ensure_relay<OuterOrigin>(o: OuterOrigin) -> Result<(), BadOrigin>
where
	OuterOrigin: Into<Result<Origin, OuterOrigin>>,
{
	match o.into() {
		Ok(Origin::Relay) => Ok(()),
		_ => Err(BadOrigin),
	}
}
