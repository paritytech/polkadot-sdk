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

//! Migration that upgrades the XCM data stored by pallets to the latest XCM version.
//!
//! [`MigrateXcmToLatest`] is made of [stages](MigrationStage), one per storage item that holds
//! versioned XCM data. A runtime picks the stages for the pallets it has:
//! - [`AssetRateStage`] re-keys `pallet_asset_rate::ConversionRateToNative`.
//! - [`TreasurySpendsStage`] upgrades the asset kind and beneficiary of `pallet_treasury::Spends`.
//! - [`BountiesStage`] upgrades the asset kind of `pallet_multi_asset_bounties::Bounties`.
//!
//! It's a multi-block migration, to add to `pallet_migrations::Config::Migrations`:
//!
//! ```ignore
//! type Migrations = (
//! 	polkadot_runtime_common::xcm_migration::MigrateXcmToLatest<
//! 		(
//! 			AssetRateStage<Runtime, VersionedLocatableAssetToLatest, AssetRateWeights>,
//! 			TreasurySpendsStage<
//! 				Runtime,
//! 				TreasuryInstance,
//! 				VersionedLocatableAssetToLatest,
//! 				VersionedLocationToLatest,
//! 				TreasuryWeights,
//! 			>,
//! 		),
//! 	>,
//! );
//! ```
//!
//! The stages read storage with the types it held before the upgrade (see [`legacy`]), so they
//! also handle values that the current types can no longer decode, like the XCM v3 variant of
//! [`VersionedLocatableAsset`].
//!
//! Every stage is idempotent: entries that are already in the latest version are left as they are.
//! Entries that can't be upgraded (e.g. XCM v3 abstract asset ids, which no longer exist) are
//! logged and removed, as they could no longer be decoded.

#[cfg(any(feature = "try-runtime", feature = "runtime-benchmarks", test))]
use alloc::vec::Vec;
use codec::{Decode, Encode, FullCodec, FullEncode, MaxEncodedLen};
use core::{fmt::Debug, marker::PhantomData};
use frame_support::{
	migrations::{MigrationId, SteppedMigration, SteppedMigrationError},
	storage::{generator, migration::storage_key_iter, PrefixIterator},
	traits::ConstU32,
	weights::WeightMeter,
	BoundedVec, ReversibleStorageHasher,
};
use scale_info::TypeInfo;
use xcm::{IdentifyVersion, IntoVersion, VersionedAssetId, VersionedLocation};

use crate::impls::VersionedLocatableAsset;

#[cfg(feature = "runtime-benchmarks")]
pub mod benchmarking;
pub mod legacy;
mod stages;
#[cfg(test)]
mod tests;
pub mod weights;

pub use stages::{AssetRateStage, BountiesStage, TreasurySpendsStage};
pub use weights::WeightInfo;

/// The log target of this migration.
const LOG_TARGET: &str = "runtime::xcm-migration";

/// The maximum length of a [`RawKey`].
///
/// Enough for a storage map key hashed with `Blake2_128Concat` and holding a
/// [`legacy::VersionedLocatableAsset`], which is the largest key migrated.
pub const MAX_RAW_KEY_LEN: u32 = 2048;

/// A raw storage key, as used to resume iterating over a storage map.
pub type RawKey = BoundedVec<u8, ConstU32<MAX_RAW_KEY_LEN>>;

/// The result of [`UpgradeToLatest::upgrade`].
#[derive(Debug, PartialEq)]
pub enum Upgraded<T> {
	/// The value was already in the latest XCM version, its encoding is the same.
	Unchanged(T),
	/// The value was upgraded, its encoding changed.
	Changed(T),
}

impl<T> Upgraded<T> {
	/// Whether the value was upgraded.
	pub fn is_changed(&self) -> bool {
		matches!(self, Upgraded::Changed(_))
	}

	/// The value in the latest XCM version.
	pub fn into_inner(self) -> T {
		match self {
			Upgraded::Unchanged(value) | Upgraded::Changed(value) => value,
		}
	}
}

