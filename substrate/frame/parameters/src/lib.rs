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
//! <b>THIS CRATE IS NOT AUDITED AND SHOULD NOT BE USED IN PRODUCTION.</b>  
//! <br>  
//!
//! # Parameters
//!
//! Allows to update configuration parameters at runtime.
//!
//! ## Pallet API
//!
//! This pallet exposes two APIs; one *inbound* side to update parameters, and one *outbound* side
//! to access said parameters. Parameters themselves are defined in the runtime config and will be
//! aggregated into an enum. Each parameter is addressed by a `key` and can have a default value.
//! This is not done by the pallet but through the [`frame_support::dynamic_params`] macro or
//! alternatives.
//!
//! Note that this is incurring one storage read per access. This should not be a problem in most
//! cases but must be considered in weight-restrained code.
//!
//! ### Inbound
//!
//! The inbound side solely consists of the [`Pallet::set_parameter`] extrinsic to update the value
//! of a parameter. Each parameter can have their own admin origin as given by the
//! [`Config::AdminOrigin`].
//!
//! ### Outbound
//!
//! The outbound side is runtime facing for the most part. More general, it provides a `Get`
//! implementation and can be used in every spot where that is accepted. Two macros are in place:
//! `define_parameters` and `dynamic_pallet_params` to define and expose parameters in a typed
//! manner.
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
//! ### Example Configuration
//!
//! Here is an example of how to define some parameters, including their default values:
#![doc = docify::embed!("src/tests/mock.rs", dynamic_params)]
//!
//! A permissioned origin can be define on a per-key basis like this:
#![doc = docify::embed!("src/tests/mock.rs", custom_origin)]
//!
//! Now the aggregated parameter needs to be injected into the pallet config:
#![doc = docify::embed!("src/tests/mock.rs", impl_config)]
//!
//! As last step, the parameters can now be used in other pallets 🙌
#![doc = docify::embed!("src/tests/mock.rs", usage)]
//!
//! ### Examples Usage
//!
//! Now to demonstrate how the values can be updated:
#![doc = docify::embed!("src/tests/unit.rs", set_parameters_example)]
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
	dynamic_params::{AggregratedKeyValue, IntoKey, Key, RuntimeParameterStore, TryIntoKey},
	EnsureOriginWithArg,
};

mod benchmarking;
#[cfg(test)]
mod tests;
mod weights;

pub use pallet::*;
pub use weights::WeightInfo;

/// The key type of a parameter.
type KeyOf<T> = <<T as Config>::RuntimeParameters as AggregratedKeyValue>::AggregratedKey;

/// The value type of a parameter.
type ValueOf<T> = <<T as Config>::RuntimeParameters as AggregratedKeyValue>::AggregratedValue;

#[frame_support::pallet]
pub mod pallet {
	use super::*;

	#[pallet::config(with_default)]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		#[pallet::no_default_bounds]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// The overarching KV type of the parameters.
		///
		/// Usually created by [`frame_support::dynamic_params`] or equivalent.
		#[pallet::no_default_bounds]
		type RuntimeParameters: AggregratedKeyValue;

		/// The origin which may update a parameter.
		///
		/// The key of the parameter is passed in as second argument to allow for fine grained
		/// control.
		#[pallet::no_default_bounds]
		type AdminOrigin: EnsureOriginWithArg<Self::RuntimeOrigin, KeyOf<Self>>;

		/// Weight information for extrinsics in this module.
		type WeightInfo: WeightInfo;

		/// Provides a default KV since the type is otherwise in-constructable.
		#[pallet::no_default]
		#[cfg(feature = "runtime-benchmarks")]
		type BenchmarkingDefault: Get<Self::RuntimeParameters>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(crate) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A Parameter was set.
		Updated {
			/// The Key-Value pair that was set.
			key_value: T::RuntimeParameters,
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
			key_value: T::RuntimeParameters,
		) -> DispatchResult {
			let (key, value) = key_value.clone().into_parts();

			T::AdminOrigin::ensure_origin(origin, &key)?;

			Parameters::<T>::mutate(key, |v| *v = value);

			Self::deposit_event(Event::Updated { key_value });

			Ok(())
		}
	}
	/// Default implementations of [`DefaultConfig`], which can be used to implement [`Config`].
	pub mod config_preludes {
		use super::*;
		use frame_support::derive_impl;

		/// A configuration for testing.
		pub struct TestDefaultConfig;

		#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig, no_aggregated_types)]
		impl frame_system::DefaultConfig for TestDefaultConfig {}

		#[frame_support::register_default_impl(TestDefaultConfig)]
		impl DefaultConfig for TestDefaultConfig {
			#[inject_runtime_type]
			type RuntimeEvent = ();
			#[inject_runtime_type]
			type RuntimeParameters = ();

			type AdminOrigin = frame_support::traits::AsEnsureOriginWithArg<
				frame_system::EnsureRoot<Self::AccountId>,
			>;

			type WeightInfo = ();
		}
	}
}

impl<T: Config> RuntimeParameterStore for Pallet<T> {
	type AggregratedKeyValue = T::RuntimeParameters;

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
