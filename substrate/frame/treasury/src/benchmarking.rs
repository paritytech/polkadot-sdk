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

//! Treasury pallet benchmarking.

#![cfg(feature = "runtime-benchmarks")]

use super::{Pallet as Treasury, *};

use frame_benchmarking::{
	v1::{account, BenchmarkError},
	v2::*,
};
use frame_support::{
	assert_err, assert_ok, ensure,
	traits::{
		tokens::{ConversionFromAssetBalance, PaymentStatus},
		EnsureOrigin, OnInitialize,
	},
};
use frame_system::RawOrigin;
use sp_core::crypto::FromEntropy;

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

const SEED: u32 = 0;

// Create the pre-requisite information needed to create a treasury `spend_local`.
fn setup_proposal<T: Config<I>, I: 'static>(
	u: u32,
) -> (T::AccountId, BalanceOf<T, I>, AccountIdLookupOf<T>) {
	let caller = account("caller", u, SEED);
	let value: BalanceOf<T, I> = T::Currency::minimum_balance() * 100u32.into();
	let _ = T::Currency::make_free_balance_be(&caller, value);
	let beneficiary = account("beneficiary", u, SEED);
	let beneficiary_lookup = T::Lookup::unlookup(beneficiary);
	(caller, value, beneficiary_lookup)
}

// Create proposals that are approved for use in `on_initialize`.
fn create_approved_proposals<T: Config<I>, I: 'static>(n: u32) -> Result<(), &'static str> {
	let spender = T::SpendOrigin::try_successful_origin();

	for i in 0..n {
		let (_, value, lookup) = setup_proposal::<T, I>(i);

		#[allow(deprecated)]
		if let Ok(origin) = &spender {
			Treasury::<T, I>::spend_local(origin.clone(), value, lookup)?;
		}
	}

	if spender.is_ok() {
		ensure!(Approvals::<T, I>::get().len() == n as usize, "Not all approved");
	}
	Ok(())
}

fn setup_pot_account<T: Config<I>, I: 'static>() {
	let pot_account = Treasury::<T, I>::account_id();
	let value = T::Currency::minimum_balance().saturating_mul(1_000_000_000u32.into());
	let _ = T::Currency::make_free_balance_be(&pot_account, value);
}

fn assert_last_event<T: Config<I>, I: 'static>(generic_event: <T as Config<I>>::RuntimeEvent) {
	frame_system::Pallet::<T>::assert_last_event(generic_event.into());
}

