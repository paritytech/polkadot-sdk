#[cfg(test)]
use super::*;

use crate::{assigned_slots, mock::TestRegistrar, slots};
use frame_support::{derive_impl, parameter_types};
use frame_system::EnsureRoot;
use pallet_balances;
use polkadot_primitives::BlockNumber;
use polkadot_runtime_parachains::{
	configuration as parachains_configuration, paras as parachains_paras,
	shared as parachains_shared,
};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	transaction_validity::TransactionPriority,
	BuildStorage,
};

type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
type Block = frame_system::mocking::MockBlockU32<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Configuration: parachains_configuration,
		ParasShared: parachains_shared,
		Parachains: parachains_paras,
		Slots: slots,
		AssignedSlots: assigned_slots,
	}
);

impl<C> frame_system::offchain::CreateTransactionBase<C> for Test
where
	RuntimeCall: From<C>,
{
	type Extrinsic = UncheckedExtrinsic;
	type RuntimeCall = RuntimeCall;
}

impl<C> frame_system::offchain::CreateInherent<C> for Test
where
	RuntimeCall: From<C>,
{
	fn create_inherent(call: Self::RuntimeCall) -> Self::Extrinsic {
		UncheckedExtrinsic::new_bare(call)
	}
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = u64;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = Block;
	type RuntimeEvent = RuntimeEvent;
	type DbWeight = ();
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

impl parachains_configuration::Config for Test {
	type WeightInfo = parachains_configuration::TestWeightInfo;
}

parameter_types! {
	pub const ParasUnsignedPriority: TransactionPriority = TransactionPriority::max_value();
}

impl parachains_paras::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = parachains_paras::TestWeightInfo;
	type UnsignedPriority = ParasUnsignedPriority;
	type QueueFootprinter = ();
	type NextSessionRotation = crate::mock::TestNextSessionRotation;
	type OnNewHead = ();
	type AssignCoretime = ();
}

impl parachains_shared::Config for Test {
	type DisabledValidators = ();
}

parameter_types! {
	pub const LeasePeriod: BlockNumber = 3;
	pub static LeaseOffset: BlockNumber = 0;
	pub const ParaDeposit: u64 = 1;
}

impl slots::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type Registrar = TestRegistrar<Test>;
	type LeasePeriod = LeasePeriod;
	type LeaseOffset = LeaseOffset;
	type ForceOrigin = EnsureRoot<Self::AccountId>;
	type WeightInfo = crate::slots::TestWeightInfo;
}

parameter_types! {
	pub const PermanentSlotLeasePeriodLength: u32 = 3;
	pub const TemporarySlotLeasePeriodLength: u32 = 2;
	pub const MaxTemporarySlotPerLeasePeriod: u32 = 2;
}

impl assigned_slots::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type AssignSlotOrigin = EnsureRoot<Self::AccountId>;
	type Leaser = Slots;
	type PermanentSlotLeasePeriodLength = PermanentSlotLeasePeriodLength;
	type TemporarySlotLeasePeriodLength = TemporarySlotLeasePeriodLength;
	type MaxTemporarySlotPerLeasePeriod = MaxTemporarySlotPerLeasePeriod;
	type WeightInfo = crate::assigned_slots::TestWeightInfo;
}

// This function basically just builds a genesis storage key/value store according to
// our desired mock up.
pub fn new_test_ext() -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 10), (2, 20), (3, 30), (4, 40), (5, 50), (6, 60)],
	}
	.assimilate_storage(&mut t)
	.unwrap();

	crate::assigned_slots::GenesisConfig::<Test> {
		max_temporary_slots: 6,
		max_permanent_slots: 2,
		_config: Default::default(),
	}
	.assimilate_storage(&mut t)
	.unwrap();

	t.into()
}

pub fn run_to_block(n: BlockNumber) {
	while System::block_number() < n {
		let mut block = System::block_number();
		// on_finalize hooks
		AssignedSlots::on_finalize(block);
		Slots::on_finalize(block);
		Parachains::on_finalize(block);
		ParasShared::on_finalize(block);
		Configuration::on_finalize(block);
		Balances::on_finalize(block);
		System::on_finalize(block);
		// Set next block
		System::set_block_number(block + 1);
		block = System::block_number();
		// on_initialize hooks
		System::on_initialize(block);
		Balances::on_initialize(block);
		Configuration::on_initialize(block);
		ParasShared::on_initialize(block);
		Parachains::on_initialize(block);
		Slots::on_initialize(block);
		AssignedSlots::on_initialize(block);
	}
}
