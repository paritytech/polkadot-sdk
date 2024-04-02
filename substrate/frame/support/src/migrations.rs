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

use crate::{
	defensive,
	storage::transactional::with_transaction_opaque_err,
	traits::{
		Defensive, GetStorageVersion, NoStorageVersionSet, PalletInfoAccess, SafeMode,
		StorageVersion,
	},
	weights::{RuntimeDbWeight, Weight, WeightMeter},
};
use codec::{Decode, Encode, MaxEncodedLen};
use impl_trait_for_tuples::impl_for_tuples;
use sp_arithmetic::traits::Bounded;
use sp_core::Get;
use sp_io::{hashing::twox_128, storage::clear_prefix, KillStorageResult};
use sp_runtime::traits::Zero;
use sp_std::{marker::PhantomData, vec::Vec};

/// Handles storage migration pallet versioning.
///
/// [`VersionedMigration`] allows developers to write migrations without worrying about checking and
/// setting storage versions. Instead, the developer wraps their migration in this struct which
/// takes care of version handling using best practices.
///
/// It takes 5 type parameters:
/// - `From`: The version being upgraded from.
/// - `To`: The version being upgraded to.
/// - `Inner`: An implementation of `UncheckedOnRuntimeUpgrade`.
/// - `Pallet`: The Pallet being upgraded.
/// - `Weight`: The runtime's RuntimeDbWeight implementation.
///
/// When a [`VersionedMigration`] `on_runtime_upgrade`, `pre_upgrade`, or `post_upgrade` method is
/// called, the on-chain version of the pallet is compared to `From`. If they match, the `Inner`
/// `UncheckedOnRuntimeUpgrade` is called and the pallets on-chain version is set to `To`
/// after the migration. Otherwise, a warning is logged notifying the developer that the upgrade was
/// a noop and should probably be removed.
///
/// By not bounding `Inner` with `OnRuntimeUpgrade`, we prevent developers from
/// accidentally using the unchecked version of the migration in a runtime upgrade instead of
/// [`VersionedMigration`].
///
/// ### Examples
/// ```ignore
/// // In file defining migrations
///
/// /// Private module containing *version unchecked* migration logic.
/// ///
/// /// Should only be used by the [`VersionedMigration`] type in this module to create something to
/// /// export.
/// ///
/// /// We keep this private so the unversioned migration cannot accidentally be used in any runtimes.
/// ///
/// /// For more about this pattern of keeping items private, see
/// /// - https://github.com/rust-lang/rust/issues/30905
/// /// - https://internals.rust-lang.org/t/lang-team-minutes-private-in-public-rules/4504/40
/// mod version_unchecked {
/// 	use super::*;
/// 	pub struct VersionUncheckedMigrateV5ToV6<T>(sp_std::marker::PhantomData<T>);
/// 	impl<T: Config> UncheckedOnRuntimeUpgrade for VersionUncheckedMigrateV5ToV6<T> {
/// 		// `UncheckedOnRuntimeUpgrade` implementation...
/// 	}
/// }
///
/// pub type MigrateV5ToV6<T, I> =
/// 	VersionedMigration<
/// 		5,
/// 		6,
/// 		VersionUncheckedMigrateV5ToV6<T, I>,
/// 		crate::pallet::Pallet<T, I>,
/// 		<T as frame_system::Config>::DbWeight
/// 	>;
///
/// // Migrations tuple to pass to the Executive pallet:
/// pub type Migrations = (
/// 	// other migrations...
/// 	MigrateV5ToV6<T, ()>,
/// 	// other migrations...
/// );
/// ```
pub struct VersionedMigration<const FROM: u16, const TO: u16, Inner, Pallet, Weight> {
	_marker: PhantomData<(Inner, Pallet, Weight)>,
}

/// A helper enum to wrap the pre_upgrade bytes like an Option before passing them to post_upgrade.
/// This enum is used rather than an Option to make the API clearer to the developer.
#[derive(Encode, Decode)]
pub enum VersionedPostUpgradeData {
	/// The migration ran, inner vec contains pre_upgrade data.
	MigrationExecuted(sp_std::vec::Vec<u8>),
	/// This migration is a noop, do not run post_upgrade checks.
	Noop,
}

