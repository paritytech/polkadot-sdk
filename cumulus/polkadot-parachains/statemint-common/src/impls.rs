// Copyright (C) 2021 Parity Technologies (UK) Ltd.
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

//! Auxillary struct/enums for Statemint runtime.
//! Taken from polkadot/runtime/common (at a21cd64) and adapted for Statemint.

use frame_support::traits::{Currency, Imbalance, OnUnbalanced};

pub type NegativeImbalance<T> = <pallet_balances::Pallet<T> as Currency<<T as frame_system::Config>::AccountId>>::NegativeImbalance;

/// Logic for the author to get a portion of fees.
pub struct ToStakingPot<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for ToStakingPot<R>
where
	R: pallet_balances::Config + pallet_collator_selection::Config,
	<R as frame_system::Config>::AccountId: From<polkadot_primitives::v1::AccountId>,
	<R as frame_system::Config>::AccountId: Into<polkadot_primitives::v1::AccountId>,
	<R as frame_system::Config>::Event: From<pallet_balances::Event<R>>,
{
	fn on_nonzero_unbalanced(amount: NegativeImbalance<R>) {
		let numeric_amount = amount.peek();
		let staking_pot = <pallet_collator_selection::Pallet<R>>::account_id();
		<pallet_balances::Pallet<R>>::resolve_creating(
			&staking_pot,
			amount,
		);
		<frame_system::Pallet<R>>::deposit_event(pallet_balances::Event::Deposit(
			staking_pot,
			numeric_amount,
		));
	}
}

pub struct DealWithFees<R>(sp_std::marker::PhantomData<R>);
impl<R> OnUnbalanced<NegativeImbalance<R>> for DealWithFees<R>
where
	R: pallet_balances::Config + pallet_collator_selection::Config,
	<R as frame_system::Config>::AccountId: From<polkadot_primitives::v1::AccountId>,
	<R as frame_system::Config>::AccountId: Into<polkadot_primitives::v1::AccountId>,
	<R as frame_system::Config>::Event: From<pallet_balances::Event<R>>,
{
	fn on_unbalanceds<B>(mut fees_then_tips: impl Iterator<Item = NegativeImbalance<R>>) {
		if let Some(mut fees) = fees_then_tips.next() {
			if let Some(tips) = fees_then_tips.next() {
				tips.merge_into(&mut fees);
			}
			<ToStakingPot<R> as OnUnbalanced<_>>::on_unbalanced(fees);
		}
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use frame_support::traits::FindAuthor;
	use frame_support::{parameter_types, PalletId, traits::ValidatorRegistration};
	use frame_system::{limits, EnsureRoot};
	use polkadot_primitives::v1::AccountId;
	use sp_core::H256;
	use sp_runtime::{
		testing::Header,
		traits::{BlakeTwo256, IdentityLookup},
		Perbill,
	};
	use pallet_collator_selection::IdentityCollator;

	type UncheckedExtrinsic = frame_system::mocking::MockUncheckedExtrinsic<Test>;
	type Block = frame_system::mocking::MockBlock<Test>;

	frame_support::construct_runtime!(
		pub enum Test where
			Block = Block,
			NodeBlock = Block,
			UncheckedExtrinsic = UncheckedExtrinsic,
		{
			System: frame_system::{Pallet, Call, Config, Storage, Event<T>},
			Balances: pallet_balances::{Pallet, Call, Storage, Config<T>, Event<T>},
			CollatorSelection: pallet_collator_selection::{Pallet, Call, Storage, Event<T>},
		}
	);

	parameter_types! {
		pub const BlockHashCount: u64 = 250;
		pub BlockLength: limits::BlockLength = limits::BlockLength::max(2 * 1024);
		pub const AvailableBlockRatio: Perbill = Perbill::one();
		pub const MaxReserves: u32 = 50;
	}

	impl frame_system::Config for Test {
		type BaseCallFilter = ();
		type Origin = Origin;
		type Index = u64;
		type BlockNumber = u64;
		type Call = Call;
		type Hash = H256;
		type Hashing = BlakeTwo256;
		type AccountId = AccountId;
		type Lookup = IdentityLookup<Self::AccountId>;
		type Header = Header;
		type Event = Event;
		type BlockHashCount = BlockHashCount;
		type BlockLength = BlockLength;
		type BlockWeights = ();
		type DbWeight = ();
		type Version = ();
		type PalletInfo = PalletInfo;
		type AccountData = pallet_balances::AccountData<u64>;
		type OnNewAccount = ();
		type OnKilledAccount = ();
		type SystemWeightInfo = ();
		type SS58Prefix = ();
		type OnSetCode = ();
	}

	impl pallet_balances::Config for Test {
		type Balance = u64;
		type Event = Event;
		type DustRemoval = ();
		type ExistentialDeposit = ();
		type AccountStore = System;
		type MaxLocks = ();
		type WeightInfo = ();
		type MaxReserves = MaxReserves;
		type ReserveIdentifier = [u8; 8];
	}

	pub struct OneAuthor;
	impl FindAuthor<AccountId> for OneAuthor {
		fn find_author<'a, I>(_: I) -> Option<AccountId>
		where
			I: 'a,
		{
			Some(Default::default())
		}
	}

	pub struct IsRegistered;
	impl ValidatorRegistration<AccountId> for IsRegistered {
		fn is_registered(_id: &AccountId) -> bool {
			true
		}
	}

	parameter_types! {
		pub const PotId: PalletId = PalletId(*b"PotStake");
		pub const MaxCandidates: u32 = 20;
		pub const MaxInvulnerables: u32 = 20;
		pub const MinCandidates: u32 = 1;
	}

	impl pallet_collator_selection::Config for Test {
		type Event = Event;
		type Currency = Balances;
		type UpdateOrigin = EnsureRoot<AccountId>;
		type PotId = PotId;
		type MaxCandidates = MaxCandidates;
		type MinCandidates = MinCandidates;
		type MaxInvulnerables = MaxInvulnerables;
		type ValidatorId = <Self as frame_system::Config>::AccountId;
		type ValidatorIdOf = IdentityCollator;
		type ValidatorRegistration = IsRegistered;
		type KickThreshold = ();
		type WeightInfo = ();
	}

	impl pallet_authorship::Config for Test {
		type FindAuthor = OneAuthor;
		type UncleGenerations = ();
		type FilterUncle = ();
		type EventHandler = ();
	}

	pub fn new_test_ext() -> sp_io::TestExternalities {
		let mut t = frame_system::GenesisConfig::default()
			.build_storage::<Test>()
			.unwrap();
		// We use default for brevity, but you can configure as desired if needed.
		pallet_balances::GenesisConfig::<Test>::default()
			.assimilate_storage(&mut t)
			.unwrap();
		t.into()
	}

	#[test]
	fn test_fees_and_tip_split() {
		new_test_ext().execute_with(|| {
			let fee = Balances::issue(10);
			let tip = Balances::issue(20);

			assert_eq!(Balances::free_balance(AccountId::default()), 0);

			DealWithFees::on_unbalanceds(vec![fee, tip].into_iter());

			// Author gets 100% of tip and 100% of fee = 30
			assert_eq!(Balances::free_balance(CollatorSelection::account_id()), 30);
		});
	}
}
