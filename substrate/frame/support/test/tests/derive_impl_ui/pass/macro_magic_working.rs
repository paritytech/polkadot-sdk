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

#[frame_support::macro_magic::export_tokens]
struct MyCoolStruct {
	field: u32,
}

// create a test receiver since `proc_support` isn't enabled so we're on our own in terms of
// what we can call
macro_rules! receiver {
	($_tokens_var:ident, $($tokens:tt)*) => {
		stringify!($($tokens)*)
	};
}

fn main() {
	let _instance: MyCoolStruct = MyCoolStruct { field: 3 };
	let _str = __export_tokens_tt_my_cool_struct!(tokens, receiver);
	// this compiling demonstrates that macro_magic is working properly
}
