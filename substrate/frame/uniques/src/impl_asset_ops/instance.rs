use crate::{asset_strategies::Attribute, *};
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::tokens::asset_ops::{
		common_asset_kinds::Instance,
		common_strategies::{
			Bytes, CanTransfer, FromTo, IfOwnedBy, JustDo, Owned, Ownership, PredefinedId,
			WithOrigin,
		},
		AssetDefinition, Create, Destroy, InspectMetadata, Transfer,
	},
	BoundedSlice,
};
use frame_system::ensure_signed;
use sp_runtime::DispatchError;

impl<T: Config<I>, I: 'static> AssetDefinition<Instance> for Pallet<T, I> {
	type Id = (T::CollectionId, T::ItemId);
}

impl<T: Config<I>, I: 'static> InspectMetadata<Instance, Ownership<T::AccountId>> for Pallet<T, I> {
	fn inspect_metadata(
		(collection, item): &Self::Id,
		_ownership: Ownership<T::AccountId>,
	) -> Result<T::AccountId, DispatchError> {
		Item::<T, I>::get(collection, item)
			.map(|a| a.owner)
			.ok_or(Error::<T, I>::UnknownItem.into())
	}
}

impl<T: Config<I>, I: 'static> InspectMetadata<Instance, Bytes> for Pallet<T, I> {
	fn inspect_metadata(
		(collection, item): &Self::Id,
		_bytes: Bytes,
	) -> Result<Vec<u8>, DispatchError> {
		ItemMetadataOf::<T, I>::get(collection, item)
			.map(|m| m.data.into())
			.ok_or(Error::<T, I>::NoMetadata.into())
	}
}

impl<'a, T: Config<I>, I: 'static> InspectMetadata<Instance, Bytes<Attribute<'a>>>
	for Pallet<T, I>
{
	fn inspect_metadata(
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

impl<T: Config<I>, I: 'static> InspectMetadata<Instance, CanTransfer> for Pallet<T, I> {
	fn inspect_metadata(
		(collection, item): &Self::Id,
		_can_transfer: CanTransfer,
	) -> Result<bool, DispatchError> {
		match (Collection::<T, I>::get(collection), Item::<T, I>::get(collection, item)) {
			(Some(cd), Some(id)) => Ok(!cd.is_frozen && !id.is_frozen),
			_ => Err(Error::<T, I>::UnknownItem.into()),
		}
	}
}

impl<T: Config<I>, I: 'static>
	Create<Instance, Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>> for Pallet<T, I>
{
	fn create(
		strategy: Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		let Owned { owner, id_assignment, .. } = strategy;
		let (collection, item) = id_assignment.params;

		Self::do_mint(collection.clone(), item, owner, |_| Ok(()))?;

		Ok((collection, item))
	}
}

impl<T: Config<I>, I: 'static>
	Create<
		Instance,
		WithOrigin<
			T::RuntimeOrigin,
			Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>,
		>,
	> for Pallet<T, I>
{
	fn create(
		strategy: WithOrigin<
			T::RuntimeOrigin,
			Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>,
		>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		let WithOrigin(origin, Owned { owner, id_assignment, .. }) = strategy;
		let (collection, item) = id_assignment.params;

		let signer = ensure_signed(origin)?;

		Self::do_mint(collection.clone(), item, owner, |collection_details| {
			ensure!(collection_details.issuer == signer, Error::<T, I>::NoPermission);
			Ok(())
		})?;

		Ok((collection, item))
	}
}

impl<T: Config<I>, I: 'static> Transfer<Instance, JustDo<T::AccountId>> for Pallet<T, I> {
	fn transfer((collection, item): &Self::Id, strategy: JustDo<T::AccountId>) -> DispatchResult {
		let JustDo(dest) = strategy;

		Self::do_transfer(collection.clone(), *item, dest, |_, _| Ok(()))
	}
}

impl<T: Config<I>, I: 'static>
	Transfer<Instance, WithOrigin<T::RuntimeOrigin, JustDo<T::AccountId>>> for Pallet<T, I>
{
	fn transfer(
		(collection, item): &Self::Id,
		strategy: WithOrigin<T::RuntimeOrigin, JustDo<T::AccountId>>,
	) -> DispatchResult {
		let WithOrigin(origin, JustDo(dest)) = strategy;

		let signer = ensure_signed(origin)?;

		Self::do_transfer(collection.clone(), *item, dest.clone(), |collection_details, details| {
			if details.owner != signer && collection_details.admin != signer {
				let approved = details.approved.take().map_or(false, |i| i == signer);
				ensure!(approved, Error::<T, I>::NoPermission);
			}
			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static> Transfer<Instance, FromTo<T::AccountId>> for Pallet<T, I> {
	fn transfer((collection, item): &Self::Id, strategy: FromTo<T::AccountId>) -> DispatchResult {
		let FromTo(from, to) = strategy;

		Self::do_transfer(collection.clone(), *item, to.clone(), |_, details| {
			ensure!(details.owner == from, Error::<T, I>::WrongOwner);
			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static> Destroy<Instance, JustDo> for Pallet<T, I> {
	fn destroy((collection, item): &Self::Id, _strategy: JustDo) -> DispatchResult {
		Self::do_burn(collection.clone(), *item, |_, _| Ok(()))
	}
}

impl<T: Config<I>, I: 'static> Destroy<Instance, WithOrigin<T::RuntimeOrigin, JustDo>>
	for Pallet<T, I>
{
	fn destroy(
		id @ (collection, item): &Self::Id,
		strategy: WithOrigin<T::RuntimeOrigin, JustDo>,
	) -> DispatchResult {
		let WithOrigin(origin, _just_do) = strategy;
		let details =
			Item::<T, I>::get(collection, item).ok_or(Error::<T, I>::UnknownCollection)?;

		<Self as Destroy<_, _>>::destroy(id, WithOrigin(origin, IfOwnedBy(details.owner)))
	}
}

impl<T: Config<I>, I: 'static> Destroy<Instance, IfOwnedBy<T::AccountId>> for Pallet<T, I> {
	fn destroy((collection, item): &Self::Id, strategy: IfOwnedBy<T::AccountId>) -> DispatchResult {
		let IfOwnedBy(who) = strategy;

		Self::do_burn(collection.clone(), *item, |_, d| {
			ensure!(d.owner == who, Error::<T, I>::NoPermission);
			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static>
	Destroy<Instance, WithOrigin<T::RuntimeOrigin, IfOwnedBy<T::AccountId>>> for Pallet<T, I>
{
	fn destroy(
		(collection, item): &Self::Id,
		strategy: WithOrigin<T::RuntimeOrigin, IfOwnedBy<T::AccountId>>,
	) -> DispatchResult {
		let WithOrigin(origin, IfOwnedBy(who)) = strategy;

		let signer = ensure_signed(origin)?;

		Self::do_burn(collection.clone(), *item, |collection_details, details| {
			let is_permitted = collection_details.admin == signer || details.owner == signer;
			ensure!(is_permitted, Error::<T, I>::NoPermission);
			ensure!(who == details.owner, Error::<T, I>::WrongOwner);
			Ok(())
		})
	}
}
