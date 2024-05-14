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

//! Mock setup for tests.

#![cfg(any(test, feature = "runtime-benchmarks"))]

use crate as pallet_account_sponsorship;
use crate::*;
use frame_support::{
	construct_runtime, derive_impl,
	weights::{FixedFee, NoFee},
};
use sp_core::ConstU8;
use sp_runtime::{
	traits::{ConstU64, IdentifyAccount, IdentityLookup, Verify},
	MultiSignature,
};

pub type Balance = u64;

pub type Signature = MultiSignature;
pub type AccountId = <<Signature as Verify>::Signer as IdentifyAccount>::AccountId;

pub type MetaTxExtension = (
	frame_system::CheckNonZeroSender<Runtime>,
	frame_system::CheckSpecVersion<Runtime>,
	frame_system::CheckTxVersion<Runtime>,
	frame_system::CheckGenesis<Runtime>,
	frame_system::CheckMortality<Runtime>,
	frame_system::CheckNonce<Runtime>,
);

impl pallet_meta_tx::Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type Signature = Signature;
	type PublicKey = <Signature as Verify>::Signer;
	type Context = ();
	type Extension = MetaTxExtension;
}

#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
impl frame_system::Config for Runtime {
	type AccountId = AccountId;
	type Lookup = IdentityLookup<Self::AccountId>;
	type Block = frame_system::mocking::MockBlock<Runtime>;
	type AccountData = pallet_balances::AccountData<<Self as pallet_balances::Config>::Balance>;
}

#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
impl pallet_balances::Config for Runtime {
	type ReserveIdentifier = [u8; 8];
	type AccountStore = System;
	type RuntimeHoldReason = RuntimeHoldReason;
	type ExistentialDeposit = ConstU64<5>;
}

pub const TX_FEE: u32 = 10;

impl pallet_transaction_payment::Config for Runtime {
	type WeightInfo = ();
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = pallet_transaction_payment::CurrencyAdapter<Balances, ()>;
	type OperationalFeeMultiplier = ConstU8<1>;
	type WeightToFee = FixedFee<TX_FEE, Balance>;
	type LengthToFee = NoFee<Balance>;
	type FeeMultiplierUpdate = ();
}

impl Config for Runtime {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type RuntimeHoldReason = RuntimeHoldReason;
	type BaseDeposit = ConstU64<5>;
	type BeneficiaryDeposit = ConstU64<1>;
	type GracePeriod = ConstU64<10>;
}

construct_runtime!(
	pub enum Runtime {
		System: frame_system,
		Balances: pallet_balances,
		MetaTx: pallet_meta_tx,
		TxPayment: pallet_transaction_payment,
		AccountSponsorship: pallet_account_sponsorship,
	}
);

pub(crate) fn new_test_ext() -> sp_io::TestExternalities {
	let mut ext = sp_io::TestExternalities::new(Default::default());
	ext.execute_with(|| {
		frame_system::GenesisConfig::<Runtime>::default().build();
		System::set_block_number(1);
	});
	ext
}
