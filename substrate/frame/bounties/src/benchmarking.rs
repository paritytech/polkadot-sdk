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
use crate as pallet_bounties;
use crate::Pallet as Bounties;

use alloc::{vec, vec::Vec};
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::{assert_err, assert_ok, traits::Currency};
use frame_system::RawOrigin;
use sp_core::crypto::FromEntropy;
use sp_runtime::traits::BlockNumberProvider;

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
struct BenchmarkBounty<T: Config<I>, I: 'static> {
	/// Bounty ID.
	bounty_id: BountyIndex,
	/// The account proposing it.
	caller: T::AccountId,
	/// The parent bounty proposer deposit.
	bond: BalanceOf<T, I>,
	/// The parent bounty curator account.
	curator: T::AccountId,
	/// The parent bounty curator stash account.
	curator_stash: T::Beneficiary,
	/// The kind of asset this child-bounty is rewarded in.
	asset_kind: T::AssetKind,
	/// The beneficiary stash account.
	beneficiary: T::Beneficiary,
	/// The (total) amount that should be paid if the bounty is rewarded.
	value: BalanceOf<T, I>,
	/// The curator fee. included in value.
	fee: BalanceOf<T, I>,
	/// Bounty description.
	description: Vec<u8>,
}

const SEED: u32 = 0;

fn set_block_number<T: Config<I>, I: 'static>(n: BlockNumberFor<T, I>) {
	<T as pallet_treasury::Config<I>>::BlockNumberProvider::set_block_number(n);
}

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

pub fn get_payment_id<T: Config<I>, I: 'static>(
	bounty_id: BountyIndex,
	to: Option<T::Beneficiary>,
) -> Option<PaymentIdOf<T, I>> {
	let bounty = pallet_bounties::Bounties::<T, I>::get(bounty_id).expect("no bounty");

	match bounty.status {
		BountyStatus::Approved { payment_status: PaymentState::Attempted { id } } => Some(id),
		BountyStatus::ApprovedWithCurator {
			payment_status: PaymentState::Attempted { id },
			..
		} => Some(id),
		BountyStatus::RefundAttempted {
			payment_status: PaymentState::Attempted { id }, ..
		} => Some(id),
		BountyStatus::PayoutAttempted { curator_stash, beneficiary, .. } =>
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

// Create the pre-requisite information needed to create a treasury `propose_bounty`.
fn setup_bounty<T: Config<I>, I: 'static>(user: u32, description: u32) -> BenchmarkBounty<T, I> {
	let caller = account("caller", user, SEED);
	let curator = account("curator", user, SEED);
	let curator_stash =
		<T as Config<I>>::BenchmarkHelper::create_beneficiary([SEED.try_into().unwrap(); 32]);
	let asset_kind = <T as Config<I>>::BenchmarkHelper::create_asset_kind(SEED);
	let beneficiary =
		<T as Config<I>>::BenchmarkHelper::create_beneficiary([(SEED + 1).try_into().unwrap(); 32]);
	let value: BalanceOf<T, I> = 100_000u32.into();
	let fee: BalanceOf<T, I> = value / 2u32.into();
	let deposit = T::BountyDepositBase::get() +
		T::DataDepositPerByte::get() * T::MaximumReasonLength::get().into();
	let _ = T::Currency::make_free_balance_be(&caller, deposit + T::Currency::minimum_balance());
	let curator_deposit =
		Pallet::<T, I>::calculate_curator_deposit(&fee, asset_kind.clone()).expect("");
	let _ = T::Currency::make_free_balance_be(
		&curator,
		curator_deposit + T::Currency::minimum_balance(),
	);
	let description = vec![0; description as usize];

	BenchmarkBounty::<T, I> {
		bounty_id: 0,
		caller,
		bond: deposit,
		curator,
		curator_stash,
		asset_kind,
		value,
		fee,
		beneficiary,
		description,
	}
}

fn create_proposed_bounty<T: Config<I>, I: 'static>(
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let mut setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
	Bounties::<T, I>::propose_bounty(
		RawOrigin::Signed(setup.caller.clone()).into(),
		Box::new(setup.asset_kind.clone()),
		setup.value,
		setup.description.clone(),
	)?;
	setup.bounty_id = BountyCount::<T, I>::get() - 1;
	Ok(setup)
}

