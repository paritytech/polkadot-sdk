// Copyright 2019-2021 Parity Technologies (UK) Ltd.
// This file is part of Parity Bridges Common.

// Parity Bridges Common is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Parity Bridges Common is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Parity Bridges Common.  If not, see <http://www.gnu.org/licenses/>.

#![cfg(test)]

use crate as pallet_bridge_relayers;

use bp_messages::{source_chain::ForbidOutboundMessages, target_chain::ForbidInboundMessages};
use bp_relayers::PaymentProcedure;
use frame_support::{parameter_types, weights::RuntimeDbWeight};
use sp_core::H256;
use sp_runtime::{
	testing::Header as SubstrateHeader,
	traits::{BlakeTwo256, IdentityLookup},
};

pub type AccountId = u64;
pub type Balance = u64;

type Block = frame_system::mocking::MockBlock<TestRuntime>;
type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<TestRuntime>;

frame_support::construct_runtime! {
	pub enum TestRuntime where
		Block = Block,
		NodeBlock = Block,
		UncheckedExtrinsic = UncheckedExtrinsic,
	{
		System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
		Balances: pallet_balances::{Pallet, Event<T>},
		Messages: pallet_bridge_messages::{Pallet, Event<T>},
		Relayers: pallet_bridge_relayers::{Pallet, Call, Event<T>},
	}
}

parameter_types! {
	pub const DbWeight: RuntimeDbWeight = RuntimeDbWeight { read: 1, write: 2 };
}

impl frame_system::Config for TestRuntime {
	type RuntimeOrigin = RuntimeOrigin;
	type Index = u64;
	type RuntimeCall = RuntimeCall;
	type BlockNumber = u64;
	type Hash = H256;
	type Hashing = BlakeTwo256;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Header = SubstrateHeader;
	type RuntimeEvent = RuntimeEvent;
	type BlockHashCount = frame_support::traits::ConstU64<250>;
	type Version = ();
	type PalletInfo = PalletInfo;
	type AccountData = pallet_balances::AccountData<Balance>;
	type OnNewAccount = ();
	type OnKilledAccount = ();
	type BaseCallFilter = frame_support::traits::Everything;
	type SystemWeightInfo = ();
	type BlockWeights = ();
	type BlockLength = ();
	type DbWeight = DbWeight;
	type SS58Prefix = ();
	type OnSetCode = ();
	type MaxConsumers = frame_support::traits::ConstU32<16>;
}

impl pallet_balances::Config for TestRuntime {
	type MaxLocks = ();
	type Balance = Balance;
	type DustRemoval = ();
	type RuntimeEvent = RuntimeEvent;
	type ExistentialDeposit = frame_support::traits::ConstU64<1>;
	type AccountStore = frame_system::Pallet<TestRuntime>;
	type WeightInfo = ();
	type MaxReserves = ();
	type ReserveIdentifier = ();
}

parameter_types! {
	pub const TestBridgedChainId: bp_runtime::ChainId = *b"test";
}

// we're not testing messages pallet here, so values in this config might be crazy
impl pallet_bridge_messages::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type WeightInfo = ();
	type MaxMessagesToPruneAtOnce = frame_support::traits::ConstU64<0>;
	type MaxUnrewardedRelayerEntriesAtInboundLane = frame_support::traits::ConstU64<8>;
	type MaxUnconfirmedMessagesAtInboundLane = frame_support::traits::ConstU64<8>;

	type MaximalOutboundPayloadSize = frame_support::traits::ConstU32<1024>;
	type OutboundPayload = ();

	type InboundPayload = ();
	type InboundRelayer = AccountId;

	type TargetHeaderChain = ForbidOutboundMessages;
	type LaneMessageVerifier = ForbidOutboundMessages;
	type MessageDeliveryAndDispatchPayment = ();

	type SourceHeaderChain = ForbidInboundMessages;
	type MessageDispatch = ForbidInboundMessages;
	type BridgedChainId = TestBridgedChainId;
}

impl pallet_bridge_relayers::Config for TestRuntime {
	type RuntimeEvent = RuntimeEvent;
	type Reward = Balance;
	type PaymentProcedure = TestPaymentProcedure;
	type WeightInfo = ();
}

/// Regular relayer that may receive rewards.
pub const REGULAR_RELAYER: AccountId = 1;

/// Relayer that can't receive rewards.
pub const FAILING_RELAYER: AccountId = 2;

/// Payment procedure that rejects payments to the `FAILING_RELAYER`.
pub struct TestPaymentProcedure;

impl PaymentProcedure<AccountId, Balance> for TestPaymentProcedure {
	type Error = ();

	fn pay_reward(relayer: &AccountId, _reward: Balance) -> Result<(), Self::Error> {
		match *relayer {
			FAILING_RELAYER => Err(()),
			_ => Ok(()),
		}
	}
}

/// Run pallet test.
pub fn run_test<T>(test: impl FnOnce() -> T) -> T {
	let t = frame_system::GenesisConfig::default().build_storage::<TestRuntime>().unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(test)
}
