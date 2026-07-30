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

//! # Scarcity Pallet
//!
//! `pallet-scarcity` defines NFT collections and item definitions, then mints instances using a
//! coinage-style ownership model: each purse key can hold at most one NFT. The pallet knows
//! ownership, supply, metadata, and deposits; it knows nothing about what an item means. Item
//! semantics and access-control policy belong to collection contracts. At this storage layer the
//! collection owner has full control over its definitions, metadata, and live instances.
//!
//! The current collection owner backs all collection state with one aggregate balance hold. To
//! transfer that responsibility safely, the owner first nominates a successor and the successor
//! claims the collection. Claiming atomically holds the exact aggregate from the successor,
//! releases it to the former owner, and transfers collection authority. This lets the successor
//! reject an unwanted or unaffordable collection.
//!
//! Cleanup proceeds from leaves to roots so every call remains bounded. The collection owner
//! burns live instances (or holders burn their own), removes item metadata, deletes empty item
//! definitions, removes collection metadata, and finally deletes the empty collection. Instance
//! metadata is bounded and removed automatically on burn. Allocated identifiers are never reused.
//!
//! Collection metadata supplies defaults inherited by every item definition, and item metadata
//! supplies defaults shared by every instance minted from that definition. A minted instance may
//! override either scope without affecting other instances. [`Pallet::instance_metadata_of`]
//! resolves one key in instance, item, then collection order. [`Pallet::metadata_of`] resolves the
//! item and collection scopes, while [`Pallet::collection_metadata_of`] reads only the collection
//! scope.
//!
//! Keys and values are bounded raw bytes. Numeric metadata is a convention rather than a pallet
//! type: games should use SCALE-encoded `u128` values when they need a shared numeric convention,
//! and must decode and validate those bytes themselves.
//!
//! Transfers are feeless when authorized through the [`AsScarcity`](extension::AsScarcity)
//! transaction extension. Their transaction priority is the time since the NFT last moved,
//! capped by the runtime. Moving an NFT consumes it from the old purse key and places it at the
//! new one. Each authorization names the permanent instance and is carried by a signed
//! transaction; runtimes using [`AsScarcity`](extension::AsScarcity) must include an account-nonce
//! transaction extension such as `frame_system::CheckNonce`. Failed dispatch restores the NFT and
//! temporarily locks the purse key.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

/// Runtime-only integration for minting an NFT without instance-scoped storage deposits.
///
/// Implementations still enforce all collection, item, supply, and ownership invariants. The
/// calling runtime pallet is responsible for bounding depositless state growth through some
/// other scarce resource or protocol rule.
pub trait MintWithoutDeposit<AccountId> {
	/// Bounded metadata key accepted by the implementation.
	type MetadataKey;
	/// Bounded metadata value accepted by the implementation.
	type MetadataValue;

	/// Mint an instance and its initial metadata without storage deposits.
	fn mint_without_deposit(
		collection: CollectionId,
		item: ItemIndex,
		to: AccountId,
		metadata: alloc::vec::Vec<(Self::MetadataKey, Self::MetadataValue)>,
	) -> Result<InstanceId, sp_runtime::DispatchError>;
}

#[cfg(feature = "runtime-benchmarks")]
mod benchmarking;

#[cfg(test)]
mod mock;

#[cfg(test)]
mod tests;

pub mod extension;
pub mod weights;

#[frame_support::pallet]
pub mod pallet {
	use crate::weights::WeightInfo;
	#[cfg(any(test, feature = "try-runtime"))]
	use alloc::collections::BTreeMap;
	use alloc::vec::Vec;
	#[cfg(any(test, feature = "try-runtime"))]
	use frame_support::traits::fungible::InspectHold as _;
	use frame_support::{
		pallet_prelude::*,
		traits::{
			fungible::{self, MutateHold as _},
			tokens::Precision,
			Footprint, IsSubType, UnixTime,
		},
		transactional,
	};
	use frame_system::pallet_prelude::*;
	#[cfg(any(test, feature = "try-runtime"))]
	use sp_runtime::TryRuntimeError;
	use sp_runtime::{
		traits::{CheckedAdd, CheckedSub, Convert, Zero},
		transaction_validity::TransactionPriority,
		ArithmeticError, DispatchError,
	};

	pub type CollectionId = u32;
	/// Index within a collection; `(CollectionId, ItemIndex)` names an item definition.
	pub type ItemIndex = u32;
	/// Permanent global serial, assigned at mint.
	pub type InstanceId = u64;
	pub type BalanceOf<T> = <<T as Config>::Currency as fungible::Inspect<
		<T as frame_system::Config>::AccountId,
	>>::Balance;
	pub type MetadataKeyOf<T> = BoundedVec<u8, <T as Config>::MaxKeyLen>;
	pub type MetadataValueOf<T> = BoundedVec<u8, <T as Config>::MaxValueLen>;

