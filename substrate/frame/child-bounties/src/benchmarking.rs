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

//! Child-bounties pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use alloc::vec;
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::ensure;
use frame_system::RawOrigin;
use pallet_bounties::Pallet as Bounties;
use pallet_treasury::Pallet as Treasury;
use sp_runtime::traits::BlockNumberProvider;

use crate::*;

const SEED: u32 = 0;

#[derive(Clone)]
struct BenchmarkChildBounty<T: Config> {
	/// Bounty ID.
	bounty_id: BountyIndex,
	/// ChildBounty ID.
	child_bounty_id: BountyIndex,
	/// The account proposing it.
	caller: T::AccountId,
	/// The master curator account.
	curator: T::AccountId,
	/// The child-bounty curator account.
	child_curator: T::AccountId,
	/// The (total) amount that should be paid if the bounty is rewarded.
	value: BalanceOf<T>,
	/// The curator fee. included in value.
	fee: BalanceOf<T>,
	/// The (total) amount that should be paid if the child-bounty is rewarded.
	child_bounty_value: BalanceOf<T>,
	/// The child-bounty curator fee. included in value.
	child_bounty_fee: BalanceOf<T>,
	/// Bounty description.
	reason: Vec<u8>,
}

fn set_block_number<T: Config>(n: BlockNumberFor<T>) {
	<T as pallet_treasury::Config>::BlockNumberProvider::set_block_number(n);
}

fn setup_bounty<T: Config>(
	user: u32,
	description: u32,
) -> (T::AccountId, T::AccountId, BalanceOf<T>, BalanceOf<T>, Vec<u8>) {
	let caller = account("caller", user, SEED);
	let value: BalanceOf<T> = T::BountyValueMinimum::get().saturating_mul(100u32.into());
	let fee = value / 2u32.into();
	let deposit = T::BountyDepositBase::get() +
		T::DataDepositPerByte::get() * T::MaximumReasonLength::get().into();
	let _ = T::Currency::make_free_balance_be(&caller, deposit + T::Currency::minimum_balance());
	let curator = account("curator", user, SEED);
	let _ = T::Currency::make_free_balance_be(
		&curator,
		fee / 2u32.into() + T::Currency::minimum_balance(),
	);
	let reason = vec![0; description as usize];
	(caller, curator, fee, value, reason)
}

fn setup_child_bounty<T: Config>(user: u32, description: u32) -> BenchmarkChildBounty<T> {
	let (caller, curator, fee, value, reason) = setup_bounty::<T>(user, description);
	let child_curator = account("child-curator", user, SEED);
	let _ = T::Currency::make_free_balance_be(
		&child_curator,
		fee / 2u32.into() + T::Currency::minimum_balance(),
	);
	let child_bounty_value = (value - fee) / 4u32.into();
	let child_bounty_fee = child_bounty_value / 2u32.into();

	BenchmarkChildBounty::<T> {
		bounty_id: 0,
		child_bounty_id: 0,
		caller,
		curator,
		child_curator,
		value,
		fee,
		child_bounty_value,
		child_bounty_fee,
		reason,
	}
}

fn activate_bounty<T: Config>(
	user: u32,
	description: u32,
) -> Result<BenchmarkChildBounty<T>, BenchmarkError> {
	let mut child_bounty_setup = setup_child_bounty::<T>(user, description);
	let curator_lookup = T::Lookup::unlookup(child_bounty_setup.curator.clone());
	Bounties::<T>::propose_bounty(
		RawOrigin::Signed(child_bounty_setup.caller.clone()).into(),
		child_bounty_setup.value,
		child_bounty_setup.reason.clone(),
	)?;

	child_bounty_setup.bounty_id = pallet_bounties::BountyCount::<T>::get() - 1;

	let approve_origin =
		T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	Bounties::<T>::approve_bounty(approve_origin, child_bounty_setup.bounty_id)?;
	set_block_number::<T>(T::SpendPeriod::get());
	Treasury::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
	Bounties::<T>::propose_curator(
		RawOrigin::Root.into(),
		child_bounty_setup.bounty_id,
		curator_lookup,
		child_bounty_setup.fee,
	)?;
	Bounties::<T>::accept_curator(
		RawOrigin::Signed(child_bounty_setup.curator.clone()).into(),
		child_bounty_setup.bounty_id,
	)?;

	Ok(child_bounty_setup)
}

