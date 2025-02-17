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

//! Bounties pallet benchmarking.

use super::*;

use alloc::{vec, vec::Vec};
use frame_benchmarking::v1::{
	account, benchmarks_instance_pallet, whitelisted_caller, BenchmarkError,
};
use frame_support::traits::Currency;
use frame_system::{pallet_prelude::BlockNumberFor as SystemBlockNumberFor, RawOrigin};
use sp_core::crypto::FromEntropy;
use sp_runtime::traits::BlockNumberProvider;

use crate::Pallet as Bounties;
use pallet_treasury::Pallet as Treasury;

/// Trait describing factory functions for dispatchables' parameters.
pub trait ArgumentsFactory<AssetKind> {
	/// Factory function for an asset kind.
	fn create_asset_kind(seed: u32) -> AssetKind;
}

/// Implementation that expects the parameters implement the [`FromEntropy`] trait.
impl<AssetKind> ArgumentsFactory<AssetKind> for ()
where
	AssetKind: FromEntropy,
{
	fn create_asset_kind(seed: u32) -> AssetKind {
		AssetKind::from_entropy(&mut seed.encode().as_slice()).unwrap()
	}
}

const SEED: u32 = 0;

fn set_block_number<T: Config<I>, I: 'static>(n: BlockNumberFor<T, I>) {
	<T as pallet_treasury::Config<I>>::BlockNumberProvider::set_block_number(n);
}

// Create bounties that are approved for use in `on_initialize`.
fn create_approved_bounties<T: Config<I>, I: 'static>(n: u32) -> Result<(), BenchmarkError> {
	for i in 0..n {
		let (caller, _curator, _fee, value, reason) =
			setup_bounty::<T, I>(i, T::MaximumReasonLength::get());
		Bounties::<T, I>::propose_bounty(RawOrigin::Signed(caller).into(), value, reason)?;
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let approve_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::approve_bounty(approve_origin, bounty_id)?;
	}
	ensure!(BountyApprovals::<T, I>::get().len() == n as usize, "Not all bounty approved");
	Ok(())
}

// Create the pre-requisite information needed to create a treasury `propose_bounty`.
fn setup_bounty<T: Config<I>, I: 'static>(
	u: u32,
	d: u32,
) -> (
	T::AccountId,
	T::AccountId,
	T::AssetKind,
	BountyBalanceOf<T, I>,
	BountyBalanceOf<T, I>,
	T::Beneficiary,
	Vec<u8>,
) {
	let caller = account("caller", u, SEED);
	// let value: BalanceOf<T, I> = T::BountyValueMinimum::get().saturating_mul(100u32.into());
	let asset_kind = <T as Config<I>>::BenchmarkHelper::create_asset_kind(SEED);
	let asset_balance: BountyBalanceOf<T, I> = 100_000u32.into();
	// TODO: revisit asset conversion
	// let native_value =
	// 	T::BalanceConverter::from_asset_balance(100u32.into(), asset_kind).unwrap_or(100u32.into());
	let fee = asset_balance / 2u32.into();
	let deposit = T::BountyDepositBase::get() +
		T::DataDepositPerByte::get() * T::MaximumReasonLength::get().into();
	let _ = T::Currency::make_free_balance_be(&caller, deposit + T::Currency::minimum_balance());
	let curator = account("curator", u, SEED);
	let _ = T::Currency::make_free_balance_be(
		&curator,
		fee / 2u32.into() + T::Currency::minimum_balance(),
	);
	let reason = vec![0; d as usize];
	(caller, curator, asset_kind, fee, asset_balance, curator_stash, reason)
}

fn create_bounty<T: Config<I>, I: 'static>(
) -> Result<(AccountIdLookupOf<T>, BountyIndex), BenchmarkError> {
	let (caller, curator, asset_kind, fee, value, curator_stash, reason) =
		setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
	let curator_lookup = T::Lookup::unlookup(curator.clone());
	let curator_stash_lookup = T::BeneficiaryLookup::unlookup(curator_stash);
	Bounties::<T, I>::propose_bounty(
		RawOrigin::Signed(caller).into(),
		Box::new(asset_kind),
		value,
		reason,
	)?;
	let bounty_id = BountyCount::<T, I>::get() - 1;
	let approve_origin =
		T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	Bounties::<T, I>::approve_bounty(approve_origin.clone(), bounty_id)?;
	set_block_number::<T, I>(T::SpendPeriod::get());
	Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());
	Bounties::<T, I>::propose_curator(approve_origin, bounty_id, curator_lookup.clone(), fee)?;
	Bounties::<T, I>::accept_curator(
		RawOrigin::Signed(curator).into(),
		bounty_id,
		curator_stash_lookup,
	)?;
	Ok((curator_lookup, bounty_id))
}

fn setup_pot_account<T: Config<I>, I: 'static>() {
	let pot_account = Bounties::<T, I>::account_id();
	let value = T::Currency::minimum_balance().saturating_mul(1_000_000_000u32.into());
	let _ = T::Currency::make_free_balance_be(&pot_account, value);
}

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

