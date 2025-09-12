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

use alloc::{borrow::Cow, vec};
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::{assert_err, assert_ok};
use frame_system::RawOrigin;
use sp_core::crypto::FromEntropy;

/// Trait describing factory functions for dispatchables' parameters.
pub trait ArgumentsFactory<AssetKind, Beneficiary, Balance> {
	/// Factory function for an asset kind.
	fn create_asset_kind(seed: u32) -> AssetKind;

	/// Factory function for a beneficiary.
	fn create_beneficiary(seed: [u8; 32]) -> Beneficiary;

	/// Conversion function from a native asset to an asset kind.
	///
	/// Used to convert `pallet::Config::BountyValueMinimum` amount to the asset kind amount, since
	/// `pallet::Config::BalanceConverter` does not implement `ConversionToAssetBalance` trait.
	fn to_asset_balance(balance: Balance, asset_id: AssetKind) -> Balance;
}

/// Implementation that expects the parameters implement the [`FromEntropy`] trait.
impl<AssetKind, Beneficiary, Balance> ArgumentsFactory<AssetKind, Beneficiary, Balance> for ()
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

	fn to_asset_balance(balance: Balance, _asset_id: AssetKind) -> Balance {
		balance.into()
	}
}

#[derive(Clone)]
struct BenchmarkBounty<T: Config<I>, I: 'static> {
	/// Parent bounty ID.
	parent_bounty_id: BountyIndex,
	/// Child-bounty ID.
	child_bounty_id: BountyIndex,
	/// The parent bounty curator account.
	curator: T::AccountId,
	/// The child-bounty curator account.
	child_curator: T::AccountId,
	/// The kind of asset the child-/bounty is rewarded in.
	asset_kind: T::AssetKind,
	/// The amount that should be paid if the bounty is rewarded.
	value: BalanceOf<T, I>,
	/// The amount that should be paid if the child-bounty is rewarded.
	child_value: BalanceOf<T, I>,
	/// The child-/bounty beneficiary account.
	beneficiary: T::Beneficiary,
	/// Bounty metadata hash.
	hash: T::Hash,
}

const SEED: u32 = 0;

fn assert_last_event<T: Config<I>, I: 'static>(
	generic_event: <T as frame_system::Config>::RuntimeEvent,
) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

fn assert_has_event<T: Config<I>, I: 'static>(
	generic_event: <T as frame_system::Config>::RuntimeEvent,
) {
	frame_system::Pallet::<T>::assert_has_event(generic_event.into());
}

pub fn get_payment_id<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
) -> Option<PaymentIdOf<T, I>> {
	let bounty = Bounties::<T, I>::get_bounty_details(parent_bounty_id, child_bounty_id)
		.expect("no bounty found");

	match bounty.3 {
		BountyStatus::FundingAttempted {
			payment_status: PaymentState::Attempted { id }, ..
		} => Some(id),
		BountyStatus::RefundAttempted {
			payment_status: PaymentState::Attempted { id }, ..
		} => Some(id),
		BountyStatus::PayoutAttempted {
			payment_status: PaymentState::Attempted { id }, ..
		} => Some(id),
		_ => None,
	}
}

// Create the pre-requisite information needed to `fund_bounty`.
fn setup_bounty<T: Config<I>, I: 'static>() -> BenchmarkBounty<T, I> {
	let asset_kind = <T as Config<I>>::BenchmarkHelper::create_asset_kind(SEED);
	T::BalanceConverter::ensure_successful(asset_kind.clone());
	let min_native_value = T::BountyValueMinimum::get().saturating_mul(100u32.into());
	let value =
		<T as Config<I>>::BenchmarkHelper::to_asset_balance(min_native_value, asset_kind.clone());
	let child_value = value / 4u32.into();
	let curator = account("curator", 0, SEED);
	let child_curator = account("child-curator", 1, SEED);
	let beneficiary =
		<T as Config<I>>::BenchmarkHelper::create_beneficiary([(SEED).try_into().unwrap(); 32]);
	let hash = T::Preimages::note(Cow::from(vec![5, 6])).unwrap();

	BenchmarkBounty::<T, I> {
		parent_bounty_id: 0,
		child_bounty_id: 0,
		curator,
		child_curator,
		asset_kind,
		value,
		child_value,
		beneficiary,
		hash,
	}
}

