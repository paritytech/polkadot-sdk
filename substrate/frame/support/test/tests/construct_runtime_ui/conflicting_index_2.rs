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

use frame_support::derive_impl;

#[frame_support::pallet(dev_mode)]
pub mod pallet {
	#[pallet::pallet]
	pub struct Pallet<T, I = ()>(_);

	#[pallet::config]
	pub trait Config<I: 'static = ()>: frame_system::Config {}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = frame_system::mocking::MockBlock<Runtime>;
}

impl pallet::Config<pallet::Instance1> for Runtime {}
impl pallet::Config<pallet::Instance2> for Runtime {}
impl pallet::Config<pallet::Instance3> for Runtime {}
impl pallet::Config<pallet::Instance4> for Runtime {}

frame_support::construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Pallet1: pallet::<Instance1> = 5,
		Pallet2: pallet::<Instance2> = 3,
		Pallet3: pallet::<Instance3>,
		Pallet4: pallet::<Instance4>,
	}
);

fn main() {}
