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

use alloc::{borrow::Cow, boxed::Box, vec, vec::Vec};
use frame_benchmarking::{v2::*, BenchmarkError};
use frame_support::assert_ok;
use frame_system::RawOrigin;
use sp_core::crypto::FromEntropy;

/// Trait describing factory functions for dispatchables' parameters.
pub trait ArgumentsFactory<AssetKind, Beneficiary, Balance> {
	/// Factory function for an asset kind.
	fn create_asset_kind(seed: u32) -> AssetKind;

	/// Factory function for a beneficiary.
	fn create_beneficiary(seed: [u8; 32]) -> Beneficiary;
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
	/// The parent bounty's basket.
	assets: Vec<(T::AssetKind, T::Balance)>,
	/// The child-bounty's basket: half of every asset of the parent.
	child_assets: Vec<(T::AssetKind, T::Balance)>,
	/// The summed native value of the parent basket, which sizes the curator deposit.
	native_value: T::Balance,
	/// The summed native value of the child basket.
	child_native_value: T::Balance,
	/// The child-/bounty beneficiary account.
	beneficiary: T::Beneficiary,
	/// Bounty metadata hash.
	metadata: T::Hash,
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

fn bounded<T: Config<I>, I: 'static>(
	assets: &[(T::AssetKind, T::Balance)],
) -> BoundedVec<(T::AssetKind, T::Balance), T::MaxAssetsPerBounty> {
	BoundedVec::try_from(assets.to_vec()).expect("at most `MaxAssetsPerBounty` assets; qed")
}

/// Reads the in-flight payment id of one asset row of a benchmark bounty.
pub fn get_payment_id<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	asset_kind: T::AssetKind,
) -> Option<PaymentIdOf<T, I>> {
	let entry = match child_bounty_id {
		None => pallet_bounties::BountyAssets::<T, I>::get(parent_bounty_id, asset_kind),
		Some(child_bounty_id) => pallet_bounties::ChildBountyAssets::<T, I>::get((
			parent_bounty_id,
			child_bounty_id,
			asset_kind,
		)),
	}?;

	entry.payment_state.get_attempt_id()
}

/// Tells the `Paymaster` that every in-flight payment of a benchmark bounty has concluded.
fn conclude_payments<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	assets: &[(T::AssetKind, T::Balance)],
) {
	for (asset_kind, _) in assets {
		let payment_id =
			get_payment_id::<T, I>(parent_bounty_id, child_bounty_id, asset_kind.clone())
				.expect("every asset has a payment in flight; qed");
		<T as pallet::Config<I>>::Paymaster::ensure_concluded(payment_id);
	}
}

/// Asserts that every asset of a benchmark bounty has a payment in flight that has not failed,
/// and that a `Paid` event was emitted for it.
fn assert_paid<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	assets: &[(T::AssetKind, T::Balance)],
) {
	for (asset_kind, _) in assets {
		let payment_id =
			get_payment_id::<T, I>(parent_bounty_id, child_bounty_id, asset_kind.clone())
				.expect("no payment attempt");
		assert_has_event::<T, I>(
			Event::Paid {
				index: parent_bounty_id,
				child_index: child_bounty_id,
				asset_kind: asset_kind.clone(),
				payment_id,
			}
			.into(),
		);
		assert_ne!(
			<T as pallet_bounties::Config<I>>::Paymaster::check_payment(payment_id),
			PaymentStatus::Failure
		);
	}
}