	/// One stored metadata entry and the deposit backing it.
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
	)]
	pub struct MetadataEntry<Value, Balance> {
		pub value: Value,
		/// Exact amount held as the storage deposit for this entry.
		pub deposit: Balance,
	}

	/// Collection owner, ownership handoff, item allocation, and deposit accounting.
	#[derive(
		CloneNoBound,
		PartialEqNoBound,
		EqNoBound,
		DebugNoBound,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct CollectionInfo<AccountId: Member, Balance: Member> {
		/// Only this account may perform collection-owner-authorized operations.
		pub owner: AccountId,
		/// Account nominated by `owner` to claim the collection.
		pub pending_owner: Option<AccountId>,
		/// The next item index to allocate, beginning at zero.
		pub next_item_index: ItemIndex,
		/// Number of item definitions which have not been deleted.
		pub item_count: u32,
		/// Number of collection-level metadata entries.
		pub metadata_count: u32,
		/// Exact deposit for the collection record itself.
		pub collection_deposit: Balance,
		/// Exact aggregate deposit held from `owner` for all collection-backed state.
		pub owner_deposit: Balance,
	}

	/// Immutable shared definition for every minted copy of an item.
	#[derive(
		CloneNoBound,
		PartialEqNoBound,
		EqNoBound,
		DebugNoBound,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct ItemDefinition<Balance: Member> {
		/// Number of instances ever minted from this definition; burns do not decrement it.
		pub supply: u32,
		/// Number of currently live instances; every burn decrements it.
		pub live_supply: u32,
		/// Number of item-level metadata entries.
		pub metadata_count: u32,
		/// Exact storage deposit included in the collection owner's aggregate hold.
		pub deposit: Balance,
	}

	/// A minted Scarcity instance.
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
	)]
	pub struct Nft {
		pub instance: InstanceId,
		pub collection: CollectionId,
		pub item: ItemIndex,
		/// Unix seconds at mint.
		pub minted_at: u64,
		/// Unix seconds; equal to `minted_at` until the first transfer.
		pub last_moved: u64,
	}

	/// Post-failure backoff lock for an NFT purse key.
	#[derive(
		Clone,
		Copy,
		PartialEq,
		Eq,
		Debug,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub struct LockInfo {
		/// Number of consecutive failed purse-key dispatches.
		pub retries: u8,
		/// Unix timestamp (seconds) at which this lock expires.
		pub until: u64,
	}

	/// The next collection identifier to allocate.
	#[pallet::storage]
	pub type NextCollectionId<T> = StorageValue<_, CollectionId, ValueQuery>;

	/// Immutable item catalogues, grouped by their collection.
	#[pallet::storage]
	pub type Collections<T: Config> =
		StorageMap<_, Twox64Concat, CollectionId, CollectionInfo<T::AccountId, BalanceOf<T>>>;

	/// Immutable item definitions, indexed within their collection.
	#[pallet::storage]
	pub type ItemDefs<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		CollectionId,
		Twox64Concat,
		ItemIndex,
		ItemDefinition<BalanceOf<T>>,
	>;

	/// Collection-wide defaults inherited by every item in a collection.
	///
	/// Entries must be removed individually before the collection can be deleted.
	#[pallet::storage]
	pub type CollectionMetadata<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		CollectionId,
		Blake2_128Concat,
		MetadataKeyOf<T>,
		MetadataEntry<MetadataValueOf<T>, BalanceOf<T>>,
	>;

	/// Per-item defaults which override collection defaults for the same key.
	///
	/// These entries are shared by every instance minted from the item definition. Item
	/// definitions outlive minted instances, so burning an instance never removes them. Entries
	/// must be removed individually before the item definition can be deleted.
	#[pallet::storage]
	pub type ItemMetadata<T: Config> = StorageNMap<
		_,
		(
			NMapKey<Twox64Concat, CollectionId>,
			NMapKey<Twox64Concat, ItemIndex>,
			NMapKey<Blake2_128Concat, MetadataKeyOf<T>>,
		),
		MetadataEntry<MetadataValueOf<T>, BalanceOf<T>>,
	>;

	/// The next permanent instance identifier to allocate.
	#[pallet::storage]
	pub type NextInstanceId<T> = StorageValue<_, InstanceId, ValueQuery>;

	/// One NFT per owner key — the coinage model.
	#[pallet::storage]
	pub type NftsByOwner<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, Nft>;

	/// Stable reverse index from instance identifier to its current owner key.
	#[pallet::storage]
	pub type Instances<T: Config> = StorageMap<_, Twox64Concat, InstanceId, T::AccountId>;

	/// Exact per-instance storage deposits, present only for deposit-paying mints.
	#[pallet::storage]
	pub type InstanceDeposits<T: Config> = StorageMap<_, Twox64Concat, InstanceId, BalanceOf<T>>;

	/// Number of metadata overrides stored for each live instance.
	///
	/// This entry exists even when the count is zero so burn cleanup remains bounded and
	/// `try_state` can verify that every live instance has explicit metadata accounting.
	#[pallet::storage]
	pub type InstanceMetadataCount<T: Config> =
		StorageMap<_, Twox64Concat, InstanceId, u32, ValueQuery>;

	/// Per-instance entries which override item and collection metadata for the same key.
	#[pallet::storage]
	pub type InstanceMetadata<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		InstanceId,
		Blake2_128Concat,
		MetadataKeyOf<T>,
		MetadataEntry<MetadataValueOf<T>, BalanceOf<T>>,
	>;

	/// Post-failure backoff locks for NFT purse keys.
	///
	/// A separate lock explicitly rejects transactions during backoff. Updating `last_moved`
	/// would only lower transaction priority and would not enforce a delay.
	#[pallet::storage]
	pub type Locked<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, LockInfo>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A collection was created and assigned to its initial owner.
		CollectionCreated { collection: CollectionId, owner: T::AccountId },
		/// The collection owner nominated or cleared a prospective owner.
		CollectionOwnerNominated { collection: CollectionId, pending_owner: Option<T::AccountId> },
		/// A nominated account claimed the collection and assumed its aggregate storage deposit.
		CollectionOwnerChanged {
			collection: CollectionId,
			old_owner: T::AccountId,
			new_owner: T::AccountId,
		},
		/// An empty collection was deleted and its remaining deposit released.
		CollectionDeleted { collection: CollectionId },
		/// An immutable item definition was assigned within a collection.
		ItemDefined { collection: CollectionId, item: ItemIndex },
		/// An unused item definition was deleted and its deposit released.
		ItemDeleted { collection: CollectionId, item: ItemIndex },
		/// An instance was minted into an empty purse key.
		Minted {
			instance: InstanceId,
			collection: CollectionId,
			item: ItemIndex,
			owner: T::AccountId,
		},
		/// An instance moved to a new purse key.
		Transferred { instance: InstanceId, to: T::AccountId },
		/// An instance was permanently removed.
		Burned { instance: InstanceId },
		/// A collection-level metadata default was inserted or overwritten.
		CollectionMetadataSet { collection: CollectionId, key: MetadataKeyOf<T> },
		/// A collection-level metadata default was removed.
		CollectionMetadataRemoved { collection: CollectionId, key: MetadataKeyOf<T> },
		/// An item-level metadata override was inserted or overwritten.
		ItemMetadataSet { collection: CollectionId, item: ItemIndex, key: MetadataKeyOf<T> },
		/// An item-level metadata override was removed.
		ItemMetadataRemoved { collection: CollectionId, item: ItemIndex, key: MetadataKeyOf<T> },
		/// An instance-level metadata override was inserted or overwritten.
		InstanceMetadataSet { instance: InstanceId, key: MetadataKeyOf<T> },
		/// An instance-level metadata override was removed.
		InstanceMetadataRemoved { instance: InstanceId, key: MetadataKeyOf<T> },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The collection identifier space is exhausted.
		TooManyCollections,
		/// The collection does not exist.
		UnknownCollection,
		/// Only the current collection owner may perform this operation.
		NoPermission,
		/// The current collection owner cannot be nominated as its replacement.
		AlreadyCollectionOwner,
		/// The signer is not the account currently nominated to claim this collection.
		NotPendingCollectionOwner,
		/// The per-collection item index space is exhausted.
		TooManyItems,
		/// The requested item definition does not exist.
		UnknownItem,
		/// An item with live instances cannot be deleted.
		ItemInUse,
		/// Item metadata must be removed before deleting the item.
		ItemMetadataNotEmpty,
		/// Item definitions must be deleted before deleting the collection.
		CollectionItemsNotEmpty,
		/// Collection metadata must be removed before deleting the collection.
		CollectionMetadataNotEmpty,
		/// Stored dependency counters or deposits do not permit deletion.
		DeletionInvariant,
		/// The destination purse key already holds an NFT.
		AddressOccupied,
		/// The permanent instance identifier space is exhausted.
		TooManyInstances,
		/// An item definition's minted supply is exhausted.
		SupplyOverflow,
		/// The requested live instance does not exist.
		UnknownInstance,
		/// The configured per-instance metadata entry limit was reached.
		TooManyInstanceMetadata,
		/// An NFT cannot be transferred to its current purse key.
		SelfTransfer,
	}

	/// A reason for holding funds as Scarcity storage deposits.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for collection, item, instance, or metadata storage.
		StorageDeposit,
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

	#[pallet::hooks]
	impl<T: Config> Hooks<BlockNumberFor<T>> for Pallet<T> {
		#[cfg(feature = "try-runtime")]
		fn try_state(_n: BlockNumberFor<T>) -> Result<(), TryRuntimeError> {
			Self::do_try_state()
		}
	}

	/// A Scarcity NFT held by the custom dispatch origin.
	#[pallet::origin]
	#[derive(
		CloneNoBound,
		PartialEqNoBound,
		EqNoBound,
		DebugNoBound,
		Encode,
		Decode,
		DecodeWithMemTracking,
		TypeInfo,
		MaxEncodedLen,
	)]
	pub enum Origin<T: Config> {
		/// The NFT is removed from storage by `AsScarcity` before dispatch and held by this
		/// origin.
		Nft { owner: T::AccountId, nft: Nft },
	}

	#[pallet::config]
	pub trait Config:
		frame_system::Config<
			RuntimeOrigin: Into<Result<Origin<Self>, Self::RuntimeOrigin>> + From<Origin<Self>>,
			RuntimeCall: IsSubType<Call<Self>>,
		> + Send
		+ Sync
	{
		#[allow(deprecated)]
		type RuntimeEvent: From<Event<Self>> + IsType<<Self as frame_system::Config>::RuntimeEvent>;

		/// A type representing the weights required by the dispatchables of this pallet.
		type WeightInfo: crate::weights::WeightInfo;

		/// Unix time source for `minted_at`, `last_moved`, rest-time priority, and failure locks.
		type UnixTime: UnixTime;

		/// Fungible used to hold Scarcity storage deposits.
		#[cfg(not(feature = "runtime-benchmarks"))]
		type Currency: fungible::MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>;

		/// Fungible used to hold Scarcity storage deposits.
		#[cfg(feature = "runtime-benchmarks")]
		type Currency: fungible::MutateHold<Self::AccountId, Reason = Self::RuntimeHoldReason>
			+ fungible::Mutate<Self::AccountId>;

		/// Overarching runtime hold reason.
		type RuntimeHoldReason: Parameter + Member + MaxEncodedLen + Copy + From<HoldReason>;

		/// Price of a collection record.
		type CollectionDeposit: Convert<Footprint, BalanceOf<Self>>;

		/// Price of an item definition.
		type ItemDeposit: Convert<Footprint, BalanceOf<Self>>;

		/// Price of one ordinary minted instance.
		type InstanceDeposit: Convert<Footprint, BalanceOf<Self>>;

		/// Price of one metadata entry.
		type MetadataDeposit: Convert<Footprint, BalanceOf<Self>>;

		/// Maximum byte length of one metadata key.
		#[pallet::constant]
		type MaxKeyLen: Get<u32>;

		/// Maximum byte length of one metadata value.
		#[pallet::constant]
		type MaxValueLen: Get<u32>;

		/// Maximum number of metadata overrides stored on one live instance.
		///
		/// This bounds burn cleanup independently of how many mutation calls are submitted.
		#[pallet::constant]
		type MaxInstanceMetadata: Get<u32>;

		/// Base lock period after a failed purse-key dispatch, in seconds.
		#[pallet::constant]
		type LockPeriod: Get<u64>;

		/// Priority ceiling for rested NFT transfers.
		#[pallet::constant]
		type MaxTransferPriority: Get<TransactionPriority>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a collection owned by the signer.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_collection())]
		pub fn create_collection(origin: OriginFor<T>) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_create_collection(owner).map(|_| ())
		}

		/// Define one immutable item and its shared metadata defaults.
		///
		/// The supplied metadata is inherited by every instance minted from this definition.
		/// Instance-specific values belong on [`Self::mint`] or [`Self::set_instance_metadata`].
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::define_item(metadata.len() as u32))]
		pub fn define_item(
			origin: OriginFor<T>,
			collection: CollectionId,
			metadata: Vec<(MetadataKeyOf<T>, MetadataValueOf<T>)>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_define_item(owner, collection, metadata).map(|_| ())
		}

		/// Mint an instance of an immutable item definition into an empty purse key.
		///
		/// `metadata` contains instance-specific overrides. Item metadata remains the shared
		/// default for every instance minted from the definition.
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::mint(metadata.len() as u32))]
		pub fn mint(
			origin: OriginFor<T>,
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
			metadata: Vec<(MetadataKeyOf<T>, MetadataValueOf<T>)>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_mint(owner, collection, item, to, metadata).map(|_| ())
		}

		/// Transfer an NFT held by the `Origin::Nft` purse-key origin.
		#[pallet::call_index(3)]
		#[pallet::weight(T::WeightInfo::transfer())]
		pub fn transfer(origin: OriginFor<T>, to: T::AccountId) -> DispatchResultWithPostInfo {
			let Ok(Origin::Nft { owner, nft }) = origin.into() else {
				return Err(DispatchError::BadOrigin.into());
			};
			ensure!(to != owner, Error::<T>::SelfTransfer);
			ensure!(!NftsByOwner::<T>::contains_key(&to), Error::<T>::AddressOccupied);

			let nft = Nft { last_moved: T::UnixTime::now().as_secs(), ..nft };
			NftsByOwner::<T>::insert(&to, nft.clone());
			Instances::<T>::insert(nft.instance, &to);
			Self::deposit_event(Event::Transferred { instance: nft.instance, to });
			Ok(Pays::No.into())
		}

		/// Burn an NFT held by the `Origin::Nft` purse-key origin.
		///
		/// Item-definition supply counts minted-ever instances and is deliberately monotonic.
		/// Item metadata belongs to the definition and remains untouched. Instance metadata is
		/// removed and its exact deposits are released.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::burn(T::MaxInstanceMetadata::get()))]
		#[transactional]
		pub fn burn(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let Ok(Origin::Nft { nft, .. }) = origin.into() else {
				return Err(DispatchError::BadOrigin.into());
			};
			Self::do_burn(nft)?;
			Ok(Pays::No.into())
		}

		/// Nominate an account to claim ownership of a collection.
		///
		/// Only the current owner may nominate or clear a prospective owner. Nomination does not
		/// change authority or move deposits.
		#[pallet::call_index(5)]
		#[pallet::weight(T::WeightInfo::nominate_collection_owner())]
		pub fn nominate_collection_owner(
			origin: OriginFor<T>,
			collection: CollectionId,
			pending_owner: Option<T::AccountId>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Collections::<T>::try_mutate(collection, |maybe_info| {
				let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
				ensure!(info.owner == owner, Error::<T>::NoPermission);
				ensure!(
					pending_owner.as_ref() != Some(&info.owner),
					Error::<T>::AlreadyCollectionOwner
				);
				info.pending_owner = pending_owner.clone();
				Ok::<_, DispatchError>(())
			})?;
			Self::deposit_event(Event::CollectionOwnerNominated { collection, pending_owner });
			Ok(())
		}

		/// Set or remove a collection-level metadata default.
		///
		/// Only the collection owner may mutate metadata. `None` releases an existing entry's
		/// deposit; when the key is absent it is a successful no-op.
		#[pallet::call_index(6)]
		#[pallet::weight(T::WeightInfo::set_collection_metadata())]
		#[transactional]
		pub fn set_collection_metadata(
			origin: OriginFor<T>,
			collection: CollectionId,
			key: MetadataKeyOf<T>,
			value: Option<MetadataValueOf<T>>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_set_collection_metadata(&owner, collection, key, value)
		}

		/// Set or remove a metadata default shared by every instance of an item.
		///
		/// This scope overrides the collection default without changing any instance-specific
		/// override. Only the collection owner may mutate metadata. `None` releases an existing
		/// entry's deposit; when the key is absent it is a successful no-op.
		#[pallet::call_index(7)]
		#[pallet::weight(T::WeightInfo::set_item_metadata())]
		#[transactional]
		pub fn set_item_metadata(
			origin: OriginFor<T>,
			collection: CollectionId,
			item: ItemIndex,
			key: MetadataKeyOf<T>,
			value: Option<MetadataValueOf<T>>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_set_item_metadata(&owner, collection, item, key, value)
		}

		/// Claim a collection after nomination by its current owner.
		///
		/// The exact aggregate storage deposit is first held from the claimant and then released
		/// to the previous owner. The operation is atomic: insufficient claimant balance or any
		/// release failure leaves ownership and both balances unchanged.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::claim_collection_ownership())]
		#[transactional]
		pub fn claim_collection_ownership(
			origin: OriginFor<T>,
			collection: CollectionId,
		) -> DispatchResult {
			let new_owner = ensure_signed(origin)?;
			Collections::<T>::try_mutate(collection, |maybe_info| {
				let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
				ensure!(
					info.pending_owner.as_ref() == Some(&new_owner),
					Error::<T>::NotPendingCollectionOwner
				);

				let old_owner = info.owner.clone();
				let deposit = info.owner_deposit;
				Self::hold(&new_owner, deposit)?;
				Self::release(&old_owner, deposit)?;
				info.owner = new_owner.clone();
				info.pending_owner = None;
				Self::deposit_event(Event::CollectionOwnerChanged {
					collection,
					old_owner,
					new_owner: new_owner.clone(),
				});
				Ok(())
			})
		}

		/// Set or remove an instance-specific metadata override.
		///
		/// Only the collection owner may mutate metadata. `None` releases an existing entry's
		/// deposit; when the key is absent it is a successful no-op.
		#[pallet::call_index(9)]
		#[pallet::weight(T::WeightInfo::set_instance_metadata())]
		#[transactional]
		pub fn set_instance_metadata(
			origin: OriginFor<T>,
			instance: InstanceId,
			key: MetadataKeyOf<T>,
			value: Option<MetadataValueOf<T>>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_set_instance_metadata(&owner, instance, key, value, true)
		}

		/// Permanently remove one live instance as its collection owner.
		///
		/// The collection layer intentionally applies no holder-level ACL. A contract-owned
		/// collection can enforce its own consent and game rules before calling this operation.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::burn_instance(T::MaxInstanceMetadata::get()))]
		#[transactional]
		pub fn burn_instance(origin: OriginFor<T>, instance: InstanceId) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_burn_instance(&owner, instance)
		}

		/// Delete an unused item definition owned by the signer.
		///
		/// Every live instance must be burned and every item metadata entry removed first.
		/// Deleted item indices are never reused.
		#[pallet::call_index(11)]
		#[pallet::weight(T::WeightInfo::delete_item())]
		#[transactional]
		pub fn delete_item(
			origin: OriginFor<T>,
			collection: CollectionId,
			item: ItemIndex,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_delete_item(&owner, collection, item)
		}

		/// Delete an empty collection owned by the signer.
		///
		/// Every item definition and collection metadata entry must be removed first. Deleted
		/// collection identifiers are never reused.
		#[pallet::call_index(12)]
		#[pallet::weight(T::WeightInfo::delete_collection())]
		#[transactional]
		pub fn delete_collection(origin: OriginFor<T>, collection: CollectionId) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_delete_collection(&owner, collection)
		}
	}

	impl<T: Config> Pallet<T> {
		fn hold_reason() -> T::RuntimeHoldReason {
			HoldReason::StorageDeposit.into()
		}

		fn hold(owner: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
			T::Currency::hold(&Self::hold_reason(), owner, amount)
		}

		fn release(owner: &T::AccountId, amount: BalanceOf<T>) -> DispatchResult {
			T::Currency::release(&Self::hold_reason(), owner, amount, Precision::Exact).map(|_| ())
		}

		fn increase_owner_deposit(
			info: &mut CollectionInfo<T::AccountId, BalanceOf<T>>,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let total = info.owner_deposit.checked_add(&amount).ok_or(ArithmeticError::Overflow)?;
			if !amount.is_zero() {
				Self::hold(&info.owner, amount)?;
			}
			info.owner_deposit = total;
			Ok(())
		}

		fn decrease_owner_deposit(
			info: &mut CollectionInfo<T::AccountId, BalanceOf<T>>,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let total =
				info.owner_deposit.checked_sub(&amount).ok_or(ArithmeticError::Underflow)?;
			if !amount.is_zero() {
				Self::release(&info.owner, amount)?;
			}
			info.owner_deposit = total;
			Ok(())
		}

		fn replace_owner_deposit(
			info: &mut CollectionInfo<T::AccountId, BalanceOf<T>>,
			old: BalanceOf<T>,
			new: BalanceOf<T>,
		) -> DispatchResult {
			let total_without_old =
				info.owner_deposit.checked_sub(&old).ok_or(ArithmeticError::Underflow)?;
			let total = total_without_old.checked_add(&new).ok_or(ArithmeticError::Overflow)?;
			if new > old {
				Self::hold(&info.owner, new.checked_sub(&old).ok_or(ArithmeticError::Underflow)?)?;
			} else if old > new {
				Self::release(
					&info.owner,
					old.checked_sub(&new).ok_or(ArithmeticError::Underflow)?,
				)?;
			}
			info.owner_deposit = total;
			Ok(())
		}

		/// Allocate a collection identifier and record its initial owner.
		pub fn do_create_collection(owner: T::AccountId) -> Result<CollectionId, DispatchError> {
			let collection = NextCollectionId::<T>::get();
			let next_collection =
				collection.checked_add(1).ok_or(Error::<T>::TooManyCollections)?;
			let footprint = Footprint::from_mel::<CollectionInfo<T::AccountId, BalanceOf<T>>>();
			let collection_deposit = T::CollectionDeposit::convert(footprint);
			Self::hold(&owner, collection_deposit)?;

			NextCollectionId::<T>::put(next_collection);
			Collections::<T>::insert(
				collection,
				CollectionInfo {
					owner: owner.clone(),
					pending_owner: None,
					next_item_index: 0,
					item_count: 0,
					metadata_count: 0,
					collection_deposit,
					owner_deposit: collection_deposit,
				},
			);
			Self::deposit_event(Event::CollectionCreated { collection, owner });
			Ok(collection)
		}

		/// Add an immutable item definition to an owned collection.
		#[transactional]
		pub fn do_define_item(
			owner: T::AccountId,
			collection: CollectionId,
			metadata: Vec<(MetadataKeyOf<T>, MetadataValueOf<T>)>,
		) -> Result<ItemIndex, DispatchError> {
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == owner, Error::<T>::NoPermission);

			let item = info.next_item_index;
			info.next_item_index = item.checked_add(1).ok_or(Error::<T>::TooManyItems)?;
			info.item_count = info.item_count.checked_add(1).ok_or(Error::<T>::TooManyItems)?;
			let footprint = Footprint::from_mel::<ItemDefinition<BalanceOf<T>>>();
			let deposit = T::ItemDeposit::convert(footprint);
			Self::increase_owner_deposit(&mut info, deposit)?;
			let definition =
				ItemDefinition { supply: 0, live_supply: 0, metadata_count: 0, deposit };

			Collections::<T>::insert(collection, info);
			ItemDefs::<T>::insert(collection, item, definition);
			Self::deposit_event(Event::ItemDefined { collection, item });
			for (key, value) in metadata {
				Self::do_set_item_metadata(&owner, collection, item, key, Some(value))?;
			}
			Ok(item)
		}

		/// Effective value for `key` on `(collection, item)`.
		///
		/// An item-level entry wins; otherwise the collection-level default is returned.
		pub fn metadata_of(
			collection: CollectionId,
			item: ItemIndex,
			key: &MetadataKeyOf<T>,
		) -> Option<MetadataValueOf<T>> {
			ItemMetadata::<T>::get((collection, item, key.clone()))
				.map(|entry| entry.value)
				.or_else(|| Self::collection_metadata_of(collection, key))
		}

		/// Effective value for `key` on a live minted instance.
		///
		/// An instance-level entry wins, followed by the item definition and then collection.
		pub fn instance_metadata_of(
			instance: InstanceId,
			key: &MetadataKeyOf<T>,
		) -> Option<MetadataValueOf<T>> {
			let owner = Instances::<T>::get(instance)?;
			let nft = NftsByOwner::<T>::get(owner)?;
			if nft.instance != instance {
				return None;
			}
			InstanceMetadata::<T>::get(instance, key)
				.map(|entry| entry.value)
				.or_else(|| Self::metadata_of(nft.collection, nft.item, key))
		}

		/// Only the collection-level value for `key`, without item-level resolution.
		pub fn collection_metadata_of(
			collection: CollectionId,
			key: &MetadataKeyOf<T>,
		) -> Option<MetadataValueOf<T>> {
			CollectionMetadata::<T>::get(collection, key).map(|entry| entry.value)
		}

		fn metadata_footprint(key: &MetadataKeyOf<T>, value: &MetadataValueOf<T>) -> Footprint {
			Footprint::from_parts(
				1,
				key.len()
					.saturating_add(value.len())
					.saturating_add(BalanceOf::<T>::max_encoded_len()),
			)
		}

		fn do_set_collection_metadata(
			owner: &T::AccountId,
			collection: CollectionId,
			key: MetadataKeyOf<T>,
			value: Option<MetadataValueOf<T>>,
		) -> DispatchResult {
			Collections::<T>::try_mutate(collection, |maybe_info| {
				let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
				ensure!(info.owner == *owner, Error::<T>::NoPermission);

				match (CollectionMetadata::<T>::get(collection, &key), value) {
					(Some(entry), Some(value)) => {
						let deposit =
							T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value));
						Self::replace_owner_deposit(info, entry.deposit, deposit)?;
						CollectionMetadata::<T>::insert(
							collection,
							&key,
							MetadataEntry { value, deposit },
						);
						Self::deposit_event(Event::CollectionMetadataSet { collection, key });
					},
					(None, Some(value)) => {
						info.metadata_count =
							info.metadata_count.checked_add(1).ok_or(ArithmeticError::Overflow)?;
						let deposit =
							T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value));
						Self::increase_owner_deposit(info, deposit)?;
						CollectionMetadata::<T>::insert(
							collection,
							&key,
							MetadataEntry { value, deposit },
						);
						Self::deposit_event(Event::CollectionMetadataSet { collection, key });
					},
					(Some(entry), None) => {
						info.metadata_count =
							info.metadata_count.checked_sub(1).ok_or(ArithmeticError::Underflow)?;
						Self::decrease_owner_deposit(info, entry.deposit)?;
						CollectionMetadata::<T>::remove(collection, &key);
						Self::deposit_event(Event::CollectionMetadataRemoved { collection, key });
					},
					(None, None) => {},
				}
				Ok(())
			})
		}

		fn do_set_item_metadata(
			owner: &T::AccountId,
			collection: CollectionId,
			item: ItemIndex,
			key: MetadataKeyOf<T>,
			value: Option<MetadataValueOf<T>>,
		) -> DispatchResult {
			Collections::<T>::try_mutate(collection, |maybe_info| {
				let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
				ensure!(info.owner == *owner, Error::<T>::NoPermission);
				let mut definition =
					ItemDefs::<T>::get(collection, item).ok_or(Error::<T>::UnknownItem)?;

				match (ItemMetadata::<T>::get((collection, item, key.clone())), value) {
					(Some(entry), Some(value)) => {
						let deposit =
							T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value));
						Self::replace_owner_deposit(info, entry.deposit, deposit)?;
						ItemMetadata::<T>::insert(
							(collection, item, key.clone()),
							MetadataEntry { value, deposit },
						);
						Self::deposit_event(Event::ItemMetadataSet { collection, item, key });
					},
					(None, Some(value)) => {
						definition.metadata_count = definition
							.metadata_count
							.checked_add(1)
							.ok_or(ArithmeticError::Overflow)?;
						let deposit =
							T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value));
						Self::increase_owner_deposit(info, deposit)?;
						ItemMetadata::<T>::insert(
							(collection, item, key.clone()),
							MetadataEntry { value, deposit },
						);
						ItemDefs::<T>::insert(collection, item, definition);
						Self::deposit_event(Event::ItemMetadataSet { collection, item, key });
					},
					(Some(entry), None) => {
						definition.metadata_count = definition
							.metadata_count
							.checked_sub(1)
							.ok_or(ArithmeticError::Underflow)?;
						Self::decrease_owner_deposit(info, entry.deposit)?;
						ItemMetadata::<T>::remove((collection, item, key.clone()));
						ItemDefs::<T>::insert(collection, item, definition);
						Self::deposit_event(Event::ItemMetadataRemoved { collection, item, key });
					},
					(None, None) => {},
				}
				Ok(())
			})
		}

		fn do_set_instance_metadata(
			owner: &T::AccountId,
			instance: InstanceId,
			key: MetadataKeyOf<T>,
			value: Option<MetadataValueOf<T>>,
			with_deposit: bool,
		) -> DispatchResult {
			let purse = Instances::<T>::get(instance).ok_or(Error::<T>::UnknownInstance)?;
			let nft = NftsByOwner::<T>::get(purse).ok_or(Error::<T>::UnknownInstance)?;
			ensure!(nft.instance == instance, Error::<T>::UnknownInstance);

			Collections::<T>::try_mutate(nft.collection, |maybe_info| {
				let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
				ensure!(info.owner == *owner, Error::<T>::NoPermission);

				match (InstanceMetadata::<T>::get(instance, &key), value) {
					(Some(entry), Some(value)) => {
						let deposit = if with_deposit {
							T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value))
						} else {
							Zero::zero()
						};
						Self::replace_owner_deposit(info, entry.deposit, deposit)?;
						InstanceMetadata::<T>::insert(
							instance,
							&key,
							MetadataEntry { value, deposit },
						);
						Self::deposit_event(Event::InstanceMetadataSet { instance, key });
					},
					(None, Some(value)) => {
						let count = InstanceMetadataCount::<T>::get(instance);
						ensure!(
							count < T::MaxInstanceMetadata::get(),
							Error::<T>::TooManyInstanceMetadata
						);
						let next_count = count.checked_add(1).ok_or(ArithmeticError::Overflow)?;
						let deposit = if with_deposit {
							T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value))
						} else {
							Zero::zero()
						};
						Self::increase_owner_deposit(info, deposit)?;
						InstanceMetadata::<T>::insert(
							instance,
							&key,
							MetadataEntry { value, deposit },
						);
						InstanceMetadataCount::<T>::insert(instance, next_count);
						Self::deposit_event(Event::InstanceMetadataSet { instance, key });
					},
					(Some(entry), None) => {
						let count = InstanceMetadataCount::<T>::get(instance);
						let next_count = count.checked_sub(1).ok_or(ArithmeticError::Underflow)?;
						Self::decrease_owner_deposit(info, entry.deposit)?;
						InstanceMetadata::<T>::remove(instance, &key);
						InstanceMetadataCount::<T>::insert(instance, next_count);
						Self::deposit_event(Event::InstanceMetadataRemoved { instance, key });
					},
					(None, None) => {},
				}
				Ok(())
			})
		}

		fn do_burn(nft: Nft) -> DispatchResult {
			let instance = nft.instance;
			let expected_metadata_count = InstanceMetadataCount::<T>::take(instance);
			let mut actual_metadata_count = 0u32;
			let mut deposit = InstanceDeposits::<T>::take(instance).unwrap_or_else(Zero::zero);
			for (_, entry) in InstanceMetadata::<T>::drain_prefix(instance) {
				actual_metadata_count = actual_metadata_count.saturating_add(1);
				deposit = deposit.checked_add(&entry.deposit).ok_or(ArithmeticError::Overflow)?;
			}
			debug_assert_eq!(actual_metadata_count, expected_metadata_count);

			ItemDefs::<T>::try_mutate(nft.collection, nft.item, |maybe_definition| {
				let definition = maybe_definition.as_mut().ok_or(Error::<T>::UnknownItem)?;
				definition.live_supply =
					definition.live_supply.checked_sub(1).ok_or(ArithmeticError::Underflow)?;
				Ok::<_, DispatchError>(())
			})?;
			Collections::<T>::try_mutate(nft.collection, |maybe_info| {
				let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
				Self::decrease_owner_deposit(info, deposit)
			})?;
			Instances::<T>::remove(instance);
			Self::deposit_event(Event::Burned { instance });
			Ok(())
		}

		fn do_burn_instance(owner: &T::AccountId, instance: InstanceId) -> DispatchResult {
			let purse = Instances::<T>::get(instance).ok_or(Error::<T>::UnknownInstance)?;
			let nft = NftsByOwner::<T>::get(&purse).ok_or(Error::<T>::UnknownInstance)?;
			ensure!(nft.instance == instance, Error::<T>::UnknownInstance);
			let info =
				Collections::<T>::get(nft.collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == *owner, Error::<T>::NoPermission);

			NftsByOwner::<T>::remove(&purse);
			Locked::<T>::remove(&purse);
			Self::do_burn(nft)
		}

		fn do_delete_item(
			owner: &T::AccountId,
			collection: CollectionId,
			item: ItemIndex,
		) -> DispatchResult {
			Collections::<T>::try_mutate(collection, |maybe_info| {
				let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
				ensure!(info.owner == *owner, Error::<T>::NoPermission);
				let definition =
					ItemDefs::<T>::get(collection, item).ok_or(Error::<T>::UnknownItem)?;
				ensure!(definition.live_supply == 0, Error::<T>::ItemInUse);
				ensure!(definition.metadata_count == 0, Error::<T>::ItemMetadataNotEmpty);
				ensure!(
					ItemMetadata::<T>::iter_prefix((collection, item)).next().is_none(),
					Error::<T>::DeletionInvariant
				);

				info.item_count =
					info.item_count.checked_sub(1).ok_or(ArithmeticError::Underflow)?;
				Self::decrease_owner_deposit(info, definition.deposit)?;
				ItemDefs::<T>::remove(collection, item);
				Self::deposit_event(Event::ItemDeleted { collection, item });
				Ok(())
			})
		}

		fn do_delete_collection(owner: &T::AccountId, collection: CollectionId) -> DispatchResult {
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == *owner, Error::<T>::NoPermission);
			ensure!(info.item_count == 0, Error::<T>::CollectionItemsNotEmpty);
			ensure!(info.metadata_count == 0, Error::<T>::CollectionMetadataNotEmpty);
			ensure!(
				ItemDefs::<T>::iter_prefix(collection).next().is_none(),
				Error::<T>::DeletionInvariant
			);
			ensure!(
				CollectionMetadata::<T>::iter_prefix(collection).next().is_none(),
				Error::<T>::DeletionInvariant
			);
			ensure!(info.owner_deposit == info.collection_deposit, Error::<T>::DeletionInvariant);

			let deposit = info.collection_deposit;
			Self::decrease_owner_deposit(&mut info, deposit)?;
			ensure!(info.owner_deposit.is_zero(), Error::<T>::DeletionInvariant);
			Collections::<T>::remove(collection);
			Self::deposit_event(Event::CollectionDeleted { collection });
			Ok(())
		}

		/// Mint an instance after enforcing collection ownership and the one-NFT-per-key rule.
		pub fn do_mint(
			owner: T::AccountId,
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
			metadata: Vec<(MetadataKeyOf<T>, MetadataValueOf<T>)>,
		) -> Result<InstanceId, DispatchError> {
			let info = Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == owner, Error::<T>::NoPermission);
			Self::do_mint_inner(collection, item, to, metadata, true)
		}

		#[transactional]
		fn do_mint_inner(
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
			metadata: Vec<(MetadataKeyOf<T>, MetadataValueOf<T>)>,
			with_deposit: bool,
		) -> Result<InstanceId, DispatchError> {
			ensure!(
				metadata.len() <= T::MaxInstanceMetadata::get() as usize,
				Error::<T>::TooManyInstanceMetadata
			);
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			let mut definition =
				ItemDefs::<T>::get(collection, item).ok_or(Error::<T>::UnknownItem)?;
			ensure!(!NftsByOwner::<T>::contains_key(&to), Error::<T>::AddressOccupied);

			let instance = NextInstanceId::<T>::get();
			let next_instance = instance.checked_add(1).ok_or(Error::<T>::TooManyInstances)?;
			let next_supply = definition.supply.checked_add(1).ok_or(Error::<T>::SupplyOverflow)?;
			let next_live_supply =
				definition.live_supply.checked_add(1).ok_or(Error::<T>::SupplyOverflow)?;
			let now = T::UnixTime::now().as_secs();
			let nft = Nft { instance, collection, item, minted_at: now, last_moved: now };
			let instance_deposit = if with_deposit {
				// Four storage entries back one instance: `NftsByOwner`, the `Instances`
				// reverse index, `InstanceDeposits`, and `InstanceMetadataCount`.
				let record_size = nft
					.encoded_size()
					.saturating_add(to.encoded_size())
					.saturating_add(instance.encoded_size())
					.saturating_add(BalanceOf::<T>::max_encoded_len())
					.saturating_add(u32::max_encoded_len());
				let deposit = T::InstanceDeposit::convert(Footprint::from_parts(4, record_size));
				Self::increase_owner_deposit(&mut info, deposit)?;
				Some(deposit)
			} else {
				None
			};

			definition.supply = next_supply;
			definition.live_supply = next_live_supply;
			NextInstanceId::<T>::put(next_instance);
			let collection_owner = info.owner.clone();
			Collections::<T>::insert(collection, info);
			ItemDefs::<T>::insert(collection, item, definition);
			NftsByOwner::<T>::insert(&to, nft);
			Instances::<T>::insert(instance, &to);
			InstanceMetadataCount::<T>::insert(instance, 0);
			if let Some(deposit) = instance_deposit {
				InstanceDeposits::<T>::insert(instance, deposit);
			}
			for (key, value) in metadata {
				Self::do_set_instance_metadata(
					&collection_owner,
					instance,
					key,
					Some(value),
					with_deposit,
				)?;
			}
			Self::deposit_event(Event::Minted { instance, collection, item, owner: to });
			Ok(instance)
		}
	}

	impl<T: Config> crate::MintWithoutDeposit<T::AccountId> for Pallet<T> {
		type MetadataKey = MetadataKeyOf<T>;
		type MetadataValue = MetadataValueOf<T>;

		fn mint_without_deposit(
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
			metadata: Vec<(Self::MetadataKey, Self::MetadataValue)>,
		) -> Result<InstanceId, DispatchError> {
			ensure!(Collections::<T>::contains_key(collection), Error::<T>::UnknownCollection);
			Self::do_mint_inner(collection, item, to, metadata, false)
		}
	}

	#[cfg(any(test, feature = "try-runtime"))]
	impl<T: Config> Pallet<T> {
		/// Check allocation counters, catalogue references, ownership indexes, deposits, locks,
		/// and metadata references.
		pub(crate) fn do_try_state() -> Result<(), TryRuntimeError> {
			let next_collection = NextCollectionId::<T>::get();
			let next_instance = NextInstanceId::<T>::get();
			let mut expected_collection_deposits = BTreeMap::<CollectionId, BalanceOf<T>>::new();

			for (collection, info) in Collections::<T>::iter() {
				if collection >= next_collection {
					return Err(TryRuntimeError::Other(
						"collection identifier is not below NextCollectionId",
					));
				}
				expected_collection_deposits.insert(collection, info.collection_deposit);
			}

			let mut actual_item_counts = BTreeMap::<CollectionId, u32>::new();
			for (collection, item, definition) in ItemDefs::<T>::iter() {
				let info = Collections::<T>::get(collection)
					.ok_or(TryRuntimeError::Other("ItemDefs entry has no matching collection"))?;
				if item >= info.next_item_index {
					return Err(TryRuntimeError::Other(
						"item index is not below the collection's next item index",
					));
				}
				if definition.live_supply > definition.supply {
					return Err(TryRuntimeError::Other(
						"item live supply exceeds its minted supply",
					));
				}
				let item_count = actual_item_counts.entry(collection).or_default();
				*item_count = item_count
					.checked_add(1)
					.ok_or(TryRuntimeError::Other("collection item count overflowed"))?;
				let expected = expected_collection_deposits
					.get_mut(&collection)
					.ok_or(TryRuntimeError::Other("ItemDefs entry has no deposit aggregate"))?;
				*expected = expected
					.checked_add(&definition.deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
			}
			for (collection, info) in Collections::<T>::iter() {
				let actual = actual_item_counts.get(&collection).copied().unwrap_or_default();
				if info.item_count != actual {
					return Err(TryRuntimeError::Other(
						"collection item count does not match stored definitions",
					));
				}
			}

			let mut live_by_item = BTreeMap::<(CollectionId, ItemIndex), u64>::new();
			for (owner, nft) in NftsByOwner::<T>::iter() {
				if nft.instance >= next_instance {
					return Err(TryRuntimeError::Other("NFT instance is not below NextInstanceId"));
				}
				if Instances::<T>::get(nft.instance) != Some(owner) {
					return Err(TryRuntimeError::Other(
						"NftsByOwner entry has no matching Instances entry",
					));
				}
				let definition = ItemDefs::<T>::get(nft.collection, nft.item)
					.ok_or(TryRuntimeError::Other("NFT has no matching item definition"))?;
				let live = live_by_item.entry((nft.collection, nft.item)).or_default();
				*live += 1;
				if *live > u64::from(definition.live_supply) {
					return Err(TryRuntimeError::Other(
						"item live supply is below its stored instance count",
					));
				}
			}
			for (collection, item, definition) in ItemDefs::<T>::iter() {
				let actual = live_by_item.get(&(collection, item)).copied().unwrap_or_default();
				if u64::from(definition.live_supply) != actual {
					return Err(TryRuntimeError::Other(
						"item live supply does not match stored instances",
					));
				}
			}

			for (instance, owner) in Instances::<T>::iter() {
				if instance >= next_instance {
					return Err(TryRuntimeError::Other(
						"Instances identifier is not below NextInstanceId",
					));
				}
				if !InstanceMetadataCount::<T>::contains_key(instance) {
					return Err(TryRuntimeError::Other(
						"live instance has no metadata count entry",
					));
				}
				if !matches!(
					NftsByOwner::<T>::get(&owner),
					Some(nft) if nft.instance == instance
				) {
					return Err(TryRuntimeError::Other(
						"Instances entry has no matching NftsByOwner entry",
					));
				}
			}

			for (instance, deposit) in InstanceDeposits::<T>::iter() {
				if instance >= next_instance {
					return Err(TryRuntimeError::Other(
						"InstanceDeposits identifier is not below NextInstanceId",
					));
				}
				if !Instances::<T>::contains_key(instance) {
					return Err(TryRuntimeError::Other(
						"InstanceDeposits entry has no matching instance",
					));
				}
				let owner = Instances::<T>::get(instance).ok_or(TryRuntimeError::Other(
					"InstanceDeposits entry has no matching owner",
				))?;
				let nft = NftsByOwner::<T>::get(owner)
					.ok_or(TryRuntimeError::Other("InstanceDeposits entry has no matching NFT"))?;
				let expected = expected_collection_deposits.get_mut(&nft.collection).ok_or(
					TryRuntimeError::Other(
						"InstanceDeposits entry has no collection deposit aggregate",
					),
				)?;
				*expected = expected
					.checked_add(&deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
			}

			let mut actual_instance_metadata_counts = BTreeMap::<InstanceId, u32>::new();
			for (instance, _, entry) in InstanceMetadata::<T>::iter() {
				if instance >= next_instance {
					return Err(TryRuntimeError::Other(
						"InstanceMetadata identifier is not below NextInstanceId",
					));
				}
				let owner = Instances::<T>::get(instance).ok_or(TryRuntimeError::Other(
					"InstanceMetadata entry has no matching instance",
				))?;
				let nft = NftsByOwner::<T>::get(owner)
					.ok_or(TryRuntimeError::Other("InstanceMetadata entry has no matching NFT"))?;
				if nft.instance != instance {
					return Err(TryRuntimeError::Other(
						"InstanceMetadata entry resolves to a different NFT",
					));
				}
				let count = actual_instance_metadata_counts.entry(instance).or_default();
				*count = count
					.checked_add(1)
					.ok_or(TryRuntimeError::Other("instance metadata count overflowed"))?;
				if *count > T::MaxInstanceMetadata::get() {
					return Err(TryRuntimeError::Other(
						"instance metadata count exceeds configured maximum",
					));
				}
				let expected = expected_collection_deposits.get_mut(&nft.collection).ok_or(
					TryRuntimeError::Other("InstanceMetadata entry has no deposit aggregate"),
				)?;
				*expected = expected
					.checked_add(&entry.deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
			}

			for (instance, declared_count) in InstanceMetadataCount::<T>::iter() {
				if !Instances::<T>::contains_key(instance) {
					return Err(TryRuntimeError::Other(
						"InstanceMetadataCount entry has no matching instance",
					));
				}
				let actual_count =
					actual_instance_metadata_counts.get(&instance).copied().unwrap_or_default();
				if declared_count != actual_count {
					return Err(TryRuntimeError::Other(
						"instance metadata count does not match stored entries",
					));
				}
				if declared_count > T::MaxInstanceMetadata::get() {
					return Err(TryRuntimeError::Other(
						"instance metadata count exceeds configured maximum",
					));
				}
			}

			for (owner, lock) in Locked::<T>::iter() {
				if !NftsByOwner::<T>::contains_key(owner) {
					return Err(TryRuntimeError::Other("Locked entry has no matching NFT"));
				}
				if lock.retries == 0 {
					return Err(TryRuntimeError::Other("Locked retry count must begin at one"));
				}
			}

			let mut actual_collection_metadata_counts = BTreeMap::<CollectionId, u32>::new();
			for (collection, _, entry) in CollectionMetadata::<T>::iter() {
				if !Collections::<T>::contains_key(collection) {
					return Err(TryRuntimeError::Other(
						"CollectionMetadata entry has no matching collection",
					));
				}
				let count = actual_collection_metadata_counts.entry(collection).or_default();
				*count = count
					.checked_add(1)
					.ok_or(TryRuntimeError::Other("collection metadata count overflowed"))?;
				let expected = expected_collection_deposits.get_mut(&collection).ok_or(
					TryRuntimeError::Other("CollectionMetadata entry has no deposit aggregate"),
				)?;
				*expected = expected
					.checked_add(&entry.deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
			}
			for (collection, info) in Collections::<T>::iter() {
				let actual =
					actual_collection_metadata_counts.get(&collection).copied().unwrap_or_default();
				if info.metadata_count != actual {
					return Err(TryRuntimeError::Other(
						"collection metadata count does not match stored entries",
					));
				}
			}

			let mut actual_item_metadata_counts = BTreeMap::<(CollectionId, ItemIndex), u32>::new();
			for ((collection, item, _), entry) in ItemMetadata::<T>::iter() {
				if !Collections::<T>::contains_key(collection) {
					return Err(TryRuntimeError::Other(
						"ItemMetadata entry has no matching collection",
					));
				}
				if !ItemDefs::<T>::contains_key(collection, item) {
					return Err(TryRuntimeError::Other(
						"ItemMetadata entry has no matching item definition",
					));
				}
				let count = actual_item_metadata_counts.entry((collection, item)).or_default();
				*count = count
					.checked_add(1)
					.ok_or(TryRuntimeError::Other("item metadata count overflowed"))?;
				let expected = expected_collection_deposits
					.get_mut(&collection)
					.ok_or(TryRuntimeError::Other("ItemMetadata entry has no deposit aggregate"))?;
				*expected = expected
					.checked_add(&entry.deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
			}
			for (collection, item, definition) in ItemDefs::<T>::iter() {
				let actual = actual_item_metadata_counts
					.get(&(collection, item))
					.copied()
					.unwrap_or_default();
				if definition.metadata_count != actual {
					return Err(TryRuntimeError::Other(
						"item metadata count does not match stored entries",
					));
				}
			}

			let mut expected_owner_holds = BTreeMap::<T::AccountId, BalanceOf<T>>::new();
			for (collection, info) in Collections::<T>::iter() {
				let expected = expected_collection_deposits
					.get(&collection)
					.ok_or(TryRuntimeError::Other("collection has no deposit aggregate"))?;
				if info.owner_deposit != *expected {
					return Err(TryRuntimeError::Other(
						"collection owner deposit does not match its stored components",
					));
				}
				let owner_total = expected_owner_holds.entry(info.owner).or_insert_with(Zero::zero);
				*owner_total = owner_total.checked_add(expected).ok_or(TryRuntimeError::Other(
					"collection owner deposit aggregate overflowed",
				))?;
			}

			for (owner, expected) in expected_owner_holds {
				if T::Currency::balance_on_hold(&Self::hold_reason(), &owner) != expected {
					return Err(TryRuntimeError::Other(
						"collection owner deposit does not match held balance",
					));
				}
			}

			Ok(())
		}
	}
}