/// Upgrades a stored value to the latest XCM version.
pub trait UpgradeToLatest<T> {
	/// The type values were stored as before the upgrade.
	///
	/// It must decode every encoding of `T`, as storage can hold values in either type.
	type Stored: FullCodec + Clone + Debug + From<T>;

	/// Returns `value` in the latest XCM version, or `value` itself if it can't be represented
	/// in it.
	fn upgrade(value: Self::Stored) -> Result<Upgraded<T>, Self::Stored>;

	/// Returns a value of the largest size that `upgrade` changes, used as the worst case in
	/// benchmarks.
	///
	/// `None` means `upgrade` doesn't change any value; benchmarks then use a value from the
	/// benchmark helper of the pallet being migrated.
	#[cfg(feature = "runtime-benchmarks")]
	fn worst_case_outdated() -> Option<Self::Stored> {
		None
	}
}

/// Leaves values as they are, for stored values that hold no outdated XCM data.
impl<T: FullCodec + Clone + Debug> UpgradeToLatest<T> for sp_runtime::traits::Identity {
	type Stored = T;

	fn upgrade(value: T) -> Result<Upgraded<T>, T> {
		Ok(Upgraded::Unchanged(value))
	}
}

/// Upgrades a [`VersionedLocatableAsset`] to the latest XCM version, including the values of its
/// removed XCM v3 variant.
pub struct VersionedLocatableAssetToLatest;
impl UpgradeToLatest<VersionedLocatableAsset> for VersionedLocatableAssetToLatest {
	type Stored = legacy::VersionedLocatableAsset;

	fn upgrade(
		asset: legacy::VersionedLocatableAsset,
	) -> Result<Upgraded<VersionedLocatableAsset>, legacy::VersionedLocatableAsset> {
		use legacy::VersionedLocatableAsset::*;

		let (location, asset_id) = match &asset {
			V3 { location, asset_id } => {
				(VersionedLocation::V3(*location), VersionedAssetId::V3(*asset_id))
			},
			V4 { location, asset_id } => {
				(VersionedLocation::V4(location.clone()), VersionedAssetId::V4(asset_id.clone()))
			},
			V5 { location, asset_id } => {
				return Ok(Upgraded::Unchanged((location.clone(), asset_id.clone()).into()))
			},
		};
		match (xcm::v5::Location::try_from(location), xcm::v5::AssetId::try_from(asset_id)) {
			(Ok(location), Ok(asset_id)) => Ok(Upgraded::Changed((location, asset_id).into())),
			_ => Err(asset),
		}
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn worst_case_outdated() -> Option<legacy::VersionedLocatableAsset> {
		Some(legacy::VersionedLocatableAsset::V3 {
			location: worst_case_v3_location(),
			asset_id: xcm::v3::AssetId::Concrete(worst_case_v3_location()),
		})
	}
}

/// Upgrades a [`VersionedLocation`] to the latest XCM version.
pub struct VersionedLocationToLatest;
impl UpgradeToLatest<VersionedLocation> for VersionedLocationToLatest {
	type Stored = VersionedLocation;

	fn upgrade(
		location: VersionedLocation,
	) -> Result<Upgraded<VersionedLocation>, VersionedLocation> {
		if location.identify_version() == xcm::latest::VERSION {
			return Ok(Upgraded::Unchanged(location));
		}
		location.clone().into_latest().map(Upgraded::Changed).map_err(|()| location)
	}