// Create the arguments for the `spend` dispatchable.
fn create_spend_arguments<T: Config<I>, I: 'static>(
	seed: u32,
) -> (T::AssetKind, AssetBalanceOf<T, I>, T::Beneficiary, BeneficiaryLookupOf<T, I>) {
	let asset_kind = T::BenchmarkHelper::create_asset_kind(seed);
	let beneficiary = T::BenchmarkHelper::create_beneficiary([seed.try_into().unwrap(); 32]);
	let beneficiary_lookup = T::BeneficiaryLookup::unlookup(beneficiary.clone());
	(asset_kind, 100u32.into(), beneficiary, beneficiary_lookup)
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `spend` is un-callable and can use weight=0.
	#[benchmark]
	fn spend_local() -> Result<(), BenchmarkError> {
		let (_, value, beneficiary_lookup) = setup_proposal::<T, _>(SEED);
		let origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let beneficiary = T::Lookup::lookup(beneficiary_lookup.clone()).unwrap();

		#[extrinsic_call]
		_(origin as T::RuntimeOrigin, value, beneficiary_lookup);

		assert_last_event::<T, I>(
			Event::SpendApproved { proposal_index: 0, amount: value, beneficiary }.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn remove_approval() -> Result<(), BenchmarkError> {
		let (spend_exists, proposal_id) =
			if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
				let (_, value, beneficiary_lookup) = setup_proposal::<T, _>(SEED);
				#[allow(deprecated)]
				Treasury::<T, _>::spend_local(origin, value, beneficiary_lookup)?;
				let proposal_id = ProposalCount::<T, _>::get() - 1;

				(true, proposal_id)
			} else {
				(false, 0)
			};

		let reject_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			#[allow(deprecated)]
			let res = Treasury::<T, _>::remove_approval(reject_origin as T::RuntimeOrigin, proposal_id);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, Error::<T, _>::ProposalNotApproved);
			}
		}

		Ok(())
	}

	#[benchmark]
	fn on_initialize_proposals(
		p: Linear<0, { T::MaxApprovals::get() - 1 }>,
	) -> Result<(), BenchmarkError> {
		setup_pot_account::<T, _>();
		create_approved_proposals::<T, _>(p)?;

		#[block]
		{
			Treasury::<T, _>::on_initialize(0u32.into());
		}

		Ok(())
	}

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `spend` is un-callable and can use weight=0.
	#[benchmark]
	fn spend() -> Result<(), BenchmarkError> {
		let origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let (asset_kind, amount, beneficiary, beneficiary_lookup) =
			create_spend_arguments::<T, _>(SEED);
		T::BalanceConverter::ensure_successful(asset_kind.clone());

		#[extrinsic_call]
		_(
			origin as T::RuntimeOrigin,
			Box::new(asset_kind.clone()),
			amount,
			Box::new(beneficiary_lookup),
			None,
		);

		let valid_from = T::BlockNumberProvider::current_block_number();
		let expire_at = valid_from.saturating_add(T::PayoutPeriod::get());
		assert_last_event::<T, I>(
			Event::AssetSpendApproved {
				index: 0,
				asset_kind,
				amount,
				beneficiary,
				valid_from,
				expire_at,
			}
			.into(),
		);
		Ok(())
	}

	#[benchmark]
	fn payout() -> Result<(), BenchmarkError> {
		let (asset_kind, amount, beneficiary, beneficiary_lookup) =
			create_spend_arguments::<T, _>(SEED);
		T::BalanceConverter::ensure_successful(asset_kind.clone());

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			Treasury::<T, _>::spend(
				origin,
				Box::new(asset_kind.clone()),
				amount,
				Box::new(beneficiary_lookup),
				None,
			)?;

			true
		} else {
			false
		};

		T::Paymaster::ensure_successful(&beneficiary, asset_kind, amount);
		let caller: T::AccountId = account("caller", 0, SEED);

		#[block]
		{
			let res = Treasury::<T, _>::payout(RawOrigin::Signed(caller.clone()).into(), 0u32);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if spend_exists {
			let id = match Spends::<T, I>::get(0).unwrap().status {
				PaymentState::Attempted { id, .. } => {
					assert_ne!(T::Paymaster::check_payment(id), PaymentStatus::Failure);
					id
				},
				_ => panic!("No payout attempt made"),
			};
			assert_last_event::<T, I>(Event::Paid { index: 0, payment_id: id }.into());
			assert!(Treasury::<T, _>::payout(RawOrigin::Signed(caller).into(), 0u32).is_err());
		}

		Ok(())
	}

	#[benchmark]
	fn check_status() -> Result<(), BenchmarkError> {
		let (asset_kind, amount, beneficiary, beneficiary_lookup) =
			create_spend_arguments::<T, _>(SEED);

		T::BalanceConverter::ensure_successful(asset_kind.clone());
		T::Paymaster::ensure_successful(&beneficiary, asset_kind.clone(), amount);
		let caller: T::AccountId = account("caller", 0, SEED);

		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			Treasury::<T, _>::spend(
				origin,
				Box::new(asset_kind),
				amount,
				Box::new(beneficiary_lookup),
				None,
			)?;

			Treasury::<T, _>::payout(RawOrigin::Signed(caller.clone()).into(), 0u32)?;
			match Spends::<T, I>::get(0).unwrap().status {
				PaymentState::Attempted { id, .. } => {
					T::Paymaster::ensure_concluded(id);
				},
				_ => panic!("No payout attempt made"),
			};

			true
		} else {
			false
		};

		#[block]
		{
			let res =
				Treasury::<T, _>::check_status(RawOrigin::Signed(caller.clone()).into(), 0u32);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if let Some(s) = Spends::<T, I>::get(0) {
			assert!(!matches!(s.status, PaymentState::Attempted { .. }));
		}

		Ok(())
	}

	#[benchmark]
	fn void_spend() -> Result<(), BenchmarkError> {
		let (asset_kind, amount, _, beneficiary_lookup) = create_spend_arguments::<T, _>(SEED);
		T::BalanceConverter::ensure_successful(asset_kind.clone());
		let spend_exists = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			Treasury::<T, _>::spend(
				origin,
				Box::new(asset_kind.clone()),
				amount,
				Box::new(beneficiary_lookup),
				None,
			)?;
			assert!(Spends::<T, I>::get(0).is_some());

			true
		} else {
			false
		};

		let origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			let res = Treasury::<T, _>::void_spend(origin as T::RuntimeOrigin, 0u32);

			if spend_exists {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		assert!(Spends::<T, I>::get(0).is_none());
		Ok(())
	}

	impl_benchmark_test_suite!(
		Treasury,
		crate::tests::ExtBuilder::default().build(),
		crate::tests::Test
	);

	mod no_spend_origin_tests {
		use super::*;

		impl_benchmark_test_suite!(
			Treasury,
			crate::tests::ExtBuilder::default().spend_origin_succesful_origin_err().build(),
			crate::tests::Test,
			benchmarks_path = benchmarking
		);
	}
}
