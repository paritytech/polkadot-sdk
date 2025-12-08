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

//! Broadcaster pallet for managing parachain data publishing and subscription.
//!
//! This pallet provides a publish-subscribe mechanism for parachains to efficiently share data
//! through the relay chain storage using child tries per publisher.
//!
//! ## Publisher Registration
//!
//! Parachains must register before they can publish data:
//!
//! - System parachains (ID < 2000): Registered via `force_register_publisher` (Root origin)
//!   with custom deposit amounts (typically zero).
//! - Public parachains (ID >= 2000): Registered via `register_publisher` requiring a deposit.
//!
//! The deposit is held using the native fungible traits with the `PublisherDeposit` hold reason.
//!
//! ## Storage Organization
//!
//! Each publisher gets a dedicated child trie identified by `(b"pubsub", ParaId)`. The child
//! trie root is stored on-chain and can be included in storage proofs for subscribers to verify
//! published data.
//!
//! ## Storage Lifecycle
//!
//! Publishers can deregister to reclaim their deposit and remove their data:
//!
//! 1. Call `cleanup_published_data` to remove all published key-value pairs from the child trie
//! 2. Call `deregister_publisher` to release the deposit and complete deregistration
//!
//! Root can force deregistration with `force_deregister_publisher`, which removes all data
//! and releases the deposit in a single call.

use alloc::vec::Vec;
use codec::{Decode, Encode};
use frame_support::{
	pallet_prelude::*,
	storage::{child::ChildInfo, types::CountedStorageMap},
	traits::{
		defensive_prelude::*,
		fungible::{
			hold::{Balanced as FunHoldBalanced, Mutate as FunHoldMutate},
			Inspect as FunInspect, Mutate as FunMutate,
		},
		tokens::Precision::Exact,
		Get,
	},
};
use frame_system::{ensure_root, ensure_signed, pallet_prelude::BlockNumberFor};
use polkadot_primitives::Id as ParaId;
use scale_info::TypeInfo;
use sp_runtime::{traits::Zero, RuntimeDebug};

pub use pallet::*;

mod traits;
pub use traits::Publish;

pub mod weights;
pub use weights::WeightInfo;

#[cfg(test)]
mod tests;

/// Information about a registered publisher.
#[derive(Encode, Decode, Clone, PartialEq, Eq, RuntimeDebug, TypeInfo, MaxEncodedLen)]
pub struct PublisherInfo<AccountId, Balance> {
	/// The account that registered and manages this publisher.
	pub manager: AccountId,
	/// The amount held as deposit for registration.
	pub deposit: Balance,
}


#[frame_support::pallet]
pub mod pallet {
	use super::*;
	use frame_system::pallet_prelude::*;

	const STORAGE_VERSION: StorageVersion = StorageVersion::new(0);

	#[pallet::pallet]
	#[pallet::storage_version(STORAGE_VERSION)]
	pub struct Pallet<T>(_);

