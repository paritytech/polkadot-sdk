use crate as pallet_multisig_stateful;
use frame_support::{
	parameter_types,
	traits::{ConstU128, ConstU16, ConstU32, ConstU64},
};
use sp_core::H256;
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;
type Balance = u128;
pub type Hash = H256;

// Configure a mock runtime to test the pallet.
frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Multisig: pallet_multisig_stateful,
	}
);

impl frame_system::Config for Test {
	type BaseCallFilter = frame_support::traits::Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type Nonce = u64;
	type Hash = Hash;
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
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type RuntimeTask = ();
}

impl pallet_balances::Config for Test {
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = ConstU128<1>;
	type AccountStore = System;
	type WeightInfo = ();
	type MaxLocks = ConstU32<10>;
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type RuntimeHoldReason = RuntimeHoldReason;
	type FreezeIdentifier = ();
	type MaxFreezes = ConstU32<10>;
	type RuntimeFreezeReason = ();
}

parameter_types! {
	pub static MaxSignatories: u32 = 4; // Adding static makes it easier to set it to different values in tests. MaxSignatories::set(100);
	pub static RemoveProposalsLimit: u8 = 1;
	pub static CreationDeposit: u128 = 2;
	pub static ProposalDeposit: u128 = 1;
}

impl pallet_multisig_stateful::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeCall = RuntimeCall;
	type MaxSignatories = MaxSignatories;
	type RemoveProposalsLimit = RemoveProposalsLimit;
	type CreationDeposit = CreationDeposit;
	type ProposalDeposit = ProposalDeposit;
}

pub(crate) const ALICE: u64 = 1;
pub(crate) const BOB: u64 = 2;
pub(crate) const CHARLIE: u64 = 3;
pub(crate) const DAVE: u64 = 4;
pub(crate) const EVE: u64 = 5;

pub(crate) const INITIAL_BALANCE: u128 = 20; 
// Build genesis storage according to the mock runtime.
pub fn new_test_ext() -> sp_io::TestExternalities {
	// frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into()
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(ALICE, INITIAL_BALANCE), (BOB, INITIAL_BALANCE), (CHARLIE, INITIAL_BALANCE)],
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	// I can add more stuff directly here.
	ext.execute_with(|| System::set_block_number(1));
	ext
}
