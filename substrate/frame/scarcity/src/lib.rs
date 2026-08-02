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
//! semantics and access-control policy can live in a higher-level runtime pallet or collection
//! contract. At this storage layer the collection owner has full control over its definitions,
//! metadata, and live instances, including force-transfer and force-burn. A runtime must
//! separately expose Scarcity calls to its chosen contract environment; this crate does not
//! provide a contract adapter.
//!
//! Purse keys are coinage-style receiving addresses, not identities: the pallet applies no
//! destination consent. Any collection owner can mint into — or force-transfer an instance to —
//! any empty purse key, and because each key holds at most one NFT, an unsolicited instance
//! blocks that key from receiving anything else until its holder burns it or transfers it away.
//! Holders should treat purse keys as disposable, minting to fresh keys they control, and
//! runtimes or contracts that need receive-consent or long-lived well-known destinations must
//! enforce that policy above this storage layer.
//!
//! The current collection owner backs all collection state with one aggregate consideration
//! ticket. To transfer that responsibility safely, the owner first nominates a successor and the
//! successor claims the collection. Claiming atomically creates an equivalent ticket for the
//! successor, drops the former owner's ticket, and transfers collection authority. This lets the
//! successor reject an unwanted or unaffordable collection while allowing the runtime to choose
//! how storage consideration is implemented.
//!
//! Footprints count logical records and their encoded payloads rather than exact trie keys and
//! hash prefixes. A runtime can account for backend overhead in its per-record base price and
//! calibrate its byte price to the desired storage policy.
//!
//! Cleanup proceeds from leaves to roots so every call remains bounded. The collection owner
//! force-burns live instances (or holders burn their own), removes item metadata, deletes empty
//! item definitions, removes collection metadata, and finally deletes the empty collection.
//! Instance metadata is bounded and removed automatically on burn. Allocated identifiers are
//! never reused.
//!
//! Collection metadata supplies defaults inherited by every item definition, and item metadata
//! supplies defaults shared by every instance minted from that definition. A minted instance may
//! override either scope without affecting other instances. [`Pallet::instance_metadata_of`]
//! resolves one key in instance, item, then collection order. [`Pallet::item_metadata_of`]
//! resolves the item and collection scopes, while [`Pallet::collection_metadata_of`] reads only
//! the collection scope.
//!
//! Keys and values are bounded raw bytes. Numeric metadata is a convention rather than a pallet
//! type: games should use SCALE-encoded `u128` values when they need a shared numeric convention,
//! and must decode and validate those bytes themselves.
//!
//! Transfers are feeless when authorized through the [`AsScarcity`](extension::AsScarcity)
//! transaction extension. Their transaction priority is the time since the NFT last moved,
//! capped by the runtime. Moving an NFT consumes it from the old purse key and places it at the
//! new one. Each authorization names the permanent instance and its current state nonce. The state
//! nonce invalidates an authorization whenever that instance moves, including collection-owner
//! force-transfers away from and back to the same purse. Following Coinage's purse model,
//! [`AsScarcity`](extension::AsScarcity) replaces the signed origin before ordinary account checks,
//! so an NFT-only purse does not need a System account. Failed dispatch restores the NFT and
//! temporarily locks the purse key; after the lock expires, the same signed transaction may be
//! submitted again if its NFT state is still current. Callers must sign mortal transactions with
//! an era shorter than [`Config::LockPeriod`] so that retrying is always a fresh signing
//! decision; see the [replay and mortality rules](extension#replay-and-mortality).

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
	use frame_support::{
		pallet_prelude::*,
		traits::{tokens::Balance, Consideration, Footprint, IsSubType, UnixTime},
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
	pub type BalanceOf<T> = <T as Config>::Balance;
	pub type CollectionInfoOf<T> = CollectionInfo<
		<T as frame_system::Config>::AccountId,
		BalanceOf<T>,
		<T as Config>::Consideration,
	>;
	pub type MetadataKeyOf<T> = BoundedVec<u8, <T as Config>::MaxKeyLen>;
	pub type MetadataValueOf<T> = BoundedVec<u8, <T as Config>::MaxValueLen>;

	/// One stored metadata entry and the deposit backing it.
	#[derive(
		Clone, PartialEq, Eq, Debug, Encode, Decode, DecodeWithMemTracking, TypeInfo, MaxEncodedLen,
	)]
	pub struct MetadataEntry<Value, Balance> {
		pub value: Value,
		/// Exact contribution to the collection's aggregate consideration.
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
	#[scale_info(skip_type_params(StorageConsideration))]
	pub struct CollectionInfo<AccountId: Member, Balance: Member, StorageConsideration: Member> {
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
		/// Exact aggregate deposit represented by `consideration`.
		pub owner_deposit: Balance,
		/// Opaque consideration ticket backing `owner_deposit` on behalf of `owner`.
		pub consideration: StorageConsideration,
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
		/// Exact storage deposit included in the collection's aggregate consideration.
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
		/// Monotonic ownership-state revision, incremented by every successful transfer.
		///
		/// Purse-key authorizations bind to this value so moving an instance away and back cannot
		/// revive an authorization created for its earlier ownership state.
		pub state_nonce: u64,
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
	pub type Collections<T: Config> = StorageMap<
		_,
		Twox64Concat,
		CollectionId,
		CollectionInfo<T::AccountId, BalanceOf<T>, T::Consideration>,
	>;

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
		/// A holder moved an instance from one purse key to another.
		Transferred { instance: InstanceId, from: T::AccountId, to: T::AccountId },
		/// A collection owner forced an instance from one purse key to another.
		ForceTransferred { instance: InstanceId, from: T::AccountId, to: T::AccountId },
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
		/// An NFT has exhausted its ownership-state nonce.
		StateNonceOverflow,
	}

	/// Hold reason available to runtimes which back [`Config::Consideration`] with fungible holds.
	#[pallet::composite_enum]
	pub enum HoldReason {
		/// Funds held for a collection's aggregate storage consideration.
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

		/// Unit in which the pallet calculates aggregate storage deposits.
		type Balance: Balance;

		/// Cost mechanism backing one collection's aggregate storage deposit.
		///
		/// The input is the exact sum produced by the deposit converters below. Keeping one
		/// ticket per collection makes ownership handoff constant-time without constraining the
		/// runtime to fungible balance holds.
		type Consideration: Consideration<Self::AccountId, BalanceOf<Self>>;

		/// Price of a collection record.
		type CollectionDeposit: Convert<Footprint, BalanceOf<Self>>;

		/// Price of an item definition.
		type ItemDeposit: Convert<Footprint, BalanceOf<Self>>;

		/// Price of one ordinary minted instance.
		type InstanceDeposit: Convert<Footprint, BalanceOf<Self>>;

		/// Price of one metadata entry.
		///
		/// The footprint contains one logical record plus the encoded key and value. Trie-key and
		/// hash-prefix overhead can be priced through the converter's per-record base.
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
		/// The destination gives no consent; any empty key is a valid target. See the module
		/// documentation on purse-key occupancy.
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
		#[pallet::feeless_if(|origin: &OriginFor<T>, _to: &T::AccountId| -> bool {
			Pallet::<T>::is_nft_origin(origin)
		})]
		pub fn transfer(origin: OriginFor<T>, to: T::AccountId) -> DispatchResultWithPostInfo {
			let Ok(Origin::Nft { owner, nft }) = origin.into() else {
				return Err(DispatchError::BadOrigin.into());
			};
			ensure!(to != owner, Error::<T>::SelfTransfer);
			ensure!(!NftsByOwner::<T>::contains_key(&to), Error::<T>::AddressOccupied);

			let from = owner;
			let state_nonce =
				nft.state_nonce.checked_add(1).ok_or(Error::<T>::StateNonceOverflow)?;
			let nft = Nft { last_moved: T::UnixTime::now().as_secs(), state_nonce, ..nft };
			NftsByOwner::<T>::insert(&to, nft.clone());
			Instances::<T>::insert(nft.instance, &to);
			Self::deposit_event(Event::Transferred { instance: nft.instance, from, to });
			Ok(Pays::No.into())
		}

		/// Burn an NFT held by the `Origin::Nft` purse-key origin.
		///
		/// Item-definition supply counts minted-ever instances and is deliberately monotonic.
		/// Item metadata belongs to the definition and remains untouched. Instance metadata is
		/// removed and its exact deposits are released.
		#[pallet::call_index(4)]
		#[pallet::weight(T::WeightInfo::burn(T::MaxInstanceMetadata::get()))]
		#[pallet::feeless_if(|origin: &OriginFor<T>| -> bool {
			Pallet::<T>::is_nft_origin(origin)
		})]
		#[transactional]
		pub fn burn(origin: OriginFor<T>) -> DispatchResultWithPostInfo {
			let Ok(Origin::Nft { owner: _, nft }) = origin.into() else {
				return Err(DispatchError::BadOrigin.into());
			};
			let metadata_count = Self::do_burn(nft)?;
			Ok((Some(T::WeightInfo::burn(metadata_count)), Pays::No).into())
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
		/// An equivalent aggregate consideration is first created for the claimant and then the
		/// previous owner's ticket is dropped. The operation is atomic: failure to establish the
		/// claimant's consideration leaves ownership and both tickets unchanged.
		#[pallet::call_index(8)]
		#[pallet::weight(T::WeightInfo::claim_collection_ownership())]
		#[transactional]
		pub fn claim_collection_ownership(
			origin: OriginFor<T>,
			collection: CollectionId,
		) -> DispatchResult {
			let new_owner = ensure_signed(origin)?;
			let info = Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(
				info.pending_owner.as_ref() == Some(&new_owner),
				Error::<T>::NotPendingCollectionOwner
			);

			let old_owner = info.owner.clone();
			let info = Self::change_collection_owner(info, new_owner.clone())?;
			Collections::<T>::insert(collection, info);
			Self::deposit_event(Event::CollectionOwnerChanged { collection, old_owner, new_owner });
			Ok(())
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

		/// Force-burn one live instance as its collection owner.
		///
		/// The collection layer intentionally applies no holder-level ACL. When a runtime exposes
		/// this call to its contract environment, a contract-owned collection can enforce its own
		/// consent and game rules before calling it.
		#[pallet::call_index(10)]
		#[pallet::weight(T::WeightInfo::force_burn(T::MaxInstanceMetadata::get()))]
		#[transactional]
		pub fn force_burn(
			origin: OriginFor<T>,
			instance: InstanceId,
		) -> DispatchResultWithPostInfo {
			let owner = ensure_signed(origin)?;
			let metadata_count = Self::do_force_burn(&owner, instance)?;
			Ok(Some(T::WeightInfo::force_burn(metadata_count)).into())
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

		/// Force-transfer one live instance as its collection owner.
		///
		/// The collection layer intentionally applies no holder-level ACL. When a runtime exposes
		/// this call to its contract environment, a contract-owned collection can enforce its own
		/// consent and game rules before calling it. The move increments the instance state nonce,
		/// invalidating prior holder authorizations.
		#[pallet::call_index(13)]
		#[pallet::weight(T::WeightInfo::force_transfer())]
		#[transactional]
		pub fn force_transfer(
			origin: OriginFor<T>,
			instance: InstanceId,
			to: T::AccountId,
		) -> DispatchResult {
			let owner = ensure_signed(origin)?;
			Self::do_force_transfer(&owner, instance, to)
		}
	}

	impl<T: Config> Pallet<T> {
		fn is_nft_origin(origin: &OriginFor<T>) -> bool {
			let converted: Result<Origin<T>, OriginFor<T>> = origin.clone().into();
			matches!(converted, Ok(Origin::Nft { .. }))
		}

		fn increase_owner_deposit(
			info: CollectionInfoOf<T>,
			amount: BalanceOf<T>,
		) -> Result<CollectionInfoOf<T>, DispatchError> {
			let total = info.owner_deposit.checked_add(&amount).ok_or(ArithmeticError::Overflow)?;
			Self::update_owner_deposit(info, total)
		}

		fn decrease_owner_deposit(
			info: CollectionInfoOf<T>,
			amount: BalanceOf<T>,
		) -> Result<CollectionInfoOf<T>, DispatchError> {
			let total =
				info.owner_deposit.checked_sub(&amount).ok_or(ArithmeticError::Underflow)?;
			Self::update_owner_deposit(info, total)
		}

		fn replace_owner_deposit(
			info: CollectionInfoOf<T>,
			old: BalanceOf<T>,
			new: BalanceOf<T>,
		) -> Result<CollectionInfoOf<T>, DispatchError> {
			let total_without_old =
				info.owner_deposit.checked_sub(&old).ok_or(ArithmeticError::Underflow)?;
			let total = total_without_old.checked_add(&new).ok_or(ArithmeticError::Overflow)?;
			Self::update_owner_deposit(info, total)
		}

		fn update_owner_deposit(
			info: CollectionInfoOf<T>,
			owner_deposit: BalanceOf<T>,
		) -> Result<CollectionInfoOf<T>, DispatchError> {
			let CollectionInfo {
				owner,
				pending_owner,
				next_item_index,
				item_count,
				metadata_count,
				collection_deposit,
				owner_deposit: _,
				consideration,
			} = info;
			let consideration = consideration.update(&owner, owner_deposit)?;
			Ok(CollectionInfo {
				owner,
				pending_owner,
				next_item_index,
				item_count,
				metadata_count,
				collection_deposit,
				owner_deposit,
				consideration,
			})
		}

		fn change_collection_owner(
			info: CollectionInfoOf<T>,
			new_owner: T::AccountId,
		) -> Result<CollectionInfoOf<T>, DispatchError> {
			let CollectionInfo {
				owner,
				pending_owner: _,
				next_item_index,
				item_count,
				metadata_count,
				collection_deposit,
				owner_deposit,
				consideration: old_consideration,
			} = info;
			// Charge the successor before refunding the former owner. The enclosing dispatchable
			// is transactional, so any failure leaves both tickets unchanged.
			let consideration = T::Consideration::new(&new_owner, owner_deposit)?;
			old_consideration.drop(&owner)?;
			Ok(CollectionInfo {
				owner: new_owner,
				pending_owner: None,
				next_item_index,
				item_count,
				metadata_count,
				collection_deposit,
				owner_deposit,
				consideration,
			})
		}

		/// Allocate a collection identifier and record its initial owner.
		pub fn do_create_collection(owner: T::AccountId) -> Result<CollectionId, DispatchError> {
			let collection = NextCollectionId::<T>::get();
			let next_collection =
				collection.checked_add(1).ok_or(Error::<T>::TooManyCollections)?;
			let footprint = Footprint::from_mel::<CollectionInfoOf<T>>();
			let collection_deposit = T::CollectionDeposit::convert(footprint);
			let consideration = T::Consideration::new(&owner, collection_deposit)?;

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
					consideration,
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
			info = Self::increase_owner_deposit(info, deposit)?;
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
		pub fn item_metadata_of(
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
				.or_else(|| Self::item_metadata_of(nft.collection, nft.item, key))
		}

		/// Only the collection-level value for `key`, without item-level resolution.
		pub fn collection_metadata_of(
			collection: CollectionId,
			key: &MetadataKeyOf<T>,
		) -> Option<MetadataValueOf<T>> {
			CollectionMetadata::<T>::get(collection, key).map(|entry| entry.value)
		}

		fn metadata_footprint(key: &MetadataKeyOf<T>, value: &MetadataValueOf<T>) -> Footprint {
			// Logical record footprint. Runtime pricing may include trie/hash overhead in its
			// per-record base rather than coupling this pallet to a storage backend.
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
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == *owner, Error::<T>::NoPermission);

			match (CollectionMetadata::<T>::get(collection, &key), value) {
				(Some(entry), Some(value)) => {
					let deposit =
						T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value));
					info = Self::replace_owner_deposit(info, entry.deposit, deposit)?;
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
					info = Self::increase_owner_deposit(info, deposit)?;
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
					info = Self::decrease_owner_deposit(info, entry.deposit)?;
					CollectionMetadata::<T>::remove(collection, &key);
					Self::deposit_event(Event::CollectionMetadataRemoved { collection, key });
				},
				(None, None) => {},
			}
			Collections::<T>::insert(collection, info);
			Ok(())
		}

		fn do_set_item_metadata(
			owner: &T::AccountId,
			collection: CollectionId,
			item: ItemIndex,
			key: MetadataKeyOf<T>,
			value: Option<MetadataValueOf<T>>,
		) -> DispatchResult {
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == *owner, Error::<T>::NoPermission);
			let mut definition =
				ItemDefs::<T>::get(collection, item).ok_or(Error::<T>::UnknownItem)?;

			match (ItemMetadata::<T>::get((collection, item, key.clone())), value) {
				(Some(entry), Some(value)) => {
					let deposit =
						T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value));
					info = Self::replace_owner_deposit(info, entry.deposit, deposit)?;
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
					info = Self::increase_owner_deposit(info, deposit)?;
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
					info = Self::decrease_owner_deposit(info, entry.deposit)?;
					ItemMetadata::<T>::remove((collection, item, key.clone()));
					ItemDefs::<T>::insert(collection, item, definition);
					Self::deposit_event(Event::ItemMetadataRemoved { collection, item, key });
				},
				(None, None) => {},
			}
			Collections::<T>::insert(collection, info);
			Ok(())
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

			let mut info =
				Collections::<T>::get(nft.collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == *owner, Error::<T>::NoPermission);

			match (InstanceMetadata::<T>::get(instance, &key), value) {
				(Some(entry), Some(value)) => {
					let deposit = if with_deposit {
						T::MetadataDeposit::convert(Self::metadata_footprint(&key, &value))
					} else {
						Zero::zero()
					};
					info = Self::replace_owner_deposit(info, entry.deposit, deposit)?;
					InstanceMetadata::<T>::insert(instance, &key, MetadataEntry { value, deposit });
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
					info = Self::increase_owner_deposit(info, deposit)?;
					InstanceMetadata::<T>::insert(instance, &key, MetadataEntry { value, deposit });
					InstanceMetadataCount::<T>::insert(instance, next_count);
					Self::deposit_event(Event::InstanceMetadataSet { instance, key });
				},
				(Some(entry), None) => {
					let count = InstanceMetadataCount::<T>::get(instance);
					let next_count = count.checked_sub(1).ok_or(ArithmeticError::Underflow)?;
					info = Self::decrease_owner_deposit(info, entry.deposit)?;
					InstanceMetadata::<T>::remove(instance, &key);
					InstanceMetadataCount::<T>::insert(instance, next_count);
					Self::deposit_event(Event::InstanceMetadataRemoved { instance, key });
				},
				(None, None) => {},
			}
			Collections::<T>::insert(nft.collection, info);
			Ok(())
		}

		fn do_burn(nft: Nft) -> Result<u32, DispatchError> {
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
			let info =
				Collections::<T>::get(nft.collection).ok_or(Error::<T>::UnknownCollection)?;
			let info = Self::decrease_owner_deposit(info, deposit)?;
			Collections::<T>::insert(nft.collection, info);
			Instances::<T>::remove(instance);
			Self::deposit_event(Event::Burned { instance });
			Ok(actual_metadata_count)
		}

		fn do_force_burn(owner: &T::AccountId, instance: InstanceId) -> Result<u32, DispatchError> {
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

		fn do_force_transfer(
			owner: &T::AccountId,
			instance: InstanceId,
			to: T::AccountId,
		) -> DispatchResult {
			let from = Instances::<T>::get(instance).ok_or(Error::<T>::UnknownInstance)?;
			let nft = NftsByOwner::<T>::get(&from).ok_or(Error::<T>::UnknownInstance)?;
			ensure!(nft.instance == instance, Error::<T>::UnknownInstance);
			let info =
				Collections::<T>::get(nft.collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == *owner, Error::<T>::NoPermission);
			ensure!(to != from, Error::<T>::SelfTransfer);
			ensure!(!NftsByOwner::<T>::contains_key(&to), Error::<T>::AddressOccupied);

			let state_nonce =
				nft.state_nonce.checked_add(1).ok_or(Error::<T>::StateNonceOverflow)?;
			let nft = Nft { last_moved: T::UnixTime::now().as_secs(), state_nonce, ..nft };
			NftsByOwner::<T>::remove(&from);
			Locked::<T>::remove(&from);
			NftsByOwner::<T>::insert(&to, nft);
			Instances::<T>::insert(instance, &to);
			Self::deposit_event(Event::ForceTransferred { instance, from, to });
			Ok(())
		}

		fn do_delete_item(
			owner: &T::AccountId,
			collection: CollectionId,
			item: ItemIndex,
		) -> DispatchResult {
			let mut info =
				Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
			ensure!(info.owner == *owner, Error::<T>::NoPermission);
			let definition = ItemDefs::<T>::get(collection, item).ok_or(Error::<T>::UnknownItem)?;
			ensure!(definition.live_supply == 0, Error::<T>::ItemInUse);
			ensure!(definition.metadata_count == 0, Error::<T>::ItemMetadataNotEmpty);
			ensure!(
				ItemMetadata::<T>::iter_prefix((collection, item)).next().is_none(),
				Error::<T>::DeletionInvariant
			);

			info.item_count = info.item_count.checked_sub(1).ok_or(ArithmeticError::Underflow)?;
			info = Self::decrease_owner_deposit(info, definition.deposit)?;
			Collections::<T>::insert(collection, info);
			ItemDefs::<T>::remove(collection, item);
			Self::deposit_event(Event::ItemDeleted { collection, item });
			Ok(())
		}

		fn do_delete_collection(owner: &T::AccountId, collection: CollectionId) -> DispatchResult {
			let info = Collections::<T>::get(collection).ok_or(Error::<T>::UnknownCollection)?;
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

			let CollectionInfo { owner, consideration, .. } = info;
			consideration.drop(&owner)?;
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
			let nft =
				Nft { instance, collection, item, minted_at: now, last_moved: now, state_nonce: 0 };
			let instance_deposit = if with_deposit {
				// Four storage entries back one instance: `NftsByOwner`, the `Instances`
				// reverse index, `InstanceDeposits`, and `InstanceMetadataCount`. This measures
				// their logical encoded payload; the runtime's per-record base can price trie
				// and hash-prefix overhead.
				let record_size = nft
					.encoded_size()
					.saturating_add(to.encoded_size().saturating_mul(2))
					.saturating_add(instance.encoded_size().saturating_mul(3))
					.saturating_add(BalanceOf::<T>::max_encoded_len())
					.saturating_add(u32::max_encoded_len());
				let deposit = T::InstanceDeposit::convert(Footprint::from_parts(4, record_size));
				info = Self::increase_owner_deposit(info, deposit)?;
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

			for (collection, info) in Collections::<T>::iter() {
				let expected = expected_collection_deposits
					.get(&collection)
					.ok_or(TryRuntimeError::Other("collection has no deposit aggregate"))?;
				if info.owner_deposit != *expected {
					return Err(TryRuntimeError::Other(
						"collection owner deposit does not match its stored components",
					));
				}
			}

			Ok(())
		}
	}
}
