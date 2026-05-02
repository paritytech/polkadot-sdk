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

pub struct Bytes(Vec<u8>);

impl codec::Encode for Bytes {}

define_versioned_type! {
	#[versioned_type(encode_like = "Bytes; Vec<u8>")]
	pub struct PristineCodeV1(pub Bytes);
}

impl codec::Encode for PristineCodeV1 {}

fn assert_encodes_like_pristine_code<T: codec::EncodeLike<PristineCodeV1>>() {}

fn main() {
	assert_encodes_like_pristine_code::<Bytes>();
	assert_encodes_like_pristine_code::<Vec<u8>>();
}
