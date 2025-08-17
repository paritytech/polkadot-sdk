use core::marker::PhantomData;

use crate::{types::asset_strategies::*, Item as ItemStorage, *};
use frame_support::{
	dispatch::DispatchResult,
	ensure,
	traits::{
		tokens::asset_ops::{common_strategies::*, *},
		EnsureOrigin,
	},
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
		_owner_strategy: Owner<T::AccountId>,
	) -> Result<T::AccountId, DispatchError> {
		ItemStorage::<T, I>::get(collection, item)
			.map(|a| a.owner)
			.ok_or(Error::<T, I>::UnknownItem.into())
	}
}

impl<T: Config<I>, I: 'static> Update<Owner<T::AccountId>> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		_owner_strategy: Owner<T::AccountId>,
		dest: &T::AccountId,
	) -> DispatchResult {
		<Pallet<T, I>>::do_transfer(*collection, *item, dest.clone(), |_, _| Ok(()))
	}
}

impl<T: Config<I>, I: 'static> Update<ChangeOwnerFrom<T::AccountId>> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		strategy: ChangeOwnerFrom<T::AccountId>,
		dest: &T::AccountId,
	) -> DispatchResult {
		let CheckState(from, ..) = strategy;

		<Pallet<T, I>>::do_transfer(*collection, *item, dest.clone(), |_, details| {
			ensure!(details.owner == from, Error::<T, I>::NoPermission);
			Ok(())
		})
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

		let origin = ensure_signed(origin)?;

		<Pallet<T, I>>::do_transfer(*collection, *item, dest.clone(), |_, details| {
			if details.owner != origin {
				let deadline = details.approvals.get(&origin).ok_or(Error::<T, I>::NoPermission)?;
				if let Some(d) = deadline {
					let block_number = T::BlockNumberProvider::current_block_number();
					ensure!(block_number <= *d, Error::<T, I>::ApprovalExpired);
				}
			}

			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static> Inspect<CanUpdate<Owner<T::AccountId>>> for Item<Pallet<T, I>> {
	fn inspect(
		id @ (collection, item): &Self::Id,
		_strategy: CanUpdate<Owner<T::AccountId>>,
	) -> Result<bool, DispatchError> {
		let encoded_transfer_disabled =
			PalletAttributes::<T::CollectionId>::TransferDisabled.encode();
		let transfer_disabled_attribute = Bytes(SystemAttribute(&encoded_transfer_disabled));

		if Self::inspect(id, transfer_disabled_attribute).is_ok() {
			return Ok(false);
		}

		match (
			CollectionConfigOf::<T, I>::get(*collection),
			ItemConfigOf::<T, I>::get(*collection, *item),
		) {
			(Some(cc), Some(ic)) =>
				Ok(cc.is_setting_enabled(CollectionSetting::TransferableItems) &&
					ic.is_setting_enabled(ItemSetting::Transferable)),
			(None, _) => Err(Error::<T, I>::UnknownCollection.into()),
			(_, None) => Err(Error::<T, I>::UnknownItem.into()),
		}
	}
}

impl<T: Config<I>, I: 'static> Update<CanUpdate<Owner<T::AccountId>>> for Item<Pallet<T, I>> {
	fn update(
		id: &Self::Id,
		_strategy: CanUpdate<Owner<T::AccountId>>,
		can_transfer: bool,
	) -> DispatchResult {
		let disable_transfer = !can_transfer;

		let encoded_transfer_disabled =
			PalletAttributes::<T::CollectionId>::TransferDisabled.encode();
		let transfer_disabled_attribute = Bytes(SystemAttribute(&encoded_transfer_disabled));

		if disable_transfer {
			let transfer_disabled = Self::inspect(id, transfer_disabled_attribute.clone()).is_ok();

			// Can't lock the item twice
			if transfer_disabled {
				return Err(Error::<T, I>::ItemLocked.into())
			}
		}

		Self::update(id, transfer_disabled_attribute, disable_transfer.then_some(&[]))
	}
}

impl<T: Config<I>, I: 'static> Inspect<Bytes> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		_bytes_strategy: Bytes,
	) -> Result<Vec<u8>, DispatchError> {
		ItemMetadataOf::<T, I>::get(collection, item)
			.map(|m| m.data.into())
			.ok_or(Error::<T, I>::MetadataNotFound.into())
	}
}

impl<T: Config<I>, I: 'static> Update<Bytes> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		_bytes_strategy: Bytes,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let maybe_check_origin = None;
		let maybe_depositor = None;

		let update = update
			.map(|metadata| <Pallet<T, I>>::construct_metadata(metadata.to_vec()))
			.transpose()?;

		match update {
			Some(metadata) => <Pallet<T, I>>::do_set_item_metadata(
				maybe_check_origin,
				*collection,
				*item,
				metadata,
				maybe_depositor,
			),
			None => <Pallet<T, I>>::do_clear_item_metadata(maybe_check_origin, *collection, *item),
		}
	}
}

