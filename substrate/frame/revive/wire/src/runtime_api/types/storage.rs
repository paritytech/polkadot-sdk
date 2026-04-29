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

use alloc::vec::Vec;
use codec::{Decode, Encode};
use pallet_revive_proc_macro::define_versioned_type;
use scale_info::TypeInfo;

define_versioned_type! {
	/// Version 1 of a contract storage key.
	#[derive(Debug, Clone, Eq, PartialEq, TypeInfo, Encode, Decode)]
	pub enum StorageKeyV1 {
		/// A fixed-size contract storage key.
		Fixed([u8; 32]),
		/// A variable-size contract storage key.
		Variable(Vec<u8>),
	}
}
