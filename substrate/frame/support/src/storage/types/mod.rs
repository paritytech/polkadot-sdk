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

//! Storage types to build abstraction on storage, they implements storage traits such as
//! StorageMap and others.
use alloc::vec::Vec;
use codec::FullCodec;
use sp_metadata_ir::{StorageEntryMetadataIR, StorageEntryModifierIR};

mod counted_map;
mod counted_nmap;
mod double_map;
mod key;
mod map;
mod nmap;
mod value;

pub use counted_map::{CountedStorageMap, CountedStorageMapInstance, Counter};
pub use counted_nmap::{CountedStorageNMap, CountedStorageNMapInstance};
pub use double_map::StorageDoubleMap;
pub use key::{
	EncodeLikeTuple, HasKeyPrefix, HasReversibleKeyPrefix, Key, KeyGenerator,
	KeyGeneratorMaxEncodedLen, ReversibleKeyGenerator, TupleToEncodedIter,
};
pub use map::StorageMap;
pub use nmap::StorageNMap;
pub use value::StorageValue;

/// Trait implementing how the storage optional value is converted into the queried type.
///
/// It is implemented most notable by:
///
/// * [`OptionQuery`] which converts an optional value to an optional value, used when querying
///   storage returns an optional value.
/// * [`ResultQuery`] which converts an optional value to a result value, used when querying storage
///   returns a result value.
/// * [`ValueQuery`] which converts an optional value to a value, used when querying storage returns
///   a value.
///
/// ## Example
#[doc = docify::embed!("src/storage/types/mod.rs", value_query_examples)]
pub trait QueryKindTrait<Value, OnEmpty> {
	/// Metadata for the storage kind.
	const METADATA: StorageEntryModifierIR;

	/// Type returned on query
	type Query: FullCodec + 'static;

	/// Convert an optional value (i.e. some if trie contains the value or none otherwise) to the
	/// query.
	fn from_optional_value_to_query(v: Option<Value>) -> Self::Query;

	/// Convert a query to an optional value.
	fn from_query_to_optional_value(v: Self::Query) -> Option<Value>;
}

/// Implements [`QueryKindTrait`] with `Query` type being `Option<_>`.
///
/// NOTE: it doesn't support a generic `OnEmpty`. This means only `None` can be returned when no
/// value is found. To use another `OnEmpty` implementation, `ValueQuery` can be used instead.
pub struct OptionQuery;
impl<Value> QueryKindTrait<Value, crate::traits::GetDefault> for OptionQuery
where
	Value: FullCodec + 'static,
{
	const METADATA: StorageEntryModifierIR = StorageEntryModifierIR::Optional;

	type Query = Option<Value>;

	fn from_optional_value_to_query(v: Option<Value>) -> Self::Query {
		// NOTE: OnEmpty is fixed to GetDefault, thus it returns `None` on no value.
		v
	}

	fn from_query_to_optional_value(v: Self::Query) -> Option<Value> {
		v
	}
}

/// Implements [`QueryKindTrait`] with `Query` type being `Result<Value, PalletError>`.
pub struct ResultQuery<Error>(core::marker::PhantomData<Error>);
impl<Value, Error, OnEmpty> QueryKindTrait<Value, OnEmpty> for ResultQuery<Error>
where
	Value: FullCodec + 'static,
	Error: FullCodec + 'static,
	OnEmpty: crate::traits::Get<Result<Value, Error>>,
{
	const METADATA: StorageEntryModifierIR = StorageEntryModifierIR::Optional;

	type Query = Result<Value, Error>;

	fn from_optional_value_to_query(v: Option<Value>) -> Self::Query {
		match v {
			Some(v) => Ok(v),
			None => OnEmpty::get(),
		}
	}

	fn from_query_to_optional_value(v: Self::Query) -> Option<Value> {
		v.ok()
	}
}

/// Implements [`QueryKindTrait`] with `Query` type being `Value`.
pub struct ValueQuery;
impl<Value, OnEmpty> QueryKindTrait<Value, OnEmpty> for ValueQuery
where
	Value: FullCodec + 'static,
	OnEmpty: crate::traits::Get<Value>,
{
	const METADATA: StorageEntryModifierIR = StorageEntryModifierIR::Default;

	type Query = Value;

	fn from_optional_value_to_query(v: Option<Value>) -> Self::Query {
		v.unwrap_or_else(|| OnEmpty::get())
	}

	fn from_query_to_optional_value(v: Self::Query) -> Option<Value> {
		Some(v)
	}
}

