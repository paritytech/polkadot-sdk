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

// Mock for Whitelist Pallet

#![cfg(test)]

use crate as pallet_whitelist;

use frame::{
	deps::{frame_support::weights::IdentityFee, sp_runtime::testing::UintAuthorityId},
	testing_prelude::*,
};
use pallet_transaction_payment::{ChargeTransactionPayment, FungibleAdapter};

/// Payment is included so the tests can observe that an `Authorized` submission is charged
/// nothing, while the privileged and relayer paths keep their own fee behaviour.
pub type TxExtension = (frame_system::AuthorizeCall<Test>, ChargeTransactionPayment<Test>);
pub type UncheckedExtrinsic = MockUncheckedExtrinsic<Test, UintAuthorityId, TxExtension>;
type Block = MockBlock<Test, UintAuthorityId, TxExtension>;

construct_runtime!(
	pub enum Test
	{
		System: frame_system,
		Balances: pallet_balances,
		TransactionPayment: pallet_transaction_payment,
		Whitelist: pallet_whitelist,
		Preimage: pallet_preimage,
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

#[derive_impl(pallet_transaction_payment::config_preludes::TestDefaultConfig)]
impl pallet_transaction_payment::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type OnChargeTransaction = FungibleAdapter<Balances, ()>;
	type WeightToFee = IdentityFee<u64>;
	type LengthToFee = IdentityFee<u64>;
}

impl pallet_preimage::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type Currency = Balances;
	type ManagerOrigin = EnsureRoot<Self::AccountId>;
	type Consideration = ();
	type WeightInfo = ();
}

impl pallet_whitelist::Config for Test {
	type RuntimeEvent = RuntimeEvent;
	type RuntimeCall = RuntimeCall;
	type WhitelistOrigin = EnsureRoot<Self::AccountId>;
	type DispatchWhitelistedOrigin =
		EitherOf<EnsureRoot<Self::AccountId>, frame_system::EnsureAuthorized<Self::AccountId>>;
	type DeferredDispatchExpiration = ConstU64<15>;
	type BlockNumberProvider = System;
	type Preimages = Preimage;
	type WeightInfo = ();
}

pub fn new_test_ext() -> TestExternalities {
	let t = RuntimeGenesisConfig::default().build_storage().unwrap();
	let mut ext = TestExternalities::new(t);
	ext.execute_with(|| System::set_block_number(1));
	ext
}

/// A runtime that has not opted in: `DispatchWhitelistedOrigin` rejects `Authorized`.
pub mod no_auth {
	use crate as pallet_whitelist;
	use frame::testing_prelude::*;

	type Block = MockBlock<Runtime>;

	construct_runtime!(
		pub enum Runtime
		{
			System: frame_system,
			Balances: pallet_balances,
			Whitelist: pallet_whitelist,
			Preimage: pallet_preimage,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Runtime {
		type Block = Block;
		type AccountData = pallet_balances::AccountData<u64>;
	}

	#[derive_impl(pallet_balances::config_preludes::TestDefaultConfig)]
	impl pallet_balances::Config for Runtime {
		type AccountStore = System;
	}

	impl pallet_preimage::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type Currency = Balances;
		type ManagerOrigin = EnsureRoot<Self::AccountId>;
		type Consideration = ();
		type WeightInfo = ();
	}

	impl pallet_whitelist::Config for Runtime {
		type RuntimeEvent = RuntimeEvent;
		type RuntimeCall = RuntimeCall;
		type WhitelistOrigin = EnsureRoot<Self::AccountId>;
		type DispatchWhitelistedOrigin = EnsureRoot<Self::AccountId>;
		type DeferredDispatchExpiration = ConstU64<15>;
		type BlockNumberProvider = System;
		type Preimages = Preimage;
		type WeightInfo = ();
	}

	pub fn new_test_ext() -> TestExternalities {
		let t = RuntimeGenesisConfig::default().build_storage().unwrap();
		let mut ext = TestExternalities::new(t);
		ext.execute_with(|| System::set_block_number(1));
		ext
	}
}
