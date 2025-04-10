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

use super::*;
use crate as pallet_child_bounties;
use crate::Pallet as ChildBounties;
use pallet_bounties::BountyStatus;

use alloc::vec;
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_system::RawOrigin;
use pallet_bounties::Pallet as Bounties;
use sp_core::crypto::FromEntropy;
use sp_runtime::traits::BlockNumberProvider;

const SEED: u32 = 0;

/// Trait describing factory functions for dispatchables' parameters.
pub trait ArgumentsFactory<AssetKind, Beneficiary> {
	/// Factory function for an asset kind.
	fn create_asset_kind(seed: u32) -> AssetKind;
	/// Factory function for a beneficiary.
	fn create_beneficiary(seed: [u8; 32]) -> Beneficiary;
}

/// Implementation that expects the parameters implement the [`FromEntropy`] trait.
impl<AssetKind, Beneficiary> ArgumentsFactory<AssetKind, Beneficiary> for ()
where
	AssetKind: FromEntropy,
	Beneficiary: FromEntropy,
{
	fn create_asset_kind(seed: u32) -> AssetKind {
		AssetKind::from_entropy(&mut seed.encode().as_slice()).unwrap()
	}

	fn create_beneficiary(seed: [u8; 32]) -> Beneficiary {
		Beneficiary::from_entropy(&mut seed.as_slice()).unwrap()
	}
}

#[derive(Clone)]
struct BenchmarkChildBounty<T: Config<I>, I: 'static> {
	/// Bounty ID.
	bounty_id: BountyIndex,
	/// ChildBounty ID.
	child_bounty_id: BountyIndex,
	/// The account proposing it.
	caller: T::AccountId,
	/// The parent bounty curator account.
	curator: T::AccountId,
	/// The parent bounty curator stash account.
	curator_stash: T::Beneficiary,
	/// The child-bounty curator account.
	child_curator: T::AccountId,
	/// The child-bounty curator stash account.
	child_curator_stash: T::Beneficiary,
	/// The kind of asset this child-bounty is rewarded in.
	asset_kind: T::AssetKind,
	/// The (total) amount that should be paid if the bounty is rewarded.
	value: BalanceOf<T, I>,
	/// The curator fee. included in value.
	fee: BalanceOf<T, I>,
	/// The (total) amount that should be paid if the child-bounty is rewarded.
	child_value: BalanceOf<T, I>,
	/// The child-bounty curator fee. included in value.
	child_fee: BalanceOf<T, I>,
	/// The child-bounty beneficiary account.
	beneficiary: T::Beneficiary,
	/// Bounty description.
	description: Vec<u8>,
}

fn set_block_number<T: Config<I>, I: 'static>(n: BlockNumberFor<T, I>) {
	<T as pallet_treasury::Config<I>>::BlockNumberProvider::set_block_number(n);
}

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

pub fn get_parent_payment_id<T: Config<I>, I: 'static>(
	bounty_id: BountyIndex,
) -> Option<PaymentIdOf<T, I>> {
	let bounty = pallet_bounties::Bounties::<T, I>::get(bounty_id).expect("no bounty");

	match bounty.status {
		BountyStatus::Approved { payment_status: PaymentState::Attempted { id } } => Some(id),
		BountyStatus::ApprovedWithCurator {
			payment_status: PaymentState::Attempted { id },
			..
		} => Some(id),
		_ => None,
	}
}

pub fn get_child_payment_id<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: BountyIndex,
	to: Option<T::Beneficiary>,
) -> Option<PaymentIdOf<T, I>> {
	let child_bounty =
		pallet_child_bounties::ChildBounties::<T, I>::get(parent_bounty_id, child_bounty_id)
			.expect("no bounty");

	match child_bounty.status {
		ChildBountyStatus::Approved { payment_status: PaymentState::Attempted { id } } => Some(id),
		ChildBountyStatus::ApprovedWithCurator {
			payment_status: PaymentState::Attempted { id },
			..
		} => Some(id),
		ChildBountyStatus::RefundAttempted {
			payment_status: PaymentState::Attempted { id },
			..
		} => Some(id),
		ChildBountyStatus::PayoutAttempted { curator_stash, beneficiary, .. } =>
			to.and_then(|account| {
				if account == curator_stash.0 {
					if let PaymentState::Attempted { id } = curator_stash.1 {
						return Some(id);
					}
				} else if account == beneficiary.0 {
					if let PaymentState::Attempted { id } = beneficiary.1 {
						return Some(id);
					}
				}
				None
			}),
		_ => None,
	}
}

