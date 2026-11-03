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

//! Test runtime for `pallet-hrmp-para`.

use crate::{self as pallet_hrmp_para, SendToRelay};
use core::cell::{Cell, RefCell};
use frame_support::derive_impl;
use hrmp_primitives::MessageToRelay;
use sp_runtime::{traits::ConvertInto, BuildStorage};

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		Balances: pallet_balances,
		HrmpPara: pallet_hrmp_para,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<u128>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = u128;
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
}

thread_local! {
	/// The messages the transport took, oldest first.
	pub static SENT: RefCell<Vec<MessageToRelay>> = const { RefCell::new(Vec::new()) };
	/// Whether the transport refuses every message.
	pub static FAIL_SEND: Cell<bool> = const { Cell::new(false) };
}

/// Records what it takes, unless [`FAIL_SEND`].
pub struct RecordingTransport;
impl SendToRelay for RecordingTransport {
	fn send(message: MessageToRelay) -> Result<(), ()> {
		if FAIL_SEND.get() {
			return Err(());
		}
		SENT.with_borrow_mut(|sent| sent.push(message));
		Ok(())
	}
}

impl pallet_hrmp_para::Config for Test {
	type Currency = Balances;
	type SendToRelay = RecordingTransport;
	// A para's sovereign account is the account with its id.
	type SovereignAccountOf = ConvertInto;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// What each of the paras 1, 2 and 3 has, before any deposit.
pub const ENDOWMENT: u128 = 100;

/// Externalities at block 1 with paras 1, 2 and 3 endowed, and the transport taking every
/// message.
pub fn build_ext() -> sp_io::TestExternalities {
	let mut storage = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: (1..=3).map(|para| (para, ENDOWMENT)).collect(),
		..Default::default()
	}
	.assimilate_storage(&mut storage)
	.unwrap();
	let mut ext: sp_io::TestExternalities = storage.into();
	ext.execute_with(|| {
		System::set_block_number(1);
		SENT.take();
		FAIL_SEND.set(false);
	});
	ext
}

/// Runs `test`, then checks the pallet's invariants.
pub fn new_test_ext(test: impl FnOnce()) {
	build_ext().execute_with(|| {
		test();
		HrmpPara::do_try_state().unwrap();
	});
}
