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

use crate::Parameter;

#[doc(hidden)]
pub trait Key {
	type Value;
	type WrappedValue: Into<Self::Value>;
}

#[doc(hidden)]
pub trait AggregratedKeyValue: Parameter {
	type AggregratedKey: Parameter + codec::MaxEncodedLen;
	type AggregratedValue: Parameter + codec::MaxEncodedLen;

	fn into_parts(self) -> (Self::AggregratedKey, Option<Self::AggregratedValue>);
}

// workaround for rust bug https://github.com/rust-lang/rust/issues/51445
pub mod workaround {
	#[doc(hidden)]
	pub trait From2<T>: Sized {
		#[must_use]
		fn from2(value: T) -> Self;
	}

	#[doc(hidden)]
	pub trait Into2<T>: Sized {
		#[must_use]
		fn into2(self) -> T;
	}

	impl<T, U> Into2<U> for T
	where
		U: From2<T>,
	{
		#[inline]
		fn into2(self) -> U {
			U::from2(self)
		}
	}

	#[doc(hidden)]
	pub trait TryInto2<T>: Sized {
		type Error;

		fn try_into2(self) -> Result<T, Self::Error>;
	}

	#[doc(hidden)]
	pub trait TryFrom2<T>: Sized {
		type Error;

		fn try_from2(value: T) -> Result<Self, Self::Error>;
	}

	impl<T, U> TryInto2<U> for T
	where
		U: TryFrom2<T>,
	{
		type Error = U::Error;

		#[inline]
		fn try_into2(self) -> Result<U, U::Error> {
			U::try_from2(self)
		}
	}
}
