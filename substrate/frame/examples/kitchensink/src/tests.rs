// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: MIT-0

// Permission is hereby granted, free of charge, to any person obtaining a copy of
// this software and associated documentation files (the "Software"), to deal in
// the Software without restriction, including without limitation the rights to
// use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies
// of the Software, and to permit persons to whom the Software is furnished to do
// so, subject to the following conditions:

// The above copyright notice and this permission notice shall be included in all
// copies or substantial portions of the Software.

// THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR
// IMPLIED, INCLUDING BUT NOT LIMITED TO THE WARRANTIES OF MERCHANTABILITY,
// FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE
// AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER
// LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM,
// OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE
// SOFTWARE.

//! Tests for pallet-example-kitchensink.

use crate::*;
use frame_support::{assert_ok, derive_impl, parameter_types, traits::VariantCountOf};
use sp_runtime::BuildStorage;
// Reexport crate as its pallet name for construct_runtime.
use crate as pallet_example_kitchensink;

type Block = frame_system::mocking::MockBlock<Test>;

// For testing the pallet, we construct a mock runtime.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
		Kitchensink: pallet_example_kitchensink::{Pallet, Call, Storage, Config<T>, Event<T>},
	}
);

/// Using a default config for [`frame_system`] in tests. See `default-config` example for more
/// details.
#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
	type WeightInfo = ();
	type FreezeIdentifier = RuntimeFreezeReason;
	type MaxFreezes = VariantCountOf<RuntimeFreezeReason>;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
}

parameter_types! {
	pub const InMetadata: u32 = 30;
}

impl Config for Test {
	type WeightInfo = ();

	type Currency = Balances;
	type InMetadata = InMetadata;

	const FOO: u32 = 100;

	fn some_function() -> u32 {
		5u32
	}
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = RuntimeGenesisConfig {
		// We use default for brevity, but you can configure as desired if needed.
		system: Default::default(),
		balances: Default::default(),
		kitchensink: pallet_example_kitchensink::GenesisConfig { bar: 32, foo: 24 },
	}
	.build_storage()
	.unwrap();
	t.into()
}

#[test]
fn set_foo_works() {
	new_test_ext().execute_with(|| {
		assert_eq!(Foo::<Test>::get(), Some(24)); // From genesis config.

		let val1 = 42;
		assert_ok!(Kitchensink::set_foo(RuntimeOrigin::root(), val1, 2));
		assert_eq!(Foo::<Test>::get(), Some(val1));
	});
}
