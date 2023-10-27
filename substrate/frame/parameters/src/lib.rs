// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! # Parameters
//!
//! Allows to update configuration parameters at runtime.
//!
//! ## Pallet API
//!
//! This pallet exposes two APIs; one *inbound* side to update parameters, and one *outbound* side to access said parameters.
//! 
//! ### Inbound 
//!
//! This solely consists of the `set_parameter` extrinsic, which allows to update a parameter. Each parameter can have their own admin.
//! 
//! ### Outbound
//!
//! The outbound side is runtime facing for the most part. More general, it provides a `Get` implementation and can be used in every spot where that is accepted. Two macros are in place: `define_parameters` and `define_aggregrated_parameters` to define and expose parameters in a typed manner.
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes, including its
//! configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! This pallet is a good fit for updating parameters without a runtime upgrade. It allows for fine-grained control over who can update what. The only down-side is that it trades off performance with convenience and should therefore only be used in places where that is proven to be uncritical.
//!
//! ### Example
//!
#![doc = docify::embed!("src/mock.rs", dynamic_params)]
//!
//! <The audience of this is those who want to know how this pallet works, to the extent of being able to build
//! something on top of it, like a DApp or another pallet. In some cases, you might want to add an example of how to
//! use this pallet in other pallets.>
//!
//! This section can most often be left as-is.
//!
//! ## Low Level / Implementation Details
//!
//! <The format of this section is up to you, but we suggest the Design-oriented approach that follows>
//!
//! <The audience of this would be your future self, or anyone who wants to gain a deep understanding of how the pallet
//! works so that they can eventually propose optimizations to it>
//!
//! ### Design Goals (optional)
//!
//! <Describe your goals with the pallet design.>
//!
//! ### Design (optional)
//!
//! <Describe how you've reached those goals. This should describe the storage layout of your pallet and what was your
//! approach in designing it that way.>
//!
//! ### Terminology (optional)
//!
//! <Optionally, explain any non-obvious terminology here. You can link to it if you want to use the terminology further
//! up>

#![cfg_attr(not(feature = "std"), no_std)]
#![allow(clippy::unused_unit)]

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

use frame_support::traits::EnsureOriginWithArg;

pub mod traits;
use traits::AggregratedKeyValue;

mod mock;
mod tests;
mod weights;

pub use module::*;
pub use weights::WeightInfo;

/// The key type of a parameter.
type KeyOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey;

/// The value type of a parameter.
type ValueOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue;

#[frame_support::pallet]
pub mod module {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The key value type for parameters. Usually created by
		/// [`crate::parameters::define_aggregrated_parameters`].
		type AggregratedKeyValue: AggregratedKeyValue;

		/// The origin which may update the parameter.
		type AdminOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, KeyOf<Self>>;

		/// Weight information for extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Parameter is updated
		Updated { key_value: T::AggregratedKeyValue },
	}

	/// Stored parameters.
	///
	/// map KeyOf<T> => Option<ValueOf<T>>
	#[pallet::storage]
	pub type Parameters<T: Config> =
		StorageMap<_, Blake2_128Concat, KeyOf<T>, ValueOf<T>, OptionQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the value of a parameter.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::set_parameter())]
		pub fn set_parameter(
			origin: OriginFor<T>,
			key_value: T::AggregratedKeyValue,
		) -> DispatchResult {
			let (key, value) = key_value.clone().into_parts();

			T::AdminOrigin::ensure_origin(origin, &key)?;

			Parameters::<T>::mutate(key, |v| *v = value);

			Self::deposit_event(Event::Updated { key_value });

			Ok(())
		}
	}
}
