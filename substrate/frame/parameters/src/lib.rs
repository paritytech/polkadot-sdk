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

#![cfg_attr(not(feature = "std"), no_std)]
#![deny(missing_docs)]

//! # **⚠️ WARNING ⚠️**
//!  
//! <br>  
//! <b>THIS CRATE IS NOT AUDITED AND SHOULD NOT BE USED IN VALUE BEARING CHAINS.</b>  
//! <br>  
//!
//! # Parameters
//!
//! Allows to update configuration parameters at runtime.
//!
//! ## Pallet API
//!
//! This pallet exposes two APIs; one *inbound* side to update parameters, and one *outbound* side
//! to access said parameters.
//!
//! ### Inbound
//!
//! This solely consists of the `set_parameter` extrinsic, which allows to update a parameter. Each
//! parameter can have their own admin origin.
//!
//! ### Outbound
//!
//! The outbound side is runtime facing for the most part. More general, it provides a `Get`
//! implementation and can be used in every spot where that is accepted. Two macros are in place:
//! `define_parameters` and `define_aggregrated_parameters` to define and expose parameters in a
//! typed manner.
//!
//! See the [`pallet`] module for more information about the interfaces this pallet exposes,
//! including its configuration trait, dispatchables, storage items, events and errors.
//!
//! ## Overview
//!
//! This pallet is a good fit for updating parameters without a runtime upgrade. It allows for
//! fine-grained control over who can update what. The only down-side is that it trades off
//! performance with convenience and should therefore only be used in places where that is proven to
//! be uncritical.
//!
//! ### Example
//!
//! Here is an example of how to define some parameters, including their default values:
#![doc = docify::embed!("src/tests/mock.rs", dynamic_params)]
//!
//! Now the aggregated parameter needs to be injected into the pallet config:
#![doc = docify::embed!("src/tests/mock.rs", impl_config)]
//!
//! As last step, the parameters can now be used in other pallets 🙌
#![doc = docify::embed!("src/tests/mock.rs", usage)]
//!
//! Now to demonstrate how the values can be updated:
#![doc = docify::embed!("src/tests/tests.rs", set_parameters_example)]
//!
//! ## Low Level / Implementation Details
//!
//! The pallet stores the parameters in a storage map and implements the matching `Get<Value>` for
//! each `Key` type. The `Get` then accesses the `Parameters` map to retrieve the value. An event is
//! emitted every time that a value was updated. It is even emitted when the value is changed to the
//! same.
//!
//! The key and value types themselves are defined by macros and aggregated into a runtime wide
//! enum. This enum is then injected into the pallet. This allows it to be used without any changed
//! to the pallet that the parameter will be utilized by.
//!
//! ### Design Goals
//!
//! 1. Easy to update without runtime upgrade.
//! 2. Exposes metadata and docs for user convenience.
//! 3. Can be permissioned on a per-key base.
//!
//! ### Design
//!
//! 1. Everything is done at runtime without the need for `const` values. `Get` allows for this -
//! which is coincidentally an upside and a downside. 2. The types are defined through macros, which
//! allows to expose metadata and docs. 3. Access control is done through the `EnsureOriginWithArg`
//! trait, that allows to pass data along to the origin check. It gets passed in the key. The
//! implementor can then match on the key and the origin to decide whether the origin is
//! permissioned to set the value.

use frame_support::pallet_prelude::*;
use frame_system::pallet_prelude::*;

use frame_support::traits::{
	dynamic_params::{IntoKey, Key, RuntimeParameterStore, TryIntoKey},
	AggregratedKeyValue, EnsureOriginWithArg,
};

#[cfg(test)]
mod tests;
mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

/// The key type of a parameter.
type KeyOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey;

/// The value type of a parameter.
type ValueOf<T> = <<T as Config>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The key value type for parameters. Usually created by
		/// [`frame_support::dynamic_params`].
		type AggregratedKeyValue: AggregratedKeyValue;

		/// The origin which may update the parameter.
		type AdminOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, KeyOf<Self>>;

		/// Weight information for extrinsics in this module.
		type WeightInfo: WeightInfo;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A Parameter was set.
		Updated {
			/// The Key-Value pair that was set.
			key_value: T::AggregratedKeyValue,
		},
	}

	/// Stored parameters.
	#[pallet::storage]
	pub type Parameters<T: Config> =
		StorageMap<_, Blake2_128Concat, KeyOf<T>, ValueOf<T>, OptionQuery>;

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Set the value of a parameter.
		///
		/// The dispatch origin of this call must be `AdminOrigin` for the given `key`.
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

impl<T: Config> RuntimeParameterStore for Pallet<T> {
	type AggregratedKeyValue = T::AggregratedKeyValue;

	fn get<KV, K>(key: K) -> Option<K::Value>
	where
		KV: AggregratedKeyValue,
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedKey:
			IntoKey<<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
		<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue:
			TryIntoKey<<KV as AggregratedKeyValue>::AggregratedValue>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>,
	{
		let key: <KV as AggregratedKeyValue>::AggregratedKey = key.into();
		let val = Parameters::<T>::get(key.into_key());
		val.and_then(|v| {
			let val: <KV as AggregratedKeyValue>::AggregratedValue = v.try_into_key().ok()?;
			let val: K::WrappedValue = val.try_into().ok()?;
			let val = val.into();
			Some(val)
		})
	}
}
