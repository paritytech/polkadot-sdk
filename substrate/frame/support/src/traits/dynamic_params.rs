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

#[doc(hidden)]
pub use codec;
pub use codec as parity_scale_codec;
#[doc(hidden)]
use frame_support::Parameter;
#[doc(hidden)]
pub use paste;
#[doc(hidden)]
pub use scale_info;
pub use sp_runtime::{self, RuntimeDebug};

pub trait RuntimeParameterStore {
	type AggregratedKeyValue: AggregratedKeyValue;

	fn get<KV, K>(key: K) -> Option<K::Value>
	where
		KV: AggregratedKeyValue,
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedKey:
			IntoKey<<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
		<<Self as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue:
			TryIntoKey<<KV as AggregratedKeyValue>::AggregratedValue>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>;
}

pub trait Key {
	type Value;
	type WrappedValue: Into<Self::Value>;
}

pub trait AggregratedKeyValue: Parameter {
	type AggregratedKey: Parameter + codec::MaxEncodedLen;
	type AggregratedValue: Parameter + codec::MaxEncodedLen;

	fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>);
}

pub trait ParameterStore<KV: AggregratedKeyValue> {
	fn get<K>(key: K) -> Option<K::Value>
	where
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>;
}

pub struct ParameterStoreAdapter<PS, KV>(sp_std::marker::PhantomData<(PS, KV)>);

impl<PS, KV> ParameterStore<KV> for ParameterStoreAdapter<PS, KV>
where
	PS: RuntimeParameterStore,
	KV: AggregratedKeyValue,
	<KV as AggregratedKeyValue>::AggregratedKey:
		IntoKey<<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedKey>,
	<KV as AggregratedKeyValue>::AggregratedValue:
		TryFromKey<<<PS as RuntimeParameterStore>::AggregratedKeyValue as AggregratedKeyValue>::AggregratedValue>,
{
	fn get<K>(key: K) -> Option<K::Value>
	where
		K: Key + Into<<KV as AggregratedKeyValue>::AggregratedKey>,
		<KV as AggregratedKeyValue>::AggregratedValue: TryInto<K::WrappedValue>,
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
		#[inline]
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

		#[inline]
		fn try_into_key(self) -> Result<U, U::Error> {
			U::try_from_key(self)
		}
	}
}
pub use workaround::*;