fn activate_child_bounty<T: Config>(
	user: u32,
	description: u32,
) -> Result<BenchmarkChildBounty<T>, BenchmarkError> {
	let mut bounty_setup = activate_bounty::<T>(user, description)?;
	let child_curator_lookup = T::Lookup::unlookup(bounty_setup.child_curator.clone());

	Pallet::<T>::add_child_bounty(
		RawOrigin::Signed(bounty_setup.curator.clone()).into(),
		bounty_setup.bounty_id,
		bounty_setup.child_bounty_value,
		bounty_setup.reason.clone(),
	)?;

	bounty_setup.child_bounty_id = ParentTotalChildBounties::<T>::get(bounty_setup.bounty_id) - 1;

	Pallet::<T>::propose_curator(
		RawOrigin::Signed(bounty_setup.curator.clone()).into(),
		bounty_setup.bounty_id,
		bounty_setup.child_bounty_id,
		child_curator_lookup,
		bounty_setup.child_bounty_fee,
	)?;

	Pallet::<T>::accept_curator(
		RawOrigin::Signed(bounty_setup.child_curator.clone()).into(),
		bounty_setup.bounty_id,
		bounty_setup.child_bounty_id,
	)?;

	Ok(bounty_setup)
}

fn setup_pot_account<T: Config>() {
	let pot_account = Bounties::<T>::account_id();
	let value = T::Currency::minimum_balance().saturating_mul(1_000_000_000u32.into());
	let _ = T::Currency::make_free_balance_be(&pot_account, value);
}