pub fn set_child_payment_status<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: BountyIndex,
	new_payment_status: PaymentState<PaymentIdOf<T, I>>,
) {
	let mut child_bounty =
		pallet_child_bounties::ChildBounties::<T, I>::get(parent_bounty_id, child_bounty_id)
			.expect("no bounty");

	match &mut child_bounty.status {
		ChildBountyStatus::Approved { payment_status } |
		ChildBountyStatus::RefundAttempted { payment_status, .. } => *payment_status = new_payment_status,
		ChildBountyStatus::PayoutAttempted { curator_stash, beneficiary, .. } => {
			curator_stash.1 = new_payment_status.clone();
			beneficiary.1 = new_payment_status;
		},
		_ => {},
	};

	pallet_child_bounties::ChildBounties::<T, I>::insert(parent_bounty_id, child_bounty_id, child_bounty);
}

fn setup_parent_bounty<T: Config<I>, I: 'static>(
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
	let curator = account("curator", user, SEED);
	let curator_stash =
		<T as Config<I>>::BenchmarkHelper::create_beneficiary([SEED.try_into().unwrap(); 32]);
	let asset_kind = <T as Config<I>>::BenchmarkHelper::create_asset_kind(SEED);
	let value: BalanceOf<T, I> = 100_000u32.into();
	let fee = value / 2u32.into();
	let deposit = T::BountyDepositBase::get() +
		T::DataDepositPerByte::get() * T::MaximumReasonLength::get().into();
	let _ = T::Currency::make_free_balance_be(&caller, deposit + T::Currency::minimum_balance());
	let curator_deposit =
		Bounties::<T, I>::calculate_curator_deposit(&fee, asset_kind.clone()).expect("");
	let _ = T::Currency::make_free_balance_be(
		&curator,
		curator_deposit + T::Currency::minimum_balance(),
	);
	let description = vec![0; description as usize];
	(caller, curator, asset_kind, fee, value, curator_stash, description)
}

fn setup_child_bounty<T: Config<I>, I: 'static>(
	user: u32,
	description: u32,
) -> BenchmarkChildBounty<T, I> {
	let (caller, curator, asset_kind, fee, value, curator_stash, description) =
		setup_parent_bounty::<T, I>(user, description);

	let child_curator = account("child-curator", user, SEED);
	let child_curator_stash =
		<T as Config<I>>::BenchmarkHelper::create_beneficiary([SEED.try_into().unwrap(); 32]);
	let child_value = (value - fee) / 4u32.into();
	let child_fee = child_value / 2u32.into();
	let child_curator_deposit = ChildBounties::<T, I>::calculate_curator_deposit(
		&curator,
		&child_curator,
		&child_fee,
		asset_kind.clone(),
	)
	.expect("");
	let beneficiary =
		<T as Config<I>>::BenchmarkHelper::create_beneficiary([(SEED + 1).try_into().unwrap(); 32]);
	let _ = T::Currency::make_free_balance_be(
		&child_curator,
		child_curator_deposit + T::Currency::minimum_balance(),
	);

	BenchmarkChildBounty::<T, I> {
		bounty_id: 0,
		child_bounty_id: 0,
		caller,
		curator,
		curator_stash,
		child_curator,
		child_curator_stash,
		asset_kind,
		value,
		fee,
		child_value,
		child_fee,
		beneficiary,
		description,
	}
}

