// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Polkadot.

// Polkadot is free software: you can redistribute it and/or modify
// it under the terms of the GNU General Public License as published by
// the Free Software Foundation, either version 3 of the License, or
// (at your option) any later version.

// Polkadot is distributed in the hope that it will be useful,
// but WITHOUT ANY WARRANTY; without even the implied warranty of
// MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.  See the
// GNU General Public License for more details.

// You should have received a copy of the GNU General Public License
// along with Polkadot.  If not, see <http://www.gnu.org/licenses/>.

use super::*;
use bounded_collections::{ConstU32, WeakBoundedVec};
use frame_benchmarking::{benchmarks, whitelisted_caller, BenchmarkError, BenchmarkResult};
use frame_support::{traits::Currency, weights::Weight};
use frame_system::RawOrigin;
use sp_std::prelude::*;
use xcm::{latest::prelude::*, v2};

type RuntimeOrigin<T> = <T as frame_system::Config>::RuntimeOrigin;

// existential deposit multiplier
const ED_MULTIPLIER: u32 = 100;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config>(crate::Pallet<T>);

/// Trait that must be implemented by runtime to be able to benchmark pallet properly.
pub trait Config: crate::Config {
	/// A `MultiLocation` that can be reached via `XcmRouter`. Used only in benchmarks.
	///
	/// If `None`, the benchmarks that depend on a reachable destination will be skipped.
	fn reachable_dest() -> Option<MultiLocation> {
		None
	}

	/// A `(MultiAsset, MultiLocation)` pair representing asset and the destination it can be
	/// teleported to. Used only in benchmarks.
	///
	/// Implementation should also make sure `dest` is reachable/connected.
	///
	/// If `None`, the benchmarks that depend on this will be skipped.
	fn teleportable_asset_and_dest() -> Option<(MultiAsset, MultiLocation)> {
		None
	}

	/// A `(MultiAsset, MultiLocation)` pair representing asset and the destination it can be
	/// reserve-transferred to. Used only in benchmarks.
	///
	/// Implementation should also make sure `dest` is reachable/connected.
	///
	/// If `None`, the benchmarks that depend on this will be skipped.
	fn reserve_transferable_asset_and_dest() -> Option<(MultiAsset, MultiLocation)> {
		None
	}
}

