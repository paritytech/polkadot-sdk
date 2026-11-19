// Copyright (C) Parity Technologies (UK) Ltd.
// This file is part of Substrate.
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

use super::*;
use frame_support::{
	pallet_prelude::*,
	traits::{OnRuntimeUpgrade, PalletInfoAccess},
	weights::Weight,
};

fn migrate_v0_to_v1<T: Config<I>, I: 'static>(accounts: &[T::AccountId]) -> Weight {
	let on_chain_version = Pallet::<T, I>::on_chain_storage_version();

	if on_chain_version == 0 {
		let total = accounts
			.iter()
			.map(|a| Pallet::<T, I>::total_balance(a))
			.fold(T::Balance::zero(), |a, e| a.saturating_add(e));
		Pallet::<T, I>::deactivate(total);

		// Remove the old `StorageVersion` type.
		frame_support::storage::unhashed::kill(&frame_support::storage::storage_prefix(
			Pallet::<T, I>::name().as_bytes(),
			"StorageVersion".as_bytes(),
		));

		// Set storage version to `1`.
		StorageVersion::new(1).put::<Pallet<T, I>>();

		log::info!(target: LOG_TARGET, "Storage to version 1");
		T::DbWeight::get().reads_writes(2 + accounts.len() as u64, 3)
	} else {
		log::info!(
			target: LOG_TARGET,
			"Migration did not execute. This probably should be removed"
		);
		T::DbWeight::get().reads(1)
	}
}

// NOTE: This must be used alongside the account whose balance is expected to be inactive.
// Generally this will be used for the XCM teleport checking account.
pub struct MigrateToTrackInactive<T, A, I = ()>(PhantomData<(T, A, I)>);
impl<T: Config<I>, A: Get<T::AccountId>, I: 'static> OnRuntimeUpgrade
	for MigrateToTrackInactive<T, A, I>
{
	fn on_runtime_upgrade() -> Weight {
		migrate_v0_to_v1::<T, I>(&[A::get()])
	}
}

// NOTE: This must be used alongside the accounts whose balance is expected to be inactive.
// Generally this will be used for the XCM teleport checking accounts.
pub struct MigrateManyToTrackInactive<T, A, I = ()>(PhantomData<(T, A, I)>);
impl<T: Config<I>, A: Get<Vec<T::AccountId>>, I: 'static> OnRuntimeUpgrade
	for MigrateManyToTrackInactive<T, A, I>
{
	fn on_runtime_upgrade() -> Weight {
		migrate_v0_to_v1::<T, I>(&A::get())
	}
}

pub struct ResetInactive<T, I = ()>(PhantomData<(T, I)>);
impl<T: Config<I>, I: 'static> OnRuntimeUpgrade for ResetInactive<T, I> {
	fn on_runtime_upgrade() -> Weight {
		let on_chain_version = Pallet::<T, I>::on_chain_storage_version();

		if on_chain_version == 1 {
			// Remove the old `StorageVersion` type.
			frame_support::storage::unhashed::kill(&frame_support::storage::storage_prefix(
				Pallet::<T, I>::name().as_bytes(),
				"StorageVersion".as_bytes(),
			));

			InactiveIssuance::<T, I>::kill();

			// Set storage version to `0`.
			StorageVersion::new(0).put::<Pallet<T, I>>();

			log::info!(target: LOG_TARGET, "Storage to version 0");
			T::DbWeight::get().reads_writes(1, 3)
		} else {
			log::info!(
				target: LOG_TARGET,
				"Migration did not execute. This probably should be removed"
			);
			T::DbWeight::get().reads(1)
		}
	}
}

/// What [`reback_reserved`] did to an account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RebackOutcome<Balance> {
	/// The reserved balance already backs all holds and named reserves. Nothing changed.
	AlreadyValid,
	/// The shortfall was moved from the free to the reserved balance.
	Repaired(Balance),
	/// The free balance cannot cover the shortfall and keep the existential deposit. Nothing
	/// changed.
	Skipped(Balance),
}