	#[cfg(feature = "runtime-benchmarks")]
	fn worst_case_outdated() -> Option<VersionedLocation> {
		Some(VersionedLocation::V3(worst_case_v3_location()))
	}
}

/// Returns the worst case value of `Upgrade` to benchmark with, or `fallback` if it has none.
#[cfg(feature = "runtime-benchmarks")]
pub(crate) fn worst_case_stored<Upgrade: UpgradeToLatest<T>, T>(
	fallback: impl FnOnce() -> T,
) -> Upgrade::Stored {
	Upgrade::worst_case_outdated().unwrap_or_else(|| fallback().into())
}

/// Whether `value` is in the latest XCM version, i.e. `Upgrade` leaves it as it is.
#[cfg(feature = "try-runtime")]
pub(crate) fn is_latest<Upgrade: UpgradeToLatest<T>, T: Clone>(value: &T) -> bool {
	matches!(Upgrade::upgrade(value.clone().into()), Ok(Upgraded::Unchanged(_)))
}

/// The largest XCM v3 location that can be upgraded to the latest XCM version.
#[cfg(feature = "runtime-benchmarks")]
pub(crate) fn worst_case_v3_location() -> xcm::v3::Location {
	use xcm::v3::{Junction, Junctions, NetworkId};

	let junction =
		Junction::AccountId32 { network: Some(NetworkId::ByGenesis([1; 32])), id: [2; 32] };
	xcm::v3::Location::new(
		u8::MAX,
		Junctions::X8(
			junction, junction, junction, junction, junction, junction, junction, junction,
		),
	)
}

/// One part of [`MigrateXcmToLatest`], upgrading the entries of a single storage map.
pub trait MigrationStage {
	/// Upgrades the entries following `last_key`, or from the start if it's `None`, as long as
	/// `meter` allows.
	///
	/// Returns the raw key of the last entry upgraded, to continue from in the next call, or
	/// `None` once all entries are done.
	fn step(
		last_key: Option<RawKey>,
		meter: &mut WeightMeter,
	) -> Result<Option<RawKey>, SteppedMigrationError>;

	/// Inserts an entry that needs upgrading and is as costly to upgrade as possible.
	#[cfg(feature = "runtime-benchmarks")]
	fn setup_worst_case_entry();

	/// Returns the state passed to [`Self::post_upgrade`].
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError>;

	/// Checks that every entry is upgraded and that none were lost.
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError>;
}

/// A sequence of [`MigrationStage`]s, run one after the other.
///
/// Implemented for tuples of up to 8 stages.
pub trait MigrationStages {
	/// Runs [`MigrationStage::step`] of the stage at `stage`.
	///
	/// Returns `None` if there is no such stage, i.e. all stages are done.
	fn step(
		stage: u32,
		last_key: Option<RawKey>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<RawKey>, SteppedMigrationError>>;

	/// Runs [`MigrationStage::pre_upgrade`] of every stage.
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<Vec<u8>>, sp_runtime::TryRuntimeError>;

	/// Runs [`MigrationStage::post_upgrade`] of every stage, with the state of
	/// [`Self::pre_upgrade`].
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(states: Vec<Vec<u8>>) -> Result<(), sp_runtime::TryRuntimeError>;
}

#[impl_trait_for_tuples::impl_for_tuples(8)]
#[tuple_types_custom_trait_bound(MigrationStage)]
impl MigrationStages for Tuple {
	#[allow(unused_variables, unused_mut, unused_assignments)]
	fn step(
		stage: u32,
		last_key: Option<RawKey>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<RawKey>, SteppedMigrationError>> {
		let mut index = 0u32;
		for_tuples!( #(
			if index == stage {
				return Some(Tuple::step(last_key, meter));
			}
			index += 1;
		)* );
		None
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<Vec<u8>>, sp_runtime::TryRuntimeError> {
		let mut states = Vec::new();
		for_tuples!( #( states.push(Tuple::pre_upgrade()?); )* );
		Ok(states)
	}

	#[cfg(feature = "try-runtime")]
	#[allow(unused_mut)]
	fn post_upgrade(states: Vec<Vec<u8>>) -> Result<(), sp_runtime::TryRuntimeError> {
		let mut states = states.into_iter();
		for_tuples!( #(
			Tuple::post_upgrade(states.next().ok_or("missing pre-upgrade state")?)?;
		)* );
		Ok(())
	}
}

/// Where [`MigrateXcmToLatest`] continues from.
#[derive(Encode, Decode, MaxEncodedLen, TypeInfo, Clone, PartialEq, Eq, Debug, Default)]
pub struct MigrationCursor {
	/// The index of the stage in progress.
	pub stage: u32,
	/// The raw key of the last entry upgraded by that stage, `None` if it hasn't started yet.
	pub last_key: Option<RawKey>,
}

/// Upgrades the XCM data stored by pallets to the latest XCM version, running `Stages` one after
/// the other.
///
/// See the [module docs](self) for how to use it.
pub struct MigrateXcmToLatest<Stages>(PhantomData<Stages>);

impl<Stages: MigrationStages> SteppedMigration for MigrateXcmToLatest<Stages> {
	type Cursor = MigrationCursor;
	type Identifier = MigrationId<13>;

