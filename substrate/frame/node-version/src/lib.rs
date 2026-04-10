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

//! # Node Version Pallet
//!
//! Tracks which version of the node software each validator is running.
//! Version information is submitted via an inherent by the block author.
//!
//! ## Upgrade Order
//!
//! Nodes can be upgraded before the runtime. When the node provides the `ndvrsn00`
//! inherent data but the runtime does not yet include this pallet, the data is silently
//! ignored (see [`sp_node_version`] for details). Once the runtime is upgraded to
//! include this pallet, the inherent will be automatically created and processed.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

use frame_support::traits::Contains;
use frame_system::pallet_prelude::BlockNumberFor;
use sp_core::H256;
use sp_node_version::{InherentError, InherentType, INHERENT_IDENTIFIER};

pub use pallet::*;

#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_support::pallet_prelude::*;
	use frame_system::pallet_prelude::*;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// Checks if an account is an active validator.
		type Authorities: Contains<Self::AccountId>;

		/// Origin that can set the latest expected version hash (governance/root).
		type SetLatestVersionOrigin: EnsureOrigin<Self::RuntimeOrigin>;

		type WeightInfo: WeightInfo;
	}

	pub trait WeightInfo {
		fn report_version() -> Weight;
		fn remove_stale_entry() -> Weight;
		fn set_latest_version() -> Weight;
	}

	impl WeightInfo for () {
		fn report_version() -> Weight {
			Weight::from_parts(10_000, 0)
		}
		fn remove_stale_entry() -> Weight {
			Weight::from_parts(10_000, 0)
		}
		fn set_latest_version() -> Weight {
			Weight::from_parts(10_000, 0)
		}
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	/// The current block author, set by [`EventHandler`] and cleared in `on_finalize`.
	#[pallet::storage]
	pub(crate) type CurrentAuthor<T: Config> = StorageValue<_, T::AccountId, OptionQuery>;

	/// Per-validator version hash and the block number when it was last reported.
	#[pallet::storage]
	pub type ValidatorVersions<T: Config> =
		StorageMap<_, Twox64Concat, T::AccountId, (H256, BlockNumberFor<T>), OptionQuery>;

	/// The latest expected version hash, set by governance.
	/// Contains the hash and the block number when it was set.
	#[pallet::storage]
	pub type LatestVersion<T: Config> = StorageValue<_, (H256, BlockNumberFor<T>), OptionQuery>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A validator reported their node version.
		VersionReported { validator: T::AccountId, version_hash: H256 },
		/// A stale validator entry was removed.
		StaleEntryRemoved { validator: T::AccountId },
		/// The latest expected version was updated by governance.
		LatestVersionSet { version_hash: H256 },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The validator is still in the active set; cannot remove.
		NotStale,
		/// No current block author available.
		NoAuthor,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn on_finalize(_n: BlockNumberFor<T>) {
			<CurrentAuthor<T>>::kill();
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Report the block author's node version. Only callable as an inherent.
		#[pallet::call_index(0)]
		#[pallet::weight((T::WeightInfo::report_version(), DispatchClass::Mandatory))]
		pub fn report_version(
			origin: OriginFor<T>,
			version_hash: H256,
		) -> DispatchResult {
			ensure_none(origin)?;
			let author =
				<CurrentAuthor<T>>::get().ok_or(Error::<T>::NoAuthor)?;
			let block_number = <frame_system::Pallet<T>>::block_number();
			<ValidatorVersions<T>>::insert(&author, (version_hash, block_number));
			Self::deposit_event(Event::VersionReported { validator: author, version_hash });
			Ok(())
		}

		/// Remove a stale entry for a validator no longer in the active set.
		/// Permissionless: anyone can call this.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::remove_stale_entry())]
		pub fn remove_stale_entry(
			origin: OriginFor<T>,
			stale_validator: T::AccountId,
		) -> DispatchResult {
			let _ = ensure_signed(origin)?;
			ensure!(!T::Authorities::contains(&stale_validator), Error::<T>::NotStale);
			<ValidatorVersions<T>>::remove(&stale_validator);
			Self::deposit_event(Event::StaleEntryRemoved { validator: stale_validator });
			Ok(())
		}

		/// Set the latest expected version hash. Only callable by governance.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::set_latest_version())]
		pub fn set_latest_version(
			origin: OriginFor<T>,
			version_hash: H256,
		) -> DispatchResult {
			T::SetLatestVersionOrigin::ensure_origin(origin)?;
			let block_number = <frame_system::Pallet<T>>::block_number();
			<LatestVersion<T>>::put((version_hash, block_number));
			Self::deposit_event(Event::LatestVersionSet { version_hash });
			Ok(())
		}
	}

	#[pallet::inherent]
	impl<T: Config> ProvideInherent for Pallet<T> {
		type Call = Call<T>;
		type Error = InherentError;
		const INHERENT_IDENTIFIER: InherentIdentifier = INHERENT_IDENTIFIER;

		fn create_inherent(data: &InherentData) -> Option<Self::Call> {
			let version_hash = data
				.get_data::<InherentType>(&INHERENT_IDENTIFIER)
				.ok()
				.flatten()?;

			let author = <CurrentAuthor<T>>::get()?;

			// Only create the inherent if the stored version differs.
			if let Some((stored_hash, _)) = <ValidatorVersions<T>>::get(&author) {
				if stored_hash == version_hash {
					return None;
				}
			}

			Some(Call::report_version { version_hash })
		}

		fn check_inherent(call: &Self::Call, data: &InherentData) -> Result<(), Self::Error> {
			if let Call::report_version { version_hash } = call {
				let expected = data
					.get_data::<InherentType>(&INHERENT_IDENTIFIER)
					.ok()
					.flatten()
					.ok_or(InherentError::VersionMismatch)?;
				if *version_hash != expected {
					return Err(InherentError::VersionMismatch);
				}
			}
			Ok(())
		}

		fn is_inherent(call: &Self::Call) -> bool {
			matches!(call, Call::report_version { .. })
		}
	}
}

