use core::marker::PhantomData;

use crate::{asset_strategies::Attribute, Collection as CollectionStorage, *};
use frame_support::{
	ensure,
	traits::{
		tokens::asset_ops::{
			common_strategies::{
				Bytes, CheckOrigin, CheckState, IfOwnedBy, Ownership, PredefinedId, WithAdmin,
				WithWitness,
			},
			AssetDefinition, Create, Destroy, Inspect,
		},
		EnsureOrigin, Get,
	},
	BoundedSlice,
};
use frame_system::ensure_signed;
use sp_runtime::DispatchError;

pub struct Collection<PalletInstance>(PhantomData<PalletInstance>);

impl<T: Config<I>, I: 'static> AssetDefinition for Collection<Pallet<T, I>> {
	type Id = T::CollectionId;
}

impl<T: Config<I>, I: 'static> Inspect<Ownership<T::AccountId>> for Collection<Pallet<T, I>> {
	fn inspect(
		collection: &Self::Id,
		_ownership: Ownership<T::AccountId>,
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

impl<T: Config<I>, I: 'static> Create<WithAdmin<T::AccountId, PredefinedId<T::CollectionId>>>
	for Collection<Pallet<T, I>>
{
	fn create(
		strategy: WithAdmin<T::AccountId, PredefinedId<T::CollectionId>>,
	) -> Result<T::CollectionId, DispatchError> {
		let WithAdmin { owner, admin, id_assignment, .. } = strategy;
		let collection = id_assignment.params;

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

impl<T: Config<I>, I: 'static>
	Create<CheckOrigin<T::RuntimeOrigin, WithAdmin<T::AccountId, PredefinedId<T::CollectionId>>>>
	for Collection<Pallet<T, I>>
{
	fn create(
		strategy: CheckOrigin<
			T::RuntimeOrigin,
			WithAdmin<T::AccountId, PredefinedId<T::CollectionId>>,
		>,
	) -> Result<T::CollectionId, DispatchError> {
		let CheckOrigin(origin, creation) = strategy;

		let WithAdmin { owner, id_assignment, .. } = &creation;
		let collection = &id_assignment.params;

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
	fn destroy(
		collection: &Self::Id,
		strategy: WithWitness<DestroyWitness>,
	) -> Result<DestroyWitness, DispatchError> {
		let WithWitness(witness) = strategy;

		<Pallet<T, I>>::do_destroy_collection(collection.clone(), witness, None)
	}
}

impl<T: Config<I>, I: 'static> Destroy<IfOwnedBy<T::AccountId, WithWitness<DestroyWitness>>>
	for Collection<Pallet<T, I>>
{
	fn destroy(
		collection: &Self::Id,
		strategy: IfOwnedBy<T::AccountId, WithWitness<DestroyWitness>>,
	) -> Result<DestroyWitness, DispatchError> {
		let CheckState(owner, WithWitness(witness)) = strategy;

		<Pallet<T, I>>::do_destroy_collection(collection.clone(), witness, Some(owner))
	}
}

impl<T: Config<I>, I: 'static> Destroy<CheckOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>>
	for Collection<Pallet<T, I>>
{
	fn destroy(
		collection: &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>,
	) -> Result<DestroyWitness, DispatchError> {
		let CheckOrigin(origin, WithWitness(witness)) = strategy;
		let maybe_check_owner = match T::ForceOrigin::try_origin(origin) {
			Ok(_) => None,
			Err(origin) => Some(ensure_signed(origin)?),
		};

		<Pallet<T, I>>::do_destroy_collection(collection.clone(), witness, maybe_check_owner)
	}
}
