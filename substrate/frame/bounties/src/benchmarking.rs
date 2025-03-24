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

#![cfg(feature = "runtime-benchmarks")]

use super::*;
use crate::{Pallet as Bounties};

use alloc::{vec, vec::Vec};
use frame_benchmarking::v1::{
	account, benchmarks_instance_pallet, whitelisted_caller, BenchmarkError,
};
use frame_support::traits::Currency;
use frame_system::{pallet_prelude::BlockNumberFor as SystemBlockNumberFor, RawOrigin};
use pallet_treasury::Pallet as Treasury;
use sp_core::crypto::FromEntropy;
use sp_runtime::traits::BlockNumberProvider;

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

// Create the pre-requisite information needed to create a treasury `propose_bounty`.
fn setup_bounty<T: Config<I>, I: 'static>(
	user: u32,
	description: u32,
) -> (
	T::AccountId,
	T::AccountId,
	T::AssetKind,
	BalanceOf<T, I>,
	BalanceOf<T, I>,
	T::Beneficiary,
	Vec<u8>,
) {
	let caller = account("caller", user, SEED);
	// Tiago: check with Muharem if we need coupling with pallet-assets
	// let value: BalanceOf<T, I> = T::BountyValueMinimum::get().saturating_mul(100u32.into());
	let asset_kind = <T as Config<I>>::BenchmarkHelper::create_asset_kind(SEED);
	let value: BalanceOf<T, I> = 100_000u32.into();
	// Tiago: check with Muharem if we need coupling with pallet-assets
	// TODO: revisit asset conversion
	// let native_value =
	// 	T::BalanceConverter::from_asset_balance(100u32.into(), asset_kind).unwrap_or(100u32.into());
	let fee: BalanceOf<T, I> = value / 2u32.into();
	let deposit = T::BountyDepositBase::get() +
		T::DataDepositPerByte::get() * T::MaximumReasonLength::get().into();
	let _ = T::Currency::make_free_balance_be(&caller, deposit + T::Currency::minimum_balance());
	let curator = account("curator", user, SEED);
	let curator_stash = account("curator_stash", user, SEED);
	let curator_deposit =
		Pallet::<T, I>::calculate_curator_deposit(&fee, asset_kind.clone()).expect("");
	let _ = T::Currency::make_free_balance_be(
		&curator,
		curator_deposit + T::Currency::minimum_balance(),
	);
	let reason = vec![0; description as usize];
	(caller, curator, asset_kind, fee, value, curator_stash, reason)
}

fn create_proposed_bounty<T: Config<I>, I: 'static>() -> Result<BountyIndex, BenchmarkError> {
	let (caller, _curator, asset_kind, _fee, value, _curator_stash, reason) =
		setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
	Bounties::<T, I>::propose_bounty(
		RawOrigin::Signed(caller.clone()).into(),
		Box::new(asset_kind.clone()),
		value,
		reason,
	)?;
	let bounty_id = BountyCount::<T, I>::get() - 1;
	Ok(bounty_id)
}

fn initialize_approved_bounty<T: Config<I>, I: 'static>(
	bounty_id: BountyIndex,
	caller: T::AccountId,
) -> Result<(), BenchmarkError> {
	let approve_origin =
		T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	Bounties::<T, I>::approve_bounty(approve_origin.clone(), bounty_id)?;
	Ok(())
}

fn approve_bounty_and_propose_curator<T: Config<I>, I: 'static>(
) -> Result<(T::AccountId, BountyIndex), BenchmarkError> {
	let (caller, curator, _asset_kind, fee, _value, curator_stash, _reason) =
		setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

	let bounty_id = create_proposed_bounty::<T, I>()?;
	initialize_approved_bounty::<T, I>(bounty_id, caller.clone())?;
	Bounties::<T, I>::check_payment_status(RawOrigin::Signed(caller).into(), bounty_id)?;

	let curator_lookup = T::Lookup::unlookup(curator.clone());
	Bounties::<T, I>::propose_curator(
		T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?,
		bounty_id,
		curator_lookup.clone(),
		fee,
	)?;
	Ok((curator, bounty_id))
}

fn approve_bounty_and_accept_curator<T: Config<I>, I: 'static>(
) -> Result<(T::AccountId, BountyIndex), BenchmarkError> {
	let (_caller, _curator, _asset_kind, _fee, _value, curator_stash, _reason) =
		setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
	let (curator, bounty_id) = approve_bounty_and_propose_curator::<T, I>()?;
	let curator_stash_lookup = T::BeneficiaryLookup::unlookup(curator_stash);
	Bounties::<T, I>::accept_curator(
		RawOrigin::Signed(curator.clone()).into(),
		bounty_id,
		curator_stash_lookup,
	)?;
	Ok((curator, bounty_id))
}