/// Create the pre-requisite information needed to `fund_bounty` with a basket of `n` assets.
fn setup_bounty<T: Config<I>, I: 'static>(n: u32) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let min_native_value = T::BountyValueMinimum::get();
	let mut assets = Vec::new();
	let mut child_assets = Vec::new();
	let mut native_value = T::Balance::zero();
	let mut child_native_value = T::Balance::zero();
	for i in 0..n {
		// Even seeds, so that every runtime's factory returns `n` distinct asset kinds.
		let asset_kind = <T as Config<I>>::BenchmarkHelper::create_asset_kind(SEED + 2 * i);
		T::BalanceConverter::ensure_successful(asset_kind.clone());
		let value = T::BalanceConverter::to_asset_balance(min_native_value, asset_kind.clone())
			.map_err(|_| BenchmarkError::Stop("Failed to convert balance"))?;
		let child_value = value / 2u32.into(); // so that retry works
		let to_native = |amount| {
			T::BalanceConverter::from_asset_balance(amount, asset_kind.clone())
				.map_err(|_| BenchmarkError::Stop("Failed to convert balance"))
		};
		native_value = native_value.saturating_add(to_native(value)?);
		child_native_value = child_native_value.saturating_add(to_native(child_value)?);
		assets.push((asset_kind.clone(), value));
		child_assets.push((asset_kind, child_value));
	}
	let curator = account("curator", 0, SEED);
	let child_curator = account("child-curator", 1, SEED);
	let beneficiary =
		<T as Config<I>>::BenchmarkHelper::create_beneficiary([(SEED).try_into().unwrap(); 32]);
	let metadata = T::Preimages::note(Cow::from(vec![5, 6])).unwrap();

	Ok(BenchmarkBounty::<T, I> {
		parent_bounty_id: 0,
		child_bounty_id: 0,
		curator,
		child_curator,
		assets,
		child_assets,
		native_value,
		child_native_value,
		beneficiary,
		metadata,
	})
}

/// Gives the funding source enough of every asset to fund the parent bounty.
fn ensure_funding_source<T: Config<I>, I: 'static>(s: &BenchmarkBounty<T, I>) {
	for (asset_kind, value) in s.assets.iter() {
		let funding_source_account = Bounties::<T, I>::funding_source_account(asset_kind.clone())
			.expect("conversion failed");
		let parent_bounty_account =
			Bounties::<T, I>::bounty_account(s.parent_bounty_id, asset_kind.clone())
				.expect("conversion failed");
		<T as pallet_bounties::Config<I>>::Paymaster::ensure_successful(
			&funding_source_account,
			&parent_bounty_account,
			asset_kind.clone(),
			*value,
		);
	}
}

fn create_parent_bounty<T: Config<I>, I: 'static>(
	n: u32,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let mut s = setup_bounty::<T, I>(n)?;

	let spend_origin = T::SpendOrigin::try_successful_origin().map_err(|_| {
		BenchmarkError::Stop("SpendOrigin has no successful origin required for the benchmark")
	})?;
	let curator_lookup = T::Lookup::unlookup(s.curator.clone());
	ensure_funding_source::<T, I>(&s);

	Bounties::<T, I>::fund_bounty(
		spend_origin,
		bounded::<T, I>(&s.assets),
		curator_lookup,
		s.metadata,
	)?;

	s.parent_bounty_id = pallet_bounties::BountyCount::<T, I>::get() - 1;

	Ok(s)
}

fn create_funded_bounty<T: Config<I>, I: 'static>(
	n: u32,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_parent_bounty::<T, I>(n)?;

	conclude_payments::<T, I>(s.parent_bounty_id, None, &s.assets);
	let caller = account("caller", 0, SEED);
	Bounties::<T, I>::check_status(RawOrigin::Signed(caller).into(), s.parent_bounty_id, None)?;

	Ok(s)
}

fn create_active_parent_bounty<T: Config<I>, I: 'static>(
	n: u32,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_funded_bounty::<T, I>(n)?;
	let curator = s.curator.clone();
	<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(&curator, s.native_value);

	Bounties::<T, I>::accept_curator(RawOrigin::Signed(curator).into(), s.parent_bounty_id, None)?;

	Ok(s)
}

fn create_child_bounty<T: Config<I>, I: 'static>(
	n: u32,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let mut s = create_active_parent_bounty::<T, I>(n)?;
	let child_curator_lookup = T::Lookup::unlookup(s.child_curator.clone());

	Bounties::<T, I>::fund_child_bounty(
		RawOrigin::Signed(s.curator.clone()).into(),
		s.parent_bounty_id,
		bounded::<T, I>(&s.child_assets),
		s.metadata,
		Some(child_curator_lookup),
	)?;
	s.child_bounty_id =
		pallet_bounties::TotalChildBountiesPerParent::<T, I>::get(s.parent_bounty_id) - 1;

	Ok(s)
}

