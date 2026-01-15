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

use alloy_core::sol;
use core::num::NonZeroU16;
use pallet_revive::{
	precompiles::{AddressMatcher, Precompile},
	Config,
};

use core::marker::PhantomData;
use pallet_revive_ui_tests::runtime::Runtime;

sol! {
	interface IPrecompileA {
		function callA() external;
	}
}

pub struct PrecompileA<T>(PhantomData<T>);

impl<T: Config> Precompile for PrecompileA<T> {
	type T = T;
	type Interface = IPrecompileA::IPrecompileACalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZeroU16::new(0x666).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
}

sol! {
	interface IPrecompileB {
		function callB() external;
	}
}

pub struct PrecompileB<T>(PhantomData<T>);

impl<T: Config> Precompile for PrecompileB<T> {
	type T = T;
	type Interface = IPrecompileB::IPrecompileBCalls;
	const MATCHER: AddressMatcher = AddressMatcher::Fixed(NonZeroU16::new(0x666).unwrap());
	const HAS_CONTRACT_INFO: bool = false;
}

const _: (PrecompileA<Runtime>, PrecompileB<Runtime>) =
	(PrecompileA(PhantomData::<Runtime>), PrecompileB(PhantomData::<Runtime>));

const _: () = pallet_revive::precompiles::check_collision_for::<
	Runtime,
	(PrecompileA<Runtime>, PrecompileB<Runtime>),
>();

fn main() {}
