// This file is part of Substrate.

// Copyright (C) 2022 Parity Technologies (UK) Ltd.
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

mod staking;
// mod signed;
// mod unsigned;
// mod weight_info;

pub use staking::*;

use crate::{self as epm, Config, *};
use frame_support::{derive_impl, pallet_prelude::*, parameter_types};
use sp_runtime::{
	generic::{Header, UncheckedExtrinsic},
	traits::IdentifyAccount,
	BuildStorage,
};

frame_support::construct_runtime!(
	pub struct Runtime {
		System: frame_system,
		Balances: pallet_balances,
		MultiPhase: epm,
	}
);

pub type AccountId = u64;
pub type Balance = u128;
pub type BlockNumber = u64;
pub type T = Runtime;
pub type Block = frame_system::mocking::MockBlock<Runtime>;

#[derive_impl(frame_system::config_preludes::TestDefaultConfig as frame_system::DefaultConfig)]
impl frame_system::Config for Runtime {
	type Block = Block;
	type AccountData = pallet_balances::AccountData<Balance>;
}

parameter_types! {
	pub static SignedPhase: BlockNumber = 3;
	pub static UnsignedPhase: BlockNumber = 5;
	pub static SignedValidationPhase: BlockNumber = Pages::get().into();
	pub static Lookhaead: BlockNumber = 0;
	pub static Pages: PageIndex = 3;
}

impl Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type SignedPhase = SignedPhase;
	type UnsignedPhase = UnsignedPhase;
	type SignedValidationPhase = SignedValidationPhase;
	type Lookhaead = Lookhaead;
	type Pages = Pages;
	type DataProvider = MockStaking;
}

parameter_types! {
	//pub static MaxVotesPerVoter: u32 = <TestNposSolution as NposSolution>::LIMIT as u32;
	pub static MaxVotesPerVoter: u32 = 16;

}

parameter_types! {
	pub const ExistentialDeposit: Balance = 1;
}

impl pallet_balances::Config for Runtime {
	type Balance = Balance;
	type RuntimeEvent = RuntimeEvent;
	type DustRemoval = ();
	type ExistentialDeposit = ExistentialDeposit;
	type AccountStore = System;
	type MaxLocks = ();
	type MaxReserves = ();
	type ReserveIdentifier = [u8; 8];
	type WeightInfo = ();
	type FreezeIdentifier = ();
	type MaxFreezes = ();
	type RuntimeHoldReason = ();
	type MaxHolds = ();
}

#[derive(Default)]
pub struct ExtBuilder;
impl ExtBuilder {
	pub(crate) fn pages(self, pages: u32) -> Self {
		Pages::set(pages);
		self
	}

	pub(crate) fn signed_phase(self, blocks: BlockNumber) -> Self {
		SignedPhase::set(blocks);
		self
	}

	pub(crate) fn validate_signed_phase(self, blocks: BlockNumber) -> Self {
		SignedValidationPhase::set(blocks);
		self
	}

	pub(crate) fn unsigned_phase(self, blocks: BlockNumber) -> Self {
		UnsignedPhase::set(blocks);
		self
	}

	pub(crate) fn lookahead(self, blocks: BlockNumber) -> Self {
		Lookhaead::set(blocks);
		self
	}

	pub(crate) fn build(self) -> sp_io::TestExternalities {
		sp_tracing::try_init_simple();

		let mut storage = frame_system::GenesisConfig::<T>::default().build_storage().unwrap();
		let _ = pallet_balances::GenesisConfig::<T> {
			balances: vec![
				// account for submitting stuff only.
				(91, 100),
				(92, 100),
				(93, 100),
				(94, 100),
				(95, 100),
				(96, 100),
				(97, 100),
				(99, 100),
				(999, 100),
				(9999, 100),
			],
		}
		.assimilate_storage(&mut storage);

		sp_io::TestExternalities::from(storage)
	}
	pub(crate) fn build_and_execute(self, test: impl FnOnce() -> ()) {
		let mut ext = self.build();
		ext.execute_with(test);
		// TODO(gpestana): add sanity checks and try_runtimes
	}
}

pub(crate) fn roll_to(n: BlockNumber) {
	let now = System::block_number();
	for bn in now + 1..n {
		System::set_block_number(bn);

		let election_prediction =
			<<Runtime as Config>::DataProvider as ElectionDataProvider>::next_election_prediction(
				bn,
			);

		log!(
			info,
			"Phase: {:?}, Round: {:?}, Election at {:?}",
			<CurrentPhase<T>>::get(),
			<Round<T>>::get(),
			election_prediction
		);
		MultiPhase::on_initialize(bn);
		// TODO(gpestana): other internal pallets.

		//all_pallets_sanity_checks();
	}
}