fn create_funded_child_bounty<T: Config<I>, I: 'static>(
	n: u32,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_child_bounty::<T, I>(n)?;
	let caller = account("caller", 0, SEED);

	conclude_payments::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id), &s.child_assets);
	Bounties::<T, I>::check_status(
		RawOrigin::Signed(caller).into(),
		s.parent_bounty_id,
		Some(s.child_bounty_id),
	)?;

	Ok(s)
}

fn create_active_child_bounty<T: Config<I>, I: 'static>(
	n: u32,
) -> Result<BenchmarkBounty<T, I>, BenchmarkError> {
	let s = create_funded_child_bounty::<T, I>(n)?;
	let caller = s.child_curator.clone();
	<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(
		&caller,
		s.child_native_value,
	);

	Bounties::<T, I>::accept_curator(
		RawOrigin::Signed(caller).into(),
		s.parent_bounty_id,
		Some(s.child_bounty_id),
	)?;

	Ok(s)
}

/// Overwrites the status of a benchmark child-/bounty and sets every asset row to
/// `payment_state`.
fn set_phase<T: Config<I>, I: 'static>(
	parent_bounty_id: BountyIndex,
	child_bounty_id: Option<BountyIndex>,
	status: BountyStatus<T::AccountId, T::Beneficiary>,
	assets: &[(T::AssetKind, T::Balance)],
	payment_state: PaymentState<PaymentIdOf<T, I>>,
) -> Result<(), BenchmarkError> {
	pallet_bounties::Pallet::<T, I>::update_bounty_status(
		parent_bounty_id,
		child_bounty_id,
		status,
	)
	.map_err(|_| BenchmarkError::Stop("no bounty"))?;
	for (asset_kind, value) in assets {
		let entry = AssetEntry { value: *value, payment_state: payment_state.clone() };
		match child_bounty_id {
			None => pallet_bounties::BountyAssets::<T, I>::insert(
				parent_bounty_id,
				asset_kind.clone(),
				entry,
			),
			Some(child_bounty_id) => pallet_bounties::ChildBountyAssets::<T, I>::insert(
				(parent_bounty_id, child_bounty_id, asset_kind.clone()),
				entry,
			),
		}
	}

	Ok(())
}

#[instance_benchmarks]
mod benchmarks {
	use super::*;

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `fund_bounty` is un-callable and can use weight=0.
	#[benchmark]
	fn fund_bounty(n: Linear<1, { T::MaxAssetsPerBounty::get() }>) -> Result<(), BenchmarkError> {
		let s = setup_bounty::<T, I>(n)?;

		let spend_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let curator_lookup = T::Lookup::unlookup(s.curator.clone());
		ensure_funding_source::<T, I>(&s);
		let assets = bounded::<T, I>(&s.assets);

		#[extrinsic_call]
		_(spend_origin, assets, curator_lookup, s.metadata);

		let parent_bounty_id = BountyCount::<T, I>::get() - 1;
		assert_last_event::<T, I>(Event::BountyCreated { index: parent_bounty_id }.into());
		assert_paid::<T, I>(parent_bounty_id, None, &s.assets);

		Ok(())
	}

	#[benchmark]
	fn fund_child_bounty(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_parent_bounty::<T, I>(n)?;
		let child_curator_lookup = T::Lookup::unlookup(s.child_curator.clone());
		let assets = bounded::<T, I>(&s.child_assets);

		#[extrinsic_call]
		_(
			RawOrigin::Signed(s.curator),
			s.parent_bounty_id,
			assets,
			s.metadata,
			Some(child_curator_lookup),
		);

		let child_bounty_id =
			pallet_bounties::TotalChildBountiesPerParent::<T, I>::get(s.parent_bounty_id) - 1;
		assert_last_event::<T, I>(
			Event::ChildBountyCreated { index: s.parent_bounty_id, child_index: child_bounty_id }
				.into(),
		);
		assert_paid::<T, I>(s.parent_bounty_id, Some(child_bounty_id), &s.child_assets);

		Ok(())
	}