impl<T: Config> pallet_authorship::EventHandler<T::AccountId, BlockNumberFor<T>> for Pallet<T> {
	fn note_author(author: T::AccountId) {
		<CurrentAuthor<T>>::put(author);
	}
}

#[cfg(test)]
mod tests {
	use super::*;
	use crate as pallet_node_version;
	use frame_support::{
		assert_noop, assert_ok, derive_impl,
		inherent::ProvideInherent,
		traits::Hooks,
	};
	use frame_system::EnsureRoot;
	use sp_core::H256;
	use sp_inherents::InherentData;
	use sp_io::TestExternalities;
	use sp_runtime::BuildStorage;

	type Block = frame_system::mocking::MockBlock<Test>;

	frame_support::construct_runtime!(
		pub enum Test
		{
			System: frame_system,
			NodeVersion: pallet_node_version,
		}
	);

	#[derive_impl(frame_system::config_preludes::TestDefaultConfig)]
	impl frame_system::Config for Test {
		type Block = Block;
	}

	pub struct MockAuthorities;
	impl Contains<u64> for MockAuthorities {
		fn contains(who: &u64) -> bool {
			// Validators are accounts 1, 2, 3
			*who == 1 || *who == 2 || *who == 3
		}
	}

	impl Config for Test {
		type Authorities = MockAuthorities;
		type SetLatestVersionOrigin = EnsureRoot<u64>;
		type WeightInfo = ();
	}

	fn new_test_ext() -> TestExternalities {
		let t = frame_system::GenesisConfig::<Test>::default().build_storage().unwrap();
		t.into()
	}

	fn version_hash(s: &str) -> H256 {
		sp_core::blake2_256(s.as_bytes()).into()
	}

	#[test]
	fn report_version_works() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			// Simulate authorship EventHandler
			<Pallet<Test> as pallet_authorship::EventHandler<u64, u64>>::note_author(1);

			assert_ok!(NodeVersion::report_version(
				RuntimeOrigin::none(),
				version_hash("1.0.0")
			));

