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

use core::marker::PhantomData;

use crate::{
	asset_strategies::{Attribute, WithItemConfig},
	Item as ItemStorage, *,
};
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::tokens::asset_ops::{
		common_strategies::{
			Bytes, CanUpdate, ChangeOwnerFrom, CheckOrigin, CheckState, ConfigValue, IfOwnedBy,
			NoParams, Owner, PredefinedId, WithConfig,
		},
		AssetDefinition, Create, Inspect, Restore, Stash, Update,
	},
	BoundedSlice,
};
use frame_system::ensure_signed;
use sp_runtime::DispatchError;

pub struct Item<PalletInstance>(PhantomData<PalletInstance>);

impl<T: Config<I>, I: 'static> AssetDefinition for Item<Pallet<T, I>> {
	type Id = (T::CollectionId, T::ItemId);
}

impl<T: Config<I>, I: 'static> Inspect<Owner<T::AccountId>> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		_ownership: Owner<T::AccountId>,
	) -> Result<T::AccountId, DispatchError> {
		ItemStorage::<T, I>::get(collection, item)
			.map(|a| a.owner)
			.ok_or(Error::<T, I>::UnknownItem.into())
	}
}

impl<T: Config<I>, I: 'static> Inspect<Bytes> for Item<Pallet<T, I>> {
	fn inspect((collection, item): &Self::Id, _bytes: Bytes) -> Result<Vec<u8>, DispatchError> {
		ItemMetadataOf::<T, I>::get(collection, item)
			.map(|m| m.data.into())
			.ok_or(Error::<T, I>::NoMetadata.into())
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Bytes<Attribute<'a>>> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		strategy: Bytes<Attribute>,
	) -> Result<Vec<u8>, DispatchError> {
		let Bytes(Attribute(attribute)) = strategy;

		let attribute =
			BoundedSlice::try_from(attribute).map_err(|_| Error::<T, I>::WrongAttribute)?;
		crate::Attribute::<T, I>::get((collection, Some(item), attribute))
			.map(|a| a.0.into())
			.ok_or(Error::<T, I>::AttributeNotFound.into())
	}
}

impl<T: Config<I>, I: 'static> Inspect<CanUpdate<Owner<T::AccountId>>> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		_can_update: CanUpdate<Owner<T::AccountId>>,
	) -> Result<bool, DispatchError> {
		match (Collection::<T, I>::get(collection), ItemStorage::<T, I>::get(collection, item)) {
			(Some(cd), Some(id)) => Ok(!cd.is_frozen && !id.is_frozen),
			_ => Err(Error::<T, I>::UnknownItem.into()),
		}
	}
}

impl<T: Config<I>, I: 'static> Create<WithItemConfig<T, I>> for Item<Pallet<T, I>> {
	fn create(
		strategy: WithItemConfig<T, I>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		let WithConfig { config: ConfigValue::<_>(owner), extra: id_assignment } = strategy;
		let (collection, item) = id_assignment.params;

		<Pallet<T, I>>::do_mint(collection.clone(), item, owner, |_| Ok(()))?;

		Ok((collection, item))
	}
}

impl<T: Config<I>, I: 'static> Create<CheckOrigin<T::RuntimeOrigin, WithItemConfig<T, I>>>
	for Item<Pallet<T, I>>
{
	fn create(
		strategy: CheckOrigin<T::RuntimeOrigin, WithItemConfig<T, I>>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		let CheckOrigin(
			origin,
			WithConfig { config: ConfigValue::<_>(owner), extra: id_assignment },
		) = strategy;
		let (collection, item) = id_assignment.params;

		let signer = ensure_signed(origin)?;

		<Pallet<T, I>>::do_mint(collection.clone(), item, owner, |collection_details| {
			ensure!(collection_details.issuer == signer, Error::<T, I>::NoPermission);
			Ok(())
		})?;

		Ok((collection, item))
	}
}

