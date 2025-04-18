// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Cumulus.
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

//! Pallet for stuff specific to parachains' usage of XCM. Right now that's just the origin
//! used by parachains when receiving `Transact` messages from other parachains or the Relay chain
//! which must be natively represented.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, Encode};
use cumulus_primitives_core::ParaId;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_runtime::{traits::BadOrigin, RuntimeDebug};
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
		#[allow(deprecated)]
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
	#[derive(
		PartialEq,
		Eq,
		Clone,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		RuntimeDebug,
		MaxEncodedLen,
	)]
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