/// Build the metadata of a storage.
///
/// Implemented by each of the storage types: value, map, countedmap, doublemap and nmap.
pub trait StorageEntryMetadataBuilder {
	/// Build into `entries` the storage metadata entries of a storage given some `docs`.
	fn build_metadata(doc: Vec<&'static str>, entries: &mut Vec<StorageEntryMetadataIR>);
}

#[cfg(test)]
mod test {
	use super::*;
	use crate::{
		assert_noop, assert_ok,
		storage::{types::ValueQuery, unhashed},
		traits::{Get, StorageInstance},
	};
	use codec::Encode;
	use sp_io::TestExternalities;
	use sp_runtime::{generic, traits::BlakeTwo256, BuildStorage};

	#[crate::pallet]
	pub mod frame_system {
		#[allow(unused)]
		use super::{frame_system, frame_system::pallet_prelude::*};
		pub use crate::dispatch::RawOrigin;
		use crate::pallet_prelude::*;

		#[pallet::pallet]
		pub struct Pallet<T>(_);

		#[pallet::config]
		#[pallet::disable_frame_system_supertrait_check]
		pub trait Config: 'static {
			type Block: sp_runtime::traits::Block;
			type AccountId;
			type BaseCallFilter: crate::traits::Contains<Self::RuntimeCall>;
			type RuntimeOrigin;
			type RuntimeCall;
			type RuntimeTask;
			type PalletInfo: crate::traits::PalletInfo;
			type DbWeight: Get<crate::weights::RuntimeDbWeight>;
		}

		#[pallet::origin]
		pub type Origin<T> = RawOrigin<<T as Config>::AccountId>;

		#[pallet::error]
		pub enum Error<T> {
			/// Required by construct_runtime
			CallFiltered,
		}

		#[pallet::call]
		impl<T: Config> Pallet<T> {}

		#[pallet::storage]
		pub type Value<T> = StorageValue<_, (u64, u64), ValueQuery>;

		#[pallet::storage]
		pub type Map<T> = StorageMap<_, Blake2_128Concat, u16, u64, ValueQuery>;

		#[pallet::storage]
		pub type NumberMap<T> = StorageMap<_, Identity, u32, u64, ValueQuery>;

		#[pallet::storage]
		pub type DoubleMap<T> =
			StorageDoubleMap<_, Blake2_128Concat, u16, Twox64Concat, u32, u64, ValueQuery>;

		#[pallet::storage]
		pub type NMap<T> = StorageNMap<
			_,
			(storage::Key<Blake2_128Concat, u16>, storage::Key<Twox64Concat, u32>),
			u64,
			ValueQuery,
		>;

		pub mod pallet_prelude {
			pub type OriginFor<T> = <T as super::Config>::RuntimeOrigin;

			pub type HeaderFor<T> =
				<<T as super::Config>::Block as sp_runtime::traits::HeaderProvider>::HeaderT;

			pub type BlockNumberFor<T> = <HeaderFor<T> as sp_runtime::traits::Header>::Number;
		}
	}

	type BlockNumber = u32;
	type AccountId = u32;
	type Header = generic::Header<BlockNumber, BlakeTwo256>;
	type UncheckedExtrinsic = generic::UncheckedExtrinsic<u32, RuntimeCall, (), ()>;
	type Block = generic::Block<Header, UncheckedExtrinsic>;

	crate::construct_runtime!(
		pub enum Runtime
		{
			System: self::frame_system,
		}
	);

	impl self::frame_system::Config for Runtime {
		type AccountId = AccountId;
		type Block = Block;
		type BaseCallFilter = crate::traits::Everything;
		type RuntimeOrigin = RuntimeOrigin;
		type RuntimeCall = RuntimeCall;
		type RuntimeTask = RuntimeTask;
		type PalletInfo = PalletInfo;
		type DbWeight = ();
	}

	pub fn key_before_prefix(mut prefix: Vec<u8>) -> Vec<u8> {
		let last = prefix.iter_mut().last().unwrap();
		assert_ne!(*last, 0, "mock function not implemented for this prefix");
		*last -= 1;
		prefix
	}

	pub fn key_after_prefix(mut prefix: Vec<u8>) -> Vec<u8> {
		let last = prefix.iter_mut().last().unwrap();
		assert_ne!(*last, 255, "mock function not implemented for this prefix");
		*last += 1;
		prefix
	}