	/// Reasons for the pallet placing a hold on funds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// The funds are held as deposit for publisher registration.
		#[codec(index = 0)]
		PublisherDeposit,
	}

	type BalanceOf<T> =
		<<T as Config>::Currency as FunInspect<<T as frame_system::Config>::AccountId>>::Balance;

	#[pallet::config]
	pub trait Config: frame_system::Config {
		/// The overarching event type.
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// Currency mechanism for managing publisher deposits.
		type Currency: FunHoldMutate<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ FunMutate<Self::AccountId>
			+ FunHoldBalanced<Self::AccountId>;

		/// Overarching hold reason.
		type RuntimeHoldReason: From<HoldReason>;

		/// Weight information for extrinsics and operations.
		type WeightInfo: WeightInfo;

		/// Maximum number of items that can be published in a single operation.
		///
		/// Must not exceed `xcm::v5::MaxPublishItems`.
		#[pallet::constant]
		type MaxPublishItems: Get<u32>;

		/// Maximum length of a published key in bytes.
		///
		/// Must not exceed `xcm::v5::MaxPublishKeyLength`.
		#[pallet::constant]
		type MaxKeyLength: Get<u32>;

		/// Maximum length of a published value in bytes.
		///
		/// Must not exceed `xcm::v5::MaxPublishValueLength`.
		#[pallet::constant]
		type MaxValueLength: Get<u32>;

		/// Maximum number of unique keys a publisher can store.
		#[pallet::constant]
		type MaxStoredKeys: Get<u32>;

		/// Maximum number of parachains that can register as publishers.
		#[pallet::constant]
		type MaxPublishers: Get<u32>;

		/// The deposit required for a parachain to register as a publisher.
		///
		/// System parachains may use `force_register_publisher` with a custom deposit amount.
		#[pallet::constant]
		type PublisherDeposit: Get<BalanceOf<Self>>;
	}

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// Data published by a parachain.
		DataPublished { publisher: ParaId, items_count: u32 },
		/// A publisher has been registered.
		PublisherRegistered { para_id: ParaId, manager: T::AccountId },
		/// A publisher has been deregistered.
		PublisherDeregistered { para_id: ParaId },
		/// Published data has been cleaned up.
		DataCleanedUp { para_id: ParaId },
	}

	/// Registered publishers and their deposit information.
	///
	/// Parachains must be registered before they can publish data. The registration includes
	/// information about the managing account and the deposit held for the registration.
	#[pallet::storage]
	pub type RegisteredPublishers<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		PublisherInfo<T::AccountId, BalanceOf<T>>,
		OptionQuery,
	>;

	/// Tracks which parachains have published data.
	///
	/// Maps parachain ID to a boolean indicating whether they have a child trie.
	/// The actual child trie info is derived deterministically from the ParaId.
	#[pallet::storage]
	pub type PublisherExists<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		bool,
		ValueQuery,
	>;

	/// Tracks all published keys per parachain.
	#[pallet::storage]
	pub type PublishedKeys<T: Config> = StorageMap<
		_,
		Twox64Concat,
		ParaId,
		BoundedBTreeSet<BoundedVec<u8, T::MaxKeyLength>, T::MaxStoredKeys>,
		ValueQuery,
	>;

	/// Child trie root for each publisher parachain.
	///
	/// Maps ParaId -> child_trie_root hash (32 bytes).
	/// This allows selective inclusion in storage proofs - only roots for publishers
	/// we're interested in need to be included.
	#[pallet::storage]
	pub type PublishedDataRoots<T: Config> = CountedStorageMap<
		_,
		Twox64Concat,
		ParaId,
		[u8; 32],
		OptionQuery,
	>;

	#[pallet::error]
	pub enum Error<T> {
		/// Too many items in a single publish operation.
		TooManyPublishItems,
		/// Key length exceeds maximum allowed.
		KeyTooLong,
		/// Value length exceeds maximum allowed.
		ValueTooLong,
		/// Too many unique keys stored for this publisher.
		TooManyStoredKeys,
		/// Maximum number of publishers reached.
		TooManyPublishers,
		/// Para is not registered as a publisher.
		NotRegistered,
		/// Para is already registered as a publisher.
		AlreadyRegistered,
		/// Cannot publish without being registered first.
		PublishNotAuthorized,
		/// Caller is not authorized to perform this action.
		NotAuthorized,
		/// Cannot deregister while published data exists. Call cleanup_published_data first.
		MustCleanupDataFirst,
		/// No published data to cleanup.
		NoDataToCleanup,
	}

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		fn integrity_test() {
			assert!(
				T::MaxPublishItems::get() <= xcm::v5::MaxPublishItems::get(),
				"Broadcaster MaxPublishItems exceeds XCM MaxPublishItems upper bound"
			);
			assert!(
				T::MaxKeyLength::get() <= xcm::v5::MaxPublishKeyLength::get(),
				"Broadcaster MaxKeyLength exceeds XCM MaxPublishKeyLength upper bound"
			);
			assert!(
				T::MaxValueLength::get() <= xcm::v5::MaxPublishValueLength::get(),
				"Broadcaster MaxValueLength exceeds XCM MaxPublishValueLength upper bound"
			);
		}
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Register a parachain as a publisher with the calling account as manager.
		///
		/// Requires `PublisherDeposit` to be held from the caller's account.
		///
		/// Parameters:
		/// - `origin`: Signed origin that will become the publisher manager and pay the deposit.
		/// - `para_id`: The parachain to register as a publisher.
		///
		/// Errors:
		/// - `AlreadyRegistered`
		/// - `InsufficientBalance` (from Currency trait)
		///
		/// Events:
		/// - `PublisherRegistered`
		#[pallet::call_index(0)]
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 1))]
		pub fn register_publisher(
			origin: OriginFor<T>,
			para_id: ParaId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;
			Self::do_register_publisher(who, para_id, T::PublisherDeposit::get())
		}

		/// Register a parachain as a publisher with a custom deposit amount.
		///
		/// Allows Root to register system parachains with zero or reduced deposits.
		///
		/// Parameters:
		/// - `origin`: Root origin.
		/// - `manager`: Account that will manage the publisher.
		/// - `deposit`: Custom deposit amount to hold (typically zero for system parachains).
		/// - `para_id`: The parachain to register as a publisher.
		///
		/// Errors:
		/// - `AlreadyRegistered`
		/// - `InsufficientBalance` (from Currency trait if deposit is non-zero)
		///
		/// Events:
		/// - `PublisherRegistered`
		#[pallet::call_index(1)]
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 1))]
		pub fn force_register_publisher(
			origin: OriginFor<T>,
			manager: T::AccountId,
			deposit: BalanceOf<T>,
			para_id: ParaId,
		) -> DispatchResult {
			ensure_root(origin)?;
			Self::do_register_publisher(manager, para_id, deposit)
		}

		/// Remove all published data for a parachain.
		///
		/// Must be called before `deregister_publisher`. Only callable by the publisher manager.
		///
		/// Parameters:
		/// - `origin`: Signed origin, must be the publisher manager.
		/// - `para_id`: The parachain to clean up.
		///
		/// Errors:
		/// - `NotRegistered`
		/// - `NotAuthorized`
		/// - `NoDataToCleanup`
		///
		/// Events:
		/// - `DataCleanedUp`
		#[pallet::call_index(2)]
		#[pallet::weight(
			T::DbWeight::get().reads(2)
			.saturating_add(T::DbWeight::get().writes(3))
			.saturating_add(Weight::from_parts(567_000, 0).saturating_mul(T::MaxStoredKeys::get().into()))
		)]
		pub fn cleanup_published_data(
			origin: OriginFor<T>,
			para_id: ParaId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let info = RegisteredPublishers::<T>::get(para_id)
				.ok_or(Error::<T>::NotRegistered)?;

			ensure!(who == info.manager, Error::<T>::NotAuthorized);
			ensure!(PublisherExists::<T>::get(para_id), Error::<T>::NoDataToCleanup);

			Self::do_cleanup_publisher(para_id)?;

			Self::deposit_event(Event::DataCleanedUp { para_id });
			Ok(())
		}

		/// Deregister a publisher and release their deposit.
		///
		/// All published data must be cleaned up first via `cleanup_published_data`.
		///
		/// Parameters:
		/// - `origin`: Signed origin, must be the publisher manager.
		/// - `para_id`: The parachain to deregister.
		///
		/// Errors:
		/// - `NotRegistered`
		/// - `NotAuthorized`
		/// - `MustCleanupDataFirst`
		///
		/// Events:
		/// - `PublisherDeregistered`
		#[pallet::call_index(3)]
		#[pallet::weight(T::DbWeight::get().reads_writes(2, 2))]
		pub fn deregister_publisher(
			origin: OriginFor<T>,
			para_id: ParaId,
		) -> DispatchResult {
			let who = ensure_signed(origin)?;

			let info = RegisteredPublishers::<T>::get(para_id)
				.ok_or(Error::<T>::NotRegistered)?;

			ensure!(who == info.manager, Error::<T>::NotAuthorized);
			ensure!(!PublisherExists::<T>::get(para_id), Error::<T>::MustCleanupDataFirst);

			Self::do_deregister(para_id, info)?;

			Self::deposit_event(Event::PublisherDeregistered { para_id });
			Ok(())
		}

		/// Force deregister a publisher, cleaning up data if necessary.
		///
		/// Combines cleanup and deregistration in a single call. Only callable by Root.
		///
		/// Parameters:
		/// - `origin`: Root origin.
		/// - `para_id`: The parachain to force deregister.
		///
		/// Errors:
		/// - `NotRegistered`
		///
		/// Events:
		/// - `DataCleanedUp` (if data existed)
		/// - `PublisherDeregistered`
		#[pallet::call_index(4)]
		#[pallet::weight(
			T::DbWeight::get().reads(2)
			.saturating_add(T::DbWeight::get().writes(5))
			.saturating_add(Weight::from_parts(567_000, 0).saturating_mul(T::MaxStoredKeys::get().into()))
		)]
		pub fn force_deregister_publisher(
			origin: OriginFor<T>,
			para_id: ParaId,
		) -> DispatchResult {
			ensure_root(origin)?;

			let info = RegisteredPublishers::<T>::get(para_id)
				.ok_or(Error::<T>::NotRegistered)?;

			// Clean up data if it exists
			if PublisherExists::<T>::get(para_id) {
				Self::do_cleanup_publisher(para_id)?;
				Self::deposit_event(Event::DataCleanedUp { para_id });
			}

			Self::do_deregister(para_id, info)?;

			Self::deposit_event(Event::PublisherDeregistered { para_id });
			Ok(())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Register a publisher, holding the deposit from the manager account.
		fn do_register_publisher(
			manager: T::AccountId,
			para_id: ParaId,
			deposit: BalanceOf<T>,
		) -> DispatchResult {
			// Check not already registered
			ensure!(
				!RegisteredPublishers::<T>::contains_key(para_id),
				Error::<T>::AlreadyRegistered
			);

			// Hold the deposit if non-zero
			if !deposit.is_zero() {
				<T as Config>::Currency::hold(
					&HoldReason::PublisherDeposit.into(),
					&manager,
					deposit,
				)?;
			}

			let info = PublisherInfo { manager: manager.clone(), deposit };

			RegisteredPublishers::<T>::insert(para_id, info);
			Self::deposit_event(Event::PublisherRegistered { para_id, manager });

			Ok(())
		}

		fn do_cleanup_publisher(para_id: ParaId) -> DispatchResult {
			let child_info = Self::derive_child_info(para_id);
			let published_keys = PublishedKeys::<T>::get(para_id);

			// Remove all key-value pairs from the child trie
			for bounded_key in published_keys.iter() {
				let key: Vec<u8> = bounded_key.clone().into();
				frame_support::storage::child::kill(&child_info, &key);
			}

			// Clean up tracking storage
			PublishedKeys::<T>::remove(para_id);
			PublishedDataRoots::<T>::remove(para_id);
			PublisherExists::<T>::remove(para_id);

			Ok(())
		}

		fn do_deregister(
			para_id: ParaId,
			info: PublisherInfo<T::AccountId, BalanceOf<T>>,
		) -> DispatchResult {
			// Release deposit if non-zero
			if !info.deposit.is_zero() {
				let released = <T as Config>::Currency::release(
					&HoldReason::PublisherDeposit.into(),
					&info.manager,
					info.deposit,
					Exact,
				)?;

				defensive_assert!(
					released == info.deposit,
					"deposit should be fully released"
				);
			}

			// Remove registration
			RegisteredPublishers::<T>::remove(para_id);

			Ok(())
		}

		/// Called by the initializer to note that a new session has started.
		pub(crate) fn initializer_on_new_session(
			_notification: &crate::initializer::SessionChangeNotification<BlockNumberFor<T>>,
			outgoing_paras: &[ParaId],
		) -> Weight {
			Self::cleanup_outgoing_publishers(outgoing_paras)
		}

		/// Remove all storage for offboarded parachains.
		fn cleanup_outgoing_publishers(outgoing: &[ParaId]) -> Weight {
			let mut total_weight = Weight::zero();
			for outgoing_para in outgoing {
				total_weight = total_weight.saturating_add(Self::cleanup_outgoing_publisher(outgoing_para));
			}
			total_weight
		}

		/// Remove all relevant storage items for an outgoing parachain.
		fn cleanup_outgoing_publisher(outgoing_para: &ParaId) -> Weight {
			if let Some(info) = RegisteredPublishers::<T>::get(outgoing_para) {
				let weight = if PublisherExists::<T>::get(outgoing_para) {
					let published_keys = PublishedKeys::<T>::get(outgoing_para);
					let key_count = published_keys.len() as u32;
					let _ = Self::do_cleanup_publisher(*outgoing_para);
					T::WeightInfo::do_cleanup_publisher(key_count)
				} else {
					Weight::zero()
				};

				let _ = Self::do_deregister(*outgoing_para, info);

				// Account for reads (RegisteredPublishers, PublisherExists) and writes (deregister)
				return weight
					.saturating_add(T::DbWeight::get().reads(2))
					.saturating_add(T::DbWeight::get().writes(1));
			}
			T::DbWeight::get().reads(1) // Just the RegisteredPublishers read
		}

		/// Processes a publish operation from a parachain.
		///
		/// Validates the publisher is registered, checks all bounds, and stores the provided
		/// key-value pairs in the publisher's dedicated child trie. Updates the child trie root
		/// and published keys tracking.
		pub fn handle_publish(
			origin_para_id: ParaId,
			data: Vec<(Vec<u8>, Vec<u8>)>,
		) -> DispatchResult {
			// Check publisher is registered
			ensure!(
				RegisteredPublishers::<T>::contains_key(origin_para_id),
				Error::<T>::PublishNotAuthorized
			);

			let items_count = data.len() as u32;

			// Validate input limits first before making any changes
			ensure!(
				data.len() <= T::MaxPublishItems::get() as usize,
				Error::<T>::TooManyPublishItems
			);

			// Validate all keys and values before creating publisher entry
			for (key, value) in &data {
				ensure!(
					key.len() <= T::MaxKeyLength::get() as usize,
					Error::<T>::KeyTooLong
				);
				ensure!(
					value.len() <= T::MaxValueLength::get() as usize,
					Error::<T>::ValueTooLong
				);
			}

			// Get or create child trie. This checks MaxPublishers limit on first publish.
			let child_info = Self::get_or_create_publisher_child_info(origin_para_id)?;

			let mut published_keys = PublishedKeys::<T>::get(origin_para_id);

			// Count new unique keys to prevent exceeding MaxStoredKeys
			let mut new_keys_count = 0u32;
			for (key, _) in &data {
				if let Ok(bounded_key) = BoundedVec::try_from(key.clone()) {
					if !published_keys.contains(&bounded_key) {
						new_keys_count += 1;
					}
				}
			}

			let current_keys_count = published_keys.len() as u32;
			ensure!(
				current_keys_count.saturating_add(new_keys_count) <= T::MaxStoredKeys::get(),
				Error::<T>::TooManyStoredKeys
			);

			// Write to child trie and track keys for enumeration
			for (key, value) in data {
				frame_support::storage::child::put(&child_info, &key, &value);

				if let Ok(bounded_key) = BoundedVec::try_from(key) {
					published_keys.try_insert(bounded_key).defensive_ok();
				}
			}

			PublishedKeys::<T>::insert(origin_para_id, published_keys);

			// Update child trie root for storage proof verification
			let child_root = frame_support::storage::child::root(&child_info,
				sp_runtime::StateVersion::V1);

			let root_array: [u8; 32] = child_root.try_into()
				.defensive_unwrap_or([0u8; 32]);
			PublishedDataRoots::<T>::insert(origin_para_id, root_array);

			Self::deposit_event(Event::DataPublished { publisher: origin_para_id, items_count });

			Ok(())
		}

		/// Returns the child trie root hash for a specific publisher.
		///
		/// The root can be included in storage proofs for subscribers to verify published data.
		pub fn get_publisher_child_root(para_id: ParaId) -> Option<Vec<u8>> {
			PublisherExists::<T>::get(para_id).then(|| {
				let child_info = Self::derive_child_info(para_id);
				frame_support::storage::child::root(&child_info, sp_runtime::StateVersion::V1)
			})
		}

		/// Gets or creates the child trie info for a publisher.
		///
		/// Checks the maximum publishers limit before creating a new publisher entry.
		fn get_or_create_publisher_child_info(para_id: ParaId) -> Result<ChildInfo, DispatchError> {
			if !PublisherExists::<T>::contains_key(para_id) {
				ensure!(
					PublishedDataRoots::<T>::count() < T::MaxPublishers::get(),
					Error::<T>::TooManyPublishers
				);
				PublisherExists::<T>::insert(para_id, true);
			}
			Ok(Self::derive_child_info(para_id))
		}

		/// Derives a deterministic child trie identifier from a parachain ID.
		///
		/// The child trie identifier is `(b"pubsub", para_id)` encoded.
		pub fn derive_child_info(para_id: ParaId) -> ChildInfo {
			ChildInfo::new_default(&(b"pubsub", para_id).encode())
		}

		/// Retrieves a value from a publisher's child trie.
		///
		/// Returns `None` if the publisher doesn't exist or the key is not found.
		pub fn get_published_value(para_id: ParaId, key: &[u8]) -> Option<Vec<u8>> {
			PublisherExists::<T>::get(para_id).then(|| {
				let child_info = Self::derive_child_info(para_id);
				frame_support::storage::child::get(&child_info, key)
			})?
		}

		/// Returns all published data for a parachain.
		///
		/// Iterates over all tracked keys for the publisher and retrieves their values from the
		/// child trie.
		pub fn get_all_published_data(para_id: ParaId) -> Vec<(Vec<u8>, Vec<u8>)> {
			if !PublisherExists::<T>::get(para_id) {
				return Vec::new();
			}

			let child_info = Self::derive_child_info(para_id);
			let published_keys = PublishedKeys::<T>::get(para_id);

			published_keys
				.into_iter()
				.filter_map(|bounded_key| {
					let key: Vec<u8> = bounded_key.into();
					frame_support::storage::child::get(&child_info, &key)
						.map(|value| (key, value))
				})
				.collect()
		}

		/// Returns a list of all parachains that have published data.
		pub fn get_all_publishers() -> Vec<ParaId> {
			PublisherExists::<T>::iter_keys().collect()
		}
	}
}

// Implement Publish trait
impl<T: Config> Publish for Pallet<T> {
	fn publish_data(publisher: ParaId, data: Vec<(Vec<u8>, Vec<u8>)>) -> DispatchResult {
		Self::handle_publish(publisher, data)
	}
}