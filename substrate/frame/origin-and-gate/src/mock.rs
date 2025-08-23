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

use crate::{self as pallet_origin_and_gate, AndGate, CompositeOriginId};
// Import pallet types directly
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU64, EnsureOrigin, Everything},
};
use frame_system::{self as system, RawOrigin};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage, Perbill,
};

type Block = frame_system::mocking::MockBlock<Test>;
pub type AccountId = u64;
pub type BlockNumber = u64;

// Custom origins for testing
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CustomOriginType {
	Alice,
	Bob,
	None,
}

impl Default for CustomOriginType {
	fn default() -> Self {
		Self::None
	}
}

pub const ROOT: AccountId = 0;
pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;
pub const CHARLIE: AccountId = 3;
pub const DAVE: AccountId = 4;
pub const TECH_FELLOWSHIP: AccountId = 4;

// Origin identifiers using CompositeOriginId
pub const ROOT_ORIGIN_ID: CompositeOriginId = CompositeOriginId { collective_id: 0, role: 0 };
pub const ALICE_ORIGIN_ID: CompositeOriginId = CompositeOriginId { collective_id: 1, role: 0 };
pub const BOB_ORIGIN_ID: CompositeOriginId = CompositeOriginId { collective_id: 2, role: 0 };
pub const CHARLIE_ORIGIN_ID: CompositeOriginId = CompositeOriginId { collective_id: 3, role: 0 };
pub const DAVE_ORIGIN_ID: CompositeOriginId = CompositeOriginId { collective_id: 4, role: 0 };
pub const TECH_FELLOWSHIP_ORIGIN_ID: CompositeOriginId =
	CompositeOriginId { collective_id: 4, role: 0 };

// Custom origin checks if sender is Alice
pub struct AliceOrigin;

impl AliceOrigin {
	pub fn origin_type() -> CustomOriginType {
		CustomOriginType::Alice
	}
}

impl EnsureOrigin<RuntimeOrigin> for AliceOrigin {
	type Success = ();

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		<RuntimeOrigin as Into<Result<RawOrigin<AccountId>, RuntimeOrigin>>>::into(o).and_then(
			|o| match o {
				RawOrigin::Signed(who) if who == ALICE => Ok(()),
				r => Err(RuntimeOrigin::from(r)),
			},
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		let alice_origin = RuntimeOrigin::from(RawOrigin::Signed(ALICE));
		Ok(alice_origin)
	}
}

// Custom origin checks if sender is Bob
pub struct BobOrigin;

impl BobOrigin {
	pub fn origin_type() -> CustomOriginType {
		CustomOriginType::Bob
	}
}

impl EnsureOrigin<RuntimeOrigin> for BobOrigin {
	type Success = ();

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		<RuntimeOrigin as Into<Result<RawOrigin<AccountId>, RuntimeOrigin>>>::into(o).and_then(
			|o| match o {
				RawOrigin::Signed(who) if who == BOB => Ok(()),
				r => Err(RuntimeOrigin::from(r)),
			},
		)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		let bob_origin = RuntimeOrigin::from(RawOrigin::Signed(BOB));
		Ok(bob_origin)
	}
}

// Type aliases for origin wrappers for use in tests
pub type SignedByAlice = frame_system::EnsureSignedBy<AliceOrigin, AccountId>;
pub type SignedByBob = frame_system::EnsureSignedBy<BobOrigin, AccountId>;

// Define custom EnsureOrigin implementations for use in tests
pub type AliceAndBob = AndGate<AliceOrigin, BobOrigin>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		OriginAndGate: pallet_origin_and_gate,
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const MaximumBlockWeight: u64 = 1024;
	pub const MaximumBlockLength: u32 = 2 * 1024;
	pub const AvailableBlockRatio: Perbill = Perbill::one();
	pub const SS58Prefix: u8 = 42;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type BlockHashCount = ConstU64<250>;
	type RuntimeTask = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type ExtensionsWeightInfo = ();
}

impl Clone for RequiredApprovalsCount {
	fn clone(&self) -> Self {
		*self
	}
}

impl Copy for RequiredApprovalsCount {}

pub type OriginId = CompositeOriginId;

