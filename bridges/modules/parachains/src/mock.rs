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

use bp_polkadot_core::parachains::ParaId;
use bp_runtime::Chain;
use frame_support::{construct_runtime, parameter_types, traits::IsInVec, weights::Weight};
use sp_runtime::{
	testing::{Header, H256},
	traits::{BlakeTwo256, Header as HeaderT, IdentityLookup},
	Perbill,
};

use crate as pallet_bridge_parachains;

pub type AccountId = u64;
pub type TestNumber = u64;

pub type RelayBlockHeader =
	sp_runtime::generic::Header<crate::RelayBlockNumber, crate::RelayBlockHasher>;

type Block = frame_system::mocking::MockBlock<TestRuntime>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;

pub const PARAS_PALLET_NAME: &str = "Paras";
pub const UNTRACKED_PARACHAIN_ID: u32 = 10;
pub const MAXIMAL_PARACHAIN_HEAD_SIZE: u32 = 512;

construct_runtime! {
	pub enum TestRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Grandpa1: pallet_bridge_grandpa::<Instance1>::{Pallet},
		Grandpa2: pallet_bridge_grandpa::<Instance2>::{Pallet},
		Parachains: pallet_bridge_parachains::{Call, Pallet, Event<T>},
	}
}

parameter_types! {
	pub const BlockHashCount: TestNumber = 250;
	pub const MaximumBlockWeight: Weight = Weight::from_ref_time(1024);
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
}

impl frame_system::Config for TestRuntime {
	type RuntimeOrigin = RuntimeOrigin;
	type Index = u64;
	type RuntimeCall = RuntimeCall;
	type BlockNumber = TestNumber;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = Header;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
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
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub const MaxRequests: u32 = 2;
	pub const HeadersToKeep: u32 = 5;
	pub const SessionLength: u64 = 5;
	pub const NumValidators: u32 = 5;
}

impl pallet_bridge_grandpa::Config<pallet_bridge_grandpa::Instance1> for TestRuntime {
	type BridgedChain = TestBridgedChain;
	type MaxRequests = MaxRequests;
	type HeadersToKeep = HeadersToKeep;
	type MaxBridgedAuthorities = frame_support::traits::ConstU32<5>;
	type MaxBridgedHeaderSize = frame_support::traits::ConstU32<512>;
	type WeightInfo = ();
}

impl pallet_bridge_grandpa::Config<pallet_bridge_grandpa::Instance2> for TestRuntime {
	type BridgedChain = TestBridgedChain;
	type MaxRequests = MaxRequests;
	type HeadersToKeep = HeadersToKeep;
	type MaxBridgedAuthorities = frame_support::traits::ConstU32<5>;
	type MaxBridgedHeaderSize = frame_support::traits::ConstU32<512>;
	type WeightInfo = ();
}

parameter_types! {
	pub const HeadsToKeep: u32 = 4;
	pub const ParasPalletName: &'static str = PARAS_PALLET_NAME;
	pub GetTenFirstParachains: Vec<ParaId> = (0..10).map(ParaId).collect();
}

impl pallet_bridge_parachains::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type BridgesGrandpaPalletInstance = pallet_bridge_grandpa::Instance1;
	type ParasPalletName = ParasPalletName;
	type TrackedParachains = IsInVec<GetTenFirstParachains>;
	type HeadsToKeep = HeadsToKeep;
	type MaxParaHeadSize = frame_support::traits::ConstU32<MAXIMAL_PARACHAIN_HEAD_SIZE>;
}

#[derive(Debug)]
pub struct TestBridgedChain;

impl Chain for TestBridgedChain {
	type BlockNumber = crate::RelayBlockNumber;
	type Hash = crate::RelayBlockHash;
	type Hasher = crate::RelayBlockHasher;
	type Header = RelayBlockHeader;

	type AccountId = AccountId;
	type Balance = u32;
	type Index = u32;
	type Signature = sp_runtime::testing::TestSignature;

	fn max_extrinsic_size() -> u32 {
		unreachable!()
	}

	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

#[derive(Debug)]
pub struct OtherBridgedChain;

impl Chain for OtherBridgedChain {
	type BlockNumber = u64;
	type Hash = crate::RelayBlockHash;
	type Hasher = crate::RelayBlockHasher;
	type Header = sp_runtime::generic::Header<u64, crate::RelayBlockHasher>;

	type AccountId = AccountId;
	type Balance = u32;
	type Index = u32;
	type Signature = sp_runtime::testing::TestSignature;

	fn max_extrinsic_size() -> u32 {
		unreachable!()
	}

	fn max_extrinsic_weight() -> Weight {
		unreachable!()
	}
}

pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	sp_io::TestExternalities::new(Default::default()).execute_with(|| {
		System::set_block_number(1);
		System::reset_events();
		test()
	})
}

pub fn test_relay_header(
	num: crate::RelayBlockNumber,
	state_root: crate::RelayBlockHash,
) -> RelayBlockHeader {
	RelayBlockHeader::new(
		num,
		Default::default(),
		state_root,
		Default::default(),
		Default::default(),
	)
}