fn initialize_parent_bounty<T: Config<I>, I: 'static>(
	user: u32,
	description: u32,
) -> Result<BenchmarkChildBounty<T, I>, BenchmarkError> {
	let mut setup = setup_child_bounty::<T, I>(user, description);

	Bounties::<T, I>::propose_bounty(
		RawOrigin::Signed(setup.caller.clone()).into(),
		Box::new(setup.asset_kind.clone()),
		setup.value,
		setup.description.clone(),
	)?;

	let bounty_id = pallet_bounties::BountyCount::<T, I>::get() - 1;
	setup.bounty_id = bounty_id;

	let approve_origin =
		T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
	let treasury_account = Bounties::<T, I>::account_id();
	let bounty_account = Bounties::<T, I>::bounty_account_id(bounty_id, setup.asset_kind.clone())
		.expect("conversion failed");
	<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
		&treasury_account,
		&bounty_account,
		setup.asset_kind.clone(),
		setup.value,
	);
	T::BalanceConverter::ensure_successful(setup.asset_kind.clone());
	Bounties::<T, I>::approve_bounty(approve_origin, setup.bounty_id)?;
	let payment_id = get_parent_payment_id::<T, I>(setup.bounty_id).expect("no payment attempt");
	<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
	Bounties::<T, I>::check_payment_status(
		RawOrigin::Signed(setup.caller.clone()).into(),
		bounty_id,
	)?;

	let curator_lookup = T::Lookup::unlookup(setup.curator.clone());
	set_block_number::<T, I>(T::SpendPeriod::get());
	Bounties::<T, I>::propose_curator(
		RawOrigin::Root.into(),
		setup.bounty_id,
		curator_lookup,
		setup.fee,
	)?;

	let curator = setup.curator.clone();
	let curator_stash_lookup = T::BeneficiaryLookup::unlookup(setup.curator_stash.clone());
	Bounties::<T, I>::accept_curator(
		RawOrigin::Signed(curator).into(),
		setup.bounty_id,
		curator_stash_lookup,
	)?;

	Ok(setup)
}

fn initialize_child_bounty<T: Config<I>, I: 'static>(
) -> Result<BenchmarkChildBounty<T, I>, BenchmarkError> {
	let mut setup = initialize_parent_bounty::<T, I>(0, T::MaximumReasonLength::get())?;

	let bounty_account =
		Bounties::<T, I>::bounty_account_id(setup.bounty_id, setup.asset_kind.clone())?;
	let child_bounty_account =
		ChildBounties::<T, I>::child_bounty_account_id(setup.bounty_id, setup.child_bounty_id);
	<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
		&bounty_account,
		&child_bounty_account,
		setup.asset_kind.clone(),
		setup.value,
	);
	T::BalanceConverter::ensure_successful(setup.asset_kind.clone());
	ChildBounties::<T, I>::add_child_bounty(
		RawOrigin::Signed(setup.curator.clone()).into(),
		setup.bounty_id,
		setup.child_value,
		setup.description.clone(),
	)?;

	let child_bounty_id = ParentTotalChildBounties::<T, I>::get(setup.bounty_id) - 1;
	setup.child_bounty_id = child_bounty_id;

	Ok(setup)
}

fn create_funded_child_bounty<T: Config<I>, I: 'static>(
) -> Result<BenchmarkChildBounty<T, I>, BenchmarkError> {
	let setup = initialize_child_bounty::<T, I>()?;
	let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
		.expect("no payment attempt");

	<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
	ChildBounties::<T, I>::check_payment_status(
		RawOrigin::Signed(setup.caller.clone()).into(),
		setup.bounty_id,
		setup.child_bounty_id,
	)?;

	Ok(setup)
}

fn create_funded_child_bounty_and_propose_curator<T: Config<I>, I: 'static>(
) -> Result<BenchmarkChildBounty<T, I>, BenchmarkError> {
	let setup = create_funded_child_bounty::<T, I>()?;
	let child_curator_lookup = T::Lookup::unlookup(setup.child_curator.clone());

	let _ = ChildBounties::<T, I>::propose_curator(
		RawOrigin::Signed(setup.curator.clone()).into(),
		setup.bounty_id,
		setup.child_bounty_id,
		child_curator_lookup,
		setup.child_fee,
	);

	Ok(setup)
}

