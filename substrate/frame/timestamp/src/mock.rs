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

//! Tests Utilities.

use super::*;
use crate as pallet_timestamp;

use frame_support::{derive_impl, parameter_types, traits::ConstU64};
use sp_io::TestExternalities;
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;
type Moment = u64;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Timestamp: pallet_timestamp,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

parameter_types! {
	pub static CapturedMoment: Option<Moment> = None;
}

pub struct MockOnTimestampSet;
impl OnTimestampSet<Moment> for MockOnTimestampSet {
	fn on_timestamp_set(moment: Moment) {
		CapturedMoment::mutate(|x| *x = Some(moment));
	}
}

impl Config for Test {
	type Moment = Moment;
	type OnTimestampSet = MockOnTimestampSet;
	type MinimumPeriod = ConstU64<5>;
	type WeightInfo = ();
}

pub(crate) fn clear_captured_moment() {
	CapturedMoment::mutate(|x| *x = None);
}

pub(crate) fn get_captured_moment() -> Option<Moment> {
	CapturedMoment::get()
}

pub(crate) fn new_test_ext() -> TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	clear_captured_moment();
	TestExternalities::new(t)
}
