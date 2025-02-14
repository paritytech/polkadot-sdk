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

//! Pallets for the chain-spec demo runtime.

use alloc::vec::Vec;
use frame::prelude::*;

#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet_bar {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub(super) type InitialAccount<T: Config> = StorageValue<Value = T::AccountId>;

	/// Simple `GenesisConfig`.
	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	#[docify::export(pallet_bar_GenesisConfig)]
	pub struct GenesisConfig<T: Config> {
		pub initial_account: Option<T::AccountId>,
	}

	#[pallet::genesis_build]
	#[docify::export(pallet_bar_build)]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		/// The storage building function that presents a direct mapping of the initial config
		/// values to the storage items.
		fn build(&self) {
			InitialAccount::<T>::set(self.initial_account.clone());
		}
	}
}

/// The sample structure used in `GenesisConfig`.
///
/// This structure does not deny unknown fields. This may lead to some problems.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FooStruct {
	pub field_a: u8,
	pub field_b: u8,
}

/// The sample structure used in `GenesisConfig`.
///
/// This structure does not deny unknown fields. This may lead to some problems.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SomeFooData1 {
	pub a: u8,
	pub b: u8,
}

/// Another sample structure used in `GenesisConfig`.
///
/// The user defined serialization is used.
#[derive(Default, serde::Serialize, serde::Deserialize)]
#[docify::export]
#[serde(deny_unknown_fields, rename_all = "camelCase")]
pub struct SomeFooData2 {
	#[serde(default, with = "sp_core::bytes")]
	pub values: Vec<u8>,
}

/// Sample enum used in `GenesisConfig`.
#[derive(Default, serde::Serialize, serde::Deserialize)]
pub enum FooEnum {
	#[default]
	Data0,
	Data1(SomeFooData1),
	Data2(SomeFooData2),
}

#[docify::export]
#[frame::pallet(dev_mode)]
pub mod pallet_foo {
	use super::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::storage]
	pub type ProcessedEnumValue<T> = StorageValue<Value = u64>;
	#[pallet::storage]
	pub type SomeInteger<T> = StorageValue<Value = u32>;

	/// The more sophisticated structure for conveying initial state.
	#[docify::export(pallet_foo_GenesisConfig)]
	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub some_integer: u32,
		pub some_enum: FooEnum,
		pub some_struct: FooStruct,
		#[serde(skip)]
		pub _phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	#[docify::export(pallet_foo_build)]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		/// The build method that indirectly maps an initial config values into the storage items.
		fn build(&self) {
			let processed_value: u64 = match &self.some_enum {
				FooEnum::Data0 => 0,
				FooEnum::Data1(v) => (v.a + v.b).into(),
				FooEnum::Data2(v) => v.values.iter().map(|v| *v as u64).sum(),
			};
			ProcessedEnumValue::<T>::set(Some(processed_value));
			SomeInteger::<T>::set(Some(self.some_integer));
		}
	}
}