benchmarks_instance_pallet! {
	propose_bounty {
		let d in 0 .. T::MaximumReasonLength::get();

		let (caller, curator, asset_kind, fee, value, _curator_stash, description) = setup_bounty::<T, I>(0, d);
	}: _(RawOrigin::Signed(caller), Box::new(asset_kind), value, description)

	approve_bounty {
		let (caller, curator, asset_kind, fee, value, _curator_stash, reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		Bounties::<T, I>::propose_bounty(RawOrigin::Signed(caller).into(), Box::new(asset_kind), value, reason)?;
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: _<T::RuntimeOrigin>(approve_origin, bounty_id)

	propose_curator {
		setup_pot_account::<T, I>();
		let (caller, curator, asset_kind, fee, value, _curator_stash, reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_lookup = T::Lookup::unlookup(curator);
		Bounties::<T, I>::propose_bounty(RawOrigin::Signed(caller).into(), Box::new(asset_kind), value, reason)?;
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::approve_bounty(approve_origin.clone(), bounty_id)?;
		set_block_number::<T, I>(T::SpendPeriod::get());
		Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());
	}: _<T::RuntimeOrigin>(approve_origin, bounty_id, curator_lookup, fee)

	approve_bounty_with_curator {
		setup_pot_account::<T, I>();
		let (caller, curator, asset_kind, fee, value, _curator_stash, reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_lookup = T::Lookup::unlookup(curator.clone());
		Bounties::<T, I>::propose_bounty(RawOrigin::Signed(caller).into(), Box::new(asset_kind), value, reason)?;
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Treasury::<T, I>::on_initialize(SystemBlockNumberFor::<T>::zero());
	}: _<T::RuntimeOrigin>(approve_origin, bounty_id, curator_lookup, fee)
	verify {
		assert_last_event::<T, I>(
			Event::CuratorProposed { bounty_id, curator }.into()
		);
	}

	// Worst case when curator is inactive and any sender unassigns the curator.
	unassign_curator {
		setup_pot_account::<T, I>();
		let (curator_lookup, bounty_id) = create_bounty::<T, I>()?;
		Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());
		let bounty_id = BountyCount::<T, I>::get() - 1;
		set_block_number::<T, I>(T::SpendPeriod::get() + T::BountyUpdatePeriod::get() + 2u32.into());
		let caller = whitelisted_caller();
	}: _(RawOrigin::Signed(caller), bounty_id)

	accept_curator {
		setup_pot_account::<T, I>();
		let (caller, curator, asset_kind, fee, value, curator_stash, reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_lookup = T::Lookup::unlookup(curator.clone());
		let curator_stash_lookup = T::BeneficiaryLookup::unlookup(curator_stash);
		Bounties::<T, I>::propose_bounty(RawOrigin::Signed(caller).into(), Box::new(asset_kind), value, reason)?;
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::approve_bounty(approve_origin.clone(), bounty_id)?;
		set_block_number::<T, I>(T::SpendPeriod::get());
		Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());
		Bounties::<T, I>::propose_curator(approve_origin, bounty_id, curator_lookup, fee)?;
	}: _(RawOrigin::Signed(curator), bounty_id, curator_stash_lookup)

	award_bounty {
		setup_pot_account::<T, I>();
		let (curator_lookup, bounty_id) = create_bounty::<T, I>()?;
		Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());

		let bounty_id = BountyCount::<T, I>::get() - 1;
		let curator = T::Lookup::lookup(curator_lookup).map_err(<&str>::from)?;

		let beneficiary = T::BeneficiaryLookup::unlookup(account("beneficiary", 0, SEED));
	}: _(RawOrigin::Signed(curator), bounty_id, beneficiary)

	claim_bounty {
		setup_pot_account::<T, I>();
		let (curator_lookup, bounty_id) = create_bounty::<T, I>()?;
		Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());

		let bounty_id = BountyCount::<T, I>::get() - 1;
		let curator = T::Lookup::lookup(curator_lookup).map_err(<&str>::from)?;

		let beneficiary_account: T::Beneficiary = account("beneficiary", 0, SEED);
		let beneficiary = T::BeneficiaryLookup::unlookup(beneficiary_account.clone());
		Bounties::<T, I>::award_bounty(RawOrigin::Signed(curator.clone()).into(), bounty_id, beneficiary)?;

		set_block_number::<T, I>(T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get() + 1u32.into());
		// TODO: ensure check passes with asset
		// ensure!(T::Currency::free_balance(&beneficiary_account).is_zero(), "Beneficiary already has balance");

	}: _(RawOrigin::Signed(curator), bounty_id)
	verify {
		// TODO: ensure check passes with asset
		// ensure!(!T::Currency::free_balance(&beneficiary_account).is_zero(), "Beneficiary didn't get paid");
	}

	close_bounty_proposed {
		setup_pot_account::<T, I>();
		let (caller, curator, asset_kind, fee, value, _curator_stash, reason) = setup_bounty::<T, I>(0, 0);
		Bounties::<T, I>::propose_bounty(RawOrigin::Signed(caller).into(), Box::new(asset_kind), value, reason)?;
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let approve_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: close_bounty<T::RuntimeOrigin>(approve_origin, bounty_id)

	close_bounty_active {
		setup_pot_account::<T, I>();
		let (curator_lookup, bounty_id) = create_bounty::<T, I>()?;
		Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let approve_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: close_bounty<T::RuntimeOrigin>(approve_origin, bounty_id)
	verify {
		assert_last_event::<T, I>(Event::BountyCanceled { index: bounty_id }.into())
	}

	extend_bounty_expiry {
		setup_pot_account::<T, I>();
		let (curator_lookup, bounty_id) = create_bounty::<T, I>()?;
		Treasury::<T, I>::on_initialize(frame_system::Pallet::<T>::block_number());

		let bounty_id = BountyCount::<T, I>::get() - 1;
		let curator = T::Lookup::lookup(curator_lookup).map_err(<&str>::from)?;
	}: _(RawOrigin::Signed(curator), bounty_id, Vec::new())
	verify {
		assert_last_event::<T, I>(Event::BountyExtended { index: bounty_id }.into())
	}

	impl_benchmark_test_suite!(Bounties, crate::tests::ExtBuilder::default().build(), crate::tests::Test)
}
