// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

//! Mocking utilities for testing with real pallets.

use crate as identity_migrator;

use frame_support::{derive_impl, parameter_types, traits::ConstU32, weights::Weight};
use frame_system::EnsureRoot;
use pallet_identity::{self, legacy::IdentityInfo};
use sp_core::H256;
use sp_io::TestExternalities;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup, Verify},
	AccountId32, BuildStorage, MultiSignature,
};

type Block = frame_system::mocking::MockBlockU32<Test>;
type AccountId = AccountId32;
type Balance = u32;

frame_support::construct_runtime!(
	pub enum Test
	{
		// System Stuff
		System: frame_system,
		Balances: pallet_balances,

		// Migrators
		Identity: pallet_identity,
		IdentityMigrator: identity_migrator,
	}
);

parameter_types! {
	pub const BlockHashCount: u32 = 250;
	pub BlockWeights: frame_system::limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(
			Weight::from_parts(4 * 1024 * 1024, u64::MAX),
		);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = BlockWeights;
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub static ExistentialDeposit: Balance = 1;
	pub const MaxReserves: u32 = 50;
}

impl pallet_balances::Config for Test {
	type MaxLocks = ();
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxReserves = MaxReserves;
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeFreezeReason = RuntimeFreezeReason;
	type FreezeIdentifier = ();
	type MaxFreezes = ConstU32<0>;
}

impl pallet_identity::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type Slashed = ();
	type BasicDeposit = ConstU32<100>;
	type ByteDeposit = ConstU32<10>;
	type SubAccountDeposit = ConstU32<100>;
	type MaxSubAccounts = ConstU32<2>;
	type IdentityInformation = IdentityInfo<ConstU32<2>>;
	type MaxRegistrars = ConstU32<20>;
	type RegistrarOrigin = EnsureRoot<AccountId>;
	type ForceOrigin = EnsureRoot<AccountId>;
	type OffchainSignature = MultiSignature;
	type SigningPublicKey = <MultiSignature as Verify>::Signer;
	type UsernameAuthorityOrigin = EnsureRoot<AccountId>;
	type PendingUsernameExpiration = ConstU32<100>;
	type MaxSuffixLength = ConstU32<7>;
	type MaxUsernameLength = ConstU32<32>;
	type WeightInfo = ();
}

impl identity_migrator::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Reaper = EnsureRoot<AccountId>;
	type ReapIdentityHandler = ();
	type WeightInfo = identity_migrator::TestWeightInfo;
}

/// Create a new set of test externalities.
pub fn new_test_ext() -> TestExternalities {
	frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
}
