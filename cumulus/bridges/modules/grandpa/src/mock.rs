// Copyright 2019-2021 Parity Technologies (UK) Ltd.
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
use bp_runtime::Chain;
use frame_support::{
	construct_runtime, parameter_types,
	traits::{ConstU32, ConstU64, Hooks},
	weights::Weight,
};
use sp_core::sr25519::Signature;
use sp_runtime::{
	testing::{Header, H256},
	traits::{BlakeTwo256, IdentityLookup},
	Perbill,
};

pub type AccountId = u64;
pub type TestHeader = crate::BridgedHeader<TestRuntime, ()>;
pub type TestNumber = crate::BridgedBlockNumber<TestRuntime, ()>;

type Block = frame_system::mocking::MockBlock<TestRuntime>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;

pub const MAX_BRIDGED_AUTHORITIES: u32 = 5;

use crate as grandpa;

construct_runtime! {
	pub enum TestRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config<T>, Storage, Event<T>},
		Grandpa: grandpa::{Pallet, Call, Event<T>},
	}
}

parameter_types! {
	pub const MaximumBlockWeight: Weight = Weight::from_parts(1024, 0);
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

impl frame_system::Config for TestRuntime {
	type RuntimeOrigin = RuntimeOrigin;
	type Index = u64;
	type RuntimeCall = RuntimeCall;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type BaseCallFilter = frame_support::traits::Everything;
	type SystemWeightInfo = ();
	type DbWeight = ();
	type BlockWeights = ();
	type BlockLength = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
}

parameter_types! {
	pub const MaxFreeMandatoryHeadersPerBlock: u32 = 2;
	pub const HeadersToKeep: u32 = 5;
	pub const SessionLength: u64 = 5;
	pub const NumValidators: u32 = 5;
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
	type BlockNumber = <TestRuntime as frame_system::Config>::BlockNumber;
	type Hash = <TestRuntime as frame_system::Config>::Hash;
	type Hasher = <TestRuntime as frame_system::Config>::Hashing;
	type Header = <TestRuntime as frame_system::Config>::Header;

	type AccountId = AccountId;
	type Balance = u64;
	type Index = u64;
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
	const REASONABLE_HEADERS_IN_JUSTIFICATON_ANCESTRY: u32 = 8;
	const MAX_HEADER_SIZE: u32 = 256;
	const AVERAGE_HEADER_SIZE_IN_JUSTIFICATION: u32 = 64;
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
