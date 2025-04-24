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
	asset_strategies::{Attribute, WithCollectionConfig},
	Collection as CollectionStorage, *,
};
use frame_support::{
	ensure,
	traits::{
		tokens::asset_ops::{
			common_strategies::{
				Bytes, CheckOrigin, CheckState, ConfigValue, IfOwnedBy, Owner, WithConfig,
				WithWitness,
			},
			AssetDefinition, Create, Destroy, Inspect,
		},
		EnsureOrigin, Get,
	},
	BoundedSlice,
};
use frame_system::ensure_signed;
use sp_runtime::{DispatchError, DispatchResult};

pub struct Collection<PalletInstance>(PhantomData<PalletInstance>);

impl<T: Config<I>, I: 'static> AssetDefinition for Collection<Pallet<T, I>> {
	type Id = T::CollectionId;
}

impl<T: Config<I>, I: 'static> Inspect<Owner<T::AccountId>> for Collection<Pallet<T, I>> {
	fn inspect(
		collection: &Self::Id,
		_ownership: Owner<T::AccountId>,
	) -> Result<T::AccountId, DispatchError> {
		CollectionStorage::<T, I>::get(collection)
			.map(|a| a.owner)
			.ok_or(Error::<T, I>::UnknownCollection.into())
	}
}

impl<T: Config<I>, I: 'static> Inspect<Bytes> for Collection<Pallet<T, I>> {
	fn inspect(collection: &Self::Id, _bytes: Bytes) -> Result<Vec<u8>, DispatchError> {
		CollectionMetadataOf::<T, I>::get(collection)
			.map(|m| m.data.into())
			.ok_or(Error::<T, I>::NoMetadata.into())
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Bytes<Attribute<'a>>> for Collection<Pallet<T, I>> {
	fn inspect(
		collection: &Self::Id,
		strategy: Bytes<Attribute>,
	) -> Result<Vec<u8>, DispatchError> {
		let Bytes(Attribute(attribute)) = strategy;

		let attribute =
			BoundedSlice::try_from(attribute).map_err(|_| Error::<T, I>::WrongAttribute)?;
		crate::Attribute::<T, I>::get((collection, Option::<T::ItemId>::None, attribute))
			.map(|a| a.0.into())
			.ok_or(Error::<T, I>::AttributeNotFound.into())
	}
}

impl<T: Config<I>, I: 'static> Create<WithCollectionConfig<T, I>> for Collection<Pallet<T, I>> {
	fn create(strategy: WithCollectionConfig<T, I>) -> Result<T::CollectionId, DispatchError> {
		let WithConfig { config, extra: id_assignment } = strategy;
		let collection = id_assignment.params;
		let (ConfigValue(owner), ConfigValue(admin)) = config;

		<Pallet<T, I>>::do_create_collection(
			collection.clone(),
			owner.clone(),
			admin.clone(),
			T::CollectionDeposit::get(),
			false,
			Event::Created { collection: collection.clone(), creator: owner, owner: admin },
		)?;

		Ok(collection)
	}
}

impl<T: Config<I>, I: 'static> Create<CheckOrigin<T::RuntimeOrigin, WithCollectionConfig<T, I>>>
	for Collection<Pallet<T, I>>
{
	fn create(
		strategy: CheckOrigin<T::RuntimeOrigin, WithCollectionConfig<T, I>>,
	) -> Result<T::CollectionId, DispatchError> {
		let CheckOrigin(origin, creation) = strategy;

		let WithConfig { config, extra: id_assignment } = &creation;
		let collection = &id_assignment.params;
		let (ConfigValue(owner), ..) = config;

		let maybe_check_signer =
			T::ForceOrigin::try_origin(origin).map(|_| None).or_else(|origin| {
				T::CreateOrigin::ensure_origin(origin, collection)
					.map(Some)
					.map_err(DispatchError::from)
			})?;

		if let Some(signer) = maybe_check_signer {
			ensure!(signer == *owner, Error::<T, I>::NoPermission);
		}

		Self::create(creation)
	}
}

impl<T: Config<I>, I: 'static> Destroy<WithWitness<DestroyWitness>> for Collection<Pallet<T, I>> {
	fn destroy(collection: &Self::Id, strategy: WithWitness<DestroyWitness>) -> DispatchResult {
		let CheckState(witness, _) = strategy;

		<Pallet<T, I>>::do_destroy_collection(collection.clone(), witness, None).map(|_witness| ())
	}
}

impl<T: Config<I>, I: 'static> Destroy<IfOwnedBy<T::AccountId, WithWitness<DestroyWitness>>>
	for Collection<Pallet<T, I>>
{
	fn destroy(
		collection: &Self::Id,
		strategy: IfOwnedBy<T::AccountId, WithWitness<DestroyWitness>>,
	) -> DispatchResult {
		let CheckState(owner, CheckState(witness, _)) = strategy;

		<Pallet<T, I>>::do_destroy_collection(collection.clone(), witness, Some(owner))
			.map(|_witness| ())
	}
}

impl<T: Config<I>, I: 'static> Destroy<CheckOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>>
	for Collection<Pallet<T, I>>
{
	fn destroy(
		collection: &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>,
	) -> DispatchResult {
		let CheckOrigin(origin, CheckState(witness, _)) = strategy;

		let maybe_check_owner = match T::ForceOrigin::try_origin(origin) {
			Ok(_) => None,
			Err(origin) => Some(ensure_signed(origin)?),
		};

		<Pallet<T, I>>::do_destroy_collection(collection.clone(), witness, maybe_check_owner)
			.map(|_witness| ())
	}
}
