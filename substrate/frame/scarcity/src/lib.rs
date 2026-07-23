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
//! `pallet-scarcity` defines immutable NFT collections and item definitions, then mints instances
//! using a coinage-style ownership model: each purse key can hold at most one NFT.
//!
//! Transfers are feeless when authorized through the [`AsScarcity`](extension::AsScarcity)
//! transaction extension. Their transaction priority is the time since the NFT last moved,
//! capped by the runtime. Moving an NFT consumes it from the old purse key and places it at the
//! new one; failed dispatch restores it and temporarily locks the purse key.

#![cfg_attr(not(feature = "std"), no_std)]

pub use pallet::*;

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
	use frame_support::{
		pallet_prelude::*,
		traits::{IsSubType, UnixTime},
	};
	use frame_system::pallet_prelude::*;
	use sp_runtime::{transaction_validity::TransactionPriority, DispatchError};

	pub type CollectionId = u32;
	/// Index within a collection; `(CollectionId, ItemIndex)` names an item definition.
	pub type ItemIndex = u32;
	/// Permanent global serial, assigned at mint.
	pub type InstanceId = u64;
	pub type AttrId = u16;

	/// One typed, on-chain-comparable statistic of an item definition.
	///
	/// Encodes identically to `(AttrId, u128)`; the named fields exist for metadata legibility.
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
	pub struct Stat {
		/// Collection-defined attribute identifier; the schema is a per-collection convention.
		pub attr: AttrId,
		/// The attribute's numeric value.
		pub value: u128,
	}

	/// The display and fusion class of an immutable item definition.
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
	pub enum Kind {
		Normal,
		Special,
		Charm,
	}

	/// Collection issuer and the next automatically assigned item index.
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
	)]
	pub struct CollectionInfo<AccountId> {
		/// The issuer; v1 permits only this account to define items and mint.
		pub owner: AccountId,
		/// The next item index to allocate, beginning at zero.
		pub next_item_index: ItemIndex,
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
	#[scale_info(skip_type_params(MaxStats, MaxMetadata))]
	pub struct ItemDefinition<MaxStats: Get<u32>, MaxMetadata: Get<u32>> {
		pub kind: Kind,
		/// A same-collection link toward the rarer variant of this concept.
		pub next_variant: Option<ItemIndex>,
		/// Typed, on-chain-comparable statistics.
		pub stats: BoundedVec<Stat, MaxStats>,
		/// Opaque display metadata.
		pub metadata: BoundedVec<u8, MaxMetadata>,
		/// Number of instances minted from this definition so far.
		pub supply: u32,
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
		/// Times this instance has moved. Strictly monotonic — the authorization epoch for any
		/// detached-signature flow (sponsored transfers, profile bindings): signatures bind this
		/// value and die on the next move by construction.
		pub moves: u32,
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

	/// Immutable item catalogues, grouped by their issuer-owned collection.
	#[pallet::storage]
	pub type Collections<T: Config> =
		StorageMap<_, Twox64Concat, CollectionId, CollectionInfo<T::AccountId>>;

	/// Immutable item definitions, indexed within their collection.
	#[pallet::storage]
	pub type ItemDefs<T: Config> = StorageDoubleMap<
		_,
		Twox64Concat,
		CollectionId,
		Twox64Concat,
		ItemIndex,
		ItemDefinition<T::MaxStats, T::MaxMetadata>,
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

	/// Post-failure backoff locks for NFT purse keys.
	#[pallet::storage]
	pub type Locked<T: Config> = StorageMap<_, Blake2_128Concat, T::AccountId, LockInfo>;

	#[pallet::event]
	#[pallet::generate_deposit(pub(super) fn deposit_event)]
	pub enum Event<T: Config> {
		/// A collection was created and assigned to its issuer.
		CollectionCreated { collection: CollectionId, owner: T::AccountId },
		/// An immutable item definition was assigned within a collection.
		ItemDefined { collection: CollectionId, item: ItemIndex, kind: Kind },
		/// An instance was minted into an empty purse key.
		Minted {
			instance: InstanceId,
			collection: CollectionId,
			item: ItemIndex,
			owner: T::AccountId,
		},
		/// An instance moved to a new purse key.
		Transferred { instance: InstanceId, to: T::AccountId },
	}

	#[pallet::error]
	pub enum Error<T> {
		/// The collection identifier space is exhausted.
		TooManyCollections,
		/// The collection does not exist.
		UnknownCollection,
		/// Only the collection issuer may define items or mint in v1.
		NoPermission,
		/// The referenced rarer item definition does not exist in this collection.
		UnknownVariant,
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
	}

	#[pallet::pallet]
	pub struct Pallet<T>(_);

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

		/// Unix time source for `minted_at` / `last_moved`; rest-time priority lands in Phase 2.
		type UnixTime: UnixTime;

		/// Maximum typed statistics stored per immutable item definition.
		#[pallet::constant]
		type MaxStats: Get<u32>;

		/// Maximum opaque metadata bytes stored per immutable item definition.
		#[pallet::constant]
		type MaxMetadata: Get<u32>;

		/// Base lock period after a failed purse-key dispatch, in seconds.
		#[pallet::constant]
		type LockPeriod: Get<u64>;

		/// Priority ceiling for rested NFT transfers.
		#[pallet::constant]
		type MaxTransferPriority: Get<TransactionPriority>;
	}

	#[pallet::call]
	impl<T: Config> Pallet<T> {
		/// Create a collection owned by the signed issuer.
		#[pallet::call_index(0)]
		#[pallet::weight(T::WeightInfo::create_collection())]
		pub fn create_collection(origin: OriginFor<T>) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_create_collection(owner).map(|_| ())
		}

		/// Define one immutable item in a collection owned by the signer.
		#[pallet::call_index(1)]
		#[pallet::weight(T::WeightInfo::define_item(
			stats.len() as u32,
			metadata.len() as u32,
		))]
		pub fn define_item(
			origin: OriginFor<T>,
			collection: CollectionId,
			kind: Kind,
			next_variant: Option<ItemIndex>,
			stats: BoundedVec<Stat, T::MaxStats>,
			metadata: BoundedVec<u8, T::MaxMetadata>,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_define_item(owner, collection, kind, next_variant, stats, metadata).map(|_| ())
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

			let nft = Nft {
				last_moved: T::UnixTime::now().as_secs(),
				moves: nft.moves.saturating_add(1),
				..nft
			};
			NftsByOwner::<T>::insert(&to, nft.clone());
			Instances::<T>::insert(nft.instance, &to);
			Self::deposit_event(Event::Transferred { instance: nft.instance, to });
			Ok(Pays::No.into())
		}
	}

	impl<T: Config> Pallet<T> {
		/// Allocate a collection identifier and record its issuer.
		pub fn do_create_collection(owner: T::AccountId) -> Result<CollectionId, DispatchError> {
			let collection = NextCollectionId::<T>::get();
			let next_collection =
				collection.checked_add(1).ok_or(Error::<T>::TooManyCollections)?;

			NextCollectionId::<T>::put(next_collection);
			Collections::<T>::insert(
				collection,
				CollectionInfo { owner: owner.clone(), next_item_index: 0 },
			);
			Self::deposit_event(Event::CollectionCreated { collection, owner });
			Ok(collection)
		}

		/// Add an immutable item definition to an issuer-owned collection.
		pub fn do_define_item(
			owner: T::AccountId,
			collection: CollectionId,
			kind: Kind,
			next_variant: Option<ItemIndex>,
			stats: BoundedVec<Stat, T::MaxStats>,
			metadata: BoundedVec<u8, T::MaxMetadata>,
		) -> Result<ItemIndex, DispatchError> {
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == owner, Error::<T>::NoPermission);
			if let Some(variant) = next_variant {
				ensure!(
					ItemDefs::<T>::contains_key(collection, variant),
					Error::<T>::UnknownVariant
				);
			}

			let item = info.next_item_index;
			info.next_item_index = item.checked_add(1).ok_or(Error::<T>::TooManyItems)?;
			let definition = ItemDefinition { kind, next_variant, stats, metadata, supply: 0 };

			Collections::<T>::insert(collection, info);
			ItemDefs::<T>::insert(collection, item, definition);
			Self::deposit_event(Event::ItemDefined { collection, item, kind });
			Ok(item)
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
			let mut definition =
				ItemDefs::<T>::get(collection, item).ok_or(Error::<T>::UnknownItem)?;
			ensure!(!NftsByOwner::<T>::contains_key(&to), Error::<T>::AddressOccupied);

			let instance = NextInstanceId::<T>::get();
			let next_instance = instance.checked_add(1).ok_or(Error::<T>::TooManyInstances)?;
			let next_supply = definition.supply.checked_add(1).ok_or(Error::<T>::SupplyOverflow)?;
			let now = T::UnixTime::now().as_secs();
			let nft = Nft { instance, collection, item, minted_at: now, last_moved: now, moves: 0 };

			definition.supply = next_supply;
			NextInstanceId::<T>::put(next_instance);
			ItemDefs::<T>::insert(collection, item, definition);
			NftsByOwner::<T>::insert(&to, nft);
			Instances::<T>::insert(instance, &to);
			Self::deposit_event(Event::Minted { instance, collection, item, owner: to });
			Ok(instance)
		}
	}
}