fn create_curator_and_award_bounty<T: Config<I>, I: 'static>(
) -> Result<(T::AccountId, BountyIndex), BenchmarkError> {
	let (_caller, curator, _asset_kind, _fee, _value, curator_stash, _reason) =
		setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
	let (curator, bounty_id) = approve_bounty_and_accept_curator::<T, I>()?;
	let beneficiary_account: T::Beneficiary = account("beneficiary", 0, SEED);
	let beneficiary = T::BeneficiaryLookup::unlookup(beneficiary_account.clone());

	Bounties::<T, I>::award_bounty(
		RawOrigin::Signed(curator.clone()).into(),
		bounty_id,
		beneficiary.clone(),
	)?;

	set_block_number::<T, I>(
		T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get() + 1u32.into(),
	);

	Ok((curator, bounty_id))
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
		let bounty_id = create_proposed_bounty::<T, I>()?;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: _<T::RuntimeOrigin>(approve_origin, bounty_id)

	propose_curator {
		let (caller, curator, asset_kind, fee, value, _curator_stash, _reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_lookup = T::Lookup::unlookup(curator);
		let bounty_id = create_proposed_bounty::<T, I>()?;
		initialize_approved_bounty::<T, I>(bounty_id, caller.clone())?;
		Bounties::<T, I>::check_payment_status(RawOrigin::Signed(caller).into(), bounty_id)?;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: _<T::RuntimeOrigin>(approve_origin, bounty_id, curator_lookup, fee)

	approve_bounty_with_curator {
		let (_caller, curator, _asset_kind, fee, _value, _curator_stash, _reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_lookup = T::Lookup::unlookup(curator.clone());
		let bounty_id = create_proposed_bounty::<T, I>()?;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: _<T::RuntimeOrigin>(approve_origin, bounty_id, curator_lookup, fee)
	verify {
		assert_last_event::<T, I>(
			Event::CuratorProposed { bounty_id, curator }.into()
		);
	}

	// Worst case when curator is inactive and any sender unassigns the curator,
	// or if `BountyUpdatePeriod` is large enough and `RejectOrigin` executes the call.
	unassign_curator {
		let (caller, curator, asset_kind, fee, value, _curator_stash, _reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_lookup = T::Lookup::unlookup(curator);
		let bounty_id = BountyCount::<T, I>::get() - 1;
		let bounty_update_period = T::BountyUpdatePeriod::get();
		let inactivity_timeout = T::SpendPeriod::get().saturating_add(bounty_update_period);
		set_block_number::<T, I>(inactivity_timeout.saturating_add(2u32.into()));

		// If `BountyUpdatePeriod` overflows the inactivity timeout the benchmark still executes the slash
		let origin = if Pallet::<T, I>::treasury_block_number() <= inactivity_timeout {
			let curator = T::Lookup::lookup(curator_lookup).map_err(<&str>::from)?;
			T::RejectOrigin::try_successful_origin().unwrap_or_else(|_| RawOrigin::Signed(curator).into())
		} else {
			let caller = whitelisted_caller();
			RawOrigin::Signed(caller).into()
		};
	}: _<T::RuntimeOrigin>(origin, bounty_id)

	accept_curator {
		let (_caller, _curator, _asset_kind, _fee, _value, curator_stash, _reason) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let (curator, bounty_id) = approve_bounty_and_propose_curator::<T, I>()?;
		let curator_stash_lookup = T::BeneficiaryLookup::unlookup(curator_stash);
	}: _(RawOrigin::Signed(curator), bounty_id, curator_stash_lookup)

	award_bounty {
		let (curator, bounty_id) = approve_bounty_and_accept_curator::<T, I>()?;
		let beneficiary = T::BeneficiaryLookup::unlookup(account("beneficiary", 0, SEED));
	}: _(RawOrigin::Signed(curator), bounty_id, beneficiary)

	claim_bounty {
		let (curator, bounty_id) = create_curator_and_award_bounty::<T, I>()?;
		set_block_number::<T, I>(T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get() + 1u32.into());
		// TODO: ensure check passes with asset
		// ensure!(T::Currency::free_balance(&beneficiary_account).is_zero(), "Beneficiary already has balance");

	}: _(RawOrigin::Signed(curator), bounty_id)
	verify {
		// Tiago: check with Muharem if we need coupling with pallet-assets
		// TODO: ensure check passes with asset
		// ensure!(!T::Currency::free_balance(&beneficiary_account).is_zero(), "Beneficiary didn't get paid");
	}

	close_bounty_proposed {
		let bounty_id = create_proposed_bounty::<T, I>()?;
		let approve_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: close_bounty<T::RuntimeOrigin>(approve_origin, bounty_id)

	close_bounty_active {
		let (_curator, bounty_id) = approve_bounty_and_accept_curator::<T, I>()?;
		let approve_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	}: close_bounty<T::RuntimeOrigin>(approve_origin, bounty_id)
	verify {
		let caller = account("caller", 0, SEED);
		Bounties::<T, I>::check_payment_status(RawOrigin::Signed(caller).into(), bounty_id)?;
		assert_last_event::<T, I>(Event::BountyCanceled { index: bounty_id }.into())
	}

	extend_bounty_expiry {
		let (curator, bounty_id) = approve_bounty_and_accept_curator::<T, I>()?;
	}: _(RawOrigin::Signed(curator), bounty_id, Vec::new())
	verify {
		assert_last_event::<T, I>(Event::BountyExtended { index: bounty_id }.into())
	}

	check_payment_status_approved {
		let (caller, _curator, _asset_kind, _fee, _value, _curator_stash, _description) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let bounty_id = create_proposed_bounty::<T, I>()?;
		initialize_approved_bounty::<T, I>(bounty_id, caller.clone())?;
	}: check_payment_status<T::RuntimeOrigin>(RawOrigin::Signed(caller).into(), bounty_id)

	check_payment_status_approved_with_curator {
		let (caller, curator, _asset_kind, fee, _value, _curator_stash, _description) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let bounty_id = create_proposed_bounty::<T, I>()?;
		let curator_lookup = T::Lookup::unlookup(curator);
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::approve_bounty_with_curator(approve_origin, bounty_id, curator_lookup, fee)?;
	}: check_payment_status<T::RuntimeOrigin>(RawOrigin::Signed(caller).into(), bounty_id)

	check_payment_status_payout_attempted {
		let (curator, bounty_id) = create_curator_and_award_bounty::<T, I>()?;
		Bounties::<T, I>::claim_bounty(RawOrigin::Signed(curator.clone()).into(), bounty_id)?;
	}: check_payment_status<T::RuntimeOrigin>(RawOrigin::Signed(curator).into(), bounty_id)

	check_payment_status_refund_attempted {
		let (curator, bounty_id) = approve_bounty_and_accept_curator::<T, I>()?;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::close_bounty(approve_origin.clone(), bounty_id)?;
	}: check_payment_status<T::RuntimeOrigin>(RawOrigin::Signed(curator).into(), bounty_id)

	process_payment_approved {
		let (caller, _curator, _asset_kind, _fee, _value, _curator_stash, _description) = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let bounty_id = create_proposed_bounty::<T, I>()?;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::approve_bounty(approve_origin.clone(), bounty_id)?;
		Bounties::<T, I>::check_payment_status(RawOrigin::Signed(caller.clone()).into(), bounty_id)?;
	}: process_payment<T::RuntimeOrigin>(RawOrigin::Signed(caller).into(), bounty_id)

	process_payment_payout_attempted {
		let (curator, bounty_id) = create_curator_and_award_bounty::<T, I>()?;
		Bounties::<T, I>::claim_bounty(RawOrigin::Signed(curator.clone()).into(), bounty_id)?;
		Bounties::<T, I>::check_payment_status(RawOrigin::Signed(curator.clone()).into(), bounty_id)?;
	}: process_payment<T::RuntimeOrigin>(RawOrigin::Signed(curator).into(), bounty_id)

	process_payment_refund_attempted {
		let (curator, bounty_id) = approve_bounty_and_accept_curator::<T, I>()?;
		let approve_origin = T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::close_bounty(approve_origin.clone(), bounty_id)?;
		Bounties::<T, I>::check_payment_status(RawOrigin::Signed(curator.clone()).into(), bounty_id)?;
	}: process_payment<T::RuntimeOrigin>(RawOrigin::Signed(curator).into(), bounty_id)

	impl_benchmark_test_suite!(Bounties, crate::mock::ExtBuilder::default().build(), crate::mock::Test)
}