/// Implementation of the `OnRuntimeUpgrade` trait for `VersionedMigration`.
///
/// Its main function is to perform the runtime upgrade in `on_runtime_upgrade` only if the on-chain
/// version of the pallets storage matches `From`, and after the upgrade set the on-chain storage to
/// `To`. If the versions do not match, it writes a log notifying the developer that the migration
/// is a noop.
impl<
		const FROM: u16,
		const TO: u16,
		Inner: crate::traits::UncheckedOnRuntimeUpgrade,
		Pallet: GetStorageVersion<InCodeStorageVersion = StorageVersion> + PalletInfoAccess,
		DbWeight: Get<RuntimeDbWeight>,
	> crate::traits::OnRuntimeUpgrade for VersionedMigration<FROM, TO, Inner, Pallet, DbWeight>
{
	/// Executes pre_upgrade if the migration will run, and wraps the pre_upgrade bytes in
	/// [`VersionedPostUpgradeData`] before passing them to post_upgrade, so it knows whether the
	/// migration ran or not.
	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		let on_chain_version = Pallet::on_chain_storage_version();
		if on_chain_version == FROM {
			Ok(VersionedPostUpgradeData::MigrationExecuted(Inner::pre_upgrade()?).encode())
		} else {
			Ok(VersionedPostUpgradeData::Noop.encode())
		}
	}

	/// Executes the versioned runtime upgrade.
	///
	/// First checks if the pallets on-chain storage version matches the version of this upgrade. If
	/// it matches, it calls `Inner::on_runtime_upgrade`, updates the on-chain version, and returns
	/// the weight. If it does not match, it writes a log notifying the developer that the migration
	/// is a noop.
	fn on_runtime_upgrade() -> Weight {
		let on_chain_version = Pallet::on_chain_storage_version();
		if on_chain_version == FROM {
			log::info!(
				"🚚 Pallet {:?} VersionedMigration migrating storage version from {:?} to {:?}.",
				Pallet::name(),
				FROM,
				TO
			);

			// Execute the migration
			let weight = Inner::on_runtime_upgrade();

			// Update the on-chain version
			StorageVersion::new(TO).put::<Pallet>();

			weight.saturating_add(DbWeight::get().reads_writes(1, 1))
		} else {
			log::warn!(
				"🚚 Pallet {:?} VersionedMigration migration {}->{} can be removed; on-chain is already at {:?}.",
				Pallet::name(),
				FROM,
				TO,
				on_chain_version
			);
			DbWeight::get().reads(1)
		}
	}

	/// Executes `Inner::post_upgrade` if the migration just ran.
	///
	/// pre_upgrade passes [`VersionedPostUpgradeData::MigrationExecuted`] to post_upgrade if
	/// the migration ran, and [`VersionedPostUpgradeData::Noop`] otherwise.
	#[cfg(feature = "try-runtime")]
	fn post_upgrade(
		versioned_post_upgrade_data_bytes: sp_std::vec::Vec<u8>,
	) -> Result<(), sp_runtime::TryRuntimeError> {
		use codec::DecodeAll;
		match <VersionedPostUpgradeData>::decode_all(&mut &versioned_post_upgrade_data_bytes[..])
			.map_err(|_| "VersionedMigration post_upgrade failed to decode PreUpgradeData")?
		{
			VersionedPostUpgradeData::MigrationExecuted(inner_bytes) =>
				Inner::post_upgrade(inner_bytes),
			VersionedPostUpgradeData::Noop => Ok(()),
		}
	}
}

/// Can store the in-code pallet version on-chain.
pub trait StoreInCodeStorageVersion<T: GetStorageVersion + PalletInfoAccess> {
	/// Write the in-code storage version on-chain.
	fn store_in_code_storage_version();
}

impl<T: GetStorageVersion<InCodeStorageVersion = StorageVersion> + PalletInfoAccess>
	StoreInCodeStorageVersion<T> for StorageVersion
{
	fn store_in_code_storage_version() {
		let version = <T as GetStorageVersion>::in_code_storage_version();
		version.put::<T>();
	}
}

