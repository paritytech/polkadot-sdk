use crate::{
	constants::currency::deposit, Balance, Balances, Call, Event, RandomnessCollectiveFlip,
	Runtime, RuntimeBlockWeights, Timestamp,
};
use frame_support::{
	parameter_types,
	traits::{Nothing, OnRuntimeUpgrade},
	weights::Weight,
};
use pallet_contracts::{
	migration,
	weights::{SubstrateWeight, WeightInfo},
	Config, DefaultAddressGenerator, DefaultContractAccessWeight, Frame, Schedule,
};
pub use parachains_common::AVERAGE_ON_INITIALIZE_RATIO;

// Prints debug output of the `contracts` pallet to stdout if the node is
// started with `-lruntime::contracts=debug`.
pub const CONTRACTS_DEBUG_OUTPUT: bool = true;

parameter_types! {
	pub const DepositPerItem: Balance = deposit(1, 0);
	pub const DepositPerByte: Balance = deposit(0, 1);
	// The lazy deletion runs inside on_initialize.
	pub DeletionWeightLimit: Weight = AVERAGE_ON_INITIALIZE_RATIO *
		RuntimeBlockWeights::get().max_block;
	// The weight needed for decoding the queue should be less or equal than a fifth
	// of the overall weight dedicated to the lazy deletion.
	pub DeletionQueueDepth: u32 = ((DeletionWeightLimit::get() / (
			<Runtime as Config>::WeightInfo::on_initialize_per_queue_item(1) -
			<Runtime as Config>::WeightInfo::on_initialize_per_queue_item(0)
		)) / 5) as u32;
	pub MySchedule: Schedule<Runtime> = Default::default();
}

impl Config for Runtime {
	type Time = Timestamp;
	type Randomness = RandomnessCollectiveFlip;
	type Currency = Balances;
	type Event = Event;
	type Call = Call;
	/// The safest default is to allow no calls at all.
	///
	/// Runtimes should whitelist dispatchables that are allowed to be called from contracts
	/// and make sure they are stable. Dispatchables exposed to contracts are not allowed to
	/// change because that would break already deployed contracts. The `Call` structure itself
	/// is not allowed to change the indices of existing pallets, too.
	type CallFilter = Nothing;
	type DepositPerItem = DepositPerItem;
	type DepositPerByte = DepositPerByte;
	type WeightPrice = pallet_transaction_payment::Pallet<Self>;
	type WeightInfo = SubstrateWeight<Self>;
	type ChainExtension = ();
	type DeletionQueueDepth = DeletionQueueDepth;
	type DeletionWeightLimit = DeletionWeightLimit;
	type Schedule = MySchedule;
	type CallStack = [Frame<Self>; 31];
	type AddressGenerator = DefaultAddressGenerator;
	type ContractAccessWeight = DefaultContractAccessWeight<RuntimeBlockWeights>;
}

pub struct Migrations;
impl OnRuntimeUpgrade for Migrations {
	fn on_runtime_upgrade() -> Weight {
		migration::migrate::<Runtime>()
	}
}