impl<T: Config<I>, I: 'static> Update<CheckOrigin<T::RuntimeOrigin, Bytes>> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, Bytes>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let CheckOrigin(origin, ..) = strategy;

		let maybe_check_origin = T::ForceOrigin::try_origin(origin)
			.map(|_| None)
			.or_else(|origin| ensure_signed(origin).map(Some).map_err(DispatchError::from))?;

		let maybe_depositor = None;

		let update = update
			.map(|metadata| <Pallet<T, I>>::construct_metadata(metadata.to_vec()))
			.transpose()?;

		match update {
			Some(metadata) => <Pallet<T, I>>::do_set_item_metadata(
				maybe_check_origin,
				*collection,
				*item,
				metadata,
				maybe_depositor,
			),
			None => <Pallet<T, I>>::do_clear_item_metadata(maybe_check_origin, *collection, *item),
		}
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Bytes<RegularAttribute<'a>>> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		bytes: Bytes<RegularAttribute>,
	) -> Result<Vec<u8>, DispatchError> {
		let namespace = AttributeNamespace::CollectionOwner;

		let Bytes(RegularAttribute(attribute)) = bytes;

		Attribute::<T, I>::get((
			collection,
			Some(item),
			namespace,
			<Pallet<T, I>>::construct_attribute_key(attribute.to_vec())?,
		))
		.map(|a| a.0.into())
		.ok_or(Error::<T, I>::AttributeNotFound.into())
	}
}

impl<'a, T: Config<I>, I: 'static> Update<Bytes<RegularAttribute<'a>>> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		bytes: Bytes<RegularAttribute>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let maybe_check_origin = None;
		let namespace = AttributeNamespace::CollectionOwner;

		let Bytes(RegularAttribute(attribute)) = bytes;

		<Pallet<T, I>>::do_update_attribute(
			maybe_check_origin,
			*collection,
			Some(*item),
			namespace,
			attribute,
			update,
		)
	}
}

impl<'a, T: Config<I>, I: 'static>
	Update<CheckOrigin<T::RuntimeOrigin, Bytes<RegularAttribute<'a>>>> for Item<Pallet<T, I>>
{
	fn update(
		(collection, item): &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, Bytes<RegularAttribute>>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let namespace = AttributeNamespace::CollectionOwner;

		let CheckOrigin(origin, Bytes(RegularAttribute(attribute))) = strategy;
		let maybe_check_origin = T::ForceOrigin::try_origin(origin)
			.map(|_| None)
			.or_else(|origin| ensure_signed(origin).map(Some).map_err(DispatchError::from))?;

		<Pallet<T, I>>::do_update_attribute(
			maybe_check_origin,
			*collection,
			Some(*item),
			namespace,
			attribute,
			update,
		)
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Bytes<SystemAttribute<'a>>> for Item<Pallet<T, I>> {
	fn inspect(
		(collection, item): &Self::Id,
		bytes: Bytes<SystemAttribute>,
	) -> Result<Vec<u8>, DispatchError> {
		let namespace = AttributeNamespace::Pallet;

		let Bytes(SystemAttribute(attribute)) = bytes;

		Attribute::<T, I>::get((
			collection,
			Some(item),
			namespace,
			<Pallet<T, I>>::construct_attribute_key(attribute.to_vec())?,
		))
		.map(|a| a.0.into())
		.ok_or(Error::<T, I>::AttributeNotFound.into())
	}
}

impl<'a, T: Config<I>, I: 'static> Update<Bytes<SystemAttribute<'a>>> for Item<Pallet<T, I>> {
	fn update(
		(collection, item): &Self::Id,
		bytes: Bytes<SystemAttribute>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let maybe_check_origin = None;
		let namespace = AttributeNamespace::Pallet;

		let Bytes(SystemAttribute(attribute)) = bytes;

		<Pallet<T, I>>::do_update_attribute(
			maybe_check_origin,
			*collection,
			Some(*item),
			namespace,
			attribute,
			update,
		)
	}
}

impl<'a, T: Config<I>, I: 'static> Inspect<Bytes<CustomAttribute<'a, T::AccountId>>>
	for Item<Pallet<T, I>>
{
	fn inspect(
		(collection, item): &Self::Id,
		bytes: Bytes<CustomAttribute<T::AccountId>>,
	) -> Result<Vec<u8>, DispatchError> {
		let Bytes(CustomAttribute(account, attribute)) = bytes;

		let namespace = AttributeNamespace::Account(account.clone());

		Attribute::<T, I>::get((
			collection,
			Some(item),
			namespace,
			<Pallet<T, I>>::construct_attribute_key(attribute.to_vec())?,
		))
		.map(|a| a.0.into())
		.ok_or(Error::<T, I>::AttributeNotFound.into())
	}
}