	/// This benchmark is short-circuited if `SpendOrigin` cannot provide
	/// a successful origin, in which case `propose_curator` is un-callable and can use weight=0.
	#[benchmark]
	fn propose_curator_parent_bounty(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_funded_bounty::<T, I>(n)?;

		let spend_origin =
			T::SpendOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		Bounties::<T, I>::unassign_curator(
			RawOrigin::Signed(s.curator.clone()).into(),
			s.parent_bounty_id,
			None,
		)?;
		let curator_lookup = T::Lookup::unlookup(s.curator.clone());

		#[block]
		{
			assert_ok!(Bounties::<T, I>::propose_curator(
				spend_origin,
				s.parent_bounty_id,
				None,
				curator_lookup,
			));
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
		let s = create_funded_child_bounty::<T, I>(1)?;
		let child_curator_lookup = T::Lookup::unlookup(s.child_curator.clone());

		Bounties::<T, I>::unassign_curator(
			RawOrigin::Signed(s.curator.clone()).into(),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
		)?;

		#[block]
		{
			assert_ok!(Bounties::<T, I>::propose_curator(
				RawOrigin::Signed(s.curator).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
				child_curator_lookup,
			));
		}

		assert_last_event::<T, I>(
			Event::CuratorProposed {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				curator: s.child_curator,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn accept_curator(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_funded_child_bounty::<T, I>(n)?;
		let caller = s.child_curator.clone();

		<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(
			&caller,
			s.child_native_value,
		);

		#[block]
		{
			assert_ok!(Bounties::<T, I>::accept_curator(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			));
		}

		assert_last_event::<T, I>(
			Event::BountyBecameActive {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
				curator: s.child_curator,
			}
			.into(),
		);

		Ok(())
	}

	#[benchmark]
	fn unassign_curator() -> Result<(), BenchmarkError> {
		let s = create_active_child_bounty::<T, I>(1)?;

		#[extrinsic_call]
		_(RawOrigin::Signed(s.curator), s.parent_bounty_id, Some(s.child_bounty_id));

		assert_last_event::<T, I>(
			Event::CuratorUnassigned {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
			}
			.into(),
		);

		Ok(())
	}

	/// Awards a parent bounty, which also reads the per-asset child allocation of every asset.
	#[benchmark]
	fn award_bounty(n: Linear<1, { T::MaxAssetsPerBounty::get() }>) -> Result<(), BenchmarkError> {
		let s = create_active_parent_bounty::<T, I>(n)?;
		let beneficiary_lookup = T::BeneficiaryLookup::unlookup(s.beneficiary.clone());

		#[extrinsic_call]
		_(RawOrigin::Signed(s.curator), s.parent_bounty_id, None, beneficiary_lookup);

		assert_last_event::<T, I>(
			Event::BountyAwarded {
				index: s.parent_bounty_id,
				child_index: None,
				beneficiary: s.beneficiary,
			}
			.into(),
		);
		assert_paid::<T, I>(s.parent_bounty_id, None, &s.assets);

		Ok(())
	}

	#[benchmark]
	fn close_parent_bounty(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_parent_bounty::<T, I>(n)?;

		let reject_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			assert_ok!(Bounties::<T, I>::close_bounty(
				reject_origin.clone(),
				s.parent_bounty_id,
				None
			));
		}

		assert_last_event::<T, I>(
			Event::BountyCanceled { index: s.parent_bounty_id, child_index: None }.into(),
		);
		assert_paid::<T, I>(s.parent_bounty_id, None, &s.assets);
		assert!(Bounties::<T, I>::close_bounty(reject_origin, s.parent_bounty_id, None).is_err());

		Ok(())
	}

	#[benchmark]
	fn close_child_bounty(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_child_bounty::<T, I>(n)?;

		let reject_origin =
			T::RejectOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;

		#[block]
		{
			assert_ok!(Bounties::<T, I>::close_bounty(
				reject_origin.clone(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			));
		}

		assert_last_event::<T, I>(
			Event::BountyCanceled {
				index: s.parent_bounty_id,
				child_index: Some(s.child_bounty_id),
			}
			.into(),
		);
		assert_paid::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id), &s.child_assets);
		assert!(Bounties::<T, I>::close_bounty(
			reject_origin,
			s.parent_bounty_id,
			Some(s.child_bounty_id)
		)
		.is_err());

		Ok(())
	}

	#[benchmark]
	fn check_status_funding(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_child_bounty::<T, I>(n)?;
		let caller = s.curator.clone();
		conclude_payments::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id), &s.child_assets);

		#[block]
		{
			assert_ok!(Bounties::<T, I>::check_status(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			));
		}

		for (asset_kind, _) in s.child_assets {
			assert_has_event::<T, I>(
				Event::BountyFundingProcessed {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					asset_kind,
				}
				.into(),
			);
		}
		let child_bounty =
			pallet_bounties::ChildBounties::<T, I>::get(s.parent_bounty_id, s.child_bounty_id)
				.expect("no bounty");
		assert!(matches!(child_bounty.status, BountyStatus::Funded { .. }));

		Ok(())
	}