/// Make the reserved balance of `who` back all of its holds and named reserves again.
///
/// Moves [`Pallet::reserved_shortfall`] from the free to the reserved balance, with
/// [`ReservableCurrency::reserve`]. An account gets under-backed when a release takes more of its
/// reserved balance than it should, which moves the difference to its free balance. This moves it
/// back. Nothing is minted. Holds, named reserves, freezes and the total issuance are unchanged.
///
/// All or nothing: if the free balance cannot cover the whole shortfall and keep the existential
/// deposit, `who` is left as it is.
pub fn reback_reserved<T: Config<I>, I: 'static>(who: &T::AccountId) -> RebackOutcome<T::Balance> {
	let shortfall = Pallet::<T, I>::reserved_shortfall(who);
	if shortfall.is_zero() {
		return RebackOutcome::AlreadyValid;
	}
	if Pallet::<T, I>::free_balance(who) < shortfall.saturating_add(T::ExistentialDeposit::get()) {
		return RebackOutcome::Skipped(shortfall);
	}

	// `reserve` only fails before it writes the account, e.g. with `TooManyConsumers`.
	match <Pallet<T, I> as ReservableCurrency<_>>::reserve(who, shortfall) {
		Ok(()) => RebackOutcome::Repaired(shortfall),
		Err(e) => {
			log::error!(target: LOG_TARGET, "Failed to reserve {:?} for {:?}: {:?}", shortfall, who, e);
			RebackOutcome::Skipped(shortfall)
		},
	}
}

/// A one-off migration that applies [`reback_reserved`] to every account in `Accounts`.
///
/// `Accounts` must list the accounts with a non-zero [`Pallet::reserved_shortfall`]. Finding them
/// on chain takes a scan of all holds and named reserves, which does not fit in one block. Listed
/// accounts that are already valid are left as they are, so a second run is a no-op.
///
/// A second run still reads every listed account. To avoid that, `UntilSpecVersion` can name the
/// spec version that the chain runs when this migration is enacted, i.e. the one it replaces. The
/// migration then only runs while [`frame_system::LastRuntimeUpgrade`] is at most that spec
/// version. `frame-executive` updates it after the migrations ran, so it moves past that version
/// once this upgrade is enacted, and a runtime that still lists this migration skips it at the cost
/// of one read. The default, `()`, names no spec version: the migration runs on every upgrade.
///
/// The check is "at most", not "equal", because try-runtime-cli sets the spec version to 0 when it
/// checks multi-block migrations, to force an upgrade in the block it builds. The migration must
/// run there, or the `try_state` of this pallet fails once the new runtime has a higher spec
/// version than the chain.
///
/// The try-runtime checks of this migration only warn about under-backed accounts that it does
/// not repair. The `try_state` of this pallet fails on them.
pub struct RebackReserved<T, Accounts, UntilSpecVersion = (), I = ()>(
	PhantomData<(T, Accounts, UntilSpecVersion, I)>,
);

impl<T: Config<I>, Accounts, UntilSpecVersion: Get<Option<u32>>, I: 'static>
	RebackReserved<T, Accounts, UntilSpecVersion, I>
{
	/// Whether the chain is not past the spec version that `UntilSpecVersion` names, if it names
	/// one. A chain that has no [`frame_system::LastRuntimeUpgrade`] was never upgraded, so it is
	/// not past any.
	fn should_run() -> bool {
		UntilSpecVersion::get().map_or(true, |until| {
			Self::on_chain_spec_version().map_or(true, |on_chain| on_chain <= until)
		})
	}

	fn on_chain_spec_version() -> Option<u32> {
		frame_system::LastRuntimeUpgrade::<T>::get().map(|info| info.spec_version.0)
	}
}

impl<T, Accounts, UntilSpecVersion, I> OnRuntimeUpgrade
	for RebackReserved<T, Accounts, UntilSpecVersion, I>