fn assert_last_event<T: Config>(generic_event: <T as Config>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

#[benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn add_child_bounty(
		d: Linear<0, { T::MaximumReasonLength::get() }>,
	) -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let bounty_setup = activate_bounty::<T>(0, d)?;

		#[extrinsic_call]
		_(
			RawOrigin::Signed(bounty_setup.curator),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_value,
			bounty_setup.reason.clone(),
		);

		assert_last_event::<T>(
			Event::Added {
				index: bounty_setup.bounty_id,
				child_index: bounty_setup.child_bounty_id,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn propose_curator() -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let bounty_setup = activate_bounty::<T>(0, T::MaximumReasonLength::get())?;
		let child_curator_lookup = T::Lookup::unlookup(bounty_setup.child_curator.clone());

		Pallet::<T>::add_child_bounty(
			RawOrigin::Signed(bounty_setup.curator.clone()).into(),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_value,
			bounty_setup.reason.clone(),
		)?;
		let child_bounty_id = ParentTotalChildBounties::<T>::get(bounty_setup.bounty_id) - 1;

		#[extrinsic_call]
		_(
			RawOrigin::Signed(bounty_setup.curator),
			bounty_setup.bounty_id,
			child_bounty_id,
			child_curator_lookup,
			bounty_setup.child_bounty_fee,
		);

		Ok(())
	}

	#[benchmark]
	fn accept_curator() -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let mut bounty_setup = activate_bounty::<T>(0, T::MaximumReasonLength::get())?;
		let child_curator_lookup = T::Lookup::unlookup(bounty_setup.child_curator.clone());

		Pallet::<T>::add_child_bounty(
			RawOrigin::Signed(bounty_setup.curator.clone()).into(),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_value,
			bounty_setup.reason.clone(),
		)?;
		bounty_setup.child_bounty_id =
			ParentTotalChildBounties::<T>::get(bounty_setup.bounty_id) - 1;

		Pallet::<T>::propose_curator(
			RawOrigin::Signed(bounty_setup.curator.clone()).into(),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_id,
			child_curator_lookup,
			bounty_setup.child_bounty_fee,
		)?;

		#[extrinsic_call]
		_(
			RawOrigin::Signed(bounty_setup.child_curator),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_id,
		);

		Ok(())
	}

	// Worst case when curator is inactive and any sender un-assigns the curator,
	// or if `BountyUpdatePeriod` is large enough and `RejectOrigin` executes the call.
	#[benchmark]
	fn unassign_curator() -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let bounty_setup = activate_child_bounty::<T>(0, T::MaximumReasonLength::get())?;
		Treasury::<T>::on_initialize(frame_system::Pallet::<T>::block_number());
		let bounty_update_period = T::BountyUpdatePeriod::get();
		let inactivity_timeout = T::SpendPeriod::get().saturating_add(bounty_update_period);
		set_block_number::<T>(inactivity_timeout.saturating_add(1u32.into()));

		// If `BountyUpdatePeriod` overflows the inactivity timeout the benchmark still
		// executes the slash
		let origin: T::RuntimeOrigin = if Pallet::<T>::treasury_block_number() <= inactivity_timeout
		{
			let child_curator = bounty_setup.child_curator;
			T::RejectOrigin::try_successful_origin()
				.unwrap_or_else(|_| RawOrigin::Signed(child_curator).into())
		} else {
			let caller = whitelisted_caller();
			RawOrigin::Signed(caller).into()
		};

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, bounty_setup.bounty_id, bounty_setup.child_bounty_id);

		Ok(())
	}

	#[benchmark]
	fn award_child_bounty() -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let bounty_setup = activate_child_bounty::<T>(0, T::MaximumReasonLength::get())?;
		let beneficiary_account = account::<T::AccountId>("beneficiary", 0, SEED);
		let beneficiary = T::Lookup::unlookup(beneficiary_account.clone());

		#[extrinsic_call]
		_(
			RawOrigin::Signed(bounty_setup.child_curator),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_id,
			beneficiary,
		);

		assert_last_event::<T>(
			Event::Awarded {
				index: bounty_setup.bounty_id,
				child_index: bounty_setup.child_bounty_id,
				beneficiary: beneficiary_account,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn claim_child_bounty() -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let bounty_setup = activate_child_bounty::<T>(0, T::MaximumReasonLength::get())?;
		let beneficiary_account = account("beneficiary", 0, SEED);
		let beneficiary = T::Lookup::unlookup(beneficiary_account);

		Pallet::<T>::award_child_bounty(
			RawOrigin::Signed(bounty_setup.child_curator.clone()).into(),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_id,
			beneficiary,
		)?;

		let beneficiary_account = account("beneficiary", 0, SEED);

		set_block_number::<T>(T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get());
		ensure!(
			T::Currency::free_balance(&beneficiary_account).is_zero(),
			"Beneficiary already has balance."
		);

		#[extrinsic_call]
		_(
			RawOrigin::Signed(bounty_setup.curator),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_id,
		);

		ensure!(
			!T::Currency::free_balance(&beneficiary_account).is_zero(),
			"Beneficiary didn't get paid."
		);

		Ok(())
	}

	// Best case scenario.
	#[benchmark]
	fn close_child_bounty_added() -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let mut bounty_setup = activate_bounty::<T>(0, T::MaximumReasonLength::get())?;

		Pallet::<T>::add_child_bounty(
			RawOrigin::Signed(bounty_setup.curator.clone()).into(),
			bounty_setup.bounty_id,
			bounty_setup.child_bounty_value,
			bounty_setup.reason.clone(),
		)?;
		bounty_setup.child_bounty_id =
			ParentTotalChildBounties::<T>::get(bounty_setup.bounty_id) - 1;

		#[extrinsic_call]
		close_child_bounty(RawOrigin::Root, bounty_setup.bounty_id, bounty_setup.child_bounty_id);

		assert_last_event::<T>(
			Event::Canceled {
				index: bounty_setup.bounty_id,
				child_index: bounty_setup.child_bounty_id,
			}
			.into(),
		);

		Ok(())
	}

	// Worst case scenario.
	#[benchmark]
	fn close_child_bounty_active() -> Result<(), BenchmarkError> {
		setup_pot_account::<T>();
		let bounty_setup = activate_child_bounty::<T>(0, T::MaximumReasonLength::get())?;
		Treasury::<T>::on_initialize(frame_system::Pallet::<T>::block_number());

		#[extrinsic_call]
		close_child_bounty(RawOrigin::Root, bounty_setup.bounty_id, bounty_setup.child_bounty_id);

		assert_last_event::<T>(
			Event::Canceled {
				index: bounty_setup.bounty_id,
				child_index: bounty_setup.child_bounty_id,
			}
			.into(),
		);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		tests::new_test_ext(),
		tests::Test
	}
}