fn create_funded_child_bounty_with_curator<T: Config<I>, I: 'static>(
) -> Result<BenchmarkChildBounty<T, I>, BenchmarkError> {
	let setup = create_funded_child_bounty_and_propose_curator::<T, I>()?;
	let child_curator_stash_lookup =
		T::BeneficiaryLookup::unlookup(setup.child_curator_stash.clone());

	let _ = ChildBounties::<T, I>::accept_curator(
		RawOrigin::Signed(setup.child_curator.clone()).into(),
		setup.bounty_id,
		setup.child_bounty_id,
		child_curator_stash_lookup,
	);

	Ok(setup)
}

fn create_awarded_child_bounty<T: Config<I>, I: 'static>(
) -> Result<BenchmarkChildBounty<T, I>, BenchmarkError> {
	let setup = create_funded_child_bounty_with_curator::<T, I>()?;
	let beneficiary = T::BeneficiaryLookup::unlookup(setup.beneficiary.clone());

	let _ = ChildBounties::<T, I>::award_child_bounty(
		RawOrigin::Signed(setup.child_curator.clone()).into(),
		setup.bounty_id,
		setup.child_bounty_id,
		beneficiary,
	);

	Ok(setup)
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn add_child_bounty(
		d: Linear<0, { T::MaximumReasonLength::get() }>,
	) -> Result<(), BenchmarkError> {
		let bounty_setup = initialize_parent_bounty::<T, I>(0, d)?;

		#[extrinsic_call]
		_(
			RawOrigin::Signed(bounty_setup.curator),
			bounty_setup.bounty_id,
			bounty_setup.child_value,
			bounty_setup.description,
		);

		let child_bounty_id = ParentTotalChildBounties::<T, I>::get(bounty_setup.bounty_id) - 1;
		assert_last_event::<T, I>(
			Event::Added {
				index: bounty_setup.bounty_id,
				child_index: bounty_setup.child_bounty_id,
			}
			.into(),
		);
		let payment_id =
			get_child_payment_id::<T, I>(bounty_setup.bounty_id, child_bounty_id, None)
				.expect("no payment attempt");
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);
		Ok(())
	}

	#[benchmark]
	fn propose_curator() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty::<T, I>()?;
		let child_curator_lookup = T::Lookup::unlookup(setup.child_curator);

		#[extrinsic_call]
		_(
			RawOrigin::Signed(setup.curator),
			setup.bounty_id,
			setup.child_bounty_id,
			child_curator_lookup,
			setup.child_fee,
		);

		Ok(())
	}

	#[benchmark]
	fn accept_curator() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty_and_propose_curator::<T, I>()?;
		let child_curator_stash_lookup =
			T::BeneficiaryLookup::unlookup(setup.child_curator_stash.clone());

		#[extrinsic_call]
		_(
			RawOrigin::Signed(setup.child_curator),
			setup.bounty_id,
			setup.child_bounty_id,
			child_curator_stash_lookup,
		);

		Ok(())
	}

	// Worst case when curator is inactive and any sender un-assigns the curator,
	// or if `BountyUpdatePeriod` is large enough and `RejectOrigin` executes the call.
	#[benchmark]
	fn unassign_curator() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty_with_curator::<T, I>()?;

		let bounty_update_period = T::BountyUpdatePeriod::get();
		let inactivity_timeout = T::SpendPeriod::get().saturating_add(bounty_update_period);
		set_block_number::<T, I>(inactivity_timeout.saturating_add(1u32.into()));

		// If `BountyUpdatePeriod` overflows the inactivity timeout the benchmark still
		// executes the slash
		let origin: T::RuntimeOrigin =
			if Pallet::<T, I>::treasury_block_number() <= inactivity_timeout {
				let child_curator = setup.child_curator;
				T::RejectOrigin::try_successful_origin()
					.unwrap_or_else(|_| RawOrigin::Signed(child_curator).into())
			} else {
				let caller = whitelisted_caller();
				RawOrigin::Signed(caller).into()
			};

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, setup.bounty_id, setup.child_bounty_id);

		Ok(())
	}

	#[benchmark]
	fn award_child_bounty() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty_with_curator::<T, I>()?;
		let beneficiary = T::BeneficiaryLookup::unlookup(setup.beneficiary.clone());

		#[extrinsic_call]
		_(
			RawOrigin::Signed(setup.child_curator),
			setup.bounty_id,
			setup.child_bounty_id,
			beneficiary,
		);

		assert_last_event::<T, I>(
			Event::Awarded {
				index: setup.bounty_id,
				child_index: setup.child_bounty_id,
				beneficiary: setup.beneficiary,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn claim_child_bounty() -> Result<(), BenchmarkError> {
		let setup = create_awarded_child_bounty::<T, I>()?;

		set_block_number::<T, I>(T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get());

		#[extrinsic_call]
		_(RawOrigin::Signed(setup.curator), setup.bounty_id, setup.child_bounty_id);

		if let Some(child_bounty) = pallet_child_bounties::ChildBounties::<T, I>::get(
			setup.bounty_id,
			setup.child_bounty_id,
		) {
			assert!(matches!(child_bounty.status, ChildBountyStatus::PayoutAttempted { .. }));
		}

		Ok(())
	}

	// Best case scenario.
	#[benchmark]
	fn close_child_bounty_added() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty::<T, I>()?;

		#[extrinsic_call]
		close_child_bounty(RawOrigin::Root, setup.bounty_id, setup.child_bounty_id);

		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);
		assert_last_event::<T, I>(
			Event::Canceled { index: setup.bounty_id, child_index: setup.child_bounty_id }.into(),
		);
		assert!(ChildBounties::<T, I>::close_child_bounty(
			RawOrigin::Root.into(),
			setup.bounty_id,
			setup.child_bounty_id
		)
		.is_err());

		Ok(())
	}

	// Worst case scenario.
	#[benchmark]
	fn close_child_bounty_active() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty_with_curator::<T, I>()?;

		#[extrinsic_call]
		close_child_bounty(RawOrigin::Root, setup.bounty_id, setup.child_bounty_id);

		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);
		assert_last_event::<T, I>(
			Event::Canceled { index: setup.bounty_id, child_index: setup.child_bounty_id }.into(),
		);
		assert!(ChildBounties::<T, I>::close_child_bounty(
			RawOrigin::Root.into(),
			setup.bounty_id,
			setup.child_bounty_id
		)
		.is_err());

		Ok(())
	}

	#[benchmark]
	fn check_payment_status_approved() -> Result<(), BenchmarkError> {
		let setup = initialize_child_bounty::<T, I>()?;

		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);

		#[extrinsic_call]
		check_payment_status(
			RawOrigin::Signed(setup.curator.clone()),
			setup.bounty_id,
			setup.child_bounty_id,
		);

		assert_last_event::<T, I>(
			Event::BecameActive { index: setup.bounty_id, child_index: setup.child_bounty_id }
				.into(),
		);
		let child_bounty = pallet_child_bounties::ChildBounties::<T, I>::get(
			setup.bounty_id,
			setup.child_bounty_id,
		)
		.expect("no bounty");
		assert!(!matches!(child_bounty.status, ChildBountyStatus::Approved { .. }));

		Ok(())
	}

	#[benchmark]
	fn check_payment_status_refund_attempted() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty_with_curator::<T, I>()?;

		let child_bounty_account =
			ChildBounties::<T, I>::child_bounty_account_id(setup.bounty_id, setup.child_bounty_id);
		let bounty_account =
			Bounties::<T, I>::bounty_account_id(setup.bounty_id, setup.asset_kind.clone())
				.expect("conversion failed");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
			&child_bounty_account,
			&bounty_account,
			setup.asset_kind.clone(),
			setup.value,
		);
		let _ = ChildBounties::<T, I>::close_child_bounty(
			RawOrigin::Root.into(),
			setup.bounty_id,
			setup.child_bounty_id,
		);
		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);

		#[extrinsic_call]
		check_payment_status(
			RawOrigin::Signed(setup.curator.clone()),
			setup.bounty_id,
			setup.child_bounty_id,
		);

		assert_last_event::<T, I>(
			Event::RefundProcessed { index: setup.bounty_id, child_index: setup.child_bounty_id }
				.into(),
		);
		assert!(pallet_child_bounties::ChildBounties::<T, I>::get(
			setup.bounty_id,
			setup.child_bounty_id
		)
		.is_none());

		Ok(())
	}

	#[benchmark]
	fn check_payment_status_payout_attempted() -> Result<(), BenchmarkError> {
		let setup = create_awarded_child_bounty::<T, I>()?;
		let (fee, asset_payout) =
			ChildBounties::<T, I>::calculate_curator_fee_and_payout(setup.child_fee, setup.child_value);

		let child_bounty_account =
			ChildBounties::<T, I>::child_bounty_account_id(setup.bounty_id, setup.child_bounty_id);
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
			&child_bounty_account,
			&setup.child_curator_stash,
			setup.asset_kind.clone(),
			fee,
		);
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
			&child_bounty_account,
			&setup.beneficiary,
			setup.asset_kind.clone(),
			asset_payout,
		);
		set_block_number::<T, I>(T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get());
		let _  = ChildBounties::<T, I>::claim_child_bounty(
			RawOrigin::Signed(setup.curator.clone()).into(),
			setup.bounty_id,
			setup.child_bounty_id,
		);
		let curator_payment_id = get_child_payment_id::<T, I>(
			setup.bounty_id,
			setup.child_bounty_id,
			Some(setup.child_curator_stash),
		)
		.expect("no payment attempt");
		let beneficiary_payment_id = get_child_payment_id::<T, I>(
			setup.bounty_id,
			setup.child_bounty_id,
			Some(setup.beneficiary.clone()),
		)
		.expect("no payment attempt");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(curator_payment_id);
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(beneficiary_payment_id);

		#[extrinsic_call]
		check_payment_status(
			RawOrigin::Signed(setup.curator.clone()),
			setup.bounty_id,
			setup.child_bounty_id,
		);

		assert_last_event::<T, I>(
			Event::PayoutProcessed {
				index: setup.bounty_id,
				child_index: setup.child_bounty_id,
				asset_kind: setup.asset_kind,
				value: asset_payout,
				beneficiary: setup.beneficiary,
			}
			.into(),
		);
		assert!(pallet_child_bounties::ChildBounties::<T, I>::get(
			setup.bounty_id,
			setup.child_bounty_id
		)
		.is_none());

		Ok(())
	}

	#[benchmark]
	fn process_payment_approved() -> Result<(), BenchmarkError> {
		let setup = initialize_child_bounty::<T, I>()?;
		
		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
		set_child_payment_status::<T, I>(setup.bounty_id, setup.child_bounty_id, PaymentState::Failed);

		#[extrinsic_call]
		process_payment(
			RawOrigin::Signed(setup.caller.clone()),
			setup.bounty_id,
			setup.child_bounty_id,
		);

		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		assert_last_event::<T, I>(Event::Paid { index: setup.bounty_id, child_index: setup.child_bounty_id, payment_id }.into());
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);
		assert!(ChildBounties::<T, I>::process_payment(
			RawOrigin::Signed(setup.caller.clone()).into(),
			setup.bounty_id,
			setup.child_bounty_id,
		)
		.is_err());

		Ok(())
	}

	#[benchmark]
	fn process_payment_refund_attempted() -> Result<(), BenchmarkError> {
		let setup = create_funded_child_bounty_with_curator::<T, I>()?;

		let _ = ChildBounties::<T, I>::close_child_bounty(
			RawOrigin::Root.into(),
			setup.bounty_id,
			setup.child_bounty_id,
		);
		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
		set_child_payment_status::<T, I>(setup.bounty_id, setup.child_bounty_id, PaymentState::Failed);

		#[extrinsic_call]
		process_payment(
			RawOrigin::Signed(setup.caller.clone()),
			setup.bounty_id,
			setup.child_bounty_id,
		);

		let payment_id = get_child_payment_id::<T, I>(setup.bounty_id, setup.child_bounty_id, None)
			.expect("no payment attempt");
		assert_last_event::<T, I>(Event::Paid { index: setup.bounty_id, child_index: setup.child_bounty_id, payment_id }.into());
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);
		assert!(ChildBounties::<T, I>::process_payment(
			RawOrigin::Signed(setup.caller.clone()).into(),
			setup.bounty_id,
			setup.child_bounty_id,
		)
		.is_err());

		Ok(())
	}

	#[benchmark]
	fn process_payment_payout_attempted() -> Result<(), BenchmarkError> {
		let setup = create_awarded_child_bounty::<T, I>()?;

		set_block_number::<T, I>(T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get());
		let _ = ChildBounties::<T, I>::claim_child_bounty(
			RawOrigin::Signed(setup.curator.clone()).into(),
			setup.bounty_id,
			setup.child_bounty_id,
		);
		let curator_payment_id = get_child_payment_id::<T, I>(
			setup.bounty_id,
			setup.child_bounty_id,
			Some(setup.child_curator_stash.clone()),
		).expect("no payment attempt");
		let beneficiary_payment_id = get_child_payment_id::<T, I>(
			setup.bounty_id,
			setup.child_bounty_id,
			Some(setup.beneficiary.clone()),
		).expect("no payment attempt");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(curator_payment_id);
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(beneficiary_payment_id);
		set_child_payment_status::<T, I>(setup.bounty_id, setup.child_bounty_id, PaymentState::Failed);

		#[extrinsic_call]
		process_payment(
			RawOrigin::Signed(setup.caller.clone()),
			setup.bounty_id,
			setup.child_bounty_id,
		);

		let curator_payment_id = get_child_payment_id::<T, I>(
			setup.bounty_id,
			setup.child_bounty_id,
			Some(setup.child_curator_stash),
		).expect("no payment attempt");
		let beneficiary_payment_id = get_child_payment_id::<T, I>(
			setup.bounty_id,
			setup.child_bounty_id,
			Some(setup.beneficiary.clone()),
		).expect("no payment attempt");
		assert_has_event::<T, I>(Event::Paid { index: setup.bounty_id, child_index: setup.child_bounty_id, payment_id: curator_payment_id }.into());
		assert_has_event::<T, I>(Event::Paid { index: setup.bounty_id, child_index: setup.child_bounty_id, payment_id: beneficiary_payment_id }.into());
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(curator_payment_id),
			PaymentStatus::Failure
		);
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(beneficiary_payment_id),
			PaymentStatus::Failure
		);
		assert!(ChildBounties::<T, I>::process_payment(
			RawOrigin::Signed(setup.caller.clone()).into(),
			setup.bounty_id,
			setup.child_bounty_id,
		)
		.is_err());

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::ExtBuilder::default().build(),
		crate::mock::Test
	}

	mod no_spend_origin_tests {
		use super::*;

		impl_benchmark_test_suite!(
			Pallet,
			crate::mock::ExtBuilder::default().spend_origin_succesful_origin_err().build(),
			crate::mock::Test,
			benchmarks_path = benchmarking
		);
	}

	mod max_bounty_update_period_tests {
		use super::*;

		impl_benchmark_test_suite!(
			Pallet,
			crate::mock::ExtBuilder::default().max_bounty_update_period().build(),
			crate::mock::Test,
			benchmarks_path = benchmarking
		);
	}
}
