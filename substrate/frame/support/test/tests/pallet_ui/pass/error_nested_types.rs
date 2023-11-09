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

use codec::{Decode, Encode};
use frame_support::PalletError;

#[frame_support::pallet]
mod pallet {
	#[pallet::config]
	pub trait Config: frame_system::Config {}

	#[pallet::pallet]
	pub struct Pallet<T>(core::marker::PhantomData<T>);

	#[pallet::error]
	pub enum Error<T> {
		CustomError(crate::MyError),
	}
}

#[derive(Encode, Decode, PalletError, scale_info::TypeInfo)]
pub enum MyError {
    Foo,
    Bar,
    Baz(NestedError),
    Struct(MyStruct),
    Wrapper(Wrapper),
}

#[derive(Encode, Decode, PalletError, scale_info::TypeInfo)]
pub enum NestedError {
    Quux
}

#[derive(Encode, Decode, PalletError, scale_info::TypeInfo)]
pub struct MyStruct {
    field: u8,
}

#[derive(Encode, Decode, PalletError, scale_info::TypeInfo)]
pub struct Wrapper(bool);

fn main() {
}
