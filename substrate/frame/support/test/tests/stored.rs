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

//! Tests for the `#[stored]` macro.

use codec::{Codec, Decode, Encode, MaxEncodedLen};
use core::fmt::Debug;
use frame_support::stored;
use scale_info::TypeInfo;

pub trait Config {
	type Balance: Clone + PartialEq + Eq + Debug + TypeInfo + Codec + MaxEncodedLen;
	type AccountId: Clone + PartialEq + Eq + Debug + TypeInfo + Codec + MaxEncodedLen;
}

// This type itself doesn't implement the requirement to be stored.
// but the associated types in Config does.
struct NotStored;

impl Config for NotStored {
	type Balance = u8;
	type AccountId = u64;
}

#[stored]
pub struct AccountData<T: Config> {
	pub free: T::Balance,
	pub reserved: T::Balance,
	pub frozen: T::Balance,
}

#[stored]
pub enum Status<T: Config> {
	Active { account: T::AccountId },
	Inactive,
	Pending(T::Balance),
}

/// Helper function to ensure types implement all required storage traits.
fn ensure_storable<T: Clone + PartialEq + Eq + Debug + TypeInfo + Codec + MaxEncodedLen>() {}

#[test]
fn test_stored_struct_implements_required_traits() {
	ensure_storable::<AccountData<NotStored>>();
}

#[test]
fn test_stored_enum_implements_required_traits() {
	ensure_storable::<Status<NotStored>>();
}