parameter_types! {
	pub static RequiredApprovalsCount: u32 = 2;
	pub static ProposalExpiry: BlockNumber = 100;
	// Default retention period for terminal proposals is set to 15 years
	// plus contingency to try to meet longest global regulatory requirement
	// Calculation: 15 years × 365.25 days × 24 hours × 60 minutes × 60 seconds ÷ 12 seconds per block = 39,447,000 blocks
	// Adding 25% safety margin: 39,447,000 × 1.25 = 49,308,750 blocks
	pub static ProposalRetentionPeriodWhenNotCancelled: BlockNumber = 50_000_000;
	// Maximum number of proposals to expire per block
	pub static MaxProposalsToExpirePerBlock: u32 = 10;
	pub static MaxRemarkLength: u32 = 1024;
	pub static MaxStorageIdLength: u32 = 128;
	pub static MaxStorageIdDescriptionLength: u32 = 256;
	pub static MaxStorageIdsPerProposal: u32 = 20;
	pub static MaxRemarksPerProposal: u32 = 50;
}

// Mock for OpenGov integration
pub struct MockReferendaOrigin;

impl EnsureOrigin<RuntimeOrigin> for MockReferendaOrigin {
	type Success = ();

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		// Check if origin is root
		match frame_system::ensure_root(o.clone()) {
			Ok(_) => {
				// Root origin
				Ok(())
			},
			Err(_) => {
				// Not root origin
				Err(o)
			},
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		// Return a root origin which will pass the origin check
		Ok(frame_system::RawOrigin::Root.into())
	}
}

// Mock Technical Fellowship to support integration with OpenGov and collectives
pub struct MockTechnicaFellowshipOrigin;

impl EnsureOrigin<RuntimeOrigin> for MockTechnicaFellowshipOrigin {
	type Success = AccountId;

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		match o.clone().into() {
			Ok(frame_system::RawOrigin::Signed(ref who)) if who == &TECH_FELLOWSHIP => {
				// Only TECH_FELLOWSHIP can act as the Technical Fellowship
				Ok(who.clone())
			},
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Ok(frame_system::RawOrigin::Signed(TECH_FELLOWSHIP).into())
	}
}

// Combined origin type for testing both collective and OpenGov origins
pub type TestCollectiveOrigin = frame_support::traits::EitherOfDiverse<
	frame_system::EnsureRoot<AccountId>,
	frame_support::traits::EitherOfDiverse<MockReferendaOrigin, MockTechnicaFellowshipOrigin>,
>;

impl pallet_origin_and_gate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type RequiredApprovalsCount = RequiredApprovalsCount;
	type Hashing = BlakeTwo256;
	type OriginId = OriginId;
	type MaxRemarkLength = MaxRemarkLength;
	type MaxStorageIdLength = MaxStorageIdLength;
	type MaxStorageIdDescriptionLength = MaxStorageIdDescriptionLength;
	type MaxStorageIdsPerProposal = MaxStorageIdsPerProposal;
	type MaxRemarksPerProposal = MaxRemarksPerProposal;
	// Use the combined origin type that supports both collectives and OpenGov
	type CollectiveOrigin = TestCollectiveOrigin;
	type ProposalExpiry = ProposalExpiry;
	type ProposalRetentionPeriodWhenNotCancelled = ProposalRetentionPeriodWhenNotCancelled;
	type MaxProposalsToExpirePerBlock = MaxProposalsToExpirePerBlock;
	type WeightInfo = ();
}

// Extension trait to add collective() method to RuntimeOrigin
pub trait RuntimeOriginExt {
	fn collective(who: AccountId) -> RuntimeOrigin;
}

impl RuntimeOriginExt for RuntimeOrigin {
	fn collective(who: AccountId) -> RuntimeOrigin {
		if who == TECH_FELLOWSHIP {
			// Use root origin for TECH_FELLOWSHIP
			RuntimeOrigin::root()
		} else if who == ROOT {
			// Use root origin for ROOT
			RuntimeOrigin::root()
		} else {
			// Use signed origin for other accounts
			RuntimeOrigin::signed(who)
		}
	}
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup.
#[cfg(any(feature = "runtime-benchmarks", test))]
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}
