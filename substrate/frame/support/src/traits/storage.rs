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

//! Traits for encoding data related to pallet's storage items.

use codec::{Encode, FullCodec, MaxEncodedLen};
use core::marker::PhantomData;
use impl_trait_for_tuples::impl_for_tuples;
use scale_info::TypeInfo;
pub use sp_core::storage::TrackedStorageKey;
use sp_core::Get;
use sp_runtime::{
	traits::{Convert, Member, Saturating},
	DispatchError, RuntimeDebug,
};
use sp_std::{collections::btree_set::BTreeSet, prelude::*};

/// An instance of a pallet in the storage.
///
/// It is required that these instances are unique, to support multiple instances per pallet in the
/// same runtime!
///
/// E.g. for module MyModule default instance will have prefix "MyModule" and other instances
/// "InstanceNMyModule".
pub trait Instance: 'static {
	/// Unique module prefix. E.g. "InstanceNMyModule" or "MyModule"
	const PREFIX: &'static str;
	/// Unique numerical identifier for an instance.
	const INDEX: u8;
}

// Dummy implementation for `()`.
impl Instance for () {
	const PREFIX: &'static str = "";
	const INDEX: u8 = 0;
}

/// An instance of a storage in a pallet.
///
/// Define an instance for an individual storage inside a pallet.
/// The pallet prefix is used to isolate the storage between pallets, and the storage prefix is
/// used to isolate storages inside a pallet.
///
/// NOTE: These information can be used to define storages in pallet such as a `StorageMap` which
/// can use keys after `twox_128(pallet_prefix())++twox_128(STORAGE_PREFIX)`
pub trait StorageInstance {
	/// Prefix of a pallet to isolate it from other pallets.
	fn pallet_prefix() -> &'static str;

	/// Return the prefix hash of pallet instance.
	///
	/// NOTE: This hash must be `twox_128(pallet_prefix())`.
	/// Should not impl this function by hand. Only use the default or macro generated impls.
	fn pallet_prefix_hash() -> [u8; 16] {
		sp_io::hashing::twox_128(Self::pallet_prefix().as_bytes())
	}

	/// Prefix given to a storage to isolate from other storages in the pallet.
	const STORAGE_PREFIX: &'static str;

	/// Return the prefix hash of storage instance.
	///
	/// NOTE: This hash must be `twox_128(STORAGE_PREFIX)`.
	fn storage_prefix_hash() -> [u8; 16] {
		sp_io::hashing::twox_128(Self::STORAGE_PREFIX.as_bytes())
	}

	/// Return the prefix hash of instance.
	///
	/// NOTE: This hash must be `twox_128(pallet_prefix())++twox_128(STORAGE_PREFIX)`.
	/// Should not impl this function by hand. Only use the default or macro generated impls.
	fn prefix_hash() -> [u8; 32] {
		let mut final_key = [0u8; 32];
		final_key[..16].copy_from_slice(&Self::pallet_prefix_hash());
		final_key[16..].copy_from_slice(&Self::storage_prefix_hash());

		final_key
	}
}

/// Metadata about storage from the runtime.
#[derive(Debug, codec::Encode, codec::Decode, Eq, PartialEq, Clone, scale_info::TypeInfo)]
pub struct StorageInfo {
	/// Encoded string of pallet name.
	pub pallet_name: Vec<u8>,
	/// Encoded string of storage name.
	pub storage_name: Vec<u8>,
	/// The prefix of the storage. All keys after the prefix are considered part of this storage.
	pub prefix: Vec<u8>,
	/// The maximum number of values in the storage, or none if no maximum specified.
	pub max_values: Option<u32>,
	/// The maximum size of key/values in the storage, or none if no maximum specified.
	pub max_size: Option<u32>,
}

/// A trait to give information about storage.
///
/// It can be used to calculate PoV worst case size.
pub trait StorageInfoTrait {
	fn storage_info() -> Vec<StorageInfo>;
}

#[cfg_attr(all(not(feature = "tuples-96"), not(feature = "tuples-128")), impl_for_tuples(64))]
#[cfg_attr(all(feature = "tuples-96", not(feature = "tuples-128")), impl_for_tuples(96))]
#[cfg_attr(feature = "tuples-128", impl_for_tuples(128))]
impl StorageInfoTrait for Tuple {
	fn storage_info() -> Vec<StorageInfo> {
		let mut res = vec![];
		for_tuples!( #( res.extend_from_slice(&Tuple::storage_info()); )* );
		res
	}
}

/// Similar to [`StorageInfoTrait`], a trait to give partial information about storage.
///
/// This is useful when a type can give some partial information with its generic parameter doesn't
/// implement some bounds.
pub trait PartialStorageInfoTrait {
	fn partial_storage_info() -> Vec<StorageInfo>;
}

/// Allows a pallet to specify storage keys to whitelist during benchmarking.
/// This means those keys will be excluded from the benchmarking performance
/// calculation.
pub trait WhitelistedStorageKeys {
	/// Returns a [`Vec<TrackedStorageKey>`] indicating the storage keys that
	/// should be whitelisted during benchmarking. This means that those keys
	/// will be excluded from the benchmarking performance calculation.
	fn whitelisted_storage_keys() -> Vec<TrackedStorageKey>;
}

#[cfg_attr(all(not(feature = "tuples-96"), not(feature = "tuples-128")), impl_for_tuples(64))]
#[cfg_attr(all(feature = "tuples-96", not(feature = "tuples-128")), impl_for_tuples(96))]
#[cfg_attr(feature = "tuples-128", impl_for_tuples(128))]
impl WhitelistedStorageKeys for Tuple {
	fn whitelisted_storage_keys() -> Vec<TrackedStorageKey> {
		// de-duplicate the storage keys
		let mut combined_keys: BTreeSet<TrackedStorageKey> = BTreeSet::new();
		for_tuples!( #(
			for storage_key in Tuple::whitelisted_storage_keys() {
				combined_keys.insert(storage_key);
			}
		 )* );
		combined_keys.into_iter().collect::<Vec<_>>()
	}
}

