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
//! semantics belong to collection contracts.
//!
//! The current collection owner backs all collection state with one aggregate balance hold. To
//! transfer that responsibility safely, the owner first nominates a successor and the successor
//! claims the collection. Claiming atomically holds the exact aggregate from the successor,
//! releases it to the former owner, and transfers collection authority. This lets the successor
//! reject an unwanted or unaffordable collection.
//!
//! Collection metadata supplies defaults inherited by every item. An item's complete metadata
//! picture is built by iterating both storage prefixes and merging the item's entries over the
//! collection entries; the item value wins when both scopes contain the same key.
//! [`Pallet::metadata_of`] performs that resolution for one key, while
//! [`Pallet::collection_metadata_of`] reads only the collection scope.
//!
//! Keys and values are bounded raw bytes. Numeric metadata is a convention rather than a pallet
//! type: games should use SCALE-encoded `u128` values when they need a shared numeric convention,
//! and must decode and validate those bytes themselves.
//!
//! Transfers are feeless when authorized through the [`AsScarcity`](extension::AsScarcity)
//! transaction extension. Their transaction priority is the time since the NFT last moved,
//! capped by the runtime. Moving an NFT consumes it from the old purse key and places it at the
//! new one. Each authorization names the permanent instance and its current state nonce, which a
//! successful transfer increments. Failed dispatch restores the NFT without advancing the nonce
//! and temporarily locks the purse key.

#![cfg_attr(not(feature = "std"), no_std)]

extern crate alloc;

pub use pallet::*;

