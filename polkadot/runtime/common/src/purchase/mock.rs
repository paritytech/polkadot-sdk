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

//! Mocking utilities for testing in purchase pallet.

#[cfg(test)]
use super::*;

use sp_core::{crypto::AccountId32, H256};
use sp_keyring::{Ed25519Keyring, Sr25519Keyring};
// The testing primitives are very useful for avoiding having to work with signatures
// or public keys. `u64` is used as the `AccountId` and no `Signature`s are required.
use crate::purchase;
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{Currency, WithdrawReasons},
};
use sp_runtime::{
	traits::{BlakeTwo256, Identity, IdentityLookup},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Vesting: pallet_vesting,
		Purchase: purchase,
	}
);

type AccountId = AccountId32;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
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
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u64>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub const MinVestedTransfer: u64 = 1;
	pub UnvestedFundsAllowedWithdrawReasons: WithdrawReasons =
		WithdrawReasons::except(WithdrawReasons::TRANSFER | WithdrawReasons::RESERVE);
}

impl pallet_vesting::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type BlockNumberToBalance = Identity;
	type MinVestedTransfer = MinVestedTransfer;
	type WeightInfo = ();
	type UnvestedFundsAllowedWithdrawReasons = UnvestedFundsAllowedWithdrawReasons;
	type BlockNumberProvider = System;
	const MAX_VESTING_SCHEDULES: u32 = 28;
}

parameter_types! {
	pub const MaxStatementLength: u32 =  1_000;
	pub const UnlockedProportion: Permill = Permill::from_percent(10);
	pub const MaxUnlocked: u64 = 10;
}

ord_parameter_types! {
	pub const ValidityOrigin: AccountId = AccountId32::from([0u8; 32]);
	pub const PaymentOrigin: AccountId = AccountId32::from([1u8; 32]);
	pub const ConfigurationOrigin: AccountId = AccountId32::from([2u8; 32]);
}

impl Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type VestingSchedule = Vesting;
	type ValidityOrigin = frame_system::EnsureSignedBy<ValidityOrigin, AccountId>;
	type ConfigurationOrigin = frame_system::EnsureSignedBy<ConfigurationOrigin, AccountId>;
	type MaxStatementLength = MaxStatementLength;
	type UnlockedProportion = UnlockedProportion;
	type MaxUnlocked = MaxUnlocked;
}

// This function basically just builds a genesis storage key/value store according to
// our desired mockup. It also executes our `setup` function which sets up this pallet for use.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| setup());
	ext
}

pub fn setup() {
	let statement = b"Hello, World".to_vec();
	let unlock_block = 100;
	Purchase::set_statement(RuntimeOrigin::signed(configuration_origin()), statement).unwrap();
	Purchase::set_unlock_block(RuntimeOrigin::signed(configuration_origin()), unlock_block)
		.unwrap();
	Purchase::set_payment_account(RuntimeOrigin::signed(configuration_origin()), payment_account())
		.unwrap();
	Balances::make_free_balance_be(&payment_account(), 100_000);
}

pub fn alice() -> AccountId {
	Sr25519Keyring::Alice.to_account_id()
}

pub fn alice_ed25519() -> AccountId {
	Ed25519Keyring::Alice.to_account_id()
}

pub fn bob() -> AccountId {
	Sr25519Keyring::Bob.to_account_id()
}

pub fn alice_signature() -> [u8; 64] {
	// echo -n "Hello, World" | subkey -s sign "bottom drive obey lake curtain smoke basket hold
	// race lonely fit walk//Alice"
	hex_literal::hex!("20e0faffdf4dfe939f2faa560f73b1d01cde8472e2b690b7b40606a374244c3a2e9eb9c8107c10b605138374003af8819bd4387d7c24a66ee9253c2e688ab881")
}

pub fn bob_signature() -> [u8; 64] {
	// echo -n "Hello, World" | subkey -s sign "bottom drive obey lake curtain smoke basket hold
	// race lonely fit walk//Bob"
	hex_literal::hex!("d6d460187ecf530f3ec2d6e3ac91b9d083c8fbd8f1112d92a82e4d84df552d18d338e6da8944eba6e84afaacf8a9850f54e7b53a84530d649be2e0119c7ce889")
}

pub fn alice_signature_ed25519() -> [u8; 64] {
	// echo -n "Hello, World" | subkey -e sign "bottom drive obey lake curtain smoke basket hold
	// race lonely fit walk//Alice"
	hex_literal::hex!("ee3f5a6cbfc12a8f00c18b811dc921b550ddf272354cda4b9a57b1d06213fcd8509f5af18425d39a279d13622f14806c3e978e2163981f2ec1c06e9628460b0e")
}

pub fn validity_origin() -> AccountId {
	ValidityOrigin::get()
}

pub fn configuration_origin() -> AccountId {
	ConfigurationOrigin::get()
}

pub fn payment_account() -> AccountId {
	[42u8; 32].into()
}
