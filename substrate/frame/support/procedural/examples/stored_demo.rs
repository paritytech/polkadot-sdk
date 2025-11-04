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

//! Example demonstrating the `#[stored]` macro.

use codec::{Codec, Decode, Encode, MaxEncodedLen};
use core::{fmt::Debug, marker::PhantomData};
use frame_support::stored;
use scale_info::TypeInfo;

// This is the new way - using the #[stored] macro
#[stored(mel(Votes))]
pub struct TallyNew<Votes, Total> {
	pub ayes: Votes,
	pub nays: Votes,
	pub support: Votes,
	dummy: PhantomData<Total>,
}

// This is the old way - manually writing everything
#[derive(
	frame_support::CloneNoBound,
	frame_support::PartialEqNoBound,
	frame_support::EqNoBound,
	frame_support::RuntimeDebugNoBound,
	TypeInfo,
	Encode,
	Decode,
	codec::DecodeWithMemTracking,
	MaxEncodedLen,
)]
#[scale_info(skip_type_params(Total))]
#[codec(mel_bound(Votes: MaxEncodedLen))]
pub struct TallyOld<Votes: Clone + PartialEq + Eq + Debug + TypeInfo + Codec, Total> {
	pub ayes: Votes,
	pub nays: Votes,
	pub support: Votes,
	dummy: PhantomData<Total>,
}

fn main() {
	println!("The #[stored] macro successfully reduces boilerplate!");
	println!("Compare:");
	println!("  - Old way: {} lines of attributes", 10);
	println!("  - New way: {} line with #[stored(...)]", 1);
}
