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

//! Test runtime for `pallet-hrmp-relay`.

use crate::{self as pallet_hrmp_relay, SendToPara};
use core::cell::{Cell, RefCell};
use frame_support::{derive_impl, parameter_types, traits::ConstU32, weights::Weight};
use hrmp_primitives::MessageToPara;
use polkadot_primitives::Id as ParaId;
use polkadot_runtime_parachains::{
	hrmp::deposits::{DepositAction, DepositChangeRequest, DepositError, OnDepositSet},
	origin as parachains_origin,
};
use sp_runtime::BuildStorage;

type Block = frame_system::mocking::MockBlock<Test>;

frame_support::construct_runtime!(
	pub enum Test {
		System: frame_system,
		ParachainsOrigin: parachains_origin,
		HrmpRelay: pallet_hrmp_relay,
	}
);

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
}

impl parachains_origin::Config for Test {}

thread_local! {
	/// The messages the transport took, oldest first.
	pub static SENT: RefCell<Vec<MessageToPara>> = const { RefCell::new(Vec::new()) };
	/// How many more messages the transport takes, or `None` for all.
	pub static ACCEPT: Cell<Option<u32>> = const { Cell::new(None) };
	/// The answers handed to `OnDepositSet`, oldest first.
	pub static ANSWERS: RefCell<Vec<(DepositChangeRequest, Result<(), DepositError>)>> =
		const { RefCell::new(Vec::new()) };
}

/// Records what it takes, while [`ACCEPT`] allows.
pub struct RecordingTransport;
impl SendToPara for RecordingTransport {
	fn send(message: MessageToPara) -> Result<(), ()> {
		match ACCEPT.get() {
			Some(0) => return Err(()),
			Some(n) => ACCEPT.set(Some(n - 1)),
			None => (),
		}
		SENT.with_borrow_mut(|sent| sent.push(message));
		Ok(())
	}
}

/// Records every answer. Each action costs a different weight.
pub struct RecordingOnDepositSet;
impl OnDepositSet for RecordingOnDepositSet {
	fn on_deposit_set(request: DepositChangeRequest, result: Result<(), DepositError>) {
		ANSWERS.with_borrow_mut(|answers| answers.push((request, result)));
	}

	fn weight_of_on_deposit_set(action: &DepositAction) -> Weight {
		match action {
			DepositAction::InitOpenChannel { .. } => Weight::from_parts(1, 10),
			DepositAction::InitAndAcceptOpenChannel { .. } => Weight::from_parts(2, 40),
			DepositAction::AcceptOpenChannel { .. } => Weight::from_parts(3, 20),
			DepositAction::PokeChannelDeposits => Weight::from_parts(4, 30),
			DepositAction::Release => Weight::from_parts(100, 100),
		}
	}
}

parameter_types! {
	pub const HolderParaId: ParaId = ParaId::new(1005);
}

impl pallet_hrmp_relay::Config for Test {
	type OnDepositSet = RecordingOnDepositSet;
	type HolderParaId = HolderParaId;
	type SendToPara = RecordingTransport;
	type MaxOutbox = ConstU32<4>;
	type WeightInfo = ();
	#[cfg(feature = "runtime-benchmarks")]
	type BenchmarkHelper = ();
}

/// Externalities at block 1, with the transport taking every message.
pub fn build_ext() -> sp_io::TestExternalities {
	let mut ext: sp_io::TestExternalities =
		frame_system::GenesisConfig::<Test>::default().build_storage().unwrap().into();
	ext.execute_with(|| {
		System::set_block_number(1);
		SENT.take();
		ACCEPT.set(None);
		ANSWERS.take();
	});
	ext
}

/// Runs `test`, then checks the pallet's invariants.
pub fn new_test_ext(test: impl FnOnce()) {
	build_ext().execute_with(|| {
		test();
		HrmpRelay::do_try_state().unwrap();
	});
}
