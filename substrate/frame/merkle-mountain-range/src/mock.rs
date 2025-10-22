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

use crate as pallet_mmr;
use crate::*;

use crate::{
	frame_system::DefaultConfig,
	primitives::{Compact, LeafDataProvider},
};
use codec::{Decode, Encode};
use frame::{
	deps::frame_support::derive_impl,
	prelude::{frame_system, frame_system::config_preludes::TestDefaultConfig},
	testing_prelude::*,
};

type Block = MockBlock<Test>;

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		MMR: pallet_mmr,
	}
);

#[derive_impl(TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl Config for Test {
	const INDEXING_PREFIX: &'static [u8] = b"mmr-";

	type Hashing = Keccak256;
	type LeafData = Compact<Keccak256, (ParentNumberAndHash<Test>, LeafData)>;
	type OnNewRoot = ();
	type BlockHashProvider = DefaultBlockHashProvider<Test>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

#[derive(Encode, Decode, Clone, Default, Eq, PartialEq, Debug)]
pub struct LeafData {
	pub a: u64,
	pub b: Vec<u8>,
}

impl LeafData {
	pub fn new(a: u64) -> Self {
		Self { a, b: Default::default() }
	}
}

parameter_types! {
	pub static LeafDataTestValue: LeafData = Default::default();
}

impl LeafDataProvider for LeafData {
	type LeafData = Self;

	fn leaf_data() -> Self::LeafData {
		LeafDataTestValue::get().clone()
	}
}
