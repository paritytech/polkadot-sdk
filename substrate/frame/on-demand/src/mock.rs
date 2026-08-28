// This file is part of Substrate.

// Copyright (C) Parity Technologies (UK) Ltd.
// SPDX-License-Identifier: Apache-2.0

// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// 	http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! Test mock for the on-demand pallet.

use crate::{self as pallet_on_demand, Config, ParaId, QueueOnDemandOrders};
use frame_support::{
	derive_impl, parameter_types,
	traits::{ConstU32, ConstU64, Hooks},
	PalletId,
};
use sp_runtime::{traits::BlockNumberProvider, BuildStorage};
use std::cell::RefCell;

pub type Balance = u64;
type RelayBlockNumber = u32;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		OnDemand: pallet_on_demand,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

thread_local! {
	/// The current Relay-chain block number, as seen by the pallet.
	pub static RELAY_BLOCK_NUMBER: RefCell<RelayBlockNumber> = const { RefCell::new(0) };
	/// Records every batch handed to [`RecordingOrderQueue`].
	pub static QUEUED_BATCHES: RefCell<Vec<Vec<(ParaId, RelayBlockNumber)>>> =
		const { RefCell::new(Vec::new()) };
}

/// A Relay-chain block number provider the tests can move forward at will.
pub struct MockRelayBlockNumberProvider;
impl BlockNumberProvider for MockRelayBlockNumberProvider {
	type BlockNumber = RelayBlockNumber;

	fn current_block_number() -> Self::BlockNumber {
		RELAY_BLOCK_NUMBER.with(|n| *n.borrow())
	}
}

/// Stands in for the XCM that would carry the batch to the Relay chain.
pub struct RecordingOrderQueue;
impl QueueOnDemandOrders<RelayBlockNumber> for RecordingOrderQueue {
	fn queue_batch(batch: Vec<(ParaId, RelayBlockNumber)>) {
		QUEUED_BATCHES.with(|b| b.borrow_mut().push(batch));
	}
}

parameter_types! {
	pub const OnDemandPalletId: PalletId = PalletId(*b"TsOnDmnd");
}

pub(crate) const BASE_FEE: u64 = 1_000;

impl Config for Test {
	type WeightInfo = ();
	type Currency = Balances;
	type RelayBlockNumberProvider = MockRelayBlockNumberProvider;
	type OrderQueue = RecordingOrderQueue;
	type PalletId = OnDemandPalletId;
	type DefaultOrderCap = ConstU32<100>;
	type DefaultDrainRatePerBlock = ConstU32<1>;
	type DefaultPriceStep = ConstU32<3>;
	type DefaultBaseFee = ConstU64<BASE_FEE>;
}

/// Move the Relay chain to `relay_block_number`.
pub fn set_relay_block_number(relay_block_number: RelayBlockNumber) {
	RELAY_BLOCK_NUMBER.with(|n| *n.borrow_mut() = relay_block_number);
}

/// All batches forwarded to the Relay chain so far.
pub fn queued_batches() -> Vec<Vec<(ParaId, RelayBlockNumber)>> {
	QUEUED_BATCHES.with(|b| b.borrow().clone())
}

/// Finalize the current block and start the next one, running the pallet's hooks.
pub fn advance_block() {
	let now = System::block_number();
	OnDemand::on_finalize(now);
	System::set_block_number(now + 1);
	OnDemand::on_initialize(now + 1);
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	set_relay_block_number(0);
	QUEUED_BATCHES.with(|b| b.borrow_mut().clear());

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(1, 1_000_000), (2, 1_000_000), (3, 1_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext: sp_io::TestExternalities = t.into();
	// Get past the genesis block so that events are recorded, and let the pallet initialize.
	ext.execute_with(|| {
		System::set_block_number(1);
		OnDemand::on_initialize(1);
	});
	ext
}