fn initialize_approved_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let setup = create_proposed_bounty::<T, I>()?;
	T::BalanceConverter::ensure_successful(setup.asset_kind.clone());
	let treasury_account = Bounties::<T, I>::account_id();
	let bounty_account =
		Bounties::<T, I>::bounty_account_id(setup.bounty_id, setup.asset_kind.clone())
			.expect("conversion failed");
	<T as pallet::Config<I>>::Paymaster::ensure_successful(
		&treasury_account,
		&bounty_account,
		setup.asset_kind.clone(),
		setup.value,
	);
	Bounties::<T, I>::approve_bounty(origin, setup.bounty_id)?;
	Ok(setup)
}

fn create_funded_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let setup = initialize_approved_bounty::<T, I>(origin)?;
	let payment_id = get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
	<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id, PaymentStatus::Success);
	Bounties::<T, I>::check_payment_status(
		RawOrigin::Signed(setup.caller.clone()).into(),
		setup.bounty_id,
	)?;

	Ok(setup)
}

fn create_funded_bounty_and_propose_curator<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let setup = create_funded_bounty::<T, I>(origin.clone())?;
	let curator_lookup = T::Lookup::unlookup(setup.curator.clone());

	Bounties::<T, I>::propose_curator(origin, setup.bounty_id, curator_lookup, setup.fee)?;

	Ok(setup)
}

fn create_funded_bounty_with_curator<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let setup = create_funded_bounty_and_propose_curator::<T, I>(origin)?;
	let curator_stash_lookup = T::BeneficiaryLookup::unlookup(setup.curator_stash.clone());

	Bounties::<T, I>::accept_curator(
		RawOrigin::Signed(setup.curator.clone()).into(),
		setup.bounty_id,
		curator_stash_lookup,
	)?;

	Ok(setup)
}