benchmarks! {
	where_clause {
		where
			T: pallet_balances::Config,
			<T as pallet_balances::Config>::Balance: From<u128> + Into<u128>,
	}
	send {
		let send_origin =
			T::SendXcmOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		if T::SendXcmOrigin::try_origin(send_origin.clone()).is_err() {
			return Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
		}
		let msg = Xcm(vec![ClearOrigin]);
		let versioned_dest: VersionedMultiLocation = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?
		.into();
		let versioned_msg = VersionedXcm::from(msg);
	}: _<RuntimeOrigin<T>>(send_origin, Box::new(versioned_dest), Box::new(versioned_msg))

	teleport_assets {
		let (asset, destination) = T::teleportable_asset_and_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;

		let transferred_amount = match &asset.fun {
			Fungible(amount) => *amount,
			_ => return Err(BenchmarkError::Stop("Benchmark asset not fungible")),
		}.into();
		let assets: MultiAssets = asset.into();

		let existential_deposit = T::ExistentialDeposit::get();
		let caller = whitelisted_caller();

		// Give some multiple of the existential deposit
		let balance = existential_deposit.saturating_mul(ED_MULTIPLIER.into());
		assert!(balance >= transferred_amount);
		let _ = <pallet_balances::Pallet<T> as Currency<_>>::make_free_balance_be(&caller, balance);
		// verify initial balance
		assert_eq!(pallet_balances::Pallet::<T>::free_balance(&caller), balance);

		let send_origin = RawOrigin::Signed(caller.clone());
		let origin_location = T::ExecuteXcmOrigin::try_origin(send_origin.clone().into())
			.map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		if !T::XcmTeleportFilter::contains(&(origin_location, assets.clone().into_inner())) {
			return Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
		}

		let recipient = [0u8; 32];
		let versioned_dest: VersionedMultiLocation = destination.into();
		let versioned_beneficiary: VersionedMultiLocation =
			AccountId32 { network: None, id: recipient.into() }.into();
		let versioned_assets: VersionedMultiAssets = assets.into();
	}: _<RuntimeOrigin<T>>(send_origin.into(), Box::new(versioned_dest), Box::new(versioned_beneficiary), Box::new(versioned_assets), 0)
	verify {
		// verify balance after transfer, decreased by transferred amount (+ maybe XCM delivery fees)
		assert!(pallet_balances::Pallet::<T>::free_balance(&caller) <= balance - transferred_amount);
	}

	reserve_transfer_assets {
		let (asset, destination) = T::reserve_transferable_asset_and_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;

		let transferred_amount = match &asset.fun {
			Fungible(amount) => *amount,
			_ => return Err(BenchmarkError::Stop("Benchmark asset not fungible")),
		}.into();
		let assets: MultiAssets = asset.into();

		let existential_deposit = T::ExistentialDeposit::get();
		let caller = whitelisted_caller();

		// Give some multiple of the existential deposit
		let balance = existential_deposit.saturating_mul(ED_MULTIPLIER.into());
		assert!(balance >= transferred_amount);
		let _ = <pallet_balances::Pallet<T> as Currency<_>>::make_free_balance_be(&caller, balance);
		// verify initial balance
		assert_eq!(pallet_balances::Pallet::<T>::free_balance(&caller), balance);

		let send_origin = RawOrigin::Signed(caller.clone());
		let origin_location = T::ExecuteXcmOrigin::try_origin(send_origin.clone().into())
			.map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		if !T::XcmReserveTransferFilter::contains(&(origin_location, assets.clone().into_inner())) {
			return Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
		}

		let recipient = [0u8; 32];
		let versioned_dest: VersionedMultiLocation = destination.into();
		let versioned_beneficiary: VersionedMultiLocation =
			AccountId32 { network: None, id: recipient.into() }.into();
		let versioned_assets: VersionedMultiAssets = assets.into();
	}: _<RuntimeOrigin<T>>(send_origin.into(), Box::new(versioned_dest), Box::new(versioned_beneficiary), Box::new(versioned_assets), 0)
	verify {
		// verify balance after transfer, decreased by transferred amount (+ maybe XCM delivery fees)
		assert!(pallet_balances::Pallet::<T>::free_balance(&caller) <= balance - transferred_amount);
	}

	execute {
		let execute_origin =
			T::ExecuteXcmOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		let origin_location = T::ExecuteXcmOrigin::try_origin(execute_origin.clone())
			.map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		let msg = Xcm(vec![ClearOrigin]);
		if !T::XcmExecuteFilter::contains(&(origin_location, msg.clone())) {
			return Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
		}
		let versioned_msg = VersionedXcm::from(msg);
	}: _<RuntimeOrigin<T>>(execute_origin, Box::new(versioned_msg), Weight::zero())

	force_xcm_version {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;
		let xcm_version = 2;
	}: _(RawOrigin::Root, Box::new(loc), xcm_version)

	force_default_xcm_version {}: _(RawOrigin::Root, Some(2))

	force_subscribe_version_notify {
		let versioned_loc: VersionedMultiLocation = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?
		.into();
	}: _(RawOrigin::Root, Box::new(versioned_loc))

	force_unsubscribe_version_notify {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;
		let versioned_loc: VersionedMultiLocation = loc.into();
		let _ = crate::Pallet::<T>::request_version_notify(loc);
	}: _(RawOrigin::Root, Box::new(versioned_loc))

	force_suspension {}: _(RawOrigin::Root, true)

	migrate_supported_version {
		let old_version = XCM_VERSION - 1;
		let loc = VersionedMultiLocation::from(MultiLocation::from(Parent));
		SupportedVersion::<T>::insert(old_version, loc, old_version);
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateSupportedVersion, Weight::zero());
	}

	migrate_version_notifiers {
		let old_version = XCM_VERSION - 1;
		let loc = VersionedMultiLocation::from(MultiLocation::from(Parent));
		VersionNotifiers::<T>::insert(old_version, loc, 0);
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateVersionNotifiers, Weight::zero());
	}

	already_notified_target {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(T::DbWeight::get().reads(1))),
		)?;
		let loc = VersionedMultiLocation::from(loc);
		let current_version = T::AdvertisedXcmVersion::get();
		VersionNotifyTargets::<T>::insert(current_version, loc, (0, Weight::zero(), current_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::NotifyCurrentTargets(None), Weight::zero());
	}

	notify_current_targets {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(T::DbWeight::get().reads_writes(1, 3))),
		)?;
		let loc = VersionedMultiLocation::from(loc);
		let current_version = T::AdvertisedXcmVersion::get();
		let old_version = current_version - 1;
		VersionNotifyTargets::<T>::insert(current_version, loc, (0, Weight::zero(), old_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::NotifyCurrentTargets(None), Weight::zero());
	}

	notify_target_migration_fail {
		let bad_loc: v2::MultiLocation = v2::Junction::Plurality {
			id: v2::BodyId::Named(WeakBoundedVec::<u8, ConstU32<32>>::try_from(vec![0; 32])
				.expect("vec has a length of 32 bits; qed")),
			part: v2::BodyPart::Voice,
		}
		.into();
		let bad_loc = VersionedMultiLocation::from(bad_loc);
		let current_version = T::AdvertisedXcmVersion::get();
		VersionNotifyTargets::<T>::insert(current_version, bad_loc, (0, Weight::zero(), current_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateAndNotifyOldTargets, Weight::zero());
	}

	migrate_version_notify_targets {
		let current_version = T::AdvertisedXcmVersion::get();
		let old_version = current_version - 1;
		let loc = VersionedMultiLocation::from(MultiLocation::from(Parent));
		VersionNotifyTargets::<T>::insert(old_version, loc, (0, Weight::zero(), current_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateAndNotifyOldTargets, Weight::zero());
	}

	migrate_and_notify_old_targets {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(T::DbWeight::get().reads_writes(1, 3))),
		)?;
		let loc = VersionedMultiLocation::from(loc);
		let old_version = T::AdvertisedXcmVersion::get() - 1;
		VersionNotifyTargets::<T>::insert(old_version, loc, (0, Weight::zero(), old_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateAndNotifyOldTargets, Weight::zero());
	}

	new_query {
		let responder = MultiLocation::from(Parent);
		let timeout = 1u32.into();
		let match_querier = MultiLocation::from(Here);
	}: {
		crate::Pallet::<T>::new_query(responder, timeout, match_querier);
	}

	take_response {
		let responder = MultiLocation::from(Parent);
		let timeout = 1u32.into();
		let match_querier = MultiLocation::from(Here);
		let query_id = crate::Pallet::<T>::new_query(responder, timeout, match_querier);
		let infos = (0 .. xcm::v3::MaxPalletsInfo::get()).map(|_| PalletInfo::new(
			u32::MAX,
			(0..xcm::v3::MaxPalletNameLen::get()).map(|_| 97u8).collect::<Vec<_>>().try_into().unwrap(),
			(0..xcm::v3::MaxPalletNameLen::get()).map(|_| 97u8).collect::<Vec<_>>().try_into().unwrap(),
			u32::MAX,
			u32::MAX,
			u32::MAX,
		).unwrap()).collect::<Vec<_>>();
		crate::Pallet::<T>::expect_response(query_id, Response::PalletsInfo(infos.try_into().unwrap()));

	}: {
		<crate::Pallet::<T> as QueryHandler>::take_response(query_id);
	}

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext_with_balances(Vec::new()),
		crate::mock::Test
	);
}
