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
use frame_benchmarking::{benchmarks, whitelisted_caller, BenchmarkError, BenchmarkResult};
use frame_support::{assert_ok, weights::Weight};
use frame_system::RawOrigin;
use xcm::latest::prelude::*;
use xcm_builder::EnsureDelivery;
use xcm_executor::traits::FeeReason;

type RuntimeOrigin<T> = <T as frame_system::Config>::RuntimeOrigin;

/// Pallet we're benchmarking here.
pub struct Pallet<T: Config>(crate::Pallet<T>);

/// Trait that must be implemented by runtime to be able to benchmark pallet properly.
pub trait Config: crate::Config {
	/// Helper that ensures successful delivery for extrinsics/benchmarks which need `SendXcm`.
	type DeliveryHelper: EnsureDelivery;

	/// A `Location` that can be reached via `XcmRouter`. Used only in benchmarks.
	///
	/// If `None`, the benchmarks that depend on a reachable destination will be skipped.
	fn reachable_dest() -> Option<Location> {
		None
	}

	/// A `(Asset, Location)` pair representing asset and the destination it can be
	/// teleported to. Used only in benchmarks.
	///
	/// Implementation should also make sure `dest` is reachable/connected.
	///
	/// If `None`, the benchmarks that depend on this will default to `Weight::MAX`.
	fn teleportable_asset_and_dest() -> Option<(Asset, Location)> {
		None
	}

	/// A `(Asset, Location)` pair representing asset and the destination it can be
	/// reserve-transferred to. Used only in benchmarks.
	///
	/// Implementation should also make sure `dest` is reachable/connected.
	///
	/// If `None`, the benchmarks that depend on this will default to `Weight::MAX`.
	fn reserve_transferable_asset_and_dest() -> Option<(Asset, Location)> {
		None
	}

	/// Sets up a complex transfer (usually consisting of a teleport and reserve-based transfer), so
	/// that runtime can properly benchmark `transfer_assets()` extrinsic. Should return a tuple
	/// `(Asset, u32, Location, dyn FnOnce())` representing the assets to transfer, the
	/// `u32` index of the asset to be used for fees, the destination chain for the transfer, and a
	/// `verify()` closure to verify the intended transfer side-effects.
	///
	/// Implementation should make sure the provided assets can be transacted by the runtime, there
	/// are enough balances in the involved accounts, and that `dest` is reachable/connected.
	///
	/// Used only in benchmarks.
	///
	/// If `None`, the benchmarks that depend on this will default to `Weight::MAX`.
	fn set_up_complex_asset_transfer() -> Option<(Assets, u32, Location, Box<dyn FnOnce()>)> {
		None
	}

	/// Gets an asset that can be handled by the AssetTransactor.
	///
	/// Used only in benchmarks.
	///
	/// Used, for example, in the benchmark for `claim_assets`.
	fn get_asset() -> Asset;
}