			assert_eq!(
				ValidatorVersions::<Test>::get(1),
				Some((version_hash("1.0.0"), 1))
			);
		});
	}

	#[test]
	fn report_version_fails_without_author() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				NodeVersion::report_version(RuntimeOrigin::none(), version_hash("1.0.0")),
				Error::<Test>::NoAuthor
			);
		});
	}

	#[test]
	fn remove_stale_entry_works() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			// Insert an entry for a non-validator account (account 10)
			ValidatorVersions::<Test>::insert(10u64, (version_hash("1.0.0"), 1u64));

			// Anyone can remove it since 10 is not in the authority set
			assert_ok!(NodeVersion::remove_stale_entry(RuntimeOrigin::signed(99), 10));
			assert_eq!(ValidatorVersions::<Test>::get(10u64), None);
		});
	}

	#[test]
	fn remove_stale_entry_fails_for_active_validator() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			ValidatorVersions::<Test>::insert(1u64, (version_hash("1.0.0"), 1u64));

			assert_noop!(
				NodeVersion::remove_stale_entry(RuntimeOrigin::signed(99), 1),
				Error::<Test>::NotStale
			);
		});
	}

	#[test]
	fn set_latest_version_works() {
		new_test_ext().execute_with(|| {
			System::set_block_number(5);
			assert_ok!(NodeVersion::set_latest_version(
				RuntimeOrigin::root(),
				version_hash("2.0.0")
			));
			assert_eq!(LatestVersion::<Test>::get(), Some((version_hash("2.0.0"), 5)));
		});
	}

	#[test]
	fn set_latest_version_fails_for_non_root() {
		new_test_ext().execute_with(|| {
			assert_noop!(
				NodeVersion::set_latest_version(RuntimeOrigin::signed(1), version_hash("2.0.0")),
				sp_runtime::DispatchError::BadOrigin
			);
		});
	}

	#[test]
	fn create_inherent_returns_none_when_version_unchanged() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let hash = version_hash("1.0.0");

			// Set current author
			<Pallet<Test> as pallet_authorship::EventHandler<u64, u64>>::note_author(1);
			// Pre-populate with same version
			ValidatorVersions::<Test>::insert(1u64, (hash, 1u64));

			let mut inherent_data = InherentData::new();
			inherent_data.put_data(INHERENT_IDENTIFIER, &hash).unwrap();

			assert!(Pallet::<Test>::create_inherent(&inherent_data).is_none());
		});
	}

	#[test]
	fn create_inherent_returns_call_when_version_changed() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let old_hash = version_hash("1.0.0");
			let new_hash = version_hash("2.0.0");

			<Pallet<Test> as pallet_authorship::EventHandler<u64, u64>>::note_author(1);
			ValidatorVersions::<Test>::insert(1u64, (old_hash, 1u64));

			let mut inherent_data = InherentData::new();
			inherent_data.put_data(INHERENT_IDENTIFIER, &new_hash).unwrap();

			let call = Pallet::<Test>::create_inherent(&inherent_data);
			assert!(call.is_some());
			match call.unwrap() {
				Call::report_version { version_hash } => assert_eq!(version_hash, new_hash),
				_ => panic!("unexpected call"),
			}
		});
	}

	#[test]
	fn create_inherent_returns_call_when_no_prior_entry() {
		new_test_ext().execute_with(|| {
			System::set_block_number(1);
			let hash = version_hash("1.0.0");

			<Pallet<Test> as pallet_authorship::EventHandler<u64, u64>>::note_author(1);

			let mut inherent_data = InherentData::new();
			inherent_data.put_data(INHERENT_IDENTIFIER, &hash).unwrap();

			let call = Pallet::<Test>::create_inherent(&inherent_data);
			assert!(call.is_some());
		});
	}

	#[test]
	fn on_finalize_clears_current_author() {
		new_test_ext().execute_with(|| {
			<Pallet<Test> as pallet_authorship::EventHandler<u64, u64>>::note_author(1);
			assert!(CurrentAuthor::<Test>::get().is_some());

			Pallet::<Test>::on_finalize(1);
			assert!(CurrentAuthor::<Test>::get().is_none());
		});
	}

	#[test]
	fn check_inherent_validates_correctly() {
		new_test_ext().execute_with(|| {
			let hash = version_hash("1.0.0");

			let mut inherent_data = InherentData::new();
			inherent_data.put_data(INHERENT_IDENTIFIER, &hash).unwrap();

			let call = Call::<Test>::report_version { version_hash: hash };
			assert!(Pallet::<Test>::check_inherent(&call, &inherent_data).is_ok());

			let wrong_call =
				Call::<Test>::report_version { version_hash: version_hash("wrong") };
			assert!(Pallet::<Test>::check_inherent(&wrong_call, &inherent_data).is_err());
		});
	}
}
