use crate::{
	constants::currency::deposit, Balance, Balances, RandomnessCollectiveFlip, Runtime,
	RuntimeCall, RuntimeEvent, Timestamp,
};
use frame_support::{
	parameter_types,
	traits::{ConstBool, ConstU32, Nothing},
};
use pallet_contracts::{
	migration::v12, weights::SubstrateWeight, Config, DebugInfo, DefaultAddressGenerator, Frame,
	Schedule,
};
use sp_runtime::Perbill;

pub use parachains_common::AVERAGE_ON_INITIALIZE_RATIO;

// Prints debug output of the `contracts` pallet to stdout if the node is
// started with `-lruntime::contracts=debug`.
pub const CONTRACTS_DEBUG_OUTPUT: DebugInfo = DebugInfo::UnsafeDebug;

parameter_types! {
	pub const DepositPerItem: Balance = deposit(1, 0);
	pub const DepositPerByte: Balance = deposit(0, 1);
	pub const DefaultDepositLimit: Balance = deposit(1024, 1024 * 1024);
	pub MySchedule: Schedule<Runtime> = Default::default();
	pub CodeHashLockupDepositPercent: Perbill = Perbill::from_percent(30);
}

impl Config for Runtime {
	type Time = Timestamp;
	type Randomness = RandomnessCollectiveFlip;
	type Currency = Balances;
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	/// The safest default is to allow no calls at all.
	///
	/// Runtimes should whitelist dispatchables that are allowed to be called from contracts
	/// and make sure they are stable. Dispatchables exposed to contracts are not allowed to
	/// change because that would break already deployed contracts. The `Call` structure itself
	/// is not allowed to change the indices of existing pallets, too.
	type CallFilter = Nothing;
	type DepositPerItem = DepositPerItem;
	type DepositPerByte = DepositPerByte;
	type DefaultDepositLimit = DefaultDepositLimit;
	type WeightPrice = pallet_transaction_payment::Pallet<Self>;
	type WeightInfo = SubstrateWeight<Self>;
	type ChainExtension = ();
	type Schedule = MySchedule;
	type CallStack = [Frame<Self>; 5];
	type AddressGenerator = DefaultAddressGenerator;
	type MaxCodeLen = ConstU32<{ 123 * 1024 }>;
	type MaxStorageKeyLen = ConstU32<128>;
	type UnsafeUnstableInterface = ConstBool<true>;
	type MaxDebugBufferLen = ConstU32<{ 2 * 1024 * 1024 }>;
	type MaxDelegateDependencies = ConstU32<32>;
	type CodeHashLockupDepositPercent = CodeHashLockupDepositPercent;
	type Migrations = (v12::Migration<Runtime>,);
}