	fn id() -> Self::Identifier {
		MigrationId { pallet_id: *b"xcm-to-latest", version_from: 0, version_to: 1 }
	}

	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		let MigrationCursor { stage, last_key } = cursor.unwrap_or_default();
		match Stages::step(stage, last_key, meter) {
			None => {
				log::info!(target: LOG_TARGET, "All stages done");
				Ok(None)
			},
			Some(Ok(Some(last_key))) => {
				Ok(Some(MigrationCursor { stage, last_key: Some(last_key) }))
			},
			Some(Ok(None)) => {
				log::info!(target: LOG_TARGET, "Stage {} done", stage);
				Ok(Some(MigrationCursor { stage: stage.saturating_add(1), last_key: None }))
			},
			Some(Err(error)) => Err(error),
		}
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		Stages::pre_upgrade().map(|states| states.encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		let states = Vec::<Vec<u8>>::decode(&mut &state[..])
			.map_err(|_| "cannot decode the pre-upgrade state")?;
		Stages::post_upgrade(states)
	}
}

/// Iterates over the entries of the storage map `Map`, following `last_key` or from the first
/// entry if it's `None`.
///
/// Keys are decoded as `K` and values as `V`: the types the map held before the upgrade. Entries
/// that can't be decoded are logged and skipped.
pub(crate) fn old_entries<Map, MapKey, MapValue, K, V>(
	last_key: Option<RawKey>,
) -> PrefixIterator<(K, V)>
where
	Map: generator::StorageMap<MapKey, MapValue>,
	Map::Hasher: ReversibleStorageHasher,
	MapKey: FullEncode,
	MapValue: FullCodec,
	K: Decode,
	V: Decode,
{
	let mut entries =
		storage_key_iter::<K, V, Map::Hasher>(Map::pallet_prefix(), Map::storage_prefix());
	if let Some(last_key) = last_key {
		entries.set_last_raw_key(last_key.into_inner());
	}
	entries
}

/// Returns the raw key of `key` in the storage map `Map`, for a key of the type the map held
/// before the upgrade.
#[cfg(any(feature = "runtime-benchmarks", test))]
pub(crate) fn old_key<Map, MapKey, MapValue>(key: &impl Encode) -> Vec<u8>
where
	Map: generator::StorageMap<MapKey, MapValue>,
	MapKey: FullEncode,
	MapValue: FullCodec,
{
	use frame_support::StorageHasher;

	let mut raw_key = Map::prefix_hash().to_vec();
	raw_key.extend(Map::Hasher::hash(&key.encode()).as_ref());
	raw_key
}

/// Runs a [`MigrationStage::step`] over the storage map `Map`.
///
/// Calls `upgrade` with the raw key and the [`old_entries`] of every entry following `last_key`,
/// or from the first entry if it's `None`, as long as `meter` allows.
pub(crate) fn step_over_map<Map, MapKey, MapValue, K, V, W>(
	last_key: Option<RawKey>,
	meter: &mut WeightMeter,
	mut upgrade: impl FnMut(&[u8], K, V),
) -> Result<Option<RawKey>, SteppedMigrationError>
where
	Map: generator::StorageMap<MapKey, MapValue>,
	Map::Hasher: ReversibleStorageHasher,
	MapKey: FullEncode,
	MapValue: FullCodec,
	K: Decode,
	V: Decode,
	W: WeightInfo,
{
	let base = W::step_base();
	let item = W::step_item();
	let required = base.saturating_add(item);
	if meter.remaining().any_lt(required) {
		return Err(SteppedMigrationError::InsufficientWeight { required });
	}
	meter.consume(base);

	// `PrefixIterator` reads the key following its last one on every call, so it sees the entries
	// that `upgrade` inserts or removes.
	let mut entries = old_entries::<Map, MapKey, MapValue, K, V>(last_key);
	while meter.try_consume(item).is_ok() {
		let Some((key, value)) = entries.next() else { return Ok(None) };
		upgrade(entries.last_raw_key(), key, value);
	}

	// At least one entry was upgraded, as `meter` was checked to fit one.
	RawKey::try_from(entries.last_raw_key().to_vec()).map(Some).map_err(|_| {
		log::error!(target: LOG_TARGET, "Storage key exceeds MAX_RAW_KEY_LEN");
		SteppedMigrationError::Failed
	})
}

/// Returns the state for [`MigrationStage::post_upgrade`]: the number of entries of `Map` that
/// `upgradable` accepts, decoded with the types the map held before the upgrade.
#[cfg(feature = "try-runtime")]
pub(crate) fn count_upgradable<Map, MapKey, MapValue, K, V>(
	upgradable: impl Fn(&K, &V) -> bool,
) -> Vec<u8>
where
	Map: generator::StorageMap<MapKey, MapValue>,
	Map::Hasher: ReversibleStorageHasher,
	MapKey: FullEncode,
	MapValue: FullCodec,
	K: Decode,
	V: Decode,
{
	let count = old_entries::<Map, MapKey, MapValue, K, V>(None)
		.filter(|(key, value)| upgradable(key, value))
		.count() as u64;
	count.encode()
}

/// Checks that every entry of `Map` can be decoded and is in the latest XCM version.
///
/// Returns the number of upgradable entries before the upgrade (from `state`, the result of
/// [`count_upgradable`]) and the number of entries after.
#[cfg(feature = "try-runtime")]
pub(crate) fn check_upgraded<Map, K, V>(
	state: Vec<u8>,
	name: &str,
	is_latest: impl Fn(&K, &V) -> bool,
) -> Result<(u64, u64), sp_runtime::TryRuntimeError>
where
	Map: frame_support::IterableStorageMap<K, V> + frame_support::storage::StoragePrefixedMap<V>,
	K: FullCodec + Debug,
	V: FullCodec,
{
	let upgradable = u64::decode(&mut &state[..]).map_err(|_| "invalid state")?;
	let raw_count = count_entries::<Map, V>();
	let mut decoded_count = 0u64;
	for (key, value) in Map::iter() {
		decoded_count += 1;
		if !is_latest(&key, &value) {
			log::error!(target: LOG_TARGET, "{name} entry {key:?} still needs upgrading");
			return Err("an entry still needs upgrading".into());
		}
	}
	if raw_count != decoded_count {
		log::error!(target: LOG_TARGET, "{name} holds entries that cannot be decoded");
		return Err("an entry cannot be decoded".into());
	}
	Ok((upgradable, raw_count))
}

/// Counts the raw entries of `Map`, including those whose key or value can't be decoded.
#[cfg(any(feature = "try-runtime", test))]
pub(crate) fn count_entries<Map, V>() -> u64
where
	Map: frame_support::storage::StoragePrefixedMap<V>,
	V: FullCodec,
{
	let prefix = Map::final_prefix();
	frame_support::storage::KeyPrefixIterator::new(prefix.to_vec(), prefix.to_vec(), |_| Ok(()))
		.count() as u64
}
