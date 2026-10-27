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

// Fill the payout queue for `asset_kind` with `n` entries so the benchmarked call hits the
// worst case: a full scan over (and re-encode of) a queue of up to `MaxQueuedSpends` entries.
// The seeded indices are offset to avoid colliding with spends created by the benchmark and
// use an order key of zero so that a subsequent sorted insert scans the entire queue.
fn fill_payout_queue<T: Config<I>, I: 'static>(asset_kind: T::AssetKind, n: u32) {
	let order_key = BlockNumberFor::<T, I>::zero();
	let queue: BoundedVec<_, T::MaxQueuedSpends> = (0..n)
		.map(|i| (i + 1_000_000, order_key))
		.collect::<alloc::vec::Vec<_>>()
		.try_into()
		.expect("n is at most MaxQueuedSpends; qed");
	PayoutQueue::<T, I>::insert(asset_kind, queue);
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

		// Worst case: `NextPayout` is already occupied by a later-maturing head, so the new spend
		// (with `valid_from = now`) preempts it. The demoted head is re-inserted into an almost
		// full payout queue, scanning all existing entries, and the head is rewritten.
		let now = T::BlockNumberProvider::current_block_number();
		let head_order_key = now.saturating_add(T::OrderExpirationPeriod::get());
		NextPayout::<T, I>::insert(
			asset_kind.clone(),
			(1_000_000u32, head_order_key, head_order_key),
		);
		fill_payout_queue::<T, I>(asset_kind.clone(), T::MaxQueuedSpends::get().saturating_sub(1));

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

		if spend_exists {
			// Worst case: the head's order lease has lapsed, so `payout` additionally reads
			// `PayoutQueue` to check whether another spend is already mature. The queue is
			// full of not-yet-mature spends, so the check decodes a full queue and the payout
			// still proceeds.
			NextPayout::<T, I>::mutate(&asset_kind, |entry| {
				if let Some((_, _, expire_at)) = entry {
					*expire_at = BlockNumberFor::<T, I>::zero();
				}
			});
			let future = BlockNumberFor::<T, I>::one() + T::PayoutPeriod::get();
			let queue: BoundedVec<_, T::MaxQueuedSpends> = (0..T::MaxQueuedSpends::get())
				.map(|i| (i + 1_000_000, future))
				.collect::<alloc::vec::Vec<_>>()
				.try_into()
				.expect("max is the bound; qed");
			PayoutQueue::<T, I>::insert(&asset_kind, queue);
			// Relay-block providers (e.g. asset-hub) report 0 at genesis; advance so the zeroed
			// order expiration is in the past.
			T::BlockNumberProvider::set_block_number(One::one());
		}

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

		let payment_id = if let Ok(origin) = T::SpendOrigin::try_successful_origin() {
			Treasury::<T, _>::spend(
				origin,
				Box::new(asset_kind.clone()),
				amount,
				Box::new(beneficiary_lookup),
				None,
			)?;

			Treasury::<T, _>::payout(RawOrigin::Signed(caller.clone()).into(), 0u32)?;
			let id = match Spends::<T, I>::get(0).unwrap().status {
				PaymentState::Attempted { id, .. } => {
					T::Paymaster::ensure_concluded(id);
					// NOTE: the in-tree mock's `ensure_concluded` concludes with `Failure`, so
					// the bench test exercises the cheaper `Failed` arm; the heavier `Success`
					// arm (spend removal plus promotion from a full queue) is pinned in the
					// verification below whenever the paymaster concludes with `Success`. The
					// mock cannot be overridden from here: its status override lives in the
					// `cfg(test)`-only tests module with a concrete `u64` payment id type.
					id
				},
				_ => panic!("No payout attempt made"),
			};

			Some(id)
		} else {
			None
		};

		// Worst case: removing the spend promotes the next entry from a full payout queue.
		fill_payout_queue::<T, I>(asset_kind.clone(), T::MaxQueuedSpends::get());

		#[block]
		{
			let res =
				Treasury::<T, _>::check_status(RawOrigin::Signed(caller.clone()).into(), 0u32);

			if payment_id.is_some() {
				assert_ok!(res);
			} else {
				assert_err!(res, crate::Error::<T, _>::InvalidIndex);
			}
		}

		if let Some(id) = payment_id {
			match T::Paymaster::check_payment(id) {
				// The payment succeeded: the spend was removed and the next queue entry promoted.
				PaymentStatus::Success => {
					assert!(Spends::<T, I>::get(0).is_none());
					assert_last_event::<T, I>(Event::SpendProcessed { index: 0 }.into());
					assert!(
						matches!(NextPayout::<T, I>::get(&asset_kind), Some((idx, _, _)) if idx != 0)
					);
				},
				// The payment failed (the in-tree mock): the spend is marked `Failed` and stays.
				_ => {
					let spend_status =
						Spends::<T, I>::get(0).expect("spend must remain on failure; qed");
					assert!(matches!(spend_status.status, PaymentState::Failed));
					assert_last_event::<T, I>(
						Event::PaymentFailed { index: 0, payment_id: id }.into(),
					);
				},
			}
		}

		Ok(())
	}

	// The rotation branch of `check_status`: a head whose payout order has expired is rotated to
	// the back of a (near) full queue and the front entry is promoted. Benchmarked separately so
	// `check_status`'s declared weight can be `check_status().max(check_status_rotation())`.
	#[benchmark]
	fn check_status_rotation() -> Result<(), BenchmarkError> {
		let origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let (asset_kind, amount, _, beneficiary_lookup) = create_spend_arguments::<T, _>(SEED);
		T::BalanceConverter::ensure_successful(asset_kind.clone());
		let caller: T::AccountId = account("caller", 0, SEED);

		// The spend becomes NextPayout (queue empty), status Pending. The head is left `Pending`
		// rather than paid-and-`InProgress`: the `Pay` trait offers no way to force an
		// `InProgress` payment status, so the InProgress rotation arm (a `check_payment` call on
		// top of the same rotation) is not covered by this benchmark.
		Treasury::<T, _>::spend(
			origin,
			Box::new(asset_kind.clone()),
			amount,
			Box::new(beneficiary_lookup),
			None,
		)?;

		// Force the head's payout order to be already expired (order `expire_at` of zero is `<
		// now`) while its spend payout window stays open, so `check_status` takes the rotation
		// branch.
		NextPayout::<T, I>::mutate(&asset_kind, |entry| {
			if let Some((_, _, expire_at)) = entry {
				*expire_at = BlockNumberFor::<T, I>::zero();
			}
		});
		// Relay-block providers (e.g. asset-hub) report 0 at genesis; advance so the zeroed order
		// expiration is in the past.
		T::BlockNumberProvider::set_block_number(One::one());

		// Worst case: a full queue, so rotation scans it, re-inserts the expired head, and
		// promotes the front. Rotation is size-neutral, so the queue may be full.
		fill_payout_queue::<T, I>(asset_kind.clone(), T::MaxQueuedSpends::get());

		#[block]
		{
			assert_ok!(Treasury::<T, _>::check_status(RawOrigin::Signed(caller).into(), 0u32));
		}

		// The expired head (index 0) was rotated to the back; a queued spend is now NextPayout.
		assert!(matches!(NextPayout::<T, I>::get(&asset_kind), Some((idx, _, _)) if idx != 0));
		assert_last_event::<T, I>(Event::PayoutQueueRotated { asset_kind, index: 0 }.into());
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

		// Worst case: voiding the spend promotes the next entry from a full payout queue.
		fill_payout_queue::<T, I>(asset_kind.clone(), T::MaxQueuedSpends::get());

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
