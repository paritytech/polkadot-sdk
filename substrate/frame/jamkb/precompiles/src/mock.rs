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

//! Test runtime for the pallet-jamkb precompile. The transport only allocates ids; tests report
//! outcomes to the pallet themselves.

use frame_support::{
	derive_impl, parameter_types,
	traits::{AsEnsureOriginWithArg, Hooks},
	weights::{Weight, WeightMeter},
	PalletId,
};
use pallet_jamkb::{
	messages::{MessageId, UpwardMessage},
	transport::{JamMessageSender, MessageSendError, OnJamOutcome, OperationId, SenderId},
	ServiceId, Timeslot,
};
use sp_runtime::{
	traits::{BlockNumberProvider, IdentityLookup},
	AccountId32, BuildStorage,
};

pub type AccountId = AccountId32;
pub type Balance = u128;
type Block = frame_system::mocking::MockBlock<Test>;

pub const ALICE: AccountId = AccountId::new([1u8; 32]);
pub const BOB: AccountId = AccountId::new([2u8; 32]);
pub const JAMKB: u32 = 7;
pub const CAP: Balance = 1_000_000;
pub const SERVICE: ServiceId = 42;
pub const JAMKB_SENDER: SenderId = 3;

#[frame_support::runtime]
mod runtime {
	#[runtime::runtime]
	#[runtime::derive(
		RuntimeCall,
		RuntimeEvent,
		RuntimeError,
		RuntimeOrigin,
		RuntimeTask,
		RuntimeHoldReason,
		RuntimeFreezeReason
	)]
	pub struct Test;

	#[runtime::pallet_index(0)]
	pub type System = frame_system;
	#[runtime::pallet_index(10)]
	pub type Balances = pallet_balances;
	#[runtime::pallet_index(20)]
	pub type Assets = pallet_assets;
	#[runtime::pallet_index(21)]
	pub type AssetsHolder = pallet_assets_holder;
	#[runtime::pallet_index(30)]
	pub type Revive = pallet_revive;
	#[runtime::pallet_index(40)]
	pub type Jamkb = pallet_jamkb;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Test {
	type Block = Block;
	type AccountId = AccountId;
	type Lookup = IdentityLookup<AccountId>;
	type AccountData = pallet_balances::AccountData<Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig as pallet_balances::DefaultConfig)]
impl pallet_balances::Config for Test {
	type Balance = Balance;
	type AccountStore = System;
}

#[derive_impl(pallet_assets::config_preludes::TestDefaultConfig as pallet_assets::DefaultConfig)]
impl pallet_assets::Config for Test {
	type Balance = Balance;
	type CreateOrigin = AsEnsureOriginWithArg<frame_system::EnsureSigned<AccountId>>;
	type ForceOrigin = frame_system::EnsureRoot<AccountId>;
	type Currency = Balances;
	type Holder = AssetsHolder;
}

impl pallet_assets_holder::Config for Test {
	type RuntimeHoldReason = RuntimeHoldReason;
	type RuntimeEvent = RuntimeEvent;
}

#[derive_impl(pallet_revive::config_preludes::TestDefaultConfig)]
impl pallet_revive::Config for Test {
	type AddressMapper = pallet_revive::AccountId32Mapper<Self>;
	type Balance = Balance;
	type Currency = Balances;
	type Precompiles = (crate::Jamkb<Self>,);
	type UploadOrigin = frame_system::EnsureSigned<AccountId>;
	type InstantiateOrigin = frame_system::EnsureSigned<AccountId>;
}

/// Allocates increasing message and operation ids and queues nothing.
pub struct MockSender;

impl JamMessageSender<Balance> for MockSender {
	fn next_message_id() -> MessageId {
		LastMessageId::mutate(|id| {
			*id += 1;
			*id
		})
	}

	fn send(_: SenderId, _: Vec<UpwardMessage<Balance>>) -> Result<OperationId, MessageSendError> {
		Ok(LastOperationId::mutate(|id| {
			*id += 1;
			*id
		}))
	}
}

/// The current timeslot, set by the tests.
pub struct Timeslots;

impl BlockNumberProvider for Timeslots {
	type BlockNumber = Timeslot;

	fn current_block_number() -> Timeslot {
		CurrentTimeslot::get()
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn set_block_number(n: Timeslot) {
		CurrentTimeslot::set(n);
	}
}

parameter_types! {
	pub static LastMessageId: MessageId = 1_000;
	pub static LastOperationId: OperationId = 100;
	pub static CurrentTimeslot: Timeslot = 1;
	pub const JamkbAssetId: u32 = JAMKB;
	pub const Cap: Balance = CAP;
	pub const MinTransfer: Balance = 10;
	pub const MemoGas: u64 = 5_000;
	pub SettlementWeight: Weight = Weight::from_parts(1_000_000_000, 1_000_000);
	pub const JamkbSender: SenderId = JAMKB_SENDER;
	pub const JamkbPalletId: PalletId = PalletId(*b"py/jamkb");
	pub const ParachainService: ServiceId = 0;
	pub const MaxUnsettledOperations: u32 = 16;
	pub const MaxBucketsPerBlock: u32 = 2;
	pub const MaxTransfersPerBucket: u32 = 4;
}

impl pallet_jamkb::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeHoldReason = RuntimeHoldReason;
	type AssetId = u32;
	type Balance = Balance;
	type Assets = Assets;
	type AssetsHolder = AssetsHolder;
	type MessageSender = MockSender;
	type DeliveryFee = ();
	type TimeslotProvider = Timeslots;
	type SenderId = JamkbSender;
	type JamkbAssetId = JamkbAssetId;
	type Cap = Cap;
	type MinTransfer = MinTransfer;
	type MemoGas = MemoGas;
	type SettlementWeight = SettlementWeight;
	type PalletId = JamkbPalletId;
	type ParachainService = ParachainService;
	type MaxUnsettledOperations = MaxUnsettledOperations;
	type MaxBucketsPerBlock = MaxBucketsPerBlock;
	type MaxTransfersPerBucket = MaxTransfersPerBucket;
	type CustodyMapping = ();
	type GovernanceOrigin = frame_system::EnsureRoot<AccountId>;
	type WeightInfo = ();
}

/// A fresh state with an initialized and attested pallet; `released` units sit in the
/// `released` account.
pub fn new_test_ext(released: Balance) -> sp_io::TestExternalities {
	let mut t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
	pallet_balances::GenesisConfig::<Test> {
		balances: vec![(ALICE, 1_000_000_000_000), (BOB, 1_000_000_000_000)],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	pallet_revive::GenesisConfig::<Test> {
		mapped_accounts: vec![ALICE, BOB],
		..Default::default()
	}
	.assimilate_storage(&mut t)
	.unwrap();
	let mut ext = sp_io::TestExternalities::new(t);
	ext.execute_with(|| {
		System::set_block_number(1);
		CurrentTimeslot::set(1);
		frame_support::assert_ok!(Jamkb::initialize(RuntimeOrigin::root()));
		frame_support::assert_ok!(Jamkb::attest(
			RuntimeOrigin::root(),
			[0xAA; 32].into(),
			released
		));
	});
	ext
}

/// Reports operation `id` as accumulated without a failure entry.
pub fn confirm(id: OperationId) {
	<Jamkb as OnJamOutcome>::on_outcome(JAMKB_SENDER, id, 2, &[], &mut WeightMeter::new());
}

/// Runs the settlement hook.
pub fn poll() {
	<Jamkb as Hooks<u64>>::on_poll(1, &mut WeightMeter::new());
}
