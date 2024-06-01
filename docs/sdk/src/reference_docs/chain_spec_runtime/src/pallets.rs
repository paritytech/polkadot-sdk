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

	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	#[docify::export(pallet_bar_GenesisConfig)]
	pub struct GenesisConfig<T: Config> {
		pub initial_account: Option<T::AccountId>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		#[docify::export(pallet_bar_build)]
		fn build(&self) {
			InitialAccount::<T>::set(self.initial_account.clone());
		}
	}
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct SomeFooData1 {
	pub a: u8,
	pub b: u8,
}

#[derive(Default, serde::Serialize, serde::Deserialize)]
pub struct SomeFooData2 {
	#[serde(default, with = "sp_core::bytes")]
	pub v: Vec<u8>,
}

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

	#[docify::export(pallet_foo_GenesisConfig)]
	#[pallet::genesis_config]
	#[derive(DefaultNoBound)]
	pub struct GenesisConfig<T: Config> {
		pub some_integer: u32,
		pub some_enum: FooEnum,
		#[serde(skip)]
		_phantom: PhantomData<T>,
	}

	#[pallet::genesis_build]
	impl<T: Config> BuildGenesisConfig for GenesisConfig<T> {
		#[docify::export(pallet_foo_build)]
		fn build(&self) {
			let v: u64 = match &self.some_enum {
				FooEnum::Data0 => 0,
				FooEnum::Data1(v) => (v.a + v.b).into(),
				FooEnum::Data2(v) => v.v.iter().map(|v| *v as u64).sum(),
			};
			ProcessedEnumValue::<T>::set(Some(v));
			SomeInteger::<T>::set(Some(self.some_integer));
		}
	}
}
