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

use frame::deps::{
	frame_support::{construct_runtime, derive_impl},
	frame_system,
};

type Block = frame_system::mocking::MockBlock<Runtime>;

impl frame_support_test_pallet_frame_crate::pallet::Config for Runtime {}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
}

construct_runtime! {
	pub struct Runtime
	{
		System: frame_system::{Pallet, Call, Storage, Config<T>, Event<T>},
		Pallet: frame_support_test_pallet_frame_crate::pallet::{Pallet, Config<T>},
	}
}

fn main() {}