where
	T: Config<I>,
	Accounts: Get<Vec<T::AccountId>>,
	UntilSpecVersion: Get<Option<u32>>,
	I: 'static,
{
	fn on_runtime_upgrade() -> Weight {
		if !Self::should_run() {
			log::warn!(
				target: LOG_TARGET,
				"RebackReserved only runs until spec version {:?}, but the chain is on {:?}. \
				 Skipped, it can be removed from the runtime.",
				UntilSpecVersion::get(),
				Self::on_chain_spec_version(),
			);
			return T::DbWeight::get().reads(1);
		}

		let accounts = Accounts::get();
		for who in &accounts {
			match reback_reserved::<T, I>(who) {
				// a stale list, or a second run.
				RebackOutcome::AlreadyValid => {
					log::warn!(target: LOG_TARGET, "{:?} is listed, but already valid", who)
				},
				RebackOutcome::Repaired(shortfall) => log::info!(
					target: LOG_TARGET,
					"Moved {:?} from free to reserved for {:?}",
					shortfall,
					who
				),
				RebackOutcome::Skipped(shortfall) => log::warn!(
					target: LOG_TARGET,
					"Skipped {:?}, its free balance cannot cover its shortfall of {:?}",
					who,
					shortfall
				),
			}
		}

		// per account, `Holds`, `Reserves` and the account are read, and the account and an event
		// are written. Repeated reads of the account hit the overlay.
		let n = accounts.len() as u64;
		T::DbWeight::get().reads_writes(3 * n, 2 * n)
	}

	#[cfg(feature = "try-runtime")]
	fn pre_upgrade() -> Result<Vec<u8>, sp_runtime::TryRuntimeError> {
		let listed = Accounts::get();
		for (who, shortfall) in Pallet::<T, I>::under_backed_accounts() {
			if !listed.contains(&who) {
				log::warn!(target: LOG_TARGET, "{:?} is short {:?}, but not listed", who, shortfall);
			}
		}

		// a skipped run changes nothing, so there is nothing to check per account.
		let listed = if Self::should_run() { listed } else { Vec::new() };
		// `on_runtime_upgrade` logs the listed accounts that are already valid.
		let snapshots = listed.into_iter().map(try_runtime::snapshot::<T, I>).collect::<Vec<_>>();
		Ok((snapshots, Pallet::<T, I>::total_issuance()).encode())
	}

	#[cfg(feature = "try-runtime")]
	fn post_upgrade(state: Vec<u8>) -> Result<(), sp_runtime::TryRuntimeError> {
		use try_runtime::{snapshot, AccountSnapshot};
		let (snapshots, issuance): (Vec<AccountSnapshot<T::AccountId, T::Balance>>, T::Balance) =
			Decode::decode(&mut &state[..]).map_err(|_| "Cannot decode the pre-upgrade state")?;

		for before in snapshots {
			let after = snapshot::<T, I>(before.who.clone());
			ensure!(after.records == before.records, "Holds or named reserves changed");
			ensure!(after.data.frozen == before.data.frozen, "Frozen balance changed");
			ensure!(after.providers == before.providers, "Providers changed");
			ensure!(after.sufficients == before.sufficients, "Sufficients changed");

			let repairable = !before.shortfall.is_zero() &&
				before.data.free >=
					before.shortfall.saturating_add(T::ExistentialDeposit::get());
			if !repairable {
				ensure!(after.data == before.data, "Skipped or valid account changed");
				ensure!(after.consumers == before.consumers, "Consumers changed");
				continue;
			}

			ensure!(after.shortfall.is_zero(), "Repairable account is still under-backed");
			ensure!(
				after.data.free == before.data.free.saturating_sub(before.shortfall),
				"Free balance did not drop by the shortfall"
			);
			ensure!(
				after.data.reserved == before.data.reserved.saturating_add(before.shortfall),
				"Reserved balance did not grow by the shortfall"
			);
			// `reserve` upgrades the account first, then adds a consumer reference if the account
			// did not have one for its balance: no reserved and no frozen balance, or not yet on
			// the new logic.
			let gains_consumer = before.data.frozen.is_zero() &&
				(before.data.reserved.is_zero() || !before.data.flags.is_new_logic());
			ensure!(
				after.consumers == before.consumers + u32::from(gains_consumer),
				"Consumers changed more than expected"
			);
		}

		ensure!(Pallet::<T, I>::total_issuance() == issuance, "Total issuance changed");
		for (who, shortfall) in Pallet::<T, I>::under_backed_accounts() {
			log::warn!(target: LOG_TARGET, "{:?} is still short {:?}", who, shortfall);
		}
		Ok(())
	}
}

#[cfg(feature = "try-runtime")]
mod try_runtime {
	use super::*;
	use frame_system::RefCount;

	/// The state of an account that [`RebackReserved`] checks after it ran.
	#[derive(Encode, Decode)]
	pub(super) struct AccountSnapshot<AccountId, Balance> {
		pub who: AccountId,
		pub data: AccountData<Balance>,
		/// `Holds` and `Reserves` of `who`, encoded.
		pub records: Vec<u8>,
		pub shortfall: Balance,
		pub providers: RefCount,
		pub consumers: RefCount,
		pub sufficients: RefCount,
	}

	/// The [`AccountSnapshot`] of `who` in the current state.
	pub(super) fn snapshot<T: Config<I>, I: 'static>(
		who: T::AccountId,
	) -> AccountSnapshot<T::AccountId, T::Balance> {
		AccountSnapshot {
			data: Pallet::<T, I>::account(&who),
			records: (Holds::<T, I>::get(&who), Reserves::<T, I>::get(&who)).encode(),
			shortfall: Pallet::<T, I>::reserved_shortfall(&who),
			providers: frame_system::Pallet::<T>::providers(&who),
			consumers: frame_system::Pallet::<T>::consumers(&who),
			sufficients: frame_system::Pallet::<T>::sufficients(&who),
			who,
		}
	}
}
