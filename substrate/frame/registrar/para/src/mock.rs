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

//! Mock runtime for `pallet-registrar-para`.
//!
//! The relay chain is not modelled here: [`RecordingSender`] captures what would have been sent
//! and can be told to fail, which is all this side needs to be tested on its own. The two halves
//! meeting for real is the job of the `pallet-registrar-test` crate.

use crate::{self as pallet_registrar_para, AssignmentChecker, HoldReason, SendToRelay};
use frame_support::{
	derive_impl, parameter_types,
	traits::{
		fungible::HoldConsideration, ConstBool, ConstU128, ConstU32, ConstantStoragePrice,
		LinearStoragePrice,
	},
};
use registrar_primitives::MessageToRelay;
use sp_runtime::BuildStorage;

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u32;

pub const ALICE: AccountId = 1;
pub const BOB: AccountId = 2;

/// The para id `reserve` hands out first.
pub const FIRST_PARA_ID: u32 = 2000;

pub const PARA_DEPOSIT: Balance = 1_000;
pub const PER_BYTE: Balance = 10;
pub const MIN_CODE_SIZE: u32 = 9;
pub const MAX_CODE_SIZE: u32 = 1_000;
pub const MAX_HEAD_SIZE: u32 = 100;
pub const PENDING_DEADLINE: BlockNumber = 50;
pub const COOLDOWN_COST: Balance = 250;

#[frame_support::runtime]
mod test_runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeFreezeReason,
		RuntimeHoldReason,
		RuntimeSlashReason,
		RuntimeLockId,
		RuntimeTask
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system::Pallet<Test>;

	#[runtime::pallet_index(1)]
	pub type Balances = pallet_balances::Pallet<Test>;

	#[runtime::pallet_index(2)]
	pub type Registrar = pallet_registrar_para::Pallet<Test>;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = frame_system::mocking::MockBlockU32<Test>;
	type AccountId = AccountId;
	type Lookup = sp_runtime::traits::IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type AccountStore = System;
	type ExistentialDeposit = ConstU128<1>;
	type RuntimeHoldReason = RuntimeHoldReason;
}

parameter_types! {
	/// Messages the pallet handed to the transport, oldest first.
	pub static SentMessages: Vec<MessageToRelay<AccountId>> = Vec::new();
	/// When true, the transport refuses everything.
	pub static SendFails: bool = false;
}

/// A [`SendToRelay`] that records instead of sending, and can be made to fail.
pub struct RecordingSender;

impl SendToRelay for RecordingSender {
	type AccountId = AccountId;

	fn send(message: MessageToRelay<Self::AccountId>) -> Result<(), ()> {
		if SendFails::get() {
			return Err(());
		}
		SentMessages::mutate(|sent| sent.push(message));
		Ok(())
	}
}

/// Take everything recorded so far, clearing the log.
pub fn take_sent() -> Vec<MessageToRelay<AccountId>> {
	SentMessages::mutate(core::mem::take)
}

parameter_types! {
	/// Para ids that still hold a coretime assignment.
	pub static AssignedParas: Vec<u32> = Vec::new();
}

parameter_types! {
	/// Signed accounts allowed to act as a para, as `(account, para id)`.
	pub static ParaOriginAccounts: Vec<(AccountId, u32)> = Vec::new();
}

parameter_types! {
	/// Paras the control-plane hook fired for, oldest first.
	pub static ControlChannelsOpened: Vec<u32> = Vec::new();
}

/// Stands in for the HRMP pallet's control-plane channel opening.
///
/// The real implementation cannot fail the caller, so neither does this: it only records.
pub struct RecordingOnRegistered;

impl hrmp_primitives::OnParaRegistered for RecordingOnRegistered {
	fn on_registered(para_id: u32) {
		ControlChannelsOpened::mutate(|opened| opened.push(para_id));
	}
}

/// Every para the control-plane hook fired for since the last call, oldest first.
pub fn take_control_channels() -> Vec<u32> {
	ControlChannelsOpened::take()
}

/// Lets the accounts listed in [`ParaOriginAccounts`] act as their para, standing in for a real
/// XCM origin. An explicit list, not an account range, so no other account (in particular the
/// entropy-derived ones the benchmarks use) can resolve as a para by accident.
pub struct ParaAccounts;

impl frame_support::traits::EnsureOrigin<RuntimeOrigin> for ParaAccounts {
	type Success = u32;

