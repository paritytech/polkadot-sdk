use core::marker::PhantomData;

use crate::{asset_strategies::Attribute, Item as ItemStorage, *};
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::tokens::asset_ops::{
		common_strategies::{
			Bytes, CanTransfer, CheckOrigin, CheckState, IfOwnedBy, Owned, Ownership, PredefinedId,
			To, Unchecked,
		},
		AssetDefinition, Create, Inspect, Stash, Transfer,
	},
	BoundedSlice,
};
use frame_system::ensure_signed;
use sp_runtime::DispatchError;

pub struct Item<PalletInstance>(PhantomData<PalletInstance>);

impl<T: Config<I>, I: 'static> AssetDefinition for Item<Pallet<T, I>> {
	type Id = (T::CollectionId, T::ItemId);
}

impl<T: Config<I>, I: 'static> Inspect<Ownership<T::AccountId>> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		_ownership: Ownership<T::AccountId>,
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

impl<T: Config<I>, I: 'static> Inspect<CanTransfer> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		_can_transfer: CanTransfer,
	) -> Result<bool, DispatchError> {
		match (Collection::<T, I>::get(collection), ItemStorage::<T, I>::get(collection, item)) {
			(Some(cd), Some(id)) => Ok(!cd.is_frozen && !id.is_frozen),
			_ => Err(Error::<T, I>::UnknownItem.into()),
		}
	}
}

impl<T: Config<I>, I: 'static>
	Create<Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>> for Item<Pallet<T, I>>
{
	fn create(
		strategy: Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		let Owned { owner, id_assignment, .. } = strategy;
		let (collection, item) = id_assignment.params;

		<Pallet<T, I>>::do_mint(collection.clone(), item, owner, |_| Ok(()))?;

		Ok((collection, item))
	}
}

impl<T: Config<I>, I: 'static>
	Create<
		CheckOrigin<
			T::RuntimeOrigin,
			Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>,
		>,
	> for Item<Pallet<T, I>>
{
	fn create(
		strategy: CheckOrigin<
			T::RuntimeOrigin,
			Owned<T::AccountId, PredefinedId<(T::CollectionId, T::ItemId)>>,
		>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		let CheckOrigin(origin, Owned { owner, id_assignment, .. }) = strategy;
		let (collection, item) = id_assignment.params;

		let signer = ensure_signed(origin)?;

		<Pallet<T, I>>::do_mint(collection.clone(), item, owner, |collection_details| {
			ensure!(collection_details.issuer == signer, Error::<T, I>::NoPermission);
			Ok(())
		})?;

		Ok((collection, item))
	}
}

impl<T: Config<I>, I: 'static> Transfer<To<T::AccountId>> for Item<Pallet<T, I>> {
	fn transfer((collection, item): &Self::Id, strategy: To<T::AccountId>) -> DispatchResult {
		let To(dest) = strategy;

		<Pallet<T, I>>::do_transfer(collection.clone(), *item, dest, |_, _| Ok(()))
	}
}

impl<T: Config<I>, I: 'static> Transfer<CheckOrigin<T::RuntimeOrigin, To<T::AccountId>>>
	for Item<Pallet<T, I>>
{
	fn transfer(
		(collection, item): &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, To<T::AccountId>>,
	) -> DispatchResult {
		let CheckOrigin(origin, To(dest)) = strategy;

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

impl<T: Config<I>, I: 'static> Transfer<IfOwnedBy<T::AccountId, To<T::AccountId>>>
	for Item<Pallet<T, I>>
{
	fn transfer(
		(collection, item): &Self::Id,
		strategy: IfOwnedBy<T::AccountId, To<T::AccountId>>,
	) -> DispatchResult {
		let CheckState(from, To(to)) = strategy;

		<Pallet<T, I>>::do_transfer(collection.clone(), *item, to.clone(), |_, details| {
			ensure!(details.owner == from, Error::<T, I>::WrongOwner);
			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static> Stash<Unchecked> for Item<Pallet<T, I>> {
	fn stash((collection, item): &Self::Id, _strategy: Unchecked) -> DispatchResult {
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
