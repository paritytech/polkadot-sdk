// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

//! Wrapper for a runtime storage value that checks if value exceeds given maximum
//! during conversion.

use codec::{Decode, Encode, MaxEncodedLen};
use frame_support::traits::Get;
use scale_info::{Type, TypeInfo};
use sp_runtime::RuntimeDebug;
use sp_std::{marker::PhantomData, ops::Deref};

/// Error that is returned when the value size exceeds maximal configured size.
#[derive(RuntimeDebug)]
pub struct MaximalSizeExceededError {
	/// Size of the value.
	pub value_size: usize,
	/// Maximal configured size.
	pub maximal_size: usize,
}

/// A bounded runtime storage value.
#[derive(Clone, Decode, Encode, Eq, PartialEq)]
pub struct BoundedStorageValue<B, V> {
	value: V,
	_phantom: PhantomData<B>,
}

impl<B, V: sp_std::fmt::Debug> sp_std::fmt::Debug for BoundedStorageValue<B, V> {
	fn fmt(&self, fmt: &mut sp_std::fmt::Formatter) -> sp_std::fmt::Result {
		self.value.fmt(fmt)
	}
}

impl<B: Get<u32>, V: Encode> BoundedStorageValue<B, V> {
	/// Construct `BoundedStorageValue` from the underlying `value` with all required checks.
	///
	/// Returns error if value size exceeds given bounds.
	pub fn try_from_inner(value: V) -> Result<Self, MaximalSizeExceededError> {
		// this conversion is heavy (since we do encoding here), so we may want to optimize it later
		// (e.g. by introducing custom Encode implementation, and turning `BoundedStorageValue` into
		// `enum BoundedStorageValue { Decoded(V), Encoded(Vec<u8>) }`)
		let value_size = value.encoded_size();
		let maximal_size = B::get() as usize;
		if value_size > maximal_size {
			Err(MaximalSizeExceededError { value_size, maximal_size })
		} else {
			Ok(BoundedStorageValue { value, _phantom: Default::default() })
		}
	}

	/// Convert into the inner type
	pub fn into_inner(self) -> V {
		self.value
	}
}

impl<B, V> Deref for BoundedStorageValue<B, V> {
	type Target = V;

	fn deref(&self) -> &Self::Target {
		&self.value
	}
}

impl<B: 'static, V: TypeInfo + 'static> TypeInfo for BoundedStorageValue<B, V> {
	type Identity = Self;

	fn type_info() -> Type {
		V::type_info()
	}
}

impl<B: Get<u32>, V: Encode> MaxEncodedLen for BoundedStorageValue<B, V> {
	fn max_encoded_len() -> usize {
		B::get() as usize
	}
}
