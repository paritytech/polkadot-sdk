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

//! Mocking utilities for testing in paras_registrar pallet.

#[cfg(test)]
use super::*;
use crate::paras_registrar;
use alloc::collections::btree_map::BTreeMap;
use frame_support::{derive_impl, parameter_types};
use frame_system::limits;
use polkadot_primitives::{Balance, BlockNumber, MAX_CODE_SIZE};
use polkadot_runtime_parachains::{configuration, origin, shared};
use sp_core::{ConstUint, H256};
use sp_io::TestExternalities;
use sp_keyring::Sr25519Keyring;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	transaction_validity::TransactionPriority,
	BuildStorage, Perbill,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlockU32<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Configuration: configuration,
		Parachains: paras,
		ParasShared: shared,
		Registrar: paras_registrar,
		ParachainsOrigin: origin,
	}
);

impl<C> frame_system::offchain::CreateTransactionBase<C> for Test
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type RuntimeCall = RuntimeCall;
}

impl<C> frame_system::offchain::CreateBare<C> for Test
where
	RuntimeCall: From<C>,
{
	fn create_bare(call: Self::RuntimeCall) -> Self::Extrinsic {
		UncheckedExtrinsic::new_bare(call)
	}
}

const NORMAL_RATIO: Perbill = Perbill::from_percent(75);
parameter_types! {
	pub BlockWeights: limits::BlockWeights =
		frame_system::limits::BlockWeights::simple_max(Weight::from_parts(1024, u64::MAX));
	pub BlockLength: limits::BlockLength =
		limits::BlockLength::max_with_normal_ratio(4 * 1024 * 1024, NORMAL_RATIO);
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<u64>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type DbWeight = ();
	type BlockWeights = BlockWeights;
	type BlockLength = BlockLength;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<u128>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
}

impl shared::Config for Test {
	type DisabledValidators = ();
}

impl origin::Config for Test {}

parameter_types! {
	pub const ParasUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
}

impl paras::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = paras::TestWeightInfo;
	type UnsignedPriority = ParasUnsignedPriority;
	type QueueFootprinter = ();
	type NextSessionRotation = crate::mock::TestNextSessionRotation;
	type OnNewHead = ();
	type AssignCoretime = ();
	type Fungible = Balances;
	type CooldownRemovalMultiplier = ConstUint<1>;
	type AuthorizeCurrentCodeOrigin = frame_system::EnsureRoot<u64>;
}

impl configuration::Config for Test {
	type WeightInfo = configuration::TestWeightInfo;
}

parameter_types! {
	pub const ParaDeposit: Balance = 10;
	pub const DataDepositPerByte: Balance = 1;
	pub const MaxRetries: u32 = 3;
}

impl Config for Test {
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type OnSwap = MockSwap;
	type ParaDeposit = ParaDeposit;
	type DataDepositPerByte = DataDepositPerByte;
	type WeightInfo = TestWeightInfo;
}

pub fn new_test_ext() -> TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();

	configuration::GenesisConfig::<Test> {
		config: configuration::HostConfiguration {
			max_code_size: MAX_CODE_SIZE,
			max_head_data_size: 1 * 1024 * 1024, // 1 MB
			..Default::default()
		},
	}
	.assimilate_storage(&mut t)
	.unwrap();

	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10_000_000), (2, 10_000_000), (3, 10_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}

parameter_types! {
	pub static SwapData: BTreeMap<ParaId, u64> = BTreeMap::new();
}

pub struct MockSwap;
impl OnSwap for MockSwap {
	fn on_swap(one: ParaId, other: ParaId) {
		let mut swap_data = SwapData::get();
		let one_data = swap_data.remove(&one).unwrap_or_default();
		let other_data = swap_data.remove(&other).unwrap_or_default();
		swap_data.insert(one, other_data);
		swap_data.insert(other, one_data);
		SwapData::set(swap_data);
	}
}

pub const BLOCKS_PER_SESSION: u32 = 3;

pub const VALIDATORS: &[Sr25519Keyring] = &[
	Sr25519Keyring::Alice,
	Sr25519Keyring::Bob,
	Sr25519Keyring::Charlie,
	Sr25519Keyring::Dave,
	Sr25519Keyring::Ferdie,
];

pub fn run_to_block(n: BlockNumber) {
	// NOTE that this function only simulates modules of interest. Depending on new pallet may
	// require adding it here.
	System::run_to_block_with::<AllPalletsWithSystem>(
		n,
		frame_system::RunToBlockHooks::default().before_finalize(|bn| {
			// Session change every 3 blocks.
			if (bn + 1) % BLOCKS_PER_SESSION == 0 {
				let session_index = shared::CurrentSessionIndex::<Test>::get() + 1;
				let validators_pub_keys = VALIDATORS.iter().map(|v| v.public().into()).collect();

				shared::Pallet::<Test>::set_session_index(session_index);
				shared::Pallet::<Test>::set_active_validators_ascending(validators_pub_keys);

				Parachains::test_on_new_session();
			}
		}),
	);
}

pub fn run_to_session(n: BlockNumber) {
	let block_number = n * BLOCKS_PER_SESSION;
	run_to_block(block_number);
}

pub fn test_genesis_head(size: usize) -> HeadData {
	HeadData(vec![0u8; size])
}

pub fn test_validation_code(size: usize) -> ValidationCode {
	let validation_code = vec![0u8; size as usize];
	ValidationCode(validation_code)
}

pub fn para_origin(id: ParaId) -> RuntimeOrigin {
	polkadot_runtime_parachains::Origin::Parachain(id).into()
}

pub fn max_code_size() -> u32 {
	configuration::ActiveConfig::<Test>::get().max_code_size
}

pub fn max_head_size() -> u32 {
	configuration::ActiveConfig::<Test>::get().max_head_data_size
}
