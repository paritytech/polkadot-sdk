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

//! A runtime with `pallet-broker` on JAM: its `Coretime` is [`crate::JamCoretime`] and the JAM
//! slot is the block number.

use crate as pallet_jam_coretime;
use alloc::vec::Vec;
use core::cell::RefCell;
use frame_support::{
	derive_impl, ord_parameter_types, parameter_types,
	traits::{ConstU32, ConstU64, Hooks},
	PalletId,
};
use frame_system::EnsureSignedBy;
use pallet_broker::{CenterTargetPrice, CoreIndex, TaskId};
use parachain_service_core::{types::MessageId, upward_message::UpwardMessage};
use sp_runtime::{
	traits::{Identity, MaybeConvert},
	BuildStorage,
};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		Broker: pallet_broker,
		JamCoretime: pallet_jam_coretime,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u64>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type AccountStore = System;
}

parameter_types! {
	pub const BrokerPalletId: PalletId = PalletId(*b"py/broke");
	/// The cores whose assignment the Parachain Service rejected.
	pub static RejectedCores: Vec<CoreIndex> = Vec::new();
}

ord_parameter_types! {
	/// The account that passes the broker's `AdminOrigin`.
	pub const Admin: u64 = 1;
}

pub struct SovereignAccountOf;
impl MaybeConvert<TaskId, u64> for SovereignAccountOf {
	fn maybe_convert(task: TaskId) -> Option<u64> {
		Some(task.into())
	}
}

impl pallet_broker::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type OnRevenue = ();
	type TimeslicePeriod = ConstU64<2>;
	type MaxLeasedCores = ConstU32<5>;
	type MaxReservedCores = ConstU32<5>;
	type Coretime = crate::JamCoretime<Test, System>;
	type ConvertBalance = Identity;
	type WeightInfo = ();
	type PalletId = BrokerPalletId;
	type AdminOrigin = EnsureSignedBy<Admin, u64>;
	type SovereignAccountOf = SovereignAccountOf;
	type MaxAutoRenewals = ConstU32<3>;
	type PriceAdapter = CenterTargetPrice<u64>;
	type MinimumCreditPurchase = ConstU64<20>;
}

impl pallet_jam_coretime::Config for Test {
	type RejectedCores = RejectedCores;
}

thread_local! {
	static UPWARD_MESSAGES: RefCell<Vec<(MessageId, UpwardMessage)>> = RefCell::new(Vec::new());
}

/// Record a message the pallet sends to the Parachain Service.
pub fn record_upward_message(message_id: MessageId, message: UpwardMessage) {
	UPWARD_MESSAGES.with(|messages| messages.borrow_mut().push((message_id, message)));
}

/// Take every message sent since the last call, with the id it was sent with.
pub fn take_upward_messages() -> Vec<(MessageId, UpwardMessage)> {
	UPWARD_MESSAGES.with(|messages| core::mem::take(&mut *messages.borrow_mut()))
}

/// Run the blocks up to `b`: the broker's tick, then this pallet's, in runtime order.
pub fn advance_to(b: u64) {
	while System::block_number() < b {
		System::set_block_number(System::block_number() + 1);
		Broker::on_initialize(System::block_number());
		JamCoretime::on_initialize(System::block_number());
	}
}

pub fn new_test_ext() -> sp_io::TestExternalities {
	take_upward_messages();
	RejectedCores::set(Vec::new());
	let storage = frame_system::GenesisConfig::<Test>::default()
		.build_storage()
		.expect("the default genesis config builds; qed");
	sp_io::TestExternalities::from(storage)
}