fn create_awarded_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let setup = create_funded_bounty_with_curator::<T, I>(origin)?;
	let beneficiary_lookup = T::BeneficiaryLookup::unlookup(setup.beneficiary.clone());

	Bounties::<T, I>::award_bounty(
		RawOrigin::Signed(setup.curator.clone()).into(),
		setup.bounty_id,
		beneficiary_lookup,
	)?;

	set_block_number::<T, I>(
		T::SpendPeriod::get() + T::BountyDepositPayoutDelay::get() + 1u32.into(),
	);
	Ok(setup)
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	#[benchmark]
	fn propose_bounty(
		d: Linear<0, { T::MaximumReasonLength::get() }>,
	) -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, d);

		#[extrinsic_call]
		_(
			RawOrigin::Signed(setup.caller),
			Box::new(setup.asset_kind),
			setup.value,
			setup.description,
		);

		let bounty_id = BountyCount::<T, I>::get() - 1;
		assert_last_event::<T, I>(Event::BountyProposed { index: bounty_id }.into());

		Ok(())
	}

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `approve_bounty` is un-callable and can use weight=0.
	#[benchmark]
	fn approve_bounty() -> Result<(), BenchmarkError> {
		let setup = create_proposed_bounty::<T, I>()?;
		let approve_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(approve_origin.clone(), setup.bounty_id);

		assert_last_event::<T, I>(Event::BountyApproved { index: setup.bounty_id }.into());
		let payment_id = get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
		assert_ne!(
			<T as pallet::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);
		assert!(Bounties::<T, I>::approve_bounty(approve_origin, setup.bounty_id).is_err());
		Ok(())
	}

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `propose_curator` is un-callable and can use weight=0.
	#[benchmark]
	fn propose_curator() -> Result<(), BenchmarkError> {
		let approve_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let setup = create_funded_bounty::<T, I>(approve_origin.clone())?;
		let curator_lookup = T::Lookup::unlookup(setup.curator.clone());

		#[extrinsic_call]
		_(approve_origin, setup.bounty_id, curator_lookup, setup.fee);

		assert_last_event::<T, I>(
			Event::CuratorProposed { bounty_id: setup.bounty_id, curator: setup.curator.clone() }
				.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn accept_curator() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_stash_lookup = T::BeneficiaryLookup::unlookup(setup.curator_stash);

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_bounty_and_propose_curator::<T, I>(origin)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::accept_curator(
				RawOrigin::Signed(setup.curator.clone()).into(),
				setup.bounty_id,
				curator_stash_lookup,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::CuratorAccepted {
					bounty_id: setup.bounty_id,
					curator: setup.curator.clone(),
				}
				.into(),
			);
		}

		Ok(())
	}

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `approve_bounty` is un-callable and can use weight=0.
	#[benchmark]
	fn approve_bounty_with_curator() -> Result<(), BenchmarkError> {
		let setup = create_proposed_bounty::<T, I>()?;
		let curator_lookup = T::Lookup::unlookup(setup.curator.clone());
		let approve_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		_(approve_origin, setup.bounty_id, curator_lookup, setup.fee);

		assert_last_event::<T, I>(
			Event::CuratorProposed { bounty_id: setup.bounty_id, curator: setup.curator.clone() }
				.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn unassign_curator() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let curator_lookup = T::Lookup::unlookup(setup.curator.clone());

		let spend_exists = if let Ok(spend_origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_bounty_with_curator::<T, I>(spend_origin)?;
			true
		} else {
			false
		};

		let bounty_update_period = T::BountyUpdatePeriod::get();
		let inactivity_timeout = T::SpendPeriod::get().saturating_add(bounty_update_period);
		set_block_number::<T, I>(inactivity_timeout.saturating_add(2u32.into()));

		// If `BountyUpdatePeriod` overflows the inactivity timeout the benchmark still executes the
		// slash
		let call_origin = if Pallet::<T, I>::treasury_block_number() <= inactivity_timeout {
			let curator = T::Lookup::lookup(curator_lookup).map_err(<&str>::from)?;
			T::RejectOrigin::try_successful_origin()
				.unwrap_or_else(|_| RawOrigin::Signed(curator).into())
		} else {
			let caller = whitelisted_caller();
			RawOrigin::Signed(caller).into()
		};

		#[block]
		{
			let res = Bounties::<T, I>::unassign_curator(call_origin, setup.bounty_id);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::CuratorUnassigned { bounty_id: setup.bounty_id }.into(),
			);
		}

		Ok(())
	}

	#[benchmark]
	fn award_bounty() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let beneficiary_lookup = T::BeneficiaryLookup::unlookup(setup.beneficiary.clone());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_bounty_with_curator::<T, I>(origin)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::award_bounty(
				RawOrigin::Signed(setup.curator.clone()).into(),
				setup.bounty_id,
				beneficiary_lookup,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::BountyAwarded {
					index: setup.bounty_id,
					beneficiary: setup.beneficiary.clone(),
				}
				.into(),
			);
		}

		Ok(())
	}

	#[benchmark]
	fn claim_bounty() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_awarded_bounty::<T, I>(origin)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::claim_bounty(
				RawOrigin::Signed(setup.curator.clone()).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if let Some(bounty) = pallet_bounties::Bounties::<T, I>::get(setup.bounty_id) {
			assert!(matches!(bounty.status, BountyStatus::PayoutAttempted { .. }));
		}

		Ok(())
	}

	/// This benchmark is short-circuited if `RejectOrigin` cannot provide
	/// a successful origin, in which case `close_bounty` is un-callable and can use weight=0.
	#[benchmark]
	fn close_bounty_proposed() -> Result<(), BenchmarkError> {
		let setup = create_proposed_bounty::<T, I>()?;
		let approve_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[extrinsic_call]
		close_bounty(approve_origin, setup.bounty_id);

		assert_last_event::<T, I>(
			Event::BountyRejected { index: setup.bounty_id, bond: setup.bond.clone() }.into(),
		);
		assert!(pallet_bounties::Bounties::<T, I>::get(setup.bounty_id).is_none());

		Ok(())
	}

	/// This benchmark is short-circuited if `RejectOrigin` cannot provide
	/// a successful origin, in which case `close_bounty` is un-callable and can use weight=0.
	#[benchmark]
	fn close_bounty_active() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_bounty_with_curator::<T, I>(origin)?;
			true
		} else {
			false
		};

		let approve_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			let res = Bounties::<T, I>::close_bounty(approve_origin.clone(), setup.bounty_id);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::close_bounty(approve_origin, setup.bounty_id).is_err());
		}

		Ok(())
	}

	#[benchmark]
	fn extend_bounty_expiry() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_bounty_with_curator::<T, I>(origin)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::extend_bounty_expiry(
				RawOrigin::Signed(setup.curator).into(),
				setup.bounty_id,
				Vec::new(),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(Event::BountyExtended { index: setup.bounty_id }.into());
		}

		Ok(())
	}

	#[benchmark]
	fn check_payment_status_approved() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			initialize_approved_bounty::<T, I>(origin)?;
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id, PaymentStatus::Success);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::check_payment_status(
				RawOrigin::Signed(setup.caller).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(Event::BountyBecameActive { index: setup.bounty_id }.into());
			let bounty =
				pallet_bounties::Bounties::<T, I>::get(setup.bounty_id).expect("no bounty");
			assert!(!matches!(bounty.status, BountyStatus::Approved { .. }));
		}

		Ok(())
	}

	#[benchmark]
	fn check_payment_status_approved_with_curator() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			initialize_approved_bounty::<T, I>(origin)?;
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id, PaymentStatus::Success);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::check_payment_status(
				RawOrigin::Signed(setup.caller).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(Event::BountyBecameActive { index: setup.bounty_id }.into());
			let bounty =
				pallet_bounties::Bounties::<T, I>::get(setup.bounty_id).expect("no bounty");
			assert!(!matches!(bounty.status, BountyStatus::ApprovedWithCurator { .. }));
		}

		Ok(())
	}

	#[benchmark]
	fn check_payment_status_refund_attempted() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_bounty_with_curator::<T, I>(origin.clone())?;
			let bounty_account =
				Bounties::<T, I>::bounty_account_id(setup.bounty_id, setup.asset_kind.clone())
					.expect("conversion failed");
			let treasury_account = Bounties::<T, I>::account_id();
			<T as pallet::Config<I>>::Paymaster::ensure_successful(
				&bounty_account,
				&treasury_account,
				setup.asset_kind.clone(),
				setup.value,
			);
			Bounties::<T, I>::close_bounty(origin, setup.bounty_id)?;
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id, PaymentStatus::Success);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::check_payment_status(
				RawOrigin::Signed(setup.caller).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(Event::BountyCanceled { index: setup.bounty_id }.into());
			assert!(pallet_bounties::Bounties::<T, I>::get(setup.bounty_id).is_none());
		}

		Ok(())
	}

	#[benchmark]
	fn check_payment_status_payout_attempted() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());
		let (fee, asset_payout) = Bounties::<T, I>::calculate_curator_fee_and_payout(
			setup.bounty_id,
			setup.fee,
			setup.value,
		);

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_awarded_bounty::<T, I>(origin)?;
			let bounty_account =
				Bounties::<T, I>::bounty_account_id(setup.bounty_id, setup.asset_kind.clone())
					.expect("conversion failed");
			<T as pallet::Config<I>>::Paymaster::ensure_successful(
				&bounty_account,
				&setup.curator_stash,
				setup.asset_kind.clone(),
				fee,
			);
			<T as pallet::Config<I>>::Paymaster::ensure_successful(
				&bounty_account,
				&setup.beneficiary,
				setup.asset_kind.clone(),
				asset_payout,
			);
			Bounties::<T, I>::claim_bounty(
				RawOrigin::Signed(setup.curator).into(),
				setup.bounty_id,
			)?;
			let curator_payment_id =
				get_payment_id::<T, I>(setup.bounty_id, Some(setup.curator_stash))
					.expect("no payment attempt");
			let beneficiary_payment_id =
				get_payment_id::<T, I>(setup.bounty_id, Some(setup.beneficiary.clone()))
					.expect("no payment attempt");
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(curator_payment_id, PaymentStatus::Success);
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(beneficiary_payment_id, PaymentStatus::Success);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::check_payment_status(
				RawOrigin::Signed(setup.caller).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::BountyClaimed {
					index: setup.bounty_id,
					asset_kind: setup.asset_kind,
					value: asset_payout,
					beneficiary: setup.beneficiary,
				}
				.into(),
			);
			assert!(pallet_bounties::Bounties::<T, I>::get(setup.bounty_id).is_none());
		}

		Ok(())
	}

	#[benchmark]
	fn process_payment_approved() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			let setup = initialize_approved_bounty::<T, I>(origin)?;
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id, PaymentStatus::Failure);
			Bounties::<T, I>::check_payment_status(
				RawOrigin::Signed(setup.caller.clone()).into(),
				setup.bounty_id,
			)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::process_payment(
				RawOrigin::Signed(setup.caller.clone()).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::process_payment(
				RawOrigin::Signed(setup.caller).into(),
				setup.bounty_id
			)
			.is_err());
		}

		Ok(())
	}

	#[benchmark]
	fn process_payment_refund_attempted() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			let setup = create_funded_bounty_with_curator::<T, I>(origin.clone())?;
			Bounties::<T, I>::close_bounty(origin, setup.bounty_id)?;
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id, PaymentStatus::Failure);
			Bounties::<T, I>::check_payment_status(
				RawOrigin::Signed(setup.caller.clone()).into(),
				setup.bounty_id,
			)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::process_payment(
				RawOrigin::Signed(setup.caller.clone()).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			let payment_id =
				get_payment_id::<T, I>(setup.bounty_id, None).expect("no payment attempt");
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::process_payment(
				RawOrigin::Signed(setup.caller).into(),
				setup.bounty_id
			)
			.is_err());
		}

		Ok(())
	}

	#[benchmark]
	fn process_payment_payout_attempted() -> Result<(), BenchmarkError> {
		let setup = setup_bounty::<T, I>(0, T::MaximumReasonLength::get());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			let setup = create_awarded_bounty::<T, I>(origin.clone())?;
			Bounties::<T, I>::claim_bounty(
				RawOrigin::Signed(setup.curator.clone()).into(),
				setup.bounty_id,
			)?;
			let curator_payment_id =
				get_payment_id::<T, I>(setup.bounty_id, Some(setup.curator_stash))
					.expect("no payment attempt");
			let beneficiary_payment_id =
				get_payment_id::<T, I>(setup.bounty_id, Some(setup.beneficiary))
					.expect("no payment attempt");
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(curator_payment_id, PaymentStatus::Failure);
			<T as pallet::Config<I>>::Paymaster::ensure_concluded(beneficiary_payment_id, PaymentStatus::Failure);
			Bounties::<T, I>::check_payment_status(
				RawOrigin::Signed(setup.caller.clone()).into(),
				setup.bounty_id,
			)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::process_payment(
				RawOrigin::Signed(setup.caller.clone()).into(),
				setup.bounty_id,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			let curator_payment_id =
				get_payment_id::<T, I>(setup.bounty_id, Some(setup.curator_stash))
					.expect("no payment attempt");
			let beneficiary_payment_id =
				get_payment_id::<T, I>(setup.bounty_id, Some(setup.beneficiary))
					.expect("no payment attempt");
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(curator_payment_id),
				PaymentStatus::Failure
			);
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(beneficiary_payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::process_payment(
				RawOrigin::Signed(setup.caller).into(),
				setup.bounty_id
			)
			.is_err());
		}

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
