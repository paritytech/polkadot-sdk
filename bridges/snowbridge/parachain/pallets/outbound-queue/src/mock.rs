// SPDX-License-Identifier: Apache-2.0
// SPDX-FileCopyrightText: 2023 Snowfork <hello@snowfork.com>
use super::*;

use frame_support::{
	parameter_types,
	traits::{Everything, Hooks},
	weights::IdentityFee,
};

use snowbridge_core::{
	gwei, meth,
	outbound::*,
	pricing::{PricingParameters, Rewards},
	ParaId, PRIMARY_GOVERNANCE_CHANNEL,
};
use sp_core::{ConstU32, ConstU8, H160, H256};
use sp_runtime::{
	traits::{BlakeTwo256, IdentityLookup, Keccak256},
	AccountId32, BuildStorage, FixedU128,
};
use sp_std::marker::PhantomData;

type Block = frame_system::mocking::MockBlock<Test>;
type AccountId = AccountId32;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system::{Pallet, Call, Storage, Event<T>},
		MessageQueue: pallet_message_queue::{Pallet, Call, Storage, Event<T>},
		OutboundQueue: crate::{Pallet, Storage, Event<T>},
	}
);

parameter_types! {
	pub const BlockHashCount: u64 = 250;
}

impl frame_system::Config for Test {
	type BaseCallFilter = Everything;
	type BlockWeights = ();
	type BlockLength = ();
	type RuntimeOrigin = RuntimeOrigin;
	type RuntimeCall = RuntimeCall;
	type RuntimeTask = RuntimeTask;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = BlockHashCount;
	type DbWeight = ();
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = ();
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type SystemWeightInfo = ();
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
	type Nonce = u64;
	type Block = Block;
}

parameter_types! {
	pub const HeapSize: u32 = 32 * 1024;
	pub const MaxStale: u32 = 32;
	pub static ServiceWeight: Option<Weight> = Some(Weight::from_parts(100, 100));
}

impl pallet_message_queue::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MessageProcessor = OutboundQueue;
	type Size = u32;
	type QueueChangeHandler = ();
	type HeapSize = HeapSize;
	type MaxStale = MaxStale;
	type ServiceWeight = ServiceWeight;
	type QueuePausedQuery = ();
}

parameter_types! {
	pub const OwnParaId: ParaId = ParaId::new(1013);
	pub Parameters: PricingParameters<u128> = PricingParameters {
		exchange_rate: FixedU128::from_rational(1, 400),
		fee_per_gas: gwei(20),
		rewards: Rewards { local: DOT, remote: meth(1) }
	};
}

pub const DOT: u128 = 10_000_000_000;

impl crate::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Hashing = Keccak256;
	type MessageQueue = MessageQueue;
	type Decimals = ConstU8<12>;
	type MaxMessagePayloadSize = ConstU32<1024>;
	type MaxMessagesPerBlock = ConstU32<20>;
	type GasMeter = ConstantGasMeter;
	type Balance = u128;
	type PricingParameters = Parameters;
	type Channels = Everything;
	type WeightToFee = IdentityFee<u128>;
	type WeightInfo = ();
}

fn setup() {
	System::set_block_number(1);
}

pub fn new_tester() -> sp_io::TestExternalities {
	let storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(setup);
	ext
}

pub fn run_to_end_of_next_block() {
	// finish current block
	MessageQueue::on_finalize(System::block_number());
	OutboundQueue::on_finalize(System::block_number());
	System::on_finalize(System::block_number());
	// start next block
	System::set_block_number(System::block_number() + 1);
	System::on_initialize(System::block_number());
	OutboundQueue::on_initialize(System::block_number());
	MessageQueue::on_initialize(System::block_number());
	// finish next block
	MessageQueue::on_finalize(System::block_number());
	OutboundQueue::on_finalize(System::block_number());
	System::on_finalize(System::block_number());
}

pub fn mock_governance_message<T>() -> Message
where
	T: Config,
{
	let _marker = PhantomData::<T>; // for clippy

	Message {
		id: None,
		channel_id: PRIMARY_GOVERNANCE_CHANNEL,
		command: Command::Upgrade {
			impl_address: H160::zero(),
			impl_code_hash: H256::zero(),
			initializer: None,
		},
	}
}

// Message should fail validation as it is too large
pub fn mock_invalid_governance_message<T>() -> Message
where
	T: Config,
{
	let _marker = PhantomData::<T>; // for clippy

	Message {
		id: None,
		channel_id: PRIMARY_GOVERNANCE_CHANNEL,
		command: Command::Upgrade {
			impl_address: H160::zero(),
			impl_code_hash: H256::zero(),
			initializer: Some(Initializer {
				params: (0..1000).map(|_| 1u8).collect::<Vec<u8>>(),
				maximum_required_gas: 0,
			}),
		},
	}
}

pub fn mock_message(sibling_para_id: u32) -> Message {
	Message {
		id: None,
		channel_id: ParaId::from(sibling_para_id).into(),
		command: Command::AgentExecute {
			agent_id: Default::default(),
			command: AgentExecuteCommand::TransferToken {
				token: Default::default(),
				recipient: Default::default(),
				amount: 0,
			},
		},
	}
}