benchmarks! {
	send {
		let send_origin =
			T::SendXcmOrigin::try_successful_origin().map_err(|_| BenchmarkError::Weightless)?;
		if T::SendXcmOrigin::try_origin(send_origin.clone()).is_err() {
			return Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
		}
		let msg = Xcm(vec![ClearOrigin]);
		let versioned_dest: VersionedLocation = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?
		.into();
		let versioned_msg = VersionedXcm::from(msg);
	}: _<RuntimeOrigin<T>>(send_origin, Box::new(versioned_dest), Box::new(versioned_msg))

	teleport_assets {
		let (asset, destination) = T::teleportable_asset_and_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;

		let assets: Assets = asset.clone().into();

		let caller: T::AccountId = whitelisted_caller();
		let send_origin = RawOrigin::Signed(caller.clone());
		let origin_location = T::ExecuteXcmOrigin::try_origin(send_origin.clone().into())
			.map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		if !T::XcmTeleportFilter::contains(&(origin_location.clone(), assets.clone().into_inner())) {
			return Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
		}

		// Ensure that origin can send to destination (e.g. setup delivery fees, ensure router setup, ...)
		let (_, _) = T::DeliveryHelper::ensure_successful_delivery(
			&origin_location,
			&destination,
			FeeReason::ChargeFees,
		);

		match &asset.fun {
			Fungible(amount) => {
				// Add transferred_amount to origin
				<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::deposit_asset(
					&Asset { fun: Fungible(*amount), id: asset.id },
					&origin_location,
					None,
				).map_err(|error| {
				  tracing::error!("Fungible asset couldn't be deposited, error: {:?}", error);
				  BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX))
				})?;
			},
			NonFungible(instance) => {
				<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::deposit_asset(&asset, &origin_location, None)
					.map_err(|error| {
						tracing::error!("Nonfungible asset couldn't be deposited, error: {:?}", error);
						BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX))
					})?;
			}
		};

		let recipient = [0u8; 32];
		let versioned_dest: VersionedLocation = destination.into();
		let versioned_beneficiary: VersionedLocation =
			AccountId32 { network: None, id: recipient.into() }.into();
		let versioned_assets: VersionedAssets = assets.into();
	}: _<RuntimeOrigin<T>>(send_origin.into(), Box::new(versioned_dest), Box::new(versioned_beneficiary), Box::new(versioned_assets), 0)

	reserve_transfer_assets {
		let (asset, destination) = T::reserve_transferable_asset_and_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;

		let assets: Assets = asset.clone().into();

		let caller: T::AccountId = whitelisted_caller();
		let send_origin = RawOrigin::Signed(caller.clone());
		let origin_location = T::ExecuteXcmOrigin::try_origin(send_origin.clone().into())
			.map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		if !T::XcmReserveTransferFilter::contains(&(origin_location.clone(), assets.clone().into_inner())) {
			return Err(BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))
		}

		// Ensure that origin can send to destination (e.g. setup delivery fees, ensure router setup, ...)
		let (_, _) = T::DeliveryHelper::ensure_successful_delivery(
			&origin_location,
			&destination,
			FeeReason::ChargeFees,
		);

		match &asset.fun {
			Fungible(amount) => {
				// Add transferred_amount to origin
				<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::deposit_asset(
					&Asset { fun: Fungible(*amount), id: asset.id.clone() },
					&origin_location,
					None,
				).map_err(|error| {
				  tracing::error!("Fungible asset couldn't be deposited, error: {:?}", error);
				  BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX))
				})?;
			},
			NonFungible(instance) => {
				<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::deposit_asset(&asset, &origin_location, None)
					.map_err(|error| {
						tracing::error!("Nonfungible asset couldn't be deposited, error: {:?}", error);
						BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX))
					})?;
			}
		};

		let recipient = [0u8; 32];
		let versioned_dest: VersionedLocation = destination.clone().into();
		let versioned_beneficiary: VersionedLocation =
			AccountId32 { network: None, id: recipient.into() }.into();
		let versioned_assets: VersionedAssets = assets.into();
	}: _<RuntimeOrigin<T>>(send_origin.into(), Box::new(versioned_dest), Box::new(versioned_beneficiary), Box::new(versioned_assets), 0)
	verify {
		match &asset.fun {
			Fungible(amount) => {
				assert_ok!(<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::withdraw_asset(
					&Asset { fun: Fungible(*amount), id: asset.id },
					&destination,
					None,
				));
			},
			NonFungible(instance) => {
				assert_ok!(<T::XcmExecutor as XcmAssetTransfers>::AssetTransactor::withdraw_asset(
					&asset,
					&destination,
					None,
				));
			}
		};
	}

	transfer_assets {
		let (assets, fee_index, destination, verify) = T::set_up_complex_asset_transfer().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;
		let caller: T::AccountId = whitelisted_caller();
		let send_origin = RawOrigin::Signed(caller.clone());
		let recipient = [0u8; 32];
		let versioned_dest: VersionedLocation = destination.into();
		let versioned_beneficiary: VersionedLocation =
			AccountId32 { network: None, id: recipient.into() }.into();
		let versioned_assets: VersionedAssets = assets.into();
	}: _<RuntimeOrigin<T>>(send_origin.into(), Box::new(versioned_dest), Box::new(versioned_beneficiary), Box::new(versioned_assets), 0, WeightLimit::Unlimited)
	verify {
		// run provided verification function
		verify();
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
	}: _<RuntimeOrigin<T>>(execute_origin, Box::new(versioned_msg), Weight::MAX)

	force_xcm_version {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;
		let xcm_version = 2;
	}: _(RawOrigin::Root, Box::new(loc), xcm_version)

	force_default_xcm_version {}: _(RawOrigin::Root, Some(2))

	force_subscribe_version_notify {
		let versioned_loc: VersionedLocation = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?
		.into();
	}: _(RawOrigin::Root, Box::new(versioned_loc))

	force_unsubscribe_version_notify {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)),
		)?;
		let versioned_loc: VersionedLocation = loc.clone().into();
		let _ = crate::Pallet::<T>::request_version_notify(loc);
	}: _(RawOrigin::Root, Box::new(versioned_loc))

	force_suspension {}: _(RawOrigin::Root, true)

	migrate_supported_version {
		let old_version = XCM_VERSION - 1;
		let loc = VersionedLocation::from(Location::from(Parent));
		SupportedVersion::<T>::insert(old_version, loc, old_version);
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateSupportedVersion, Weight::zero());
	}

	migrate_version_notifiers {
		let old_version = XCM_VERSION - 1;
		let loc = VersionedLocation::from(Location::from(Parent));
		VersionNotifiers::<T>::insert(old_version, loc, 0);
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateVersionNotifiers, Weight::zero());
	}

	already_notified_target {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(T::DbWeight::get().reads(1))),
		)?;
		let loc = VersionedLocation::from(loc);
		let current_version = T::AdvertisedXcmVersion::get();
		VersionNotifyTargets::<T>::insert(current_version, loc, (0, Weight::zero(), current_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::NotifyCurrentTargets(None), Weight::zero());
	}

	notify_current_targets {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(T::DbWeight::get().reads_writes(1, 3))),
		)?;
		let loc = VersionedLocation::from(loc);
		let current_version = T::AdvertisedXcmVersion::get();
		let old_version = current_version - 1;
		VersionNotifyTargets::<T>::insert(current_version, loc, (0, Weight::zero(), old_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::NotifyCurrentTargets(None), Weight::zero());
	}

	notify_target_migration_fail {
		let newer_xcm_version = xcm::prelude::XCM_VERSION;
		let older_xcm_version = newer_xcm_version - 1;
		let bad_location: Location = Plurality {
			id: BodyId::Unit,
			part: BodyPart::Voice,
		}.into();
		let bad_location = VersionedLocation::from(bad_location)
			.into_version(older_xcm_version)
			.expect("Version convertion should work");
		let current_version = T::AdvertisedXcmVersion::get();
		VersionNotifyTargets::<T>::insert(current_version, bad_location, (0, Weight::zero(), current_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateAndNotifyOldTargets, Weight::zero());
	}

	migrate_version_notify_targets {
		let current_version = T::AdvertisedXcmVersion::get();
		let old_version = current_version - 1;
		let loc = VersionedLocation::from(Location::from(Parent));
		VersionNotifyTargets::<T>::insert(old_version, loc, (0, Weight::zero(), current_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateAndNotifyOldTargets, Weight::zero());
	}

	migrate_and_notify_old_targets {
		let loc = T::reachable_dest().ok_or(
			BenchmarkError::Override(BenchmarkResult::from_weight(T::DbWeight::get().reads_writes(1, 3))),
		)?;
		let loc = VersionedLocation::from(loc);
		let old_version = T::AdvertisedXcmVersion::get() - 1;
		VersionNotifyTargets::<T>::insert(old_version, loc, (0, Weight::zero(), old_version));
	}: {
		crate::Pallet::<T>::check_xcm_version_change(VersionMigrationStage::MigrateAndNotifyOldTargets, Weight::zero());
	}

	new_query {
		let responder = Location::from(Parent);
		let timeout = 1u32.into();
		let match_querier = Location::from(Here);
	}: {
		crate::Pallet::<T>::new_query(responder, timeout, match_querier);
	}

	take_response {
		let responder = Location::from(Parent);
		let timeout = 1u32.into();
		let match_querier = Location::from(Here);
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

	claim_assets {
		let claim_origin = RawOrigin::Signed(whitelisted_caller());
		let claim_location = T::ExecuteXcmOrigin::try_origin(claim_origin.clone().into()).map_err(|_| BenchmarkError::Override(BenchmarkResult::from_weight(Weight::MAX)))?;
		let asset: Asset = T::get_asset();
		// Trap assets for claiming later
		crate::Pallet::<T>::drop_assets(
			&claim_location,
			asset.clone().into(),
			&XcmContext { origin: None, message_id: [0u8; 32], topic: None }
		);
		let versioned_assets = VersionedAssets::from(Assets::from(asset));
	}: _<RuntimeOrigin<T>>(claim_origin.into(), Box::new(versioned_assets), Box::new(VersionedLocation::from(claim_location)))

	impl_benchmark_test_suite!(
		Pallet,
		crate::mock::new_test_ext_with_balances(Vec::new()),
		crate::mock::Test
	);
}

pub mod helpers {
	use super::*;
	pub fn native_teleport_as_asset_transfer<T>(
		native_asset_location: Location,
		destination: Location,
	) -> Option<(Assets, u32, Location, Box<dyn FnOnce()>)>
	where
		T: Config + pallet_balances::Config,
		u128: From<<T as pallet_balances::Config>::Balance>,
	{
		// Relay/native token can be teleported to/from AH.
		let amount = T::ExistentialDeposit::get() * 100u32.into();
		let assets: Assets =
			Asset { fun: Fungible(amount.into()), id: AssetId(native_asset_location) }.into();
		let fee_index = 0u32;

		// Give some multiple of transferred amount
		let balance = amount * 10u32.into();
		let who = whitelisted_caller();
		let _ =
			<pallet_balances::Pallet::<T> as frame_support::traits::Currency<_>>::make_free_balance_be(&who, balance);
		// verify initial balance
		assert_eq!(pallet_balances::Pallet::<T>::free_balance(&who), balance);

		// verify transferred successfully
		let verify = Box::new(move || {
			// verify balance after transfer, decreased by transferred amount (and delivery fees)
			assert!(pallet_balances::Pallet::<T>::free_balance(&who) <= balance - amount);
		});
		Some((assets, fee_index, destination, verify))
	}
}
