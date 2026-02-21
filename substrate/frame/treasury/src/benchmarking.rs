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

use frame_benchmarking::v2::*;
use frame_support::{
	assert_err, assert_ok,
	dispatch::RawOrigin,
	traits::{
		fungible::Inspect,
		tokens::{
			ConversionFromAssetBalance, Fortitude::Polite, PaymentStatus, Preservation::Preserve,
		},
		Currency, EnsureOrigin, Hooks, ReservableCurrency,
	},
};
use sp_core::crypto::FromEntropy;

type MigrationConfig<T, I> = <T as Config<I>>::LazyMigrationV0ToV1Config;
type CurrencyOf<T, I> =
	<MigrationConfig<T, I> as migration::LazyMigrationV0ToV1Config<T, I>>::Currency;
type MaxApprovalsFor<T, I> =
	<MigrationConfig<T, I> as migration::LazyMigrationV0ToV1Config<T, I>>::MaxApprovals;

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

fn setup_pot_account<T: Config<I>, I: 'static>() {
	let pot_account = Treasury::<T, I>::account_id();
	let value = T::Fungible::minimum_balance().saturating_mul(1_000_000_000u32.into());
	let _ =
		T::Fungible::mint_into(&pot_account, value.saturating_sub(T::Fungible::minimum_balance()));
}

fn assert_last_event<T: Config<I>, I: 'static>(
	generic_event: <T as frame_system::Config>::RuntimeEvent,
) {
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

#[allow(dead_code)]
fn setup_old_proposal<T: Config<I>, I: 'static>(
	index: u32,
	proposer: &T::AccountId,
	bond: BalanceOf<T, I>,
	beneficiary: &T::AccountId,
	value: BalanceOf<T, I>,
	approved: bool,
) -> migration::Proposal<T::AccountId, BalanceOf<T, I>> {
	CurrencyOf::<T, I>::make_free_balance_be(
		&proposer,
		bond + CurrencyOf::<T, I>::minimum_balance(),
	);
	assert_ok!(CurrencyOf::<T, I>::reserve(&proposer, bond));

	CurrencyOf::<T, I>::make_free_balance_be(&beneficiary, CurrencyOf::<T, I>::minimum_balance());

	let proposal = migration::Proposal {
		proposer: proposer.clone(),
		value,
		beneficiary: beneficiary.clone(),
		bond,
	};

	type MigrationConfig<T, I> = <T as Config<I>>::LazyMigrationV0ToV1Config;

	migration::Proposals::<T, I>::insert(index, proposal.clone());
	if approved {
		assert_ok!(migration::Approvals::<T, I, MaxApprovalsFor<T, I>>::try_append(index));
	}

	proposal
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;
	use crate::migration;

	#[benchmark]
	fn on_initialize() -> Result<(), BenchmarkError> {
		setup_pot_account::<T, _>();

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

	#[benchmark]
	fn burn_funds() -> Result<(), BenchmarkError> {
		setup_pot_account::<T, _>();

		let mut budget_remaining = Treasury::<T, _>::pot();
		let mut imbalance = PositiveImbalanceOf::<T, _>::zero();

		#[block]
		{
			Treasury::<T, _>::burn_funds(&mut budget_remaining, &mut imbalance, 2u32.into());
		}

		assert_eq!(budget_remaining, Treasury::<T, _>::pot() - imbalance.peek());
		Ok(())
	}

	#[benchmark]
	fn migration_v1_next_step() -> Result<(), BenchmarkError> {
		type MigrationConfig<T, I> = <T as Config<I>>::LazyMigrationV0ToV1Config;

		// Heaviest step is to unwrap the approvals list, so we'll just do that.
		migration::Approvals::<T, _, MaxApprovalsFor<T, _>>::put(BoundedVec::truncate_from(
			(0..MaxApprovalsFor::<T, I>::get()).collect(),
		));

		#[block]
		{
			migration::LazyMigrationV0ToV1::<T, _, MigrationConfig<T, _>>::next_step(None);
		}

		assert_eq!(migration::Proposals::<T, _>::iter().count(), 0);
		Ok(())
	}

	#[benchmark]
	fn migration_v1_spend_approval() -> Result<(), BenchmarkError> {
		setup_pot_account::<T, _>();
		let proposer: T::AccountId = account("proposer", 0, SEED);
		let beneficiary: T::AccountId = account("beneficiary", 0, SEED);
		let bond = T::Fungible::minimum_balance().saturating_mul(1_000u32.into());
		let value = T::Fungible::minimum_balance().saturating_mul(1_000_000u32.into());

		setup_old_proposal::<T, _>(0, &proposer, bond, &beneficiary, value, true);

		#[block]
		{
			migration::LazyMigrationV0ToV1::<T, _, MigrationConfig<T, _>>::step_spend_approval(&0);
		}

		assert_eq!(CurrencyOf::<T, I>::reserved_balance(&proposer), bond);
		assert_eq!(T::Fungible::reducible_balance(&beneficiary, Preserve, Polite), value);
		Ok(())
	}

	#[benchmark]
	fn migration_v1_remove_proposal() -> Result<(), BenchmarkError> {
		setup_pot_account::<T, _>();
		let proposer: T::AccountId = account("proposer", 0, SEED);
		let beneficiary: T::AccountId = account("beneficiary", 0, SEED);
		let bond = T::Fungible::minimum_balance().saturating_mul(1_000u32.into());

		let proposal = setup_old_proposal::<T, _>(
			0,
			&proposer,
			bond,
			&beneficiary,
			T::Fungible::minimum_balance().saturating_mul(1_000_000u32.into()),
			false,
		);

		#[block]
		{
			migration::LazyMigrationV0ToV1::<T, _, MigrationConfig<T, _>>::step_remove_proposal(&(
				0, proposal,
			));
		}

		assert_eq!(CurrencyOf::<T, I>::reserved_balance(&proposer), Zero::zero());
		assert_eq!(T::Fungible::reducible_balance(&proposer, Preserve, Polite), bond);
		assert_eq!(T::Fungible::reducible_balance(&beneficiary, Preserve, Polite), 0u32.into());

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