/// The resource footprint of a bunch of blobs. We assume only the number of blobs and their total
/// size in bytes matter.
#[derive(Default, Copy, Clone, Eq, PartialEq, RuntimeDebug)]
pub struct Footprint {
	/// The number of blobs.
	pub count: u64,
	/// The total size of the blobs in bytes.
	pub size: u64,
}

impl Footprint {
	pub fn from_parts(items: usize, len: usize) -> Self {
		Self { count: items as u64, size: len as u64 }
	}

	pub fn from_encodable(e: impl Encode) -> Self {
		Self::from_parts(1, e.encoded_size())
	}
}

/// A storage price that increases linearly with the number of elements and their size.
pub struct LinearStoragePrice<Base, Slope, Balance>(PhantomData<(Base, Slope, Balance)>);
impl<Base, Slope, Balance> Convert<Footprint, Balance> for LinearStoragePrice<Base, Slope, Balance>
where
	Base: Get<Balance>,
	Slope: Get<Balance>,
	Balance: From<u64> + sp_runtime::Saturating,
{
	fn convert(a: Footprint) -> Balance {
		let s: Balance = (a.count.saturating_mul(a.size)).into();
		s.saturating_mul(Slope::get()).saturating_add(Base::get())
	}
}

/// Some sort of cost taken from account temporarily in order to offset the cost to the chain of
/// holding some data [`Footprint`] in state.
///
/// The cost may be increased, reduced or dropped entirely as the footprint changes.
///
/// A single ticket corresponding to some particular datum held in storage. This is an opaque
/// type, but must itself be stored and generally it should be placed alongside whatever data
/// the ticket was created for.
///
/// While not technically a linear type owing to the need for `FullCodec`, *this should be
/// treated as one*. Don't type to duplicate it, and remember to drop it when you're done with
/// it.
#[must_use]
pub trait Consideration<AccountId>: Member + FullCodec + TypeInfo + MaxEncodedLen {
	/// Create a ticket for the `new` footprint attributable to `who`. This ticket *must* ultimately
	/// be consumed through `update` or `drop` once the footprint changes or is removed.
	fn new(who: &AccountId, new: Footprint) -> Result<Self, DispatchError>;

	/// Optionally consume an old ticket and alter the footprint, enforcing the new cost to `who`
	/// and returning the new ticket (or an error if there was an issue).
	///
	/// For creating tickets and dropping them, you can use the simpler `new` and `drop` instead.
	fn update(self, who: &AccountId, new: Footprint) -> Result<Self, DispatchError>;

	/// Consume a ticket for some `old` footprint attributable to `who` which should now been freed.
	fn drop(self, who: &AccountId) -> Result<(), DispatchError>;

	/// Consume a ticket for some `old` footprint attributable to `who` which should be sacrificed.
	///
	/// This is infallible. In the general case (and it is left unimplemented), then it is
	/// equivalent to the consideration never being dropped. Cases which can handle this properly
	/// should implement, but it *MUST* rely on the loss of the consideration to the owner.
	fn burn(self, _: &AccountId) {
		let _ = self;
	}
}

impl<A> Consideration<A> for () {
	fn new(_: &A, _: Footprint) -> Result<Self, DispatchError> {
		Ok(())
	}
	fn update(self, _: &A, _: Footprint) -> Result<(), DispatchError> {
		Ok(())
	}
	fn drop(self, _: &A) -> Result<(), DispatchError> {
		Ok(())
	}
}

macro_rules! impl_incrementable {
	($($type:ty),+) => {
		$(
			impl Incrementable for $type {
				fn increment(&self) -> Option<Self> {
					let mut val = self.clone();
					val.saturating_inc();
					Some(val)
				}

				fn initial_value() -> Option<Self> {
					Some(0)
				}
			}
		)+
	};
}

/// A trait representing an incrementable type.
///
/// The `increment` and `initial_value` functions are fallible.
/// They should either both return `Some` with a valid value, or `None`.
pub trait Incrementable
where
	Self: Sized,
{
	/// Increments the value.
	///
	/// Returns `Some` with the incremented value if it is possible, or `None` if it is not.
	fn increment(&self) -> Option<Self>;

	/// Returns the initial value.
	///
	/// Returns `Some` with the initial value if it is available, or `None` if it is not.
	fn initial_value() -> Option<Self>;
}

impl_incrementable!(u8, u16, u32, u64, u128, i8, i16, i32, i64, i128);

#[cfg(test)]
mod tests {
	use super::*;
	use sp_core::ConstU64;

	#[test]
	fn linear_storage_price_works() {
		type Linear = LinearStoragePrice<ConstU64<7>, ConstU64<3>, u64>;
		let p = |count, size| Linear::convert(Footprint { count, size });

		assert_eq!(p(0, 0), 7);
		assert_eq!(p(0, 1), 7);
		assert_eq!(p(1, 0), 7);

		assert_eq!(p(1, 1), 10);
		assert_eq!(p(8, 1), 31);
		assert_eq!(p(1, 8), 31);

		assert_eq!(p(u64::MAX, u64::MAX), u64::MAX);
	}
}