impl<T: GetStorageVersion<InCodeStorageVersion = NoStorageVersionSet> + PalletInfoAccess>
	StoreInCodeStorageVersion<T> for NoStorageVersionSet
{
	fn store_in_code_storage_version() {
		StorageVersion::default().put::<T>();
	}
}

/// Trait used by [`migrate_from_pallet_version_to_storage_version`] to do the actual migration.
pub trait PalletVersionToStorageVersionHelper {
	fn migrate(db_weight: &RuntimeDbWeight) -> Weight;
}

impl<T: GetStorageVersion + PalletInfoAccess> PalletVersionToStorageVersionHelper for T
where
	T::InCodeStorageVersion: StoreInCodeStorageVersion<T>,
{
	fn migrate(db_weight: &RuntimeDbWeight) -> Weight {
		const PALLET_VERSION_STORAGE_KEY_POSTFIX: &[u8] = b":__PALLET_VERSION__:";

		fn pallet_version_key(name: &str) -> [u8; 32] {
			crate::storage::storage_prefix(name.as_bytes(), PALLET_VERSION_STORAGE_KEY_POSTFIX)
		}

		sp_io::storage::clear(&pallet_version_key(<T as PalletInfoAccess>::name()));

		<T::InCodeStorageVersion as StoreInCodeStorageVersion<T>>::store_in_code_storage_version();

		db_weight.writes(2)
	}
}

