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

//! Types and traits for dynamic parameters.
//!
//! Can be used by 3rd party macros to define dynamic parameters that are compatible with the the
//! `parameters` pallet.

use codec::MaxEncodedLen;
use frame_support::Parameter;

/// A dynamic parameter store across an aggregated KV type.
pub trait RuntimeParameterStore {
	type AggregratedKeyValue: AggregratedKeyValue;

	/// Get the value of a parametrized key.
	///
	/// Should return `None` if no explicit value was set instead of a default.
	fn get<KV, K>(key: K) -> Option<K::Value>
	where
		KV: AggregratedKeyValue,
		K: Key + Into<<KV as AggregratedKeyValue>::Key>,
		<KV as AggregratedKeyValue>::Key: IntoKey<
			<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::Key,
		>,
		<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::Value:
			TryIntoKey<<KV as AggregratedKeyValue>::Value>,
		<KV as AggregratedKeyValue>::Value: TryInto<K::WrappedValue>;
}

/// A dynamic parameter store across a concrete KV type.
pub trait ParameterStore<KV: AggregratedKeyValue> {
	/// Get the value of a parametrized key.
	fn get<K>(key: K) -> Option<K::Value>
	where
		K: Key + Into<<KV as AggregratedKeyValue>::Key>,
		<KV as AggregratedKeyValue>::Value: TryInto<K::WrappedValue>;
}

/// Key of a dynamic parameter.
pub trait Key {
	/// The value that the key is parametrized with.
	type Value;

	/// An opaque representation of `Self::Value`.
	type WrappedValue: Into<Self::Value>;
}

/// The aggregated key-value type of a dynamic parameter store.
pub trait AggregratedKeyValue: Parameter {
	/// The aggregated key type.
	type Key: Parameter + MaxEncodedLen;

	/// The aggregated value type.
	type Value: Parameter + MaxEncodedLen;

	/// Split the aggregated key-value type into its parts.
	fn into_parts(self) -> (Self::Key, Option<Self::Value>);
}

impl AggregratedKeyValue for () {
	type Key = ();
	type Value = ();

	fn into_parts(self) -> (Self::Key, Option<Self::Value>) {
		((), None)
	}
}

/// Allows to create a `ParameterStore` from a `RuntimeParameterStore`.
///
/// This concretization is useful when configuring pallets, since a pallet will require a parameter
/// store for its own KV type and not the aggregated runtime-wide KV type.
pub struct ParameterStoreAdapter<PS, KV>(sp_std::marker::PhantomData<(PS, KV)>);

impl<PS, KV> ParameterStore<KV> for ParameterStoreAdapter<PS, KV>
where
	PS: RuntimeParameterStore,
	KV: AggregratedKeyValue,
	<KV as AggregratedKeyValue>::Key:
		IntoKey<<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::Key>,
	<KV as AggregratedKeyValue>::Value: TryFromKey<
		<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::Value,
	>,
{
	fn get<K>(key: K) -> Option<K::Value>
	where
		K: Key + Into<<KV as AggregratedKeyValue>::Key>,
		<KV as AggregratedKeyValue>::Value: TryInto<K::WrappedValue>,
	{
		PS::get::<KV, K>(key)
	}
}

// workaround for rust bug https://github.com/rust-lang/rust/issues/51445
mod workaround {
	pub trait FromKey<T>: Sized {
		#[must_use]
		fn from_key(value: T) -> Self;
	}

	pub trait IntoKey<T>: Sized {
		#[must_use]
		fn into_key(self) -> T;
	}

	impl<T, U> IntoKey<U> for T
	where
		U: FromKey<T>,
	{
		fn into_key(self) -> U {
			U::from_key(self)
		}
	}

	pub trait TryIntoKey<T>: Sized {
		type Error;

		fn try_into_key(self) -> Result<T, Self::Error>;
	}

	pub trait TryFromKey<T>: Sized {
		type Error;

		fn try_from_key(value: T) -> Result<Self, Self::Error>;
	}

	impl<T, U> TryIntoKey<U> for T
	where
		U: TryFromKey<T>,
	{
		type Error = U::Error;

		fn try_into_key(self) -> Result<U, U::Error> {
			U::try_from_key(self)
		}
	}
}
pub use workaround::*;