	struct Prefix;
	impl StorageInstance for Prefix {
		fn pallet_prefix() -> &'static str {
			"test"
		}
		const STORAGE_PREFIX: &'static str = "foo";
	}

	#[docify::export]
	#[test]
	pub fn value_query_examples() {
		/// Custom default impl to be used with `ValueQuery`.
		struct UniverseSecret;
		impl Get<u32> for UniverseSecret {
			fn get() -> u32 {
				42
			}
		}

		/// Custom default impl to be used with `ResultQuery`.
		struct GetDefaultForResult;
		impl Get<Result<u32, ()>> for GetDefaultForResult {
			fn get() -> Result<u32, ()> {
				Err(())
			}
		}

		type A = StorageValue<Prefix, u32, ValueQuery>;
		type B = StorageValue<Prefix, u32, OptionQuery>;
		type C = StorageValue<Prefix, u32, ResultQuery<()>, GetDefaultForResult>;
		type D = StorageValue<Prefix, u32, ValueQuery, UniverseSecret>;

		TestExternalities::default().execute_with(|| {
			// normal value query returns default
			assert_eq!(A::get(), 0);

			// option query returns none
			assert_eq!(B::get(), None);

			// result query returns error
			assert_eq!(C::get(), Err(()));

			// value query with custom on empty returns 42
			assert_eq!(D::get(), 42);
		});
	}

	#[test]
	fn value_translate_works() {
		let t = RuntimeGenesisConfig::default().build_storage().unwrap();
		TestExternalities::new(t).execute_with(|| {
			type Value = self::frame_system::Value<Runtime>;

			// put the old value `1111u32` in the storage.
			let key = Value::storage_value_final_key();
			unhashed::put_raw(&key, &1111u32.encode());

			// translate
			let translate_fn = |old: Option<u32>| -> Option<(u64, u64)> {
				old.map(|o| (o.into(), (o * 2).into()))
			};
			let res = Value::translate(translate_fn);
			debug_assert!(res.is_ok());

			// new storage should be `(1111, 1111 * 2)`
			assert_eq!(Value::get(), (1111, 2222));
		})
	}

	#[test]
	fn map_translate_works() {
		let t = RuntimeGenesisConfig::default().build_storage().unwrap();
		TestExternalities::new(t).execute_with(|| {
			type NumberMap = self::frame_system::NumberMap<Runtime>;

			// start with a map of u32 -> u64.
			for i in 0u32..100u32 {
				unhashed::put(&NumberMap::hashed_key_for(&i), &(i as u64));
			}

			assert_eq!(
				NumberMap::iter().collect::<Vec<_>>(),
				(0..100).map(|x| (x as u32, x as u64)).collect::<Vec<_>>(),
			);

			// do translation.
			NumberMap::translate(
				|k: u32, v: u64| if k % 2 == 0 { Some((k as u64) << 32 | v) } else { None },
			);

			assert_eq!(
				NumberMap::iter().collect::<Vec<_>>(),
				(0..50u32)
					.map(|x| x * 2)
					.map(|x| (x, (x as u64) << 32 | x as u64))
					.collect::<Vec<_>>(),
			);
		})
	}

	#[test]
	fn try_mutate_works() {
		let t = RuntimeGenesisConfig::default().build_storage().unwrap();
		TestExternalities::new(t).execute_with(|| {
			type Value = self::frame_system::Value<Runtime>;
			type NumberMap = self::frame_system::NumberMap<Runtime>;
			type DoubleMap = self::frame_system::DoubleMap<Runtime>;

			assert_eq!(Value::get(), (0, 0));
			assert_eq!(NumberMap::get(0), 0);
			assert_eq!(DoubleMap::get(0, 0), 0);

			// `assert_noop` ensures that the state does not change
			assert_noop!(
				Value::try_mutate(|value| -> Result<(), &'static str> {
					*value = (2, 2);
					Err("don't change value")
				}),
				"don't change value"
			);

			assert_noop!(
				NumberMap::try_mutate(0, |value| -> Result<(), &'static str> {
					*value = 4;
					Err("don't change value")
				}),
				"don't change value"
			);

			assert_noop!(
				DoubleMap::try_mutate(0, 0, |value| -> Result<(), &'static str> {
					*value = 6;
					Err("don't change value")
				}),
				"don't change value"
			);

			// Showing this explicitly for clarity
			assert_eq!(Value::get(), (0, 0));
			assert_eq!(NumberMap::get(0), 0);
			assert_eq!(DoubleMap::get(0, 0), 0);

			assert_ok!(Value::try_mutate(|value| -> Result<(), &'static str> {
				*value = (2, 2);
				Ok(())
			}));

			assert_ok!(NumberMap::try_mutate(0, |value| -> Result<(), &'static str> {
				*value = 4;
				Ok(())
			}));

			assert_ok!(DoubleMap::try_mutate(0, 0, |value| -> Result<(), &'static str> {
				*value = 6;
				Ok(())
			}));

			assert_eq!(Value::get(), (2, 2));
			//assert_eq!(NumberMap::get(0), 4);
			assert_eq!(DoubleMap::get(0, 0), 6);
		});
	}
}
