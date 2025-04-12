// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

use crate as pallet_ambassador_registration;
use frame_support::{
	parameter_types,
	traits::{ConstU32, ConstU64, ConstU16, EnsureOrigin, Everything},
};
use frame_system as system;
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type Balance = u64;

// Configure mock runtime.
frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		AmbassadorRegistration: pallet_ambassador_registration,
	}
);

pub type Block = frame_system::mocking::MockBlock<Test>;

parameter_types! {
	pub const BlockHashCount: u64 = 250;
	pub const SS58Prefix: u16 = 42;
}

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
	type BlockHashCount = ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ConstU16<42>;
	type OnSetCode = ();
	type MaxConsumers = ConstU32<16>;
	type RuntimeTask = ();
	type ExtensionsWeightInfo = ();
	type SingleBlockMigrations = ();
	type MultiBlockMigrator = ();
	type PreInherents = ();
	type PostInherents = ();
	type PostTransactions = ();
}

parameter_types! {
	pub const ExistentialDeposit: u64 = 1;
	pub const MaxLocks: u32 = 50;
}

impl pallet_balances::Config for Test {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = MaxLocks;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeFreezeReason = ();
	type DoneSlashHandler = ();
}

pub struct MockVerifierOrigin;
impl EnsureOrigin<RuntimeOrigin> for MockVerifierOrigin {
	type Success = u64;
	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		if let Ok(Some(who)) = system::ensure_signed_or_root(o.clone()) {
			if who == 100 {
				return Ok(who);
			}
		}
		Err(o)
	}
}

pub struct MockAdminOrigin;
impl EnsureOrigin<RuntimeOrigin> for MockAdminOrigin {
	type Success = u64;
	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		if let Ok(Some(who)) = system::ensure_signed_or_root(o.clone()) {
			if who == 200 {
				return Ok(who);
			}
		}
		Err(o)
	}
}

impl pallet_ambassador_registration::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type VerifierOrigin = MockVerifierOrigin;
	type AdminOrigin = MockAdminOrigin;
	type WeightInfo = ();
}

// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![
			(1, 100), // User with 100 units
			(2, 200), // Another User with 200 units
			(100, 1000), // Verifier with 1000 units
			(200, 1000), // Admin with 1000 units
			(999, 1000), // Old Admin with 1000 units
		],
		dev_accounts: None,
	}
	.assimilate_storage(&mut t)
	.unwrap();
	t.into()
}