	#[benchmark]
	fn check_status_refund(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_child_bounty::<T, I>(n)?;
		let caller = s.curator.clone();

		Bounties::<T, I>::close_bounty(
			RawOrigin::Signed(caller.clone()).into(),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
		)?;
		conclude_payments::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id), &s.child_assets);

		#[block]
		{
			assert_ok!(Bounties::<T, I>::check_status(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			));
		}

		for (asset_kind, _) in s.child_assets {
			assert_has_event::<T, I>(
				Event::BountyRefundProcessed {
					index: s.parent_bounty_id,
					child_index: Some(s.child_bounty_id),
					asset_kind,
				}
				.into(),
			);
		}
		assert_eq!(
			pallet_bounties::ChildBounties::<T, I>::get(s.parent_bounty_id, s.child_bounty_id),
			None
		);

		Ok(())
	}

	/// Completes the payout of a parent bounty whose child-bounty was paid out, so the parent
	/// reads and then removes a child allocation for every asset.
	#[benchmark]
	fn check_status_payout(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_child_bounty::<T, I>(n)?;
		let caller = s.curator.clone();
		let beneficiary_lookup = T::BeneficiaryLookup::unlookup(s.beneficiary.clone());

		// Pay the child-bounty out. The parent keeps its allocation for every asset.
		Bounties::<T, I>::award_bounty(
			RawOrigin::Signed(s.child_curator.clone()).into(),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			beneficiary_lookup.clone(),
		)?;
		conclude_payments::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id), &s.child_assets);
		Bounties::<T, I>::check_status(
			RawOrigin::Signed(caller.clone()).into(),
			s.parent_bounty_id,
			Some(s.child_bounty_id),
		)?;

		Bounties::<T, I>::award_bounty(
			RawOrigin::Signed(caller.clone()).into(),
			s.parent_bounty_id,
			None,
			beneficiary_lookup,
		)?;
		conclude_payments::<T, I>(s.parent_bounty_id, None, &s.assets);

		#[block]
		{
			assert_ok!(Bounties::<T, I>::check_status(
				RawOrigin::Signed(caller).into(),
				s.parent_bounty_id,
				None,
			));
		}

		for ((asset_kind, value), (_, child_value)) in s.assets.into_iter().zip(s.child_assets) {
			assert_has_event::<T, I>(
				Event::BountyPayoutProcessed {
					index: s.parent_bounty_id,
					child_index: None,
					asset_kind,
					value: value - child_value,
					beneficiary: s.beneficiary.clone(),
				}
				.into(),
			);
		}
		assert_eq!(pallet_bounties::Bounties::<T, I>::get(s.parent_bounty_id), None);

		Ok(())
	}

	#[benchmark]
	fn retry_payment_funding(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_child_bounty::<T, I>(n)?;
		let caller = s.curator.clone();

		// Every asset row in the "funding failed" state. The child-bounty status remains
		// `Funding` (set by `create_child_bounty`), which is what `retry_payment` matches on to
		// reinitiate the funding payments.
		conclude_payments::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id), &s.child_assets);
		set_phase::<T, I>(
			s.parent_bounty_id,
			Some(s.child_bounty_id),
			BountyStatus::Funding { curator: s.child_curator.clone() },
			&s.child_assets,
			PaymentState::Failed,
		)?;

		#[block]
		{
			assert_ok!(Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller.clone()).into(),
				s.parent_bounty_id,
				Some(s.child_bounty_id),
			));
		}

		assert_paid::<T, I>(s.parent_bounty_id, Some(s.child_bounty_id), &s.child_assets);

		Ok(())
	}

	/// Retries the refunds of a parent bounty, which also reads the per-asset child allocation of
	/// every asset.
	#[benchmark]
	fn retry_payment_refund(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_parent_bounty::<T, I>(n)?;
		let caller = s.curator.clone();

		// "Refund failed" state: the bounty status is `Closing` and every asset row is `Failed`,
		// so `retry_payment` reinitiates every refund payment.
		set_phase::<T, I>(
			s.parent_bounty_id,
			None,
			BountyStatus::Closing { curator: Some(s.curator.clone()) },
			&s.assets,
			PaymentState::Failed,
		)?;

		#[block]
		{
			assert_ok!(Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller.clone()).into(),
				s.parent_bounty_id,
				None,
			));
		}

		assert_paid::<T, I>(s.parent_bounty_id, None, &s.assets);

		Ok(())
	}

	/// Retries the payouts of a parent bounty, which also reads the per-asset child allocation of
	/// every asset.
	#[benchmark]
	fn retry_payment_payout(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_parent_bounty::<T, I>(n)?;
		let caller = s.curator.clone();

		// "Payout failed" state: the bounty status is `Awarding` and every asset row is `Failed`,
		// so `retry_payment` reinitiates every payout payment.
		set_phase::<T, I>(
			s.parent_bounty_id,
			None,
			BountyStatus::Awarding {
				curator: s.curator.clone(),
				beneficiary: s.beneficiary.clone(),
			},
			&s.assets,
			PaymentState::Failed,
		)?;

		#[block]
		{
			assert_ok!(Bounties::<T, I>::retry_payment(
				RawOrigin::Signed(caller.clone()).into(),
				s.parent_bounty_id,
				None,
			));
		}

		assert_paid::<T, I>(s.parent_bounty_id, None, &s.assets);
		// A repeated retry is a no-op now that the asset rows are back in `Attempted`.
		let ids = s
			.assets
			.iter()
			.map(|(asset_kind, _)| {
				get_payment_id::<T, I>(s.parent_bounty_id, None, asset_kind.clone())
			})
			.collect::<Vec<_>>();
		assert_ok!(Bounties::<T, I>::retry_payment(
			RawOrigin::Signed(caller).into(),
			s.parent_bounty_id,
			None,
		));
		assert_eq!(
			s.assets
				.iter()
				.map(|(asset_kind, _)| get_payment_id::<T, I>(
					s.parent_bounty_id,
					None,
					asset_kind.clone()
				))
				.collect::<Vec<_>>(),
			ids
		);

		Ok(())
	}

	#[benchmark]
	fn increase_value(
		n: Linear<1, { T::MaxAssetsPerBounty::get() }>,
	) -> Result<(), BenchmarkError> {
		let s = create_active_parent_bounty::<T, I>(n)?;
		let caller = s.curator.clone();
		let (asset_kind, value) = s.assets[0].clone();
		let new_value = value + value;

		// Ensure the curator can fund the (possibly larger) deposit for the new value.
		<T as pallet_bounties::Config<I>>::Consideration::ensure_successful(
			&caller,
			s.native_value.saturating_add(s.native_value),
		);

		#[extrinsic_call]
		_(RawOrigin::Signed(caller), s.parent_bounty_id, Box::new(asset_kind.clone()), value);

		assert_last_event::<T, I>(
			Event::BountyValueIncreased {
				index: s.parent_bounty_id,
				asset_kind,
				old_value: value,
				new_value,
			}
			.into(),
		);

		Ok(())
	}

	impl_benchmark_test_suite! {
		Pallet,
		crate::mock::ExtBuilder::default().build(),
		crate::mock::Test
	}
}