#[cfg_attr(all(not(feature = "tuples-96"), not(feature = "tuples-128")), impl_for_tuples(64))]
#[cfg_attr(all(feature = "tuples-96", not(feature = "tuples-128")), impl_for_tuples(96))]
#[cfg_attr(feature = "tuples-128", impl_for_tuples(128))]
impl PalletVersionToStorageVersionHelper for T {
	fn migrate(db_weight: &RuntimeDbWeight) -> Weight {
		let mut weight = Weight::zero();

		for_tuples!( #( weight = weight.saturating_add(T::migrate(db_weight)); )* );

		weight
	}
}

/// Migrate from the `PalletVersion` struct to the new [`StorageVersion`] struct.
///
/// This will remove all `PalletVersion's` from the state and insert the in-code storage version.
pub fn migrate_from_pallet_version_to_storage_version<
	Pallets: PalletVersionToStorageVersionHelper,
>(
	db_weight: &RuntimeDbWeight,
) -> Weight {
	Pallets::migrate(db_weight)
}

/// `RemovePallet` is a utility struct used to remove all storage items associated with a specific
/// pallet.
///
/// This struct is generic over two parameters:
/// - `P` is a type that implements the `Get` trait for a static string, representing the pallet's
///   name.
/// - `DbWeight` is a type that implements the `Get` trait for `RuntimeDbWeight`, providing the
///   weight for database operations.
///
/// On runtime upgrade, the `on_runtime_upgrade` function will clear all storage items associated
/// with the specified pallet, logging the number of keys removed. If the `try-runtime` feature is
/// enabled, the `pre_upgrade` and `post_upgrade` functions can be used to verify the storage
/// removal before and after the upgrade.
///
/// # Examples:
/// ```ignore
/// construct_runtime! {
/// 	pub enum Runtime
/// 	{
/// 		System: frame_system = 0,
///
/// 		SomePalletToRemove: pallet_something = 1,
/// 		AnotherPalletToRemove: pallet_something_else = 2,
///
/// 		YourOtherPallets...
/// 	}
/// };
///
/// parameter_types! {
/// 		pub const SomePalletToRemoveStr: &'static str = "SomePalletToRemove";
/// 		pub const AnotherPalletToRemoveStr: &'static str = "AnotherPalletToRemove";
/// }
///
/// pub type Migrations = (
/// 	RemovePallet<SomePalletToRemoveStr, RocksDbWeight>,
/// 	RemovePallet<AnotherPalletToRemoveStr, RocksDbWeight>,
/// 	AnyOtherMigrations...
/// );
///
/// pub type Executive = frame_executive::Executive<
/// 	Runtime,
/// 	Block,
/// 	frame_system::ChainContext<Runtime>,
/// 	Runtime,
/// 	Migrations
/// >;
/// ```
///
/// WARNING: `RemovePallet` has no guard rails preventing it from bricking the chain if the
/// operation of removing storage for the given pallet would exceed the block weight limit.
///
/// If your pallet has too many keys to be removed in a single block, it is advised to wait for
/// a multi-block scheduler currently under development which will allow for removal of storage
/// items (and performing other heavy migrations) over multiple blocks
/// (see <https://github.com/paritytech/substrate/issues/13690>).
pub struct RemovePallet<P: Get<&'static str>, DbWeight: Get<RuntimeDbWeight>>(
	PhantomData<(P, DbWeight)>,
);
impl<P: Get<&'static str>, DbWeight: Get<RuntimeDbWeight>> frame_support::traits::OnRuntimeUpgrade
	for RemovePallet<P, DbWeight>
{
	fn on_runtime_upgrade() -> frame_support::weights::Weight {
		let hashed_prefix = twox_128(P::get().as_bytes());
		let keys_removed = match clear_prefix(&hashed_prefix, None) {
			KillStorageResult::AllRemoved(value) => value,
			KillStorageResult::SomeRemaining(value) => {
				log::error!(
					"`clear_prefix` failed to remove all keys for {}. THIS SHOULD NEVER HAPPEN! 🚨",
					P::get()
				);
				value
			},
		} as u64;

		log::info!("Removed {} {} keys 🧹", keys_removed, P::get());

		DbWeight::get().reads_writes(keys_removed + 1, keys_removed)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<sp_std::vec::Vec<u8>, sp_runtime::TryRuntimeError> {
		use crate::storage::unhashed::contains_prefixed_key;

		let hashed_prefix = twox_128(P::get().as_bytes());
		match contains_prefixed_key(&hashed_prefix) {
			true => log::info!("Found {} keys pre-removal 👀", P::get()),
			false => log::warn!(
				"Migration RemovePallet<{}> can be removed (no keys found pre-removal).",
				P::get()
			),
		};
		Ok(sp_std::vec::Vec::new())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(_state: sp_std::vec::Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use crate::storage::unhashed::contains_prefixed_key;

		let hashed_prefix = twox_128(P::get().as_bytes());
		match contains_prefixed_key(&hashed_prefix) {
			true => {
				log::error!("{} has keys remaining post-removal ❗", P::get());
				return Err("Keys remaining post-removal, this should never happen 🚨".into())
			},
			false => log::info!("No {} keys found post-removal 🎉", P::get()),
		};
		Ok(())
	}
}

/// A migration that can proceed in multiple steps.
pub trait SteppedMigration {
	/// The cursor type that stores the progress (aka. state) of this migration.
	type Cursor: codec::FullCodec + codec::MaxEncodedLen;

	/// The unique identifier type of this migration.
	type Identifier: codec::FullCodec + codec::MaxEncodedLen;

	/// The unique identifier of this migration.
	///
	/// If two migrations have the same identifier, then they are assumed to be identical.
	fn id() -> Self::Identifier;

	/// The maximum number of steps that this migration can take.
	///
	/// This can be used to enforce progress and prevent migrations becoming stuck forever. A
	/// migration that exceeds its max steps is treated as failed. `None` means that there is no
	/// limit.
	fn max_steps() -> Option<u32> {
		None
	}

	/// Try to migrate as much as possible with the given weight.
	///
	/// **ANY STORAGE CHANGES MUST BE ROLLED-BACK BY THE CALLER UPON ERROR.** This is necessary
	/// since the caller cannot return a cursor in the error case. [`Self::transactional_step`] is
	/// provided as convenience for a caller. A cursor of `None` implies that the migration is at
	/// its end. A migration that once returned `Nonce` is guaranteed to never be called again.
	fn step(
		cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError>;

	/// Same as [`Self::step`], but rolls back pending changes in the error case.
	fn transactional_step(
		mut cursor: Option<Self::Cursor>,
		meter: &mut WeightMeter,
	) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
		with_transaction_opaque_err(move || match Self::step(cursor, meter) {
			Ok(new_cursor) => {
				cursor = new_cursor;
				sp_runtime::TransactionOutcome::Commit(Ok(cursor))
			},
			Err(err) => sp_runtime::TransactionOutcome::Rollback(Err(err)),
		})
		.map_err(|()| SteppedMigrationError::Failed)?
	}
}

/// Error that can occur during a [`SteppedMigration`].
#[derive(Debug, Encode, Decode, MaxEncodedLen, scale_info::TypeInfo)]
pub enum SteppedMigrationError {
	// Transient errors:
	/// The remaining weight is not enough to do anything.
	///
	/// Can be resolved by calling with at least `required` weight. Note that calling it with
	/// exactly `required` weight could cause it to not make any progress.
	InsufficientWeight {
		/// Amount of weight required to make progress.
		required: Weight,
	},
	// Permanent errors:
	/// The migration cannot decode its cursor and therefore not proceed.
	///
	/// This should not happen unless (1) the migration itself returned an invalid cursor in a
	/// previous iteration, (2) the storage got corrupted or (3) there is a bug in the caller's
	/// code.
	InvalidCursor,
	/// The migration encountered a permanent error and cannot continue.
	Failed,
}

/// Notification handler for status updates regarding Multi-Block-Migrations.
#[impl_trait_for_tuples::impl_for_tuples(8)]
pub trait MigrationStatusHandler {
	/// Notifies of the start of a runtime migration.
	fn started() {}

	/// Notifies of the completion of a runtime migration.
	fn completed() {}
}

/// Handles a failed runtime migration.
///
/// This should never happen, but is here for completeness.
pub trait FailedMigrationHandler {
	/// Infallibly handle a failed runtime migration.
	///
	/// Gets passed in the optional index of the migration in the batch that caused the failure.
	/// Returning `None` means that no automatic handling should take place and the callee decides
	/// in the implementation what to do.
	fn failed(migration: Option<u32>) -> FailedMigrationHandling;
}

/// Do now allow any transactions to be processed after a runtime upgrade failed.
///
/// This is **not a sane default**, since it prevents governance intervention.
pub struct FreezeChainOnFailedMigration;

impl FailedMigrationHandler for FreezeChainOnFailedMigration {
	fn failed(_migration: Option<u32>) -> FailedMigrationHandling {
		FailedMigrationHandling::KeepStuck
	}
}

/// Enter safe mode on a failed runtime upgrade.
///
/// This can be very useful to manually intervene and fix the chain state. `Else` is used in case
/// that the safe mode could not be entered.
pub struct EnterSafeModeOnFailedMigration<SM, Else: FailedMigrationHandler>(
	PhantomData<(SM, Else)>,
);

impl<Else: FailedMigrationHandler, SM: SafeMode> FailedMigrationHandler
	for EnterSafeModeOnFailedMigration<SM, Else>
where
	<SM as SafeMode>::BlockNumber: Bounded,
{
	fn failed(migration: Option<u32>) -> FailedMigrationHandling {
		let entered = if SM::is_entered() {
			SM::extend(Bounded::max_value())
		} else {
			SM::enter(Bounded::max_value())
		};

		// If we could not enter or extend safe mode (for whatever reason), then we try the next.
		if entered.is_err() {
			Else::failed(migration)
		} else {
			FailedMigrationHandling::KeepStuck
		}
	}
}

/// How to proceed after a runtime upgrade failed.
///
/// There is NO SANE DEFAULT HERE. All options are very dangerous and should be used with care.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FailedMigrationHandling {
	/// Resume extrinsic processing of the chain. This will not resume the upgrade.
	///
	/// This should be supplemented with additional measures to ensure that the broken chain state
	/// does not get further messed up by user extrinsics.
	ForceUnstuck,
	/// Set the cursor to `Stuck` and keep blocking extrinsics.
	KeepStuck,
	/// Don't do anything with the cursor and let the handler decide.
	///
	/// This can be useful in cases where the other two options would overwrite any changes that
	/// were done by the handler to the cursor.
	Ignore,
}

/// Something that can do multi step migrations.
pub trait MultiStepMigrator {
	/// Hint for whether [`Self::step`] should be called.
	fn ongoing() -> bool;

	/// Do the next step in the MBM process.
	///
	/// Must gracefully handle the case that it is currently not upgrading.
	fn step() -> Weight;
}

impl MultiStepMigrator for () {
	fn ongoing() -> bool {
		false
	}

	fn step() -> Weight {
		Weight::zero()
	}
}

/// Multiple [`SteppedMigration`].
pub trait SteppedMigrations {
	/// The number of migrations that `Self` aggregates.
	fn len() -> u32;

	/// The `n`th [`SteppedMigration::id`].
	///
	/// Is guaranteed to return `Some` if `n < Self::len()`.
	fn nth_id(n: u32) -> Option<Vec<u8>>;

	/// The [`SteppedMigration::max_steps`] of the `n`th migration.
	///
	/// Is guaranteed to return `Some` if `n < Self::len()`.
	fn nth_max_steps(n: u32) -> Option<Option<u32>>;

	/// Do a [`SteppedMigration::step`] on the `n`th migration.
	///
	/// Is guaranteed to return `Some` if `n < Self::len()`.
	fn nth_step(
		n: u32,
		cursor: Option<Vec<u8>>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>>;

	/// Do a [`SteppedMigration::transactional_step`] on the `n`th migration.
	///
	/// Is guaranteed to return `Some` if `n < Self::len()`.
	fn nth_transactional_step(
		n: u32,
		cursor: Option<Vec<u8>>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>>;

	/// The maximal encoded length across all cursors.
	fn cursor_max_encoded_len() -> usize;

	/// The maximal encoded length across all identifiers.
	fn identifier_max_encoded_len() -> usize;

	/// Assert the integrity of the migrations.
	///
	/// Should be executed as part of a test prior to runtime usage. May or may not need
	/// externalities.
	#[cfg(feature = "std")]
	fn integrity_test() -> Result<(), &'static str> {
		use crate::ensure;
		let l = Self::len();

		for n in 0..l {
			ensure!(Self::nth_id(n).is_some(), "id is None");
			ensure!(Self::nth_max_steps(n).is_some(), "steps is None");

			// The cursor that we use does not matter. Hence use empty.
			ensure!(
				Self::nth_step(n, Some(vec![]), &mut WeightMeter::new()).is_some(),
				"steps is None"
			);
			ensure!(
				Self::nth_transactional_step(n, Some(vec![]), &mut WeightMeter::new()).is_some(),
				"steps is None"
			);
		}

		Ok(())
	}
}

impl SteppedMigrations for () {
	fn len() -> u32 {
		0
	}

	fn nth_id(_n: u32) -> Option<Vec<u8>> {
		None
	}

	fn nth_max_steps(_n: u32) -> Option<Option<u32>> {
		None
	}

	fn nth_step(
		_n: u32,
		_cursor: Option<Vec<u8>>,
		_meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		None
	}

	fn nth_transactional_step(
		_n: u32,
		_cursor: Option<Vec<u8>>,
		_meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		None
	}

	fn cursor_max_encoded_len() -> usize {
		0
	}

	fn identifier_max_encoded_len() -> usize {
		0
	}
}

// A collection consisting of only a single migration.
impl<T: SteppedMigration> SteppedMigrations for T {
	fn len() -> u32 {
		1
	}

	fn nth_id(_n: u32) -> Option<Vec<u8>> {
		Some(T::id().encode())
	}

	fn nth_max_steps(n: u32) -> Option<Option<u32>> {
		// It should be generally fine to call with n>0, but the code should not attempt to.
		n.is_zero()
			.then_some(T::max_steps())
			.defensive_proof("nth_max_steps should only be called with n==0")
	}

	fn nth_step(
		_n: u32,
		cursor: Option<Vec<u8>>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		if !_n.is_zero() {
			defensive!("nth_step should only be called with n==0");
			return None
		}

		let cursor = match cursor {
			Some(cursor) => match T::Cursor::decode(&mut &cursor[..]) {
				Ok(cursor) => Some(cursor),
				Err(_) => return Some(Err(SteppedMigrationError::InvalidCursor)),
			},
			None => None,
		};

		Some(T::step(cursor, meter).map(|cursor| cursor.map(|cursor| cursor.encode())))
	}

	fn nth_transactional_step(
		n: u32,
		cursor: Option<Vec<u8>>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		if n != 0 {
			defensive!("nth_transactional_step should only be called with n==0");
			return None
		}

		let cursor = match cursor {
			Some(cursor) => match T::Cursor::decode(&mut &cursor[..]) {
				Ok(cursor) => Some(cursor),
				Err(_) => return Some(Err(SteppedMigrationError::InvalidCursor)),
			},
			None => None,
		};

		Some(
			T::transactional_step(cursor, meter).map(|cursor| cursor.map(|cursor| cursor.encode())),
		)
	}

	fn cursor_max_encoded_len() -> usize {
		T::Cursor::max_encoded_len()
	}

	fn identifier_max_encoded_len() -> usize {
		T::Identifier::max_encoded_len()
	}
}

#[impl_trait_for_tuples::impl_for_tuples(1, 30)]
impl SteppedMigrations for Tuple {
	fn len() -> u32 {
		for_tuples!( #( Tuple::len() )+* )
	}

	fn nth_id(n: u32) -> Option<Vec<u8>> {
		let mut i = 0;

		for_tuples!( #(
			if (i + Tuple::len()) > n {
				return Tuple::nth_id(n - i)
			}

			i += Tuple::len();
		)* );

		None
	}

	fn nth_step(
		n: u32,
		cursor: Option<Vec<u8>>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		let mut i = 0;

		for_tuples!( #(
			if (i + Tuple::len()) > n {
				return Tuple::nth_step(n - i, cursor, meter)
			}

			i += Tuple::len();
		)* );

		None
	}

	fn nth_transactional_step(
		n: u32,
		cursor: Option<Vec<u8>>,
		meter: &mut WeightMeter,
	) -> Option<Result<Option<Vec<u8>>, SteppedMigrationError>> {
		let mut i = 0;

		for_tuples! ( #(
			if (i + Tuple::len()) > n {
				return Tuple::nth_transactional_step(n - i, cursor, meter)
			}

			i += Tuple::len();
		)* );

		None
	}

	fn nth_max_steps(n: u32) -> Option<Option<u32>> {
		let mut i = 0;

		for_tuples!( #(
			if (i + Tuple::len()) > n {
				return Tuple::nth_max_steps(n - i)
			}

			i += Tuple::len();
		)* );

		None
	}

	fn cursor_max_encoded_len() -> usize {
		let mut max_len = 0;

		for_tuples!( #(
			max_len = max_len.max(Tuple::cursor_max_encoded_len());
		)* );

		max_len
	}

	fn identifier_max_encoded_len() -> usize {
		let mut max_len = 0;

		for_tuples!( #(
			max_len = max_len.max(Tuple::identifier_max_encoded_len());
		)* );

		max_len
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate::{assert_ok, storage::unhashed};

	#[derive(Decode, Encode, MaxEncodedLen, Eq, PartialEq)]
	pub enum Either<L, R> {
		Left(L),
		Right(R),
	}

	pub struct M0;
	impl SteppedMigration for M0 {
		type Cursor = ();
		type Identifier = u8;

		fn id() -> Self::Identifier {
			0
		}

		fn step(
			_cursor: Option<Self::Cursor>,
			_meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			log::info!("M0");
			unhashed::put(&[0], &());
			Ok(None)
		}
	}

	pub struct M1;
	impl SteppedMigration for M1 {
		type Cursor = ();
		type Identifier = u8;

		fn id() -> Self::Identifier {
			1
		}

		fn step(
			_cursor: Option<Self::Cursor>,
			_meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			log::info!("M1");
			unhashed::put(&[1], &());
			Ok(None)
		}

		fn max_steps() -> Option<u32> {
			Some(1)
		}
	}

	pub struct M2;
	impl SteppedMigration for M2 {
		type Cursor = ();
		type Identifier = u8;

		fn id() -> Self::Identifier {
			2
		}

		fn step(
			_cursor: Option<Self::Cursor>,
			_meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			log::info!("M2");
			unhashed::put(&[2], &());
			Ok(None)
		}

		fn max_steps() -> Option<u32> {
			Some(2)
		}
	}

	pub struct F0;
	impl SteppedMigration for F0 {
		type Cursor = ();
		type Identifier = u8;

		fn id() -> Self::Identifier {
			3
		}

		fn step(
			_cursor: Option<Self::Cursor>,
			_meter: &mut WeightMeter,
		) -> Result<Option<Self::Cursor>, SteppedMigrationError> {
			log::info!("F0");
			unhashed::put(&[3], &());
			Err(SteppedMigrationError::Failed)
		}
	}

	// Three migrations combined to execute in order:
	type Triple = (M0, (M1, M2));
	// Six migrations, just concatenating the ones from before:
	type Hextuple = (Triple, Triple);

	#[test]
	fn singular_migrations_work() {
		assert_eq!(M0::max_steps(), None);
		assert_eq!(M1::max_steps(), Some(1));
		assert_eq!(M2::max_steps(), Some(2));

		assert_eq!(<(M0, M1)>::nth_max_steps(0), Some(None));
		assert_eq!(<(M0, M1)>::nth_max_steps(1), Some(Some(1)));
		assert_eq!(<(M0, M1, M2)>::nth_max_steps(2), Some(Some(2)));

		assert_eq!(<(M0, M1)>::nth_max_steps(2), None);
	}

	#[test]
	fn tuple_migrations_work() {
		assert_eq!(<() as SteppedMigrations>::len(), 0);
		assert_eq!(<((), ((), ())) as SteppedMigrations>::len(), 0);
		assert_eq!(<Triple as SteppedMigrations>::len(), 3);
		assert_eq!(<Hextuple as SteppedMigrations>::len(), 6);

		// Check the IDs. The index specific functions all return an Option,
		// to account for the out-of-range case.
		assert_eq!(<Triple as SteppedMigrations>::nth_id(0), Some(0u8.encode()));
		assert_eq!(<Triple as SteppedMigrations>::nth_id(1), Some(1u8.encode()));
		assert_eq!(<Triple as SteppedMigrations>::nth_id(2), Some(2u8.encode()));

		sp_io::TestExternalities::default().execute_with(|| {
			for n in 0..3 {
				<Triple as SteppedMigrations>::nth_step(
					n,
					Default::default(),
					&mut WeightMeter::new(),
				);
			}
		});
	}

	#[test]
	fn integrity_test_works() {
		sp_io::TestExternalities::default().execute_with(|| {
			assert_ok!(<() as SteppedMigrations>::integrity_test());
			assert_ok!(<M0 as SteppedMigrations>::integrity_test());
			assert_ok!(<M1 as SteppedMigrations>::integrity_test());
			assert_ok!(<M2 as SteppedMigrations>::integrity_test());
			assert_ok!(<Triple as SteppedMigrations>::integrity_test());
			assert_ok!(<Hextuple as SteppedMigrations>::integrity_test());
		});
	}

	#[test]
	fn transactional_rollback_works() {
		sp_io::TestExternalities::default().execute_with(|| {
			assert_ok!(<(M0, F0) as SteppedMigrations>::nth_transactional_step(
				0,
				Default::default(),
				&mut WeightMeter::new()
			)
			.unwrap());
			assert!(unhashed::exists(&[0]));

			let _g = crate::StorageNoopGuard::new();
			assert!(<(M0, F0) as SteppedMigrations>::nth_transactional_step(
				1,
				Default::default(),
				&mut WeightMeter::new()
			)
			.unwrap()
			.is_err());
			assert!(<(F0, M1) as SteppedMigrations>::nth_transactional_step(
				0,
				Default::default(),
				&mut WeightMeter::new()
			)
			.unwrap()
			.is_err());
		});
	}
}