	fn try_origin(o: RuntimeOrigin) -> Result<Self::Success, RuntimeOrigin> {
		let signed: Result<frame_system::RawOrigin<AccountId>, _> = o.clone().into();
		match signed {
			Ok(frame_system::RawOrigin::Signed(who)) => ParaOriginAccounts::get()
				.iter()
				.find(|(account, _)| *account == who)
				.map(|(_, para_id)| *para_id)
				.ok_or(o),
			_ => Err(o),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn try_successful_origin() -> Result<RuntimeOrigin, ()> {
		Err(())
	}
}

/// The origin para `para_id` itself calls with, backed by a fresh stand-in account.
pub fn para_origin(para_id: u32) -> RuntimeOrigin {
	let account = 1_000_000 + para_id as AccountId;
	ParaOriginAccounts::mutate(|paras| {
		if !paras.contains(&(account, para_id)) {
			paras.push((account, para_id));
		}
	});
	RuntimeOrigin::signed(account)
}

/// An [`AssignmentChecker`] backed by the [`AssignedParas`] list.
pub struct MockAssignments;

impl AssignmentChecker for MockAssignments {
	fn has_assignment(para_id: u32) -> bool {
		AssignedParas::get().contains(&para_id)
	}
}

parameter_types! {
	pub const ParaDeposit: Balance = PARA_DEPOSIT;
	pub const DataDepositPerByte: Balance = PER_BYTE;
	pub const ReservationHoldReason: RuntimeHoldReason =
		RuntimeHoldReason::Registrar(HoldReason::ParaIdReservation);
	pub const RegistrationHoldReason: RuntimeHoldReason =
		RuntimeHoldReason::Registrar(HoldReason::Registration);
}

impl pallet_registrar_para::Config for Test {
	type ReservationConsideration = HoldConsideration<
		AccountId,
		Balances,
		ReservationHoldReason,
		ConstantStoragePrice<ParaDeposit, Balance>,
	>;
	type RegistrationConsideration = HoldConsideration<
		AccountId,
		Balances,
		RegistrationHoldReason,
		LinearStoragePrice<ConstU128<0>, DataDepositPerByte, Balance>,
	>;
	type SendToRelay = RecordingSender;
	type AssignmentChecker = MockAssignments;
	// The mock stands in for the coretime host, so the startup check must be satisfied by a
	// checker that can actually report an assignment.
	type RequireAssignmentLock = ConstBool<true>;
	type RelayOrigin = frame_system::EnsureRoot<AccountId>;
	type ParachainOrigin = ParaAccounts;
	type FirstPublicParaId = ConstU32<FIRST_PARA_ID>;
	type MinCodeSize = ConstU32<MIN_CODE_SIZE>;
	type MaxCodeSize = ConstU32<MAX_CODE_SIZE>;
	type MaxHeadDataSize = ConstU32<MAX_HEAD_SIZE>;
	type PendingDeadline = ConstU32<PENDING_DEADLINE>;
	type BlockNumberProvider = System;
	type Fungible = Balances;
	type UpgradeCooldownCost = ConstU128<COOLDOWN_COST>;
	type OnRegistered = RecordingOnRegistered;
	type WeightInfo = ();
}

/// Externalities with Alice and Bob funded, and the message log cleared.
pub fn new_test_ext() -> sp_io::TestExternalities {
	SentMessages::set(Vec::new());
	SendFails::set(false);
	AssignedParas::set(Vec::new());
	ParaOriginAccounts::set(Vec::new());
	ControlChannelsOpened::set(Vec::new());

	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(ALICE, 1_000_000), (BOB, 1_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();

	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// Run `f` against fresh externalities, then assert the pallet's invariants still hold.
///
/// Every test goes through here, so no test can leave storage in a shape the pallet says is
/// impossible. A test that deliberately corrupts storage must put it back before returning.
pub fn build_and_execute(f: impl FnOnce()) {
	new_test_ext().execute_with(|| {
		f();
		assert_try_state_ok();
	});
}

/// Advance to block `n`, so block-number-dependent logic can be exercised.
pub fn run_to_block(n: BlockNumber) {
	while System::block_number() < n {
		System::set_block_number(System::block_number() + 1);
	}
}

/// Assert the pallet's storage invariants hold.
pub fn assert_try_state_ok() {
	frame_support::assert_ok!(crate::Pallet::<Test>::do_try_state());
}

/// Assert the pallet's storage invariants are violated.
///
/// Tests that deliberately corrupt storage must restore it afterwards, so the state they leave
/// behind is one [`assert_try_state_ok`] would still accept.
pub fn assert_try_state_invalid() {
	assert!(crate::Pallet::<Test>::do_try_state().is_err());
}

/// Every event this pallet emitted, oldest first, clearing the log.
pub fn registrar_events() -> Vec<pallet_registrar_para::Event<Test>> {
	let events = System::events()
		.into_iter()
		.filter_map(|e| match e.event {
			RuntimeEvent::Registrar(inner) => Some(inner),
			_ => None,
		})
		.collect();
	System::reset_events();
	events
}