fn create_parent_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let mut s = setup_bounty::<T, I>();

	let funding_source_account =
		Bounties::<T, I>::funding_source_account(s.asset_kind.clone()).expect("conversion failed");
	let parent_bounty_account =
		Bounties::<T, I>::bounty_account(s.parent_bounty_id, s.asset_kind.clone())
			.expect("conversion failed");
	let curator_lookup = T::Lookup::unlookup(s.curator.clone());
	<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
		&funding_source_account,
		&parent_bounty_account,
		s.asset_kind.clone(),
		s.value,
	);
	<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(&s.curator, 0u32.into());

	Bounties::<T, I>::fund_bounty(
		origin,
		Box::new(s.asset_kind.clone()),
		s.value,
		curator_lookup,
		s.hash,
	)?;

	s.parent_bounty_id = pallet_bounties::BountyCount::<T, I>::get() - 1;

	Ok(s)
}

fn create_funded_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_parent_bounty::<T, I>(origin)?;

	let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, None).expect("no payment attempt");
	<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id);

	let caller = account("caller", 0, SEED);
	Bounties::<T, I>::check_status(RawOrigin::Signed(caller).into(), s.parent_bounty_id, None)?;

	Ok(s)
}

fn create_active_parent_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_funded_bounty::<T, I>(origin)?;
	let curator = s.curator.clone();
	<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(&curator, s.value);

	Bounties::<T, I>::accept_curator(RawOrigin::Signed(curator).into(), s.parent_bounty_id, None)?;

	Ok(s)
}

fn create_child_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let mut s = create_active_parent_bounty::<T, I>(origin.clone())?;
	let child_curator_lookup = T::Lookup::unlookup(s.child_curator.clone());
	<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(
		&s.child_curator,
		0u32.into(),
	);

	Bounties::<T, I>::fund_child_bounty(
		RawOrigin::Signed(s.curator.clone()).into(),
		s.parent_bounty_id,
		s.child_value,
		Some(child_curator_lookup),
		s.hash,
	)?;
	s.child_bounty_id =
		pallet_bounties::TotalChildBountiesPerParent::<T, I>::get(s.parent_bounty_id) - 1;

	Ok(s)
}

fn create_funded_child_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_child_bounty::<T, I>(origin)?;
	let caller = account("caller", 0, SEED);

	let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
		.expect("no payment attempt");
	<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id);
	Bounties::<T, I>::check_status(
		RawOrigin::Signed(caller).into(),
		s.parent_bounty_id,
		Some(s.child_bounty_id),
	)?;

	Ok(s)
}

fn create_active_child_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_funded_child_bounty::<T, I>(origin)?;
	let caller = s.child_curator.clone();
	<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(&caller, s.child_value);

	Bounties::<T, I>::accept_curator(
		RawOrigin::Signed(caller).into(),
		s.parent_bounty_id,
		Some(s.child_bounty_id),
	)?;

	Ok(s)
}

fn create_awarded_child_bounty<T: Config<I>, I: 'static>(
	origin: T::RuntimeOrigin,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_active_child_bounty::<T, I>(origin)?;
	let caller = s.child_curator.clone();
	let beneficiary_lookup = T::BeneficiaryLookup::unlookup(s.beneficiary.clone());

	Bounties::<T, I>::award_bounty(
		RawOrigin::Signed(caller).into(),
		s.parent_bounty_id,
		Some(s.child_bounty_id),
		beneficiary_lookup,
	)?;

	Ok(s)
}