/// Runtime-only integration for minting an NFT without an instance storage deposit.
///
/// Implementations still enforce all collection, item, supply, and ownership invariants. The
/// calling runtime pallet is responsible for bounding depositless state growth through some
/// other scarce resource or protocol rule.
pub trait MintWithoutDeposit<AccountId> {
	/// Mint an instance without creating an [`InstanceDeposits`] entry.
	fn mint_without_deposit(
		collection: CollectionId,
		item: ItemIndex,
		to: AccountId,
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
	use sp_runtime::traits::Zero;
	#[cfg(any(test, feature = "try-runtime"))]
	use sp_runtime::TryRuntimeError;
	use sp_runtime::{
		traits::{CheckedAdd, CheckedSub, Convert},
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
		/// Monotonic version of authorization-relevant state.
		///
		/// Purse-key authorizations bind to this value. Every state change that should
		/// invalidate an outstanding authorization must increment it.
		pub state_nonce: u64,
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
	/// With no collection deletion call, entries persist until the owner removes them explicitly.
	#[pallet::storage]
	pub type CollectionMetadata<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		CollectionId,
		Blake2_128Concat,
		MetadataKeyOf<T>,
		MetadataEntry<MetadataValueOf<T>, BalanceOf<T>>,
	>;

	/// Per-item entries which override collection defaults for the same key.
	///
	/// Item definitions outlive minted instances, so burning an instance never removes these
	/// entries. With no item deletion call, entries persist until the owner removes them.
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

	/// Post-failure backoff locks for NFT purse keys.
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
		/// An immutable item definition was assigned within a collection.
		ItemDefined { collection: CollectionId, item: ItemIndex },
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
		/// The destination purse key already holds an NFT.
		AddressOccupied,
		/// The permanent instance identifier space is exhausted.
		TooManyInstances,
		/// An item definition's minted supply is exhausted.
		SupplyOverflow,
		/// An NFT cannot be transferred to its current purse key.
		SelfTransfer,
		/// The NFT's authorization state can no longer be advanced.
		StateNonceOverflow,
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

		/// Define one immutable item in a collection owned by the signer.
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
		#[pallet::call_index(2)]
		#[pallet::weight(T::WeightInfo::mint())]
		pub fn mint(
			origin: OriginFor<T>,
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_mint(owner, collection, item, to).map(|_| ())
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

			let state_nonce =
				nft.state_nonce.checked_add(1).ok_or(Error::<T>::StateNonceOverflow)?;
			let nft = Nft { state_nonce, last_moved: T::UnixTime::now().as_secs(), ..nft };
			NftsByOwner::<T>::insert(&to, nft.clone());
			Instances::<T>::insert(nft.instance, &to);
			Self::deposit_event(Event::Transferred { instance: nft.instance, to });
			Ok(Pays::No.into())
		}

		/// Burn an NFT held by the `Origin::Nft` purse-key origin.
		///
		/// Item-definition supply counts minted-ever instances and is deliberately monotonic.
		/// Item metadata belongs to the definition, not this instance, and remains untouched.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::burn())]
		#[transactional]
		pub fn burn(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let Ok(Origin::Nft { nft, .. }) = origin.into() else {
				return Err(DispatchError::BadOrigin.into());
			};
			Instances::<T>::remove(nft.instance);
			if let Some(deposit) = InstanceDeposits::<T>::take(nft.instance) {
				Collections::<T>::try_mutate(nft.collection, |maybe_info| {
					let info = maybe_info.as_mut().ok_or(Error::<T>::UnknownCollection)?;
					Self::decrease_owner_deposit(info, deposit)
				})?;
			}
			Self::deposit_event(Event::Burned { instance: nft.instance });
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

		/// Set or remove an item-level metadata override.
		///
		/// Only the collection owner may mutate metadata. `None` releases an existing entry's
		/// deposit; when the key is absent it is a successful no-op.
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
			Self::hold(&info.owner, amount)?;
			info.owner_deposit = total;
			Ok(())
		}

		fn decrease_owner_deposit(
			info: &mut CollectionInfo<T::AccountId, BalanceOf<T>>,
			amount: BalanceOf<T>,
		) -> DispatchResult {
			let total =
				info.owner_deposit.checked_sub(&amount).ok_or(ArithmeticError::Underflow)?;
			Self::release(&info.owner, amount)?;
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
			let footprint = Footprint::from_mel::<ItemDefinition<BalanceOf<T>>>();
			let deposit = T::ItemDeposit::convert(footprint);
			Self::increase_owner_deposit(&mut info, deposit)?;
			let definition = ItemDefinition { supply: 0, deposit };

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
				ensure!(ItemDefs::<T>::contains_key(collection, item), Error::<T>::UnknownItem);

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
						let deposit =
							T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value));
						Self::increase_owner_deposit(info, deposit)?;
						ItemMetadata::<T>::insert(
							(collection, item, key.clone()),
							MetadataEntry { value, deposit },
						);
						Self::deposit_event(Event::ItemMetadataSet { collection, item, key });
					},
					(Some(entry), None) => {
						Self::decrease_owner_deposit(info, entry.deposit)?;
						ItemMetadata::<T>::remove((collection, item, key.clone()));
						Self::deposit_event(Event::ItemMetadataRemoved { collection, item, key });
					},
					(None, None) => {},
				}
				Ok(())
			})
		}

		/// Mint an instance after enforcing collection ownership and the one-NFT-per-key rule.
		pub fn do_mint(
			owner: T::AccountId,
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
		) -> Result<InstanceId, DispatchError> {
			let info = Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == owner, Error::<T>::NoPermission);
			Self::do_mint_inner(collection, item, to, true)
		}

		#[transactional]
		fn do_mint_inner(
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
			with_deposit: bool,
		) -> Result<InstanceId, DispatchError> {
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			let mut definition =
				ItemDefs::<T>::get(collection, item).ok_or(Error::<T>::UnknownItem)?;
			ensure!(!NftsByOwner::<T>::contains_key(&to), Error::<T>::AddressOccupied);

			let instance = NextInstanceId::<T>::get();
			let next_instance = instance.checked_add(1).ok_or(Error::<T>::TooManyInstances)?;
			let next_supply = definition.supply.checked_add(1).ok_or(Error::<T>::SupplyOverflow)?;
			let now = T::UnixTime::now().as_secs();
			let nft =
				Nft { instance, collection, item, state_nonce: 0, minted_at: now, last_moved: now };
			let instance_deposit = if with_deposit {
				// Three storage entries back one instance: `NftsByOwner`, the `Instances`
				// reverse index, and the `InstanceDeposits` amount itself.
				let record_size = nft
					.encoded_size()
					.saturating_add(to.encoded_size())
					.saturating_add(instance.encoded_size())
					.saturating_add(BalanceOf::<T>::max_encoded_len());
				let deposit = T::InstanceDeposit::convert(Footprint::from_parts(3, record_size));
				Self::increase_owner_deposit(&mut info, deposit)?;
				Some(deposit)
			} else {
				None
			};

			definition.supply = next_supply;
			NextInstanceId::<T>::put(next_instance);
			Collections::<T>::insert(collection, info);
			ItemDefs::<T>::insert(collection, item, definition);
			NftsByOwner::<T>::insert(&to, nft);
			Instances::<T>::insert(instance, &to);
			if let Some(deposit) = instance_deposit {
				InstanceDeposits::<T>::insert(instance, deposit);
			}
			Self::deposit_event(Event::Minted { instance, collection, item, owner: to });
			Ok(instance)
		}
	}

	impl<T: Config> crate::MintWithoutDeposit<T::AccountId> for Pallet<T> {
		fn mint_without_deposit(
			collection: CollectionId,
			item: ItemIndex,
			to: T::AccountId,
		) -> Result<InstanceId, DispatchError> {
			ensure!(Collections::<T>::contains_key(collection), Error::<T>::UnknownCollection);
			Self::do_mint_inner(collection, item, to, false)
		}
	}

	#[cfg(any(test, feature = "try-runtime"))]
	impl<T: Config> Pallet<T> {
		/// Check allocation counters, catalogue references, ownership indexes, deposits, locks,
		/// and metadata references.
		pub(crate) fn do_try_state() -> Result<(), TryRuntimeError> {
			let next_collection = NextCollectionId::<T>::get();
			let next_instance = NextInstanceId::<T>::get();
			let mut collection_count = 0u64;
			let mut expected_collection_deposits = BTreeMap::<CollectionId, BalanceOf<T>>::new();

			for (collection, info) in Collections::<T>::iter() {
				if collection >= next_collection {
					return Err(TryRuntimeError::Other(
						"collection identifier is not below NextCollectionId",
					));
				}
				collection_count += 1;

				let mut item_count = 0u64;
				for (item, _) in ItemDefs::<T>::iter_prefix(collection) {
					if item >= info.next_item_index {
						return Err(TryRuntimeError::Other(
							"item index is not below the collection's next item index",
						));
					}
					item_count += 1;
				}
				if item_count != u64::from(info.next_item_index) {
					return Err(TryRuntimeError::Other(
						"collection next item index does not match its item definition count",
					));
				}
				expected_collection_deposits.insert(collection, info.collection_deposit);
			}

			if collection_count != u64::from(next_collection) {
				return Err(TryRuntimeError::Other(
					"NextCollectionId does not match the collection count",
				));
			}

			let mut total_supply = 0u128;
			for (collection, item, definition) in ItemDefs::<T>::iter() {
				let info = Collections::<T>::get(collection)
					.ok_or(TryRuntimeError::Other("ItemDefs entry has no matching collection"))?;
				if item >= info.next_item_index {
					return Err(TryRuntimeError::Other(
						"item index is not below the collection's next item index",
					));
				}
				total_supply += u128::from(definition.supply);
				let expected = expected_collection_deposits
					.get_mut(&collection)
					.ok_or(TryRuntimeError::Other("ItemDefs entry has no deposit aggregate"))?;
				*expected = expected
					.checked_add(&definition.deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
			}
			if total_supply != u128::from(next_instance) {
				return Err(TryRuntimeError::Other(
					"NextInstanceId does not match total minted supply",
				));
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
				if *live > u64::from(definition.supply) {
					return Err(TryRuntimeError::Other(
						"item definition supply is below its live instance count",
					));
				}
			}

			for (instance, owner) in Instances::<T>::iter() {
				if instance >= next_instance {
					return Err(TryRuntimeError::Other(
						"Instances identifier is not below NextInstanceId",
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

			for owner in Locked::<T>::iter_keys() {
				if !NftsByOwner::<T>::contains_key(owner) {
					return Err(TryRuntimeError::Other("Locked entry has no matching NFT"));
				}
			}

			for (collection, _, entry) in CollectionMetadata::<T>::iter() {
				if !Collections::<T>::contains_key(collection) {
					return Err(TryRuntimeError::Other(
						"CollectionMetadata entry has no matching collection",
					));
				}
				let expected = expected_collection_deposits.get_mut(&collection).ok_or(
					TryRuntimeError::Other("CollectionMetadata entry has no deposit aggregate"),
				)?;
				*expected = expected
					.checked_add(&entry.deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
			}

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
				let expected = expected_collection_deposits
					.get_mut(&collection)
					.ok_or(TryRuntimeError::Other("ItemMetadata entry has no deposit aggregate"))?;
				*expected = expected
					.checked_add(&entry.deposit)
					.ok_or(TryRuntimeError::Other("collection deposit aggregate overflowed"))?;
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
