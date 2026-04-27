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

use pallet_revive_proc_macro::define_versioned_type;

define_versioned_type! {
	pub struct UiStructFieldOverrideV1 {
		pub first: u8,
		pub second: u16,
	}

	#[versioned_type(extend)]
	pub struct UiStructFieldOverrideV2 {
		#[versioned_type(override)]
		pub second: u32,
		pub third: u64,
	}
}

fn main() {
	let _value = UiStructFieldOverrideV2 { first: 1, second: 2, third: 3 };
}