impl<'a, T: Config<I>, I: 'static> Update<Bytes<CustomAttribute<'a, T::AccountId>>>
	for Item<Pallet<T, I>>
{
	fn update(
		(collection, item): &Self::Id,
		bytes: Bytes<CustomAttribute<T::AccountId>>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let maybe_check_origin = None;

		let Bytes(CustomAttribute(account, attribute)) = bytes;
		let namespace = AttributeNamespace::Account(account.clone());

		<Pallet<T, I>>::do_update_attribute(
			maybe_check_origin,
			*collection,
			Some(*item),
			namespace,
			attribute,
			update,
		)
	}
}

impl<'a, T: Config<I>, I: 'static>
	Update<CheckOrigin<T::RuntimeOrigin, Bytes<CustomAttribute<'a, T::AccountId>>>>
	for Item<Pallet<T, I>>
{
	fn update(
		(collection, item): &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin, Bytes<CustomAttribute<T::AccountId>>>,
		update: Option<&[u8]>,
	) -> DispatchResult {
		let CheckOrigin(origin, Bytes(CustomAttribute(account, attribute))) = strategy;
		let maybe_check_origin = T::ForceOrigin::try_origin(origin)
			.map(|_| None)
			.or_else(|origin| ensure_signed(origin).map(Some).map_err(DispatchError::from))?;
		let namespace = AttributeNamespace::Account(account.clone());

		<Pallet<T, I>>::do_update_attribute(
			maybe_check_origin,
			*collection,
			Some(*item),
			namespace,
			attribute,
			update,
		)
	}
}

impl<T: Config<I>, I: 'static> Create<WithItemOwner<T::AccountId, T::CollectionId, T::ItemId>>
	for Item<Pallet<T, I>>
{
	fn create(
		strategy: WithItemOwner<T::AccountId, T::CollectionId, T::ItemId>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		use crate::{asset_strategies::ItemConfig as ItemConfigStrategy, ItemConfig};

		let WithConfig { config: owner_cfg, extra } = strategy;
		let (collection, ..) = extra.params;

		let item_config =
			ItemConfig { settings: <Pallet<T, I>>::get_default_item_settings(&collection)? };

		Self::create(WithItemConfig::new(
			(owner_cfg, ItemConfigStrategy::with_config_value(item_config)),
			extra,
		))
	}
}

impl<T: Config<I>, I: 'static> Create<WithItemConfig<T::AccountId, T::CollectionId, T::ItemId>>
	for Item<Pallet<T, I>>
{
	fn create(
		strategy: WithItemConfig<T::AccountId, T::CollectionId, T::ItemId>,
	) -> Result<(T::CollectionId, T::ItemId), DispatchError> {
		let WithConfig {
			config: (ConfigValue(owner), ConfigValue(item_config)),
			extra: DeriveAndReportId { params: (collection, item), .. },
		} = strategy;

		<Pallet<T, I>>::do_mint(collection, item, None, owner, item_config, |_, _| Ok(()))?;

		Ok((collection, item))
	}
}

impl<T: Config<I>, I: 'static> Destroy<NoParams> for Item<Pallet<T, I>> {
	fn destroy((collection, item): &Self::Id, _strategy: NoParams) -> DispatchResult {
		<Pallet<T, I>>::do_burn(*collection, *item, |_details| Ok(()))
	}
}

impl<T: Config<I>, I: 'static> Destroy<IfOwnedBy<T::AccountId>> for Item<Pallet<T, I>> {
	fn destroy((collection, item): &Self::Id, strategy: IfOwnedBy<T::AccountId>) -> DispatchResult {
		let CheckState(account, ..) = strategy;

		<Pallet<T, I>>::do_burn(*collection, *item, |details| {
			ensure!(details.owner == account, Error::<T, I>::NoPermission);

			Ok(())
		})
	}
}

impl<T: Config<I>, I: 'static> Destroy<CheckOrigin<T::RuntimeOrigin>> for Item<Pallet<T, I>> {
	fn destroy(
		(collection, item): &Self::Id,
		strategy: CheckOrigin<T::RuntimeOrigin>,
	) -> DispatchResult {
		let CheckOrigin(origin, ..) = strategy;

		let maybe_check_origin = T::ForceOrigin::try_origin(origin)
			.map(|_| None)
			.or_else(|origin| ensure_signed(origin).map(Some).map_err(DispatchError::from))?;

		<Pallet<T, I>>::do_burn(*collection, *item, |details| {
			if let Some(check_origin) = maybe_check_origin {
				ensure!(details.owner == check_origin, Error::<T, I>::NoPermission);
			}

			Ok(())
		})
	}
}