impl<T: Config<I>, I: 'static> Update<Owner<T::AccountId>> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		_strategy: Owner<T::AccountId>,
		dest: &T::AccountId,
	) -> DispatchResult {
		<Pallet<T, I>>::do_transfer(collection.clone(), *item, dest.clone(), |_, _| Ok(()))
	}
}

impl<T: Config<I>, I: 'static> Update<CheckOrigin<T::RuntimeOrigin, Owner<T::AccountId>>>
	for Item<Pallet<T, I>>
{
	fn update(
		(collection, item): &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, Owner<T::AccountId>>,
		dest: &T::AccountId,
	) -> DispatchResult {
		let CheckOrigin(origin, ..) = strategy;

		let signer = ensure_signed(origin)?;

		<Pallet<T, I>>::do_transfer(
			collection.clone(),
			*item,
			dest.clone(),
			|collection_details, details| {
				if details.owner != signer && collection_details.admin != signer {
					let approved = details.approved.take().map_or(false, |i| i == signer);
					ensure!(approved, Error::<T, I>::NoPermission);
				}
				Ok(())
			},
		)
	}
}

impl<T: Config<I>, I: 'static> Update<ChangeOwnerFrom<T::AccountId>> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		strategy: ChangeOwnerFrom<T::AccountId>,
		dest: &T::AccountId,
	) -> DispatchResult {
		let CheckState(from, ..) = strategy;

		<Pallet<T, I>>::do_transfer(collection.clone(), *item, dest.clone(), |_, details| {
			ensure!(details.owner == from, Error::<T, I>::WrongOwner);
			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static> Stash<NoParams> for Item<Pallet<T, I>> {
	fn stash((collection, item): &Self::Id, _strategy: NoParams) -> DispatchResult {
		<Pallet<T, I>>::do_burn(collection.clone(), *item, |_, _| Ok(()))
	}
}

impl<T: Config<I>, I: 'static> Stash<CheckOrigin<T::RuntimeOrigin>> for Item<Pallet<T, I>> {
	fn stash(
		(collection, item): &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin>,
	) -> DispatchResult {
		let CheckOrigin(origin, ..) = strategy;

		let signer = ensure_signed(origin)?;

		<Pallet<T, I>>::do_burn(collection.clone(), *item, |collection_details, details| {
			let is_permitted = collection_details.admin == signer || details.owner == signer;
			ensure!(is_permitted, Error::<T, I>::NoPermission);
			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static> Stash<IfOwnedBy<T::AccountId>> for Item<Pallet<T, I>> {
	fn stash((collection, item): &Self::Id, strategy: IfOwnedBy<T::AccountId>) -> DispatchResult {
		let CheckState(who, ..) = strategy;

		<Pallet<T, I>>::do_burn(collection.clone(), *item, |_, d| {
			ensure!(d.owner == who, Error::<T, I>::NoPermission);
			Ok(())
		})
	}
}

// NOTE: pallet-uniques create and restore operations are equivalent.
// If an NFT was burned, it can be "re-created" (equivalently, "restored").
// It will be "re-created" with all the data still bound to it.
// If an NFT is minted for the first time, it can be regarded as "restored" with an empty data
// because it is indistinguishable from a burned empty NFT from the chain's perspective.
impl<T: Config<I>, I: 'static> Restore<WithConfig<ConfigValue<Owner<T::AccountId>>>>
	for Item<Pallet<T, I>>
{
	fn restore(
		(collection, item): &Self::Id,
		strategy: WithConfig<ConfigValue<Owner<T::AccountId>>>,
	) -> DispatchResult {
		let item_exists = ItemStorage::<T, I>::contains_key(collection, item);
		ensure!(!item_exists, Error::<T, I>::InUse);

		Self::create(WithConfig::new(
			strategy.config,
			PredefinedId::from((collection.clone(), *item)),
		))?;

		Ok(())
	}
}