pub fn set_status<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	new_payment_status: PaymentState<PaymentIdOf<T, I>>,
) -> Result<(), BenchmarkError> {
	let bounty =
		pallet_bounties::Pallet::<T, I>::get_bounty_details(parent_bounty_id, child_bounty_id)
			.expect("no bounty");

	let new_status = match bounty.3 {
		BountyStatus::FundingAttempted { curator, .. } =>
			BountyStatus::FundingAttempted { payment_status: new_payment_status, curator },
		BountyStatus::RefundAttempted { curator, .. } =>
			BountyStatus::RefundAttempted { payment_status: new_payment_status, curator },
		BountyStatus::PayoutAttempted { curator, beneficiary, .. } =>
			BountyStatus::PayoutAttempted {
				payment_status: new_payment_status,
				curator,
				beneficiary,
			},
		_ => return Err(BenchmarkError::Stop("unexpected bounty status")),
	};

	let _ = pallet_bounties::Pallet::<T, I>::update_bounty_details(
		parent_bounty_id,
		child_bounty_id,
		new_status,
		None,
	);

	Ok(())
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `fund_bounty` is un-callable and can use weight=0.
	#[benchmark]
	fn fund_bounty() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();

		let approve_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let curator_lookup = T::Lookup::unlookup(s.curator.clone());
		let funding_source_account = Bounties::<T, I>::funding_source_account(s.asset_kind.clone())
			.expect("conversion failed");
		let parent_bounty_account =
			Bounties::<T, I>::bounty_account(s.parent_bounty_id, s.asset_kind.clone())
				.expect("conversion failed");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
			&funding_source_account,
			&parent_bounty_account,
			s.asset_kind.clone(),
			s.value,
		);
		<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(
			&s.curator,
			0u32.into(),
		);

		#[extrinsic_call]
		_(approve_origin, Box::new(s.asset_kind), s.value, curator_lookup, s.hash);

		let parent_bounty_id = BountyCount::<T, I>::get() - 1;
		assert_last_event::<T, I>(Event::BountyCreated { index: parent_bounty_id }.into());
		let payment_id =
			get_payment_id::<T, I>(parent_bounty_id, None).expect("no payment attempt");
		assert_has_event::<T, I>(
			Event::Paid { index: s.parent_bounty_id, child_index: None, payment_id }.into(),
		);
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);

		Ok(())
	}

	#[benchmark]
	fn fund_child_bounty() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let child_curator_lookup = T::Lookup::unlookup(s.child_curator.clone());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_parent_bounty::<T, I>(origin)?;
			<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(
				&s.child_curator,
				0u32.into(),
			);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::fund_child_bounty(
				RawOrigin::Signed(s.curator).into(),
				s.parent_bounty_id,
				s.child_value,
				Some(child_curator_lookup),
				s.hash,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			let child_bounty_id =
				pallet_bounties::TotalChildBountiesPerParent::<T, I>::get(s.parent_bounty_id) - 1;
			assert_last_event::<T, I>(
				Event::ChildBountyCreated {
					index: s.parent_bounty_id,
					child_index: child_bounty_id,
				}
				.into(),
			);
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(child_bounty_id))
				.expect("no payment attempt");
			assert_has_event::<T, I>(
				Event::Paid {
					index: s.parent_bounty_id,
					child_index: Some(child_bounty_id),
					payment_id,
				}
				.into(),
			);
			assert_ne!(
				<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
		}

		Ok(())
	}

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `fund_bounty` is un-callable and can use weight=0.
	#[benchmark]
	fn propose_curator_parent_bounty() -> Result<(), BenchmarkError> {
		let approve_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let s = create_funded_bounty::<T, I>(approve_origin.clone())?;

		Bounties::<T, I>::unassign_curator(
			RawOrigin::Signed(s.curator.clone()).into(),
			s.parent_bounty_id,
			None,
		)?;
		let curator_lookup = T::Lookup::unlookup(s.curator.clone());

		#[block]
		{
			let _ = Bounties::<T, I>::propose_curator(
				approve_origin,
				s.parent_bounty_id,
				None,
				curator_lookup,
			);
		}

		assert_last_event::<T, I>(
			Event::CuratorProposed {
				index: s.parent_bounty_id,
				child_index: None,
				curator: s.curator,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn propose_curator_child_bounty() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let child_curator_lookup = T::Lookup::unlookup(s.child_curator.clone());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_child_bounty::<T, I>(origin)?;
			Bounties::<T, I>::unassign_curator(
				RawOrigin::Signed(s.curator.clone()).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::propose_curator(
				RawOrigin::Signed(s.curator).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				child_curator_lookup,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::CuratorProposed {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					curator: s.child_curator,
				}
				.into(),
			);
		}

		Ok(())
	}

	#[benchmark]
	fn accept_curator() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.child_curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_funded_child_bounty::<T, I>(origin)?;
			<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(
				&caller,
				s.child_value,
			);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::accept_curator(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::BountyBecameActive {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					curator: s.child_curator,
				}
				.into(),
			);
		}

		Ok(())
	}

	#[benchmark]
	fn unassign_curator() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_child_bounty::<T, I>(origin)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::unassign_curator(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::CuratorUnassigned {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
				}
				.into(),
			);
		}

		Ok(())
	}

	#[benchmark]
	fn award_bounty() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.child_curator.clone();
		let beneficiary_lookup = T::BeneficiaryLookup::unlookup(s.beneficiary.clone());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_child_bounty::<T, I>(origin)?;
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::award_bounty(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				beneficiary_lookup,
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::BountyAwarded {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					beneficiary: s.beneficiary,
				}
				.into(),
			);
		}

		Ok(())
	}

	#[benchmark]
	fn close_parent_bounty() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_parent_bounty::<T, I>(origin)?;
			true
		} else {
			false
		};

		let reject_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			let res =
				Bounties::<T, I>::close_bounty(reject_origin.clone(), s.parent_bounty_id, None);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::BountyCanceled { index: s.parent_bounty_id, child_index: None }.into(),
			);
			let payment_id =
				get_payment_id::<T, I>(s.parent_bounty_id, None).expect("no payment attempt");
			assert_has_event::<T, I>(
				Event::Paid { index: s.parent_bounty_id, child_index: None, payment_id }.into(),
			);
			assert_ne!(
				<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(
				Bounties::<T, I>::close_bounty(reject_origin, s.parent_bounty_id, None).is_err()
			);
		}

		Ok(())
	}

	#[benchmark]
	fn close_child_bounty() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_child_bounty::<T, I>(origin)?;
			true
		} else {
			false
		};

		let reject_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			let res = Bounties::<T, I>::close_bounty(
				reject_origin.clone(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::BountyCanceled {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
				}
				.into(),
			);
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			assert_has_event::<T, I>(
				Event::Paid {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					payment_id,
				}
				.into(),
			);
			assert_ne!(
				<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::close_bounty(
				reject_origin,
				s.parent_bounty_id,
				Some(s.child_bounty_id)
			)
			.is_err());
		}

		Ok(())
	}

	#[benchmark]
	fn check_status_funding() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_child_bounty::<T, I>(origin)?;
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::check_status(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_last_event::<T, I>(
				Event::BountyFundingProcessed {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
				}
				.into(),
			);
			let child_bounty =
				pallet_bounties::ChildBounties::<T, I>::get(s.parent_bounty_id, s.child_bounty_id)
					.expect("no bounty");
			assert!(matches!(child_bounty.status, BountyStatus::Funded { .. }));
		}

		Ok(())
	}

	#[benchmark]
	fn check_status_refund() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_child_bounty::<T, I>(origin)?;
			Bounties::<T, I>::close_bounty(
				RawOrigin::Signed(caller.clone()).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			)?;
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::check_status(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_has_event::<T, I>(
				Event::BountyRefundProcessed {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
				}
				.into(),
			);
			let _ =
				pallet_bounties::ChildBounties::<T, I>::get(s.parent_bounty_id, s.child_bounty_id)
					.is_none();
		}

		Ok(())
	}

	#[benchmark]
	fn check_status_payout() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.child_curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_awarded_child_bounty::<T, I>(origin)?;
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::check_status(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			assert_has_event::<T, I>(
				Event::BountyPayoutProcessed {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					asset_kind: s.asset_kind,
					value: s.child_value,
					beneficiary: s.beneficiary,
				}
				.into(),
			);
			let _ =
				pallet_bounties::ChildBounties::<T, I>::get(s.parent_bounty_id, s.child_bounty_id)
					.is_none();
		}

		Ok(())
	}

	#[benchmark]
	fn retry_payment_funding() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_child_bounty::<T, I>(origin)?;
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			<T as pallet_bounties::Config<I>>::Paymaster::ensure_concluded(payment_id);
			let _ = set_status::<T, I>(
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				PaymentState::Failed,
			);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller.clone()).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			assert_last_event::<T, I>(
				Event::Paid {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					payment_id,
				}
				.into(),
			);
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			)
			.is_err());
		}

		Ok(())
	}

	#[benchmark]
	fn retry_payment_refund() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_child_bounty::<T, I>(origin)?;
			let new_status = BountyStatus::RefundAttempted {
				payment_status: PaymentState::Failed,
				curator: Some(s.child_curator),
			};
			let _ = pallet_bounties::Pallet::<T, I>::update_bounty_details(
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				new_status,
				None,
			);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller.clone()).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			assert_last_event::<T, I>(
				Event::Paid {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					payment_id,
				}
				.into(),
			);
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			)
			.is_err());
		}

		Ok(())
	}

	#[benchmark]
	fn retry_payment_payout() -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>();
		let caller = s.curator.clone();

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			create_active_child_bounty::<T, I>(origin)?;
			let new_status = BountyStatus::PayoutAttempted {
				payment_status: PaymentState::Failed,
				curator: s.child_curator.clone(),
				beneficiary: s.beneficiary.clone(),
			};
			let _ = pallet_bounties::Pallet::<T, I>::update_bounty_details(
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				new_status,
				None,
			);
			true
		} else {
			false
		};

		#[block]
		{
			let res = Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller.clone()).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, I>::InvalidIndex);
			}
		}

		if spend_exists {
			let payment_id = get_payment_id::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id))
				.expect("no payment attempt");
			assert_last_event::<T, I>(
				Event::Paid {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					payment_id,
				}
				.into(),
			);
			assert_ne!(
				<T as pallet::Config<I>>::Paymaster::check_payment(payment_id),
				PaymentStatus::Failure
			);
			assert!(Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
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
}
