use crate::{asset_strategies::Attribute, *};
use frame_support::{
	ensure,
	traits::{
		tokens::asset_ops::{
			common_asset_kinds::Class,
			common_strategies::{
				Adminable, Bytes, IfOwnedByWithWitness, Ownership, PredefinedId, WithOrigin,
				WithWitness,
			},
			AssetDefinition, Create, Destroy, InspectMetadata,
		},
		EnsureOrigin, Get,
	},
	BoundedSlice,
};
use frame_system::ensure_signed;
use sp_runtime::DispatchError;

impl<T: Config<I>, I: 'static> AssetDefinition<Class> for Pallet<T, I> {
	type Id = T::CollectionId;
}

impl<T: Config<I>, I: 'static> InspectMetadata<Class, Ownership<T::AccountId>> for Pallet<T, I> {
	fn inspect_metadata(
		collection: &Self::Id,
		_ownership: Ownership<T::AccountId>,
	) -> Result<T::AccountId, DispatchError> {
		Collection::<T, I>::get(collection)
			.map(|a| a.owner)
			.ok_or(Error::<T, I>::UnknownCollection.into())
	}
}

impl<T: Config<I>, I: 'static> InspectMetadata<Class, Bytes> for Pallet<T, I> {
	fn inspect_metadata(collection: &Self::Id, _bytes: Bytes) -> Result<Vec<u8>, DispatchError> {
		CollectionMetadataOf::<T, I>::get(collection)
			.map(|m| m.data.into())
			.ok_or(Error::<T, I>::NoMetadata.into())
	}
}

impl<'a, T: Config<I>, I: 'static> InspectMetadata<Class, Bytes<Attribute<'a>>> for Pallet<T, I> {
	fn inspect_metadata(
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

impl<T: Config<I>, I: 'static> Create<Class, Adminable<T::AccountId, PredefinedId<T::CollectionId>>>
	for Pallet<T, I>
{
	fn create(
		strategy: Adminable<T::AccountId, PredefinedId<T::CollectionId>>,
	) -> Result<T::CollectionId, DispatchError> {
		let Adminable { owner, admin, id_assignment, .. } = strategy;
		let collection = id_assignment.params;

		Self::do_create_collection(
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
	Create<
		Class,
		WithOrigin<T::RuntimeOrigin, Adminable<T::AccountId, PredefinedId<T::CollectionId>>>,
	> for Pallet<T, I>
{
	fn create(
		strategy: WithOrigin<
			T::RuntimeOrigin,
			Adminable<T::AccountId, PredefinedId<T::CollectionId>>,
		>,
	) -> Result<T::CollectionId, DispatchError> {
		let WithOrigin(origin, creation) = strategy;

		let Adminable { owner, id_assignment, .. } = &creation;
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

		<Self as Create<_, _>>::create(creation)
	}
}

impl<T: Config<I>, I: 'static> Destroy<Class, WithWitness<DestroyWitness>> for Pallet<T, I> {
	fn destroy(
		collection: &Self::Id,
		strategy: WithWitness<DestroyWitness>,
	) -> Result<DestroyWitness, DispatchError> {
		let WithWitness(witness) = strategy;

		Self::do_destroy_collection(collection.clone(), witness, None)
	}
}

impl<T: Config<I>, I: 'static>
	Destroy<Class, WithOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>> for Pallet<T, I>
{
	fn destroy(
		collection: &Self::Id,
		strategy: WithOrigin<T::RuntimeOrigin, WithWitness<DestroyWitness>>,
	) -> Result<DestroyWitness, DispatchError> {
		let WithOrigin(origin, destroy) = strategy;

		T::ForceOrigin::ensure_origin(origin)?;

		<Self as Destroy<_, _>>::destroy(collection, destroy)
	}
}

impl<T: Config<I>, I: 'static> Destroy<Class, IfOwnedByWithWitness<T::AccountId, DestroyWitness>>
	for Pallet<T, I>
{
	fn destroy(
		collection: &Self::Id,
		strategy: IfOwnedByWithWitness<T::AccountId, DestroyWitness>,
	) -> Result<DestroyWitness, DispatchError> {
		let IfOwnedByWithWitness { owner, witness } = strategy;

		Self::do_destroy_collection(collection.clone(), witness, Some(owner))
	}
}

impl<T: Config<I>, I: 'static>
	Destroy<Class, WithOrigin<T::RuntimeOrigin, IfOwnedByWithWitness<T::AccountId, DestroyWitness>>>
	for Pallet<T, I>
{
	fn destroy(
		collection: &Self::Id,
		strategy: WithOrigin<T::RuntimeOrigin, IfOwnedByWithWitness<T::AccountId, DestroyWitness>>,
	) -> Result<DestroyWitness, DispatchError> {
		let WithOrigin(origin, IfOwnedByWithWitness { owner, witness }) = strategy;
		let maybe_check_owner = match T::ForceOrigin::try_origin(origin) {
			Ok(_) => None,
			Err(origin) => Some(ensure_signed(origin)?),
		};

		if let Some(signer) = maybe_check_owner {
			ensure!(signer == owner, Error::<T, I>::NoPermission);
		}

		Self::do_destroy_collection(collection.clone(), witness, Some(owner))
	}
}
