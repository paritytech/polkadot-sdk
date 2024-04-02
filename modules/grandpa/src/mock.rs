// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

// From construct_runtime macro
#![allow(clippy::from_over_into)]

use bp_header_chain::ChainWithGrandpa;
use bp_runtime::{Chain, ChainId};
use frame_support::{
	construct_runtime, derive_impl, parameter_types, traits::Hooks, weights::Weight,
};
use sp_core::sr25519::Signature;

pub type AccountId = u64;
pub type TestHeader = sp_runtime::testing::Header;
pub type TestNumber = u64;

type Block = frame_system::mocking::MockBlock<TestRuntime>;

pub const MAX_BRIDGED_AUTHORITIES: u32 = 5;

use crate as grandpa;

construct_runtime! {
	pub enum TestRuntime
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Grandpa: grandpa::{Pallet, Call, Event<T>},
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for TestRuntime {
	type Block = Block;
}

parameter_types! {
	pub const MaxFreeMandatoryHeadersPerBlock: u32 = 2;
	pub const HeadersToKeep: u32 = 5;
}

impl grandpa::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type BridgedChain = TestBridgedChain;
	type MaxFreeMandatoryHeadersPerBlock = MaxFreeMandatoryHeadersPerBlock;
	type HeadersToKeep = HeadersToKeep;
	type WeightInfo = ();
}

#[derive(Debug)]
pub struct TestBridgedChain;

impl Chain for TestBridgedChain {
	const ID: ChainId = *b"tbch";

	type BlockNumber = frame_system::pallet_prelude::BlockNumberFor<TestRuntime>;
	type Hash = <TestRuntime as frame_system::Config>::Hash;
	type Hasher = <TestRuntime as frame_system::Config>::Hashing;
	type Header = TestHeader;

	type AccountId = AccountId;
	type Balance = u64;
	type Nonce = u64;
	type Signature = Signature;

	fn max_extrinsic_size() -> u32 {
		unreachable!()
	}
	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

impl ChainWithGrandpa for TestBridgedChain {
	const WITH_CHAIN_GRANDPA_PALLET_NAME: &'static str = "";
	const MAX_AUTHORITIES_COUNT: u32 = MAX_BRIDGED_AUTHORITIES;
	const REASONABLE_HEADERS_IN_JUSTIFICATION_ANCESTRY: u32 = 8;
	const MAX_MANDATORY_HEADER_SIZE: u32 = 256;
	const AVERAGE_HEADER_SIZE: u32 = 64;
}

/// Return test externalities to use in tests.
pub fn new_test_ext() -> sp_io::TestExternalities {
	sp_io::TestExternalities::new(Default::default())
}

/// Return test within default test externalities context.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	new_test_ext().execute_with(|| {
		let _ = Grandpa::on_initialize(0);
		test()
	})
}

/// Return test header with given number.
pub fn test_header(num: TestNumber) -> TestHeader {
	// We wrap the call to avoid explicit type annotations in our tests
	bp_test_utils::test_header(num)
}
