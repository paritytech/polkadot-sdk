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

//! Pallet for stuff specific to parachains' usage of XCM. Right now that's just the origin
//! used by parachains when receiving `Transact` messages from other parachains or the Relay chain
//! which must be natively represented.

#![cfg_attr(not(feature = "std"), no_std)]

use codec::{Decode, DecodeLimit, Encode};
use cumulus_primitives_core::{
	relay_chain::BlockNumber as RelayBlockNumber, DmpMessageHandler, ParaId,
};
use frame_support::dispatch::Weight;
pub use pallet::*;
use scale_info::TypeInfo;
use sp_runtime::{traits::BadOrigin, RuntimeDebug};
use sp_std::{convert::TryFrom, prelude::*};
use xcm::{
	latest::{ExecuteXcm, Outcome, Parent, Xcm},
	VersionedXcm, MAX_XCM_DECODE_DEPTH,
};

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::pallet]
	#[pallet::generate_store(pub(super) trait Store)]
	pub struct Pallet<T>(_);

	/// The module configuration trait.
	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type Event: From<Event<Self>> + IsType<<Self as frame_system::Config>::Event>;

		type XcmExecutor: ExecuteXcm<Self::Call>;
	}

	#[pallet::error]
	pub enum Error<T> {}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {}

	#[pallet::call]
	impl<T: Config> Pallet<T> {}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Downward message is invalid XCM.
		/// \[ id \]
		InvalidFormat([u8; 8]),
		/// Downward message is unsupported version of XCM.
		/// \[ id \]
		UnsupportedVersion([u8; 8]),
		/// Downward message executed with the given outcome.
		/// \[ id, outcome \]
		ExecutedDownward([u8; 8], Outcome),
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

/// For an incoming downward message, this just adapts an XCM executor and executes DMP messages
/// immediately. Their origin is asserted to be the Parent location.
///
/// The weight `limit` is only respected as the maximum for an individual message.
///
/// Because this largely ignores the given weight limit, it probably isn't good for most production
/// uses. Use DmpQueue pallet for a more robust design.
pub struct UnlimitedDmpExecution<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> DmpMessageHandler for UnlimitedDmpExecution<T> {
	fn handle_dmp_messages(
		iter: impl Iterator<Item = (RelayBlockNumber, Vec<u8>)>,
		limit: Weight,
	) -> Weight {
		let mut used = Weight::new();
		for (_sent_at, data) in iter {
			let id = sp_io::hashing::twox_64(&data[..]);
			let msg = VersionedXcm::<T::Call>::decode_all_with_depth_limit(
				MAX_XCM_DECODE_DEPTH,
				&mut data.as_slice(),
			)
			.map(Xcm::<T::Call>::try_from);
			match msg {
				Err(_) => Pallet::<T>::deposit_event(Event::InvalidFormat(id)),
				Ok(Err(())) => Pallet::<T>::deposit_event(Event::UnsupportedVersion(id)),
				Ok(Ok(x)) => {
					let outcome = T::XcmExecutor::execute_xcm(Parent, x, limit.ref_time());
					used += Weight::from_ref_time(outcome.weight_used());
					Pallet::<T>::deposit_event(Event::ExecutedDownward(id, outcome));
				},
			}
		}
		used
	}
}

/// For an incoming downward message, this just adapts an XCM executor and executes DMP messages
/// immediately. Their origin is asserted to be the Parent location.
///
/// This respects the given weight limit and silently drops messages if they would break it. It
/// probably isn't good for most production uses. Use DmpQueue pallet for a more robust design.
pub struct LimitAndDropDmpExecution<T>(sp_std::marker::PhantomData<T>);
impl<T: Config> DmpMessageHandler for LimitAndDropDmpExecution<T> {
	fn handle_dmp_messages(
		iter: impl Iterator<Item = (RelayBlockNumber, Vec<u8>)>,
		limit: Weight,
	) -> Weight {
		let mut used = Weight::new();
		for (_sent_at, data) in iter {
			let id = sp_io::hashing::twox_64(&data[..]);
			let msg = VersionedXcm::<T::Call>::decode_all_with_depth_limit(
				MAX_XCM_DECODE_DEPTH,
				&mut data.as_slice(),
			)
			.map(Xcm::<T::Call>::try_from);
			match msg {
				Err(_) => Pallet::<T>::deposit_event(Event::InvalidFormat(id)),
				Ok(Err(())) => Pallet::<T>::deposit_event(Event::UnsupportedVersion(id)),
				Ok(Ok(x)) => {
					let weight_limit = limit.saturating_sub(used);
					let outcome = T::XcmExecutor::execute_xcm(Parent, x, weight_limit.ref_time());
					used += Weight::from_ref_time(outcome.weight_used());
					Pallet::<T>::deposit_event(Event::ExecutedDownward(id, outcome));
				},
			}
		}
		used
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
